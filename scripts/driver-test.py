#!/usr/bin/env python3
"""Tests for .claude/skills/run-stokesforge/driver.py: no display, no Xlib, stdlib only.

Run: python3 scripts/driver-test.py
Fake compositors and apps are plain `sleep` processes; the X and reservation folders
are redirected to a temporary folder through the SF_X_*/SF_RESERVE_DIR seams.
"""
import contextlib, importlib.util, io, os, pathlib, signal, subprocess, sys, tempfile, threading, time, unittest, warnings
warnings.simplefilter("ignore", ResourceWarning)

HERE = pathlib.Path(__file__).resolve().parent
DRIVER = HERE.parent / ".claude/skills/run-stokesforge/driver.py"


def load(env):
    """Imports a fresh copy of the driver under the given environment."""
    for k in ("SF_RUN_DIR", "SF_DISPLAY", "SF_VISIBLE", "SF_X_SOCKET_DIR", "SF_X_LOCK_DIR",
              "SF_RESERVE_DIR", "SF_STOP_GRACE"):
        os.environ.pop(k, None)
    os.environ.update(env)
    spec = importlib.util.spec_from_file_location("driver_under_test", DRIVER)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class Base(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        self.xs, self.xl, self.res, self.run = (self.tmp / d for d in ("xs", "xl", "res", "run"))
        for d in (self.xs, self.xl, self.run):
            d.mkdir()
        self.env = {"SF_RUN_DIR": str(self.run), "SF_X_SOCKET_DIR": str(self.xs),
                    "SF_X_LOCK_DIR": str(self.xl), "SF_RESERVE_DIR": str(self.res)}
        self.pids = []

    def tearDown(self):
        for p in self.pids:
            try:
                os.killpg(p, signal.SIGKILL)
            except OSError:
                pass
        os.environ.clear()
        os.environ.update(self._env)

    def child_env(self):
        return dict(os.environ, **self.env)

    def holder(self):
        """A process that reserves a display, prints its number, and holds it until stdin closes."""
        code = (f"import importlib.util,sys;s=importlib.util.spec_from_file_location('d',{str(DRIVER)!r});"
                "m=importlib.util.module_from_spec(s);s.loader.exec_module(m);"
                "n,fd=m.reserve_display();print(n,flush=True);sys.stdin.read()")
        p = subprocess.Popen([sys.executable, "-I", "-c", code], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, text=True, env=self.child_env())
        n = int(p.stdout.readline())
        return p, n

    def detached(self, mod, name, argv, env=None):
        """Starts a fake process through the driver from a short-lived parent, so it is
        reparented (no zombie) like the real ones; returns its pid."""
        code = (f"import importlib.util;s=importlib.util.spec_from_file_location('d',{str(DRIVER)!r});"
                "m=importlib.util.module_from_spec(s);s.loader.exec_module(m);"
                f"m.RUN.mkdir(parents=True,exist_ok=True);m.spawn({name!r},{argv!r})")
        subprocess.run([sys.executable, "-I", "-c", code], env=self.child_env(), check=True)
        pid = int((self.run / f"{name}.pid").read_text())
        self.pids.append(pid)
        return pid


def gone(pid):
    try:
        os.kill(pid, 0)
        return False
    except ProcessLookupError:
        return True


def wait_gone(pid, t=5):
    end = time.time() + t
    while time.time() < end:
        if gone(pid):
            return True
        time.sleep(0.05)
    return gone(pid)


class Reservation(Base):
    def test_threads_get_distinct_numbers(self):
        d = load(self.env)
        got, lock = [], threading.Lock()
        def work():
            n, fd = d.reserve_display()
            with lock:
                got.append(n)
        ts = [threading.Thread(target=work) for _ in range(8)]
        [t.start() for t in ts]; [t.join() for t in ts]
        self.assertEqual(sorted(got), list(range(99, 107)))

    def test_processes_get_distinct_numbers(self):
        hs = [self.holder() for _ in range(4)]
        try:
            self.assertEqual(sorted(n for _, n in hs), [99, 100, 101, 102])
        finally:
            for p, _ in hs:
                p.stdin.close(); p.wait()

    def test_existing_x_socket_or_lock_is_skipped(self):
        d = load(self.env)
        (self.xs / "X99").touch()
        self.assertEqual(d.reserve_display()[0], 100)
        (self.xl / ".X101-lock").touch()
        self.assertEqual(d.reserve_display()[0], 102)

    def test_driver_never_creates_the_x_lock(self):
        d = load(self.env)
        d.reserve_display()
        self.assertEqual(list(self.xl.iterdir()), [])

    def test_killed_holder_frees_the_number(self):
        p, n = self.holder()
        self.assertEqual(n, 99)
        p.kill(); p.wait()
        d = load(self.env)
        self.assertEqual(d.reserve_display()[0], 99)

    def test_where_reserves_nothing(self):
        env = dict(self.child_env())
        subprocess.run([sys.executable, "-I", str(DRIVER), "where"], env=env, check=True,
                       capture_output=True)
        self.assertFalse(self.res.exists() and any(self.res.iterdir()))

    def test_import_reserves_nothing(self):
        load(self.env)
        self.assertFalse(self.res.exists())

    def test_sf_display_is_used_and_reserves_nothing(self):
        d = load(dict(self.env, SF_DISPLAY=":123"))
        self.assertEqual(d.current_display(), ":123")
        self.assertFalse(self.res.exists() and any(self.res.iterdir()))


class Mode(Base):
    def test_mode_clauses(self):
        d = load(self.env)
        both = lambda n: "/x/" + n
        self.assertEqual(d.choose_mode({}, both), "invisible")
        self.assertEqual(d.choose_mode({}, lambda n: None if n == "mutter" else "/x"), "visible")
        self.assertEqual(d.choose_mode({}, lambda n: None if n == "dbus-run-session" else "/x"), "visible")
        self.assertEqual(d.choose_mode({"SF_VISIBLE": "1"}, both), "visible")

    def test_where_prints_mode(self):
        out = subprocess.run([sys.executable, "-I", str(DRIVER), "where"], env=self.child_env(),
                             check=True, capture_output=True, text=True).stdout
        self.assertRegex(out, r"mode=(visible|invisible)")

    def test_environments(self):
        d = load(self.env)
        base = {"WAYLAND_DISPLAY": "wayland-0", "PATH": "/bin"}
        self.assertEqual(d.xwayland_env(base, "sf-99")["WAYLAND_DISPLAY"], "sf-99")
        self.assertEqual(d.xwayland_env(base, None)["WAYLAND_DISPLAY"], "wayland-0")
        self.assertNotIn("WAYLAND_DISPLAY", d.app_env(base, ":99", "/c"))
        self.assertEqual(d.app_env(base, ":99", "/c")["DISPLAY"], ":99")
        self.assertEqual(d.wayland_name(":100"), "sf-100")
        self.assertEqual(d.compositor_argv("sf-9")[:2], ["dbus-run-session", "--"])


class Stop(Base):
    def test_stop_leaves_nothing_child_included(self):
        d = load(self.env)
        marker = self.tmp / "child-got-term"
        grandchild = (f"import signal,sys,time;"
                      f"signal.signal(signal.SIGTERM,lambda *a:(open({str(marker)!r},'w').close(),sys.exit(0)));"
                      "time.sleep(300)")
        comp = self.detached(d, "compositor", ["sh", "-c", f"python3 -c \"{grandchild}\" & wait"])
        xw = self.detached(d, "xwayland", ["sleep", "300"])
        app = self.detached(d, "app", ["sleep", "300"])
        time.sleep(0.8)
        d.stop([])
        for pid in [comp, xw, app]:
            self.assertTrue(wait_gone(pid), pid)
        time.sleep(0.3)
        self.assertTrue(marker.exists(), "the compositor's child did not get SIGTERM")

    def test_term_ignoring_process_is_killed_after_the_grace(self):
        d = load(dict(self.env, SF_STOP_GRACE="0.5"))
        pid = self.detached(d, "app", ["sh", "-c", "trap '' TERM; while :; do sleep 1; done"])
        time.sleep(0.3)
        d.stop([])
        self.assertTrue(wait_gone(pid))

    def test_stop_app_keeps_compositor_and_xwayland(self):
        d = load(self.env)
        comp = self.detached(d, "compositor", ["sleep", "300"])
        xw = self.detached(d, "xwayland", ["sleep", "300"])
        app = self.detached(d, "app", ["sleep", "300"])
        d.stop(["app"])
        self.assertTrue(wait_gone(app))
        self.assertFalse(gone(comp)); self.assertFalse(gone(xw))
        # and the CLI dispatch passes the argument
        out = subprocess.run([sys.executable, "-I", str(DRIVER), "stop", "app"], env=self.child_env())
        self.assertFalse(gone(comp)); self.assertFalse(gone(xw))

    def test_stop_twice_and_pidfiles_removed(self):
        d = load(self.env)
        self.detached(d, "app", ["sleep", "300"])
        d.stop([]); d.stop([])
        for n in ("app", "xwayland", "compositor"):
            self.assertFalse((self.run / f"{n}.pid").exists())

    def test_stale_pid_is_not_killed(self):
        d = load(self.env)
        other = subprocess.Popen(["sleep", "300"])  # same process group as the test: pid != pgid
        self.pids_other = other
        (self.run / "app.pid").write_text(str(other.pid))
        d.stop([])
        time.sleep(0.2)
        self.assertIsNone(other.poll())
        other.kill(); other.wait()
        (self.run / "app.pid").write_text("999999")
        d.stop([])  # no error


class Gestures(Base):
    def setUp(self):
        super().setUp()
        self.d = load(self.env)

    def kinds(self, ev):
        return [e[0] for e in ev if e[0] != "sleep"]

    def test_plain_clicks_unchanged(self):
        for how, btn, n in (("left", 1, 1), ("right", 3, 1), ("double", 1, 2)):
            ev = self.d.plan_click(10, 20, how)
            self.assertEqual([e for e in ev if e[0] == "btndown"], [("btndown", btn)] * n)
            self.assertEqual(self.kinds(ev).count("btnup"), n)
            self.assertFalse([e for e in ev if e[0].startswith("key")])
            self.assertEqual(self.kinds(ev)[:3], ["move"] * 3)

    def test_shift_click_wraps_the_button(self):
        ev = self.d.plan_click(10, 20, "shift+left")
        k = self.kinds(ev)
        self.assertEqual(k[3:], ["keydown", "btndown", "btnup", "keyup"])
        self.assertEqual(ev[[e[0] for e in ev].index("keydown")], ("keydown", "Shift_L"))

    def test_two_modifiers_release_in_reverse(self):
        ev = [e for e in self.d.plan_click(1, 1, "shift+ctrl+left") if e[0].startswith("key")]
        self.assertEqual(ev, [("keydown", "Shift_L"), ("keydown", "Control_L"),
                              ("keyup", "Control_L"), ("keyup", "Shift_L")])

    def test_unknown_modifier_refused(self):
        with self.assertRaises(SystemExit):
            self.d.plan_click(1, 1, "meta+left")
        with self.assertRaises(SystemExit):
            self.d.plan_drag(1, 1, 5, 5, ["meta"])

    def test_drag_order(self):
        ev = self.d.plan_drag(100, 100, 200, 160, ["shift"])
        k = self.kinds(ev)
        self.assertEqual(k[:3], ["move"] * 3)               # hover before press
        self.assertLess(max(i for i, e in enumerate(ev) if e[0] == "move" and e[1:] == (100, 100)),
                        [e[0] for e in ev].index("btndown"))
        down = [e[0] for e in ev].index("btndown")
        up = [e[0] for e in ev].index("btnup")
        moves = [e for e in ev[down:up] if e[0] == "move"]
        self.assertGreaterEqual(len(moves), 10)
        self.assertEqual(moves[-1], ("move", 200, 160))
        self.assertEqual(k[-2:], ["btnup", "keyup"])         # release last
        self.assertLess([e[0] for e in ev].index("keydown"), down)


class Capture(Base):
    """`ss` options (#517): pure argument parsing and the ImageMagick command."""

    def setUp(self):
        super().setUp()
        self.d = load(dict(self.env, SF_GEOMETRY="1400x900"))

    def test_parse_ss_accepts(self):
        p = self.d.parse_ss
        self.assertEqual(p(["a.png"]), ("a.png", None, None))
        self.assertEqual(p(["a.png", "--crop", "300x200+10+20"]), ("a.png", (300, 200, 10, 20), None))
        self.assertEqual(p(["--max-width", "800", "a.png"]), ("a.png", None, 800))
        self.assertEqual(p(["a.png", "--crop", "1400x900+0+0", "--max-width", "5"]), ("a.png", (1400, 900, 0, 0), 5))

    def test_parse_ss_refuses_with_exit_2(self):
        for argv in (["a.png", "--crop", "300x200"], ["a.png", "--crop", "300x200+1400+0"],
                     ["a.png", "--crop", "300x200+1200+0"], ["a.png", "--crop", "0x200+0+0"],
                     ["a.png", "--max-width", "0"], ["a.png", "--max-width", "-3"], ["a.png", "--max-width", "x"],
                     ["a.png", "--crop"], ["a.png", "--bogus"], [], ["a.png", "b.png"]):
            with self.assertRaises(SystemExit) as cm, contextlib.redirect_stderr(io.StringIO()) as err:
                self.d.parse_ss(argv)
            self.assertEqual(cm.exception.code, 2, argv)
            self.assertIn("ss:", err.getvalue())

    def test_post_argv(self):
        pa = self.d.post_argv
        self.assertIsNone(pa("a.png", None, None))
        self.assertEqual(pa("a.png", (300, 200, 10, 20), 800),
                         ["magick", "a.png", "-crop", "300x200+10+20", "+repage", "-resize", "800x>", "a.png"])
        self.assertEqual(pa("a.png", None, 800), ["magick", "a.png", "-resize", "800x>", "a.png"])
        self.assertEqual(pa("a.png", (1, 2, 3, 4), None), ["magick", "a.png", "-crop", "1x2+3+4", "+repage", "a.png"])


if __name__ == "__main__":
    unittest.main(verbosity=1)

#!/usr/bin/env python3
"""Tests of scripts/cargo-queue.py (#476): stdlib unittest, no user bus, no real cargo.

A fake `cargo`, `systemd-run`, `systemctl` and `gdbus` sit on PATH behind a copy of the shim named
`cargo`. The flocks are real. Run: python3 scripts/cargo-queue-test.py
"""
import importlib.util
import json
import os
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "cargo-queue.py")

spec = importlib.util.spec_from_file_location("cargo_queue", SRC)
cq = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cq)

FAKE_CARGO = r'''#!/usr/bin/python3
import json, os, signal, subprocess, sys, time
log = os.environ["FAKE_LOG"]
tag = os.environ.get("FAKE_TAG", "")
def put(d):
    d.update(tag=tag, pid=os.getpid())
    with open(log, "a") as f:
        f.write(json.dumps(d) + "\n")
def term(s, f):
    put({"ev": "signal", "sig": int(s)})
    sys.exit(143)
signal.signal(signal.SIGTERM, term)
signal.signal(signal.SIGINT, term)
d = {"ev": "start", "args": sys.argv[1:], "marker": os.environ.get("SF_CARGO_QUEUE_HELD"), "cwd": os.getcwd(),
     "foo": os.environ.get("FOO"), "nice": os.nice(0), "jobs": [os.environ.get(k) for k in ("CARGO_BUILD_JOBS", "NEXTEST_TEST_THREADS", "RUST_TEST_THREADS")]}
if os.environ.get("FAKE_STDIN"):
    d["stdin"] = sys.stdin.read()
put(d)
if os.environ.get("FAKE_BG"):
    p = subprocess.Popen(["sleep", "30"], close_fds=False)
    put({"ev": "bg", "bgpid": p.pid})
gate = os.environ.get("FAKE_GATE")
t0 = time.time()
while gate and not os.path.exists(gate) and time.time() - t0 < 25:
    time.sleep(0.02)
if os.environ.get("FAKE_DIE"):
    os.kill(os.getpid(), int(os.environ["FAKE_DIE"]))
    time.sleep(5)
put({"ev": "end"})
sys.exit(int(os.environ.get("FAKE_EXIT", "0")))
'''

FAKE_SYSTEMD_RUN = '''#!/bin/sh
echo "$*" >> "$FAKE_SDLOG"
while [ "$1" != "--" ]; do shift; done
shift
exec "$@"
'''

FAKE_SYSTEMCTL = '''#!/bin/sh
echo "$*" >> "$FAKE_CTLLOG"
exit "${FAKE_SYSTEMCTL_RC:-0}"
'''


def script(path, text):
    with open(path, "w") as f:
        f.write(text)
    os.chmod(path, os.stat(path).st_mode | stat.S_IXUSR)


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="cq-test-")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        t = self.tmp
        self.shimdir, self.fakedir, self.qdir = (os.path.join(t, x) for x in ("shim", "fake", "state"))
        for d in (self.shimdir, self.fakedir):
            os.makedirs(d)
        self.shim = os.path.join(self.shimdir, "cargo")
        shutil.copy(SRC, self.shim)
        script(os.path.join(self.fakedir, "cargo"), FAKE_CARGO)
        script(os.path.join(self.fakedir, "systemd-run"), FAKE_SYSTEMD_RUN)
        script(os.path.join(self.fakedir, "systemctl"), FAKE_SYSTEMCTL)
        self.log, self.sdlog, self.ctllog = (os.path.join(t, x) for x in ("fake.log", "sd.log", "ctl.log"))
        self.env = {"PATH": "%s:%s:/usr/bin:/bin" % (self.shimdir, self.fakedir), "HOME": t,
                    "SF_CARGO_QUEUE_DIR": self.qdir, "SF_CARGO_QUEUE_POLL": "0.05",
                    "FAKE_LOG": self.log, "FAKE_SDLOG": self.sdlog, "FAKE_CTLLOG": self.ctllog,
                    "SF_CARGO_QUEUE_SLICE": "sfq-test.slice", "SF_CARGO_QUEUE_POWER_DIR": os.path.join(t, "nopower")}
        self.procs = []
        self.addCleanup(self.reap)

    def reap(self):
        for p in self.procs:
            if p.poll() is None:
                p.kill()
            p.wait()
            p.outfile.close()
        for e in self.events():
            if e["ev"] == "bg":
                try:
                    os.kill(e["bgpid"], signal.SIGKILL)
                except OSError:
                    pass

    def start(self, *args, tag="", gate=None, extra=None, shim=None, cwd=None, stdin=None):
        env = dict(self.env, FAKE_TAG=tag)
        if gate:
            env["FAKE_GATE"] = os.path.join(self.tmp, gate)
        env.update(extra or {})
        out = open(os.path.join(self.tmp, "out-%s-%d" % (tag or "x", len(self.procs))), "w+")
        p = subprocess.Popen([shim or self.shim, *args], env=env, cwd=cwd or self.tmp, stdout=out, stderr=out,
                             stdin=stdin if stdin is not None else subprocess.DEVNULL)
        p.outfile = out
        self.procs.append(p)
        return p

    def output(self, p):
        p.outfile.flush()
        p.outfile.seek(0)
        return p.outfile.read()

    def open_gate(self, name):
        open(os.path.join(self.tmp, name), "w").close()

    def events(self):
        try:
            with open(self.log) as f:
                return [json.loads(x) for x in f if x.strip()]
        except OSError:
            return []

    def seen(self, ev, tag=None):
        return [e for e in self.events() if e["ev"] == ev and (tag is None or e["tag"] == tag)]

    def until(self, cond, timeout=10):
        t0 = time.time()
        while time.time() - t0 < timeout:
            if cond():
                return True
            time.sleep(0.02)
        return False

    def sd_calls(self):
        try:
            with open(self.sdlog) as f:
                return f.read().splitlines()
        except OSError:
            return []

    def ctl_calls(self):
        try:
            with open(self.ctllog) as f:
                return f.read().splitlines()
        except OSError:
            return []

    def tickets(self):
        try:
            return os.listdir(os.path.join(self.qdir, "queue"))
        except OSError:
            return []

    def hold_a(self):
        """A heavy cargo is running and holds the only slot."""
        a = self.start("test", tag="a", gate="ga")
        self.assertTrue(self.until(lambda: self.seen("start", "a")))
        return a

    def idle(self, secs=0.6):
        time.sleep(secs)


class TestQueue(Base):
    def setUp(self):
        super().setUp()
        os.makedirs(self.qdir)     # one slot (background), whatever the table says for the default mode
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:
            json.dump({"selected": "background", "effective": "background"}, f)
        assert cq.MODES["background"]["slots"] == 1

    def test_second_test_waits_for_the_first(self):
        a = self.hold_a()
        b = self.start("test", tag="b", gate="gb")
        self.idle()
        self.assertFalse(self.seen("start", "b"), "second cargo test started while the first ran")
        self.open_gate("ga")
        self.assertTrue(self.until(lambda: self.seen("start", "b")))
        order = [(e["ev"], e["tag"]) for e in self.events() if e["ev"] in ("start", "end")]
        self.assertLess(order.index(("end", "a")), order.index(("start", "b")))
        self.open_gate("gb")
        self.assertEqual(b.wait(10), 0)
        self.assertEqual(a.wait(10), 0)

    def test_kill9_of_holder_frees_the_slot_and_stops_its_cargo(self):
        a = self.hold_a()
        b = self.start("test", tag="b", gate="gb")
        self.idle()
        self.assertFalse(self.seen("start", "b"))
        a.kill()
        self.assertTrue(self.until(lambda: self.seen("start", "b")), "waiter did not proceed after kill -9")
        self.assertTrue(self.until(lambda: self.seen("signal", "a")), "cargo of the killed shim got no SIGTERM")
        self.open_gate("gb")
        self.assertEqual(b.wait(10), 0)

    def test_leftover_grandchild_does_not_hold_the_slot(self):
        a = self.start("test", tag="a", extra={"FAKE_BG": "1"})
        self.assertEqual(a.wait(10), 0)
        b = self.start("test", tag="b")
        self.assertEqual(b.wait(5), 0, "next run waited behind a leftover background process")
        self.assertTrue(self.seen("bg"))

    def test_leftover_grandchild_does_not_keep_a_killed_shims_lock(self):
        a = self.start("test", tag="a", gate="ga", extra={"FAKE_BG": "1"})
        self.assertTrue(self.until(lambda: self.seen("bg")))
        b = self.start("test", tag="b", gate="gb")
        self.idle()
        self.assertFalse(self.seen("start", "b"))
        a.kill()
        self.assertTrue(self.until(lambda: self.seen("start", "b")), "a leftover process kept the dead shim's lock")
        self.open_gate("gb")

    def test_marker_runs_at_once_when_queue_full(self):
        self.hold_a()
        b = self.start("test", tag="b", extra={"SF_CARGO_QUEUE_HELD": "1"})
        self.assertEqual(b.wait(5), 0)
        self.assertEqual(len(self.sd_calls()), 1, "nested call must not open a scope")

    def test_off_has_no_ticket_and_no_scope(self):
        p = self.start("test", tag="o", extra={"SF_CARGO_QUEUE": "off"}, gate="go")
        self.assertTrue(self.until(lambda: self.seen("start", "o")))
        self.assertEqual(self.tickets(), [])
        self.open_gate("go")
        self.assertEqual(p.wait(5), 0)
        self.assertEqual(self.sd_calls(), [])

    def test_run_builds_in_queue_then_runs_outside(self):
        p = self.start("run", "-p", "x", "--", "--flag", tag="r")
        self.assertEqual(p.wait(10), 0)
        starts = self.seen("start", "r")
        self.assertEqual([s["args"] for s in starts], [["build", "-p", "x"], ["run", "-p", "x", "--", "--flag"]])
        self.assertEqual(starts[0]["marker"], "1")
        self.assertIsNone(starts[1]["marker"])
        self.assertEqual(len(self.sd_calls()), 1, "only the build is in the slice")
        self.assertIn("--slice=sfq-test.slice", self.sd_calls()[0])
        self.assertEqual(self.tickets(), [])

    def test_run_stops_when_the_build_fails(self):
        p = self.start("run", tag="r", extra={"FAKE_EXIT": "101"})
        self.assertEqual(p.wait(10), 101)
        self.assertEqual(len(self.seen("start")), 1)

    def test_light_commands_do_not_wait(self):
        self.hold_a()
        for args in (["fmt"], ["metadata"], ["nextest", "--version"], ["test", "--help"], ["--version"], ["tree"],
                     ["frobnicate"]):
            p = self.start(*args, tag="l")
            self.assertEqual(p.wait(5), 0, args)
        self.assertEqual(len(self.sd_calls()), 1)      # only the holder

    def test_help_after_double_dash_still_queues(self):
        self.hold_a()
        p = self.start("test", "--", "--help", tag="h")
        self.idle()
        self.assertIsNone(p.poll())
        self.assertFalse(self.seen("start", "h"))

    def test_heavy_detection_and_args_unchanged(self):
        for args in (["+nightly", "test"], ["--color", "always", "t"], ["-v", "b"], ["--color=never", "check", "-p", "x"]):
            p = self.start(*args, tag="d")
            self.assertEqual(p.wait(5), 0)
            self.assertEqual(self.seen("start", "d")[-1]["args"], args)
        self.assertEqual(len(self.sd_calls()), 4)

    def test_parse_args(self):
        self.assertEqual(cq.parse_args(["+nightly", "test"]), (1, "test"))
        self.assertEqual(cq.parse_args(["-Z", "x", "--config", "a=b", "-vv", "clippy"]), (5, "clippy"))
        self.assertEqual(cq.parse_args(["--version"]), (None, None))

    def test_exit_code_and_signal_death(self):
        self.assertEqual(self.start("test", tag="e", extra={"FAKE_EXIT": "3"}).wait(10), 3)
        p = self.start("test", tag="s", extra={"FAKE_DIE": str(int(signal.SIGUSR1))})
        self.assertEqual(p.wait(10), -int(signal.SIGUSR1))

    def test_term_and_int_reach_cargo(self):
        for n, sig in ((1, signal.SIGTERM), (2, signal.SIGINT)):
            tag = "sig%d" % n
            p = self.start("test", tag=tag, gate="never")
            self.assertTrue(self.until(lambda: self.seen("start", tag)))
            p.send_signal(sig)
            self.assertTrue(self.until(lambda: self.seen("signal", tag)))
            p.wait(10)
            self.assertEqual(self.tickets(), [])

    def test_cwd_env_stdin_kept(self):
        d = os.path.join(self.tmp, "work")
        os.makedirs(d)
        r, w = os.pipe()
        os.write(w, b"hello")
        os.close(w)
        p = self.start("test", tag="k", cwd=d, extra={"FOO": "bar", "FAKE_STDIN": "1"}, stdin=r)
        os.close(r)
        self.assertEqual(p.wait(10), 0)
        s = self.seen("start", "k")[0]
        self.assertEqual((s["cwd"], s["foo"], s["stdin"]), (os.path.realpath(d), "bar", "hello"))

    def test_rust_analyzer_parent_is_never_queued(self):
        self.hold_a()
        ra = os.path.join(self.tmp, "rust-analyzer")
        script(ra, '#!/bin/sh\n"%s" check\nexit $?\n' % self.shim)
        env = dict(self.env, FAKE_TAG="ra")
        p = subprocess.run([ra], env=env, cwd=self.tmp, timeout=5)
        self.assertEqual(p.returncode, 0)
        self.assertEqual(len(self.sd_calls()), 1)
        self.assertTrue(self.seen("start", "ra"))

    def test_systemctl_failure_runs_with_a_warning_and_nice(self):
        p = self.start("test", tag="n", extra={"FAKE_SYSTEMCTL_RC": "1"})
        self.assertEqual(p.wait(10), 0)
        out = self.output(p)
        self.assertEqual(out.count("warning"), 1, out)
        self.assertEqual(self.sd_calls(), [])
        self.assertEqual(self.seen("start", "n")[0]["nice"], cq.MODES["background"]["fallback_nice"])

    def jobs_env(self, tag):
        return self.seen("start", tag)[0]["jobs"]

    def test_jobs_env_in_a_mode_with_jobs_not_in_full(self):
        j = str(cq.MODES["background"]["jobs"])
        self.assertEqual(self.start("test", tag="j").wait(10), 0)
        self.assertEqual(self.jobs_env("j"), [j, j, j])
        with mock.patch.dict(os.environ, self.env):
            cq.set_mode(selected="full")
        self.assertIsNone(cq.MODES["full"]["jobs"])
        self.assertEqual(self.start("test", tag="f").wait(10), 0)
        self.assertEqual(self.jobs_env("f"), [None, None, None])

    def test_callers_jobs_win(self):
        self.assertEqual(self.start("test", tag="c", extra={"CARGO_BUILD_JOBS": "3"}).wait(10), 0)
        j = str(cq.MODES["background"]["jobs"])
        self.assertEqual(self.jobs_env("c"), ["3", j, j])

    def test_jobs_env_also_without_a_scope(self):
        self.assertEqual(self.start("test", tag="n", extra={"FAKE_SYSTEMCTL_RC": "1"}).wait(10), 0)
        self.assertEqual(self.jobs_env("n")[0], str(cq.MODES["background"]["jobs"]))

    def test_run_exec_and_passthrough_have_no_jobs_env(self):
        self.assertEqual(self.start("run", tag="r").wait(10), 0)
        first, second = self.seen("start", "r")
        self.assertEqual(first["jobs"][0], str(cq.MODES["background"]["jobs"]))
        self.assertEqual(second["jobs"], [None, None, None])
        self.assertEqual(self.start("fmt", tag="p").wait(10), 0)
        self.assertEqual(self.jobs_env("p"), [None, None, None])

    def test_normal_path_has_no_nice(self):
        self.assertEqual(self.start("test", tag="z").wait(10), 0)
        self.assertEqual(self.seen("start", "z")[0]["nice"], 0)

    def test_waiter_names_the_holder(self):
        a = self.hold_a()
        b = self.start("test", "-p", "needle", tag="b")
        self.assertTrue(self.until(lambda: "waiting" in self.output(b)))
        out = self.output(b)
        self.assertIn("pid %d" % a.pid, out)
        self.assertIn(os.path.realpath(self.tmp), out)
        self.assertIn("cargo test", out)

    def test_shim_first_in_path_finds_the_next_cargo(self):
        p = subprocess.run([self.shim, "fmt"], env=dict(self.env, FAKE_TAG="p"), cwd=self.tmp, timeout=10)
        self.assertEqual(p.returncode, 0)
        env = dict(self.env, PATH=self.shimdir + ":/usr/bin:/bin")
        p = subprocess.run([self.shim, "fmt"], env=env, cwd=self.tmp, timeout=10, capture_output=True, text=True)
        self.assertEqual(p.returncode, 127)

    def test_arrival_order_and_priority(self):
        self.hold_a()
        self.start("test", tag="b", gate="gb")
        self.idle(0.15)
        self.start("test", tag="c", gate="gc")
        self.idle(0.15)
        self.start("test", tag="d", gate="gd", extra={"SF_CARGO_QUEUE": "first"})
        self.idle(0.3)
        self.open_gate("ga")
        self.assertTrue(self.until(lambda: self.seen("start", "d")))
        self.idle(0.3)
        self.assertFalse(self.seen("start", "b") or self.seen("start", "c"))
        self.open_gate("gd")
        self.assertTrue(self.until(lambda: self.seen("start", "b")))
        self.idle(0.3)
        self.assertFalse(self.seen("start", "c"))
        self.open_gate("gb")
        self.assertTrue(self.until(lambda: self.seen("start", "c")))
        self.open_gate("gc")
        order = [e["tag"] for e in self.events() if e["ev"] == "start"]
        self.assertEqual(order, ["a", "d", "b", "c"])

    def test_more_slots_free_a_waiter(self):
        self.hold_a()
        self.start("test", tag="b", gate="gb")
        self.idle()
        self.assertFalse(self.seen("start", "b"))
        with mock.patch.dict(os.environ, self.env):
            cq.set_mode(selected="full")
        self.assertTrue(self.until(lambda: self.seen("start", "b")), "more slots should let both run")
        self.assertEqual(len([e for e in self.events() if e["ev"] == "end"]), 0)


class TestModes(Base):
    def setUp(self):
        super().setUp()
        p = mock.patch.dict(os.environ, self.env)
        p.start()
        self.addCleanup(p.stop)

    def last_set(self):
        calls = [c for c in self.ctl_calls() if "set-property" in c]
        return calls[-1]

    def test_set_mode_values_and_scaled_quota(self):
        m = cq.MODES["background"]
        patch = mock.patch.dict(m, cpu_quota_pct=50)     # the knob is unused by the shipped table
        patch.start()
        self.addCleanup(patch.stop)
        cq.set_mode(selected="background")
        c = self.last_set()
        n = os.cpu_count()
        self.assertIn("--user set-property --runtime sfq-test.slice", c)
        self.assertIn("CPUWeight=%d" % m["cpu_weight"], c)
        self.assertIn("CPUQuota=%d%%" % (n * m["cpu_quota_pct"]), c)
        self.assertIn("MemoryHigh=%s" % m["memory_high"], c)
        self.assertEqual(cq.read_state()["selected"], "background")

    def test_uncapped_mode_passes_an_empty_quota(self):
        cq.set_mode(selected="full")
        args = self.last_set().split()
        self.assertIn("CPUQuota=", args)

    def test_unknown_mode_is_refused(self):
        with self.assertRaises(ValueError):
            cq.set_mode(selected="turbo")

    def test_default_mode_is_dev(self):
        self.assertEqual(cq.read_state()["effective"], "dev")

    def test_auto_decide_truth_table(self):
        d = cq.auto_decide
        for sel in cq.MODES:
            self.assertEqual(d(sel, None, True), ("background", "battery"))
            self.assertEqual(d(sel, 10**9, True), ("background", "battery"))
            self.assertEqual(d(sel, 600000, False), ("full", "away"))
            self.assertEqual(d(sel, 599999, False), (sel, "selected"))
            self.assertEqual(d(sel, None, False), (sel, "selected"))
        self.assertEqual(d("dev", 5000, False, away_ms=1000), ("full", "away"))

    def test_auto_tick_reads_idle_and_power(self):
        script(os.path.join(self.fakedir, "gdbus"), '#!/bin/sh\necho "(uint64 700000,)"\n')
        cq.set_mode(selected="dev", auto=True)
        st = cq.read_state()
        self.assertEqual((st["effective"], st["why"]), ("full", "away"))
        # the user comes back, unreadable idle time keeps the selection
        script(os.path.join(self.fakedir, "gdbus"), '#!/bin/sh\nexit 1\n')
        st = cq.auto_tick()
        self.assertEqual((st["effective"], st["why"]), ("dev", "selected"))
        # on battery: this machine's four lines are not a discharge, a discharging BAT0 is
        root = os.path.join(self.tmp, "nopower")
        self.power(root, "ADP1", "Mains", online="1")
        self.power(root, "BAT0", "Battery", status="Charging")
        self.power(root, "hid-0018:04F3:4229.0002-battery-7", "Battery", online="1", status="Unknown", capacity="0")
        self.power(root, "ucsi-source-psy-USBC000:001", "USB", online="1")
        self.assertFalse(cq.read_on_battery(root))
        self.assertEqual(cq.auto_tick()["why"], "selected")
        self.power(root, "BAT0", "Battery", status="Discharging")
        self.assertTrue(cq.read_on_battery(root))
        st = cq.auto_tick()
        self.assertEqual((st["effective"], st["why"]), ("background", "battery"))
        cq.set_mode(auto=False)
        self.assertEqual(cq.read_state()["effective"], "dev")

    def power(self, root, name, typ, **files):
        d = os.path.join(root, name)
        os.makedirs(d, exist_ok=True)
        for k, v in dict(files, type=typ).items():
            with open(os.path.join(d, k), "w") as f:
                f.write(v + "\n")

    def test_status_json(self):
        d = cq.status_dict()
        self.assertFalse(d["installed"])
        inst = os.path.join(self.tmp, "installed-cargo")
        shutil.copy(SRC, inst)
        with mock.patch.dict(os.environ, {"SF_CARGO_QUEUE_INSTALLED": inst}):
            d = cq.status_dict()
            self.assertTrue(d["installed"] and not d["stale"])
            with open(inst, "a") as f:
                f.write("# changed\n")
            self.assertTrue(cq.status_dict()["stale"])
        self.assertEqual(set(d["modes"]), {"full", "dev", "background"})

    def test_status_lists_running_and_waiting(self):
        cq.set_mode(selected="background")      # one slot
        a = self.start("test", tag="a", gate="ga")
        self.assertTrue(self.until(lambda: self.seen("start", "a")))
        self.start("test", "-p", "q", tag="b")
        self.assertTrue(self.until(lambda: len(cq.status_dict()["waiting"]) == 1))
        d = cq.status_dict()
        self.assertEqual([r["pid"] for r in d["running"]], [a.pid])
        self.assertIn("-p q", d["waiting"][0]["command"])


class TestInstall(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="cq-install-")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.fake = os.path.join(self.tmp, "fakebin")
        os.makedirs(self.fake)
        script(os.path.join(self.fake, "systemctl"), '#!/bin/sh\necho "$*" >> "$HOME/ctl.log"\n')
        self.env = {"HOME": self.tmp, "PATH": self.fake + ":/usr/bin:/bin"}
        self.rc = os.path.join(self.tmp, ".bashrc")

    def sh(self, name):
        r = subprocess.run(["bash", os.path.join(HERE, name)], env=self.env, capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout

    def rc_text(self):
        with open(self.rc, newline="") as f:
            return f.read()

    def test_install_twice_keeps_one_block_last_and_uninstall_restores(self):
        original = "alias x=y\n. ~/.cargo/env\n"
        with open(self.rc, "w") as f:
            f.write(original)
        out = self.sh("cargo-queue-install.sh")
        self.assertIn("SF_CARGO_QUEUE=off", out)
        self.assertIn("new terminals", out.lower().replace("only new", "new"))
        once = self.rc_text()
        self.sh("cargo-queue-install.sh")
        self.assertEqual(self.rc_text(), once)
        self.assertEqual(once.count("stokesforge cargo-queue (#476) >>>"), 1)
        # something appended after the block: the block moves back to the end
        with open(self.rc, "a") as f:
            f.write('. "$HOME/.cargo/env"\n')
        self.sh("cargo-queue-install.sh")
        t = self.rc_text()
        self.assertEqual(t.count("(#476) >>>"), 1)
        self.assertTrue(t.endswith("<<< stokesforge cargo-queue (#476) <<<\n"))
        self.assertTrue(t.startswith(original + '. "$HOME/.cargo/env"\n'))
        shim = os.path.join(self.tmp, ".local/share/stokesforge/cargo-queue/bin/cargo")
        self.assertTrue(os.access(shim, os.X_OK))
        unit = os.path.join(self.tmp, ".config/systemd/user/sfbuild.slice")
        with open(unit) as f:
            text = f.read()
        self.assertNotIn("ManagedOOM", [l.split("=")[0] for l in text.splitlines()])
        self.assertNotIn("-", os.path.basename(unit).replace(".slice", ""))
        self.sh("cargo-queue-uninstall.sh")
        self.assertEqual(self.rc_text(), original + '. "$HOME/.cargo/env"\n')
        self.assertFalse(os.path.exists(unit))
        self.assertFalse(os.path.exists(shim))

    def test_uninstall_restores_byte_for_byte_without_trailing_newline(self):
        for original in ("export A=1", "", "line\n"):
            with open(self.rc, "w", newline="") as f:
                f.write(original)
            self.sh("cargo-queue-install.sh")
            self.sh("cargo-queue-install.sh")
            self.sh("cargo-queue-uninstall.sh")
            self.assertEqual(self.rc_text(), original, repr(original))

    def test_block_puts_the_shim_directory_first_in_path(self):
        self.sh("cargo-queue-install.sh")
        self.assertIn('export PATH="$HOME/.local/share/stokesforge/cargo-queue/bin:$PATH"', self.rc_text())

    def test_slice_name_has_no_dash(self):
        self.assertNotIn("-", cq.SLICE.replace(".slice", ""))


if __name__ == "__main__":
    unittest.main()

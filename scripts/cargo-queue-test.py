#!/usr/bin/env python3
"""Tests of scripts/cargo-queue.py (#476): stdlib unittest, no user bus, no real cargo.

A fake `cargo`, `systemd-run`, `systemctl`, `gdbus` and `busctl` sit on PATH behind a copy of the shim
named `cargo`. The flocks are real. NOTHING here may reach the real power profile (#477): every test has a
canary `busctl` whose log must stay empty unless the test opts in (`enable_bus`), and the state folder is
always a temporary one. Run: python3 scripts/cargo-queue-test.py
"""
import contextlib
import importlib.util
import io
import json
import os
import pty
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

os.environ.pop("SF_CARGO_QUEUE_PROFILE", None)     # a stray value would switch the profile link on for every test
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

# Fake `busctl` (system bus, net.hadess.PowerProfiles): the profile is the content of FAKE_BUSSTATE, every
# call is one line of FAKE_BUSLOG. FAKE_BUS_{GET,SET}_RC fail the call, FAKE_BUS_{GET,SET}_SLEEP make it hang.
FAKE_BUSCTL = '''#!/usr/bin/python3
import os, sys, time
with open(os.environ["FAKE_BUSLOG"], "a") as f:
    f.write(" ".join(sys.argv[1:]) + "\\n")
kind = "GET" if "get-property" in sys.argv else "SET" if "set-property" in sys.argv else None
if kind is None:
    sys.exit(2)
seen = ""
if kind == "GET":      # the value is read before the pause, as on a slow bus: a late answer can be stale
    with open(os.environ["FAKE_BUSSTATE"]) as f:
        seen = f.read().strip()
time.sleep(float(os.environ.get("FAKE_BUS_%s_SLEEP" % kind, "0")))
if int(os.environ.get("FAKE_BUS_%s_RC" % kind, "0")):
    sys.exit(int(os.environ["FAKE_BUS_%s_RC" % kind]))
if kind == "GET":
    print('s "%s"' % seen)
else:
    with open(os.environ["FAKE_BUSSTATE"], "w") as f:
        f.write(sys.argv[-1])
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
        script(os.path.join(self.fakedir, "busctl"), FAKE_BUSCTL)      # the canary: first on PATH, logs every call
        self.log, self.sdlog, self.ctllog = (os.path.join(t, x) for x in ("fake.log", "sd.log", "ctl.log"))
        self.buslog, self.busstate = os.path.join(t, "bus.log"), os.path.join(t, "bus.state")
        self.bus_ok = False
        self.addCleanup(self.check_canary)
        self.env = {"PATH": "%s:%s:/usr/bin:/bin" % (self.shimdir, self.fakedir), "HOME": t,
                    "SF_CARGO_QUEUE_DIR": self.qdir, "SF_CARGO_QUEUE_POLL": "0.05",
                    "FAKE_LOG": self.log, "FAKE_SDLOG": self.sdlog, "FAKE_CTLLOG": self.ctllog,
                    "FAKE_BUSLOG": self.buslog, "FAKE_BUSSTATE": self.busstate,
                    "SF_CARGO_QUEUE_SLICE": "sfq-test.slice", "SF_CARGO_QUEUE_POWER_DIR": os.path.join(t, "nopower")}
        self.procs = []
        self.addCleanup(self.reap)

    def check_canary(self):
        if not self.bus_ok:
            self.assertEqual(self.bus_calls(), [], "a test reached busctl without opting in")

    def bus_calls(self):
        try:
            with open(self.buslog) as f:
                return f.read().splitlines()
        except OSError:
            return []

    def bus_writes(self):
        return [c.split()[-1] for c in self.bus_calls() if "set-property" in c]

    def enable_bus(self, found="balanced"):
        """Opt in: the profile link is on, the fake profile is `found`. Also for this process's environment."""
        self.bus_ok = True
        with open(self.busstate, "w") as f:
            f.write(found)
        self.env["SF_CARGO_QUEUE_PROFILE"] = "on"
        p = mock.patch.dict(os.environ, {"SF_CARGO_QUEUE_PROFILE": "on"})
        p.start()
        self.addCleanup(p.stop)

    def power(self, root, name, typ, **files):
        d = os.path.join(root, name)
        os.makedirs(d, exist_ok=True)
        for k, v in dict(files, type=typ).items():
            with open(os.path.join(d, k), "w") as f:
                f.write(v + "\n")

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

    def start(self, *args, tag="", gate=None, extra=None, shim=None, cwd=None, stdin=None, stderr=None):
        env = dict(self.env, FAKE_TAG=tag)
        if gate:
            env["FAKE_GATE"] = os.path.join(self.tmp, gate)
        env.update(extra or {})
        out = open(os.path.join(self.tmp, "out-%s-%d" % (tag or "x", len(self.procs))), "w+")
        p = subprocess.Popen([shim or self.shim, *args], env=env, cwd=cwd or self.tmp, stdout=out, stderr=stderr if stderr is not None else out,
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

    def pty_start(self, *args, **kw):
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        p = self.start(*args, stderr=slave, **kw)
        os.close(slave)
        return p

    def test_typed_command_jumps_a_full_queue_and_counts_as_running(self):
        self.hold_a()
        u = self.pty_start("test", tag="u", gate="gu")
        self.assertTrue(self.until(lambda: self.seen("start", "u")), "a command typed by the user waited")
        with mock.patch.dict(os.environ, self.env):
            run, wait = cq.snapshot()
        self.assertEqual(sorted(r["pid"] for r in run), sorted([u.pid, self.procs[0].pid]))
        self.assertEqual(len(self.sd_calls()), 2, "still runs in the slice")
        b = self.start("test", tag="b")
        self.idle()
        self.assertFalse(self.seen("start", "b"))
        self.assertTrue(self.until(lambda: "2 ahead" in self.output(b)), self.output(b))

    def test_agent_pipe_and_first_still_wait(self):
        self.hold_a()
        cases = {"agent": dict(extra={"CLAUDECODE": "1"}), "first": dict(extra={"SF_CARGO_QUEUE": "first"})}
        procs = {k: self.pty_start("test", tag=k, **v) for k, v in cases.items()}
        procs["pipe"] = self.start("test", tag="pipe", stderr=subprocess.PIPE)
        self.idle()
        for k in procs:
            self.assertFalse(self.seen("start", k), "%s did not wait" % k)
        self.assertTrue(all(p.poll() is None for p in procs.values()))
        for p in procs.values():
            p.kill()
            if p.stderr:
                p.stderr.close()

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


def effective_background(case):
    with mock.patch.dict(os.environ, case.env):
        return cq.effective_mode()


class TestPriority(Base):
    """Classes, the small lane, ageing, waits.tsv and the waiting messages (#517)."""
    SMALL = ("build", "-p", "x")

    def setUp(self):
        super().setUp()
        os.makedirs(self.qdir)
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:
            json.dump({"selected": "background", "effective": "background"}, f)
        self.meminfo = os.path.join(self.tmp, "meminfo")
        self.set_mem(8 * 1024 * 1024)
        self.env["SF_CARGO_QUEUE_MEMINFO"] = self.meminfo
        self.env.pop("SF_CARGO_QUEUE_CLASS", None)

    def set_mem(self, kb):
        with open(self.meminfo, "w") as f:
            f.write("MemTotal: 16000000 kB\nMemAvailable: %d kB\n" % kb)

    def go(self, *args, tag, root, gate=None, **kw):
        """Start a shim with its own build root (CARGO_TARGET_DIR), so that roots never collide by accident."""
        extra = dict(kw.pop("extra", {}), CARGO_TARGET_DIR=os.path.join(self.tmp, "t-" + root))
        return self.start(*args, tag=tag, gate=gate or "g" + tag, extra=extra, **kw)

    def started(self, tag):
        return bool(self.seen("start", tag))

    def tsv(self):
        try:
            with open(os.path.join(self.qdir, "waits.tsv")) as f:
                return [x.rstrip("\n").split("\t") for x in f]
        except OSError:
            return []

    def test_classify_table(self):
        small = [["+nightly", "build", "-p", "x"], ["check", "--package=x"], ["clippy", "-px", "-p", "y"],
                 ["test", "-p", "x", "foo"], ["test", "-p", "x", "--", "name"], ["t", "-p", "x", "--test", "t1"],
                 ["test", "-p", "x", "--lib"], ["nextest", "run", "-p", "x", "-E", "test(a)"],
                 ["build", "-p", "x", "--profile", "dev"], ["run", "-p", "x", "--", "--arg", "v"],
                 ["build", "-p", "x", "--all-targets"]]
        big = [["test", "-p", "x"], ["clippy", "--workspace", "--all-targets"], ["build", "-p", "x", "--release"],
               ["build", "-p", "x", "--profile", "release"], ["build"], ["build", "-p", "a", "-p", "b", "-p", "c"],
               ["bench", "-p", "x"], ["doc", "-p", "x"], ["install", "-p", "x"], ["build", "-p", "x", "--frob", "v"],
               ["test", "--workspace", "-p", "x", "foo"], ["build", "-p", "x", "--exclude", "y"],
               ["nextest", "list", "-p", "x"], ["build", "--all"], ["test", "foo"]]
        with mock.patch.dict(os.environ):
            os.environ.pop("SF_CARGO_QUEUE_CLASS", None)
            for a in small:
                self.assertEqual(cq.classify(a), "small", a)
            for a in big:
                self.assertEqual(cq.classify(a), "big", a)
            os.environ["SF_CARGO_QUEUE_CLASS"] = "big"
            self.assertEqual(cq.classify(self.SMALL), "big")
            os.environ["SF_CARGO_QUEUE_CLASS"] = "small"
            self.assertEqual(cq.classify(["test"]), "small")
            os.environ["SF_CARGO_QUEUE_CLASS"] = "low"
            self.assertEqual(cq.classify(["test"]), "low")
            os.environ["SF_CARGO_QUEUE_CLASS"] = "huge"
            with contextlib.redirect_stderr(io.StringIO()) as err:
                self.assertEqual(cq.classify(["test"]), "big")
            self.assertIn("ignored", err.getvalue())

    def test_small_build_passes_a_busy_big_queue(self):
        self.go("test", tag="a", root="a")
        self.assertTrue(self.until(lambda: self.started("a")))
        self.go("test", tag="b", root="b")
        self.idle(0.2)
        c = self.go(*self.SMALL, tag="c", root="c")
        self.assertTrue(self.until(lambda: self.started("c")), self.output(c))
        self.idle(0.3)
        self.assertFalse(self.started("b"))
        self.assertEqual([r[3] for r in self.tsv()], ["normal", "extra"])
        self.open_gate("gc")

    def test_lane_needs_memory(self):
        self.set_mem(2 * 1024 * 1024)
        self.go("test", tag="a", root="a")
        self.assertTrue(self.until(lambda: self.started("a")))
        self.go(*self.SMALL, tag="c", root="c")
        self.idle(0.5)
        self.assertFalse(self.started("c"), "started with 2 GiB available")
        self.set_mem(8 * 1024 * 1024)
        self.assertTrue(self.until(lambda: self.started("c")), "should start once memory is back")

    def test_lane_refuses_the_root_of_a_running_build(self):
        self.go("test", tag="a", root="same")
        self.assertTrue(self.until(lambda: self.started("a")))
        self.go(*self.SMALL, tag="c", root="same")
        self.idle(0.5)
        self.assertFalse(self.started("c"), "shared a build root with a running build")

    def test_lane_is_for_background_mode_only(self):
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:
            json.dump({"selected": "dev", "effective": "dev"}, f)
        assert cq.MODES["dev"]["slots"] == 2
        self.go("test", tag="a", root="a")
        self.go("test", tag="a2", root="a2")
        self.assertTrue(self.until(lambda: self.started("a2")))
        self.go(*self.SMALL, tag="c", root="c")
        self.idle(0.5)
        self.assertFalse(self.started("c"), "no lane in dev mode")

    def test_small_before_big_when_a_slot_frees(self):
        self.go("test", tag="a", root="a")
        self.assertTrue(self.until(lambda: self.started("a")))
        self.go(*self.SMALL, tag="x", root="x")             # takes the lane
        self.assertTrue(self.until(lambda: self.started("x")))
        self.go("test", tag="b", root="b")
        self.idle(0.15)
        self.go(*self.SMALL, tag="c", root="c")             # lane busy: waits, although later than b
        self.idle(0.3)
        self.assertFalse(self.started("b") or self.started("c"))
        self.open_gate("ga")
        self.assertTrue(self.until(lambda: self.started("c")))
        self.idle(0.3)
        self.assertFalse(self.started("b"))

    def test_a_big_waiter_ages_into_rank_one(self):
        ageing = {"SF_CARGO_QUEUE_AGE": "0.5"}
        self.go("test", tag="a", root="a", extra=ageing)
        self.assertTrue(self.until(lambda: self.started("a")))
        self.go(*self.SMALL, tag="x", root="x", extra=ageing)
        self.assertTrue(self.until(lambda: self.started("x")))
        self.go("test", tag="b", root="b", extra=ageing)
        self.idle(0.9)
        self.go(*self.SMALL, tag="c", root="c", extra=ageing)
        self.idle(0.3)
        self.open_gate("ga")
        self.assertTrue(self.until(lambda: self.started("b")))
        self.idle(0.3)
        self.assertFalse(self.started("c"), "the aged big build should go first")

    def set_mode_file(self, mode):
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:
            json.dump({"selected": mode, "effective": mode}, f)

    LOW = {"SF_CARGO_QUEUE_CLASS": "low"}

    def test_low_ticket_name_and_class_in_the_ticket(self):
        self.set_mode_file("dev")
        self.go("test", tag="a", root="a")
        self.go("test", tag="a2", root="a2")
        self.assertTrue(self.until(lambda: self.started("a2")))
        self.go("test", tag="l", root="l", extra=self.LOW)
        self.assertTrue(self.until(lambda: any(n.startswith("w-3-") for n in self.tickets())), self.tickets())
        with mock.patch.dict(os.environ, self.env):
            run, wait = cq.snapshot()
        self.assertEqual([w["class"] for w in wait], ["low"])
        self.assertEqual(wait[0]["prio"], 3)

    def test_low_waits_behind_every_other_waiter(self):
        self.set_mode_file("dev")
        self.go("test", tag="a", root="a")
        self.go("test", tag="a2", root="a2")
        self.assertTrue(self.until(lambda: self.started("a2")))
        self.go("test", tag="l", root="l", extra=self.LOW)
        self.idle(0.2)
        self.go("test", tag="b", root="b")
        self.idle(0.2)
        self.open_gate("ga")
        self.assertTrue(self.until(lambda: self.started("b")))
        self.idle(0.4)
        self.assertFalse(self.started("l"), "a low build started although a big one was still running or waiting")
        self.open_gate("gb")
        self.open_gate("ga2")
        self.assertTrue(self.until(lambda: self.started("l")))

    def test_aged_low_waiter_still_starts_after_a_later_big(self):
        self.set_mode_file("dev")
        ageing = {"SF_CARGO_QUEUE_AGE": "0.5"}
        self.go("test", tag="a", root="a", extra=ageing)
        self.go("test", tag="a2", root="a2", extra=ageing)
        self.assertTrue(self.until(lambda: self.started("a2")))
        self.go("test", tag="l", root="l", extra=dict(ageing, **self.LOW))
        self.idle(0.9)
        self.go("test", tag="b", root="b", extra=ageing)
        self.idle(0.3)
        self.open_gate("ga")
        self.assertTrue(self.until(lambda: self.started("l") or self.started("b")))
        self.idle(0.3)
        self.assertTrue(self.started("l"), "the aged low waiter should go first (earlier arrival, same rank)")
        self.assertFalse(self.started("b"))

    def test_low_runs_with_two_jobs_in_full_mode(self):
        self.set_mode_file("full")
        self.assertIsNone(cq.MODES["full"]["jobs"])
        self.assertEqual(self.start("test", tag="l", extra=self.LOW).wait(10), 0)
        self.assertEqual(self.jobs_env("l"), ["2", "2", "2"])
        self.assertEqual(self.start("test", tag="k", extra=dict(self.LOW, CARGO_BUILD_JOBS="5")).wait(10), 0)
        self.assertEqual(self.jobs_env("k"), ["5", "2", "2"])

    def jobs_env(self, tag):
        return self.seen("start", tag)[0]["jobs"]

    def test_low_typed_at_a_tty_is_still_rank_three_and_waits(self):
        self.set_mode_file("dev")
        self.go("test", tag="a", root="a")
        self.go("test", tag="a2", root="a2")
        self.assertTrue(self.until(lambda: self.started("a2")))
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        self.go("test", tag="l", root="l", extra=self.LOW, stderr=slave)
        os.close(slave)
        self.assertTrue(self.until(lambda: any(n.startswith("w-3-") for n in self.tickets())), self.tickets())
        self.idle(0.3)
        self.assertFalse(self.started("l"), "a low build typed at a tty skipped the wait")

    def test_low_never_starts_in_background_mode(self):
        self.assertEqual(effective_background(self), "background")
        self.go("test", tag="l", root="l", extra=self.LOW)
        self.idle(0.8)
        self.assertFalse(self.started("l"), "a low build started in background mode with a free slot")
        self.set_mode_file("dev")
        self.assertTrue(self.until(lambda: self.started("l")), "should start once the mode leaves background")

    def test_waits_tsv_lines(self):
        self.assertEqual(self.start("test", tag="z", gate="gz", extra={"CARGO_TARGET_DIR": self.tmp + "/tz"}).poll(), None)
        self.assertTrue(self.until(lambda: self.started("z")))
        self.open_gate("gz")
        self.assertTrue(self.until(lambda: len(self.tsv()) == 1))
        row = self.tsv()[0]
        self.assertEqual(len(row), 10, row)
        self.assertEqual((row[2], row[3], row[4], row[5], row[6]), ("started", "normal", "big", "background", "0"))
        self.assertRegex(row[0], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertRegex(row[1], r"^\d+\.\d$")
        self.assertEqual(row[9], "cargo test")
        # a lane start, then a give-up while waiting, then a passthrough that writes nothing
        self.go("test", tag="a", root="a")
        self.assertTrue(self.until(lambda: self.started("a")))
        self.go(*self.SMALL, tag="c", root="c")
        self.assertTrue(self.until(lambda: len(self.tsv()) == 3))
        self.assertEqual(self.tsv()[2][3], "extra")
        b = self.go("test", tag="b", root="b")
        self.idle(0.4)
        b.terminate()
        b.wait(10)
        self.assertTrue(self.until(lambda: len(self.tsv()) == 4))
        row = self.tsv()[3]
        self.assertEqual((row[2], row[6]), ("gave-up", "2"), row)
        n = len(self.tsv())
        self.assertEqual(self.start("fmt", tag="p").wait(10), 0)
        self.assertEqual(len(self.tsv()), n, "a passthrough must write no line")

    def test_waiting_message_repeats_and_started_after(self):
        self.go("test", tag="a", root="a")
        self.assertTrue(self.until(lambda: self.started("a")))
        b = self.go("test", tag="b", root="b", extra={"SF_CARGO_QUEUE_PRINT": "0.3"})
        self.idle(1.3)
        out = self.output(b)
        lines = [x for x in out.splitlines() if "waiting" in x and "ahead (position" in x]
        self.assertGreaterEqual(len(lines), 2, out)
        self.assertEqual(len([x for x in out.splitlines() if x.startswith("  - pid")]), 1, "ahead list only on change")
        self.assertIn("class big, mode background, 1 at a time + 1 small lane", lines[0])
        self.open_gate("ga")
        self.assertTrue(self.until(lambda: "started after" in self.output(b)), self.output(b))
        self.assertIn("(lane normal)", self.output(b))


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


class TestProfile(Base):
    """The FullIA link to the performance power profile (#477). The fake busctl holds the profile."""

    def setUp(self):
        super().setUp()
        p = mock.patch.dict(os.environ, self.env)
        p.start()
        self.addCleanup(p.stop)
        self.enable_bus("balanced")
        os.makedirs(self.qdir, exist_ok=True)

    def current(self):
        with open(self.busstate) as f:
            return f.read().strip()

    def user_sets(self, name):
        with open(self.busstate, "w") as f:
            f.write(name)

    def record(self):
        try:
            with open(os.path.join(self.qdir, "profile.json")) as f:
                return json.load(f)
        except OSError:
            return None

    def gdbus(self, ms):
        script(os.path.join(self.fakedir, "gdbus"), '#!/bin/sh\necho "(uint64 %d,)"\n' % ms)

    def battery(self, status):
        self.power(os.environ["SF_CARGO_QUEUE_POWER_DIR"], "BAT0", "Battery", status=status)

    def cli(self, *args, extra=None):
        env = dict(os.environ, **(extra or {}))
        return subprocess.run([sys.executable, "-I", SRC, *args], env=env, capture_output=True, text=True, timeout=60)

    def test_enter_full_writes_performance_and_leaving_puts_back_what_was_found(self):
        cq.set_mode(selected="full")
        self.assertEqual(self.bus_writes(), ["performance"])
        self.assertEqual(self.current(), "performance")
        self.assertEqual(self.record(), {"wrote": True, "set": "performance", "found": "balanced"})
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), ["performance", "balanced"])
        self.assertEqual(self.current(), "balanced")
        self.assertIsNone(self.record())

    def test_busctl_is_asked_without_a_prompt_and_with_a_timeout(self):
        cq.set_mode(selected="full")
        for c in self.bus_calls():
            self.assertIn("--system", c)
            self.assertIn("--allow-interactive-authorization=false", c)
            self.assertIn("--timeout=", c)
            self.assertIn("net.hadess.PowerProfiles", c)

    def test_a_profile_the_user_changed_meanwhile_is_not_touched(self):
        cq.set_mode(selected="full")
        self.user_sets("power-saver")
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), ["performance"])
        self.assertEqual(self.current(), "power-saver")
        self.assertIsNone(self.record())

    def test_found_performance_writes_nothing_in_or_out(self):
        self.user_sets("performance")
        cq.set_mode(selected="full")
        self.assertEqual(self.record(), {"wrote": False})
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), [])
        self.assertEqual(self.current(), "performance")
        self.assertIsNone(self.record())

    def test_on_battery_entry_reads_but_writes_nothing_and_leaving_writes_nothing(self):
        self.battery("Discharging")
        cq.set_mode(selected="full")
        self.assertEqual(len(self.bus_calls()), 1)
        self.assertIn("get-property", self.bus_calls()[0])
        self.assertEqual(self.bus_writes(), [])
        self.assertEqual(self.record(), {"wrote": False, "waiting": "balanced"})
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), [])
        self.assertIsNone(self.record())

    def test_leaving_on_battery_still_restores(self):
        cq.set_mode(selected="full")
        self.battery("Discharging")
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), ["performance", "balanced"])

    def test_dev_and_background_never_call_busctl(self):
        for m in ("dev", "background", "dev", "background"):
            cq.set_mode(selected=m)
        cq.set_mode(auto=True)
        cq.auto_tick()
        self.assertEqual(self.bus_calls(), [])
        self.assertIsNone(self.record())

    def test_repeating_full_and_ticking_writes_once(self):
        for _ in range(3):
            cq.set_mode(selected="full")
        for _ in range(3):
            cq.auto_tick()
        self.assertEqual(self.bus_writes(), ["performance"])
        self.assertEqual(len(self.bus_calls()), 2)       # one read, one write

    def test_full_by_away_then_back_to_the_selected_full_keeps_the_profile(self):
        self.gdbus(0)
        cq.set_mode(selected="full", auto=True)
        self.gdbus(700000)
        cq.auto_tick()
        self.assertEqual(cq.read_state()["why"], "away")
        self.gdbus(0)
        st = cq.auto_tick()
        self.assertEqual((st["effective"], st["why"]), ("full", "selected"))
        self.assertEqual(self.bus_writes(), ["performance"])

    def test_auto_enters_when_away_and_restores_when_back(self):
        self.gdbus(0)
        cq.set_mode(selected="dev", auto=True)
        self.assertEqual(self.bus_calls(), [])
        self.gdbus(700000)
        cq.auto_tick()
        self.assertEqual(self.current(), "performance")
        self.gdbus(0)
        cq.auto_tick()
        self.assertEqual(self.current(), "balanced")
        self.assertEqual(self.bus_writes(), ["performance", "balanced"])

    def test_a_mode_file_left_in_dev_with_a_record_is_restored_by_the_tick_even_with_auto_off(self):
        cq.set_mode(selected="full")
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:
            json.dump({"selected": "dev", "effective": "dev", "auto": False, "why": "selected"}, f)
        cq.auto_tick()
        self.assertEqual(self.bus_writes(), ["performance", "balanced"])
        self.assertIsNone(self.record())

    def test_a_mode_file_already_full_without_a_record_is_not_entered_by_the_tick(self):
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:
            json.dump({"selected": "full", "effective": "full", "auto": True, "why": "selected"}, f)
        self.gdbus(0)
        cq.auto_tick()
        self.assertEqual(self.bus_calls(), [])

    def test_a_writer_that_drops_unknown_keys_cannot_erase_the_record(self):
        r = self.cli("mode", "full")
        self.assertEqual(r.returncode, 0, r.stderr)
        with open(os.path.join(self.qdir, "mode.json")) as f:
            self.assertEqual(sorted(json.load(f)), ["auto", "effective", "selected", "why"])
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:      # a stale shim copy rewrites the 4 keys it knows
            json.dump({"selected": "full", "effective": "full", "auto": False, "why": "selected"}, f)
        r = self.cli("mode", "dev")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.bus_writes(), ["performance", "balanced"])
        self.assertEqual(self.current(), "balanced")

    def test_six_processes_ticking_at_once_write_once(self):
        self.gdbus(0)
        cq.set_mode(selected="dev", auto=True)
        # the idle reading is slow, so all six decide "enter" before any of them has written the state
        script(os.path.join(self.fakedir, "gdbus"), '#!/bin/sh\nsleep 0.4\necho "(uint64 700000,)"\n')
        code = ("import importlib.util,sys;s=importlib.util.spec_from_file_location('c',sys.argv[1]);"
                "m=importlib.util.module_from_spec(s);s.loader.exec_module(m);m.auto_tick()")
        env = dict(os.environ, FAKE_BUS_GET_SLEEP="0.3")
        ps = [subprocess.Popen([sys.executable, "-I", "-c", code, SRC], env=env) for _ in range(6)]
        for p in ps:
            self.assertEqual(p.wait(60), 0)
        self.assertEqual(self.bus_writes(), ["performance"])
        self.assertEqual(self.current(), "performance")

    def test_auto_turned_off_during_the_slow_reads_is_not_overwritten(self):
        self.gdbus(700000)
        cq.set_mode(selected="dev", auto=False)
        st = cq.set_mode(effective="full", why="away", only_if_auto=True)
        self.assertEqual(st["effective"], "dev")
        self.assertEqual(self.bus_calls(), [])

    # ---- failures end as a note, never as an exception
    def assert_state_and_slice(self, mode):
        self.assertEqual(cq.read_state()["effective"], mode)
        self.assertTrue([c for c in self.ctl_calls() if "set-property" in c])

    def test_no_busctl_at_all(self):
        os.unlink(os.path.join(self.fakedir, "busctl"))
        with mock.patch.dict(os.environ, {"PATH": self.fakedir}):      # the real busctl must not be reachable
            cq.set_mode(selected="full")
            self.assert_state_and_slice("full")
            self.assertEqual(cq.status_dict()["profile_note"], "read-failed")
            r = self.cli("mode", "dev")
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_read_failure_skips_the_entry_with_a_note(self):
        with mock.patch.dict(os.environ, {"FAKE_BUS_GET_RC": "1"}):
            cq.set_mode(selected="full")
            self.assert_state_and_slice("full")
            self.assertEqual(self.bus_writes(), [])
            self.assertEqual(cq.status_dict()["profile_note"], "read-failed")
            r = self.cli("mode", "full")
            self.assertEqual(r.returncode, 0)
            self.assertIn("cannot read the power profile", r.stderr)
            self.assertIn("power profile: cannot read", self.cli("status").stdout)
        cq.set_mode(selected="dev")
        self.assertEqual(cq.status_dict()["profile_note"], "")
        self.assertEqual(self.bus_writes(), [])

    def test_a_value_that_is_not_a_known_profile_reads_as_a_failure(self):
        self.user_sets("turbo")
        cq.set_mode(selected="full")
        self.assertEqual(self.bus_writes(), [])
        self.assertEqual(cq.status_dict()["profile_note"], "read-failed")
        self.assertEqual(cq.write_profile("turbo"), "fail")
        self.assertEqual(self.bus_writes(), [])

    def test_write_refused_leaves_the_profile_and_a_note_and_leaving_writes_nothing(self):
        with mock.patch.dict(os.environ, {"FAKE_BUS_SET_RC": "1"}):
            cq.set_mode(selected="full")
        self.assert_state_and_slice("full")
        self.assertEqual(cq.status_dict()["profile_note"], "write-failed")
        self.assertEqual(len(self.bus_writes()), 1)
        cq.set_mode(selected="dev")
        self.assertEqual(len(self.bus_writes()), 1)
        self.assertEqual(self.current(), "balanced")

    def test_hanging_read_is_cut_by_the_timeout(self):
        with mock.patch.dict(os.environ, {"FAKE_BUS_GET_SLEEP": "20", "SF_CARGO_QUEUE_PROFILE_TIMEOUT": "0.3"}):
            t0 = time.time()
            cq.set_mode(selected="full")
            self.assertLess(time.time() - t0, 5)
        self.assert_state_and_slice("full")
        self.assertEqual(cq.status_dict()["profile_note"], "read-failed")

    def test_hanging_write_keeps_the_record_and_the_exit_compares_before_writing(self):
        with mock.patch.dict(os.environ, {"FAKE_BUS_SET_SLEEP": "20", "SF_CARGO_QUEUE_PROFILE_TIMEOUT": "0.3"}):
            cq.set_mode(selected="full")
        self.assertEqual(self.record()["wrote"], True)
        self.assertEqual(self.current(), "balanced")        # the hung write never landed
        cq.set_mode(selected="dev")
        self.assertEqual(len(self.bus_writes()), 1)         # only the hung attempt: nothing to put back
        self.assertIsNone(self.record())

    def test_refused_restore_keeps_the_record_and_the_next_tick_retries(self):
        cq.set_mode(selected="full")
        with mock.patch.dict(os.environ, {"FAKE_BUS_SET_RC": "1"}):
            cq.set_mode(selected="dev")
        self.assertEqual(self.current(), "performance")
        self.assertEqual(self.record()["note"], "restore-failed")
        self.assertEqual(cq.status_dict()["profile_note"], "restore-failed")
        cq.auto_tick()
        self.assertEqual(self.current(), "balanced")
        self.assertIsNone(self.record())

    def test_unreadable_profile_on_exit_keeps_the_record(self):
        cq.set_mode(selected="full")
        with mock.patch.dict(os.environ, {"FAKE_BUS_GET_RC": "1"}):
            cq.set_mode(selected="dev")
        self.assertEqual(self.record()["note"], "restore-failed")
        cq.auto_tick()
        self.assertEqual(self.current(), "balanced")

    def test_queued_shim_with_failing_busctl_keeps_the_build_exit_code(self):
        with open(os.path.join(self.qdir, "mode.json"), "w") as f:      # leftover record, mode no longer FullIA
            json.dump({"selected": "dev", "effective": "dev", "auto": True, "why": "selected"}, f)
        with open(os.path.join(self.qdir, "profile.json"), "w") as f:
            json.dump({"wrote": True, "set": "performance", "found": "balanced"}, f)
        self.gdbus(0)
        p = self.start("test", tag="q", extra={"FAKE_BUS_GET_RC": "1", "FAKE_EXIT": "3"})
        self.assertEqual(p.wait(20), 3, self.output(p))
        self.assertTrue(self.seen("start", "q"))
        self.assertTrue(self.bus_calls())

    def test_an_exception_inside_the_link_never_reaches_the_caller(self):
        os.mkdir(os.path.join(self.qdir, "profile.json"))      # the record cannot be written: a real exception
        st = cq.set_mode(selected="full")
        self.assertEqual(st["effective"], "full")
        self.assertEqual(self.bus_writes(), [])
        self.gdbus(700000)
        cq.set_mode(selected="dev", auto=True)
        cq.auto_tick()
        self.assertEqual(cq.read_state()["effective"], "full")
        self.assertEqual(self.bus_writes(), [])
        p = self.start("test", tag="x", extra={"FAKE_EXIT": "3"})
        self.assertEqual(p.wait(20), 3, self.output(p))

    # ---- #477 lot 2: while FullIA lasts the tick follows the power source (mains = performance, battery = found)
    def test_unplugging_in_full_puts_the_found_profile_back_once(self):
        cq.set_mode(selected="full")
        self.battery("Discharging")
        cq.auto_tick()
        self.assertEqual(self.bus_writes(), ["performance", "balanced"])
        self.assertEqual(self.current(), "balanced")
        self.assertEqual(self.record(), {"wrote": False, "waiting": "balanced"})
        self.assertEqual(cq.read_state()["effective"], "full")
        n = len(self.bus_calls())
        cq.auto_tick()
        self.assertEqual(len(self.bus_calls()), n)

    def test_plugging_back_sets_performance_and_leaving_restores(self):
        cq.set_mode(selected="full")
        self.battery("Discharging")
        cq.auto_tick()
        self.battery("Charging")
        cq.auto_tick()
        self.assertEqual(self.bus_writes(), ["performance", "balanced", "performance"])
        self.assertEqual(self.record(), {"wrote": True, "set": "performance", "found": "balanced"})
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes()[-1], "balanced")
        self.assertEqual(self.current(), "balanced")
        self.assertIsNone(self.record())

    def test_full_entered_on_battery_and_then_plugged_in_enters_at_the_tick(self):
        self.battery("Discharging")
        cq.set_mode(selected="full")
        self.assertEqual(self.bus_writes(), [])
        self.battery("Charging")
        cq.auto_tick()
        self.assertEqual(self.bus_writes(), ["performance"])
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), ["performance", "balanced"])
        self.assertEqual(self.current(), "balanced")

    def test_user_change_while_waiting_on_battery_is_final_for_this_full(self):
        self.battery("Discharging")
        cq.set_mode(selected="full")
        self.user_sets("power-saver")
        self.battery("Charging")
        cq.auto_tick()
        self.assertEqual(self.bus_writes(), [])
        self.assertEqual(self.record(), {"wrote": False})
        n = len(self.bus_calls())
        for _ in range(3):
            cq.auto_tick()
        self.assertEqual(len(self.bus_calls()), n)
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), [])
        self.assertEqual(self.current(), "power-saver")

    def test_user_change_in_full_on_mains_is_not_overwritten_when_unplugging_or_replugging(self):
        cq.set_mode(selected="full")
        self.user_sets("power-saver")
        self.battery("Discharging")
        cq.auto_tick()
        self.battery("Charging")
        cq.auto_tick()
        self.assertEqual(self.bus_writes(), ["performance"])
        self.assertEqual(self.current(), "power-saver")
        self.assertEqual(self.record(), {"wrote": False})

    def test_found_performance_is_never_written_when_unplugging_and_replugging(self):
        self.user_sets("performance")
        cq.set_mode(selected="full")
        self.battery("Discharging")
        cq.auto_tick()
        self.battery("Charging")
        cq.auto_tick()
        cq.set_mode(selected="dev")
        self.assertEqual(self.bus_writes(), [])
        self.assertEqual(self.current(), "performance")

    def test_restore_refused_on_unplug_keeps_the_record_and_the_next_tick_retries(self):
        cq.set_mode(selected="full")
        self.battery("Discharging")
        with mock.patch.dict(os.environ, {"FAKE_BUS_SET_RC": "1"}):
            cq.auto_tick()
        self.assertEqual(self.current(), "performance")
        self.assertEqual(self.record()["wrote"], True)
        self.assertEqual(self.record()["note"], "restore-failed")
        cq.auto_tick()
        self.assertEqual(self.current(), "balanced")
        self.assertEqual(self.record(), {"wrote": False, "waiting": "balanced"})

    def test_read_failing_on_the_plug_in_tick_changes_nothing_and_the_next_tick_enters(self):
        self.battery("Discharging")
        cq.set_mode(selected="full")
        self.battery("Charging")
        before = self.record()
        with mock.patch.dict(os.environ, {"FAKE_BUS_GET_RC": "1"}):
            cq.auto_tick()
        self.assertEqual(self.record(), before)
        self.assertEqual(self.bus_writes(), [])
        cq.auto_tick()
        self.assertEqual(self.bus_writes(), ["performance"])

    def test_no_busctl_call_on_mains_with_written_or_on_battery_while_waiting(self):
        cq.set_mode(selected="full")
        n = len(self.bus_calls())
        for _ in range(5):
            cq.auto_tick()
        self.assertEqual(len(self.bus_calls()), n)
        cq.set_mode(selected="dev")
        self.battery("Discharging")
        cq.set_mode(selected="full")
        n = len(self.bus_calls())
        for _ in range(5):
            cq.auto_tick()
        self.assertEqual(len(self.bus_calls()), n)

    def test_six_processes_ticking_right_after_a_plug_in_write_once(self):
        self.battery("Discharging")
        cq.set_mode(selected="full")
        self.battery("Charging")
        code = ("import importlib.util,sys;s=importlib.util.spec_from_file_location('c',sys.argv[1]);"
                "m=importlib.util.module_from_spec(s);s.loader.exec_module(m);m.auto_tick()")
        env = dict(os.environ, FAKE_BUS_GET_SLEEP="0.4")
        ps = [subprocess.Popen([sys.executable, "-I", "-c", code, SRC], env=env) for _ in range(6)]
        for p in ps:
            self.assertEqual(p.wait(60), 0)
        self.assertEqual(self.bus_writes(), ["performance"])

    def test_corrupt_record_reads_as_no_record(self):
        for text in ("{not json", "[]", '{"wrote": "yes"}', '{"wrote": true, "set": "turbo", "found": "balanced"}'):
            with open(os.path.join(self.qdir, "profile.json"), "w") as f:
                f.write(text)
            self.assertIsNone(cq.read_profile_record(), text)
            self.assertEqual(cq.status_dict()["profile_note"], "")

    def test_profile_restore_verb_puts_back_and_forgets(self):
        cq.set_mode(selected="full")
        r = self.cli("profile-restore")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.current(), "balanced")
        self.assertIsNone(self.record())
        self.assertEqual(self.cli("profile-restore").returncode, 0)
        self.assertEqual(len(self.bus_writes()), 2)

    def test_the_link_is_off_by_default_with_a_state_folder_and_on_without_one(self):
        with mock.patch.dict(os.environ):
            os.environ.pop("SF_CARGO_QUEUE_PROFILE")
            self.assertFalse(cq.profile_enabled())
            os.environ.pop("SF_CARGO_QUEUE_DIR")
            self.assertTrue(cq.profile_enabled())
            os.environ["SF_CARGO_QUEUE_PROFILE"] = "off"
            self.assertFalse(cq.profile_enabled())

    def test_only_full_is_linked(self):
        self.assertEqual({k: m["profile"] for k, m in cq.MODES.items()},
                         {"full": "performance", "dev": None, "background": None})


class TestProfileGuard(Base):
    def test_a_state_folder_without_opt_in_never_calls_busctl(self):
        with mock.patch.dict(os.environ, self.env):
            self.assertFalse(cq.profile_enabled())
            cq.set_mode(selected="full")
            cq.auto_tick()
            cq.set_mode(selected="dev")
        self.assertEqual(self.start("test", tag="g").wait(10), 0)
        # check_canary asserts the log is still empty


class TestInstall(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="cq-install-")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.fake = os.path.join(self.tmp, "fakebin")
        os.makedirs(self.fake)
        script(os.path.join(self.fake, "systemctl"), '#!/bin/sh\necho "$*" >> "$HOME/ctl.log"\n')
        script(os.path.join(self.fake, "busctl"), FAKE_BUSCTL)     # the real power profile is never reachable from here
        self.buslog, self.busstate = os.path.join(self.tmp, "bus.log"), os.path.join(self.tmp, "bus.state")
        self.env = {"HOME": self.tmp, "PATH": self.fake + ":/usr/bin:/bin", "FAKE_BUSLOG": self.buslog, "FAKE_BUSSTATE": self.busstate}
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

    def bus_lines(self):
        try:
            with open(self.buslog) as f:
                return f.read().splitlines()
        except OSError:
            return []

    def test_uninstall_puts_the_power_profile_back_before_removing_anything(self):
        self.sh("cargo-queue-install.sh")
        self.assertEqual(self.bus_lines(), [])
        state = os.path.join(self.tmp, ".local/state/stokesforge/cargo-queue")
        os.makedirs(state)
        with open(os.path.join(state, "profile.json"), "w") as f:
            json.dump({"wrote": True, "set": "performance", "found": "power-saver"}, f)
        with open(self.busstate, "w") as f:
            f.write("performance")
        self.sh("cargo-queue-uninstall.sh")
        with open(self.busstate) as f:
            self.assertEqual(f.read(), "power-saver")
        self.assertFalse(os.path.exists(os.path.join(state, "profile.json")))
        self.assertTrue(os.path.isdir(state))        # the state folder is kept

    def test_uninstall_without_a_record_calls_nothing(self):
        self.sh("cargo-queue-install.sh")
        self.sh("cargo-queue-uninstall.sh")
        self.assertEqual(self.bus_lines(), [])

    def test_slice_name_has_no_dash(self):
        self.assertNotIn("-", cq.SLICE.replace(".slice", ""))


if __name__ == "__main__":
    unittest.main()

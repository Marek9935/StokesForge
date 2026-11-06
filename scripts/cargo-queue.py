#!/usr/bin/python3 -I
"""Local build queue and build modes for cargo (#476).

One stdlib-only file, two roles.

* Shim: installed as a copy named `cargo` in front of the real one (see
  scripts/cargo-queue-install.sh). Heavy subcommands wait in a small local queue (one file per
  waiting cargo, held under flock), then run the real cargo inside a systemd user scope whose
  slice carries the limits of the current mode. Everything else goes straight to the real cargo.
* CLI: `cargo-queue.py mode [full|dev|background]`, `auto on|off`, `status [--json]`, `apply`.
  suivi.py imports this file to drive the same functions from the tracking page.

The numbers of the modes live only in MODES below. A queued build gets its job count from the mode
when it starts and keeps it; a live mode change only moves weight, memory, quota and affinity.
"""
import ctypes
import fcntl
import json
import os
import re
import signal
import subprocess
import sys
import time

# Per mode. slots: builds running at once. cpu_weight: CPUWeight of the slice (only matters under
# contention, so the GUI wins). cpu_quota_pct: percent of the whole machine (None = no cap).
# cpus: None or "ecore" (E-cores only, from /sys/devices/cpu_atom/cpus; ignored when absent).
# jobs: cargo/nextest/libtest job count given to a queued build (None = cargo's default).
# memory_high: MemoryHigh of the slice. fallback_nice: nice value when no systemd scope is possible.
# Measured 2026-11-06 (clean workspace build, power-saver 20 W): a 50 % CPUQuota was slower, used more
# energy and disturbed frames more than `-j 11`; E-cores only was the slowest; MemoryHigh 12G cost no
# time on a clean test build (anon memory of one test build is 5-6 GiB, the rest is cache). Two builds at
# once gain 14-19 % over two in a row; a third gains 6 % on plain builds and nothing on test builds. Table in #476.
# cpu_quota_pct and cpus stay as knobs, unused by the current modes.
MODES = {
    "full": {"slots": 2, "cpu_weight": 100, "cpu_quota_pct": None, "cpus": None, "jobs": None, "memory_high": "20G", "fallback_nice": 0},
    "dev": {"slots": 2, "cpu_weight": 20, "cpu_quota_pct": None, "cpus": None, "jobs": 11, "memory_high": "20G", "fallback_nice": 10},
    "background": {"slots": 1, "cpu_weight": 10, "cpu_quota_pct": None, "cpus": None, "jobs": 6, "memory_high": "12G", "fallback_nice": 19},
}
DEFAULT_MODE = "dev"
SLICE = "sfbuild.slice"     # no dash: a dash would nest the slice under another one
MARKER = "SF_CARGO_QUEUE_HELD"

HEAVY = {"build", "b", "check", "c", "clippy", "test", "t", "bench", "nextest", "run", "r", "doc", "d",
         "rustc", "rustdoc", "fix", "install", "miri"}
GLOBAL_WITH_VALUE = {"--color", "--config", "-Z", "-C"}
HELP_FLAGS = {"--help", "-h", "--version", "-V", "--list"}
FORWARDED = (signal.SIGTERM, signal.SIGINT, signal.SIGHUP, signal.SIGQUIT)
AWAY_MS = 600000


# ---------------------------------------------------------------- paths and state
def state_dir():
    d = os.environ.get("SF_CARGO_QUEUE_DIR")
    if d:
        return d
    base = os.environ.get("XDG_STATE_HOME") or os.path.join(os.path.expanduser("~"), ".local", "state")
    return os.path.join(base, "stokesforge", "cargo-queue")


def slice_name():
    return os.environ.get("SF_CARGO_QUEUE_SLICE") or SLICE


def installed_path():
    p = os.environ.get("SF_CARGO_QUEUE_INSTALLED")
    if p:
        return p
    return os.path.join(os.path.expanduser("~"), ".local", "share", "stokesforge", "cargo-queue", "bin", "cargo")


def poll_seconds():
    return float(os.environ.get("SF_CARGO_QUEUE_POLL", "0.5"))


def write_atomic(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = "%s.%d.tmp" % (path, os.getpid())
    with open(tmp, "w") as f:
        f.write(text)
    os.replace(tmp, path)


def read_state():
    try:
        with open(os.path.join(state_dir(), "mode.json")) as f:
            d = json.load(f)
        if not isinstance(d, dict):
            d = {}
    except (OSError, ValueError):
        d = {}
    sel = d.get("selected") if d.get("selected") in MODES else DEFAULT_MODE
    eff = d.get("effective") if d.get("effective") in MODES else sel
    return {"selected": sel, "auto": bool(d.get("auto")), "effective": eff, "why": str(d.get("why") or "selected")}


def write_state(st):
    write_atomic(os.path.join(state_dir(), "mode.json"), json.dumps(st, sort_keys=True) + "\n")


def effective_mode():
    return read_state()["effective"]


# ---------------------------------------------------------------- applying a mode
def nproc():
    return os.cpu_count() or 1


def cpu_list(text):
    out = []
    for part in text.strip().split(","):
        if not part:
            continue
        a, _, b = part.partition("-")
        out.extend(range(int(a), int(b or a) + 1))
    return out


def mode_cpus(mode):
    """CPU set for the mode, or None for 'all'."""
    if MODES[mode].get("cpus") != "ecore":
        return None
    try:
        with open("/sys/devices/cpu_atom/cpus") as f:
            return cpu_list(f.read()) or None
    except (OSError, ValueError):
        return None


def all_cpus():
    try:
        with open("/sys/devices/system/cpu/online") as f:
            return cpu_list(f.read())
    except (OSError, ValueError):
        return list(range(nproc()))


def slice_properties(mode):
    m = MODES[mode]
    quota = "" if m["cpu_quota_pct"] is None else "%d%%" % (nproc() * m["cpu_quota_pct"])
    return ["CPUWeight=%d" % m["cpu_weight"], "CPUQuota=" + quota, "MemoryHigh=%s" % m["memory_high"]]


def apply_slice(mode):
    """Set the slice limits for `mode` on the user manager. True when it worked."""
    try:
        r = subprocess.run(["systemctl", "--user", "set-property", "--runtime", slice_name(), *slice_properties(mode)],
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        return r.returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def slice_cgroup_dir():
    """Directory of the slice in the cgroup tree, from the user manager's own path."""
    try:
        with open("/proc/self/cgroup") as f:
            line = [x for x in f.read().splitlines() if x.startswith("0::")][0]
    except (OSError, IndexError):
        return None
    parts = line[3:].split("/")
    for i, p in enumerate(parts):
        if p.startswith("user@") and p.endswith(".service"):
            root = os.environ.get("SF_CARGO_QUEUE_CGROOT", "/sys/fs/cgroup")
            return os.path.join(root, *parts[:i + 1], slice_name())
    return None


def cgroup_pids(d):
    pids = []
    for base, _dirs, _files in os.walk(d):
        try:
            with open(os.path.join(base, "cgroup.procs")) as f:
                pids += [int(x) for x in f.read().split()]
        except (OSError, ValueError):
            pass
    return pids


def move_running(mode):
    """Put the threads of the slice's processes on the CPU set of `mode`. Returns how many were set."""
    d = slice_cgroup_dir()
    if not d or not os.path.isdir(d):
        return 0
    cpus = mode_cpus(mode) or all_cpus()
    n = 0
    for pid in cgroup_pids(d):
        try:
            tids = [int(t) for t in os.listdir("/proc/%d/task" % pid)]
        except (OSError, ValueError):
            continue
        for tid in tids:
            try:
                os.sched_setaffinity(tid, cpus)
                n += 1
            except OSError:
                pass
    return n


def apply_mode(mode):
    ok = apply_slice(mode)
    move_running(mode)
    return ok


# ---------------------------------------------------------------- auto variation
def auto_decide(selected, idle_ms, on_battery, away_ms=AWAY_MS):
    """(mode, why): battery wins, then an absent user, else the selected mode. idle_ms None = unknown."""
    if on_battery:
        return "background", "battery"
    if idle_ms is not None and idle_ms >= away_ms:
        return "full", "away"
    return selected, "selected"


def read_idle_ms():
    try:
        r = subprocess.run(["gdbus", "call", "--session", "--dest", "org.gnome.Mutter.IdleMonitor",
                            "--object-path", "/org/gnome/Mutter/IdleMonitor/Core",
                            "--method", "org.gnome.Mutter.IdleMonitor.GetIdletime"],
                           stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None
    m = re.search(r"uint64\s+(\d+)", r.stdout) if r.returncode == 0 else None
    return int(m.group(1)) if m else None


def read_on_battery(root=None):
    """True only when a laptop battery (type Battery, name BAT*) reports Discharging. Mains, USB and
    device batteries (a pen, a mouse) are ignored: their `online`/`status` mean something else."""
    root = root or os.environ.get("SF_CARGO_QUEUE_POWER_DIR", "/sys/class/power_supply")
    try:
        names = sorted(os.listdir(root))
    except OSError:
        return False
    for n in names:
        if not n.startswith("BAT"):
            continue
        try:
            with open(os.path.join(root, n, "type")) as f:
                if f.read().strip() != "Battery":
                    continue
            with open(os.path.join(root, n, "status")) as f:
                if f.read().strip() == "Discharging":
                    return True
        except OSError:
            pass
    return False


def set_mode(selected=None, auto=None, effective=None, why=None, apply=True):
    """Write mode.json and apply the effective mode to the slice. Returns the new state."""
    st = read_state()
    if selected is not None:
        if selected not in MODES:
            raise ValueError("unknown mode: %s" % selected)
        st["selected"] = selected
    if auto is not None:
        st["auto"] = bool(auto)
    if effective is None:
        if st["auto"]:
            effective, why = auto_decide(st["selected"], read_idle_ms(), read_on_battery())
        else:
            effective, why = st["selected"], "selected"
    st["effective"], st["why"] = effective, why or "selected"
    write_state(st)
    if apply:
        apply_mode(st["effective"])
    return st


def auto_tick():
    """One pass of the auto variation; applies a change of the effective mode. Returns the state."""
    st = read_state()
    if not st["auto"]:
        return st
    eff, why = auto_decide(st["selected"], read_idle_ms(), read_on_battery())
    if (eff, why) != (st["effective"], st["why"]):
        return set_mode(effective=eff, why=why)
    return st


# ---------------------------------------------------------------- the queue
def queue_dir():
    return os.path.join(state_dir(), "queue")


class QueueLock:
    """The short lock around every look at the queue. Its fd is not inherited (PEP 446)."""

    def __enter__(self):
        os.makedirs(queue_dir(), exist_ok=True)
        self.fd = os.open(os.path.join(state_dir(), "queue.lock"), os.O_CREAT | os.O_RDWR, 0o600)
        fcntl.flock(self.fd, fcntl.LOCK_EX)
        return self

    def __exit__(self, *a):
        os.close(self.fd)


def probe_dead(path):
    """True when nobody holds the ticket's flock (its owner is gone). Never unlinks."""
    try:
        fd = os.open(path, os.O_RDONLY)
    except OSError:
        return False
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return True
    except OSError:
        return False
    finally:
        os.close(fd)


def read_ticket(path):
    try:
        with open(path) as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def list_tickets(purge=False, own=None):
    """Sorted [(name, info)] of live tickets. With purge, unlinks the dead ones. Call under QueueLock."""
    out = []
    try:
        names = sorted(n for n in os.listdir(queue_dir()) if n[:2] in ("w-", "r-") and n.endswith(".json"))
    except OSError:
        return out
    for n in names:
        p = os.path.join(queue_dir(), n)
        if n != own and probe_dead(p):
            if purge:
                try:
                    os.unlink(p)
                except OSError:
                    pass
            continue
        out.append((n, read_ticket(p)))
    return out


def snapshot():
    if not os.path.isdir(queue_dir()):
        return [], []
    with QueueLock():
        items = list_tickets()
    run = [dict(i, prio=int(n.split("-")[1])) for n, i in items if n.startswith("r-")]
    wait = [dict(i, prio=int(n.split("-")[1])) for n, i in items if n.startswith("w-")]
    return run, wait


def typed_by_user():
    """A person typed this in a terminal: stderr is a tty and no agent marker. SF_CARGO_QUEUE=first keeps
    its own meaning (head of the queue, still waits for a slot), so it never counts as typed."""
    return (sys.stderr.isatty() and not os.environ.get("CLAUDECODE")
            and os.environ.get("SF_CARGO_QUEUE") != "first")


def human_prio():
    if os.environ.get("SF_CARGO_QUEUE") == "first":
        return 0
    return 0 if sys.stderr.isatty() and not os.environ.get("CLAUDECODE") else 1


class Ticket:
    def __init__(self, command):
        self.path = None
        self.fd = None
        self.name = None
        self.command = command
        with QueueLock():
            self.name = "w-%d-%020d-%d.json" % (human_prio(), time.time_ns(), os.getpid())
            self.path = os.path.join(queue_dir(), self.name)
            self.fd = os.open(self.path, os.O_CREAT | os.O_RDWR, 0o600)
            fcntl.flock(self.fd, fcntl.LOCK_EX)
            self.write()

    def write(self):
        data = {"pid": os.getpid(), "cwd": os.getcwd(), "command": self.command, "start": time.time(),
                "target": os.environ.get("CARGO_TARGET_DIR", "")}
        os.ftruncate(self.fd, 0)
        os.pwrite(self.fd, json.dumps(data).encode(), 0)

    def promote(self):
        new = "r-" + self.name[2:]
        os.rename(self.path, os.path.join(queue_dir(), new))   # same inode: the flock stays
        self.name, self.path = new, os.path.join(queue_dir(), new)
        self.write()

    def release(self):
        if self.fd is None:
            return
        try:
            os.unlink(self.path)
        except OSError:
            pass
        os.close(self.fd)
        self.fd = None


def describe_ticket(info, running):
    since = ""
    if info.get("start"):
        since = ", %s %d min" % ("running" if running else "waiting", int((time.time() - info["start"]) // 60))
    return "pid %s%s in %s: %s" % (info.get("pid", "?"), since, info.get("cwd", "?"), info.get("command", "?"))


def wait_turn(ticket):
    """Block until this ticket may run. Prints who is ahead. A command typed by the user skips the wait."""
    last_key, last_print, last_auto = None, 0.0, 0.0
    if typed_by_user():     # the user's own terminal starts at once; the ticket still counts as running
        with QueueLock():
            list_tickets(purge=True, own=ticket.name)
            ticket.promote()
        return
    while True:
        now = time.time()
        if now - last_auto > 10:
            last_auto = now
            if read_state()["auto"]:
                auto_tick()
        with QueueLock():
            items = list_tickets(purge=True, own=ticket.name)
            slots = MODES[effective_mode()]["slots"]
            running = [x for x in items if x[0].startswith("r-")]
            waiting = [x for x in items if x[0].startswith("w-")]
            if len(running) < slots and waiting and waiting[0][0] == ticket.name:
                ticket.promote()
                return
            ahead = running + [x for x in waiting if x[0] < ticket.name]
        key = tuple(n for n, _ in ahead)
        if key != last_key or now - last_print >= 60:
            last_key, last_print = key, now
            lines = ["cargo-queue: waiting, %d ahead (mode %s, %d at a time). SF_CARGO_QUEUE=off skips the queue."
                     % (len(ahead), effective_mode(), slots)]
            lines += ["  - " + describe_ticket(i, n.startswith("r-")) for n, i in ahead]
            print("\n".join(lines), file=sys.stderr, flush=True)
        time.sleep(poll_seconds())


# ---------------------------------------------------------------- the shim
def find_real_cargo():
    me = os.path.realpath(__file__)
    mydir = os.path.dirname(me)
    for d in os.environ.get("PATH", "").split(os.pathsep):
        if not d:
            continue
        if os.path.realpath(d) == mydir:
            continue
        c = os.path.join(d, "cargo")
        if os.path.isfile(c) and os.access(c, os.X_OK) and os.path.realpath(c) != me:
            return c
    return None


def parse_args(args):
    """(index, name) of the subcommand, skipping +toolchain and global flags; (None, None) if absent."""
    i = 0
    while i < len(args):
        a = args[i]
        if a.startswith("+") and i == 0:
            i += 1
        elif a in GLOBAL_WITH_VALUE:
            i += 2
        elif a.startswith("-"):
            i += 1
        else:
            return i, a
    return None, None


def wants_help(args):
    for a in args:
        if a == "--":
            return False
        if a in HELP_FLAGS:
            return True
    return False


def parent_comm():
    try:
        with open("/proc/%d/comm" % os.getppid()) as f:
            return f.read().strip()
    except OSError:
        return ""


def passes_through(args):
    """Why this call skips the queue, or None."""
    if os.environ.get("SF_CARGO_QUEUE") == "off":
        return "off"
    if os.environ.get(MARKER):
        return "nested"
    if parent_comm().startswith("rust-analyzer"):
        return "rust-analyzer"
    idx, sub = parse_args(args)
    if sub is None or sub not in HEAVY:
        return "not heavy"
    if wants_help(args):
        return "help"
    return None


def set_pdeathsig():
    try:
        ctypes.CDLL(None, use_errno=True).prctl(1, int(signal.SIGTERM), 0, 0, 0)   # PR_SET_PDEATHSIG
    except (OSError, AttributeError):
        pass


class Run:
    """Signal state shared with the handlers: the child to forward to, the ticket to drop."""
    child = None
    ticket = None


def on_signal(signum, _frame):
    if Run.child is not None and Run.child.poll() is None:
        try:
            Run.child.send_signal(signum)
        except OSError:
            pass
        return
    die_by(signum)


def die_by(signum):
    if Run.ticket is not None:
        Run.ticket.release()
    signal.signal(signum, signal.SIG_DFL)
    os.kill(os.getpid(), signum)
    sys.exit(128 + signum)


def run_in_scope(real, args, mode, use_scope):
    """Run the real cargo as a child and return its exit status (negative: killed by that signal)."""
    cpus = mode_cpus(mode)
    nice = MODES[mode]["fallback_nice"]
    env = dict(os.environ)
    env[MARKER] = "1"
    jobs = MODES[mode].get("jobs")
    if jobs:       # the caller's own value wins; a -j on the command line wins over the variable
        for k in ("CARGO_BUILD_JOBS", "NEXTEST_TEST_THREADS", "RUST_TEST_THREADS"):
            env.setdefault(k, str(jobs))

    def pre():
        try:
            with open("/proc/self/oom_score_adj", "w") as f:
                f.write("500")
        except OSError:
            pass
        set_pdeathsig()
        if cpus:
            try:
                os.sched_setaffinity(0, cpus)
            except OSError:
                pass
        if not use_scope and nice:
            try:
                os.nice(nice)
            except OSError:
                pass

    cmd = [real, *args]
    if use_scope:
        cmd = ["systemd-run", "--user", "--scope", "--quiet", "--collect", "--slice=" + slice_name(), "--"] + cmd
    Run.child = subprocess.Popen(cmd, env=env, preexec_fn=pre)
    try:
        return Run.child.wait()
    finally:
        Run.child = None


def shim(argv):
    args = argv[1:]
    real = find_real_cargo()
    if real is None:
        print("cargo-queue: no real cargo found on PATH after this shim", file=sys.stderr)
        return 127
    if passes_through(args):
        os.execv(real, [real, *args])
    idx, sub = parse_args(args)
    for s in FORWARDED:
        signal.signal(s, on_signal)
    ticket = Ticket("cargo " + " ".join(args))
    Run.ticket = ticket
    wait_turn(ticket)
    mode = effective_mode()
    use_scope = apply_slice(mode)
    if not use_scope:
        print("cargo-queue: warning: cannot set the build slice (no user systemd?); running without limits, nice %d"
              % MODES[mode]["fallback_nice"], file=sys.stderr, flush=True)
    is_run = sub in ("run", "r")
    run_args = args
    if is_run:      # build in the queue; the application itself runs outside it
        end = args.index("--") if "--" in args else len(args)
        run_args = args[:idx] + ["build"] + args[idx + 1:end]
    rc = run_in_scope(real, run_args, mode, use_scope)
    if rc < 0:
        die_by(-rc)
    ticket.release()
    Run.ticket = None
    if is_run and rc == 0:
        os.execv(real, [real, *args])
    return rc


# ---------------------------------------------------------------- status and CLI
def status_dict():
    st = read_state()
    me = os.path.realpath(__file__)
    inst = installed_path()
    installed = os.path.isfile(inst)
    stale = False
    if installed:
        try:
            with open(inst, "rb") as a, open(me, "rb") as b:
                stale = a.read() != b.read()
        except OSError:
            stale = True
    try:
        run, wait = snapshot()
    except OSError:
        run, wait = [], []
    return {"installed": installed, "stale": stale, "selected": st["selected"], "effective": st["effective"],
            "auto": st["auto"], "why": st["why"], "modes": MODES, "nproc": nproc(), "running": run, "waiting": wait}


def main(argv):
    if len(argv) < 2 or argv[1] in ("-h", "--help"):
        print(__doc__)
        return 0 if len(argv) > 1 else 2
    cmd, rest = argv[1], argv[2:]
    if cmd == "mode":
        if not rest:
            st = read_state()
            print("%s (effective %s, %s)" % (st["selected"], st["effective"], st["why"]))
            return 0
        if rest[0] not in MODES:
            print("unknown mode %r; one of: %s" % (rest[0], ", ".join(MODES)), file=sys.stderr)
            return 2
        st = set_mode(selected=rest[0])
        print("mode %s (effective %s)" % (st["selected"], st["effective"]))
        return 0
    if cmd == "auto":
        if rest not in (["on"], ["off"]):
            print("usage: auto on|off", file=sys.stderr)
            return 2
        st = set_mode(auto=rest[0] == "on")
        print("auto %s (effective %s, %s)" % (rest[0], st["effective"], st["why"]))
        return 0
    if cmd == "status":
        d = status_dict()
        if "--json" in rest:
            print(json.dumps(d, sort_keys=True))
        else:
            print("mode %s (effective %s, %s), auto %s, %d running, %d waiting, %s" % (
                d["selected"], d["effective"], d["why"], "on" if d["auto"] else "off", len(d["running"]),
                len(d["waiting"]), "installed" + (" (stale)" if d["stale"] else "") if d["installed"] else "not installed"))
            for i in d["running"]:
                print("  running: " + describe_ticket(i, True))
            for i in d["waiting"]:
                print("  waiting: " + describe_ticket(i, False))
        return 0
    if cmd == "apply":
        ok = apply_mode(effective_mode())
        print("applied %s%s" % (effective_mode(), "" if ok else " (systemctl failed)"))
        return 0 if ok else 1
    print("unknown command %r" % cmd, file=sys.stderr)
    return 2


if __name__ == "__main__":
    if os.path.basename(sys.argv[0]) == "cargo":
        sys.exit(shim(sys.argv))
    sys.exit(main(sys.argv))

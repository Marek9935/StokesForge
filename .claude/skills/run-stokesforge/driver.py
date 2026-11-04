#!/usr/bin/env python3
"""Driver for stokesforge-app: headless mutter + rootful Xwayland + XTEST injection + capture.

Subcommands (each call is independent; state lives in $SF_RUN_DIR):
  start [file.stf]      reserves a free display (:99, :100...), starts the compositor and
                        Xwayland, then the app (X11), and waits for the first frame.
                        Invisible mode (default when `mutter` and `dbus-run-session` exist and
                        SF_VISIBLE is unset): a headless mutter on its own Wayland socket
                        `sf-<n>`, nothing appears on the desktop. Visible mode otherwise:
                        Xwayland opens a window in the current Wayland session.
  where                 prints the display, the run folder and the mode; reserves nothing
  ss FILE.png           captures the X screen (the whole root window)
  click X Y [MODS+]left|right|double   moves the pointer and clicks (MODS: ctrl, shift, alt)
  drag X1 Y1 X2 Y2 [MODS]   hover, press, move in steps, release
  move X Y              moves the pointer
  key NAME [NAME...]    keys (e.g. ctrl+n, F2, Return, Escape)
  type TEXT             types text
  stop [app]            kills the app, Xwayland and the compositor (process groups);
                        `stop app` kills only the app
"""
import os, subprocess, sys, time, signal, pathlib, shutil, fcntl

ROOT = pathlib.Path(__file__).resolve().parents[3]
# One run folder per checkout (worktree): two sessions never share a display.
RUN = pathlib.Path(os.environ.get("SF_RUN_DIR", f"/tmp/stokesforge-run/{ROOT.name}"))


# --- Display reservation -------------------------------------------------
# The reservation folder does not depend on SF_RUN_DIR: all checkouts share it.
# A display number is held by a non-blocking flock on `<n>.lock`; the fd is passed
# to the compositor and to Xwayland, so the kernel frees the number when they die.

def x_socket_dir():
    return pathlib.Path(os.environ.get("SF_X_SOCKET_DIR", "/tmp/.X11-unix"))


def x_lock_dir():
    return pathlib.Path(os.environ.get("SF_X_LOCK_DIR", "/tmp"))


def reserve_dir():
    return pathlib.Path(os.environ.get("SF_RESERVE_DIR", "/tmp/stokesforge-run/.displays"))


def reserve_display(first=99):
    """Returns (n, fd): a free display number and the inheritable fd holding its lock.
    Never creates /tmp/.X<n>-lock: Xwayland owns it."""
    rd = reserve_dir()
    rd.mkdir(parents=True, exist_ok=True)
    n = first
    while True:
        if (x_socket_dir() / f"X{n}").exists() or (x_lock_dir() / f".X{n}-lock").exists():
            n += 1
            continue
        fd = os.open(rd / f"{n}.lock", os.O_RDWR | os.O_CREAT, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            os.close(fd)
            n += 1
            continue
        os.set_inheritable(fd, True)
        return n, fd


def current_display():
    """SF_DISPLAY wins, then the display `start` wrote, else :99."""
    if os.environ.get("SF_DISPLAY"):
        return os.environ["SF_DISPLAY"]
    try:
        return (RUN / "display").read_text().strip()
    except Exception:
        return ":99"


def choose_mode(env=None, which=shutil.which):
    """'invisible' (headless mutter) or 'visible'; a pure function of env and tools."""
    env = os.environ if env is None else env
    if env.get("SF_VISIBLE"):
        return "visible"
    if which("mutter") and which("dbus-run-session"):
        return "invisible"
    return "visible"


DISPLAY = current_display()
APP = pathlib.Path(os.environ.get("SF_APP", ROOT / "target/debug/stokesforge-app"))
GEOM = os.environ.get("SF_GEOMETRY", "1400x900")


def wayland_name(display):
    return "sf-" + display.lstrip(":")


def compositor_argv(wl, geom=GEOM):
    # dbus-run-session keeps mutter off the user's own bus.
    return ["dbus-run-session", "--", "mutter", "--headless", "--no-x11",
            "--wayland-display", wl, "--virtual-monitor", geom]


def xwayland_env(base, wl=None):
    """Environment of Xwayland: pointed at the headless compositor when there is one."""
    env = dict(base)
    if wl:
        env["WAYLAND_DISPLAY"] = wl
    return env


def app_env(base, display, config_dir):
    """The app is an X11 client: it never sees the Wayland socket."""
    env = dict(base, DISPLAY=display, STOKESFORGE_X11="1", STOKESFORGE_DEMO="1",
               STOKESFORGE_CONFIG_DIR=str(config_dir))
    env.pop("WAYLAND_DISPLAY", None)
    return env


def pidfile(name):
    return RUN / f"{name}.pid"


def read_pid(name):
    try:
        return int(pidfile(name).read_text())
    except Exception:
        return None


def alive(name):
    pid = read_pid(name)
    if pid is None:
        return False
    try:
        os.kill(pid, 0)
        return True
    except Exception:
        return False


def spawn(name, argv, env=None, fds=()):
    log = open(RUN / f"{name}.log", "w")
    p = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT, env=env,
                         start_new_session=True, pass_fds=tuple(fds))
    pidfile(name).write_text(str(p.pid))
    return p


def where(args=()):
    print(f"display={DISPLAY} run={RUN} mode={choose_mode()}")


def wait_for(path, timeout):
    end = time.time() + timeout
    while time.time() < end:
        if pathlib.Path(path).exists():
            return True
        time.sleep(0.2)
    return pathlib.Path(path).exists()


def stack_usable():
    if not alive("xwayland"):
        return False
    # A recorded compositor that died takes the whole stack with it.
    return not (pidfile("compositor").exists() and not alive("compositor"))


def start(args):
    global DISPLAY
    RUN.mkdir(parents=True, exist_ok=True)
    if not APP.exists():
        sys.exit(f"{APP} missing: cargo build -p stokesforge-app")
    if not stack_usable():
        stop([])
        fd = None
        if os.environ.get("SF_DISPLAY"):
            DISPLAY = os.environ["SF_DISPLAY"]
        else:
            n, fd = reserve_display()
            DISPLAY = f":{n}"
        (RUN / "display").write_text(DISPLAY)
        fds = [fd] if fd is not None else []
        wl = None
        if choose_mode() == "invisible":
            wl = wayland_name(DISPLAY)
            sock = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/%d" % os.getuid())) / wl
            spawn("compositor", compositor_argv(wl), None, fds)
            timeout = float(os.environ.get("SF_COMPOSITOR_TIMEOUT", "10"))
            if not wait_for(sock, timeout):
                print(f"headless mutter gave no socket {sock} after {timeout:g} s "
                      f"(see {RUN}/compositor.log): falling back to visible mode")
                kill_group("compositor")
                pidfile("compositor").unlink(missing_ok=True)
                wl = None
        xenv = xwayland_env(os.environ, wl)
        spawn("xwayland", ["Xwayland", DISPLAY, "-geometry", GEOM, "-noreset"], xenv, fds)
        if fd is not None:
            os.close(fd)
        wait_for(x_socket_dir() / f"X{DISPLAY.lstrip(':')}", 10)
    if alive("app"):
        stop(["app"])
    # The app writes its preferences under $RUN/config, never the user's own (#332).
    (RUN / "config").mkdir(exist_ok=True)
    spawn("app", [str(APP), *args], app_env(os.environ, DISPLAY, RUN / "config"))
    time.sleep(10)  # the first wgpu/llvmpipe render is slow
    print("app:", "alive" if alive("app") else "DEAD (see app.log)")
    focus()


def kill_group(name, grace=None):
    """SIGTERM the process group started under `name`, SIGKILL it after the grace."""
    grace = float(os.environ.get("SF_STOP_GRACE", "3")) if grace is None else grace
    pid = read_pid(name)
    if pid is None:
        return
    try:
        # start_new_session made the pid its own group leader; a stale pid names no group.
        os.killpg(pid, signal.SIGTERM)
    except (ProcessLookupError, PermissionError):
        return
    end = time.time() + grace
    while time.time() < end:
        try:
            os.killpg(pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.1)
    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def stop(args=()):
    names = ["app"] if args else ["app", "xwayland", "compositor"]
    for name in names:
        kill_group(name)
        pidfile(name).unlink(missing_ok=True)


def xconn():
    from Xlib import display
    return display.Display(DISPLAY)


# --- Gestures: a pure planner of events, a thin executor -------------------

MODS = {"ctrl": "Control_L", "shift": "Shift_L", "alt": "Alt_L"}


def parse_mods(names):
    out = []
    for m in names:
        if m.lower() not in MODS:
            sys.exit(f"unknown modifier: {m} (ctrl, shift, alt)")
        out.append(MODS[m.lower()])
    return out


def hover_events(x, y):
    # egui must see the hover one frame BEFORE the click. A 1st move to a distant target
    # is not taken for a usable hover (click lost); a 2nd micro-move makes it reliable.
    return [("move", x, y), ("sleep", 0.5), ("move", x + 1, y), ("sleep", 0.4),
            ("move", x, y), ("sleep", 0.4)]


def plan_click(x, y, how="left"):
    x, y = int(x), int(y)
    *mods, kind = how.split("+")
    if kind not in ("left", "right", "double"):
        sys.exit(f"unknown click: {how} ([mods+]left|right|double)")
    mods = parse_mods(mods)
    btn = 3 if kind == "right" else 1
    ev = hover_events(x, y)
    ev += [("keydown", m) for m in mods]
    for _ in range(2 if kind == "double" else 1):
        ev += [("btndown", btn), ("sleep", 0.05), ("btnup", btn), ("sleep", 0.08)]
    ev += [("keyup", m) for m in reversed(mods)]
    ev.append(("sleep", 0.8))  # egui only repaints on events: let the frame come out
    return ev


def plan_drag(x1, y1, x2, y2, mods=(), steps=30):
    """Model: scripts/perf/drag.py (hover, press, 0.3 s, steps every 20 ms, wait, release)."""
    x1, y1, x2, y2 = int(x1), int(y1), int(x2), int(y2)
    mods = parse_mods(mods)
    ev = hover_events(x1, y1)
    ev += [("keydown", m) for m in mods]
    ev += [("btndown", 1), ("sleep", 0.3)]
    for i in range(1, steps + 1):
        ev += [("move", x1 + (x2 - x1) * i // steps, y1 + (y2 - y1) * i // steps),
               ("sleep", 0.02)]
    ev += [("sleep", 0.3), ("btnup", 1)]
    ev += [("keyup", m) for m in reversed(mods)]
    ev.append(("sleep", 0.5))
    return ev


def execute(events):
    from Xlib import X, XK
    from Xlib.ext import xtest
    d = xconn()
    code = lambda m: d.keysym_to_keycode(XK.string_to_keysym(m))
    for e in events:
        if e[0] == "sleep":
            time.sleep(e[1]); continue
        if e[0] == "move":
            xtest.fake_input(d, X.MotionNotify, x=e[1], y=e[2])
        elif e[0] == "btndown":
            xtest.fake_input(d, X.ButtonPress, e[1])
        elif e[0] == "btnup":
            xtest.fake_input(d, X.ButtonRelease, e[1])
        elif e[0] == "keydown":
            xtest.fake_input(d, X.KeyPress, code(e[1]))
        elif e[0] == "keyup":
            xtest.fake_input(d, X.KeyRelease, code(e[1]))
        d.sync()


def click(x, y, how="left"):
    execute(plan_click(x, y, how))


def drag(x1, y1, x2, y2, mods=()):
    execute(plan_drag(x1, y1, x2, y2, mods))


def move(x, y):
    execute([("move", int(x), int(y)), ("sleep", 0.5)])


def press(d, keysym_name, mods=()):
    from Xlib import X, XK
    from Xlib.ext import xtest
    ks = XK.string_to_keysym(keysym_name)
    if ks == 0:
        sys.exit(f"unknown key: {keysym_name}")
    code = d.keysym_to_keycode(ks)
    # Shift needed if the keysym is in the 2nd column of the keyboard table
    # (capitals, '_', '#', '(' ...).
    shift = d.keycode_to_keysym(code, 0) != ks and d.keycode_to_keysym(code, 1) == ks
    modcodes = [d.keysym_to_keycode(XK.string_to_keysym(m)) for m in mods]
    if shift:
        modcodes.append(d.keysym_to_keycode(XK.string_to_keysym("Shift_L")))
    for m in modcodes:
        xtest.fake_input(d, X.KeyPress, m)
    xtest.fake_input(d, X.KeyPress, code); d.sync(); time.sleep(0.04)
    xtest.fake_input(d, X.KeyRelease, code)
    for m in reversed(modcodes):
        xtest.fake_input(d, X.KeyRelease, m)
    d.sync(); time.sleep(0.12)


def focus():
    """Without a window manager, nobody gives keyboard focus."""
    from Xlib import X
    d = xconn()
    def walk(w):
        for c in w.query_tree().children:
            yield c
            yield from walk(c)
    for w in walk(d.screen().root):
        try:
            if w.get_wm_name() or w.get_wm_class():
                w.map(); w.set_input_focus(X.RevertToParent, X.CurrentTime)
                d.sync(); return
        except Exception:
            pass
    sys.exit("app window not found")


def key(names):
    d = xconn()
    for n in names:
        parts = n.split("+")
        press(d, parts[-1], [MODS[m.lower()] for m in parts[:-1]])
    time.sleep(0.8)


def type_text(text):
    d = xconn()
    names = {" ": "space", ".": "period", ",": "comma", "-": "minus", "_": "underscore",
             "=": "equal", "(": "parenleft", ")": "parenright", "\n": "Return", "/": "slash",
             "#": "numbersign", '"': "quotedbl", ":": "colon", "[": "bracketleft",
             "]": "bracketright", "!": "exclam"}
    for c in text:
        press(d, names.get(c, c))
    time.sleep(0.8)


def flush_frames():
    """egui only repaints on events and the X capture sees the PREVIOUS
    image (2-3 frames behind under Xwayland): four pointer micro-moves force
    the frames that bring out the current state."""
    from Xlib import X
    from Xlib.ext import xtest
    d = xconn()
    p = d.screen().root.query_pointer()
    for dx in (1, 0, 1, 0):
        xtest.fake_input(d, X.MotionNotify, x=p.root_x + dx, y=p.root_y)
        d.sync(); time.sleep(0.6)


def ss(path):
    # Empirical: the 1st capture after an action still shows the previous state;
    # the 2nd (after a new flush) shows the current state.
    flush_frames()
    subprocess.run(["import", "-display", DISPLAY, "-window", "root", path], check=True)
    flush_frames()
    subprocess.run(["import", "-display", DISPLAY, "-window", "root", path], check=True)
    print(path)


if __name__ == "__main__":
    cmd, *a = sys.argv[1:] or ["-h"]
    if cmd == "start": start(a)
    elif cmd == "stop": stop(a)
    elif cmd == "where": where()
    elif cmd == "ss": ss(a[0])
    elif cmd == "click": click(a[0], a[1], a[2] if len(a) > 2 else "left")
    elif cmd == "drag": drag(*a[:4], a[4].split("+") if len(a) > 4 else ())
    elif cmd == "move": move(a[0], a[1])
    elif cmd == "key": key(a)
    elif cmd == "focus": focus()
    elif cmd == "type": type_text(" ".join(a))
    else: print(__doc__)

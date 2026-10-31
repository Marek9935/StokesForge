#!/usr/bin/env python3
"""Driver for stokesforge-app: rootful Xwayland + XTEST injection + capture.

Subcommands (each call is independent; state lives in $SF_RUN_DIR):
  start [fichier.stf]   starts Xwayland on the first free display (:99, :100...) then the app (X11), waits for the first frame
  where                 prints the display and the run folder of this checkout
  ss FICHIER.png        captures the X screen (the whole root window)
  click X Y [left|right|double]   moves the pointer and clicks
  move X Y              moves the pointer
  key NOM [NOM...]      keys (e.g. ctrl+n, F2, Return, Escape)
  type TEXTE            types text
  stop                  kills the app and Xwayland
"""
import os, subprocess, sys, time, signal, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[3]
# One run folder per checkout (worktree): two sessions never share a display.
RUN = pathlib.Path(os.environ.get("SF_RUN_DIR", f"/tmp/stokesforge-run/{ROOT.name}"))


def free_display(first=99):
    k = first
    while (pathlib.Path(f"/tmp/.X11-unix/X{k}").exists()
           or pathlib.Path(f"/tmp/.X{k}-lock").exists()):
        k += 1
    return f":{k}"


def current_display():
    """SF_DISPLAY wins, then the display `start` wrote, else :99."""
    if os.environ.get("SF_DISPLAY"):
        return os.environ["SF_DISPLAY"]
    try:
        return (RUN / "display").read_text().strip()
    except Exception:
        return ":99"


DISPLAY = current_display()
APP = pathlib.Path(os.environ.get("SF_APP", ROOT / "target/debug/stokesforge-app"))
GEOM = os.environ.get("SF_GEOMETRY", "1400x900")


def pidfile(name):
    return RUN / f"{name}.pid"


def alive(name):
    try:
        os.kill(int(pidfile(name).read_text()), 0)
        return True
    except Exception:
        return False


def spawn(name, argv, env=None):
    log = open(RUN / f"{name}.log", "w")
    p = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT,
                         env=env, start_new_session=True)
    pidfile(name).write_text(str(p.pid))
    return p


def where(args=()):
    print(f"display={DISPLAY} run={RUN}")


def start(args):
    global DISPLAY
    RUN.mkdir(parents=True, exist_ok=True)
    if not alive("xwayland"):
        DISPLAY = os.environ.get("SF_DISPLAY") or free_display()
        (RUN / "display").write_text(DISPLAY)
    if not APP.exists():
        sys.exit(f"{APP} missing: cargo build -p stokesforge-app")
    if not alive("xwayland"):
        # Rootful: opens as a window in the current Wayland session
        # (no Xvfb here, no sudo to install it).
        spawn("xwayland", ["Xwayland", DISPLAY, "-geometry", GEOM, "-noreset"])
        time.sleep(3)
    if alive("app"):
        stop(["app"])
    # The app writes its preferences under $RUN/config, never the user's own (#332).
    (RUN / "config").mkdir(exist_ok=True)
    env = dict(os.environ, DISPLAY=DISPLAY, STOKESFORGE_X11="1", STOKESFORGE_DEMO="1",
               STOKESFORGE_CONFIG_DIR=str(RUN / "config"))
    env.pop("WAYLAND_DISPLAY", None)
    spawn("app", [str(APP), *args], env)
    time.sleep(10)  # the first wgpu/llvmpipe render is slow
    print("app:", "alive" if alive("app") else "DEAD (see app.log)")
    focus()


def stop(args=()):
    for name in (["app"] if args else ["app", "xwayland"]):
        try:
            os.killpg(int(pidfile(name).read_text()), signal.SIGTERM)
        except Exception:
            pass


def xconn():
    from Xlib import display
    return display.Display(DISPLAY)


def click(x, y, how="left"):
    from Xlib import X
    from Xlib.ext import xtest
    d = xconn()
    xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y))
    d.sync(); time.sleep(0.5)  # egui must see the hover one frame BEFORE the click
    # 1st move to a distant target: egui does not take it for a usable
    # hover (click lost); a 2nd micro-move makes it reliable.
    xtest.fake_input(d, X.MotionNotify, x=int(x) + 1, y=int(y))
    d.sync(); time.sleep(0.4)
    xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y))
    d.sync(); time.sleep(0.4)
    btn = 3 if how == "right" else 1
    for _ in range(2 if how == "double" else 1):
        xtest.fake_input(d, X.ButtonPress, btn); d.sync(); time.sleep(0.05)
        xtest.fake_input(d, X.ButtonRelease, btn); d.sync(); time.sleep(0.08)
    time.sleep(0.8)  # egui only repaints on events: let the frame come out


def move(x, y):
    from Xlib import X
    from Xlib.ext import xtest
    d = xconn()
    xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y)); d.sync()
    time.sleep(0.5)


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


MODS = {"ctrl": "Control_L", "shift": "Shift_L", "alt": "Alt_L"}


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
    elif cmd == "stop": stop()
    elif cmd == "where": where()
    elif cmd == "ss": ss(a[0])
    elif cmd == "click": click(a[0], a[1], a[2] if len(a) > 2 else "left")
    elif cmd == "move": move(a[0], a[1])
    elif cmd == "key": key(a)
    elif cmd == "focus": focus()
    elif cmd == "type": type_text(" ".join(a))
    else: print(__doc__)

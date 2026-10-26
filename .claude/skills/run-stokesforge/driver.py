#!/usr/bin/env python3
"""Pilote de stokesforge-app : Xwayland « rootful » + injection XTEST + capture.

Sous-commandes (chaque appel est indépendant ; l'état vit dans $SF_RUN_DIR) :
  start [fichier.stf]   lance Xwayland :99 puis l'app (X11), attend la 1re image
  ss FICHIER.png        capture l'écran X (toute la fenêtre racine :99)
  click X Y [left|right|double]   déplace le pointeur et clique
  move X Y              déplace le pointeur
  key NOM [NOM...]      touches (ex. ctrl+n, F2, Return, Escape)
  type TEXTE            saisit du texte
  stop                  tue l'app et Xwayland
"""
import os, subprocess, sys, time, signal, pathlib

DISPLAY = os.environ.get("SF_DISPLAY", ":99")
RUN = pathlib.Path(os.environ.get("SF_RUN_DIR", "/tmp/stokesforge-run"))
ROOT = pathlib.Path(__file__).resolve().parents[3]
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


def start(args):
    RUN.mkdir(parents=True, exist_ok=True)
    if not APP.exists():
        sys.exit(f"{APP} absent : cargo build -p stokesforge-app")
    if not alive("xwayland"):
        # Rootful : s'ouvre comme une fenêtre dans la session Wayland courante
        # (pas de Xvfb ici, pas de sudo pour l'installer).
        spawn("xwayland", ["Xwayland", DISPLAY, "-geometry", GEOM, "-noreset"])
        time.sleep(3)
    if alive("app"):
        stop(["app"])
    env = dict(os.environ, DISPLAY=DISPLAY, STOKESFORGE_X11="1", STOKESFORGE_DEMO="1")
    env.pop("WAYLAND_DISPLAY", None)
    spawn("app", [str(APP), *args], env)
    time.sleep(10)  # le premier rendu wgpu/llvmpipe est lent
    print("app:", "vivante" if alive("app") else "MORTE (voir app.log)")
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
    d.sync(); time.sleep(0.5)  # egui doit voir le survol à une image AVANT le clic
    # 1er mouvement vers une cible lointaine : egui ne le prend pas pour un
    # survol exploitable (clic perdu) ; un 2e micro-mouvement le fiabilise.
    xtest.fake_input(d, X.MotionNotify, x=int(x) + 1, y=int(y))
    d.sync(); time.sleep(0.4)
    xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y))
    d.sync(); time.sleep(0.4)
    btn = 3 if how == "right" else 1
    for _ in range(2 if how == "double" else 1):
        xtest.fake_input(d, X.ButtonPress, btn); d.sync(); time.sleep(0.05)
        xtest.fake_input(d, X.ButtonRelease, btn); d.sync(); time.sleep(0.08)
    time.sleep(0.8)  # egui ne repeint qu'à l'événement : laisser l'image sortir


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
        sys.exit(f"touche inconnue : {keysym_name}")
    code = d.keysym_to_keycode(ks)
    # Shift nécessaire si le keysym est en 2e colonne de la table du clavier
    # (majuscules, '_', '#', '(' ...).
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
    """Sans gestionnaire de fenêtres, personne ne donne le focus clavier."""
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
    sys.exit("fenêtre de l'app introuvable")


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
    """egui ne repeint que sur événement et la capture X voit l'image
    PRÉCÉDENTE (retard de 2-3 images sous Xwayland) : quatre micro-mouvements du pointeur forcent les images
    qui font apparaître l'état courant."""
    from Xlib import X
    from Xlib.ext import xtest
    d = xconn()
    p = d.screen().root.query_pointer()
    for dx in (1, 0, 1, 0):
        xtest.fake_input(d, X.MotionNotify, x=p.root_x + dx, y=p.root_y)
        d.sync(); time.sleep(0.6)


def ss(path):
    # Empirique : la 1re capture après une action montre encore l'état d'avant ;
    # la 2e (après un nouveau flush) montre l'état courant.
    flush_frames()
    subprocess.run(["import", "-display", DISPLAY, "-window", "root", path], check=True)
    flush_frames()
    subprocess.run(["import", "-display", DISPLAY, "-window", "root", path], check=True)
    print(path)


if __name__ == "__main__":
    cmd, *a = sys.argv[1:] or ["-h"]
    if cmd == "start": start(a)
    elif cmd == "stop": stop()
    elif cmd == "ss": ss(a[0])
    elif cmd == "click": click(a[0], a[1], a[2] if len(a) > 2 else "left")
    elif cmd == "move": move(a[0], a[1])
    elif cmd == "key": key(a)
    elif cmd == "focus": focus()
    elif cmd == "type": type_text(" ".join(a))
    else: print(__doc__)

#!/usr/bin/env bash
# Assembles and VALIDATES the standalone distribution of the `stokesforge-app`
# window, Python bridge included, **and** the `stokesforge-selftest`
# self-check binary.
#
# This is the lock of task 5.6, on the packaging side: if this script exits 0,
# THREE things are true from a packaged binary, not from `cargo run`, and not
# against the repository's vendored interpreter:
#   1. the `stokesforge-app` window opens and closes cleanly;
#   2. `stokesforge-selftest`, built and linked exactly like `stokesforge-app`
#      (same relative rpath, same `lib/python` layout), REPLAYS the end
#      criterion of the whole foundation: edit/undo/redo, a disk round trip,
#      and a Python script that reads a column without copying, writes another
#      one through the bus, undone from the host as well as from Python;
#   3. this self-check binary exits with code 0.
#
# Both binaries are built on the same model as `scripts/pyhello-dist.sh` (same
# pitfalls no. 1/no. 2, same `lib/python` layout) and copied SIDE BY SIDE into
# `dist/stokesforge-app/`, so that both find `lib/python` by the same relative
# path (`<executable directory>/lib/python`, see `EmbeddedPython::locate`).
#
# # What this script does NOT prove
#
# `stokesforge-app` starts the interpreter **lazily**, at the first script
# (see `stokesforge_app_state::state`): opening and closing it by auto-quit
# therefore runs no script and never touches Python. This is exactly the gap
# that `stokesforge-selftest` fills: it alone, in this script, really runs a
# Python script from the package.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"

PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/stokesforge-app"
SOFILE="libpython${PYTHON_XY}.so.1.0"
AUTOQUIT_MS="${STOKESFORGE_APP_DIST_AUTOQUIT_MS:-1500}"

# --- 1. Build ---------------------------------------------------------------
# PYO3_CONFIG_FILE: find libpython at COMPILE time (pitfall no. 1).
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE

# RELATIVE rpath, as for pyhello: both binaries will find their libpython in
# the folder where they are copied (pitfall no. 2), once moved out of the
# repository; hence the need to copy them to the SAME place (step 2).
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'

echo "[dist] release build of stokesforge-app (python feature)"
cd "$REPO_ROOT"
cargo build --locked -p stokesforge-app --features python --release --bin stokesforge-app

echo "[dist] release build of stokesforge-selftest (python feature)"
cargo build --locked -p stokesforge-script --features python --release --bin stokesforge-selftest

# --- 2. Assembly ------------------------------------------------------------
echo "[dist] assembling $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/lib/python/lib"

cp "$REPO_ROOT/target/release/stokesforge-app" "$DIST/stokesforge-app"
# NEXT TO stokesforge-app, not in a subfolder: this is what makes it find
# lib/python by the same relative path that EmbeddedPython::locate() resolves
# for stokesforge-app (<executable directory>/lib/python).
cp "$REPO_ROOT/target/release/stokesforge-selftest" "$DIST/stokesforge-selftest"

# Same layout as dist/pyhello: libpython only once in lib/, where the rpath
# points; the embedded prefix under lib/python/lib, where
# EmbeddedPython::locate() expects it (<executable dir>/lib/python).
cp "$PREFIX/lib/$SOFILE" "$DIST/lib/$SOFILE"
ln -s "$SOFILE" "$DIST/lib/libpython${PYTHON_XY}.so"
cp -a "$PREFIX/lib/python${PYTHON_XY}" "$DIST/lib/python/lib/"
find "$PREFIX/lib" -maxdepth 1 -name '*.so*' ! -name 'libpython*' \
    -exec cp -a {} "$DIST/lib/python/lib/" \;
rm -rf "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip" \
       "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip-"*.dist-info
# __pycache__ kept: see the doc of pyhello-dist.sh (same reason, same gain of
# ~165 ms at the first script).

# Licenses of the code, the resources and the runtime actually delivered (#129).
"$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"

TAILLE="$(du -sm "$DIST" | cut -f1)"
echo "[dist] size of $DIST: ${TAILLE} MB"

# --- 3. Linking proof -------------------------------------------------------
echo "[dist] ldd | grep python (stokesforge-app):"
ldd "$DIST/stokesforge-app" | grep -i python
if ! ldd "$DIST/stokesforge-app" | grep -q "$DIST/lib/$SOFILE"; then
    echo "[dist] ERROR: $SOFILE is not resolved from $DIST/lib for stokesforge-app" >&2
    exit 1
fi

echo "[dist] ldd | grep python (stokesforge-selftest):"
ldd "$DIST/stokesforge-selftest" | grep -i python
if ! ldd "$DIST/stokesforge-selftest" | grep -q "$DIST/lib/$SOFILE"; then
    echo "[dist] ERROR: $SOFILE is not resolved from $DIST/lib for stokesforge-selftest" >&2
    exit 1
fi

# --- 4. Proof that the packaged window opens and closes --------------------
#
# Cleaned environment (`env -i`): minimal PATH and HOME, plus the strict
# minimum for a real window to open on THIS machine: a window needs a display
# server, which `pyhello` (purely console) did not require.
#
# `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR` are deliberately ABSENT from this
# environment: on this machine (GNOME/Wayland session), the close scheduled by
# `STOKESFORGE_APP_AUTOQUIT_MS` never wakes the `winit` event loop under its
# Wayland plugin; the process stays indefinitely in `ep_poll`, observed with a
# direct `cargo run` (`timeout 90` with no output). The same binary, under
# Xwayland (`DISPLAY` without `WAYLAND_DISPLAY`), opens and closes normally
# (code 0, a few seconds): so this is a limit of the `winit`/`eframe` Wayland
# plugin on THIS compositor, not a defect of `stokesforge-app`. Forcing
# Xwayland here is the most honest workaround until it is investigated on the
# `eframe` side; see the report of task 5.6 for the detail.
if [[ -z "${DISPLAY:-}" ]]; then
    echo "[dist] ERROR: DISPLAY is not set; an accessible X11/Xwayland server is required" >&2
    exit 1
fi

# Throwaway configuration folder (#333): with no argument, the first opening
# writes the demonstration into `demo.stf` of this folder and opens it; this
# is the production path that the packaged window tests, without touching the
# machine's real profile.
CONFIG_DIR="$(mktemp -d)"
trap 'rm -rf "$CONFIG_DIR"' EXIT

echo "[dist] running in a cleaned environment, STOKESFORGE_APP_AUTOQUIT_MS=$AUTOQUIT_MS"
set +e
env -i PATH=/usr/bin:/bin HOME=/tmp \
       DISPLAY="$DISPLAY" XAUTHORITY="${XAUTHORITY:-}" \
       STOKESFORGE_CONFIG_DIR="$CONFIG_DIR" \
       STOKESFORGE_APP_AUTOQUIT_MS="$AUTOQUIT_MS" \
       "$DIST/stokesforge-app" 2>&1 | tee "$DIST/../stokesforge-app-run.log"
CODE="${PIPESTATUS[0]}"
set -e

if [[ "$CODE" -ne 0 ]]; then
    echo "[dist] ERROR: the packaged window exited with code $CODE (expected 0)" >&2
    exit 1
fi

# The first opening offered the demonstration: the file exists and the marker
# is set (#333).
if [[ ! -s "$CONFIG_DIR/demo.stf" ]] || ! grep -q '"first_run_done": true' "$CONFIG_DIR/preferences.json" 2>/dev/null; then
    echo "[dist] ERROR: the first opening did not write demo.stf and the marker in $CONFIG_DIR" >&2
    exit 1
fi

echo "[dist] OK: packaged window opened then closed cleanly (code 0), ${TAILLE} MB"

# --- 5. Proof that the foundation's end criterion holds FROM THE PACKAGE ---
#
# This is where this script stops being a proof that a window opens and becomes
# the proof of the WHOLE end criterion. EMPTIED then TRAPPED environment, as
# for pyhello-dist.sh: if the resolution of the embedded prefix or its
# isolation leaked toward a system Python, the fake PYTHONPATH/PYTHONHOME would
# reveal it rather than let it go unnoticed. No DISPLAY needed:
# stokesforge-selftest is a console program, not a window.
echo "[dist] self-check from the package, cleaned and trapped environment"
set +e
env -i PATH=/usr/bin:/bin HOME=/tmp \
       PYTHONPATH=/tmp/piege PYTHONHOME=/tmp/piege2 \
       "$DIST/stokesforge-selftest" 2>&1 | tee "$DIST/../stokesforge-selftest-run.log"
CODE_SELFTEST="${PIPESTATUS[0]}"
set -e

if [[ "$CODE_SELFTEST" -ne 0 ]]; then
    echo "[dist] ERROR: stokesforge-selftest exited with code $CODE_SELFTEST (expected 0); the foundation's end criterion is NOT verified from the package" >&2
    exit 1
fi

echo "[dist] OK: packaged window AND foundation end criterion verified from the package (code 0), ${TAILLE} MB"

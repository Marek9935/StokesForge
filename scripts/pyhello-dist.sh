#!/usr/bin/env bash
# Assembles and VALIDATES the standalone distribution of the Python hello-world.
#
# This is the lock of phase 5: if this script exits 0, the embedded Python
# stack is packageable and isolated. It builds, assembles, then reruns the
# binary in an emptied and trapped environment.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"

PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/pyhello"
SOFILE="libpython${PYTHON_XY}.so.1.0"

# --- 1. Build ---------------------------------------------------------------
# PYO3_CONFIG_FILE: find libpython at COMPILE time (pitfall no. 1).
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE

# RELATIVE rpath: the binary will find its libpython in its own folder
# (pitfall no. 2). $ORIGIN must reach the linker literally, hence the SINGLE
# quotes. We use RUSTFLAGS rather than .cargo/config.toml:
# `[target.x86_64-unknown-linux-gnu]` there would be global to the workspace
# and would impose this rpath on all the other binaries.
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'

echo "[dist] release build of stokesforge-pyhello"
cd "$REPO_ROOT"
cargo build --locked -p stokesforge-python --features python --release --bin stokesforge-pyhello

# --- 2. Assembly ------------------------------------------------------------
echo "[dist] assembling $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/lib/python/lib"

cp "$REPO_ROOT/target/release/stokesforge-pyhello" "$DIST/pyhello"

# libpython lives ONLY ONCE, in dist/pyhello/lib, where the rpath points.
# It weighs ~209 MB (unstripped distribution): duplicating it under
# lib/python/lib would double the size for nothing. The extension modules of
# lib-dynload reference it by SONAME and find it already loaded.
cp "$PREFIX/lib/$SOFILE" "$DIST/lib/$SOFILE"
ln -s "$SOFILE" "$DIST/lib/libpython${PYTHON_XY}.so"

# The embedded prefix, as EmbeddedPython::locate() expects it:
#   <executable dir>/lib/python/lib/python3.12/{,lib-dynload,site-packages}
cp -a "$PREFIX/lib/python${PYTHON_XY}" "$DIST/lib/python/lib/"

# Auxiliary libraries of the prefix (tcl/tk…), except libpython, already placed.
find "$PREFIX/lib" -maxdepth 1 -name '*.so*' ! -name 'libpython*' \
    -exec cp -a {} "$DIST/lib/python/lib/" \;

# `bin/` is excluded: initialization through PyConfig needs only `home` and the
# module paths, never a python3 executable.
# pip is removed: architecture decision, no pip exposed to the user.
rm -rf "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip" \
       "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip-"*.dist-info
# The __pycache__ shipped by python-build-standalone are KEPT: removing them
# forces CPython to recompile the standard library at the first import
# (measured: ~250 ms of init instead of ~85 ms), and a distribution installed
# read-only could never rebuild this cache.

# Licenses of the code, the resources and the runtime actually delivered (#129).
"$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"

TAILLE="$(du -sm "$DIST" | cut -f1)"
echo "[dist] size of $DIST: ${TAILLE} MB"

# --- 3. Linking proof -------------------------------------------------------
echo "[dist] ldd | grep python:"
ldd "$DIST/pyhello" | grep -i python
if ! ldd "$DIST/pyhello" | grep -q "$DIST/lib/$SOFILE"; then
    echo "[dist] ERROR: $SOFILE is not resolved from $DIST/lib" >&2
    exit 1
fi

# --- 4. Isolation proof -----------------------------------------------------
# EMPTIED then trapped environment: if isolation leaks, sys.path will contain them.
echo "[dist] running in a cleaned and trapped environment"
env -i PATH=/usr/bin:/bin HOME=/tmp \
       PYTHONPATH=/tmp/piege PYTHONHOME=/tmp/piege2 \
       "$DIST/pyhello" | tee "$DIST/../pyhello-run.log"

SORTIE="$(env -i PATH=/usr/bin:/bin HOME=/tmp PYTHONPATH=/tmp/piege \
              PYTHONHOME=/tmp/piege2 "$DIST/pyhello")"
if grep -q "/tmp/piege" <<<"$SORTIE"; then
    echo "[dist] ERROR: the environment leaked into sys.path" >&2
    exit 1
fi
if grep -qE "^sys\.path\[[0-9]\] *: */usr/lib/python3" <<<"$SORTIE"; then
    echo "[dist] ERROR: a system Python path is in sys.path" >&2
    exit 1
fi
if ! grep -q "sys.flags.isolated    : true" <<<"$SORTIE"; then
    echo "[dist] ERROR: the interpreter is not in isolated mode" >&2
    exit 1
fi

echo "[dist] OK: standalone, isolated distribution, ${TAILLE} MB"

#!/usr/bin/env bash
# Vendors the embedded CPython interpreter and installs pinned numpy in it.
#
# Idempotent: can be rerun without side effects. Touches no system Python.
# Reads all its versions from vendor/python/lock.env (pitfall no. 6: a single lock).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$REPO_ROOT/vendor/python"
# shellcheck source=../vendor/python/lock.env
source "$VENDOR_DIR/lock.env"

# Windows (#332): run under Git Bash. Other triple, `python.exe` at the root
# of the prefix (no bin/).
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
        PBS_TRIPLE="$PBS_TRIPLE_WINDOWS"
        PY_REL="python.exe" ;;
    *) PY_REL="bin/python3" ;;
esac

ARCHIVE="$VENDOR_DIR/cpython-${PYTHON_VERSION}-${PBS_TRIPLE}-${PBS_FLAVOR}.tar.gz"
EXTRACTED="$VENDOR_DIR/extracted"
PREFIX="$EXTRACTED/python"

# --- 1. Archive -------------------------------------------------------------
# Downloaded only if missing: the archive is ~110 MB and is not versioned.
if [[ -f "$ARCHIVE" ]]; then
    echo "[vendor] archive already present: $ARCHIVE"
else
    ASSET="cpython-${PYTHON_VERSION}+${PBS_RELEASE}-${PBS_TRIPLE}-${PBS_FLAVOR}.tar.gz"
    URL="https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_RELEASE}/${ASSET}"
    echo "[vendor] downloading $URL"
    mkdir -p "$VENDOR_DIR"
    curl -fL --retry 3 -o "$ARCHIVE.part" "$URL"
    mv "$ARCHIVE.part" "$ARCHIVE"
fi

# --- 2. Extraction ----------------------------------------------------------
if [[ -x "$PREFIX/$PY_REL" ]]; then
    echo "[vendor] prefix already extracted: $PREFIX"
else
    echo "[vendor] extracting into $EXTRACTED"
    mkdir -p "$EXTRACTED"
    tar -xzf "$ARCHIVE" -C "$EXTRACTED"
fi

PY="$PREFIX/$PY_REL"
[[ -x "$PY" ]] || { echo "[vendor] ERROR: $PY not found after extraction" >&2; exit 1; }

# Guard: the extracted interpreter must be the one in the lock.
GOT_VERSION="$("$PY" -c 'import platform; print(platform.python_version())')"
if [[ "$GOT_VERSION" != "$PYTHON_VERSION" ]]; then
    echo "[vendor] ERROR: interpreter $GOT_VERSION, lock $PYTHON_VERSION" >&2
    exit 1
fi

# --- 3. pinned numpy --------------------------------------------------------
# `-I` isolates the pip call itself (no stray PYTHONPATH or user site).
# `--no-deps`: numpy has no runtime dependency, we refuse any surprise.
# `--only-binary=:all:`: never a local build, we want the official wheel.
CURRENT_NUMPY="$("$PY" -I -c 'import numpy; print(numpy.__version__)' 2>/dev/null || true)"
if [[ "$CURRENT_NUMPY" == "$NUMPY_VERSION" ]]; then
    echo "[vendor] numpy $NUMPY_VERSION already installed in the embedded prefix"
else
    if ! "$PY" -I -m pip --version >/dev/null 2>&1; then
        echo "[vendor] pip missing, bootstrapping with ensurepip"
        "$PY" -I -m ensurepip --upgrade
    fi
    echo "[vendor] installing numpy==$NUMPY_VERSION"
    "$PY" -I -m pip install --no-deps --only-binary=:all: --upgrade "numpy==$NUMPY_VERSION"
fi

# --- 4. pinned scipy (#191) --------------------------------------------------
# Same guards as numpy: `--no-deps` (numpy is already there, at the lock) and
# `--only-binary=:all:` (never a local build).
CURRENT_SCIPY="$("$PY" -I -c 'import scipy; print(scipy.__version__)' 2>/dev/null || true)"
if [[ "$CURRENT_SCIPY" == "$SCIPY_VERSION" ]]; then
    echo "[vendor] scipy $SCIPY_VERSION already installed in the embedded prefix"
else
    echo "[vendor] installing scipy==$SCIPY_VERSION"
    "$PY" -I -m pip install --no-deps --only-binary=:all: --upgrade "scipy==$SCIPY_VERSION"
fi

# --- 5. Final check ---------------------------------------------------------
# `-X utf8`: under Windows, Python would write cp1252 into the pipe (unreadable
# accents in Git Bash); `-I` would ignore PYTHONIOENCODING (#332).
"$PY" -I -X utf8 -c "
import numpy, scipy, sys
assert numpy.__version__ == '$NUMPY_VERSION', numpy.__version__
assert scipy.__version__ == '$SCIPY_VERSION', scipy.__version__
from scipy.optimize import curve_fit
print('[vendor] OK  python', sys.version.split()[0], ' numpy', numpy.__version__, ' scipy', scipy.__version__)
print('[vendor] numpy loaded from', numpy.__file__)
print('[vendor] scipy loaded from', scipy.__file__)
"

#!/usr/bin/env bash
# Generates the PyO3 configuration pointing at the embedded interpreter (pitfall no. 1).
#
# Writes target/pyo3-config.txt from the versioned template
# crates/stokesforge-python/pyo3-config.template.txt, then prints the absolute
# path of the produced file on stdout, so that a caller can do:
#     export PYO3_CONFIG_FILE="$(scripts/python-config.sh)"
# Progress messages go to stderr so they do not pollute that output.
#
# Call it in a subshell, do not source it: it ends with `exit 1` if the
# embedded prefix is missing.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"
TEMPLATE="$REPO_ROOT/crates/stokesforge-python/pyo3-config.template.txt"
PY_REL="bin/python3"
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
        # Windows (#332): python.exe and python312.lib (libs/); Windows-style
        # paths (C:/…) for pyo3-build-config.
        TEMPLATE="$REPO_ROOT/crates/stokesforge-python/pyo3-config.windows.template.txt"
        PY_REL="python.exe"
        PREFIX="$(cygpath -m "$PREFIX")" ;;
esac
OUT="$REPO_ROOT/target/pyo3-config.txt"

if [[ ! -x "$PREFIX/$PY_REL" ]]; then
    echo "[config] ERROR: embedded prefix missing ($PREFIX). Run scripts/python-vendor.sh first" >&2
    exit 1
fi

mkdir -p "$(dirname "$OUT")"
# Substitute the only machine-dependent field, then remove comment lines and
# empty lines: pyo3-build-config reads only key=value pairs. The prefix has no
# "|", which is chosen as the sed delimiter to avoid escaping the "/".
sed -e "s|@PYTHON_PREFIX@|$PREFIX|g" -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' \
    "$TEMPLATE" > "$OUT"

echo "[config] PYO3_CONFIG_FILE=$OUT (prefix=$PREFIX)" >&2
echo "$OUT"

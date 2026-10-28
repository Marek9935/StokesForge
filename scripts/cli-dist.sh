#!/usr/bin/env bash
# Headless Linux distribution #71: same Python layout as app-dist.sh.
# Checks from the package: numpy reading, writes, graph, .stf and undo.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/stokesforge-cli"
SOFILE="libpython${PYTHON_XY}.so.1.0"
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'
cd "$REPO_ROOT"
cargo build --locked -p stokesforge-cli --features python --release
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
[[ "$TARGET_DIR" = /* ]] || TARGET_DIR="$REPO_ROOT/$TARGET_DIR"

rm -rf "$DIST"
mkdir -p "$DIST/lib/python/lib"
cp "$TARGET_DIR/release/stokesforge-cli" "$DIST/stokesforge-cli"
cp "$PREFIX/lib/$SOFILE" "$DIST/lib/$SOFILE"
ln -s "$SOFILE" "$DIST/lib/libpython${PYTHON_XY}.so"
cp -a "$PREFIX/lib/python${PYTHON_XY}" "$DIST/lib/python/lib/"
find "$PREFIX/lib" -maxdepth 1 -name '*.so*' ! -name 'libpython*' \
    -exec cp -a {} "$DIST/lib/python/lib/" \;
rm -rf "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip" \
       "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip-"*.dist-info
# __pycache__ kept, libpython only once, no pip delivered: see pyhello.
# Licenses of the code, the resources and the runtime actually delivered (#129).
"$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"

LINKS="$(ldd "$DIST/stokesforge-cli")"
if ! grep -Fq "$DIST/lib/$SOFILE" <<<"$LINKS"; then
    echo '[dist] libpython is not resolved from the package' >&2
    exit 1
fi

PROBE="$(mktemp -d)"
trap 'rm -rf "$PROBE"' EXIT
cat > "$PROBE/calcul.pyst" <<'PY'
import sys
import numpy as np
assert sys.flags.isolated == 1
assert all(p.startswith(sys.prefix) for p in sys.path), sys.path
assert np.__file__.startswith(sys.prefix)
book = pyst.create_workbook('Résultats')
sheet = pyst.create_worksheet(book, 'Calcul')
x = pyst.create_column(sheet, 'X')
y = pyst.create_column(sheet, 'Y')
pyst.set_values(x, np.arange(4., dtype=np.float64))
pyst.set_values(y, pyst.column(x)**2)
pyst.plot(y, x, name='Graph1', title='$x^2$')
print('packaged calculation OK')
PY
cat > "$PROBE/lecture.pyst" <<'PY'
y = pyst.resolve('[Résultats]Calcul!Y')
assert pyst.values(y) == (0.,1.,4.,9.)
assert pyst.info(pyst.resolve('[Graph1]'))['kind'] == 'graph'
pyst.set_values(y, [42.])
assert pyst.undo()
assert pyst.values(y) == (0.,1.,4.,9.)
print('packaged reload and undo OK')
PY
# No STOKESFORGE_PYTHON_HOME: the prefix must be found next to the binary.
env -i PATH=/usr/bin:/bin HOME="$PROBE" \
    PYTHONPATH=/tmp/stokesforge-piege PYTHONHOME=/tmp/stokesforge-piege2 \
    "$DIST/stokesforge-cli" run --out "$PROBE/resultat.stf" "$PROBE/calcul.pyst"
env -i PATH=/usr/bin:/bin HOME="$PROBE" \
    PYTHONPATH=/tmp/stokesforge-piege PYTHONHOME=/tmp/stokesforge-piege2 \
    "$DIST/stokesforge-cli" run --project "$PROBE/resultat.stf" "$PROBE/lecture.pyst"
echo "[dist] OK: standalone, isolated CLI, $(du -sm "$DIST" | cut -f1) MB"

#!/usr/bin/env bash
# Distribution Linux headless #71 : même disposition Python que app-dist.sh.
# Vérifie depuis le paquet lecture numpy, écritures, graphe, .plx et annulation.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/plotyx-cli"
SOFILE="libpython${PYTHON_XY}.so.1.0"
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'
cd "$REPO_ROOT"
cargo build -p plotyx-cli --features python --release
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
[[ "$TARGET_DIR" = /* ]] || TARGET_DIR="$REPO_ROOT/$TARGET_DIR"

rm -rf "$DIST"
mkdir -p "$DIST/lib/python/lib"
cp "$TARGET_DIR/release/plotyx-cli" "$DIST/plotyx-cli"
cp "$PREFIX/lib/$SOFILE" "$DIST/lib/$SOFILE"
ln -s "$SOFILE" "$DIST/lib/libpython${PYTHON_XY}.so"
cp -a "$PREFIX/lib/python${PYTHON_XY}" "$DIST/lib/python/lib/"
find "$PREFIX/lib" -maxdepth 1 -name '*.so*' ! -name 'libpython*' \
    -exec cp -a {} "$DIST/lib/python/lib/" \;
rm -rf "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip" \
       "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip-"*.dist-info
# __pycache__ conservés, libpython une seule fois, aucun pip livré : cf. pyhello.
LINKS="$(ldd "$DIST/plotyx-cli")"
if ! grep -Fq "$DIST/lib/$SOFILE" <<<"$LINKS"; then
    echo '[dist] libpython is not resolved from the package' >&2
    exit 1
fi

PROBE="$(mktemp -d)"
trap 'rm -rf "$PROBE"' EXIT
cat > "$PROBE/calcul.py" <<'PY'
import sys
import numpy as np
assert sys.flags.isolated == 1
assert all(p.startswith(sys.prefix) for p in sys.path), sys.path
assert np.__file__.startswith(sys.prefix)
book = pyx.create_workbook('Résultats')
sheet = pyx.create_worksheet(book, 'Calcul')
x = pyx.create_column(sheet, 'X')
y = pyx.create_column(sheet, 'Y')
pyx.set_values(x, np.arange(4., dtype=np.float64))
pyx.set_values(y, pyx.column(x)**2)
pyx.plot(y, x, name='Graph1', title='$x^2$')
print('packaged calculation OK')
PY
cat > "$PROBE/lecture.py" <<'PY'
y = pyx.resolve('[Résultats]Calcul!Y')
assert pyx.values(y) == (0.,1.,4.,9.)
assert pyx.info(pyx.resolve('[Graph1]'))['kind'] == 'graph'
pyx.set_values(y, [42.])
assert pyx.undo()
assert pyx.values(y) == (0.,1.,4.,9.)
print('packaged reload and undo OK')
PY
# Aucun PLOTYX_PYTHON_HOME : le préfixe doit être trouvé à côté du binaire.
env -i PATH=/usr/bin:/bin HOME="$PROBE" \
    PYTHONPATH=/tmp/plotyx-piege PYTHONHOME=/tmp/plotyx-piege2 \
    "$DIST/plotyx-cli" run --out "$PROBE/resultat.plx" "$PROBE/calcul.py"
env -i PATH=/usr/bin:/bin HOME="$PROBE" \
    PYTHONPATH=/tmp/plotyx-piege PYTHONHOME=/tmp/plotyx-piege2 \
    "$DIST/plotyx-cli" run --project "$PROBE/resultat.plx" "$PROBE/lecture.py"
echo "[dist] OK — CLI autonome, isolé, $(du -sm "$DIST" | cut -f1) Mo"

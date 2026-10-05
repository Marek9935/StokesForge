#!/usr/bin/env bash
# Assemble et VALIDE la distribution autonome du hello-world Python.
#
# C'est le verrou de la phase 5 : si ce script sort 0, la pile Python embarquée
# est empaquetable et isolée. Il construit, assemble, puis relance le binaire
# dans un environnement vidé et piégé.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"

PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/pyhello"
SOFILE="libpython${PYTHON_XY}.so.1.0"

# --- 1. Construction --------------------------------------------------------
# PYO3_CONFIG_FILE : trouver libpython à la COMPILATION (piège n°1).
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE

# rpath RELATIF : le binaire trouvera sa libpython dans son propre dossier
# (piège n°2). $ORIGIN doit arriver littéralement jusqu'à l'éditeur de liens,
# d'où les guillemets SIMPLES. On passe par RUSTFLAGS plutôt que par
# .cargo/config.toml : `[target.x86_64-unknown-linux-gnu]` y serait global au
# workspace et imposerait ce rpath à tous les autres binaires.
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'

echo "[dist] construction release de stokesforge-pyhello"
cd "$REPO_ROOT"
cargo build -p stokesforge-python --features python --release --bin stokesforge-pyhello

# --- 2. Assemblage ----------------------------------------------------------
echo "[dist] assemblage de $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/lib/python/lib"

cp "$REPO_ROOT/target/release/stokesforge-pyhello" "$DIST/pyhello"

# libpython vit UNE SEULE FOIS, dans dist/pyhello/lib, là où pointe le rpath.
# Elle pèse ~209 Mo (distribution non strippée) : la dupliquer sous
# lib/python/lib doublerait la taille pour rien. Les modules d'extension de
# lib-dynload la référencent par SONAME et la trouvent déjà chargée.
cp "$PREFIX/lib/$SOFILE" "$DIST/lib/$SOFILE"
ln -s "$SOFILE" "$DIST/lib/libpython${PYTHON_XY}.so"

# Le préfixe embarqué, tel que EmbeddedPython::locate() l'attend :
#   <dir de l'exécutable>/lib/python/lib/python3.12/{,lib-dynload,site-packages}
cp -a "$PREFIX/lib/python${PYTHON_XY}" "$DIST/lib/python/lib/"

# Bibliothèques annexes du préfixe (tcl/tk…), sauf libpython déjà placée.
find "$PREFIX/lib" -maxdepth 1 -name '*.so*' ! -name 'libpython*' \
    -exec cp -a {} "$DIST/lib/python/lib/" \;

# `bin/` est exclu : l'initialisation par PyConfig n'a besoin que de `home` et
# des chemins de modules, jamais d'un exécutable python3.
# pip est retiré : décision d'architecture — pas de pip exposé à l'utilisateur.
rm -rf "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip" \
       "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip-"*.dist-info
# Les __pycache__ livrés par python-build-standalone sont CONSERVÉS : les
# supprimer force CPython à recompiler la bibliothèque standard au premier
# import (mesuré : ~250 ms d'init au lieu de ~85 ms), et une distribution
# installée en lecture seule ne pourrait jamais reconstituer ce cache.

# Licences du code, des ressources et du runtime effectivement livré (#129).
"$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"

TAILLE="$(du -sm "$DIST" | cut -f1)"
echo "[dist] taille de $DIST : ${TAILLE} Mo"

# --- 3. Preuve de liaison ---------------------------------------------------
echo "[dist] ldd | grep python :"
ldd "$DIST/pyhello" | grep -i python
if ! ldd "$DIST/pyhello" | grep -q "$DIST/lib/$SOFILE"; then
    echo "[dist] ERREUR : $SOFILE n'est pas résolue depuis $DIST/lib" >&2
    exit 1
fi

# --- 4. Preuve d'isolation --------------------------------------------------
# Environnement VIDÉ puis piégé : si l'isolation fuit, sys.path les contiendra.
echo "[dist] exécution en environnement nettoyé et piégé"
env -i PATH=/usr/bin:/bin HOME=/tmp \
       PYTHONPATH=/tmp/piege PYTHONHOME=/tmp/piege2 \
       "$DIST/pyhello" | tee "$DIST/../pyhello-run.log"

SORTIE="$(env -i PATH=/usr/bin:/bin HOME=/tmp PYTHONPATH=/tmp/piege \
              PYTHONHOME=/tmp/piege2 "$DIST/pyhello")"
if grep -q "/tmp/piege" <<<"$SORTIE"; then
    echo "[dist] ERREUR : l'environnement a fui dans sys.path" >&2
    exit 1
fi
if grep -qE "^sys\.path\[[0-9]\] *: */usr/lib/python3" <<<"$SORTIE"; then
    echo "[dist] ERREUR : un chemin Python système est dans sys.path" >&2
    exit 1
fi
if ! grep -q "sys.flags.isolated    : true" <<<"$SORTIE"; then
    echo "[dist] ERREUR : l'interpréteur n'est pas en mode isolé" >&2
    exit 1
fi

echo "[dist] OK — distribution autonome, isolée, ${TAILLE} Mo"

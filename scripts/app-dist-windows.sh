#!/usr/bin/env bash
# Assemble et VALIDE la distribution autonome Windows de `stokesforge-app`
# (pont Python compris) et de `stokesforge-selftest` (#332). Pendant Windows de
# scripts/app-dist.sh. À lancer sous Git Bash, après scripts/python-vendor.sh.
#
# PREMIÈRE PASSE, NON VÉRIFIÉE SOUS WINDOWS : écrite depuis Linux. Points à
# confirmer sur un poste Windows : liaison de python312.dll, DLL du runtime
# Visual C++, chargement du pont depuis le paquet, ouverture de la fenêtre,
# presse-papiers, polices, chemins avec espaces ou accents.
#
# Disposition produite (dist/stokesforge-app-windows/) :
#   stokesforge-app.exe, stokesforge-selftest.exe, python312.dll   (côte à côte)
#   lib/python/{Lib,DLLs}   le préfixe embarqué, là où
#                           EmbeddedPython::locate() le cherche
# python312.dll est résolue depuis le dossier de l'exécutable (recherche DLL
# par défaut de Windows) : pas d'équivalent du rpath nécessaire.
set -euo pipefail

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "[dist] ERREUR : script Windows, à lancer sous Git Bash" >&2; exit 1 ;;
esac

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"

PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/stokesforge-app-windows"
DLL="python${PYTHON_XY/./}.dll"
AUTOQUIT_MS="${STOKESFORGE_APP_DIST_AUTOQUIT_MS:-1500}"

PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE

cd "$REPO_ROOT"
echo "[dist] construction release de stokesforge-app (feature python)"
cargo build -p stokesforge-app --features python --release --bin stokesforge-app
echo "[dist] construction release de stokesforge-selftest (feature python)"
cargo build -p stokesforge-script --features python --release --bin stokesforge-selftest

echo "[dist] assemblage de $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/lib/python"
cp target/release/stokesforge-app.exe target/release/stokesforge-selftest.exe "$DIST/"
cp "$PREFIX/$DLL" "$DIST/$DLL"
# Runtime Visual C++ livré par python-build-standalone à côté de python.exe.
for dll in "$PREFIX"/vcruntime*.dll "$PREFIX"/msvcp*.dll; do
    [[ -f "$dll" ]] && cp "$dll" "$DIST/"
done
cp -a "$PREFIX/Lib" "$DIST/lib/python/Lib"
cp -a "$PREFIX/DLLs" "$DIST/lib/python/DLLs"
rm -rf "$DIST/lib/python/Lib/site-packages/pip" \
       "$DIST/lib/python/Lib/site-packages/pip-"*.dist-info \
       "$DIST/lib/python/Lib/test" "$DIST/lib/python/Lib/idlelib" \
       "$DIST/lib/python/Lib/turtledemo"

# Les notices Python/numpy/scipy sont lues dans lib/python/lib/python3.12 par
# licenses-dist.sh (disposition Linux) : à adapter et à compléter avec les
# notices du runtime Windows (vendor/python/licenses) sur un poste Windows.
# Tant que ce n'est pas fait, aucune distribution Windows n'est publiable.
if [[ "${STOKESFORGE_DIST_SKIP_LICENSES:-0}" != "1" ]]; then
    "$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"
else
    echo "[dist] ATTENTION : notices de licences NON livrées (STOKESFORGE_DIST_SKIP_LICENSES=1)" >&2
fi

echo "[dist] taille de $DIST : $(du -sm "$DIST" | cut -f1) Mo"

echo "[dist] fenêtre empaquetée, fermeture auto après ${AUTOQUIT_MS} ms"
STOKESFORGE_APP_AUTOQUIT_MS="$AUTOQUIT_MS" "$DIST/stokesforge-app.exe"

echo "[dist] auto-vérification depuis le paquet, environnement piégé"
PYTHONPATH=C:/piege PYTHONHOME=C:/piege2 "$DIST/stokesforge-selftest.exe"

echo "[dist] OK — distribution Windows ouverte puis auto-vérifiée (code 0)"

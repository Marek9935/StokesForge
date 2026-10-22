#!/usr/bin/env bash
# Livre les textes des licences dans un paquet déjà assemblé (#129).
# La résolution Cargo inclut python, les dépendances transitives et toutes
# les plateformes ; elle ne compile ni le workspace ni l'interpréteur.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ $# -eq 1 && -d "$1" ]] || {
    echo "Usage : scripts/licenses-dist.sh <répertoire du paquet assemblé>" >&2
    exit 1
}
PACKAGE_DIR="$(cd "$1" && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"
# Plateforme du paquet : celle de la machine, sauf STOKESFORGE_LICENSES_PLATFORM
# (linux|windows), pour que licenses-test.sh éprouve aussi les refus de l'autre.
case "${STOKESFORGE_LICENSES_PLATFORM:-$(uname -s)}" in
    MINGW*|MSYS*|CYGWIN*|windows) PLATFORM=windows ;;
    *) PLATFORM=linux ;;
esac
NOTICES_DIR="$REPO_ROOT/vendor/python/licenses-$PLATFORM"
# shellcheck source=../vendor/python/licenses-linux/versions.env
source "$NOTICES_DIR/versions.env"
# shellcheck source=licenses-tools.env
source "$REPO_ROOT/scripts/licenses-tools.env"

if [[ "$PLATFORM" == windows ]]; then
    EXPECTED_TRIPLE="$PBS_TRIPLE_WINDOWS"
else
    EXPECTED_TRIPLE="$PBS_TRIPLE"
fi
if [[ "$PYTHON_VERSION" != "$LICENSE_PYTHON_VERSION" ||
      "$PBS_RELEASE" != "$LICENSE_PBS_RELEASE" ||
      "$EXPECTED_TRIPLE" != "$LICENSE_PBS_TRIPLE" ]]; then
    echo "[licences] ERREUR : actualiser vendor/python/licenses-$PLATFORM pour le verrou Python" >&2
    exit 1
fi

if [[ "$PLATFORM" == windows ]]; then
    # Disposition lib/python/{Lib,DLLs} (scripts/app-dist-windows.sh) ; le
    # LICENSE.txt de CPython est à la racine du préfixe, copié dans lib/python.
    PYTHON_LIB="$PACKAGE_DIR/lib/python/Lib"
    PYTHON_RUNTIME_LICENSE="$PACKAGE_DIR/lib/python/LICENSE.txt"
else
    PYTHON_LIB="$PACKAGE_DIR/lib/python/lib/python${PYTHON_XY}"
    PYTHON_RUNTIME_LICENSE="$PYTHON_LIB/LICENSE.txt"
fi
NUMPY_LICENSES="$PYTHON_LIB/site-packages/numpy-${NUMPY_VERSION}.dist-info/licenses"
SCIPY_LICENSE="$PYTHON_LIB/site-packages/scipy-${SCIPY_VERSION}.dist-info/LICENSE.txt"
for notice in "$PYTHON_RUNTIME_LICENSE" "$NUMPY_LICENSES/LICENSE.txt" "$SCIPY_LICENSE"; do
    [[ -s "$notice" ]] || {
        echo "[licences] ERREUR : notice absente du paquet : $notice" >&2
        exit 1
    }
done
for tool in cargo-deny cargo-about; do
    command -v "$tool" >/dev/null || {
        echo "[licences] ERREUR : installer $tool (voir docs/licence-et-extensions.md)" >&2
        exit 1
    }
done
[[ "$(cargo-deny --version)" == "cargo-deny ${CARGO_DENY_VERSION}" &&
   "$(cargo-about --version)" == "cargo-about ${CARGO_ABOUT_VERSION}" ]] || {
    echo "[licences] ERREUR : utiliser les versions de scripts/licenses-tools.env" >&2
    exit 1
}

cd "$REPO_ROOT"
cargo deny --locked check licenses
TEMP_DIR="$(mktemp -d "$PACKAGE_DIR/.licenses-XXXXXX")"
trap 'rm -rf "$TEMP_DIR"' EXIT
"$REPO_ROOT/scripts/licenses-rust.sh" "$TEMP_DIR/THIRD-PARTY-LICENSES.txt"

cp LICENSE LICENSE-EXCEPTION "$PACKAGE_DIR/"
mkdir -p "$PACKAGE_DIR/licenses/fonts" "$PACKAGE_DIR/licenses/epaint-default-fonts" \
    "$PACKAGE_DIR/licenses/phosphor" \
    "$PACKAGE_DIR/licenses/winit" "$PACKAGE_DIR/licenses/smithay-clipboard" \
    "$PACKAGE_DIR/licenses/python" "$PACKAGE_DIR/licenses/numpy" "$PACKAGE_DIR/licenses/scipy"
for notice in vendor/fonts/*/OFL.txt; do
    family="$(basename "$(dirname "$notice")")"
    mkdir -p "$PACKAGE_DIR/licenses/fonts/$family"
    cp "$notice" "$PACKAGE_DIR/licenses/fonts/$family/OFL.txt"
done
cp crates/stokesforge-app/assets/icons/LICENSE "$PACKAGE_DIR/licenses/phosphor/LICENSE"
cp -a vendor/epaint-default-fonts/. "$PACKAGE_DIR/licenses/epaint-default-fonts/"
cp vendor/winit/LICENSE vendor/winit/STOKESFORGE.md "$PACKAGE_DIR/licenses/winit/"
cp vendor/smithay-clipboard/LICENSE "$PACKAGE_DIR/licenses/smithay-clipboard/"
cp -a "$NOTICES_DIR/." "$PACKAGE_DIR/licenses/python/"
cp "$PYTHON_RUNTIME_LICENSE" "$PACKAGE_DIR/licenses/python/LICENSE-runtime.txt"
cp -a "$NUMPY_LICENSES/." "$PACKAGE_DIR/licenses/numpy/"
cp "$SCIPY_LICENSE" "$PACKAGE_DIR/licenses/scipy/LICENSE.txt"
mv "$TEMP_DIR/THIRD-PARTY-LICENSES.txt" "$PACKAGE_DIR/THIRD-PARTY-LICENSES.txt"
echo "[licences] OK : licences et notices livrées dans $PACKAGE_DIR"

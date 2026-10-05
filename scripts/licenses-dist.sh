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
# shellcheck source=../vendor/python/licenses/versions.env
source "$REPO_ROOT/vendor/python/licenses/versions.env"
# shellcheck source=licenses-tools.env
source "$REPO_ROOT/scripts/licenses-tools.env"

if [[ "$PYTHON_VERSION" != "$LICENSE_PYTHON_VERSION" ||
      "$PBS_RELEASE" != "$LICENSE_PBS_RELEASE" ||
      "$PBS_TRIPLE" != "$LICENSE_PBS_TRIPLE" ]]; then
    echo "[licences] ERREUR : actualiser vendor/python/licenses pour le verrou Python" >&2
    exit 1
fi

PYTHON_LIB="$PACKAGE_DIR/lib/python/lib/python${PYTHON_XY}"
NUMPY_LICENSES="$PYTHON_LIB/site-packages/numpy-${NUMPY_VERSION}.dist-info/licenses"
for notice in "$PYTHON_LIB/LICENSE.txt" "$NUMPY_LICENSES/LICENSE.txt"; do
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
    "$PACKAGE_DIR/licenses/python" "$PACKAGE_DIR/licenses/numpy"
for notice in vendor/fonts/*/OFL.txt; do
    family="$(basename "$(dirname "$notice")")"
    mkdir -p "$PACKAGE_DIR/licenses/fonts/$family"
    cp "$notice" "$PACKAGE_DIR/licenses/fonts/$family/OFL.txt"
done
cp crates/stokesforge-app/assets/icons/LICENSE "$PACKAGE_DIR/licenses/phosphor/LICENSE"
cp -a vendor/epaint-default-fonts/. "$PACKAGE_DIR/licenses/epaint-default-fonts/"
cp vendor/winit/LICENSE vendor/winit/STOKESFORGE.md "$PACKAGE_DIR/licenses/winit/"
cp vendor/smithay-clipboard/LICENSE "$PACKAGE_DIR/licenses/smithay-clipboard/"
cp -a vendor/python/licenses/. "$PACKAGE_DIR/licenses/python/"
cp "$PYTHON_LIB/LICENSE.txt" "$PACKAGE_DIR/licenses/python/LICENSE-runtime.txt"
cp -a "$NUMPY_LICENSES/." "$PACKAGE_DIR/licenses/numpy/"
mv "$TEMP_DIR/THIRD-PARTY-LICENSES.txt" "$PACKAGE_DIR/THIRD-PARTY-LICENSES.txt"
echo "[licences] OK : licences et notices livrées dans $PACKAGE_DIR"

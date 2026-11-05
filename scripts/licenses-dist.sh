#!/usr/bin/env bash
# Delivers the license texts into an already assembled package (#129).
# The Cargo resolution includes python, transitive dependencies and all
# platforms; it builds neither the workspace nor the interpreter.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ $# -eq 1 && -d "$1" ]] || {
    echo "Usage: scripts/licenses-dist.sh <assembled package directory>" >&2
    exit 1
}
PACKAGE_DIR="$(cd "$1" && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"
# Package platform: the machine's own, unless STOKESFORGE_LICENSES_PLATFORM
# (linux|windows) is set, so that licenses-test.sh also tests the refusals of
# the other one.
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
    echo "[licences] ERROR: update vendor/python/licenses-$PLATFORM to match the Python lock" >&2
    exit 1
fi

if [[ "$PLATFORM" == windows ]]; then
    # Layout lib/python/{Lib,DLLs} (scripts/app-dist-windows.sh); CPython's
    # LICENSE.txt is at the prefix root, copied into lib/python.
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
        echo "[licences] ERROR: notice missing from the package: $notice" >&2
        exit 1
    }
done
for tool in cargo-deny cargo-about; do
    command -v "$tool" >/dev/null || {
        echo "[licences] ERROR: install $tool (see docs/license-and-extensions.md)" >&2
        exit 1
    }
done
[[ "$(cargo-deny --version)" == "cargo-deny ${CARGO_DENY_VERSION}" &&
   "$(cargo-about --version)" == "cargo-about ${CARGO_ABOUT_VERSION}" ]] || {
    echo "[licences] ERROR: use the versions from scripts/licenses-tools.env" >&2
    exit 1
}

cd "$REPO_ROOT"
cargo deny --locked check licenses
TEMP_DIR="$(mktemp -d "$PACKAGE_DIR/.licenses-XXXXXX")"
trap 'rm -rf "$TEMP_DIR"' EXIT
"$REPO_ROOT/scripts/licenses-rust.sh" "$TEMP_DIR/THIRD-PARTY-LICENSES.txt"

cp LICENSE "$PACKAGE_DIR/"
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
echo "[licences] OK: licenses and notices delivered in $PACKAGE_DIR"

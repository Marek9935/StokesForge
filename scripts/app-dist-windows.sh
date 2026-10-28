#!/usr/bin/env bash
# Assembles, VALIDATES and packages the Windows distribution of
# `stokesforge-app` (Python bridge included) and of `stokesforge-selftest`
# (#332): an Inno Setup installer. Windows counterpart of scripts/app-dist.sh.
# Run it under Git Bash, after scripts/python-vendor.sh. Prerequisites: Rust
# MSVC, cargo-deny and cargo-about at the versions of scripts/licenses-tools.env,
# Inno Setup 6 (ISCC.exe, looked up through $ISCC, the PATH, then the usual
# installation folders).
#
# Flow: 1. release build; 2. assembly of the staging folder
# (dist/stokesforge-app-windows/); 3. license notices; 4. tests of the staging
# (window, selftest in an emptied and trapped environment, two refusal
# witnesses); 5. build of the installer; 6. test of the installer (installation
# in a folder with a space and an accent, selftest from the installation,
# complete uninstallation).
#
# Staging layout:
#   stokesforge-app.exe, stokesforge-selftest.exe, python312.dll, Visual C++
#   runtime DLLs (side by side: the Windows DLL search starts from the
#                 executable's folder, no rpath equivalent is needed)
#   lib/python/{Lib,DLLs,LICENSE.txt}   the embedded prefix, where
#                 EmbeddedPython::locate() looks for it
#
# Variables: STOKESFORGE_DIST_SKIP_LICENSES=1 (local trial without cargo-about,
# package not publishable), STOKESFORGE_VERSION (default: version of
# stokesforge-app), STOKESFORGE_APP_DIST_AUTOQUIT_MS,
# STOKESFORGE_DIST_SKIP_WINDOW=1 (machine without a desktop, e.g. a Windows
# service: skips opening the window, never the selftest), ISCC.
set -euo pipefail

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "[dist] ERROR: Windows script, run it under Git Bash" >&2; exit 1 ;;
esac
# Git Bash rewrites the arguments that look like a path (`/DIR=…`,
# `/VERYSILENT`): we turn that off for the calls to ISCC, to the installer and
# to reg.
export MSYS2_ARG_CONV_EXCL='*'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"

PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/stokesforge-app-windows"
DLL="python${PYTHON_XY/./}.dll"
AUTOQUIT_MS="${STOKESFORGE_APP_DIST_AUTOQUIT_MS:-1500}"
VERSION="${STOKESFORGE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/crates/stokesforge-app/Cargo.toml" | head -1)}"

# Emptied environment: only what Windows itself requires and a PATH reduced to
# System32 (no Python, no folder of the repository can provide a DLL).
clean_env() {
    env -i SYSTEMROOT="${SYSTEMROOT:-C:\\Windows}" SystemDrive="${SystemDrive:-C:}" \
        TEMP="${TEMP:-C:\\Windows\\Temp}" TMP="${TMP:-C:\\Windows\\Temp}" \
        PATH=/c/Windows/System32:/c/Windows "$@"
}

find_iscc() {
    local candidate
    for candidate in "${ISCC:-}" "$(command -v ISCC.exe 2>/dev/null || true)" \
        "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" \
        "/c/Program Files/Inno Setup 6/ISCC.exe" \
        "${LOCALAPPDATA:-}/Programs/Inno Setup 6/ISCC.exe"; do
        if [[ -n "$candidate" && -x "$candidate" ]]; then
            printf '%s' "$candidate"
            return 0
        fi
    done
    return 1
}
ISCC_BIN="$(find_iscc)" || {
    echo "[dist] ERROR: Inno Setup 6 not found (ISCC.exe); install it or set \$ISCC" >&2
    exit 1
}

PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE

cd "$REPO_ROOT"
echo "[dist] release build of stokesforge-app (python feature)"
cargo build --locked -p stokesforge-app --features python --release --bin stokesforge-app
echo "[dist] release build of stokesforge-selftest (python feature)"
cargo build --locked -p stokesforge-script --features python --release --bin stokesforge-selftest

echo "[dist] assembling $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/lib/python"
cp target/release/stokesforge-app.exe target/release/stokesforge-selftest.exe "$DIST/"
cp "$PREFIX/$DLL" "$DIST/$DLL"
# Visual C++ runtime next to the executable: a clean machine does not always
# have the redistributable. `vcruntime140*.dll` comes from
# python-build-standalone; `msvcp140.dll` is not there, but the numpy wheel
# embeds the official copy under a suffixed name (delvewheel): renamed copy,
# same bytes.
cp "$PREFIX"/vcruntime140.dll "$PREFIX"/vcruntime140_1.dll "$DIST/"
cp "$PREFIX"/Lib/site-packages/numpy.libs/msvcp140-*.dll "$DIST/msvcp140.dll"
cp -a "$PREFIX/Lib" "$DIST/lib/python/Lib"
cp -a "$PREFIX/DLLs" "$DIST/lib/python/DLLs"
cp "$PREFIX/LICENSE.txt" "$DIST/lib/python/LICENSE.txt"
# Slimming (W5): no pip, no tests, no tkinter/Tcl/Tk, no _msi, no scipy wheel
# left in site-packages. `import tkinter` fails in a script.
rm -rf "$DIST/lib/python/Lib/site-packages/pip" \
       "$DIST/lib/python/Lib/site-packages/pip-"*.dist-info \
       "$DIST/lib/python/Lib/site-packages/"*.whl \
       "$DIST/lib/python/Lib/test" "$DIST/lib/python/Lib/idlelib" \
       "$DIST/lib/python/Lib/turtledemo" "$DIST/lib/python/Lib/tkinter" \
       "$DIST/lib/python/DLLs/"_test*.pyd "$DIST/lib/python/DLLs/_ctypes_test.pyd" \
       "$DIST/lib/python/DLLs/_msi.pyd" "$DIST/lib/python/DLLs/_tkinter.pyd" \
       "$DIST/lib/python/DLLs/tcl86t.dll" "$DIST/lib/python/DLLs/tk86t.dll"

# STOKESFORGE_DIST_SKIP_LICENSES=1: local trial without cargo-about. The
# package then has no notices: the installer is named "-NO-NOTICES" and is not
# publishable.
SUFFIX=""
if [[ "${STOKESFORGE_DIST_SKIP_LICENSES:-0}" == "1" ]]; then
    echo "[dist] WARNING: license notices NOT delivered (STOKESFORGE_DIST_SKIP_LICENSES=1)" >&2
    cp LICENSE "$DIST/"
    SUFFIX="-NO-NOTICES"
else
    "$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"
fi

echo "[dist] size of $DIST: $(du -sm "$DIST" | cut -f1) MB"

# --- Tests of the staging ---------------------------------------------------
if [[ "${STOKESFORGE_DIST_SKIP_WINDOW:-0}" == "1" ]]; then
    echo "[dist] WARNING: opening the window skipped (STOKESFORGE_DIST_SKIP_WINDOW=1)" >&2
else
    # Throwaway configuration folder (#333): with no argument, the first opening
    # writes the demonstration into `demo.stf` of this folder and opens it; this
    # is the production path that the packaged window tests, without touching the
    # real %APPDATA%.
    CONFIG_DIR="$(mktemp -d)"
    echo "[dist] packaged window, auto-close after ${AUTOQUIT_MS} ms"
    STOKESFORGE_APP_AUTOQUIT_MS="$AUTOQUIT_MS" clean_env \
        STOKESFORGE_CONFIG_DIR="$(cygpath -w "$CONFIG_DIR")" \
        STOKESFORGE_APP_AUTOQUIT_MS="$AUTOQUIT_MS" "$DIST/stokesforge-app.exe"
    if [[ ! -s "$CONFIG_DIR/demo.stf" ]] || ! grep -q '"first_run_done": true' "$CONFIG_DIR/preferences.json" 2>/dev/null; then
        echo "[dist] ERROR: the first opening did not write demo.stf and the marker in $CONFIG_DIR" >&2
        rm -rf "$CONFIG_DIR"
        exit 1
    fi
    rm -rf "$CONFIG_DIR"
fi

echo "[dist] self-check from the staging, emptied and trapped environment"
clean_env PYTHONPATH=C:/piege PYTHONHOME=C:/piege2 "$DIST/stokesforge-selftest.exe"

# Witness 1: python312.dll must come from the executable's folder. An exe alone,
# without its DLL, does not start (otherwise the DLL comes from elsewhere and
# the package lies).
ALONE="$(mktemp -d)"
cp "$DIST/stokesforge-selftest.exe" "$ALONE/"
if clean_env "$ALONE/stokesforge-selftest.exe" >/dev/null 2>&1; then
    echo "[dist] ERROR: stokesforge-selftest starts without $DLL next to it" >&2
    exit 1
fi
rm -rf "$ALONE"
# Witness 2: the embedded prefix must come from lib/python. Moved away, the
# self-check must fail (otherwise it reads a Python elsewhere).
mv "$DIST/lib/python" "$DIST/lib/python.off"
if clean_env "$DIST/stokesforge-selftest.exe" >/dev/null 2>&1; then
    mv "$DIST/lib/python.off" "$DIST/lib/python"
    echo "[dist] ERROR: stokesforge-selftest succeeds without lib/python" >&2
    exit 1
fi
mv "$DIST/lib/python.off" "$DIST/lib/python"

# --- Installer --------------------------------------------------------------
SETUP="$REPO_ROOT/dist/StokesForge-${VERSION}${SUFFIX}-windows-setup.exe"
echo "[dist] building installer $VERSION ($ISCC_BIN)"
rm -f "$SETUP"
"$ISCC_BIN" /Q "/DAppVersion=$VERSION" "/DSuffix=$SUFFIX" "/DSourceDir=$(cygpath -w "$DIST")" \
    "/DOutputDir=$(cygpath -w "$REPO_ROOT/dist")" "$(cygpath -w "$REPO_ROOT/scripts/stokesforge.iss")"
[[ -s "$SETUP" ]] || { echo "[dist] ERROR: installer missing: $SETUP" >&2; exit 1; }
echo "[dist] installer: $SETUP ($(du -sm "$SETUP" | cut -f1) MB)"

# Test of the installer: folder with a space and an accent, per user, selftest
# from the installation, .stf association, complete uninstallation.
WORK="$(mktemp -d)"
INSTALL="$WORK/Stokes Forgé"
"$SETUP" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER \
    "/DIR=$(cygpath -w "$INSTALL")" "/LOG=$(cygpath -w "$WORK/install.log")"
for expected in stokesforge-app.exe stokesforge-selftest.exe "$DLL" msvcp140.dll \
    lib/python/Lib/os.py LICENSE unins000.exe; do
    [[ -e "$INSTALL/$expected" ]] || { echo "[dist] ERROR: $expected missing from the installation" >&2; exit 1; }
done
echo "[dist] self-check from the installation ($INSTALL)"
clean_env PYTHONPATH=C:/piege PYTHONHOME=C:/piege2 "$INSTALL/stokesforge-selftest.exe"
reg query 'HKCU\Software\Classes\StokesForge.Project\shell\open\command' >/dev/null \
    || { echo "[dist] ERROR: .stf association missing from the registry" >&2; exit 1; }
"$INSTALL/unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
# The uninstaller relaunches itself from a temporary folder: wait for it to finish.
for _ in $(seq 1 60); do
    [[ -e "$INSTALL" ]] || break
    sleep 1
done
[[ ! -e "$INSTALL" ]] || { echo "[dist] ERROR: incomplete uninstallation: $INSTALL" >&2; ls "$INSTALL" >&2; exit 1; }
if reg query 'HKCU\Software\Classes\StokesForge.Project' >/dev/null 2>&1; then
    echo "[dist] ERROR: .stf keys left in the registry after uninstallation" >&2
    exit 1
fi
rm -rf "$WORK"

echo "[dist] OK: staging opened and self-checked, installer installed, verified then uninstalled (code 0)"

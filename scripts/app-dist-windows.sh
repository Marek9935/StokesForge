#!/usr/bin/env bash
# Assemble, VALIDE et empaquette la distribution Windows de `stokesforge-app`
# (pont Python compris) et de `stokesforge-selftest` (#332) : un installeur
# Inno Setup. Pendant Windows de scripts/app-dist.sh. À lancer sous Git Bash,
# après scripts/python-vendor.sh. Prérequis : Rust MSVC, cargo-deny et
# cargo-about aux versions de scripts/licenses-tools.env, Inno Setup 6 (ISCC.exe,
# cherché par $ISCC, le PATH, puis les dossiers d'installation habituels).
#
# Déroulé : 1. compilation release ; 2. assemblage du dossier de staging
# (dist/stokesforge-app-windows/) ; 3. notices de licences ; 4. épreuves du
# staging (fenêtre, selftest en environnement vidé et piégé, deux témoins de
# refus) ; 5. compilation de l'installeur ; 6. épreuve de l'installeur
# (installation dans un dossier à espace et accent, selftest depuis
# l'installation, désinstallation complète).
#
# Disposition du staging :
#   stokesforge-app.exe, stokesforge-selftest.exe, python312.dll, DLL du runtime
#   Visual C++   (côte à côte : la recherche de DLL de Windows part du dossier de
#                 l'exécutable, aucun équivalent du rpath n'est nécessaire)
#   lib/python/{Lib,DLLs,LICENSE.txt}   le préfixe embarqué, là où
#                 EmbeddedPython::locate() le cherche
#
# Variables : STOKESFORGE_DIST_SKIP_LICENSES=1 (essai local sans cargo-about,
# paquet non publiable), STOKESFORGE_VERSION (défaut : version de stokesforge-app),
# STOKESFORGE_APP_DIST_AUTOQUIT_MS, STOKESFORGE_DIST_SKIP_WINDOW=1 (poste sans
# bureau, ex. service Windows : saute l'ouverture de la fenêtre, jamais le
# selftest), ISCC.
set -euo pipefail

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "[dist] ERREUR : script Windows, à lancer sous Git Bash" >&2; exit 1 ;;
esac
# Git Bash réécrit les arguments qui ressemblent à un chemin (`/DIR=…`,
# `/VERYSILENT`) : on le coupe pour les appels à ISCC, à l'installeur et à reg.
export MSYS2_ARG_CONV_EXCL='*'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"

PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/stokesforge-app-windows"
DLL="python${PYTHON_XY/./}.dll"
AUTOQUIT_MS="${STOKESFORGE_APP_DIST_AUTOQUIT_MS:-1500}"
VERSION="${STOKESFORGE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/crates/stokesforge-app/Cargo.toml" | head -1)}"

# Environnement vidé : seuls ce qu'exige Windows lui-même et un PATH réduit à
# System32 (aucun Python, aucun dossier du dépôt ne peut fournir une DLL).
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
    echo "[dist] ERREUR : Inno Setup 6 introuvable (ISCC.exe) ; l'installer ou poser \$ISCC" >&2
    exit 1
}

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
# Runtime Visual C++ à côté de l'exécutable : un poste vierge n'a pas toujours
# le redistribuable. `vcruntime140*.dll` vient de python-build-standalone ;
# `msvcp140.dll` n'y est pas, mais la roue numpy en embarque la copie
# officielle sous un nom suffixé (delvewheel) : copie renommée, mêmes octets.
cp "$PREFIX"/vcruntime140.dll "$PREFIX"/vcruntime140_1.dll "$DIST/"
cp "$PREFIX"/Lib/site-packages/numpy.libs/msvcp140-*.dll "$DIST/msvcp140.dll"
cp -a "$PREFIX/Lib" "$DIST/lib/python/Lib"
cp -a "$PREFIX/DLLs" "$DIST/lib/python/DLLs"
cp "$PREFIX/LICENSE.txt" "$DIST/lib/python/LICENSE.txt"
# Allègement (W5) : ni pip, ni tests, ni tkinter/Tcl/Tk, ni _msi, ni la roue
# scipy restée dans site-packages. `import tkinter` échoue dans un script.
rm -rf "$DIST/lib/python/Lib/site-packages/pip" \
       "$DIST/lib/python/Lib/site-packages/pip-"*.dist-info \
       "$DIST/lib/python/Lib/site-packages/"*.whl \
       "$DIST/lib/python/Lib/test" "$DIST/lib/python/Lib/idlelib" \
       "$DIST/lib/python/Lib/turtledemo" "$DIST/lib/python/Lib/tkinter" \
       "$DIST/lib/python/DLLs/"_test*.pyd "$DIST/lib/python/DLLs/_ctypes_test.pyd" \
       "$DIST/lib/python/DLLs/_msi.pyd" "$DIST/lib/python/DLLs/_tkinter.pyd" \
       "$DIST/lib/python/DLLs/tcl86t.dll" "$DIST/lib/python/DLLs/tk86t.dll"

# STOKESFORGE_DIST_SKIP_LICENSES=1 : essai local sans cargo-about. Le paquet
# n'a alors pas ses notices : l'installeur est nommé « -NO-NOTICES » et n'est
# pas publiable.
SUFFIX=""
if [[ "${STOKESFORGE_DIST_SKIP_LICENSES:-0}" == "1" ]]; then
    echo "[dist] ATTENTION : notices de licences NON livrées (STOKESFORGE_DIST_SKIP_LICENSES=1)" >&2
    cp LICENSE "$DIST/"
    SUFFIX="-NO-NOTICES"
else
    "$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"
fi

echo "[dist] taille de $DIST : $(du -sm "$DIST" | cut -f1) Mo"

# --- Épreuves du staging ----------------------------------------------------
if [[ "${STOKESFORGE_DIST_SKIP_WINDOW:-0}" == "1" ]]; then
    echo "[dist] ATTENTION : ouverture de la fenêtre sautée (STOKESFORGE_DIST_SKIP_WINDOW=1)" >&2
else
    # Dossier de configuration jetable (#333) : sans argument, la première
    # ouverture écrit la démonstration dans `demo.stf` de ce dossier et l'ouvre ;
    # c'est ce parcours de production que la fenêtre empaquetée éprouve, sans
    # toucher au vrai %APPDATA%.
    CONFIG_DIR="$(mktemp -d)"
    echo "[dist] fenêtre empaquetée, fermeture auto après ${AUTOQUIT_MS} ms"
    STOKESFORGE_APP_AUTOQUIT_MS="$AUTOQUIT_MS" clean_env \
        STOKESFORGE_CONFIG_DIR="$(cygpath -w "$CONFIG_DIR")" \
        STOKESFORGE_APP_AUTOQUIT_MS="$AUTOQUIT_MS" "$DIST/stokesforge-app.exe"
    if [[ ! -s "$CONFIG_DIR/demo.stf" ]] || ! grep -q '"first_run_done": true' "$CONFIG_DIR/preferences.json" 2>/dev/null; then
        echo "[dist] ERREUR : la première ouverture n'a pas écrit demo.stf et le marqueur dans $CONFIG_DIR" >&2
        rm -rf "$CONFIG_DIR"
        exit 1
    fi
    rm -rf "$CONFIG_DIR"
fi

echo "[dist] auto-vérification depuis le staging, environnement vidé et piégé"
clean_env PYTHONPATH=C:/piege PYTHONHOME=C:/piege2 "$DIST/stokesforge-selftest.exe"

# Témoin 1 : python312.dll doit venir du dossier de l'exécutable. Un exe seul,
# sans sa DLL, ne démarre pas (sinon la DLL vient d'ailleurs et le paquet ment).
ALONE="$(mktemp -d)"
cp "$DIST/stokesforge-selftest.exe" "$ALONE/"
if clean_env "$ALONE/stokesforge-selftest.exe" >/dev/null 2>&1; then
    echo "[dist] ERREUR : stokesforge-selftest démarre sans $DLL à côté de lui" >&2
    exit 1
fi
rm -rf "$ALONE"
# Témoin 2 : le préfixe embarqué doit venir de lib/python. Déplacé, l'auto-
# vérification doit échouer (sinon elle lit un Python ailleurs).
mv "$DIST/lib/python" "$DIST/lib/python.off"
if clean_env "$DIST/stokesforge-selftest.exe" >/dev/null 2>&1; then
    mv "$DIST/lib/python.off" "$DIST/lib/python"
    echo "[dist] ERREUR : stokesforge-selftest réussit sans lib/python" >&2
    exit 1
fi
mv "$DIST/lib/python.off" "$DIST/lib/python"

# --- Installeur -------------------------------------------------------------
SETUP="$REPO_ROOT/dist/StokesForge-${VERSION}${SUFFIX}-windows-setup.exe"
echo "[dist] compilation de l'installeur $VERSION ($ISCC_BIN)"
rm -f "$SETUP"
"$ISCC_BIN" /Q "/DAppVersion=$VERSION" "/DSuffix=$SUFFIX" "/DSourceDir=$(cygpath -w "$DIST")" \
    "/DOutputDir=$(cygpath -w "$REPO_ROOT/dist")" "$(cygpath -w "$REPO_ROOT/scripts/stokesforge.iss")"
[[ -s "$SETUP" ]] || { echo "[dist] ERREUR : installeur absent : $SETUP" >&2; exit 1; }
echo "[dist] installeur : $SETUP ($(du -sm "$SETUP" | cut -f1) Mo)"

# Épreuve de l'installeur : dossier avec espace et accent, par utilisateur,
# selftest depuis l'installation, association .stf, désinstallation complète.
WORK="$(mktemp -d)"
INSTALL="$WORK/Stokes Forgé"
"$SETUP" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER \
    "/DIR=$(cygpath -w "$INSTALL")" "/LOG=$(cygpath -w "$WORK/install.log")"
for expected in stokesforge-app.exe stokesforge-selftest.exe "$DLL" msvcp140.dll \
    lib/python/Lib/os.py LICENSE unins000.exe; do
    [[ -e "$INSTALL/$expected" ]] || { echo "[dist] ERREUR : $expected absent de l'installation" >&2; exit 1; }
done
echo "[dist] auto-vérification depuis l'installation ($INSTALL)"
clean_env PYTHONPATH=C:/piege PYTHONHOME=C:/piege2 "$INSTALL/stokesforge-selftest.exe"
reg query 'HKCU\Software\Classes\StokesForge.Project\shell\open\command' >/dev/null \
    || { echo "[dist] ERREUR : association .stf absente du registre" >&2; exit 1; }
"$INSTALL/unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
# Le désinstalleur se relance depuis un dossier temporaire : attendre sa fin.
for _ in $(seq 1 60); do
    [[ -e "$INSTALL" ]] || break
    sleep 1
done
[[ ! -e "$INSTALL" ]] || { echo "[dist] ERREUR : désinstallation incomplète : $INSTALL" >&2; ls "$INSTALL" >&2; exit 1; }
if reg query 'HKCU\Software\Classes\StokesForge.Project' >/dev/null 2>&1; then
    echo "[dist] ERREUR : clés .stf restées dans le registre après désinstallation" >&2
    exit 1
fi
rm -rf "$WORK"

echo "[dist] OK — staging ouvert et auto-vérifié, installeur installé, vérifié puis désinstallé (code 0)"

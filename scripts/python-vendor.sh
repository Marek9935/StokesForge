#!/usr/bin/env bash
# Vendorise l'interpréteur CPython embarqué et y installe numpy épinglé.
#
# Idempotent : relançable sans effet de bord. Ne touche à aucun Python système.
# Lit toutes ses versions dans vendor/python/lock.env (piège n°6 : un seul verrou).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$REPO_ROOT/vendor/python"
# shellcheck source=../vendor/python/lock.env
source "$VENDOR_DIR/lock.env"

# Sélection du triplet par OS (tâche 5.1b) : `$OS` vaut `Windows_NT` sous
# Git Bash/MSYS2 sur Windows (posé par cmd.exe, jamais par bash lui-même,
# donc fiable même si `uname` répond MINGW64/MSYS). Ailleurs, `uname -s`.
if [[ "${OS:-}" == "Windows_NT" ]]; then
    PBS_TRIPLE="$PBS_TRIPLE_WINDOWS"
    PY_BIN_REL="python.exe"
else
    PBS_TRIPLE="$PBS_TRIPLE_LINUX"
    PY_BIN_REL="bin/python3"
fi

ARCHIVE="$VENDOR_DIR/cpython-${PYTHON_VERSION}-${PBS_TRIPLE}-${PBS_FLAVOR}.tar.gz"
EXTRACTED="$VENDOR_DIR/extracted"
PREFIX="$EXTRACTED/python"

# --- 1. Archive -------------------------------------------------------------
# Téléchargée seulement si absente : l'archive fait ~110 Mo et n'est pas versionnée.
if [[ -f "$ARCHIVE" ]]; then
    echo "[vendor] archive déjà présente : $ARCHIVE"
else
    ASSET="cpython-${PYTHON_VERSION}+${PBS_RELEASE}-${PBS_TRIPLE}-${PBS_FLAVOR}.tar.gz"
    URL="https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_RELEASE}/${ASSET}"
    echo "[vendor] téléchargement de $URL"
    mkdir -p "$VENDOR_DIR"
    curl -fL --retry 3 -o "$ARCHIVE.part" "$URL"
    mv "$ARCHIVE.part" "$ARCHIVE"
fi

# --- 2. Extraction ----------------------------------------------------------
PY="$PREFIX/$PY_BIN_REL"
if [[ -x "$PY" ]]; then
    echo "[vendor] préfixe déjà extrait : $PREFIX"
else
    echo "[vendor] extraction dans $EXTRACTED"
    mkdir -p "$EXTRACTED"
    tar -xzf "$ARCHIVE" -C "$EXTRACTED"
fi

[[ -x "$PY" ]] || { echo "[vendor] ERREUR : $PY introuvable après extraction" >&2; exit 1; }

# Garde-fou : l'interpréteur extrait doit bien être celui du verrou.
GOT_VERSION="$("$PY" -c 'import platform; print(platform.python_version())')"
if [[ "$GOT_VERSION" != "$PYTHON_VERSION" ]]; then
    echo "[vendor] ERREUR : interpréteur $GOT_VERSION, verrou $PYTHON_VERSION" >&2
    exit 1
fi

# --- 3. numpy épinglé -------------------------------------------------------
# `-I` isole l'appel pip lui-même (pas de PYTHONPATH/site utilisateur qui traîne).
# `--no-deps` : numpy n'a pas de dépendance d'exécution, on refuse toute surprise.
# `--only-binary=:all:` : jamais de compilation locale, on veut la roue officielle.
CURRENT_NUMPY="$("$PY" -I -c 'import numpy; print(numpy.__version__)' 2>/dev/null || true)"
if [[ "$CURRENT_NUMPY" == "$NUMPY_VERSION" ]]; then
    echo "[vendor] numpy $NUMPY_VERSION déjà installé dans le préfixe embarqué"
else
    if ! "$PY" -I -m pip --version >/dev/null 2>&1; then
        echo "[vendor] pip absent, amorçage par ensurepip"
        "$PY" -I -m ensurepip --upgrade
    fi
    echo "[vendor] installation de numpy==$NUMPY_VERSION"
    "$PY" -I -m pip install --no-deps --only-binary=:all: --upgrade "numpy==$NUMPY_VERSION"
fi

# --- 4. Vérification finale -------------------------------------------------
"$PY" -I -c "
import numpy, sys
assert numpy.__version__ == '$NUMPY_VERSION', numpy.__version__
print('[vendor] OK  python', sys.version.split()[0], ' numpy', numpy.__version__)
print('[vendor] numpy chargé depuis', numpy.__file__)
"

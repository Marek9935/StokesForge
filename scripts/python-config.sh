#!/usr/bin/env bash
# Génère la configuration PyO3 pointant sur l'interpréteur embarqué (piège n°1).
#
# Écrit target/pyo3-config.txt à partir du gabarit versionné
# crates/stokesforge-python/pyo3-config.template.txt, puis affiche sur stdout le
# chemin absolu du fichier produit — de sorte qu'un appelant puisse faire :
#     export PYO3_CONFIG_FILE="$(scripts/python-config.sh)"
# Les messages d'avancement partent sur stderr pour ne pas polluer cette sortie.
#
# À appeler en sous-shell, pas à sourcer : il se termine par `exit 1` si le
# préfixe embarqué manque.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"
TEMPLATE="$REPO_ROOT/crates/stokesforge-python/pyo3-config.template.txt"
PY_REL="bin/python3"
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
        # Windows (#332) : python.exe et python312.lib (libs/) ; chemins en
        # style Windows (C:/…) pour pyo3-build-config.
        TEMPLATE="$REPO_ROOT/crates/stokesforge-python/pyo3-config.windows.template.txt"
        PY_REL="python.exe"
        PREFIX="$(cygpath -m "$PREFIX")" ;;
esac
OUT="$REPO_ROOT/target/pyo3-config.txt"

if [[ ! -x "$PREFIX/$PY_REL" ]]; then
    echo "[config] ERREUR : préfixe embarqué absent ($PREFIX). Lance d'abord scripts/python-vendor.sh" >&2
    exit 1
fi

mkdir -p "$(dirname "$OUT")"
# Substitution du seul champ dépendant de la machine, puis retrait des lignes
# de commentaire et des lignes vides : pyo3-build-config ne lit que des paires
# clé=valeur. Le préfixe ne contient pas de « | », choisi comme séparateur sed
# pour éviter d'échapper les « / ».
sed -e "s|@PYTHON_PREFIX@|$PREFIX|g" -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' \
    "$TEMPLATE" > "$OUT"

echo "[config] PYO3_CONFIG_FILE=$OUT (prefix=$PREFIX)" >&2
echo "$OUT"

#!/usr/bin/env bash
# Génère la configuration PyO3 pointant sur l'interpréteur embarqué (piège n°1).
#
# Écrit target/pyo3-config.txt à partir du gabarit versionné
# crates/plotyx-python/pyo3-config.template.txt, puis affiche sur stdout le
# chemin absolu du fichier produit — de sorte qu'un appelant puisse faire :
#     export PYO3_CONFIG_FILE="$(scripts/python-config.sh)"
# Les messages d'avancement partent sur stderr pour ne pas polluer cette sortie.
#
# À appeler en sous-shell, pas à sourcer : il se termine par `exit 1` si le
# préfixe embarqué manque.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"
OUT="$REPO_ROOT/target/pyo3-config.txt"

# Même détection que scripts/python-vendor.sh (tâche 5.1b) : deux gabarits,
# deux dispositions de fabricant, cf. pyo3-config.windows.template.txt.
if [[ "${OS:-}" == "Windows_NT" ]]; then
    TEMPLATE="$REPO_ROOT/crates/plotyx-python/pyo3-config.windows.template.txt"
    PY_BIN_REL="python.exe"
else
    TEMPLATE="$REPO_ROOT/crates/plotyx-python/pyo3-config.template.txt"
    PY_BIN_REL="bin/python3"
fi

if [[ ! -x "$PREFIX/$PY_BIN_REL" ]]; then
    echo "[config] ERREUR : préfixe embarqué absent ($PREFIX). Lance d'abord scripts/python-vendor.sh" >&2
    exit 1
fi

mkdir -p "$(dirname "$OUT")"

# Chemin substitué dans le gabarit, et chemin renvoyé sur stdout : sous MSYS
# (Git Bash), `$PREFIX`/`$OUT` sont des chemins POSIX (`/c/...`) — un
# artefact du shell, pas un format que `rustc`/pyo3-build-config comprend.
# Le premier est lu comme du **contenu** de fichier, le second devient la
# valeur de la variable PYO3_CONFIG_FILE exportée pour `cargo` : ni l'un ni
# l'autre ne passe par la conversion d'arguments/PATH que MSYS fait pour un
# processus natif, donc aucun des deux ne se traduit tout seul.
# `/c/foo` → `c:/foo` (barres obliques gardées : Windows et pyo3-build-config
# les acceptent aussi bien que des antislashs, qui eux demanderaient
# d'échapper `sed`) ; côté Unix, où ce préfixe n'existe pas, inchangé.
if [[ "${OS:-}" == "Windows_NT" ]]; then
    PREFIX_NATIVE="$(printf '%s' "$PREFIX" | sed -E 's#^/([A-Za-z])/#\1:/#')"
    OUT_NATIVE="$(printf '%s' "$OUT" | sed -E 's#^/([A-Za-z])/#\1:/#')"
else
    PREFIX_NATIVE="$PREFIX"
    OUT_NATIVE="$OUT"
fi

# Substitution du seul champ dépendant de la machine, puis retrait des lignes
# de commentaire et des lignes vides : pyo3-build-config ne lit que des paires
# clé=valeur. `s|...|` : le préfixe POSIX ne contient pas de « | », mais le
# préfixe natif Windows contient des « \ » — sans effet sur `sed -e "s|...|"`,
# qui ne les traite pas spécialement côté remplacement.
sed -e "s|@PYTHON_PREFIX@|$PREFIX_NATIVE|g" -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' \
    "$TEMPLATE" > "$OUT"

echo "[config] PYO3_CONFIG_FILE=$OUT_NATIVE (prefix=$PREFIX_NATIVE)" >&2
echo "$OUT_NATIVE"

#!/usr/bin/env bash
# Vérifie que tous les crates du workspace portent une même version X.Y.Z
# (#349, #350). Ne lit ni tag ni CHANGELOG.
#
# Usage : scripts/versions-alignees.sh [X.Y.Z]
#   sans argument : les crates doivent être égaux entre eux ;
#   avec argument : ils doivent tous être en X.Y.Z.
#
# Refuse (code 1) si un crate dérive (liste de chaque crate et sa version), si
# aucun crate n'est trouvé, ou si un crate n'a pas de ligne `version = "…"`
# (par ex. `version.workspace = true`, non lu ici). Les dépendances internes
# ne sont pas couvertes (#353).
# Appelé par le travail `fmt` (GitHub et GitLab) et par scripts/release-notes.sh.
# STOKESFORGE_REPO_ROOT (essais) remplace la racine du dépôt.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
WANTED="${1:-}"

names=()
versions=()
bad=0
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    name="$(basename "$(dirname "$manifest")")"
    found="$(sed -n 's/^version = "\(.*\)"/\1/p' "$manifest" | head -1)"
    if [ -z "$found" ]; then
        echo "[versions] ERREUR : $name n'a pas de ligne « version = \"X.Y.Z\" »" >&2
        bad=1
        continue
    fi
    names+=("$name")
    versions+=("$found")
done

if [ "${#names[@]}" = 0 ] && [ "$bad" = 0 ]; then
    echo "[versions] ERREUR : aucun crate trouvé dans $REPO_ROOT/crates" >&2
    exit 1
fi

reference="${WANTED:-${versions[0]:-}}"
for i in "${!names[@]}"; do
    if [ "${versions[$i]}" != "$reference" ]; then
        echo "[versions] ERREUR : ${names[$i]} est en ${versions[$i]}, attendu $reference" >&2
        bad=1
    fi
done
[ "$bad" = 0 ] || exit 1
echo "[versions] ${#names[@]} crates alignés en $reference"

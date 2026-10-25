#!/usr/bin/env bash
# Vérifie qu'un tag de version est cohérent avec le dépôt, puis imprime les
# notes de cette version (section de CHANGELOG.md) sur la sortie standard (#333).
#
# Usage : scripts/release-notes.sh vX.Y.Z
#
# Refuse (code 1) si :
#   - le tag n'a pas la forme vX.Y.Z (sans suffixe) ;
#   - un crate du workspace n'est pas à cette version (le tag décrit le code,
#     pas l'inverse) ;
#   - CHANGELOG.md n'a pas de section `## [X.Y.Z]` non vide.
# Appelé par les deux travaux de publication (GitLab `publier`, GitHub
# `release-linux`) : un tag mal posé échoue avant tout téléversement.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${1:-}"

if [[ ! "$TAG" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
    echo "[release] ERREUR : tag « $TAG » invalide, attendu vX.Y.Z" >&2
    exit 1
fi
VERSION="${BASH_REMATCH[1]}"

bad=0
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
    found="$(sed -n 's/^version = "\(.*\)"/\1/p' "$manifest" | head -1)"
    if [ "$found" != "$VERSION" ]; then
        echo "[release] ERREUR : $(basename "$(dirname "$manifest")") est en $found, le tag demande $VERSION" >&2
        bad=1
    fi
done
[ "$bad" = 0 ] || exit 1

notes="$(awk -v header="## [$VERSION]" '
    index($0, header) == 1 { on = 1; next }
    on && /^## / { exit }
    on { print }
' "$REPO_ROOT/CHANGELOG.md")"
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
    echo "[release] ERREUR : CHANGELOG.md n'a pas de section « ## [$VERSION] » non vide" >&2
    exit 1
fi
printf '%s\n' "$notes"

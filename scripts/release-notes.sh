#!/usr/bin/env bash
# Vérifie qu'un tag de version est cohérent avec le dépôt, puis imprime les
# notes de cette version (section de CHANGELOG.md) sur la sortie standard (#333).
#
# Usage : scripts/release-notes.sh vX.Y.Z      (ou vX.Y.Z-rcN, voir plus bas)
#
# Refuse (code 1) si :
#   - le tag n'a pas la forme vX.Y.Z ou vX.Y.Z-rcN ;
#   - un crate du workspace n'est pas à la version X.Y.Z (le tag décrit le
#     code, pas l'inverse) ;
#   - pour vX.Y.Z seulement, CHANGELOG.md n'a pas de section `## [X.Y.Z]` non
#     vide.
# Un tag -rcN est une pré-version : seules les versions des crates sont
# vérifiées, aucune note n'est imprimée, et rien ne crée de release (la règle
# de `publier`, .gitlab-ci.yml, n'accepte que vX.Y.Z). Il sert à éprouver la
# construction et le téléversement de l'archive Linux avant le vrai tag.
# Appelé par les deux travaux de publication (GitLab `publier`, GitHub
# `release-linux`) : un tag mal posé échoue avant tout téléversement.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${1:-}"

if [[ ! "$TAG" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-rc[0-9]+)?$ ]]; then
    echo "[release] ERREUR : tag « $TAG » invalide, attendu vX.Y.Z ou vX.Y.Z-rcN" >&2
    exit 1
fi
VERSION="${BASH_REMATCH[1]}"
PRERELEASE="${BASH_REMATCH[2]}"

bad=0
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
    found="$(sed -n 's/^version = "\(.*\)"/\1/p' "$manifest" | head -1)"
    if [ "$found" != "$VERSION" ]; then
        echo "[release] ERREUR : $(basename "$(dirname "$manifest")") est en $found, le tag demande $VERSION" >&2
        bad=1
    fi
done
[ "$bad" = 0 ] || exit 1
[ -z "$PRERELEASE" ] || exit 0

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

#!/usr/bin/env bash
# Vérifie qu'un tag de version est cohérent avec le dépôt, puis imprime les
# notes de cette version (section de CHANGELOG.md) sur la sortie standard (#333).
#
# Usage : scripts/release-notes.sh vX.Y.Z      (ou vX.Y.Z-rcN, voir plus bas)
#
# Refuse (code 1) si :
#   - le tag n'a pas la forme vX.Y.Z ou vX.Y.Z-rcN ;
#   - un crate du workspace n'est pas à la version X.Y.Z (le tag décrit le
#     code, pas l'inverse) : contrôle délégué à scripts/versions-alignees.sh,
#     qui tourne aussi à chaque push (travail `fmt`) ;
#   - pour vX.Y.Z seulement, CHANGELOG.md n'a pas de section `## [X.Y.Z]` non
#     vide, ou la section porte encore « à dater » dans son titre (toléré sur
#     un -rcN, la date n'étant posée qu'au vrai tag).
# Un tag -rcN est une pré-version : seules les versions des crates sont
# vérifiées, aucune note n'est imprimée. Il sert à éprouver la construction et
# le téléversement des paquets : `publier` (scripts/release-publier.sh, GitHub)
# crée alors la release GitLab puis la supprime aussitôt (essai, le tag reste).
# Appelé par `release-linux` (GitHub, .github/workflows/ci.yml) et par
# `publier` : un tag mal posé échoue avant tout téléversement.
# STOKESFORGE_REPO_ROOT (essais, scripts/release-notes-test.sh) remplace la
# racine du dépôt.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
TAG="${1:-}"

if [[ ! "$TAG" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-rc[0-9]+)?$ ]]; then
    echo "[release] ERREUR : tag « $TAG » invalide, attendu vX.Y.Z ou vX.Y.Z-rcN" >&2
    exit 1
fi
VERSION="${BASH_REMATCH[1]}"
PRERELEASE="${BASH_REMATCH[2]}"

"$(dirname "${BASH_SOURCE[0]}")/versions-alignees.sh" "$VERSION" >/dev/null
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
if grep -q "^## \[$VERSION\].*à dater" "$REPO_ROOT/CHANGELOG.md"; then
    echo "[release] ERREUR : la section « ## [$VERSION] » de CHANGELOG.md porte encore « à dater » : la dater avant un tag sans -rc" >&2
    exit 1
fi
printf '%s\n' "$notes"

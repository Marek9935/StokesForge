#!/usr/bin/env bash
# Refuse un tag posé sur une branche de travail (#354) : le commit tagué doit
# être ancêtre de origin/main ou d'une branche origin/release/* (maintenance
# d'une ancienne majeure). Une branche vN n'est pas acceptée.
#
# Usage : scripts/release-tag-sur-main.sh <tag> <commit>
#   (en CI : "$GITHUB_REF_NAME" "$GITHUB_SHA", après un checkout fetch-depth: 0)
#
# Refuse (code 1) avec une annotation GitHub `::error title=tag hors main::…`.
# Appelé par `release-linux` et `publier` (.github/workflows/ci.yml).
# STOKESFORGE_REPO_ROOT (essais, scripts/release-tag-test.sh) remplace la
# racine du dépôt.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
TAG="${1:-}"
COMMIT="${2:-}"
if [ -z "$TAG" ] || [ -z "$COMMIT" ]; then
    echo "usage : release-tag-sur-main.sh <tag> <commit>" >&2
    exit 1
fi

ok=0
for ref in origin/main $(git -C "$REPO_ROOT" for-each-ref --format='%(refname:short)' 'refs/remotes/origin/release/*'); do
    if git -C "$REPO_ROOT" merge-base --is-ancestor "$COMMIT" "$ref" 2>/dev/null; then ok=1; fi
done
if [ "$ok" != 1 ]; then
    echo "::error title=tag hors main::le commit $COMMIT du tag $TAG n'est ancêtre ni de origin/main ni d'une branche release/* : tag posé sur une branche de travail"
    exit 1
fi

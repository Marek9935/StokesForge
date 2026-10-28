#!/usr/bin/env bash
# Refuses a tag placed on a working branch (#354): the tagged commit must be
# an ancestor of origin/main or of an origin/release/* branch (maintenance of
# an old major version). A vN branch is not accepted.
#
# Usage: scripts/release-tag-on-main.sh <tag> <commit>
#   (in CI: "$GITHUB_REF_NAME" "$GITHUB_SHA", after a checkout with fetch-depth: 0)
#
# Refuses (code 1) with a GitHub annotation `::error title=tag not on main::…`.
# Called by `release-linux` and `publier` (.github/workflows/ci.yml).
# STOKESFORGE_REPO_ROOT (tests, scripts/release-tag-test.sh) replaces the
# repository root.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
TAG="${1:-}"
COMMIT="${2:-}"
if [ -z "$TAG" ] || [ -z "$COMMIT" ]; then
    echo "usage: release-tag-on-main.sh <tag> <commit>" >&2
    exit 1
fi

ok=0
for ref in origin/main $(git -C "$REPO_ROOT" for-each-ref --format='%(refname:short)' 'refs/remotes/origin/release/*'); do
    if git -C "$REPO_ROOT" merge-base --is-ancestor "$COMMIT" "$ref" 2>/dev/null; then ok=1; fi
done
if [ "$ok" != 1 ]; then
    echo "::error title=tag not on main::commit $COMMIT of tag $TAG is an ancestor of neither origin/main nor a release/* branch: tag placed on a working branch"
    exit 1
fi

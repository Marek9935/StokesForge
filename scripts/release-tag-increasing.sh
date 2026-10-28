#!/usr/bin/env bash
# Refuses a version tag that does not exceed the existing tags (#355).
#
# Usage: scripts/release-tag-increasing.sh <tag> [commit]
#   commit: tagged commit (default HEAD, which is the tagged commit in CI).
#
# Rules (the candidate tag itself is excluded from the existing tags, by name):
#   - a final release vX.Y.Z must exceed every existing final release; the
#     existing -rcN tags do not count (an abandoned rc does not block the
#     next one);
#   - a vX.Y.Z-rcN must exceed the existing -rcM tags of the same version
#     X.Y.Z, and must not target a version whose final release is already tagged.
# Scope: on origin/main, all v* tags; if the commit is only on an
# origin/release/N.x branch, only the tags of major version N (maintenance of
# an old major version). Comparisons are numeric, component by component:
# `sort -V` and `git tag --sort=v:refname` put X.Y.Z BEFORE X.Y.Z-rcN, which
# is wrong here.
# Limit: an existing tag moved to another target cannot be detected in CI;
# the protection of v* tags in GitLab covers it (docs/tests.md).
# Called by `release-linux` and `publier` (.github/workflows/ci.yml), right
# after scripts/release-tag-on-main.sh.
# STOKESFORGE_REPO_ROOT (tests, scripts/release-tag-test.sh) replaces the
# repository root.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
TAG="${1:-}"
COMMIT="${2:-HEAD}"
RE='^v([0-9]+)\.([0-9]+)\.([0-9]+)(-rc([0-9]+))?$'

if [[ ! "$TAG" =~ $RE ]]; then
    echo "[release] ERROR: invalid tag \"$TAG\", expected vX.Y.Z ou vX.Y.Z-rcN" >&2
    exit 1
fi
cx=$((10#${BASH_REMATCH[1]})); cy=$((10#${BASH_REMATCH[2]})); cz=$((10#${BASH_REMATCH[3]}))
c_rc="${BASH_REMATCH[5]}"

# Scope: major version forced if the commit is not on origin/main.
scope_major=""
if ! git -C "$REPO_ROOT" merge-base --is-ancestor "$COMMIT" origin/main 2>/dev/null; then
    for ref in $(git -C "$REPO_ROOT" for-each-ref --format='%(refname:short)' 'refs/remotes/origin/release/*'); do
        if git -C "$REPO_ROOT" merge-base --is-ancestor "$COMMIT" "$ref" 2>/dev/null; then
            if [[ "$ref" =~ ^origin/release/([0-9]+)\.x$ ]]; then
                scope_major=$((10#${BASH_REMATCH[1]}))
                break
            fi
        fi
    done
    if [ -z "$scope_major" ]; then
        echo "[release] ERROR: the commit is neither on origin/main nor on an origin/release/N.x branch" >&2
        exit 1
    fi
fi

bad=0
while IFS= read -r existing; do
    [ -n "$existing" ] && [ "$existing" != "$TAG" ] || continue
    [[ "$existing" =~ $RE ]] || continue
    ex=$((10#${BASH_REMATCH[1]})); ey=$((10#${BASH_REMATCH[2]})); ez=$((10#${BASH_REMATCH[3]}))
    e_rc="${BASH_REMATCH[5]}"
    [ -z "$scope_major" ] || [ "$ex" = "$scope_major" ] || continue

    # cmp: sign of (candidate X.Y.Z) - (existing X.Y.Z)
    cmp=0
    for pair in "$cx $ex" "$cy $ey" "$cz $ez"; do
        set -- $pair
        if [ "$1" -gt "$2" ]; then cmp=1; break; fi
        if [ "$1" -lt "$2" ]; then cmp=-1; break; fi
    done

    if [ -z "$c_rc" ]; then
        # Final release: above every existing final release.
        if [ -z "$e_rc" ] && [ "$cmp" != 1 ]; then
            echo "[release] ERROR: $TAG does not exceed the existing final release $existing" >&2
            bad=1
        fi
    elif [ "$cmp" = 0 ]; then
        if [ -z "$e_rc" ]; then
            echo "[release] ERROR: $TAG targets a version whose final release $existing is already tagged" >&2
            bad=1
        elif [ "$((10#$c_rc))" -le "$((10#$e_rc))" ]; then
            echo "[release] ERROR: $TAG does not exceed the existing rc $existing" >&2
            bad=1
        fi
    fi
done < <(git -C "$REPO_ROOT" tag --list 'v*')

[ "$bad" = 0 ] || exit 1

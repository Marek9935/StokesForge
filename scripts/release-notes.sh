#!/usr/bin/env bash
# Checks that a version tag is consistent with the repository, then prints
# the notes of that version (section of CHANGELOG.md) on standard output (#333).
#
# Usage: scripts/release-notes.sh vX.Y.Z      (or vX.Y.Z-rcN, see below)
#
# Refuses (code 1) if:
#   - the tag does not have the form vX.Y.Z or vX.Y.Z-rcN;
#   - a workspace crate is not at version X.Y.Z (the tag describes the code,
#     not the other way round): the check is delegated to
#     scripts/versions-alignees.sh, which also runs on every push (`fmt` job);
#   - for vX.Y.Z only, CHANGELOG.md has no non-empty `## [X.Y.Z]` section, or
#     the section title still contains "undated" (tolerated on an -rcN, as the
#     date is set only at the real tag).
# An -rcN tag is a pre-release: only the crate versions are checked, and no
# notes are printed. It tests the build and the upload of the packages:
# `publier` (scripts/release-publier.sh, GitHub) then creates the GitLab
# release and deletes it at once (trial run, the tag stays).
# Called by `release-linux` (GitHub, .github/workflows/ci.yml) and by
# `publier`: a badly placed tag fails before any upload.
# STOKESFORGE_REPO_ROOT (tests, scripts/release-notes-test.sh) replaces the
# repository root.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
TAG="${1:-}"

if [[ ! "$TAG" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-rc[0-9]+)?$ ]]; then
    echo "[release] ERROR: invalid tag \"$TAG\", expected vX.Y.Z or vX.Y.Z-rcN" >&2
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
    echo "[release] ERROR: CHANGELOG.md has no non-empty section \"## [$VERSION]\"" >&2
    exit 1
fi
if grep -q "^## \[$VERSION\].*undated" "$REPO_ROOT/CHANGELOG.md"; then
    echo "[release] ERROR: section \"## [$VERSION]\" of CHANGELOG.md is still \"undated\": date it before a tag without -rc" >&2
    exit 1
fi
printf '%s\n' "$notes"

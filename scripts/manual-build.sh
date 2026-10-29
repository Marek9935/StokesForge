#!/usr/bin/env bash
# Builds the user manual (#345, #362) with mdBook into manual/book/. With a tag,
# the title carries the version and the built site is packed into
# dist/StokesForge-X.Y.Z-manual.tar.gz (top folder StokesForge-X.Y.Z-manual/),
# which `scripts/release-upload.sh manual` sends to the GitLab package registry.
# The mdBook version is pinned in scripts/manual-tools.env.
#
# Usage: scripts/manual-build.sh [vX.Y.Z (or vX.Y.Z-rcN)]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${1:-}"

if ! command -v mdbook > /dev/null; then
    source "$REPO_ROOT/scripts/manual-tools.env"
    echo "[manual] ERROR: mdbook not found in PATH (version $MDBOOK_VERSION, see scripts/manual-tools.env)" >&2
    exit 1
fi

if [ -z "$TAG" ]; then
    mdbook build "$REPO_ROOT/manual"
    echo "[manual] built: $REPO_ROOT/manual/book"
    exit 0
fi

if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[0-9]+)?$ ]]; then
    echo "[manual] ERROR: \"$TAG\" is not a tag (expected vX.Y.Z or vX.Y.Z-rcN)" >&2
    exit 1
fi
VERSION="${TAG#v}"
MDBOOK_BOOK__TITLE="StokesForge ${VERSION} manual" mdbook build "$REPO_ROOT/manual"

NOM="StokesForge-${VERSION}-manual"
ARCHIVE="$REPO_ROOT/dist/${NOM}.tar.gz"
mkdir -p "$REPO_ROOT/dist"
tar -czf "$ARCHIVE" -C "$REPO_ROOT/manual" --transform "s,^book,${NOM}," book
echo "[manual] archive: $ARCHIVE"

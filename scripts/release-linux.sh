#!/usr/bin/env bash
# Packs the Linux distribution, already assembled and tested by
# `scripts/app-dist.sh`, into a publishable archive (#333):
#   dist/StokesForge-<version>-linux-x86_64.tar.gz
# containing a folder `StokesForge-<version>/` (executables, lib/python,
# license notices, `install.sh` from scripts/install-linux.sh, #387).
#
# Usage: scripts/release-linux.sh vX.Y.Z   (the tag, or STOKESFORGE_VERSION=X.Y.Z)
# Rebuilds nothing: run scripts/python-vendor.sh first, then
# scripts/app-dist.sh, which are the guards of the distribution.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$REPO_ROOT/dist/stokesforge-app"
VERSION="${1:+${1#v}}"
VERSION="${VERSION:-${STOKESFORGE_VERSION:-}}"

if [ -z "$VERSION" ]; then
    echo "[release] ERROR: version missing (argument vX.Y.Z or STOKESFORGE_VERSION)" >&2
    exit 1
fi
for needed in stokesforge-app stokesforge-selftest lib/python; do
    if [ ! -e "$DIST/$needed" ]; then
        echo "[release] ERROR: $DIST/$needed missing, run scripts/app-dist.sh" >&2
        exit 1
    fi
done
if ! ls "$DIST" | grep -qi licen; then
    echo "[release] ERROR: no license notice in $DIST" >&2
    exit 1
fi

# The installer sits in the archive, next to the program it installs (#387).
install -m 755 "$REPO_ROOT/scripts/install-linux.sh" "$DIST/install.sh"

ARCHIVE="$REPO_ROOT/dist/StokesForge-${VERSION}-linux-x86_64.tar.gz"
tar -C "$DIST/.." --transform "s|^stokesforge-app|StokesForge-${VERSION}|" \
    -czf "$ARCHIVE" stokesforge-app
if ! tar -tzf "$ARCHIVE" | grep -qx "StokesForge-${VERSION}/install.sh"; then
    echo "[release] ERROR: install.sh missing from $ARCHIVE" >&2
    exit 1
fi
echo "[release] $ARCHIVE"

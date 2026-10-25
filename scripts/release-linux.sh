#!/usr/bin/env bash
# Empaquette la distribution Linux déjà assemblée et éprouvée par
# `scripts/app-dist.sh` en archive publiable (#333) :
#   dist/StokesForge-<version>-linux-x86_64.tar.gz
# contenant un dossier `StokesForge-<version>/` (exécutables, lib/python,
# notices de licences).
#
# Usage : scripts/release-linux.sh vX.Y.Z   (le tag, ou STOKESFORGE_VERSION=X.Y.Z)
# Ne reconstruit rien : lancer d'abord scripts/python-vendor.sh puis
# scripts/app-dist.sh, qui sont les verrous de la distribution.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$REPO_ROOT/dist/stokesforge-app"
VERSION="${1:+${1#v}}"
VERSION="${VERSION:-${STOKESFORGE_VERSION:-}}"

if [ -z "$VERSION" ]; then
    echo "[release] ERREUR : version absente (argument vX.Y.Z ou STOKESFORGE_VERSION)" >&2
    exit 1
fi
for needed in stokesforge-app stokesforge-selftest lib/python; do
    if [ ! -e "$DIST/$needed" ]; then
        echo "[release] ERREUR : $DIST/$needed manquant, lancer scripts/app-dist.sh" >&2
        exit 1
    fi
done
if ! ls "$DIST" | grep -qi licen; then
    echo "[release] ERREUR : aucune notice de licences dans $DIST" >&2
    exit 1
fi

ARCHIVE="$REPO_ROOT/dist/StokesForge-${VERSION}-linux-x86_64.tar.gz"
tar -C "$DIST/.." --transform "s|^stokesforge-app|StokesForge-${VERSION}|" \
    -czf "$ARCHIVE" stokesforge-app
echo "[release] $ARCHIVE"

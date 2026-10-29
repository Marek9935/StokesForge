#!/usr/bin/env bash
# Uploads the package of a tag (Linux archive, Windows installer or user
# manual) to the GitLab generic package registry (#333), from where
# `scripts/release-publish.sh` links it to the GitLab release. Runs in the
# GitHub `release-linux`, `release-manual` and `dist-windows` jobs, after their
# assembly.
#
# Usage: scripts/release-upload.sh linux|windows|manual vX.Y.Z (or vX.Y.Z-rcN)
# Environment: GITLAB_RELEASE_TOKEN (GitHub secret: GitLab project token,
# Developer role, `api` scope). STOKESFORGE_RELEASE_DRYRUN=1 prints the URL
# without sending anything (local trial, token not required).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATEFORME="${1:?usage: release-upload.sh linux|windows|manual vX.Y.Z}"
TAG="${2:?usage: release-upload.sh linux|windows|manual vX.Y.Z}"
VERSION="${TAG#v}"
PROJECT_ID=86104040
API="${CI_API_V4_URL:-https://gitlab.com/api/v4}"

case "$PLATEFORME" in
    linux) ARCHIVE="$REPO_ROOT/dist/StokesForge-${VERSION}-linux-x86_64.tar.gz"; PRODUCTEUR=release-linux.sh ;;
    windows) ARCHIVE="$REPO_ROOT/dist/StokesForge-${VERSION}-windows-setup.exe"; PRODUCTEUR=app-dist-windows.sh ;;
    manual) ARCHIVE="$REPO_ROOT/dist/StokesForge-${VERSION}-manual.tar.gz"; PRODUCTEUR=manual-build.sh ;;
    *) echo "[release] ERROR: unknown platform \"$PLATEFORME\" (linux, windows or manual)" >&2; exit 1 ;;
esac
URL="${API}/projects/${PROJECT_ID}/packages/generic/stokesforge/${VERSION}/$(basename "$ARCHIVE")"

if [ ! -f "$ARCHIVE" ]; then
    echo "[release] ERROR: $ARCHIVE missing, run scripts/$PRODUCTEUR" >&2
    exit 1
fi
if [ "${STOKESFORGE_RELEASE_DRYRUN:-}" = 1 ]; then
    echo "PUT $URL"
    exit 0
fi
if [ -z "${GITLAB_RELEASE_TOKEN:-}" ]; then
    echo "[release] ERROR: the GitHub secret GITLAB_RELEASE_TOKEN is missing" >&2
    echo "[release] (GitLab project token, Developer role, api scope: see docs/v1-criteria.md)" >&2
    exit 1
fi
# The header goes through the configuration read on standard input (`-K -`):
# the token does not appear in the process list. No process substitution
# (`@<(...)`): under Git Bash, curl does not read it (code 26, rc4).
printf 'header = "PRIVATE-TOKEN: %s"\n' "$GITLAB_RELEASE_TOKEN" | curl --fail-with-body --silent --show-error \
    -K - --upload-file "$ARCHIVE" "$URL"
echo
echo "[release] $PLATEFORME uploaded : $URL"

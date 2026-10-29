#!/usr/bin/env bash
# Installs the nfpm packager (#390) at the version and checksum pinned in
# scripts/linux-packages.env, into the folder given (default: $RUNNER_TEMP or
# /tmp), and adds that folder to GITHUB_PATH when run by GitHub Actions.
# The download is verified with sha256sum before anything is unpacked.
#
# Usage: scripts/nfpm-install.sh [folder]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=linux-packages.env
source "$REPO_ROOT/scripts/linux-packages.env"
dest="${1:-${RUNNER_TEMP:-/tmp}}"
mkdir -p "$dest"
archive="$dest/nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz"
curl -fsSL -o "$archive" "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz"
echo "${NFPM_SHA256}  ${archive}" | sha256sum -c -
tar -xzf "$archive" -C "$dest" nfpm
rm -f "$archive"
if [ -n "${GITHUB_PATH:-}" ]; then
    echo "$dest" >> "$GITHUB_PATH"
fi
"$dest/nfpm" --version | grep -q "GitVersion: *${NFPM_VERSION}$"
echo "[nfpm] ${NFPM_VERSION} installed in $dest"

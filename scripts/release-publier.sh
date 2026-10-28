#!/usr/bin/env bash
# Creates the GitLab release of a tag (#333): notes taken from CHANGELOG.md,
# links to the Windows installer (`dist-windows` job) and to the Linux archive
# (`release-linux`), which these GitHub jobs (.github/workflows/ci.yml) upload
# to the GitLab package registry. Runs in the GitHub `publier` job, which
# `needs` both: the packages are already there.
#
# Usage: scripts/release-publier.sh vX.Y.Z | vX.Y.Z-rcN
# Environment: GITLAB_RELEASE_TOKEN (GitHub secret: GitLab project token,
# Developer role, `api` scope). STOKESFORGE_RELEASE_DRYRUN=1 prints the request
# without sending it (local trial, token not required).
#
# An -rcN tag is a TRIAL: the release is created and deleted at once (the
# tag stays). This tests the create call before the real tag, without leaving
# a pre-release release behind.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${1:?usage: release-publier.sh vX.Y.Z | vX.Y.Z-rcN}"
if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[0-9]+)?$ ]]; then
    echo "[release] ERROR: no release for \"$TAG\" (expected vX.Y.Z or vX.Y.Z-rcN)" >&2
    exit 1
fi
VERSION="${TAG#v}"
ESSAI=0
[[ "$TAG" == *-rc* ]] && ESSAI=1

PROJECT_ID=86104040
API="${CI_API_V4_URL:-https://gitlab.com/api/v4}"
PAQUETS="${API}/projects/${PROJECT_ID}/packages/generic/stokesforge/${VERSION}"

NOTES="$("$REPO_ROOT/scripts/release-notes.sh" "$TAG")"
NOM="StokesForge ${VERSION}"
if [ "$ESSAI" = 1 ]; then
    NOTES="Publication trial (${TAG}): this release is deleted at once."
    NOM="${NOM} (trial)"
fi
SETUP="StokesForge-${VERSION}-windows-setup.exe"
LINUX="StokesForge-${VERSION}-linux-x86_64.tar.gz"

args=(
    --data-urlencode "tag_name=${TAG}"
    --data-urlencode "name=${NOM}"
    --data-urlencode "description=${NOTES}"
    --data-urlencode "assets[links][][name]=Windows installer (${SETUP})"
    --data-urlencode "assets[links][][url]=${PAQUETS}/${SETUP}"
    --data-urlencode "assets[links][][link_type]=package"
    --data-urlencode "assets[links][][name]=Linux x86_64 archive (${LINUX})"
    --data-urlencode "assets[links][][url]=${PAQUETS}/${LINUX}"
    --data-urlencode "assets[links][][link_type]=package"
)

if [ "${STOKESFORGE_RELEASE_DRYRUN:-}" = 1 ]; then
    printf '%s\n' "${args[@]}"
    [ "$ESSAI" = 1 ] && echo "(trial: DELETE ${API}/projects/${PROJECT_ID}/releases/${TAG} next)"
    exit 0
fi
if [ -z "${GITLAB_RELEASE_TOKEN:-}" ]; then
    echo "[release] ERROR: the GitHub secret GITLAB_RELEASE_TOKEN is missing" >&2
    exit 1
fi

# Token through the configuration read on standard input (`-K -`): it does
# not appear in the process list.
gitlab() {
    printf 'header = "PRIVATE-TOKEN: %s"\n' "$GITLAB_RELEASE_TOKEN" | curl --fail-with-body --silent --show-error -K - "$@"
}

# Both package jobs are finished (`needs`); a short wait covers a possible
# indexing delay, without publishing a dead link.
for fichier in "$LINUX" "$SETUP"; do
    essais=10
    until gitlab --output /dev/null --range 0-0 "${PAQUETS}/${fichier}" 2> /dev/null; do
        essais=$((essais - 1))
        if [ "$essais" -le 0 ]; then
            echo "[release] ERROR: ${fichier} missing from the GitLab package registry (${PAQUETS})" >&2
            exit 1
        fi
        sleep 6
    done
done

gitlab -X POST "${args[@]}" "${API}/projects/${PROJECT_ID}/releases"
echo
echo "[release] release ${TAG} created"
if [ "$ESSAI" = 1 ]; then
    gitlab -X DELETE --output /dev/null "${API}/projects/${PROJECT_ID}/releases/${TAG}"
    echo "[release] trial: release ${TAG} deleted (the tag stays)"
fi

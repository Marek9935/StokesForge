#!/usr/bin/env bash
# Creates the GitLab release of a tag (#333): notes taken from CHANGELOG.md,
# links to the Windows installer (`dist-windows` job), to the Linux archive
# (`release-linux`) and to the user manual (`release-manual`), which these
# GitHub jobs (.github/workflows/ci.yml) upload to the GitLab package registry.
# Runs in the GitHub `publier` job, which `needs` all three: the packages are
# already there.
#
# Usage: scripts/release-publish.sh vX.Y.Z | vX.Y.Z-rcN
# Environment: GITLAB_RELEASE_TOKEN (GitHub secret: GitLab project token,
# Developer role, `api` scope). STOKESFORGE_RELEASE_DRYRUN=1 prints the request
# without sending it (local trial, token not required).
#
# For a FINAL tag, once the release exists, it also starts a GitLab pipeline on
# the tag with STOKESFORGE_PUBLISH_MANUAL=1: the `pages` job of .gitlab-ci.yml
# then publishes the manual of the release on GitLab Pages (#362). The tag
# pipeline that GitLab starts at push time must not do it: the site goes live
# only once the release and its packages exist. The token's role must be
# allowed to run pipelines on the protected tags `v*` (else Maintainer).
#
# An -rcN tag is a TRIAL: the release is created and deleted at once (the
# tag stays). This tests the create call before the real tag, without leaving
# a pre-release release behind.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${1:?usage: release-publish.sh vX.Y.Z | vX.Y.Z-rcN}"
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
MANUEL="StokesForge-${VERSION}-manual.tar.gz"
# Pipeline that publishes the manual (final tag only, see above). JSON: the
# documented form of `variables` for this endpoint.
PIPELINE="{\"ref\":\"${TAG}\",\"variables\":[{\"key\":\"STOKESFORGE_PUBLISH_MANUAL\",\"value\":\"1\"}]}"

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
    --data-urlencode "assets[links][][name]=User manual (${MANUEL})"
    --data-urlencode "assets[links][][url]=${PAQUETS}/${MANUEL}"
    --data-urlencode "assets[links][][link_type]=package"
)

if [ "${STOKESFORGE_RELEASE_DRYRUN:-}" = 1 ]; then
    printf '%s\n' "${args[@]}"
    [ "$ESSAI" = 0 ] && echo "(final: POST ${API}/projects/${PROJECT_ID}/pipeline ${PIPELINE} next)"
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

# The three package jobs are finished (`needs`); a short wait covers a possible
# indexing delay, without publishing a dead link.
for fichier in "$LINUX" "$SETUP" "$MANUEL"; do
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
if [ "$ESSAI" = 0 ]; then
    gitlab -X POST --output /dev/null --header "Content-Type: application/json" \
        --data "$PIPELINE" "${API}/projects/${PROJECT_ID}/pipeline"
    echo "[release] pipeline started on ${TAG}: the \`pages\` job publishes the manual"
fi
if [ "$ESSAI" = 1 ]; then
    gitlab -X DELETE --output /dev/null "${API}/projects/${PROJECT_ID}/releases/${TAG}"
    echo "[release] trial: release ${TAG} deleted (the tag stays)"
fi

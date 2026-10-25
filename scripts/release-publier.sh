#!/usr/bin/env bash
# Crée la release GitLab d'un tag vX.Y.Z (#333) : notes tirées de CHANGELOG.md,
# lien vers l'installeur Windows produit par `dist-windows` et, si
# STOKESFORGE_LINUX_RELEASE_URL est posée, vers l'archive Linux publiée par
# GitHub (travail `release-linux` de .github/workflows/ci.yml).
#
# Environnement GitLab CI : CI_COMMIT_TAG, CI_API_V4_URL, CI_PROJECT_ID,
# CI_PROJECT_URL, CI_JOB_TOKEN. STOKESFORGE_RELEASE_DRYRUN=1 imprime la requête
# sans l'envoyer (essai local).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${CI_COMMIT_TAG:?CI_COMMIT_TAG absent : ce script ne tourne que sur un tag}"
VERSION="${TAG#v}"

NOTES="$("$REPO_ROOT/scripts/release-notes.sh" "$TAG")"
SETUP="StokesForge-${VERSION}-windows-setup.exe"
SETUP_URL="${CI_PROJECT_URL}/-/jobs/artifacts/${TAG}/raw/dist/${SETUP}?job=dist-windows"

args=(
    --data-urlencode "tag_name=${TAG}"
    --data-urlencode "name=StokesForge ${VERSION}"
    --data-urlencode "description=${NOTES}"
    --data-urlencode "assets[links][][name]=Installeur Windows (${SETUP})"
    --data-urlencode "assets[links][][url]=${SETUP_URL}"
    --data-urlencode "assets[links][][link_type]=package"
)
if [ -n "${STOKESFORGE_LINUX_RELEASE_URL:-}" ]; then
    args+=(
        --data-urlencode "assets[links][][name]=Archive Linux x86_64 (GitHub)"
        --data-urlencode "assets[links][][url]=${STOKESFORGE_LINUX_RELEASE_URL%/}/${TAG}"
        --data-urlencode "assets[links][][link_type]=package"
    )
fi

if [ "${STOKESFORGE_RELEASE_DRYRUN:-}" = 1 ]; then
    printf '%s\n' "${args[@]}"
    exit 0
fi
curl --fail-with-body --silent --show-error -X POST \
    --header "JOB-TOKEN: ${CI_JOB_TOKEN:?CI_JOB_TOKEN absent}" \
    "${args[@]}" "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/releases"
echo
echo "[release] release ${TAG} créée"

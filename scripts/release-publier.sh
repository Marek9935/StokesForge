#!/usr/bin/env bash
# Crée la release GitLab d'un tag vX.Y.Z (#333) : notes tirées de CHANGELOG.md,
# lien vers l'installeur Windows produit par `dist-windows` et vers l'archive
# Linux que le travail `release-linux` de GitHub (.github/workflows/ci.yml)
# téléverse dans le registre de paquets GitLab ; le script attend qu'elle y soit
# (STOKESFORGE_LINUX_WAIT secondes, 3600 par défaut).
#
# Environnement GitLab CI : CI_COMMIT_TAG, CI_API_V4_URL, CI_PROJECT_ID,
# CI_PROJECT_URL, CI_JOB_TOKEN. STOKESFORGE_RELEASE_DRYRUN=1 imprime la requête
# sans l'envoyer (essai local).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${CI_COMMIT_TAG:?CI_COMMIT_TAG absent : ce script ne tourne que sur un tag}"
VERSION="${TAG#v}"
if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "[release] ERREUR : pas de release pour « $TAG » (attendu vX.Y.Z, sans suffixe)" >&2
    exit 1
fi

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
LINUX="StokesForge-${VERSION}-linux-x86_64.tar.gz"
LINUX_URL="${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/generic/stokesforge/${VERSION}/${LINUX}"
args+=(
    --data-urlencode "assets[links][][name]=Archive Linux x86_64 (${LINUX})"
    --data-urlencode "assets[links][][url]=${LINUX_URL}"
    --data-urlencode "assets[links][][link_type]=package"
)

if [ "${STOKESFORGE_RELEASE_DRYRUN:-}" = 1 ]; then
    printf '%s\n' "${args[@]}"
    exit 0
fi
# L'archive Linux vient d'un autre travail (GitHub) : on ne publie pas un lien mort.
attente="${STOKESFORGE_LINUX_WAIT:-3600}"
until curl --silent --fail --output /dev/null --range 0-0 \
        --header "JOB-TOKEN: ${CI_JOB_TOKEN:?CI_JOB_TOKEN absent}" "$LINUX_URL"; do
    if [ "$attente" -le 0 ]; then
        echo "[release] ERREUR : ${LINUX} absente du registre de paquets GitLab." >&2
        echo "[release] Le travail release-linux de GitHub a-t-il échoué ? Relancer ce travail ensuite." >&2
        exit 1
    fi
    echo "[release] attente de ${LINUX} (${attente} s au plus)"
    sleep 30
    attente=$((attente - 30))
done
curl --fail-with-body --silent --show-error -X POST \
    --header "JOB-TOKEN: ${CI_JOB_TOKEN:?CI_JOB_TOKEN absent}" \
    "${args[@]}" "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/releases"
echo
echo "[release] release ${TAG} créée"

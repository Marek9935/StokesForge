#!/usr/bin/env bash
# Crée la release GitLab d'un tag (#333) : notes tirées de CHANGELOG.md, liens
# vers l'installeur Windows (travail `dist-windows`) et vers l'archive Linux
# (`release-linux`), que ces travaux de GitHub (.github/workflows/ci.yml)
# téléversent dans le registre de paquets GitLab. Tourne dans le travail
# `publier` de GitHub, qui `needs` les deux : les paquets y sont déjà.
#
# Usage : scripts/release-publier.sh vX.Y.Z | vX.Y.Z-rcN
# Environnement : GITLAB_RELEASE_TOKEN (secret GitHub : jeton de projet GitLab,
# rôle Developer, portée `api`). STOKESFORGE_RELEASE_DRYRUN=1 imprime la requête
# sans l'envoyer (essai local, jeton non exigé).
#
# Un tag -rcN est un ESSAI : la release est créée puis aussitôt supprimée (le
# tag reste), ce qui éprouve l'appel de création avant le vrai tag sans laisser
# de release de pré-version.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${1:?usage : release-publier.sh vX.Y.Z | vX.Y.Z-rcN}"
if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc[0-9]+)?$ ]]; then
    echo "[release] ERREUR : pas de release pour « $TAG » (attendu vX.Y.Z ou vX.Y.Z-rcN)" >&2
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
    NOTES="Essai de publication (${TAG}) : cette release est supprimée aussitôt."
    NOM="${NOM} (essai)"
fi
SETUP="StokesForge-${VERSION}-windows-setup.exe"
LINUX="StokesForge-${VERSION}-linux-x86_64.tar.gz"

args=(
    --data-urlencode "tag_name=${TAG}"
    --data-urlencode "name=${NOM}"
    --data-urlencode "description=${NOTES}"
    --data-urlencode "assets[links][][name]=Installeur Windows (${SETUP})"
    --data-urlencode "assets[links][][url]=${PAQUETS}/${SETUP}"
    --data-urlencode "assets[links][][link_type]=package"
    --data-urlencode "assets[links][][name]=Archive Linux x86_64 (${LINUX})"
    --data-urlencode "assets[links][][url]=${PAQUETS}/${LINUX}"
    --data-urlencode "assets[links][][link_type]=package"
)

if [ "${STOKESFORGE_RELEASE_DRYRUN:-}" = 1 ]; then
    printf '%s\n' "${args[@]}"
    [ "$ESSAI" = 1 ] && echo "(essai : DELETE ${API}/projects/${PROJECT_ID}/releases/${TAG} ensuite)"
    exit 0
fi
if [ -z "${GITLAB_RELEASE_TOKEN:-}" ]; then
    echo "[release] ERREUR : le secret GitHub GITLAB_RELEASE_TOKEN est absent" >&2
    exit 1
fi

# Jeton par la configuration lue sur l'entrée standard (`-K -`) : il n'apparaît
# pas dans la liste des processus.
gitlab() {
    printf 'header = "PRIVATE-TOKEN: %s"\n' "$GITLAB_RELEASE_TOKEN" | curl --fail-with-body --silent --show-error -K - "$@"
}

# Les deux travaux de paquets sont terminés (`needs`) ; une courte attente couvre
# un éventuel retard d'indexation, sans publier un lien mort.
for fichier in "$LINUX" "$SETUP"; do
    essais=10
    until gitlab --output /dev/null --range 0-0 "${PAQUETS}/${fichier}" 2> /dev/null; do
        essais=$((essais - 1))
        if [ "$essais" -le 0 ]; then
            echo "[release] ERREUR : ${fichier} absent du registre de paquets GitLab (${PAQUETS})" >&2
            exit 1
        fi
        sleep 6
    done
done

gitlab -X POST "${args[@]}" "${API}/projects/${PROJECT_ID}/releases"
echo
echo "[release] release ${TAG} créée"
if [ "$ESSAI" = 1 ]; then
    gitlab -X DELETE --output /dev/null "${API}/projects/${PROJECT_ID}/releases/${TAG}"
    echo "[release] essai : release ${TAG} supprimée (le tag reste)"
fi

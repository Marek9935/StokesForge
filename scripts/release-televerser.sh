#!/usr/bin/env bash
# Téléverse le paquet d'un tag (archive Linux ou installeur Windows) dans le
# registre de paquets génériques de GitLab (#333), d'où
# `scripts/release-publier.sh` le lie à la release GitLab. Tourne dans les
# travaux `release-linux` et `dist-windows` de GitHub, après leur assemblage.
#
# Usage : scripts/release-televerser.sh linux|windows vX.Y.Z (ou vX.Y.Z-rcN)
# Environnement : GITLAB_RELEASE_TOKEN (secret GitHub : jeton de projet GitLab,
# rôle Developer, portée `api`). STOKESFORGE_RELEASE_DRYRUN=1 imprime l'URL sans
# rien envoyer (essai local, jeton non exigé).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATEFORME="${1:?usage : release-televerser.sh linux|windows vX.Y.Z}"
TAG="${2:?usage : release-televerser.sh linux|windows vX.Y.Z}"
VERSION="${TAG#v}"
PROJECT_ID=86104040
API="${CI_API_V4_URL:-https://gitlab.com/api/v4}"

case "$PLATEFORME" in
    linux) ARCHIVE="$REPO_ROOT/dist/StokesForge-${VERSION}-linux-x86_64.tar.gz"; PRODUCTEUR=release-linux.sh ;;
    windows) ARCHIVE="$REPO_ROOT/dist/StokesForge-${VERSION}-windows-setup.exe"; PRODUCTEUR=app-dist-windows.sh ;;
    *) echo "[release] ERREUR : plateforme « $PLATEFORME » inconnue (linux ou windows)" >&2; exit 1 ;;
esac
URL="${API}/projects/${PROJECT_ID}/packages/generic/stokesforge/${VERSION}/$(basename "$ARCHIVE")"

if [ ! -f "$ARCHIVE" ]; then
    echo "[release] ERREUR : $ARCHIVE absent, lancer scripts/$PRODUCTEUR" >&2
    exit 1
fi
if [ "${STOKESFORGE_RELEASE_DRYRUN:-}" = 1 ]; then
    echo "PUT $URL"
    exit 0
fi
if [ -z "${GITLAB_RELEASE_TOKEN:-}" ]; then
    echo "[release] ERREUR : le secret GitHub GITLAB_RELEASE_TOKEN est absent" >&2
    echo "[release] (jeton de projet GitLab, rôle Developer, portée api : voir docs/criteres-v1.md)" >&2
    exit 1
fi
# L'en-tête passe par un fichier anonyme : le jeton n'apparaît pas dans la
# liste des processus.
curl --fail-with-body --silent --show-error \
    --header @<(printf 'PRIVATE-TOKEN: %s\n' "$GITLAB_RELEASE_TOKEN") \
    --upload-file "$ARCHIVE" "$URL"
echo
echo "[release] $PLATEFORME téléversé : $URL"

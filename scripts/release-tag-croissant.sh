#!/usr/bin/env bash
# Refuse un tag de version qui ne dépasse pas les tags existants (#355).
#
# Usage : scripts/release-tag-croissant.sh <tag> [commit]
#   commit : commit tagué (défaut HEAD, qui est le commit tagué en CI).
#
# Règles (le tag candidat lui-même est exclu des tags existants, par son nom) :
#   - une finale vX.Y.Z doit dépasser toute finale existante ; les -rcN
#     existants ne comptent pas (un rc abandonné ne bloque pas la suivante) ;
#   - un vX.Y.Z-rcN doit dépasser les -rcM existants de la même version X.Y.Z
#     et ne pas viser une version dont la finale est déjà posée.
# Portée : sur origin/main, tous les tags v* ; si le commit n'est que sur une
# branche origin/release/N.x, seulement les tags de majeure N (maintenance
# d'une ancienne majeure). Les comparaisons sont numériques composante par
# composante : `sort -V` et `git tag --sort=v:refname` placent X.Y.Z AVANT
# X.Y.Z-rcN, ce qui est faux ici.
# Limite : un tag existant reposé sur une autre cible n'est pas détectable en
# CI ; la protection des tags v* de GitLab le tient (docs/tests.md).
# Appelé par `release-linux` et `publier` (.github/workflows/ci.yml), juste
# après scripts/release-tag-sur-main.sh.
# STOKESFORGE_REPO_ROOT (essais, scripts/release-tag-test.sh) remplace la
# racine du dépôt.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
TAG="${1:-}"
COMMIT="${2:-HEAD}"
RE='^v([0-9]+)\.([0-9]+)\.([0-9]+)(-rc([0-9]+))?$'

if [[ ! "$TAG" =~ $RE ]]; then
    echo "[release] ERREUR : tag « $TAG » invalide, attendu vX.Y.Z ou vX.Y.Z-rcN" >&2
    exit 1
fi
cx=$((10#${BASH_REMATCH[1]})); cy=$((10#${BASH_REMATCH[2]})); cz=$((10#${BASH_REMATCH[3]}))
c_rc="${BASH_REMATCH[5]}"

# Portée : majeure imposée si le commit n'est pas sur origin/main.
scope_major=""
if ! git -C "$REPO_ROOT" merge-base --is-ancestor "$COMMIT" origin/main 2>/dev/null; then
    for ref in $(git -C "$REPO_ROOT" for-each-ref --format='%(refname:short)' 'refs/remotes/origin/release/*'); do
        if git -C "$REPO_ROOT" merge-base --is-ancestor "$COMMIT" "$ref" 2>/dev/null; then
            if [[ "$ref" =~ ^origin/release/([0-9]+)\.x$ ]]; then
                scope_major=$((10#${BASH_REMATCH[1]}))
                break
            fi
        fi
    done
    if [ -z "$scope_major" ]; then
        echo "[release] ERREUR : le commit n'est ni sur origin/main ni sur une branche origin/release/N.x" >&2
        exit 1
    fi
fi

bad=0
while IFS= read -r existing; do
    [ -n "$existing" ] && [ "$existing" != "$TAG" ] || continue
    [[ "$existing" =~ $RE ]] || continue
    ex=$((10#${BASH_REMATCH[1]})); ey=$((10#${BASH_REMATCH[2]})); ez=$((10#${BASH_REMATCH[3]}))
    e_rc="${BASH_REMATCH[5]}"
    [ -z "$scope_major" ] || [ "$ex" = "$scope_major" ] || continue

    # cmp : signe de (candidat X.Y.Z) - (existant X.Y.Z)
    cmp=0
    for pair in "$cx $ex" "$cy $ey" "$cz $ez"; do
        set -- $pair
        if [ "$1" -gt "$2" ]; then cmp=1; break; fi
        if [ "$1" -lt "$2" ]; then cmp=-1; break; fi
    done

    if [ -z "$c_rc" ]; then
        # Finale : au-dessus de toute finale existante.
        if [ -z "$e_rc" ] && [ "$cmp" != 1 ]; then
            echo "[release] ERREUR : $TAG ne dépasse pas la finale existante $existing" >&2
            bad=1
        fi
    elif [ "$cmp" = 0 ]; then
        if [ -z "$e_rc" ]; then
            echo "[release] ERREUR : $TAG vise une version dont la finale $existing est déjà posée" >&2
            bad=1
        elif [ "$((10#$c_rc))" -le "$((10#$e_rc))" ]; then
            echo "[release] ERREUR : $TAG ne dépasse pas le rc existant $existing" >&2
            bad=1
        fi
    fi
done < <(git -C "$REPO_ROOT" tag --list 'v*')

[ "$bad" = 0 ] || exit 1

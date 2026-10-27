#!/usr/bin/env bash
# Vérifie que tous les crates du workspace portent une même version X.Y.Z
# (#349, #350). Ne lit ni tag ni CHANGELOG.
#
# Usage : scripts/versions-alignees.sh [X.Y.Z]
#   sans argument : les crates doivent être égaux entre eux ;
#   avec argument : ils doivent tous être en X.Y.Z.
#
# Refuse (code 1) si un crate dérive (liste de chaque crate et sa version), si
# aucun crate n'est trouvé, ou si la section [package] d'un crate n'a pas
# exactement une ligne `version = "…"` (`version.workspace = true` refusé :
# non lu ici). Seule la section [package] est lue (#352).
# Les dépendances internes `stokesforge-x = { version = "…" }` doivent porter
# la même version (path seul permis ; table [dependencies.stokesforge-x] et
# `package = "stokesforge-…"` refusés, non lus), et FORMAT_VERSION de
# stokesforge-persistence exige le témoin tests/temoins/vN/projet.stf (#353).
# Cargo.lock doit porter chaque crate du workspace à la même version (#351) :
# refus (code 1) si le verrou est absent ou en retard (`cargo update --workspace`).
# Appelé par le travail `fmt` (GitHub et GitLab) et par scripts/release-notes.sh.
# STOKESFORGE_REPO_ROOT (essais) remplace la racine du dépôt.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
WANTED="${1:-}"

names=()
versions=()
bad=0
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    name="$(basename "$(dirname "$manifest")")"
    # Section [package] seule : du [package] au prochain en-tête de section.
    found="$(awk '
        /^\[/ { in_pkg = ($0 ~ /^\[package\][[:space:]]*(#.*)?$/); next }
        in_pkg && /^version\.workspace[[:space:]]*=/ { ws = 1 }
        in_pkg && /^version[[:space:]]*=[[:space:]]*"/ {
            v = $0
            sub(/^version[[:space:]]*=[[:space:]]*"/, "", v)
            sub(/".*$/, "", v)
            n++; val = v
        }
        END {
            if (ws) { print "WORKSPACE"; exit }
            if (n > 1) { print "DOUBLE"; exit }
            if (n == 1) print val
        }' "$manifest")"
    if [ "$found" = WORKSPACE ]; then
        echo "[versions] ERREUR : $name hérite de « version.workspace = true », non lu ici : écrire « version = \"X.Y.Z\" » dans [package]" >&2
        bad=1
        continue
    fi
    if [ "$found" = DOUBLE ]; then
        echo "[versions] ERREUR : $name a plusieurs lignes « version = » dans [package]" >&2
        bad=1
        continue
    fi
    if [ -z "$found" ]; then
        echo "[versions] ERREUR : $name n'a pas de ligne « version = \"X.Y.Z\" » dans [package]" >&2
        bad=1
        continue
    fi
    names+=("$name")
    versions+=("$found")
done

if [ "${#names[@]}" = 0 ] && [ "$bad" = 0 ]; then
    echo "[versions] ERREUR : aucun crate trouvé dans $REPO_ROOT/crates" >&2
    exit 1
fi

reference="${WANTED:-${versions[0]:-}}"
for i in "${!names[@]}"; do
    if [ "${versions[$i]}" != "$reference" ]; then
        echo "[versions] ERREUR : ${names[$i]} est en ${versions[$i]}, attendu $reference" >&2
        bad=1
    fi
done

# Dépendances internes : `stokesforge-x = { version = "V", … }` dans toute
# section *dependencies (dev-, build-, target.'cfg(…)'.*) doit porter la version
# de référence. Une dépendance par chemin seul est permise. Les formes que ce
# contrôle ne sait pas lire sont refusées plutôt que laissées passer (#353).
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    name="$(basename "$(dirname "$manifest")")"
    dep_errors="$(awk -v ref="$reference" -v crate="$name" '
        /^[[:space:]]*\[/ {
            sect = $0
            dep_sect = (sect ~ /dependencies[[:space:]]*\][[:space:]]*(#.*)?$/)
            if (sect ~ /dependencies\.stokesforge-/) {
                printf "%s : dépendance interne en table [..], forme non lue (écrire stokesforge-x = { version = \"%s\", path = \"…\" }) : %s\n", crate, ref, sect
            }
            next
        }
        /package[[:space:]]*=[[:space:]]*"stokesforge-/ {
            printf "%s : dépendance interne renommée (package = \"stokesforge-…\"), forme non lue : %s\n", crate, $0
            next
        }
        dep_sect && /^[[:space:]]*stokesforge-[A-Za-z0-9_-]+[[:space:]]*=/ {
            dep = $0
            sub(/^[[:space:]]*/, "", dep)
            sub(/[[:space:]]*=.*$/, "", dep)
            if ($0 ~ /version[[:space:]]*=[[:space:]]*"/) {
                v = $0
                sub(/^.*version[[:space:]]*=[[:space:]]*"/, "", v)
                sub(/".*$/, "", v)
            } else if ($0 ~ /^[[:space:]]*stokesforge-[A-Za-z0-9_-]+[[:space:]]*=[[:space:]]*"/) {
                v = $0
                sub(/^[^=]*=[[:space:]]*"/, "", v)
                sub(/".*$/, "", v)
            } else {
                next   # chemin seul
            }
            if (v != ref) {
                printf "%s : dépendance %s en %s, attendu %s\n", crate, dep, v, ref
            }
        }' "$manifest")"
    if [ -n "$dep_errors" ]; then
        while IFS= read -r line; do
            echo "[versions] ERREUR : $line" >&2
        done <<< "$dep_errors"
        bad=1
    fi
done

# Format .stf : FORMAT_VERSION = N exige le témoin tests/temoins/vN/projet.stf
# (invariant 12 : un témoin ne se modifie jamais, une version de format neuve
# en apporte un). Git ne versionne pas un dossier vide : on exige le fichier.
SCHEMA="$REPO_ROOT/crates/stokesforge-persistence/src/schema.rs"
fv_lines=""
[ ! -e "$SCHEMA" ] || fv_lines="$(grep -E '^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?const[[:space:]]+FORMAT_VERSION[[:space:]]*:[[:space:]]*[a-z0-9]+[[:space:]]*=[[:space:]]*[0-9]+[[:space:]]*;' "$SCHEMA" || true)"
if [ "$(printf '%s' "$fv_lines" | grep -c .)" != 1 ]; then
    echo "[versions] ERREUR : FORMAT_VERSION introuvable (ou en plusieurs exemplaires) dans $SCHEMA" >&2
    bad=1
else
    fv="$(printf '%s' "$fv_lines" | sed 's/^.*=[[:space:]]*\([0-9][0-9]*\)[[:space:]]*;.*$/\1/')"
    temoin="$REPO_ROOT/crates/stokesforge-persistence/tests/temoins/v$fv/projet.stf"
    if [ ! -f "$temoin" ]; then
        echo "[versions] ERREUR : FORMAT_VERSION = $fv sans témoin $temoin (invariant 12, docs/persistance.md)" >&2
        bad=1
    fi
fi

# Cargo.lock : l'entrée de chaque crate du workspace porte la version de
# référence. Un `cargo update --workspace` oublié après un changement de version
# (règle #333) laisse l'ancienne version dans le verrou (#351), sans cargo.
LOCK="$REPO_ROOT/Cargo.lock"
if [ ! -f "$LOCK" ]; then
    echo "[versions] ERREUR : Cargo.lock absent ($LOCK), versionné pour des constructions reproductibles" >&2
    bad=1
else
    for name in "${names[@]}"; do
        locked="$(awk -v n="$name" '
            $0 == "name = \"" n "\"" { want = 1; next }
            want && /^version = "/ { v = $0; sub(/^version = "/, "", v); sub(/".*$/, "", v); print v; exit }
            /^\[\[package\]\]/ { want = 0 }' "$LOCK")"
        if [ -z "$locked" ]; then
            echo "[versions] ERREUR : $name absent de Cargo.lock : lancer « cargo update --workspace »" >&2
            bad=1
        elif [ "$locked" != "$reference" ]; then
            echo "[versions] ERREUR : Cargo.lock porte $name en $locked, attendu $reference : lancer « cargo update --workspace »" >&2
            bad=1
        fi
    done
fi

[ "$bad" = 0 ] || exit 1
echo "[versions] ${#names[@]} crates alignés en $reference"

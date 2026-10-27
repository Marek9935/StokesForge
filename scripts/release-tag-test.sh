#!/usr/bin/env bash
# Essai de scripts/release-tag-sur-main.sh (#354) et scripts/release-tag-croissant.sh (#355)
# sur un dépôt git temporaire isolé : sans réseau, sans toucher au vrai dépôt.
# Usage : scripts/release-tag-test.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export STOKESFORGE_REPO_ROOT="$tmp/repo"

g() { git -C "$tmp/repo" -c user.name=essai -c user.email=essai@example.invalid -c commit.gpgsign=false "$@"; }
git init -q "$tmp/repo"
tree="$(g mktree </dev/null)"
commit() { g commit-tree -m "$1" "${@:2}" "$tree"; }
c_main="$(commit main)"
c_release="$(commit release)"   # hors main : seule origin/release/1.x le contient
c_feature="$(commit feature)"
c_v2="$(commit v2)"
g update-ref refs/remotes/origin/main "$c_main"
g update-ref refs/remotes/origin/release/1.x "$c_release"
g update-ref refs/heads/feature "$c_feature"
g update-ref refs/remotes/origin/v2 "$c_v2"

fail=0
# expect <code> <description> <motif du message ou ""> <script> <args…>
expect() {
    local code="$1" desc="$2" motif="$3" got=0
    shift 3
    "$HERE/$1" "${@:2}" >"$tmp/err" 2>&1 || got=$?
    if [ "$got" != "$code" ]; then
        echo "ECHEC : $desc (code $got, attendu $code)" >&2
        fail=1
    elif [ -n "$motif" ]; then
        grep -q -- "$motif" "$tmp/err" || { echo "ECHEC : $desc : message « $motif » absent" >&2; fail=1; }
    fi
}

# --- #354 : le commit tagué est sur main ou sur release/* ---
expect 0 "commit sur origin/main" "" release-tag-sur-main.sh v1.0.0 "$c_main"
expect 0 "commit seulement sur origin/release/1.x" "" release-tag-sur-main.sh v1.0.0 "$c_release"
expect 1 "commit sur une branche de travail" "::error title=tag hors main::" release-tag-sur-main.sh v1.0.0 "$c_feature"
expect 1 "commit seulement sur origin/v2" "tag hors main" release-tag-sur-main.sh v1.0.0 "$c_v2"

# --- #355 : tag croissant ---
# croissant <code> <description> <motif> <tag> <commit> <tags existants…>
croissant() {
    local code="$1" desc="$2" motif="$3" tag="$4" commit="$5"
    shift 5
    for t in $(g tag --list); do g tag -d "$t" >/dev/null; done
    for t in "$@"; do g tag "$t" "$c_main"; done
    expect "$code" "$desc" "$motif" release-tag-croissant.sh "$tag" "$commit"
}
croissant 1 "rc8 alors que la finale 1.0.0 existe" "finale" v1.0.0-rc8 "$c_main" v1.0.0-rc7 v1.0.0
croissant 1 "0.9.0 après 1.0.0" "ne dépasse pas" v0.9.0 "$c_main" v1.0.0
croissant 0 "tag déjà présent (exclu par son nom)" "" v1.0.0 "$c_main" v1.0.0
croissant 0 "1.0.1-rc1 après la finale 1.0.0" "" v1.0.1-rc1 "$c_main" v1.0.0
croissant 0 "finale 1.0.1 au-dessus de son rc" "" v1.0.1 "$c_main" v1.0.1-rc1
croissant 1 "1.0.0-rc1 après la finale 1.0.0" "finale" v1.0.0-rc1 "$c_main" v1.0.0
croissant 1 "1.0.0 déjà posé mais 2.0.0 existe" "ne dépasse pas" v1.0.0 "$c_main" v1.0.0 v2.0.0
croissant 1 "1.4.2 sur main, 2.0.0 existe" "ne dépasse pas" v1.4.2 "$c_main" v1.0.0 v2.0.0
croissant 0 "1.4.2 sur release/1.x seul, 2.0.0 hors portée" "" v1.4.2 "$c_release" v1.0.0 v2.0.0
croissant 1 "1.0.0 sur release/1.x, finale 1.4.0" "ne dépasse pas" v1.0.0 "$c_release" v1.4.0 v2.0.0
croissant 0 "1.1.0 malgré le rc abandonné 1.2.0-rc1" "" v1.1.0 "$c_main" v1.2.0-rc1
croissant 1 "rc2 après rc3" "ne dépasse pas" v1.0.1-rc2 "$c_main" v1.0.1-rc3
croissant 0 "rc10 après rc9 (comparaison numérique)" "" v1.0.1-rc10 "$c_main" v1.0.1-rc9
croissant 1 "tag mal formé" "invalide" v1.0 "$c_main" v1.0.0
croissant 1 "commit hors main et hors release" "ni sur origin/main" v1.0.1 "$c_feature" v1.0.0

[ "$fail" = 0 ] && echo "release-tag-test : ok"
exit "$fail"

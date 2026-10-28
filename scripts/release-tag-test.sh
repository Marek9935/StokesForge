#!/usr/bin/env bash
# Test of scripts/release-tag-on-main.sh (#354) and scripts/release-tag-increasing.sh (#355)
# on an isolated temporary git repository: no network, the real repository is not touched.
# Usage: scripts/release-tag-test.sh
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
c_release="$(commit release)"   # not on main: only origin/release/1.x contains it
c_feature="$(commit feature)"
c_v2="$(commit v2)"
g update-ref refs/remotes/origin/main "$c_main"
g update-ref refs/remotes/origin/release/1.x "$c_release"
g update-ref refs/heads/feature "$c_feature"
g update-ref refs/remotes/origin/v2 "$c_v2"

fail=0
# expect <code> <description> <message pattern or ""> <script> <args…>
expect() {
    local code="$1" desc="$2" motif="$3" got=0
    shift 3
    "$HERE/$1" "${@:2}" >"$tmp/err" 2>&1 || got=$?
    if [ "$got" != "$code" ]; then
        echo "FAILED: $desc (code $got, expected $code)" >&2
        fail=1
    elif [ -n "$motif" ]; then
        grep -q -- "$motif" "$tmp/err" || { echo "FAILED: $desc: message \"$motif\" missing" >&2; fail=1; }
    fi
}

# --- #354: the tagged commit is on main or on release/* ---
expect 0 "commit on origin/main" "" release-tag-on-main.sh v1.0.0 "$c_main"
expect 0 "commit only on origin/release/1.x" "" release-tag-on-main.sh v1.0.0 "$c_release"
expect 1 "commit on a working branch" "::error title=tag not on main::" release-tag-on-main.sh v1.0.0 "$c_feature"
expect 1 "commit only on origin/v2" "tag not on main" release-tag-on-main.sh v1.0.0 "$c_v2"

# --- #355: increasing tag ---
# croissant <code> <description> <pattern> <tag> <commit> <existing tags…>
croissant() {
    local code="$1" desc="$2" motif="$3" tag="$4" commit="$5"
    shift 5
    for t in $(g tag --list); do g tag -d "$t" >/dev/null; done
    for t in "$@"; do g tag "$t" "$c_main"; done
    expect "$code" "$desc" "$motif" release-tag-increasing.sh "$tag" "$commit"
}
croissant 1 "rc8 while final release 1.0.0 exists" "final release" v1.0.0-rc8 "$c_main" v1.0.0-rc7 v1.0.0
croissant 1 "0.9.0 after 1.0.0" "does not exceed" v0.9.0 "$c_main" v1.0.0
croissant 0 "tag already present (excluded by name)" "" v1.0.0 "$c_main" v1.0.0
croissant 0 "1.0.1-rc1 after final release 1.0.0" "" v1.0.1-rc1 "$c_main" v1.0.0
croissant 0 "final release 1.0.1 above its rc" "" v1.0.1 "$c_main" v1.0.1-rc1
croissant 1 "1.0.0-rc1 after final release 1.0.0" "final release" v1.0.0-rc1 "$c_main" v1.0.0
croissant 1 "1.0.0 already tagged but 2.0.0 exists" "does not exceed" v1.0.0 "$c_main" v1.0.0 v2.0.0
croissant 1 "1.4.2 on main, 2.0.0 exists" "does not exceed" v1.4.2 "$c_main" v1.0.0 v2.0.0
croissant 0 "1.4.2 on release/1.x only, 2.0.0 out of scope" "" v1.4.2 "$c_release" v1.0.0 v2.0.0
croissant 1 "1.0.0 on release/1.x, final release 1.4.0" "does not exceed" v1.0.0 "$c_release" v1.4.0 v2.0.0
croissant 0 "1.1.0 despite the abandoned rc 1.2.0-rc1" "" v1.1.0 "$c_main" v1.2.0-rc1
croissant 1 "rc2 after rc3" "does not exceed" v1.0.1-rc2 "$c_main" v1.0.1-rc3
croissant 0 "rc10 after rc9 (numeric comparison)" "" v1.0.1-rc10 "$c_main" v1.0.1-rc9
croissant 1 "malformed tag" "invalid" v1.0 "$c_main" v1.0.0
croissant 1 "commit neither on main nor on release" "neither on origin/main" v1.0.1 "$c_feature" v1.0.0

[ "$fail" = 0 ] && echo "release-tag-test : ok"
exit "$fail"

#!/usr/bin/env bash
# Start a work package: worktree, colour, named orchestrator session (#413).
#
#   scripts/chantier.sh [--dry-run] <issue-number>...
#
# For each issue, if ../_stokesforge/feature<n> is missing, creates it from an
# up-to-date origin/main on branch feature/<n>-<slug> (slug from the GitLab
# title; no upstream tracking, so `main` is never the upstream). The colour is
# the first free of the six squares, kept until the worktree is removed. Then
# starts `claude -n "<square> #<n> <short name>"` from the main checkout, with
# the worktree(s) added by --add-dir. Run it from the main checkout.
#
# Test seam: SF_ISSUE_TITLE replaces the `glab issue view` lookup.
# --dry-run creates nothing and prints what it would do.
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

dry=0
issues=()
for a in "$@"; do
    case "$a" in
    --dry-run) dry=1 ;;
    -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
    *[!0-9]* | '') echo "usage: chantier.sh [--dry-run] <issue-number>..." >&2; exit 2 ;;
    *) issues+=("$a") ;;
    esac
done
[ "${#issues[@]}" -gt 0 ] || { echo "usage: chantier.sh [--dry-run] <issue-number>..." >&2; exit 2; }

main="$(sf_main_dir)"
root="$(sf_wt_root)"

title_of() {
    if [ -n "${SF_ISSUE_TITLE:-}" ]; then printf '%s\n' "$SF_ISSUE_TITLE"; return; fi
    glab issue view "$1" -F json | python3 -c 'import json,sys; print(json.load(sys.stdin)["title"])'
}

# "Fix axis label: wrong side" -> "fix-axis-label" (at most 40 characters).
slug_of() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]\{1,\}/-/g; s/^-//; s/-$//' \
        | cut -c1-40 | sed 's/-$//'
}

# Short name for the session: the title up to the first colon or dash, 40 characters.
short_of() {
    printf '%s' "$1" | sed 's/ *[:—] .*//; s/ - .*//' | cut -c1-40
}

if [ "$dry" = 0 ] && git -C "$main" remote | grep -qx origin; then
    git -C "$main" fetch origin
fi
base=main
git -C "$main" rev-parse --verify -q origin/main > /dev/null && base=origin/main

names=()
wts=()
for n in "${issues[@]}"; do
    title="$(title_of "$n")"
    wt="$root/feature$n"
    if [ -d "$wt" ]; then
        colour="$(sf_colour_of "$wt")"
    else
        colour="$(sf_first_free)"
        branch="feature/$n-$(slug_of "$title")"
        if [ -z "$colour" ]; then
            echo "all six colours are taken: finish or remove a worktree first (git worktree list)" >&2
            exit 1
        fi
        if [ "$dry" = 1 ]; then
            echo "would run: git worktree add $wt -b $branch --no-track $base"
        else
            mkdir -p "$root"
            sf_with_lock "$root/.colour.lock" bash -c '
                set -e
                . "$1/workflow-lib.sh"
                colour="$(sf_first_free)"
                [ -n "$colour" ] || { echo "all six colours are taken" >&2; exit 1; }
                git -C "$2" worktree add "$3" -b "$4" --no-track "$5"
                printf "%s\n" "$colour" > "$(sf_colour_file "$3")"
            ' _ "$(dirname "$0")" "$main" "$wt" "$branch" "$base"
            colour="$(sf_colour_of "$wt")"
        fi
    fi
    [ -n "$colour" ] || colour="?"
    names+=("$colour #$n $(short_of "$title")")
    wts+=("$wt")
done

# One issue: its name. Several issues in one session: see session_name.
session_name() { printf '%s' "${names[0]}"; }
name="$(session_name)"

args=(claude -n "$name")
for wt in "${wts[@]}"; do args+=(--add-dir "$wt"); done

printf '%s\n' "$name"
if [ "$dry" = 1 ]; then
    printf 'would run from %s:' "$main"
    printf ' claude -n "%s"' "$name"
    for wt in "${wts[@]}"; do printf ' --add-dir %s' "$wt"; done
    printf '\n'
    exit 0
fi
cd "$main"
exec "${args[@]}"

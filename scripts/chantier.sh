#!/usr/bin/env bash
# Start a work package from the current Claude session (#413, #417).
#
#   scripts/chantier.sh [--dry-run] <issue-number>...
#
# Run BY the orchestrator, in the current session, from the main checkout (the
# user starts plain `claude` or does `/clear`, then types `lance #<n>`). It starts
# no session. For each issue, if ../_stokesforge/feature<n> is missing, creates it
# from an up-to-date origin/main on branch feature/<n>-<slug> (slug from the GitLab
# title; no upstream tracking, so `main` is never the upstream). The colour is
# the first free of the six squares, kept until the worktree is removed: rerunning
# the script (after a `/clear`) finds the worktree and prints the same colour.
# Then prints a short briefing for the agent: the tag "<square> #<n> <short name>"
# of each issue, its worktree and branch, and the steps to follow.
#
# Short name: the first 3 words of the title, in the issue's own language. Several
# issues: one worktree each, one session. Seven work packages cannot run at once:
# with the six colours taken the script refuses with a message and a non-zero exit.
#
# Test seams: SF_ISSUE_TITLE_<n> (or SF_ISSUE_TITLE for any issue) replaces the
# `glab issue view` lookup; SF_NO_FETCH=1 skips `git fetch`.
# --dry-run creates nothing and prints what it would create.
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

dry=0
issues=()
for a in "$@"; do
    case "$a" in
    --dry-run) dry=1 ;;
    -h | --help) sed -n '2,20p' "$0"; exit 0 ;;
    *[!0-9]* | '') echo "usage: chantier.sh [--dry-run] <issue-number>..." >&2; exit 2 ;;
    *) issues+=("$a") ;;
    esac
done
[ "${#issues[@]}" -gt 0 ] || { echo "usage: chantier.sh [--dry-run] <issue-number>..." >&2; exit 2; }

main="$(sf_main_dir)"
root="$(realpath -m "$(sf_wt_root)")"

title_of() {
    local var="SF_ISSUE_TITLE_$1"
    if [ -n "${!var:-}" ]; then printf '%s\n' "${!var}"; return; fi
    if [ -n "${SF_ISSUE_TITLE:-}" ]; then printf '%s\n' "$SF_ISSUE_TITLE"; return; fi
    glab issue view "$1" -F json | python3 -c 'import json,sys; print(json.load(sys.stdin)["title"])'
}

# "Fix axis label: wrong side" -> "fix-axis-label" (at most 40 characters).
slug_of() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]\{1,\}/-/g; s/^-//; s/-$//' \
        | cut -c1-40 | sed 's/-$//'
}

# Short name: the title up to the first colon or dash, then its first 3 words,
# without a trailing small word ("Frames follow the" reads "Frames follow").
short_of() {
    # The small-word list is English and French: issue titles may be French.
    printf '%s' "$1" | sed 's/ *[:—] .*//; s/ - .*//' | awk '
        # lang: fr
        BEGIN { split("the a an of to in on for and with by at from de la le les des du et à au aux en un une pour par sur", w, " "); for (k in w) small[w[k]] = 1 }
        { n = (NF < 3) ? NF : 3; while (n > 1 && (tolower($n) in small)) n--; s = $1; for (i = 2; i <= n; i++) s = s " " $i; print s }'
}

all_taken() {
    local all=""
    for c in "${SF_COLOURS[@]}"; do all="$all$c"; done
    echo "6 work packages in progress ($all): finish or remove one first (git worktree list)" >&2
    exit 1
}
reserved=""   # colours promised in this run (dry-run only)

if [ "$dry" = 0 ] && [ -z "${SF_NO_FETCH:-}" ] && git -C "$main" remote | grep -qx origin; then
    git -C "$main" fetch origin
fi
base=main
git -C "$main" rev-parse --verify -q origin/main > /dev/null && base=origin/main

names=()
wts=()
brs=()
for n in "${issues[@]}"; do
    title="$(title_of "$n")"
    wt="$root/feature$n"
    if [ -d "$wt" ]; then
        colour="$(sf_colour_of "$wt")"
        branch="$(git -C "$wt" symbolic-ref --short HEAD 2> /dev/null || echo '?')"
        if [ -z "$colour" ] && [ "$dry" = 0 ]; then
            colour="$(sf_first_free)"
            [ -n "$colour" ] || all_taken
            printf '%s\n' "$colour" > "$(sf_colour_file "$wt")"
        fi
    else
        colour="$(sf_first_free)"
        if [ "$dry" = 1 ]; then
            for c in "${SF_COLOURS[@]}"; do
                if ! { sf_used_colours; printf '%s\n' $reserved; } | grep -xF "$c" > /dev/null; then colour="$c"; break; fi
                colour=""
            done
            reserved="$reserved $colour"
        fi
        branch="feature/$n-$(slug_of "$title")"
        [ -n "$colour" ] || all_taken
        if [ "$dry" = 1 ]; then
            echo "would run: git worktree add $wt -b $branch --no-track $base"
        else
            mkdir -p "$root"
            sf_with_lock "$root/.colour.lock" bash -c '
                set -e
                . "$1/workflow-lib.sh"
                colour="$(sf_first_free)"
                [ -n "$colour" ] || { echo "6 work packages in progress: finish or remove one first" >&2; exit 1; }
                git -C "$2" worktree add "$3" -b "$4" --no-track "$5"
                printf "%s\n" "$colour" > "$(sf_colour_file "$3")"
            ' _ "$(dirname "$0")" "$main" "$wt" "$branch" "$base"
            colour="$(sf_colour_of "$wt")"
        fi
    fi
    [ -n "$colour" ] || colour="?"
    names+=("$colour #$n $(short_of "$title")")
    wts+=("$wt")
    brs+=("$branch")
done

if [ "$dry" = 1 ]; then echo "(dry run: nothing created)"; fi
echo "Work package briefing (scripts/chantier.sh); the tag goes in forms, the To-test page and notifications:"
for i in "${!names[@]}"; do
    echo "  ${names[$i]}  worktree ${wts[$i]}  branch ${brs[$i]}"
done
cat <<'EOF'
Several issues: one worktree each, one session. Never work in another worktree.
Steps, in order:
 1. Read each issue, ALL its comments, the sibling issues, `git worktree list`
    (stokesforge-feature, section 1).
 2. Register the work package in the `chantiers` collection of the To-test page
    (URL in stokesforge-verification/references/to-test-page.md).
 3. One start decision form for all open decisions, cards tagged with the issue's
    square (stokesforge-bug-report/references/decision-form.md).
 4. Agent `perimetre`, then Sonnet implements in the worktree (CLAUDE.md, "Delegation").
 5. Visible change: `scripts/try.sh <n>` opens the branch on the user's screen;
    their OK is the go-ahead (stokesforge-verification, step 6).
 6. Merge under `scripts/main-lock.sh` from the main checkout (step 6), read the CI.
 7. Add the To-test entry, delete the `chantiers/<n>` document (step 8).
 8. Clean up the worktree and branch (section "Clean up after a merge on main").
EOF

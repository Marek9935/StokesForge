#!/usr/bin/env bash
# Claude Code status line of the project (#413): which work package this session
# is in (colour and issue number), which other colours are live, and who holds
# the lock on `main`. If the user has their own ~/.claude/statusline.sh it is
# called first (same JSON on stdin) and ours is appended after " | ".
#
#   <your line> | 🟦 #413 | others 🟩🟧 | main lock: 🟩 #407
#
# Set by .claude/settings.json (`statusLine`). Never fails: a status line must
# not break the session, so every error falls back to printing less.
. "$(dirname "$0")/workflow-lib.sh" 2> /dev/null || exit 0

input="$(cat)"
cwd="$(printf '%s' "$input" | jq -r '.workspace.current_dir // .cwd // empty' 2> /dev/null)"
[ -n "$cwd" ] || cwd="$PWD"

own="" ; num="" ; others=""
while read -r wt; do
    [ -n "$wt" ] || continue
    col="$(sf_colour_of "$wt" 2> /dev/null)"
    [ -n "$col" ] || continue
    case "$cwd/" in
    "$wt"/*) own="$col" ; num="$(sf_issue_of "$wt")" ;;
    *) others="$others$col" ;;
    esac
done < <(sf_worktrees 2> /dev/null)

line=""
if [ -x "$HOME/.claude/statusline.sh" ]; then
    line="$(printf '%s' "$input" | "$HOME/.claude/statusline.sh" 2> /dev/null | head -n 1)"
fi

part=""
if [ -n "${SF_SESSION:-}" ]; then
    # Started by scripts/chantier.sh: show the session's own squares and numbers,
    # and as "others" only the live colours that are not this session's.
    part="$(printf '%s' "$SF_SESSION" | grep -oE '[^ ]+ #[0-9]+' | paste -sd' ' -)"
    rest=""
    for c in $(printf '%s' "$own$others" | grep -oE '🟦|🟩|🟧|🟪|🟥|🟨'); do
        case "$SF_SESSION" in *"$c"*) ;; *) rest="$rest$c" ;; esac
    done
    others="$rest"
elif [ -n "$own" ]; then part="$own #$num"; else part="main"; fi
[ -z "$others" ] || part="$part | others $others"
lock="$(sf_wt_root 2> /dev/null)/.main.lock/owner"
[ ! -f "$lock" ] || part="$part | main lock: $(cut -d' ' -f1,2 "$lock")"

if [ -n "$line" ]; then printf '%s | %s\n' "$line" "$part"; else printf '%s\n' "$part"; fi

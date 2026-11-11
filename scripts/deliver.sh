#!/usr/bin/env bash
# Deliver a work package's branch to main (or to its integration line) in one stop-at-first-failure run (#449).
#
#   scripts/deliver.sh [--wait-tag rc|final] <issue-number> <message-file>
#
# Run from the MAIN checkout, on `main` (the script of a worktree may be run with the
# main checkout as the current directory: it finds its siblings by its own path and
# works on the repository of the current directory). The message file holds the merge
# commit message; its first line starts with `Merge #<n>:`.
#
# Before anything else it refuses while a trial entry of the issue is a retour nobody settled (#496).
#
# Order: checks, take the main lock (main-lock.sh), then a trap that releases it on
# every exit; clean main and clean branch worktree; fetch origin main (SF_NO_FETCH is
# ignored: this is the one fetch that matters), fast-forward if behind, stop if
# diverged (never a force). If main already holds a `Merge #<n>:` first-parent commit
# that contains the branch (an earlier run got that far), that commit is M and the run
# goes straight to the push. Otherwise, if main is not in the branch yet, main is merged
# INTO the branch (in its worktree) and the run stops with exit 3: recheck, then rerun.
# Else `git merge --no-ff -F` gives M. Push main (`--no-follow-tags`, 4 retries after
# 2, 4, 8, 16 s; test seam SF_DELIVER_DELAYS="2 4 8 16"). Last, if the issue still has
# trial entries with `try`, `suivi.py test merged <n> M` writes M on them. No tag is
# ever created or pushed. The run stops before `chantier.sh --release`, which stays
# with the orchestrator. The CI is not polled: the watcher of `suivi.py serve`
# shows red on the tracking page (read it only for a tag, a release or a change to CI itself).
#
# `--wait-tag rc|final` (#467): the issue stays open on purpose because only a tag proves it.
# Refused before the lock if the message closes the issue (`Closes #<n>` and the other GitLab
# keywords); after the push it runs `suivi.py tag-wait <n> <kind>`, and the service then adds the
# label `attente-rc` / `attente-final` on GitLab (no glab call here).
#
# INTEGRATION LINE (#480). If the issue's record declares a line (suivi.py line <n>, e.g. PostV1.1),
# everything above happens on that line instead of main: in its permanent worktree
# (../_stokesforge/line-<line>, which must be clean and on the line), pushed as `origin <line>`.
# The main checkout is only the place the script is run from: it may be dirty or on another
# branch, and it is never touched. The message may not close anything (`Closes #n`, `Fixes`,
# `Resolves`: use `Refs #n`; a closing keyword would close the issue when the line lands) and
# `--wait-tag` is refused; both before the lock. A line that moved is merged into the branch
# (exit 3), as main is. A delivery to main refuses a branch that holds a commit of a line that
# is not on main yet (a branch must not drag the line into main); a branch based on a line
# already landed is fine.
#
# Exit: 0 delivered, 1 refused or failed (main unchanged unless M was already merged
# locally), 2 usage, 3 main moved: it was merged into the branch, recheck and rerun.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/workflow-lib.sh"

die() { echo "deliver: $*" >&2; exit 1; }

usage() { echo "usage: deliver.sh [--wait-tag rc|final] <issue-number> <message-file>" >&2; exit 2; }
wait_tag=""
if [ "${1:-}" = "--wait-tag" ]; then
    [ "$#" -ge 2 ] || usage
    case "$2" in rc | final) wait_tag="$2" ;; *) usage ;; esac
    shift 2
fi
[ "$#" -eq 2 ] && [[ "$1" =~ ^[0-9]+$ ]] || usage
n="$1"
msgfile="$2"
[ -f "$msgfile" ] && [ -r "$msgfile" ] || die "cannot read the message file $msgfile"
msgfile="$(realpath "$msgfile")"   # the run may change directory (line worktree)
line="$(sf_line_of "$n")"
if [ "$line" != main ]; then
    [ -z "$wait_tag" ] || die "#$n is on the line $line: --wait-tag is for main, a line merge proves nothing yet"
    if grep -qiE '\b(clos(e|es|ed)|fix(es|ed)?|resolv(e|es|ed)) +#[0-9]+' "$msgfile"; then
        die "#$n is on the line $line: the message may not close an issue (Closes/Fixes/Resolves #n); write Refs #n, the issue closes when the line lands"
    fi
fi
if [ -n "$wait_tag" ] && grep -qiE "\\b(clos(e|es|ed)|fix(es|ed)?|resolv(e|es|ed)) +#$n\\b" "$msgfile"; then
    die "the message closes #$n (Closes/Fixes/Resolves #$n), but --wait-tag keeps it open until the tag: remove that line"
fi
first="$(head -n 1 "$msgfile")"
case "$first" in "Merge #$n:"*) ;; *) die "the first line of the message must start with 'Merge #$n:' (got: $first)" ;; esac

# #496: a retour fixed without a new trial must be settled first (read-only, before the lock).
# Exit 1 = unsettled retours (listed); anything else (no state yet) is not a refusal.
set +e
unsettled="$(python3 -I "$here/suivi.py" test unsettled "$n" 2> /dev/null)"
unsettled_rc=$?
set -e
if [ "$unsettled_rc" -eq 1 ] && [ -n "$unsettled" ]; then
    die "#$n has unsettled retours: $(echo $unsettled). Settle each one first: scripts/suivi.py test settle <id> --commit <fix> --note \"...\""
fi

main="$(sf_main_dir)"
top="$(realpath "$(git rev-parse --show-toplevel)")"
[ "$top" = "$(realpath "$main")" ] || die "run it from the main checkout ($main), not from $top"
if [ "$line" = main ]; then
    [ "$(git symbolic-ref --short HEAD 2> /dev/null || true)" = main ] || die "the main checkout is not on the branch main"
fi

"$here/main-lock.sh" take "deliver #$n" > /dev/null || exit 1
trap '"$here/main-lock.sh" release > /dev/null 2>&1 || true' EXIT

suivi() { python3 -I "$here/suivi.py" "$@"; }

if [ "$line" = main ]; then
    [ -z "$(git status --porcelain)" ] || die "the main checkout has modified or untracked files"
else
    lwt="$(sf_line_dir "$line")"
    [ "$(git -C "$lwt" symbolic-ref --short HEAD 2> /dev/null || true)" = "$line" ] \
        || die "the line $line has no worktree $lwt on that branch (scripts/line.sh create $line); never a fallback to main"
    [ -z "$(git -C "$lwt" status --porcelain)" ] || die "the line worktree $lwt has modified or untracked files"
    cd "$lwt"   # from here on the checkout is the line's; the main checkout stays as it is
fi

branches="$(git for-each-ref --format='%(refname:short)' "refs/heads/feature/$n-*")"
nb="$(printf '%s' "$branches" | grep -c . || true)"
[ "$nb" -le 1 ] || die "several branches feature/$n-*: $(echo $branches)"
branch="$branches"
wt=""
if [ -n "$branch" ]; then
    wt="$(git worktree list --porcelain | awk -v b="refs/heads/$branch" '/^worktree /{p=substr($0,10)} $1=="branch" && $2==b {print p}' | head -n 1)"
    if [ -n "$wt" ] && [ -n "$(git -C "$wt" status --porcelain)" ]; then
        die "the worktree $wt has modified or untracked files"
    fi
fi

git fetch -q origin "$line" || die "git fetch origin $line failed"
if [ "$(git rev-parse "$line")" != "$(git rev-parse "origin/$line")" ]; then
    if git merge-base --is-ancestor "$line" "origin/$line"; then
        git merge -q --ff-only "origin/$line" || die "cannot fast-forward $line to origin/$line"
    elif git merge-base --is-ancestor "origin/$line" "$line"; then
        :   # the line is ahead: an earlier run merged and did not push
    else
        die "$line and origin/$line have diverged; nothing done (never forced)"
    fi
fi

# An earlier run may already have merged: a first-parent `Merge #<n>:` commit holding the branch.
m=""
while read -r h subj; do
    case "$subj" in "Merge #$n:"*) ;; *) continue ;; esac
    if [ -z "$branch" ] || git merge-base --is-ancestor "$branch" "$h"; then
        m="$h"
        [ -z "$branch" ] && break   # branch gone: the newest one; otherwise the oldest wins (keep looping)
    fi
done < <(git log "$line" --first-parent --format='%H %s')

if [ -z "$m" ]; then
    [ -n "$branch" ] || die "no branch feature/$n-* and no 'Merge #$n:' commit on main: nothing to deliver"
    if git merge-base --is-ancestor "$branch" "$line"; then
        die "$branch has nothing $line does not have: nothing to merge"
    fi
    if [ "$line" = main ]; then
        # A branch built on a line that has not landed would drag the line into main. Landed = on main.
        for l in $(sf_lines); do
            git rev-parse --verify -q "refs/heads/$l" > /dev/null || continue
            for b in $(git merge-base --all "$branch" "refs/heads/$l" 2> /dev/null || true); do
                git merge-base --is-ancestor "$b" main \
                    || die "$branch holds the commit $(git rev-parse --short "$b") of the line $l, which is not on main: deliver it to the line, or rebase it on main"
            done
        done
    fi
    if ! git merge-base --is-ancestor "$line" "$branch"; then
        [ -n "$wt" ] || die "$line is not in $branch and the branch has no worktree to merge it in"
        if ! git -C "$wt" merge --no-edit "$line" > /dev/null 2>&1; then
            git -C "$wt" merge --abort > /dev/null 2>&1 || true
            die "merging $line into $branch conflicts; resolve it in $wt, then rerun"
        fi
        echo "deliver: $line had moved; it is now merged into $branch. Recheck the branch (stokesforge-verification), then rerun deliver.sh $n $msgfile" >&2
        exit 3
    fi
    if ! git merge --no-ff -q -F "$msgfile" "$branch" > /dev/null; then
        if [ -f "$(git rev-parse --git-path MERGE_HEAD)" ]; then git merge --abort > /dev/null 2>&1 || true; fi
        die "the merge of $branch failed; $line is unchanged"
    fi
    m="$(git rev-parse HEAD)"
fi

delays="${SF_DELIVER_DELAYS:-2 4 8 16}"
push_main() { git push -q --no-follow-tags origin "$line"; }
pushed=0
if push_main; then pushed=1; else
    for d in $delays; do
        echo "deliver: push failed, retrying in ${d}s" >&2
        sleep "$d"
        if push_main; then pushed=1; break; fi
    done
fi
[ "$pushed" = 1 ] || die "push failed after retries; the merge $m is local only, rerun to push it"

if [ -n "$wait_tag" ]; then
    suivi tag-wait "$n" "$wait_tag" || die "pushed $m, but declaring the wait failed; run: scripts/suivi.py tag-wait $n $wait_tag"
fi
if suivi test list --issue "$n" | grep -q '(try)'; then
    suivi test merged "$n" "$m" || die "pushed $m, but recording it on the trial entries failed; rerun to record it"
else
    echo "deliver: no entry to try on #$n: no commit recorded"
fi
echo "deliver: #$n merged and pushed as $m$([ "$line" = main ] || echo " on $line")"

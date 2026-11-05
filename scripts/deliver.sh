#!/usr/bin/env bash
# Deliver a work package's branch to main in one stop-at-first-failure run (#449).
#
#   scripts/deliver.sh [--wait-tag rc|final] <issue-number> <message-file>
#
# Run from the MAIN checkout, on `main` (the script of a worktree may be run with the
# main checkout as the current directory: it finds its siblings by its own path and
# works on the repository of the current directory). The message file holds the merge
# commit message; its first line starts with `Merge #<n>:`.
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
if [ -n "$wait_tag" ] && grep -qiE "\\b(clos(e|es|ed)|fix(es|ed)?|resolv(e|es|ed)) +#$n\\b" "$msgfile"; then
    die "the message closes #$n (Closes/Fixes/Resolves #$n), but --wait-tag keeps it open until the tag: remove that line"
fi
first="$(head -n 1 "$msgfile")"
case "$first" in "Merge #$n:"*) ;; *) die "the first line of the message must start with 'Merge #$n:' (got: $first)" ;; esac

main="$(sf_main_dir)"
top="$(realpath "$(git rev-parse --show-toplevel)")"
[ "$top" = "$(realpath "$main")" ] || die "run it from the main checkout ($main), not from $top"
[ "$(git symbolic-ref --short HEAD 2> /dev/null || true)" = main ] || die "the main checkout is not on the branch main"

"$here/main-lock.sh" take "deliver #$n" > /dev/null || exit 1
trap '"$here/main-lock.sh" release > /dev/null 2>&1 || true' EXIT

suivi() { python3 -I "$here/suivi.py" "$@"; }

[ -z "$(git status --porcelain)" ] || die "the main checkout has modified or untracked files"

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

git fetch -q origin main || die "git fetch origin main failed"
if [ "$(git rev-parse main)" != "$(git rev-parse origin/main)" ]; then
    if git merge-base --is-ancestor main origin/main; then
        git merge -q --ff-only origin/main || die "cannot fast-forward main to origin/main"
    elif git merge-base --is-ancestor origin/main main; then
        :   # main is ahead: an earlier run merged and did not push
    else
        die "main and origin/main have diverged; nothing done (never forced)"
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
done < <(git log main --first-parent --format='%H %s')

if [ -z "$m" ]; then
    [ -n "$branch" ] || die "no branch feature/$n-* and no 'Merge #$n:' commit on main: nothing to deliver"
    if git merge-base --is-ancestor "$branch" main; then
        die "$branch has nothing main does not have: nothing to merge"
    fi
    if ! git merge-base --is-ancestor main "$branch"; then
        [ -n "$wt" ] || die "main is not in $branch and the branch has no worktree to merge it in"
        if ! git -C "$wt" merge --no-edit main > /dev/null 2>&1; then
            git -C "$wt" merge --abort > /dev/null 2>&1 || true
            die "merging main into $branch conflicts; resolve it in $wt, then rerun"
        fi
        echo "deliver: main had moved; it is now merged into $branch. Recheck the branch (stokesforge-verification), then rerun deliver.sh $n $msgfile" >&2
        exit 3
    fi
    if ! git merge --no-ff -q -F "$msgfile" "$branch" > /dev/null; then
        if [ -f "$(git rev-parse --git-path MERGE_HEAD)" ]; then git merge --abort > /dev/null 2>&1 || true; fi
        die "the merge of $branch failed; main is unchanged"
    fi
    m="$(git rev-parse HEAD)"
fi

delays="${SF_DELIVER_DELAYS:-2 4 8 16}"
push_main() { git push -q --no-follow-tags origin main; }
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
echo "deliver: #$n merged and pushed as $m"

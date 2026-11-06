#!/usr/bin/env bash
# The permanent integration line of the work that must not reach `main` yet (#480).
#
#   scripts/line.sh create <line>     # e.g. PostV1.1
#   scripts/line.sh sync <line>
#
# Run from the MAIN checkout (which is never moved: it may be dirty or on another branch).
# The line lives in its own permanent worktree ../_stokesforge/line-<line>; both verbs take the
# one lock of main-lock.sh for the few seconds they need (git only, no cargo under it).
#
# create: fetches origin, makes the branch <line> from an up-to-date origin/main, pushes it
#   (`origin <line>`, no upstream tracking to main) and adds the worktree. Refused if the name is
#   `main` or not a branch name, or if the branch already exists locally or on origin.
# sync: `main` reaches the line by a regular merge, never the other way before the V1.1 tag. The
#   worktree must be clean and on the line; the line is fast-forwarded to origin/<line> (diverged:
#   refused, never forced), origin/main is merged into it (`Merge main into <line>`), and the line is pushed.
#   A conflict aborts the merge: the line is left as it was, the lock is released, exit 1; resolve it by
#   hand in the line worktree (one orchestrator, under the lock: scripts/main-lock.sh take).
# Exit: 0 done (or nothing to do), 1 refused or failed, 2 usage.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/workflow-lib.sh"

die() { echo "line: $*" >&2; exit 1; }
usage() { echo "usage: line.sh create|sync <line>" >&2; exit 2; }
[ "$#" -eq 2 ] || usage
verb="$1"
line="$2"
case "$verb" in create | sync) ;; *) usage ;; esac
[[ "$line" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "not a line name: $line"
[ "$line" != main ] || die "main is not an integration line"

main="$(sf_main_dir)"
top="$(realpath "$(git rev-parse --show-toplevel)")"
[ "$top" = "$(realpath "$main")" ] || die "run it from the main checkout ($main), not from $top"
lwt="$(sf_line_dir "$line")"

"$here/main-lock.sh" take "line $verb $line" > /dev/null || exit 1
trap '"$here/main-lock.sh" release > /dev/null 2>&1 || true' EXIT

git fetch -q origin || die "git fetch origin failed"

if [ "$verb" = create ]; then
    ! git rev-parse --verify -q "refs/heads/$line" > /dev/null || die "the branch $line already exists here"
    ! git rev-parse --verify -q "refs/remotes/origin/$line" > /dev/null || die "origin/$line already exists"
    [ ! -e "$lwt" ] || die "$lwt already exists"
    git rev-parse --verify -q refs/remotes/origin/main > /dev/null || die "no origin/main"
    git branch --no-track "$line" origin/main
    if ! git push -q origin "$line"; then
        git branch -q -D "$line"
        die "push of $line failed; nothing created"
    fi
    git worktree add -q "$lwt" "$line" || die "cannot add the worktree $lwt (the branch $line is pushed)"
    echo "line: $line created from origin/main, worktree $lwt"
    exit 0
fi

# sync
[ "$(git -C "$lwt" symbolic-ref --short HEAD 2> /dev/null || true)" = "$line" ] \
    || die "$lwt is not the worktree of $line (scripts/line.sh create $line)"
[ -z "$(git -C "$lwt" status --porcelain)" ] || die "the line worktree $lwt has modified or untracked files"
git rev-parse --verify -q "refs/remotes/origin/$line" > /dev/null || die "no origin/$line"
if [ "$(git -C "$lwt" rev-parse HEAD)" != "$(git rev-parse "origin/$line")" ]; then
    if git merge-base --is-ancestor "$line" "origin/$line"; then
        git -C "$lwt" merge -q --ff-only "origin/$line" || die "cannot fast-forward $line to origin/$line"
    elif git merge-base --is-ancestor "origin/$line" "$line"; then
        :   # ahead: an earlier run merged and did not push
    else
        die "$line and origin/$line have diverged; nothing done (never forced)"
    fi
fi
if ! git merge-base --is-ancestor origin/main "$line"; then
    if ! git -C "$lwt" merge --no-ff -q -m "Merge main into $line" origin/main > /dev/null 2>&1; then
        git -C "$lwt" merge --abort > /dev/null 2>&1 || true
        die "merging origin/main into $line conflicts; $line is unchanged. Resolve it by hand in $lwt (under scripts/main-lock.sh take)"
    fi
fi
git push -q --no-follow-tags origin "$line" || die "push of $line failed; the merge is local only, rerun to push it"
echo "line: $line holds origin/main and is pushed"

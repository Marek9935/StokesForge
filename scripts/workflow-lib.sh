#!/usr/bin/env bash
# Shared helpers of the parallel-orchestrator workflow (#413). Sourced, not run:
# chantier.sh, try.sh, main-lock.sh, deliver.sh, workflow-test.sh.
#
# Layout: the main checkout is the repository root; the worktrees of the work
# packages live in ../_stokesforge/feature<n> (CLAUDE.md, "Conventions"; suivi.md, "Work packages and worktrees"). A work
# package's colour lives in ../_stokesforge/.suivi (scripts/suivi.py, #441), not
# in these helpers.

SF_COLOURS=(🟦 🟩 🟧 🟪 🟥 🟨)
# The tracking page, served locally by the work-package server (#441).
SF_TO_TEST_URL="http://127.0.0.1:8441/"

# Main checkout (absolute), whichever worktree we run from.
sf_main_dir() {
    local common
    common="$(git rev-parse --path-format=absolute --git-common-dir)"
    dirname "$common"
}

sf_wt_root() { printf '%s/../_stokesforge\n' "$(sf_main_dir)"; }

# Paths of the linked worktrees (every one but the main checkout).
sf_worktrees() {
    local main path
    main="$(sf_main_dir)"
    git -C "$main" worktree list --porcelain \
        | sed -n 's/^worktree //p' | while read -r path; do
            [ "$path" = "$main" ] || printf '%s\n' "$path"
        done
}

# Template target (#523): built from main, copied into every new worktree (scripts/template-target.sh).
# SF_TEMPLATE_DIR moves it (tests).
sf_template_dir() { printf '%s\n' "${SF_TEMPLATE_DIR:-$(realpath -m "$(sf_wt_root)")/.template}"; }

# Integration line of issue $1 (#480): `main` unless its record declares one (a sub-issue
# inherits its parent's). Reads the records through suivi.py: no git, no glab. A declared line
# whose branch is missing is the caller's refusal, never a fallback to `main`.
sf_line_of() {
    python3 -I "$(dirname "${BASH_SOURCE[0]}")/suivi.py" line "$1"
}

# Permanent worktree of line $1: ../_stokesforge/line-<line> (absolute, not checked for existence).
sf_line_dir() { printf '%s/line-%s\n' "$(realpath -m "$(sf_wt_root)")" "$1"; }

# Lines that have a permanent worktree (the branch each one has checked out), one per line.
sf_lines() {
    local wt
    sf_worktrees | while read -r wt; do
        [ -n "$wt" ] || continue
        case "$(basename "$wt")" in line-*) git -C "$wt" symbolic-ref --short HEAD 2> /dev/null || true ;; esac
    done
}

# Issue number of a worktree, empty if none. The folder name comes first
# (feature<n>, feature<n><letters> for a second worktree of the same issue, so
# feature426b is #426; feature4260 is #4260), the branch feature/<n>-<topic> is the
# fallback. A detached HEAD therefore still yields its issue.
sf_issue_of() {
    local base
    base="$(basename "$1")"
    if [[ "$base" =~ ^feature([0-9]+)[a-z]*$ ]]; then
        printf '%s\n' "${BASH_REMATCH[1]}"
        return 0
    fi
    git -C "$1" symbolic-ref --short HEAD 2> /dev/null | sed -n 's|^feature/\([0-9][0-9]*\)-.*|\1|p'
}

# Worktrees of issue <n>, one path per line.
sf_issue_worktrees() {
    local wt
    sf_worktrees | while read -r wt; do
        [ -n "$wt" ] || continue
        [ "$(sf_issue_of "$wt")" = "$1" ] && printf '%s\n' "$wt" || true
    done
}

# Run "$@" holding the directory lock $1 (mkdir is atomic; waits up to 10 s).
sf_with_lock() {
    local lock="$1" i=0
    shift
    mkdir -p "$(dirname "$lock")"
    until mkdir "$lock" 2> /dev/null; do
        i=$((i + 1))
        [ "$i" -le 100 ] || { echo "lock $lock is held" >&2; return 1; }
        sleep 0.1
    done
    local rc=0
    "$@" || rc=$?
    rmdir "$lock"
    return "$rc"
}

# ---- Resume from an existing branch (#563) ---------------------------------------------------
# A paused work package keeps its branch (local, and pushed to origin); `chantier.sh <n>` and a package's
# `--start` recreate the worktree from it, and scripts/try.sh may do the same offline. Branches are found
# by `feature/<n>-` (never `feature/<n>b-...`, never a recomputed slug: the issue may have been retitled).
# They are read from refs only (local, and origin's remote-tracking refs, fresh after chantier.sh's fetch).

# Branch names of issue $1, local or on origin, one per line (empty: none).
sf_issue_branches() {
    local main
    main="$(sf_main_dir)"
    {
        git -C "$main" for-each-ref --format='%(refname:short)' "refs/heads/feature/$1-*"
        git -C "$main" for-each-ref --format='%(refname:short)' "refs/remotes/origin/feature/$1-*" | sed 's|^origin/||'
    } | sort -u
}

# Where branch $1 lives: local | remote | same | behind (local behind origin) | ahead | diverged.
sf_branch_relation() {
    local main l r
    main="$(sf_main_dir)"
    l="$(git -C "$main" rev-parse -q --verify "refs/heads/$1" || true)"
    r="$(git -C "$main" rev-parse -q --verify "refs/remotes/origin/$1" || true)"
    if [ -z "$l" ]; then echo remote
    elif [ -z "$r" ]; then echo local
    elif [ "$l" = "$r" ]; then echo same
    elif git -C "$main" merge-base --is-ancestor "$l" "$r"; then echo behind
    elif git -C "$main" merge-base --is-ancestor "$r" "$l"; then echo ahead
    else echo diverged
    fi
}

# Succeeds when branch $2 of issue $1 is merged into $3 (origin/main or origin/<line>): every tip of it is an
# ancestor AND a `Merge #<n>:` commit exists there. An empty new branch is an ancestor too, never "merged".
sf_branch_merged() {
    local main ref
    main="$(sf_main_dir)"
    for ref in "refs/heads/$2" "refs/remotes/origin/$2"; do
        git -C "$main" rev-parse -q --verify "$ref" > /dev/null || continue
        git -C "$main" merge-base --is-ancestor "$ref" "$3" || return 1
    done
    [ -n "$(git -C "$main" log --first-parent -n1 --format=%H --grep="^Merge #$1:" "$3")" ]
}

# Recreate the worktree $1 from the existing branch $2, with no claim and no merge (rc 1 and a message when
# local and origin diverged). Local only: the branch as it is; origin only: a local branch from it, no
# upstream; both: the local one, fast-forwarded when behind origin, kept when ahead.
sf_recreate_wt() {
    local wt="$1" b="$2" main rel
    main="$(sf_main_dir)"
    rel="$(sf_branch_relation "$b")"
    case "$rel" in
    diverged) echo "$b and origin/$b have diverged; nothing created (never forced): reconcile them by hand" >&2; return 1 ;;
    remote) git -C "$main" worktree add "$wt" -b "$b" --no-track "origin/$b" || return 1 ;;
    *)
        git -C "$main" worktree add "$wt" "$b" || return 1
        [ "$rel" != behind ] || git -C "$wt" merge -q --ff-only "origin/$b" || return 1 ;;
    esac
}

# Merge $2 (origin/main or origin/<line>) into the checked-out branch of worktree $1, once. Up to date or merged:
# rc 0. A conflict (or any other failure): the merge is aborted, branch head and tree untouched, rc 1, and the
# conflicting files are printed on one line (empty when the merge failed for another reason).
sf_merge_base_in() {
    local wt="$1" base="$2" files
    git -C "$wt" merge-base --is-ancestor "$base" HEAD && return 0
    if git -C "$wt" merge -q --no-edit "$base" > /dev/null 2>&1; then return 0; fi
    files="$(git -C "$wt" diff --name-only --diff-filter=U | paste -sd' ' -)"
    git -C "$wt" merge --abort > /dev/null 2>&1 || true
    printf '%s\n' "$files"
    return 1
}

# ---- Pause and the pause-trial folder (#563) -------------------------------------------------
# A full pause (chantier.sh --pause) removes the worktree; scripts/try.sh may recreate one for a trial; both
# first ask the same question: is this folder safe to delete? One line per reason on stdout, nothing when safe.

# Ignored paths a worktree may carry at removal (build output, caches, copies made by the scripts); anything
# else is refused by name (it may be a private file).
sf_ignored_ok() {
    case "$1" in
    target | target/* | .target-copying | .target-copying/* | dist | dist/* | manual/book | manual/book/*) return 0 ;;
    __pycache__ | __pycache__/* | */__pycache__ | */__pycache__/* ) return 0 ;;
    vendor/python/extracted | vendor/python/extracted/* | vendor/smithay-clipboard/Cargo.lock) return 0 ;;
    esac
    return 1
}

# Processes whose cwd or executable is inside folder $1: "<pid> <name>" lines.
sf_wt_busy() {
    local wt d pid k t
    wt="$(realpath -m "$1")"
    for d in /proc/[0-9]*; do
        pid="${d#/proc/}"
        [ "$pid" != "$$" ] || continue
        for k in cwd exe; do
            t="$(readlink "$d/$k" 2> /dev/null)" || continue
            t="${t% (deleted)}"
            case "$t" in
            "$wt" | "$wt"/*) printf '%s %s\n' "$pid" "$(cat "$d/comm" 2> /dev/null)"; break ;;
            esac
        done
    done
}

# Reasons why worktree $1 cannot be removed now (empty output: it can). Reads git and /proc only.
sf_wt_problems() {
    local wt="$1" gd line p lock held
    if ! git -C "$wt" rev-parse --git-dir > /dev/null 2>&1; then echo "$wt is not a worktree"; return 0; fi
    gd="$(git -C "$wt" rev-parse --absolute-git-dir)"
    git -C "$wt" symbolic-ref -q HEAD > /dev/null 2>&1 || echo "$wt has a detached HEAD"
    for p in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD rebase-merge rebase-apply BISECT_LOG; do
        [ ! -e "$gd/$p" ] || echo "$wt has a merge, rebase or similar in progress ($p)"
    done
    while IFS= read -r line; do
        case "$line" in
        '?? '*) echo "$wt has an untracked file: ${line#?? }" ;;
        '!! '*) sf_ignored_ok "${line#!! }" || echo "$wt has an ignored file outside the allow-list: ${line#!! }" ;;
        '') ;;
        *) echo "$wt has an uncommitted change: ${line:3}" ;;
        esac
    done < <(git -C "$wt" status --porcelain --ignored=matching)
    held=0
    for lock in "$wt"/target/.cargo-lock "$wt"/target/*/.cargo-lock "$wt"/target/*/.cargo-build-lock "$wt"/target/*/.cargo-artifact-lock "$wt"/.target-copying; do
        [ -e "$lock" ] || continue
        flock -n -x "$lock" true 2> /dev/null || held=1
    done
    [ "$held" = 0 ] || echo "$wt has a running cargo build or a running target copy"
    while IFS= read -r line; do
        [ -z "$line" ] || echo "$wt is in use by process $line"
    done < <(sf_wt_busy "$wt")
}

# Marker of a worktree recreated by try.sh for a trial during a pause: a file in its git dir, never tracked.
sf_trial_marker() { printf '%s/sf-pause-trial\n' "$(git -C "$1" rev-parse --absolute-git-dir)"; }

# Remove worktree $1 and its target without waiting on a huge delete: target/ is renamed aside (same disk, outside
# the worktree), the worktree removed (never --force), then the renamed folder deleted. On a refused removal the
# target goes back. A kill between the steps leaves a `.deleting-*` folder that template-target.sh weekly clears.
sf_remove_wt() {
    local wt="$1" main aside=""
    main="$(sf_main_dir)"
    if [ -e "$wt/target" ]; then
        aside="$(dirname "$wt")/.deleting-$(basename "$wt")-$$"
        mv "$wt/target" "$aside" || return 1
    fi
    if ! git -C "$main" worktree remove "$wt" > /dev/null; then
        [ -z "$aside" ] || mv "$aside" "$wt/target"
        return 1
    fi
    [ -z "$aside" ] || rm -rf "$aside"
}

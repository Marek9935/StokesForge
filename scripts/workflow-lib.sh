#!/usr/bin/env bash
# Shared helpers of the parallel-orchestrator workflow (#413). Sourced, not run:
# chantier.sh, try.sh, main-lock.sh, workflow-test.sh.
#
# Layout: the main checkout is the repository root; the worktrees of the work
# packages live in ../_stokesforge/feature<n> (CLAUDE.md, "Worktrees"). A work
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

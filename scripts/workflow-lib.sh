#!/usr/bin/env bash
# Shared helpers of the parallel-orchestrator workflow (#413). Sourced, not run:
# chantier.sh, try.sh, main-lock.sh, workflow-test.sh.
#
# Layout: the main checkout is the repository root; the worktrees of the work
# packages live in ../_stokesforge/feature<n> (CLAUDE.md, "Worktrees"). A work
# package's colour is a file in the worktree's git dir
# (.git/worktrees/feature<n>/stokesforge-colour): `git worktree remove` or
# `prune` deletes it, which frees the colour.

SF_COLOURS=(🟦 🟩 🟧 🟪 🟥 🟨)
# The To-test page (artifact with a database), shared by every session.
SF_TO_TEST_URL="https://claude.ai/artifact/UsyyHWZc5cj1enWrRr7yEJ"

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

sf_colour_file() { printf '%s/stokesforge-colour\n' "$(git -C "$1" rev-parse --absolute-git-dir)"; }

# Colour of a worktree, empty if none.
sf_colour_of() {
    local f
    f="$(sf_colour_file "$1")"
    [ -f "$f" ] && cat "$f" || true
}

# Colours held by the live worktrees, one per line.
sf_used_colours() {
    local wt
    sf_worktrees | while read -r wt; do sf_colour_of "$wt"; done | sed '/^$/d'
}

# First colour of SF_COLOURS that nobody holds; empty if all six are taken.
sf_first_free() {
    local used c
    used="$(sf_used_colours)"
    for c in "${SF_COLOURS[@]}"; do
        printf '%s\n' "$used" | grep -xF "$c" > /dev/null || { printf '%s\n' "$c"; return 0; }
    done
}

# Issue number of a worktree (from its branch feature/<n>-<topic>), empty if none.
sf_issue_of() {
    git -C "$1" symbolic-ref --short HEAD 2> /dev/null | sed -n 's|^feature/\([0-9][0-9]*\)-.*|\1|p'
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

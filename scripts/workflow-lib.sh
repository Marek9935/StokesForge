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

# Work packages with sub-issues (#419): ../_stokesforge/.packages/<parent> holds
# the package colour on line 1, then one "<n><TAB><title>" line per sub-issue.
# The colour stays reserved between two sub-issues, until `chantier.sh --done`.
# An empty line 1 means the package is PAUSED (`chantier.sh --pause`, #441): it
# keeps its sub-issue list but holds no colour. The file's mtime is the "since"
# date of the colour (or of the pause).
sf_pkg_dir() { printf '%s/.packages\n' "$(sf_wt_root)"; }

# Colour reserved for package <parent>, empty if none or paused.
sf_pkg_colour() {
    local f
    f="$(sf_pkg_dir)/$1"
    [ -f "$f" ] && head -n 1 "$f" || true
}

# Write package file $1 with colour $2 (may be empty = paused) and the lines of
# file $3. Skipped when nothing changes; when the colour is unchanged the mtime
# ("since") is kept even if the sub-issue list changed.
sf_pkg_write() {
    local f="$1" colour="$2" list="$3" new ref
    new="$(mktemp)"
    { printf '%s\n' "$colour"; cat "$list"; } > "$new"
    if [ -f "$f" ] && cmp -s "$f" "$new"; then rm -f "$new"; return 0; fi
    mkdir -p "$(dirname "$f")"
    if [ -f "$f" ] && [ -n "$colour" ] && [ "$(head -n 1 "$f")" = "$colour" ]; then
        ref="$(mktemp)"
        touch -r "$f" "$ref"
        cat "$new" > "$f"
        touch -r "$ref" "$f"
        rm -f "$ref"
    else
        cat "$new" > "$f"
    fi
    rm -f "$new"
}

# Reserve (or keep) the colour of package <parent>; $2 = file of "<n><TAB><title>"
# lines. Prints the colour. Call with the colour lock held. A paused package gets
# the first free colour again.
sf_pkg_reserve() {
    local parent="$1" list="$2" colour
    colour="$(sf_pkg_colour "$parent")"
    [ -n "$colour" ] || colour="$(sf_first_free)"
    [ -n "$colour" ] || return 1
    sf_pkg_write "$(sf_pkg_dir)/$parent" "$colour" "$list"
    printf '%s\n' "$colour"
}

# Parents of the package files, paused ones included, one per line.
sf_pkgs() {
    local f
    for f in "$(sf_pkg_dir)"/*; do
        if [ -f "$f" ]; then basename "$f"; fi
    done
}

# Members of package <parent> as " <parent> <sub> <sub> " (for a `case` test).
sf_pkg_members() {
    printf ' %s %s \n' "$1" "$(tail -n +2 "$(sf_pkg_dir)/$1" | cut -f1 | tr '\n' ' ')"
}

# Paths of the worktrees that belong to package <parent> (itself or a sub-issue).
sf_pkg_worktrees() {
    local members wt i
    members="$(sf_pkg_members "$1")"
    sf_worktrees | while read -r wt; do
        [ -n "$wt" ] || continue
        i="$(sf_issue_of "$wt")"
        case "$members" in *" $i "*) printf '%s\n' "$wt" ;; esac
    done
}

# Colours held by the live worktrees and the reserved (not paused) packages, one per line.
sf_used_colours() {
    local wt p
    sf_worktrees | while read -r wt; do sf_colour_of "$wt"; done | sed '/^$/d'
    sf_pkgs | while read -r p; do sf_pkg_colour "$p"; done | sed '/^$/d'
}

# First colour of SF_COLOURS that nobody holds; empty if all six are taken.
sf_first_free() {
    local used c
    used="$(sf_used_colours)"
    for c in "${SF_COLOURS[@]}"; do
        printf '%s\n' "$used" | grep -xF "$c" > /dev/null || { printf '%s\n' "$c"; return 0; }
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

# Colour already held by a worktree of issue <n>, empty if none.
sf_issue_colour() {
    local wt c
    while read -r wt; do
        [ -n "$wt" ] || continue
        c="$(sf_colour_of "$wt")"
        if [ -n "$c" ]; then printf '%s\n' "$c"; return 0; fi
    done < <(sf_issue_worktrees "$1")
}

# Colour issue <n> should have: that of worktree $3 (if given), else one already
# held by a worktree of the issue, else the package colour $2, else the first free.
sf_colour_for_issue() {
    local n="$1" pkg="${2:-}" own="" c=""
    [ -z "${3:-}" ] || own="$(sf_colour_of "$3")"
    c="${own:-$(sf_issue_colour "$n")}"
    c="${c:-$pkg}"
    c="${c:-$(sf_first_free)}"
    printf '%s\n' "$c"
}

# Write colour $2 to every colourless worktree of issue <n>. Colour lock held.
sf_colour_spread() {
    local wt
    while read -r wt; do
        [ -n "$wt" ] || continue
        [ -n "$(sf_colour_of "$wt")" ] || printf '%s\n' "$2" > "$(sf_colour_file "$wt")"
    done < <(sf_issue_worktrees "$1")
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

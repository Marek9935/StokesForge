#!/usr/bin/env bash
# Open a branch on the user's screen, from the main checkout (#413, #417). Its value
# is for a branch (feature/<n>); for `main`, `cargo run -p stokesforge-app` is the
# same. The orchestrator runs it itself at the trial of a visible change.
#
#   scripts/try.sh [--dry-run] <issue-number>|main
#
# Finds the worktree of branch feature/<n>-*, prints what is new (the last merges
# on main, then the commits of the branch), builds `stokesforge-app` with the
# worktree's own target folder and runs it in native Wayland (WAYLAND_DISPLAY
# kept, STOKESFORGE_X11 unset) on the user's own preferences (no separate config
# folder) with the demo project open (STOKESFORGE_DEMO=1, as the run-stokesforge
# driver does). `main` tries the main checkout itself. The user's OK on this
# trial is the go-ahead to merge a visible change.
# --dry-run prints the build and run commands without running them.
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

dry=0
target=""
for a in "$@"; do
    case "$a" in
    --dry-run) dry=1 ;;
    -h | --help) sed -n '2,18p' "$0"; exit 0 ;;
    *) target="$a" ;;
    esac
done
[ -n "$target" ] || { echo "usage: try.sh [--dry-run] <issue-number>|main  (opens a branch on the screen; for main, cargo run -p stokesforge-app is the same)" >&2; exit 2; }

main="$(sf_main_dir)"
if [ "$target" = main ]; then
    wt="$main"
else
    wt=""
    cur=""
    while read -r line; do
        case "$line" in
        "worktree "*) cur="${line#worktree }" ;;
        "branch refs/heads/feature/$target-"*) wt="$cur" ;;
        esac
    done < <(git -C "$main" worktree list --porcelain)
    if [ -z "$wt" ]; then
        echo "No worktree for issue #$target. Work already merged? Try: scripts/try.sh main" >&2
        exit 1
    fi
fi

echo "== last merges on main"
git -C "$main" log --first-parent --merges -5 --format='%h %s' main
if [ "$target" != main ]; then
    echo "== commits of $(git -C "$wt" symbolic-ref --short HEAD) not yet on main"
    git -C "$wt" log --format='%h %s' main..HEAD
fi

export CARGO_TARGET_DIR="$wt/target"
build=(cargo build -p stokesforge-app --manifest-path "$wt/Cargo.toml")
run=("$wt/target/debug/stokesforge-app")
if [ "$dry" = 1 ]; then
    echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
    printf 'would build:'; printf ' %q' "${build[@]}"; printf '\n'
    printf 'would run:'; printf ' %q' "${run[@]}"; printf '\n'
    exit 0
fi
"${build[@]}"
unset STOKESFORGE_X11
export STOKESFORGE_DEMO=1
exec "${run[@]}"

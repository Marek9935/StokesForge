#!/usr/bin/env bash
# Open a branch on the user's screen, from the main checkout (#413, #417). Its value
# is for a branch (feature/<n>); for `main`, `cargo run -p stokesforge-app` is the
# same. The orchestrator runs it itself at the trial of a visible change.
#
#   scripts/try.sh [--dry-run] [--release] <issue-number>|main
#
# Finds the worktree of branch feature/<n>-*, prints what is new (the last merges
# on main, then the commits of the branch), builds `stokesforge-app` with the
# worktree's own target folder and runs it in native Wayland (WAYLAND_DISPLAY
# kept, STOKESFORGE_X11 unset) on the user's own preferences (no separate config
# folder) with the demo project open (STOKESFORGE_DEMO=1, as the run-stokesforge
# driver does). `main` tries the main checkout itself. The user's OK on this
# trial is the go-ahead to merge a visible change.
# --dry-run prints the build and run commands without running them.
# --release (#474) builds with `cargo build --release` and runs the release binary of the
# same worktree (debug stays the default, for its build time; debug is much slower to draw).
# An issue declared on an integration line (#480, suivi.py line <n>) is read against that line:
# the last merges are those of the line and "not yet on" names the line (`try.sh main` unchanged).
# The build skips the cargo queue (#476): a trial is for the user at the PC, who must not wait
# behind agent builds (SF_CARGO_QUEUE=off on the build command only; the app itself is unchanged).
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

dry=0
release=0
target=""
for a in "$@"; do
    case "$a" in
    --dry-run) dry=1 ;;
    --release) release=1 ;;
    -h | --help) sed -n '2,20p' "$0"; exit 0 ;;
    *) target="$a" ;;
    esac
done
[ -n "$target" ] || { echo "usage: try.sh [--dry-run] [--release] <issue-number>|main  (opens a branch on the screen; for main, cargo run -p stokesforge-app is the same)" >&2; exit 2; }

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

line=main
[ "$target" = main ] || line="$(sf_line_of "$target")"
if [ "$line" != main ] && ! git -C "$main" rev-parse --verify -q "refs/heads/$line" > /dev/null; then
    echo "#$target is on the line $line, but the branch $line does not exist here (scripts/line.sh create $line)" >&2
    exit 1
fi
echo "== last merges on $line"
git -C "$main" log --first-parent --merges -5 --format='%h %s' "$line"
if [ "$target" != main ]; then
    echo "== commits of $(git -C "$wt" symbolic-ref --short HEAD) not yet on $line"
    git -C "$wt" log --format='%h %s' "$line..HEAD"
fi

export CARGO_TARGET_DIR="$wt/target"
if [ "$release" = 1 ]; then
    build=(env SF_CARGO_QUEUE=off cargo build --release -p stokesforge-app --manifest-path "$wt/Cargo.toml")
    run=("$wt/target/release/stokesforge-app")
else
    build=(env SF_CARGO_QUEUE=off cargo build -p stokesforge-app --manifest-path "$wt/Cargo.toml")
    run=("$wt/target/debug/stokesforge-app")
fi
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

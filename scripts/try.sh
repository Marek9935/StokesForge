#!/usr/bin/env bash
# Open a branch on the user's screen, from the main checkout (#413, #417). Its value
# is for a branch (feature/<n>); for `main`, `cargo run -p stokesforge-app` is the
# same. The orchestrator runs it itself at the trial of a visible change.
#
#   scripts/try.sh [--dry-run] [--release] <issue-number>|main|<line> [<file.stf>]
#
# Finds the worktree of branch feature/<n>-*, prints what is new (the last merges
# on main, then the commits of the branch), builds `stokesforge-app` with the
# worktree's own target folder and runs it in native Wayland (WAYLAND_DISPLAY
# kept, STOKESFORGE_X11 unset) on the user's own preferences (no separate config
# folder) with the demo project open (STOKESFORGE_DEMO=1, as the run-stokesforge
# driver does). `main` tries the main checkout itself. The user's OK on this
# trial is the go-ahead to merge a visible change.
# <file.stf> (#518) opens that project instead of the demo: a fresh copy in a scratch folder at every run
# (the original is never written); a missing file or another extension is refused.
# --dry-run prints the build and run commands without running them.
# --release (#474) builds with `cargo build --release` and runs the release binary of the
# same worktree (debug stays the default, for its build time; debug is much slower to draw).
# A line name (e.g. `PostV1.1`, #480) tries the whole line: it builds and opens the permanent line
# worktree ../_stokesforge/line-<line> like `main` opens the main checkout; the log is the line's
# commits not yet on main. The user's OK on it is recorded as trial entries of the issue named in
# scripts/line.sh (LINE_TRIAL_ISSUE) whose try command is `scripts/try.sh <line>`.
# An issue declared on an integration line (#480, suivi.py line <n>) is read against that line:
# the last merges are those of the line and "not yet on" names the line (`try.sh main` unchanged).
# A PAUSED issue without a folder (#563, chantier.sh --pause): the folder is recreated here from the LOCAL branch (no
# network, no fetch, no merge of main, no claim; a branch known only by origin's last-fetched ref is used as it is),
# the template target copied when possible, then built and opened. A marker in the folder's git dir names it a
# pause-trial folder. When the app exits, `chantier.sh --tidy-trial <n>` removes it if every trial entry of the issue
# is answered (OK or retour) and nothing changed there; otherwise it stays, and the service tries again at the last
# answer. --dry-run creates nothing. An unpaused issue without a folder keeps the message below.
# The build skips the cargo queue (#476): a trial is for the user at the PC, who must not wait
# behind agent builds (SF_CARGO_QUEUE=off on the build command only; the app itself is unchanged).
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

dry=0
release=0
target=""
file=""
for a in "$@"; do
    case "$a" in
    --dry-run) dry=1 ;;
    --release) release=1 ;;
    -h | --help) sed -n '2,20p' "$0"; exit 0 ;;
    *) if [ -z "$target" ]; then target="$a"; elif [ -z "$file" ]; then file="$a"; else echo "try.sh: too many arguments" >&2; exit 2; fi ;;
    esac
done
[ -n "$target" ] || { echo "usage: try.sh [--dry-run] [--release] <issue-number>|main [<file.stf>]  (opens a branch on the screen; for main, cargo run -p stokesforge-app is the same)" >&2; exit 2; }
if [ -n "$file" ]; then
    [ -f "$file" ] || { echo "try.sh: no such file: $file" >&2; exit 1; }
    case "$file" in *.stf) ;; *) echo "try.sh: not a .stf file: $file" >&2; exit 1 ;; esac
fi

main="$(sf_main_dir)"
recreated=""
line=main
if [ "$target" = main ]; then
    wt="$main"
elif [[ "$target" =~ ^[A-Za-z][A-Za-z0-9._-]*$ ]]; then
    # a line name: its permanent worktree, never a fallback to another checkout
    line="$target"
    wt="$(sf_line_dir "$line")"
    if [ "$(git -C "$wt" symbolic-ref --short HEAD 2> /dev/null || true)" != "$line" ]; then
        echo "No worktree $wt on the line $line (scripts/line.sh create $line)." >&2
        exit 1
    fi
else
    wt=""
    cur=""
    while read -r entry; do
        case "$entry" in
        "worktree "*) cur="${entry#worktree }" ;;
        "branch refs/heads/feature/$target-"*) wt="$cur" ;;
        esac
    done < <(git -C "$main" worktree list --porcelain)
    if [ -z "$wt" ] && [[ "$target" =~ ^[0-9]+$ ]] && python3 -I "$(dirname "$0")/suivi.py" paused "$target" 2> /dev/null; then
        # #563: a paused issue: the folder comes back from the local branch, nothing else is done to it
        pbranch="$(sf_issue_branches "$target")"
        case "$pbranch" in
        '') echo "#$target is paused but has no branch feature/$target-* (local or origin's last-fetched); nothing created" >&2; exit 1 ;;
        *$'\n'*) echo "#$target has several branches ($(printf '%s' "$pbranch" | paste -sd' ' -)); nothing created" >&2; exit 1 ;;
        esac
        wt="$(realpath -m "$(sf_wt_root)")/feature$target"
        if [ "$dry" = 1 ]; then
            echo "would recreate $wt from the local branch $pbranch (no network, no merge of main, no claim), copy the template target, build, run, then tidy the folder when every entry is answered"
            recreated=dry
        else
            sf_recreate_wt "$wt" "$pbranch" || exit 1
            : > "$(sf_trial_marker "$wt")"
            "$(dirname "$0")/template-target.sh" copy "$wt" 2>&1 | tail -n 1 || true
            recreated=1
        fi
    elif [ -z "$wt" ]; then
        echo "No worktree for issue #$target. Work already merged? Try: scripts/try.sh main" >&2
        exit 1
    fi
fi

# A plain number or a suffixed sibling (488b): the line of its issue number.
issue=""
[[ "$target" =~ ^[0-9]+[A-Za-z]*$ ]] && issue="${target%%[!0-9]*}"
[ -z "$issue" ] || line="$(sf_line_of "$issue")"
if [ "$line" != main ] && [ -n "$issue" ] && ! git -C "$main" rev-parse --verify -q "refs/heads/$line" > /dev/null; then
    echo "#$target is on the line $line, but the branch $line does not exist here (scripts/line.sh create $line)" >&2
    exit 1
fi
echo "== last merges on $line"
git -C "$main" log --first-parent --merges -5 --format='%h %s' "$line"
if [ "$target" != main ] && [ "$recreated" != dry ]; then
    ahead="$line"
    [ "$target" != "$line" ] || ahead=main    # a line is compared with main
    echo "== commits of $(git -C "$wt" symbolic-ref --short HEAD) not yet on $ahead"
    git -C "$wt" log --format='%h %s' "$ahead..HEAD"
fi

export CARGO_TARGET_DIR="$wt/target"
if [ "$release" = 1 ]; then
    build=(env SF_CARGO_QUEUE=off cargo build --release -p stokesforge-app --manifest-path "$wt/Cargo.toml")
    run=("$wt/target/release/stokesforge-app")
else
    build=(env SF_CARGO_QUEUE=off cargo build -p stokesforge-app --manifest-path "$wt/Cargo.toml")
    run=("$wt/target/debug/stokesforge-app")
fi
copy=""
if [ -n "$file" ]; then
    if [ "$dry" = 1 ]; then
        copy="${TMPDIR:-/tmp}/stokesforge-try.XXXXXX/$(basename "$file")"
    else
        copy="$(mktemp -d "${TMPDIR:-/tmp}/stokesforge-try.XXXXXX")/$(basename "$file")"
        cp -- "$file" "$copy"
    fi
    run+=("$copy")
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
# A folder marked as a pause-trial folder (#563): the app runs as a child, the cleanup follows its exit.
if [ -n "$issue" ] && [ -d "$wt" ] && [ -f "$(sf_trial_marker "$wt" 2> /dev/null || echo /nonexistent)" ]; then
    rc=0
    "${run[@]}" || rc=$?
    "$(dirname "$0")/chantier.sh" --tidy-trial "$issue" || echo "try: the trial folder $wt stays for now (the page says why when a file changed)" >&2
    exit "$rc"
fi
exec "${run[@]}"

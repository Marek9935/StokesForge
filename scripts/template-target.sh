#!/usr/bin/env bash
# Template target (#523): a build of `main` that every new worktree starts from, kept current
# after each merge, never rebuilt cold for nothing.
#
#   scripts/template-target.sh refresh        warm incremental build of origin/main, then sweep
#   scripts/template-target.sh copy <worktree> copy the template target into a NEW worktree
#   scripts/template-target.sh prune           empty the incremental/release of paused worktrees
#   scripts/template-target.sh weekly          prune, then a refresh if the template is oversize
#   scripts/template-target.sh status
#
# Layout (../_stokesforge/.template, SF_TEMPLATE_DIR): src/ is `git archive origin/main`, NOT a git
# worktree (a worktree would show in the colour lists); target/ its target dir; stamp = commit +
# `rustc -vV`; log/ the JSON of the last refresh; lock = flock file (refresh exclusive, copy shared
# and never waiting); pending = a refresh asked while one ran.
#
# refresh. src/ is synced by content (rsync --checksum --delete), so unchanged sources keep their mtime
# and cargo rebuilds only what the merge changed. The five commands are exactly the ones the agents
# run (same feature sets), all through the cargo queue shim in the `low` class (2 jobs, last, never in
# background mode). Their JSON messages list every artifact built or found fresh; scripts/template-sweep.py
# then deletes the rest of target/ (stale deps, fingerprints, build dirs) and `incremental/`.
# `cargo clean` + a cold rebuild happen only when the toolchain changed (stamp) or the target is
# still above SF_TEMPLATE_MAX_KB (default 80 GB) after a sweep. A refresh asked while one runs
# writes `pending` and exits 0; the running one does one more pass. Refuses without the installed shim.
#
# copy. Only for a worktree that has no target/ (chantier.sh calls it right after creating one).
# `cp -a` of debug and release, NEVER hard links (cargo rewrites fingerprints and outputs in place).
# Skipped, with one line saying why, when there is no template, a refresh holds the lock (never
# waited for), or less than 50 GB would remain free (SF_DF_AVAIL_KB fixes the free space in tests).
#
# prune. Worktrees paused for SF_TEMPLATE_PRUNE_DAYS (default 3) days or more lose
# target/debug/incremental and target/release (deps kept). Never a held issue, a line-* worktree,
# a worktree without a record, or one whose cargo lock files are held. The list comes from
# `chantier.sh --status --json` (SF_TEMPLATE_STATUS_JSON: a file with that JSON, in tests).
#
# Seams: SF_TEMPLATE_DIR, SF_TEMPLATE_CARGO (the shim), SF_TEMPLATE_REF (default origin/main,
# else main), SF_TEMPLATE_MAX_KB, SF_TEMPLATE_MIN_FREE_KB (default 50 GB), SF_DF_AVAIL_KB.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$here/workflow-lib.sh"

T="$(sf_template_dir)"
usage="usage: template-target.sh refresh | copy <worktree> | prune | weekly | status"
say() { echo "template: $*"; }
max_kb="${SF_TEMPLATE_MAX_KB:-83886080}"        # 80 GB
min_free_kb="${SF_TEMPLATE_MIN_FREE_KB:-52428800}"   # 50 GB

# The five refresh commands (cargo arguments), as the agents run them plus the JSON message format.
REFRESH_CMDS=(
    "nextest run --workspace --no-run --cargo-message-format json"
    "test --workspace --doc --no-run --message-format json"
    "clippy --workspace --all-targets --message-format json"
    "build -p stokesforge-app --message-format json"
    "test -p stokesforge-app --release --lib --no-run --message-format json"
)

shim() { printf '%s\n' "${SF_TEMPLATE_CARGO:-$HOME/.local/share/stokesforge/cargo-queue/bin/cargo}"; }
size_kb() { du -sk "$1" | cut -f1; }
ref() {
    if [ -n "${SF_TEMPLATE_REF:-}" ]; then printf '%s\n' "$SF_TEMPLATE_REF"
    elif git -C "$(sf_main_dir)" rev-parse --verify -q origin/main > /dev/null; then echo origin/main
    else echo main; fi
}

build_all() {   # run the five commands; the JSON of each in log/<i>.json
    local i=0 c cargo
    cargo="$(shim)"
    rm -rf "$T/log"; mkdir -p "$T/log"
    for c in "${REFRESH_CMDS[@]}"; do
        i=$((i + 1))
        # shellcheck disable=SC2086
        ( cd "$T/src" && CARGO_TARGET_DIR="$T/target" SF_CARGO_QUEUE_CLASS=low "$cargo" $c \
            > "$T/log/$i.json" 2> "$T/log/$i.err" ) || { say "command $i failed: cargo $c (see $T/log/$i.err)"; return 1; }
    done
}

sweep() { python3 -I "$here/template-sweep.py" "$T/target" "$T"/log/*.json; }

refresh_once() {
    local commit tmp want old
    commit="$(git -C "$(sf_main_dir)" rev-parse "$(ref)")"
    tmp="$(mktemp -d "$T/sync.XXXXXX")"
    git -C "$(sf_main_dir)" archive "$commit" | tar -x -C "$tmp"
    mkdir -p "$T/src"
    rsync -rlpc --delete "$tmp/" "$T/src/"
    rm -rf "$tmp"
    want="$(cd "$T/src" && rustc -vV)"
    if [ -d "$T/target" ]; then
        old="$(tail -n +2 "$T/stamp" 2> /dev/null || true)"
        if [ "$old" != "$want" ]; then
            say "toolchain changed (or no stamp): cargo clean, cold rebuild"
            ( cd "$T/src" && CARGO_TARGET_DIR="$T/target" "$(shim)" clean )
        fi
    fi
    say "refresh of $(echo "$commit" | cut -c1-8) (warm, class low)"
    build_all || return 1
    sweep || return 1
    if [ "$(size_kb "$T/target")" -gt "$max_kb" ]; then
        say "target above $((max_kb / 1024 / 1024)) GB after the sweep: cargo clean, cold rebuild"
        ( cd "$T/src" && CARGO_TARGET_DIR="$T/target" "$(shim)" clean )
        build_all || return 1
        sweep || return 1
        if [ "$(size_kb "$T/target")" -gt "$max_kb" ]; then
            say "WARNING: still above the threshold after a clean build; raise SF_TEMPLATE_MAX_KB or free disk"
        fi
    fi
    printf '%s\n%s\n' "$commit" "$want" > "$T/stamp"
    say "ready at $(echo "$commit" | cut -c1-8), $(( $(size_kb "$T/target") / 1024 / 1024 )) GB"
}

cmd_refresh() {
    [ -x "$(shim)" ] || { say "the cargo queue shim is not installed ($(shim)); run scripts/cargo-queue-install.sh first" >&2; return 1; }
    mkdir -p "$T"
    exec 9> "$T/lock"
    if ! flock -n -x 9; then
        : > "$T/pending"
        if ! flock -n -x 9; then say "a refresh is running; it will do one more pass"; return 0; fi
    fi
    export PATH="$HOME/.cargo/bin:$PATH"       # a timer or a detached shell has no cargo of its own
    local rc=0
    while :; do
        rm -f "$T/pending"
        refresh_once || { rc=$?; break; }
        [ -e "$T/pending" ] || break
        say "a refresh was asked meanwhile: one more pass"
    done
    return "$rc"
}

cmd_copy() {
    local wt="${1:-}" tsize avail remain
    [ -n "$wt" ] && [ -d "$wt" ] || { echo "$usage" >&2; return 2; }
    if [ ! -d "$T/target" ] || [ ! -f "$T/stamp" ]; then
        say "none yet (scripts/template-target.sh refresh builds it): the first build in $wt is cold"; return 0
    fi
    if [ -e "$wt/target" ]; then say "skipped: $wt already has a target/"; return 0; fi
    exec 9>> "$T/lock"
    if ! flock -n -s 9; then say "skipped: a refresh holds the template, the first build in $wt is cold"; return 0; fi
    tsize="$(size_kb "$T/target")"
    avail="${SF_DF_AVAIL_KB:-$(df --output=avail -k "$wt" | tail -n 1 | tr -d ' ')}"
    remain=$((avail - tsize))
    if [ "$remain" -lt "$min_free_kb" ]; then
        say "skipped: only $((remain / 1024 / 1024)) GB would stay free (minimum $((min_free_kb / 1024 / 1024)) GB), the first build in $wt is cold"
        return 0
    fi
    rm -rf "$wt/.target-copying"
    if cp -a "$T/target" "$wt/.target-copying" && mv "$wt/.target-copying" "$wt/target"; then
        say "copied the target of $(head -n 1 "$T/stamp" | cut -c1-8) ($((tsize / 1024 / 1024)) GB, debug and release) into $wt/target"
    else
        rm -rf "$wt/.target-copying"
        say "the copy into $wt failed; the first build there is cold"
    fi
}

cmd_prune() {
    local json days name wt p held lock status_json
    days="${SF_TEMPLATE_PRUNE_DAYS:-3}"
    if [ -n "${SF_TEMPLATE_STATUS_JSON:-}" ]; then status_json="$(cat "$SF_TEMPLATE_STATUS_JSON")"
    else status_json="$(cd "$(sf_main_dir)" && "$here/chantier.sh" --status --json)"; fi
    json="$(printf '%s' "$status_json" | python3 -I -c '
import json, sys
d = json.load(sys.stdin)
days = int(sys.argv[1])
for p in d.get("paused", []):
    if p.get("days", 0) >= days:
        for w in p.get("worktrees", []):
            if not w.startswith("line-"):
                print(w)
' "$days")"
    local root
    root="$(realpath -m "$(sf_wt_root)")"
    while IFS= read -r name; do
        [ -n "$name" ] || continue
        wt="$root/$name"
        [ -d "$wt/target" ] || continue
        held=0
        for lock in "$wt"/target/.cargo-lock "$wt"/target/*/.cargo-lock "$wt"/target/*/.cargo-build-lock "$wt"/target/*/.cargo-artifact-lock; do
            [ -e "$lock" ] || continue
            if ! flock -n -x "$lock" true 2> /dev/null; then held=1; fi
        done
        if [ "$held" = 1 ]; then say "kept $name: a cargo build is running there"; continue; fi
        for p in "$wt"/target/debug/incremental "$wt"/target/release; do
            [ -e "$p" ] || continue
            rm -rf "$p"
        done
        say "pruned $name (paused $days days or more): incremental and release removed"
    done <<< "$json"
}

cmd_weekly() {
    cmd_prune || true
    if [ -d "$T/target" ] && [ "$(size_kb "$T/target")" -gt "$max_kb" ]; then
        say "template above the threshold: refresh"
        cmd_refresh
    fi
}

cmd_status() {
    if [ ! -d "$T" ]; then say "none ($T)"; return 0; fi
    say "dir $T"
    [ -f "$T/stamp" ] && say "commit $(head -n 1 "$T/stamp"), $(sed -n 's/^release: //p' "$T/stamp"), $(( $(size_kb "$T/target") / 1024 / 1024 )) GB (threshold $((max_kb / 1024 / 1024)) GB)" \
        || say "no stamp (never built, or a refresh is in progress)"
    if [ -f "$T/lock" ] && ! flock -n -x "$T/lock" true; then say "a refresh is running"; fi
    [ ! -e "$T/pending" ] || say "a refresh is pending"
}

case "${1:-}" in
refresh) cmd_refresh ;;
copy) shift; cmd_copy "${1:-}" ;;
prune) cmd_prune ;;
weekly) cmd_weekly ;;
status) cmd_status ;;
*) echo "$usage" >&2; exit 2 ;;
esac

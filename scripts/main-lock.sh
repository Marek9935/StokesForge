#!/usr/bin/env bash
# The lock on `main`: one orchestrator at a time merges and pushes.
#
#   scripts/main-lock.sh take <name> [--dry-run]   # name = "<square> #<n>"
#   scripts/main-lock.sh release [--dry-run]
#   scripts/main-lock.sh status
#
# The lock is the directory ../_stokesforge/.main.lock (mkdir is atomic) with an
# `owner` file (name and date). It is never broken automatically: flock cannot
# span two Bash tool calls, and a stale lock is a decision for a human or for the
# orchestrator that reads `status`. Sequence: take, merge main into the branch,
# recheck, `--no-ff` merge, push, release (stokesforge-verification, step 6).
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

lock="$(sf_wt_root)/.main.lock"
cmd="${1:-status}"
dry=0
for a in "$@"; do [ "$a" = "--dry-run" ] && dry=1; done

case "$cmd" in
take)
    name="${2:-}"
    [ -n "$name" ] && [ "$name" != "--dry-run" ] || { echo "usage: main-lock.sh take <name>" >&2; exit 2; }
    if [ -d "$lock" ]; then
        echo "main is locked by: $(cat "$lock/owner" 2> /dev/null || echo unknown)" >&2
        exit 1
    fi
    if [ "$dry" = 1 ]; then echo "would take $lock for $name"; exit 0; fi
    mkdir -p "$(dirname "$lock")"
    mkdir "$lock" 2> /dev/null || { echo "main is locked by: $(cat "$lock/owner" 2> /dev/null || echo unknown)" >&2; exit 1; }
    printf '%s %s\n' "$name" "$(date -Iseconds)" > "$lock/owner"
    echo "main lock taken by $name"
    ;;
release)
    if [ "$dry" = 1 ]; then echo "would release $lock"; exit 0; fi
    rm -f "$lock/owner"
    rmdir "$lock" 2> /dev/null || true
    echo "main lock released"
    ;;
status)
    if [ -d "$lock" ]; then echo "locked: $(cat "$lock/owner" 2> /dev/null || echo unknown)"; else echo "free"; fi
    ;;
*)
    echo "usage: main-lock.sh take <name> | release | status" >&2
    exit 2
    ;;
esac

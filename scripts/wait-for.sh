#!/usr/bin/env bash
# Wait until a log has a line matching a regex, without a polling loop (#517).
#
#   scripts/wait-for.sh [--pid PID] [--lines N] <log> <regex> [timeout_s]
#
# Reads the log from its first line (`tail -n +1 -F`, so a log not created yet is waited for), stops at the
# first line matching the ERE (`grep -E -m1`), kills the tail, and prints on every exit either
# `wait-for: matched after Ns: <line>` or `wait-for: no match ... after Ns`, then the last N lines (default 20).
# Timeout: default 540 s; above 590 s it is clamped (a Bash tool call is cut at 600 s) with a note.
# Exit: 0 match, 1 timeout, 2 usage / bad regex / bad number, 3 the --pid process ended with no match.
# Run it with a Bash tool timeout of 600000 ms. No pgrep, no sleep.
set -u

usage() {
    echo "usage: wait-for.sh [--pid PID] [--lines N] <log> <regex> [timeout_s]" >&2
    exit 2
}

pid="" lines=20
while [ $# -gt 0 ]; do
    case "$1" in
    --pid) [ $# -ge 2 ] || usage; pid=$2; shift 2 ;;
    --lines) [ $# -ge 2 ] || usage; lines=$2; shift 2 ;;
    --) shift; break ;;
    -*) if [ $# -ge 2 ]; then break; fi; usage ;;   # a regex may start with a dash only after the log
    *) break ;;
    esac
done
[ $# -ge 2 ] && [ $# -le 3 ] || usage
log=$1 re=$2 limit=${3:-540}
is_num() { case "$1" in '' | *[!0-9]*) return 1 ;; esac; }
is_num "$limit" && [ "$limit" -ge 1 ] || { echo "wait-for: bad timeout '$limit'" >&2; exit 2; }
is_num "$lines" || { echo "wait-for: bad --lines '$lines'" >&2; exit 2; }
if [ -n "$pid" ]; then is_num "$pid" || { echo "wait-for: bad --pid '$pid'" >&2; exit 2; }; fi
grep -E -- "$re" </dev/null >/dev/null 2>&1
[ $? -eq 2 ] && { echo "wait-for: bad regex '$re'" >&2; exit 2; }
if [ "$limit" -gt 590 ]; then
    echo "wait-for: timeout $limit s clamped to 590 s" >&2
    limit=590
fi

tailpid=""
cleanup() { [ -n "$tailpid" ] && kill "$tailpid" 2>/dev/null; }
trap cleanup EXIT
trap 'exit 130' INT TERM

pidopt=()
[ -n "$pid" ] && pidopt=(--pid="$pid")
exec {fd}< <(exec tail "${pidopt[@]}" -s 0.2 -n +1 -F -- "$log" 2>/dev/null)
tailpid=$!

line=$(timeout "$limit" grep -E -m1 -- "$re" <&"$fd")
rc=$?
cleanup
tailpid=""

show_tail() { [ "$lines" -gt 0 ] && [ -f "$log" ] && tail -n "$lines" -- "$log"; return 0; }
case $rc in
0)
    echo "wait-for: matched after ${SECONDS}s: $line"
    show_tail
    exit 0
    ;;
124)
    echo "wait-for: no match for '$re' in $log after ${SECONDS}s (timeout $limit s)"
    show_tail
    exit 1
    ;;
*)
    if [ -n "$pid" ]; then
        echo "wait-for: no match for '$re' in $log after ${SECONDS}s (process $pid ended)"
        show_tail
        exit 3
    fi
    echo "wait-for: no match for '$re' in $log after ${SECONDS}s (log reading stopped)"
    show_tail
    exit 1
    ;;
esac

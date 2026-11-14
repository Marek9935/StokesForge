#!/usr/bin/env bash
# Tests of scripts/wait-for.sh (#517). Run: bash scripts/wait-for-test.sh
set -u
here=$(cd "$(dirname "$0")" && pwd)
wf="$here/wait-for.sh"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/wait-for-test.XXXXXX")
bgpids=()
trap 'for p in "${bgpids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done; rm -rf "$tmp"' EXIT
fails=0
fail() { echo "FAIL: $*"; fails=$((fails + 1)); }
ok() { echo "ok: $*"; }
now() { date +%s.%N; }
lt() { awk -v a="$1" -v b="$2" 'BEGIN { exit !(a < b) }'; }   # a < b
# leftover tail processes reading this log (ps, not pgrep: the script must not use pgrep at all)
tails_left() { ps -eo args= | grep -F -- "tail" | grep -F -- "$1" | grep -v grep | wc -l; }

# 1. a line appended after 1 s: exit 0 quickly, tail gone, even though the log stays quiet afterwards
log="$tmp/a.log"; echo "first" > "$log"
( sleep 1; echo "READY now" >> "$log" ) & bgpids+=($!)
t0=$(now); out=$("$wf" "$log" 'READY' 30); rc=$?; t1=$(now)
el=$(awk -v a="$t1" -v b="$t0" 'BEGIN { print a - b }')
[ $rc -eq 0 ] && lt "$el" 3 && echo "$out" | grep -q 'wait-for: matched after .*READY now' \
    && ok "match after 1 s returns at once" || fail "match after 1 s (rc=$rc, ${el}s): $out"
[ "$(tails_left "$log")" = 0 ] && ok "tail is killed after the match" || fail "tail left running after the match"

# 2. the match is already in the log: 0 at once (reads from line 1)
log="$tmp/b.log"; printf 'one\nDONE here\ntwo\nthree\n' > "$log"
t0=$(now); out=$("$wf" --lines 2 "$log" 'DONE' 30); rc=$?; t1=$(now)
[ $rc -eq 0 ] && lt "$(awk -v a="$t1" -v b="$t0" 'BEGIN { print a - b }')" 3 && echo "$out" | grep -q 'matched after 0s: DONE here' \
    && [ "$(echo "$out" | tail -n 2 | tr '\n' ' ')" = "two three " ] \
    && ok "match already present, --lines 2" || fail "already present (rc=$rc): $out"

# 3. the log is created after the start
log="$tmp/c.log"
( sleep 1; echo "late READY" > "$log" ) & bgpids+=($!)
out=$("$wf" "$log" 'READY' 30); rc=$?
[ $rc -eq 0 ] && ok "log created after the start" || fail "late log (rc=$rc): $out"

# 4. timeout: exit 1, the tail of the log printed
log="$tmp/d.log"; printf 'x1\nx2\nx3\n' > "$log"
t0=$(now); out=$("$wf" "$log" 'NEVER' 2); rc=$?; t1=$(now)
[ $rc -eq 1 ] && lt "$(awk -v a="$t1" -v b="$t0" 'BEGIN { print a - b }')" 6 && echo "$out" | grep -q 'no match' && echo "$out" | grep -q '^x3$' \
    && ok "timeout 2 s prints the tail" || fail "timeout (rc=$rc): $out"
[ "$(tails_left "$log")" = 0 ] && ok "tail is killed after a timeout" || fail "tail left running after the timeout"

# 5. --pid of a process that ends: exit 3
log="$tmp/e.log"; echo "nothing" > "$log"
sleep 1 & p=$!; bgpids+=($p)
out=$("$wf" --pid "$p" "$log" 'NEVER' 30); rc=$?
[ $rc -eq 3 ] && ok "--pid ended: exit 3" || fail "--pid (rc=$rc): $out"

# 6. usage errors, regex starting with a dash, bad regex, clamp note
"$wf" >/dev/null 2>&1; [ $? -eq 2 ] && ok "no args: 2" || fail "no args"
"$wf" "$tmp/a.log" >/dev/null 2>&1; [ $? -eq 2 ] && ok "one arg: 2" || fail "one arg"
"$wf" "$tmp/a.log" 'x' abc >/dev/null 2>&1; [ $? -eq 2 ] && ok "bad timeout: 2" || fail "bad timeout"
"$wf" "$tmp/a.log" '(' 5 >/dev/null 2>&1; [ $? -eq 2 ] && ok "bad regex: 2" || fail "bad regex"
log="$tmp/f.log"; printf 'a\n-x flag\n' > "$log"
out=$("$wf" "$log" '-x' 5); rc=$?
[ $rc -eq 0 ] && ok "regex -x is matched, not read as an option" || fail "dash regex (rc=$rc): $out"
out=$("$wf" "$log" '-x' 9999 2>&1); echo "$out" | grep -q 'clamped to 590' && ok "timeout above 590 is clamped with a note" || fail "clamp: $out"

# 7. the script contains neither pgrep nor sleep as a command
if grep -nE '(^|[^-a-z_])(pgrep|sleep)([^-a-z_]|$)' "$wf" | grep -vE '^[0-9]+:\s*#' | grep -vE '^[0-9]+:.*(No pgrep|--pid|tail .* -s )' ; then
    fail "wait-for.sh uses pgrep or sleep"
else
    ok "no pgrep, no sleep"
fi

[ $fails -eq 0 ] && echo "all wait-for tests passed" && exit 0
echo "$fails failed"; exit 1

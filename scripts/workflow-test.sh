#!/usr/bin/env bash
# Tests of the parallel-orchestrator workflow scripts (#413): colours, session
# name, try.sh target, main lock, status line, driver run folders. Everything runs
# in a throwaway repository; nothing touches the real worktrees.
#
#   scripts/workflow-test.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap '[ -n "${KEEP:-}" ] || rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
eq() { [ "$1" = "$2" ] || fail "$3: expected [$2], got [$1]"; }

main="$tmp/StokesForge"
mkdir -p "$main"
git -C "$main" init -q -b main
git -C "$main" -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
cd "$main"
add_wt() { git worktree add -q "$tmp/_stokesforge/feature$1" -b "feature/$1-x" --no-track main; }
colour_of() { cat "$(git -C "$tmp/_stokesforge/feature$1" rev-parse --absolute-git-dir)/stokesforge-colour"; }

# Colours: first free, kept, freed on removal.
SF_ISSUE_TITLE="Fix axis label" "$here/chantier.sh" --dry-run 5 > "$tmp/out" 
grep -qx '🟦 #5 Fix axis label' "$tmp/out" || fail "session name: $(cat "$tmp/out")"
grep -q "claude -n .*#5 Fix axis label.*--add-dir .*feature5" "$tmp/out" || fail "claude line"
[ ! -e "$tmp/_stokesforge/feature5" ] || fail "dry-run created a worktree"

export SF_ISSUE_TITLE="Fix axis label"
PATH="$tmp/bin:$PATH"
mkdir -p "$tmp/bin"
printf '#!/bin/sh\necho "claude $*" > "%s/claude.args"\n' "$tmp" > "$tmp/bin/claude"
chmod +x "$tmp/bin/claude"
"$here/chantier.sh" 1 > /dev/null
grep -q '🟦 #1 Fix axis label' "$tmp/claude.args" || fail "claude started with the name"
"$here/chantier.sh" 2 > /dev/null
eq "$(colour_of 1)" 🟦 "A colour"
eq "$(colour_of 2)" 🟩 "B colour"
eq "$(git -C "$tmp/_stokesforge/feature1" rev-parse --abbrev-ref '@{u}' 2>&1 | head -c 5)" fatal "no upstream"
git worktree remove "$tmp/_stokesforge/feature1"
"$here/chantier.sh" 3 > /dev/null
eq "$(colour_of 3)" 🟦 "freed colour is reused"
"$here/chantier.sh" 3 > /dev/null
eq "$(colour_of 3)" 🟦 "colour kept on rerun"
[ -f "$tmp/_stokesforge/feature3/stokesforge-colour" ] && fail "colour file in the worktree root"
unset SF_ISSUE_TITLE

# Two issues in one session: every square, 3 words each; seventh refused.
out="$(SF_ISSUE_TITLE_7="Graph size: width and height fields" SF_ISSUE_TITLE_8="Frames follow the axes" "$here/chantier.sh" --dry-run 7 8)"
grep -xF '🟧 #7 Graph size · 🟪 #8 Frames follow' <<< "$out" > /dev/null || fail "two-issue name: $out"
for k in 11 12 13; do add_wt "$k"; done
printf '%s\n' 🟧 > "$(git -C "$tmp/_stokesforge/feature11" rev-parse --absolute-git-dir)/stokesforge-colour"
printf '%s\n' 🟪 > "$(git -C "$tmp/_stokesforge/feature12" rev-parse --absolute-git-dir)/stokesforge-colour"
printf '%s\n' 🟥 > "$(git -C "$tmp/_stokesforge/feature13" rev-parse --absolute-git-dir)/stokesforge-colour"
add_wt 14; printf '%s\n' 🟨 > "$(git -C "$tmp/_stokesforge/feature14" rev-parse --absolute-git-dir)/stokesforge-colour"
if SF_ISSUE_TITLE="Seventh" "$here/chantier.sh" 9 2> "$tmp/err7"; then fail "seventh work package accepted"; fi
grep -q '6 work packages in progress (🟦🟩🟧🟪🟥🟨)' "$tmp/err7" || fail "seventh message: $(cat "$tmp/err7")"
[ ! -e "$tmp/_stokesforge/feature9" ] || fail "seventh worktree created"
for k in 11 12 13 14; do git worktree remove "$tmp/_stokesforge/feature$k"; done

# try.sh targets the worktree's own target folder; unknown issue points to main.
out="$("$here/try.sh" --dry-run 2)"
echo "$out" | grep -q "CARGO_TARGET_DIR=$tmp/_stokesforge/feature2/target" || fail "try target: $out"
grep -q 'STOKESFORGE_DEMO=1' "$here/try.sh" || fail "try.sh opens the demo"
echo "$out" | grep -q -- "--manifest-path $tmp/_stokesforge/feature2/Cargo.toml" || fail "try manifest"
if "$here/try.sh" --dry-run 99 2> "$tmp/err"; then fail "try.sh 99 should fail"; fi
grep -q 'try.sh main' "$tmp/err" || fail "try.sh 99 message"

# Main lock: a second take fails, release frees.
"$here/main-lock.sh" take "A #1" > /dev/null
if "$here/main-lock.sh" take "B #2" 2> /dev/null; then fail "second take succeeded"; fi
"$here/main-lock.sh" status | grep -q 'locked: A #1' || fail "lock status"

# Status line: own colour, others, lock.
line="$(cd "$tmp/_stokesforge/feature2" && printf '{"cwd":"%s"}' "$tmp/_stokesforge/feature2" | HOME="$tmp/nohome" "$here/statusline.sh")"
echo "$line" | grep -q '^🟩 #2 | others 🟦 | main lock: A #1' || fail "statusline: $line"
# A session started by chantier.sh runs in the main checkout: the name tells its work package.
line="$(cd "$main" && printf '{"cwd":"%s"}' "$main" | SF_SESSION="🟩 #2 Fix axis" HOME="$tmp/nohome" "$here/statusline.sh")"
echo "$line" | grep -q '^🟩 #2 | others 🟦 | main lock: A #1' || fail "statusline from main checkout: $line"
"$here/main-lock.sh" release > /dev/null
eq "$("$here/main-lock.sh" status)" free "lock released"

# Driver: two checkouts get two run folders.
for r in r1 r2; do
    mkdir -p "$tmp/$r/.claude/skills/run-stokesforge"
    cp "$here/../.claude/skills/run-stokesforge/driver.py" "$tmp/$r/.claude/skills/run-stokesforge/"
done
w1="$(env -u SF_RUN_DIR -u SF_DISPLAY python3 "$tmp/r1/.claude/skills/run-stokesforge/driver.py" where)"
w2="$(env -u SF_RUN_DIR -u SF_DISPLAY python3 "$tmp/r2/.claude/skills/run-stokesforge/driver.py" where)"
[ "$w1" != "$w2" ] || fail "driver run folders are equal: $w1"

echo "workflow-test: all passed"

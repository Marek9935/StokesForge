#!/usr/bin/env bash
# Tests of the parallel-orchestrator workflow scripts (#413, #417): colours and
# briefing, try.sh target, main lock, driver run folders. Everything runs
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

# Colours: first free, kept, freed on removal. No session is started.
export SF_NO_FETCH=1
SF_ISSUE_TITLE="Fix axis label" "$here/chantier.sh" --dry-run 5 > "$tmp/out"
grep -q '^  🟦 #5 Fix axis label  worktree ' "$tmp/out" || fail "tag: $(cat "$tmp/out")"
grep -q 'scripts/try.sh <n>' "$tmp/out" || fail "briefing steps"
grep -q 'one worktree each, one session' "$tmp/out" || fail "briefing: several issues"
grep -qx 'Start EVERY message to the user with: 🟦 #5 Fix axis label' "$tmp/out" || fail "briefing: message tag"
grep -q '^To-test page: https://claude.ai/artifact/' "$tmp/out" || fail "briefing: To-test link"
# A one-word domain before a colon keeps the next words.
SF_ISSUE_TITLE="Workflow: every message of an orchestrator" "$here/chantier.sh" --dry-run 6 > "$tmp/out6"; grep -q "^  🟦 #6 Workflow every message  worktree " "$tmp/out6" || fail "domain prefix name: $(cat "$tmp/out6")"
[ "$(wc -l < "$tmp/out")" -le 30 ] || fail "briefing too long"
grep -q 'claude -n\|--add-dir' "$tmp/out" && fail "dry-run still mentions a claude launch"
[ ! -e "$tmp/_stokesforge/feature5" ] || fail "dry-run created a worktree"

export SF_ISSUE_TITLE="Fix axis label"
"$here/chantier.sh" 1 > "$tmp/run1"
grep -q '^  🟦 #1 Fix axis label' "$tmp/run1" || fail "real run tag: $(cat "$tmp/run1")"
# Rerun (after a /clear): same colour, same worktree, nothing new.
n_before="$(git worktree list | wc -l)"
"$here/chantier.sh" 1 > "$tmp/rerun1"
grep -q '^  🟦 #1 Fix axis label' "$tmp/rerun1" || fail "rerun colour: $(cat "$tmp/rerun1")"
eq "$(git worktree list | wc -l)" "$n_before" "rerun creates nothing"
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

# Two issues in one session: one worktree each, 3-word names; seventh refused.
out="$(SF_ISSUE_TITLE_7="Graph size: width and height fields" SF_ISSUE_TITLE_8="Frames follow the axes" "$here/chantier.sh" --dry-run 7 8)"
grep -qF '🟧 #7 Graph size  worktree' <<< "$out" || fail "two-issue tag 7: $out"
grep -qF '🟪 #8 Frames follow  worktree' <<< "$out" || fail "two-issue tag 8: $out"
SF_ISSUE_TITLE_7="Graph size: width and height fields" SF_ISSUE_TITLE_8="Frames follow the axes" "$here/chantier.sh" 7 8 > /dev/null
[ -d "$tmp/_stokesforge/feature7" ] && [ -d "$tmp/_stokesforge/feature8" ] || fail "two issues, two worktrees"
eq "$(colour_of 7)$(colour_of 8)" 🟧🟪 "two-issue colours"
for k in 7 8; do git worktree remove "$tmp/_stokesforge/feature$k"; done
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

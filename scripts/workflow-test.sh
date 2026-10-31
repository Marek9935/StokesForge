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
# No network: sub-issue lookups read a fixture (an empty list unless a test says so).
echo '[]' > "$tmp/none.json"
export SF_ISSUES_JSON="$tmp/none.json"
SF_ISSUE_TITLE="Fix axis label" "$here/chantier.sh" --dry-run 5 > "$tmp/out"
grep -q '^  🟦 #5 Fix axis label  worktree ' "$tmp/out" || fail "tag: $(cat "$tmp/out")"
grep -q 'scripts/try.sh <n>' "$tmp/out" || fail "briefing steps"
grep -q 'one worktree each, one session' "$tmp/out" || fail "briefing: several issues"
grep -qx 'Start EVERY message to the user with: 🟦 #5 Fix axis label' "$tmp/out" || fail "briefing: message tag"
grep -q '^To-test page: https://claude.ai/artifact/' "$tmp/out" || fail "briefing: To-test link"
# A one-word domain before a colon keeps the next words.
SF_ISSUE_TITLE="Workflow: every message of an orchestrator" "$here/chantier.sh" --dry-run 6 > "$tmp/out6"; grep -q "^  🟦 #6 Workflow every message  worktree " "$tmp/out6" || fail "domain prefix name: $(cat "$tmp/out6")"
SF_ISSUE_TITLE="Workflow: launch a work package" "$here/chantier.sh" --dry-run 6 > "$tmp/out6b"; grep -q "^  🟦 #6 Workflow launch  worktree " "$tmp/out6b" || fail "small word dropped after the domain"
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

# (Colours 🟦 and 🟩 are held by the worktrees 3 and 2 here; the package takes the next one.)
# Work package with sub-issues (#419): detected from a fixture, no worktree
# without --start, one colour reserved for the whole package.
cat > "$tmp/issues.json" <<'JSON'
[
 {"iid": 221, "title": "Third thing", "state": "opened", "description": "Sub-issue of #217"},
 {"iid": 219, "title": "Rejected fields: show why", "state": "opened", "description": "PARENT : #217\nrest"},
 {"iid": 220, "title": "Second step (#217)", "state": "opened", "description": ""},
 {"iid": 2170, "title": "Other", "state": "opened", "description": "Parent: #2170 and Parent: #2171"},
 {"iid": 230, "title": "Closed one", "state": "closed", "description": "Parent: #217"},
 {"iid": 231, "title": "Old form", "state": "opened", "description": "Sous-ticket du chantier #217"}
]
JSON
export SF_ISSUES_JSON="$tmp/issues.json"
export SF_ISSUE_BODY_217=$'Work:\n- #231 old form\n- #220\n- #219'
export SF_ISSUE_TITLE_217="Parent work: many sub-issues"
export SF_ISSUE_TITLE_88="Plain issue"
"$here/chantier.sh" 217 > "$tmp/pkg"
grep -qF '🟧 #217 Parent work' "$tmp/pkg" || fail "package tag: $(cat "$tmp/pkg")"
grep -n '^  [0-9]\. #' "$tmp/pkg" | sed 's/^[0-9]*://' | tr '\n' '|' > "$tmp/order"
eq "$(cat "$tmp/order")" "  1. #231 Old form|  2. #220 Second step (#217)|  3. #219 Rejected fields|  4. #221 Third thing|" "package order (parent body, then number; #2170 and closed ignored)"
grep -q 'first card of the start form' "$tmp/pkg" || fail "briefing: list is the first card"
grep -q -- '--start 231' "$tmp/pkg" || fail "briefing: --start hint"
[ ! -e "$tmp/_stokesforge/feature217" ] && [ ! -e "$tmp/_stokesforge/feature219" ] || fail "worktree created without --start"
eq "$(head -n 1 "$tmp/_stokesforge/.packages/217")" 🟧 "package colour reserved"
"$here/chantier.sh" 217 --start 219 > "$tmp/pkg2"
eq "$(colour_of 219)" 🟧 "sub-issue gets the package colour"
grep -qF 'Start EVERY message to the user with: 🟧 #217 › #219 Rejected fields' "$tmp/pkg2" || fail "sub-issue tag: $(cat "$tmp/pkg2")"
[ ! -e "$tmp/_stokesforge/feature220" ] || fail "other sub-issue created"
# The colour stays reserved between two sub-issues; a plain issue takes the next one.
git worktree remove "$tmp/_stokesforge/feature219"
"$here/chantier.sh" 88 > /dev/null
eq "$(colour_of 88)" 🟪 "package colour still reserved"
git worktree remove "$tmp/_stokesforge/feature88"; git branch -q -D feature/88-plain-issue
"$here/chantier.sh" 217 --start 220 > /dev/null
eq "$(colour_of 220)" 🟧 "second sub-issue, same colour"
if "$here/chantier.sh" 217 --start 99 2> /dev/null; then fail "--start of a non sub-issue accepted"; fi
if "$here/chantier.sh" --done 217 2> "$tmp/errd"; then fail "--done accepted with a worktree left"; fi
grep -q 'still has worktrees' "$tmp/errd" || fail "--done message"
git worktree remove "$tmp/_stokesforge/feature220"
"$here/chantier.sh" --done 217 > /dev/null
[ ! -e "$tmp/_stokesforge/.packages/217" ] || fail "--done keeps the reservation"
"$here/chantier.sh" 88 > /dev/null
eq "$(colour_of 88)" 🟧 "colour free after --done"
git worktree remove "$tmp/_stokesforge/feature88"
# A plain issue (no sub-issue) behaves as before.
"$here/chantier.sh" --dry-run 88 > "$tmp/out88"; grep -q '^  🟧 #88 Plain issue  worktree ' "$tmp/out88" || fail "plain issue changed"
unset SF_ISSUE_BODY_217 SF_ISSUE_TITLE_217 SF_ISSUE_TITLE_88
export SF_ISSUES_JSON="$tmp/none.json"

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

#!/usr/bin/env bash
# Tests of the parallel-orchestrator workflow scripts (#413, #417, #441): colours
# (scripts/suivi.py records) and briefing, try.sh target, main lock, driver run
# folders. Everything runs in a throwaway repository and a throwaway state folder
# (the sandbox's own ../_stokesforge/.suivi); nothing touches the real worktrees.
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
suivi() { python3 -I "$here/suivi.py" "$@"; }
# Field $2 of the record of issue $1 (empty for null); nested owner.session via owner_sid.
rec() { python3 -I -c 'import json,sys; v=json.load(open(sys.argv[1]))[sys.argv[2]]; print("" if v is None else v)' "$tmp/_stokesforge/.suivi/issues/$1.json" "$2"; }
colour_of() { rec "$1" colour; }
owner_sid() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["owner"]["session"])' "$tmp/_stokesforge/.suivi/issues/$1.json"; }
# Edit a field of a record (to age it).
setrec() { python3 -I -c 'import json,sys; p=sys.argv[1]; d=json.load(open(p)); d[sys.argv[2]]=sys.argv[3]; json.dump(d,open(p,"w"))' "$tmp/_stokesforge/.suivi/issues/$1.json" "$2" "$3"; }
# Remove the worktree of issue $1 by hand and mark the issue done (as --release does).
rmwt() { git worktree remove "$tmp/_stokesforge/feature$1"; suivi done "$1" > /dev/null; }
# This test plays the Claude session that owns the issues.
export CLAUDE_CODE_SESSION_ID=sid-1 CLAUDE_PID=$$ SF_TODAY=2026-11-03

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
grep -qx 'Tracking page: http://127.0.0.1:8441/' "$tmp/out" || fail "briefing: tracking page link"
grep -q 'scripts/suivi.py status' "$tmp/out" || fail "briefing: suivi status step"
# A one-word domain before a colon keeps the next words.
SF_ISSUE_TITLE="Workflow: every message of an orchestrator" "$here/chantier.sh" --dry-run 6 > "$tmp/out6"; grep -q "^  🟦 #6 Workflow every message  worktree " "$tmp/out6" || fail "domain prefix name: $(cat "$tmp/out6")"
SF_ISSUE_TITLE="Workflow: launch a work package" "$here/chantier.sh" --dry-run 6 > "$tmp/out6b"; grep -q "^  🟦 #6 Workflow launch  worktree " "$tmp/out6b" || fail "small word dropped after the domain"
[ "$(wc -l < "$tmp/out")" -le 30 ] || fail "briefing too long"
grep -q 'claude -n\|--add-dir' "$tmp/out" && fail "dry-run still mentions a claude launch"
[ ! -e "$tmp/_stokesforge/feature5" ] || fail "dry-run created a worktree"

export SF_ISSUE_TITLE="Fix axis label"
"$here/chantier.sh" 1 > "$tmp/run1"
grep -q '^  🟦 #1 Fix axis label' "$tmp/run1" || fail "real run tag: $(cat "$tmp/run1")"
eq "$(owner_sid 1)" sid-1 "lance records the owner session"
# Rerun (after a /clear): same colour, same worktree, nothing new.
n_before="$(git worktree list | wc -l)"
"$here/chantier.sh" 1 > "$tmp/rerun1"
grep -q '^  🟦 #1 Fix axis label' "$tmp/rerun1" || fail "rerun colour: $(cat "$tmp/rerun1")"
eq "$(git worktree list | wc -l)" "$n_before" "rerun creates nothing"
# A rerun in a NEW session (after /clear): same colour, new owner.
CLAUDE_CODE_SESSION_ID=sid-2 "$here/chantier.sh" 1 > "$tmp/rerun2"
grep -q '^  🟦 #1 Fix axis label' "$tmp/rerun2" || fail "new session keeps the colour: $(cat "$tmp/rerun2")"
eq "$(owner_sid 1)" sid-2 "rerun records the new owner"
"$here/chantier.sh" 2 > /dev/null
eq "$(colour_of 1)" 🟦 "A colour"
eq "$(colour_of 2)" 🟩 "B colour"
eq "$(git -C "$tmp/_stokesforge/feature1" rev-parse --abbrev-ref '@{u}' 2>&1 | head -c 5)" fatal "no upstream"
rmwt 1
"$here/chantier.sh" 3 > /dev/null
eq "$(colour_of 3)" 🟦 "freed colour is reused"
"$here/chantier.sh" 3 > /dev/null
eq "$(colour_of 3)" 🟦 "colour kept on rerun"
[ ! -e "$tmp/_stokesforge/feature3/stokesforge-colour" ] && [ -f "$tmp/_stokesforge/.suivi/issues/3.json" ] || fail "colour lives in .suivi only"
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
 {"iid": 231, "title": "Old form", "state": "opened", "description": "Sous-ticket du chantier #217"},
 {"iid": 232, "title": "Short form", "state": "opened", "description": "Sous-ticket de #217."},
 {"iid": 419, "title": "Docs quoting an example", "state": "opened", "description": "Example: `Parent : #217`, and\n```\nParent: #217\n```"}
]
JSON
export SF_ISSUES_JSON="$tmp/issues.json"
export SF_ISSUE_BODY_217=$'Work:\n- #231 old form\n- #220\n- #219'
export SF_ISSUE_TITLE_217="Parent work: many sub-issues"
export SF_ISSUE_TITLE_88="Plain issue"
"$here/chantier.sh" 217 > "$tmp/pkg"
grep -qF '🟧 #217 Parent work' "$tmp/pkg" || fail "package tag: $(cat "$tmp/pkg")"
grep -n '^  [0-9]\. #' "$tmp/pkg" | sed 's/^[0-9]*://' | tr '\n' '|' > "$tmp/order"
eq "$(cat "$tmp/order")" "  1. #231 Old form|  2. #220 Second step (#217)|  3. #219 Rejected fields|  4. #221 Third thing|  5. #232 Short form|" "package order (parent body, then number; #2170, closed and quoted ignored)"
grep -q 'second card of the start form, after the understanding card' "$tmp/pkg" || fail "briefing: list is the second card"
grep -q -- '--start 231' "$tmp/pkg" || fail "briefing: --start hint"
[ ! -e "$tmp/_stokesforge/feature217" ] && [ ! -e "$tmp/_stokesforge/feature219" ] || fail "worktree created without --start"
eq "$(rec 217 colour)" 🟧 "package colour reserved"
eq "$(rec 217 kind)" package "package record"
"$here/chantier.sh" 217 --start 219 > "$tmp/pkg2"
eq "$(colour_of 219)" 🟧 "sub-issue gets the package colour"
grep -qF 'Start EVERY message to the user with: 🟧 #217 › #219 Rejected fields' "$tmp/pkg2" || fail "sub-issue tag: $(cat "$tmp/pkg2")"
[ ! -e "$tmp/_stokesforge/feature220" ] || fail "other sub-issue created"
# The colour stays reserved between two sub-issues; a plain issue takes the next one.
rmwt 219
"$here/chantier.sh" 88 > /dev/null
eq "$(colour_of 88)" 🟪 "package colour still reserved"
rmwt 88; git branch -q -D feature/88-plain-issue
"$here/chantier.sh" 217 --start 220 > /dev/null
eq "$(colour_of 220)" 🟧 "second sub-issue, same colour"
if "$here/chantier.sh" 217 --start 99 2> /dev/null; then fail "--start of a non sub-issue accepted"; fi
if "$here/chantier.sh" --done 217 2> "$tmp/errd"; then fail "--done accepted with a worktree left"; fi
grep -q 'still has worktrees' "$tmp/errd" || fail "--done message"
rmwt 220
"$here/chantier.sh" --done 217 > /dev/null
[ -n "$(rec 217 done_at)" ] || fail "--done does not mark the package"
SF_ISSUE_TITLE_89="Plain issue" "$here/chantier.sh" 89 > /dev/null
eq "$(colour_of 89)" 🟧 "colour free after --done"
rmwt 89
# A plain issue (no sub-issue) behaves as before.
SF_ISSUE_TITLE_89="Plain issue" "$here/chantier.sh" --dry-run 89 > "$tmp/out89"; grep -q '^  🟧 #89 Plain issue  worktree ' "$tmp/out89" || fail "plain issue changed"
unset SF_ISSUE_BODY_217 SF_ISSUE_TITLE_217 SF_ISSUE_TITLE_88
export SF_ISSUES_JSON="$tmp/none.json"

# Two issues in one session: one worktree each, 3-word names; seventh refused.
out="$(SF_ISSUE_TITLE_7="Graph size: width and height fields" SF_ISSUE_TITLE_8="Frames follow the axes" "$here/chantier.sh" --dry-run 7 8)"
grep -qF '🟧 #7 Graph size  worktree' <<< "$out" || fail "two-issue tag 7: $out"
grep -qF '🟪 #8 Frames follow  worktree' <<< "$out" || fail "two-issue tag 8: $out"
SF_ISSUE_TITLE_7="Graph size: width and height fields" SF_ISSUE_TITLE_8="Frames follow the axes" "$here/chantier.sh" 7 8 > /dev/null
[ -d "$tmp/_stokesforge/feature7" ] && [ -d "$tmp/_stokesforge/feature8" ] || fail "two issues, two worktrees"
eq "$(colour_of 7)$(colour_of 8)" 🟧🟪 "two-issue colours"
for k in 7 8; do rmwt "$k"; done
for k in 11 12 13 14; do add_wt "$k"; suivi claim "$k" > /dev/null; done
eq "$(colour_of 11)$(colour_of 12)$(colour_of 13)$(colour_of 14)" 🟧🟪🟥🟨 "six colours held"
if SF_ISSUE_TITLE="Seventh" "$here/chantier.sh" 9 2> "$tmp/err7"; then fail "seventh work package accepted"; fi
grep -q '6 work packages in progress (🟦🟩🟧🟪🟥🟨)' "$tmp/err7" || fail "seventh message: $(cat "$tmp/err7")"
[ ! -e "$tmp/_stokesforge/feature9" ] || fail "seventh worktree created"
for k in 11 12 13 14; do rmwt "$k"; done

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

# Colours per issue, --status, --pause, --release (#441). feature2 (green) and
# feature3 (blue) are alive here; the first free colour is orange.
S="$tmp/_stokesforge"
# Branches left by the package tests above (their worktrees are gone).
git branch -q -D feature/219-rejected-fields-show-why feature/220-second-step-217 feature/89-plain-issue 2> /dev/null || true
wtadd() { git worktree add -q "$S/feature$1" -b "feature/$1-x" --no-track main; }
unset SF_ISSUE_TITLE
export SF_ISSUE_TITLE_30="Thirty things" SF_ISSUE_TITLE_31="Thirty-one" SF_ISSUE_TITLE_88="Plain issue"

# Folder name first: feature426b is #426, feature4260 is #4260, a detached HEAD still has its issue.
eq "$(. "$here/workflow-lib.sh"; sf_issue_of "$S/feature426b")" 426 "issue of feature426b"
eq "$(. "$here/workflow-lib.sh"; sf_issue_of "$S/feature4260")" 4260 "feature4260 is not #426"
git worktree add -q "$S/odd" -b feature/77-y --no-track main
eq "$(. "$here/workflow-lib.sh"; sf_issue_of "$S/odd")" 77 "branch fallback"
git worktree remove "$S/odd"

# Several worktrees of one issue share one colour, a colourless one gets it on rerun.
for k in 30 30b 30d; do wtadd "$k"; done
"$here/chantier.sh" 30 > /dev/null
eq "$(colour_of 30)" 🟧 "siblings share the record's colour"
wtadd 30c
"$here/chantier.sh" 30 > /dev/null
eq "$(colour_of 30)" 🟧 "added sibling: same colour on rerun"
[ "$(ls "$S/.suivi/issues" | grep -c '^30')" = 1 ] || fail "one record for the four worktrees of #30"
"$here/chantier.sh" 31 > /dev/null
eq "$(colour_of 31)" 🟪 "next plain issue takes the next colour"
git -C "$S/feature30b" checkout -q --detach
wtadd 50
setrec 30 since 2026-10-20
for k in 2 3 31; do setrec "$k" since 2026-11-01; done
"$here/chantier.sh" --status --json > "$tmp/st.json"
python3 -I - "$tmp/st.json" << 'PY' || fail "--status --json: $(cat "$tmp/st.json")"
import json, sys
d = json.load(open(sys.argv[1]))
c = d["colours"]
assert [e["colour"] for e in c] == ["🟦", "🟩", "🟧", "🟪", "🟥", "🟨"], c
assert c[0]["issue"] == 3 and c[1]["issue"] == 2 and c[3]["issue"] == 31, c
assert c[2]["issue"] == 30 and c[2]["since"] == "2026-10-20" and c[2]["days"] == 14, c[2]
assert sorted(c[2]["worktrees"]) == ["feature30", "feature30b", "feature30c", "feature30d"], c[2]
assert c[3]["since"] == "2026-11-01" and c[3]["days"] == 2, c[3]
assert c[4]["free"] and c[5]["free"], c
assert d["colourless"] == ["feature50"], d["colourless"]
assert d["paused"] == [], d
assert c[2]["held_by"] == ["agent"] and c[2]["name"] == "Thirty things", c[2]
PY
"$here/chantier.sh" --status > "$tmp/st.txt"
grep -q '^🟧 #30 ' "$tmp/st.txt" && grep -q '^🟥 free' "$tmp/st.txt" || fail "--status text: $(cat "$tmp/st.txt")"
# --pause works on any issue: colour freed, worktree kept, previous colour back on resume.
# lang: fr begin
eq "$("$here/chantier.sh" --pause 31)" "Dossier #31 en pause : couleur 🟪 libérée." "--pause of a plain issue"
# lang: fr end
[ -d "$S/feature31" ] || fail "--pause removed the worktree"
"$here/chantier.sh" --status --json | python3 -I -c '
import json, sys
d = json.load(sys.stdin)
assert d["colours"][3]["free"] and [p["issue"] for p in d["paused"]] == [31] and d["paused"][0]["worktrees"] == ["feature31"], d' || fail "--status after --pause 31"
"$here/chantier.sh" 31 > /dev/null
eq "$(colour_of 31)" 🟪 "resume gets the previous colour"
git worktree remove "$S/feature50"; git branch -q -d feature/50-x

# Package 217: a rerun keeps the "since" date; --pause frees the colour and keeps the list.
export SF_ISSUES_JSON="$tmp/issues.json" SF_ISSUE_BODY_217=$'Work:\n- #231 old form\n- #220\n- #219'
export SF_ISSUE_TITLE_217="Parent work: many sub-issues"
"$here/chantier.sh" 217 --start 219 > /dev/null
eq "$(colour_of 219)" 🟥 "package colour"
setrec 217 since 2026-10-25
"$here/chantier.sh" 217 --start 219 > /dev/null
eq "$(rec 217 since)" 2026-10-25 "package rerun keeps its since date"
"$here/chantier.sh" --status --json | python3 -I -c '
import json, sys
e = json.load(sys.stdin)["colours"][4]
assert e["package"]["parent"] == 217 and e["since"] == "2026-10-25" and e["days"] == 9, e
assert [s["n"] for s in e["package"]["subs"]][:2] == [231, 220], e
assert e["worktrees"] == ["feature219"], e' || fail "--status package"
if "$here/chantier.sh" --pause 217 2> "$tmp/errp"; then fail "--pause accepted with a worktree left"; fi
grep -q 'still has worktrees' "$tmp/errp" || fail "--pause message"
eq "$(rec 217 colour)" 🟥 "colour kept after a refused --pause"
# lang: fr begin
out="$("$here/chantier.sh" --no-trial --release 219)"
eq "$out" "Couleur 🟥 reste prise : paquet #217, prochaine sous-issue ; après la dernière : scripts/chantier.sh --done 217" "release of a sub-issue"
# lang: fr end
"$here/chantier.sh" --dry-run --pause 217 > /dev/null
eq "$(rec 217 colour)" 🟥 "--dry-run --pause changes nothing"
# lang: fr begin
eq "$("$here/chantier.sh" --pause 217)" "Paquet #217 en pause : couleur 🟥 libérée." "--pause of a package"
# lang: fr end
eq "$(rec 217 colour)" "" "paused: no colour"
eq "$(rec 217 last_colour)" 🟥 "paused: previous colour remembered"
python3 -I -c 'import json,sys; assert json.load(open(sys.argv[1]))["subs"][0]["n"] == 231' "$S/.suivi/issues/217.json" || fail "paused package keeps its sub-issue list"
"$here/chantier.sh" 88 > /dev/null
eq "$(colour_of 88)" 🟥 "a paused package frees its colour"
"$here/chantier.sh" --status --json | python3 -I -c '
import json, sys
d = json.load(sys.stdin)
assert [p["issue"] for p in d["paused"]] == [217] and d["colours"][4]["issue"] == 88, d' || fail "--status paused"
"$here/chantier.sh" 217 --start 220 > "$tmp/pkg3"
grep -qF '🟨 #217 › #220' "$tmp/pkg3" && ! grep -qF '?' "$tmp/pkg3" || fail "start after pause: $(cat "$tmp/pkg3")"
eq "$(rec 217 colour)" 🟨 "package no longer paused (🟥 is taken: first free)"
# lang: fr begin
out="$("$here/chantier.sh" --no-trial --release 220)"
case "$out" in "Couleur 🟨 reste prise : paquet #217"*) ;; *) fail "release 220: $out" ;; esac
out="$("$here/chantier.sh" --done 217)"
eq "$out" "Couleur 🟨 libérée." "--done line"
"$here/chantier.sh" 217 > /dev/null
"$here/chantier.sh" --pause 217 > /dev/null
eq "$("$here/chantier.sh" --done 217)" "Paquet #217 terminé (il n'avait plus de couleur)." "--done on a paused package"
# lang: fr end
rmwt 88
unset SF_ISSUE_BODY_217 SF_ISSUE_TITLE_217
export SF_ISSUES_JSON="$tmp/none.json"

# --release: merged and clean, nothing else removed first; one fixed French line.
exclude="$(git rev-parse --absolute-git-dir)/info/exclude"
printf 'junk.log\ntarget/\n__pycache__/\n' >> "$exclude"
kept() { [ -d "$S/feature$1" ] && git rev-parse --verify -q "feature/$1-x" > /dev/null; }
wtadd 40; git -C "$S/feature40" -c user.name=t -c user.email=t@t commit -q --allow-empty -m unmerged
wtadd 41; touch "$S/feature41/new.txt"
wtadd 42
wtadd 43; touch "$S/feature43/junk.log"
for k in 40 41 43; do
    if out="$("$here/chantier.sh" --release "$k" 2> "$tmp/errr")"; then fail "--release $k accepted"; fi
    kept "$k" || fail "--release $k removed something"
    [ -z "$out" ] || fail "--release $k printed [$out]"
    cp "$tmp/errr" "$tmp/errr$k"
done
grep -q 'not merged' "$tmp/errr40" || fail "unmerged refusal: $(cat "$tmp/errr40")"
grep -q 'new.txt' "$tmp/errr41" || fail "untracked file not listed: $(cat "$tmp/errr41")"
grep -q 'junk.log' "$tmp/errr43" || fail "ignored file not listed: $(cat "$tmp/errr43")"
# Nothing is ever forced: the script has no -D and no --force.
grep -v '^ *#' "$here/chantier.sh" | grep -E 'branch -D|--force' && fail "chantier.sh forces a removal" || true
if (cd "$S/feature42" && "$here/chantier.sh" --release 42 > /dev/null 2> "$tmp/errw"); then fail "--release from a worktree accepted"; fi
grep -q 'main checkout' "$tmp/errw" || fail "from a worktree message"
git checkout -q -b elsewhere
if "$here/chantier.sh" --release 42 > /dev/null 2> "$tmp/errb"; then fail "--release off main accepted"; fi
git checkout -q main; git branch -q -d elsewhere
kept 42 || fail "refused releases removed feature42"
"$here/chantier.sh" --dry-run --no-trial --release 42 > "$tmp/dry"
kept 42 && grep -q 'would run: git worktree remove' "$tmp/dry" || fail "--dry-run --release"
# V2: no trial entry -> refused, nothing removed; one entry -> accepted.
if "$here/chantier.sh" --release 3 > /dev/null 2> "$tmp/errt"; then fail "--release without a trial entry accepted"; fi
grep -q 'no trial entry' "$tmp/errt" && [ -e "$S/feature3" ] || fail "--release without a trial entry: $(cat "$tmp/errt")"
printf '{"title":"t","steps":["s"],"expect":"e"}' > "$tmp/t3.json"
python3 -I "$here/suivi.py" test add 3 "$tmp/t3.json" > /dev/null
# lang: fr begin
out="$("$here/chantier.sh" --release 3 2> "$tmp/errr")"
eq "$out" "Couleur 🟦 libérée." "release 3 line"
[ ! -e "$S/feature3" ] && ! git rev-parse --verify -q feature/3-fix-axis-label > /dev/null || fail "release 3 left something"
out="$("$here/chantier.sh" --no-trial --release 30 2> /dev/null)"
case "$out" in "Couleur 🟧 reste prise : "*feature30b*) ;; *) fail "release 30: $out" ;; esac
# lang: fr end
[ ! -e "$S/feature30" ] || fail "release 30 kept the worktree"
for k in 30b 30c; do "$here/chantier.sh" --no-trial --release "$k" > /dev/null; done
git branch -q -d feature/30b-x   # detached worktree: its branch was not the one released
# lang: fr begin
eq "$("$here/chantier.sh" --no-trial --release 30d)" "Couleur 🟧 libérée." "last worktree of the issue frees the colour"
# lang: fr end
# target/ and __pycache__/ (build and Python caches) are the ignored folders that do not block.
SF_ISSUE_TITLE="Target" "$here/chantier.sh" 44 > /dev/null
mkdir "$S/feature44/target"; touch "$S/feature44/target/x"
mkdir "$S/feature44/__pycache__"; touch "$S/feature44/__pycache__/x.pyc"
"$here/chantier.sh" --no-trial --release 44 | grep -q '^Couleur .* libérée\.$' || fail "release with target/ and __pycache__/"
[ ! -e "$S/feature44" ] || fail "target/ or __pycache__/ blocked the release"
for k in 40 41 42 43; do git worktree remove --force "$S/feature$k"; done
git branch -q -D feature/40-x feature/41-x feature/42-x feature/43-x feature/30d-x feature/44-target 2> /dev/null || true

# deliver.sh (#449): its own sandbox (a bare fake remote, its own .suivi), run before
# the final `suivi check` because the sandbox records would be flagged there.
(
    sb="$tmp/dsb"
    export SF_SUIVI_DIR="$sb/_stokesforge/.suivi" SF_DELIVER_DELAYS="0 0 0 0"
    sbsuivi() { python3 -I "$here/suivi.py" "$@"; }
    dsb() {   # a fresh sandbox: main + remote, feature/7-x with one commit, one entry to try
        cd /; rm -rf "$sb"; mkdir -p "$sb/_stokesforge"
        git init -q --bare -b main "$sb/remote.git"
        git init -q -b main "$sb/StokesForge"
        cd "$sb/StokesForge"
        git config user.name t; git config user.email t@t; git config push.followTags true
        echo base > f; git add f; git commit -q -m init
        git remote add origin "$sb/remote.git"; git push -q origin main
        wt="$sb/_stokesforge/feature7"
        git worktree add -q "$wt" -b feature/7-x --no-track main
        git -C "$wt" commit -q --allow-empty -m work
        echo 'Merge #7: add x' > "$sb/msg"
        printf '{"title":"t","steps":["s"],"expect":"e","try":"scripts/try.sh 7"}' > "$sb/e.json"
        sbsuivi test add 7 "$sb/e.json" > /dev/null
    }
    deliver() { ( cd "${DCWD:-$sb/StokesForge}" && "$here/deliver.sh" "${DN:-7}" "${DMSG:-$sb/msg}" ) > "$sb/out" 2>&1; }
    main_head() { git -C "$sb/StokesForge" rev-parse main; }
    remote_head() { git -C "$sb/remote.git" rev-parse main; }
    entry_field() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2],"-"))' "$SF_SUIVI_DIR/tests/$1.json" "$2"; }
    lock_free() { [ ! -e "$sb/_stokesforge/.main.lock" ] || fail "$1: the main lock is still held"; }
    untouched() {   # $1 label: main unchanged since $2, entry still to try, no merge in progress
        eq "$(main_head)" "$2" "$1: main moved"
        eq "$(entry_field 7-1 commit)" "-" "$1: the entry got a commit"
        [ -n "$(entry_field 7-1 try)" ] && [ "$(entry_field 7-1 try)" != "-" ] || fail "$1: the entry lost its try"
        git -C "$sb/StokesForge" rev-parse -q --verify MERGE_HEAD > /dev/null && fail "$1: MERGE_HEAD left" || true
        lock_free "$1"
    }
    refused() { if deliver; then fail "$1: accepted: $(cat "$sb/out")"; fi; untouched "$1" "$base"; }

    # Happy path: one --no-ff merge with the message, pushed, recorded, no tag pushed.
    dsb
    git -C "$wt" tag -a v9 -m "local tag" HEAD
    deliver || fail "happy path: $(cat "$sb/out")"
    m="$(main_head)"
    eq "$(git -C "$sb/StokesForge" log -1 --format=%s main)" "Merge #7: add x" "merge message"
    eq "$(git -C "$sb/StokesForge" rev-list --parents -n 1 main | wc -w)" 3 "merge commit has two parents"
    eq "$(remote_head)" "$m" "remote at M"
    eq "$(entry_field 7-1 commit)" "$m" "entry at M"
    eq "$(entry_field 7-1 try)" "-" "entry lost try"
    lock_free "happy"
    eq "$(git -C "$sb/remote.git" tag | wc -l)" 0 "a local tag was pushed"
    # Follow-up delivery, no new entries: succeeds, the old commit stays.
    git -C "$wt" merge -q --no-edit main
    git -C "$wt" commit -q --allow-empty -m more
    echo 'Merge #7: more' > "$sb/msg2"
    DMSG="$sb/msg2" deliver || fail "follow-up delivery: $(cat "$sb/out")"
    eq "$(entry_field 7-1 commit)" "$m" "follow-up kept the old commit"
    eq "$(remote_head)" "$(main_head)" "follow-up pushed"

    # --wait-tag (#467): refused before the lock if the message closes the issue; otherwise the wait
    # is recorded locally, never through glab (a fake glab on PATH logs any call).
    mkdir -p "$tmp/fakebin"; printf '#!/usr/bin/env bash\necho "$*" >> "%s/glab.log"\nexit 1\n' "$tmp" > "$tmp/fakebin/glab"
    chmod +x "$tmp/fakebin/glab"
    wdeliver() { ( cd "$sb/StokesForge" && PATH="$tmp/fakebin:$PATH" "$here/deliver.sh" "$@" ) > "$sb/out" 2>&1; }
    dsb; base="$(main_head)"
    printf 'Merge #7: add x\n\nCloses #7\n' > "$sb/msgc"
    if wdeliver --wait-tag rc 7 "$sb/msgc"; then fail "--wait-tag with Closes accepted"; fi
    grep -q 'keeps it open' "$sb/out" || fail "--wait-tag refusal text: $(cat "$sb/out")"
    untouched "--wait-tag with Closes" "$base"
    [ ! -e "$SF_SUIVI_DIR/tagwait/7.json" ] || fail "--wait-tag refused but a wait was recorded"
    if wdeliver --wait-tag soon 7 "$sb/msg"; then fail "--wait-tag soon accepted"; fi
    wdeliver --wait-tag final 7 "$sb/msg" || fail "--wait-tag final: $(cat "$sb/out")"
    eq "$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["kind"])' "$SF_SUIVI_DIR/tagwait/7.json")" final "tagwait record kind"
    [ ! -e "$tmp/glab.log" ] || fail "deliver --wait-tag called glab: $(cat "$tmp/glab.log")"
    # Cleanup warning: a merge that does not close the issue and declared no wait warns (stderr),
    # the fixed colour line stays the only stdout; a declared wait or a Closes keeps it silent.
    "$here/chantier.sh" --release 7 > "$sb/rout" 2> "$sb/rerr" || fail "release with a wait: $(cat "$sb/rerr")"
    ! grep -q warning "$sb/rerr" || fail "release warned although the wait is declared: $(cat "$sb/rerr")"
    dsb
    deliver || fail "plain delivery: $(cat "$sb/out")"
    "$here/chantier.sh" --release 7 > "$sb/rout" 2> "$sb/rerr" || fail "release without Closes: $(cat "$sb/rerr")"
    grep -q 'warning: #7 stays open' "$sb/rerr" || fail "no warning for an issue left open: $(cat "$sb/rerr")"
    eq "$(wc -l < "$sb/rout")" 1 "release: the colour line is the only stdout"
    dsb; printf 'Merge #7: add x\n\nCloses #7\n' > "$sb/msg"
    deliver || fail "closing delivery: $(cat "$sb/out")"
    "$here/chantier.sh" --release 7 > "$sb/rout" 2> "$sb/rerr" || fail "release with Closes: $(cat "$sb/rerr")"
    ! grep -q warning "$sb/rerr" || fail "warned although the merge closes the issue: $(cat "$sb/rerr")"

    # Refusals: main unchanged, lock free.
    dsb; base="$(main_head)"
    echo 'Merge #77: x' > "$sb/bad"; DMSG="$sb/bad" refused "bad message"
    DMSG="$sb/none" refused "missing file"
    DN=77 refused "wrong issue"
    git -C "$sb/StokesForge" checkout -q -b other; refused "not on main"; git -C "$sb/StokesForge" checkout -q main
    DCWD="$wt" refused "run from a worktree"
    touch "$sb/StokesForge/dirt"; refused "dirty main"; rm "$sb/StokesForge/dirt"
    touch "$wt/dirt"; refused "dirty worktree"; rm "$wt/dirt"
    # An empty branch has nothing to merge.
    git -C "$wt" reset -q --hard main; refused "empty branch"
    dsb; base="$(main_head)"
    # A held lock: refused, the other owner's file intact.
    mkdir "$sb/_stokesforge/.main.lock"; echo "someone" > "$sb/_stokesforge/.main.lock/owner"
    if deliver; then fail "held lock: accepted"; fi
    eq "$(cat "$sb/_stokesforge/.main.lock/owner")" someone "held lock: owner file touched"
    rm -rf "$sb/_stokesforge/.main.lock"
    eq "$(main_head)" "$base" "held lock: main moved"
    # Fetch failure (SF_NO_FETCH=1 is exported and must be ignored).
    git -C "$sb/StokesForge" remote set-url origin "$sb/nowhere.git"
    refused "fetch failure"
    git -C "$sb/StokesForge" remote set-url origin "$sb/remote.git"
    # Merge failure: a pre-merge-commit hook refuses.
    printf '#!/bin/sh\nexit 1\n' > "$sb/StokesForge/.git/hooks/pre-merge-commit"; chmod +x "$sb/StokesForge/.git/hooks/pre-merge-commit"
    refused "merge failure"
    rm "$sb/StokesForge/.git/hooks/pre-merge-commit"

    # Push: 4 refusals then success; always refused; fixed and rerun; a foreign commit on top.
    dsb; base="$(main_head)"
    hook="$sb/remote.git/hooks/pre-receive"
    printf '#!/bin/sh\nc=$(cat "%s/count" 2>/dev/null || echo 0); c=$((c+1)); echo $c > "%s/count"\n[ $c -gt 4 ]\n' "$sb" "$sb" > "$hook"; chmod +x "$hook"
    deliver || fail "push after 4 failures: $(cat "$sb/out")"
    eq "$(remote_head)" "$(main_head)" "push on the 5th try"
    dsb; base="$(main_head)"
    printf '#!/bin/sh\nexit 1\n' > "$hook"; chmod +x "$hook"
    if deliver; then fail "push always refused: accepted"; fi
    m="$(main_head)"; [ "$m" != "$base" ] || fail "push refused: the merge is not kept locally"
    eq "$(entry_field 7-1 commit)" "-" "push refused: entry recorded"
    lock_free "push refused"
    git -C "$sb/StokesForge" commit -q --allow-empty -m foreign          # a foreign commit on top of unpushed M
    rm "$hook"
    deliver || fail "rerun after the hook was fixed: $(cat "$sb/out")"
    eq "$(entry_field 7-1 commit)" "$m" "rerun records M, not HEAD"
    eq "$(remote_head)" "$(main_head)" "rerun pushed"
    eq "$(git -C "$sb/StokesForge" rev-list --merges --first-parent main | wc -l)" 1 "rerun merged again"
    # Recording fails (the colour lock is held): pushed, rerun records.
    dsb; base="$(main_head)"
    mkdir "$sb/_stokesforge/.colour.lock"
    if SF_SUIVI_LOCK_WAIT=0.2 deliver; then fail "record failure: accepted"; fi
    eq "$(remote_head)" "$(main_head)" "record failure: not pushed"
    eq "$(entry_field 7-1 commit)" "-" "record failure: recorded anyway"
    lock_free "record failure"
    rmdir "$sb/_stokesforge/.colour.lock"
    deliver || fail "rerun records: $(cat "$sb/out")"
    eq "$(entry_field 7-1 commit)" "$(main_head)" "rerun records M"
    eq "$(git -C "$sb/StokesForge" rev-list --merges --first-parent main | wc -l)" 1 "record rerun merged again"

    # Main moved on the remote: main goes into the branch, exit 3, nothing else moves; rerun merges.
    dsb
    git clone -q "$sb/remote.git" "$sb/other"
    git -C "$sb/other" -c user.name=o -c user.email=o@o commit -q --allow-empty -m "elsewhere"
    git -C "$sb/other" push -q origin main
    moved="$(git -C "$sb/other" rev-parse HEAD)"
    set +e; deliver; rc=$?; set -e
    eq "$rc" 3 "main moved: exit code"
    eq "$(main_head)" "$moved" "main moved: main is the fetched one"
    git -C "$sb/StokesForge" merge-base --is-ancestor main feature/7-x || fail "main moved: the branch lacks main"
    eq "$(entry_field 7-1 commit)" "-" "main moved: entry recorded"
    lock_free "main moved"
    deliver || fail "rerun after exit 3: $(cat "$sb/out")"
    eq "$(entry_field 7-1 commit)" "$(main_head)" "rerun after exit 3 records M"
    # Conflict while merging main into the branch.
    dsb
    echo branch > "$wt/f"; git -C "$wt" commit -q -am "edit f"
    git clone -q "$sb/remote.git" "$sb/other"
    echo other > "$sb/other/f"; git -C "$sb/other" -c user.name=o -c user.email=o@o commit -q -am "edit f elsewhere"
    git -C "$sb/other" push -q origin main
    set +e; deliver; rc=$?; set -e
    eq "$rc" 1 "conflict: exit code"
    git -C "$wt" rev-parse -q --verify MERGE_HEAD > /dev/null && fail "conflict: MERGE_HEAD left in the worktree" || true
    lock_free "conflict"
)

# No inconsistency left (duplicate colour, record without worktree, worktree without record).
suivi check > "$tmp/check.out" || fail "suivi check: $(cat "$tmp/check.out")"
[ -f "$S/.suivi/control.json" ] || fail "check writes control.json"

# Driver: two checkouts get two run folders.
for r in r1 r2; do
    mkdir -p "$tmp/$r/.claude/skills/run-stokesforge"
    cp "$here/../.claude/skills/run-stokesforge/driver.py" "$tmp/$r/.claude/skills/run-stokesforge/"
done
w1="$(env -u SF_RUN_DIR -u SF_DISPLAY python3 "$tmp/r1/.claude/skills/run-stokesforge/driver.py" where)"
w2="$(env -u SF_RUN_DIR -u SF_DISPLAY python3 "$tmp/r2/.claude/skills/run-stokesforge/driver.py" where)"
[ "$w1" != "$w2" ] || fail "driver run folders are equal: $w1"

# test-all.sh: nextest when present (then the doc tests), plain cargo test otherwise,
# a red step fails the script, arguments are refused. A fake cargo logs its calls.
fc="$tmp/fakecargo"; mkdir -p "$fc"
cat > "$fc/cargo" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "$FAKE_LOG"
case "$1 $2" in
    "nextest --version") [ "${FAKE_NEXTEST:-1}" = 1 ] || exit 1 ;;
    "nextest run") [ "${FAKE_RED:-}" != nextest ] || exit 1 ;;
    "test --workspace") [ "${FAKE_RED:-}" != doc ] || { [[ " $* " == *" --doc "* ]] && exit 1; } ;;
esac
exit 0
EOF
chmod +x "$fc/cargo"
ta() { : > "$tmp/fake.log"; PATH="$fc:$PATH" FAKE_LOG="$tmp/fake.log" "$here/test-all.sh" "$@" > /dev/null 2>&1; }
ta || fail "test-all: nextest present, exit code"
eq "$(grep -v -- '--version' "$tmp/fake.log")" "nextest run --workspace --no-fail-fast
test --workspace --doc" "test-all: nextest then doc tests, in order"
FAKE_NEXTEST=0 ta || fail "test-all: nextest absent, exit code"
eq "$(grep -v -- '--version' "$tmp/fake.log")" "test --workspace" "test-all: nextest absent runs cargo test --workspace only"
set +e; FAKE_RED=nextest ta; rc=$?; set -e
[ "$rc" -ne 0 ] || fail "test-all: red nextest must fail"
set +e; FAKE_RED=doc ta; rc=$?; set -e
[ "$rc" -ne 0 ] || fail "test-all: red doc tests must fail"
set +e; ta some_filter; rc=$?; set -e
[ "$rc" -ne 0 ] || fail "test-all: an argument must be refused"
eq "$(grep -c . "$tmp/fake.log")" 0 "test-all: a refused argument runs no cargo"

echo "workflow-test: all passed"

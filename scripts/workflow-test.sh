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

real_common="$(git -C "$here/.." rev-parse --path-format=absolute --git-common-dir)"
stamps_before="$( (ls "$real_common/sf-tested" 2> /dev/null || true) | sort | md5sum)"   # #515: the real repo gets no stamp from here
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
# #518: a .stf argument is opened as a copy; a missing file or another extension is refused.
: > "$tmp/proj.stf"; : > "$tmp/data.csv"
out="$("$here/try.sh" --dry-run 2 "$tmp/proj.stf")" || fail "try.sh with a .stf failed"
echo "$out" | grep -q 'would run:.*stokesforge-app .*stokesforge-try\..*/proj.stf' || fail "try.sh does not run the copy: $out"
if "$here/try.sh" --dry-run 2 "$tmp/none.stf" > /dev/null 2>&1; then fail "try.sh accepted a missing file"; fi
if "$here/try.sh" --dry-run 2 "$tmp/data.csv" > /dev/null 2>&1; then fail "try.sh accepted a .csv"; fi
# The trial skips the cargo queue (#476), in debug and in release, on the build command only.
echo "$out" | grep -q 'would build: env SF_CARGO_QUEUE=off cargo build -p stokesforge-app' || fail "try queue bypass: $out"
echo "$out" | grep -q 'would run: .*SF_CARGO_QUEUE' && fail "try run carries the bypass"
# --release (#474), before or after the target: release build and binary; absent, neither.
echo "$out" | grep -q -- '--release' && fail "try dry-run mentions release"
echo "$out" | grep -q 'target/release' && fail "try dry-run names the release binary"
for rel in "--dry-run --release 2" "--dry-run 2 --release" "--release --dry-run 2"; do
    # shellcheck disable=SC2086
    rout="$("$here/try.sh" $rel)"
    echo "$rout" | grep -q 'would build: env SF_CARGO_QUEUE=off cargo build --release' || fail "try $rel build: $rout"
    echo "$rout" | grep -q "would run: $tmp/_stokesforge/feature2/target/release/stokesforge-app" || fail "try $rel run: $rout"
done
if "$here/try.sh" --dry-run 99 2> "$tmp/err"; then fail "try.sh 99 should fail"; fi
grep -q 'try.sh main' "$tmp/err" || fail "try.sh 99 message"
# A suffixed sibling worktree (feature999b, branch feature/999b-x) resolves like a plain number (#489).
git worktree add -q "$tmp/_stokesforge/feature999b" -b "feature/999b-x" --no-track main
out="$("$here/try.sh" --dry-run 999b)" || fail "try.sh 999b failed"
echo "$out" | grep -q "CARGO_TARGET_DIR=$tmp/_stokesforge/feature999b/target" || fail "try 999b target: $out"
git worktree remove "$tmp/_stokesforge/feature999b"; git branch -q -D feature/999b-x

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
    export SF_TEMPLATE_REFRESH="echo refresh >> '$tmp/refresh.log'"     # #523: the detached refresh, replaced by a log line
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
        printf '{"title":"t","steps":["s"],"expect":"e","try":"scripts/try.sh 7","goal":"g","around":"a"}' > "$sb/e.json"
        sbsuivi test add 7 "$sb/e.json" > /dev/null
        stamp
    }
    stamp() {   # the test stamp (#515) of the head of feature/7-x, as test-all.sh would write it
        mkdir -p "$sb/StokesForge/.git/sf-tested"
        : > "$sb/StokesForge/.git/sf-tested/$(git -C "$wt" rev-parse "${1:-HEAD}")"
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
    refused_nostamp() { if deliver; then fail "$1: accepted: $(cat "$sb/out")"; fi; grep -q 'no test stamp' "$sb/out" || fail "$1: message: $(cat "$sb/out")"; }
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
    eq "$(wc -l < "$tmp/refresh.log")" 1 "a delivery to main starts the template refresh"
    # Follow-up delivery, no new entries: succeeds, the old commit stays.
    git -C "$wt" merge -q --no-edit main
    git -C "$wt" commit -q --allow-empty -m more; stamp
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
    # #496: an unsettled retour of the issue refuses the delivery before anything moves; settled, it goes through.
    set_status() { python3 -I -c 'import json,sys; p=sys.argv[1]; d=json.load(open(p)); d["status"]=sys.argv[2]; json.dump(d,open(p,"w"))' "$SF_SUIVI_DIR/tests/7-1.json" "$1"; }
    set_status retour
    refused "unsettled retour"
    grep -q '7-1' "$sb/out" && grep -q 'test settle' "$sb/out" || fail "unsettled retour: message names the entry and the command: $(cat "$sb/out")"
    sbsuivi test settle 7-1 --note "fixed" > /dev/null || fail "settle in the sandbox"
    deliver || fail "settled retour: refused: $(cat "$sb/out")"
    dsb; base="$(main_head)"
    # #518: an OK + idea never blocks the delivery; one line names the unsettled idea.
    python3 -I -c 'import json,sys; p=sys.argv[1]; d=json.load(open(p)); d["status"]="ok"; d["idea"]="more"; d.pop("settled",None); json.dump(d,open(p,"w"))' "$SF_SUIVI_DIR/tests/7-1.json"
    deliver || fail "OK + idea blocked the delivery: $(cat "$sb/out")"
    grep -q 'unsettled ideas.*7-1' "$sb/out" || fail "OK + idea: the delivery does not name the idea: $(cat "$sb/out")"
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

    # Tested commit (#515): the head of the branch needs the stamp test-all.sh writes.
    dsb; base="$(main_head)"
    rm -rf "$sb/StokesForge/.git/sf-tested"
    refused_nostamp "no stamp"; untouched "no stamp" "$base"
    grep -q 'test-all.sh' "$sb/out" || fail "no stamp: the message does not name test-all.sh"
    stamp "HEAD~1"   # the stamp of the parent commit does not cover the head
    refused_nostamp "stamp of the parent commit"; untouched "stamp of the parent commit" "$base"
    stamp
    deliver || fail "stamped head: $(cat "$sb/out")"
    # A rerun that only pushes needs no stamp: the merge is done.
    dsb; base="$(main_head)"
    printf '#!/bin/sh\nexit 1\n' > "$sb/remote.git/hooks/pre-receive"; chmod +x "$sb/remote.git/hooks/pre-receive"
    if deliver; then fail "push refused: accepted"; fi
    rm "$sb/remote.git/hooks/pre-receive"; rm -rf "$sb/StokesForge/.git/sf-tested"
    deliver || fail "rerun to push without a stamp: $(cat "$sb/out")"
    eq "$(remote_head)" "$(main_head)" "rerun to push without a stamp: pushed"

    # Manual screenshots (#515): SF_MANUAL_SHOTS stands for manual-screenshots.sh and logs its calls.
    shots="$tmp/shots.sh"
    printf '#!/usr/bin/env bash\necho called >> "%s/shots.log"\nmkdir -p manual/src/images\nprintf PNG%%s "$SHOTS_CONTENT" > manual/src/images/a.png\n' "$tmp" > "$shots"; chmod +x "$shots"
    ui_change() { mkdir -p "$wt/crates/x"; echo ui > "$wt/crates/x/lib.rs"; git -C "$wt" add crates; git -C "$wt" commit -q -m "ui"; stamp; }
    # UI change, the fake renders an image: an image commit inside M, stamped.
    dsb; ui_change; rm -f "$tmp/shots.log"
    SHOTS_CONTENT=1 SF_MANUAL_SHOTS="$shots" deliver || fail "screenshots: $(cat "$sb/out")"
    eq "$(grep -c called "$tmp/shots.log")" 1 "screenshots: the fake ran once"
    eq "$(git -C "$wt" log -1 --format=%s)" "manual: regenerate screenshots (#7)" "screenshots: image commit on the branch"
    git -C "$sb/StokesForge" cat-file -e main:manual/src/images/a.png || fail "screenshots: the image is not in M"
    git -C "$sb/StokesForge" merge-base --is-ancestor "$(git -C "$wt" rev-parse HEAD)" main || fail "screenshots: the image commit is not inside M"
    eq "$(remote_head)" "$(main_head)" "screenshots: pushed"
    # UI change, the fake renders nothing new (image already committed and identical): no extra commit.
    dsb; ui_change
    mkdir -p "$wt/manual/src/images"; printf PNG1 > "$wt/manual/src/images/a.png"
    git -C "$wt" add manual; git -C "$wt" commit -q -m "image"; stamp
    n0="$(git -C "$wt" rev-list --count HEAD)"
    SHOTS_CONTENT=1 SF_MANUAL_SHOTS="$shots" deliver || fail "screenshots unchanged: $(cat "$sb/out")"
    eq "$(git -C "$wt" rev-list --count HEAD)" "$n0" "screenshots unchanged: an extra commit was made"
    # The fake fails: exit 1, main unchanged.
    dsb; ui_change; base="$(main_head)"
    SF_MANUAL_SHOTS="exit 1" refused "screenshots failure"
    grep -q 'screenshots failed' "$sb/out" || fail "screenshots failure message: $(cat "$sb/out")"
    # Only docs/ changed: the fake is not called.
    dsb; rm -f "$tmp/shots.log"
    mkdir -p "$wt/docs"; echo d > "$wt/docs/d.md"; git -C "$wt" add docs; git -C "$wt" commit -q -m "docs"; stamp
    SHOTS_CONTENT=1 SF_MANUAL_SHOTS="$shots" deliver || fail "docs only: $(cat "$sb/out")"
    [ ! -e "$tmp/shots.log" ] || fail "docs only: the screenshots were regenerated"
    # Only tests and a non-interface crate changed: the fake is not called.
    dsb; rm -f "$tmp/shots.log"
    mkdir -p "$wt/crates/x/tests" "$wt/crates/stokesforge-arch-guard/src"
    echo t > "$wt/crates/x/tests/t.rs"; echo g > "$wt/crates/stokesforge-arch-guard/src/lib.rs"
    git -C "$wt" add crates; git -C "$wt" commit -q -m "tests"; stamp
    SHOTS_CONTENT=1 SF_MANUAL_SHOTS="$shots" deliver || fail "tests only: $(cat "$sb/out")"
    [ ! -e "$tmp/shots.log" ] || fail "tests only: the screenshots were regenerated"
    # The head moves after the first check (here a commit made while the screenshots run): the check
    # inside the lock refuses it.
    dsb; ui_change; base="$(main_head)"
    SF_MANUAL_SHOTS='git commit -q --allow-empty -m sneaky' refused_nostamp "head moved after the first check"
    untouched "head moved after the first check" "$base"
    # An unstamped branch is refused BEFORE the screenshots run.
    dsb; ui_change; rm -rf "$sb/StokesForge/.git/sf-tested"; rm -f "$tmp/shots.log"
    SHOTS_CONTENT=1 SF_MANUAL_SHOTS="$shots" refused_nostamp "unstamped before the screenshots"
    [ ! -e "$tmp/shots.log" ] || fail "unstamped: the screenshots ran anyway"

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
    grep -q 'Rerun scripts/test-all.sh' "$sb/out" || fail "exit 3 message: $(cat "$sb/out")"
    # The head changed: the old stamp does not cover it; a rerun without a new stamp is refused.
    refused_nostamp "rerun after exit 3 without a new stamp"
    eq "$(entry_field 7-1 commit)" "-" "refused rerun after exit 3 recorded"
    lock_free "refused rerun after exit 3"
    stamp
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

    # ---- Integration line (#480): PostV1.1, its worktree, the delivery, the release, sync.
    L=PostV1.1
    lsetup() {   # dsb + the line created + issue 7 declared on it
        dsb; "$here/line.sh" create $L > /dev/null
        lwt="$sb/_stokesforge/line-$L"
        sbsuivi claim 7 --line $L > /dev/null
    }
    lhead() { git -C "$lwt" rev-parse HEAD; }
    rhead() { git -C "$sb/remote.git" rev-parse "refs/heads/$L"; }
    lcommit() { git -C "$lwt" commit -q --allow-empty -m "$1"; git -C "$lwt" push -q origin $L; }
    recline() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1])).get("line","-"))' "$SF_SUIVI_DIR/issues/$1.json"; }
    chant() { ( cd "$sb/StokesForge" && "$here/chantier.sh" "$@" ); }

    # create: the branch, the worktree, pushed from main; a second create is refused.
    dsb; "$here/line.sh" create $L > "$sb/out" 2>&1 || fail "line create: $(cat "$sb/out")"
    lwt="$sb/_stokesforge/line-$L"
    eq "$(lhead)" "$(main_head)" "line starts at main"
    eq "$(rhead)" "$(main_head)" "line pushed"
    if "$here/line.sh" create $L > /dev/null 2>&1; then fail "second create accepted"; fi
    lock_free "create"

    # A declared line: the worktree starts from origin/<line>, the record carries it; rerun without
    # the declaration puts the record back on main.
    lcommit "line only"
    export SF_ISSUE_TITLE="Nine"
    SF_ISSUE_LINE_9=$L chant 9 > "$sb/out" 2>&1 || fail "chantier on a line: $(cat "$sb/out")"
    eq "$(git -C "$sb/StokesForge" rev-parse feature/9-nine)" "$(lhead)" "worktree based on origin/$L"
    eq "$(recline 9)" $L "record line"
    grep -q "^Line $L: " "$sb/out" || fail "briefing line: $(cat "$sb/out")"
    chant 9 > /dev/null
    eq "$(recline 9)" - "declaration removed: record back on main"
    eq "$(sbsuivi line 9)" main "line of 9"
    # An unknown line is refused, nothing created (no fallback to main).
    if SF_ISSUE_LINE_10=Nope chant 10 > "$sb/out" 2>&1; then fail "unknown line accepted"; fi
    grep -q 'origin/Nope does not exist' "$sb/out" || fail "unknown line message: $(cat "$sb/out")"
    [ ! -e "$sb/_stokesforge/feature10" ] && ! git -C "$sb/StokesForge" rev-parse -q --verify feature/10-nine > /dev/null || fail "unknown line created something"
    unset SF_ISSUE_TITLE

    # Delivery to the line: M on the line, pushed; main, origin/main and the main checkout untouched,
    # even dirty and on another branch; the message path is relative.
    lsetup; base="$(main_head)"
    git -C "$sb/StokesForge" checkout -q -b other; echo dirt > "$sb/StokesForge/dirt"
    rlines="$(wc -l < "$tmp/refresh.log")"
    DMSG=../msg deliver || fail "deliver to the line: $(cat "$sb/out")"
    eq "$(wc -l < "$tmp/refresh.log")" "$rlines" "a delivery to a line started a template refresh"
    m="$(lhead)"
    eq "$(git -C "$lwt" log -1 --format=%s)" "Merge #7: add x" "line merge message"
    eq "$(git -C "$lwt" rev-list --parents -n 1 HEAD | wc -w)" 3 "line merge has two parents"
    eq "$(rhead)" "$m" "line pushed at M"
    eq "$(git -C "$sb/remote.git" rev-parse refs/heads/main)" "$base" "origin/main moved"
    eq "$(git -C "$sb/StokesForge" rev-parse refs/heads/main)" "$base" "main moved"
    eq "$(git -C "$sb/StokesForge" symbolic-ref --short HEAD)" other "main checkout moved"
    [ -f "$sb/StokesForge/dirt" ] || fail "the main checkout lost its dirt"
    eq "$(entry_field 7-1 commit)" "$m" "entry at M (on the line)"
    lock_free "line delivery"
    # The release tests the merge against the line and deletes the branch from the line worktree.
    git -C "$sb/StokesForge" checkout -q main; rm "$sb/StokesForge/dirt"
    ( cd "$sb/StokesForge" && "$here/chantier.sh" --release 7 > "$sb/rout" 2> "$sb/rerr" ) || fail "release of a line issue: $(cat "$sb/rerr")"
    [ ! -e "$wt" ] && ! git -C "$sb/StokesForge" rev-parse -q --verify feature/7-x > /dev/null || fail "release left the worktree or the branch"
    [ -d "$lwt" ] || fail "release removed the line worktree"
    ! grep -q warning "$sb/rerr" || fail "a line release warned: $(cat "$sb/rerr")"
    # Released only when merged into the LINE: merged into main alone it stays.
    lsetup
    git -C "$sb/StokesForge" merge -q --no-ff -m "Merge #7: direct" feature/7-x
    if ( cd "$sb/StokesForge" && "$here/chantier.sh" --release 7 > /dev/null 2> "$sb/rerr" ); then fail "release accepted a branch not in the line"; fi
    grep -q "not merged into $L" "$sb/rerr" || fail "release refusal: $(cat "$sb/rerr")"
    [ -d "$wt" ] && git -C "$sb/StokesForge" rev-parse -q --verify feature/7-x > /dev/null || fail "refused release removed something"

    # The line moved: it is merged into the branch (exit 3); the rerun merges once.
    lsetup
    git clone -q "$sb/remote.git" "$sb/other2"
    git -C "$sb/other2" checkout -q $L
    git -C "$sb/other2" -c user.name=o -c user.email=o@o commit -q --allow-empty -m "line elsewhere"
    git -C "$sb/other2" push -q origin $L
    set +e; deliver; rc=$?; set -e
    eq "$rc" 3 "line moved: exit code"
    git -C "$sb/StokesForge" merge-base --is-ancestor "origin/$L" feature/7-x || fail "line moved: the branch lacks the line"
    eq "$(entry_field 7-1 commit)" "-" "line moved: entry recorded"
    lock_free "line moved"
    stamp
    deliver || fail "rerun after exit 3 on the line: $(cat "$sb/out")"
    eq "$(git -C "$lwt" log --first-parent --merges --format=%s | grep -c '^Merge #7:')" 1 "one merge on the line"

    # The test stamp rule holds on a line too.
    lsetup; rm -rf "$sb/StokesForge/.git/sf-tested"
    refused_nostamp "line delivery without a stamp"
    eq "$(lhead)" "$(rhead)" "line delivery without a stamp: line moved"
    stamp
    deliver || fail "line delivery with a stamp: $(cat "$sb/out")"

    # A closing keyword or --wait-tag on a line: refused before the lock.
    lsetup; lbase="$(lhead)"
    printf 'Merge #7: add x\n\nCloses #8\n' > "$sb/msgc"
    if wdeliver 7 "$sb/msgc"; then fail "Closes on a line accepted"; fi
    grep -q 'Refs #n' "$sb/out" || fail "closing refusal text: $(cat "$sb/out")"
    if wdeliver --wait-tag rc 7 "$sb/msg"; then fail "--wait-tag on a line accepted"; fi
    eq "$(lhead)" "$lbase" "refusals moved the line"
    lock_free "line refusals"
    [ ! -e "$SF_SUIVI_DIR/tagwait/7.json" ] || fail "a wait was recorded on a line"

    # Main refuses a branch holding a line-only commit; a landed line is fine.
    dsb; "$here/line.sh" create $L > /dev/null; lwt="$sb/_stokesforge/line-$L"; base="$(main_head)"
    lcommit "line only"
    git -C "$wt" merge -q --no-edit $L; stamp
    refused "branch with a line-only commit"
    grep -q 'not on main' "$sb/out" || fail "line-only message: $(cat "$sb/out")"
    git -C "$sb/StokesForge" merge -q --no-ff -m "Merge line" $L; git -C "$sb/StokesForge" push -q origin main   # the line lands
    git -C "$wt" merge -q --no-edit main; stamp
    deliver || fail "branch based on a landed line refused: $(cat "$sb/out")"

    # sync: main reaches the line by a merge; a conflict aborts and leaves the line alone.
    dsb; "$here/line.sh" create $L > /dev/null; lwt="$sb/_stokesforge/line-$L"
    git clone -q "$sb/remote.git" "$sb/other3"
    git -C "$sb/other3" -c user.name=o -c user.email=o@o commit -q --allow-empty -m "main moves"
    git -C "$sb/other3" push -q origin main
    "$here/line.sh" sync $L > "$sb/out" 2>&1 || fail "sync: $(cat "$sb/out")"
    git -C "$lwt" merge-base --is-ancestor "$(git -C "$sb/other3" rev-parse HEAD)" HEAD || fail "sync: main is not in the line"
    eq "$(rhead)" "$(lhead)" "sync pushed"
    lock_free "sync"
    echo line > "$lwt/f"; git -C "$lwt" commit -q -am "edit f on the line"; git -C "$lwt" push -q origin $L
    echo main > "$sb/other3/f"; git -C "$sb/other3" -c user.name=o -c user.email=o@o commit -q -am "edit f on main"
    git -C "$sb/other3" push -q origin main
    lbase="$(lhead)"
    if "$here/line.sh" sync $L > "$sb/out" 2>&1; then fail "conflicting sync accepted"; fi
    eq "$(lhead)" "$lbase" "conflicting sync moved the line"
    git -C "$lwt" rev-parse -q --verify MERGE_HEAD > /dev/null && fail "sync conflict: MERGE_HEAD left" || true
    eq "$(rhead)" "$lbase" "conflicting sync pushed"
    lock_free "sync conflict"

    # ---- A declared line read from the GitLab label ligne-<line> (fake glab); sub-issues follow the parent.
    mkdir -p "$tmp/labelbin" "$tmp/labels"
    cat > "$tmp/labelbin/glab" <<'GL'
#!/usr/bin/env bash
# fake glab: `issue view N -F json` -> title and the labels in $FAKE_LABELS/N (a JSON array)
[ "$1 $2" = "issue view" ] || exit 1
lab="$(cat "$FAKE_LABELS/$3" 2> /dev/null || echo '[]')"
printf '{"title": "Issue %s", "labels": %s}\n' "$3" "$lab"
GL
    chmod +x "$tmp/labelbin/glab"
    lchant() { ( cd "$sb/StokesForge" && env -u SF_ISSUE_TITLE PATH="$tmp/labelbin:$PATH" FAKE_LABELS="$tmp/labels" "$here/chantier.sh" "$@" ); }
    dsb; "$here/line.sh" create $L > /dev/null; lwt="$sb/_stokesforge/line-$L"; lcommit "line only"
    echo '["bug","ligne-PostV1.1"]' > "$tmp/labels/12"
    lchant 12 > "$sb/out" 2>&1 || fail "chantier with the label: $(cat "$sb/out")"
    eq "$(recline 12)" $L "label declares the line"
    eq "$(git -C "$sb/StokesForge" rev-parse feature/12-issue-12)" "$(lhead)" "label: worktree based on the line"
    echo '["bug"]' > "$tmp/labels/12"
    lchant 12 > /dev/null
    eq "$(recline 12)" - "label removed, rerun: record back on main"
    echo '["ligne-PostV1.1"]' > "$tmp/labels/20"; echo '[]' > "$tmp/labels/21"
    echo '[{"iid": 21, "title": "Sub", "state": "opened", "description": "Parent: #20"}]' > "$sb/sub.json"
    SF_ISSUES_JSON="$sb/sub.json" SF_ISSUE_BODY_20="- #21" lchant 20 --start 21 > "$sb/out" 2>&1 || fail "sub-issue of a line parent: $(cat "$sb/out")"
    eq "$(git -C "$sb/StokesForge" rev-parse feature/21-sub)" "$(lhead)" "sub-issue follows the parent's line"
    eq "$(sbsuivi line 21)" $L "sub-issue line in the record"
    echo '["ligne-PostV1.1","ligne-Other"]' > "$tmp/labels/22"
    if lchant 22 > /dev/null 2>&1; then fail "two ligne-* labels accepted"; fi

    # ---- line.sh land
    mkdir -p "$tmp/landbin"
    cat > "$tmp/landbin/glab" <<'GL'
#!/usr/bin/env bash
# fake glab for `api`: issue state in $FAKE_GL/state-N, labelled issues in $FAKE_GL/labelled, calls logged
d="$FAKE_GL"; echo "$*" >> "$d/calls"
shift; method=GET; path=""; fields=()
while [ "$#" -gt 0 ]; do
    case "$1" in --method) method="$2"; shift 2 ;; -f) fields+=("$2"); shift 2 ;; *) path="$1"; shift ;; esac
done
case "$path" in
*"/issues?labels="*) printf '[%s]\n' "$(sed 's/[0-9][0-9]*/{"iid": &}/g; s/} {/}, {/g' "$d/labelled")" ;;
*/notes) n="${path%/notes}"; n="${n##*/}"; echo "$n ${fields[*]}" >> "$d/notes"; echo '{}' ;;
*/issues/*)
    n="${path##*/}"
    if [ "$method" = PUT ]; then
        for f in "${fields[@]}"; do
            case "$f" in
            state_event=close) [ ! -e "$d/failclose-$n" ] || exit 1; echo closed > "$d/state-$n" ;;
            remove_labels=*) sed -i "s/\b$n\b//" "$d/labelled" ;;
            esac
        done
        echo '{}'
    else
        printf '{"state": "%s"}\n' "$(cat "$d/state-$n" 2> /dev/null || echo opened)"
    fi ;;
esac
GL
    chmod +x "$tmp/landbin/glab"
    land() { ( cd "$sb/StokesForge" && PATH="$tmp/landbin:$PATH" FAKE_GL="$sb/gl" "$here/line.sh" land $L ) > "$sb/out" 2>&1; }
    lland_setup() {   # a line holding #7 and #8, the final tag on main, the trial OK on the line
        lsetup; mkdir -p "$sb/gl"; echo "7 8 12" > "$sb/gl/labelled"
        wt8="$sb/_stokesforge/feature8"
        git worktree add -q "$wt8" -b feature/8-y --no-track main
        git -C "$wt8" commit -q --allow-empty -m work8
        : > "$sb/StokesForge/.git/sf-tested/$(git -C "$wt8" rev-parse HEAD)"
        sbsuivi claim 8 --line $L > /dev/null
        printf '{"title":"t","steps":["s"],"expect":"e","try":"scripts/try.sh 8","goal":"g","around":"a"}' > "$sb/e8.json"
        sbsuivi test add 8 "$sb/e8.json" > /dev/null
        deliver || fail "land setup, #7: $(cat "$sb/out")"
        git -C "$wt8" merge -q --no-edit $L
        : > "$sb/StokesForge/.git/sf-tested/$(git -C "$wt8" rev-parse HEAD)"
        echo 'Merge #8: add y' > "$sb/msg8"
        DN=8 DMSG="$sb/msg8" deliver || fail "land setup, #8: $(cat "$sb/out")"
        printf '{"title":"whole line","steps":["s"],"expect":"e","try":"scripts/try.sh PostV1.1","goal":"g","around":"a"}' > "$sb/eline.json"
        sbsuivi test add 480 "$sb/eline.json" > /dev/null
        git -C "$sb/StokesForge" tag v1.1.0 main
    }
    trial_set() { python3 -I -c 'import json,sys; p=sys.argv[1]; d=json.load(open(p)); d["status"]=sys.argv[2]; json.dump(d,open(p,"w"))' "$SF_SUIVI_DIR/tests/480-1.json" "$1"; }
    trial_ok() { trial_set ok; }
    notes_of() { grep -c "^$1 " "$sb/gl/notes" 2> /dev/null || true; }

    # Refusals: no tag, a tag off main, an -rc tag only (the trial is OK, so only the tag can refuse), then a
    # trial not OK. Main and the line stay as they are, GitLab is not called.
    lland_setup; base="$(main_head)"; lbase="$(lhead)"; trial_ok
    git -C "$sb/StokesForge" tag -d v1.1.0 > /dev/null
    if land; then fail "land without the tag accepted"; fi
    grep -q 'does not exist' "$sb/out" || fail "no-tag message: $(cat "$sb/out")"
    git -C "$sb/StokesForge" tag v1.1.0 "$lbase"       # on the line only
    if land; then fail "land with a tag off main accepted"; fi
    grep -q 'not on main' "$sb/out" || fail "tag off main message: $(cat "$sb/out")"
    git -C "$sb/StokesForge" tag -d v1.1.0 > /dev/null
    git -C "$sb/StokesForge" tag v1.1.0-rc1 main
    if land; then fail "land with an -rc tag only accepted"; fi
    grep -q 'does not exist' "$sb/out" || fail "rc-only message: $(cat "$sb/out")"
    git -C "$sb/StokesForge" tag v1.1.0 main
    trial_set todo
    if land; then fail "land with the trial not OK accepted"; fi
    grep -q 'not OK yet' "$sb/out" || fail "trial refusal text: $(cat "$sb/out")"
    eq "$(main_head)" "$base" "refused land moved main"
    eq "$(lhead)" "$lbase" "refused land moved the line"
    lock_free "land refusals"
    [ ! -e "$sb/gl/calls" ] || fail "a refused land called GitLab: $(cat "$sb/gl/calls")"

    # main moved since the line was last synced: it is merged into the line, pushed, exit 3, main untouched.
    git clone -q "$sb/remote.git" "$sb/other4"
    git -C "$sb/other4" -c user.name=o -c user.email=o@o commit -q --allow-empty -m "main moves"
    git -C "$sb/other4" push -q origin main; trial_ok
    base="$(git -C "$sb/remote.git" rev-parse refs/heads/main)"
    set +e; land; rc=$?; set -e
    eq "$rc" 3 "land with main absent from the line: exit code ($(cat "$sb/out"))"
    eq "$(git -C "$sb/remote.git" rev-parse refs/heads/main)" "$base" "land exit 3 moved origin/main"
    git -C "$lwt" merge-base --is-ancestor "$base" HEAD || fail "land exit 3: main is not in the line"
    eq "$(rhead)" "$(lhead)" "land exit 3 pushed the line"
    lock_free "land exit 3"
    [ ! -e "$sb/gl/calls" ] || fail "exit 3 called GitLab"

    # Happy path, with a failure while closing the second issue: rerun finishes, nothing closed twice.
    touch "$sb/gl/failclose-8"
    if land; then fail "land with a GitLab failure accepted"; fi
    m="$(git -C "$sb/remote.git" rev-parse refs/heads/main)"
    eq "$(git -C "$sb/remote.git" log -1 --format=%s refs/heads/main)" "Merge $L: land the integration line" "land merge message"
    eq "$(git -C "$sb/remote.git" rev-list --parents -n 1 refs/heads/main | wc -w)" 3 "landing merge has two parents"
    eq "$(cat "$sb/gl/state-7")" closed "#7 closed"
    [ ! -e "$sb/gl/state-8" ] || fail "#8 closed despite the failure"
    lock_free "land partial"
    [ ! -e "$sb/_stokesforge/land-tmp" ] || fail "temporary worktree left"
    rm "$sb/gl/failclose-8"
    land || fail "land rerun: $(cat "$sb/out")"
    eq "$(git -C "$sb/remote.git" rev-parse refs/heads/main)" "$m" "rerun merged again"
    eq "$(cat "$sb/gl/state-8")" closed "#8 closed by the rerun"
    eq "$(notes_of 7)" 1 "#7 commented twice"
    grep -q "$m" "$sb/gl/notes" || fail "the comment does not cite the merge commit"
    [ ! -e "$sb/gl/state-12" ] || fail "an issue the line did not deliver was closed"
    eq "$(tr -d ' \n' < "$sb/gl/labelled")" "" "label left on an issue"
    eq "$(sbsuivi line 7)" main "records keep the line"
    [ ! -e "$lwt" ] && ! git -C "$sb/StokesForge" rev-parse -q --verify "refs/heads/$L" > /dev/null || fail "line worktree or branch left"
    ! git -C "$sb/remote.git" rev-parse -q --verify "refs/heads/$L" > /dev/null || fail "remote line branch left"
    eq "$(git -C "$sb/StokesForge" rev-parse main)" "$m" "local main not in step"
    lock_free "land"

    # try.sh on a line: its worktree is built and opened, the log is against main; an unknown line is refused.
    dsb; "$here/line.sh" create $L > /dev/null; lwt="$sb/_stokesforge/line-$L"; lcommit "line only"
    out="$( cd "$sb/StokesForge" && "$here/try.sh" --dry-run $L )" || fail "try.sh on a line"
    grep -q "CARGO_TARGET_DIR=$lwt/target" <<< "$out" || fail "try.sh line target: $out"
    grep -q 'line only' <<< "$out" || fail "try.sh line log: $out"
    if ( cd "$sb/StokesForge" && "$here/try.sh" --dry-run Nope ) > /dev/null 2>&1; then fail "try.sh on an unknown line accepted"; fi
    git -C "$lwt" checkout -q -b side; git -C "$lwt" commit -q --allow-empty -m side; git -C "$lwt" checkout -q $L
    git -C "$lwt" merge -q --no-ff -m "Merge #99: marker" side; git -C "$lwt" push -q origin $L
    sbsuivi claim 7 --line $L > /dev/null
    out="$( cd "$sb/StokesForge" && "$here/try.sh" --dry-run 7 )" || fail "try.sh on a line issue"
    grep -q "== last merges on $L" <<< "$out" && grep -q "not yet on $L" <<< "$out" && grep -q 'Merge #99: marker' <<< "$out" || fail "try.sh issue log against the line: $out"
)

# Template target (#523): refresh, sweep, copy, prune, with a fake cargo (it logs its calls and prints
# fake JSON artifacts) and a fake rustc; its own sandbox.
(
    sb="$tmp/tsb"; rm -rf "$sb"; mkdir -p "$sb/_stokesforge" "$sb/fake" "$sb/home"
    export SF_SUIVI_DIR="$sb/_stokesforge/.suivi" HOME="$sb/home"
    git init -q -b main "$sb/StokesForge"; cd "$sb/StokesForge"
    git config user.name t; git config user.email t@t
    echo base > f; git add f; git commit -q -m init
    mkfifo "$sb/gate"
    export SF_DF_AVAIL_KB=999999999     # plenty of room unless a case lowers it
    T="$sb/_stokesforge/.template"
    export SF_TEMPLATE_DIR="$T" SF_TEMPLATE_REF=main SF_TEMPLATE_CARGO="$sb/fake/cargo" FAKE_LOG="$sb/cargo.log" FAKE_GATE="$sb/gate"
    printf '#!/usr/bin/env bash\ncat "%s/rustc.txt"\n' "$sb" > "$sb/fake/rustc"
    cat > "$sb/fake/cargo" <<'EOF'
#!/usr/bin/env bash
echo "${SF_CARGO_QUEUE_CLASS:-none} $*" >> "$FAKE_LOG"
[ "$1" != clean ] || { rm -rf "${CARGO_TARGET_DIR:?}"/*; exit 0; }
if [ -n "${FAKE_BLOCK:-}" ] && [ ! -e "$FAKE_BLOCK.done" ]; then : > "$FAKE_BLOCK.done"; echo blocked >> "$FAKE_BLOCK"; cat "$FAKE_GATE" > /dev/null; fi
[ -z "${FAKE_FAIL:-}" ] || exit 1
d="$CARGO_TARGET_DIR/debug"; r="$CARGO_TARGET_DIR/release"
mkdir -p "$d/deps" "$r/deps" "$d/.fingerprint/foo-0123456789abcdef" "$d/build/bs-aaaaaaaaaaaaaaaa/out"
head -c 65536 /dev/zero > "$d/deps/libfoo-0123456789abcdef.rlib"; : > "$d/deps/libfoo-0123456789abcdef.d"; : > "$r/deps/libbar-fedcba9876543210.rlib"
: > "$d/build/bs-aaaaaaaaaaaaaaaa/out/gen.rs"; mkdir -p "$d/.fingerprint/bs-aaaaaaaaaaaaaaaa"
printf '{"reason":"compiler-artifact","filenames":["%s","%s"],"fresh":true}\n' "$d/deps/libfoo-0123456789abcdef.rlib" "$r/deps/libbar-fedcba9876543210.rlib"
printf '{"reason":"build-script-executed","out_dir":"%s"}\nnot json\n' "$d/build/bs-aaaaaaaaaaaaaaaa/out"
EOF
    chmod +x "$sb/fake/cargo" "$sb/fake/rustc"
    export PATH="$sb/fake:$PATH"
    echo "rustc 1.98.1" > "$sb/rustc.txt"
    tt() { "$here/template-target.sh" "$@"; }
    clog() { cat "$FAKE_LOG" 2> /dev/null || true; }
    calls() { grep -vc ' clean$' "$FAKE_LOG" || true; }

    # Refresh, cold: exactly the four commands, each in the low class, no cargo clean, a stamp.
    tt refresh > "$sb/out" 2>&1 || fail "template refresh: $(cat "$sb/out")"
    eq "$(clog)" "low nextest run --workspace --no-run --cargo-message-format json
low clippy --workspace --all-targets --message-format json
low build -p stokesforge-app --message-format json
low test -p stokesforge-app --release --lib --no-run --message-format json" "template refresh: the command list (all in the low class)"
    [ -f "$T/stamp" ] && [ -f "$T/src/f" ] && [ -f "$T/target/debug/deps/libfoo-0123456789abcdef.rlib" ] || fail "template refresh: stamp, source or target missing"

    # Sweep: stale files go, listed files and their .d stay, incremental/ is removed.
    d="$T/target/debug"
    mkdir -p "$d/incremental/x" "$d/build/old-bbbbbbbbbbbbbbbb" "$d/.fingerprint/stale-cccccccccccccccc"
    : > "$d/deps/libstale-cccccccccccccccc.rlib"; : > "$d/deps/libstale-cccccccccccccccc.d"
    : > "$FAKE_LOG"
    tt refresh > "$sb/out" 2>&1 || fail "template refresh 2: $(cat "$sb/out")"
    [ ! -e "$d/deps/libstale-cccccccccccccccc.rlib" ] && [ ! -e "$d/deps/libstale-cccccccccccccccc.d" ] || fail "sweep: a stale deps file survived"
    [ ! -e "$d/.fingerprint/stale-cccccccccccccccc" ] && [ ! -e "$d/build/old-bbbbbbbbbbbbbbbb" ] || fail "sweep: a stale fingerprint or build dir survived"
    [ ! -e "$d/incremental" ] || fail "sweep: incremental/ survived"
    [ -f "$d/deps/libfoo-0123456789abcdef.rlib" ] && [ -f "$d/deps/libfoo-0123456789abcdef.d" ] || fail "sweep: a listed file or its .d was deleted"
    [ -d "$d/.fingerprint/foo-0123456789abcdef" ] && [ -f "$d/build/bs-aaaaaaaaaaaaaaaa/out/gen.rs" ] && [ -f "$T/target/release/deps/libbar-fedcba9876543210.rlib" ] || fail "sweep: a kept unit lost its fingerprint, build dir or release file"
    eq "$(grep -c ' clean$' "$FAKE_LOG")" 0 "same toolchain, small target: no cargo clean"

    # A failed command sweeps nothing and leaves the stamp alone.
    : > "$d/deps/libkeep-dddddddddddddddd.rlib"; : > "$FAKE_LOG"; before="$(cat "$T/stamp")"
    if FAKE_FAIL=1 tt refresh > "$sb/out" 2>&1; then fail "a failing build was accepted"; fi
    [ -f "$d/deps/libkeep-dddddddddddddddd.rlib" ] || fail "a failed refresh swept the target"
    eq "$(cat "$T/stamp")" "$before" "a failed refresh rewrote the stamp"
    rm -f "$d/deps/libkeep-dddddddddddddddd.rlib"

    # A toolchain change cleans first; a target above the threshold after the sweep cleans and rebuilds once.
    echo "rustc 1.99.0" > "$sb/rustc.txt"; : > "$FAKE_LOG"
    tt refresh > "$sb/out" 2>&1 || fail "toolchain refresh: $(cat "$sb/out")"
    eq "$(head -n 1 "$FAKE_LOG")" "none clean" "toolchain changed: clean comes first"
    eq "$(calls)" 4 "toolchain changed: one rebuild"
    : > "$FAKE_LOG"
    SF_TEMPLATE_MAX_KB=1 tt refresh > "$sb/out" 2>&1 || fail "oversize refresh: $(cat "$sb/out")"
    eq "$(grep -c ' clean$' "$FAKE_LOG")" 1 "oversize target: exactly one clean"
    eq "$(calls)" 8 "oversize target: sweep, clean, one more build"
    grep -q 'WARNING: still above' "$sb/out" || fail "oversize after a clean build is not reported"
    : > "$FAKE_LOG"
    SF_TEMPLATE_CARGO="$sb/none" tt refresh > "$sb/out" 2>&1 && fail "refresh without the shim accepted"
    grep -q 'not installed' "$sb/out" || fail "missing shim message: $(cat "$sb/out")"

    # A refresh asked while one runs: pending, exit 0, and the running one does one more pass.
    : > "$FAKE_LOG"; rm -f "$sb/blocked" "$sb/blocked.done"
    FAKE_BLOCK="$sb/blocked" tt refresh > "$sb/out1" 2>&1 &
    bg=$!
    "$here/wait-for.sh" "$sb/blocked" blocked 30 > /dev/null || fail "pending: the first refresh never started"
    tt refresh > "$sb/out2" 2>&1 || fail "pending: the second refresh failed: $(cat "$sb/out2")"
    [ -e "$T/pending" ] || fail "pending: not written"
    echo go > "$sb/gate"
    wait "$bg" || fail "pending: the first refresh failed: $(cat "$sb/out1")"
    eq "$(calls)" 8 "pending: a second pass ran"
    [ ! -e "$T/pending" ] || fail "pending: left behind"
    unset bg

    # Copy into a new worktree: both profiles, no incremental, real copies (inodes differ).
    mkdir -p "$sb/wt1"; tt copy "$sb/wt1" > "$sb/out" 2>&1 || fail "copy: $(cat "$sb/out")"
    grep -q 'copied' "$sb/out" || fail "copy message: $(cat "$sb/out")"
    [ -f "$sb/wt1/target/debug/deps/libfoo-0123456789abcdef.rlib" ] && [ -f "$sb/wt1/target/release/deps/libbar-fedcba9876543210.rlib" ] || fail "copy: debug or release missing"
    [ ! -e "$sb/wt1/target/debug/incremental" ] || fail "copy: incremental copied"
    [ "$(stat -c %i "$sb/wt1/target/debug/deps/libfoo-0123456789abcdef.rlib")" != "$(stat -c %i "$d/deps/libfoo-0123456789abcdef.rlib")" ] || fail "copy: hard link"
    # A worktree that already has a target is left alone.
    mkdir -p "$sb/wt2/target"; echo mine > "$sb/wt2/target/planted"
    tt copy "$sb/wt2" > "$sb/out" 2>&1 || fail "copy onto an existing target: $(cat "$sb/out")"
    eq "$(ls -A "$sb/wt2/target")" planted "copy onto an existing target changed it"
    # Free space: less than 50 GB would remain.
    mkdir -p "$sb/wt3"; SF_DF_AVAIL_KB=1000 tt copy "$sb/wt3" > "$sb/out" 2>&1 || fail "copy low space: $(cat "$sb/out")"
    [ ! -e "$sb/wt3/target" ] && grep -q 'would stay free' "$sb/out" || fail "copy low space: $(cat "$sb/out")"
    # Template lock held exclusive (a refresh runs): no copy, one line, no wait.
    mkdir -p "$sb/wt4"; rm -f "$sb/held"
    flock -x "$T/lock" bash -c "echo held > '$sb/held'; cat '$sb/gate' > /dev/null" &
    bg=$!
    "$here/wait-for.sh" "$sb/held" held 30 > /dev/null || fail "lock holder never started"
    set +e; timeout 5 "$here/template-target.sh" copy "$sb/wt4" > "$sb/out" 2>&1; rc=$?; set -e
    echo go > "$sb/gate"; wait "$bg"
    eq "$rc" 0 "copy with the lock held: exit code (124 = it waited)"
    [ ! -e "$sb/wt4/target" ] && grep -q 'refresh holds' "$sb/out" || fail "copy with the lock held: $(cat "$sb/out")"
    # No template: no error.
    mkdir -p "$sb/wt5"; SF_TEMPLATE_DIR="$sb/nothing" tt copy "$sb/wt5" > "$sb/out" 2>&1 || fail "no template: $(cat "$sb/out")"
    grep -q 'none yet' "$sb/out" && [ ! -e "$sb/wt5/target" ] || fail "no template message: $(cat "$sb/out")"

    # chantier.sh: the briefing says it; a rerun on an existing worktree copies nothing; --dry-run copies nothing.
    chant() { ( cd "$sb/StokesForge" && "$here/chantier.sh" "$@" ); }
    export SF_ISSUE_TITLE="Template copy"
    chant --dry-run 21 > "$sb/out" 2>&1 || fail "chantier dry run: $(cat "$sb/out")"
    grep -q 'would copy' "$sb/out" && [ ! -e "$sb/_stokesforge/feature21" ] || fail "dry run: $(cat "$sb/out")"
    chant 21 > "$sb/out" 2>&1 || fail "chantier: $(cat "$sb/out")"
    grep -q '^Build cache #21: template: copied' "$sb/out" || fail "briefing line: $(cat "$sb/out")"
    [ -f "$sb/_stokesforge/feature21/target/debug/deps/libfoo-0123456789abcdef.rlib" ] || fail "chantier did not copy"
    rm -rf "$sb/_stokesforge/feature21/target"
    chant 21 > "$sb/out" 2>&1 || fail "chantier rerun: $(cat "$sb/out")"
    [ ! -e "$sb/_stokesforge/feature21/target" ] || fail "chantier rerun copied into an existing worktree"
    [ "$(wc -l < "$sb/out")" -le 30 ] || fail "briefing too long with the build cache line"
    unset SF_ISSUE_TITLE

    # Prune: paused 3 days or more lose incremental and release; deps stay; held, line, recordless and
    # build-locked worktrees are untouched.
    for w in 31 32 33 34 35 line-PostV1.1; do
        mkdir -p "$sb/_stokesforge/feature$w/target/debug/incremental" "$sb/_stokesforge/feature$w/target/debug/deps" "$sb/_stokesforge/feature$w/target/release"
        [ "$w" != line-PostV1.1 ] || mv "$sb/_stokesforge/feature$w" "$sb/_stokesforge/$w"
    done
    for w in feature31 feature32 feature33 feature34 feature35 line-PostV1.1; do
        : > "$sb/_stokesforge/$w/target/debug/deps/keep"; : > "$sb/_stokesforge/$w/target/release/r"; : > "$sb/_stokesforge/$w/target/debug/incremental/i"
        : > "$sb/_stokesforge/$w/target/debug/.cargo-build-lock"
    done
    cat > "$sb/status.json" <<'JSON'
{"colours": [{"free": false, "issue": 33, "days": 9, "worktrees": ["feature33"]}],
 "paused": [{"issue": 31, "days": 5, "worktrees": ["feature31"]}, {"issue": 32, "days": 1, "worktrees": ["feature32"]},
            {"issue": 34, "days": 5, "worktrees": ["feature34"]}, {"issue": 36, "days": 8, "worktrees": ["line-PostV1.1"]}],
 "colourless": ["feature35"], "done": []}
JSON
    rm -f "$sb/held2"
    flock -x "$sb/_stokesforge/feature34/target/debug/.cargo-build-lock" bash -c "echo held > '$sb/held2'; cat '$sb/gate' > /dev/null" &
    bg=$!
    "$here/wait-for.sh" "$sb/held2" held 30 > /dev/null || fail "build lock holder never started"
    SF_TEMPLATE_STATUS_JSON="$sb/status.json" tt prune > "$sb/out" 2>&1 || fail "prune: $(cat "$sb/out")"
    echo go > "$sb/gate"; wait "$bg"
    w="$sb/_stokesforge"
    [ ! -e "$w/feature31/target/debug/incremental" ] && [ ! -e "$w/feature31/target/release" ] && [ -f "$w/feature31/target/debug/deps/keep" ] || fail "prune: a worktree paused 5 days was not pruned (or lost deps)"
    for k in feature32 feature33 feature34 feature35 line-PostV1.1; do
        [ -f "$w/$k/target/release/r" ] && [ -f "$w/$k/target/debug/incremental/i" ] || fail "prune touched $k"
    done
    grep -q 'kept feature34' "$sb/out" || fail "prune: the busy worktree is not reported: $(cat "$sb/out")"
    unset bg
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
    "fmt --all") [ "${FAKE_RED:-}" != fmt ] || exit 1 ;;
    "test --workspace") [ "${FAKE_RED:-}" != doc ] || { [[ " $* " == *" --doc "* ]] && exit 1; } ;;
esac
exit 0
EOF
chmod +x "$fc/cargo"
# Runs a copy of test-all.sh inside a sandbox git repo: the real repo never gets a stamp (#515).
tas="$tmp/tasb"; mkdir -p "$tas/scripts"
cp "$here/test-all.sh" "$tas/scripts/test-all.sh"
printf 'import os, sys\nsys.exit(1 if os.environ.get("FAKE_RED") == "french" else 0)\n' > "$tas/scripts/french-guard.py"
git init -q -b main "$tas"; git -C "$tas" config user.name t; git -C "$tas" config user.email t@t
git -C "$tas" add scripts; git -C "$tas" commit -q -m init
ta() { : > "$tmp/fake.log"; PATH="$fc:$PATH" FAKE_LOG="$tmp/fake.log" "$tas/scripts/test-all.sh" "$@" > /dev/null 2>&1; }
ta || fail "test-all: nextest present, exit code"
eq "$(grep -v -- '--version' "$tmp/fake.log")" "fmt --all -- --check
nextest run --workspace --no-fail-fast
test --workspace --doc" "test-all: fmt check, then nextest, then doc tests, in order"
FAKE_NEXTEST=0 ta || fail "test-all: nextest absent, exit code"
eq "$(grep -v -- '--version' "$tmp/fake.log")" "fmt --all -- --check
test --workspace" "test-all: nextest absent runs the fmt check and cargo test --workspace only"
set +e; FAKE_RED=nextest ta; rc=$?; set -e
[ "$rc" -ne 0 ] || fail "test-all: red nextest must fail"
set +e; FAKE_RED=doc ta; rc=$?; set -e
[ "$rc" -ne 0 ] || fail "test-all: red doc tests must fail"
set +e; ta some_filter; rc=$?; set -e
[ "$rc" -ne 0 ] || fail "test-all: an argument must be refused"
eq "$(grep -c . "$tmp/fake.log")" 0 "test-all: a refused argument runs no cargo"

# The stamp (#515): written for HEAD after a green run on a clean tree, in the common dir.
tstamp="$tas/.git/sf-tested/$(git -C "$tas" rev-parse HEAD)"
rm -rf "$tas/.git/sf-tested"
ta || fail "test-all: clean tree, exit code"
[ -e "$tstamp" ] || fail "test-all: no stamp after a green run on a clean tree"
rm -rf "$tas/.git/sf-tested"
set +e; FAKE_RED=nextest ta; set -e
[ ! -e "$tstamp" ] || fail "test-all: a red run left a stamp"
set +e; FAKE_RED=doc ta; set -e
[ ! -e "$tstamp" ] || fail "test-all: red doc tests left a stamp"
set +e; FAKE_RED=fmt ta; rc=$?; set -e
[ "$rc" -ne 0 ] && [ ! -e "$tstamp" ] || fail "test-all: a fmt failure must fail and leave no stamp"
set +e; FAKE_RED=french ta; rc=$?; set -e
[ "$rc" -ne 0 ] && [ ! -e "$tstamp" ] || fail "test-all: a French-guard failure must fail and leave no stamp"
echo x > "$tas/tracked"; git -C "$tas" add tracked; git -C "$tas" commit -q -m t
tstamp="$tas/.git/sf-tested/$(git -C "$tas" rev-parse HEAD)"
echo y >> "$tas/tracked"   # a modified tracked file
ta || fail "test-all: dirty tree, exit code"
[ ! -e "$tstamp" ] || fail "test-all: a dirty tree got a stamp"
git -C "$tas" checkout -q tracked
touch "$tas/untracked"
ta || fail "test-all: untracked file, exit code"
[ ! -e "$tstamp" ] || fail "test-all: a tree with an untracked file got a stamp"
rm "$tas/untracked"
# Old stamps are pruned, recent ones kept.
mkdir -p "$tas/.git/sf-tested"; : > "$tas/.git/sf-tested/old"; : > "$tas/.git/sf-tested/recent"
touch -d '40 days ago' "$tas/.git/sf-tested/old"
ta || fail "test-all: prune run"
[ ! -e "$tas/.git/sf-tested/old" ] && [ -e "$tas/.git/sf-tested/recent" ] || fail "test-all: pruning"
# None of the above touched the real repository.
eq "$( (ls "$real_common/sf-tested" 2> /dev/null || true) | sort | md5sum)" "$stamps_before" "workflow-test changed the stamps of the real repository"

echo "workflow-test: all passed"

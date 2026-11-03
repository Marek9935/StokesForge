#!/usr/bin/env bash
# Start a work package from the current Claude session (#413, #417).
#
#   scripts/chantier.sh [--dry-run] <issue-number>...
#
# Run BY the orchestrator, in the current session, from the main checkout (the
# user starts plain `claude` or does `/clear`, then types `lance #<n>`). It starts
# no session. For each issue, if ../_stokesforge/feature<n> is missing, creates it
# from an up-to-date origin/main on branch feature/<n>-<slug> (slug from the GitLab
# title; no upstream tracking, so `main` is never the upstream), then CLAIMS the
# issue for this session with scripts/suivi.py (#441).
#
# Colours live in ../_stokesforge/.suivi, one record per issue (scripts/suivi.py;
# its header states the rule). An issue holds its colour while an agent session
# owns it, a QCM of the issue is open or a trial entry awaits the user; otherwise
# it is paused: colour free, worktree kept. A rerun (after /clear) finds the same
# worktree, becomes the new owner and gets the previous colour back if it is still
# free, else the first free of the six squares. The other worktrees of the issue
# (feature426b, feature426d, ...) share the colour of its record.
# Then prints a short briefing for the agent: the tag "<square> #<n> <short name>"
# of each issue, its worktree and branch, and the steps to follow.
#
# Short name: the first 3 words of the title, in the issue's own language. Several
# issues: one worktree each, one session. Seven work packages cannot run at once:
# with the six colours held the script refuses with a message and a non-zero exit.
#
# Work package with sub-issues (#419): if open issues name an issue as their
# parent ("Parent: #N", "Sous-ticket du chantier #N", "Suite du chantier #N",
# "sub-issue of #N", or a title ending "(#N)", any case), the issue is a PACKAGE.
# The briefing lists its sub-issues in the order the parent's description first
# mentions them (then by number): that list is the first card of the single start
# form. No worktree is created for the package itself: after the form, run
#   scripts/chantier.sh <parent> --start <sub-issue>
# once per sub-issue, when it starts (parallel ones may share a call). One colour
# per package (the record of the parent); every sub-issue worktree gets it. Between
# two sub-issues the package keeps it only while a session owns the parent (the
# claim made by `lance #<parent>`); after /clear or a crash it is paused like any
# issue. The tag is "<square> #<parent> › #<sub> <name>" while a sub-issue is in
# progress.
#   scripts/chantier.sh --done <parent>
# marks the package finished after the last merge (refused while a worktree of the
# package exists).
#   scripts/chantier.sh --pause <n>
# drops the owner of any issue or package: its colour is freed unless a QCM or a
# trial holds it; worktrees are kept (a package: refused while one of its
# worktrees exists). A later claim takes the previous colour back if free.
#
# Colours (#441):
#   scripts/chantier.sh --status [--json]
# is `scripts/suivi.py status`: the six colours, free or held (issue, why held,
# worktrees, since, days), then paused issues and worktrees without a record.
#   scripts/chantier.sh [--dry-run] --release <n>|<n><letters>
# from the main checkout on `main`: removes ONE merged, clean worktree
# (feature<n>, feature426b, ...) and its merged local branch (never --force, never
# -D), then prints ONE line saying whether the colour is free or still held; the
# issue is marked done when its last worktree goes and nothing holds it.
#
# Test seams: SF_ISSUE_TITLE_<n> (or SF_ISSUE_TITLE for any issue) replaces the
# `glab issue view` lookup; SF_ISSUES_JSON (a file with the JSON of `glab issue
# list`) replaces the sub-issue search, SF_ISSUE_BODY_<n> the parent's description;
# SF_NO_FETCH=1 skips `git fetch`, SF_TODAY=YYYY-MM-DD fixes today, SF_SUIVI_DIR
# moves the state folder. --dry-run creates nothing and prints what it would create
# (or remove, pause).
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"
suivi() { python3 -I "$(dirname "$0")/suivi.py" "$@"; }

usage="usage: chantier.sh [--dry-run] [--start <sub-issue>]... <issue-number>... | --done <parent> | --pause <n> | --release <n>[<letters>] | --status [--json]"
dry=0
issues=()
starts=()
done_parent=""
pause_n=""
release_name=""
status=0
json=0
while [ "$#" -gt 0 ]; do
    case "$1" in
    --dry-run) dry=1 ;;
    --start | --done | --pause)
        case "${2:-}" in *[!0-9]* | '') echo "$usage" >&2; exit 2 ;; esac
        case "$1" in
        --start) starts+=("$2") ;;
        --done) done_parent="$2" ;;
        *) pause_n="$2" ;;
        esac
        shift ;;
    --release)
        [[ "${2:-}" =~ ^[0-9]+[a-z]*$ ]] || { echo "$usage" >&2; exit 2; }
        release_name="$2"
        shift ;;
    --status) status=1 ;;
    --json) json=1 ;;
    -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d'; exit 0 ;;
    *[!0-9]* | '') echo "$usage" >&2; exit 2 ;;
    *) issues+=("$1") ;;
    esac
    shift
done
[ "$json" = 0 ] || [ "$status" = 1 ] || { echo "$usage" >&2; exit 2; }
[ "${#issues[@]}" -gt 0 ] || [ -n "$done_parent$pause_n$release_name" ] || [ "$status" = 1 ] \
    || { echo "$usage" >&2; exit 2; }

main="$(sf_main_dir)"
root="$(realpath -m "$(sf_wt_root)")"

title_of() {
    local var="SF_ISSUE_TITLE_$1"
    if [ -n "${!var:-}" ]; then printf '%s\n' "${!var}"; return; fi
    if [ -n "${SF_ISSUE_TITLE:-}" ]; then printf '%s\n' "$SF_ISSUE_TITLE"; return; fi
    glab issue view "$1" -F json | python3 -c 'import json,sys; print(json.load(sys.stdin)["title"])'
}

# "Fix axis label: wrong side" -> "fix-axis-label" (at most 40 characters).
slug_of() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]\{1,\}/-/g; s/^-//; s/-$//' \
        | cut -c1-40 | sed 's/-$//'
}

# Short name: the title up to the first colon or dash, then its first 3 words,
# without a trailing small word ("Frames follow the" reads "Frames follow").
# $2 = maximum number of words (default 3).
short_of() {
    # The small-word list is English and French: issue titles may be French.
    printf '%s' "$1" | sed 's/ *[:—] .*//; s/ - .*//' | awk -v max="${2:-3}" '
        # lang: fr
        BEGIN { split("the a an of to in on for and with by at from de la le les des du et à au aux en un une pour par sur", w, " "); for (k in w) small[w[k]] = 1 }
        { n = (NF < max) ? NF : max; while (n > 1 && (tolower($n) in small)) n--; s = $1; for (i = 2; i <= n; i++) s = s " " $i; print s }'
}

# A one-word prefix before a colon is a domain ("Workflow: every message…"):
# keep it and add the first words after the colon, so the name says something.
name_of() {
    local head rest
    head="$(short_of "$1")"
    case "$1" in
    *:*) rest="${1#*:}" ;;
    *) rest="" ;;
    esac
    if [ -n "$rest" ] && [ "$(printf '%s' "$head" | wc -w)" -eq 1 ]; then
        printf '%s %s\n' "$head" "$(short_of "$(printf '%s' "$rest" | sed 's/^ *//')" 2)"
    else
        printf '%s\n' "$head"
    fi
}

reserved=""   # colours promised earlier in this dry run

# Open sub-issues of issue $1, one "<number><TAB><title>" line each, in the order
# the parent's description first mentions them, then by number (#419).
children_of() {
    local n="$1" list body var tmpf
    var="SF_ISSUE_BODY_$n"
    tmpf="$(mktemp)"
    if [ -n "${SF_ISSUES_JSON:-}" ]; then
        list="$SF_ISSUES_JSON"
    else
        list="$tmpf"
        # Every open issue, page by page (100 per page), merged into one JSON list.
        python3 -I - "$list" << 'PY' || echo '[]' > "$list"
import json, subprocess, sys
out, page = [], 1
while True:
    r = subprocess.run(["glab", "issue", "list", "--per-page", "100", "--page", str(page), "-O", "json"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        break
    try:
        batch = json.loads(r.stdout or "[]")
    except ValueError:
        break
    out += batch
    if len(batch) < 100:
        break
    page += 1
json.dump(out, open(sys.argv[1], "w"))
PY
    fi
    if [ -n "${!var+x}" ]; then body="${!var}"
    elif [ -n "${SF_ISSUES_JSON:-}" ]; then body=""
    else
        body="$(glab issue view "$n" -F json 2> /dev/null \
            | python3 -c 'import json,sys; print(json.load(sys.stdin).get("description") or "")' 2> /dev/null || true)"
    fi
    SF_PARENT_BODY="$body" python3 -I - "$n" "$list" << 'PY'
import json, os, re, sys
n, path = sys.argv[1], sys.argv[2]
body = os.environ.get("SF_PARENT_BODY", "")
try:
    issues = json.load(open(path))
except Exception:
    issues = []
# Case-insensitive; "(?!\d)" so that #2170 never matches #217.
# lang: fr
refs = [r"parent\s*:\s*#%s(?!\d)", r"sous-ticket\s+(?:du\s+chantier\s+|de\s+)#%s(?!\d)",
        r"suite\s+du\s+chantier\s+#%s(?!\d)", r"sub-issue\s+of\s+#%s(?!\d)"]
pats = [re.compile(r % n, re.I) for r in refs]
title_pat = re.compile(r"\(#%s\)\s*$" % n)
found = []
for it in issues:
    iid = str(it.get("iid", ""))
    if not iid or iid == n or it.get("state", "opened") != "opened":
        continue
    title, desc = it.get("title") or "", it.get("description") or ""
    # Text in backticks is quoted (an example, a command), never a parent link.
    desc = re.sub(r"```.*?```", " ", desc, flags=re.S)
    desc = re.sub(r"`[^`\n]*`", " ", desc)
    if any(p.search(desc) or p.search(title) for p in pats) or title_pat.search(title.strip()):
        found.append((iid, title.replace("\t", " ").replace("\n", " ")))
def rank(item):
    m = re.search(r"#%s(?!\d)" % item[0], body)
    return (m.start() if m else len(body) + 1, int(item[0]))
for iid, title in sorted(found, key=rank):
    print(f"{iid}\t{title}")
PY
    rm -f "$tmpf"
}

if [ "$status" = 1 ]; then
    if [ "$json" = 1 ]; then suivi status --json; else suivi status; fi
    exit $?
fi

# --release <name>: one merged, clean worktree and its merged local branch (#441).
# Every condition is checked before anything is removed. The colour line comes from suivi.py.
release_wt() {
    local name="$1" wt="$root/feature$1" known top branch ref issue dirty ignored
    known="$(sf_worktrees)"   # (not piped into grep -q: pipefail + SIGPIPE)
    if ! grep -qxF "$wt" <<< "$known" && ! grep -qxF "$(realpath -m "$wt")" <<< "$known"; then
        echo "release: no worktree $wt" >&2
        return 1
    fi
    top="$(realpath "$(git rev-parse --show-toplevel)")"
    if [ "$top" != "$(realpath "$main")" ]; then
        echo "release: run it from the main checkout ($main), not from $top" >&2
        return 1
    fi
    if [ "$(git -C "$main" symbolic-ref --short HEAD 2> /dev/null)" != main ]; then
        echo "release: the main checkout is not on the branch main" >&2
        return 1
    fi
    branch="$(git -C "$wt" symbolic-ref --short HEAD 2> /dev/null || true)"
    ref="${branch:-$(git -C "$wt" rev-parse HEAD)}"
    if ! git -C "$main" merge-base --is-ancestor "$ref" HEAD; then
        echo "release: ${branch:-HEAD of $wt} is not merged into main; nothing removed" >&2
        return 1
    fi
    dirty="$(git -C "$wt" status --porcelain)"
    if [ -n "$dirty" ]; then
        printf 'release: %s has modified or untracked files, nothing removed:\n%s\n' "$wt" "$dirty" >&2
        return 1
    fi
    ignored="$(git -C "$wt" status --porcelain --ignored | grep '^!! ' | grep -vx '!! target/' || true)"
    if [ -n "$ignored" ]; then
        printf 'release: %s has ignored files other than target/, nothing removed:\n%s\n' "$wt" "$ignored" >&2
        return 1
    fi
    issue="$(sf_issue_of "$wt")"
    if [ "$dry" = 1 ]; then
        echo "would run: git worktree remove $wt"
        [ -z "$branch" ] || echo "would run: git branch -d $branch"
        return 0
    fi
    git -C "$main" worktree remove "$wt" > /dev/null || return 1
    if [ -n "$branch" ]; then git -C "$main" branch -d "$branch" > /dev/null || return 1; fi
    suivi release-line "$issue" --mark-done
}

if [ -n "$release_name" ]; then
    release_wt "$release_name" || exit 1
    exit 0
fi

# Worktrees of a package (itself or a sub-issue), one path per line.
package_worktrees() {
    local members wt i
    members=" $(suivi members "$1") "
    sf_worktrees | while read -r wt; do
        [ -n "$wt" ] || continue
        i="$(sf_issue_of "$wt")"
        case "$members" in *" $i "*) printf '%s\n' "$wt" ;; esac
    done
}

# --pause <n>: drop the owner of any issue or package; the worktrees stay.
if [ -n "$pause_n" ]; then
    if suivi is-package "$pause_n"; then
        left="$(package_worktrees "$pause_n" | paste -sd' ' -)"
        if [ -n "$left" ]; then
            echo "package #$pause_n still has worktrees: $left (merge and remove them first)" >&2
            exit 1
        fi
    fi
    if [ "$dry" = 1 ]; then
        echo "would pause #$pause_n: drop its owner, keep its worktrees and sub-issue list"
        exit 0
    fi
    suivi pause "$pause_n"
    exit $?
fi

if [ -n "$done_parent" ]; then
    suivi is-package "$done_parent" || { echo "no package #$done_parent" >&2; exit 1; }
    left="$(package_worktrees "$done_parent" | paste -sd' ' -)"
    if [ -n "$left" ]; then
        echo "package #$done_parent still has worktrees: $left (merge and remove them first)" >&2
        exit 1
    fi
    suivi done "$done_parent"
    exit $?
fi

if [ "$dry" = 0 ] && [ -z "${SF_NO_FETCH:-}" ] && git -C "$main" remote | grep -qx origin; then
    git -C "$main" fetch origin
fi
base=main
git -C "$main" rev-parse --verify -q origin/main > /dev/null && base=origin/main

# Create (or find) the worktree of issue $1 titled $2 and claim the issue for this
# session. $3 = parent package, $4 = its colour (dry run only). Sets wt, colour, branch.
ensure_wt() {
    local n="$1" title="$2" parent="${3:-}" pcolour="${4:-}" args
    args=(--name "$(name_of "$title")")
    [ -z "$parent" ] || args+=(--parent "$parent")
    wt="$root/feature$n"
    if [ -d "$wt" ]; then
        branch="$(git -C "$wt" symbolic-ref --short HEAD 2> /dev/null || echo '?')"
        if [ "$dry" = 0 ]; then
            colour="$(suivi claim "$n" "${args[@]}")" || exit 1
        else
            colour="${pcolour:-$(suivi claim --dry-run "$n" "${args[@]}" --avoid "$reserved")}" || exit 1
        fi
        return
    fi
    branch="feature/$n-$(slug_of "$title")"
    if [ "$dry" = 1 ]; then
        if [ -n "$pcolour" ]; then
            colour="$pcolour"
        else
            colour="$(suivi claim --dry-run "$n" "${args[@]}" --avoid "$reserved")" || exit 1
            reserved="$reserved $colour"
        fi
        echo "would run: git worktree add $wt -b $branch --no-track $base"
    else
        suivi claim --dry-run "$n" "${args[@]}" > /dev/null || exit 1   # refuse before creating anything
        mkdir -p "$root"
        git -C "$main" worktree add "$wt" -b "$branch" --no-track "$base"
        colour="$(suivi claim "$n" "${args[@]}")" || exit 1
    fi
}

# Sub-issues of each given issue (one lookup per issue).
declare -A kids=()
for n in "${issues[@]}"; do kids[$n]="$(children_of "$n")"; done
for m in "${starts[@]:-}"; do
    [ -n "$m" ] || continue
    ok=0
    for n in "${issues[@]}"; do
        printf '%s\n' "${kids[$n]}" | cut -f1 | grep -qx "$m" && ok=1
    done
    [ "$ok" = 1 ] || { echo "--start $m: not a sub-issue of ${issues[*]}" >&2; exit 2; }
done

names=()
wts=()
brs=()
pkgnotes=()
for n in "${issues[@]}"; do
    title="$(title_of "$n")"
    if [ -z "${kids[$n]}" ]; then
        ensure_wt "$n" "$title"
        [ -n "$colour" ] || colour="?"
        names+=("$colour #$n $(name_of "$title")")
        wts+=("$wt")
        brs+=("$branch")
        continue
    fi
    # A package: one colour for all of it (the record of the parent).
    pname="$(name_of "$title")"
    subargs=()
    while IFS=$'\t' read -r m mt; do
        [ -z "$m" ] || subargs+=(--sub "$m=$mt")
    done <<< "${kids[$n]}"
    if [ "$dry" = 1 ]; then
        pcolour="$(suivi claim --dry-run "$n" --package --name "$pname" --avoid "$reserved")" || exit 1
        reserved="$reserved $pcolour"
    else
        pcolour="$(suivi claim "$n" --package --name "$pname" "${subargs[@]}")" || exit 1
    fi
    note="Package $pcolour #$n $pname: sub-issues, in this order (first card of the start form; the user removes or adds one):"
    k=0
    in_progress=0
    while IFS=$'\t' read -r m mt; do
        k=$((k + 1))
        started=0
        case " ${starts[*]:-} " in *" $m "*) started=1 ;; esac
        [ -d "$root/feature$m" ] && started=1
        mark=""
        if [ "$started" = 1 ]; then
            ensure_wt "$m" "$mt" "$n" "$pcolour"
            names+=("$pcolour #$n › #$m $(name_of "$mt")")
            wts+=("$wt")
            brs+=("$branch")
            in_progress=1
            mark="  <- in progress, worktree $wt"
        fi
        note="$note"$'\n'"  $k. #$m $(name_of "$mt")$mark"
    done <<< "${kids[$n]}"
    if [ "$in_progress" = 0 ]; then
        names+=("$pcolour #$n $pname")
        wts+=("(none yet: package)")
        brs+=("-")
        note="$note"$'\n'"No worktree yet. After the start form, run: scripts/chantier.sh $n --start <sub-issue>  (e.g. --start $(printf '%s\n' "${kids[$n]}" | head -n 1 | cut -f1))"
    else
        note="$note"$'\n'"Start the next sub-issue with: scripts/chantier.sh $n --start <sub-issue>  (one worktree per sub-issue, created when it starts)"
    fi
    pkgnotes+=("$note")
done

if [ "$dry" = 1 ]; then echo "(dry run: nothing created)"; fi
echo "Work package briefing (scripts/chantier.sh); the tag goes in forms, the tracking page and notifications:"
for i in "${!names[@]}"; do
    echo "  ${names[$i]}  worktree ${wts[$i]}  branch ${brs[$i]}"
done
for x in "${pkgnotes[@]:-}"; do [ -z "$x" ] || printf '%s\n' "$x"; done
tags=""
for x in "${names[@]}"; do tags="${tags:+$tags · }$x"; done
# The user tells parallel terminals apart by this tag, without reading (#418).
echo "Start EVERY message to the user with: $tags"
echo "Tracking page: $SF_TO_TEST_URL"
cat <<'BRIEFING'
Several issues: one worktree each, one session. Never work in another worktree.
Steps, in order:
 1. Read each issue, ALL its comments, the sibling issues, `git worktree list`
    (stokesforge-feature, section 1).
 2. `scripts/suivi.py status`: who holds which colour. Colours are claimed by this
    script and freed by `--release`, `--pause`, `--done`; never edit .suivi by hand.
 3. One start decision form for all open decisions, cards tagged with the issue's
    square (stokesforge-bug-report/references/decision-form.md). A package: one
    form for all of it, first card = the sub-issue list and order above.
 4. At each change of state: `scripts/suivi.py set <n> --state
    questions|code|essai|fusion --what "..." [--wait you|dep|ag --waiting "..."]`.
 5. Agent `perimetre`, then Sonnet implements in the worktree (CLAUDE.md, "Delegation").
 6. Visible change: `scripts/try.sh <n>` opens the branch on the user's screen;
    their OK is the go-ahead (stokesforge-verification, step 6).
 7. Merge under `scripts/main-lock.sh` from the main checkout (same step 6), read the CI.
 8. Add the trial entries (suivi.py test add / merged) (stokesforge-verification, step 8).
 9. Clean up: `scripts/chantier.sh --release <n>` (worktree and branch; copy its one
    line in the report). A package stopping with no worktree: `--pause <parent>`;
    after its last sub-issue: `--done <parent>`.
BRIEFING

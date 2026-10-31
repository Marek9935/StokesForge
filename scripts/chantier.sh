#!/usr/bin/env bash
# Start a work package from the current Claude session (#413, #417).
#
#   scripts/chantier.sh [--dry-run] <issue-number>...
#
# Run BY the orchestrator, in the current session, from the main checkout (the
# user starts plain `claude` or does `/clear`, then types `lance #<n>`). It starts
# no session. For each issue, if ../_stokesforge/feature<n> is missing, creates it
# from an up-to-date origin/main on branch feature/<n>-<slug> (slug from the GitLab
# title; no upstream tracking, so `main` is never the upstream). The colour is
# the first free of the six squares, kept until the worktree is removed: rerunning
# the script (after a `/clear`) finds the worktree and prints the same colour.
# Then prints a short briefing for the agent: the tag "<square> #<n> <short name>"
# of each issue, its worktree and branch, and the steps to follow.
#
# Short name: the first 3 words of the title, in the issue's own language. Several
# issues: one worktree each, one session. Seven work packages cannot run at once:
# with the six colours taken the script refuses with a message and a non-zero exit.
#
# Work package with sub-issues (#419): if open issues name an issue as their
# parent ("Parent: #N", "Sous-ticket du chantier #N", "Suite du chantier #N",
# "sub-issue of #N", or a title ending "(#N)", any case), the issue is a PACKAGE.
# The briefing lists its sub-issues in the order the parent's description first
# mentions them (then by number): that list is the first card of the single start
# form. No worktree is created for the package itself: after the form, run
#   scripts/chantier.sh <parent> --start <sub-issue>
# once per sub-issue, when it starts (parallel ones may share a call). One colour
# per package, reserved in ../_stokesforge/.packages/<parent> between two
# sub-issues; every sub-issue worktree gets it. The tag is
# "<square> #<parent> › #<sub> <name>" while a sub-issue is in progress.
#   scripts/chantier.sh --done <parent>
# releases the reservation after the last merge (refused while a worktree of the
# package exists).
#
# Test seams: SF_ISSUE_TITLE_<n> (or SF_ISSUE_TITLE for any issue) replaces the
# `glab issue view` lookup; SF_ISSUES_JSON (a file with the JSON of `glab issue
# list`) replaces the sub-issue search, SF_ISSUE_BODY_<n> the parent's description;
# SF_NO_FETCH=1 skips `git fetch`. --dry-run creates nothing and prints what it
# would create.
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

usage="usage: chantier.sh [--dry-run] [--start <sub-issue>]... <issue-number>... | --done <parent>"
dry=0
issues=()
starts=()
done_parent=""
while [ "$#" -gt 0 ]; do
    case "$1" in
    --dry-run) dry=1 ;;
    --start | --done)
        case "${2:-}" in *[!0-9]* | '') echo "$usage" >&2; exit 2 ;; esac
        if [ "$1" = --start ]; then starts+=("$2"); else done_parent="$2"; fi
        shift ;;
    -h | --help) sed -n '2,32p' "$0"; exit 0 ;;
    *[!0-9]* | '') echo "$usage" >&2; exit 2 ;;
    *) issues+=("$1") ;;
    esac
    shift
done
[ "${#issues[@]}" -gt 0 ] || [ -n "$done_parent" ] || { echo "$usage" >&2; exit 2; }

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

all_taken() {
    local all=""
    for c in "${SF_COLOURS[@]}"; do all="$all$c"; done
    echo "6 work packages in progress ($all): finish or remove one first (git worktree list)" >&2
    exit 1
}
reserved=""   # colours promised in this run (dry-run only)

# First colour held by no worktree, no reserved package and no earlier dry-run promise.
free_colour() {
    local c
    for c in "${SF_COLOURS[@]}"; do
        if ! { sf_used_colours; printf '%s\n' $reserved; } | grep -xF "$c" > /dev/null; then
            printf '%s\n' "$c"
            return
        fi
    done
}

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
        glab issue list --per-page 100 -O json > "$list" 2> /dev/null || echo '[]' > "$list"
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
refs = [r"parent\s*:\s*#%s(?!\d)", r"sous-ticket\s+du\s+chantier\s+#%s(?!\d)",
        r"suite\s+du\s+chantier\s+#%s(?!\d)", r"sub-issue\s+of\s+#%s(?!\d)"]
pats = [re.compile(r % n, re.I) for r in refs]
title_pat = re.compile(r"\(#%s\)\s*$" % n)
found = []
for it in issues:
    iid = str(it.get("iid", ""))
    if not iid or iid == n or it.get("state", "opened") != "opened":
        continue
    title, desc = it.get("title") or "", it.get("description") or ""
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

if [ -n "$done_parent" ]; then
    f="$(sf_pkg_dir)/$done_parent"
    [ -f "$f" ] || { echo "no colour reserved for package #$done_parent" >&2; exit 1; }
    members=" $done_parent $(tail -n +2 "$f" | cut -f1 | tr '\n' ' ')"
    left=""
    while read -r wt; do
        [ -n "$wt" ] || continue
        i="$(sf_issue_of "$wt")"
        case "$members" in *" $i "*) left="$left $wt" ;; esac
    done < <(sf_worktrees)
    if [ -n "$left" ]; then
        echo "package #$done_parent still has worktrees:$left (merge and remove them first)" >&2
        exit 1
    fi
    colour="$(sf_pkg_colour "$done_parent")"
    rm -f "$f"
    echo "package #$done_parent released ($colour free again)"
    exit 0
fi

if [ "$dry" = 0 ] && [ -z "${SF_NO_FETCH:-}" ] && git -C "$main" remote | grep -qx origin; then
    git -C "$main" fetch origin
fi
base=main
git -C "$main" rev-parse --verify -q origin/main > /dev/null && base=origin/main

# Create (or find) the worktree of issue $1 titled $2; $3 = the package colour, if
# any. Sets wt, colour, branch.
ensure_wt() {
    local n="$1" title="$2" pkgcolour="${3:-}"
    wt="$root/feature$n"
    if [ -d "$wt" ]; then
        colour="$(sf_colour_of "$wt")"
        branch="$(git -C "$wt" symbolic-ref --short HEAD 2> /dev/null || echo '?')"
        if [ -z "$colour" ] && [ "$dry" = 0 ]; then
            colour="${pkgcolour:-$(sf_first_free)}"
            [ -n "$colour" ] || all_taken
            printf '%s\n' "$colour" > "$(sf_colour_file "$wt")"
        fi
        return
    fi
    colour="$pkgcolour"
    if [ -z "$colour" ]; then
        colour="$(sf_first_free)"
        if [ "$dry" = 1 ]; then
            colour="$(free_colour)"
            reserved="$reserved $colour"
        fi
    fi
    branch="feature/$n-$(slug_of "$title")"
    [ -n "$colour" ] || all_taken
    if [ "$dry" = 1 ]; then
        echo "would run: git worktree add $wt -b $branch --no-track $base"
    else
        mkdir -p "$root"
        sf_with_lock "$root/.colour.lock" bash -c '
            set -e
            . "$1/workflow-lib.sh"
            colour="${6:-$(sf_first_free)}"
            [ -n "$colour" ] || { echo "6 work packages in progress: finish or remove one first" >&2; exit 1; }
            git -C "$2" worktree add "$3" -b "$4" --no-track "$5"
            printf "%s\n" "$colour" > "$(sf_colour_file "$3")"
        ' _ "$(dirname "$0")" "$main" "$wt" "$branch" "$base" "$pkgcolour"
        colour="$(sf_colour_of "$wt")"
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
    # A package: one colour for all of it, reserved under the colour lock.
    pcolour="$(sf_pkg_colour "$n")"
    if [ -z "$pcolour" ]; then
        if [ "$dry" = 1 ]; then
            pcolour="$(free_colour)"
            reserved="$reserved $pcolour"
        else
            mkdir -p "$root"
            printf '%s\n' "${kids[$n]}" > "$root/.pkglist.$$"
            pcolour="$(sf_with_lock "$root/.colour.lock" bash -c '
                . "$1/workflow-lib.sh"; sf_pkg_reserve "$2" "$3"' _ "$(dirname "$0")" "$n" "$root/.pkglist.$$")" || true
            rm -f "$root/.pkglist.$$"
        fi
        [ -n "$pcolour" ] || all_taken
    elif [ "$dry" = 0 ]; then
        # Rerun: keep the colour, refresh the list.
        printf '%s\n' "${kids[$n]}" > "$root/.pkglist.$$"
        sf_with_lock "$root/.colour.lock" bash -c '
            . "$1/workflow-lib.sh"; sf_pkg_reserve "$2" "$3" > /dev/null' _ "$(dirname "$0")" "$n" "$root/.pkglist.$$"
        rm -f "$root/.pkglist.$$"
    fi
    pname="$(name_of "$title")"
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
            ensure_wt "$m" "$mt" "$pcolour"
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
echo "Work package briefing (scripts/chantier.sh); the tag goes in forms, the To-test page and notifications:"
for i in "${!names[@]}"; do
    echo "  ${names[$i]}  worktree ${wts[$i]}  branch ${brs[$i]}"
done
for x in "${pkgnotes[@]:-}"; do [ -z "$x" ] || printf '%s\n' "$x"; done
tags=""
for x in "${names[@]}"; do tags="${tags:+$tags · }$x"; done
# The user tells parallel terminals apart by this tag, without reading (#418).
echo "Start EVERY message to the user with: $tags"
echo "To-test page: $SF_TO_TEST_URL"
cat <<'EOF'
Several issues: one worktree each, one session. Never work in another worktree.
Steps, in order:
 1. Read each issue, ALL its comments, the sibling issues, `git worktree list`
    (stokesforge-feature, section 1).
 2. Register the work package in the `chantiers` collection of the To-test page
    (link above; procedure in stokesforge-verification/references/to-test-page.md).
 3. One start decision form for all open decisions, cards tagged with the issue's
    square (stokesforge-bug-report/references/decision-form.md). A package: one
    form for all of it, first card = the sub-issue list and order above.
 4. Agent `perimetre`, then Sonnet implements in the worktree (CLAUDE.md, "Delegation").
 5. Visible change: `scripts/try.sh <n>` opens the branch on the user's screen;
    their OK is the go-ahead (stokesforge-verification, step 6).
 6. Merge under `scripts/main-lock.sh` from the main checkout (same step 6), read the CI.
 7. Add the To-test entry, delete the `chantiers/<n>` document (step 8). A package:
    one entry per merged sub-issue; the `chantiers/<parent>` document stays until the last.
 8. Clean up the worktree and branch (section "Clean up after a merge on main");
    after the last sub-issue, `scripts/chantier.sh --done <parent>` frees the colour.
EOF

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
# the first free of the six squares. One colour per ISSUE: its other worktrees
# (feature426b, feature426d, ...) share it; rerunning the script (after a
# `/clear`) finds the worktrees, gives the colourless ones the issue's colour and
# prints the same colour. It is freed by `--release`, not by hand.
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
#   scripts/chantier.sh --pause <parent>
# frees the package colour while no sub-issue is under way (the sub-issue list is
# kept; refused while a worktree of the package exists). A later `--start` takes
# the first free colour again.
#
# Colours (#441):
#   scripts/chantier.sh --status [--json]
# the six colours, free or held (issue, worktrees or package, since, days), the
# worktrees without a colour and the paused packages; the orchestrator compares it
# with the `chantiers` collection of the To-test page (colour check).
#   scripts/chantier.sh [--dry-run] --release <n>|<n><letters>
# from the main checkout on `main`: removes ONE merged, clean worktree
# (feature<n>, feature426b, ...) and its merged local branch (never --force, never
# -D), then prints ONE line saying whether the colour is free or still held.
#
# Test seams: SF_ISSUE_TITLE_<n> (or SF_ISSUE_TITLE for any issue) replaces the
# `glab issue view` lookup; SF_ISSUES_JSON (a file with the JSON of `glab issue
# list`) replaces the sub-issue search, SF_ISSUE_BODY_<n> the parent's description;
# SF_NO_FETCH=1 skips `git fetch`, SF_TODAY=YYYY-MM-DD fixes today for --status.
# --dry-run creates nothing and prints what it would create (or remove, pause).
set -euo pipefail
. "$(dirname "$0")/workflow-lib.sh"

usage="usage: chantier.sh [--dry-run] [--start <sub-issue>]... <issue-number>... | --done <parent> | --pause <parent> | --release <n>[<letters>] | --status [--json]"
dry=0
issues=()
starts=()
done_parent=""
pause_parent=""
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
        *) pause_parent="$2" ;;
        esac
        shift ;;
    --release)
        [[ "${2:-}" =~ ^[0-9]+[a-z]*$ ]] || { echo "$usage" >&2; exit 2; }
        release_name="$2"
        shift ;;
    --status) status=1 ;;
    --json) json=1 ;;
    -h | --help) sed -n '2,55p' "$0"; exit 0 ;;
    *[!0-9]* | '') echo "$usage" >&2; exit 2 ;;
    *) issues+=("$1") ;;
    esac
    shift
done
[ "$json" = 0 ] || [ "$status" = 1 ] || { echo "$usage" >&2; exit 2; }
[ "${#issues[@]}" -gt 0 ] || [ -n "$done_parent$pause_parent$release_name" ] || [ "$status" = 1 ] \
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

# The fixed lines printed by --pause, --release and --done: the user reads them in
# the orchestrator's report, copied as they are.
# lang: fr begin
colour_line() {
    case "$1" in
    freed) printf 'Couleur %s libérée.\n' "$2" ;;
    kept) printf 'Couleur %s reste prise : %s\n' "$2" "$3" ;;
    nocolour) printf 'Ce dossier ne tenait aucune couleur.\n' ;;
    paused) printf 'Paquet #%s en pause : couleur %s libérée.\n' "$2" "$3" ;;
    paused-already) printf 'Paquet #%s déjà en pause (aucune couleur prise).\n' "$2" ;;
    done-paused) printf 'Paquet #%s terminé (il n'"'"'avait plus de couleur).\n' "$2" ;;
    reason-package) printf 'paquet #%s, prochaine sous-issue ; après la dernière : scripts/chantier.sh --done %s\n' "$2" "$2" ;;
    esac
}
# lang: fr end

# --status [--json]: the six colours and who holds them (#441). Bash lists the
# facts (one line each), Python assembles the answer, so the JSON is never built by hand.
status_facts() {
    local wt c m p f
    while read -r wt; do
        [ -n "$wt" ] || continue
        c="$(sf_colour_of "$wt")"
        m=""
        [ -z "$c" ] || m="$(stat -c %Y "$(sf_colour_file "$wt")")"
        printf 'W\t%s\t%s\t%s\t%s\n' "$(basename "$wt")" "$(sf_issue_of "$wt")" "$c" "$m"
    done < <(sf_worktrees)
    while read -r p; do
        [ -n "$p" ] || continue
        f="$(sf_pkg_dir)/$p"
        printf 'P\t%s\t%s\t%s\n' "$p" "$(sf_pkg_colour "$p")" "$(stat -c %Y "$f")"
        tail -n +2 "$f" | while IFS=$'\t' read -r m c; do printf 'S\t%s\t%s\t%s\n' "$p" "$m" "$c"; done
    done < <(sf_pkgs)
}

status_report() {
    status_facts | python3 -I -c '
import datetime, json, os, sys
mode, colours = sys.argv[1], sys.argv[2:]
today = datetime.date.fromisoformat(os.environ.get("SF_TODAY") or datetime.date.today().isoformat())
wts, pkgs = [], {}
for line in sys.stdin:
    f = line.rstrip("\n").split("\t")
    if f[0] == "W":
        wts.append({"name": f[1], "issue": int(f[2]) if f[2] else None, "colour": f[3], "mtime": int(f[4]) if f[4] else None})
    elif f[0] == "P":
        pkgs[f[1]] = {"parent": int(f[1]), "colour": f[2], "mtime": int(f[3]), "subs": []}
    elif f[0] == "S":
        pkgs[f[1]]["subs"].append({"n": int(f[2]), "title": f[3]})
def date(ts):
    return datetime.date.fromtimestamp(ts)
def since(ts):
    d = date(ts)
    return d.isoformat(), (today - d).days
out = []
for c in colours:
    hw = [w for w in wts if w["colour"] == c]
    hp = [p for p in pkgs.values() if p["colour"] == c]
    if not hw and not hp:
        out.append({"colour": c, "free": True})
        continue
    mt = min([w["mtime"] for w in hw] + [p["mtime"] for p in hp])
    iso, days = since(mt)
    issues = sorted({p["parent"] for p in hp} | {w["issue"] for w in hw if w["issue"] is not None})
    pk = hp[0] if hp else None
    out.append({"colour": c, "free": False, "issue": pk["parent"] if pk else (hw[0]["issue"]),
                "issues": issues,
                "package": {"parent": pk["parent"], "subs": pk["subs"]} if pk else None,
                "worktrees": [w["name"] for w in hw], "since": iso, "days": days})
colourless = [w["name"] for w in wts if not w["colour"]]
paused = []
for p in pkgs.values():
    if not p["colour"]:
        iso, days = since(p["mtime"])
        paused.append({"parent": p["parent"], "subs": p["subs"], "since": iso, "days": days})
if mode == "json":
    json.dump({"colours": out, "colourless": colourless, "paused": paused}, sys.stdout, ensure_ascii=False, indent=1)
    print()
    sys.exit(0)
for e in out:
    if e["free"]:
        print(e["colour"], "free")
        continue
    who = "package #%d (%s)" % (e["package"]["parent"], ", ".join("#%d" % s["n"] for s in e["package"]["subs"])) if e["package"] else "#%s" % e["issue"]
    flag = "  CONFLICT: " + ", ".join("#%d" % i for i in e["issues"]) if len(e["issues"]) > 1 else ""
    print(e["colour"], who, " ".join(e["worktrees"]), "since", e["since"], "(%d d)" % e["days"] + flag)
if colourless:
    print("no colour:", " ".join(colourless))
for p in paused:
    print("paused: package #%d since %s (%d d)" % (p["parent"], p["since"], p["days"]))
' "$([ "$json" = 1 ] && echo json || echo human)" "${SF_COLOURS[@]}"
}

if [ "$status" = 1 ]; then
    status_report
    exit 0
fi

# --release <name>: one merged, clean worktree and its merged local branch (#441).
# Every condition is checked before anything is removed. Runs with the colour lock.
release_wt() {
    local name="$1" wt="$root/feature$1" known top branch ref issue colour dirty ignored sibs p names=""
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
    colour="$(sf_colour_of "$wt")"
    if [ "$dry" = 1 ]; then
        echo "would run: git worktree remove $wt"
        [ -z "$branch" ] || echo "would run: git branch -d $branch"
        return 0
    fi
    git -C "$main" worktree remove "$wt" > /dev/null || return 1
    if [ -n "$branch" ]; then git -C "$main" branch -d "$branch" > /dev/null || return 1; fi
    if [ -z "$colour" ]; then colour_line nocolour; return 0; fi
    sibs="$(sf_issue_worktrees "$issue" | while read -r p; do basename "$p"; done | paste -sd, - | sed 's/,/, /g')"
    if [ -n "$sibs" ]; then colour_line kept "$colour" "$sibs"; return 0; fi
    while read -r p; do
        [ -n "$p" ] || continue
        case "$(sf_pkg_members "$p")" in
        *" $issue "*) [ "$(sf_pkg_colour "$p")" != "$colour" ] || { colour_line kept "$colour" "$(colour_line reason-package "$p")"; return 0; } ;;
        esac
    done < <(sf_pkgs)
    colour_line freed "$colour"
}

if [ -n "$release_name" ]; then
    sf_with_lock "$root/.colour.lock" release_wt "$release_name" || exit 1
    exit 0
fi

# --pause <parent>: free the colour of a package between two sub-issues, keep its list.
if [ -n "$pause_parent" ]; then
    f="$(sf_pkg_dir)/$pause_parent"
    [ -f "$f" ] || { echo "no package #$pause_parent" >&2; exit 1; }
    left="$(sf_pkg_worktrees "$pause_parent" | paste -sd' ' -)"
    if [ -n "$left" ]; then
        echo "package #$pause_parent still has worktrees: $left (merge and remove them first)" >&2
        exit 1
    fi
    colour="$(sf_pkg_colour "$pause_parent")"
    if [ -z "$colour" ]; then colour_line paused-already "$pause_parent"; exit 0; fi
    if [ "$dry" = 1 ]; then
        echo "would pause package #$pause_parent: free $colour, keep its sub-issue list"
        exit 0
    fi
    tail -n +2 "$f" > "$root/.pkglist.$$"
    sf_with_lock "$root/.colour.lock" bash -c '
        . "$1/workflow-lib.sh"; sf_pkg_write "$(sf_pkg_dir)/$2" "" "$3"' _ "$(dirname "$0")" "$pause_parent" "$root/.pkglist.$$"
    rm -f "$root/.pkglist.$$"
    colour_line paused "$pause_parent" "$colour"
    exit 0
fi

if [ -n "$done_parent" ]; then
    f="$(sf_pkg_dir)/$done_parent"
    [ -f "$f" ] || { echo "no colour reserved for package #$done_parent" >&2; exit 1; }
    left="$(sf_pkg_worktrees "$done_parent" | paste -sd' ' -)"
    if [ -n "$left" ]; then
        echo "package #$done_parent still has worktrees: $left (merge and remove them first)" >&2
        exit 1
    fi
    colour="$(sf_pkg_colour "$done_parent")"
    rm -f "$f"
    if [ -n "$colour" ]; then colour_line freed "$colour"; else colour_line done-paused "$done_parent"; fi
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
        if [ "$dry" = 0 ]; then
            # One colour per issue: this worktree keeps its own, else takes the one a
            # sibling (feature<n>b...) holds, else the package's, else the first free;
            # every colourless sibling gets it too.
            mkdir -p "$root"
            colour="$(sf_with_lock "$root/.colour.lock" bash -c '
                . "$1/workflow-lib.sh"
                c="$(sf_colour_for_issue "$2" "$3" "$4")"
                [ -n "$c" ] || exit 1
                [ -n "$(sf_colour_of "$4")" ] || printf "%s\n" "$c" > "$(sf_colour_file "$4")"
                sf_colour_spread "$2" "$c"
                printf "%s\n" "$c"' _ "$(dirname "$0")" "$n" "$pkgcolour" "$wt")" || all_taken
        elif [ -z "$colour" ]; then
            colour="${pkgcolour:-$(sf_issue_colour "$n")}"
        fi
        return
    fi
    colour="${pkgcolour:-$(sf_issue_colour "$n")}"
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
            colour="$(sf_colour_for_issue "$7" "$6")"
            [ -n "$colour" ] || { echo "6 work packages in progress: finish or remove one first" >&2; exit 1; }
            git -C "$2" worktree add "$3" -b "$4" --no-track "$5"
            printf "%s\n" "$colour" > "$(sf_colour_file "$3")"
            sf_colour_spread "$7" "$colour"
        ' _ "$(dirname "$0")" "$main" "$wt" "$branch" "$base" "$pkgcolour" "$n"
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
 2. Colour check: run `scripts/chantier.sh --status --json`, compare it with the
    `chantiers` collection of the To-test page (link above), fix the whole page and
    write `controle/dernier` (stokesforge-verification/references/to-test-page.md,
    "Colour check"); register this work package there.
 3. One start decision form for all open decisions, cards tagged with the issue's
    square (stokesforge-bug-report/references/decision-form.md). A package: one
    form for all of it, first card = the sub-issue list and order above.
 4. Agent `perimetre`, then Sonnet implements in the worktree (CLAUDE.md, "Delegation").
 5. Visible change: `scripts/try.sh <n>` opens the branch on the user's screen;
    their OK is the go-ahead (stokesforge-verification, step 6).
 6. Merge under `scripts/main-lock.sh` from the main checkout (same step 6), read the CI.
 7. Add the To-test entry, delete the `chantiers/<n>` document (step 8). A package:
    one entry per merged sub-issue; the `chantiers/<parent>` document stays until the last.
 8. Clean up: `scripts/chantier.sh --release <n>` (worktree and branch; copy its one
    line in the report). A package stopping with no worktree: `--pause <parent>`;
    after its last sub-issue: `--done <parent>`.
EOF

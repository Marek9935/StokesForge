#!/usr/bin/env bash
# The permanent integration line of the work that must not reach `main` yet (#480).
#
#   scripts/line.sh create <line>     # e.g. PostV1.1
#   scripts/line.sh sync <line>
#   scripts/line.sh land <line>
#
# Run from the MAIN checkout (which is never moved: it may be dirty or on another branch).
# The line lives in its own permanent worktree ../_stokesforge/line-<line>; both verbs take the
# one lock of main-lock.sh for the few seconds they need (git only, no cargo under it).
#
# create: fetches origin, makes the branch <line> from an up-to-date origin/main, pushes it
#   (`origin <line>`, no upstream tracking to main) and adds the worktree. Refused if the name is
#   `main` or not a branch name, or if the branch already exists locally or on origin.
# sync: `main` reaches the line by a regular merge, never the other way before the V1.1 tag. The
#   worktree must be clean and on the line; the line is fast-forwarded to origin/<line> (diverged:
#   refused, never forced), origin/main is merged into it (`Merge main into <line>`), and the line is pushed.
#   A conflict aborts the merge: the line is left as it was, the lock is released, exit 1; resolve it by
#   hand in the line worktree (one orchestrator, under the lock: scripts/main-lock.sh take).
# land: the day the line goes into main. Refused unless (1) the FINAL tag LAND_TAG exists and is on
#   origin/main (an -rc tag does not count) and (2) the user's OK on the whole line is recorded: at
#   least one trial entry of the issue LINE_TRIAL_ISSUE (`suivi.py test`) whose try command is
#   `scripts/try.sh <line>`, all of them OK (`suivi.py line-trial`). If main is not in the line yet it
#   is merged into the line and pushed, main stays as it is, exit 3: recheck, rerun. Otherwise the line
#   is merged into main (`--no-ff`, "Merge <line>: ...", in a temporary detached worktree: the main
#   checkout is never touched, but its branch main is fast-forwarded when it is on main and clean) and
#   main is pushed. Then, rerunnable at any step: every issue with a `Merge #<n>:` commit in M^1..M^2 (read
#   from the merge commit M, never from `main..line`, which is empty once landed) is commented with M and
#   closed on GitLab (the state is read first and read back: an issue closed already is skipped); the
#   `ligne-<line>` label leaves every issue; `suivi.py line-clear` drops `line` from the records; the line
#   worktree and the branch, local and on origin, are deleted. Needs `glab`. Test seams:
#   SF_DELIVER_DELAYS (push retries), SF_LAND_TAG (replaces LAND_TAG).
# Exit: 0 done (or nothing to do), 1 refused or failed, 2 usage, 3 (land) main was merged into the line.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/workflow-lib.sh"

LAND_TAG="${SF_LAND_TAG:-v1.1.0}"   # the final tag the landing waits for
LINE_TRIAL_ISSUE=480                # the issue whose trial entries hold the OK on the whole line
GITLAB_PROJECT=86104040

die() { echo "line: $*" >&2; exit 1; }
usage() { echo "usage: line.sh create|sync|land <line>" >&2; exit 2; }
[ "$#" -eq 2 ] || usage
verb="$1"
line="$2"
case "$verb" in create | sync | land) ;; *) usage ;; esac
[[ "$line" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "not a line name: $line"
[ "$line" != main ] || die "main is not an integration line"

main="$(sf_main_dir)"
top="$(realpath "$(git rev-parse --show-toplevel)")"
[ "$top" = "$(realpath "$main")" ] || die "run it from the main checkout ($main), not from $top"
lwt="$(sf_line_dir "$line")"

"$here/main-lock.sh" take "line $verb $line" > /dev/null || exit 1
trap '"$here/main-lock.sh" release > /dev/null 2>&1 || true' EXIT

git fetch -q origin || die "git fetch origin failed"

if [ "$verb" = create ]; then
    ! git rev-parse --verify -q "refs/heads/$line" > /dev/null || die "the branch $line already exists here"
    ! git rev-parse --verify -q "refs/remotes/origin/$line" > /dev/null || die "origin/$line already exists"
    [ ! -e "$lwt" ] || die "$lwt already exists"
    git rev-parse --verify -q refs/remotes/origin/main > /dev/null || die "no origin/main"
    git branch --no-track "$line" origin/main
    if ! git push -q origin "$line"; then
        git branch -q -D "$line"
        die "push of $line failed; nothing created"
    fi
    git worktree add -q "$lwt" "$line" || die "cannot add the worktree $lwt (the branch $line is pushed)"
    echo "line: $line created from origin/main, worktree $lwt"
    exit 0
fi

# sync, land
[ "$(git -C "$lwt" symbolic-ref --short HEAD 2> /dev/null || true)" = "$line" ] \
    || die "$lwt is not the worktree of $line (scripts/line.sh create $line)"
[ -z "$(git -C "$lwt" status --porcelain)" ] || die "the line worktree $lwt has modified or untracked files"
git rev-parse --verify -q "refs/remotes/origin/$line" > /dev/null || die "no origin/$line"
if [ "$(git -C "$lwt" rev-parse HEAD)" != "$(git rev-parse "origin/$line")" ]; then
    if git merge-base --is-ancestor "$line" "origin/$line"; then
        git -C "$lwt" merge -q --ff-only "origin/$line" || die "cannot fast-forward $line to origin/$line"
    elif git merge-base --is-ancestor "origin/$line" "$line"; then
        :   # ahead: an earlier run merged and did not push
    else
        die "$line and origin/$line have diverged; nothing done (never forced)"
    fi
fi
merge_main_into_line() {
    if ! git -C "$lwt" merge --no-ff -q -m "Merge main into $line" origin/main > /dev/null 2>&1; then
        git -C "$lwt" merge --abort > /dev/null 2>&1 || true
        die "merging origin/main into $line conflicts; $line is unchanged. Resolve it by hand in $lwt (under scripts/main-lock.sh take)"
    fi
}
push_retry() {   # push_retry <git push args...>: 4 retries after 2, 4, 8, 16 s (SF_DELIVER_DELAYS)
    local d
    git push -q --no-follow-tags "$@" && return 0
    for d in ${SF_DELIVER_DELAYS:-2 4 8 16}; do
        echo "line: push failed, retrying in ${d}s" >&2
        sleep "$d"
        git push -q --no-follow-tags "$@" && return 0
    done
    return 1
}

if [ "$verb" = sync ]; then
    if ! git merge-base --is-ancestor origin/main "$line"; then merge_main_into_line; fi
    git push -q --no-follow-tags origin "$line" || die "push of $line failed; the merge is local only, rerun to push it"
    echo "line: $line holds origin/main and is pushed"
    exit 0
fi

# land
command -v glab > /dev/null || die "glab is required to close the issues (glab auth login)"
suivi() { python3 -I "$here/suivi.py" "$@"; }
git fetch -q --tags origin || die "git fetch --tags origin failed"
tagc="$(git rev-parse -q --verify "refs/tags/$LAND_TAG^{commit}" || true)"
[ -n "$tagc" ] || die "the final tag $LAND_TAG does not exist (an -rc tag does not count): the line does not land before it"
git merge-base --is-ancestor "$tagc" origin/main || die "the tag $LAND_TAG is not on main: the line does not land before it is"
if ! out="$(suivi line-trial "$LINE_TRIAL_ISSUE" "$line")"; then die "$out"; fi

# An earlier run may already have merged: a first-parent `Merge <line>:` commit of main that holds the line.
m=""
while read -r h subj; do
    case "$subj" in "Merge $line:"*) ;; *) continue ;; esac
    if git merge-base --is-ancestor "refs/heads/$line" "$h"; then m="$h"; break; fi
done < <(git log origin/main --first-parent --format='%H %s')

if [ -z "$m" ]; then
    if ! git merge-base --is-ancestor origin/main "$line"; then
        merge_main_into_line
        git push -q --no-follow-tags origin "$line" || die "push of $line failed; the merge is local only, rerun to push it"
        echo "line: main had moved; it is now merged into $line and pushed. Recheck the line (stokesforge-verification), then rerun scripts/line.sh land $line" >&2
        exit 3
    fi
    tmpw="$(dirname "$lwt")/land-tmp"
    [ ! -e "$tmpw" ] || die "$tmpw exists (an earlier run stopped?): remove it with git worktree remove --force"
    git worktree add -q --detach "$tmpw" origin/main || die "cannot add the temporary worktree $tmpw"
    land_cleanup() { git worktree remove --force "$tmpw" > /dev/null 2>&1 || true; "$here/main-lock.sh" release > /dev/null 2>&1 || true; }
    trap land_cleanup EXIT
    if ! git -C "$tmpw" merge --no-ff -q -m "Merge $line: land the integration line" "refs/heads/$line" > /dev/null 2>&1; then
        git -C "$tmpw" merge --abort > /dev/null 2>&1 || true
        die "merging $line into main failed; main is unchanged"
    fi
    m="$(git -C "$tmpw" rev-parse HEAD)"
    ( cd "$tmpw" && push_retry origin HEAD:refs/heads/main ) || die "push of main failed after retries; nothing kept (the merge was in a temporary worktree): rerun"
    # keep the local branch main in step when that costs nothing
    if [ "$(git symbolic-ref --short HEAD 2> /dev/null || true)" = main ] && [ -z "$(git status --porcelain)" ]; then
        git merge -q --ff-only origin/main > /dev/null 2>&1 || true
    fi
    git fetch -q origin main || true
fi

# Close every issue the line delivered, read from M (rerunnable: closed issues are skipped).
api() { glab api "$@"; }
state_of() {
    api "projects/$GITLAB_PROJECT/issues/$1" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["state"])'
}
issues="$(git log --format=%s "$m^1..$m^2" | sed -n 's/^Merge #\([0-9][0-9]*\):.*/\1/p' | sort -un)"
for n in $issues; do
    st="$(state_of "$n")" || die "cannot read the state of #$n on GitLab; rerun"
    if [ "$st" = closed ]; then echo "line: #$n is closed already"; continue; fi
    api --method POST "projects/$GITLAB_PROJECT/issues/$n/notes" -f "body=Landed on main by $m (the line $line merged into main after $LAND_TAG)." > /dev/null \
        || die "cannot comment on #$n; rerun"
    api --method PUT "projects/$GITLAB_PROJECT/issues/$n" -f state_event=close > /dev/null || die "cannot close #$n; rerun"
    [ "$(state_of "$n")" = closed ] || die "#$n is not closed after the request; rerun"
    echo "line: #$n closed"
done

# The label leaves every issue, the records forget the line, the line is deleted.
label="ligne-$line"
labelled="$(api "projects/$GITLAB_PROJECT/issues?labels=$label&state=all&per_page=100" \
    | python3 -I -c 'import json,sys; print(" ".join(str(i["iid"]) for i in json.load(sys.stdin)))')" || die "cannot list the issues labelled $label; rerun"
for n in $labelled; do
    api --method PUT "projects/$GITLAB_PROJECT/issues/$n" -f "remove_labels=$label" > /dev/null || die "cannot remove $label from #$n; rerun"
done
suivi line-clear "$line" > /dev/null
git worktree remove "$lwt" || die "cannot remove the line worktree $lwt"
git merge-base --is-ancestor "refs/heads/$line" origin/main && git branch -q -D "$line"
git push -q origin --delete "$line" 2> /dev/null || echo "line: could not delete origin/$line: delete it by hand" >&2
echo "line: $line landed in main as $m; issues closed, label and line removed"

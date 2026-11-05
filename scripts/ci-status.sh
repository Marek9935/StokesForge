#!/usr/bin/env bash
# Results of the Linux CI (GitHub) read from GitLab, for one commit.
#
#   scripts/ci-status.sh [commit]     # HEAD by default
#   scripts/ci-status.sh --head       # the head of `main` on GitLab, jobs only (#452)
#
# Prints one status per job `github-actions/<job>` (with the failing step and
# the first cause on failure), then the commit comments posted by the reporter
# (≈50 log lines per failure). See docs/tests.md, "Using CI day to day".
# `--head` asks GitLab (not the local clone, not `git ls-remote`: no ssh agent is
# needed) for the newest commit of `main`, prints `HEAD <sha>` first, and skips the
# commit comments. The tracking page's CI watcher (scripts/suivi.py) reads it.
# Requires `glab` signed in to gitlab.com.
set -euo pipefail

PROJECT_ID=86104040
PROJECT_PATH=askrzypczak/stokesforge

command -v glab > /dev/null || { echo "glab is required (glab auth login)" >&2; exit 1; }
head_mode=0
if [ "${1:-}" = "--head" ]; then
    head_mode=1
    sha="$(glab api "projects/$PROJECT_ID/repository/commits?ref_name=main&per_page=1" \
        | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d[0]["id"] if d else "")')"
    [ -n "$sha" ] || { echo "GitLab returned no commit for main" >&2; exit 1; }
    echo "HEAD $sha"
else
    sha="$(git rev-parse "${1:-HEAD}")"
fi

# External statuses are not listed by the REST commits API: we go through the
# GitLab pipeline of the commit (full SHA required) and its `external` stage.
iid="$(glab api "projects/$PROJECT_ID/pipelines?sha=$sha" \
    | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d[0]["iid"] if d else "")')"
if [ -z "$iid" ]; then
    echo "No GitLab pipeline for $sha: the GitHub CI has not finished yet, or the mirror has not received the commit." >&2
    exit 2
fi

echo "== $sha (GitLab pipeline #$iid)"
glab api graphql -f query="{project(fullPath:\"$PROJECT_PATH\"){pipeline(iid:\"$iid\"){stages{nodes{name jobs{nodes{name status detailedStatus{label}}}}}}}}" \
    | python3 -c '
import json, sys
for stage in json.load(sys.stdin)["data"]["project"]["pipeline"]["stages"]["nodes"]:
    for job in stage["jobs"]["nodes"]:
        label = job["detailedStatus"]["label"]
        print("%-34s %-8s | %s" % (job["name"], job["status"], label))
'

[ "$head_mode" = 1 ] && exit 0

echo
echo "== commit comments"
glab api "projects/$PROJECT_ID/repository/commits/$sha/comments" | python3 -c '
import json, sys
notes = json.load(sys.stdin)
if not notes:
    print("(none: no blocking failure)")
for note in notes:
    print(note["note"]); print("-----")
'

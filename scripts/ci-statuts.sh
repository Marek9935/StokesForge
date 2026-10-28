#!/usr/bin/env bash
# Results of the Linux CI (GitHub) read from GitLab, for one commit.
#
#   scripts/ci-statuts.sh [commit]     # HEAD by default
#
# Prints one status per job `github-actions/<job>` (with the failing step and
# the first cause on failure), then the commit comments posted by the reporter
# (≈50 log lines per failure). See docs/tests.md, "Using CI day to day".
# Requires `glab` signed in to gitlab.com.
set -euo pipefail

PROJECT_ID=86104040
PROJECT_PATH=askrzypczak/stokesforge

command -v glab > /dev/null || { echo "glab is required (glab auth login)" >&2; exit 1; }
sha="$(git rev-parse "${1:-HEAD}")"

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

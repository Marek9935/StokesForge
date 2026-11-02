#!/usr/bin/env bash
# Results of the weekly toolchain workflow (#412), read from the public
# GitHub API of the mirror: no `gh`, no token (60 requests per hour).
#   scripts/toolchain-weekly-status.sh [number of runs, default 3]
set -euo pipefail
exec python3 -I - "${1:-3}" <<'EOF'
import json, sys, urllib.error, urllib.request

API = "https://api.github.com/repos/Marek9935/StokesForge/actions"

def get(path):
    with urllib.request.urlopen(API + path, timeout=30) as response:
        return json.load(response)

try:
    runs = get(f"/workflows/toolchain-weekly.yml/runs?per_page={int(sys.argv[1])}")["workflow_runs"]
except urllib.error.HTTPError as err:
    if err.code != 404:
        raise
    sys.exit("toolchain-weekly.yml is not on the mirror's default branch yet")
if not runs:
    print("no run yet (the workflow runs from main only)")
for run in runs:
    print(run["created_at"][:10], run["event"], run["status"], run["conclusion"], run["html_url"])
    for job in get(f"/runs/{run['id']}/jobs")["jobs"]:
        print("   ", job["name"], job["conclusion"])
EOF

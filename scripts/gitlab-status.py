#!/usr/bin/env python3
"""Publish the GitHub mirror results in the GitLab pipeline of the same commit.

Called by a finished workflow_run, never by the code a PR tests. Tokens stay
in the environment, and HTTP errors put neither headers nor bodies in the
logs. No external library is needed.
"""

import io
import json
import os
from pathlib import Path
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile


REPOSITORY = "Marek9935/StokesForge"
GITHUB = f"https://api.github.com/repos/{REPOSITORY}"
GITLAB = "https://gitlab.com/api/v4/projects/86104040"
JOBS = ("fmt", "licenses", "clippy", "test", "robustesse", "test-python")
# GitLab caps the description of an external status.
DESCRIPTION_MAX = 255
LOG_MAX = 20_000_000
# License report: one file per push to main, uploaded as a GitLab package.
PACKAGE_MAX = 20_000_000
LICENSES_ARTIFACT = "third-party-licenses"
LICENSES_FILE = "THIRD-PARTY-LICENSES.txt"
# Commit comment: enough to understand the failure without opening GitHub.
CONTEXT_BEFORE = 10
CONTEXT_LINES = 50
COMMENT_MAX = 15_000


def request_json(url, headers, data=None):
    """Three bounded attempts for pipeline conflicts and transient errors."""
    body = None if data is None else json.dumps(data).encode()
    for attempt in range(3):
        request = urllib.request.Request(
            url, data=body, headers={**headers, "Content-Type": "application/json"}
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            retry = error.code in (409, 429, 500, 502, 503, 504)
            if not retry or attempt == 2:
                # Do not print the Request object or the response: they may
                # contain authentication data.
                raise RuntimeError(f"API: HTTP error {error.code}") from None
        except (urllib.error.URLError, TimeoutError):
            if attempt == 2:
                raise RuntimeError("API unreachable after three attempts") from None
        time.sleep(2 ** attempt)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def fetch_github_bytes(url, github_token, limit):
    """Bytes of a GitHub resource served by redirect, or None. Never fatal.

    The API answers with a redirect to signed storage. We follow it by hand,
    without an authorization header, so the GitHub token is not sent to another
    domain.
    """
    opener = urllib.request.build_opener(NoRedirect)
    request = urllib.request.Request(
        url,
        headers={"Authorization": f"Bearer {github_token}", "Accept": "application/vnd.github+json"},
    )
    try:
        try:
            response = opener.open(request, timeout=20)
        except urllib.error.HTTPError as error:
            location = error.headers.get("Location") if error.code in (301, 302, 303, 307) else None
            if not location or not location.startswith("https://"):
                return None
            response = urllib.request.urlopen(location, timeout=60)
        with response:
            return response.read(limit)
    except (urllib.error.URLError, TimeoutError, OSError):
        return None


def fetch_job_log(job_id, github_token):
    """Log of a GitHub job, or None: it only enriches the report."""
    data = fetch_github_bytes(f"{GITHUB}/actions/jobs/{job_id}/logs", github_token, LOG_MAX)
    return None if data is None else data.decode("utf-8", "replace")


def fetch_artifact_file(run_id, artifact, filename, github_token, gh_headers):
    """Content of a file in a GitHub artifact (zip), or None."""
    listing = request_json(f"{GITHUB}/actions/runs/{run_id}/artifacts?per_page=100", gh_headers)
    found = next((a for a in listing.get("artifacts", []) if a["name"] == artifact and not a["expired"]), None)
    if found is None:
        return None
    archive = fetch_github_bytes(found["archive_download_url"], github_token, PACKAGE_MAX)
    if archive is None:
        return None
    try:
        with zipfile.ZipFile(io.BytesIO(archive)) as bundle:
            return bundle.read(filename)
    except (zipfile.BadZipFile, KeyError):
        return None


def upload_package(package, version, filename, data, gitlab_token):
    """Upload a file to the GitLab generic package registry."""
    url = f"{GITLAB}/packages/generic/{package}/{version}/{filename}"
    request = urllib.request.Request(
        url, data=data, method="PUT",
        headers={"PRIVATE-TOKEN": gitlab_token, "Content-Type": "application/octet-stream"},
    )
    try:
        with urllib.request.urlopen(request, timeout=60):
            return url
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"API: HTTP error {error.code}") from None
    except (urllib.error.URLError, TimeoutError):
        raise RuntimeError("API unreachable") from None


ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
TIMESTAMP = re.compile(r"^\d{4}-\d\d-\d\dT[\d:.]+Z ?")
# First line that says why: cargo/rustc error, panic, failing test, script
# error. `warning:` lines and compilation noise are ignored.
CAUSE = re.compile(
    r"^(error(\[E\d+\])?:|\s*thread '.*' .*panicked|\[licences\] ERROR|Traceback|"
    r"E: |fatal:|.*: command not found|.*No such file)"
)
# Fallback: a failing test with no visible panic in the log.
TEST_FAILED = re.compile(r"^\S.* \.\.\. FAILED")
REGISTRY = re.compile(r"/\S*?/registry/src/[^/]+/")
LOCATION = re.compile(r"^\s*--> (\S+)")


def clean_lines(log):
    return [TIMESTAMP.sub("", ANSI.sub("", raw)).rstrip() for raw in log.splitlines()]


def find_cause(lines):
    """Index of the first failure cause, or None."""
    for pattern in (CAUSE, TEST_FAILED):
        for i, line in enumerate(lines):
            if pattern.match(line):
                return i
    return None


def failure_excerpt(log):
    """First failure cause of a log, on one line, or None."""
    lines = clean_lines(log)
    i = find_cause(lines)
    if i is None:
        return None
    line = lines[i]
    excerpt = REGISTRY.sub("", line.strip())
    for following in lines[i + 1 : i + 4]:
        location = LOCATION.match(following)
        if location:
            excerpt += f" ({location.group(1)})"
            break
    # A panic: the message is on the next line.
    if "panicked" in line and i + 1 < len(lines) and lines[i + 1].strip():
        excerpt += ": " + lines[i + 1].strip()
    return excerpt


def failure_context(log):
    """About fifty lines on the failure.

    GitHub ends the failing step with `##[error]`: what follows it (job
    cleanup) is noise, and what precedes it is the output of the step. The
    window starts ten lines before the first cause when there is one, and
    otherwise goes back fifty lines from the marker (or from the end of the log).
    """
    lines = clean_lines(log)
    end = next((n for n, line in enumerate(lines) if line.startswith("##[error]")), None)
    stop = len(lines) if end is None else end + 1
    cause = find_cause(lines[:stop])
    if cause is not None:
        start = max(0, cause - CONTEXT_BEFORE)
    else:
        start = max(0, stop - CONTEXT_LINES)
    # A code fence in the log would close the comment too early.
    text = "\n".join(lines[start : min(start + CONTEXT_LINES, stop)]).replace("```", "'" * 3)
    return text if len(text) <= COMMENT_MAX else text[:COMMENT_MAX] + "\n[…truncated]"


def failure_comment(name, job_url, attempt, detail, log):
    """GitLab commit comment for a failed job."""
    body = f"**github-actions/{name}** — attempt {attempt}, failed"
    if detail:
        body += f" ({detail})"
    body += f"\n\n[Full log on GitHub]({job_url})\n"
    if log:
        body += f"\n```text\n{failure_context(log)}\n```\n"
    return body


def failure_detail(job, log):
    """Failed step + first cause, for the status description."""
    step = next(
        (s.get("name") for s in job.get("steps", []) if s.get("conclusion") == "failure"), None
    )
    parts = []
    if step:
        parts.append(f"step \"{step}\"")
    excerpt = failure_excerpt(log) if log else None
    if excerpt:
        parts.append(excerpt)
    return ": ".join(parts) or None


def clip(text):
    return text if len(text) <= DESCRIPTION_MAX else text[: DESCRIPTION_MAX - 1] + "…"


def trusted_run(event):
    run = event.get("workflow_run", {})
    if (
        event.get("action") != "completed"
        or event.get("repository", {}).get("full_name") != REPOSITORY
        or run.get("head_repository", {}).get("full_name") != REPOSITORY
        or run.get("event") != "push"
        or run.get("path") != ".github/workflows/ci.yml"
    ):
        return None
    if not re.fullmatch(r"[0-9a-f]{40}", run.get("head_sha", "")):
        raise RuntimeError("Invalid workflow SHA")
    if not isinstance(run.get("id"), int) or not isinstance(run.get("run_attempt"), int):
        raise RuntimeError("Invalid workflow id or attempt")
    return run


def job_status(job, canceled=False):
    if job is None:
        return ("canceled" if canceled else "failed", "Job missing from the attempt")
    conclusion = job.get("conclusion")
    if conclusion == "cancelled":
        return "canceled", "Job canceled on GitHub"
    # A reproduction can fail without failing CI. The statuses API has no
    # allow_failure parameter: the result stays explicitly non-validating
    # (skipped), with its reason, rather than a false success.
    reproduction_failed = job["name"] == "robustesse" and (
        conclusion == "failure"
        or any(step.get("conclusion") == "failure" for step in job.get("steps", []))
    )
    if reproduction_failed:
        return "skipped", "Non-blocking failure of the reproductions; see the logs"
    states = {
        "success": "success",
        "failure": "failed",
        "timed_out": "failed",
        "cancelled": "canceled",
        "skipped": "skipped",
        "neutral": "skipped",
    }
    return states.get(conclusion, "failed"), f"GitHub result: {conclusion or 'unknown'}"


def report(event, github_token, gitlab_token):
    run = trusted_run(event)
    if run is None:
        print("Event ignored: only pushes of the mirror CI workflow are published")
        return
    if not github_token or not gitlab_token:
        raise RuntimeError("Configure the GitHub secret GITLAB_STATUS_TOKEN (GitLab API token)")
    gh_headers = {"Authorization": f"Bearer {github_token}", "Accept": "application/vnd.github+json"}
    gl_headers = {"PRIVATE-TOKEN": gitlab_token}
    current = request_json(f"{GITHUB}/actions/runs/{run['id']}", gh_headers)
    if current["run_attempt"] != run["run_attempt"]:
        print("Old attempt ignored: a more recent rerun exists")
        return

    jobs = {}
    page = 1
    while True:
        response = request_json(
            f"{GITHUB}/actions/runs/{run['id']}/jobs?filter=latest&per_page=100&page={page}",
            gh_headers,
        )
        for job in response["jobs"]:
            jobs[job["name"]] = job
        if page * 100 >= response["total_count"]:
            break
        page += 1

    # head_branch holds the name of the tested ref, without refs/heads or refs/tags.
    ref = run["head_branch"]
    query = urllib.parse.urlencode({"sha": run["head_sha"], "ref": ref, "per_page": 1})
    pipeline_url = f"{GITLAB}/pipelines?{query}&order_by=id&sort=desc"
    pipelines = request_json(pipeline_url, gl_headers)
    pipeline_id = pipelines[0]["id"] if pipelines else None
    for name in JOBS:
        job = jobs.get(name)
        state, description = job_status(job, run.get("conclusion") == "cancelled")
        log = None
        if state == "failed" and job:
            log = fetch_job_log(job["id"], github_token)
            detail = failure_detail(job, log)
            if detail:
                description = detail
        payload = {
            "state": state,
            "name": f"github-actions/{name}",
            "ref": ref,
            "target_url": (
                f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}/job/{job['id']}"
                if job else f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}"
            ),
            "description": clip(f"Attempt {run['run_attempt']} — {description}"),
        }
        if pipeline_id is not None:
            payload["pipeline_id"] = pipeline_id
        request_json(f"{GITLAB}/statuses/{run['head_sha']}", gl_headers, payload)
        if state == "failed" and job:
            note = failure_comment(
                name, payload["target_url"], run["run_attempt"], description, log
            )
            try:
                request_json(
                    f"{GITLAB}/repository/commits/{run['head_sha']}/comments",
                    gl_headers,
                    {"note": note},
                )
            except RuntimeError as error:
                # The status is already published: the comment is only a bonus.
                print(f"Comment not published for {name}: {error}", file=sys.stderr)
        if pipeline_id is None:
            # The first status can create an external pipeline. All the following
            # ones must join that pipeline, even if it did not exist before.
            pipelines = request_json(pipeline_url, gl_headers)
            if not pipelines:
                raise RuntimeError("GitLab pipeline created but not found")
            pipeline_id = pipelines[0]["id"]
        print(f"github-actions/{name}: {state}")
    publish_licenses(run, jobs.get("licenses"), github_token, gitlab_token, gh_headers)


def publish_licenses(run, job, github_token, gitlab_token, gh_headers):
    """License report as a GitLab package, readable without going through GitHub."""
    if not job or job.get("conclusion") != "success":
        return
    try:
        data = fetch_artifact_file(run["id"], LICENSES_ARTIFACT, LICENSES_FILE, github_token, gh_headers)
        if data is None:
            print("License report not found on GitHub", file=sys.stderr)
            return
        url = upload_package(LICENSES_ARTIFACT, run["head_sha"][:12], LICENSES_FILE, data, gitlab_token)
        print(f"License report uploaded: {url}")
    except RuntimeError as error:
        # The statuses are already published: the report is only a bonus.
        print(f"License report not uploaded: {error}", file=sys.stderr)


def main():
    try:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        # The filter comes even before reading the secrets.
        if trusted_run(event) is None:
            print("Event ignored")
            return
        report(event, os.environ.get("GITHUB_TOKEN"), os.environ.get("GITLAB_STATUS_TOKEN"))
    except (RuntimeError, KeyError, ValueError, OSError) as error:
        print(f"Reporting to GitLab failed: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()

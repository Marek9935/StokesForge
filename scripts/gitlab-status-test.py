#!/usr/bin/env python3
"""Contracts of the GitHub → GitLab report, with no network and no real token."""

import contextlib
import copy
import importlib.util
import io
import json
import zipfile
from pathlib import Path
import unittest
from unittest.mock import patch
import urllib.error


spec = importlib.util.spec_from_file_location("gitlab_status", Path(__file__).with_name("gitlab-status.py"))
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)


def event():
    return {
        "action": "completed",
        "repository": {"full_name": status.REPOSITORY},
        "workflow_run": {
            "id": 42, "run_attempt": 2, "head_sha": "a" * 40,
            "event": "push", "head_branch": "main", "conclusion": "success",
            "path": ".github/workflows/ci.yml",
            "head_repository": {"full_name": status.REPOSITORY},
        },
    }


class ReportTest(unittest.TestCase):
    def setUp(self):
        self.event = event()
        self.jobs = [
            {"name": name, "id": i + 1, "conclusion": "success", "steps": []}
            for i, name in enumerate(status.JOBS)
        ]
        self.posts = []
        self.pages = []
        self.pipeline = [{"id": 99}]
        self.log = None
        self.comments = []
        self.licenses = b"rapport"
        self.uploads = []

    def api(self, url, headers, data=None):
        if data is not None and url.endswith("/comments"):
            self.assertIn("/repository/commits/" + "a" * 40, url)
            self.comments.append(data["note"])
            return {}
        if data is not None:
            self.assertEqual(headers, {"PRIVATE-TOKEN": "secret-sentinelle"})
            self.assertTrue(url.endswith("/statuses/" + "a" * 40))
            self.posts.append(copy.deepcopy(data))
            self.pipeline = [{"id": 99}]
            return {}
        if "/jobs?" in url:
            self.pages.append(url)
            return {"total_count": len(self.jobs), "jobs": self.jobs}
        if "/pipelines?" in url:
            self.assertIn("sha=" + "a" * 40, url)
            self.assertIn("ref=main", url)
            return self.pipeline
        return {"run_attempt": 2}

    def report(self):
        with patch.object(status, "request_json", side_effect=self.api):
            with patch.object(status, "fetch_job_log", side_effect=lambda *_: self.log):
                with patch.object(status, "fetch_artifact_file", return_value=self.licenses):
                    with patch.object(status, "upload_package", side_effect=self.upload):
                        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                            status.report(self.event, "github-sentinelle", "secret-sentinelle")

    def upload(self, package, version, filename, data, token):
        self.assertEqual(token, "secret-sentinelle")
        self.uploads.append((package, version, filename, data))
        return "https://gitlab.example/pkg"

    def test_success_and_tolerated_rerun_keep_links_sha_and_pipeline(self):
        self.jobs[4]["steps"] = [{"conclusion": "failure"}]
        self.report()
        self.assertEqual(len(self.posts), 6)
        for post in self.posts:
            self.assertEqual(post["pipeline_id"], 99)
            self.assertIn("Attempt 2", post["description"])
            self.assertRegex(post["target_url"], r"/actions/runs/42/job/\d+$")
        self.assertEqual([p["state"] for p in self.posts], ["success"] * 4 + ["skipped", "success"])
        self.assertIn("Non-blocking failure", self.posts[4]["description"])

    def test_failure_timeout_and_cancellation_do_not_turn_green(self):
        for conclusion, expected in [("failure", "failed"), ("timed_out", "failed"), ("cancelled", "canceled")]:
            with self.subTest(conclusion=conclusion):
                self.posts.clear()
                self.jobs[0]["conclusion"] = conclusion
                self.report()
                self.assertEqual(self.posts[0]["state"], expected)
        self.jobs = []
        self.event["workflow_run"]["conclusion"] = "cancelled"
        self.posts.clear()
        self.report()
        self.assertEqual([p["state"] for p in self.posts], ["canceled"] * 6)

    def test_failure_publishes_step_and_first_error_of_the_log(self):
        self.jobs[2]["conclusion"] = "failure"
        self.jobs[2]["steps"] = [
            {"name": "Set up job", "conclusion": "success"},
            {"name": "Run cargo clippy", "conclusion": "failure"},
        ]
        self.log = (
            "2026-10-24T17:00:00.1Z    Checking foo v0.0.0\n"
            "2026-10-24T17:00:01.2Z warning: unused variable\n"
            "2026-10-24T17:00:02.2Z \x1b[31merror\x1b[0m: the borrowed expression implements the required traits\n"
            "2026-10-24T17:00:02.4Z    --> crates/x/src/app.rs:1295:69\n"
            "2026-10-24T17:00:03.0Z error: could not compile `x`\n"
        )
        self.report()
        description = self.posts[2]["description"]
        self.assertEqual(self.posts[2]["state"], "failed")
        self.assertIn('step "Run cargo clippy"', description)
        self.assertIn("the borrowed expression", description)
        self.assertIn("crates/x/src/app.rs:1295:69", description)
        self.assertNotIn("unused variable", description)
        self.assertLessEqual(len(description), status.DESCRIPTION_MAX)

    def test_failure_posts_a_fifty_line_commit_comment(self):
        self.jobs[3]["conclusion"] = "failure"
        self.jobs[3]["steps"] = [{"name": "Run cargo test", "conclusion": "failure"}]
        before = [f"line before {n}" for n in range(30)]
        after = [f"line after {n}" for n in range(100)]
        self.log = "\n".join(before + ["error: the cause"] + after)
        self.report()
        self.assertEqual(len(self.comments), 1)
        note = self.comments[0]
        self.assertIn("github-actions/test", note)
        self.assertIn("/actions/runs/42/job/", note)
        self.assertIn("error: the cause", note)
        self.assertIn("line before 29", note)
        self.assertNotIn("line before 5\n", note)  # beyond the context before
        self.assertNotIn("line after 60", note)  # beyond 50 lines
        code = note.split("```text\n")[1].split("\n```")[0]
        self.assertEqual(len(code.splitlines()), status.CONTEXT_LINES)

    def test_window_anchored_on_the_github_error_marker(self):
        # Real log of `cargo fmt --check`: no `error:` line, the useful output
        # is just before `##[error]`, the job cleanup after.
        log = "\n".join(
            ["noise from a previous step"] * 40
            + ["Diff in crates/x/src/lib.rs:74:", "-old", "+new", "##[error]Process completed with exit code 1.",
               "Post job cleanup.", "[command]/usr/bin/git version"]
        )
        context = status.failure_context(log)
        self.assertIn("+new", context)
        self.assertIn("Diff in crates/x/src/lib.rs:74:", context)
        self.assertNotIn("Post job cleanup", context)
        self.assertLessEqual(len(context.splitlines()), status.CONTEXT_LINES)

    def test_comment_only_for_a_failure_and_without_a_log_it_stays_useful(self):
        self.report()
        self.assertEqual(self.comments, [])
        self.jobs[0]["conclusion"] = "failure"
        self.report()  # journal absent
        self.assertEqual(len(self.comments), 1)
        self.assertNotIn("```", self.comments[0])
        self.assertIn("Full log", self.comments[0])

    def test_rejected_comment_does_not_break_the_report(self):
        self.jobs[0]["conclusion"] = "failure"
        def api(url, headers, data=None):
            if url.endswith("/comments"):
                raise RuntimeError("API : erreur HTTP 403")
            return self.api(url, headers, data)
        with patch.object(status, "request_json", side_effect=api):
            with patch.object(status, "fetch_job_log", return_value="error: x"):
                with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    status.report(self.event, "github", "secret-sentinelle")
        self.assertEqual(len(self.posts), 6)

    def test_licenses_uploaded_as_gitlab_package_only_if_the_job_succeeds(self):
        self.report()
        self.assertEqual(self.uploads, [("third-party-licenses", "a" * 12, "THIRD-PARTY-LICENSES.txt", b"rapport")])
        self.uploads.clear()
        self.jobs[1]["conclusion"] = "failure"  # licenses
        self.report()
        self.assertEqual(self.uploads, [])
        self.jobs[1]["conclusion"] = "success"
        self.licenses = None  # artefact introuvable
        self.report()
        self.assertEqual(self.uploads, [])
        self.assertEqual(len(self.posts), 18)  # statuses always go through

    def test_rejected_license_upload_does_not_break_the_report(self):
        with patch.object(status, "request_json", side_effect=self.api):
            with patch.object(status, "fetch_artifact_file", return_value=b"x"):
                with patch.object(status, "upload_package", side_effect=RuntimeError("API : erreur HTTP 403")):
                    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                        status.report(self.event, "github", "secret-sentinelle")
        self.assertEqual(len(self.posts), 6)

    def test_zip_artifact_extracted_without_sending_the_token_elsewhere(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as bundle:
            bundle.writestr("THIRD-PARTY-LICENSES.txt", "licences")
        listing = {"artifacts": [
            {"name": "autre", "expired": False, "archive_download_url": "https://api.github.com/x"},
            {"name": status.LICENSES_ARTIFACT, "expired": False, "archive_download_url": "https://api.github.com/ok"},
        ]}
        with patch.object(status, "request_json", return_value=listing):
            with patch.object(status, "fetch_github_bytes", return_value=buffer.getvalue()) as fetch:
                data = status.fetch_artifact_file(42, status.LICENSES_ARTIFACT, "THIRD-PARTY-LICENSES.txt", "gh", {})
        self.assertEqual(data, b"licences")
        self.assertEqual(fetch.call_args.args[0], "https://api.github.com/ok")
        listing["artifacts"][1]["expired"] = True
        with patch.object(status, "request_json", return_value=listing):
            self.assertIsNone(status.fetch_artifact_file(42, status.LICENSES_ARTIFACT, "THIRD-PARTY-LICENSES.txt", "gh", {}))

    def test_non_https_or_missing_redirect_rejected(self):
        for location in (None, "http://evil.example/x"):
            headers = {} if location is None else {"Location": location}
            error = urllib.error.HTTPError("https://api.github.com", 302, "redirect", headers, None)
            with patch.object(status.urllib.request.OpenerDirector, "open", side_effect=error):
                with patch.object(status.urllib.request, "urlopen") as direct:
                    self.assertIsNone(status.fetch_github_bytes("https://api.github.com/x", "gh", 10))
                    direct.assert_not_called()

    def test_unavailable_or_huge_log_does_not_break_the_report(self):
        self.jobs[3]["conclusion"] = "failure"
        self.jobs[3]["steps"] = [{"name": "Run cargo test", "conclusion": "failure"}]
        self.report()  # log missing: the step alone is enough
        self.assertIn('step "Run cargo test"', self.posts[3]["description"])
        self.posts.clear()
        self.log = "error: " + "x" * 5000
        self.report()
        self.assertLessEqual(len(self.posts[3]["description"]), status.DESCRIPTION_MAX)
        self.assertTrue(self.posts[3]["description"].endswith("…"))

    def test_success_downloads_no_log(self):
        with patch.object(status, "request_json", side_effect=self.api):
            with patch.object(status, "fetch_job_log") as fetch:
                with contextlib.redirect_stdout(io.StringIO()):
                    status.report(self.event, "github", "secret-sentinelle")
        fetch.assert_not_called()

    def test_panic_keeps_its_message_and_comes_before_the_failed_line(self):
        log = (
            "test a::b ... FAILED\n"
            "thread 'a::b' (1) panicked at /root/.cargo/registry/src/index-1/k-0.1/src/x.rs:7:6:\n"
            "No adapter found\n"
        )
        excerpt = status.failure_excerpt(log)
        self.assertIn("No adapter found", excerpt)
        self.assertNotIn("/root/.cargo", excerpt)
        self.assertEqual(status.failure_excerpt("test a::b ... FAILED\n"), "test a::b ... FAILED")

    def test_missing_job_is_a_failure_not_an_implicit_success(self):
        self.jobs.pop(0)
        self.report()
        self.assertEqual(self.posts[0]["state"], "failed")
        self.assertIn("missing", self.posts[0]["description"])

    def test_fork_pr_and_other_workflow_make_no_request(self):
        for field, value in [("event", "pull_request"), ("head_repository", {"full_name": "fork/repo"}), ("path", ".github/workflows/autre.yml")]:
            with self.subTest(field=field):
                candidate = event()
                candidate["workflow_run"][field] = value
                with patch.object(status, "request_json") as api:
                    with contextlib.redirect_stdout(io.StringIO()):
                        status.report(candidate, None, None)
                    api.assert_not_called()

    def test_old_attempt_does_not_republish(self):
        with patch.object(status, "request_json", return_value={"run_attempt": 3}) as api:
            with contextlib.redirect_stdout(io.StringIO()):
                status.report(self.event, "github", "gitlab")
            self.assertEqual(api.call_count, 1)

    def test_pagination_and_pipeline_creation(self):
        self.pipeline = []
        fake_jobs = [{"name": "autre", "id": 100, "conclusion": "success"}] * 100
        def paginated(url, headers, data=None):
            if "/jobs?" in url:
                self.pages.append(url)
                return {"total_count": 106, "jobs": fake_jobs if "&page=1" in url else self.jobs}
            return self.api(url, headers, data)
        with patch.object(status, "request_json", side_effect=paginated):
            with patch.object(status, "fetch_job_log", return_value=None):
                with contextlib.redirect_stdout(io.StringIO()):
                    status.report(self.event, "github", "secret-sentinelle")
        self.assertEqual(len(self.pages), 2)
        self.assertNotIn("pipeline_id", self.posts[0])
        self.assertTrue(all(p["pipeline_id"] == 99 for p in self.posts[1:]))
        self.assertTrue(all(p["state"] == "success" for p in self.posts))

    def test_http_errors_retried_bounded_and_no_secret_in_messages(self):
        success = io.BytesIO(b'{}')
        conflict = urllib.error.HTTPError("https://gitlab.com", 409, "secret-sentinelle", {}, None)
        with patch.object(status.urllib.request, "urlopen", side_effect=[conflict, success]) as api:
            with patch.object(status.time, "sleep") as sleep:
                self.assertEqual(status.request_json("https://gitlab.com", {"PRIVATE-TOKEN": "secret-sentinelle"}, {}), {})
                self.assertEqual(api.call_count, 2)
                sleep.assert_called_once_with(1)
        for code, attempts in [(401, 1), (403, 1), (503, 3)]:
            error = urllib.error.HTTPError("https://gitlab.com", code, "secret-sentinelle", {}, None)
            with patch.object(status.urllib.request, "urlopen", side_effect=error) as api:
                with patch.object(status.time, "sleep"):
                    with self.assertRaises(RuntimeError) as raised:
                        status.request_json("https://gitlab.com", {"PRIVATE-TOKEN": "secret-sentinelle"})
                self.assertNotIn("secret-sentinelle", str(raised.exception))
                self.assertEqual(api.call_count, attempts)


if __name__ == "__main__":
    unittest.main()

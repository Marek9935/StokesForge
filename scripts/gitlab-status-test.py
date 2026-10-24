#!/usr/bin/env python3
"""Contrats du retour GitHub → GitLab, sans réseau ni jeton réel."""

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

    def test_succes_et_reproduction_toleree_gardent_liens_sha_et_pipeline(self):
        self.jobs[4]["steps"] = [{"conclusion": "failure"}]
        self.report()
        self.assertEqual(len(self.posts), 6)
        for post in self.posts:
            self.assertEqual(post["pipeline_id"], 99)
            self.assertIn("Tentative 2", post["description"])
            self.assertRegex(post["target_url"], r"/actions/runs/42/job/\d+$")
        self.assertEqual([p["state"] for p in self.posts], ["success"] * 4 + ["skipped", "success"])
        self.assertIn("Échec non bloquant", self.posts[4]["description"])

    def test_echec_timeout_et_annulation_ne_deviennent_pas_verts(self):
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

    def test_echec_publie_etape_et_premiere_erreur_du_journal(self):
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
        self.assertIn("étape « Run cargo clippy »", description)
        self.assertIn("the borrowed expression", description)
        self.assertIn("crates/x/src/app.rs:1295:69", description)
        self.assertNotIn("unused variable", description)
        self.assertLessEqual(len(description), status.DESCRIPTION_MAX)

    def test_echec_poste_un_commentaire_de_commit_de_cinquante_lignes(self):
        self.jobs[3]["conclusion"] = "failure"
        self.jobs[3]["steps"] = [{"name": "Run cargo test", "conclusion": "failure"}]
        before = [f"ligne avant {n}" for n in range(30)]
        after = [f"ligne après {n}" for n in range(100)]
        self.log = "\n".join(before + ["error: la cause"] + after)
        self.report()
        self.assertEqual(len(self.comments), 1)
        note = self.comments[0]
        self.assertIn("github-actions/test", note)
        self.assertIn("/actions/runs/42/job/", note)
        self.assertIn("error: la cause", note)
        self.assertIn("ligne avant 29", note)
        self.assertNotIn("ligne avant 5\n", note)  # au-delà du contexte avant
        self.assertNotIn("ligne après 60", note)  # au-delà de 50 lignes
        code = note.split("```text\n")[1].split("\n```")[0]
        self.assertEqual(len(code.splitlines()), status.CONTEXT_LINES)

    def test_commentaire_seulement_pour_un_echec_et_sans_journal_il_reste_utile(self):
        self.report()
        self.assertEqual(self.comments, [])
        self.jobs[0]["conclusion"] = "failure"
        self.report()  # journal absent
        self.assertEqual(len(self.comments), 1)
        self.assertNotIn("```", self.comments[0])
        self.assertIn("Journal complet", self.comments[0])

    def test_commentaire_refuse_ne_casse_pas_le_rapport(self):
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

    def test_licences_deposees_en_paquet_gitlab_seulement_si_le_travail_reussit(self):
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
        self.assertEqual(len(self.posts), 18)  # les statuts passent toujours

    def test_depot_de_licences_refuse_ne_casse_pas_le_rapport(self):
        with patch.object(status, "request_json", side_effect=self.api):
            with patch.object(status, "fetch_artifact_file", return_value=b"x"):
                with patch.object(status, "upload_package", side_effect=RuntimeError("API : erreur HTTP 403")):
                    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                        status.report(self.event, "github", "secret-sentinelle")
        self.assertEqual(len(self.posts), 6)

    def test_artefact_zip_extrait_sans_envoyer_le_jeton_ailleurs(self):
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

    def test_redirection_non_https_ou_absente_refusee(self):
        for location in (None, "http://evil.example/x"):
            headers = {} if location is None else {"Location": location}
            error = urllib.error.HTTPError("https://api.github.com", 302, "redirect", headers, None)
            with patch.object(status.urllib.request.OpenerDirector, "open", side_effect=error):
                with patch.object(status.urllib.request, "urlopen") as direct:
                    self.assertIsNone(status.fetch_github_bytes("https://api.github.com/x", "gh", 10))
                    direct.assert_not_called()

    def test_journal_indisponible_ou_enorme_ne_casse_pas_le_rapport(self):
        self.jobs[3]["conclusion"] = "failure"
        self.jobs[3]["steps"] = [{"name": "Run cargo test", "conclusion": "failure"}]
        self.report()  # journal absent : l'étape seule suffit
        self.assertIn("étape « Run cargo test »", self.posts[3]["description"])
        self.posts.clear()
        self.log = "error: " + "x" * 5000
        self.report()
        self.assertLessEqual(len(self.posts[3]["description"]), status.DESCRIPTION_MAX)
        self.assertTrue(self.posts[3]["description"].endswith("…"))

    def test_succes_ne_telecharge_aucun_journal(self):
        with patch.object(status, "request_json", side_effect=self.api):
            with patch.object(status, "fetch_job_log") as fetch:
                with contextlib.redirect_stdout(io.StringIO()):
                    status.report(self.event, "github", "secret-sentinelle")
        fetch.assert_not_called()

    def test_la_panique_garde_son_message_et_passe_avant_la_ligne_failed(self):
        log = (
            "test a::b ... FAILED\n"
            "thread 'a::b' (1) panicked at /root/.cargo/registry/src/index-1/k-0.1/src/x.rs:7:6:\n"
            "No adapter found\n"
        )
        excerpt = status.failure_excerpt(log)
        self.assertIn("No adapter found", excerpt)
        self.assertNotIn("/root/.cargo", excerpt)
        self.assertEqual(status.failure_excerpt("test a::b ... FAILED\n"), "test a::b ... FAILED")

    def test_travail_absent_est_un_echec_et_non_un_succes_implicite(self):
        self.jobs.pop(0)
        self.report()
        self.assertEqual(self.posts[0]["state"], "failed")
        self.assertIn("absent", self.posts[0]["description"])

    def test_pr_fork_et_autre_workflow_ne_font_aucune_requete(self):
        for field, value in [("event", "pull_request"), ("head_repository", {"full_name": "fork/repo"}), ("path", ".github/workflows/autre.yml")]:
            with self.subTest(field=field):
                candidate = event()
                candidate["workflow_run"][field] = value
                with patch.object(status, "request_json") as api:
                    with contextlib.redirect_stdout(io.StringIO()):
                        status.report(candidate, None, None)
                    api.assert_not_called()

    def test_ancienne_tentative_ne_republie_pas(self):
        with patch.object(status, "request_json", return_value={"run_attempt": 3}) as api:
            with contextlib.redirect_stdout(io.StringIO()):
                status.report(self.event, "github", "gitlab")
            self.assertEqual(api.call_count, 1)

    def test_pagination_et_creation_du_pipeline(self):
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

    def test_erreurs_http_reprises_bornees_et_aucun_secret_dans_les_messages(self):
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

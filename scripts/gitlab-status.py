#!/usr/bin/env python3
"""Publie les résultats du miroir GitHub dans le pipeline GitLab du même commit.

Appelé par workflow_run terminé, jamais par le code testé d'une PR. Les jetons
restent dans l'environnement et les erreurs HTTP ne versent ni en-têtes ni
corps dans les journaux. Aucune bibliothèque externe n'est nécessaire.
"""

import json
import os
from pathlib import Path
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


REPOSITORY = "Marek9935/StokesForge"
GITHUB = f"https://api.github.com/repos/{REPOSITORY}"
GITLAB = "https://gitlab.com/api/v4/projects/86104040"
JOBS = ("fmt", "licenses", "clippy", "test", "robustesse", "test-python")


def request_json(url, headers, data=None):
    """Trois essais bornés pour les conflits de pipeline et erreurs transitoires."""
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
                # Ne pas afficher l'objet Request ni la réponse : ils peuvent
                # contenir des informations d'authentification.
                raise RuntimeError(f"API : erreur HTTP {error.code}") from None
        except (urllib.error.URLError, TimeoutError):
            if attempt == 2:
                raise RuntimeError("API injoignable après trois essais") from None
        time.sleep(2 ** attempt)


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
        raise RuntimeError("SHA du workflow invalide")
    if not isinstance(run.get("id"), int) or not isinstance(run.get("run_attempt"), int):
        raise RuntimeError("Identifiant ou tentative du workflow invalide")
    return run


def job_status(job, canceled=False):
    if job is None:
        return ("canceled" if canceled else "failed", "Travail absent de la tentative")
    conclusion = job.get("conclusion")
    if conclusion == "cancelled":
        return "canceled", "Travail annulé sur GitHub"
    # Une reproduction peut échouer sans faire échouer CI. L'API des statuts
    # n'a pas de paramètre allow_failure : le résultat reste explicitement
    # non validant (skipped), avec sa raison, plutôt qu'un faux succès.
    reproduction_failed = job["name"] == "robustesse" and (
        conclusion == "failure"
        or any(step.get("conclusion") == "failure" for step in job.get("steps", []))
    )
    if reproduction_failed:
        return "skipped", "Échec non bloquant des reproductions ; voir les journaux"
    states = {
        "success": "success",
        "failure": "failed",
        "timed_out": "failed",
        "cancelled": "canceled",
        "skipped": "skipped",
        "neutral": "skipped",
    }
    return states.get(conclusion, "failed"), f"Résultat GitHub : {conclusion or 'inconnu'}"


def report(event, github_token, gitlab_token):
    run = trusted_run(event)
    if run is None:
        print("Événement ignoré : seuls les push du workflow CI du miroir sont publiés")
        return
    if not github_token or not gitlab_token:
        raise RuntimeError("Configurer le secret GitHub GITLAB_STATUS_TOKEN (jeton GitLab API)")
    gh_headers = {"Authorization": f"Bearer {github_token}", "Accept": "application/vnd.github+json"}
    gl_headers = {"PRIVATE-TOKEN": gitlab_token}
    current = request_json(f"{GITHUB}/actions/runs/{run['id']}", gh_headers)
    if current["run_attempt"] != run["run_attempt"]:
        print("Ancienne tentative ignorée : une relance plus récente existe")
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

    # head_branch porte le nom de la référence testée, sans refs/heads ou refs/tags.
    ref = run["head_branch"]
    query = urllib.parse.urlencode({"sha": run["head_sha"], "ref": ref, "per_page": 1})
    pipeline_url = f"{GITLAB}/pipelines?{query}&order_by=id&sort=desc"
    pipelines = request_json(pipeline_url, gl_headers)
    pipeline_id = pipelines[0]["id"] if pipelines else None
    for name in JOBS:
        job = jobs.get(name)
        state, description = job_status(job, run.get("conclusion") == "cancelled")
        payload = {
            "state": state,
            "name": f"github-actions/{name}",
            "ref": ref,
            "target_url": (
                f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}/job/{job['id']}"
                if job else f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}"
            ),
            "description": f"Tentative {run['run_attempt']} — {description}",
        }
        if pipeline_id is not None:
            payload["pipeline_id"] = pipeline_id
        request_json(f"{GITLAB}/statuses/{run['head_sha']}", gl_headers, payload)
        if pipeline_id is None:
            # Le premier statut peut créer un pipeline externe. Les suivants
            # doivent tous rejoindre ce pipeline, même s'il n'existait pas avant.
            pipelines = request_json(pipeline_url, gl_headers)
            if not pipelines:
                raise RuntimeError("Pipeline GitLab créé mais introuvable")
            pipeline_id = pipelines[0]["id"]
        print(f"github-actions/{name} : {state}")


def main():
    try:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        # Le filtre précède même la lecture des secrets.
        if trusted_run(event) is None:
            print("Événement ignoré")
            return
        report(event, os.environ.get("GITHUB_TOKEN"), os.environ.get("GITLAB_STATUS_TOKEN"))
    except (RuntimeError, KeyError, ValueError, OSError) as error:
        print(f"Retour GitLab impossible : {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()

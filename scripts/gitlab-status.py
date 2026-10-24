#!/usr/bin/env python3
"""Publie les résultats du miroir GitHub dans le pipeline GitLab du même commit.

Appelé par workflow_run terminé, jamais par le code testé d'une PR. Les jetons
restent dans l'environnement et les erreurs HTTP ne versent ni en-têtes ni
corps dans les journaux. Aucune bibliothèque externe n'est nécessaire.
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
# GitLab borne la description d'un statut externe.
DESCRIPTION_MAX = 255
LOG_MAX = 20_000_000
# Rapport de licences : un fichier par push de main, déposé en paquet GitLab.
PACKAGE_MAX = 20_000_000
LICENSES_ARTIFACT = "third-party-licenses"
LICENSES_FILE = "THIRD-PARTY-LICENSES.txt"
# Commentaire de commit : de quoi comprendre l'échec sans ouvrir GitHub.
CONTEXT_BEFORE = 10
CONTEXT_LINES = 50
COMMENT_MAX = 15_000


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


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def fetch_github_bytes(url, github_token, limit):
    """Octets d'une ressource GitHub servie par redirection, ou None. Jamais fatal.

    L'API répond par une redirection vers un stockage signé. On la suit à la
    main, sans en-tête d'autorisation, pour ne pas envoyer le jeton GitHub à un
    autre domaine.
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
    """Journal d'un travail GitHub, ou None : il ne fait qu'enrichir le rapport."""
    data = fetch_github_bytes(f"{GITHUB}/actions/jobs/{job_id}/logs", github_token, LOG_MAX)
    return None if data is None else data.decode("utf-8", "replace")


def fetch_artifact_file(run_id, artifact, filename, github_token, gh_headers):
    """Contenu d'un fichier d'un artefact GitHub (zip), ou None."""
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
    """Dépose un fichier dans le registre de paquets génériques de GitLab."""
    url = f"{GITLAB}/packages/generic/{package}/{version}/{filename}"
    request = urllib.request.Request(
        url, data=data, method="PUT",
        headers={"PRIVATE-TOKEN": gitlab_token, "Content-Type": "application/octet-stream"},
    )
    try:
        with urllib.request.urlopen(request, timeout=60):
            return url
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"API : erreur HTTP {error.code}") from None
    except (urllib.error.URLError, TimeoutError):
        raise RuntimeError("API injoignable") from None


ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
TIMESTAMP = re.compile(r"^\d{4}-\d\d-\d\dT[\d:.]+Z ?")
# Première ligne qui dit pourquoi : erreur cargo/rustc, panique, test rouge,
# erreur de script. Les `warning:` et le bruit de compilation sont ignorés.
CAUSE = re.compile(
    r"^(error(\[E\d+\])?:|\s*thread '.*' .*panicked|\[licences\] ERREUR|Traceback|"
    r"E: |fatal:|.*: command not found|.*No such file)"
)
# Repli : un test rouge sans panique visible dans le journal.
TEST_FAILED = re.compile(r"^\S.* \.\.\. FAILED")
REGISTRY = re.compile(r"/\S*?/registry/src/[^/]+/")
LOCATION = re.compile(r"^\s*--> (\S+)")


def clean_lines(log):
    return [TIMESTAMP.sub("", ANSI.sub("", raw)).rstrip() for raw in log.splitlines()]


def find_cause(lines):
    """Indice de la première cause d'échec, ou None."""
    for pattern in (CAUSE, TEST_FAILED):
        for i, line in enumerate(lines):
            if pattern.match(line):
                return i
    return None


def failure_excerpt(log):
    """Première cause d'échec d'un journal, sur une ligne, ou None."""
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
    # Une panique : le message est sur la ligne suivante.
    if "panicked" in line and i + 1 < len(lines) and lines[i + 1].strip():
        excerpt += " : " + lines[i + 1].strip()
    return excerpt


def failure_context(log):
    """Une cinquantaine de lignes autour de la première cause, ou la fin du journal."""
    lines = clean_lines(log)
    i = find_cause(lines)
    start = max(0, i - CONTEXT_BEFORE) if i is not None else max(0, len(lines) - CONTEXT_LINES)
    # Une clôture de bloc dans le journal fermerait le commentaire trop tôt.
    text = "\n".join(lines[start : start + CONTEXT_LINES]).replace("```", "'" * 3)
    return text if len(text) <= COMMENT_MAX else text[:COMMENT_MAX] + "\n[…tronqué]"


def failure_comment(name, job_url, attempt, detail, log):
    """Commentaire de commit GitLab pour un travail en échec."""
    body = f"**github-actions/{name}** — tentative {attempt}, échec"
    if detail:
        body += f" ({detail})"
    body += f"\n\n[Journal complet sur GitHub]({job_url})\n"
    if log:
        body += f"\n```text\n{failure_context(log)}\n```\n"
    return body


def failure_detail(job, log):
    """Étape en échec + première cause, pour la description du statut."""
    step = next(
        (s.get("name") for s in job.get("steps", []) if s.get("conclusion") == "failure"), None
    )
    parts = []
    if step:
        parts.append(f"étape « {step} »")
    excerpt = failure_excerpt(log) if log else None
    if excerpt:
        parts.append(excerpt)
    return " : ".join(parts) or None


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
            "description": clip(f"Tentative {run['run_attempt']} — {description}"),
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
                # Le statut est déjà publié : le commentaire n'est qu'un plus.
                print(f"Commentaire non publié pour {name} : {error}", file=sys.stderr)
        if pipeline_id is None:
            # Le premier statut peut créer un pipeline externe. Les suivants
            # doivent tous rejoindre ce pipeline, même s'il n'existait pas avant.
            pipelines = request_json(pipeline_url, gl_headers)
            if not pipelines:
                raise RuntimeError("Pipeline GitLab créé mais introuvable")
            pipeline_id = pipelines[0]["id"]
        print(f"github-actions/{name} : {state}")
    publish_licenses(run, jobs.get("licenses"), github_token, gitlab_token, gh_headers)


def publish_licenses(run, job, github_token, gitlab_token, gh_headers):
    """Rapport de licences en paquet GitLab, lisible sans passer par GitHub."""
    if not job or job.get("conclusion") != "success":
        return
    try:
        data = fetch_artifact_file(run["id"], LICENSES_ARTIFACT, LICENSES_FILE, github_token, gh_headers)
        if data is None:
            print("Rapport de licences introuvable sur GitHub", file=sys.stderr)
            return
        url = upload_package(LICENSES_ARTIFACT, run["head_sha"][:12], LICENSES_FILE, data, gitlab_token)
        print(f"Rapport de licences déposé : {url}")
    except RuntimeError as error:
        # Les statuts sont déjà publiés : le rapport n'est qu'un plus.
        print(f"Rapport de licences non déposé : {error}", file=sys.stderr)


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

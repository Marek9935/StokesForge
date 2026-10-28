#!/usr/bin/env bash
# Résultats de la CI Linux (GitHub) lus depuis GitLab, pour un commit.
#
#   scripts/ci-statuts.sh [commit]     # HEAD par défaut
#
# Affiche un statut par travail `github-actions/<travail>` (avec l'étape fautive
# et la première cause en cas d'échec), puis les commentaires de commit posés par
# le rapporteur (≈50 lignes de journal par échec). Voir docs/tests.md,
# « Using CI day to day ». Exige `glab` connecté à gitlab.com.
set -euo pipefail

PROJECT_ID=86104040
PROJECT_PATH=askrzypczak/stokesforge

command -v glab > /dev/null || { echo "glab est requis (glab auth login)" >&2; exit 1; }
sha="$(git rev-parse "${1:-HEAD}")"

# Les statuts externes ne sont pas listés par l'API REST des commits : on passe
# par le pipeline GitLab du commit (SHA complet obligatoire) et son étage `external`.
iid="$(glab api "projects/$PROJECT_ID/pipelines?sha=$sha" \
    | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d[0]["iid"] if d else "")')"
if [ -z "$iid" ]; then
    echo "Pas de pipeline GitLab pour $sha : la CI GitHub n'a pas encore fini, ou le miroir n'a pas reçu le commit." >&2
    exit 2
fi

echo "== $sha (pipeline GitLab #$iid)"
glab api graphql -f query="{project(fullPath:\"$PROJECT_PATH\"){pipeline(iid:\"$iid\"){stages{nodes{name jobs{nodes{name status detailedStatus{label}}}}}}}}" \
    | python3 -c '
import json, sys
for stage in json.load(sys.stdin)["data"]["project"]["pipeline"]["stages"]["nodes"]:
    for job in stage["jobs"]["nodes"]:
        label = job["detailedStatus"]["label"]
        print("%-34s %-8s | %s" % (job["name"], job["status"], label))
'

echo
echo "== commentaires du commit"
glab api "projects/$PROJECT_ID/repository/commits/$sha/comments" | python3 -c '
import json, sys
notes = json.load(sys.stdin)
if not notes:
    print("(aucun : pas d'"'"'échec bloquant)")
for note in notes:
    print(note["note"]); print("-----")
'

#!/bin/bash
# Mesure le coût par image pendant le déplacement d'une fenêtre flottante
# (#340/#341). Prérequis : `git apply scripts/perf/instrumentation.patch`,
# puis build du profil voulu. Détails : docs/mesure-performance.md.
#
# usage : scripts/perf/mesure-fenetres.sh debug|release [nb_graphes_a_ouvrir (0-6)]
set -euo pipefail
profil=${1:?debug|release}
nb=${2:-6}
racine=$(cd "$(dirname "$0")/../.." && pwd)
driver="$racine/.claude/skills/run-stokesforge/driver.py"
dir_perf="$racine/scripts/perf"
export SF_APP="$racine/target/$profil/stokesforge-app"
export STOKESFORGE_PROF=1
[ -x "$SF_APP" ] || { echo "$SF_APP absent : cargo build [--release] -p stokesforge-app" >&2; exit 1; }

essai() {
    python3 "$driver" start > /dev/null
    # Double-clic sur les graphes du Project Manager (Panneaux … Lignes).
    for ((i = 0; i < nb; i++)); do python3 "$driver" click 60 $((216 + 27 * i)) double; done
    python3 "$driver" key Escape   # sort du renommage laissé par le dernier double-clic
    sleep 6
    # Glisse la barre de titre d'une fenêtre qui n'occupe pas toute la largeur.
    python3 "$dir_perf/drag.py" 440 138
    # Le tableau ne s'imprime qu'à la 1re image suivant 2 s : deux mouvements de
    # souris espacés en réclament une, sinon le dernier bloc reste muet.
    sleep 2.5; python3 "$driver" move 700 400; sleep 2.5; python3 "$driver" move 710 410
    sortie=$(python3 "$dir_perf/summ.py")
    python3 "$driver" stop app   # garde Xwayland pour l'essai suivant
}
# Le glisser est parfois perdu (surtout en debug, où l'app est lente) : 3 essais.
for tentative in 1 2 3; do
    essai
    case "$sortie" in *"raté"*) echo "essai $tentative : glissement raté" >&2 ;; *) break ;; esac
done
python3 "$driver" stop
echo "### $profil, $nb graphes ouverts (+ Book1 et Graph1)"
echo "$sortie"

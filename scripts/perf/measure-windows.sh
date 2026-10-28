#!/bin/bash
# Measures the cost per frame while a floating window is being moved
# (#340/#341). Prerequisite: `git apply scripts/perf/instrumentation.patch`,
# then build of the wanted profile. Details: docs/performance-measurement.md.
#
# usage: scripts/perf/measure-windows.sh debug|release [number_of_graphs_to_open (0-6)]
set -euo pipefail
profil=${1:?debug|release}
nb=${2:-6}
racine=$(cd "$(dirname "$0")/../.." && pwd)
driver="$racine/.claude/skills/run-stokesforge/driver.py"
dir_perf="$racine/scripts/perf"
export SF_APP="$racine/target/$profil/stokesforge-app"
export STOKESFORGE_PROF=1
[ -x "$SF_APP" ] || { echo "$SF_APP missing: cargo build [--release] -p stokesforge-app" >&2; exit 1; }

essai() {
    python3 "$driver" start > /dev/null
    # Double-click on the graphs in the Project Manager (Panels … Rows).
    for ((i = 0; i < nb; i++)); do python3 "$driver" click 60 $((216 + 27 * i)) double; done
    python3 "$driver" key Escape   # leaves the rename left by the last double-click
    sleep 6
    # Drag the title bar of a window that does not take the full width.
    python3 "$dir_perf/drag.py" 440 138
    # The table is printed only at the 1st frame after 2 s: two spaced mouse
    # moves request one, otherwise the last block stays silent.
    sleep 2.5; python3 "$driver" move 700 400; sleep 2.5; python3 "$driver" move 710 410
    sortie=$(python3 "$dir_perf/summ.py")
    python3 "$driver" stop app   # keeps Xwayland for the next attempt
}
# The drag is sometimes lost (mostly in debug, where the app is slow): 3 attempts.
for tentative in 1 2 3; do
    essai
    case "$sortie" in *"missed"*) echo "attempt $tentative: drag missed" >&2 ;; *) break ;; esac
done
python3 "$driver" stop
echo "### $profil, $nb graphs open (+ Book1 and Graph1)"
echo "$sortie"

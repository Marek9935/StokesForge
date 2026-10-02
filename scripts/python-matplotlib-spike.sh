#!/usr/bin/env bash
# Spike #69 : copie du runtime normal, roues figées, test embarqué hors écran.
# La copie vit dans extracted/ (ignoré par Git), pas dans la distribution normale.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="$REPO_ROOT/vendor/python/extracted/python"
SPIKE_ROOT="$REPO_ROOT/vendor/python/extracted/matplotlib-spike"
PREFIX="$SPIKE_ROOT/python"
[[ -x "$BASE/bin/python3" ]] || { echo 'Run scripts/python-vendor.sh first' >&2; exit 1; }
if [[ ! -f "$SPIKE_ROOT/base-lock.env" ]] || ! cmp -s "$REPO_ROOT/vendor/python/lock.env" "$SPIKE_ROOT/base-lock.env"; then
    rm -rf "$SPIKE_ROOT"
    mkdir -p "$SPIKE_ROOT"
    cp -a "$BASE" "$PREFIX"
    cp "$REPO_ROOT/vendor/python/lock.env" "$SPIKE_ROOT/base-lock.env"
fi
"$PREFIX/bin/python3" -I -m pip install --no-deps --only-binary=:all: -r "$REPO_ROOT/vendor/python/matplotlib-requirements.txt"
"$PREFIX/bin/python3" -I -m pip check
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
# Même libpython ABI que la base : conserver son rpath évite un second build.
export RUSTFLAGS="-C link-arg=-Wl,-rpath,$BASE/lib"
export STOKESFORGE_PYTHON_HOME="$PREFIX"
cd "$REPO_ROOT"
cargo run -p stokesforge-script --features python --example matplotlib-spike
# Coût d'import en processus frais : premier échantillon puis médiane des suivants.
"$BASE/bin/python3" -I - "$BASE" "$PREFIX" <<'PY'
import json, pathlib, statistics, subprocess, sys, tempfile
base, spike = map(pathlib.Path, sys.argv[1:])
code = "import time; t=time.perf_counter(); import numpy; n=time.perf_counter(); import matplotlib; from matplotlib.backends.backend_svg import FigureCanvasSVG; m=time.perf_counter(); print(n-t, m-n)"
samples = []
with tempfile.TemporaryDirectory() as cfg:
    for _ in range(6):
        env = {'MPLCONFIGDIR': cfg}
        output = subprocess.check_output([str(spike/'bin/python3'), '-I', '-c', code], env=env, text=True)
        samples.append([float(x)*1000 for x in output.split()])
def size(p):
    return int(subprocess.check_output(['du', '-sb', str(p)], text=True).split()[0])
b, s = size(base), size(spike)
report = dict(base_bytes=b, spike_bytes=s, added_bytes=s-b, samples_numpy_then_matplotlib_ms=samples, matplotlib_first_ms=samples[0][1], matplotlib_median_fresh_process_ms=statistics.median(x[1] for x in samples[1:]))
path = pathlib.Path('/tmp/stokesforge69-measurements.json')
path.write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report, indent=2))
PY

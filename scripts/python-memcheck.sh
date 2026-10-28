#!/usr/bin/env bash
# Instrumented check of the zero-copy bridge (task 5.2): runs the test binary of
# tests/zerocopy.rs under valgrind memcheck.
#
# What it looks for: INVALID reads or writes at the Rust/PyO3/numpy boundary,
# not leaks. That is why:
#   - no --leak-check option is passed: the "still reachable" allocations of
#     the CPython allocator (interpreter never finalized, see
#     src/interpreter.rs) are many and EXPECTED; counting them as leaks would
#     drown the useful signal in known, accepted noise;
#   - PYTHONMALLOC=malloc is set BEFORE running the binary: without it,
#     CPython's small-object allocator ("pymalloc") reuses memory areas in a
#     way valgrind does not understand, and produces a flood of false
#     "uninitialized read" positives. `malloc` routes all Python allocations
#     through the system malloc, the only path memcheck can follow correctly;
#   - --error-exitcode=1: a memcheck that finds an error must make this script
#     fail, not just produce an ignored report.
#
# `miri` is NOT an alternative here: miri does not run foreign library calls
# (FFI), so nothing that crosses PyO3 (this whole module). It remains the
# relevant tool for the rest of the StokesForge model, which is pure Rust
# without FFI.
#
# `valgrind` is NOT installed on this development machine (checked:
# `which valgrind` fails). This script detects it, says so clearly on
# stderr, and exits 0: the absence of the tool is not a failure of the
# zero-copy bridge, but a check that did not run. Nobody must be able to read
# "this script passed" as "the instrumented check took place".
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"

if ! command -v valgrind >/dev/null 2>&1; then
    cat >&2 <<'EOF'
[memcheck] valgrind missing: the instrumented check did not run.
[memcheck]   sudo dnf install valgrind
[memcheck] then rerun this script.
[memcheck] (nothing was verified here; this is NOT a confirmation that the
[memcheck]  zero-copy bridge is memory-safe, only that the tool is missing.)
EOF
    exit 0
fi

# --- 1. Building the test binary --------------------------------------------
# Same three variables as scripts/python-test.sh (see its header for the
# detail of the three pitfalls they cover).
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$PREFIX/lib"
export STOKESFORGE_PYTHON_HOME="$PREFIX"

cd "$REPO_ROOT"

# The name of the produced binary contains an unpredictable build hash
# (target/debug/deps/zerocopy-<hash>); we find it afterwards by taking, among
# the executable "zerocopy-*" binaries, the most recently modified one (marker
# set just before the build).
REPERE="$REPO_ROOT/target/.memcheck-repere"
mkdir -p "$REPO_ROOT/target"
touch "$REPERE"

echo "[memcheck] building the tests/zerocopy.rs binary" >&2
cargo test -p stokesforge-python --features python --test zerocopy --no-run

BINARY="$(find "$REPO_ROOT/target/debug/deps" -maxdepth 1 -type f -executable \
    -name 'zerocopy-*' -newer "$REPERE" -print | head -n1)"
rm -f "$REPERE"

if [[ -z "$BINARY" ]]; then
    echo "[memcheck] ERROR: tests/zerocopy.rs binary not found after the build" >&2
    exit 1
fi
echo "[memcheck] binary: $BINARY" >&2

# --- 2. Running under valgrind -----------------------------------------------
# --test-threads=1: one test thread at a time. CPython starts only once per
# process (Once, see tests/zerocopy.rs), so parallelism changes nothing for
# coverage; run serially, a valgrind report that points at an error stays
# readable right on the test that triggered it.
#
# STOKESFORGE_UNDER_VALGRIND=1: set for the test binary, not for valgrind
# itself. The measurement test (borrow vs copy) reads it to know that it runs
# under instrumentation: valgrind slows the borrow (dominated by a few calls)
# and the copy (dominated by a memory transfer) in very different ways, which
# makes the RATIO measured under instrumentation meaningless; see
# tests/zerocopy.rs for the detail. The test then prints its two measurements
# without asserting the ratio, rather than failing on a false negative.
# --nocapture: without it, libtest swallows the output of passing tests, and
# it is precisely under valgrind that we want to SEE the two measurements of
# the test emprunt_au_moins_vingt_fois_plus_rapide_que_la_copie, not only to
# know that it passed.
echo "[memcheck] running under valgrind (PYTHONMALLOC=malloc, without --leak-check)" >&2
PYTHONMALLOC=malloc STOKESFORGE_UNDER_VALGRIND=1 valgrind \
    --error-exitcode=1 \
    --track-origins=yes \
    "$BINARY" --test-threads=1 --nocapture

echo "[memcheck] OK: no invalid read/write detected by valgrind" >&2

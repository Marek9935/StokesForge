#!/usr/bin/env bash
# Runs the Python tests: bridge, scripts, robustness, application and CLI
# against the same vendored interpreter.
#
# Three things to set, and nothing more:
#   - PYO3_CONFIG_FILE: where PyO3 finds libpython at COMPILE time (pitfall no. 1);
#   - RUSTFLAGS -rpath: where the test binary finds libpython at RUN time
#     (pitfall no. 2). Here the rpath is absolute: the test binary is not
#     distributed; the distribution is what uses $ORIGIN;
#   - STOKESFORGE_PYTHON_HOME: the embedded prefix, for tests only.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"

PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
# Only the crates listed below are built here: this rpath is not imposed on
# any other crate of the repository.
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
        # Windows (#332): no rpath; python312.dll is found through PATH.
        export PATH="$PREFIX:$PATH" ;;
    *)
        export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$PREFIX/lib" ;;
esac
export STOKESFORGE_PYTHON_HOME="$PREFIX"

cd "$REPO_ROOT"
# One invocation per crate rather than a single `-p A -p B`: each has its own
# `python` feature, and `set -e` stops everything as soon as one fails.
cargo test -p stokesforge-python --features python "$@"
cargo test -p stokesforge-script --features python "$@"

# Robustness bench, boundary batteries (b7, b9).
#
# `RUST_TEST_THREADS=1` rather than `-- --test-threads=1`: the latter would
# take the only `--` that cargo accepts, and so would prevent writing
# `scripts/python-test.sh -- --ignored`. The variable does exactly the same,
# without competing for the position of the arguments.
#
# Why a single thread: these batteries share one interpreter and one script
# engine per test file (CPython starts only once per process), and the
# interruption cases read `is_running()`, which only makes sense if a single
# script runs at a time.
#
# The REPRODUCTIONS of anomalies are `#[ignore]` there (see the crate's top
# doc): this line runs only the guards, and stays green. To replay the
# reproductions of the Python boundary:
#     scripts/python-test.sh -- --ignored
RUST_TEST_THREADS=1 cargo test -p stokesforge-robustness --features python "$@"

# Console integration and headless CLI: same variables, same runtime.
# The state (script console, lazy bridge) lives in `stokesforge-app-state`
# since #263; the `stokesforge-app` shell relays the feature.
cargo test -p stokesforge-app-state --features python "$@"
cargo test -p stokesforge-app --features python "$@"
cargo test -p stokesforge-cli --features python "$@"

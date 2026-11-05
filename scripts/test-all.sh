#!/usr/bin/env bash
# Whole-workspace test run (#452): cargo-nextest when installed (tests in
# parallel processes, no-fail-fast), then the doc tests that nextest does not
# run; otherwise plain `cargo test --workspace`. Takes no argument: to filter,
# use `cargo test -p <crate> <name>`.
#
#   scripts/test-all.sh
set -euo pipefail
if [ "$#" -ne 0 ]; then
    echo "test-all.sh takes no argument; filter with: cargo test -p <crate> <name>" >&2
    exit 2
fi
cd "$(dirname "$0")/.."
if cargo nextest --version > /dev/null 2>&1; then
    cargo nextest run --workspace --no-fail-fast
    cargo test --workspace --doc
else
    cargo test --workspace
fi

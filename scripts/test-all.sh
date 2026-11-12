#!/usr/bin/env bash
# Whole-workspace test run (#452): cargo-nextest when installed (tests in
# parallel processes, no-fail-fast), then the doc tests that nextest does not
# run; otherwise plain `cargo test --workspace`. Takes no argument: to filter,
# use `cargo test -p <crate> <name>`.
#
# After a green run it writes the stamp `<git common dir>/sf-tested/<HEAD sha>` (#515), which
# scripts/deliver.sh requires before merging. The stamp is written only if the tree was
# clean (untracked files included) when the run started and HEAD did not move during it;
# otherwise a notice is printed and nothing is written. Stamps older than 30 days are pruned.
#
#   scripts/test-all.sh
set -euo pipefail
if [ "$#" -ne 0 ]; then
    echo "test-all.sh takes no argument; filter with: cargo test -p <crate> <name>" >&2
    exit 2
fi
cd "$(dirname "$0")/.."
start_head="$(git rev-parse HEAD 2> /dev/null || true)"
start_dirty="$(git status --porcelain 2> /dev/null || echo unknown)"
if cargo nextest --version > /dev/null 2>&1; then
    cargo nextest run --workspace --no-fail-fast
    cargo test --workspace --doc
else
    cargo test --workspace
fi

# Green: stamp the tested commit (#515).
if [ -z "$start_head" ]; then
    exit 0
elif [ -n "$start_dirty" ]; then
    echo "test-all: tests passed, but the tree was not clean when they started: no stamp written" >&2
elif [ "$(git rev-parse HEAD)" != "$start_head" ]; then
    echo "test-all: tests passed, but HEAD moved during the run: no stamp written" >&2
else
    stamps="$(git rev-parse --path-format=absolute --git-common-dir)/sf-tested"
    mkdir -p "$stamps"
    : > "$stamps/$start_head"
    find "$stamps" -type f -mtime +30 -delete
    echo "test-all: stamp written for $start_head"
fi

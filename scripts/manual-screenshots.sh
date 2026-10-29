#!/usr/bin/env bash
# Renders the screenshots of the user manual (#365) into manual/src/images/.
# The real application is drawn headless on the demo project (egui_kittest,
# English, light palette, 1440 x 780 points at 2x), by the ignored test
# `render_manual_screenshots` of crates/stokesforge-app/src/app/manual_screenshots.rs.
# A PNG is rewritten only if its pixels changed: a second run changes nothing.
# Then `git status` shows which images changed. The list of images (and the
# ones taken by hand) is manual/screenshots.json.
#
# LOCAL ONLY: it needs a GPU adapter for wgpu (or a software one, lavapipe), so
# it is not part of the CI. Isolated from the real profile (temporary
# configuration folder).
#
# Usage: scripts/manual-screenshots.sh [name-filter]
#   name-filter: only the shots whose name contains it (e.g. import-wizard)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [ -n "${1:-}" ]; then
    export STOKESFORGE_SHOT="$1"
fi

cargo test -p stokesforge-app --release --lib render_manual_screenshots \
    -- --ignored --test-threads=1 --nocapture

echo "[manual-screenshots] changed images:"
git status --short -uall manual/src/images

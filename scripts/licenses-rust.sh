#!/usr/bin/env bash
# cargo-about + upstream notices missing from the crates.io packages. Path shared
# by CI and the distributions, so that both keep the same copyrights.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ $# -eq 1 ]] || {
    echo "Usage: scripts/licenses-rust.sh <notices file>" >&2
    exit 1
}
OUTPUT_FILE="$1"
[[ "$OUTPUT_FILE" = /* ]] || OUTPUT_FILE="$PWD/$OUTPUT_FILE"
cd "$REPO_ROOT"
if ! sha256sum --check --status vendor/crate-license-notices/checksums.sha256; then
    echo "[licences] ERROR: update the supplementary notices to match Cargo.lock" >&2
    exit 1
fi
# shellcheck source=licenses-tools.env
source scripts/licenses-tools.env
[[ "$(cargo-about --version)" == "cargo-about ${CARGO_ABOUT_VERSION}" ]] || {
    echo "[licences] ERROR: use the version from scripts/licenses-tools.env" >&2
    exit 1
}
mkdir -p "$(dirname "$OUTPUT_FILE")"
TEMP_FILE="$(mktemp "$(dirname "$OUTPUT_FILE")/.licenses-rust-XXXXXX")"
trap 'rm -f "$TEMP_FILE"' EXIT
# cargo-about is a native executable: under Git Bash (path conversion turned
# off by app-dist-windows.sh), it needs a Windows path.
ABOUT_OUT="$TEMP_FILE"
if command -v cygpath >/dev/null 2>&1; then ABOUT_OUT="$(cygpath -m "$TEMP_FILE")"; fi
cargo about generate --workspace --all-features --locked --fail \
    scripts/licenses.hbs -o "$ABOUT_OUT"
cat vendor/crate-license-notices/NOTICE.txt >> "$TEMP_FILE"
mv "$TEMP_FILE" "$OUTPUT_FILE"

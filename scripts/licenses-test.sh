#!/usr/bin/env bash
# Refusal witnesses: an incomplete delivery, stale notices and an out-of-policy
# license must fail before a false green result is delivered.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROBE_DIR="$(mktemp -d)"
trap 'rm -rf "$PROBE_DIR"' EXIT

# The refusals hold for each package platform (#332): a delivery without a
# notice, notices of another version or of another triple.
mkdir -p "$PROBE_DIR/repo/scripts" "$PROBE_DIR/repo/vendor/python"
cp "$REPO_ROOT/scripts/licenses-dist.sh" "$REPO_ROOT/scripts/licenses-tools.env" \
    "$PROBE_DIR/repo/scripts/"
cp "$REPO_ROOT/vendor/python/lock.env" "$PROBE_DIR/repo/vendor/python/"
for platform in linux windows; do
    export STOKESFORGE_LICENSES_PLATFORM="$platform"
    mkdir -p "$PROBE_DIR/package-$platform"
    if "$REPO_ROOT/scripts/licenses-dist.sh" "$PROBE_DIR/package-$platform" > "$PROBE_DIR/missing-$platform.log" 2>&1; then
        echo "[licences] ERROR: a $platform runtime without a notice was accepted" >&2
        exit 1
    fi
    grep -Fq 'notice missing from the package' "$PROBE_DIR/missing-$platform.log"
    # Minimal copy: never alter the repository lock for these witnesses.
    mkdir -p "$PROBE_DIR/repo/vendor/python/licenses-$platform"
    versions="$REPO_ROOT/vendor/python/licenses-$platform/versions.env"
    sed 's/^LICENSE_PYTHON_VERSION=.*/LICENSE_PYTHON_VERSION=0.0.0/' "$versions" \
        > "$PROBE_DIR/repo/vendor/python/licenses-$platform/versions.env"
    if "$PROBE_DIR/repo/scripts/licenses-dist.sh" "$PROBE_DIR/package-$platform" > "$PROBE_DIR/stale-$platform.log" 2>&1; then
        echo "[licences] ERROR: stale $platform notices were accepted" >&2
        exit 1
    fi
    grep -Fq "update vendor/python/licenses-$platform" "$PROBE_DIR/stale-$platform.log"

    # Notices of another platform (different triple): refused.
    other_triple="x86_64-unknown-linux-gnu"
    [[ "$platform" == linux ]] && other_triple="x86_64-pc-windows-msvc"
    sed "s/^LICENSE_PBS_TRIPLE=.*/LICENSE_PBS_TRIPLE=$other_triple/" "$versions" \
        > "$PROBE_DIR/repo/vendor/python/licenses-$platform/versions.env"
    if "$PROBE_DIR/repo/scripts/licenses-dist.sh" "$PROBE_DIR/package-$platform" > "$PROBE_DIR/triple-$platform.log" 2>&1; then
        echo "[licences] ERROR: notices of another platform were accepted ($platform)" >&2
        exit 1
    fi
    grep -Fq "update vendor/python/licenses-$platform" "$PROBE_DIR/triple-$platform.log"
done
unset STOKESFORGE_LICENSES_PLATFORM

mkdir -p "$PROBE_DIR/repo/vendor/crate-license-notices"
cp "$REPO_ROOT/scripts/licenses-rust.sh" "$PROBE_DIR/repo/scripts/"
cp "$REPO_ROOT/vendor/crate-license-notices/"{checksums.sha256,NOTICE.txt,provenance.json} \
    "$PROBE_DIR/repo/vendor/crate-license-notices/"
echo 'stale lock' > "$PROBE_DIR/repo/Cargo.lock"
if "$PROBE_DIR/repo/scripts/licenses-rust.sh" "$PROBE_DIR/notices.txt" > "$PROBE_DIR/lock.log" 2>&1; then
    echo "[licences] ERROR: supplements for another lock were accepted" >&2
    exit 1
fi
grep -Fq 'update the supplementary notices to match Cargo.lock' "$PROBE_DIR/lock.log"

# The notices file is readable by everyone (#486): mktemp makes it 600 and the
# mv used to keep it. A fake cargo-about stands in for the real generator.
mkdir -p "$PROBE_DIR/fakebin" "$PROBE_DIR/repo-ok/scripts" "$PROBE_DIR/repo-ok/vendor/crate-license-notices"
cat > "$PROBE_DIR/fakebin/cargo-about" <<'SH'
#!/bin/sh
echo "cargo-about $FAKE_ABOUT_VERSION"
SH
cat > "$PROBE_DIR/fakebin/cargo" <<'SH'
#!/bin/sh
# cargo about generate ... -o <file>
while [ $# -gt 0 ]; do
    if [ "$1" = -o ]; then echo "generated notices" >> "$2"; fi
    shift
done
SH
chmod +x "$PROBE_DIR/fakebin/cargo" "$PROBE_DIR/fakebin/cargo-about"
cp "$REPO_ROOT/scripts/licenses-rust.sh" "$REPO_ROOT/scripts/licenses-tools.env" "$PROBE_DIR/repo-ok/scripts/"
cp "$REPO_ROOT/Cargo.lock" "$PROBE_DIR/repo-ok/"
cp "$REPO_ROOT/vendor/crate-license-notices/"{checksums.sha256,NOTICE.txt,provenance.json} \
    "$PROBE_DIR/repo-ok/vendor/crate-license-notices/"
FAKE_ABOUT_VERSION="$(sed -n 's/^CARGO_ABOUT_VERSION=//p' "$REPO_ROOT/scripts/licenses-tools.env" | tr -d '"')"
export FAKE_ABOUT_VERSION
PATH="$PROBE_DIR/fakebin:$PATH" "$PROBE_DIR/repo-ok/scripts/licenses-rust.sh" "$PROBE_DIR/ok/notices.txt" > "$PROBE_DIR/ok.log" 2>&1 \
    || { cat "$PROBE_DIR/ok.log" >&2; exit 1; }
if [[ "$(stat -c %a "$PROBE_DIR/ok/notices.txt")" != 644 ]]; then
    echo "[licences] ERROR: the notices file is not readable by everyone (mode $(stat -c %a "$PROBE_DIR/ok/notices.txt"))" >&2
    exit 1
fi

mkdir -p "$PROBE_DIR/crate/src"
cat > "$PROBE_DIR/crate/Cargo.toml" <<'TOML'
[package]
name = "licence-interdite"
version = "0.1.0"
license = "GPL-2.0-only"
TOML
: > "$PROBE_DIR/crate/src/lib.rs"
if cargo deny --offline --manifest-path "$PROBE_DIR/crate/Cargo.toml" \
    --config "$REPO_ROOT/deny.toml" check licenses > "$PROBE_DIR/rejected.log" 2>&1; then
    echo "[licences] ERROR: an out-of-policy license was accepted" >&2
    exit 1
fi
grep -Fq 'error[rejected]' "$PROBE_DIR/rejected.log"
echo "[licences] OK: missing and stale notices, notices of another platform, different lock, unreadable output and out-of-policy license refused"

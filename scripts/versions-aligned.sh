#!/usr/bin/env bash
# Checks that all workspace crates have the same version X.Y.Z
# (#349, #350). Reads neither tag nor CHANGELOG.
#
# Usage: scripts/versions-aligned.sh [X.Y.Z]
#   no argument: the crates must be equal to each other;
#   with argument: they must all be at X.Y.Z.
#
# Refuses (code 1) if a crate drifts (lists each crate and its version), if
# no crate is found, or if the [package] section of a crate does not have
# exactly one line `version = "…"` (`version.workspace = true` refused: not
# read here). Only the [package] section is read (#352).
# Internal dependencies `stokesforge-x = { version = "…" }` must have the
# same version (path only is allowed; table [dependencies.stokesforge-x] and
# `package = "stokesforge-…"` refused, not read), and FORMAT_VERSION of
# stokesforge-persistence requires the witness tests/witnesses/vN/project.stf (#353).
# Cargo.lock must hold each workspace crate at the same version (#351):
# refused (code 1) if the lock file is missing or behind (`cargo update --workspace`).
# Called by the `fmt` job (GitHub and GitLab) and by scripts/release-notes.sh.
# STOKESFORGE_REPO_ROOT (tests) replaces the repository root.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
WANTED="${1:-}"

names=()
versions=()
bad=0
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    name="$(basename "$(dirname "$manifest")")"
    # [package] section only: from [package] to the next section header.
    found="$(awk '
        /^\[/ { in_pkg = ($0 ~ /^\[package\][[:space:]]*(#.*)?$/); next }
        in_pkg && /^version\.workspace[[:space:]]*=/ { ws = 1 }
        in_pkg && /^version[[:space:]]*=[[:space:]]*"/ {
            v = $0
            sub(/^version[[:space:]]*=[[:space:]]*"/, "", v)
            sub(/".*$/, "", v)
            n++; val = v
        }
        END {
            if (ws) { print "WORKSPACE"; exit }
            if (n > 1) { print "DOUBLE"; exit }
            if (n == 1) print val
        }' "$manifest")"
    if [ "$found" = WORKSPACE ]; then
        echo "[versions] ERROR: $name inherits \"version.workspace = true\", not read here: write \"version = \\"X.Y.Z\\"\" in [package]" >&2
        bad=1
        continue
    fi
    if [ "$found" = DOUBLE ]; then
        echo "[versions] ERROR: $name has several \"version =\" lines in [package]" >&2
        bad=1
        continue
    fi
    if [ -z "$found" ]; then
        echo "[versions] ERROR: $name has no line \"version = \\"X.Y.Z\\"\" in [package]" >&2
        bad=1
        continue
    fi
    names+=("$name")
    versions+=("$found")
done

if [ "${#names[@]}" = 0 ] && [ "$bad" = 0 ]; then
    echo "[versions] ERROR: no crate found in $REPO_ROOT/crates" >&2
    exit 1
fi

reference="${WANTED:-${versions[0]:-}}"
for i in "${!names[@]}"; do
    if [ "${versions[$i]}" != "$reference" ]; then
        echo "[versions] ERROR: ${names[$i]} is ${versions[$i]}, expected $reference" >&2
        bad=1
    fi
done

# Internal dependencies: `stokesforge-x = { version = "V", … }` in any
# *dependencies section (dev-, build-, target.'cfg(…)'.*) must have the
# reference version. A path-only dependency is allowed. The forms this check
# cannot read are refused rather than let through (#353).
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    name="$(basename "$(dirname "$manifest")")"
    dep_errors="$(awk -v ref="$reference" -v crate="$name" '
        /^[[:space:]]*\[/ {
            sect = $0
            dep_sect = (sect ~ /dependencies[[:space:]]*\][[:space:]]*(#.*)?$/)
            if (sect ~ /dependencies\.stokesforge-/) {
                printf "%s : internal dependency as a [..] table, unreadable form (write stokesforge-x = { version = \"%s\", path = \"…\" }) : %s\n", crate, ref, sect
            }
            next
        }
        /package[[:space:]]*=[[:space:]]*"stokesforge-/ {
            printf "%s : renamed internal dependency (package = \"stokesforge-…\"), unreadable form : %s\n", crate, $0
            next
        }
        dep_sect && /^[[:space:]]*stokesforge-[A-Za-z0-9_-]+[[:space:]]*=/ {
            dep = $0
            sub(/^[[:space:]]*/, "", dep)
            sub(/[[:space:]]*=.*$/, "", dep)
            if ($0 ~ /version[[:space:]]*=[[:space:]]*"/) {
                v = $0
                sub(/^.*version[[:space:]]*=[[:space:]]*"/, "", v)
                sub(/".*$/, "", v)
            } else if ($0 ~ /^[[:space:]]*stokesforge-[A-Za-z0-9_-]+[[:space:]]*=[[:space:]]*"/) {
                v = $0
                sub(/^[^=]*=[[:space:]]*"/, "", v)
                sub(/".*$/, "", v)
            } else {
                next   # path only
            }
            if (v != ref) {
                printf "%s: dependency %s is %s, expected %s\n", crate, dep, v, ref
            }
        }' "$manifest")"
    if [ -n "$dep_errors" ]; then
        while IFS= read -r line; do
            echo "[versions] ERROR: $line" >&2
        done <<< "$dep_errors"
        bad=1
    fi
done

# .stf format: FORMAT_VERSION = N requires the witness tests/witnesses/vN/project.stf
# (invariant 12: a witness is never modified, a new format version brings
# one). Git does not track an empty folder: the file is required.
SCHEMA="$REPO_ROOT/crates/stokesforge-persistence/src/schema.rs"
fv_lines=""
[ ! -e "$SCHEMA" ] || fv_lines="$(grep -E '^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?const[[:space:]]+FORMAT_VERSION[[:space:]]*:[[:space:]]*[a-z0-9]+[[:space:]]*=[[:space:]]*[0-9]+[[:space:]]*;' "$SCHEMA" || true)"
if [ "$(printf '%s' "$fv_lines" | grep -c .)" != 1 ]; then
    echo "[versions] ERROR: FORMAT_VERSION not found (or found several times) in $SCHEMA" >&2
    bad=1
else
    fv="$(printf '%s' "$fv_lines" | sed 's/^.*=[[:space:]]*\([0-9][0-9]*\)[[:space:]]*;.*$/\1/')"
    witness="$REPO_ROOT/crates/stokesforge-persistence/tests/witnesses/v$fv/project.stf"
    if [ ! -f "$witness" ]; then
        echo "[versions] ERROR: FORMAT_VERSION = $fv without witness $witness (invariant 12, docs/persistence.md)" >&2
        bad=1
    fi
fi

# Cargo.lock: the entry of each workspace crate has the reference version.
# A forgotten `cargo update --workspace` after a version change (rule #333)
# leaves the old version in the lock file (#351), without cargo.
LOCK="$REPO_ROOT/Cargo.lock"
if [ ! -f "$LOCK" ]; then
    echo "[versions] ERROR: Cargo.lock missing ($LOCK), it is tracked for reproducible builds" >&2
    bad=1
else
    for name in "${names[@]}"; do
        locked="$(awk -v n="$name" '
            $0 == "name = \"" n "\"" { want = 1; next }
            want && /^version = "/ { v = $0; sub(/^version = "/, "", v); sub(/".*$/, "", v); print v; exit }
            /^\[\[package\]\]/ { want = 0 }' "$LOCK")"
        if [ -z "$locked" ]; then
            echo "[versions] ERROR: $name missing from Cargo.lock: run \"cargo update --workspace\"" >&2
            bad=1
        elif [ "$locked" != "$reference" ]; then
            echo "[versions] ERROR: Cargo.lock has $name at $locked, expected $reference: run \"cargo update --workspace\"" >&2
            bad=1
        fi
    done
fi

[ "$bad" = 0 ] || exit 1
echo "[versions] ${#names[@]} crates in line at $reference"

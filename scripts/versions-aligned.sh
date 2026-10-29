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
# The pyst API version (PYST_API_VERSION "X.Y" in stokesforge-script/src/surface.rs,
# #356) requires the frozen snapshot crates/stokesforge-script/tests/api-witnesses/vX.Y.txt
# (the `pyst-api` blocks of docs/python-and-scripting.md, one line
# `<kind> <name><TAB><entry>`, sorted LC_ALL=C; print it with
# `scripts/versions-aligned.sh --pyst-snapshot`). The doc must equal this snapshot,
# the version must follow the previous snapshot (0.x: next minor, surface must change;
# from 1.0: same major = minor+1 and pure addition, major+1 = minor 0 and a change),
# the API major may not exceed the crates' major, and a snapshot present in a tag
# `v*` must be unchanged in the working tree (skipped, with a note, without tags).
# Called by the `fmt` job (GitHub and GitLab) and by scripts/release-notes.sh.
# STOKESFORGE_REPO_ROOT (tests) replaces the repository root.
set -euo pipefail

REPO_ROOT="${STOKESFORGE_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

# The `pyst-api` blocks of the doc as `<kind> <name><TAB><entry>` lines, in the
# order of the doc. Same reading as crates/stokesforge-script/tests/frozen_api.rs
# (header "```pyst-api kind name", trim_end, blank lines skipped, closing "```").
# Code 3 and a message on stderr if a block is malformed or not closed.
pyst_doc_lines() { # pyst_doc_lines <doc>
    awk '
        { sub(/\r$/, "") }
        open {
            l = $0; sub(/[[:space:]]+$/, "", l)
            if (l == "```") { open = 0; next }
            if (l ~ /^[[:space:]]*$/) next
            printf "%s %s\t%s\n", kind, name, l
            next
        }
        index($0, "```pyst-api ") == 1 {
            n = split(substr($0, 13), w, " ")
            if (n != 2) { printf "bad block header: %s\n", $0 > "/dev/stderr"; bad = 1; next }
            kind = w[1]; name = w[2]; open = 1
        }
        END {
            if (open) { print "pyst-api block not closed" > "/dev/stderr"; exit 3 }
            if (bad) exit 3
        }' "$1"
}

PYST_DOC="$REPO_ROOT/docs/python-and-scripting.md"
if [ "${1:-}" = "--pyst-snapshot" ]; then
    pyst_doc_lines "$PYST_DOC" | LC_ALL=C sort
    exit 0
fi
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

# pyst API version (#356): constant, frozen snapshot, bump rules, tags.
PYST_SURFACE="$REPO_ROOT/crates/stokesforge-script/src/surface.rs"
PYST_DIR="$REPO_ROOT/crates/stokesforge-script/tests/api-witnesses"
PYST_REL="crates/stokesforge-script/tests/api-witnesses"
pv_lines=""
[ ! -e "$PYST_SURFACE" ] || pv_lines="$(grep -E '^[[:space:]]*pub(\([a-z]+\))?[[:space:]]+const[[:space:]]+PYST_API_VERSION[[:space:]]*:[[:space:]]*&(.static)?[[:space:]]*str[[:space:]]*=[[:space:]]*"[^"]*"[[:space:]]*;' "$PYST_SURFACE" || true)"
if [ "$(printf '%s' "$pv_lines" | grep -c .)" != 1 ]; then
    echo "[versions] ERROR: pyst API: PYST_API_VERSION not found (or found several times) in $PYST_SURFACE" >&2
    bad=1
else
    pv="$(printf '%s' "$pv_lines" | sed 's/^[^"]*"\([^"]*\)".*$/\1/')"
    if ! printf '%s' "$pv" | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
        echo "[versions] ERROR: pyst API: PYST_API_VERSION \"$pv\" is not of the form \"MAJOR.MINOR\"" >&2
        bad=1
    else
        pv_major="${pv%%.*}"
        pv_minor="${pv##*.}"
        snap="$PYST_DIR/v$pv.txt"
        doc_err=0
        doc_errfile="$(mktemp)"
        doc_now="$(pyst_doc_lines "$PYST_DOC" 2>"$doc_errfile" | LC_ALL=C sort)" || doc_err=1
        if [ -s "$doc_errfile" ] || [ "$doc_err" = 1 ]; then
            echo "[versions] ERROR: pyst API: $(cat "$doc_errfile") ($PYST_DOC)" >&2
            doc_err=1
            bad=1
        fi
        rm -f "$doc_errfile"
        if [ ! -f "$snap" ]; then
            echo "[versions] ERROR: pyst API $pv without snapshot $snap (create it: scripts/versions-aligned.sh --pyst-snapshot > …)" >&2
            bad=1
        elif [ "$doc_err" = 0 ]; then
            snap_now="$(LC_ALL=C sort "$snap")"
            if [ "$doc_now" != "$snap_now" ]; then
                echo "[versions] ERROR: pyst API $pv: the doc differs from the frozen snapshot v$pv.txt; a surface change needs a new API version and a new snapshot (+ doc only, - snapshot only):" >&2
                diff <(printf '%s\n' "$snap_now") <(printf '%s\n' "$doc_now") | sed -n 's/^< /  - /p; s/^> /  + /p' >&2 || true
                bad=1
            else
                # Previous snapshot (numeric order) and the bump rules.
                prev=""
                newer=""
                for f in "$PYST_DIR"/v*.txt; do
                    [ -e "$f" ] || continue
                    v="$(basename "$f" .txt)"; v="${v#v}"
                    printf '%s' "$v" | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' || continue
                    [ "$v" != "$pv" ] || continue
                    vM="${v%%.*}"; vm="${v##*.}"
                    if [ "$vM" -gt "$pv_major" ] || { [ "$vM" = "$pv_major" ] && [ "$vm" -gt "$pv_minor" ]; }; then
                        newer="$v"
                    elif [ -z "$prev" ] || [ "$vM" -gt "${prev%%.*}" ] || { [ "$vM" = "${prev%%.*}" ] && [ "$vm" -gt "${prev##*.}" ]; }; then
                        prev="$v"
                    fi
                done
                if [ -n "$newer" ]; then
                    echo "[versions] ERROR: pyst API: snapshot v$newer.txt is newer than PYST_API_VERSION $pv" >&2
                    bad=1
                elif [ -n "$prev" ]; then
                    pM="${prev%%.*}"; pm="${prev##*.}"
                    old_now="$(LC_ALL=C sort "$PYST_DIR/v$prev.txt")"
                    same=0
                    [ "$old_now" != "$snap_now" ] || same=1
                    why=""
                    if [ "$pM" = 0 ] && [ "$pv_major" = 0 ]; then
                        # alpha: any change, next minor.
                        [ "$pv_minor" = "$((pm + 1))" ] || why="alpha (0.x): expected 0.$((pm + 1)) after $prev, not $pv"
                        [ -n "$why" ] || [ "$same" = 0 ] || why="version bump $prev -> $pv without surface change"
                    elif [ "$pM" = 0 ] && [ "$pv" = "1.0" ]; then
                        :   # stabilization, the surface may be unchanged.
                    elif [ "$pM" = 0 ]; then
                        why="after the alpha $prev the next version is 0.$((pm + 1)) or 1.0 (stabilization), not $pv"
                    elif [ "$pv_major" = "$pM" ]; then
                        if [ "$pv_minor" != "$((pm + 1))" ]; then
                            why="expected $pM.$((pm + 1)) after $prev, not $pv"
                        elif [ "$same" = 1 ]; then
                            why="version bump $prev -> $pv without surface change"
                        elif [ -n "$(comm -23 <(printf '%s\n' "$old_now") <(printf '%s\n' "$snap_now"))" ]; then
                            why="a line was modified or removed since $prev: that needs a new major ($((pM + 1)).0), not $pv"
                        fi
                    elif [ "$pv_major" = "$((pM + 1))" ]; then
                        if [ "$pv_minor" != 0 ]; then
                            why="a new major starts at minor 0: expected $pv_major.0, not $pv"
                        elif [ "$same" = 1 ]; then
                            why="major bump $prev -> $pv without surface change"
                        fi
                    else
                        why="$pv does not follow $prev"
                    fi
                    if [ -n "$why" ]; then
                        echo "[versions] ERROR: pyst API $pv: $why" >&2
                        bad=1
                    fi
                fi
            fi
        fi
        # The API major may not exceed the software major.
        sw_major="${reference%%.*}"
        if [ -n "$sw_major" ] && [ "$pv_major" -gt "$sw_major" ] 2>/dev/null; then
            echo "[versions] ERROR: pyst API $pv: API major $pv_major > software major $sw_major ($reference)" >&2
            bad=1
        fi
    fi
fi

# Released snapshots are frozen: the one of a tag `v*` must be unchanged.
if [ -e "$REPO_ROOT/.git" ] && command -v git >/dev/null 2>&1; then
    tags="$(git -C "$REPO_ROOT" tag -l 'v*' 2>/dev/null || true)"
else
    tags=""
fi
if [ -z "$tags" ]; then
    echo "[versions] note: no git tag v* here, frozen snapshots of released API versions not checked" >&2
else
    while IFS= read -r tag; do
        files="$(git -C "$REPO_ROOT" ls-tree -r --name-only "$tag" -- "$PYST_REL/" 2>/dev/null | grep -E "^$PYST_REL/v[0-9]+\.[0-9]+\.txt\$" || true)"
        [ -n "$files" ] || continue
        while IFS= read -r f; do
            if [ ! -f "$REPO_ROOT/$f" ]; then
                echo "[versions] ERROR: pyst API: $f is in tag $tag but missing here (a released snapshot is frozen)" >&2
                bad=1
            elif ! git -C "$REPO_ROOT" show "$tag:$f" | cmp -s - "$REPO_ROOT/$f"; then
                echo "[versions] ERROR: pyst API: $f differs from its content at tag $tag (a released snapshot is frozen: a surface change brings a new version and a new snapshot)" >&2
                bad=1
            fi
        done <<< "$files"
    done <<< "$tags"
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

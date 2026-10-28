#!/usr/bin/env bash
# Test of scripts/release-notes.sh and scripts/versions-alignees.sh on a fake repository (#333): no network,
# the real repository is not touched. Usage: scripts/release-notes-test.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/crates/a" "$tmp/crates/b"
printf '[package]\nname = "a"\nversion = "1.2.3"\n' > "$tmp/crates/a/Cargo.toml"
printf '[package]\nname = "b"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
lock() { # lock <version of a> <version of b>
    printf '[[package]]\nname = "a"\nversion = "%s"\n\n[[package]]\nname = "b"\nversion = "%s"\n' "$1" "$2" > "$tmp/Cargo.lock"
}
lock 1.2.3 1.2.3
pers="$tmp/crates/stokesforge-persistence"
mkdir -p "$pers/src" "$pers/tests/temoins/v1"
printf 'pub(crate) const FORMAT_VERSION: u32 = 1;\n' > "$pers/src/schema.rs"
: > "$pers/tests/temoins/v1/projet.stf"

fail=0
attend() { # attend <code> <description> <tag>
    local code="$1" desc="$2" tag="$3" got=0
    STOKESFORGE_REPO_ROOT="$tmp" "$HERE/release-notes.sh" "$tag" >/dev/null 2>&1 || got=$?
    if [ "$got" != "$code" ]; then
        echo "FAILED: $desc (code $got, expected $code)" >&2
        fail=1
    fi
}

aligne() { # aligne <code> <description> [version]
    local code="$1" desc="$2" got=0
    shift 2
    STOKESFORGE_REPO_ROOT="$tmp" "$HERE/versions-alignees.sh" "$@" >/dev/null 2>"$tmp/err" || got=$?
    if [ "$got" != "$code" ]; then
        echo "FAILED: $desc (code $got, expected $code)" >&2
        fail=1
    fi
}

printf '## [Unreleased]\n\n## [1.2.3] - 2026-01-01\n\n- A line.\n' > "$tmp/CHANGELOG.md"
attend 0 "dated section, tag without -rc" v1.2.3
attend 1 "crate version differs" v1.2.4
attend 1 "invalid tag form" v1.2
attend 0 "-rc, correct versions" v1.2.3-rc1

printf '## [1.2.3] - undated\n\n- A line.\n' > "$tmp/CHANGELOG.md"
attend 1 "\"undated\" refused without -rc" v1.2.3
attend 0 "\"undated\" tolerated with -rc" v1.2.3-rc1

printf '## [1.2.3] - 2026-01-01\n\n## [1.2.2] - 2025-12-01\n' > "$tmp/CHANGELOG.md"
attend 1 "empty section refused" v1.2.3

# b out of line: refused with and without -rc.
printf '## [1.2.3] - 2026-01-01\n\n- A line.\n' > "$tmp/CHANGELOG.md"
printf '[package]\nname = "b"\nversion = "1.2.4"\n' > "$tmp/crates/b/Cargo.toml"
attend 1 "crate b out of line" v1.2.3
attend 1 "crate b out of line, -rc" v1.2.3-rc1
aligne 1 "b out of line, no argument"
printf '[package]\nname = "b"\nversion.workspace = true\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "b without version line"
grep -q "version.workspace" "$tmp/err" || { echo "FAILED: message \"version.workspace\" missing" >&2; fail=1; }
printf '[package]\nname = "b"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "b without version"
grep -q "no line" "$tmp/err" || { echo "FAILED: message \"no version line\" missing" >&2; fail=1; }
printf '[package]\nname = "b"\n[dependencies.x]\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "version outside [package] ignored"
printf '[package]\nname = "b"\nedition = "2021"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "version after edition in [package]"
printf '[package]\nname = "b"\nversion = "1.2.3"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "two version lines"
grep -q "several" "$tmp/err" || { echo "FAILED: message \"several\" missing" >&2; fail=1; }
printf '[package]\nname = "b"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"

# Internal dependencies (#353).
B_OK='[package]\nname = "b"\nversion = "1.2.3"\n'
printf "$B_OK"'[dependencies]\na = { version = "1.2.2", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "non-internal dependency ignored"
printf "$B_OK"'[dependencies]\nstokesforge-a = { version = "1.2.2", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "internal dependency out of line"
grep -q "b: dependency stokesforge-a is 1.2.2" "$tmp/err" || { echo "FAILED: out-of-line dependency message missing" >&2; fail=1; }
printf "$B_OK"'[target.'"'"'cfg(unix)'"'"'.dev-dependencies]\nstokesforge-a = { version = "1.2.2", path = "../a", optional = true }\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "internal dependency out of line in target dev-dependencies"
printf "$B_OK"'[dependencies]\nstokesforge-a = { version = "1.2.3", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "internal dependency in line"
printf "$B_OK"'[dependencies]\nstokesforge-a = { path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "internal dependency by path only"
printf "$B_OK"'[dependencies.stokesforge-a]\nversion = "1.2.3"\npath = "../a"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "internal dependency as table refused"
grep -q "unreadable form" "$tmp/err" || { echo "FAILED: message \"unreadable form\" missing" >&2; fail=1; }
printf "$B_OK"'[dependencies]\nx = { package = "stokesforge-a", version = "1.2.3", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "renamed internal dependency refused"
printf "$B_OK" > "$tmp/crates/b/Cargo.toml"

# Format version and witness (#353).
printf 'pub const FORMAT_VERSION: u32 = 2;\n' > "$pers/src/schema.rs"
aligne 1 "FORMAT_VERSION = 2 without v2 witness"
grep -q "FORMAT_VERSION = 2" "$tmp/err" || { echo "FAILED: witness message missing" >&2; fail=1; }
mkdir "$pers/tests/temoins/v2"
aligne 1 "empty v2 folder, no projet.stf"
: > "$pers/tests/temoins/v2/projet.stf"
aligne 0 "FORMAT_VERSION = 2 with v2 witness"
printf '// rien\n' > "$pers/src/schema.rs"
aligne 1 "FORMAT_VERSION not found"
grep -q "FORMAT_VERSION not found" "$tmp/err" || { echo "FAILED: message \"not found\" missing" >&2; fail=1; }
printf 'pub(crate) const FORMAT_VERSION: u32 = 1;\n' > "$pers/src/schema.rs"

# Cargo.lock (#351).
lock 1.2.3 1.2.2
aligne 1 "Cargo.lock behind for b"
grep -q "cargo update --workspace" "$tmp/err" || { echo "FAILED: message \"cargo update\" missing" >&2; fail=1; }
lock 1.2.3 1.2.3
rm "$tmp/Cargo.lock"
aligne 1 "Cargo.lock missing"
grep -q "Cargo.lock missing" "$tmp/err" || { echo "FAILED: message \"Cargo.lock missing\" missing" >&2; fail=1; }
lock 1.2.3 1.2.3

# versions-alignees.sh alone: neither tag nor CHANGELOG read.
aligne 0 "crates in line, no argument"
aligne 0 "crates in line, right argument" 1.2.3
aligne 1 "argument differs from crates" 1.2.4
rm "$tmp/CHANGELOG.md"
aligne 0 "missing CHANGELOG has no effect"
printf '## [1.2.3] - undated\n' > "$tmp/CHANGELOG.md"
aligne 0 "\"undated\" has no effect"
printf '## [1.2.2] - 2025-12-01\n\n- A line.\n' > "$tmp/CHANGELOG.md"
attend 1 "tag section missing" v1.2.3

# No crate: refused with a message.
mv "$tmp/crates" "$tmp/crates.off"
mkdir "$tmp/crates"
aligne 1 "no crate"
grep -q "no crate found" "$tmp/err" || { echo "FAILED: message \"no crate found\" missing" >&2; fail=1; }
rmdir "$tmp/crates"
mv "$tmp/crates.off" "$tmp/crates"

# Argument 1.2.3 while the crates are at 1.2.4.
printf '[package]\nname = "a"\nversion = "1.2.4"\n' > "$tmp/crates/a/Cargo.toml"
printf '[package]\nname = "b"\nversion = "1.2.4"\n' > "$tmp/crates/b/Cargo.toml"
lock 1.2.4 1.2.4
aligne 1 "crates at 1.2.4, argument 1.2.3" 1.2.3
aligne 0 "crates at 1.2.4 in line" 

[ "$fail" = 0 ] && echo "release-notes-test : ok"
exit "$fail"

#!/usr/bin/env bash
# Test of scripts/release-notes.sh and scripts/versions-aligned.sh on a fake repository (#333): no network,
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
mkdir -p "$pers/src" "$pers/tests/witnesses/v1"
printf 'pub(crate) const FORMAT_VERSION: u32 = 1;\n' > "$pers/src/schema.rs"
: > "$pers/tests/witnesses/v1/project.stf"

# pyst API (#356): constant, doc with two blocks, frozen snapshot.
surf="$tmp/crates/stokesforge-script/src/surface.rs"
apid="$tmp/crates/stokesforge-script/tests/api-witnesses"
mkdir -p "$tmp/crates/stokesforge-script/src" "$apid" "$tmp/docs"
setver() { printf 'pub const PYST_API_VERSION: &str = "%s";\n' "$1" > "$surf"; }
writedoc() { # writedoc <entries of the block "module pyst">
    printf 'Intro\n\n```pyst-api module pyst\n%s\n```\n\n```pyst-api returns info\nkind\nname\n```\n' "$1" > "$tmp/docs/python-and-scripting.md"
}
snap() { # snap <version>: freezes the doc as it is, with the script's own reader
    STOKESFORGE_REPO_ROOT="$tmp" "$HERE/versions-aligned.sh" --pyst-snapshot > "$apid/v$1.txt"
}
setcrates() { # setcrates <X.Y.Z>
    printf '[package]\nname = "a"\nversion = "%s"\n' "$1" > "$tmp/crates/a/Cargo.toml"
    printf '[package]\nname = "b"\nversion = "%s"\n' "$1" > "$tmp/crates/b/Cargo.toml"
    lock "$1" "$1"
}
BASE_ENTRIES=$'function a(x)\nfunction b()'
reset_api() { # 0.1 baseline, snapshot written by hand (not by the script)
    rm -rf "$apid" "$tmp/.git"
    mkdir -p "$apid"
    setver 0.1
    writedoc "$BASE_ENTRIES"
    printf 'module pyst\tfunction a(x)\nmodule pyst\tfunction b()\nreturns info\tkind\nreturns info\tname\n' > "$apid/v0.1.txt"
}
reset_api

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
    STOKESFORGE_REPO_ROOT="$tmp" "$HERE/versions-aligned.sh" "$@" >/dev/null 2>"$tmp/err" || got=$?
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
mkdir "$pers/tests/witnesses/v2"
aligne 1 "empty v2 folder, no project.stf"
: > "$pers/tests/witnesses/v2/project.stf"
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

# versions-aligned.sh alone: neither tag nor CHANGELOG read.
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


# pyst API version (#356).
reset_api
aligne 0 "pyst API 0.1, doc = snapshot"
STOKESFORGE_REPO_ROOT="$tmp" "$HERE/versions-aligned.sh" --pyst-snapshot | cmp -s - "$apid/v0.1.txt" || { echo "FAILED: --pyst-snapshot differs from the hand-written snapshot" >&2; fail=1; }
writedoc "$BASE_ENTRIES"$'\nfunction c()'
aligne 1 "pyst API: line added without bump"
grep -q "pyst API" "$tmp/err" && grep -q "function c()" "$tmp/err" || { echo "FAILED: pyst API message (line) missing" >&2; fail=1; }
setver 0.2; snap 0.2
aligne 0 "pyst API 0.2 with an added line"
writedoc 'function a(x)'; snap 0.2
aligne 0 "pyst API 0.2 with a removed line (alpha)"
reset_api; setver 0.2; snap 0.2
aligne 1 "pyst API 0.2 identical to 0.1"
grep -q "without surface change" "$tmp/err" || { echo "FAILED: empty bump message missing" >&2; fail=1; }
reset_api; setver 0.2
aligne 1 "pyst API 0.2 without snapshot"
grep -q "without snapshot" "$tmp/err" || { echo "FAILED: snapshot message missing" >&2; fail=1; }
reset_api; writedoc "$BASE_ENTRIES"$'\nfunction c()'; setver 0.3; snap 0.3
aligne 1 "pyst API skipped version 0.1 -> 0.3"
reset_api; writedoc "$BASE_ENTRIES"$'\nfunction c()'; setver 1.1; snap 1.1
aligne 1 "pyst API 0.1 -> 1.1"
reset_api; setver 1.0; snap 1.0; rm "$apid/v0.1.txt"
aligne 0 "pyst API 1.0 alone"
reset_api; setver 1.0; snap 1.0
aligne 0 "pyst API 0.1 -> 1.0 stabilization, surface unchanged"
# From 1.0.
reset_api; rm "$apid/v0.1.txt"; setver 1.0; snap 1.0
writedoc "$BASE_ENTRIES"$'\nfunction c()'; setver 1.1; snap 1.1
aligne 0 "pyst API 1.0 -> 1.1 pure addition"
setver 1.2; snap 1.2; rm "$apid/v1.1.txt"
aligne 1 "pyst API 1.0 -> 1.2 skipped version"
writedoc "$BASE_ENTRIES"; setver 1.1; snap 1.1; rm -f "$apid/v1.2.txt"
aligne 1 "pyst API 1.1 identical to 1.0"
writedoc 'function a(x)'; snap 1.1
aligne 1 "pyst API 1.1 dropping a line"
grep -q "major" "$tmp/err" || { echo "FAILED: \"major\" message missing" >&2; fail=1; }
writedoc $'function a(x, y=1)\nfunction b()'; snap 1.1
aligne 1 "pyst API 1.1 with a changed signature"
rm "$apid/v1.1.txt"
setcrates 2.0.0
writedoc 'function a(x)'; setver 2.0; snap 2.0
aligne 0 "pyst API 2.0 with a removal, software 2.0.0"
setver 2.1; snap 2.1; rm "$apid/v2.0.txt"
writedoc 'function a(x)'; setver 2.0; snap 2.0; rm "$apid/v2.1.txt"
writedoc "$BASE_ENTRIES"$'\nfunction c()'; setver 2.1; snap 2.1; rm "$apid/v2.0.txt"
aligne 1 "pyst API 2.1 after 1.0, minor not reset"
setcrates 1.2.3
writedoc 'function a(x)'; setver 2.0; snap 2.0; rm -f "$apid/v2.1.txt"
aligne 1 "pyst API 2.0 with software 1.2.3"
grep -q "software major" "$tmp/err" || { echo "FAILED: software major message missing" >&2; fail=1; }
reset_api; rm "$apid/v0.1.txt"; writedoc "$BASE_ENTRIES"; setver 1.9; snap 1.9
writedoc "$BASE_ENTRIES"$'\nfunction c()'; setver 1.10; snap 1.10
aligne 0 "pyst API 1.9 -> 1.10 (numeric order)"
reset_api; rm "$apid/v0.1.txt"; setver 1.11
printf 'module pyst\tfunction a(x)\nmodule pyst\tfunction b()\nreturns info\tkind\nreturns info\tname\n' > "$apid/v1.2.txt"
printf 'module pyst\tfunction a(x)\nmodule pyst\tfunction b()\nmodule pyst\tfunction c()\nreturns info\tkind\nreturns info\tname\n' > "$apid/v1.10.txt"
writedoc "$BASE_ENTRIES"$'\nfunction c()\nfunction d()'; snap 1.11
aligne 0 "pyst API 1.11 follows 1.10, not 1.2 (numeric order)"
# Constant.
reset_api; : > "$surf"
aligne 1 "PYST_API_VERSION missing"
setver 0.1; setver 0.1 >> "$surf"; cat "$surf" "$surf" > "$surf.x"; mv "$surf.x" "$surf"
aligne 1 "PYST_API_VERSION twice"
cp "$apid/v0.1.txt" "$apid/v1.txt"; cp "$apid/v0.1.txt" "$apid/v1.0.0.txt"
setver 1
aligne 1 "PYST_API_VERSION \"1\""
grep -q "MAJOR.MINOR" "$tmp/err" || { echo "FAILED: form message missing for 1" >&2; fail=1; }
setver 1.0.0
aligne 1 "PYST_API_VERSION \"1.0.0\""
grep -q "MAJOR.MINOR" "$tmp/err" || { echo "FAILED: form message missing for 1.0.0" >&2; fail=1; }
# Reading the doc.
reset_api
printf '%s\n' '```pyst-api module x' >> "$tmp/docs/python-and-scripting.md"
aligne 1 "pyst-api block not closed"
reset_api
printf 'Intro\n\n```pyst-api module pyst\n\nfunction b()   \n\n  \nfunction a(x)\n```   \n\n```pyst-api returns info\nname\nkind\n```\n' > "$tmp/docs/python-and-scripting.md"
aligne 0 "blank lines, trailing spaces, other order"
printf 'Intro\r\n\r\n```pyst-api module pyst\r\nfunction a(x)\r\nfunction b()\r\n```\r\n\r\n```pyst-api returns info\r\nkind\r\nname\r\n```\r\n' > "$tmp/docs/python-and-scripting.md"
aligne 0 "CRLF doc"
# Tags: the snapshot of a released version is frozen.
reset_api
aligne 0 "no tag: passes"
grep -q "no git tag" "$tmp/err" || { echo "FAILED: no-tag note missing" >&2; fail=1; }
git -C "$tmp" init -q
git -C "$tmp" add -A
git -C "$tmp" -c user.name=t -c user.email=t@t commit -q -m base
git -C "$tmp" tag v1.0.1
aligne 0 "tag v1.0.1, snapshot unchanged"
writedoc "$BASE_ENTRIES"$'\nfunction c()'
printf 'module pyst\tfunction c()\n' >> "$apid/v0.1.txt"
aligne 1 "tag v1.0.1, snapshot v0.1 modified with the doc"
grep -q "tag v1.0.1" "$tmp/err" || { echo "FAILED: tag message missing" >&2; fail=1; }
rm "$apid/v0.1.txt"
aligne 1 "tag v1.0.1, snapshot v0.1 deleted"
rm -rf "$tmp/.git"
reset_api
setcrates 1.2.3

# Argument 1.2.3 while the crates are at 1.2.4.
printf '[package]\nname = "a"\nversion = "1.2.4"\n' > "$tmp/crates/a/Cargo.toml"
printf '[package]\nname = "b"\nversion = "1.2.4"\n' > "$tmp/crates/b/Cargo.toml"
lock 1.2.4 1.2.4
aligne 1 "crates at 1.2.4, argument 1.2.3" 1.2.3
aligne 0 "crates at 1.2.4 in line" 

[ "$fail" = 0 ] && echo "release-notes-test : ok"
exit "$fail"

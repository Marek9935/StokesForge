#!/usr/bin/env bash
# Essai de scripts/release-notes.sh et scripts/versions-alignees.sh sur un dépôt factice (#333) : sans réseau,
# sans toucher au vrai dépôt. Usage : scripts/release-notes-test.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/crates/a" "$tmp/crates/b"
printf '[package]\nname = "a"\nversion = "1.2.3"\n' > "$tmp/crates/a/Cargo.toml"
printf '[package]\nname = "b"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
lock() { # lock <version de a> <version de b>
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
        echo "ECHEC : $desc (code $got, attendu $code)" >&2
        fail=1
    fi
}

aligne() { # aligne <code> <description> [version]
    local code="$1" desc="$2" got=0
    shift 2
    STOKESFORGE_REPO_ROOT="$tmp" "$HERE/versions-alignees.sh" "$@" >/dev/null 2>"$tmp/err" || got=$?
    if [ "$got" != "$code" ]; then
        echo "ECHEC : $desc (code $got, attendu $code)" >&2
        fail=1
    fi
}

printf '## [Non publié]\n\n## [1.2.3] - 2026-01-01\n\n- Une ligne.\n' > "$tmp/CHANGELOG.md"
attend 0 "section datée, tag sans -rc" v1.2.3
attend 1 "version des crates différente" v1.2.4
attend 1 "forme de tag invalide" v1.2
attend 0 "-rc, versions bonnes" v1.2.3-rc1

printf '## [1.2.3] - à dater\n\n- Une ligne.\n' > "$tmp/CHANGELOG.md"
attend 1 "« à dater » refusé sans -rc" v1.2.3
attend 0 "« à dater » toléré avec -rc" v1.2.3-rc1

printf '## [1.2.3] - 2026-01-01\n\n## [1.2.2] - 2025-12-01\n' > "$tmp/CHANGELOG.md"
attend 1 "section vide refusée" v1.2.3

# b désaligné : refusé avec et sans -rc.
printf '## [1.2.3] - 2026-01-01\n\n- Une ligne.\n' > "$tmp/CHANGELOG.md"
printf '[package]\nname = "b"\nversion = "1.2.4"\n' > "$tmp/crates/b/Cargo.toml"
attend 1 "crate b désaligné" v1.2.3
attend 1 "crate b désaligné, -rc" v1.2.3-rc1
aligne 1 "b désaligné, sans argument"
printf '[package]\nname = "b"\nversion.workspace = true\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "b sans ligne version"
grep -q "version.workspace" "$tmp/err" || { echo "ECHEC : message « version.workspace » absent" >&2; fail=1; }
printf '[package]\nname = "b"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "b sans version"
grep -q "pas de ligne" "$tmp/err" || { echo "ECHEC : message « pas de ligne version » absent" >&2; fail=1; }
printf '[package]\nname = "b"\n[dependencies.x]\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "version hors [package] ignorée"
printf '[package]\nname = "b"\nedition = "2021"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "version après edition dans [package]"
printf '[package]\nname = "b"\nversion = "1.2.3"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "deux lignes version"
grep -q "plusieurs" "$tmp/err" || { echo "ECHEC : message « plusieurs » absent" >&2; fail=1; }
printf '[package]\nname = "b"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"

# Dépendances internes (#353).
B_OK='[package]\nname = "b"\nversion = "1.2.3"\n'
printf "$B_OK"'[dependencies]\na = { version = "1.2.2", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "dépendance non interne ignorée"
printf "$B_OK"'[dependencies]\nstokesforge-a = { version = "1.2.2", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "dépendance interne désalignée"
grep -q "b : dépendance stokesforge-a en 1.2.2" "$tmp/err" || { echo "ECHEC : message de dépendance désalignée absent" >&2; fail=1; }
printf "$B_OK"'[target.'"'"'cfg(unix)'"'"'.dev-dependencies]\nstokesforge-a = { version = "1.2.2", path = "../a", optional = true }\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "dépendance interne désalignée en target dev-dependencies"
printf "$B_OK"'[dependencies]\nstokesforge-a = { version = "1.2.3", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "dépendance interne alignée"
printf "$B_OK"'[dependencies]\nstokesforge-a = { path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 0 "dépendance interne par chemin seul"
printf "$B_OK"'[dependencies.stokesforge-a]\nversion = "1.2.3"\npath = "../a"\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "dépendance interne en table refusée"
grep -q "forme non lue" "$tmp/err" || { echo "ECHEC : message « forme non lue » absent" >&2; fail=1; }
printf "$B_OK"'[dependencies]\nx = { package = "stokesforge-a", version = "1.2.3", path = "../a" }\n' > "$tmp/crates/b/Cargo.toml"
aligne 1 "dépendance interne renommée refusée"
printf "$B_OK" > "$tmp/crates/b/Cargo.toml"

# Version de format et témoin (#353).
printf 'pub const FORMAT_VERSION: u32 = 2;\n' > "$pers/src/schema.rs"
aligne 1 "FORMAT_VERSION = 2 sans témoin v2"
grep -q "FORMAT_VERSION = 2" "$tmp/err" || { echo "ECHEC : message de témoin absent" >&2; fail=1; }
mkdir "$pers/tests/temoins/v2"
aligne 1 "dossier v2 vide, sans projet.stf"
: > "$pers/tests/temoins/v2/projet.stf"
aligne 0 "FORMAT_VERSION = 2 avec témoin v2"
printf '// rien\n' > "$pers/src/schema.rs"
aligne 1 "FORMAT_VERSION introuvable"
grep -q "FORMAT_VERSION introuvable" "$tmp/err" || { echo "ECHEC : message « introuvable » absent" >&2; fail=1; }
printf 'pub(crate) const FORMAT_VERSION: u32 = 1;\n' > "$pers/src/schema.rs"

# Cargo.lock (#351).
lock 1.2.3 1.2.2
aligne 1 "Cargo.lock en retard pour b"
grep -q "cargo update --workspace" "$tmp/err" || { echo "ECHEC : message « cargo update » absent" >&2; fail=1; }
lock 1.2.3 1.2.3
rm "$tmp/Cargo.lock"
aligne 1 "Cargo.lock absent"
grep -q "Cargo.lock absent" "$tmp/err" || { echo "ECHEC : message « Cargo.lock absent » absent" >&2; fail=1; }
lock 1.2.3 1.2.3

# versions-alignees.sh seul : ni tag ni CHANGELOG lus.
aligne 0 "crates alignés, sans argument"
aligne 0 "crates alignés, bon argument" 1.2.3
aligne 1 "argument différent des crates" 1.2.4
rm "$tmp/CHANGELOG.md"
aligne 0 "CHANGELOG absent sans effet"
printf '## [1.2.3] - à dater\n' > "$tmp/CHANGELOG.md"
aligne 0 "« à dater » sans effet"
printf '## [1.2.2] - 2025-12-01\n\n- Une ligne.\n' > "$tmp/CHANGELOG.md"
attend 1 "section du tag absente" v1.2.3

# Aucun crate : refusé avec un message.
mv "$tmp/crates" "$tmp/crates.off"
mkdir "$tmp/crates"
aligne 1 "aucun crate"
grep -q "aucun crate" "$tmp/err" || { echo "ECHEC : message « aucun crate » absent" >&2; fail=1; }
rmdir "$tmp/crates"
mv "$tmp/crates.off" "$tmp/crates"

# Argument 1.2.3 alors que les crates sont en 1.2.4.
printf '[package]\nname = "a"\nversion = "1.2.4"\n' > "$tmp/crates/a/Cargo.toml"
printf '[package]\nname = "b"\nversion = "1.2.4"\n' > "$tmp/crates/b/Cargo.toml"
lock 1.2.4 1.2.4
aligne 1 "crates en 1.2.4, argument 1.2.3" 1.2.3
aligne 0 "crates en 1.2.4 alignés" 

[ "$fail" = 0 ] && echo "release-notes-test : ok"
exit "$fail"

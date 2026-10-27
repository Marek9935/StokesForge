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
grep -q "pas de ligne" "$tmp/err" || { echo "ECHEC : message « pas de ligne version » absent" >&2; fail=1; }
printf '[package]\nname = "b"\nversion = "1.2.3"\n' > "$tmp/crates/b/Cargo.toml"

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
aligne 1 "crates en 1.2.4, argument 1.2.3" 1.2.3
aligne 0 "crates en 1.2.4 alignés" 

[ "$fail" = 0 ] && echo "release-notes-test : ok"
exit "$fail"

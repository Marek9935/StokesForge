#!/usr/bin/env bash
# Essai de scripts/release-notes.sh sur un dépôt factice (#333) : sans réseau,
# sans toucher au vrai dépôt. Usage : scripts/release-notes-test.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/crates/a"
printf '[package]\nname = "a"\nversion = "1.2.3"\n' > "$tmp/crates/a/Cargo.toml"

fail=0
attend() { # attend <code> <description> <tag>
    local code="$1" desc="$2" tag="$3" got=0
    STOKESFORGE_REPO_ROOT="$tmp" "$HERE/release-notes.sh" "$tag" >/dev/null 2>&1 || got=$?
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

[ "$fail" = 0 ] && echo "release-notes-test : ok"
exit "$fail"

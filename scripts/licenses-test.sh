#!/usr/bin/env bash
# Témoins de refus : une livraison incomplète, des notices périmées et une
# licence hors politique doivent échouer avant de livrer un faux résultat vert.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROBE_DIR="$(mktemp -d)"
trap 'rm -rf "$PROBE_DIR"' EXIT

mkdir -p "$PROBE_DIR/package"
if "$REPO_ROOT/scripts/licenses-dist.sh" "$PROBE_DIR/package" > "$PROBE_DIR/missing.log" 2>&1; then
    echo "[licences] ERREUR : un runtime sans notice a été accepté" >&2
    exit 1
fi
grep -Fq 'notice absente du paquet' "$PROBE_DIR/missing.log"

# Copie minimale : ne jamais altérer le verrou du dépôt pour ce témoin.
mkdir -p "$PROBE_DIR/repo/scripts" "$PROBE_DIR/repo/vendor/python/licenses"
cp "$REPO_ROOT/scripts/licenses-dist.sh" "$REPO_ROOT/scripts/licenses-tools.env" \
    "$PROBE_DIR/repo/scripts/"
cp "$REPO_ROOT/vendor/python/lock.env" "$PROBE_DIR/repo/vendor/python/"
sed 's/^LICENSE_PYTHON_VERSION=.*/LICENSE_PYTHON_VERSION=0.0.0/' \
    "$REPO_ROOT/vendor/python/licenses/versions.env" \
    > "$PROBE_DIR/repo/vendor/python/licenses/versions.env"
if "$PROBE_DIR/repo/scripts/licenses-dist.sh" "$PROBE_DIR/package" > "$PROBE_DIR/stale.log" 2>&1; then
    echo "[licences] ERREUR : des notices périmées ont été acceptées" >&2
    exit 1
fi
grep -Fq 'actualiser vendor/python/licenses' "$PROBE_DIR/stale.log"

mkdir -p "$PROBE_DIR/repo/vendor/crate-license-notices"
cp "$REPO_ROOT/scripts/licenses-rust.sh" "$PROBE_DIR/repo/scripts/"
cp "$REPO_ROOT/vendor/crate-license-notices/"{checksums.sha256,NOTICE.txt,provenance.json} \
    "$PROBE_DIR/repo/vendor/crate-license-notices/"
echo 'verrou périmé' > "$PROBE_DIR/repo/Cargo.lock"
if "$PROBE_DIR/repo/scripts/licenses-rust.sh" "$PROBE_DIR/notices.txt" > "$PROBE_DIR/lock.log" 2>&1; then
    echo "[licences] ERREUR : des compléments pour un autre verrou ont été acceptés" >&2
    exit 1
fi
grep -Fq 'actualiser les notices complémentaires avec Cargo.lock' "$PROBE_DIR/lock.log"

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
    echo "[licences] ERREUR : une licence hors politique a été acceptée" >&2
    exit 1
fi
grep -Fq 'error[rejected]' "$PROBE_DIR/rejected.log"
echo "[licences] OK : notices manquantes, périmées, verrou différent et licence hors politique refusés"

#!/usr/bin/env bash
# TEMPORAIRE (#333) : diagnostic du crash 0xc0000005 de stokesforge-app sous Windows.
# Tests de la lib en parallèle, événements JSON (« started » / « ok »), puis
# module fautif lu dans le journal d'événements Windows.
set -uo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
export PATH="$PREFIX:$PATH"
export STOKESFORGE_PYTHON_HOME="$PREFIX"
export RUST_BACKTRACE=1
export RUSTC_BOOTSTRAP=1
cd "$REPO_ROOT"
cargo test -p stokesforge-app --features python --lib -- -Z unstable-options --format json
rc=$?
echo "== code de sortie cargo : $rc"
if [ "$rc" -ne 0 ]; then
    powershell.exe -NoProfile -Command "Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000} -MaxEvents 3 | Format-List TimeCreated,Message" || true
fi
exit "$rc"

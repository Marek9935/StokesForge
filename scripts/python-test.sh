#!/usr/bin/env bash
# Lance les tests du pont Python et du fil de script contre l'interpréteur
# vendorisé.
#
# Trois choses à poser, et rien de plus :
#   - PYO3_CONFIG_FILE : où PyO3 trouve libpython à la COMPILATION (piège n°1) ;
#   - RUSTFLAGS -rpath : où le binaire de test trouve libpython à l'EXÉCUTION
#     (piège n°2). Ici le rpath est absolu — le binaire de test n'est pas
#     distribué ; c'est la distribution qui utilise $ORIGIN ;
#   - PLOTYX_PYTHON_HOME : le préfixe embarqué, pour les tests uniquement.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"

PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
# Seuls les crates listés plus bas sont construits ici : on n'impose ce rpath
# à aucun autre crate du dépôt.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$PREFIX/lib"
export PLOTYX_PYTHON_HOME="$PREFIX"

cd "$REPO_ROOT"
# Deux invocations plutôt qu'un seul `-p A -p B` : chaque crate porte sa propre
# feature `python`, et `set -e` arrête tout dès que l'un des deux échoue.
cargo test -p plotyx-python --features python "$@"
cargo test -p plotyx-script --features python "$@"

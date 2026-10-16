#!/usr/bin/env bash
# Lance les tests Python : pont, scripts, robustesse, application et CLI
# contre le même interpréteur vendorisé.
#
# Trois choses à poser, et rien de plus :
#   - PYO3_CONFIG_FILE : où PyO3 trouve libpython à la COMPILATION (piège n°1) ;
#   - RUSTFLAGS -rpath : où le binaire de test trouve libpython à l'EXÉCUTION
#     (piège n°2). Ici le rpath est absolu — le binaire de test n'est pas
#     distribué ; c'est la distribution qui utilise $ORIGIN ;
#   - STOKESFORGE_PYTHON_HOME : le préfixe embarqué, pour les tests uniquement.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"

PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
# Seuls les crates listés plus bas sont construits ici : on n'impose ce rpath
# à aucun autre crate du dépôt.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$PREFIX/lib"
export STOKESFORGE_PYTHON_HOME="$PREFIX"

cd "$REPO_ROOT"
# Une invocation par crate plutôt qu'un seul `-p A -p B` : chacun porte sa
# propre feature `python`, et `set -e` arrête tout dès que l'un échoue.
cargo test -p stokesforge-python --features python "$@"
cargo test -p stokesforge-script --features python "$@"

# Banc de robustesse, batteries de frontière (b7, b9).
#
# `RUST_TEST_THREADS=1` plutôt que `-- --test-threads=1` : le second occuperait
# le seul `--` que cargo accepte, et empêcherait donc d'écrire
# `scripts/python-test.sh -- --ignored`. La variable fait exactement la même
# chose, sans se disputer la position des arguments.
#
# Pourquoi un seul fil : ces batteries partagent un unique interpréteur et un
# unique moteur de script par fichier de test — CPython ne démarre qu'une fois
# par processus — et les cas d'interruption lisent `is_running()`, qui n'a de
# sens que si un seul script tourne à la fois.
#
# Les REPRODUCTIONS d'anomalies y sont `#[ignore]` (voir la doc de tête du
# crate) : cette ligne ne lance que les garde-fous, et reste verte. Pour
# rejouer les reproductions de la frontière Python :
#     scripts/python-test.sh -- --ignored
RUST_TEST_THREADS=1 cargo test -p stokesforge-robustesse --features python "$@"

# Intégration de la console et CLI headless : mêmes variables, même runtime.
# L'état (console de script, pont paresseux) vit dans `stokesforge-app-state`
# depuis #263 ; la coquille `stokesforge-app` relaie la feature.
cargo test -p stokesforge-app-state --features python "$@"
cargo test -p stokesforge-app --features python "$@"
cargo test -p stokesforge-cli --features python "$@"

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

# `PLOTYX_PYTHON_HOME` est lu côté Rust par `std::env::var` puis `PathBuf`
# (`EmbeddedPython::locate`, piège n°4) : comme `PYO3_CONFIG_FILE`, ce n'est
# pas PATH, MSYS ne le traduit pas pour le process natif — même conversion
# `/c/foo` → `c:/foo` que scripts/python-config.sh.
if [[ "${OS:-}" == "Windows_NT" ]]; then
    PLOTYX_PYTHON_HOME="$(printf '%s' "$PREFIX" | sed -E 's#^/([A-Za-z])/#\1:/#')"
else
    PLOTYX_PYTHON_HOME="$PREFIX"
fi
export PLOTYX_PYTHON_HOME

# Où le binaire de test trouve `libpython`/`python312.dll` à l'EXÉCUTION
# (piège n°2) — mécanisme différent par OS, pas seulement un chemin différent :
#   - Unix : rpath absolu posé à l'édition de liens (`ld` sait chercher une
#     bibliothèque partagée par un chemin gravé dans le binaire) ;
#   - Windows : le chargeur ne connaît pas de rpath, il résout les DLL par
#     PATH (et le répertoire de l'exécutable) — `-Wl,-rpath` n'existe même
#     pas pour `link.exe`/`lld-link`, qui rejette le flag. On préfixe donc
#     PATH avec le préfixe embarqué (racine, `python312.dll` y vit
#     directement — pas de `lib/`, cf. pyo3-config.windows.template.txt) au
#     lieu de toucher à RUSTFLAGS.
if [[ "${OS:-}" == "Windows_NT" ]]; then
    export PATH="$PREFIX:$PATH"
else
    # Seuls les crates listés plus bas sont construits ici : on n'impose ce
    # rpath à aucun autre crate du dépôt.
    export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$PREFIX/lib"
fi

cd "$REPO_ROOT"
# Une invocation par crate plutôt qu'un seul `-p A -p B` : chacun porte sa
# propre feature `python`, et `set -e` arrête tout dès que l'un échoue.
cargo test -p plotyx-python --features python "$@"
cargo test -p plotyx-script --features python "$@"

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
RUST_TEST_THREADS=1 cargo test -p plotyx-robustesse --features python "$@"

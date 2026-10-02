#!/usr/bin/env bash
# Contrôle instrumenté du pont zéro-copie (tâche 5.2) : lance le binaire de
# tests de tests/zerocopy.rs sous valgrind memcheck.
#
# Ce que ça cherche : des lectures ou écritures INVALIDES à la frontière
# Rust/PyO3/numpy — pas des fuites. C'est pourquoi :
#   - aucune option --leak-check n'est passée : les allocations « encore
#     joignables » de l'allocateur CPython (interpréteur jamais finalisé,
#     cf. src/interpreter.rs) sont nombreuses et ATTENDUES ; les compter comme
#     des fuites noierait le signal utile sous du bruit connu et accepté ;
#   - PYTHONMALLOC=malloc est posé AVANT de lancer le binaire : sans ça,
#     l'allocateur de petits objets ("pymalloc") de CPython réutilise des
#     zones mémoire d'une manière que valgrind ne comprend pas, et produit un
#     torrent de faux positifs « lecture non initialisée ». `malloc` route
#     toutes les allocations Python par le malloc du système, seul chemin que
#     memcheck sait suivre correctement ;
#   - --error-exitcode=1 : un memcheck qui trouve une erreur doit faire
#     échouer ce script, pas se contenter d'un rapport ignoré.
#
# `miri` n'est PAS une alternative ici : miri n'exécute pas d'appel de
# bibliothèque étrangère (FFI), donc rien de ce qui traverse PyO3 (tout ce
# module). Il reste l'outil pertinent pour le reste du modèle StokesForge, qui est
# du Rust pur sans FFI.
#
# `valgrind` n'est PAS installé sur cette machine de développement (vérifié :
# `which valgrind` échoue). Ce script le détecte, l'écrit clairement sur
# stderr, et sort en 0 : l'absence de l'outil n'est pas un échec du pont
# zéro-copie, mais un contrôle non exécuté. Personne ne doit pouvoir lire
# « ce script est passé » comme « le contrôle instrumenté a eu lieu ».
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="$REPO_ROOT/vendor/python/extracted/python"

if ! command -v valgrind >/dev/null 2>&1; then
    cat >&2 <<'EOF'
[memcheck] valgrind absent : le contrôle instrumenté n'a pas tourné.
[memcheck]   sudo dnf install valgrind
[memcheck] puis relance ce script.
[memcheck] (rien n'a été vérifié ici — ceci n'est PAS une confirmation que le
[memcheck]  pont zéro-copie est mémoire-sûr, seulement que l'outil manque.)
EOF
    exit 0
fi

# --- 1. Construction du binaire de tests ------------------------------------
# Mêmes trois variables que scripts/python-test.sh (voir son en-tête pour le
# détail des trois pièges qu'elles couvrent).
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$PREFIX/lib"
export STOKESFORGE_PYTHON_HOME="$PREFIX"

cd "$REPO_ROOT"

# Le nom du binaire produit contient un hash de build non prévisible
# (target/debug/deps/zerocopy-<hash>) ; on le retrouve après coup en prenant,
# parmi les binaires exécutables « zerocopy-* », le plus récemment modifié —
# repère posé juste avant la construction.
REPERE="$REPO_ROOT/target/.memcheck-repere"
mkdir -p "$REPO_ROOT/target"
touch "$REPERE"

echo "[memcheck] construction du binaire de tests/zerocopy.rs" >&2
cargo test -p stokesforge-python --features python --test zerocopy --no-run

BINARY="$(find "$REPO_ROOT/target/debug/deps" -maxdepth 1 -type f -executable \
    -name 'zerocopy-*' -newer "$REPERE" -print | head -n1)"
rm -f "$REPERE"

if [[ -z "$BINARY" ]]; then
    echo "[memcheck] ERREUR : binaire de tests/zerocopy.rs introuvable après construction" >&2
    exit 1
fi
echo "[memcheck] binaire : $BINARY" >&2

# --- 2. Exécution sous valgrind ----------------------------------------------
# --test-threads=1 : un seul fil de test à la fois. CPython ne démarre qu'une
# fois par processus (Once, cf. tests/zerocopy.rs) donc le parallélisme ne
# change rien à la couverture ; en série, un rapport valgrind qui pointe une
# erreur reste lisible pile sur le test qui l'a déclenchée.
#
# STOKESFORGE_UNDER_VALGRIND=1 : posée pour le binaire de tests, pas pour valgrind
# lui-même. Le test de mesure (emprunt vs copie) la lit pour savoir qu'il
# tourne sous instrumentation : valgrind ralentit l'emprunt (dominé par
# quelques appels) et la copie (dominée par un transfert mémoire) de façons
# très différentes, ce qui rend le RAPPORT mesuré sous instrumentation
# dépourvu de sens — voir tests/zerocopy.rs pour le détail. Le test affiche
# alors ses deux mesures sans affirmer le rapport, plutôt que d'échouer sur un
# faux négatif.
# --nocapture : sans quoi libtest avale la sortie des tests qui réussissent —
# et c'est précisément sous valgrind qu'on veut VOIR les deux mesures du test
# emprunt_au_moins_vingt_fois_plus_rapide_que_la_copie, pas seulement savoir
# qu'il est passé.
echo "[memcheck] lancement sous valgrind (PYTHONMALLOC=malloc, sans --leak-check)" >&2
PYTHONMALLOC=malloc STOKESFORGE_UNDER_VALGRIND=1 valgrind \
    --error-exitcode=1 \
    --track-origins=yes \
    "$BINARY" --test-threads=1 --nocapture

echo "[memcheck] OK — aucune lecture/écriture invalide détectée par valgrind" >&2

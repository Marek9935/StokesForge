#!/usr/bin/env bash
# Assemble et VALIDE la distribution autonome de la fenêtre `stokesforge-app`, pont
# Python compris, **et** le binaire d'auto-vérification `stokesforge-selftest`.
#
# C'est le verrou de la tâche 5.6, côté empaquetage : si ce script sort 0,
# TROIS choses sont vraies depuis un binaire empaqueté — pas depuis
# `cargo run`, ni contre l'interpréteur vendorisé du dépôt :
#   1. la fenêtre `stokesforge-app` s'ouvre et se ferme proprement ;
#   2. `stokesforge-selftest` — construit et lié exactement comme `stokesforge-app`,
#      même rpath relatif, même disposition `lib/python` — REJOUE le critère
#      de fin du socle entier : édition/annulation/rétablissement,
#      aller-retour disque, et un script Python qui lit une colonne sans
#      copie, en écrit une autre par le bus, annulée depuis l'hôte comme
#      depuis Python ;
#   3. ce binaire d'auto-vérification sort en code 0.
#
# Les deux binaires sont construits sur le même modèle que
# `scripts/pyhello-dist.sh` (mêmes pièges n°1/n°2, même disposition
# `lib/python`) et copiés CÔTE À CÔTE dans `dist/stokesforge-app/`, pour que les
# deux trouvent `lib/python` par le même chemin relatif
# (`<dossier de l'exécutable>/lib/python`, cf. `EmbeddedPython::locate`).
#
# # Ce que ce script NE prouve PAS
#
# `stokesforge-app` démarre l'interpréteur **paresseusement**, au premier script
# (voir `stokesforge_app_state::state`) : son ouverture-fermeture par auto-fermeture ne
# lance donc aucun script et ne touche jamais Python. C'est exactement le
# trou que `stokesforge-selftest` comble : lui seul, dans ce script, exécute
# réellement un script Python depuis le paquet.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../vendor/python/lock.env
source "$REPO_ROOT/vendor/python/lock.env"

PREFIX="$REPO_ROOT/vendor/python/extracted/python"
DIST="$REPO_ROOT/dist/stokesforge-app"
SOFILE="libpython${PYTHON_XY}.so.1.0"
AUTOQUIT_MS="${STOKESFORGE_APP_DIST_AUTOQUIT_MS:-1500}"

# --- 1. Construction --------------------------------------------------------
# PYO3_CONFIG_FILE : trouver libpython à la COMPILATION (piège n°1).
PYO3_CONFIG_FILE="$("$REPO_ROOT/scripts/python-config.sh")"
export PYO3_CONFIG_FILE

# rpath RELATIF, comme pour pyhello : les deux binaires trouveront leur
# libpython dans le dossier où ils sont copiés (piège n°2), une fois déplacés
# hors du dépôt — d'où le besoin de les copier au MÊME endroit (étape 2).
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/lib'

echo "[dist] construction release de stokesforge-app (feature python)"
cd "$REPO_ROOT"
cargo build -p stokesforge-app --features python --release --bin stokesforge-app

echo "[dist] construction release de stokesforge-selftest (feature python)"
cargo build -p stokesforge-script --features python --release --bin stokesforge-selftest

# --- 2. Assemblage ----------------------------------------------------------
echo "[dist] assemblage de $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/lib/python/lib"

cp "$REPO_ROOT/target/release/stokesforge-app" "$DIST/stokesforge-app"
# À CÔTÉ de stokesforge-app, pas dans un sous-dossier : c'est ce qui lui fait
# trouver lib/python par le même chemin relatif qu'EmbeddedPython::locate()
# résout pour stokesforge-app (<dossier de l'exécutable>/lib/python).
cp "$REPO_ROOT/target/release/stokesforge-selftest" "$DIST/stokesforge-selftest"

# Même disposition que dist/pyhello : libpython une seule fois dans lib/, là
# où pointe le rpath ; le préfixe embarqué sous lib/python/lib, là où
# EmbeddedPython::locate() l'attend (<dir de l'exécutable>/lib/python).
cp "$PREFIX/lib/$SOFILE" "$DIST/lib/$SOFILE"
ln -s "$SOFILE" "$DIST/lib/libpython${PYTHON_XY}.so"
cp -a "$PREFIX/lib/python${PYTHON_XY}" "$DIST/lib/python/lib/"
find "$PREFIX/lib" -maxdepth 1 -name '*.so*' ! -name 'libpython*' \
    -exec cp -a {} "$DIST/lib/python/lib/" \;
rm -rf "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip" \
       "$DIST/lib/python/lib/python${PYTHON_XY}/site-packages/pip-"*.dist-info
# __pycache__ conservés : voir la doc de pyhello-dist.sh (même raison, même
# gain de ~165 ms au premier script).

# Licences du code, des ressources et du runtime effectivement livré (#129).
"$REPO_ROOT/scripts/licenses-dist.sh" "$DIST"

TAILLE="$(du -sm "$DIST" | cut -f1)"
echo "[dist] taille de $DIST : ${TAILLE} Mo"

# --- 3. Preuve de liaison ---------------------------------------------------
echo "[dist] ldd | grep python (stokesforge-app) :"
ldd "$DIST/stokesforge-app" | grep -i python
if ! ldd "$DIST/stokesforge-app" | grep -q "$DIST/lib/$SOFILE"; then
    echo "[dist] ERREUR : $SOFILE n'est pas résolue depuis $DIST/lib pour stokesforge-app" >&2
    exit 1
fi

echo "[dist] ldd | grep python (stokesforge-selftest) :"
ldd "$DIST/stokesforge-selftest" | grep -i python
if ! ldd "$DIST/stokesforge-selftest" | grep -q "$DIST/lib/$SOFILE"; then
    echo "[dist] ERREUR : $SOFILE n'est pas résolue depuis $DIST/lib pour stokesforge-selftest" >&2
    exit 1
fi

# --- 4. Preuve que la fenêtre empaquetée s'ouvre et se ferme ----------------
#
# Environnement nettoyé (`env -i`) : PATH et HOME minimaux, plus le strict
# nécessaire pour qu'une vraie fenêtre puisse s'ouvrir sur CE poste — une
# fenêtre a besoin d'un serveur d'affichage, ce que `pyhello` (purement
# console) n'exigeait pas.
#
# `WAYLAND_DISPLAY` et `XDG_RUNTIME_DIR` sont délibérément ABSENTS de cet
# environnement : sur ce poste (session GNOME/Wayland), la fermeture
# programmée par `STOKESFORGE_APP_AUTOQUIT_MS` ne réveille jamais la boucle
# d'événements de `winit` sous son greffon Wayland — le processus reste
# indéfiniment dans `ep_poll`, constaté par `cargo run` direct (`timeout 90`
# sans sortie). Le même binaire, sous Xwayland (`DISPLAY` sans
# `WAYLAND_DISPLAY`), s'ouvre et se ferme normalement (code 0, quelques
# secondes) : c'est donc une limite du greffon Wayland de `winit`/`eframe`
# sur CE compositeur, pas un défaut de `stokesforge-app`. Forcer Xwayland ici est
# le contournement le plus honnête tant que ce n'est pas creusé côté
# `eframe` ; voir le rapport de la tâche 5.6 pour le détail.
if [[ -z "${DISPLAY:-}" ]]; then
    echo "[dist] ERREUR : DISPLAY n'est pas posée — il faut un serveur X11/Xwayland accessible" >&2
    exit 1
fi

# Dossier de configuration jetable (#333) : sans argument, la première
# ouverture écrit la démonstration dans `demo.stf` de ce dossier et l'ouvre ;
# c'est ce parcours de production que la fenêtre empaquetée éprouve, sans
# toucher au vrai profil de la machine.
CONFIG_DIR="$(mktemp -d)"
trap 'rm -rf "$CONFIG_DIR"' EXIT

echo "[dist] exécution en environnement nettoyé, STOKESFORGE_APP_AUTOQUIT_MS=$AUTOQUIT_MS"
set +e
env -i PATH=/usr/bin:/bin HOME=/tmp \
       DISPLAY="$DISPLAY" XAUTHORITY="${XAUTHORITY:-}" \
       STOKESFORGE_CONFIG_DIR="$CONFIG_DIR" \
       STOKESFORGE_APP_AUTOQUIT_MS="$AUTOQUIT_MS" \
       "$DIST/stokesforge-app" 2>&1 | tee "$DIST/../stokesforge-app-run.log"
CODE="${PIPESTATUS[0]}"
set -e

if [[ "$CODE" -ne 0 ]]; then
    echo "[dist] ERREUR : la fenêtre empaquetée est sortie en code $CODE (attendu 0)" >&2
    exit 1
fi

# La première ouverture a offert la démonstration : le fichier existe et le
# marqueur est posé (#333).
if [[ ! -s "$CONFIG_DIR/demo.stf" ]] || ! grep -q '"first_run_done": true' "$CONFIG_DIR/preferences.json" 2>/dev/null; then
    echo "[dist] ERREUR : la première ouverture n'a pas écrit demo.stf et le marqueur dans $CONFIG_DIR" >&2
    exit 1
fi

echo "[dist] OK — fenêtre empaquetée ouverte puis fermée proprement (code 0), ${TAILLE} Mo"

# --- 5. Preuve que le critère de fin du socle tient DEPUIS LE PAQUET -------
#
# C'est ici que ce script cesse d'être une preuve d'ouverture de fenêtre pour
# devenir la preuve du critère de fin ENTIER. Environnement VIDÉ puis PIÉGÉ,
# comme pour pyhello-dist.sh : si la résolution du préfixe embarqué ou son
# isolation fuyaient vers un Python système, le faux PYTHONPATH/PYTHONHOME le
# révélerait plutôt que de passer inaperçu. Aucun DISPLAY nécessaire :
# stokesforge-selftest est un programme console, pas une fenêtre.
echo "[dist] auto-vérification depuis le paquet, environnement nettoyé et piégé"
set +e
env -i PATH=/usr/bin:/bin HOME=/tmp \
       PYTHONPATH=/tmp/piege PYTHONHOME=/tmp/piege2 \
       "$DIST/stokesforge-selftest" 2>&1 | tee "$DIST/../stokesforge-selftest-run.log"
CODE_SELFTEST="${PIPESTATUS[0]}"
set -e

if [[ "$CODE_SELFTEST" -ne 0 ]]; then
    echo "[dist] ERREUR : stokesforge-selftest est sorti en code $CODE_SELFTEST (attendu 0) — le critère de fin du socle n'est PAS vérifié depuis le paquet" >&2
    exit 1
fi

echo "[dist] OK — fenêtre empaquetée ET critère de fin du socle vérifiés depuis le paquet (code 0), ${TAILLE} Mo"

---
name: run-stokesforge
description: Build, run, drive and screenshot the StokesForge desktop app (eframe/egui) on Linux — launch it, click menus, type, take PNG screenshots, import a CSV with the CLI, run the shell tests. Use when asked to "run", "start", "launch", "screenshot" StokesForge, or to confirm a UI change works in the real app and not only in tests.
---

# run-stokesforge

StokesForge est une appli de bureau egui (Rust). Elle n'a ni port ni API : on la
pilote avec `driver.py` (Xwayland « rootful » + injection de souris/clavier XTEST
+ capture `import` d'ImageMagick). Un second chemin, plus simple, prend une
seule capture d'une fenêtre Wayland native.

Les chemins sont relatifs à la racine du dépôt. Aucun `sudo` requis.

## Prérequis (vérifiés présents sur cette machine, Fedora)

`Xwayland`, `ImageMagick` (`import`, `magick`), Python 3 avec `python-xlib`
(`pip install python-xlib` sinon), une session Wayland ouverte
(`$WAYLAND_DISPLAY`). Pas de Xvfb/xdotool : inutile.

## Build

```sh
cargo build -p stokesforge-app -p stokesforge-cli   # ~7 s si déjà compilé
```

La feature `python` n'est pas dans le build par défaut : la console Python
affiche « Not available in this build ». Voir `scripts/python-test.sh`.

## Run (agent path) : `driver.py`

```sh
D=.claude/skills/run-stokesforge/driver.py
python3 $D start [projet.stf]      # Xwayland :99 (1400x900) + l'app en X11 ; ~15 s
python3 $D click 20 11             # menu File   (x y [left|right|double])
python3 $D click 58 183            # File > Preferences…
python3 $D ss /tmp/shot.png        # capture, puis la LIRE avec Read
python3 $D key ctrl+z              # F2, Return, Escape, ctrl+shift+s…
python3 $D type "Renamed"          # saisie de texte
python3 $D stop                    # tue l'app et Xwayland
```

Coordonnées : pixels écran X, fenêtre de 1100x700 en (0,0), échelle 1.0.
Sans argument, `start` ouvre le projet de démonstration (Book1 + Graph1).
Journaux : `/tmp/stokesforge-run/{app,xwayland}.log` (`SF_RUN_DIR` pour changer).

Exemple vérifié de bout en bout : Préférences → bascule Français (toute
l'interface change) → retour English ; clic Book1 dans le Project Manager →
`F2` → `type Renamed` → `Return` → nom changé → `ctrl+z` → annulé.

Projet à ouvrir avec `start` : `target/debug/stokesforge-cli import --out p.stf
fichier.csv`.

## Run (chemin rapide, une seule capture, sans Xwayland)

```sh
STOKESFORGE_APP_SCREENSHOT=/tmp/w.ppm target/debug/stokesforge-app & P=$!
# attendre « capture écrite » (≈10-30 s), puis :
kill $P; magick /tmp/w.ppm /tmp/w.png
```

Fenêtre Wayland native, rendue à l'échelle de l'écran (1650x1050 ici). Le
format est PPM. Aucune interaction possible.

## Tests

```sh
cargo test -p stokesforge-app --test shell   # boucle complète sans fenêtre, ~7 s
```

Suite complète : voir `CLAUDE.md` (Commandes) et `stokesforge-verification`.

## Gotchas

- **Les fenêtres s'ouvrent vraiment sur le bureau de l'utilisateur** : Xwayland
  est lancé sans `-rootless`, et l'app ordinaire (chemin rapide) aussi. Les
  garder brèves, toujours finir par `stop`/`kill`.
- **`STOKESFORGE_APP_AUTOQUIT_MS` n'est évalué qu'à l'image peinte** (`autoquit`
  dans `app.rs`, qui demande son propre réveil par `request_repaint_after`).
  Vérifié le 2026-10-12 : l'app se ferme seule après 2-3 s en Wayland natif, avec
  `STOKESFORGE_X11=1` et sous `driver.py start`. Si la fenêtre ne peint jamais
  (masquée, pas d'affichage), rien ne la ferme : borner par `timeout` et
  terminer par un `kill` explicite (par pid : `kill $P`, jamais `pgrep -x`, qui tuerait aussi une instance de l'utilisateur). **N'utilise pas `pkill -f stokesforge-app`** : le
  motif attrape le shell de la commande et le tue (exit 144) ; `pkill -x`/`kill $(pgrep -x …)`.
- **La capture X retarde d'une action sous Xwayland.** `ss` fait donc quatre
  micro-mouvements de pointeur (egui ne repeint que sur événement), capture
  une première fois pour jeter l'image, recommence, et garde la seconde. Ne
  pas court-circuiter par un `import` direct.
- **Un clic sur une cible lointaine est perdu** si le pointeur n'a pas survolé
  la cible avant. `click` fait donc mouvement + micro-mouvement + attente avant
  d'appuyer. Les entrées de menu en ont besoin.
- **Pas de gestionnaire de fenêtres → pas de focus clavier.** `start` appelle
  `focus()` ; sans cela `key`/`type` ne font rien. Les menus sont en pixels
  fixes (en français les largeurs changent, ex. « Préférences… » reste en
  (58,183) mais le menu est plus large).
- **La langue choisie est enregistrée** dans `~/.config/stokesforge/preferences.json`
  (hors projet). Si un test passe en Français, repasser en English avant de finir.
- Premier rendu lent (wgpu en logiciel, `MESA: warning … Xe KMD` sans gravité) :
  `start` attend 10 s ; une capture trop précoce montre une fenêtre noire.
- `type` sait les lettres, chiffres et `. , - _ = ( ) / # " : [ ] !` ; ajouter
  les autres à la table `names` de `driver.py` au besoin.

## Troubleshooting

- `touche inconnue : #` → caractère absent de la table `names` de `type_text`.
- `app: MORTE` après `start` → lire `/tmp/stokesforge-run/app.log`.
- `fenêtre de l'app introuvable` (`focus`) → l'app n'est pas (encore) mappée :
  relancer `start` ou attendre.
- Capture montrant le menu ouvert mais pas son effet → l'action n'est pas
  passée (clic perdu) ou un 2e `ss` suffit ; refaire le `click`.

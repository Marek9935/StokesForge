# Mesurer la fluidité de l'interface (#340, #341)

Procédure reproductible pour chiffrer le coût par image de l'application
pendant le déplacement d'une fenêtre flottante. Elle a servi le 2026-10-23 à
départager « code lent » et « build debug lent » (résultats plus bas). Rien de
ce qui suit n'est dans le code de production : les chronomètres sont un patch
à appliquer puis à retirer.

## Lancer l'application en release

```sh
cargo run --release -p stokesforge-app     # binaire : target/release/stokesforge-app
```

`cargo run` seul compile en **debug**, sans aucune optimisation (le
`Cargo.toml` n'a pas de `[profile.dev]`) : egui, Plotters et Typst y sont
nettement plus lents. Toute mesure de fluidité se fait en release, ou se compare
debug/release.

## Refaire la mesure

Prérequis : ceux de `.claude/skills/run-stokesforge` (Xwayland, ImageMagick,
Python avec `python-xlib`).

```sh
git apply scripts/perf/instrumentation.patch          # ajoute app/prof.rs et ses appels
cargo build -p stokesforge-app                         # debug
cargo build --release -p stokesforge-app               # release
scripts/perf/mesure-fenetres.sh debug 6                # 6 graphes + Book1 + Graph1
scripts/perf/mesure-fenetres.sh release 6
git apply -R scripts/perf/instrumentation.patch       # retire les chronomètres
```

Le script passe par le pilote `run-stokesforge`, qui pose `STOKESFORGE_DEMO=1`. Le second argument (0 à 6) est le nombre de graphes du projet de démonstration
ouverts en plus de Book1 et Graph1 : refaire varier ce nombre donne la courbe
« coût par fenêtre ».

Le script lance l'application sous Xwayland (`driver.py`), ouvre les graphes par
double-clic dans le Project Manager, glisse la barre de titre d'une fenêtre
pendant 6 s (`scripts/perf/drag.py`, mouvement sinusoïdal) puis imprime les
blocs de 2 s pendant le glissement (`scripts/perf/summ.py`). Le glissement est
parfois perdu, surtout en debug : le script réessaie jusqu'à 3 fois.

`STOKESFORGE_PROF=1` active les chronomètres ; `driver.py` honore `SF_APP`
(binaire à lancer) pour choisir le profil.

### Lire le tableau

Colonnes : nombre d'appels, moyenne, maximum, total, en millisecondes sur 2 s.

| Ligne | Ce qu'elle mesure |
|---|---|
| `00 intervalle entre images` | temps réel entre deux `ui()` ; 8,3 ms = 120 images/s |
| `00b cpu_usage image precedente` | CPU de l'image entière d'après eframe (`ui()` + tessellation + préparation de la peinture, sans l'attente de synchronisation verticale) |
| `01 ui total` | `App::ui` en entier |
| `05 show_workspace` | pile des fenêtres flottantes |
| `07`, `08` | `sync_window_views` et l'aller-retour `set_window_geometry` vers l'acteur de session |
| `09`, `10` | par fenêtre flottante : coque et `tab_contents` |

`cpu_usage − ui total` est la part hors `ui()` (tessellation d'egui, copie des
formes) ; l'attente GPU n'est pas dedans.

## Résultats du 2026-10-23 (release et debug, Xwayland + llvmpipe)

Mesures pendant le glissement d'une fenêtre, Core Ultra 7 155H :

| Build | Fenêtres | Images/s | CPU/image | `ui()` | par fenêtre (`09`) |
|---|---|---|---|---|---|
| debug | 2 | ~75 | 12 ms | 8,7 ms | 1,9 ms |
| debug | 8 | ~23 | 40 ms | 25,6 ms | 1,3 ms |
| release | 8 | ~120 | 3,3 ms | 1,1 ms | 0,07 ms |

Constats :
- le build debug explique l'essentiel (×10 sur `ui()`, ×7 sur l'image entière) ;
- `set_window_geometry` (aller-retour avec l'acteur) coûte 0,1 à 0,2 ms : pas en cause ;
- la clé du cache de rendu ne contient pas la position de la fenêtre : un déplacement ne le relance pas ;
- en debug, seuls 40 % de `ui()` sont dans les fenêtres flottantes ; le reste vient des panneaux, des barres d'outils et de ce qui est publié à chaque image.

## Limites connues

- Rendu logiciel (llvmpipe) sous Xwayland : la part GPU n'est pas représentative.
- La fenêtre « Lignes » occupe toute la largeur de la zone de travail et ne se déplace pas : le script cible une autre fenêtre (coordonnées 440,138 du projet de démonstration). Un autre projet ou une autre résolution demande d'adapter ces coordonnées dans `mesure-fenetres.sh`.
- Pas encore mesuré : un classeur de 10 colonnes × 400 lignes (#341), qui demande un projet à importer (`stokesforge-cli import`) et un geste de pan ou de sélection dans la grille.
- Pistes de #341 non confirmées par la mesure : `column_staleness` et `is_selected` appelés par cellule (`docs/` n'en dit rien ; voir `grid.rs`).

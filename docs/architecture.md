# Architecture

Quatorze crates, une seule direction de dépendance, et deux features
optionnelles. Cette page donne la carte ; le détail de chaque étage est dans
sa propre page.

## Le graphe, de bas en haut

```text
plotyx-id            NodeId seul, zéro dépendance
   ↑          ↑
plotyx-plot   |      PlotSpec : quoi tracer, sans savoir comment
   ↑          |
plotyx-core ──┘      arbre de projet, colonnes, résolution d'un spec
   ↑
plotyx-commands      les commandes annulables + la pile d'historique
   ↑
plotyx-persistence   format .plx (save / load)
   ↑
plotyx-session       l'acteur : le seul &mut Project du processus
   ↑           ↑                    ↑
plotyx-cli   plotyx-script   plotyx-render-egui
             (+ plotyx-python)      ↑
                     ↑        plotyx-render ← plotyx-render-svg
                     └──────── plotyx-app ───┘
```

`plotyx-robustesse` est à part : il dépend de tout le workspace et ne contient
aucun code de production ([`tests.md`](tests.md)).

## Ce que porte chaque crate

| Crate | Rôle | Page |
|---|---|---|
| `plotyx-id` | `NodeId`, et rien d'autre | [modèle](modele-de-donnees.md) |
| `plotyx-plot` | `PlotSpec` : la description d'un graphe, neutre vis-à-vis du renderer | [modèle](modele-de-donnees.md) |
| `plotyx-core` | `Project` / `Tree` / `Node` / `Column`, plus `resolve` | [modèle](modele-de-donnees.md) |
| `plotyx-commands` | les commandes, la pile d'undo hybride, les macros | [commandes](commandes-et-historique.md) |
| `plotyx-persistence` | `save`/`load`, schéma de transport, format `.plx` | [persistance](persistance.md) |
| `plotyx-session` | l'acteur, les `Snapshot`, la `revision` | [session](session.md) |
| `plotyx-render` | mesure de texte, pipeline Plotters, **tout le rendu** | [rendu](rendu.md) |
| `plotyx-render-egui` | backend écran, cache de formes, zoom/pan, feathering | [rendu](rendu.md) |
| `plotyx-render-svg` | export SVG, et par-dessus PDF et PNG | [rendu](rendu.md) |
| `plotyx-python` | interpréteur CPython embarqué, aller-retour numpy | [python](python-et-script.md) |
| `plotyx-script` | le fil de script, la surface `pyx`, l'interruption | [python](python-et-script.md) |
| `plotyx-app` | la coquille UI eframe (banc d'essai) | [application](application.md) |
| `plotyx-cli` | binaire headless de vérification du lien | [application](application.md) |
| `plotyx-robustesse` | 21 batteries de tests aux limites, 2 sondes hors-processus | [tests](tests.md) |

## Les trois lignes de séparation

Le découpage n'est pas décoratif ; chacune de ces trois frontières est
**vérifiée par un test** et non par la relecture.

1. **Aucune dépendance UI sous `plotyx-session`.** `plotyx-core`,
   `plotyx-plot`, `plotyx-commands`, `plotyx-persistence`, `plotyx-session` et
   `plotyx-script` portent chacun un `tests/no_ui_dependency.rs` : une liste
   blanche de dépendances, qui force une décision consciente à toute nouvelle
   arête plutôt que de compter sur une relecture.
2. **Le `PlotSpec` ne connaît aucun moteur de rendu.** `plotyx-plot` ne
   dépend que de `plotyx-id` : ni `plotters`, ni `egui`, ni `typst`, ni même
   `serde`. C'est ce qui fait que changer de renderer ne touche pas la
   description des graphes.
3. **Une fonctionnalité de tracé n'entre jamais dans un seul backend.** Elle
   va dans `plotyx-render`, générique sur `DrawingBackend`. Les deux backends
   ne diffèrent que par la façon de poser des pixels. La seule exception
   vivante est `plotyx-render-egui::feathering`, et elle est justifiée à
   l'endroit où elle vit : la tessellation n'existe que pour l'écran.

Le détail de ces règles et de ce qui les tient : [`invariants.md`](invariants.md).

## Un cycle Cargo évité, et la trace qu'il a laissée

`plotyx-id` existe pour une seule raison, et elle explique une bizarrerie
visible du graphe. Faire porter à `NodeKind::Graph` un `plotyx_plot::PlotSpec`
crée l'arête `plotyx-core → plotyx-plot` ; `plotyx-plot` dépendait déjà de
`plotyx-core` en entier, pour le seul type `NodeId`. Les deux arêtes ensemble
sont un cycle que Cargo refuse (*cyclic package dependency* — vérifié, pas
supposé).

`NodeId` a donc été descendu dans un crate minimal sans dépendance, sous les
deux. `plotyx-core` le réexporte à l'identique (`pub use plotyx_id::NodeId`) :
aucun appelant de `plotyx_core::NodeId` n'a eu à changer. Le raisonnement
complet est en tête de `crates/plotyx-id/src/lib.rs`.

## Les features

Aucune n'est dans `default` : le workspace se construit et se teste
entièrement sans elles.

| Feature | Crates | Ce qu'elle ajoute | Ce qu'elle exige |
|---|---|---|---|
| `python` | `plotyx-python`, `plotyx-script`, `plotyx-app`, `plotyx-robustesse` | PyO3, interpréteur embarqué, fil de script, console de script | `PYO3_CONFIG_FILE` (voir `scripts/python-config.sh`) et l'interpréteur vendorisé de `vendor/python/` |
| `typst` | `plotyx-render`, `plotyx-render-egui`, `plotyx-render-svg` | notations scientifiques : un texte contenant `$` est compilé par Typst | rien de plus que les crates Typst |

Sans la feature, le crate concerné compile vide ou retombe sur le chemin
ordinaire — c'est la condition du droit d'abandonner Typst ou Python sans rien
démonter. Conséquence pratique : `cargo build --workspace` **ne compile pas**
ces chemins. Pour les exercer, `--all-features` (Typst) ou
`scripts/python-test.sh` (Python, qui pose les variables d'environnement).

## Où sont les binaires

| Binaire | Crate | À quoi il sert |
|---|---|---|
| `plotyx-app` | `plotyx-app` | la fenêtre — `plotyx-app [chemin]` |
| `plotyx-cli` | `plotyx-cli` | vérifier que le workspace se lie de bout en bout, sans UI |
| `plotyx-selftest` | `plotyx-script` | rejouer le critère de fin du socle depuis une distribution empaquetée |
| `plotyx-pyhello` | `plotyx-python` | prouver l'isolation de la pile Python empaquetée |
| `plotyx-sonde-alloc`, `plotyx-sonde-pile` | `plotyx-robustesse` | deux sondes hors-processus (échec d'allocation, débordement de pile) |

Les `scripts/*.sh` de la racine assemblent et relancent les distributions
(`dist/`) dans un environnement nettoyé.

## Vocabulaire

- **Nœud** — une entrée de l'arbre de projet : dossier, classeur, feuille,
  colonne, graphe, matrice, note.
- **Fenêtre** (*window*) — un nœud affichable en sous-fenêtre : classeur,
  graphe, matrice, note. `Tree::windows()` les liste ; leurs noms sont uniques
  à l'échelle du projet entier, pas seulement du parent.
- **Révision** — le compteur monotone de `plotyx-session`, incrémenté à chaque
  mutation acceptée. Un client sait que son instantané est périmé en comparant
  la révision, jamais les arbres.
- **Spec** — un `plotyx_plot::PlotSpec` : ce qu'il faut tracer.
- **Instantané** (*snapshot*) — une vue figée et immuable du projet, rendue
  par l'acteur : un `Arc<Tree>` plus la révision et l'état de l'historique.
- **Macro** — un groupe de commandes qui s'annule d'un seul `Ctrl+Z`.
- **Phases** — les commentaires citent des repères de chantier : phases 0-5
  pour le socle, **R0-R8** pour le rendu (R0 mesure, R3 ancrage du spec dans
  le modèle, R4 les deux backends, R5 l'élargissement, R6 PDF, R7 interaction,
  R8 Typst). Toutes closes ; les plans correspondants sont dans
  `docs/archive/`.

# Architecture

Vingt crates, une seule direction de dépendance, et une feature
optionnelle (`python`). Cette page donne la carte ; le détail de chaque étage est dans
sa propre page.

## Le graphe, de bas en haut

Flèche = « dépend de » (manifestes `Cargo.toml`, vérifiés par
`cargo metadata --no-deps --format-version 1 --offline`).

```text
stokesforge-id            NodeId seul, zéro dépendance
   ↑
stokesforge-analysis      calcul pur (lignes de base, intégrales, ajustements, pics,
                          déconvolution) : f64 en entrée et en sortie, aucune
                          dépendance stokesforge — lu par core, commands,
                          persistence, session, script et app
   ↑
stokesforge-plot          PlotSpec : quoi tracer, sans savoir comment
   ↑
stokesforge-core          arbre de projet, colonnes, résolution d'un spec
   ↑            ↑            ↑
stokesforge-commands  stokesforge-persistence  stokesforge-render ← stokesforge-render-svg
(core, plot)     (core, plot, import)  (core, plot)     (core, plot, render)
   ↑       ↑
stokesforge-session       l'acteur : le &mut Project de la session
(commands, persistence, core)
   ↑            ↑
stokesforge-script   stokesforge-render-egui (core, plot, render, session)
(analysis, commands, core, import, importer, plot, python, session)
+ stokesforge-python (aucune dépendance stokesforge)

stokesforge-cli    (core, commands, import, importer, persistence)
               import : un `Project` neuf construit directement, puis `.stf`
               run (python) : script, python, session ; même acteur que la console
stokesforge-import (aucune dépendance stokesforge) — lecture des fichiers d'appareil ;
               lu par persistence : le miroir de la recette dans le `.stf` (S2, #323)
stokesforge-importer (core, import) — plan de feuilles et colonnes du modèle, sans
               commandes : partagé par l'application, le CLI et les scripts
stokesforge-clipboard (aucune dépendance stokesforge) — offre multi-format au
               presse-papiers système (SVG, PNG, PDF), un backend par affichage
stokesforge-app-state (analysis, commands, core, import, importer, persistence, plot,
               render, render-svg, session ; script et python en option) — l'état de
               l'application SANS interface : AppState, sélection, préférences,
               formulaires d'analyse, textes (Fluent) et erreurs mises en phrase
stokesforge-app    (app-state, analysis, clipboard, commands, core, import, plot,
               render, render-egui, render-svg, session ; python en option) — la
               coquille eframe : peint l'état, traduit clavier et souris en actions
```

`stokesforge-robustesse` est à part : il dépend du socle et du pont Python, et ne
contient aucun code de production ([`tests.md`](tests.md)). `stokesforge-arch-guard`
est un outil de test : la lecture commune des manifestes pour les garde-fous de
dépendances.

## Ce que porte chaque crate

| Crate | Rôle | Page |
|---|---|---|
| `stokesforge-id` | `NodeId`, et rien d'autre | [modèle](modele-de-donnees.md) |
| `stokesforge-analysis` | traitement mathématique pur (#181) : lignes de base, correction par référence, intégrales, ajustements, pics, déconvolution ; `f64` où `NaN` dit « vide », résultats à clés stables ; ni UI, ni Python, ni autre crate du workspace | [modèle](modele-de-donnees.md) |
| `stokesforge-plot` | `PlotSpec` : la description d'un graphe, neutre vis-à-vis du renderer | [modèle](modele-de-donnees.md) |
| `stokesforge-core` | `Project` / `Tree` / `Node` / `Column`, plus `resolve` (spec → données) et `compose` (colonnes → séries d'un graphe, #257 : appariement X/Y, palette par rang, calques ; lecture du `PlotSpec` ; aucune phrase, aucune mutation) | [modèle](modele-de-donnees.md) |
| `stokesforge-commands` | les commandes, la pile d'undo hybride, les macros | [commandes](commandes-et-historique.md) |
| `stokesforge-persistence` | `save`/`load`, schéma de transport, format `.stf` | [persistance](persistance.md) |
| `stokesforge-session` | l'acteur, les `Snapshot`, la `revision` | [session](session.md) |
| `stokesforge-import` | lecture des fichiers d'appareil (CSV/TXT/ASC), recettes, aperçu ; indépendant de core/UI | [application](application/projet-et-import.md) |
| `stokesforge-render` | mesure de texte, pipeline Plotters, **tout le rendu** | [rendu](rendu.md) |
| `stokesforge-render-egui` | backend écran (`DrawingBackend` → `egui::Shape`), cache de formes, lecture des entrées egui (molette, glisser), feathering ; le calcul du zoom/pan et du point le plus proche est dans `stokesforge-render` (#262) | [rendu](rendu.md) |
| `stokesforge-render-svg` | export SVG, et par-dessus PDF et PNG | [rendu](rendu.md) |
| `stokesforge-python` | interpréteur CPython embarqué, aller-retour numpy | [python](python-et-script.md) |
| `stokesforge-script` | le fil de script, la surface `pyst`, l'interruption | [python](python-et-script.md) |
| `stokesforge-app-state` | l'état de l'application **sans egui** (#263) : `AppState` et ses `Action`, sélection, préférences, formulaires d'analyse, Object Manager, texte d'interface (`tr!`, catalogues Fluent `en`/`fr`) et erreurs mises en phrase ; testable sans fenêtre (`tests/shell.rs`). Ni `egui`, ni `eframe`, ni `winit`, ni `wgpu` : le compilateur tient la frontière | [application](application/etat-et-console.md) |
| `stokesforge-app` | la coquille UI eframe (banc d'essai) : fenêtres, grille, menus, thème, icônes, copie de la figure au presse-papiers ; peint l'état de `stokesforge-app-state`. Avec lui, **seuls** crates qui traduisent l'interface (Fluent : `i18n-embed`, `i18n-embed-fl`, `rust-embed`, `fluent-syntax`) | [application](application/coquille-et-code.md) |
| `stokesforge-cli` | import et scripts Python sans interface | [application](application/etat-et-console.md) |
| `stokesforge-importer` | du résultat de `stokesforge-import` aux colonnes du modèle (plan de feuilles, construction des colonnes) — sans commandes ni interface, partagé par l'application, le CLI et les scripts | [application](application/projet-et-import.md) |
| `stokesforge-clipboard` | offre d'un même contenu au presse-papiers système sous plusieurs types MIME (la figure d'un graphe : SVG, PNG, PDF) ; Wayland, X11, Windows, macOS ; indépendant du reste du workspace | [application](application/graphe.md) |
| `stokesforge-robustesse` | 22 batteries de tests aux limites, 2 sondes hors-processus | [tests](tests.md) |
| `stokesforge-arch-guard` | lecture commune des `Cargo.toml` pour les garde-fous de dépendances (dev-dependency) | [tests](tests.md) |

## Les trois lignes de séparation

Le découpage n'est pas décoratif ; chacune de ces trois frontières est
**vérifiée par un test** et non par la relecture.

1. **Aucune dépendance UI sous `stokesforge-session`, ni dans l'état de
   l'application.** `stokesforge-analysis`, `stokesforge-core`,
   `stokesforge-plot`, `stokesforge-commands`, `stokesforge-persistence`, `stokesforge-session`,
   `stokesforge-script`, `stokesforge-import`, `stokesforge-importer`, `stokesforge-clipboard` et
   `stokesforge-app-state` (#263 : l'état de l'application, sans egui ; la liste
   blanche y refuse `winit` et `stokesforge-clipboard`, qui le tire) portent chacun un
   `tests/no_ui_dependency.rs` : une liste blanche de dépendances, qui force
   une décision consciente à toute nouvelle arête plutôt que de compter sur une
   relecture. La lecture du manifeste est commune (`stokesforge-arch-guard`) :
   dépendances normales, de build, conditionnelles (`[target.…]`) et renommées
   (`package = …`, jugées sur leur nom réel) comptent ; les `dev-dependencies`
   non. `stokesforge-render` a son propre `tests/frontiere.rs`, qui s'appuie sur la
   même lecture.
2. **Le `PlotSpec` ne connaît aucun moteur de rendu.** `stokesforge-plot` ne
   dépend que de `stokesforge-id` : ni `plotters`, ni `egui`, ni `typst`, ni même
   `serde`. C'est ce qui fait que changer de renderer ne touche pas la
   description des graphes.
3. **Une fonctionnalité de tracé n'entre jamais dans un seul backend.** Elle
   va dans `stokesforge-render`, générique sur `DrawingBackend`. Les deux backends
   ne diffèrent que par la façon de poser des pixels. La seule exception
   vivante est `stokesforge-render-egui::feathering`, et elle est justifiée à
   l'endroit où elle vit : la tessellation n'existe que pour l'écran.

Le détail de ces règles et de ce qui les tient : [`invariants.md`](invariants.md).

## Un cycle Cargo évité, et la trace qu'il a laissée

`stokesforge-id` existe pour une seule raison, et elle explique une bizarrerie
visible du graphe. Faire porter à `NodeKind::Graph` un `stokesforge_plot::PlotSpec`
crée l'arête `stokesforge-core → stokesforge-plot` ; `stokesforge-plot` dépendait déjà de
`stokesforge-core` en entier, pour le seul type `NodeId`. Les deux arêtes ensemble
sont un cycle que Cargo refuse (*cyclic package dependency* — vérifié, pas
supposé).

`NodeId` a donc été descendu dans un crate minimal sans dépendance, sous les
deux. `stokesforge-core` le réexporte à l'identique (`pub use stokesforge_id::NodeId`) :
aucun appelant de `stokesforge_core::NodeId` n'a eu à changer. Le raisonnement
complet est en tête de `crates/stokesforge-id/src/lib.rs`.

## Les features

Une seule feature est hors `default`, `python` : le workspace se construit et se
teste entièrement sans elle.

| Feature | Crates | Ce qu'elle ajoute | Ce qu'elle exige |
|---|---|---|---|
| `python` | `stokesforge-python`, `stokesforge-script`, `stokesforge-app-state`, `stokesforge-app`, `stokesforge-robustesse` | PyO3, interpréteur embarqué, fil de script, console de script | `PYO3_CONFIG_FILE` (voir `scripts/python-config.sh`) et l'interpréteur vendorisé de `vendor/python/` |

Sans la feature `python`, les crates concernés compilent vides — c'est la
condition du droit d'abandonner Python sans rien démonter. Conséquence
pratique : `cargo build --workspace` **ne compile pas** ce chemin ; pour
l'exercer, `scripts/python-test.sh` pose les variables d'environnement
(commandes de référence : [`.codex/README.md`](../.codex/README.md)).

**Typst n'est pas une feature.** Depuis l'issue #29, `typst`, `typst-layout`,
`typst-render` et `typst-svg` (tous `=0.15.1`) sont des dépendances fermes de
`stokesforge-render`, `stokesforge-render-egui` et `stokesforge-render-svg`, toujours
compilées : `cargo build` et `cargo test --workspace` les couvrent. Ce qui
reste optionnel est le **chemin d'exécution** d'un backend (formules par Typst
ou en texte brut), détaillé dans [`rendu.md`](rendu.md#typst-r8).

## Où sont les binaires

| Binaire | Crate | À quoi il sert |
|---|---|---|
| `stokesforge-app` | `stokesforge-app` | la fenêtre — `stokesforge-app [chemin]` |
| `stokesforge-cli` | `stokesforge-cli` | `import` : appareils → `.stf`, projet neuf sans session ; `recipes` ; `run` (python) : script via session sans fenêtre |
| `stokesforge-selftest` | `stokesforge-script` | rejouer le critère de fin du socle depuis une distribution empaquetée |
| `stokesforge-pyhello` | `stokesforge-python` | prouver l'isolation de la pile Python empaquetée |
| `stokesforge-sonde-alloc`, `stokesforge-sonde-pile` | `stokesforge-robustesse` | deux sondes hors-processus (échec d'allocation, débordement de pile) |

Les `scripts/*.sh` de la racine assemblent et relancent les distributions
(`dist/`) dans un environnement nettoyé.

**Windows (#332, première passe non vérifiée sous Windows).**
`scripts/app-dist-windows.sh` (Git Bash) est le pendant de `app-dist.sh` ;
`python-vendor.sh`, `python-config.sh` (gabarit
`pyo3-config.windows.template.txt`) et `python-test.sh` détectent Windows.
Le préfixe embarqué y a la disposition python-build-standalone
(`lib/python/{Lib,DLLs}`, `python312.dll` à côté de l'exécutable), prise en
charge par `EmbeddedPython` (`cfg(windows)`). Reste : exécuter le tout sous
Windows, établir les notices du runtime Windows (`licenses-dist.sh` refuse
tant que `vendor/python/licenses` est celui de Linux) et fournir un runner
(tâche CI `dist-windows`, manuelle).

## Vocabulaire

- **Nœud** — une entrée de l'arbre de projet : dossier, classeur, feuille,
  colonne, graphe, matrice, note.
- **Fenêtre** (*window*) — un nœud affichable en sous-fenêtre : classeur,
  graphe, matrice, note. `Tree::windows()` les liste ; leurs noms sont uniques
  à l'échelle du projet entier, pas seulement du parent.
- **Révision** — le compteur monotone de `stokesforge-session`, incrémenté à chaque
  mutation acceptée. Un client sait que son instantané est périmé en comparant
  la révision, jamais les arbres.
- **Spec** — un `stokesforge_plot::PlotSpec` : ce qu'il faut tracer.
- **Instantané** (*snapshot*) — une vue figée et immuable du projet, rendue
  par l'acteur : un `Arc<Tree>` plus la révision et l'état de l'historique.
- **Macro** — un groupe de commandes qui s'annule d'un seul `Ctrl+Z`.
- **Phases** — les commentaires citent des repères de chantier : phases 0-5
  pour le socle, **R0-R8** pour le rendu (R0 mesure, R3 ancrage du spec dans
  le modèle, R4 les deux backends, R5 l'élargissement, R6 PDF, R7 interaction,
  R8 Typst). Toutes closes ; les plans correspondants sont dans
  `docs/archive/`.

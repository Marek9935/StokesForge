# Audit #328 : les figures de la démo et leurs gestes d'interface

Audit de **lecture** (2026-10-23), sans exécution de l'application : on cherche,
pour chaque élément des six graphes de la démo
(`crates/stokesforge-app/src/demo/graphes.rs`), le geste d'interface qui le
produit. Les ✗ sont établis par absence de tout appel dans `app/`, `app-state/`
et `render-egui` (recherche textuelle sans résultat). Aucun geste caché dans les
menus contextuels de l'Object Manager n'a été cherché au-delà de ceux cités.

## Tableau par graphe

Les nombres entre crochets renvoient aux manques numérotés plus bas.

| Graphe | Élément | Geste d'interface |
|---|---|---|
| `Graph1` (:66) | courbe, libellés d'axe, couleur et largeur | ✓ Graph › Plot (menu/glisser), Graph Properties › Axes / Series |
| `Panneaux` (:90) | grille 2×2 de 4 cases (`PanelSpec::at(l,c)`) | **✗ [1]** |
| | une série par case, type Line/Scatter/Histogram/Bar | ✓ par case, mais seulement en 1×1 |
| | Histogram 20 classes (:92) | **✗ [2]** |
| `Axes` (:144) | échelle log, grille | ✓ Graph Properties › Axes (`plot_settings.rs`) |
| | série sur l'axe Y de droite du même panneau (`SeriesAxis::Right`, :165) | **✗ [3]** |
| | titre de l'axe de droite | ✓ onglet Axes › Right (`PlotAxis::Right`, state.rs:2213) |
| | calque aux axes secondaires (:181) | ✓ Graph › New Layer (menus.rs:456-505), ou colonnes X2/Y2 (`ColumnRole`, column.rs:205) |
| | titres et unités des axes du calque | ✓ onglet Axes (Top et Right) |
| `TypesTrace` (:197) | 5 cases en grille 2×3 | **✗ [1]** |
| | les 5 types de tracé | ✓ liste de types (plot_settings.rs:771-776) |
| | ErrorBar avec `y_error` (:205) | **✗ [4]** |
| | Histogram 30 classes | **✗ [2]** |
| | titre de case (:258) | ✓ `panel.title` (plot_settings.rs:379) |
| `Annotations` (:265) | texte avec formule Typst | ✓ Graph › Add Text, ou outil Texte |
| | rectangle, ellipse, ligne, flèche | ✓ barre Tools (`shape_tools.rs`), glisser sur la zone de tracé |
| | légende dans un coin | ✓ onglet Legend (`legend_editor`) |
| `Groupes` (:302) | six courbes en un groupe | ✓ tracé de plusieurs colonnes ensemble (state.rs:9400) |
| | nuancier du groupe | ✓ `curve_settings.rs` (`set_group_gradient`) |
| `Lignes` (:336) | séries « ligne » | ✓ glisser des lignes depuis la gouttière (gestes.md:96), `Action::PlotRows`, menu de ligne (grid.rs:915) |

## Éléments sans geste

Classification : **A** = le geste existe, la doc manque ; **B** = fonctionnalité
manquante.

1. **Grille de plusieurs cases non superposées : B.** Rien dans l'application
   n'ajoute un `PanelSpec::at(l, c)` à un graphe existant. Les seuls appels sont
   `app.rs:2678` (graphe neuf en 1×1), et `analysis_preview` / `object_drag` /
   `state.rs:9413` pour des calques. `graphe.md:594` l'écarte explicitement
   (« pas d'ajout/suppression de série ou de panneau depuis la fenêtre »).
   Chantier à part entière. Ce n'est pas un doublon de #168, qui ne couvre que
   les calques X3/Y3 et suivants.
2. **Nombre de classes d'un histogramme : B.** `SeriesSpec::histogram_bins` n'a
   aucun widget. `graphe.md:592` le reconnaît comme « laissé à une itération
   suivante ». Petit chantier : un champ de plus dans Series.
3. **Série sur l'axe Y de droite du même panneau : B.** `SeriesAxis::Right`
   n'est assigné que par la démo, `pyst` et la persistance. Le seul widget voisin
   est l'onglet Axes › Right, qui règle l'axe sans y placer de série. Le besoin
   est en partie couvert par un calque Y à droite (New Layer, ou colonnes X2/Y2),
   mais c'est une autre structure : la démo met deux séries dans le même
   panneau, un calque est un panneau à part. À trancher : le calque suffit-il, ou
   veut-on un choix « axe de la série » ?
4. **Barres d'erreur (`y_error`) : B.** Le type ErrorBar se choisit, mais aucune
   colonne ne peut être désignée comme erreur : pas de rôle de colonne d'erreur
   (`ColumnRole` n'a que X, Y, X2…Y4 et leurs variantes), pas de widget. Seul
   `analysis.rs` lit `y_error` (comme `sigma`). Sans erreur, le tracé ErrorBar ne
   montre rien. C'est le plus gênant : un type de tracé proposé qui ne peut pas
   être alimenté.

## Documentation (cas A, à traiter dans la foulée)

- `gestes.md` ne cite ni New Layer, ni l'ajout de formes, ni le groupement par
  tracé multiple, ni le nuancier. Ces gestes existent et se trouvent dans
  `graphe.md:131` et `menus-et-barres.md:21`, mais pas dans le catalogue de
  gestes.
- La syntaxe Typst des textes (`$lambda_1$`) n'est documentée que côté rendu.
  Non vérifié : un texte d'interface ou `graphe.md` l'explique-t-il à
  l'utilisateur ?

## Découpage proposé en sous-issues (non créées)

- **S1 (A, petit)** : compléter `gestes.md` et les info-bulles : New Layer,
  formes, groupes et nuancier, Typst dans Add Text. Contrôle : on refait
  `Annotations`, `Groupes` et `Lignes` en suivant la doc.
- **S2 (B, petit)** : widget « Classes » (`histogram_bins`) dans Series. Il
  couvre `Panneaux` et `TypesTrace`.
- **S3 (B, moyen)** : désigner la colonne d'erreur d'une série ErrorBar. À
  trancher entre un rôle de colonne (`yEr`, comme X2/Y2) et un champ dans Series.
- **S4 (B, à arbitrer)** : assigner une série à l'axe Y de droite du panneau, ou
  déclarer que le calque suffit et réécrire la démo `Axes` avec un calque.
- **S5 (B, gros, décision d'abord)** : grille de plusieurs cases (ajout,
  suppression, placement ligne/colonne). Dépend du choix de produit « figures
  multi-panneaux dans la V1 ». Avant le code, un formulaire de décisions : geste
  (menu Graph, glisser, Object Manager), nombre de cases, indépendance des axes.
- **Option** : si S5 est repoussé après la V1, réécrire les démos `Panneaux` et
  `TypesTrace` en graphes séparés et le dire dans la doc de la démo. L'issue #328
  accepte explicitement un graphe de démo non reproductible.

**Cohérence** : `docs/application/etat-et-console.md` (description de la démo),
`docs/gestes.md`, `docs/application/graphe.md`.

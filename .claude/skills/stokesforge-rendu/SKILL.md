---
name: stokesforge-rendu
description: Travailler sur le rendu de StokesForge — tracés, axes, échelles, séries, légende, marqueurs, styles de trait, décimation, zoom/pan, export SVG/PDF/PNG, formules Typst. À utiliser dès qu'une tâche touche `stokesforge-render`, `stokesforge-render-egui` ou `stokesforge-render-svg`, ou dès qu'un symptôme est visuel (figure décalée, texte tronqué, marges fausses, écran différent de l'export, figure qui ne se rafraîchit pas). Contient les deux pièges silencieux du dépôt — la mesure de texte de Plotters sans ses polices, et le risque d'écrire une fonctionnalité dans un seul backend.
---

# Toucher au rendu de StokesForge

Le rendu est la zone du dépôt où **un bug peut être silencieux**. C'est ce qui
justifie le poids des règles ci-dessous : elles ne protègent pas d'une erreur
de compilation, elles protègent d'une figure fausse qui a l'air juste.

Lire `docs/rendu.md` avant de commencer ; charger `stokesforge-render` **d'abord**,
puis seulement le backend visé.

## L'architecture en une phrase

```text
stokesforge-render          tout le rendu, générique sur DrawingBackend
   ↑                        ↑
stokesforge-render-egui    stokesforge-render-svg
  (écran)              (SVG → PDF, PNG)
```

Les deux backends ne diffèrent que par **la façon de poser des pixels**. Types
de série, échelles, primitives, rognage, légende, décimation, interaction,
formules : tout est dans `stokesforge-render`.
Vocabulaire d'interface (#322) : le texte Typst entre `$…$` se dit « Math » / « Maths » à l'écran et dans les clés (`plot-math-*`, `format-math`) ; « Formula » est réservé au calcul d'une colonne. Les identifiants du code (`formula::`, `TextFormat::Formula`) gardent leur nom.

`RenderError` (nœud inconnu, pas un graphe, échec de Plotters) vit dans
`stokesforge-render` (#255) ; `stokesforge-render-egui` la réexporte. Ne pas la
redéfinir côté backend : `errors.rs` de `stokesforge-app-state` (crate sans egui), la lit
sans passer par `stokesforge-render-egui`.

## Piège 1 — la mesure de texte (invariant 4)

`plotters` est pris **sans ses fonts** (`default-features = false`). Deux
conséquences à connaître par cœur :

- un `DrawingBackend` qui ne surcharge pas `draw_text` **panique** à la
  première graduation ;
- un qui ne surcharge pas `estimate_text_size` mesure le texte au **nombre
  d'octets UTF-8** — « λ » compte pour 2, « 111 » mesure comme « WWW » — et **ne
  signale rien**.

Toute mesure passe donc par `stokesforge_render::text`, et par nulle part ailleurs.
Ce qui est partagé, ce sont les `epaint::FontDefinitions` — pas l'instance
`epaint::Fonts`. Le danger n'est pas « deux instances », c'est « deux jeux de
polices ».

**Où la mesure entre dans la géométrie**, concrètement : `DrawingArea::titled`
demande la hauteur du titre à `estimate_text_size`, puis fait descendre le haut
de la zone de tracé d'autant. Une mesure fausse de quelques pixels déplace la
zone entière, axes compris. La figure exportée n'est alors plus la figure vue.

**L'angle mort** : `configure_series_labels` et `MultiLineText` de Plotters
mesurent le texte eux-mêmes, par `FontData::estimate_layout`, donc par
l'heuristique naïve — jamais par `estimate_text_size`. C'est pour cette raison
précise que la légende est écrite à la main dans `stokesforge_render::legend`. Ne
pas la « simplifier » en la redéléguant à Plotters.

## Piège 2 — une fonctionnalité n'entre jamais dans un seul backend (invariant 5)

Le pipeline Plotters est écrit **une fois**, générique sur `DB: DrawingBackend`
(`stokesforge_render::pipeline`). Un test peut constater un désaccord ; il ne peut
pas empêcher qu'il naisse. Si chaque backend appelait son propre
`ChartBuilder`, la moindre divergence de marge, de police ou d'ordre d'appel
deviendrait un désaccord à trouver après coup.

Une seule exception vit aujourd'hui : `stokesforge-render-egui::feathering` — un
réglage de tessellation, en aval de la collecte des `Shape`, et un SVG ne
triangule rien. Il n'y a littéralement rien à mettre de l'autre côté.

Corollaire pratique : une traduction de donnée (une cellule mixte qui devient
`NaN`, par exemple) se fait **en amont, une fois**, jamais dans un backend.

## Piège 3 — le cache et le cadrage de vue

- La clé de cache de `stokesforge-render-egui` est
  `(revision, rect, pixels_per_point, view_generation)`. Une figure qui ne se
  rafraîchit pas, ou qui se reconstruit trop souvent, se diagnostique là.
- **Le feathering n'entre pas dans la clé** (invariant 6, verrouillé par
  `stokesforge-render-egui/tests/feathering.rs`) : couper la frange
  d'anticrénelage change la tessellation, pas les `Shape`, et la tessellation a
  lieu à chaque image de toute façon. L'y ajouter ferait reconstruire toute la
  figure à chaque pression de bouton — payer cher exactement au moment qu'on
  voulait alléger. `view_generation`, lui, **doit** y être : un zoom change la
  géométrie tracée, pas seulement sa finition.
- **Une sélection n'entre ni dans la clé ni dans le pipeline.** Les poignées
  d'une série sélectionnée (#98 : carrés de `HANDLE_SIDE` tous les
  `HANDLE_SPACING` pixels d'affichage le long du tracé, encre
  `pipeline::figure_ink`, jamais de surlignage qui masquerait le style) sont
  placées et dessinées par `stokesforge_render::series_selection`
  (`paint_handles`, générique sur le backend), puis posées par `GraphView`
  **par-dessus** la figure en cache : sélectionner ne reconstruit rien, et
  l'export ne les voit pas. Un futur repère de sélection suit ce motif.
- **Zoom et pan ne sont pas des mutations du modèle.** `ViewState`
  (`stokesforge_render::view`, sans egui, #262) vit sur `GraphView`, à côté du `PlotSpec`, et n'y touche jamais : personne ne veut
  annuler un zoom par Ctrl+Z. La règle « toute mutation passe par une
  commande » ne s'applique pas au cadrage de vue — et le panneau de
  configuration des graphes n'a, et n'aura, aucun champ de navigation.
  Le pan du graphe se fait par Ctrl + glisser ou clic milieu + glisser (jamais
  au glisser simple, #318) ; le geste est décidé à la pression (`decide_nav`) et
  le feathering ne se coupe que pendant un geste réel (invariant 6).

**Une règle en `f64` ne s'écrit pas dans `stokesforge-render-egui`** (#254,
#262). Cadrage d'un geste, zoom, pan, point le plus proche du curseur :
`stokesforge-render` (`view`, `interact`), curseur en `(f64, f64)`. Le crate
égui n'en garde que l'adaptateur `Pos2` ↔ `(f64, f64)` et la lecture des
entrées.

**Un écran qui se peint sans passer par `GraphView`** (l'aperçu de la fenêtre
d'analyse, #261) garde sa **peinture** dans `stokesforge-app` (`rect_filled`,
`Shape::line`, couleurs de `theme`) mais n'écrit **aucune géométrie** :
pixel ↔ données, zoom, cadre initial, aimantation et graduations viennent de
`stokesforge-render` (`PanelTransform::new`, `axis_frame`, `snap_to_curve`,
`scale::axis_ticks`). Un nouvel écran de ce genre en suit la règle : l'app
convertit en `Pos2` et dessine, `render` calcule. Jamais un second algorithme
de pas ou de format de graduation (invariant 5).

## Ordre de tracé d'un panneau

Il n'est pas arbitraire ; chaque étape dépend de ce que la précédente a appris :

1. **préparer** chaque série en géométrie (`series`) — l'échelle de son axe est
   appliquée ; le cadrage et le tracé liront ensuite **la même** géométrie ;
2. **cadrer** les axes sur cette géométrie ;
3. **construire** la zone de tracé — la largeur en pixels n'existe que là ;
4. **décimer** (`decimate`, LTTB), qui a besoin de cette largeur ;
5. **peindre** en pixels (`paint`) ;
6. **la légende** en dernier, mesurée par le même chemin que les axes.

**Calques superposés (#167)** : un `PanelSpec` avec `overlay: Some(OverlaySpec)`
recouvre le dernier calque ordinaire de sa case ; le pipeline réserve
l'**union** des aires de graduation de la case (`CellPlan`) pour que tous ses
calques partagent la même zone de tracé. Un calque s'identifie par son
**indice dans `PlotSpec::panels`** (`DrawnPanel::index`), jamais par son rang
dans `DrawnPlot::panels` (qui saute les calques trop petits) ni par
`(row, column)`. Zoom et déplacement cadrent tous les calques de la case ; la
molette ou le glisser sur un axe ne cadrent que cette échelle. Détail :
`docs/rendu.md` (paragraphe « Calques superposés ») ; tests `geometrie.rs`,
`stokesforge-render-svg/tests/calques.rs`.

Deux décisions déjà arbitrées, à ne pas rejouer : l'échelle log transforme
**les valeurs**, pas le type de coordonnée (sinon la fonction de tracé
existerait en huit monomorphisations) ; le rognage se fait **en pixels**, après
projection, parce qu'une droite en coordonnées de données n'en est pas
forcément une sur un axe log.

## Tests — et pourquoi le témoin est la méthode la plus importante du dépôt

| Test | Ce qu'il tient |
|---|---|
| `stokesforge-render/tests/mesure.rs` | la source unique de mesure (invariant 4) |
| `stokesforge-render-egui/tests/accord.rs` | écran et export placent les mêmes primitives aux mêmes pixels |
| `stokesforge-render-egui/tests/formes.rs` | les conventions d'ancrage propres à chaque backend |
| `stokesforge-render-egui/tests/feathering.rs` | le feathering hors de la clé de cache |
| `stokesforge-render-svg/tests/frontiere.rs` | l'export ne voit ni `egui`, ni `eframe`, ni `winit` — donc se teste sans serveur graphique |
| `stokesforge-render/src/view.rs`, `src/interact.rs` (modules `tests`) | cadrage d'un geste (zoom, pan, échelle log, axe inversé, calques superposés, axe seul) et point le plus proche (#262), sans egui |
| `stokesforge-render/src/{interact,scale}.rs` (modules `tests`) | géométrie de l'aperçu d'analyse (#261) : `PanelTransform::new`, cadre initial, `snap_to_curve`, `axis_ticks` |
| `stokesforge-render/tests/geometrie.rs`, `stokesforge-render-svg/tests/calques.rs` | placement des calques superposés et de leurs axes en haut / à droite (#167) |

Un test qui compare deux chemins peut passer parce que les deux sont justes —
ou parce que **les deux sont faux de la même façon**. `mesure.rs` et
`accord.rs` portent donc des **témoins**, des cas qui doivent être
**différents** :

- la même figure tracée avec le `SVGBackend` **nu** (celui qui retombe sur
  l'heuristique par octets) doit se placer **ailleurs** ;
- pour chaque fonctionnalité ajoutée, **une variante du graphe qui la change
  doit produire une figure différente**.

Donc : ajouter une fonctionnalité de tracé, c'est ajouter son témoin à
`accord.rs`. Sans lui, une enveloppe qui déléguerait `estimate_text_size` par
erreur passerait pour correcte. Si vous touchez à ces tests, **lisez leurs
témoins d'abord**.

## Typst (dépendance ferme)

Typst n'est plus une feature (issue #29) : il est toujours compilé, et
`cargo test --workspace` / `cargo clippy --workspace --all-targets` couvrent ses
chemins, sans commande ciblée. **Tout titre, titre d'axe et entrée de légende
passe par Typst, les graduations jamais** (issue #32) : le pipeline pose
`formula::rich_style` sur ces textes, et les backends aiguillent sur
`formula::is_rich` — jamais sur le contenu (`$`). Un nouveau texte de figure
(annotation…) choisit son style dans le pipeline, pas dans un backend :
`rich_style` pour un texte saisi (markup Typst), `literal_style` pour un texte
repris d'une colonne ou généré par le rendu, affiché tel quel (`formula::source`
l'échappe dans la branche Typst, issue #33). Ce qui
reste optionnel est le chemin du backend : `collect_shapes` et
`render_to_string` tracent tout en texte brut (le **témoin** des tests) ;
`collect_shapes_with_typst`, `render_to_string_with_typst` et `with_typst()`
passent les textes riches par Typst. `GraphView` (donc l'application)
passe **toujours** par Typst, avec un `FormulaCache` borné (`FORMULA_CACHE_LIMIT`)
et des polices résolues une fois (`TypstScreen::with_fonts`). La couleur du
texte (`style.color()`, via `formula::ink`) entre dans la compilation et dans la
clé du cache : une formule n'est jamais teintée après coup (issue #30). Agrandie avec
la fenêtre, une formule garde son maillage mais change de texture, rastérisée de
nouveau à la taille affichée dans `scaled` (issue #31) : ne pas mettre le
rectangle affiché dans la clé du cache principal. L'export (SVG, PDF, PNG)
y est branché (#34) ; le texte Typst d'un SVG reste du texte `<text>` par défaut
(`SvgTextMode`, émetteur `typst_text.rs`, repli en contour **par glyphe** — #162),
les contours de `typst-svg` étant l'option « Contours » : ne plus supposer que
Typst sort des contours. Une formule qui ne compile pas s'affiche
en texte brut, `$…$` compris — le graphe ne disparaît pas (invariant 10).

## Aperçu dans la vue, une seule écriture dans le modèle

Un geste continu sur la figure (glisser un texte libre, éditer sur place) ne
modifie pas le modèle à chaque image : l'aperçu est posé sur la vue
(`GraphView::set_annotation_preview`, appliqué au clone du spec comme le zoom,
compteur dans `view_generation`), et l'application envoie **une** action à la
fin — une entrée d'historique. Côté application, les signaux de la vue sont
traduits par `app::graph_window::annotation_interactions` et les éditeurs sur
place vivent dans `app/graph_text_edit.rs` (validation :
`commit_annotation_editor`, testée sans fenêtre). `hold_preview` garde l'aperçu jusqu'à ce que le
modèle l'ait rattrapé, sans image de l'ancienne position entre les deux
(issue #35). Un futur geste du même genre suit ce motif. Un champ de saisie
posé sur la figure se place par une fonction pure (`annotation_field` : centre
du rendu, corps × échelle d'affichage, jamais tourné) et devient transparent
quand il n'a pas le focus, pour que l'aperçu reste visible (issues #36, #37).

Un titre ou la légende déplacé (issue #53) suit le même motif
(`set_text_preview`) ; sa position est une `TextAnchor` du modèle, tracée par le
pipeline (donc à l'export), et **la mise en page ne dépend pas de cette
position** : la bande du titre et l'aire de graduation restent réservées, vides.

## Checklist

- [ ] `docs/rendu.md` lu ; `stokesforge-render` chargé avant le backend
- [ ] la fonctionnalité est dans `stokesforge-render`, générique sur le backend
- [ ] aucune mesure de texte hors de `stokesforge_render::text`
- [ ] témoin ajouté à `accord.rs` pour toute fonctionnalité nouvelle
- [ ] clé de cache inchangée sauf raison explicite ; feathering toujours dehors
- [ ] zoom/pan restés dans `ViewState`, hors du `PlotSpec`
- [ ] si Typst est touché : le témoin en texte brut reste intact, et un titre de formule inchangé n'est pas recompilé (`cache_session.rs`)
- [ ] `fmt` + `clippy` + `cargo test --workspace` verts

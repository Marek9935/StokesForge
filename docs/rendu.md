# Rendu

Trois crates, et une règle qui explique leur découpage : **une fonctionnalité
de tracé n'entre jamais dans un seul backend** (invariant 5).

```text
plotyx-render          tout le rendu, générique sur DrawingBackend
   ↑                        ↑
plotyx-render-egui    plotyx-render-svg
  (écran)              (SVG → PDF, PNG)
```

Les deux backends ne diffèrent que par **la façon de poser des pixels**. Tout
le reste — types de série, échelles, primitives, légende, décimation, rognage,
formules — est dans `plotyx-render`.

## `plotyx-render` — le socle commun

| Module | Ce qu'il porte |
|---|---|
| `text` | **la source unique de mesure de texte** (invariant 4) |
| `fonts` | choix de police, trois paires embarquées ou une police système |
| `pipeline` | la suite d'appels Plotters, écrite une fois |
| `series` | les six `PlotType` traduits en géométrie |
| `scale` | les échelles d'axe, dont la logarithmique |
| `paint` | les primitives en pixels : traits, marqueurs, barres, moustaches |
| `clip` | rognage géométrique d'une polyligne contre le rectangle de tracé |
| `legend` | la légende, mise en page à la main |
| `decimate` | LTTB |
| `interact` | correspondance pixel ↔ données d'un panneau tracé |
| `hit` | composant (titre, zone de tracé, axe, titre d'axe) sous un pixel, sur la géométrie allouée par Plotters (`DrawnPlot::title_height`, `DrawnPanel::axis_areas`) |
| `color` | lecture des couleurs `#RRGGBB` du spec |
| `formula` | Typst (feature `typst`) |
| `sample` | deux graphes de démonstration |

### L'ordre dans lequel un panneau se trace

Cet ordre n'est pas arbitraire ; chaque étape dépend de ce que la précédente a
appris.

1. **préparer** chaque série en géométrie (`series`) — l'échelle de son axe est
   appliquée, les histogrammes sont classés. Le cadrage et le tracé liront
   ensuite **la même** géométrie, donc ne pourront pas la lire différemment ;
2. **cadrer** les axes sur cette géométrie ;
3. **construire** la zone de tracé — c'est seulement là que la largeur en
   pixels existe ;
4. **décimer** (`decimate`), qui a besoin de cette largeur ;
5. **peindre**, en pixels (`paint`) ;
6. **la légende** en dernier, par-dessus, et mesurée par le même chemin que les
   axes.

Rien de tout cela ne connaît le type des colonnes tracées : le pipeline reçoit
des `ResolvedValues`, c'est-à-dire des `f64`, et il écarte déjà les points non
finis. C'est ce qui a permis aux **colonnes à types mixtes** d'entrer sans
qu'une ligne de rendu bouge : `plotyx_core::Column::numeric_view` donne `NaN`
pour une cellule texte ou vide, et une colonne mixte trace donc exactement ses
cellules numériques (`plotyx-render/tests/geometrie.rs`,
`une_colonne_mixte_ne_trace_que_ses_cellules_numeriques`). L'endroit correct
pour ce genre de traduction est toujours celui-là — en amont, une fois — et
jamais un backend (invariant 5).

### Deux pièges Plotters, à connaître avant de toucher au rendu

`plotters` est pris **sans ses fonts** (`default-features = false`).

- Un `DrawingBackend` qui ne surcharge pas `draw_text` **panique** à la
  première graduation.
- Un qui ne surcharge pas `estimate_text_size` mesure le texte au **nombre
  d'octets UTF-8** — silencieusement.

Et l'angle mort qui suit : `configure_series_labels` et `MultiLineText`
mesurent le texte **eux-mêmes**, par `FontData::estimate_layout`, donc par
l'heuristique naïve, **jamais** par `estimate_text_size`. C'est pour cette
raison précise que `legend` est écrite à la main plutôt que déléguée.

Où la mesure entre dans la géométrie, concrètement : `DrawingArea::titled` —
appelé dès qu'un graphe ou un panneau a un titre — demande la hauteur du titre
à `estimate_text_size`, puis fait **descendre le haut de la zone de tracé** de
cette hauteur. Une mesure fausse de quelques pixels déplace la zone entière,
axes compris, sans lever la moindre erreur.

### Pourquoi le tracé descend en pixels

Trois besoins ne passent pas par `ChartContext::draw_series` :

- **les styles de trait** — un tiret se mesure en pixels, pas en unités de
  données, et `DashedLineSeries` ne connaît qu'un motif à deux temps ;
- **les marqueurs** — Plotters n'a d'élément tout fait ni pour le carré, ni
  pour le losange, ni pour la croix droite ;
- **la légende**, pour la raison ci-dessus.

Descendre en pixels ne coûte ni fidélité ni accord : c'est le même code, dans
le pipeline commun, qui calcule les coordonnées entières ; les deux backends ne
voient qu'un `draw_path`, un `draw_rect` ou un `draw_circle` déjà résolu.

### L'échelle logarithmique transforme les données, pas le type

Plotters sait faire un axe log (`LogRange`) et ce n'est **pas** ce qui est
fait : `LogRange` change le *type* de la coordonnée, donc le type du
`ChartContext`. Avec deux axes, deux échelles et un axe secondaire, la fonction
qui trace un panneau existerait en **huit** monomorphisations — exactement la
divergence que le pipeline unique existe pour empêcher, rentrée par la porte du
système de types.

Ici : un seul type de coordonnée, `f64` linéaire, et l'échelle appliquée **aux
valeurs**. Seuls deux endroits savent qu'il y a une échelle : l'entrée
(`forward`) et l'étiquetage (`format_tick`). Le prix, dit en une ligne : les
graduations sont des **décades**, pas les graduations mineures que `LogRange`
sait poser.

Sur une échelle log, zéro et les négatifs n'ont pas d'image : ils sont retirés
du tracé, comme le sont déjà les `NaN`.

### Rognage : en pixels, pas en données

Un axe logarithmique n'est linéaire que dans l'espace transformé ; une droite
en coordonnées de données n'y est donc pas forcément une droite — mais elle
**redevient** une droite dès qu'elle est projetée en pixels, la projection
pixel ↔ transformé étant toujours affine. Une intersection segment/rectangle
est une opération sur des droites : elle doit se faire après la projection.

Algorithme : Liang-Barsky sur un rectangle aligné aux axes (le rectangle de
tracé n'est jamais incliné).

### LTTB : ce qu'il règle, et ce qu'il ne règle pas

C'est écrit ici plutôt que dilué ailleurs, parce que la mesure contredit
l'intuition :

| séries | points/série | total | tessellation | ce que le LTTB retire |
|---|---|---|---|---|
| 50 | 6 000 | 300 k | 18,3 ms | ~73 % des points |
| 500 | 600 | 300 k | 22,7 ms | **rien** |

600 points étalés sur 1 600 px sont déjà sous-échantillonnés. Le cas de charge
du document — 500 spectres de 600 points — tient sa charge du **nombre de
séries**, pas de leur longueur ; le levier qui le couvre est le *feathering*,
pas celui-ci. Le LTTB sert **l'autre cas** : quelques séries très longues, celui
de l'acquisition.

*Largest Triangle Three Buckets* choisit dans chaque tranche le point formant
le plus grand triangle avec le point retenu à gauche et la moyenne de la
tranche de droite : concrètement, **il garde les pics**, là où « un point sur
N » les perd.

### Polices

Trois paires texte + Math embarquées (Libertinus, Fira, Noto — SIL OFL 1.1,
provenance dans `vendor/fonts/README.md`), ou une police système par son nom
avec repli sur Noto.

`text::font_definitions` reste la source unique de mesure : ce module lui donne
un **paramètre** (`FontChoice`) au lieu de la figer. D'où un unique point
d'entrée `fonts::resolve`, et non deux fonctions « pour l'écran » / « pour
l'export » — un appelant qui changerait de police d'un seul côté reproduirait
exactement le défaut que `text.rs` existe pour empêcher.

## `plotyx-render-egui` — l'écran

`GraphView` est ce que l'UI tient réellement : un cache, plus la mise en
relation d'un `Snapshot` de session et d'un nœud `Graph`.

### Le backend collecte, il ne peint pas

`EguiBackend` ne tient pas de `Painter` : il écrit dans un `&mut Vec<Shape>`
qu'on lui prête. egui est en mode immédiat — il repeint à chaque mouvement de
souris. Un backend qui dessinerait dans un `Painter` rejouerait tout le
pipeline Plotters à **chaque image**, même immobile. Avec un collecteur, le
résultat se met en cache et une image sans changement coûte un
`painter.extend(...)`.

### La clé de cache a exactement quatre termes

`(revision, rect, pixels_per_point, view_generation)`

- **`revision`** — le compteur de `plotyx-session` ; le renderer devient un
  client de plus du même protocole, il n'y avait rien à inventer ;
- **`rect`** — le rectangle logique passé au pipeline ; un appel direct peut
  changer sa taille, alors que l'application garde `DEFAULT_FIGURE_SIZE` et
  transforme les formes en sortie pour redimensionner la fenêtre sans
  redisposer les axes et les textes ;
- **`pixels_per_point`** — l'exception qu'`epaint` documente lui-même :
  `Shape::Text` en dépend et doit être reconstruit quand il change ;
- **`view_generation`** — le compteur de `ViewState`, qui monte à chaque zoom,
  pan ou rectangle élastique commis. Le zoom **change la géométrie tracée**,
  donc doit invalider. Un compteur plutôt que les bornes : un entier suffit à
  invalider, la valeur du cadrage n'a pas à transiter par là.

Ce qui n'y est délibérément **pas** : le *feathering* (invariant 6).

### Zoom et pan ne sont pas des mutations du modèle

Un zoom molette est un **cadrage de vue éphémère**. Le faire passer par une
commande annulable en ferait une entrée d'historique au même titre qu'un
renommage — et personne ne veut annuler un zoom avec `Ctrl+Z`.

`ViewState` vit donc à côté du modèle, sur `GraphView`, et ne touche jamais au
`PlotSpec` stocké dans le `Tree`. Le mécanisme réutilise ce qui existait déjà :
`AxisSpec::has_explicit_range` fait primer un cadrage explicite sur les données
calculées ; `ViewState::apply` pose `auto_range = false` et les deux bornes sur
un **clone** du spec, juste avant de résoudre. Seul le déclenchement est
nouveau.

Un panneau est identifié par `(row, column)`, **pas par son indice** :
`DrawnPlot::panels` omet les panneaux trop petits pour être tracés (`MIN_CELL`),
ce qui décale les indices.

Gestes câblés : molette (zoom), glisser principal (pan), glisser secondaire
(rectangle de zoom élastique), double-clic (réinitialisation), plus réticule et
désignation de point (`PointHit`, rayon `HIT_RADIUS`).

### Le levier d'anticrénelage

Le *feathering* — la frange d'un pixel qu'`epaint` ajoute à chaque trait — pèse
**78 % du coût de tessellation** : 22,1 ms par image avec, **4,88 ms sans**, sur
le même graphe de 300 000 points. C'est le levier le plus rentable du chantier,
et il n'était pas dans le plan avant la mesure.

Il se coupe à l'entrée du glisser et se rétablit au relâchement : le crénelage
ne se voit pas sur une image qui bouge, et c'est exactement pendant le
déplacement que les 22,1 ms font mal. Ce n'est donc pas un réglage exposé à
l'utilisateur, c'est un **état du rendu**.

`plotyx-render-egui::feathering` est la seule exception vivante à l'invariant 5,
et elle n'en est pas vraiment une : `TessellationOptions` est un réglage de la
pile egui, en aval de la collecte des `Shape`, et un SVG ne triangule rien. Il
n'y a rien à mettre de l'autre côté.

C'est une fonction libre prenant le `Context` — et non une méthode de
`GraphView` — parce que `TessellationOptions` vaut pour **toute la fenêtre** :
deux vues qui le poseraient chacune poseraient deux valeurs contradictoires.

## `plotyx-render-svg` — l'export

> « L'export est au `PlotSpec` ce que la persistance était au modèle : la seule
> chose qui prouve que la couche neutre est réellement neutre. »

C'est pourquoi il est écrit **en même temps** que le backend écran, pas après.

Ce crate ne connaît ni `egui`, ni `eframe`, ni `winit` (`tests/frontiere.rs`),
et mesure pourtant le texte exactement comme l'écran — la mesure vient
d'`epaint`, tiré par `plotyx-render`. Conséquence pratique : **le rendu se teste
sans serveur graphique**, donc en intégration continue.

`MeasuredSvgBackend` est une **enveloppe** autour de `plotters-svg::SVGBackend`,
pas un backend réécrit : ~800 lignes maintenues en amont font déjà tout ce qu'il
faut ; il ne leur manque qu'`estimate_text_size`, et c'est précisément celle qui
compte.

PDF et PNG ne sont **pas des backends** : ce sont des conversions de la
`String` que `render_to_string` rend déjà — `usvg` + `svg2pdf` pour l'un,
`usvg` + `resvg` pour l'autre. Le PNG prend un facteur d'échelle multiplicatif,
pas une taille de pixels : le document reste vectoriel jusqu'à la
rasterisation.

Une décision non annoncée par les plans, prise en écrivant le PDF : **charger
les polices système à la conversion**. Sans `fontdb`, `usvg` ne signale aucune
erreur et **abandonne silencieusement tout le texte** — mesuré : 599 opérateurs
`Tj` deviennent zéro. Un export sans un seul titre ni libellé d'axe n'est pas un
défaut de fidélité mineur, c'est un graphe muet. Repli sur le filet Noto
embarqué si aucune police système ne convient ; le PNG réutilise la même
politique telle quelle.

## Typst (feature `typst`, R8)

Un `&str` est une formule Typst **si et seulement s'il contient `$`**, le
délimiteur natif du mode mathématique. Décision prise dans `formula.rs` : pas de
nouveau champ dans `plotyx_plot`, donc **pas de migration du format `.plx`**.
S'applique à `PlotSpec::title`, `PanelSpec::title` et `AxisSpec::label`, qui
sont des `String` ordinaires.

Le partage suit la même règle que le reste :

| Où | Quoi |
|---|---|
| `plotyx-render/formula.rs` | le `World` Typst minimal, la compilation, le cache qui l'amortit entre deux images |
| `plotyx-render-egui/typst_screen.rs` | `typst-render` → RGBA → texture → `Shape::Mesh` |
| `plotyx-render-svg/typst_export.rs` | fragments `typst-svg` inlinés |

Côté export, le mécanisme mérite d'être connu avant d'y toucher : `SVGBackend`
écrit directement dans sa `String` cible et son champ est privé, donc
impossible d'injecter du XML au moment de dessiner. La parade est un
**rectangle-témoin** — une couleur unique par témoin (rouge fixé à une valeur
invraisemblable, vert et bleu encodant un compteur), posé à la place du texte,
puis substitué par `splice` une fois le document entier écrit, après
revérification de la position et de la taille.

Sans la feature, ce module n'existe pas et les deux backends replient sur
`Shape::Text` brut. Une formule qui ne compile pas ne fait jamais échouer le
rendu : le texte brut, `$…$` compris, s'affiche tel quel (invariant 10).

## Ce qu'il faut savoir avant de modifier

- **Le seul endroit du chantier où un bug est silencieux est la mesure de
  texte.** Avant d'y toucher, lire `plotyx-render/src/text.rs` et l'invariant 4.
- Le filet est `plotyx-render-egui/tests/accord.rs`. S'il change, lire ses deux
  familles de **témoins** avant : sans elles, un test d'égalité entre deux
  chemins passe aussi bien parce que les deux sont justes que parce que les deux
  sont faux de la même façon.
- Les graphes d'exemple vivent dans `plotyx-render::sample`, donc dans un crate
  de production, parce que le test d'accord (dans `render-egui`) et l'exemple
  visuel (dans `render-svg`) doivent porter sur **le même** graphe et que les
  deux crates ne peuvent pas dépendre l'un de l'autre. Le regarder :
  `cargo run -p plotyx-render-svg --example figure`.
- Les tests : `render/tests/{mesure,geometrie,frontiere}.rs`,
  `render-egui/tests/{accord,formes,cache_session,feathering,interaction,typst}.rs`,
  `render-svg/tests/{export,frontiere,pdf,typst}.rs`.

# Rendu

Trois crates, et une règle qui explique leur découpage : **une fonctionnalité
de tracé n'entre jamais dans un seul backend** (invariant 5).

```text
stokesforge-render          tout le rendu, générique sur DrawingBackend
   ↑                        ↑
stokesforge-render-egui    stokesforge-render-svg
  (écran)              (SVG → PDF, PNG)
```

Les deux backends ne diffèrent que par **la façon de poser des pixels**. Tout
le reste — types de série, échelles, primitives, légende, décimation, rognage,
formules — est dans `stokesforge-render`.

## `stokesforge-render` — le socle commun

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
| `series_selection` | série sous un pixel, sur ses pixels réellement tracés, et poignées de la série sélectionnée (#98) |
| `color` | lecture des couleurs `#RRGGBB` du spec (adaptateur de `stokesforge_plot::parse_hex`, **source unique** partagée avec le sélecteur de l'interface), et l'encre automatique contrastée avec le fond (`auto_ink`) |
| `formula` | Typst (dépendance ferme, pas une feature) : le style « texte riche » des titres, titres d'axe et légende, la compilation, le cache |
| `sample` | deux figures témoins à titres explicites |

Les titres de `sample::minimal()` et `sample::realistic()` sont conservés
pour exercer les bandes de titre et la mesure du texte dans les tests
d'accord écran/export et les bancs de rendu (#103). Ces exemples n'installent
pas un titre par défaut : les graphes créés par l'application, démo comprise,
ou par `pyst.plot` sans option `title` sont sans titre. Le pipeline ne réserve
aucune bande quand `PlotSpec::title` est vide.

### Transparence

Toute couleur du spec peut porter une opacité (`#RRGGBBAA`,
`color::rgba_or`) : traits, marqueurs, barres, texte, échelles, fond. Le
fond par défaut d'un graphe est sans couleur (`stokesforge_plot::TRANSPARENT`).
Un fond d'opacité nulle n'est **pas peint** : l'export SVG, PNG ou PDF n'a pas
de fond (`stokesforge-render-svg`, test `un_fond_sans_couleur_donne_un_png_transparent`).
À l'écran, `stokesforge-app` peint un blanc sous la figure
(`app::graph_window::display_backdrop`) :
ce blanc vit dans l'application, jamais dans ce crate, sinon il serait exporté.
Pour choisir l'encre automatique, un fond (semi-)transparent est jugé
composé sur du blanc (`color::over_white`).

### Couleurs du texte et des échelles : l'encre automatique

Le titre du graphe (`PlotSpec::title_color`), le titre de chaque axe
(`AxisSpec::title_color`) et son échelle (`AxisSpec::tick_color` : ligne
d'axe, graduations, libellés, et la grille reprise à 20 % / 10 % d'opacité)
ont leur couleur. Une chaîne vide veut dire **automatique** :
`color::auto_ink(fond)`, blanche sur un fond sombre, sombre sur un fond clair
(le choix au meilleur rapport de contraste WCAG). La même encre sert au titre
du panneau, au cadre et au texte de la légende, et au contour des barres. Un
fond sombre fait donc passer d'un coup en blanc tout ce qui est en couleur
automatique — à l'écran comme à l'export, puisque le pipeline est commun
(`stokesforge-render-svg/tests/couleurs.rs`).

Police d'une zone de texte (issues #77, #78) : la police et le corps d'un
titre, d'un titre d'axe, de la légende et d'un texte libre sont des champs de la
zone (`stokesforge_plot::TextFont`), pas du contenu. Le pipeline en tire le style
(`pipeline::font_style`, corps borné à 4-200) et la police voyage dans la
**famille du style** (`formula::rich_style_font` : `stokesforge-typst|Fira Sans`, comme
l'angle d'un texte tourné) ; `formula::source` pose alors `#set text(font: …)`
et `#show math.equation: set text(font: …)` avant le texte, les deux backends
l'ignorant. La police Math suit (`math_family_for`) : Libertinus Sans →
Libertinus Math, Fira Sans → Fira Math, Noto Sans → Noto Sans Math, une police
système → Noto Sans Math (repli). Tests : `formula.rs`
(`la_police_d_une_zone_est_posee_dans_le_source_avec_sa_math`), `geometrie.rs`
(`le_corps_d_une_zone_de_texte_change_sa_taille`), `stokesforge-render-svg/tests/typst.rs`.

Le titre et la légende se **sélectionnent** au clic (`GraphView::selected_text`,
cadre bleu comme un texte libre, issue #77) ; une entrée de légende désigne la
légende entière, qui se déplace aussi en la glissant par une entrée.

Polices dans le texte (issue #63, **remplacé** pour le choix de police par ce
qui précède — la barre ne pose plus de balise de police ; un `#text(font: …)`
saisi à la main reste lu) : un texte peut nommer sa police,
`#text(font: "Fira Sans")[…]`. Le `World` Typst (`formula::requested_fonts`)
lit ces noms dans le markup et charge la famille demandée, embarquée
(Libertinus Sans, Fira Sans, Noto Sans) sinon installée sur la machine
(`fonts::available_families`, base `fontdb` scannée une fois). Introuvable :
Typst retombe sur la police du document, le texte ne disparaît pas — mais la
figure change d'une machine à l'autre, puisque seules les polices embarquées
voyagent. Le gras vient de l'axe `wght` de Noto Sans ; l'italique de la face
italique de Noto Sans embarquée avec lui (Typst ne fabrique pas d'italique) ;
Fira et Libertinus n'ont que le régulier.

Une formule Typst suit la même couleur (issue #30) : les backends passent
`style.color()` à la compilation (`formula::ink`), qui pose
`#set text(fill: rgb(…))`. La couleur est donc **dans l'image rastérisée** ou
dans le fragment SVG, pas une teinte appliquée après coup : une couleur écrite
dans la formule elle-même (`#text(fill: red)[…]`) reste prioritaire. Un texte
d'opacité nulle ne dessine rien, formule comprise.

`configure_mesh` de Plotters n'a qu'un style de titre d'axe et qu'un style de
ligne d'axe pour les deux axes principaux : l'axe du bas et celui de gauche
sont donc tracés en **deux passes**, chacune coupant l'autre axe
(`pipeline::AxisColors`). La géométrie n'en dépend pas.

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
qu'une ligne de rendu bouge : `stokesforge_core::Column::numeric_view` donne `NaN`
pour une cellule texte ou vide, et une colonne mixte trace donc exactement ses
cellules numériques (`stokesforge-render/tests/geometrie.rs`,
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

## `stokesforge-render-egui` — l'écran

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

- **`revision`** — le compteur de `stokesforge-session` ; le renderer devient un
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

Gestes câblés : Ctrl+molette (zoom ; la molette seule ne fait rien), double-clic
(réinitialisation), plus réticule et désignation de point (`PointHit`, rayon
`HIT_RADIUS`). Le glisser principal dépend de l'**outil** de la vue
(`GraphTool`, issue #62, réglage de vue comme le cadrage, jamais annulable) :
`Pointer` (défaut) fait défiler la vue et déplace les textes, `ZoomBox` trace le
rectangle de zoom élastique, `Text` pose un texte au clic
(`GraphEvent::PlaceText`, puis retour au pointeur). Le clic droit n'est plus un
geste de zoom : il ouvre le menu contextuel.

### Le levier d'anticrénelage

Le *feathering* — la frange d'un pixel qu'`epaint` ajoute à chaque trait — pèse
**78 % du coût de tessellation** : 22,1 ms par image avec, **4,88 ms sans**, sur
le même graphe de 300 000 points. C'est le levier le plus rentable du chantier,
et il n'était pas dans le plan avant la mesure.

Il se coupe à l'entrée du glisser et se rétablit au relâchement : le crénelage
ne se voit pas sur une image qui bouge, et c'est exactement pendant le
déplacement que les 22,1 ms font mal. Ce n'est donc pas un réglage exposé à
l'utilisateur, c'est un **état du rendu**.

`stokesforge-render-egui::feathering` est la seule exception vivante à l'invariant 5,
et elle n'en est pas vraiment une : `TessellationOptions` est un réglage de la
pile egui, en aval de la collecte des `Shape`, et un SVG ne triangule rien. Il
n'y a rien à mettre de l'autre côté.

C'est une fonction libre prenant le `Context` — et non une méthode de
`GraphView` — parce que `TessellationOptions` vaut pour **toute la fenêtre** :
deux vues qui le poseraient chacune poseraient deux valeurs contradictoires.

## `stokesforge-render-svg` — l'export

> « L'export est au `PlotSpec` ce que la persistance était au modèle : la seule
> chose qui prouve que la couche neutre est réellement neutre. »

C'est pourquoi il est écrit **en même temps** que le backend écran, pas après.

Ce crate ne connaît ni `egui`, ni `eframe`, ni `winit` (`tests/frontiere.rs`),
et mesure pourtant le texte exactement comme l'écran — la mesure vient
d'`epaint`, tiré par `stokesforge-render`. Conséquence pratique : **le rendu se teste
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

**L'export depuis l'application** (issue #34) passe par `export::export_to_file`
(`ExportFormat` lu sur l'extension : `.svg`, `.pdf`, `.png` à un agrandissement
donné), **toujours avec Typst** : `render_to_string_with_typst`, puis
`document_to_pdf` ou `document_to_png` sur ce SVG. L'application lui passe le
spec cadré par la vue (`GraphView::framed_spec`) : on exporte le zoom courant,
sans toucher au modèle. `export::export_bytes_many` rend plusieurs formats d'une
même figure en ne la composant qu'une fois (la copie vers le presse-papiers,
#149, en demande trois) ; chaque conversion échoue seule. La taille est `DEFAULT_SIZE`, celle où l'écran met la
figure en page. Le blanc d'affichage d'un fond transparent reste à l'écran.
Le fragment Typst inliné redéclare `xmlns:xlink` (ses glyphes sont des
`<use xlink:href>`) : sans lui `usvg` refusait le document, donc tout PDF ou PNG
d'un graphe à texte Typst (`un_graphe_typst_sexporte_en_svg_pdf_et_png`).

## Typst (R8)

**Tout** titre du graphe ou d'un panneau, tout titre d'axe et toute entrée de
légende passe par Typst, avec ou sans `$` ; les graduations jamais (issue #32).
Deux moteurs de texte côte à côte ne s'alignaient pas. Le rôle est décidé par
le pipeline, dans le style : `formula::rich_style` pose la famille réservée
`stokesforge-typst`, et les deux backends aiguillent sur `formula::is_rich`, à la
mesure comme au dessin. Aucun champ n'est ajouté à `stokesforge_plot` : **pas de
migration du format `.stf`**. Le texte est donc compilé comme du **markup
Typst** : `$…$` pour le mode mathématique, `*gras*`, `\$` pour un dollar.

Pour que des libellés voisins s'alignent, `compile` fait précéder le texte d'un
**étai** invisible de largeur nulle, borné de l'ascendante à la descendante de la
police : texte ordinaire, indices et exposants simples ont tous la même boîte et
la même ligne de base (19,07 pt de haut à 14 pt pour Noto) ; une formule plus
haute (fraction, somme) agrandit la boîte sans être coupée. Une légende mixte a
donc des lignes de même hauteur.

**Textes libres** (issue #35) : tracés en dernier, par-dessus la légende,
centrés (`HPos`/`VPos::Center`) sur leur point de données projeté comme une
série (`scale::forward`, même axe secondaire) ; un point hors du cadrage, ou
≤ 0 sur un axe log, n'est pas tracé, mais un texte dont le point est visible
peut déborder sur les graduations (décision du 2026-10-01). Un angle libre ne
passe pas par Plotters (`FontTransform` : quarts de tour seulement) : il voyage
dans le style (`formula::rotated_family`, famille `stokesforge-typst@<degrés>`) et
`formula::source` enveloppe le texte dans `#stokesforge-rotate` (Typst
`rotate(reflow: true)`, l'étai dans la rotation), dont la page épouse la boîte
tournée — l'ancrage centré reste le centre du texte. `DrawnPanel::annotations`
rapporte centre, taille du texte droit et rotation : `hit` désigne le rectangle
**tourné** (`PlotComponent::Annotation`, prioritaire). Côté écran,
`GraphView` porte l'aperçu d'une édition ou d'un glisser
(`set_annotation_preview`, appliqué au clone comme le zoom, compteur ajouté à
`view_generation`), la sélection et les signaux `GraphEvent` (double-clic,
texte relâché) ; l'aperçu validé reste affiché jusqu'à ce que le modèle change
(`hold_preview`). Le champ de saisie de l'éditeur (`annotation_field`, issue
#37) est posé sur le centre et le corps de ce texte rendu, jamais tourné : un
angle libre de `DragValue` (fractionnaire, minuscule, négatif) reste une durée
Typst valide (`formula::source`, issue #36 — l'hypothèse d'un format `1e-7deg`
était fausse : le format décimal de Rust n'a pas d'exposant). Sans Typst (témoin, repli), le texte est brut et non tourné.

**Objets graphiques** (issue #76) : rectangle, ellipse, ligne, flèche
(`stokesforge_render::shapes`), tracés par le pipeline commun entre la légende et
les textes libres (étape « 6 bis »), donc identiques à l'écran et à l'export.
Les deux points sont projetés **sans** test d'appartenance au cadrage
(`Ranges::forward`) puis rognés en pixels : trait par `clip_polyline`
(contour fermé), remplissage par Sutherland-Hodgman (`clip_polygon`), pointe de
flèche omise si elle est hors cadre. L'ellipse est un polygone convexe dont le nombre de côtés suit le rayon en pixels (`ellipse_segments` : corde à moins de 0,1 px de l'arc, #91).
Le contour (rectangle, ellipse, ligne, hampe de flèche) est tracé **au sous-pixel** : Plotters n'accepte que des coordonnées entières, qui crénelaient le cercle quel que soit le nombre de côtés. Convention lue par les deux backends (`stokesforge_render::subpixel`) : points fixes ×16 et drapeau dans l'épaisseur ; l'écran décode dans `draw_path`, l'export réécrit les `<polyline>` marquées (`stokesforge-render-svg::subpixel::resolve`, appelé par chaque point d'entrée de l'export). `SubpixelPath` passe par un `CoordMapper` sans `Rect::truncate`, qui écraserait les points fixes. Un backend qui ignore la convention trace ces chemins seize fois trop grands.
`DrawnPanel::shapes` (`DrawnShape`, pixels non rognés) sert à `hit` :
`PlotComponent::Shape`, désigné **sur son trait** (ou dedans s'il est rempli)
dans la zone de tracé, sous les textes libres. Côté écran, `GraphView`
(`shape_tools.rs`) porte l'aperçu (`set_shape_preview`, même mécanisme que les
textes), la sélection, les huit poignées d'un rectangle ou d'une ellipse (deux
extrémités pour une ligne), les outils de tracé (`GraphTool::Rectangle`…) et
le signal unique `GraphEvent::ShapeChanged` au relâchement. Un déplacement sur
axes linéaires se fait **en données** (les points tracés sont arrondis au
pixel).

**Titres et légende déplaçables** (issue #53) : titre du graphe, titre de
panneau, titres d'axe et cadre de la légende portent une `TextAnchor` optionnelle
(`stokesforge-plot::text_anchor`, centre en **données** du panneau, pas bornée au
cadrage : un titre vit hors de la zone de tracé). Sans ancre, Plotters pose le
texte (bande du titre, aire de graduation, coin de la légende) ; avec une
ancre, le pipeline le trace lui-même, centré sur le point projeté
(`Projection::pixel`, sans exiger qu'il soit dans le cadrage), par-dessus les
séries — **les deux backends, donc l'export SVG/PDF/PNG**. **La mise en page ne
change pas** : un titre déplacé garde sa bande, vide (`reserve_title_band`,
même hauteur que `DrawingArea::titled`, verrouillé par un test d'égalité des
zones de tracé), et un titre d'axe déplacé garde son aire de graduation —
sinon la zone de tracé changerait au lâcher et le titre, ancré aux données,
sauterait. Une ancre non finie est ignorée (`TextAnchor::is_finite`) ; si la
projection échoue (≤ 0 sur un axe log), le texte retombe au centre de la zone
de tracé. `DrawnPanel::texts`, `DrawnPlot::title` et `DrawnPanel::legend_rows`
rapportent centre, taille et rotation de chaque texte — **approchés** pour un
texte à sa place par défaut (Plotters le pose), exacts pour un texte ancré —
et `hit::component_at` rend `PlotComponent::Text(TextTarget)` /
`LegendEntry`, avant la zone de tracé qu'ils recouvrent. Côté écran,
`GraphView::handle_text_drag` déplace le centre du texte avec le pointeur
(aperçu `set_text_preview`, même motif que les textes libres) et le
double-clic émet `GraphEvent::EditText` / `EditLegendEntry`. Sur un objet graphique, une courbe ou un axe (#113), il
émet `OpenShapeProperties` / `OpenSeriesProperties` / `OpenAxisProperties` — l'application
(`app::graph_window::annotation_interactions`) traduit les axes et courbes en
`Action::OpenObjectProperties`, comme le
double-clic d'une ligne de l'Object Manager ; ailleurs, il réinitialise le zoom.
Pour un axe, l'application lit `(row, column)` dans le `DrawnPanel` désigné
et ouvre les propriétés du calque correspondant, même si d'autres panneaux
ont été omis du rendu. Le menu contextuel conserve cette adresse au clic.

**Texte repris automatiquement** (issue #33) : un nom de série laissé vide
reprend le nom long ou le nom de la colonne, souvent importé d'un appareil
(`Abs_280 #1`). Lu comme du markup, `_` ouvrirait de l'italique et `#` du code.
La légende lui donne donc `formula::literal_style` (famille
`stokesforge-typst-litteral`), comme à la ligne « … et N autres » : la branche Typst
des backends le passe par `formula::source`, qui l'enveloppe en chaîne Typst
`#"…"` (`formula::escape`) — composé par Typst, affiché tel quel, sans
guillemets typographiques ni `...` changé en points de suspension. Le chemin
sans Typst (témoin des tests, repli) affiche le texte d'origine, jamais
sa forme échappée. Un nom **saisi** reste du markup voulu.

Le partage suit la même règle que le reste :

| Où | Quoi |
|---|---|
| `stokesforge-render/formula.rs` | le `World` Typst minimal, la compilation, le cache qui l'amortit entre deux images |
| `stokesforge-render-egui/typst_screen.rs` | `typst-render` → RGBA → texture → `Shape::Mesh` |
| `stokesforge-render-svg/typst_export.rs` | fragments `typst-svg` inlinés |

Côté export, le mécanisme mérite d'être connu avant d'y toucher : `SVGBackend`
écrit directement dans sa `String` cible et son champ est privé, donc
impossible d'injecter du XML au moment de dessiner. La parade est un
**rectangle-témoin** — une couleur unique par témoin (rouge fixé à une valeur
invraisemblable, vert et bleu encodant un compteur), posé à la place du texte,
puis substitué par `splice` une fois le document entier écrit, après
revérification de la position et de la taille.

Typst n'est plus une feature Cargo (issue #29) : `typst`, `typst-layout`,
`typst-render` et `typst-svg` (tous `=0.15.1`) sont des dépendances fermes des
trois crates, toujours compilées, et `cargo test --workspace` exerce ces
chemins. Ce qui reste optionnel est le **chemin d'exécution du backend** :
`collect_shapes` (écran) et `render_to_string` (export SVG, donc PDF et PNG qui
s'appuient dessus) tracent tout en `Shape::Text` / texte brut ;
`collect_shapes_with_typst`, `render_to_string_with_typst` et `with_typst()`
passent les formules par Typst. Le chemin en texte brut sert de **témoin** aux
tests. L'application exporte par `export::export_to_file`, donc avec Typst
(issue #34) ; `render_to_string`, `render_to_pdf` et `render_to_png` restent le
chemin brut des tests et de l'exemple `figure`.

Un texte riche qui ne compile pas ne fait jamais échouer le rendu : le texte
brut, `$…$` compris, s'affiche tel quel par `epaint`, mesure comprise
(invariant 10). C'est le seul cas où un titre ou une entrée de légende n'est
pas composé par Typst.

### Côté écran : `GraphView` passe toujours par Typst

`GraphView` (donc `stokesforge-app`) appelle toujours `collect_shapes_with_typst`. Il
résout les polices Typst **une fois**, à sa création (`TypstScreen::with_fonts`) ;
si cette résolution échoue, tout reste en texte brut. Il tient un
`FormulaCache<FormulaTexture>` entre deux reconstructions de la figure, borné par
`FORMULA_CACHE_LIMIT` (256) : au-delà, le cache est vidé **avant** la
reconstruction qui suit. Les textures ne sont donc recompilées que si le texte
d'une formule, sa taille, sa couleur (issue #30) ou le `pixels_per_point`
change, pas à chaque reconstruction.

`TypstScreen::measure` passe par ce même cache, sur **la même entrée** que le
dessin : mesurer puis dessiner une formule ne la compile qu'une fois (avant,
chaque mesure recompilait).

Deux défauts de R8, restés invisibles tant que l'application n'affichait pas
Typst, ont été vus à la première capture et corrigés :

- **le maillage cisaillait la texture** : `typst_screen::mesh` remplace les
  sommets de `Mesh::add_rect_with_uv` et doit suivre **son** ordre
  (haut-gauche, haut-droit, bas-gauche, bas-droit), faute de quoi chaque
  formule s'affiche en bruit dans une boîte pourtant juste — la boîte
  englobante ne le voit pas, le test
  `chaque_sommet_du_maillage_porte_la_texture_de_son_coin` si ;
- **la page coupait les indices et les jambages** : Typst borne une ligne de
  la hauteur des capitales à la ligne de base ; `compile` pose
  `top-edge`/`bottom-edge: "bounds"` pour que la page aille jusqu'au bas de
  l'encre (`un_indice_ou_un_jambage_nest_pas_coupe_par_la_page`). La hauteur
  d'un libellé varie donc avec son contenu ; l'alignement des lignes de base
  entre libellés voisins relève de l'issue #32.

Une formule agrandie avec la fenêtre (`paint_scaled`) reste nette (issue #31) :
voir « Agrandir un graphe sans flouter le texte » ci-dessous.

## Ce qu'il faut savoir avant de modifier

- **Le seul endroit du chantier où un bug est silencieux est la mesure de
  texte.** Avant d'y toucher, lire `stokesforge-render/src/text.rs` et l'invariant 4.
- Le filet est `stokesforge-render-egui/tests/accord.rs`. S'il change, lire ses deux
  familles de **témoins** avant : sans elles, un test d'égalité entre deux
  chemins passe aussi bien parce que les deux sont justes que parce que les deux
  sont faux de la même façon.
- Les graphes d'exemple vivent dans `stokesforge-render::sample`, donc dans un crate
  de production, parce que le test d'accord (dans `render-egui`) et l'exemple
  visuel (dans `render-svg`) doivent porter sur **le même** graphe et que les
  deux crates ne peuvent pas dépendre l'un de l'autre. Le regarder :
  `cargo run -p stokesforge-render-svg --example figure`.
- Les tests : `render/tests/{mesure,geometrie,frontiere}.rs`,
  `render-egui/tests/{accord,formes,cache_session,feathering,interaction,typst}.rs`,
  `render-svg/tests/{export,frontiere,pdf,typst}.rs`. Côté Typst :
  `cache_session.rs` (`graph_view_passe_un_titre_formule_par_typst`,
  `reconstruire_la_figure_ne_recompile_pas_un_titre_inchange`) verrouille que
  `GraphView` passe par Typst et ne recompile pas un titre inchangé ;
  `typst_screen.rs` (`mesurer_puis_dessiner_partage_une_seule_entree`,
  `chaque_sommet_du_maillage_porte_la_texture_de_son_coin`) verrouille le
  partage de l'entrée de cache entre mesure et dessin et l'ordre des sommets ;
  `formula.rs` (`un_indice_ou_un_jambage_nest_pas_coupe_par_la_page`) la
  hauteur de page ; `render-egui/tests/typst.rs`
  (`la_texture_dune_formule_prend_la_couleur_du_texte`) et
  `render-svg/tests/couleurs.rs` (`un_titre_typst_prend_l_encre_automatique`,
  `un_titre_typst_suit_title_color`) la couleur, pixels de la texture et
  fragment SVG compris ; `render-egui/tests/typst.rs`
  (`un_titre_sans_dollar_se_mesure_et_se_dessine_par_typst`,
  `ecran_et_export_placent_les_textes_typst_au_meme_endroit`,
  `une_legende_mixte_a_des_lignes_de_meme_hauteur`) et
  `formula.rs` (`les_libelles_simples_ont_tous_la_meme_hauteur`) l'aiguillage
  par rôle et l'étai (#32).

### Agrandir un graphe sans flouter le texte

`GraphView::paint_scaled` pose la figure à `DEFAULT_FIGURE_SIZE` puis la met à
l'échelle de la fenêtre. `Shape::transform` d'epaint étire le maillage d'un
texte déjà rastérisé à la taille logique : agrandi, il devient flou (issue
#18). `stokesforge_render_egui::scaled` remet donc chaque `Shape::Text` en page à la
taille affichée (police × échelle) et garde le résultat par
`(clé du cache, rectangle affiché)`. Les positions restent celles de la mise
en page logique : la mesure du texte (invariant 4) n'est pas refaite.

Une formule Typst est un `Shape::Mesh` texturé : son maillage est transformé
comme toute forme (même place, même boîte), mais désigne une **autre texture**,
la même formule rastérisée à `pixels_per_point × échelle` par
`FormulaTexture::rerender` à partir du document déjà compilé (issue #31) — pas
de recompilation, pas de nouvelle mesure. Ces textures agrandies vivent dans
`ScaledShapes`, hors du `FormulaCache` de la vue (elles ne comptent pas dans
`FORMULA_CACHE_LIMIT`), et sont reprises tant que l'échelle ne change pas. Une
image ne dépasse jamais `max_texture_side` : au-delà, la résolution est
plafonnée. Rastériser directement à l'échelle affichée pendant la collecte a été
écarté : le rectangle affiché serait entré dans la clé du cache principal, donc
tout le pipeline rejoué à chaque pas d'un redimensionnement. Tests :
`scaled.rs` (`une_formule_agrandie_est_rasterisee_a_la_taille_affichee`,
`deplacer_sans_agrandir_ne_rasterise_rien`) et `cache_session.rs`
(`un_graphe_agrandi_peint_ses_formules_a_la_taille_affichee`).

Le maillage d'une formule est en outre **calé sur la grille des pixels
physiques** (`scaled::snap_to_pixels`, issue #86) : coin haut-gauche sur un pixel
entier, boîte à la taille exacte de la texture. Posée ou étirée à une fraction de
pixel, l'image était mélangée à ses voisines par le filtrage linéaire — flou
surtout visible sur un petit texte (figure réduite), à peine sur un grand. Le
décalage reste inférieur à un pixel ; la mise en page n'est pas touchée
(`une_formule_reduite_est_calee_sur_les_pixels`). Hors du chemin `scaled`
(`paint` avec rectangle logique = rectangle affiché) le calage n'est pas
appliqué.

## Visibilité et sélection (#81–85)

Le filtre `SeriesSpec::visible` précède préparation, autoscale et légende.
Les textes libres, formes et titres masqués sont omis avec leur géométrie de
désignation. `title_visible` vaut pour le titre du graphe, du panneau et de
l'axe, avec ou sans ancre ; le contenu et la police restent mémorisés. Un
titre de graphe ou de panneau masqué ne réserve plus sa bande. Les axes
conservent leur aire de graduation quand seul leur titre est masqué. Cette
règle vit dans le pipeline partagé, donc vaut aussi en SVG, PDF et PNG.

`DrawnPanel::series_geometry` conserve les pixels des séries après décimation,
projection sur l'axe gauche ou droit et rognage (tronçons de ligne, moustaches,
marqueurs, barres et extrémité libre de chaque barre). `series_selection` les
désigne ; `GraphView` y peint la sélection sans modifier le cache de figure ni
le modèle. Les rangs restent ceux du spec, même quand une série est masquée.

Une série sélectionnée n'est **pas surlignée** (#98) : son tracé reste
intact, et de petits carrés pleins la signalent, comme dans OriginLab —
`HANDLE_SIDE` (5 px de côté) tous les `HANDLE_SPACING` (50 px), mesurés **le
long du tracé** et **en pixels d'affichage**, donc indépendants de la densité
des données, du LTTB, du zoom et de la taille de la fenêtre
(`series_selection::along_path`, départ et arrivée de chaque tronçon marqués).
Un nuage sans ligne reçoit ses carrés sur un sous-ensemble régulier de ses
marqueurs, des barres au milieu de leur extrémité libre, côté valeur
(`thinned`, même espacement) ; les moustaches n'en portent pas. Couleur :
`pipeline::figure_ink`, l'encre automatique du fond de la figure (noire sur
fond clair, blanche sur fond sombre). Le placement et le dessin
(`paint_handles`, générique sur `DrawingBackend`) vivent dans
`stokesforge-render` ; `GraphView` les appelle par-dessus la figure en cache,
avec un `EguiBackend` posé sur le rectangle affiché. Les poignées ne sont donc
ni dans la clé de cache (sélectionner ne reconstruit rien) ni dans le pipeline
(l'export SVG/PDF/PNG ne les voit pas) : la sélection est un état de vue,
comme le zoom. Tests : `series_selection.rs` (espacement, arrivée, échelle
d'affichage, nuage et barres), `geometrie.rs` (extrémité libre d'une barre
négative, moustaches hors des tronçons, encre), `render-egui/tests/interaction.rs`
(`une_serie_selectionnee_porte_des_poignees_sans_changer_son_trace`). La
sélection d'un objet graphique garde son cadre et ses poignées propres
(`shape_tools.rs`).
Object Manager utilise `series_swatch`, qui appelle la vignette de légende
partagée (`legend::series_key`), sans redessiner le style dans l'application.

`GraphView::select_annotation` / `select_text` complètent la sélection de
série et d'objet depuis l'arbre, y compris pour les contenus masqués ; une
seule nature est sélectionnée à la fois. La revalidation écarte les adresses
absentes du spec. Les propriétés et les barres de format réutilisent cette
même vue en fenêtre détachée.

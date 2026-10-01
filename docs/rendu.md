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
| `color` | lecture des couleurs `#RRGGBB` du spec (adaptateur de `plotyx_plot::parse_hex`, **source unique** partagée avec le sélecteur de l'interface), et l'encre automatique contrastée avec le fond (`auto_ink`) |
| `formula` | Typst (dépendance ferme, pas une feature) : le style « texte riche » des titres, titres d'axe et légende, la compilation, le cache |
| `sample` | deux graphes de démonstration |

### Transparence

Toute couleur du spec peut porter une opacité (`#RRGGBBAA`,
`color::rgba_or`) : traits, marqueurs, barres, texte, échelles, fond. Le
fond par défaut d'un graphe est sans couleur (`plotyx_plot::TRANSPARENT`).
Un fond d'opacité nulle n'est **pas peint** : l'export SVG, PNG ou PDF n'a pas
de fond (`plotyx-render-svg`, test `un_fond_sans_couleur_donne_un_png_transparent`).
À l'écran, `plotyx-app` peint un blanc sous la figure (`display_backdrop`) :
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
(`plotyx-render-svg/tests/couleurs.rs`).

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

## Typst (R8)

**Tout** titre du graphe ou d'un panneau, tout titre d'axe et toute entrée de
légende passe par Typst, avec ou sans `$` ; les graduations jamais (issue #32).
Deux moteurs de texte côte à côte ne s'alignaient pas. Le rôle est décidé par
le pipeline, dans le style : `formula::rich_style` pose la famille réservée
`plotyx-typst`, et les deux backends aiguillent sur `formula::is_rich`, à la
mesure comme au dessin. Aucun champ n'est ajouté à `plotyx_plot` : **pas de
migration du format `.plx`**. Le texte est donc compilé comme du **markup
Typst** : `$…$` pour le mode mathématique, `*gras*`, `\$` pour un dollar.

Pour que des libellés voisins s'alignent, `compile` fait précéder le texte d'un
**étai** invisible de largeur nulle, borné de l'ascendante à la descendante de la
police : texte ordinaire, indices et exposants simples ont tous la même boîte et
la même ligne de base (19,07 pt de haut à 14 pt pour Noto) ; une formule plus
haute (fraction, somme) agrandit la boîte sans être coupée. Une légende mixte a
donc des lignes de même hauteur.

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

Typst n'est plus une feature Cargo (issue #29) : `typst`, `typst-layout`,
`typst-render` et `typst-svg` (tous `=0.15.1`) sont des dépendances fermes des
trois crates, toujours compilées, et `cargo test --workspace` exerce ces
chemins. Ce qui reste optionnel est le **chemin d'exécution du backend** :
`collect_shapes` (écran) et `render_to_string` (export SVG, donc PDF et PNG qui
s'appuient dessus) tracent tout en `Shape::Text` / texte brut ;
`collect_shapes_with_typst`, `render_to_string_with_typst` et `with_typst()`
passent les formules par Typst. Le chemin en texte brut sert de **témoin** aux
tests. Brancher l'export sur Typst est l'objet de l'issue #34, pas encore fait :
aujourd'hui seul l'écran affiche les formules composées.

Un texte riche qui ne compile pas ne fait jamais échouer le rendu : le texte
brut, `$…$` compris, s'affiche tel quel par `epaint`, mesure comprise
(invariant 10). C'est le seul cas où un titre ou une entrée de légende n'est
pas composé par Typst.

### Côté écran : `GraphView` passe toujours par Typst

`GraphView` (donc `plotyx-app`) appelle toujours `collect_shapes_with_typst`. Il
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
#18). `plotyx_render_egui::scaled` remet donc chaque `Shape::Text` en page à la
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

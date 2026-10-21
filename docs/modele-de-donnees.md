# Modèle de données

Trois crates : `stokesforge-id` (l'identité), `stokesforge-plot` (la description d'un
graphe) et `stokesforge-core` (l'arbre, les colonnes, et la résolution qui met les
deux en présence).

Aucun des trois ne connaît d'interface graphique, et aucun ne connaît de
moteur de rendu.

## L'arbre de projet

```text
Project
├── allocateur d'identifiants  (hors de l'arbre — invariant 7)
└── Arc<Tree>                  (carte plate NodeId → Arc<Node>, une racine,
        │                          et les couleurs enregistrées du projet)
        └── Node { id, name, parent, kind }
              └── NodeKind
                  ├── Folder     { children }   dossiers et fenêtres
                  ├── Workbook   { children }   feuilles
                  ├── Worksheet  { children }   colonnes
                  ├── Column(Column)            les valeurs
                  ├── Graph      { spec, … }    un PlotSpec
                  ├── Matrix     { … }
                  ├── Note       { … }
                  └── Unknown(RawVariant)       lu d'un fichier plus récent
```

Les **couleurs enregistrées** du projet (`stokesforge_plot::SavedColors`, rangées
par usage : courbes, texte, fonds) ne sont pas un nœud mais sont portées par
`Tree` (`Tree::saved_colors`) : elles voyagent ainsi avec l'instantané de
session, le retour arrière et le fichier `.stf` sans chemin à part, et se
modifient par la commande `SetSavedColors`. Les couleurs enregistrées « pour
l'utilisateur » ne sont, elles, pas dans le modèle (voir
`docs/application/graphe.md`).

**Ligne x' (#164).** `ColumnMeta::x_prime: Option<f64>` porte, par colonne, une
abscisse propre (typiquement l'instant d'un spectre dans une cinétique). Toujours
finie ; elle s'écrit comme les autres champs de `ColumnMeta`, donc par
`Command::SetColumnMeta` (aucune commande nouvelle). `ColumnMeta` n'est plus
`Eq` (un `f64`).

**Série « ligne » (#166).** `SeriesSpec::row = Some(r)` : la série n'est plus une
colonne mais la ligne `r` de la feuille de `y` (qui ne sert que d'ancre) ; ses
points sont, pour chaque colonne de la feuille portant un x', la cellule `r` de
cette colonne à l'abscisse x' (`NaN` si la colonne est plus courte), dans
l'ordre de la feuille. `x` est ignoré. Résolue à la volée par
`stokesforge_core::resolve` (donc vivante : éditer la feuille ou un x' met la
série à jour) ; une feuille sans x' donne une série vide, la perte de l'ancre
une référence rompue. **La ligne suit les insertions et suppressions de lignes**
de sa feuille (#307) : `InsertRows` pousse `r >= at` à `r + count`, `DeleteRows`
ramène `r >= at + count` à `r - count`, dans le même delta (champ `plots`, donc
annulable d'un bloc). Une ligne `r` dans la plage supprimée ne change pas : la
série trace alors la ligne qui a pris sa place (cas ouvert, décision D1 de #307).
`DeleteCells` ne déplace aucune ligne de feuille et ne touche pas aux séries.
`NodeCopy::capture` embarque toutes les colonnes x' de la
feuille quand un graphe copié a une telle série.

**Groupe de séries (#177).** `SeriesSpec::group: Option<u32>` : les séries d'un
calque qui portent le même numéro forment un groupe. Il naît à l'**ajout
simultané** de deux séries ou plus (`PanelSpec::push_series_batch` : Plot ▸ sur
plusieurs colonnes, dépôt de colonnes ou de lignes, via `plot_columns`,
`add_series_to_graph`, `add_rows_to_graph`) ; une série ajoutée seule n'en a pas.
Le numéro n'a de sens que dans son calque (`next_group_id`, `group_members`,
`group_ids`). Un groupe n'est **pas** un nœud ni une commande : c'est un champ du
`PlotSpec`, modifié par `EditPlotSpec`/`SetPlotSpec` comme le reste. Son style
commun n'est pas stocké à part : la fenêtre des séries l'écrit dans chaque
membre (une série réglée seule ensuite garde son exception). Le nœud de
l'Object Manager sous un calque (`VisibilityTarget::Group`, ancien « Plot Group »,
affiché « Series ») liste toutes les séries du calque ; chaque groupe de séries y
est un nœud parent (voir ci-dessous).

**Nuancier d'un groupe (#213).** `PanelSpec::group_gradients`
(`BTreeMap<u32, Vec<String>>`) retient les arrêts du dernier nuancier appliqué
à un groupe (`set_group_gradient`) ; sans entrée, aucun nuancier n'a été choisi.
Une série qui rejoint le groupe (`set_series_group`) est recolorée, dissoudre
ou supprimer le groupe oublie l'entrée. Ni le nom ni la provenance du nuancier
ne sont retenus : la fenêtre des séries le reconnaît par ses arrêts.

**Nom, ordre et gestes d'un groupe (#197).** `PanelSpec::group_names`
(`BTreeMap<u32, String>`) donne un nom à un groupe ; sans entrée, le nom
automatique « Group n » (`object_manager::group_title`, partout où un groupe est
nommé). Un nom n'a de sens que pour un groupe qui existe : `group_label`
l'ignore sinon, et `next_group_id` ne réemploie pas le numéro d'un nom resté
seul, donc un nom ne passe jamais à un groupe neuf. **Un groupe est toujours
contigu** dans `PanelSpec::series` (l'ordre de tracé et de légende) : les
gestes sont des méthodes de `PanelSpec`, qui rangent les séries par
`slots()` (une série seule ou un groupe et tous ses membres — la projection que
l'Object Manager et la fenêtre des séries se partagent) : `set_series_group`
(entrer à la suite des membres, sortir juste derrière le groupe, en créer un),
`dissolve_group` (les séries restent, style gardé), `remove_group`,
`rename_group`, `move_series` (reste dans son groupe, saute un groupe entier),
`move_group`, `apply_group_gradient`. Un groupe n'est toujours pas un nœud ni
une commande : l'interface passe par `EditPlotSpec` (motifs `RenameGroup`,
`DissolveGroup`, `RemoveGroup`, `ReorderGroup`, `ReorderSeries`), `pyst` par
`SetPlotSpec` (`group_series` rassemble des séries en un groupe neuf, #196 ; surface : `docs/python-et-script.md`, § Groupes de séries).

**Nuanciers (#178).** `stokesforge_plot::gradient` : `GRADIENT_PRESETS` (bleu →
rouge, noir → rouge…) et `gradient_colors(arrêts, n)`, couleurs `#RRGGBB`
régulièrement espacées entre le premier et le dernier arrêt, quel que soit `n`.
Rien n'est stocké au moment d'appliquer : choisir un nuancier (ou changer un arrêt) écrit
aussitôt les couleurs dans les séries du brouillon (#213). Un nuancier **personnalisé** s'enregistre sous un nom :
`SavedColors::gradients` (`SavedGradient { name, stops }`, bornés comme les
couleurs, `with_gradient_added` remplace par le nom, `with_gradient_removed`),
donc dans le projet (`SetSavedColors`, annulable, `.stf`) ou pour l'utilisateur
(`couleurs.json`, hors historique), par `Action::SaveGradient` /
`RemoveSavedGradient`.

Les listes d'enfants vivent **dans** la nature du nœud : une colonne, un
graphe, une matrice ou une note n'en a pas, et c'est garanti à la compilation
plutôt que par une vérification.

**Fenêtre** (*window*) = classeur, graphe, matrice ou note — un nœud
affichable en sous-fenêtre. Leurs noms sont uniques dans leur **dossier parent** (#105). Un chemin
canonique inclut ce dossier ; un nom seul est refusé s’il est ambigu.

### L'arbre est persistant, au sens fonctionnel

Muter un arbre, c'est `Arc::make_mut` sur l'arbre — ce qui clone la carte, des
paires de 16 octets, **pas les nœuds** — puis `Arc::make_mut` sur les seuls
nœuds touchés. Un client qui détient un ancien `Arc<Tree>` voit donc un
instantané figé, et tous les nœuds non touchés restent physiquement partagés
entre les deux versions.

C'est le mécanisme qui rend l'undo de structure gratuit : garder l'arbre
d'avant et celui d'après coûte un pointeur.

Les mutations de `Tree` sont `pub(crate)` : `Project` est la seule façade
publique de mutation. Chacune **valide tout avant d'écrire quoi que ce soit**,
pour qu'un échec laisse l'arbre inchangé.

**À lire** : `stokesforge-core/src/tree.rs`, `src/node.rs`, `src/project.rs`.

### Noms et chemins

Un nom valide est non vide, sans espace de bordure, d'au plus 255 caractères
Unicode, et sans caractère réservé (`[`, `]`, `!`, `/`), de contrôle, ou marque
de formatage/direction invisible — `U+202E` RIGHT-TO-LEFT OVERRIDE retournerait
visuellement tout chemin qui le cite.

La grammaire de chemins est celle d'Origin : `[Book]Sheet!Col`, avec dossiers
optionnels en préfixe (`/Manips/[Book1]Sheet1!ColA`). Une seule
implémentation dans le dépôt — `Tree::resolve` — et c'est celle que le pont
Python appelle aussi.

**À lire** : `stokesforge-core/src/name.rs`, `src/path.rs`.

## Les colonnes

Une colonne est un vecteur de valeurs de longueur libre, plus ses métadonnées.
Son **nom vit dans le nœud**, pas dans la colonne.

Trois décisions structurent le module :

- **Valeur manquante** — `f64::NAN` pour `Numeric` et `DateTime`, `None` pour
  `Text` (distinct de `Some(String::new())`, qui est une chaîne vide
  délibérée), `MixedCell::Empty` pour `Mixed`.
- **Buffers contigus, jamais chunkés**, partagés derrière un `Arc` et mutés
  par `Arc::make_mut`. C'est ce qui permet au pont Python de tenir une vue
  numpy **sans copie** tant qu'il détient un clone de l'`Arc`.
- **Compteur de modification** (`Column::version`) : savoir qu'une colonne a
  changé sans relire ses valeurs. C'est aussi la condition optimiste que
  `Command::SetCells { expected_version }` vérifie, et qui ferme le TOCTOU
  entre l'instantané qu'un script a lu et la commande qu'il envoie.

Types : `Numeric`, `DateTime` (timestamp POSIX en secondes, affiché et édité en ISO 8601 par `datetime::format_datetime`, #218), `Text`, `Mixed`. Rôles de tracé : `X`, `Y`
(défaut), `X2`, `Y2`, `Z`, `XError`, `YError`, `Label`, aucun. `X2`/`Y2` (#167)
alimentent un **second calque superposé** (`PanelSpec::overlay`,
`OverlaySpec { x_top, y_right }`) : chaque Y2 s'apparie au X2 le plus proche à
sa gauche, comme Y avec X (`stokesforge_core::compose::series_from_columns`, `PlannedSeries`).
Un calque identifie par son **indice** dans `PlotSpec::panels` ; `(row, column)`
ne sert qu'à la grille, plusieurs calques pouvant partager une case.

**À lire** : `stokesforge-core/src/column.rs`.

### `Mixed` — le type porté par la cellule

Les trois premiers types disent « toute la colonne est ainsi ». `Mixed` dit
« chaque ligne décide » : son buffer est un `Vec<MixedCell>`, et un
`MixedCell` est indépendamment `Empty`, `Number(f64)` ou `Text(String)`.

Le besoin est celui d'un import réel. Une colonne de mesures qui porte
`1,2 / 1,3 / n/a / 1,5` n'avait que deux issues, toutes deux mauvaises : tout
en nombres, et le `n/a` devient une valeur manquante indiscernable d'une
mesure absente ; tout en texte, et plus rien n'est traçable.

Quatre choses à savoir avant d'y toucher.

**Les trois types homogènes restent des chemins rapides.** Faire de *toute*
colonne un `Vec<MixedCell>` coûterait 32 octets par ligne au lieu de 8 (un
`MixedCell` a la taille d'une `String` plus son discriminant) et, surtout,
retirerait le buffer `Arc<Vec<f64>>` contigu sur lequel reposent la vue numpy
sans copie du pont Python et la résolution zéro-copie d'une série. `Mixed` est
un type **de plus**, payé par qui s'en sert.

**Trois « manquantes » distinctes au stockage, une seule à l'affichage.**
`Empty`, `Number(f64::NAN)` et `Text("")` se stockent distinctement et se
comparent distinctement ; les trois rendent `is_missing()` vrai. C'est la même
asymétrie que `None` contre `Some("")` pour une colonne texte.

**Une colonne mixte est traçable.** `Column::numeric_view` en tire une vue
`f64` où les cellules texte et vides valent `NaN` — ce que le renderer écarte
déjà. Le résultat est qu'une colonne mixte trace exactement ses cellules
numériques, et qu'aucun backend n'a rien appris (invariant 5). C'est la
**seule** lecture du module qui copie : un `Vec<MixedCell>` n'est pas un
`Vec<f64>`, il n'y a rien à partager. Le coût est `O(n)` par résolution et par
colonne mixte *référencée par une série*, jamais par colonne du projet.

**Les conversions sont destructrices, comme les autres, et `Text → Mixed`
promeut.** Un texte qui s'analyse en nombre devient `Number` — par
`parse_f64_text`, la seule analyse du module, donc jamais en désaccord avec
`Text → Numeric`. C'est ce qui donne son intérêt au changement de type : la
colonne de l'exemple devient trois nombres et un texte.

Côté calcul, une colonne mixte n'est **pas** « non numérique » : `formula`
l'accepte et ses trous valent `NaN` ligne à ligne. Seule une colonne
entièrement texte est refusée en bloc.

**Formules par identifiant (#321).** `ColumnMeta::formula` est une
`ColumnFormula` : une suite de fragments `Text` (opérateurs, nombres,
parenthèses) et `Column(NodeId)` — la colonne lue, retenue par son identifiant,
jamais par une lettre de position. Les lettres (`A`, `B`… par position dans la
feuille) n'existent qu'aux frontières : `Tree::formula_from_letters` (saisie,
collage, `pyst`) les convertit en identifiants et refuse une lettre inconnue ou
la colonne elle-même ; `Tree::formula_letters` les rend à l'affichage, `#REF`
pour une colonne qui n'est plus dans la feuille (supprimée ou déplacée ailleurs :
elle reste stockée, Annuler la rend). Insérer, supprimer ou déplacer des colonnes
(dont les sorties d'une analyse, #320) ne change donc jamais ce qu'une formule
lit. Une formule dupliquée avec sa feuille lit les colonnes **de la copie**
(`Project::insert_copy`) ; un identifiant lu n'est jamais réattribué
(`Project::from_tree`).

## Les calculs d'analyse (#181)

`stokesforge-analysis` est le calcul pur (aucune dépendance du workspace, `NaN`
dit « vide »). `stokesforge-core` n'en prend que les types de données et fait le
lien avec l'arbre : `Tree::calculations()` (`BTreeMap<NodeId, Arc<Calculation>>`,
dans `calc.rs`). Un `Calculation` n'est **pas un nœud** : sans nom, parent ni
fenêtre, il voyage avec l'instantané, le retour arrière et le `.stf` à côté des
couleurs enregistrées, avec un `NodeId` du même allocateur (jamais réutilisé).

- Il relie une `Operation` (baseline, correction, intégrale, ajustement, pics,
  déconvolution), ses séries d'entrée (`SeriesRef`, `AuxRef`), ses colonnes de
  sortie (`outputs`) et ses résultats (valeurs, erreurs types, avertissements).
- La trace « cette colonne vient de tel calcul » se lit dans `outputs`
  (`Tree::calculation_of_column`), **jamais** sur la colonne : une copie ou un
  déplacement ne peut pas la dédoubler.
- `compute(tree, op, series, aux) -> Computed` est la fonction pure que
  partagent l'aperçu, la commande, le recalcul de la session et `pyst.analysis`.
- Péremption (`Staleness`) : `Manual` (recalcul automatique coupé), `InputMissing`
  (une entrée a disparu), `Failed` (le calcul échoue). Une sortie supprimée
  reste pendante et se relit « sortie supprimée » ; supprimer une entrée n'édite
  pas les calculs qui la citent.
- Une colonne calculée est **en lecture seule** pour les cellules, le type et la
  suppression de cellules (`CoreError::CalculatedColumnReadOnly`) ; ses métadonnées
  restent éditables, hors formule.
- Écriture : commandes ([`commandes-et-historique.md`](commandes-et-historique.md)) ;
  recalcul ciblé par empreinte, comme les formules (invariant 11) ; fichier :
  [`persistance.md`](persistance.md).

## `PlotSpec` — ce qu'il faut tracer

```text
PlotSpec
├── title, background
└── panels : PanelSpec           (grille row/column, au moins un panneau)
    ├── title
    ├── bottom / left / right / top → AxisSpec  (échelle, limites, grille)
    ├── legend : LegendSpec
    ├── series : SeriesSpec
    │   ├── y : NodeId, x / x_error / y_error : Option<NodeId>
    │   ├── row : Option<usize>   (série « ligne », #166 : voir ci-dessous)
    │   ├── plot_type : PlotType
    │   ├── line : LineSpec, symbol : SymbolSpec
    │   └── axis : SeriesAxis    (gauche ou droite)
    └── annotations : AnnotationSpec   (textes libres, issue #35)
        ├── text                  (markup Typst)
        ├── x, y + axis : SeriesAxis   (centre, en données)
        └── color, size, rotation (vide = automatique ; degrés, antihoraire)
```

Une série ne porte **aucune donnée** et **aucun nom de colonne** — seulement
l'identité (`NodeId`) des colonnes qu'elle trace. C'est ce qui fait que
renommer ou déplacer une colonne ne casse rien.

- `PlotType` : `Scatter`, `Line`, `LineScatter`, `Bar`, `Histogram`,
  `ErrorBar`.
- `AxisScale` : `Linear`, `Log`. `AxisSide` : `Bottom`, `Left`, `Right`, `Top`.
- Le style n'emprunte son vocabulaire à aucun moteur : des énumérations
  nommées (`LineStyle`, `SymbolShape`), dont la table de traduction vit dans
  le renderer.
- `AnnotationSpec` : un texte libre **ancré aux données** — son centre est le
  point `(x, y)` de l'ordonnée `axis`, il suit donc le zoom ; il n'est pas borné
  au cadrage (marges comprises, #163), seul un point hors échelle (≤ 0 en log) n'est pas tracé. `AnnotationRef { panel, index }` l'adresse dans un
  `PlotSpec` (`get`, `set` qui remplace ou ajoute, `remove`) : pas de commande
  dédiée, tout passe par `SetPlotSpec`. `PlotSpec::set_text_color` (thème)
  écrit aussi leur couleur.
- `ShapeSpec` (issue #76) : un **objet graphique** — rectangle, ellipse, ligne
  ou flèche (`ShapeKind`) — ancré aux données par deux points `(x1, y1)` /
  `(x2, y2)` (coins opposés, ou queue et pointe) de l'ordonnée `axis`.
  Couleur de trait (vide = encre automatique), épaisseur, couleur de
  remplissage (vide = aucun ; l'opacité est l'alpha de `#RRGGBBAA`).
  `PanelSpec::shapes` est **une seule liste ordonnée** : l'ordre d'empilement
  est le rang (le dernier est dessus) ; `ShapeRef { panel, index }`
  l'adresse (`get`, `set`, `remove`, `bring_to_front`, `send_to_back`). Comme
  les textes libres, aucune commande dédiée : tout passe par `SetPlotSpec`.
- `TextAnchor { x, y, axis }` (issue #53) : la position d'un titre déplacé,
  **centre en données** comme un texte libre mais non bornée au cadrage. Elle
  vit dans `PlotSpec::title_anchor`, `PanelSpec::title_anchor`,
  `AxisSpec::title_anchor` et `LegendSpec::anchor` ; `None` = la place par
  défaut. `TextTarget` (`Title`, `PanelTitle`, `AxisTitle`, `Legend`) l'adresse
  dans un `PlotSpec` (`anchor`, `set_anchor`, `text`, `set_text`) ;
  `PlotSpec::reset_text_anchors` les retire toutes. Pas de commande dédiée :
  tout passe par `SetPlotSpec`.
- `TextFont { family, size }` (issues #77, #78) : la police et le corps d'une
  **zone** de texte entière — jamais une balise dans son contenu. Vide / `None`
  = la police du graphe et le corps du pipeline. Champs `PlotSpec::title_font`,
  `PanelSpec::title_font`, `AxisSpec::title_font`, `LegendSpec::font` ; un texte
  libre garde son corps dans `AnnotationSpec::size` et reçoit `font`.
  `TextTarget::font` / `set_font` l'adressent. Pas de commande dédiée :
  `SetPlotSpec` (`Action::SetTextFont` côté application).
- **Couleur d'une zone** (#90) : `PlotSpec::title_color`, `AxisSpec::title_color`,
  `PanelSpec::title_color`, `LegendSpec::color` (vide = automatique).
  `TextTarget::color` / `set_color` les adressent ; `Action::SetTextColor`
  côté application. En édition du texte, la couleur reste une balise Typst sur
  le passage surligné.
- `AxisSpec::has_explicit_range` : un cadrage explicite prime sur les données
  calculées. C'est le levier — déjà présent, rien n'a été inventé pour lui —
  sur lequel le zoom/pan de l'écran s'appuie ([`rendu.md`](rendu.md)).

**À lire** : `stokesforge-plot/src/lib.rs` (tête), puis le module du sous-type visé.

## `resolve` — le seul point de contact

`stokesforge_core::resolve` traduit un spec contre l'arbre courant : `NodeId` →
colonne → données prêtes à tracer. Seul avec `compose` (ci-dessous), il connaît
`stokesforge-plot` ; tout le reste du crate ignore jusqu'à l'existence de
`PlotSpec`.

Deux propriétés à connaître avant d'y toucher :

- **Zéro copie.** Les colonnes sont déjà des `Arc<Vec<f64>>`. La résolution
  clone l'`Arc` — un incrément de compteur — jamais le buffer. Un
  `ResolvedValues` est une vue sur la même mémoire que la colonne,
  éventuellement tronquée au préfixe commun d'une série dont les colonnes ont
  des longueurs différentes (batterie `b16`). **Une exception, et une
  seule** : une colonne `Mixed` n'a pas de buffer `f64` à partager, sa vue est
  matérialisée (voir plus haut).
- **Une référence rompue est un état de première classe, pas une exception.**
  `ResolvedSeries` existe toujours, avec `missing` non vide et `ok()` faux.
  Rien ne plante, la série n'est pas retirée du spec, elle reste visible et
  marquée (invariant 10).

**À lire** : `stokesforge-core/src/resolve.rs` (tête).

## `compose` — fabriquer les séries d'un graphe (#257)

`stokesforge_core::compose` est le sens inverse de `resolve` : il lit le `Tree`
et fabrique des `SeriesSpec`, ou lit un `PlotSpec`. Une seule règle pour
l'interface (`AppState::plot_columns`, dans `stokesforge-app-state`, `add_series_to_graph`, `add_rows_to_graph`,
`plot_rows`, la fenêtre d'analyse), `pyst.plot` et l'annexe d'une analyse :

- **appariement** (`series_from_columns`, `x_for`) : chaque Y est apparié au X
  le plus proche à sa **gauche** dans la feuille, X non sélectionné compris ; un
  Y2 prend de même un X2 et va au second calque (`PlannedSeries::secondary`).
  Sans X à gauche, l'abscisse reste implicite (numéro de ligne) ;
- **palette** : toute série passe par `SeriesSpec::from_palette(y, rang)` (trait
  et marqueurs), le rang étant celui de la série dans son calque ; jamais
  `palette_color` écrit à la main ;
- **calques** : `overlay_base` (le calque ordinaire que recouvre un calque
  superposé), `second_layer_of` (le calque X2/Y2, `OverlaySpec::SECOND_AXES`) ;
  les calques se nomment dans `stokesforge-plot` (`OverlaySpec::SECOND_AXES`,
  `TOP_X`, `new`) ;
- **lignes** (`row_series`, `x_prime_columns`, `spec_has_x`) : le nom de la série
  est un paramètre (une phrase d'interface que ce crate ne sait pas écrire).

Rien n'y mute : l'appelant envoie le résultat par une commande annulable
(`AddGraph`, `SetPlotSpec`). Retirer une série du panneau passe par
`PanelSpec::remove_series`, comme `remove_group` pour un groupe.

## Ce qu'il faut savoir avant de modifier

- Une donnée lue par un calcul d'analyse entre dans l'empreinte du recalcul,
  comme pour les formules : voir « Recalcul des formules » dans `CLAUDE.md`.
- Ajouter une méthode `pub` mutante à `Project` sans commande correspondante
  dans `stokesforge-commands` ouvre un chemin d'écriture non annulable
  (invariant 1).
- Ajouter une dépendance à `stokesforge-core`, `stokesforge-plot` ou `stokesforge-id` fait
  échouer leur `tests/no_ui_dependency.rs` tant qu'elle n'est pas ajoutée à la
  liste blanche — délibérément.
- Ajouter un champ à un type du modèle ne suffit pas : le format `.stf` a ses
  **propres** structures de transport et des conversions écrites à la main
  ([`persistance.md`](persistance.md)).
- Certains plafonds ressemblent à des bugs et n'en sont pas (longueur de nom,
  nombre de classes d'histogramme, `MAX_NODE_ID`, `MAX_CELLS`…). Ils sont
  documentés à l'endroit où ils sont déclarés, en constante nommée, et
  recensés dans `docs/archive/rustrewrite/questions-ouvertes.md` §11.

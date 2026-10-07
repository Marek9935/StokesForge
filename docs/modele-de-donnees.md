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
`docs/application.md`).

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
une référence rompue. `NodeCopy::capture` embarque toutes les colonnes x' de la
feuille quand un graphe copié a une telle série.

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

Types : `Numeric`, `DateTime`, `Text`, `Mixed`. Rôles de tracé : `X`, `Y`
(défaut), `X2`, `Y2`, `Z`, `XError`, `YError`, `Label`, aucun. `X2`/`Y2` (#167)
alimentent un **second calque superposé** (`PanelSpec::overlay`,
`OverlaySpec { x_top, y_right }`) : chaque Y2 s'apparie au X2 le plus proche à
sa gauche, comme Y avec X (`state::series_from_columns`, `PlannedSeries`).
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

`stokesforge_core::resolve` est le **seul** module du crate à connaître
`stokesforge-plot`. Il traduit un spec contre l'arbre courant : `NodeId` → colonne
→ données prêtes à tracer. Tout le reste du crate ignore jusqu'à l'existence
de `PlotSpec`.

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

## Ce qu'il faut savoir avant de modifier

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

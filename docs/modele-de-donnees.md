# Modèle de données

Trois crates : `plotyx-id` (l'identité), `plotyx-plot` (la description d'un
graphe) et `plotyx-core` (l'arbre, les colonnes, et la résolution qui met les
deux en présence).

Aucun des trois ne connaît d'interface graphique, et aucun ne connaît de
moteur de rendu.

## L'arbre de projet

```text
Project
├── allocateur d'identifiants  (hors de l'arbre — invariant 7)
└── Arc<Tree>                  (carte plate NodeId → Arc<Node>, plus une racine)
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

Les listes d'enfants vivent **dans** la nature du nœud : une colonne, un
graphe, une matrice ou une note n'en a pas, et c'est garanti à la compilation
plutôt que par une vérification.

**Fenêtre** (*window*) = classeur, graphe, matrice ou note — un nœud
affichable en sous-fenêtre. Leurs noms sont uniques à l'échelle du **projet
entier**, pas seulement du parent (c'est ce que vérifie la batterie `b11`).

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

**À lire** : `plotyx-core/src/tree.rs`, `src/node.rs`, `src/project.rs`.

### Noms et chemins

Un nom valide est non vide, sans espace de bordure, d'au plus 255 caractères
Unicode, et sans caractère réservé (`[`, `]`, `!`, `/`), de contrôle, ou marque
de formatage/direction invisible — `U+202E` RIGHT-TO-LEFT OVERRIDE retournerait
visuellement tout chemin qui le cite.

La grammaire de chemins est celle d'Origin : `[Book]Sheet!Col`, avec dossiers
optionnels en préfixe (`/Manips/[Book1]Sheet1!ColA`). Une seule
implémentation dans le dépôt — `Tree::resolve` — et c'est celle que le pont
Python appelle aussi.

**À lire** : `plotyx-core/src/name.rs`, `src/path.rs`.

## Les colonnes

Une colonne est un vecteur de valeurs de longueur libre, plus ses métadonnées.
Son **nom vit dans le nœud**, pas dans la colonne.

Trois décisions structurent le module :

- **Valeur manquante** — `f64::NAN` pour `Numeric` et `DateTime`, `None` pour
  `Text` (distinct de `Some(String::new())`, qui est une chaîne vide
  délibérée).
- **Buffers contigus, jamais chunkés**, partagés derrière un `Arc` et mutés
  par `Arc::make_mut`. C'est ce qui permet au pont Python de tenir une vue
  numpy **sans copie** tant qu'il détient un clone de l'`Arc`.
- **Compteur de modification** (`Column::version`) : savoir qu'une colonne a
  changé sans relire ses valeurs. C'est aussi la condition optimiste que
  `Command::SetCells { expected_version }` vérifie, et qui ferme le TOCTOU
  entre l'instantané qu'un script a lu et la commande qu'il envoie.

Types : `Numeric`, `DateTime`, `Text`. Rôles de tracé : `X`, `Y` (défaut),
`Z`, `XError`, `YError`, `Label`, aucun.

**À lire** : `plotyx-core/src/column.rs`.

## `PlotSpec` — ce qu'il faut tracer

```text
PlotSpec
├── title, background
└── panels : PanelSpec           (grille row/column, au moins un panneau)
    ├── title
    ├── bottom / left / right / top → AxisSpec  (échelle, limites, grille)
    ├── legend : LegendSpec
    └── series : SeriesSpec
        ├── y : NodeId, x / x_error / y_error : Option<NodeId>
        ├── plot_type : PlotType
        ├── line : LineSpec, symbol : SymbolSpec
        └── axis : SeriesAxis    (gauche ou droite)
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
- `AxisSpec::has_explicit_range` : un cadrage explicite prime sur les données
  calculées. C'est le levier — déjà présent, rien n'a été inventé pour lui —
  sur lequel le zoom/pan de l'écran s'appuie ([`rendu.md`](rendu.md)).

**À lire** : `plotyx-plot/src/lib.rs` (tête), puis le module du sous-type visé.

## `resolve` — le seul point de contact

`plotyx_core::resolve` est le **seul** module du crate à connaître
`plotyx-plot`. Il traduit un spec contre l'arbre courant : `NodeId` → colonne
→ données prêtes à tracer. Tout le reste du crate ignore jusqu'à l'existence
de `PlotSpec`.

Deux propriétés à connaître avant d'y toucher :

- **Zéro copie.** Les colonnes sont déjà des `Arc<Vec<f64>>`. La résolution
  clone l'`Arc` — un incrément de compteur — jamais le buffer. Un
  `ResolvedValues` est une vue sur la même mémoire que la colonne,
  éventuellement tronquée au préfixe commun d'une série dont les colonnes ont
  des longueurs différentes (batterie `b16`).
- **Une référence rompue est un état de première classe, pas une exception.**
  `ResolvedSeries` existe toujours, avec `missing` non vide et `ok()` faux.
  Rien ne plante, la série n'est pas retirée du spec, elle reste visible et
  marquée (invariant 10).

**À lire** : `plotyx-core/src/resolve.rs` (tête).

## Ce qu'il faut savoir avant de modifier

- Ajouter une méthode `pub` mutante à `Project` sans commande correspondante
  dans `plotyx-commands` ouvre un chemin d'écriture non annulable
  (invariant 1).
- Ajouter une dépendance à `plotyx-core`, `plotyx-plot` ou `plotyx-id` fait
  échouer leur `tests/no_ui_dependency.rs` tant qu'elle n'est pas ajoutée à la
  liste blanche — délibérément.
- Ajouter un champ à un type du modèle ne suffit pas : le format `.plx` a ses
  **propres** structures de transport et des conversions écrites à la main
  ([`persistance.md`](persistance.md)).
- Certains plafonds ressemblent à des bugs et n'en sont pas (longueur de nom,
  nombre de classes d'histogramme, `MAX_NODE_ID`, `MAX_CELLS`…). Ils sont
  documentés à l'endroit où ils sont déclarés, en constante nommée, et
  recensés dans `docs/archive/rustrewrite/questions-ouvertes.md` §11.

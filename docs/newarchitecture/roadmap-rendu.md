# Roadmap — chantier de rendu (Plotters + backend egui)

Statut : **plan arrêté le 2026-09-10, calendrier non engagé.** Ce document
dit *quand* ; `architecture-choisie.md` dit *comment* et **fait foi** sur
l'architecture. En cas de divergence, c'est l'architecture qui gagne et ce
document qui est à corriger — même règle que le couple
`rustrewrite/roadmap.md` ↔ `cahier-des-charges-technique.md`.

Il ne remplace pas `rustrewrite/roadmap.md` (le socle, phases 0-5, closes) :
il **prend la suite**. Le socle est fini ; ce plan est le premier des deux
chantiers qui se posent dessus, l'autre étant l'UI réelle (§9).

**Séquencement retenu : « le contrat d'abord ».** Deux autres ordres ont
été examinés et écartés — §10.

---

## But de ce plan

Livrer un système de graphes qui **rend à l'écran et exporte**, sans
préempter l'UI réelle.

La doctrine est celle du socle, transposée : *la persistance passait avant
l'UI parce qu'elle était le test le plus dur du modèle*. Ici, **l'export
est au `PlotSpec` ce que la persistance était au modèle** — la seule chose
qui prouve que la couche neutre est réellement neutre. Un `PlotSpec` qui ne
sait piloter qu'un écran est un `PlotSpec` en forme d'écran, et on ne s'en
aperçoit qu'au premier export.

### Le critère de fin, en une phrase

> **Une colonne tracée à l'écran, la même figure exportée en SVG puis en
> PDF, axes au même endroit — vérifié par un test, pas à l'œil.**

Il est choisi pour forcer le **seul défaut silencieux** du chantier : un
désaccord de mesure de texte entre les deux backends décale les axes sans
lever la moindre erreur (`architecture-choisie.md` §4.4).

**Clôture séparée pour Typst**, puisqu'il vit derrière un *feature flag* et
que sa phase peut se solder par un abandon :

> Un label `λ_max` correct à l'écran **et** dans le PDF exporté, sans
> dépendance de font chez le lecteur.

---

## Correction d'estimation à l'ouverture

Une première estimation orale annonçait 6 à 7 semaines hors Typst. **Elle
était fausse par omission** : `NodeKind::Graph` ne porte aujourd'hui que des
métadonnées de fenêtre (`node.rs` : *« contenu de tracé ajouté dans une
tranche ultérieure »*). Il faut donc y ancrer le `PlotSpec` et le persister
— une phase entière (R3) qui n'était pas comptée.

**Estimation corrigée : 9 à 11 semaines hors Typst, 11 à 13 avec.**
Calibrage repris des phases du socle réellement livrées (phase 1 :
≈1,5-2 semaines pour 5 125 lignes ; phase 2 : ≈2 semaines pour 4 484
lignes), soit ~2 500 lignes/semaine, tests compris.

---

## Ce qu'il y a à porter, mesuré

| Source Python | Lignes | Nature du port |
|---|---:|---|
| `plotyx/plot/spec.py` | 518 | dataclasses `frozen=True` + 6 énumérations → structs et enums Rust, **quasi verbatim** |
| `plotyx/plot/resolve.py` | 314 | résolution des références, `prepare()`, `histogram`, `bar_width` |
| `plotyx/plot/edit.py` | 187 | **hors périmètre** — voir §9 |
| `tests/test_plot_spec.py` | 289 | **la spécification du port, déjà écrite** |
| *(référence)* `ui/mpl_renderer.py` | 852 | ce que couvre un renderer complet : pan, zoom, réticule |

Le port de la couche neutre est mécanique, et ce n'est pas une supposition :
elle a survécu à un changement complet de renderer le 2026-09-07 **sans
qu'une ligne bouge** (mémoire projet `plotyx-matplotlib-default-renderer`).
C'est l'argument le plus solide au dossier pour la garder neutre ici aussi.

---

## Phase R0 — Spike de mesure (≈3-4 jours)

> **Statut : close le 2026-09-10.** Trois nombres, mesurés sur Core Ultra 7
> 155H, en `--release`, versions épinglées `=0.36.2` et `=0.15.1` ; code du
> spike jeté, aucun fichier du dépôt touché par la mesure.
>
> | # | Mesuré | Seuil fixé d'avance | Verdict |
> |---|---|---|---|
> | 0.1 | **0,000000 px** d'écart (540 comparaisons) | ≤ 0,05 px | **tenu, par égalité exacte** |
> | 0.2 | **0,156 ms** / formule inédite, monde chaud (froid 3,7 ms) | ≤ 150 ms | **Typst retenu tel quel** |
> | 0.3 | **22,1 ms** / frame à 300 k points | > 8 ms | **LTTB entre en R5** |
>
> Les trois nombres sont reportés dans `architecture-choisie.md` (points
> ouverts 1 et 2, clos ; §4.2 et §4.3 corrigés). **La mesure a aussi
> invalidé deux chiffres du document et révélé un levier absent** — détail
> en §4.2 : le cache `Mesh` coûte 45,6 Mo et non 14, et le *feathering* pèse
> 78 % de la tessellation. Le 2,4 Mo du cache `Vec<Shape>`, lui, est
> confirmé au chiffre près.
>
> Corrections d'API relevées au passage, à ne pas redécouvrir :
> `Fonts::new` prend **deux** arguments en 0.36.2, `Context::run` s'appelle
> `run_ui`, et l'API `typst` 0.15.1 a bougé par rapport à tous les exemples
> publics (`FileId::new` veut un `RootedPath`, `VirtualPath::new` rend un
> `Result`, `PagedDocument` vient de `typst-layout`). Le `World` fait bien
> ~90 lignes : l'estimation du §7 est bonne.

**Objectif** : répondre aux trois questions qui peuvent **changer une
décision**, avant qu'une ligne de code ne dépende de la réponse. Aucune
architecture, aucun crate : du code jetable, jeté à la fin.

| # | Tâche | Livrable |
|---|---|---|
| 0.1 | Construire une `epaint::Fonts` **hors `Context`** (`Fonts::new(TextOptions, FontDefinitions)`), mesurer un jeu de chaînes, comparer aux mesures d'une vraie application egui | Deux colonnes de nombres, identiques ou non |
| 0.2 | Compiler ~20 formules **réelles** (λ_max, cm⁻¹, indices, fractions de cinétique) avec un `World` Typst minimal ; mesurer première compilation et compilations suivantes | Latence à froid, latence à chaud |
| 0.3 | 500 × 600 points en `Shape::Path`, mesurer `Context::tessellate` par frame | Millisecondes par frame |

> **Règle de méthode, non négociable : fixer les seuils de décision AVANT
> de mesurer.** Écrire noir sur blanc « au-delà de X ms par formule inédite,
> Typst est abandonné » et « au-delà de Y ms par frame, le LTTB entre en
> R5 » *avant* de lancer le chronomètre. Sinon la mesure ne tranche rien :
> on rationalise le nombre obtenu.

**Ce que chaque mesure décide :**

- **0.1** valide le mécanisme central du chantier (`architecture-choisie.md`
  §4.3). S'il échoue — `Fonts::new` refuse de vivre sans `Context`, ou les
  mesures divergent — l'invariant du critère de fin doit être tenu
  autrement, et **il faut le savoir maintenant**, pas en R4 ;
- **0.2** peut solder R8 avant de l'ouvrir. C'est le point ouvert n°2 de
  `architecture-choisie.md` ;
- **0.3** décide si le LTTB est en R5 ou plus tard. Point ouvert n°1.

**Sortie de phase** : trois nombres, reportés dans les points ouverts de
`architecture-choisie.md`, et le code du spike supprimé.

---

## Phase R1 — `Tree::resolve` dans le socle (≈2-3 jours)

> **Statut : close le 2026-09-10.** `Tree::resolve(&str)` et
> `Tree::resolve_parsed(&ObjectPath)` dans `plotyx-core` ;
> `Project::resolve_path` et `Project::resolve` réduites à deux délégations
> d'une ligne ; `bridge.rs::resolve_in_tree` supprimé (−67 lignes), en-tête
> réécrit. Une seule implémentation de la grammaire dans le dépôt.
> Vérifications : 257 tests du workspace, 21 tests de `plotyx-script` sous la
> feature `python` — **aucun test modifié**, le critère de sortie est tenu —
> et clippy sans avertissement, feature `python` comprise (`--workspace`
> seul compile `plotyx-script` **vide** et ne voit pas `bridge.rs`).
>
> **La fusion a révélé une divergence, et c'était son but.** Les deux
> implémentations rendaient un message différent pour une colonne
> introuvable : `Project` donnait la forme **canonique** du chemin, `bridge`
> la **chaîne brute** de l'appelant. Elles divergeaient dès que l'entrée
> n'était pas déjà canonique, `parse_path` normalisant les espaces de bordure
> et les `//` répétés — sur `"  /Manips//[Book1]Sheet1!Absente  "`, deux
> messages distincts. Ce texte remonte tel quel dans `UnknownPathError` côté
> Python, aucun test ne l'assertait. **Tranché pour la forme canonique** —
> c'est l'original, et la seule disponible pour `resolve_parsed`, qui n'a pas
> de texte d'entrée. Verrouillé par un test.
>
> Trois propriétés tacites de la résolution sont désormais documentées, parce
> qu'elles se seraient perdues à la troisième réécriture : le drapeau
> `rooted` n'est jamais consulté (pas de dossier courant dans le modèle) ; le
> préfixe de dossiers ne sert jamais à *trouver* la fenêtre mais à vérifier
> *après coup* qu'elle est là où le chemin le dit ; la casse est exacte
> partout.
>
> Deux points laissés en l'état, à reprendre en R3 : `folder_names_above` est
> `pub(crate)` et devra devenir publique si R3 veut un `path_of` côté `Tree` ;
> et sur `Project`, `resolve` prend un `&ObjectPath` quand sur `Tree` elle
> prend un `&str` — l'asymétrie suit la signature imposée ici, elle se lit
> mal, à renommer si elle gêne.

**Objectif** : une seule résolution de chemin dans le dépôt, avant qu'il n'y
en ait trois.

`plotyx-script/src/bridge.rs` a déjà dû réécrire la résolution contre un
`Tree`, faute d'avoir le `Project` (que seul l'acteur possède). Sa propre
documentation nomme la dette :

> *« C'est un doublon assumé et visible ; le supprimer demanderait une
> `Tree::resolve` dans `plotyx-core`, ce que cette tâche n'a pas le droit de
> faire. »*

Ce chantier a ce droit, et il en a besoin : la résolution du `PlotSpec`
(R3) contre un `Arc<Tree>` serait la **troisième** copie.

| # | Tâche | Livrable |
|---|---|---|
| 1.1 | `Tree::resolve(&self, path: &str) -> CoreResult<NodeId>` dans `plotyx-core`, même grammaire `[Book1]Sheet1!ColA` | API publique de lecture |
| 1.2 | `Project::resolve_path` délègue à `Tree::resolve` | Une seule implémentation |
| 1.3 | `bridge.rs::resolve_in_tree` supprimé au profit de l'appel | Doublon résorbé |

**Sortie de phase** : les tests existants de `plotyx-script` passent **sans
modification**. C'est le critère : si un test doit changer, la sémantique a
bougé et le remplacement n'est pas neutre.

---

## Phase R2 — `plotyx-plot`, le spec neutre (≈1,5 semaine)

> **Statut : close le 2026-09-11.** Crate `crates/plotyx-plot/` (1 307
> lignes), 6 énumérations et 7 structures portées de
> `legacy-python/src/plotyx/plot/spec.py`. **Critère de sortie tenu** :
> `cargo tree -p plotyx-plot` ne montre que `plotyx-core`. 288 tests au
> workspace (257 avant, +31), clippy sans avertissement ; hors du nouveau
> crate, seules deux lignes ont bougé (le membre dans le `Cargo.toml` du
> workspace, et le `Cargo.lock`).
>
> **Les références de colonnes sont des `NodeId`** — la décision §11.1
> d'`architecture-choisie.md` s'applique ici, et le port l'a rendue facile :
> la version Python utilisait déjà des « clés de registre » opaques, pas des
> noms.
>
> **Le `__post_init__` a presque entièrement disparu dans les types.**
> Quatorze validations sont devenues invérifiables : les six `_coerce` vers
> des énumérations (aucune chaîne n'entre dans ce crate), les conversions
> numériques, `y_key` obligatoire (`y: NodeId` non optionnel), et surtout
> l'axe de rattachement — `SeriesAxis` n'a que deux variantes là où la
> version Python vérifiait à l'exécution que `AxisSide` valait gauche ou
> droite. Deux validations disparaissent **avec changement de comportement
> assumé** : `histogram_bins` remonté de 0 à 1 devient un `NonZeroU32` (un
> appelant qui passe 0 a un bug, autant qu'il ne compile pas), et les
> `row`/`column` négatifs ramenés à 0 deviennent des `usize`. Une seule
> survit, faute d'être exprimable : « au moins un panneau », tenue par un
> newtype `Panels` dont aucune opération publique ne peut vider le contenu.
>
> **L'identifiant de série est supprimé**, remplacé par un adressage par
> position (`SeriesRef { panel, index }`). Il n'existait que pour `edit.py`,
> hors périmètre, et le porter aurait imposé `uuid` ou `rand` — donc cassé
> le critère de sortie. *Point à surveiller* : si l'UI doit un jour retenir
> une sélection **par-dessus** une édition qui supprime une autre série,
> l'indice ne suffira plus ; la parade est de faire allouer l'identifiant
> par la session, qui possède déjà l'`IdAllocator`, plutôt que de
> réintroduire un générateur dans ce crate. Un champ à ajouter, pas une
> forme à changer.
>
> **Dix-huit tests Python sur 33 ne sont pas transposés**, et la liste
> figure en tête de `tests/plot_spec.rs` pour que la question ne se repose
> pas : onze couvrent `edit.py` (hors programme), les sept autres couvrent
> une sérialisation qui n'existe pas ici ou une validation devenue
> impossible par construction. Deux propriétés de la section « édition » ont
> quand même été reprises parce qu'elles portaient sur le spec :
> l'immuabilité et le cycle de la palette.
>
> **Écarts de comportement par rapport à Python**, tous voulus : `0` n'est
> plus remonté à `1` pour les classes d'histogramme ; les positions
> négatives ne sont plus ramenées à zéro ; `bar_width = 0.0` devient
> `Option::None` (0.0 était une largeur légale, donc une sentinelle
> ambiguë) ; aucune valeur textuelle n'est plus acceptée là où Python
> coerçait `"scatter"` — cette conversion appartient à la persistance,
> comme pour `Column`.

**Objectif** : le `PlotSpec` en Rust, **sans dépendance de dessin**.

| # | Tâche | Livrable |
|---|---|---|
| 2.1 | Les 6 énumérations : `PlotType`, `AxisScale`, `AxisSide`, `LineStyle`, `SymbolShape`, `LegendPosition` | Port verbatim |
| 2.2 | `LineSpec`, `SymbolSpec`, `SeriesSpec`, `AxisSpec`, `LegendSpec`, `PanelSpec`, `PlotSpec` | Structs immuables |
| 2.3 | Port de `tests/test_plot_spec.py` (289 l.) | Suite de non-régression |
| 2.4 | Test de non-dépendance, sur le modèle de `plotyx-core/tests/no_ui_dependency.rs` | Frontière vérifiée par la CI |

### 2.1 La frontière qui fait tout tenir

`plotyx-plot` **ne dépend pas de `plotters`**, ni d'`egui`, ni de `typst`.
Sa seule dépendance est `plotyx-core`, pour les `NodeId`.

C'est la transposition exacte de « aucun Qt dans `core` », et c'est ce qui a
permis à la couche Python de survivre au remplacement de son renderer.
Rendre la règle vérifiable par le compilateur plutôt que par la relecture
est le même choix qu'au §2.4 du cahier des charges du socle.

**Écarté — fusionner `plotyx-plot` dans `plotyx-core`** : moins de crates,
mais le modèle se mettrait à connaître des couleurs et des styles de trait,
et plus rien n'empêcherait `plotters` d'y remonter.

**Sortie de phase** : `cargo tree -p plotyx-plot` ne montre ni `plotters`,
ni `egui`, ni `typst`.

---

## Phase R3 — Ancrage dans le modèle (≈1,5 semaine)

> **Statut : close le 2026-09-11.** `NodeKind::Graph { meta, spec: PlotSpec }`
> dans `plotyx-core` ; persistance avec `#[serde(default)]`, `FORMAT_VERSION`
> toujours à 1, vérifié par un test qui rejoue un fichier écrit *avant*
> cette phase ; port de `resolve.py` en `plotyx-core::resolve` (18 tests) ;
> `Project::add_graph` créé sur le patron d'`add_column`. 308 tests au
> workspace (288 avant, +20), clippy sans avertissement, `plotyx-script`
> inchangé et vert (21 tests).
>
> **Un neuvième crate est apparu, non prévu au découpage du §8
> d'`architecture-choisie.md` : `plotyx-id`.** Faire porter `Graph` par un
> `PlotSpec` demandait `plotyx-core → plotyx-plot`, alors que `plotyx-plot`
> dépendait déjà de `plotyx-core` pour `NodeId` — un cycle Cargo direct,
> vérifié empiriquement plutôt que supposé (`cargo check` refuse avec
> *cyclic package dependency*). Résolu en descendant `NodeId` seul (pas
> `IdAllocator`) dans un crate minimal sans dépendance, réexporté à
> l'identique par `plotyx-core` : aucun appelant existant n'a rien à
> changer, et `plotyx-plot` en ressort avec une frontière **plus** stricte
> (`cargo tree` ne montre plus que `plotyx-id`). Détail complet en
> `architecture-choisie.md` §8.
>
> **Les références brisées sont un état de première classe**, porté tel
> quel depuis `resolve.py` : `ResolvedSeries` existe toujours, avec
> `missing: Vec<MissingRole>` et `ok()`/`broken()` — jamais un panic, jamais
> une série silencieusement vide. La tâche 3.6 le verrouille par deux tests
> distincts : renommer la colonne référencée laisse la série résoluble (et
> son libellé suit le nouveau nom) ; la supprimer produit `ok() == false`,
> `missing == [Y]`, sans qu'aucune autre valeur de la série ne soit touchée.
>
> **Non porté, délibérément** : `ComputeCancelled` et `prepare()` de
> `resolve.py`, l'annulation coopérative d'un calcul en fil de fond —
> infrastructure qui n'existe pas côté Rust, le recalcul automatique
> n'étant ni construit ni planifié (§11.4). `resolve_spec` couvre la
> résolution ; le pré-calcul d'histogramme que `prepare()` intercalait reste
> disponible séparément (`histogram`), à appeler par un futur renderer.
>
> **Écart assumé** : `Project::add_graph` prend désormais `spec: PlotSpec`
> en paramètre obligatoire plutôt que de rester inchangée — cohérent avec
> `add_column`, qui impose déjà son contenu à la construction.

**Objectif** : un graphe du projet **porte** son spec, et le fichier le
relit.

C'est la phase qui touche au socle, et la seule. Elle était prévue :
`node.rs` annonce le `Graph` comme *« contenu de tracé ajouté dans une
tranche ultérieure »*. C'est cette tranche.

| # | Tâche | Livrable |
|---|---|---|
| 3.1 | `NodeKind::Graph { meta, spec: PlotSpec }` | Le graphe porte son contenu |
| 3.2 | Champ `spec` dans le `NodeRecord::Graph` de `plotyx-persistence`, avec `#[serde(default)]` | Aller-retour disque |
| 3.3 | Port de `resolve.py` contre un `Arc<Tree>` : `ResolvedSeries`, `ResolvedPanel`, `ResolvedPlot` | Spec → données prêtes à tracer |
| 3.4 | Helpers `bar_width`, `histogram` | Port des deux fonctions |
| 3.5 | Test d'aller-retour : un projet avec graphe, sauvé puis rechargé, rend un spec égal | Roundtrip vert |
| 3.6 | Test : renommer une colonne référencée laisse le graphe résoluble ; la supprimer produit une référence brisée explicite | La décision §3.3 verrouillée par un test |

### 3.1 Pourquoi la persistance ne coûte pas de version de format

`FORMAT_VERSION` reste à **1**. Le format est MessagePack avec `serde`,
choisi précisément pour cette propriété (cahier des charges §7) : un champ
porteur de `#[serde(default)]` s'ajoute sans casser la lecture d'un fichier
antérieur, et un champ inconnu est ignoré. **C'est le premier encaissement
réel de cette décision** — jusqu'ici elle n'avait jamais été mise à
l'épreuve.

### 3.2 Ce qui n'est délibérément pas fait ici

**Aucune commande d'édition du spec.** Modifier un graphe (changer une
couleur, ajouter une série) devra passer par le bus et être annulable —
donc étendre les huit primitives de `plotyx-commands`. Ce n'est pas ce
chantier : c'est le chantier de l'UI réelle, avec le panneau Properties
sous les yeux, qui dira quelles primitives sont les bonnes. Ici, le spec
est **lu**, jamais écrit ; il se construit en dur dans les tests.

C'est aussi pourquoi `edit.py` (187 l.) n'est pas au tableau des ports.

### 3.3 Les références du spec sont des `NodeId` — décidé le 2026-09-10

Une série du spec désigne sa colonne par **identité** (`NodeId`), jamais par
nom ni par chemin textuel. Renommer ou déplacer une colonne laisse le graphe
intact ; supprimer une colonne produit une référence **brisée et
détectable**, à afficher comme telle et jamais comme une série vide — d'où
la tâche 3.6.

C'est aussi la première arête de dépendance du projet : un graphe est un
nœud dérivé qui ne calcule rien. Le raisonnement complet, la seconde
décision qui l'accompagne (un recalcul automatique n'entre pas dans
l'historique) et l'index inverse écarté sont dans `architecture-choisie.md`
§11.

Coût nul décidé ici ; migration de tous les fichiers déjà écrits si c'est
décidé après.

**Sortie de phase** : un projet contenant un graphe avec deux séries se
sauve, se recharge, et se résout en données traçables — sans qu'aucun
pixel n'ait encore été dessiné.

---

## Phase R4 — Les deux backends, graphe minimal (≈2 semaines)

> **Statut : close le 2026-09-12.** Trois crates au lieu des deux prévus
> (`plotyx-render`, `plotyx-render-egui`, `plotyx-render-svg` — 1 864 lignes
> de code, 1 746 de tests et d'exemple), le test d'accord vert, et une figure
> qu'on peut regarder des deux côtés. 479 tests au workspace (440 avant,
> +39), 9 `#[ignore]` inchangés, clippy sans avertissement (feature `typst`
> et feature `python` comprises), `cargo fmt` sans dérive nouvelle (les 25
> échecs préexistants de §7.4 du guide de robustesse sont intacts).
> **Critère de sortie tenu** : `cargo tree -p plotyx-plot` ne montre toujours
> que `plotyx-id`, et `cargo tree -p plotyx-render-svg` ne montre ni `egui`,
> ni `eframe`, ni `winit`.
>
> **Un crate non prévu : `plotyx-render`.** Le §8 d'`architecture-choisie.md`
> n'annonçait que les deux backends. Il en manquait un troisième, pour la
> même raison qu'en R3 avec `plotyx-id` : deux frères ont besoin de la même
> chose et aucun ne peut dépendre de l'autre sans mentir sur ce qu'il est.
> La chose commune est double — la mesure de texte (4.1) *et* la suite
> d'appels Plotters. Faire porter l'une ou l'autre par `plotyx-render-svg`
> obligerait le backend écran à dépendre du crate d'export ; les faire porter
> par `plotyx-render-egui` ferait entrer toute la pile egui dans l'export, ce
> que le découpage interdit. Détail et justification : §8 du même document.
>
> **Le pipeline Plotters est écrit une fois, générique sur le backend**, et
> c'est le vrai enseignement de la phase. La tâche 4.5 demandait un test qui
> constate l'accord ; un test constate, il n'empêche pas. Avec deux
> `ChartBuilder` distincts, la moindre divergence de marge ou de taille de
> police serait devenue un désaccord à trouver après coup. Avec un seul, les
> deux backends ne diffèrent plus que par la façon de poser des pixels — la
> règle que R5 se donne (« aucune fonctionnalité n'entre dans un seul
> backend ») est ainsi tenue par la structure et non par la discipline.
>
> **Une correction à la mesure R0 0.1, trouvée en écrivant le test qui devait
> l'entériner.** R0 concluait les métriques de texte indépendantes du
> `pixels_per_point`. **C'est vrai des avances, faux de la hauteur de
> ligne** : `epaint` cale la hauteur d'une ligne sur la grille de pixels
> *physiques* (`text_layout.rs` : `max_row_height =
> point_scale.round_to_pixel(max_row_height)`). Mesuré à 9,7 pt de corps : 11
> points de haut à `pixels_per_point = 1,0`, 12 à 1,25. R0 ne pouvait pas le
> voir, elle comparait des avances de glyphes et non des rectangles de
> galley. La conséquence aurait été exactement du type que la phase existe
> pour empêcher : `DrawingArea::titled` fait descendre la zone de tracé de la
> hauteur du titre, donc **la figure exportée aurait dépendu de l'écran de
> celui qui l'exporte**. Tranché en séparant les deux usages dans le trait
> `TextSource` : on **mesure** toujours à `pixels_per_point = 1,0`, on ne
> **dessine** qu'au `pixels_per_point` réel. Prix payé : au plus un pixel
> physique d'écart entre la boîte réservée et les glyphes sur un écran HiDPI,
> sans effet sur la géométrie puisque le texte est ancré et non étiré. Le
> fait qui justifie la règle est verrouillé *dans le sens de l'affirmation*
> par un test : si `epaint` corrigeait ce comportement, le test tomberait et
> la constante pourrait disparaître.
>
> **Deuxième piège trouvé en chemin, celui-là mortel** : avec `plotters` en
> `default-features = false`, l'implémentation **par défaut** de `draw_text`
> finit sur `FontData::draw` de `style/font/naive.rs`, qui est un
> `panic!("The font implementation is unable to draw text")`. Autrement dit,
> tout backend qui ne surcharge pas `draw_text` ne mesure pas mal : il
> **plante** à la première graduation. Les deux backends du chantier la
> surchargent (egui directement, SVG par délégation à `SVGBackend`), et le
> backend muet des tests de géométrie s'y est cogné — c'est là que le fait
> est noté.
>
> **Le test d'accord compare tout, pas les axes seulement.** Même `PlotSpec`,
> mêmes dimensions, et l'égalité porte sur le multiensemble de **tous les
> segments** — axes, graduations, grille et courbe — en coordonnées entières,
> à quatre dimensions de figure différentes. Il porte aussi un **témoin** :
> la même figure tracée avec le `SVGBackend` nu, dont les positions doivent
> **différer**. Sans lui, une enveloppe qui déléguerait `estimate_text_size`
> par erreur passerait pour correcte — c'est la méthode du témoin positif du
> spike R0 0.1. Il vit dans `plotyx-render-egui/tests/accord.rs` et non côté
> SVG : en dépendance de développement, egui entrerait dans l'arbre du crate
> d'export et ferait mentir sa frontière.
>
> **Écarts assumés du périmètre**, tous signalés par un `// R5 :` à l'endroit
> exact où ils s'insèrent plutôt que par une liste qui se périmerait : un
> seul panneau (le premier), toutes les séries tracées en ligne quel que soit
> leur `PlotType`, pas de légende, pas d'axe secondaire, pas d'échelle
> logarithmique. Une série à référence brisée est **comptée** et laissée de
> côté, ce qui est le minimum honnête tant qu'il n'y a nulle part où
> l'écrire ; le §11.1 demandera plus en R5. `fill_polygon` passe par
> `PathShape::convex_polygon`, correct aujourd'hui (rien de concave n'est
> tracé, `draw_path` étant surchargée) et à reprendre avec les barres et les
> aires.
>
> **La tâche 4.6 est double.** La fenêtre du banc (`cargo run -p plotyx-app`)
> montre le graphe peint par `plotyx-render-egui`, avec son compteur de cache
> sous la figure — 8 succès pour 2 calculs après stabilisation de la mise en
> page, c'est-à-dire la tâche 4.3 visible à l'œil nu. Et l'export se regarde
> sans serveur graphique : `cargo run -p plotyx-render-svg --example figure`.
> Un troisième mécanisme de vérification a été ajouté au banc, sur le modèle
> de `PLOTYX_APP_AUTOQUIT_MS` : `PLOTYX_APP_SCREENSHOT=fichier.ppm` fait
> déposer à la fenêtre une capture d'elle-même. Sous Wayland, aucun outil
> externe ne peut photographier une fenêtre donnée ; et sur un chantier de
> rendu, « la fenêtre s'ouvre et ne plante pas » a cessé de suffire.
>
> **Reste ouvert pour R5+** : le `pixels_per_point` est dans la clé de cache
> mais n'entre plus dans la géométrie — il n'y reste que pour l'exception
> `Shape::Text` d'`epaint`, ce qui est correct et mérite d'être su ; et la
> mesure R0 0.3 n'a pas été refaite sur le pipeline réel, donc le 2,432 Mo
> pour 300 000 points reste un chiffre de spike, pas de production.

**Le cœur du plan.** Objectif : les deux backends existent **en même
temps**, sur le graphe le plus pauvre possible, et un test les force à
s'accorder.

Graphe minimal = **une** `LineSeries`, axes linéaires, pas de légende, pas
de marqueur, pas d'échelle log.

| # | Tâche | Livrable |
|---|---|---|
| 4.1 | Les mêmes `FontDefinitions` pour les deux backends, plus un test qui verrouille l'égalité des mesures | Source de mesure unique |
| 4.2 | `plotyx-render-egui` : `DrawingBackend` collectant dans un `Vec<Shape>`, `estimate_text_size` sur 4.1 | ~250-300 lignes |
| 4.3 | Cache sur `(revision, rect, pixels_per_point)` | Une frame sans changement = `painter.extend` |
| 4.4 | `plotyx-render-svg` : `SVGBackend`, **`estimate_text_size` surchargé sur 4.1** | Export SVG |
| 4.5 | **Le test d'accord** : même `PlotSpec`, mêmes dimensions → positions d'axes identiques entre les deux backends | Le filet de sécurité du chantier |
| 4.6 | Un rendu **visible** : la fenêtre du banc d'essai, ou un PNG hors écran | Retour visuel |

### 4.1 Pourquoi 4.4 n'est pas optionnel

`plotters-svg` **ne surcharge pas** `estimate_text_size`. Sans 4.4, il
retombe sur le défaut du trait → `layout_box` → et, les fonts de Plotters
étant désactivées, sur `style/font/naive.rs` :

```rust
let em = size / 1.24 / 1.24;
// largeur ≈ em * 0.7 * text.len()
```

Une largeur monospace estimée sur le **nombre d'octets UTF-8** : « λ »
compte pour 2, « µm » pour 3, « 111 » mesure comme « WWW ». Les marges
réservées aux labels seraient fausses, donc la zone de tracé différente de
celle de l'écran. Détail complet : `architecture-choisie.md` §4.4.

### 4.2 Pourquoi 4.6 est dans la phase

Le séquencement « le contrat d'abord » a un défaut connu : cinq semaines
sans rien voir bouger. Sur un chantier de **rendu**, le retour visuel n'est
pas un confort — c'est ce qui fait remonter les erreurs qu'aucun test ne
formule. La tâche 4.6 emprunte cette qualité au séquencement écarté du §10.

**Sortie de phase** : le critère de fin est vrai pour un graphe à une
courbe. Tout le reste du plan consiste à le garder vrai en élargissant.

---

## Phase R5 — Élargissement (≈2 semaines)

> **Statut : close le 2026-09-12.** Le `PlotSpec` réel est couvert — six
> types de tracé, deux échelles, axe secondaire, styles de trait et de
> marqueur, légende, grille de panneaux, décimation — et **pas une ligne de
> rendu n'est entrée dans un backend** : tout est descendu dans le pipeline
> commun de `plotyx-render`, qui gagne six modules (`series`, `scale`,
> `paint`, `legend`, `decimate`, `sample` : 1 780 lignes) pendant que
> `plotyx-render-egui` et `plotyx-render-svg` n'ajoutent, à eux deux, que le
> levier de *feathering* (133 lignes) et un exemple élargi. La règle de la
> phase n'a pas eu à être surveillée : le pipeline générique de R4 la tenait
> par construction, et c'est le vrai dividende de cette structure.
> 518 tests au workspace (479 avant, **+39**), 9 `#[ignore]` inchangés,
> clippy sans avertissement (features `typst` et `python` comprises),
> `cargo fmt` sans dérive nouvelle (exactement les 25 échecs préexistants
> de §7.4 du guide de robustesse, sur les mêmes 15 fichiers). **Critère de
> sortie tenu** : `cargo tree -p plotyx-plot` ne montre toujours que
> `plotyx-id`, et `cargo tree -p plotyx-render-svg` ni `egui`, ni `eframe`,
> ni `winit`.
>
> **Le graphe réaliste existe et se regarde** :
> `cargo run -p plotyx-render-svg --example figure` — cinq panneaux dans une
> grille 2 × 3 (donc une cellule vide), 8 séries, ordonnée logarithmique,
> axe secondaire, barres d'erreur, histogramme, barres, trois styles de
> trait, quatre formes de marqueur, une légende par panneau, et une
> référence brisée. Le test d'accord compare les deux backends sur **ce**
> graphe-là, à trois dimensions de figure, en plus du graphe minimal de R4
> qui continue de passer inchangé.
>
> **Quatre décisions prises en cours de route, qu'aucun document source
> n'annonçait.**
>
> 1. **L'échelle logarithmique transforme les valeurs ; elle ne change pas
>    le type de coordonnée.** `LogRange` de Plotters existe, et le prendre
>    aurait fait exister la fonction qui trace un panneau en **huit**
>    monomorphisations (deux axes × deux échelles × l'axe secondaire) — ou
>    en huit branches redessinant la même chose. C'est la divergence que le
>    pipeline unique existe pour empêcher, qui serait rentrée par la porte
>    du système de types. Un seul `Cartesian2d<f64, f64>` donc : la zone de
>    tracé raisonne en log₁₀, et deux endroits seulement savent qu'il y a
>    une échelle, l'entrée des données et l'étiquetage. **Prix payé, réel** :
>    les graduations sont des décades, pas les graduations mineures
>    logarithmiques que `LogRange` sait poser ; et les valeurs nulles ou
>    négatives disparaissent d'un axe log, comme les `NaN` disparaissent
>    déjà partout.
> 2. **L'axe secondaire est toujours attaché, même inutilisé.**
>    `set_secondary_coord` ne prend à l'axe principal que les aires du haut
>    et de la droite — qui n'existent pas quand on ne les a pas demandées.
>    Une seule forme de `chart` pour les deux cas, donc un seul chemin de
>    tracé de série ; la branche aurait coûté deux fois le même code.
>    Vérifié plutôt que supposé : le rendu du graphe minimal de R4 est
>    inchangé au pixel près.
> 3. **La légende est écrite ici, pas déléguée à
>    `configure_series_labels()`.** Plotters dimensionne son cadre avec
>    `MultiLineText::estimate_dimension`, qui passe par
>    `FontData::estimate_layout` — **pas** par `estimate_text_size` du
>    backend. Nos fonts Plotters étant coupées, c'est l'heuristique
>    monospace par octets UTF-8 du §4.4 : le cadre serait mesuré faux
>    pendant que le texte dedans serait dessiné juste. Et il serait faux
>    **de la même façon des deux côtés**, donc le test d'accord ne
>    l'attraperait pas — c'est le cas qui montre qu'un test d'accord ne
>    dispense pas de savoir par où passe la mesure.
> 4. **Les séries se peignent en pixels, plus en coordonnées de données.**
>    Un tiret se mesure en pixels ; Plotters n'a d'élément tout fait ni pour
>    le carré, ni pour le losange, ni pour la croix droite ; et son
>    `DashedLineSeries` ne connaît qu'un motif à deux temps, donc pas de
>    `DashDot`. Le tracé passe donc par `plotting_area().use_screen_coord()`
>    après `map_coordinate` — ce qui est **exactement** le chemin que
>    `draw_series` empruntait, troncature au rectangle comprise
>    (`element/mod.rs` : `rect.truncate(…)`), d'où l'absence de régression
>    sur le graphe minimal.
>
> **Le levier de *feathering* vit dans `plotyx-render-egui::feathering`, et
> nulle part ailleurs — c'est la seule exception de la phase à la règle
> « aucune fonctionnalité dans un seul backend », et ce n'en est pas
> vraiment une : il n'y a rien à mettre de l'autre côté.**
> `TessellationOptions` est un réglage d'`egui::Context`, en aval de la
> collecte des `Shape` ; `plotyx-render` ne connaît pas egui, et un SVG n'a
> pas de tessellation du tout — il décrit des courbes, il ne les triangule
> pas. Trois précisions à ne pas redécouvrir :
>
> - **ce n'est pas une méthode de `GraphView`.** Le réglage vaut pour tout
>   le `Context`, donc pour tous les widgets de l'image ; une vue qui le
>   poserait en peignant le poserait pour les autres, et deux vues en
>   poseraient deux valeurs contradictoires. C'est une fonction libre qui
>   prend le `Context`, appelée par qui sait qu'une interaction commence ;
> - **la clé de cache n'a pas grossi**, et un test l'exige désormais plutôt
>   que trois commentaires : basculer la frange deux fois de suite laisse
>   `ShapeCache::misses()` inchangé. L'architecture le dit (§4.2) ; il
>   fallait que quelque chose le vérifie, parce que c'est exactement le
>   terme qu'on ajouterait « par prudence » ;
> - **le levier est mesuré, pas affirmé** : `tests/feathering.rs` tessèle la
>   même figure deux fois et compte les sommets. Si `epaint` changeait de
>   stratégie de frange, le test tomberait et la décision du 2026-09-10
>   serait à réexaminer.
>
> **La frontière posée pour R7 tient en deux appels** : à l'entrée du
> glisser `feathering::apply(ctx, Feathering::Off)`, au relâchement
> `apply(ctx, Feathering::Full)` — ou, en une ligne,
> `apply(ctx, Feathering::during_drag(response.dragged()))`. Rien d'autre
> n'est à écrire, et rien n'est à changer dans le module.
>
> **Le LTTB ne règle pas le cas mesuré, et il est livré en le sachant.** La
> mesure R0 0.3 disait que la charge du cas de référence (500 séries × 600
> points) vient du **nombre de séries** et non de leur longueur : 600 points
> sur 1 600 px sont déjà sous-échantillonnés, le LTTB n'y retire rien. Il
> est implémenté pour ce qu'il sert réellement — quelques séries très
> longues, le cas de l'acquisition — et le test le vérifie sur ce cas-là :
> 40 000 points sur 800 px, plus de la moitié retirés, cadrage inchangé,
> **et les deux backends décimant à l'identique**. Ce dernier point n'est
> pas une formalité : le seuil dépend de la largeur en pixels de la zone de
> tracé, donc c'est le seul endroit du pipeline où la géométrie dépend de la
> taille de la figure autrement que par une homothétie — et donc le seul où
> un désaccord de mise en page se traduirait par un désaccord de *données*.
> Deux règles écrites plutôt que devinées : on ne décime pas une série à
> barres d'erreur (les moustaches se dissocieraient de leurs points) ni un
> nuage sans trait (le LTTB raisonne le long d'un chemin).
>
> **Le test d'accord a gagné un second témoin, et c'est lui qui a le plus de
> valeur.** Celui de R4 — le `SVGBackend` nu doit placer la figure ailleurs
> — reste. R5 y ajoute : pour **chacune** des neuf fonctionnalités de la
> phase, une variante du graphe réaliste qui la débranche doit produire une
> figure **différente**, et les deux backends doivent rester d'accord sur la
> variante aussi. Sans lui, une fonctionnalité jamais appliquée laisserait
> l'accord au vert — deux figures identiquement amputées s'accordent
> parfaitement. La comparaison elle-même s'est élargie des segments seuls
> aux cercles, polygones, rectangles et **contenus de texte** ; le texte par
> son contenu et non sa position, parce que le SVG porte le point d'ancrage
> brut et egui le coin supérieur gauche du galley — deux descriptions
> correctes et incomparables (`tests/formes.rs` couvre la position côté
> egui). Un détail qui a mordu tout de suite : le cadre de référence du
> message d'échec ne se calcule que sur les **segments**, le fond de la
> figure étant un rectangle qui couvre toute la zone par construction — l'y
> inclure rendait le témoin de R4 muet.
>
> **Une référence brisée se voit enfin.** R4 la comptait sans la dessiner,
> faute d'endroit où l'écrire ; il y en a un maintenant : elle figure en
> légende, en gris, annotée « — référence rompue ». C'est le minimum que
> demandait le §11.1 (« à afficher comme telle et jamais comme une série
> vide ») et pas plus : un bandeau d'avertissement appartient à l'UI réelle.
>
> **Trois plafonds posés, parce que rien ne borne ce qui vient d'un
> fichier**, dans la même veine que les six de `questions-ouvertes.md` §11 :
> `MAX_CELLS = 256` — une position de panneau est un `usize` libre, donc
> deux panneaux en `0` et en `usize::MAX` déclarent une grille indécoupable ;
> au-delà, les panneaux sont rangés dans l'ordre de déclaration dans la
> grille la plus carrée qui les contienne ; `MIN_CELL = 48 × 36 px` — un
> panneau plus petit est compté comme omis plutôt que dessiné en bouillie ;
> `MAX_DASHES = 20 000` par polyligne — un pointillé de trois pixels sur un
> chemin d'un million de pixels produirait des centaines de milliers de
> formes, donc au-delà, trait plein. Et un quatrième, cosmétique :
> `MAX_ENTRIES = 24` en légende, la dernière ligne disant combien manquent.
>
> **Un module de production qui n'en est pas tout à fait un :
> `plotyx_render::sample`.** Le graphe réaliste doit être à la fois ce que
> le test d'accord compare et ce que l'exemple SVG montre ; or le test vit
> dans `plotyx-render-egui` (il lui faut les deux backends) et l'exemple
> dans `plotyx-render-svg` (il ne doit pas voir egui), deux crates qui ne
> peuvent pas dépendre l'un de l'autre — et un `[dev-dependency]` ne
> traverse pas. Le graphe commun descend donc d'un cran, comme `plotyx-id`
> en R3 et `plotyx-render` en R4. Le dupliquer aurait garanti qu'il
> divergerait, et la divergence aurait porté précisément sur ce que le test
> prétend vérifier. À reconsidérer le jour où il existera un `.plx`
> d'exemple dans le dépôt : il n'y en a aucun aujourd'hui.
>
> **Reste ouvert.** Le rognage est celui de Plotters, c'est-à-dire une
> **troncature au rectangle** : un segment qui sort voit son extrémité
> ramenée au bord, pas coupée. Invisible tant que le cadrage suit les
> données, ce sera faux dès le premier zoom — donc à traiter en R7, par un
> vrai rognage (Liang-Barsky) au moment où le cadrage cesse de venir des
> données. Le coût de tessellation n'a pas été **re-mesuré** sur le pipeline
> réel : les 22,1 ms / 4,88 ms restent des chiffres de spike, et la phase
> ajoute des formes que le spike ne comptait pas (marqueurs, barres,
> légende). Enfin, si la cible est bien « 500 spectres à l'écran », ni le
> LTTB ni le feathering ne suffisent durablement, et la vraie question —
> réduire le **nombre de formes**, pas le nombre de points — reste entière ;
> R5 ne l'a pas tranchée, comme le §5.1 ci-dessous l'annonçait. **Devenue
> sans objet le 2026-09-12** : ce scénario suppose un rafraîchissement
> continu (l'acquisition live), tranchée hors périmètre du logiciel
> (`questions-ouvertes.md` §6) — un affichage statique de 500 spectres ne
> paie ce coût qu'une fois, `plotyx-app` ne redessinant que sur événement.

**Objectif** : couvrir le `PlotSpec` réel, **en ajoutant aux deux backends
à la fois**.

| # | Tâche | Livrable |
|---|---|---|
| 5.1 | Types de série : lignes, points, barres, histogrammes (`PlotType`) | Couverture des séries |
| 5.2 | Échelles logarithmiques, axes secondaires (`AxisScale`, `AxisSide`) | Couverture des axes |
| 5.3 | Légende (`LegendSpec`), styles de trait et de marqueur | Couverture du style |
| 5.4 | Panneaux multiples (`PanelSpec`) | Couverture de la mise en page |
| 5.5 | LTTB — **rendu nécessaire par la mesure 0.3** (22,1 ms contre un seuil de 8) | Décimation |
| 5.5 bis | `TessellationOptions::feathering` désactivé **pendant l'interaction souris**, rétabli au repos | Le levier qui pèse 78 % |
| 5.6 | Le test 4.5 étendu à chaque ajout | Divergence impossible |

> **La règle de la phase, en une ligne : aucune fonctionnalité n'entre dans
> un seul backend.** C'est ce qui rend le plan différent d'un plan qui
> ferait l'écran puis l'export — là, la divergence est structurellement
> empêchée, pas surveillée.

### 5.1 Ce que la mesure 0.3 dit vraiment — le seuil est franchi, mais le LTTB n'est pas l'outil

Le seuil était fixé d'avance, il est franchi (22,1 ms contre 8), donc le
LTTB entre en R5 : on conclut selon le seuil, sans le renégocier après coup.
**Mais la même mesure montre que le LTTB ne réglerait pas le cas mesuré**,
et il faut l'écrire avant de commencer la tâche 5.5.

Le LTTB décime *le long* d'une série. Dans le cas de charge du document —
500 spectres de 600 points — 600 points étalés sur 1 600 px sont déjà
sous-échantillonnés : il n'y a rien à décimer. La charge vient du **nombre
de séries**, pas de leur longueur. Le balayage le montre, à total de points
constant :

| séries | points/série | total | tessellation |
|---|---|---|---|
| 50 | 6 000 | 300 k | 18,3 ms — le LTTB ramènerait à 80 k pts ≈ 6,5 ms |
| 500 | 600 | 300 k | 22,7 ms — le LTTB ne retire rien |

Le coût est linéaire à **~75 ns/point**, et la barre des 8 ms tombe vers
**110 000 points au total**. Donc :

- le LTTB reste utile — il couvre le cas « quelques séries très longues »,
  qui est le cas de l'acquisition ;
- le **feathering** (tâche 5.5 bis) est le levier qui couvre le cas
  « beaucoup de séries », et il suffit à lui seul à repasser sous le seuil
  (4,88 ms) ;
- si la cible est bien « 500 spectres à l'écran », **aucun des deux ne suffit
  durablement** et R5 devra poser la vraie question : réduire le nombre de
  formes, pas le nombre de points. Ce n'est pas tranché ici.

> **Devenu sans objet le 2026-09-12.** Ce troisième cas suppose un
> rafraîchissement continu — l'écran redessiné en boucle sans qu'on y
> touche — et ça décrit l'acquisition live, désormais tranchée hors
> périmètre du logiciel (`questions-ouvertes.md` §6). Un affichage statique
> de 500 spectres, lui, ne paie ce coût **qu'une fois** : `plotyx-app` ne
> redessine que sur événement (entrée, script en cours, capture —
> `crates/plotyx-app/src/app.rs`), jamais en boucle. Rien à construire ici.

**Décidé le 2026-09-10 : le feathering se coupe à l'entrée en interaction
souris, et se rétablit au relâchement.** C'est le déclencheur qui rend
l'arbitrage gratuit plutôt que douloureux — le crénelage ne se voit pas sur
une image qui bouge, et c'est exactement pendant le déplacement que les
22,1 ms font mal. Au repos, la qualité est pleine ; sous la main, on repasse
à 4,88 ms, donc sous le seuil. Ce n'est pas un réglage exposé à
l'utilisateur mais un état du rendu, à câbler avec la gestion du drag de la
phase R7.

Conséquence sur le cache : couper le feathering change la tessellation, pas
les `Shape`. Le cache du §4.2 (clé `(revision, rect, pixels_per_point)`)
n'est donc **pas** invalidé par le basculement — c'est la retessellation qui
change de coût, et elle a lieu de toute façon à chaque frame. Rien à ajouter
à la clé.

Non mesuré, et à ne pas oublier : **l'upload GPU par frame** (aucun
affichage dans l'environnement de R0). Les 22,1 ms sont du CPU seul.

**Sortie de phase** : le critère de fin est vrai pour un graphe réaliste,
issu d'un fichier d'exemple.

---

## Phase R6 — Export PDF (≈2-3 jours)

> **Statut : close le 2026-09-12.** `svg2pdf` 0.13.0 branché dans
> `plotyx-render-svg` — pas de nouveau crate — via un module `pdf` de 108
> lignes et un fichier de tests de 139 lignes (3 tests). 521 tests au
> workspace (518 avant, **+3**), 9 `#[ignore]` inchangés, clippy sans
> avertissement, `cargo fmt` sans dérive nouvelle (les 25 échecs
> préexistants de §7.4 du guide de robustesse, sur les mêmes fichiers).
> **Critère de sortie tenu** : `cargo tree -p plotyx-plot` toujours limité à
> `plotyx-id`, `cargo tree -p plotyx-render-svg` toujours sans `egui`, ni
> `eframe`, ni `winit` — avec `svg2pdf` désormais dedans, comme attendu.
>
> **Le chemin est exactement celui annoncé, à un détail d'API près.**
> `document_to_pdf` reparse la `String` que [`render_to_string`] vient
> d'écrire avec `usvg::Tree::from_str`, puis appelle `svg2pdf::to_pdf` —
> `render_to_pdf` n'est qu'un appel à [`render_to_string`] suivi de celui-là.
> Ce n'est pas un troisième backend : aucun `DrawingBackend` n'est écrit ici,
> le module ne dessine rien.
>
> **Une décision prise en écrivant le module, qu'aucun document source
> n'annonçait : charger les polices système de la machine d'export avant de
> convertir.** Mesuré sur le graphe réaliste de R5 (110 balises `<text>`) :
> sans le moindre `fontdb`, `usvg` ne signale **aucune erreur** mais
> abandonne silencieusement tout le texte — le flux de contenu passe de 599
> opérateurs `Tj` à zéro. Un export PDF sans un seul titre, libellé d'axe ni
> graduation n'est pas un défaut de fidélité mineur, c'est un graphe muet ;
> `fontdb_mut().load_system_fonts()` est un simple parcours du système de
> fichiers, sans serveur graphique ni écran, donc sans coût pour la
> discipline du chantier. **Ceci ne tranche pas le point ouvert n°6
> d'`architecture-choisie.md` §5.2** (outliner ou non le texte de l'export) :
> ça choisit seulement, en attendant, l'alternative la moins mauvaise entre
> deux comportements non tranchés. Et ça déplace l'asymétrie déjà connue
> côté SVG plutôt que de la reproduire à l'identique — côté SVG, c'est la
> machine qui **lit** le fichier qui a besoin d'une police compatible ; côté
> PDF, c'est la machine qui **l'exporte**, une fois, après quoi `svg2pdf`
> embarque un sous-ensemble de la police choisie et le fichier devient
> autonome. Un PDF est donc, une fois produit, plus portable qu'un SVG — mais
> deux exports du même graphe sur deux machines peuvent embarquer deux
> polices différentes pour « sans-serif », avec des glyphes différents et
> sans effet sur la géométrie, puisque les axes sont déjà placés par la
> mesure `epaint` avant que `svg2pdf` n'intervienne. **À savoir pour la CI de
> ce dépôt** : `fmt`/`clippy`/`test` tournent sur `rust:1`, une image sans
> aucune police installée — les PDF produits pendant ces jobs n'ont donc
> aucun texte visible, silencieusement, exactement comme décrit ci-dessus.
> Sans conséquence sur la tâche 6.2, dont aucune assertion ne porte sur la
> présence de texte : seulement sur la structure PDF (nombre de pages,
> `MediaBox`) et la taille du fichier. Une machine de développement normale
> (celle qui fait tourner `plotyx-app`) a des polices, et le texte y
> apparaît.
>
> **Le test 6.2 compare trois sources, pas deux.** La taille demandée à
> l'export, celle réellement écrite dans l'attribut `width`/`height` du SVG,
> et la `MediaBox` que `lopdf` retrouve dans le PDF — comparer seulement les
> deux premières ne verrait pas une divergence entre [`render_to_string`] et
> [`render_to_pdf`], deux appels séparés au pipeline même s'ils partagent
> aujourd'hui tout leur code. Il porte sur le graphe réaliste de R5 (cinq
> panneaux, six types de tracé, échelle logarithmique), pas un canevas vide
> qui s'accorderait trivialement, et un second test fait varier la taille de
> figure pour vérifier que la `MediaBox` varie avec elle — sans lui, une
> fonction qui ignorerait `size` passerait le premier test pour une mauvaise
> raison. Un troisième test écrit sur disque, symétrique à
> `ecriture_sur_disque` côté SVG.
>
> **Un choix de dépendance de test, pas de production, documenté ici faute
> d'un autre endroit où le faire : `lopdf`**, en dev-dépendance et sans
> fonctionnalités par défaut (ni chiffrement, ni image, ni horodatage). « Le
> PDF s'ouvre » demande un analyseur qui ne partage aucun code avec
> `svg2pdf` ; une recherche d'octets dans notre propre sortie n'aurait prouvé
> que « `svg2pdf` a écrit ce qu'on lui a demandé d'écrire ». `lopdf` n'entre
> pas dans `architecture-choisie.md` §8 (qui ne couvre que les dépendances de
> production) et ne pèse sur aucune des deux frontières surveillées :
> `plotyx-render-svg/tests/frontiere.rs` reste vert, `lopdf` n'atteignant ni
> `egui`, ni `eframe`, ni `winit`.
>
> **Reste ouvert**, et volontairement hors de cette phase : le point ouvert
> n°6 d'`architecture-choisie.md` (outliner ou non le texte de l'export) ;
> l'exemple visuel (`cargo run -p plotyx-render-svg --example figure`) n'a
> pas été étendu au PDF — le tableau 6.1-6.2 n'en demandait pas, et R4/R5
> l'ont déjà donné côté SVG ; l'inlining de fragments Typst outlinés, seule
> façon de rendre le choix de police d'export sans objet, est la phase R8.

| # | Tâche | Livrable |
|---|---|---|
| 6.1 | `svg2pdf` 0.13.0 branché sur la sortie de `plotyx-render-svg` | PDF |
| 6.2 | Test : le PDF s'ouvre, ses dimensions correspondent au SVG | Non-régression |

Courte parce que tout le travail difficile est déjà fait en amont : le PDF
est une conversion du SVG, pas un troisième backend.

---

## Phase R7 — Interaction (≈2 semaines)

> **Statut : close le 2026-09-13.** Les quatre tâches du tableau sont faites
> et testées ; le rognage réel (point 2 des contraintes d'ouverture de la
> phase) reste reporté, comme R5 le prévoyait déjà en ouvrant cette phase —
> précisions et raisons plus bas, dans le même esprit que R5 documentait son
> propre report vers R7. 550 tests au workspace (530 avant, **+20**),
> 9 `#[ignore]` inchangés, clippy sans avertissement (features `typst` et
> `python` comprises), `cargo fmt` sans dérive nouvelle (exactement les 25
> échecs préexistants du guide de robustesse, sur les mêmes 15 fichiers —
> vérifié en revenant dessus après un `cargo fmt --all` complet, qui les
> aurait sinon corrigés en silence et fait disparaître ce que R4-R6
> documentaient comme un état connu, pas une propriété à réparer au passage).
> **Critères de sortie tenus** : `cargo tree -p plotyx-plot` toujours limité à
> `plotyx-id`, `cargo tree -p plotyx-render-svg` toujours sans `egui`, `eframe`
> ni `winit`.
>
> **Deux crates gagnent un module, aucun n'en gagne un dépendant.**
> `plotyx-render::interact` (nouveau, 300 lignes dont la moitié de tests) porte
> `PanelTransform` — la correspondance pixel ↔ données d'un panneau tracé,
> [`DrawnPanel::pixel_origin`]/[`pixel_size`] ajoutés à cet effet — et
> `nearest_on_curve`, l'équivalent du couple `ReverseCoordTranslate` +
> `min_by_key` de la tâche 7.2. **Générique sur le backend, comme le reste de
> `plotyx-render`**, alors que `plotyx-render-svg` n'en a aujourd'hui aucun
> usage (un SVG statique n'a pas de curseur) : le placer à côté de
> `plotyx-render-egui` l'aurait rendu inaccessible le jour où un lecteur SVG
> interactif en aurait besoin, pour un coût nul aujourd'hui — exactement le
> raisonnement qui avait fait descendre `plotyx-render` lui-même en R4.
> `plotyx-render-egui::view` (nouveau) porte `ViewState`/`Framing` : l'état de
> vue éphémère, jamais une mutation du modèle, jamais posé sur le `PlotSpec`
> stocké — il s'applique à un **clone**, juste avant résolution, en réutilisant
> tel quel le mécanisme du cadrage explicite qu'`AxisSpec::has_explicit_range`
> offrait déjà depuis R5 (`crates/plotyx-render/tests/geometrie.rs::un_cadrage_explicite_prime_sur_les_donnees`).
> Rien de neuf n'a donc été inventé pour le cadrage lui-même, seulement pour
> ce qui le déclenche.
>
> **La clé de cache gagne un quatrième terme, `view_generation: u64`** —
> un compteur que `ViewState` avance à chaque zoom, pan ou rectangle élastique
> commis, pas les bornes elles-mêmes (variables en nombre, par panneau, en
> `f64` — peu `Copy`-friendly, et sans intérêt pour la clé : seul le fait
> qu'elles aient changé compte). C'est la règle inverse de celle du
> *feathering*, et volontairement : le zoom/pan **change la géométrie
> tracée**, le feathering ne change que la tessellation — les deux moitiés de
> la phrase d'`architecture-choisie.md` §4.2 sont maintenant exercées chacune
> par un terme de la clé, l'une en l'ajoutant, l'autre en refusant de le
> faire.
>
> **`GraphView::paint` change de signature : `&egui::Ui`, plus `&Painter`.**
> La navigation a besoin d'un `Response` — molette, glissé, position du
> curseur — qu'un `Painter` seul ne porte pas ; `paint` fait maintenant
> lui-même `ui.interact(rect, self.id, Sense::click_and_drag())`, avec un
> `egui::Id` propre à chaque `GraphView` (compteur atomique) plutôt que dérivé
> du chemin d'`Ui` de l'appelant, pour que deux vues peintes depuis le même
> point du code ne partagent pas le même geste. Trois fichiers de tests et
> `plotyx-app/src/app.rs` mis à jour en conséquence ; aucune régression, les
> anciens tests passent inchangés pour le reste (`ui.painter()` devient `ui`,
> egui reborrow `&mut Ui` en `&Ui` tout seul).
>
> **Un piège trouvé en écrivant le test du rectangle élastique, pas en le
> lisant dans une doc.** `egui` diffère la décision « clic ou glissé » sur un
> widget sensible aux deux jusqu'à ce que le mouvement la tranche
> (`egui::interaction::interact`, commentaire *"the mouse first is pressed,
> it could be either [...] so we postpone the decision"*) : `drag_started_by`
> ne devient vrai qu'à l'image où la décision tombe, et
> `interact_pointer_pos()` y porte déjà la position **courante**, pas celle de
> la pression initiale — capturer le coin de départ du rectangle à cet
> instant-là le collabait au point d'arrivée (rectangle de largeur nulle,
> jamais commis). `drag_delta()` reste fiable quelle que soit la date de la
> décision (c'est le mouvement de **cette** image) : `pos - drag_delta()`
> retrouve le point de départ réel, sans dépendre de `drag_started_by` du
> tout. Le pan (bouton principal) n'y avait pas été exposé par hasard : il
> utilisait déjà `pos - delta` pour retrouver le panneau visé, ce qui est la
> même correction appliquée un cran plus tôt sans que le piège n'ait été vu
> comme tel sur le moment.
>
> **Une décision arbitraire, à écrire parce que rien ne l'imposait** : le
> bouton principal glisse (pan), le bouton secondaire trace le rectangle de
> zoom élastique. Aucun document source ne tranchait ce choix ; matplotlib
> et la plupart des outils scientifiques attachent plutôt le rectangle à une
> touche modificatrice sur le bouton principal, ce qui resterait à faire si
> `plotyx-app` veut un jour s'aligner dessus — le mécanisme (`PanelTransform`,
> `ViewState`) est indifférent au choix de déclencheur.
>
> **Une désignation limitée à l'axe principal**, pas à l'axe secondaire
> (droit) : une série de droite vivrait sur un second `PanelTransform` (bornes
> `y2_range`, échelle `right.scale`) qu'il faudrait construire à côté, et rien
> n'en a aujourd'hui l'usage (`plotyx-app` n'affiche encore qu'un banc, sans
> axe secondaire visible). Écrit dans `PointHit` et dans `crate::view`, pas
> seulement ici, pour ne pas se reperdre.
>
> **Une extension non demandée par le tableau, ajoutée parce que son absence
> aurait rendu la démonstration inutilisable** : double-clic pour réinitialiser
> le zoom. Sans lui, un zoom posé pendant la session `plotyx-app` n'a aucun
> chemin de retour sinon relancer le banc — `GraphView::reset_view()` est
> exposée publiquement, câblée au double-clic dans `handle_navigation`, et
> testée comme les quatre tâches du tableau.
>
> **Le rognage réel (Liang-Barsky), reporté une deuxième fois — mais
> confirmé, pas juste renvoyé.** R5 notait déjà que la troncature au
> rectangle de Plotters (`element/mod.rs` : `BackendCoordOnly::map` appelle
> `rect.truncate(coord_trans.translate(from))`, chemin qu'empruntent tous les
> `area.draw(&PathElement::new(…))` de `plotyx_render::paint`) deviendrait
> visible « dès le premier zoom ». R7 le vérifie plutôt que de le répéter :
> un cadrage explicite étroit fait effectivement extrapoler la plupart des
> points d'une série à des coordonnées pixel très éloignées, que Plotters
> ramène ensuite **par composante** au bord du panneau — un segment qui sort
> en diagonale se couche sur le bord au lieu d'être coupé à l'intersection
> réelle. Un nouveau test,
> `un_cadrage_etroit_zoome_reste_borne_au_rectangle`
> (`plotyx-render-egui/tests/formes.rs`), le déclenche et vérifie ce qui doit
> rester vrai malgré le défaut : aucune forme n'échappe au rectangle alloué,
> aucune coordonnée `NaN` ni infinie — la dégradation reste bornée, pas
> corrompue. **Reporté quand même**, faute d'un temps qui aurait dû couvrir
> aussi la division d'une polyligne en plusieurs `PathElement` disjoints
> quand un vrai rognage lui fait quitter puis retrouver le panneau plusieurs
> fois, l'interaction avec les motifs de tirets et les moustaches d'erreur, et
> la ré-vérification du test d'accord sur un graphe zoomé pour les deux
> backends — un sous-chantier à part entière, pas un correctif ponctuel. Il
> resterait, comme documenté depuis R5, une fonction de `plotyx-render`
> générique sur le backend, pas un correctif d'un seul des deux.
>
> **Reste ouvert**, en plus du rognage réel ci-dessus : le zoom/pan et la
> désignation de point ne couvrent que l'axe principal (voir plus haut) ; le
> déclencheur du rectangle élastique (bouton secondaire) n'est pas
> configurable et n'a pas été confronté à un utilisateur réel ; et
> `feathering::apply` reste un réglage de tout l'`egui::Context`, donc commun
> à toutes les vues si `plotyx-app` en affichait un jour plusieurs à la
> fois — situation déjà documentée comme telle par R5.5 bis, inchangée ici
> faute d'un second `GraphView` simultané pour l'exercer.

| # | Tâche | Livrable |
|---|---|---|
| 7.1 | Zoom molette, pan au glisser | Navigation |
| 7.2 | Sélection de point : `ReverseCoordTranslate` + `min_by_key` sur la distance | Désignation |
| 7.3 | Réticule, peint en `Shape` **par-dessus le cache** — sans l'invalider | Le pendant du blitting `copy_from_bbox` de la version Python |
| 7.4 | Rectangle de zoom élastique | Navigation |

Référence de couverture : `ui/mpl_renderer.py` (852 l.), qui dit ce qu'un
renderer complet doit savoir faire. **Ne pas porter son architecture** —
elle est faite pour Matplotlib et pour Qt.

> **Ce que 7.3 doit préserver** : bouger le réticule ne doit pas invalider
> le cache de `Shape`. C'est la propriété qui remplace le blitting de la
> version Python, et elle est gratuite si le découpage du §4.2 de
> l'architecture est respecté — elle cesse de l'être si le réticule entre
> dans le pipeline Plotters.

---

## Phase R8 — Typst (≈2 semaines, peut se solder par un abandon)

**Objectif** : les notations scientifiques, derrière un *feature flag*.

**Condition d'ouverture** : la mesure 0.2 est sous le seuil fixé en R0. Si
elle ne l'est pas, cette phase ne s'ouvre pas et le chantier se clôt à R7 —
c'est un résultat, pas un échec.

| # | Tâche | Livrable |
|---|---|---|
| 8.1 | `World` Typst custom, fonts embarquées, `include_system_fonts(false)` | ~100-150 lignes |
| 8.2 | Cache : hash de formule **et** `pixels_per_point` | Latence amortie |
| 8.3 | Chemin écran : `typst-render` → RGBA → texture → `Shape::Mesh` | Labels à l'écran |
| 8.4 | Chemin export : `typst-svg` inliné dans le SVG de Plotters | Labels à l'export |
| 8.5 | `estimate_text_size` interroge Typst pour les labels Typst, l'`epaint::Fonts` pour les autres | **L'invariant du §4.3** |
| 8.6 | Feature `typst` absente de `default` ; sans elle, repli sur `Shape::Text` brut | CI verte sans Typst |

### 8.1 Ce que la fusion SVG coûte réellement — vérifié, pas supposé

Le cauchemar habituel de la fusion de deux SVG (collision d'`id` entre
`<defs>`) **n'a pas lieu ici** : Typst nomme ses définitions par **hash de
contenu** (`DedupId(char, hash128)`), donc deux fragments se dédupliquent
au lieu d'entrer en collision ; et `plotters-svg` n'émet **aucun** `id`,
`defs` ou `clipPath`. Reste du travail mécanique : envelopper chaque
fragment dans un `<g transform>` (Typst en points, Plotters en pixels), et
injecter dans la `String` rendue par `SVGBackend::with_string`.

### 8.2 L'inversion à ne pas oublier

Dans l'export, la partie difficile est le **chiffre des graduations**, pas
la formule : Typst sort ses glyphes en contours, donc autonomes et
portables, tandis que `plotters-svg` émet `<text font-family>`, qui dépend
de la font installée chez le lecteur. C'est le point ouvert n°6 de
`architecture-choisie.md`, à trancher avant le premier export destiné à une
publication.

---

## 9 — Ce que ce plan ne couvre pas, volontairement

- **Commandes d'édition du spec** — étendre les huit primitives de
  `plotyx-commands` pour que « changer une couleur » soit annulable. À
  faire avec le panneau Properties sous les yeux, pas avant : c'est lui qui
  dira quelles primitives sont les bonnes. Port de `edit.py` inclus.
- **UI réelle** — docking, grille éditable, panneau Properties. Chantier
  séparé. Le renderer n'en a pas besoin : il se teste **sans serveur
  graphique**, par l'export SVG. C'est un argument de plus pour le faire
  d'abord.
- **Rendu GPU** — possible sans rien casser (`Shape::Callback` /
  `egui_wgpu::CallbackTrait` : les séries dans un shader, l'habillage en
  `Shape`), mais hors périmètre. À rouvrir **seulement** si la mesure 0.3
  puis l'usage réel le réclament, et en sachant que cela engage wgpu comme
  backend unique.
- **Fitting** — toujours à définir séparément.
- **Format `.plx` final (Zarr)** — R3 s'ajoute au format provisoire sans le
  remplacer.

---

## 10 — Les deux séquencements écartés

Gardés avec leur raison, comme partout ailleurs dans ce dossier.

### « L'écran d'abord »

R0 → R1 → R2 → R3 → backend egui complet **et** interaction → export
ensuite. Une courbe qui bouge à la souris en ~6 semaines au lieu de ~8.

**Écarté** : l'accord des mesures de texte arriverait **après** que le
backend egui ait figé ses choix, et l'export devrait rattraper toute la
surface d'un coup. C'est exactement la configuration où un décalage d'axes
passe inaperçu — le seul défaut de ce chantier qui ne lève pas d'erreur.

> **Si ce plan est repris un jour**, une mitigation le rend acceptable :
> écrire `estimate_text_size` sur l'`epaint::Fonts` partagée **dès la
> première ligne** du backend egui, même sans backend SVG en face. Coût nul
> sur le moment, et l'invariant survit.

### « La verticale mince »

Un `PlotSpec` minuscule, puis les deux backends **plus** le PDF **plus**
l'interaction minimale, soit la chaîne complète de bout en bout très tôt,
puis élargissement en largeur.

**Écarté de peu.** C'est la méthode du socle, et elle est bonne. Le plan
retenu lui emprunte d'ailleurs sa meilleure idée (tâche 4.6). Ce qui a fait
pencher : elle disperse l'effort sur cinq fronts avant qu'aucun ne soit
solide, alors que R4 concentre tout sur l'unique invariant dont l'échec
serait silencieux.

---

## 11 — Points restant à trancher

Aucun ne bloque une phase. Deux sont des mesures de R0, un est une décision
de produit.

1. ~~Seuil d'abandon de Typst.~~ **Fixé puis mesuré le 2026-09-10** : seuil
   à 150 ms, mesure à 0,156 ms — **Typst retenu**.
2. ~~Seuil d'entrée du LTTB.~~ **Fixé puis mesuré le 2026-09-10** : seuil à
   8 ms/frame, mesure à 22,1 ms — **LTTB en R5**, avec la nuance du §5.1 sur
   ce que le LTTB ne règle pas.
3. **Texte de l'export : tout outliner, ou assumer l'asymétrie ?**
   (`architecture-choisie.md` point ouvert n°6) — à trancher avant le
   premier export destiné à une publication. **Précision du 2026-09-12
   (R6)** : côté PDF, l'asymétrie n'est pas celle du SVG (la machine qui
   **lit**) mais celle de la machine qui **exporte** — sans police système
   au moment de la conversion, `svg2pdf` abandonne tout le texte sans la
   moindre erreur (mesuré : 599 opérateurs `Tj` sur le graphe réaliste de
   R5, zéro sans police). `plotyx-render-svg` charge donc les polices
   système par défaut ; ça ne tranche pas ce point, ça évite seulement
   l'alternative strictement pire.
4. ~~Compteur de modification par colonne.~~ **Tranché et construit le
   2026-09-10** : `Column::version()` (`architecture-choisie.md` point
   ouvert n°8, clos).

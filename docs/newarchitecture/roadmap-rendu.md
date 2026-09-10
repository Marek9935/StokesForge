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

**Objectif** : couvrir le `PlotSpec` réel, **en ajoutant aux deux backends
à la fois**.

| # | Tâche | Livrable |
|---|---|---|
| 5.1 | Types de série : lignes, points, barres, histogrammes (`PlotType`) | Couverture des séries |
| 5.2 | Échelles logarithmiques, axes secondaires (`AxisScale`, `AxisSide`) | Couverture des axes |
| 5.3 | Légende (`LegendSpec`), styles de trait et de marqueur | Couverture du style |
| 5.4 | Panneaux multiples (`PanelSpec`) | Couverture de la mise en page |
| 5.5 | LTTB — **rendu nécessaire par la mesure 0.3** (22,1 ms contre un seuil de 8) | Décimation |
| 5.5 bis | `TessellationOptions::feathering` conditionnel : le désactiver au-delà d'un seuil de densité, ou pendant un drag | Le levier qui pèse 78 % |
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

Non mesuré, et à ne pas oublier : **l'upload GPU par frame** (aucun
affichage dans l'environnement de R0). Les 22,1 ms sont du CPU seul.

**Sortie de phase** : le critère de fin est vrai pour un graphe réaliste,
issu d'un fichier d'exemple.

---

## Phase R6 — Export PDF (≈2-3 jours)

| # | Tâche | Livrable |
|---|---|---|
| 6.1 | `svg2pdf` 0.13.0 branché sur la sortie de `plotyx-render-svg` | PDF |
| 6.2 | Test : le PDF s'ouvre, ses dimensions correspondent au SVG | Non-régression |

Courte parce que tout le travail difficile est déjà fait en amont : le PDF
est une conversion du SVG, pas un troisième backend.

---

## Phase R7 — Interaction (≈2 semaines)

**Objectif** : le graphe devient manipulable.

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
   premier export destiné à une publication.
4. ~~Compteur de modification par colonne.~~ **Tranché et construit le
   2026-09-10** : `Column::version()` (`architecture-choisie.md` point
   ouvert n°8, clos).

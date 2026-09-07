# Cahier des charges technique — cœur Rust jusqu'à l'API de script

Statut : **spécification arrêtée le 2026-09-08**, implémentation non
commencée au-delà du squelette de phase 0. Ce document fait foi sur
l'architecture ; il complète `roadmap.md` (qui dit *quand*) en disant
*comment* et *pourquoi pas autrement*. Comme `resume-decisions.md` et
`format-fichier-plx.md`, il conserve les options écartées et leur raison —
pour ne pas rejouer les mêmes débats dans six mois.

Il ne couvre **pas** l'UI réelle : elle sera reprise entièrement en
conversation (cf. §1), et les décisions ci-dessous sont prises pour ne rien
lui préempter.

---

## 1. Périmètre

Suit la chaîne de dépendance établie le 2026-09-07 (étapes 1 à 6) :

| # | Étape | Dans ce document |
|---|---|---|
| 1 | Modèle de données | oui — §4 |
| 2 | Bus de commandes / undo | oui — §5 |
| 3 | Persistance | oui — §7 |
| 4 | Coquille UI minimale | oui — §11 |
| 5 | Pipeline Python embarqué + renderer Matplotlib | oui — §8, §9 |
| 6 | API de script | oui — §10 |
| 7 | UI réelle (docking, grille éditable, Properties) | **non** |

**Explicitement hors périmètre**, place réservée mais rien à construire :

- l'**import de fichiers** (`plotyx-io`) — absent de la chaîne des étapes
  1-6 ; les 10 fixtures de `legacy-python/tests/fixtures/import/` restent
  la spécification du jour où il arrivera ;
- le **format `.plx` final** (Zarr) — voir §7, la persistance de ce
  périmètre est délibérément provisoire ;
- le **mode « visualiseur léger »** (ex-PyQtGraph) — question toujours
  ouverte, cf. §14.

---

## 2. Les quatre décisions d'architecture

### 2.1 Undo — hybride structure / données

**Décision.** L'historique manipule deux natures d'entrée :

- **structure** (arbre, noms, ordre, métadonnées, `PlotSpec`) → *snapshot
  persistant* : muter produit un nouvel arbre qui partage tout l'inchangé,
  annuler remet l'ancien pointeur ;
- **données** (contenu numérique et texte des colonnes) → *delta* :
  la commande enregistre les anciennes valeurs des seules cellules
  écrites.

**Ce que ça achète.** Les inverses disparaissent là où ils sont difficiles
à écrire — déplacements, collisions de noms, suppression/restauration — et
avec eux **toute la machinerie de durée de vie des objets supprimés**
(décision 4 de la phase 2 Python : miroir de pile, `release()`, gestion de
l'éviction, tests `gc`/`weakref`). L'ancien arbre retient l'objet
supprimé ; il n'y a rien à libérer explicitement. Les inverses qui restent
sont ceux qui sont presque impossibles à rater : réécrire les anciennes
valeurs de N cellules.

**Ce que ça coûte, sans euphémisme.** Deux natures d'entrée dans une seule
pile, avec une discipline d'ordre à tenir (§5.3). Et l'hybride
**n'élimine pas** la classe de bugs « inverse faux » — il la confine à la
moitié facile. Le test de propriété du §12 reste obligatoire.

**Écarté — delta pur** (le port mécanique de Python) : un seul mécanisme,
les dix types de commandes et leurs tests se transposent presque tels
quels, mémoire minimale. Rejeté parce qu'il oblige à réécrire en Rust la
machinerie de durée de vie ci-dessus, et parce que ce projet a déjà produit
empiriquement un bug de cette classe (`insert_rows` / `delete_rows` non
inverses, cf. `roadmap.md` §2.4) sur la moitié structurelle, précisément
celle que l'hybride supprime.

**Écarté — snapshot intégral** (modèle entièrement persistant, undo
impossible à écrire faux, un seul mécanisme) : séduisant, mais il n'est
tenable que si éditer une cellule ne copie pas toute la colonne, donc il
impose de **chunker** les colonnes. Or les étapes 5 et 6 exigent des
colonnes **contiguës** — `rust-numpy` ne peut passer à Matplotlib qu'un
buffer contigu (§9.2), et `pyx.column()` doit rendre une vue numpy sur le
buffer réel (`cahier-des-charges-api-script.md` §2.3). Une colonne chunkée
ne se voit pas comme un seul tableau numpy : chunker casse le zéro-copie
des deux côtés à la fois. Le motif de rejet est **la perte du zéro-copie,
pas la RAM** — chiffres à l'appui au §2.1.1, l'argument mémoire ne
départage pas.

#### 2.1.1 Ce que coûte réellement l'historique en mémoire

Projet lourd de référence : 50 colonnes × 1M points = **400 Mo de données**,
arbre de ~2000 nœuds, 100 entrées d'historique.

| | entrée structurelle | entrée de données (1 cellule) | pile de 100 |
|---|---|---|---|
| Delta pur | ~200 o | ~16 o | < 1 Mo |
| **Hybride (retenu)** | taille de l'arbre, ~100 Ko | ~16 o | **~10 Mo** |
| Snapshot intégral + chunks | ~100 Ko | 1 chunk : 32 à 512 Ko | 3 à 50 Mo |

Face aux 400 Mo de données, aucune de ces piles n'est un problème.
L'historique est **entièrement en RAM** dans les trois cas, comme
aujourd'hui avec `QUndoStack` ; aucun déversement sur disque n'est prévu.

Le seul vrai piège mémoire est **commun aux trois** et doit être documenté
dans l'UI : supprimer un classeur de 400 Mo garde ces 400 Mo vivants tant
que l'entrée d'annulation est sur la pile. C'est nécessaire — sans ça, pas
d'annulation possible. Le garde-fou est la **limite d'historique** (§5.4).

### 2.2 Propriété du modèle — acteur sur thread dédié

**Décision.** Le `Project` vit sur son propre thread, dans `plotyx-session`.
L'UI et la console de script sont deux clients symétriques qui envoient des
requêtes et reçoivent des snapshots immuables.

**Pourquoi.** `cahier-des-charges-api-script.md` §5 exige un script sur un
thread dédié, jamais le thread UI. Si le modèle appartenait au thread UI,
chaque appel de l'API traverserait ce thread et attendrait son tour : un
script qui écrit 10 000 valeurs serait cadencé par la boucle de rendu et
calerait pendant qu'un graphe de 1M points se peint. Avec un acteur, le
script n'attend jamais une frame.

**Bénéfice structurel.** La règle d'or — aucune mutation hors du bus —
cesse d'être une convention testée pour devenir une **propriété de
propriété** : l'acteur détient le seul `&mut Project` du processus. Aucun
autre crate ne peut muter, non par discipline mais parce qu'il n'a pas la
valeur. Le test Python `test_no_mutation_outside_command.py` n'a plus
d'équivalent à écrire — il n'a plus d'objet.

**Ce que ça coûte.** Une centaine de lignes de plomberie (canaux,
boucle) — prix bas précisément parce que la structure est persistante :
l'UI garde son dernier `Arc` d'arbre et peint à 60 fps sans jamais
solliciter l'acteur.

**Écarté — modèle sur le thread UI + workers** (ce que fait Python) :
plus simple, mais couple la console de script à la boucle de rendu, dans un
périmètre où cette console est justement une livraison attendue. Décider
maintenant évite un refactor à mi-parcours.

**Écarté — `Arc<RwLock<Project>>`** : contention et deadlocks, et rendu
inutile par le modèle persistant — on partage des snapshots immuables, pas
un verrou.

### 2.3 Rendu — PyO3 + Matplotlib, par élimination

**Décision.** Un seul moteur : Matplotlib, dans un interpréteur Python
embarqué via PyO3, conformément à `resume-decisions.md` §4.

Ce n'est pas une préférence, c'est le résidu d'une élimination provoquée
par le périmètre. **Parce que l'étape 6 est dedans**, la console de script
impose de toute façon un interpréteur Python embarqué :

- « rendu natif Rust intégral » ne fait donc plus économiser l'empaquetage
  Python — on en porterait le poids **sans** profiter de Matplotlib ;
- « hybride, Matplotlib en sous-processus pour l'export » perd son seul
  argument (éviter PyO3), puisque PyO3 sera présent pour la console ; il
  ramènerait en prime les deux moteurs de rendu et le « ce que je vois
  n'est pas ce que j'exporte » que la phase 6 vient justement de corriger.

La seule manière de rouvrir la question serait de troquer Python contre un
langage de script natif (Rhai, Lua), ce que
`cahier-des-charges-api-script.md` §8 exclut explicitement (compatibilité
des scripts déjà écrits, accès à numpy).

**Conséquence directe** : l'interpréteur ayant deux clients (`render-mpl`
et `script`) et ne pouvant être initialisé qu'une fois, un crate
`plotyx-python` en possède seul le cycle de vie (§8).

### 2.4 Découpage — crates fins

**Décision.** Neuf crates (§3), frontières imposées par le compilateur.

**Pourquoi.** C'est le mécanisme qui rend vérifiable l'équivalent Rust de
« aucun Qt dans `core` » : aucun PyO3, aucun egui dans le cœur. Et tout ce
qui touche à Python vit derrière un *feature flag*, ce qui garde
`cargo test` et la CI utilisables **sur une machine sans Python installé** —
propriété non négociable pour ne pas rendre le socle otage du chantier le
plus lourd.

**Écarté — 3-4 crates** : moins de friction à écrire, mais plus rien
n'empêche PyO3 ou egui de remonter dans le cœur, et la CI exigerait Python
partout.

---

## 3. Carte des crates

| Crate | Rôle | Dépend de | Python ? |
|---|---|---|---|
| `plotyx-core` | Modèle : arbre, nœuds, colonnes. Aucune mutation publique. | — | non |
| `plotyx-commands` | Commandes et pile d'historique à deux natures | `core` | non |
| `plotyx-session` | **L'acteur** : possède le `Project`, sert les clients | `core`, `commands`, `persistence` | non |
| `plotyx-persistence` | `save` / `load` d'un `Project` complet | `core` | non |
| `plotyx-plot` | `PlotSpec` neutre, sans moteur de rendu | `core` | non |
| `plotyx-python` | Cycle de vie de l'interpréteur embarqué et du GIL | — | **oui** |
| `plotyx-render-mpl` | Spec → figure Matplotlib → buffer RGBA | `plot`, `core`, `python` | **oui** |
| `plotyx-script` | Surface `plotyx.api`, thread de script | `session`, `python` | **oui** |
| `plotyx-app` | Coquille egui + binaire | tous | via features |

`plotyx-cli` (squelette de phase 0) reste comme binaire headless de test.
`plotyx-io` (import) n'est pas créé dans ce périmètre.

Deux écarts par rapport au découpage esquissé avant décision, à noter :
`plotyx-session` s'ajoute (conséquence directe de §2.2), et `plotyx-io`
disparaît du périmètre.

---

## 4. Modèle — `plotyx-core`

### 4.1 Identité des nœuds — compteur monotone, **pas** `slotmap`

**Écart assumé avec `roadmap.md` §0.4**, qui proposait `slotmap` /
`generational-arena`. Le choix de l'undo hybride l'invalide :

une arène à réutilisation de slots peut allouer `(slot 3, génération 2)`
pour un nouveau nœud après qu'un nœud `(slot 3, génération 1)` a été
retiré de l'arbre courant. Annuler la suppression réinsérerait alors
`(3, v1)` dans un arbre où le slot 3 est déjà pris — collision d'identité
introduite par l'annulation elle-même.

**Retenu** : `NodeId(u64)`, compteur monotone, **jamais réutilisé**, stocké
dans une `HashMap<NodeId, Node>`. Une restauration ne peut structurellement
pas entrer en collision. Le compteur (sa valeur haute) est **persisté avec
le fichier**, sans quoi un rechargement rouvrirait le risque.

Ce `NodeId` tient les trois rôles à la fois : clé d'arbre, clé opaque
exposée aux scripts (`cahier-des-charges-api-script.md` §3) et référence
de série dans un `PlotSpec`.

*Alternative gardée en réserve* : UUID v4, seulement si l'identité doit un
jour survivre à la fusion de deux projets distincts — besoin inexistant
aujourd'hui, et un `u64` est plus petit et plus rapide.

### 4.2 Arbre

`Arc<Tree>` où `Tree` contient la `HashMap<NodeId, Node>` et la racine.
Muter = cloner l'arbre (`Arc::make_mut`) et remplacer les nœuds touchés.
Un clone coûte la carte des nœuds — quelques centaines de Ko au pire, cf.
§2.1.1 — les charges utiles étant derrière `Arc`, elles ne sont pas
copiées.

Structure conservée à l'identique du modèle Python :
`Folder → Workbook → Worksheet → Column`, avec `Graph` / `Matrix` / `Note`
en nœuds frères du `Workbook` ; grammaire de chemins `[Book1]Sheet1!ColA`
reprise de `core/paths.py`.

*Alternative écartée* : `im` / `rpds` (cartes persistantes, clone en
O(log n)). Inutile à cette échelle — l'arbre d'un document se compte en
milliers de nœuds, pas en millions — et c'est une dépendance de moins.

### 4.3 Colonnes — contiguës, jamais chunkées

| Type | Représentation | Valeur manquante |
|---|---|---|
| Numérique / date-heure | `Arc<Vec<f64>>` | `f64::NAN` |
| Texte | `Arc<Vec<Option<String>>>` | `None`, distinct de `Some("")` |

La contiguïté est une **exigence contractuelle**, pas un détail
d'implémentation : elle conditionne le zéro-copie vers Matplotlib (§9.2)
et vers les scripts (§10). Aucune évolution ne doit la remettre en cause
sans rouvrir §2.1.

Écriture par `Arc::make_mut` : la copie n'a lieu **que** si un worker de
rendu ou un script détient encore ce buffer. En édition courante, sans
lecteur concurrent, l'écriture est en place et gratuite.

Longueurs hétérogènes tolérées entre colonnes d'une même feuille
(décision 2 de la phase 1 Python, conservée — l'argument ne dépend pas du
langage).

### 4.4 Invariants à tester dès la phase 1

1. Une colonne de longueur L n'a pas de ligne au-delà de L : insertion et
   suppression de lignes ignorent **le même ensemble** de colonnes trop
   courtes. C'est l'invariant dont la violation a produit le bug
   `insert_rows` / `delete_rows` côté Python (`roadmap.md` §2.4) — à
   porter comme test, pas à redécouvrir.
2. Unicité des noms entre frères d'un même parent.
3. Aucun `NodeId` réutilisé sur toute la vie d'un projet, rechargement
   compris.

---

## 5. Historique — `plotyx-commands`

### 5.1 La pile à deux natures

```
enum Entry {
    Structural { before: Arc<Tree>, after: Arc<Tree>, label: String },
    Data       { delta: Box<dyn DataDelta>, label: String },
    Macro      { entries: Vec<Entry>, label: String },
}
```

`before` et `after` partagent presque tout : le coût d'en garder les deux
est un pointeur, et le redo devient symétrique de l'undo.

`DataDelta` reste un trait avec `apply` / `unapply` — c'est la moitié
« facile » assumée au §2.1.

### 5.2 Commandes

Mêmes primitives que Python : `SetCells`, `AddColumn`, `DeleteColumn`,
`Rename`, `Move`, `SetColumnType`, `InsertRows`, `DeleteRows`, et les
commandes de `PlotSpec`.

**Application avant empilement** : la commande est appliquée par le bus,
et n'est empilée que si elle réussit. Un échec (nom en double, index hors
bornes) remonte en `Err` et la pile n'a pas bougé. Reprise de la
décision 3 de la phase 2 Python, dont la justification change : Python
s'en servait pour éviter qu'une exception traverse la frontière C++ de Qt ;
ici c'est simplement le comportement correct d'une API à `Result`.

`SetColumnType` reste le **seul** cas à snapshot de données documenté : la
conversion de type est destructrice dans les deux sens, aucun delta ne la
reconstruit.

### 5.3 Macros — la discipline qui fait tenir l'hybride

Une macro s'annule en rejouant ses entrées **strictement en ordre inverse
d'insertion**, quelles que soient leurs natures. C'est la seule règle qui
fait composer snapshots de structure et deltas de données : un snapshot
restauré hors d'ordre annulerait silencieusement un delta appliqué
après lui.

À tester explicitement sur une macro mixte (une modification de structure
et une écriture de cellules dans la même macro, dans les deux ordres).

`abort_macro()` : dépiler et annuler les entrées de la macro en cours,
puis les jeter. Aucun contournement nécessaire — la contrainte Qt qui
imposait, côté Python, de pousser une commande se déclarant obsolète
pendant son `redo()` (décision 6) n'existe pas ici.

### 5.4 Limite d'historique

Configurable, par défaut bornée. C'est le seul garde-fou contre le piège
mémoire du §2.1.1 (données retenues par une entrée de suppression).
Éviction en tête de pile ; une entrée évincée relâche ses `Arc`, et les
buffers qu'elle était seule à retenir sont libérés à ce moment.

---

## 6. Session — `plotyx-session`, l'acteur

### 6.1 Protocole

Un canal de requêtes (FIFO, donc ordre déterministe), une réponse par
requête :

```
enum Request {
    Execute { command: Command, reply: Reply<Result<Revision, CommandError>> },
    Undo { reply: … }, Redo { reply: … },
    BeginMacro { label: String }, EndMacro, AbortMacro,
    Save { path: PathBuf, reply: … }, Load { path: PathBuf, reply: … },
    Snapshot { reply: Reply<Snapshot> },
}

struct Snapshot { tree: Arc<Tree>, revision: u64 }
```

La `Revision` est un compteur monotone : elle permet à un client de savoir
si le snapshot qu'il détient est périmé sans comparer les arbres.

### 6.2 Clients

- **UI** : envoie des requêtes, conserve le dernier `Snapshot` et peint
  depuis lui, sans verrou ni appel à l'acteur pendant le rendu. Une
  réponse qui arrive déclenche `egui::Context::request_repaint()`.
- **Script** : bloque sur la réponse — et doit **relâcher le GIL pendant
  l'attente** (`Python::allow_threads`), sans quoi le thread de rendu ne
  pourra jamais l'acquérir. C'est la subtilité la plus facile à rater de
  tout ce document.
- **Rendu** : ne passe pas par l'acteur pour lire — il reçoit un
  `Snapshot` au lancement de la tâche et travaille dessus. Les buffers de
  colonnes étant derrière `Arc`, ils restent valides même si l'utilisateur
  continue d'éditer (une écriture concurrente déclenchera une copie, cf.
  §4.3).

### 6.3 Ce que l'acteur rend inutile

- le test « aucune mutation hors commande » : il n'y a plus de `&mut`
  ailleurs ;
- tout verrou sur le modèle ;
- la question « qui a le droit d'écrire » : personne, sauf l'acteur.

---

## 7. Persistance — `plotyx-persistence`

Frontière **figée dès maintenant**, format volontairement provisoire :

```
fn save(project: &Project, path: &Path) -> Result<(), PersistenceError>;
fn load(path: &Path) -> Result<Project, PersistenceError>;
```

Une fonction par sens, un `Project` complet, rien d'autre. Aucun appelant
ne connaît le format. C'est ce qui permettra de remplacer le format
provisoire (`serde` + `bincode`) par le Zarr étudié dans
`format-fichier-plx.md` sans rien casser au-dessus.

Propriétés obligatoires **même en version provisoire**, reprises de
`format-fichier-plx.md` §3 — les rajouter après coup sur des fichiers déjà
écrits coûte une migration :

- écriture **atomique** (fichier temporaire + renommage) ;
- **numéro de version en premier**, lu avant tout le reste, refus propre
  d'un fichier plus récent que le lecteur ;
- distinction valeur manquante / chaîne vide préservée ;
- **valeur haute du compteur de `NodeId`** persistée (§4.1) ;
- `plot_spec` sérialisé de façon **opaque et tolérante** : les champs
  inconnus sont ignorés à la lecture. C'est ce principe qui a permis au
  schéma HDF5 de ne pas bouger d'une ligne lors de l'ajout de Matplotlib ;
  il doit survivre au changement de langage.

**Sortie** : un `Project` non trivial (plusieurs classeurs, feuilles,
colonnes de longueurs différentes, valeurs manquantes, texte vide vs
absent) sauvegardé, rechargé, comparé à l'original.

---

## 8. Interpréteur embarqué — `plotyx-python`

### 8.1 Propriété et initialisation

Ce crate est le **seul** à initialiser l'interpréteur et le seul à exposer
l'accès au GIL. `render-mpl` et `script` se posent dessus.

**Initialisation paresseuse, obligatoire.** L'interpréteur, numpy et
Matplotlib ne sont chargés qu'au premier besoin réel (ouverture d'un
graphe ou de la console). Sans ça, le gain de temps de démarrage — l'un
des arguments du portage — est intégralement annulé par l'import de
Matplotlib au lancement. Une fenêtre vide et une feuille de calcul doivent
s'ouvrir **sans** interpréteur.

Interpréteur **vendored et figé** (`python-build-standalone`), wheels numpy
et Matplotlib épinglées, un jeu par plateforme. Pas de Python système, pas
de `pip` exposé, `PYTHONHOME` pointé sur le préfixe embarqué,
auto-initialisation de PyO3 désactivée.

### 8.2 GIL et contention — limite assumée

Un seul thread tient le GIL à la fois. Conséquence à documenter plutôt
qu'à prétendre résoudre : **un script exécutant une longue boucle Python
pure bloque le rendu**. C'est inhérent à CPython, c'est déjà le cas
aujourd'hui, et c'est la raison pour laquelle l'interruption du script est
un mécanisme à concevoir explicitement
(`cahier-des-charges-api-script.md` §5) et non une case à cocher.

### 8.3 L'empaquetage se prouve en premier

**Exigence de séquencement, pas de confort.** Avant d'écrire la moindre
fonctionnalité de graphe, un *hello-world* doit être empaqueté et lancé
**sur Linux et sur Windows** : binaire Rust + interpréteur vendored +
numpy + Matplotlib, rendant une figure triviale dans une texture egui.

C'est l'application directe du « point de vigilance n°3 » du roadmap
Python (le packaging se valide maintenant, pas au mois 6) au risque le
plus lourd de cette architecture : embarquer ~150 à 300 Mo de pile
scientifique par plateforme. Découvrir un blocage d'empaquetage après
avoir construit le renderer complet coûterait le renderer.

---

## 9. Spec de graphe et rendu — `plotyx-plot`, `plotyx-render-mpl`

### 9.1 Spec neutre

`PlotSpec` / `PanelSpec` / `SeriesSpec` / `AxisSpec` : structures
immuables, `serde`, **sans aucune connaissance d'un moteur de rendu**. Une
série désigne ses colonnes par `NodeId`, jamais par copie des données.
Port direct de `plotyx.plot`, dont un test dédié
(`test_plot_spec_is_engine_neutral.py`) vérifie déjà la propriété côté
Python — à porter.

### 9.2 Chemin de données — hypothèse à valider en premier

Le budget de rendu hérité de la phase 6 (141 ms pour 1M points) **repose
entièrement** sur le zéro-copie Rust → numpy. Or `rust-numpy` distingue
les constructeurs qui **copient** de ceux qui **empruntent** le buffer
Rust ; seul le second tient le budget.

À valider par une mesure dès le hello-world du §8.3, avant tout le reste :
si l'emprunt s'avère impraticable, c'est le budget de rendu qui change, et
cette hypothèse doit être invalidée tôt, pas découverte tard.

### 9.3 Sortie graphique et réticule

Matplotlib peint dans un canvas Agg hors-écran
(`FigureCanvasAgg.buffer_rgba()`), le buffer RGBA devient une texture egui.

**Point de vigilance mesuré** : le réticule blitté coûte 0,2 ms côté
Matplotlib (`copy_from_bbox` / `restore_region`), mais ré-uploader une
texture plein écran à chaque déplacement coûterait bien davantage. Le
budget ne survit que si la texture est mise à jour **partiellement**, sur
la seule région modifiée. À vérifier tôt : c'est ce qui décide si le
réticule reste fluide.

---

## 10. API de script — `plotyx-script`

Surface v1, modèle de confiance, vocabulaire d'erreurs et règles
d'exécution : `cahier-des-charges-api-script.md` fait foi, sans
modification. Ce document n'ajoute que les points propres à l'hôte Rust :

- **Chaque appel mutant est une `Request::Execute` vers l'acteur.** Le
  principe 1 (« toute mutation passe par le command bus ») n'est plus une
  règle à respecter mais la seule voie disponible.
- **`pyx.column(handle)`** rend une vue numpy `writeable = False`
  empruntant le `Arc<Vec<f64>>`. L'objet numpy doit maintenir l'`Arc`
  vivant pendant toute son existence (propriétaire attaché en `base`),
  sans quoi une réécriture concurrente libérerait le buffer sous la vue.
- **Le thread de script relâche le GIL pendant l'attente d'une réponse**
  (§6.2).
- **Clés** : le `NodeId` du §4.1 est la clé opaque du §3 du cahier des
  charges. Une clé absente de l'arbre courant lève
  `BrokenReferenceError` ; une clé restaurée par `undo` redevient valide
  sans travail supplémentaire, l'ancien arbre l'ayant conservée.

---

## 11. Coquille UI minimale — `plotyx-app`

Banc d'essai, pas une ébauche de l'UI finale (§1) :

- fenêtre `eframe`/`egui`, une grille brute affichant une `Worksheet` ;
- **Ctrl+Z** (annuler), **Ctrl+Y** et **Ctrl+Shift+Z** (rétablir, les deux
  conventions), **Ctrl+S** / **Ctrl+O** — câblés à la main sur l'acteur ;
- état des actions à la main également : grisé quand la pile est vide,
  libellé « Annuler *Renommer* » depuis le `label` de l'entrée. Rien de
  tout cela n'est fourni par un framework en Rust, contrairement à
  `QUndoStack` + `QAction` qui le tenaient à jour tout seuls.

**Sortie** : une valeur modifiée, Ctrl+Z l'annule, Ctrl+S écrit, rouvrir
restitue l'état exact.

---

## 12. Conventions transverses

**Erreurs.** `thiserror` et un type d'erreur nommé par crate ; `anyhow`
uniquement dans `plotyx-app`. Aucun `unwrap()` ni `panic!` dans les crates
bibliothèques hors invariant documenté — lint clippy activé.

**Tests.**

- Propriété d'undo : N commandes aléatoires → tout annuler → égalité avec
  l'état initial, via `proptest` (qui réduit automatiquement un
  contre-exemple). C'est le filet qui rattrape la moitié « delta »
  conservée par l'hybride.
- Cas que ce test ne couvre pas et qui exigent des tests dédiés, comme
  côté Python : abandon de macro à mi-chemin, redo après annulation
  partielle, éviction par la limite d'historique, macro mixte (§5.3).
- Aller-retour de persistance (§7).
- `plotyx-core` sans dépendance non autorisée : garde-fou déjà en place
  (`crates/plotyx-core/tests/no_ui_dependency.rs`), à étendre aux autres
  crates non-Python.
- **La CI doit rester verte sans Python installé** : tout ce qui touche à
  PyO3 derrière un feature flag.

**Langue.** Identifiants en anglais, commentaires et documentation en
français, messages utilisateur en français — comme le code Python actuel.

**Budgets de performance**, hérités de la phase 6 et à re-mesurer, pas à
supposer : 141 ms pour le premier rendu de 1M points ; 0,2 ms pour le
réticule blitté (§9.3) ; ouverture d'une fenêtre et d'une feuille **sans**
initialisation de l'interpréteur (§8.1).

---

## 13. Séquencement

`roadmap.md` couvre les phases 0 à 4. Ce document impose trois phases
supplémentaires, dans cet ordre :

| Phase | Contenu | Verrou |
|---|---|---|
| 5 | Pipeline Python : empaquetage prouvé Linux + Windows, zéro-copie mesuré | §8.3 et §9.2 avant tout code de graphe |
| 6 | Renderer Matplotlib complet, mise à jour partielle de texture | après 5 |
| 7 | API de script | après 2 (bus stable) **et** 5 |

Les phases 5 à 7 ne sont pas encore rédigées dans `roadmap.md` — à faire
pour que les deux documents restent cohérents.

---

## 14. Risques et questions ouvertes

**Risques identifiés, par ordre décroissant de gravité.**

1. **Empaquetage de la pile Python par plateforme** — le plus lourd du
   projet, et le seul dont l'échec invaliderait l'architecture entière.
   Mitigé par §8.3 (le prouver en premier), pas éliminé.
2. **Zéro-copie Rust → numpy** — hypothèse dont dépend le budget de rendu
   (§9.2). À mesurer, pas à supposer.
3. **Mise à jour partielle de texture** pour le réticule (§9.3) — décide
   de la fluidité de l'interaction.
4. **Ordre d'annulation dans les macros mixtes** (§5.3) — la seule
   fragilité conceptuelle introduite par l'undo hybride.

**Questions non tranchées.**

- Garder ou abandonner le mode « visualiseur léger » (ex-PyQtGraph) ? Son
  seul argument différenciant (`useOpenGL`) n'est toujours activé nulle
  part (`resume-decisions.md` §8).
- Coalescence des éditions consécutives dans une même colonne en une seule
  entrée d'historique (comportement d'un tableur) : souhaitable, mais
  aucune décision prise sur le critère de regroupement.
- Calendrier réel non chiffré, comme pour le reste du portage.

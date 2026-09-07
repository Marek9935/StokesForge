# Cahier des charges technique — socle Rust et pont Python

Statut : **spécification arrêtée le 2026-09-08**, implémentation non
commencée au-delà du squelette de phase 0. Ce document fait foi sur
l'architecture ; il complète `roadmap.md` (qui dit *quand*) en disant
*comment* et *pourquoi pas autrement*. Comme `resume-decisions.md` et
`format-fichier-plx.md`, il conserve les options écartées et leur raison.

**Objet du socle** : disposer de tout ce qu'il faut pour **ouvrir le
chantier de l'interface**, et de rien de plus. Le modèle, l'annulation, le
fichier, une fenêtre, et surtout la **tuyauterie Rust ↔ Python** éprouvée.
Ni système de graphes, ni UI réelle : ces deux chantiers doivent pouvoir
démarrer sur un socle qui ne bougera plus sous eux.

---

## 1. Périmètre

| Dans le socle | Hors socle |
|---|---|
| Modèle de données (§4) | Système de graphes : `PlotSpec`, renderer Matplotlib |
| Historique, Ctrl+Z / Ctrl+Y (§5) | Surface complète de `plotyx.api` (v1) |
| Acteur possédant le modèle (§6) | Import de fichiers |
| Persistance minimale (§7) | Format `.plx` final (Zarr) |
| **Pont Rust ↔ Python (§8)** | UI réelle : docking, grille éditable, Properties |
| Coquille UI minimale (§9) | Mode « visualiseur léger » |

### 1.1 Le critère de fin, en une phrase

Le socle est terminé quand, **depuis un binaire empaqueté sur Linux et sur
Windows** :

> une cellule éditée dans la fenêtre s'annule par Ctrl+Z et se rétablit par
> Ctrl+Y ; Ctrl+S écrit un fichier que Ctrl+O rouvre à l'identique ; et un
> script Python lit une colonne **sans copie**, en écrit une autre **via le
> bus** — et **Ctrl+Z annule ce que le script vient d'écrire**.

Cette dernière clause est le vrai test : elle traverse d'un coup l'acteur,
le GIL, la durée de vie des buffers partagés et l'historique. Tant qu'elle
ne passe pas, le pont n'est pas prouvé.

---

## 2. Les quatre décisions d'architecture

Décisions prises le 2026-09-08. **Le rétrécissement du périmètre n'en
rouvre aucune** — §2.5 dit pourquoi, décision par décision.

### 2.1 Undo — hybride structure / données

**Décision.** L'historique manipule deux natures d'entrée :

- **structure** (arbre, noms, ordre, métadonnées) → *snapshot persistant* :
  muter produit un nouvel arbre qui partage tout l'inchangé, annuler remet
  l'ancien pointeur ;
- **données** (contenu des colonnes) → *delta* : la commande enregistre les
  anciennes valeurs des seules cellules écrites.

**Ce que ça achète.** Les inverses disparaissent là où ils sont difficiles
à écrire — déplacements, collisions de noms, suppression/restauration — et
avec eux **toute la machinerie de durée de vie des objets supprimés**
(décision 4 de la phase 2 Python : miroir de pile, `release()`, gestion de
l'éviction, tests `gc`/`weakref`). L'ancien arbre retient l'objet
supprimé ; il n'y a rien à libérer explicitement. Les inverses restants
sont ceux qu'on ne rate pas : réécrire les anciennes valeurs de N cellules.

**Ce que ça coûte.** Deux natures d'entrée dans une seule pile, avec une
discipline d'ordre à tenir (§5.3). Et l'hybride **n'élimine pas** la classe
de bugs « inverse faux » : il la confine à la moitié facile. Le test de
propriété du §10 reste obligatoire.

**Écarté — delta pur** (le port mécanique de Python) : un seul mécanisme,
les dix types de commandes et leurs tests se transposent presque tels
quels, mémoire minimale. Rejeté parce qu'il oblige à réécrire la machinerie
de durée de vie ci-dessus, et parce que ce projet a déjà produit
empiriquement un bug de cette classe (`insert_rows` / `delete_rows` non
inverses, `roadmap.md` §2.4) sur la moitié structurelle — celle que
l'hybride supprime.

**Écarté — snapshot intégral** (modèle entièrement persistant, undo
impossible à écrire faux) : séduisant, mais tenable seulement si éditer une
cellule ne copie pas toute la colonne, donc il impose de **chunker** les
colonnes. Or une colonne chunkée ne se voit pas comme un seul tableau
numpy : cela casserait le zéro-copie du §8.3, qui est justement le livrable
central du socle. Motif de rejet : **la perte du zéro-copie, pas la RAM** —
les chiffres du §2.1.1 montrent que la mémoire ne départage pas.

#### 2.1.1 Ce que coûte réellement l'historique en mémoire

Projet lourd de référence : 50 colonnes × 1M points = **400 Mo de
données**, arbre de ~2000 nœuds, 100 entrées d'historique.

| | entrée structurelle | entrée de données (1 cellule) | pile de 100 |
|---|---|---|---|
| Delta pur | ~200 o | ~16 o | < 1 Mo |
| **Hybride (retenu)** | taille de l'arbre, ~100 Ko | ~16 o | **~10 Mo** |
| Snapshot intégral + chunks | ~100 Ko | 1 chunk : 32 à 512 Ko | 3 à 50 Mo |

Face aux 400 Mo de données, aucune de ces piles n'est un problème.
L'historique est **entièrement en RAM** dans les trois cas, comme
aujourd'hui avec `QUndoStack` ; aucun déversement sur disque n'est prévu.

Le seul vrai piège mémoire est **commun aux trois** : supprimer un classeur
de 400 Mo garde ces 400 Mo vivants tant que l'entrée d'annulation est sur
la pile. C'est nécessaire — sans ça, pas d'annulation. Le garde-fou est la
limite d'historique (§5.4).

### 2.2 Propriété du modèle — acteur sur thread dédié

**Décision.** Le `Project` vit sur son propre thread, dans `plotyx-session`.
L'UI et le thread de script sont deux clients symétriques : ils envoient
des requêtes et reçoivent des snapshots immuables.

**Pourquoi, dans ce périmètre précis.** Le pont Python est le livrable, et
le script s'exécute sur un thread dédié (jamais le thread UI). Si le modèle
appartenait au thread UI, chaque appel du script devrait traverser ce
thread et attendre son tour — on éprouverait alors une tuyauterie qui n'est
pas celle qu'on livrera. L'acteur fait du script un client de première
classe dès le premier jour.

**Bénéfice structurel.** La règle d'or — aucune mutation hors du bus —
cesse d'être une convention testée pour devenir une **propriété de
propriété** : l'acteur détient le seul `&mut Project` du processus. Le test
Python `test_no_mutation_outside_command.py` n'a plus d'équivalent à
écrire ; il n'a plus d'objet.

**Ce que ça coûte.** Une centaine de lignes de plomberie — prix bas
précisément parce que la structure est persistante : l'UI garde son dernier
`Arc` d'arbre et peint sans jamais solliciter l'acteur.

**Écarté — modèle sur le thread UI + workers** (ce que fait Python) : plus
simple, mais couple le script à la boucle de rendu, dans un périmètre dont
le script est justement l'objet.

**Écarté — `Arc<RwLock<Project>>`** : contention et deadlocks, et rendu
inutile par le modèle persistant — on partage des snapshots immuables, pas
un verrou.

### 2.3 Python embarqué — PyO3, un objectif du socle

**Décision.** Interpréteur CPython **embarqué dans le processus** via PyO3,
vendored et figé, propriété exclusive du crate `plotyx-python` (§8).

Dans le périmètre initial, cette décision découlait du renderer Matplotlib.
Le renderer étant sorti du socle, elle ne découle plus de rien — elle
**est** un objectif : « Python accessible » fait partie de ce qui doit être
solide avant d'ouvrir le chantier de l'interface.

**Écarté — sous-processus Python + IPC** : isolation des crashs meilleure,
mais un aller-retour par appel rend inutilisable une console interactive,
et la vue numpy sans copie du §8.3 devient impossible (les buffers ne sont
plus dans le même espace d'adressage). Le sous-processus reste pertinent
pour un export ponctuel, cf. §12.

**Écarté — langage de script natif** (Rhai, Lua) : supprimerait tout le
poids Python, mais `cahier-des-charges-api-script.md` §8 fait de la
compatibilité des scripts déjà écrits et de l'accès à numpy un contrat.

### 2.4 Découpage — crates fins

**Décision.** Sept crates (§3), frontières imposées par le compilateur.

**Pourquoi.** C'est le mécanisme qui rend vérifiable l'équivalent Rust de
« aucun Qt dans `core` » : aucun PyO3, aucun egui dans le cœur. Et tout ce
qui touche à Python vit derrière un *feature flag*, ce qui garde
`cargo test` et la CI utilisables **sur une machine sans Python installé** —
non négociable pour ne pas rendre le socle otage de son chantier le plus
lourd.

**Écarté — 3-4 crates** : moins de friction, mais plus rien n'empêche PyO3
ou egui de remonter dans le cœur, et la CI exigerait Python partout.

### 2.5 Le rétrécissement ne rouvre aucune des quatre

| Décision | Tenait par | Après rétrécissement |
|---|---|---|
| Undo hybride | zéro-copie vers Matplotlib **et** vers les scripts | tient : le zéro-copie vers les scripts est le livrable §8.3 |
| Acteur | thread de script exigé par l'API | tient, et se renforce : le script est l'objet du socle |
| PyO3 | conséquence du renderer | tient, comme **objectif** et non plus comme conséquence (§2.3) |
| Crates fins | frontières vérifiables | tient, mais passe de 9 à **7** : `plot` et `render-mpl` sortent |

---

## 3. Carte des crates

| Crate | Rôle | Dépend de | Python ? |
|---|---|---|---|
| `plotyx-core` | Modèle : arbre, nœuds, colonnes. Aucune mutation publique. | — | non |
| `plotyx-commands` | Commandes et pile d'historique à deux natures | `core` | non |
| `plotyx-persistence` | `save` / `load` d'un `Project` complet | `core` | non |
| `plotyx-session` | **L'acteur** : possède le `Project`, sert les clients | `core`, `commands`, `persistence` | non |
| `plotyx-python` | Interpréteur embarqué, GIL, conversion de tableaux. **Ne connaît pas le modèle.** | — | **oui** |
| `plotyx-script` | Module `pyx`, thread de script, surface-témoin | `session`, `python` | **oui** |
| `plotyx-app` | Coquille egui + binaire | tous | via features |

`plotyx-cli` (squelette de phase 0) reste comme binaire headless de test.

**Frontière à tenir** : `plotyx-python` ignore tout du modèle — il ne sait
que faire vivre un interpréteur et convertir des tableaux. C'est ce qui
permettra au futur `render-mpl` de se poser dessus sans passer par
`plotyx-script`.

---

## 4. Modèle — `plotyx-core`

### 4.1 Identité des nœuds — compteur monotone, **pas** `slotmap`

**Écart assumé avec `roadmap.md` §0.4**, qui proposait `slotmap` /
`generational-arena`. Le choix de l'undo hybride l'invalide :

une arène à réutilisation de slots peut allouer `(slot 3, génération 2)`
pour un nouveau nœud après qu'un nœud `(slot 3, génération 1)` a été retiré
de l'arbre courant. Annuler la suppression réinsérerait `(3, v1)` dans un
arbre où le slot 3 est repris — **une collision d'identité créée par
l'annulation elle-même**.

**Retenu** : `NodeId(u64)`, compteur monotone, **jamais réutilisé**, dans
une `HashMap<NodeId, Node>`. Une restauration ne peut structurellement pas
entrer en collision. La valeur haute du compteur est **persistée avec le
fichier**, sans quoi un rechargement rouvrirait le risque.

Ce `NodeId` tient les trois rôles : clé d'arbre, clé opaque exposée aux
scripts (`cahier-des-charges-api-script.md` §3), et — plus tard —
référence de série dans un `PlotSpec`.

*Alternative en réserve* : UUID v4, seulement si l'identité doit un jour
survivre à la fusion de deux projets distincts. Besoin inexistant, et un
`u64` est plus petit et plus rapide.

### 4.2 Arbre

`Arc<Tree>`, où `Tree` contient la `HashMap<NodeId, Node>` et la racine.
Muter = cloner l'arbre (`Arc::make_mut`) et remplacer les nœuds touchés.
Un clone coûte la carte des nœuds — ~100 Ko au pire (§2.1.1) — les charges
utiles étant derrière `Arc`, elles ne sont pas copiées.

Structure conservée du modèle Python : `Folder → Workbook → Worksheet →
Column`, `Graph` / `Matrix` / `Note` en nœuds frères du `Workbook`,
grammaire de chemins `[Book1]Sheet1!ColA` reprise de `core/paths.py`. Les
nœuds `Graph` existent dans le modèle **sans** rien savoir d'un rendu — le
système de graphes viendra se poser dessus.

*Écarté* : `im` / `rpds` (cartes persistantes, clone en O(log n)). Inutile
à cette échelle — un document se compte en milliers de nœuds — et c'est une
dépendance de moins.

### 4.3 Colonnes — contiguës, jamais chunkées

| Type | Représentation | Valeur manquante |
|---|---|---|
| Numérique / date-heure | `Arc<Vec<f64>>` | `f64::NAN` |
| Texte | `Arc<Vec<Option<String>>>` | `None`, distinct de `Some("")` |

La contiguïté est une **exigence contractuelle** : elle conditionne le
zéro-copie du §8.3. Aucune évolution ne doit la remettre en cause sans
rouvrir §2.1.

Écriture par `Arc::make_mut` : la copie n'a lieu **que** si un script ou une
tâche de fond détient encore ce buffer. En édition courante, sans lecteur
concurrent, l'écriture est en place et gratuite.

Longueurs hétérogènes tolérées entre colonnes d'une même feuille
(décision 2 de la phase 1 Python — l'argument ne dépend pas du langage).

### 4.4 Invariants à tester dès la phase 1

1. Une colonne de longueur L n'a pas de ligne au-delà de L : insertion et
   suppression de lignes ignorent **le même ensemble** de colonnes trop
   courtes. C'est l'invariant dont la violation a produit le bug
   `insert_rows` / `delete_rows` côté Python — à porter comme test, pas à
   redécouvrir.
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

`before` et `after` partagent presque tout : garder les deux coûte un
pointeur, et le redo devient symétrique de l'undo.

### 5.2 Commandes

Primitives du socle : `SetCells`, `AddColumn`, `DeleteColumn`, `Rename`,
`Move`, `SetColumnType`, `InsertRows`, `DeleteRows`. Les commandes de
`PlotSpec` viendront avec le système de graphes.

**Application avant empilement** : la commande est appliquée par le bus et
n'est empilée que si elle réussit. Un échec (nom en double, index hors
bornes) remonte en `Err` et la pile n'a pas bougé. Reprise de la décision 3
de la phase 2 Python, dont la justification change : Python s'en servait
pour éviter qu'une exception traverse la frontière C++ de Qt ; ici c'est
simplement le comportement correct d'une API à `Result`.

`SetColumnType` reste le **seul** cas à snapshot de données documenté : la
conversion de type est destructrice dans les deux sens.

**Pas de coalescence.** Deux `SetCells` consécutifs sur la même colonne
restent deux entrées distinctes dans l'historique — décidé le 2026-09-08,
tranché plutôt que laissé ouvert. Le comportement « tableur » (fusionner
des frappes successives en une seule entrée) exigerait un critère de
regroupement (fenêtre de temps ? même cellule ? même appelant ?) qui
n'apporte rien à ce socle et qui se décidera, si besoin, avec l'UI réelle
où ces frappes ont lieu.

### 5.3 Macros — la discipline qui fait tenir l'hybride

Une macro s'annule en rejouant ses entrées **strictement en ordre inverse
d'insertion**, quelles que soient leurs natures. C'est la seule règle qui
fait composer snapshots de structure et deltas de données : un snapshot
restauré hors d'ordre annulerait silencieusement un delta appliqué après
lui.

À tester explicitement sur une macro mixte — une modification de structure
et une écriture de cellules dans la même macro, dans les deux ordres.

`abort_macro()` : dépiler, annuler, jeter. Aucun contournement nécessaire —
la contrainte Qt qui imposait côté Python de pousser une commande se
déclarant obsolète pendant son `redo()` (décision 6) n'existe pas ici.

### 5.4 Limite d'historique

Configurable, bornée par défaut. Seul garde-fou contre le piège mémoire du
§2.1.1. Une entrée évincée relâche ses `Arc` ; les buffers qu'elle seule
retenait sont libérés à ce moment.

---

## 6. Session — `plotyx-session`, l'acteur

### 6.1 Protocole

Un canal de requêtes FIFO — donc ordre déterministe — une réponse par
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

La `Revision` est un compteur monotone : un client sait si son snapshot est
périmé sans comparer les arbres.

### 6.2 Clients

- **UI** : envoie des requêtes, conserve le dernier `Snapshot`, peint
  depuis lui sans verrou. Une réponse déclenche
  `egui::Context::request_repaint()`.
- **Script** : bloque sur la réponse, et **relâche le GIL pendant
  l'attente** (§8.2). C'est la subtilité la plus facile à rater du socle.

### 6.3 Invariant non négociable

**L'acteur ne touche jamais à Python.** Il ne détient jamais le GIL, ne
construit jamais d'objet Python, n'appelle jamais `plotyx-python`. C'est ce
qui rend impossible le cycle « le script tient le GIL et attend l'acteur,
l'acteur attend le GIL ». La frontière est vérifiable : `plotyx-session` ne
dépend pas de `plotyx-python` (§3).

---

## 7. Persistance — `plotyx-persistence`

Frontière **figée dès maintenant**, format volontairement provisoire :

```
fn save(project: &Project, path: &Path) -> Result<(), PersistenceError>;
fn load(path: &Path) -> Result<Project, PersistenceError>;
```

Une fonction par sens, un `Project` complet, rien d'autre. Aucun appelant
ne connaît le format — c'est ce qui permettra de remplacer le provisoire
(`serde` + `bincode`) par le Zarr de `format-fichier-plx.md` sans rien
casser au-dessus.

Propriétés obligatoires **même en version provisoire**, reprises de
`format-fichier-plx.md` §3 — les rajouter après coup sur des fichiers déjà
écrits coûte une migration :

- écriture **atomique** (fichier temporaire + renommage) ;
- **numéro de version en premier**, lu avant tout le reste, refus propre
  d'un fichier plus récent que le lecteur ;
- distinction valeur manquante / chaîne vide préservée ;
- **valeur haute du compteur de `NodeId`** persistée (§4.1) ;
- champs inconnus **ignorés** à la lecture. C'est ce principe qui a permis
  au schéma HDF5 de ne pas bouger d'une ligne lors de l'ajout de
  Matplotlib ; il doit survivre au changement de langage, et c'est lui qui
  permettra d'ajouter les `PlotSpec` plus tard sans casser les fichiers du
  socle.

---

## 8. Le pont Rust ↔ Python — le cœur du socle

### 8.1 Propriété et cycle de vie

`plotyx-python` est le **seul** crate à initialiser l'interpréteur et le
seul à exposer l'accès au GIL.

- Interpréteur **vendored et figé** (`python-build-standalone`), wheels
  numpy épinglées, un jeu par plateforme. Pas de Python système, pas de
  `pip` exposé, `PYTHONHOME` pointé sur le préfixe embarqué,
  auto-initialisation de PyO3 désactivée.
- **Initialisation paresseuse, obligatoire.** L'interpréteur n'est chargé
  qu'au premier besoin réel (ouverture de la console). Une fenêtre et une
  feuille de calcul doivent s'ouvrir **sans** interpréteur : sans cette
  règle, le gain de temps de démarrage — l'un des motifs du portage — est
  annulé dès le lancement.
- **Pas de finalisation** de CPython à l'extinction : la finalisation est
  une source connue de plantages en sortie, et n'apporte rien à un
  processus qui se termine.

### 8.2 GIL — trois règles

1. **Le thread UI ne touche jamais à Python.** Dans le socle, il n'en a
   aucun besoin ; l'y autoriser ferait dépendre la fluidité du rendu de la
   disponibilité du GIL.
2. **Le thread de script relâche le GIL pendant qu'il attend l'acteur**
   (`allow_threads`). Sans ça, il affame tout autre usage de Python pendant
   chaque aller-retour.
3. **L'acteur ne prend jamais le GIL** (§6.3) — la règle qui rend le
   deadlock structurellement impossible.

**Limite assumée, à documenter plutôt qu'à prétendre résoudre** : un script
exécutant une longue boucle Python pure garde le GIL. C'est inhérent à
CPython et déjà vrai aujourd'hui ; c'est pourquoi l'interruption est un
mécanisme à concevoir (§8.4), pas une case à cocher.

### 8.3 Tableaux — l'asymétrie est une décision

| Sens | Mécanisme | Copie ? |
|---|---|---|
| Rust → Python (lecture) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
| Python → Rust (écriture) | copie du tableau numpy dans un `Vec<f64>` neuf | **oui** |

La lecture sans copie est le livrable ; l'écriture avec copie est un
**choix**, pas une concession : le modèle doit posséder ses buffers, et
adopter de la mémoire allouée par l'allocateur Python ferait dépendre la
durée de vie d'une colonne de celle d'un objet Python. Le contrat de
`cahier-des-charges-api-script.md` §2.3 est respecté : la lecture est une
vue, l'écriture passe par une commande.

**Deux points à valider par la mesure, en tout premier** :

- `rust-numpy` distingue les constructeurs qui **copient** de ceux qui
  **empruntent** le buffer Rust. Seul l'emprunt tient le contrat — à
  vérifier explicitement, pas à supposer d'après le nom de la fonction.
- L'objet numpy rendu doit **maintenir l'`Arc` vivant** pendant toute son
  existence (propriétaire attaché en `base`). Sans ça, une écriture
  concurrente déclencherait le `Arc::make_mut` du §4.3 et libérerait le
  buffer sous une vue encore utilisée par le script. Test dédié :
  garder une vue vivante, écrire dans la colonne depuis l'UI, relire la
  vue.

### 8.4 Thread de script

- **Un script à la fois** dans le socle ; pas de scripts concurrents.
- `print()` et tracebacks capturés et rendus au client, jamais perdus dans
  la sortie du processus.
- **Interruption coopérative** : un drapeau vérifié par les appels de
  l'API, plus un hook de trace pour le code Python pur. Le mécanisme est à
  concevoir ici, dans le socle — pas au moment de la console complète : il
  conditionne la forme du thread.

### 8.5 Surface-témoin — quatre appels, pas la v1

Un interpréteur qui démarre ne prouve rien. La surface minimale qui
traverse **tout** le pont :

| Appel | Ce qu'il éprouve |
|---|---|
| `pyx.resolve(path)` | clés opaques, grammaire de chemins |
| `pyx.column(handle)` | vue numpy sans copie, `Arc` maintenu vivant (§8.3) |
| `pyx.set_column(handle, values)` | mutation via l'acteur et le bus, GIL relâché pendant l'attente |
| `pyx.undo()` | délégation à la pile d'historique depuis Python |

C'est **explicitement pas** la surface v1 de
`cahier-des-charges-api-script.md`, qui reste le contrat pour plus tard,
inchangé. Ces quatre-là ne sont pas choisis pour leur utilité mais parce
qu'ensemble elles rendent le critère du §1.1 vérifiable.

Vocabulaire d'erreurs limité à ce qu'elles peuvent lever :
`UnknownPathError`, `BrokenReferenceError`, `InvalidValueError` — noms
repris tels quels du cahier des charges API, pour ne pas avoir à les
renommer ensuite.

### 8.6 L'empaquetage se prouve en premier

**Exigence de séquencement, pas de confort.** Avant tout autre code du
pont, un *hello-world* doit être empaqueté et lancé **sur Linux et sur
Windows** : binaire Rust + interpréteur vendored + numpy, avec un aller
simple et un retour de tableau mesurés.

C'est l'application du « point de vigilance n°3 » du roadmap Python (le
packaging se valide maintenant, pas à la fin) au risque le plus lourd de
cette architecture. Découvrir un blocage d'empaquetage après avoir écrit
tout le pont coûterait le pont.

---

## 9. Coquille UI minimale — `plotyx-app`

Banc d'essai, **pas** une ébauche de l'UI finale : elle sera reprise
entièrement en conversation, et rien ici ne doit la préempter.

- fenêtre `eframe`/`egui`, grille brute affichant une `Worksheet` ;
- **Ctrl+Z** (annuler), **Ctrl+Y** et **Ctrl+Shift+Z** (rétablir — les deux
  conventions), **Ctrl+S** / **Ctrl+O** ;
- état des actions à la main : grisé quand la pile est vide, libellé
  « Annuler *Renommer* » depuis le `label` de l'entrée. Rien de cela n'est
  fourni par un framework en Rust, contrairement à `QUndoStack` +
  `QAction` qui le tenaient à jour tout seuls ;
- une zone de saisie de script minimale, suffisante pour exercer §8.5.

---

## 10. Conventions transverses

**Erreurs.** `thiserror` et un type nommé par crate ; `anyhow` uniquement
dans `plotyx-app`. Aucun `unwrap()` ni `panic!` dans les bibliothèques hors
invariant documenté — lint clippy activé.

**Tests.**

- Propriété d'undo : N commandes aléatoires → tout annuler → égalité avec
  l'état initial, via `proptest` (qui réduit automatiquement un
  contre-exemple). C'est le filet qui rattrape la moitié « delta »
  conservée par l'hybride.
- Cas hors de portée de ce test, à couvrir explicitement : abandon de macro
  à mi-chemin, redo après annulation partielle, éviction par la limite
  d'historique, macro mixte (§5.3).
- Aller-retour de persistance (§7).
- Durée de vie d'une vue numpy sous écriture concurrente (§8.3).
- Le critère du §1.1, en test d'intégration bout en bout.
- Garde-fou de dépendances déjà en place
  (`crates/plotyx-core/tests/no_ui_dependency.rs`), à étendre aux autres
  crates non-Python.
- **La CI doit rester verte sans Python installé** : tout ce qui touche à
  PyO3 derrière un feature flag.

**Langue.** Identifiants en anglais, commentaires et documentation en
français, messages utilisateur en français — comme le code Python actuel.

**Budgets.** Ouverture d'une fenêtre et d'une feuille **sans**
initialisation de l'interpréteur (§8.1). Les budgets de rendu hérités de la
phase 6 (141 ms pour 1M points, 0,2 ms pour le réticule) ne concernent pas
ce socle : ils reviendront avec le système de graphes.

---

## 11. Séquencement

`roadmap.md` détaille les phases 0 à 5. Ordre imposé par ce document :

| Phase | Contenu | Verrou |
|---|---|---|
| 0 | Squelette Cargo, CI | fait |
| 1 | Modèle (§4) | — |
| 2 | Historique + acteur (§5, §6) | après 1 |
| 3 | Persistance (§7) | après 1 ; valide le modèle par aller-retour disque |
| 4 | Coquille UI (§9) | après 2 et 3 |
| 5 | **Pont Python (§8)** | empaquetage (§8.6) et zéro-copie (§8.3) **avant** le reste de la phase |

La phase 5 peut démarrer en parallèle de 3-4 : `plotyx-python` ne dépend
d'aucun des deux. Seule sa surface-témoin (§8.5) attend l'acteur.

---

## 12. Hors périmètre — décisions déjà prises pour la suite

Ces chantiers ne sont pas dans le socle, mais leurs décisions sont
arrêtées : les rouvrir demanderait une raison nouvelle.

- **Système de graphes** : `PlotSpec` neutre (port de `plotyx.plot`), rendu
  par **Matplotlib** dans l'interpréteur du §8, sortie
  `FigureCanvasAgg.buffer_rgba()` en texture egui. Deux points de vigilance
  déjà identifiés : le budget de 141 ms dépend du zéro-copie du §8.3, et le
  réticule à 0,2 ms ne survit que si la texture est mise à jour
  **partiellement**, sur la seule région modifiée.
- **Surface `plotyx.api` v1** : `cahier-des-charges-api-script.md`, sans
  modification. La surface-témoin du §8.5 en est un sous-ensemble strict.
- **Format `.plx` final** : Zarr (`zarrs`), `format-fichier-plx.md`.
- **Import de fichiers** : les 10 fixtures de
  `legacy-python/tests/fixtures/import/` sont la spécification.
- **Export ponctuel par sous-processus** : écarté pour la console (§2.3),
  mais reste pertinent pour un export lourd — la raison du rejet (latence
  par appel) ne s'applique pas à une opération unique.

---

## 13. Risques et questions ouvertes

**Risques, par gravité décroissante.**

1. **Empaquetage de la pile Python par plateforme** — le seul dont l'échec
   invaliderait l'architecture. Mitigé par §8.6, pas éliminé.
2. **Zéro-copie et durée de vie sous vue numpy** (§8.3) — le contrat central
   du socle. À mesurer et à tester, pas à supposer.
3. **Ordre d'annulation dans les macros mixtes** (§5.3) — la seule
   fragilité conceptuelle introduite par l'undo hybride.
4. **Interruption d'un script en boucle Python pure** (§8.2) — limite
   connue de CPython ; le mécanisme du §8.4 la contient sans la supprimer.

**Décisions différées sans risque pour le socle** — tranchées comme
« pas ici », pas laissées ouvertes :

- **Coalescence des éditions** : explicitement écartée du socle, cf. §5.2.
- **Mode « visualiseur léger »** (ex-PyQtGraph, garder ou abandonner) :
  décidé le 2026-09-08 de reporter ce choix au chantier de l'UI réelle,
  sans risque pour le socle — aucun crate ci-dessus n'en dépend, et son
  seul argument différenciant (`useOpenGL`) n'est de toute façon toujours
  activé nulle part aujourd'hui (`resume-decisions.md` §8). À trancher
  quand l'UI se construira, pas avant.

**Question non tranchée, hors architecture** : calendrier réel non
chiffré.

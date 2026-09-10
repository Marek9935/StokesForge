# Roadmap Rust — le socle et le pont Python

Statut : **architecture arrêtée le 2026-09-08** (`cahier-des-charges-technique.md`),
**calendrier non engagé**. Ce document ne remplace pas `ROADMAP.md`
(Python/Qt, actif) — c'est le plan du portage. Il dit *quand* ;
`cahier-des-charges-technique.md` dit *comment* et fait foi sur
l'architecture. En cas de divergence entre les deux, **c'est le cahier des
charges qui gagne**, et ce document qui est à corriger.

**Amendement du 2026-09-10 — le renderer.** Le renderer n'est plus
Matplotlib : le rendu passe à **Plotters** avec un backend egui écrit par
nous (`docs/newarchitecture/`). **Aucune phase de ce plan ne change** —
les phases 0 à 5 sont toutes en amont du choix de rendu, et le renderer
était déjà hors périmètre depuis le 2026-09-08. Seuls les §6 et §7
ci-dessous sont corrigés.

**But de ce plan** : livrer le strict nécessaire pour **ouvrir le chantier
de l'interface**, et rien de plus.

- Phases 1-2 : le socle jugé « bas risque, portage mécanique » dans
  `resume-decisions.md` §7 — modèle, bus de commandes, undo/redo.
- Phases 3-4 : persistance minimale puis coquille UI minimale — la
  persistance passe avant l'UI parce qu'elle est le test le plus dur du
  modèle, pas une fonctionnalité de confort (raisonnement en phase 3).
- Phase 5 : le **pont Rust ↔ Python** — la tuyauterie, sans système de
  graphes par-dessus.

**Cible resserrée le 2026-09-08 : Linux d'abord, seul** (cahier des
charges §1.1). Windows n'est pas hors périmètre — il reste la condition de
clôture définitive du socle (§8.6) — mais c'est une étape ultérieure,
rejouée une fois le critère ci-dessous vrai sur Linux, pas portée en
parallèle dès la phase 0.

Le critère de fin est celui du §1.1 du cahier des charges : depuis un
binaire empaqueté sur Linux, une cellule s'annule par Ctrl+Z, un fichier se
rouvre à l'identique, **et un script Python lit une colonne sans copie, en
écrit une autre via le bus, que Ctrl+Z annule.** Windows rejoue ensuite le
même critère, en aval, pas en même temps.

Les points restant à trancher — aucun ne bloque une phase — sont regroupés
dans `questions-ouvertes.md`, rédigé pour être lu sans connaissance de
Rust. Trois tombent dans le périmètre phasé ici : la valeur de la limite
d'historique (phase 2), le comportement de `pyx.undo()` sur une pile vide
(tâche 5.5) et ce qu'on dit d'une vue numpy périmée (tâche 5.2).

Rien au-delà n'est phasé ici — système de graphes, surface `plotyx.api` v1,
import, docking, grille éditable. L'UI en particulier sera reprise
entièrement en conversation au moment de la construire (§7 en garde une
trace technique, pas un phasage).

Référence structurelle : `ROADMAP.md` phases 0-2 (Python), dont ce plan est
la transposition Rust — même découpage, mêmes décisions quand elles tiennent
indépendamment du langage, écarts justifiés un par un.

---

## Phase 0 — Fondations projet Rust (≈3-5 jours)

> **Statut : close le 2026-09-08.** Le test fumée 0.5 (`crates/plotyx-core/tests/smoke.rs`) a été livré avec la phase 1, faute de `Project` avant elle.

**Objectif** : squelette qui compile, teste et s'exécute en CI, avant tout
code métier — même principe que la phase 0 Python (le packaging/CI se valide
tout de suite, pas à la fin).

| # | Tâche | Livrable |
|---|---|---|
| 0.1 | Workspace Cargo multi-crates : `plotyx-core` (modèle, zéro dépendance UI), `plotyx-commands` (bus/undo), `plotyx-cli` (binaire de test headless) — soit **2 des 7 crates cibles** de `cahier-des-charges-technique.md` §3, plus le binaire headless ; les cinq autres (`persistence`, `session`, `python`, `script`, `app`) arrivent avec leur phase | `cargo build` vert, 3 crates |
| 0.2 | `rustfmt` + `clippy -D warnings` en CI, sur le modèle du `ruff` déjà en place côté Python | Lint bloquant |
| 0.3 | GitLab CI : build + test + clippy sur runner Linux — **matrice Windows différée à la phase 5** (cahier des charges §1.1, resserrement du 2026-09-08), pas construite par anticipation | Pipeline |
| 0.4 | Choix et verrouillage des crates de base — **voir `cahier-des-charges-technique.md` §4.1 : ni `slotmap` ni `uuid`**, l'undo hybride impose un `NodeId(u64)` monotone jamais réutilisé | `Cargo.toml` figé |
| 0.5 | Un test smoke qui construit un `Project` vide et le détruit sans fuite (`cargo test`, éventuellement `miri` en CI optionnelle) | CI verte |

> Pas de binaire packagé à ce stade (pas d'UI à empaqueter) — l'équivalent
> `briefcase`/AppImage n'a de sens qu'une fois une UI choisie (§7), donc hors
> périmètre de cette phase.

**Sortie de phase** : trois crates qui compilent, testent en CI, sans aucune
dépendance UI dans `plotyx-core`.

---

## Phase 1 — Modèle de données (≈1,5-2 semaines)

> **Statut : close le 2026-09-08.** Livré dans `plotyx-core` : `NodeId`/`IdAllocator`, `check_name`, grammaire de chemins, `Column` (buffers `Arc`, deltas), `Node`/`Tree`/`Project`. Écarts consignés : égalité des flottants **bit à bit** (`NaN == NaN`, sinon aucun test d'égalité d'arbre ne tient) ; primitives de lignes **par colonne** en plus de celles par feuille (nécessaires à l'annulation de `DeleteRows`) ; `path_of` d'une fenêtre à la racine rend `/[Book1]` (même forme qu'en Python). Coût réel d'une écriture sous snapshot : cahier des charges §4.3.

**Objectif** : le même contrat que la phase 1 Python, dans `plotyx-core`,
100 % testable sans UI.

### 1.1 Choix de structure — écart volontaire par rapport à Python

> **Révisé le 2026-09-08, argument corrigé le même jour par la revue
> technique** : la clé générationnelle proposée ci-dessous est remplacée par
> un `NodeId(u64)` monotone jamais réutilisé. Le motif n'est **pas** une
> collision d'identité — les générations de `slotmap` l'empêchent
> précisément — mais le fait qu'une arène **ne sait pas réinsérer à une clé
> donnée** : un nœud restauré par `undo` recevrait une clé neuve, ce qui
> casse le contrat de `cahier-des-charges-api-script.md` §3 (« une clé
> résolue vers un objet supprimé puis restauré par `undo` reste valide »).
> Raisonnement complet, et deux autres raisons indépendantes, dans
> `cahier-des-charges-technique.md` §4.1, qui fait foi. Reste valable du
> texte ci-dessous : une carte plate à clés plutôt que des `Rc`, et cette
> carte tenant lieu de registre — c'est la `HashMap<NodeId, Arc<Node>>` de
> la spec §4.2 qui joue ce rôle, pas une arène.

Le modèle Python utilise des références directes en mémoire (objets Qt-liés,
signaux). En Rust, un arbre à parents/enfants avec références directes se
heurte tout de suite au borrow checker (`Rc<RefCell<>>` partout = perte des
garanties qu'on est venu chercher). **Décision proposée : une arène
générationnelle** (`slotmap` ou équivalent), pas des `Rc`:

- Chaque nœud (`Folder`, `Workbook`, `Worksheet`, `Column`, `Graph`, `Matrix`,
  `Note`) vit dans une arène unique, adressé par une clé générationnelle
  (index + génération — invalide automatiquement toute clé pointant vers un
  slot réutilisé).
- **Bénéfice direct pour le §4 de `resume-decisions.md`** : cette clé
  d'arène est structurellement la même chose que la clé opaque UUID
  d'`ObjectRegistry` côté Python (`core/registry.py`) — au lieu de maintenir
  un registre séparé en plus de l'arbre, l'arène **est** le registre. Une
  simplification réelle, pas juste un changement de vocabulaire.
- Un objet supprimé puis restauré par `undo` doit garder la **même** clé —
  implique de ne pas libérer le slot au moment de la commande de suppression
  mais seulement quand la commande elle-même sort de la pile (même règle
  que la décision 4 de la phase 2 Python, cf. §2.3 plus bas).

### 1.2 Contenu du modèle — repris à l'identique, langage mis à part

- `Column` : `Arc<Vec<f64>>` (au lieu de `np.ndarray`) — le `Arc` est
  requis par le partage sans copie vers numpy et par le
  copie-à-l'écriture, cf. spec §4.3 — + métadonnées identiques
  (nom, unité, label, commentaire, rôle X/Y/yErr, format d'affichage, type) ;
  valeur manquante = `f64::NAN`, pas d'`Option<f64>` par élément — même
  décision 1 de la phase 1 Python, pour la même raison (vitesse, densité
  mémoire, pas de sentinelle par ligne). Colonne texte : type séparé
  (`Vec<Option<String>>` ou équivalent), même distinction valeur-manquante
  vs chaîne-vide qu'aujourd'hui.
- `Worksheet` : liste ordonnée de clés-colonnes, longueurs hétérogènes
  tolérées (décision 2 Python conservée à l'identique — l'argument ne
  dépend pas du langage).
- `Project` : arbre `Folder → Workbook → Worksheet → Column`, `Graph` /
  `Matrix` / `Note` en nœuds frères du `Workbook`, chemins uniques
  `[Book1]Sheet1!ColA` — grammaire de `core/paths.py` reprise telle quelle
  (déjà actée comme socle stable dans `cahier-des-charges-api-script.md`
  §3).
- ~~Notifications : canal d'événements interne (`crossbeam-channel` ou
  callbacks enregistrés) rejouant `dataChanged`/`structureChanged`.~~
  **Révisé le 2026-09-08 — sans objet.** Il n'y a pas d'abonnement dans
  `plotyx-core` : l'acteur (spec §6) répond un `Snapshot { tree, revision }`
  et le client compare la `revision` à celle qu'il détient. Pas d'abonné,
  donc pas de ré-entrance possible, donc pas de traduction vers une boucle
  d'événements à concevoir. La règle « pas de signal Qt dans `plotyx-core` »
  reste vraie, elle est simplement devenue triviale.

### 1.3 Ce qui ne se transpose pas tel quel

- ~~Le canal d'événements du modèle doit rester **synchrone et sans
  ré-entrance**.~~ **Révisé le 2026-09-08 — sans objet**, pour la même
  raison qu'en §1.2 : il n'y a pas de canal d'événements. La classe de bug
  visée (un abonné qui mute le modèle pendant qu'il traite une
  notification) disparaît avec l'abonnement lui-même — c'est un bénéfice
  supplémentaire de l'acteur, à ne pas perdre de vue si l'idée d'un canal
  d'abonnement revenait un jour.
- Pas de `weakref` idiomatique en Rust — le rôle des références faibles de
  `ObjectRegistry` (décision 4 phase 1 Python) est repris par le `NodeId`
  (§1.1, révisé) : une clé absente de l'arbre courant est l'équivalent
  exact d'un `weakref` mort, sans bibliothèque dédiée. C'est ce que l'API
  de script expose comme `BrokenReferenceError`.

**Sortie de phase** : `cargo test` couvre `plotyx-core` à un niveau
équivalent aux >85 % Python, zéro dépendance UI dans le crate (vérifié par
un test qui échoue si une dépendance non listée en whitelist apparaît dans
`Cargo.toml`, sur le même principe que `tests/test_core_no_qt.py`).

---

## Phase 2 — Bus de commandes, Undo & acteur (≈2 semaines)

> **Statut : close le 2026-09-08.** `plotyx-commands` (8 commandes, `History` à entrées `Structural`/`Data`/`Macro`, limite 100 par défaut, propriété `proptest` 50+ commandes → undo → égalité bit à bit → redo → égalité) et `plotyx-session` (acteur sur thread nommé, `std::sync::mpsc`, `Snapshot { tree, revision, history }`, 4 threads × 50 commandes → 200 révisions contiguës). Décisions consignées : spec §5.3 (macro avortée restaure le redo ; commande sans effet reste une entrée), §5.4 (limite), §6.1 (contenu du snapshot) ; `questions-ouvertes.md` question 9 (macro globale à la session). Les requêtes `Save`/`Load` de l'acteur arrivent avec la phase 3.

**Objectif** : la même règle d'or que la phase 2 Python — **aucune mutation
du modèle en dehors d'une commande** — sans `QUndoStack`, qui est une classe
Qt et n'existe pas si le cœur reste sans dépendance Qt (cf. `resume-decisions.md`
§3-4 : UI cible envisagée egui, pas cxx-qt).

> **Révisé le 2026-09-08.** Deux points de cette phase sont remplacés par
> `cahier-des-charges-technique.md`, qui fait foi :
> - la pile n'est pas un `Vec<Box<dyn Command>>` homogène mais une pile à
>   **deux natures d'entrée** (snapshot de structure / delta de données),
>   cf. spec §2.1 et §5.1 — le §2.3 ci-dessous (durée de vie des objets
>   supprimés) devient **sans objet** : l'ancien arbre retient l'objet ;
> - la phase inclut désormais **l'acteur** (`plotyx-session`, spec §6) :
>   c'est lui qui détient le `Project`, ce qui fait de la règle d'or une
>   propriété de propriété plutôt qu'une convention testée.
>
> Restent valables tels quels : l'application avant empilement (§2.1), la
> sémantique des macros (§2.1) et l'invariant lignes (§2.4).

### 2.1 Remplacement de `QUndoStack` — à construire, pas à importer

Pas d'équivalent Rust mûr à réutiliser tel quel pour ce cas précis (pile de
commandes + macros + intégration undo-menu native) : `plotyx-commands`
définit son propre `CommandBus`, sur le même contrat que Python plutôt que
sur l'implémentation :

```
trait Command {
    fn apply(&mut self, project: &mut Project) -> Result<(), CommandError>;
    fn unapply(&mut self, project: &mut Project);
    fn label(&self) -> &str;
}

struct CommandBus {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    limit: Option<usize>,
    macro_depth: usize,
    // ...
}
```

- `CommandBus::push()` **applique avant d'empiler**, exactement la décision 3
  de la phase 2 Python (`apply()`/`redo()` peut échouer — nom en double,
  index hors bornes — la pile ne doit pas bouger si `apply` retourne `Err`).
  Cet argument est en fait **plus net en Rust** : Python le justifiait pour
  contenir une exception traversant la frontière C++ de Qt ; en Rust,
  `Result<(), CommandError>` est le mécanisme d'erreur ordinaire, la même
  règle s'impose donc sans qu'il y ait de frontière FFI en jeu.
- Commandes de base — **liste faisant foi : spec §5.2**, huit primitives :
  `SetCells`, `AddColumn`, `DeleteColumn`, `Rename`, `Move`,
  `SetColumnType`, `InsertRows`, `DeleteRows`. (La rédaction initiale n'en
  citait que cinq ; les trois manquantes sont celles qui portent
  l'invariant lignes du §2.4 et le seul snapshot de données assumé,
  `SetColumnType`.) Les commandes de `PlotSpec` viendront avec le système
  de graphes, hors socle.
- Stratégie mémoire identique : delta/paramètres d'abord, snapshot en
  dernier recours (ex. changement de type de colonne, décision 5 Python),
  copy-on-write sur les buffers non touchés — `Vec<f64>` derrière un `Arc`
  partagé jusqu'à la première écriture donne le COW gratuitement, sans
  bibliothèque dédiée.
- `begin_macro()`/`end_macro()`/`abort_macro()` : même sémantique que la
  décision 6 Python, mais **plus simple à implémenter** — pas de contrainte
  `QUndoStack` sans `cancelMacro()` à contourner par une commande qui se
  déclare obsolète pendant son `redo()`. Le bus maison peut directement
  dépiler et annuler les commandes de la macro en cours puis les jeter, sans
  détour.

### 2.2 Paquet séparé — la contrainte Python ne s'applique plus, la structure reste bonne

La séparation `commands` / `core` en Python vient de ce que Qt ne doit pas
fuiter dans `core/`. Cette contrainte précise disparaît (aucun des deux
crates ne dépend de Qt), mais **la séparation elle-même reste justifiée** :
`plotyx-core` reste utilisable seul (scripts, tests, futur binding script
cf. `cahier-des-charges-api-script.md`) sans embarquer la logique
d'historique. Dépendance à sens unique conservée : `plotyx-commands` connaît
`plotyx-core`, jamais l'inverse.

### 2.3 Durée de vie des objets supprimés — ~~via l'arène~~ sans objet

> **Révisé le 2026-09-08 — cette section n'a plus d'objet**, et c'est le
> bénéfice principal de l'undo hybride (spec §2.1). L'ancien arbre, retenu
> par l'entrée d'historique, retient le nœud supprimé : il n'y a **rien à
> libérer explicitement**, donc rien à comptabiliser. Une entrée évincée par
> la limite d'historique relâche son `Arc`, et le nœud disparaît à ce
> moment-là, tout seul (spec §5.4). Texte d'origine conservé ci-dessous
> comme mémoire de l'option écartée.

~~Décision 4 de la phase 2 Python (une commande de suppression garde une
référence forte, le bus fait le ménage à l'éviction). En Rust, avec l'arène
de la phase 1 : une commande de suppression retient la **clé** de l'objet
retiré, pas l'objet lui-même — le slot de l'arène n'est marqué libre qu'au
moment où la commande elle-même sort définitivement de la pile (éviction par
`limit`, troncature de la branche redo, `clear()`). Testable directement en
comptant les slots vivants de l'arène avant/après, sans `gc`/`weakref`.~~

### 2.4 Invariant lignes — hérité tel quel

L'écart corrigé en Python entre `insert_rows` et `delete_rows` (les deux
doivent ignorer le même ensemble de colonnes trop courtes pour être
atteintes) est un invariant du **modèle** (phase 1), indépendant du
langage — à porter comme test dès la phase 1, pas à redécouvrir en phase 2
côté Rust.

**Sortie de phase** : même test de fond que Python — script qui applique 50
commandes aléatoires, undo tout, vérifie l'égalité bit à bit avec l'état
initial (`proptest` ou un test maison équivalent à
`tests/test_undo_random.py`).

---

## Phase 3 — Persistance minimale (≈1-1,5 semaine)

> **Statut : close le 2026-09-08.** `plotyx-persistence` (`save`/`load` d'un
> `Project` complet, écriture atomique, en-tête `PLYX` + version lue avant
> tout, structures de transport propres au crate) et les requêtes
> `Save`/`Load` de l'acteur (`Load` vide l'historique, est refusé pendant une
> macro, et laisse le projet intact si la lecture échoue). **Format : `serde`
> + MessagePack (`rmp-serde`), pas `bincode`** — la propriété « champs
> inconnus ignorés » impose un conteneur auto-descriptif ; raison complète
> dans le cahier des charges §7. Corrigé au passage : l'allocateur
> d'identifiants sautait un cran à chaque rechargement (confusion entre
> « prochain identifiant » et « plus grand attribué »).


**Objectif** : un `save`/`load` réel sur disque, pas encore le format
`.plx` final. Placée **avant** l'UI, pas après — deux raisons de fond, pas
de confort :

- Le schéma du modèle (phase 1) n'est vraiment validé que quand il a
  survécu à un aller-retour disque réel — test plus dur que n'importe quel
  test en mémoire, à faire porter tôt.
- Le jalon de sortie de cette étape (phase 4) est « fenêtre + fichier +
  Ctrl+Z » : « fichier » est une dépendance de ce jalon, pas une
  conséquence — il doit donc exister avant la coquille UI, pas après.

### 3.1 Frontière, pas format — ce qui compte à ce stade

Pas besoin d'implémenter ici le schéma Zarr final étudié dans
`format-fichier-plx.md` : un format provisoire (`serde` + `bincode`, ou
équivalent) suffit, **à condition que la frontière soit la bonne** :

```
fn save(project: &Project, path: &Path) -> Result<(), PersistenceError>;
fn load(path: &Path) -> Result<Project, PersistenceError>;
```

Une seule fonction dans chaque sens, qui prend/rend un `Project` complet,
rien d'autre — aucun appelant (phase 4, futur binding script) ne doit
connaître le format sous-jacent. **Deux couplages sous la signature** sont
traités en spec §7.1 : les structures de transport vivent dans
`plotyx-persistence` (pas de dérivations `serde` dans `plotyx-core`), et le
chargement est assumé **intégral** — la lecture partielle qu'autorisera
Zarr s'ajoutera à côté, elle ne remplacera pas `load`. Si cette frontière est respectée
maintenant, le format binaire provisoire peut être remplacé par Zarr plus
tard sans rien casser au-dessus ; la deviner autrement ferait porter le
coût du changement de format à la coquille UI et au reste.

### 3.2 Ce qui se garde du design déjà étudié, même en version provisoire

Repris de `format-fichier-plx.md` §3, indépendamment du conteneur choisi
pour cette phase :

- écriture **atomique** (fichier temporaire + renommage), pas de mutation
  in-place partielle qui pourrait laisser un fichier à moitié écrit ;
- **numéro de version en premier**, lu avant tout le reste, avec refus
  propre sur un fichier futur — même à ce stade provisoire, pour ne pas
  avoir à le rajouter après coup sur des fichiers déjà écrits ;
- distinction **valeur manquante vs chaîne vide** pour les colonnes texte.

### 3.3 Ce qui n'est délibérément pas fait ici

- Pas de chunking, pas de lecture partielle d'une colonne à 1M de points —
  ce sont des propriétés du futur format Zarr, pas un besoin de ce jalon.
- Pas de script de conversion depuis un `.plx` HDF5 existant
  (`format-fichier-plx.md` §5) — à écrire seulement si et quand un tel
  fichier doit réellement être ouvert.

**Sortie de phase** : un test qui construit un `Project` non trivial
(plusieurs `Workbook`/`Worksheet`/`Column`, valeurs manquantes incluses),
le sauvegarde, le recharge, et vérifie l'égalité complète avec l'original.

---

## Phase 4 — Coquille UI minimale (≈1 semaine)

> **Statut : close le 2026-09-08.** `plotyx-app` : fenêtre `eframe`/`egui`
> 0.36, grille virtualisée, panneau des feuilles, barre de menus aux entrées
> grisées et libellées (« Annuler *Saisie dans A* »), Ctrl+Z / Ctrl+Y /
> Ctrl+Maj+Z / Ctrl+S / Ctrl+O. **Toute la logique est dans un `AppState`
> sans dépendance à egui**, piloté par un enum d'actions : c'est ce qui rend
> la boucle vérifiable en test automatique — 22 tests d'intégration sans
> fenêtre, dont le critère de sortie de phase (saisir, enregistrer, saisir
> autre chose, rouvrir, arbre **égal bit à bit**). La fenêtre elle-même est
> prouvée par une auto-fermeture pilotée par variable d'environnement :
> ouverture, rendu, sortie en code 0. Deux constats non anticipés en
> §4.3 et §6.2 du cahier des charges.


**Objectif** : faire tourner **la boucle complète** une fois — modèle →
commande → undo → sauvegarde → rechargement — dans une vraie fenêtre, avant
d'investir dans le docking ou la grille éditable. Une intégration bout-en-
bout ratée coûte une réécriture à ce stade ; ratée après avoir construit du
docking par-dessus, elle coûte la réécriture *et* le docking avec.

Volontairement pauvre — pas une ébauche de l'UI finale, un banc d'essai :

- une fenêtre `egui`/`eframe` qui s'ouvre, une grille brute (pas
  d'éditique, pas de délégués) qui affiche une `Worksheet` ;
- Ctrl+Z / Ctrl+Y (et Ctrl+Shift+Z) câblés à la main sur les requêtes
  `Undo`/`Redo` de l'acteur — **à
  l'inverse de Qt, rien de tout ça n'existe avant d'être écrit.**
  `QUndoStack` + `QAction::Undo` donnaient le raccourci, le menu et son
  libellé (« Annuler *Renommer*… ») presque gratuitement dès que les
  commandes existaient ; ici il n'y a aucun framework d'undo fourni par
  `egui` — le gestionnaire de raccourci, l'état du menu (grisé si pile
  vide) et le libellé sont à écrire un par un, et n'apparaissent jamais
  avant qu'on les code ;
- Ctrl+S / Ctrl+O câblés sur `save`/`load` (phase 3).

**Sortie de phase** : une fenêtre s'ouvre, on modifie une valeur, Ctrl+Z
l'annule, Ctrl+S écrit un fichier, rouvrir ce fichier restitue l'état
exact. Ce n'est pas encore la fin du socle — il manque le pont Python
(phase 5) pour que le chantier de l'interface puisse s'ouvrir. Le docking,
la grille éditable, le panneau Properties et le système de graphes restent
hors de ce jalon (§6-7).

---

## Phase 5 — Pont Rust ↔ Python (≈2-3 semaines)

> **Statut : close sur Linux le 2026-09-09**, hors tâche 5.1b (Windows), qui
> reste la condition de clôture définitive du socle (§8.6). Le critère du
> §1.1 est vérifié : `crates/plotyx-script/tests/script.rs`,
> `critere_de_fin_lecture_sans_copie_ecriture_par_le_bus_annulation_par_ctrl_z`.
> **Vérifiable en une commande depuis le paquet, depuis le 2026-09-09** :
> `dist/plotyx-app/plotyx-selftest` rejoue les sept étapes du critère et sort
> en 0 ou en 1 ; `scripts/app-dist.sh` le lance en fin d'empaquetage, en
> environnement vidé et piégé, et échoue si le code n'est pas 0. La même
> logique tourne dans `cargo test` (`crates/plotyx-script/tests/selftest.rs`).
> Sa capacité à **échouer** a été prouvée en sabotant une étape.
>
> **Ce qui reste non prouvé, et qu'il ne faut pas croire couvert** : le
> chemin **interface** proprement dit. L'auto-vérification appelle l'acteur
> directement, comme le ferait la fenêtre ; elle ne passe par aucune ligne de
> `plotyx-app`. Les raccourcis Ctrl+Z / Ctrl+Y / Ctrl+S / Ctrl+O, la saisie
> dans la grille et le panneau de script ne sont couverts que par les tests
> sans fenêtre de `plotyx-app`, et l'empaquetage ne prouve d'eux que
> l'ouverture et la fermeture de la fenêtre. **Personne n'a encore cliqué.**


**Objectif** : la tuyauterie, éprouvée — **sans** système de graphes
par-dessus. C'est la phase qui rend « Python accessible » avant d'ouvrir le
chantier de l'interface. Spécification complète : `cahier-des-charges-technique.md`
§8, qui fait foi.

Peut démarrer **en parallèle des phases 3-4** : `plotyx-python` ne dépend
ni de la persistance ni de l'UI. Seule sa surface-témoin attend l'acteur de
la phase 2.

| # | Tâche | Verrou |
|---|---|---|
| 5.1a | ✅ **Fait le 2026-09-08** — hello-world empaqueté Linux : binaire + CPython 3.12.14 vendored + numpy 2.5.3, PyO3 0.29.2 / rust-numpy 0.29.0. Les 7 pièges de la spec §8.6.1 traversés, **plus deux non prévus** (GIL retenu après l'initialisation, `__pycache__` à conserver — §8.6.1). Mesures : init ≈ 51 ms, aller 1M `f64` ≈ 0,8 ms, retour ≈ 0,5 ms, distribution 307 Mo. **Le verrou est levé.** | **avant tout le reste de la phase** |
| 5.1b | Le même hello-world rejoué **sur Windows** — jalon distinct, après 5.1a, **avant de déclarer le socle clos** (spec §1.1, §8.6) ; peut glisser après 5.2-5.6 si Windows n'est pas encore requis pour avancer sur Linux | après 5.1a ; avant la clôture du socle |
| 5.2 | ✅ **Fait le 2026-09-08** — emprunt sans copie Rust → numpy (`borrow_from_array` + conteneur `#[pyclass]` clonant l'`Arc`), copie numpy → Rust y compris **non contiguë**. 19 tests, dont l'égalité **du pointeur** vu par numpy et du buffer Rust, la vue périmée, la croissance sous la vue, la survie au `drop` côté Rust. **Emprunt ≈ 5-9 µs contre ≈ 2 ms pour la copie** d'un million de flottants. Contrôle instrumenté (`valgrind`) **exécuté le 2026-09-09 : zéro erreur** (§8.3.1). | après 5.1, avant 5.3 |
| 5.3 | ✅ **Fait le 2026-09-09** — initialisation paresseuse et idempotente, mode isolé, aucune finalisation, préfixe embarqué. Le fil qui initialise **relâche le GIL** aussitôt (piège 8, §8.6.1). | après 5.1 |
| 5.4 | ✅ **Fait le 2026-09-09** — `plotyx-script` : fil **permanent** (jamais un fil par script), GIL relâché pendant l'attente de l'acteur (vérifié par un test où un autre fil prend le GIL pendant que le script boucle), `print` et traces d'appels capturés, interruption à deux moitiés (drapeau lu par les appels `pyx` **et** `PyThreadState_SetAsyncExc` pour une boucle Python pure — les deux testées). | après 5.3 et phase 2 |
| 5.5 | ✅ **Fait le 2026-09-09** — quatre appels, poignée opaque (`#[pyclass]`, pas un entier fabricable), trois erreurs nommées. **`pyx.undo()` rend `False` sur une pile vide** (question 2 tranchée par sa recommandation). 21 tests, dont **le critère de fin du §1.1** : un script lit `A` sans copie (adresse du buffer vérifiée identique), écrit `B` par le bus, et un `undo` venu du côté Rust rétablit l'arbre **bit à bit**. | après 5.4 |
| 5.6 | ✅ **Fait le 2026-09-09** — panneau de script repliable dans la fenêtre : saisie, « Exécuter » (Ctrl+Entrée), « Arrêter », zone de sortie. Le script tourne sur un **fil détaché** sondé à chaque image : la fenêtre ne gèle pas, donc le bouton « Arrêter » reste utilisable. L'interpréteur ne démarre qu'au **premier** script, jamais à l'ouverture. `plotyx-script` est une dépendance **optionnelle** : sans la feature, la fenêtre s'ouvre et fonctionne sans Python. `scripts/app-dist.sh` empaquette le tout (333 Mo) et le lance en environnement nettoyé. | après 5.5 et phase 4 |

> ⚠️ **5.1a et 5.2 sont des verrous, pas des tâches.** L'empaquetage d'une
> pile Python par plateforme est le risque le plus lourd de toute
> l'architecture, et le budget de rendu du futur système de graphes repose
> entièrement sur le zéro-copie de 5.2. Les découvrir en fin de phase
> coûterait la phase entière — c'est le « point de vigilance n°3 » du
> roadmap Python appliqué ici. **5.1b (Windows) n'est pas un verrou sur le
> reste de la phase** — resserrement du 2026-09-08 : le pont s'écrit et se
> teste sur Linux, Windows valide ensuite qu'il n'y a rien d'implicite dans
> ce qui a été construit.

**Sortie de phase, sur Linux — le critère du §1.1 du cahier des charges** :
un script tapé dans la fenêtre lit une colonne sans copie, en écrit une
autre, et **Ctrl+Z annule ce qu'il a écrit**. **Sortie du socle** :
le même critère rejoué sur Windows (5.1b), pas avant.

---

## 6 — Ce que ce plan ne couvre pas, volontairement

Au-delà d'ici, plus de socle : des chantiers qui se posent dessus, et dont
les décisions sont déjà arrêtées (`cahier-des-charges-technique.md` §12)
sans être planifiées :

- **système de graphes** — `PlotSpec` neutre et renderer **Plotters**
  (backend egui écrit par nous), en trois crates : `plotyx-plot`,
  `plotyx-render-egui`, `plotyx-render-svg`. Volontairement exclu du
  socle : la tuyauterie doit être solide avant qu'on construise dessus.
  **Désormais phasé à part** : `docs/newarchitecture/roadmap-rendu.md`
  (phases R0-R8, arrêtées le 2026-09-10). Une seule phase y touche au
  socle — R3, qui ancre le `PlotSpec` dans `NodeKind::Graph`, la « tranche
  ultérieure » qu'annonce `node.rs`.
  *Corrigé le 2026-09-10 — était « renderer Matplotlib dans l'interpréteur
  de la phase 5 » ;*
- **surface `plotyx.api` v1** — `cahier-des-charges-api-script.md` ; la
  surface-témoin de 5.5 en est un sous-ensemble strict, choisi pour
  éprouver le pont, pas pour être utile ;
- **UI réelle** — docking, grille éditable, panneau Properties ; la
  coquille de la phase 4 n'en a volontairement rien ;
- **format `.plx` final** (Zarr) — `format-fichier-plx.md` ; la phase 3 ne
  construit qu'une frontière, pas ce format ;
- **import de fichiers** — les 10 fixtures de
  `legacy-python/tests/fixtures/import/` en sont la spécification le jour
  venu.

---

## 7 — Notes techniques pour l'UI (mémoire, pas un phasage)

L'UI sera retravaillée entièrement en conversation au moment de la
construire — pas de phases figées à l'avance ici. Ce qui suit est le
répertoire des éléments déjà discutés, à ne pas rejouer, sans engager un
ordre ni un découpage :

- **egui retenu sur iced** pour le docking (`egui_dock`, éprouvé sur
  Rerun.io) — détail complet et limites connues (thème, HiDPI/IME, menu
  natif, `QTableView` équivalent plus pauvre) dans `resume-decisions.md` §3.
  La limite « `egui_plot` insuffisant pour remplacer Matplotlib » y reste
  vraie et n'est **pas** contredite par la décision ci-dessous : le rendu
  ne passe pas par `egui_plot`, mais par Plotters, egui ne servant que de
  surface de dessin.
- **Renderer réécrit en Rust natif — décidé le 2026-09-10.** **Plotters**
  + un backend `DrawingBackend` écrit par nous sur egui (~250-300 lignes ;
  `egui-plotter` écarté, il dépend d'egui 0.32/0.35 quand nous sommes en
  0.36.2). Le backend collecte dans un `Vec<Shape>` et met en cache sur
  `(revision, rect, pixels_per_point)` — le renderer devient un client de
  plus de l'acteur. Labels scientifiques par **Typst**, derrière un
  *feature flag* comme `python`. Export SVG natif, PDF via `svg2pdf`.
  Détail, chiffrage et options écartées : `docs/newarchitecture/`.
  > *Remplace « Renderer conservé, pas réécrit » (Matplotlib embarqué via
  > PyO3 + `rust-numpy`, sortie `FigureCanvasAgg.buffer_rgba()` en texture
  > egui, `resume-decisions.md` §4). L'interpréteur **vendored et figé**
  > reste, lui — il sert la console de script, cf. ci-dessous.*
- **Nuance de robustesse à reporter, pas à perdre** : l'argument « un appel
  PyO3 renvoie un `PyResult`, donc une exception devient une `Err` Rust
  gérée à la frontière plutôt que de dérouler la boucle d'événements »
  visait Matplotlib. Il **reste valable pour la console de script**, qui
  est désormais le seul consommateur de PyO3 — il contient le bug, il ne
  l'élimine pas. Côté rendu, il devient sans objet : plus de Python sur ce
  chemin, donc plus d'exception à contenir.
- **Console de script** : même interpréteur PyO3 que le renderer, deuxième
  porte d'entrée quasi gratuite une fois le pipeline en place. Contrat
  complet dans `cahier-des-charges-api-script.md` (surface v1, modèle de
  confiance, exécution sur thread dédié avec interruption coopérative).
- **Format de fichier `.plx`** : HDF5 remis en question spécifiquement pour
  un cœur Rust (FFI `libhdf5`, thread-safety fragile) — **Zarr** retenu
  (`zarrs`, Rust pur, hiérarchie de groupes native, chunking utile pour
  l'acquisition live, mode *zip store* pour garder un fichier unique).
  Détail complet, alternatives écartées et non-rétrocompatibilité assumée
  dans `format-fichier-plx.md`.
- **Mode « visualiseur léger » (ex-PyQtGraph) : plus une question ouverte.**
  Tranché le 2026-09-08 (spec §13) — le choix « garder ou abandonner » est
  **reporté au chantier de l'UI réelle**, pas laissé en suspens : aucun
  crate du socle n'en dépend, et son seul argument différenciant
  (`useOpenGL`) n'est de toute façon activé nulle part aujourd'hui
  (`resume-decisions.md` §8). À trancher quand l'UI se construira, avec les
  éléments qu'on n'a pas encore.
- **Bug de classe éliminé par construction en UI immédiate** : la fuite par
  connexion signal Qt oubliée (cf. mémoire projet
  `plotyx-orphaned-subwindow-bug`) ne peut pas se produire de la même façon
  en egui — rien à désabonner. Argument de robustesse propre à l'UI, pas au
  modèle/bus de ce document.
- **Calendrier non chiffré** — à faire avant tout engagement réel sur l'UI,
  sur le modèle du roadmap phasé Python existant.

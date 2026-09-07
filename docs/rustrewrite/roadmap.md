# Roadmap Rust — jusqu'à la boucle complète (fenêtre + fichier + undo)

Statut : **étude, rien d'engagé**. Ce document ne remplace pas `ROADMAP.md`
(Python/Qt, actif) — c'est le plan d'un portage éventuel.

Phases 1-2 : le socle jugé « bas risque, portage mécanique » dans
`resume-decisions.md` §7 — modèle de données, bus de commandes, undo/redo.
Phases 3-4 : le prolongement logique décidé le 2026-09-07 pour valider ce
socle en conditions réelles avant d'investir dans le confort visuel —
persistance minimale puis coquille UI minimale, ensemble le premier jalon
démontrable (« ça tourne, ça s'annule, ça se sauve »). Raisonnement complet
de cet ordre en §5.

Rien au-delà (docking, grille éditable, renderer Matplotlib embarqué,
console de script, UI complète) n'est phasé ici — c'est le « vrai travail
itératif » que `resume-decisions.md` renvoie à plus tard, et l'UI en
particulier sera reprise entièrement en conversation au moment de la
construire, pas planifiée à l'avance (§6 de ce document en garde une trace
technique, pas un phasage).

Référence structurelle : `ROADMAP.md` phases 0-2 (Python), dont ce plan est
la transposition Rust — même découpage, mêmes décisions quand elles tiennent
indépendamment du langage, écarts justifiés un par un.

---

## Phase 0 — Fondations projet Rust (≈3-5 jours)

**Objectif** : squelette qui compile, teste et s'exécute en CI, avant tout
code métier — même principe que la phase 0 Python (le packaging/CI se valide
tout de suite, pas à la fin).

| # | Tâche | Livrable |
|---|---|---|
| 0.1 | Workspace Cargo multi-crates : `plotyx-core` (modèle, zéro dépendance UI), `plotyx-commands` (bus/undo), `plotyx-cli` (binaire de test headless) | `cargo build` vert, 3 crates |
| 0.2 | `rustfmt` + `clippy -D warnings` en CI, sur le modèle du `ruff` déjà en place côté Python | Lint bloquant |
| 0.3 | GitLab CI : build + test + clippy sur runner Linux, matrice Linux/Windows dès que 0.1 le permet | Pipeline |
| 0.4 | Choix et verrouillage des crates de base — **voir `cahier-des-charges-technique.md` §4.1 : ni `slotmap` ni `uuid`**, l'undo hybride impose un `NodeId(u64)` monotone jamais réutilisé | `Cargo.toml` figé |
| 0.5 | Un test smoke qui construit un `Project` vide et le détruit sans fuite (`cargo test`, éventuellement `miri` en CI optionnelle) | CI verte |

> Pas de binaire packagé à ce stade (pas d'UI à empaqueter) — l'équivalent
> `briefcase`/AppImage n'a de sens qu'une fois une UI choisie (§6), donc hors
> périmètre de cette phase.

**Sortie de phase** : trois crates qui compilent, testent en CI, sans aucune
dépendance UI dans `plotyx-core`.

---

## Phase 1 — Modèle de données (≈1,5-2 semaines)

**Objectif** : le même contrat que la phase 1 Python, dans `plotyx-core`,
100 % testable sans UI.

### 1.1 Choix de structure — écart volontaire par rapport à Python

> **Révisé le 2026-09-08** : la clé générationnelle proposée ci-dessous est
> remplacée par un `NodeId(u64)` monotone jamais réutilisé — une arène à
> réutilisation de slots entre en collision d'identité lors d'une
> restauration par `undo`. Raisonnement complet et décision qui fait foi :
> `cahier-des-charges-technique.md` §4.1. Le reste de cette section (une
> arène plutôt que des `Rc`, l'arène tenant lieu de registre) reste valable.

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

- `Column` : `Vec<f64>` (au lieu de `np.ndarray`) + métadonnées identiques
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
- Notifications : pas de signal Qt possible dans `plotyx-core` (zéro
  dépendance UI, même règle que Python). Un canal d'événements interne
  (`crossbeam-channel` ou callbacks enregistrés) rejoue le rôle de
  `dataChanged`/`structureChanged` ; la couche UI choisie plus tard
  s'abonne à ce canal — pas de traduction vers un event-loop UI décidée ici,
  volontairement (dépend du choix UI, §6).

### 1.3 Ce qui ne se transpose pas tel quel

- Le canal d'événements du modèle doit rester **synchrone et sans
  ré-entrance** (un abonné qui déclenche une mutation pendant qu'il traite
  un événement est un bug, à interdire par construction — un `RefCell`
  emprunté deux fois panique plutôt que corrompre, ce qui est déjà une
  amélioration par rapport à l'équivalent Python silencieux).
- Pas de `weakref` idiomatique en Rust — le rôle des références faibles de
  `ObjectRegistry` (décision 4 phase 1 Python) est repris par la
  génération de la clé d'arène (§1.1) : une clé dont la génération ne
  correspond plus au slot est l'équivalent exact d'un `weakref` mort, sans
  bibliothèque dédiée.

**Sortie de phase** : `cargo test` couvre `plotyx-core` à un niveau
équivalent aux >85 % Python, zéro dépendance UI dans le crate (vérifié par
un test qui échoue si une dépendance non listée en whitelist apparaît dans
`Cargo.toml`, sur le même principe que `tests/test_core_no_qt.py`).

---

## Phase 2 — Bus de commandes & Undo (≈1,5-2 semaines)

**Objectif** : la même règle d'or que la phase 2 Python — **aucune mutation
du modèle en dehors d'une commande** — sans `QUndoStack`, qui est une classe
Qt et n'existe pas si le cœur reste sans dépendance Qt (cf. `resume-decisions.md`
§3-4 : UI cible envisagée egui, pas cxx-qt).

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
- Commandes de base identiques : `SetCellsCommand`, `AddColumnCommand`,
  `DeleteColumnCommand`, `RenameCommand`, `MoveCommand`.
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

### 2.3 Durée de vie des objets supprimés — via l'arène, pas via un compteur de références

Décision 4 de la phase 2 Python (une commande de suppression garde une
référence forte, le bus fait le ménage à l'éviction). En Rust, avec l'arène
de la phase 1 : une commande de suppression retient la **clé** de l'objet
retiré, pas l'objet lui-même — le slot de l'arène n'est marqué libre qu'au
moment où la commande elle-même sort définitivement de la pile (éviction par
`limit`, troncature de la branche redo, `clear()`). Testable directement en
comptant les slots vivants de l'arène avant/après, sans `gc`/`weakref`
(Rust n'en a pas besoin ici — c'est un test plus simple qu'en Python, pas
un renoncement).

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
connaître le format sous-jacent. Si cette frontière est respectée
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

**Objectif** : faire tourner **la boucle complète** une fois — modèle →
commande → undo → sauvegarde → rechargement — dans une vraie fenêtre, avant
d'investir dans le docking ou la grille éditable. Une intégration bout-en-
bout ratée coûte une réécriture à ce stade ; ratée après avoir construit du
docking par-dessus, elle coûte la réécriture *et* le docking avec.

Volontairement pauvre — pas une ébauche de l'UI finale, un banc d'essai :

- une fenêtre `egui`/`eframe` qui s'ouvre, une grille brute (pas
  d'éditique, pas de délégués) qui affiche une `Worksheet` ;
- Ctrl+Z / Ctrl+Y câblés à la main sur `CommandBus::undo()`/`redo()` — **à
  l'inverse de Qt, rien de tout ça n'existe avant d'être écrit.**
  `QUndoStack` + `QAction::Undo` donnaient le raccourci, le menu et son
  libellé (« Annuler *Renommer*… ») presque gratuitement dès que les
  commandes existaient ; ici il n'y a aucun framework d'undo fourni par
  `egui` — le gestionnaire de raccourci, l'état du menu (grisé si pile
  vide) et le libellé sont à écrire un par un, et n'apparaissent jamais
  avant qu'on les code ;
- Ctrl+S / Ctrl+O câblés sur `save`/`load` (phase 3).

**Sortie de phase** : le jalon démontrable — une fenêtre s'ouvre, on
modifie une valeur, Ctrl+Z l'annule, Ctrl+S écrit un fichier, rouvrir ce
fichier restitue l'état exact. Le docking, la grille éditable, le panneau
Properties et le renderer restent hors de ce jalon (§5-6).

---

## 5 — Ce que ce plan ne couvre pas, volontairement

Conforme à `resume-decisions.md` §7 : au-delà d'ici, plus de portage
mécanique ni de banc d'essai minimal, seulement du travail itératif à
phaser plus tard si la piste avance :

- arbre de projet exposé en UI, docking, grille éditable, panneau
  Properties — le banc d'essai de la phase 4 n'en a volontairement aucun ;
- pipeline PyO3 (renderer Matplotlib embarqué) et empaquetage d'un Python
  figé par plateforme — indépendant, peut être mené en parallèle de la
  phase 4 ou après, ne bloque rien d'elle ;
- format de fichier `.plx` **final** (Zarr) — étudié séparément dans
  `format-fichier-plx.md` ; la phase 3 ne construit qu'une frontière
  provisoire, pas ce format ;
- API de script — spécifiée dans `cahier-des-charges-api-script.md`,
  s'appuie sur le bus de commandes (phase 2) *et* sur le pipeline PyO3
  (non phasé) : elle ne peut être construite avant que les deux existent,
  sous peine de deviner un contrat qu'il faudrait réécrire.

---

## 6 — Notes techniques pour l'UI (mémoire, pas un phasage)

L'UI sera retravaillée entièrement en conversation au moment de la
construire — pas de phases figées à l'avance ici. Ce qui suit est le
répertoire des éléments déjà discutés, à ne pas rejouer, sans engager un
ordre ni un découpage :

- **egui retenu sur iced** pour le docking (`egui_dock`, éprouvé sur
  Rerun.io) — détail complet et limites connues (thème, HiDPI/IME, menu
  natif, `QTableView` équivalent plus pauvre, `egui_plot` insuffisant pour
  remplacer Matplotlib) dans `resume-decisions.md` §3.
- **Renderer conservé, pas réécrit** : Matplotlib embarqué via **PyO3** +
  **`rust-numpy`** (zéro copie dans les deux sens), sortie en buffer RGBA
  (`FigureCanvasAgg.buffer_rgba()`) uploadée comme texture egui.
  Interpréteur **vendored et figé** (`python-build-standalone`), pas de
  Python système — détail en `resume-decisions.md` §4.
- **Bénéfice de robustesse identifié pour cette architecture précise** : un
  appel PyO3 renvoie un `PyResult`, donc une exception Matplotlib devient une
  `Err` Rust gérée à la frontière plutôt que de dérouler la boucle
  d'événements — contient le bug, ne l'élimine pas (nuance à garder).
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
- **Question ouverte, non tranchée** : garder PyQtGraph/le mode
  « visualiseur léger » dans un portage Rust, ou l'abandonner — son seul
  argument différenciant (`useOpenGL`) n'est toujours pas activé nulle part
  à ce jour (`resume-decisions.md` §8).
- **Bug de classe éliminé par construction en UI immédiate** : la fuite par
  connexion signal Qt oubliée (cf. mémoire projet
  `plotyx-orphaned-subwindow-bug`) ne peut pas se produire de la même façon
  en egui — rien à désabonner. Argument de robustesse propre à l'UI, pas au
  modèle/bus de ce document.
- **Calendrier non chiffré** — à faire avant tout engagement réel sur l'UI,
  sur le modèle du roadmap phasé Python existant.

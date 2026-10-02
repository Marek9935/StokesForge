# Développer et maintenir StokesForge avec Codex

Ce guide est le contexte durable du projet. Le fichier `AGENTS.md` à la
racine demande de le lire avant toute implémentation. Un document placé
seulement dans le dossier `.codex/` du dépôt n'est pas une consigne racine
automatiquement chargée : voir la
[documentation officielle](https://learn.chatgpt.com/docs/agent-configuration/agents-md).

## 1. Reconnaître le terrain avant de coder

1. Lire `git status --short`, puis `git diff` sur les fichiers concernés.
   Identifier les modifications existantes avant d'y intervenir.
2. Lire `docs/README.md`, puis la page du domaine ci-dessous. Consulter
   `docs/invariants.md` avant de changer les contrats du modèle, l'historique,
   la session, le rendu ou le pont Python.
3. Rechercher avec `rg` et `rg --files` les symboles, usages, composants
   analogues et tests existants. Lire les commentaires `//!` des modules
   concernés et suivre le chemin depuis l'appelant jusqu'à l'implémentation.
4. Vérifier les `Cargo.toml` concernés, `Cargo.lock`, `rust-toolchain.toml`,
   les scripts pertinents et `.gitlab-ci.yml`. Utiliser les versions et les
   API réellement présentes. Pour inspecter le workspace sans compilation :
   `cargo metadata --no-deps --format-version 1 --offline`.
5. Identifier ce qui sera réutilisé et choisir les contrôles **avant** de
   modifier. Étendre le composant responsable du comportement ; justifier
   toute nouvelle dépendance ou abstraction par un manque constaté.

Charger d'abord le domaine utile, puis élargir aux appelants et dépendants
quand le contrat l'exige. Les sources font foi sur l'état implémenté ; les
demandes de l'utilisateur définissent le changement attendu. Signaler et
corriger les divergences documentaires pertinentes sans transformer la tâche
en refonte générale.

### Skills de projet repris de Claude

Les onze skills maintenus dans [`.claude/skills/`](../.claude/skills/README.md)
sont également disponibles pour Codex via les liens de `.agents/skills/`.
Ce sont les mêmes fichiers : modifier le skill source dans `.claude/skills/`
met à jour les deux assistants. Pour une tâche donnée, lire le `SKILL.md`
concerné et ses seules références utiles, après le parcours de lecture
ci-dessus ; ne pas charger les onze systématiquement.

| Besoin | Skill à consulter |
|---|---|
| Ajouter une fonctionnalité | [`stokesforge-feature`](../.agents/skills/stokesforge-feature/SKILL.md) |
| Diagnostiquer un symptôme, trier des retours | [`stokesforge-bug-report`](../.agents/skills/stokesforge-bug-report/SKILL.md) |
| Corriger un bug reproduit | [`stokesforge-bug-fix`](../.agents/skills/stokesforge-bug-fix/SKILL.md) |
| Modifier le modèle ou ajouter une action annulable | [`stokesforge-commande`](../.agents/skills/stokesforge-commande/SKILL.md) |
| Tracé, axes, légendes et exports | [`stokesforge-rendu`](../.agents/skills/stokesforge-rendu/SKILL.md) |
| Import CSV/TXT/ASC et prévisualisation | [`stokesforge-import`](../.agents/skills/stokesforge-import/SKILL.md) |
| Pont PyO3, scripts et console Python | [`stokesforge-python`](../.agents/skills/stokesforge-python/SKILL.md) |
| Tests, vérification finale et commit | [`stokesforge-verification`](../.agents/skills/stokesforge-verification/SKILL.md) |
| Expliquer le découpage du logiciel | [`stokesforge-architecture`](../.agents/skills/stokesforge-architecture/SKILL.md) |
| Expliquer ce que le logiciel sait faire | [`stokesforge-fonctionnalites`](../.agents/skills/stokesforge-fonctionnalites/SKILL.md) |
| Écrire ou modifier un texte visible | [`stokesforge-textes-interface`](../.agents/skills/stokesforge-textes-interface/SKILL.md) |

Les skills s'enchaînent selon le besoin : `stokesforge-feature` ou
`stokesforge-bug-report` → `stokesforge-bug-fix` si une correction est demandée →
`stokesforge-verification` après une modification. Ajouter `stokesforge-commande`,
`stokesforge-rendu`, `stokesforge-import` ou `stokesforge-python` seulement pour les domaines
touchés. Dans Codex, on peut invoquer explicitement un skill avec
`$stokesforge-feature` (et non la syntaxe `/stokesforge-feature` de Claude).
Les mentions d'outils propres à Claude, comme `AskUserQuestion`, désignent
l'intention du workflow : utiliser l'outil de question disponible dans Codex
si un arbitrage utilisateur est réellement nécessaire. En cas de divergence,
suivre la demande de l'utilisateur, `AGENTS.md`, ce guide et le code actuel.

## 2. Où chercher

Les chemins de ce tableau sont relatifs à la racine du dépôt.

| Domaine | Documentation de départ | Code à examiner |
|---|---|---|
| Découpage et dépendances | `docs/architecture.md` | `Cargo.toml`, manifestes des crates |
| Arbre, colonnes, identifiants, description de graphe | `docs/modele-de-donnees.md` | `stokesforge-core`, `stokesforge-id`, `stokesforge-plot` |
| Undo/redo et macros | `docs/commandes-et-historique.md` | `stokesforge-commands`, méthodes de `stokesforge-core` appelées |
| Sauvegarde, chargement et format `.stf` | `docs/persistance.md` | `stokesforge-persistence`, types du modèle et du graphe |
| Acteur, concurrence, instantanés | `docs/session.md` | `stokesforge-session`, commandes et persistance appelées |
| Tracé, axes, légendes, export, Typst | `docs/rendu.md` | `stokesforge-render` d'abord, puis `stokesforge-render-egui` / `stokesforge-render-svg`, `stokesforge-core/src/resolve.rs` |
| Fenêtre, grille, raccourcis | `docs/application.md` | `stokesforge-app/src/state.rs`, `app.rs`, `selection.rs`, `object_manager.rs`, `theme.rs` ; `stokesforge-session` |
| Import CSV/TXT/ASC et aperçu | `docs/chantier-import-previsualisation.md` (passation) | `stokesforge-import/src/{recipe,parse,preview,store}.rs`, intégration dans `stokesforge-app/src/state.rs` et `app.rs`, `stokesforge-import/tests/fixtures.rs` |
| Python embarqué et scripts `pyst` | `docs/python-et-script.md`, `crates/stokesforge-python/README.md` | `stokesforge-python`, `stokesforge-script`, `scripts/python-*.sh` |
| Tests et robustesse | `docs/tests.md` | tests du crate ; index `stokesforge-robustesse/src/lib.rs`, puis batterie `bN` concernée |
| Distribution | `crates/stokesforge-python/README.md`, `docs/application.md` | `scripts/app-dist.sh`, `scripts/cli-dist.sh`, `scripts/pyhello-dist.sh`, `vendor/python/lock.env` |

Les noms de crates désignent `crates/<nom>/`. Le manifeste racine inclut
`stokesforge-import`, même si certaines anciennes pages ne le recensent pas.
Le CLI `stokesforge-cli` permet aussi de vérifier le socle sans interface.

Les documents de `docs/archive/` expliquent les arbitrages historiques.
Les références anciennes à `architecture-choisie.md`, `format-fichier-plx.md`
ou `questions-ouvertes.md` s'y résolvent. Les notes `docs/chantier-*.md` et
`docs/chantiers/` demandent une confrontation au code avant de reprendre un
« reste à faire ». La présence éventuelle de `legacy-python/` ne change pas
la cible active Rust.

## 3. Réutiliser les frontières existantes

- Les clients soumettent les mutations métier via
  `SessionHandle::execute(Command)` ; l'acteur possède le modèle, les clients
  lisent des instantanés. Préserver undo/redo, macros, révisions et identifiants.
- `SessionHandle::new_project`, comme `load`, remplace l'identité du projet
  et vide l'historique et les géométries ; il refuse une macro ouverte.
  La confirmation avant perte de travail appartient à `AppState`.
- Dans l'application, réutiliser `Action` / `AppState::dispatch` pour les
  actions concernées. Garder la logique testable dans `state.rs` et la
  traduction des événements et l'affichage dans `app.rs`.
- L'import reste indépendant de l'UI, de `core`, des commandes et de la
  session. `stokesforge-importer` convertit ses résultats vers les colonnes du modèle
  pour l'application, le CLI et `pyst.import_file` ; les scripts écrivent via
  les commandes de leur session, GIL relâché, dans une macro annulable.
  Réutiliser recettes, parseur et aperçu avant de créer un second chemin.
- Les fonctionnalités de tracé partagées vont dans `stokesforge-render`, générique
  sur le backend. Les mesures de texte passent par `stokesforge_render::text`.
  Plotters sans ses polices exige les surcharges `draw_text` et
  `estimate_text_size` ; ses helpers de légende peuvent contourner ces mesures.
- Zoom/pan relèvent de `ViewState`, pas d'une mutation du `PlotSpec`.
  Le feathering reste hors de la clé de cache ; `view_generation` y reste.
- Préserver les tests de frontière (`no_ui_dependency.rs`, `frontiere.rs`).
  Une nouvelle dépendance doit respecter l'architecture avant de mettre à
  jour une liste blanche.
- Aucun objet Python ne traverse le canal de l'acteur ; le script relâche le
  GIL pendant l'attente. L'interpréteur démarre à la première exécution de
  script, sans bloquer le fil UI.
- Préserver les distinctions entre valeurs manquantes, texte vide, nombres
  et cellules mixtes. Pour les fichiers enregistrés, relire les conversions,
  les valeurs inconnues et les tests de compatibilité avant de changer le schéma.

## 4. Choisir les bons contrôles

Commandes à lancer depuis la racine. La chaîne Rust est déclarée `stable`,
avec `rustfmt` et `clippy`, dans `rust-toolchain.toml`.

Pour une modification Rust, commencer par les tests du crate et du comportement
touchés (`<crate>` et `<fichier>` sont à remplacer) :

```sh
cargo test -p <crate>
cargo test -p <crate> --test <fichier>
```

Avant de livrer une modification Rust, reprendre les contrôles de CI :

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`cargo build --workspace` construit les cibles par défaut. Éviter un
reformatage sans rapport avec la tâche. Pour un changement uniquement
documentaire, vérifier les chemins, les commandes et le diff suffit ; il
n'est pas nécessaire de reconstruire le workspace.

### Fonctionnalités optionnelles

La feature `python` n'est pas activée par défaut (c'est la seule hors
`default`). Un contrôle sans la feature ne valide pas le code qu'elle protège.

Typst n'est plus une feature (issue #29) : c'est une dépendance ferme des trois
crates de rendu. `cargo test --workspace` et `cargo clippy --workspace
--all-targets -- -D warnings` le compilent et le testent ; aucune commande
ciblée n'est nécessaire. `--all-features` n'active plus que Python et exige son
environnement.

Pour Python, examiner d'abord l'interpréteur dans
`vendor/python/extracted/python/` et lire les scripts. Si nécessaire,
`scripts/python-vendor.sh` télécharge les versions déclarées dans
`vendor/python/lock.env`. Puis `scripts/python-test.sh` configure PyO3, le
rpath et le préfixe embarqué, et teste `stokesforge-python`, `stokesforge-script`,
`stokesforge-robustesse` (en série), `stokesforge-app` et `stokesforge-cli` avec `python`.
Le travail CI `test-python` exécute ce script automatiquement et bloque en
cas d'échec. Les reproductions `#[ignore]` exigent encore `-- --ignored`.
Appliquer le même environnement aux vérifications Clippy avec
`--features python`. Ne pas utiliser le Python système comme substitut.
Le spike optionnel Matplotlib se rejoue par `scripts/python-matplotlib-spike.sh`
dans une copie distincte du runtime ; voir `docs/python-matplotlib-spike.md`.

### Contrôles spécifiques

- Rendu : vérifier mesure, accord écran/SVG et témoins qui doivent différer ;
  exercer les exports concernés et les features modifiées.
- UI : tester la logique via `AppState` ; pour le clavier, le focus ou la
  présentation, compléter par une vérification visuelle si un affichage est
  disponible. Sinon préciser ce qui reste non vérifié.
- Wayland (#72) : les patches `vendor/winit` et `vendor/smithay-clipboard`
  partagent le récepteur de dépôt et de presse-papiers entre les viewports.
  Les deux copies sont hors workspace. Le parseur et le canal exacts sont
  testés par `stokesforge-app/tests/wayland_{uri,bridge}.rs`. Compléter `fmt`
  par les commandes ciblées de `vendor/winit/STOKESFORGE.md` ; ne pas
  reformater les fichiers amont sans rapport avec le correctif.
  Wayland est le défaut ; `STOKESFORGE_X11=1` force le secours X11. Le protocole
  manuel, les alternatives et le retrait sont dans `vendor/winit/STOKESFORGE.md`.
- Robustesse : lire le motif de chaque `#[ignore]` avant de le lancer.
  `cargo test -p stokesforge-robustesse -- --ignored` rejoue ces cas ; certains
  peuvent être coûteux ou dépendre de l'ordonnancement. Ne pas retirer un
  `ignore` sans vérifier que le test devient un garde-fou fiable.
- Distribution : lire les scripts avant exécution ; ils reconstruisent leur
  sortie dans `dist/`. `app-dist.sh` exige un affichage X11/Xwayland pour sa
  vérification de fenêtre ; `cli-dist.sh` vérifie le CLI empaqueté sans écran,
  environnement vidé et piégé. Utiliser ces scripts dans le cadre d'un travail
  sur l'empaquetage, pas comme simple contrôle documentaire.

## 5. Laisser une base cohérente pour la session suivante

Relire `git diff --check`, le diff des fichiers modifiés et `git status --short`
(les fichiers nouveaux ne figurent pas dans un simple `git diff`). Ne pas
inclure de données personnelles, de projets de démonstration locaux ou
d'artefacts générés dans une modification sans lien avec eux.

Actualiser la page `docs/` concernée lorsque le comportement ou un contrat
change, et les commentaires de module lorsque le raisonnement change.
Actualiser ce guide si les outils ou chemins évoluent. Éviter d'y copier les
détails volatils déjà maintenus dans le code ou les manifestes.

Dans le compte rendu, distinguer les tests exécutés, les échecs rencontrés
et les vérifications non réalisées. Ne pas annoncer une validation Python
ou visuelle à partir des seuls tests Rust par défaut.

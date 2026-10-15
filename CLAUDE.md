# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Contexte

Branche `dev` : portage de StokesForge (analyse/tracé de données scientifiques, façon OriginLab) de Python/Qt vers Rust. Tout le code vit dans le workspace Cargo à la racine — l'ancienne implémentation Python/Qt (`legacy-python/`, qui servait de spec de comportement) a été supprimée de cette branche ; elle continue d'exister sous le tag `legacy-python`.

Documentation du code : `docs/` — [`docs/README.md`](docs/README.md) est le point d'entrée et route par tâche. `docs/invariants.md` liste les règles à ne pas casser ; les lire avant de toucher au rendu, à l'historique ou à l'acteur.

`docs/archive/` conserve les documents de conception antérieurs (cahier des charges technique, roadmaps 0-5 et R0-R8, rapports de robustesse, options explorées). Ils ne sont plus maintenus, mais **les commentaires du code les citent par leur nom de fichier** (`architecture-choisie.md` §4.3, `guide-de-correction-robustesse.md` §F6…) : toute citation de ce genre se résout dans `docs/archive/`. Là où ils divergent du code, c'est le code qui a raison. Ils datent d'avant le renommage (#87) : ils parlent encore de PlotYx, `plotyx-*`, `.plx` et `pyx`, qui se lisent aujourd'hui StokesForge, `stokesforge-*`, `.stf` et `pyst` (le fichier `format-fichier-plx.md` garde son nom).

Exception : les rapports de bugs `retours-2026-09-23-bugs.md` et `retours-2026-09-24-bugs.md` (B1…B9, « Bug 1 »…), cités dans `crates/stokesforge-app/src/app.rs` et `src/app/`, ont été supprimés — tous leurs bugs sont corrigés. Ils se lisent par `git show a6fd933^:docs/chantiers/<fichier>`. Les bugs se suivent désormais en issues GitLab (`glab issue list --label bug`), plus en `.md`.

## Commandes

```sh
cargo build --workspace
cargo test --workspace                       # inclut stokesforge-robustesse (sauf #[ignore])
cargo test -p <crate> <nom_test>              # un seul test
cargo test -p stokesforge-robustesse -- --ignored  # bancs coûteux
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Architecture (crates/, dépendances bas→haut)

`stokesforge-id` (clés stables) + `stokesforge-plot` (description de graphe neutre vis-à-vis du renderer) + `stokesforge-analysis` (calcul pur, #181 : lignes de base, correction, intégrale, ajustement, pics, déconvolution ; aucune dépendance du workspace, ni UI ni Python) → `stokesforge-core` (modèle Project/Folder/Workbook/Worksheet/Column, zéro dépendance UI) → `stokesforge-commands` (bus de commandes annulables) et `stokesforge-persistence` (format `.stf`), deux crates **parallèles** au-dessus de `core`/`plot` → `stokesforge-session` (acteur propriétaire du modèle pour ses clients, concurrence ; dépend de commands **et** persistence) → `stokesforge-script`/`stokesforge-python` (pont PyO3) → `stokesforge-app` (coquille UI sur eframe). `stokesforge-cli import` construit un `Project` neuf directement, sans session ; `stokesforge-cli run` (feature `python`) passe par `ScriptEngine` et une session headless, avec les mêmes commandes et groupes annulables que la console. `stokesforge-import` (fichiers d'appareil) ne dépend d'aucun crate du workspace ; `stokesforge-importer` en fait des colonnes du modèle (partagé par l'application, le CLI et les scripts `pyst.import_file`, sans commandes) ; `stokesforge-clipboard` (offre multi-format au presse-papiers système : SVG, PNG, PDF d'une figure ; Wayland par le patch `vendor/smithay-clipboard`, X11, Windows, macOS) ne dépend d'aucun crate du workspace ; `stokesforge-arch-guard` est l'outil de test des garde-fous de dépendances. Le graphe exact est dans `docs/architecture.md`.

Rendu (phases R0-R8 closes) : `stokesforge-render` (mesure de texte unique via `epaint`, pipeline Plotters générique sur le backend, et **tout le rendu** : types de série, échelles d'axe, primitives en pixels, rognage, légende, LTTB, interaction, formules Typst, graphes d'exemple) → `stokesforge-render-egui` (backend `DrawingBackend` collectant des `egui::Shape`, cache `(revision, rect, pixels_per_point, view_generation)`, zoom/pan, levier de *feathering*) et `stokesforge-render-svg` (export SVG, et par-dessus PDF et PNG). Détail : `docs/rendu.md`.

`stokesforge-robustesse` : pas de code de prod, dépend de tout le workspace — vingt-deux batteries de tests (`b1`…`b22`, voir son `src/lib.rs`) qui éprouvent le socle aux limites.

Feature hors `default` : `python` (pont PyO3, exige `PYO3_CONFIG_FILE` — voir `scripts/python-test.sh`), seule du dépôt ; `cargo build --workspace` ne la compile pas. `scripts/python-test.sh` teste pont, scripts, robustesse, application et CLI ; le travail CI `test-python` est automatique et bloquant. Typst (notations scientifiques) est une dépendance ferme des trois crates de rendu : `cargo build/test --workspace` le compilent et le testent.

Règles de chargement par domaine (quel crate/doc charger selon la tâche) : `.claude/rules.md`.

**Conventions par domaine : `.claude/conventions.md`**, à lire avant d'écrire du code dans le domaine — elles valent autant que celles de ce fichier :

| Avant de toucher… | Lire la section |
|---|---|
| `stokesforge-app` (fenêtre, champ, icône, couleur, texte, barre d'outils, onglets, arbre, défilement, Wayland) | « Application » |
| Object Manager, panneaux PM/OM/console, menus et clics droits | « Object Manager, panneaux fixes, menus » |
| création de graphe, fenêtres détachées | « Graphes neufs et fenêtres détachées » |
| séries, groupes de séries | « Séries et groupes » |
| copier/coller d'objets, copie de figure | « Copie d'objets et de figures » |
| analyse (`stokesforge-analysis`, calculs) | « Analyse » |
| `pyst` (écritures, navigation) | « Script `pyst` » |
| rendu (`stokesforge-render*`) | « Rendu : pièges silencieux » (+ skill `stokesforge-rendu`) |

Skills de projet : `.claude/skills/` — douze compétences par type de tâche
(implémenter une fonctionnalité, signaler puis corriger un bug, ajouter une
commande annulable, rendu, import, pont Python, vérifier avant de committer,
répondre sur l'architecture ou sur les fonctionnalités, écrire un texte
d'interface). Index et
enchaînements : [`.claude/skills/README.md`](.claude/skills/README.md).

## Conventions

- Licence (#129) : GPL-3.0-or-later avec `LICENSE-EXCEPTION`, limitée aux interfaces de la liste fermée `docs/licence-et-extensions.md` (`pyst` aujourd'hui). Une nouvelle interface couverte exige un commit explicite dans cette liste. Préserver les licences des composants tiers ; une dépendance ou un runtime modifié actualise les notices et passe `cargo deny check licenses`. Les distributions utilisent `scripts/licenses-dist.sh`. Les contributions extérieures portent le DCO de `CONTRIBUTING.md` ; il ne remplace pas un CLA pour un futur relicenciement.
- Notices Rust : `scripts/licenses-rust.sh` ajoute les copyrights amont manquants de `vendor/crate-license-notices/`. Leur empreinte lie le rapport au `Cargo.lock` exact ; toute modification du verrou exige de revalider ces compléments avant de générer les notices.
- `rustfmt` + `clippy -D warnings` obligatoires (CI `.gitlab-ci.yml`).
- Quand un travail installe une règle nouvelle (standard d'interface, motif à suivre, frontière), mettre à jour **au besoin** les skills concernés (`.claude/skills/`) et ce fichier ou `.claude/conventions.md`, dans le même commit que le code, pour garder le développement cohérent. Voir `.claude/skills/README.md` (règles transversales).
- Décisions à faire trancher : jusqu'à cinq, `AskUserQuestion` ; **six ou plus**, un formulaire de décisions publié en Artifact (une carte par décision : contexte, avant, après schématisés, avantage et défaut de chaque option ; l'utilisateur est dyslexique). Gabarit et marche à suivre : `.claude/skills/stokesforge-bug-report/references/formulaire-de-decisions.md`.
- Bugs et évolutions se suivent en issues GitLab (`glab issue list`), pas en `.md` ; chaque issue porte une ligne « **Cohérence** » qui dit quels skills et quelles docs mettre à jour.
- Pendant le signalement et la création ou l'enrichissement d'issues, les retours suivants complètent les issues, même formulés comme « ajoute » ou « supprime ». Ne passer au code que sur demande explicite d'implémentation dans le dépôt ; voir la règle de contexte de `stokesforge-bug-report`.
- Commits et docs techniques en français.
- Après fusion et push sur `dev`, nettoyer automatiquement le worktree temporaire de la tâche et sa branche locale fusionnée, sauf conservation demandée ; suivre `stokesforge-verification`, « Nettoyer après une fusion sur `dev` ».
- Toute mutation du modèle passe par une commande annulable de `stokesforge-commands`, jamais par une primitive `core` en direct.
- Un libellé d'historique est une **donnée** (`stokesforge_commands::HistoryLabel`, `MacroKind` fermé pour `begin_macro`), jamais une phrase : les crates du dessous ne mettent rien en phrase, `stokesforge-app` le fait par `history_text` et les clés `history-*` (#46).
- Erreurs : le `Display` (`#[error(...)]`) des crates du dessous est en **anglais brut**, jamais traduit (CLI, `pyst`, journaux). `stokesforge-app` les met en phrase par `errors::<famille>::error_message`, qui associe chaque variante à une clé `error-*` du catalogue `en` ; un `match` exhaustif, sans `_`, force à associer toute variante nouvelle. Jamais de `to_string()` ni de `{err}` d'une erreur affichée à l'utilisateur. Les `std::io::Error` passent par `errors::io::error_message` (clés `error-io-*`) ; `ErrorKind` étant `#[non_exhaustive]`, son `match` garde un repli `_` (`error-io-other`), seule exception à « sans `_` », commentée dans le code. Les messages de la barre d'état (`AppState::notice`/`fail`) sont des `tr!` (clés `status-*` / `error-*`), gardés par `aucun_libelle_en_dur`.

## Pièges connus

- Avant undo/historique ou format `.stf` : `docs/commandes-et-historique.md`, `docs/persistance.md`, puis `docs/archive/rustrewrite/format-fichier-plx.md` pour les options déjà écartées.
- `docs/archive/rustrewrite/questions-ouvertes.md` §11 : plafonds/compteurs documentés — vérifier avant de "corriger" un comportement qui ressemble à un bug.
- Recalcul des formules (#193) : ciblé par empreinte (`stokesforge-session/src/recalc.rs`, invariant 11). **Toute donnée lue par un calcul de formule entre dans l'empreinte** (la version d'une colonne ne couvre que type et valeurs, ni nom, ni méta, ni position), et **toute écriture de données d'une colonne tire une version neuve** par `Project`. Le recalcul complet reste comme oracle (`FormulaRecalc::Full`, batterie `b22`). Étendre le mécanisme (fonction de formule, #184, #186) : tests à écrire dans `docs/tests.md`, « Recalcul ciblé ».

## Délégation et choix du modèle

Objectif : garder le contexte de la session principale petit, et réserver Opus au jugement, avec une sortie courte.

- **Implémentation (fonctionnalité, correctif, restructuration)** : Opus fixe seulement le périmètre, par l'agent `perimetre` (`.claude/agents/perimetre.md` : fichiers à toucher, cas de test et témoins, pièges, 40 lignes au plus, sans code). **Sonnet** implémente et écrit les tests d'après ce périmètre, puis vérifie chaque témoin par mutation. Pas de relecture Opus en fin de tâche, sauf demande.
- Une recherche qui demande de lire **beaucoup** (plus d'une dizaine de fichiers, ou sans savoir où chercher) part à l'agent `recherche` (`.claude/agents/recherche.md`, Haiku). Lire soi-même un fichier connu ou faire une recherche ciblée (un `rg`, deux fichiers) : le démarrage à froid d'un agent coûte plus cher.
- Sa réponse cite `fichier:ligne` ; revérifier soi-même toute conclusion **d'absence** (« n'existe pas », « personne n'appelle ») et tout extrait sur lequel on s'appuie pour écrire du code.
- Jamais délégués à Haiku : conception, modèle de données, commandes, invariants, tests (périmètre par `perimetre`, écriture par Sonnet), textes d'interface, choix à faire trancher.
- Sortie longue d'une commande (`cargo test`, `clippy`, `cargo build`) : pas d'agent — rediriger le journal complet vers un fichier (`> "$tmp/test.log" 2>&1`) et n'afficher que le résumé (`rg -n 'FAILED|panicked|^error|^warning' …`, `tail`) ; relire le fichier au besoin plutôt que relancer.

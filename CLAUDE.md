# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Contexte

Branche `rewriterust` : portage de PlotYx (analyse/tracé de données scientifiques, façon OriginLab) de Python/Qt vers Rust. Tout le code vit dans le workspace Cargo à la racine — l'ancienne implémentation Python/Qt (`legacy-python/`, qui servait de spec de comportement) a été supprimée de cette branche ; elle continue d'exister sur `main`.

Documentation du code : `docs/` — [`docs/README.md`](docs/README.md) est le point d'entrée et route par tâche. `docs/invariants.md` liste les règles à ne pas casser ; les lire avant de toucher au rendu, à l'historique ou à l'acteur.

`docs/archive/` conserve les documents de conception antérieurs (cahier des charges technique, roadmaps 0-5 et R0-R8, rapports de robustesse, options explorées). Ils ne sont plus maintenus, mais **les commentaires du code les citent par leur nom de fichier** (`architecture-choisie.md` §4.3, `guide-de-correction-robustesse.md` §F6…) : toute citation de ce genre se résout dans `docs/archive/`. Là où ils divergent du code, c'est le code qui a raison.

Exception : les rapports de bugs `retours-2026-09-23-bugs.md` et `retours-2026-09-24-bugs.md` (B1…B9, « Bug 1 »…), cités dans `crates/plotyx-app/src/app.rs`, ont été supprimés — tous leurs bugs sont corrigés. Ils se lisent par `git show a6fd933^:docs/chantiers/<fichier>`. Les bugs se suivent désormais en issues GitLab (`glab issue list --label bug`), plus en `.md`.

## Commandes

```sh
cargo build --workspace
cargo test --workspace                       # inclut plotyx-robustesse (sauf #[ignore])
cargo test -p <crate> <nom_test>              # un seul test
cargo test -p plotyx-robustesse -- --ignored  # bancs coûteux
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Architecture (crates/, dépendances bas→haut)

`plotyx-id` (clés stables) + `plotyx-plot` (description de graphe neutre vis-à-vis du renderer) → `plotyx-core` (modèle Project/Folder/Workbook/Worksheet/Column, zéro dépendance UI) → `plotyx-commands` (bus de commandes annulables) → `plotyx-persistence` (format `.plx`) → `plotyx-session` (acteur propriétaire du modèle, concurrence) → `plotyx-cli` (binaire headless) et `plotyx-script`/`plotyx-python` (pont PyO3) → `plotyx-app` (coquille UI sur eframe).

Rendu (phases R0-R8 closes) : `plotyx-render` (mesure de texte unique via `epaint`, pipeline Plotters générique sur le backend, et **tout le rendu** : types de série, échelles d'axe, primitives en pixels, rognage, légende, LTTB, interaction, formules Typst, graphes d'exemple) → `plotyx-render-egui` (backend `DrawingBackend` collectant des `egui::Shape`, cache `(revision, rect, pixels_per_point, view_generation)`, zoom/pan, levier de *feathering*) et `plotyx-render-svg` (export SVG, et par-dessus PDF et PNG). Détail : `docs/rendu.md`.

`plotyx-robustesse` : pas de code de prod, dépend de tout le workspace — vingt-et-une batteries de tests (`b1`…`b21`, voir son `src/lib.rs`) qui éprouvent le socle aux limites.

Features hors `default` : `python` (pont PyO3, exige `PYO3_CONFIG_FILE` — voir `scripts/python-test.sh`) et `typst` (notations scientifiques). `cargo build --workspace` ne compile ni l'une ni l'autre.

Règles de chargement par domaine (quel crate/doc charger selon la tâche) : `.claude/rules.md`.

Skills de projet : `.claude/skills/` — dix compétences par type de tâche
(implémenter une fonctionnalité, signaler puis corriger un bug, ajouter une
commande annulable, rendu, import, pont Python, vérifier avant de committer,
répondre sur l'architecture ou sur les fonctionnalités). Index et
enchaînements : [`.claude/skills/README.md`](.claude/skills/README.md).

## Conventions

- `rustfmt` + `clippy -D warnings` obligatoires (CI `.gitlab-ci.yml`).
- Commits et docs techniques en français.
- Toute mutation du modèle passe par une commande annulable de `plotyx-commands`, jamais par une primitive `core` en direct.
- `plotyx-app` : l'aspect des barres de défilement (largeur, couleur) vient **uniquement** de `theme::scroll_style`, posé globalement par `theme::install` ; jamais de `ScrollStyle` local (verrouillé par le test `aucun_scroll_style_hors_du_theme`). La place d'une barre se lit par `theme::scroll_bar_allocated_width`.

## Pièges connus

- Avant undo/historique ou format `.plx` : `docs/commandes-et-historique.md`, `docs/persistance.md`, puis `docs/archive/rustrewrite/format-fichier-plx.md` pour les options déjà écartées.
- `docs/archive/rustrewrite/questions-ouvertes.md` §11 : plafonds/compteurs documentés — vérifier avant de "corriger" un comportement qui ressemble à un bug.
- Rendu : `plotters` est pris **sans ses fonts** (`default-features = false`). Un `DrawingBackend` qui ne surcharge pas `draw_text` **panique** à la première graduation, et un qui ne surcharge pas `estimate_text_size` mesure le texte au nombre d'octets UTF-8 — silencieusement. Toute mesure passe par `plotyx_render::text` et par nulle part ailleurs (`architecture-choisie.md` §4.3/§4.4).
- Rendu : **une fonctionnalité de tracé n'entre jamais dans un seul backend** — elle va dans `plotyx-render`, qui est générique sur le backend. Corollaire du même piège : les éléments de Plotters qui mesurent du texte eux-mêmes (`configure_series_labels`, `MultiLineText`) passent par `FontData::estimate_layout`, donc par l'heuristique naïve, **pas** par `estimate_text_size` — la légende est écrite à la main pour cette raison (`plotyx_render::legend`).
- Rendu : le levier `TessellationOptions::feathering` vit dans `plotyx_render_egui::feathering`, en fonction libre sur le `Context`. Il n'est **pas** dans la clé de cache et ne doit pas y entrer (`architecture-choisie.md` §4.2, verrouillé par `plotyx-render-egui/tests/feathering.rs`). Son déclencheur, le glisser, est câblé dans `plotyx-render-egui/src/lib.rs` depuis R7.3.

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Contexte

Branche `rewriterust` : portage de PlotYx (analyse/tracé de données scientifiques, façon OriginLab) de Python/Qt vers Rust. Tout le code vit dans le workspace Cargo à la racine — l'ancienne implémentation Python/Qt (`legacy-python/`, qui servait de spec de comportement) a été supprimée de cette branche ; elle continue d'exister sur `main`.

Architecture qui fait foi : `docs/rustrewrite/cahier-des-charges-technique.md`. Décisions déjà tranchées : `resume-decisions.md`. Ne pas rouvrir ces choix sans relire.

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

`plotyx-id` (clés stables) + `plotyx-plot` (description de graphe neutre vis-à-vis du renderer) → `plotyx-core` (modèle Project/Folder/Workbook/Worksheet/Column, zéro dépendance UI) → `plotyx-commands` (bus de commandes annulables) → `plotyx-persistence` (format `.plx`) → `plotyx-session` (acteur propriétaire du modèle, concurrence) → `plotyx-cli` (binaire headless) et `plotyx-script`/`plotyx-python` (pont PyO3, API `plotyx.api`) → `plotyx-app` (coquille UI sur eframe 0.36.2 — le toolkit est tranché depuis `docs/newarchitecture/architecture-choisie.md` §3).

Chantier de rendu (`docs/newarchitecture/`, phases R0-R5 closes) : `plotyx-render` (mesure de texte unique via `epaint`, pipeline Plotters générique sur le backend, et **tout le rendu** : types de série, échelles d'axe, primitives en pixels, légende, LTTB, graphes d'exemple) → `plotyx-render-egui` (backend `DrawingBackend` collectant des `egui::Shape`, cache `(revision, rect, pixels_per_point)`, levier de *feathering*) et `plotyx-render-svg` (export SVG). L'architecture qui fait foi pour eux est `docs/newarchitecture/architecture-choisie.md`, le calendrier `roadmap-rendu.md`.

`plotyx-robustesse` : pas de code de prod, dépend de tout le workspace — vingt batteries de tests (`b1`…`b20`, voir son `src/lib.rs`) qui éprouvent le socle aux limites.

Règles de chargement par domaine (quel crate/doc charger selon la tâche) : `.claude/rules.md`.

## Conventions

- `rustfmt` + `clippy -D warnings` obligatoires (CI `.gitlab-ci.yml`).
- Commits et docs techniques en français.
- Toute mutation du modèle passe par une commande annulable de `plotyx-commands`, jamais par une primitive `core` en direct.

## Pièges connus

- Avant undo/historique ou format `.plx` : relire `cahier-des-charges-technique.md` et `format-fichier-plx.md`, options déjà écartées.
- `docs/rustrewrite/questions-ouvertes.md` : plafonds/compteurs documentés — vérifier avant de "corriger" un comportement qui ressemble à un bug.
- Rendu : `plotters` est pris **sans ses fonts** (`default-features = false`). Un `DrawingBackend` qui ne surcharge pas `draw_text` **panique** à la première graduation, et un qui ne surcharge pas `estimate_text_size` mesure le texte au nombre d'octets UTF-8 — silencieusement. Toute mesure passe par `plotyx_render::text` et par nulle part ailleurs (`architecture-choisie.md` §4.3/§4.4).
- Rendu : **une fonctionnalité de tracé n'entre jamais dans un seul backend** — elle va dans `plotyx-render`, qui est générique sur le backend. Corollaire du même piège : les éléments de Plotters qui mesurent du texte eux-mêmes (`configure_series_labels`, `MultiLineText`) passent par `FontData::estimate_layout`, donc par l'heuristique naïve, **pas** par `estimate_text_size` — la légende est écrite à la main pour cette raison (`plotyx_render::legend`).
- Rendu : le levier `TessellationOptions::feathering` vit dans `plotyx_render_egui::feathering`, en fonction libre sur le `Context`. Il n'est **pas** dans la clé de cache et ne doit pas y entrer (`architecture-choisie.md` §4.2, verrouillé par `plotyx-render-egui/tests/feathering.rs`). Son déclencheur (le glisser) est le travail de R7.

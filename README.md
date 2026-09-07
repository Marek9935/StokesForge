# PlotYx — branche `rewriterust`

Cette branche est dédiée au portage de PlotYx vers Rust. PyQt/PySide n'est
plus à l'ordre du jour ici — le développement actif se fait dans le
workspace Cargo à la racine.

## Layout

- **`crates/`** — workspace Cargo :
  - `plotyx-core` : modèle de données (arbre de projet, colonnes), zéro
    dépendance UI.
  - `plotyx-commands` : bus de commandes annulables (undo/redo) sur
    `plotyx-core`.
  - `plotyx-cli` : binaire headless de vérification, pas l'UI (le choix
    d'UI n'est pas encore fait, voir plus bas).
- **`docs/rustrewrite/`** — le plan et les décisions techniques :
  - [`cahier-des-charges-technique.md`](docs/rustrewrite/cahier-des-charges-technique.md) :
    **le document qui fait foi sur l'architecture** — undo hybride, modèle
    possédé par un acteur, rendu Matplotlib via PyO3, découpage en crates,
    avec les options écartées et leurs raisons.
  - [`roadmap.md`](docs/rustrewrite/roadmap.md) : phases 0-4 (fondations,
    modèle, bus de commandes/undo, persistance, coquille UI minimale) et le
    répertoire des éléments déjà discutés pour l'UI réelle (à retravailler
    entièrement en conversation le moment venu, pas phasée à l'avance).
  - [`resume-decisions.md`](docs/rustrewrite/resume-decisions.md) : le
    raisonnement complet du choix de portage (egui vs iced, PyO3 pour
    garder le renderer Matplotlib, etc.).
  - [`format-fichier-plx.md`](docs/rustrewrite/format-fichier-plx.md) :
    réévaluation du format `.plx` (Zarr plutôt que HDF5 pour un cœur Rust).
  - [`cahier-des-charges-api-script.md`](docs/rustrewrite/cahier-des-charges-api-script.md) :
    contrat de l'API de script `plotyx.api`.
- **`legacy-python/`** — l'implémentation Python/Qt précédente (phases 0-6
  du roadmap Python), conservée **telle quelle, non maintenue sur cette
  branche**. Elle sert de spécification de comportement pour le portage
  (664 tests qui encodent ce que le modèle et le bus de commandes doivent
  faire) — voir `legacy-python/README.md` et `legacy-python/ROADMAP.md`
  pour son état au moment du gel. Le développement Python continue sur
  `main`, pas ici.

## Démarrer

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

`rust-toolchain.toml` épingle la chaîne stable + `rustfmt`/`clippy`.

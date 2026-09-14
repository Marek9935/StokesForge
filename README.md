# PlotYx — branche `rewriterust`

Cette branche est dédiée au portage de PlotYx vers Rust. PyQt/PySide n'est
plus à l'ordre du jour ici — le développement actif se fait dans le
workspace Cargo à la racine.

## Layout

- **`crates/`** — le workspace Cargo : quatorze crates, du modèle de données
  à la fenêtre. Le graphe de dépendances et le rôle de chacun sont dans
  [`docs/architecture.md`](docs/architecture.md).
- **`docs/`** — la documentation du code, [`docs/README.md`](docs/README.md)
  en point d'entrée. `docs/archive/` conserve les documents de conception
  antérieurs (cahier des charges, roadmaps, rapports) : historique, pas
  référence.
- **`scripts/`** — vendorisation de CPython, configuration PyO3, assemblage
  des distributions de `dist/`.
- **`vendor/`** — l'interpréteur CPython figé (non versionné) et les polices
  embarquées.

L'ancienne implémentation Python/Qt a été supprimée de cette branche ; elle
continue d'exister sur `main`.

## Démarrer

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

`rust-toolchain.toml` épingle la chaîne stable + `rustfmt`/`clippy`.

Les features `python` (pont PyO3) et `typst` (notations scientifiques) sont
absentes de `default` : les commandes ci-dessus ne les compilent pas. Voir
[`docs/architecture.md`](docs/architecture.md#les-features).

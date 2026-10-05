# StokesForge — branche `rewriterust`

Cette branche est dédiée au portage de StokesForge vers Rust. PyQt/PySide n'est
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

La feature `python` (pont PyO3) est absente de `default` : les commandes
ci-dessus ne la compilent pas. Typst (notations scientifiques) est une
dépendance ferme, toujours compilée. Voir
[`docs/architecture.md`](docs/architecture.md#les-features).

## Licence et contributions

Copyright © 2026 Aliocha Skrzypczak. Cette branche est sous
**[GPL-3.0-or-later](LICENSE) avec [exception d'extension](LICENSE-EXCEPTION)**.
Les scripts utilisant uniquement l'API publique `pyst`, sans incorporer ni
lier le code de StokesForge dans leur propre programme, peuvent être publiés
sous la licence choisie par leur auteur. La liste fermée des interfaces
couvertes et les notices tierces sont décrites dans
[`docs/licence-et-extensions.md`](docs/licence-et-extensions.md).

Les distributions livrent `LICENSE`, `LICENSE-EXCEPTION`,
`THIRD-PARTY-LICENSES.txt` et un dossier `licenses/` (polices, icônes, patches,
CPython et numpy). Les outils de contrôle et de génération des notices
doivent être installés avant d'exécuter les scripts de distribution ; voir
la page de licence. Le travail CI `licenses` vérifie la politique et génère
les notices.

Les contributions extérieures suivent [`CONTRIBUTING.md`](CONTRIBUTING.md)
et le [DCO 1.1](DCO). Ce changement de licence concerne `rewriterust` ;
l'ancienne implémentation Python/Qt sur `main` conserve son statut propre.

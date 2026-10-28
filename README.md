# StokesForge — branche `main`

Le dépôt de référence est sur GitLab (`gitlab.com/askrzypczak/stokesforge`) :
issues, releases et téléchargements s'y trouvent. Le dépôt GitHub est un miroir
en lecture seule, qui sert à la CI Linux.

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
continue d'exister sous le tag `legacy-python`.

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
[`docs/architecture.md`](docs/architecture.md#features).

## Licence et contributions

Copyright © 2026 Aliocha Skrzypczak. Cette branche est sous
**[GPL-3.0-or-later](LICENSE)**, sans permission supplémentaire.

**Usage non couvert par la GPL** (par exemple un connecteur propriétaire pour
un appareil) : une permission supplémentaire ou une licence séparée peut être
demandée à l'auteur, qui l'étudie au cas par cas et l'accorde, le cas échéant,
par écrit. Aucune n'est accordée par défaut. Les notices tierces sont décrites
dans [`docs/licence-et-extensions.md`](docs/licence-et-extensions.md).

Les distributions livrent `LICENSE`,
`THIRD-PARTY-LICENSES.txt` et un dossier `licenses/` (polices, icônes, patches,
CPython, numpy et scipy). Les outils de contrôle et de génération des notices
doivent être installés avant d'exécuter les scripts de distribution ; voir
la page de licence. Le travail CI `licenses` vérifie la politique et génère
les notices.

Les contributions extérieures suivent [`CONTRIBUTING.md`](CONTRIBUTING.md)
et le [DCO 1.1](DCO) en attendant un CLA (#348). Cette licence concerne `main` ;
l'ancienne implémentation Python/Qt (tag `legacy-python`) conserve son statut propre.
La version 1.0.0 portait une exception d'extension pour `pyst` ; elle n'a pas
été reprise après 1.0.0.

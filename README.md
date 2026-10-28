# StokesForge — `main` branch

The reference repository is on GitLab (`gitlab.com/askrzypczak/stokesforge`):
issues, releases and downloads live there. The GitHub repository is a
read-only mirror, used for the Linux CI.

This branch is dedicated to the port of StokesForge to Rust. PyQt/PySide is
no longer on the agenda here — active development happens in the Cargo
workspace at the root.

## Layout

- **`crates/`** — the Cargo workspace: fourteen crates, from the data model
  to the window. The dependency graph and the role of each crate are in
  [`docs/architecture.md`](docs/architecture.md).
- **`docs/`** — the code documentation, with [`docs/README.md`](docs/README.md)
  as the entry point. `docs/archive/` keeps the earlier design documents
  (specifications, roadmaps, reports): history, not reference.
- **`scripts/`** — vendoring of CPython, PyO3 configuration, assembly of the
  distributions in `dist/`.
- **`vendor/`** — the frozen CPython interpreter (not versioned) and the
  embedded fonts.

The old Python/Qt implementation was removed from this branch; it still exists
under the `legacy-python` tag.

## Getting started

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

`rust-toolchain.toml` pins the stable toolchain + `rustfmt`/`clippy`.

The `python` feature (PyO3 bridge) is absent from `default`: the commands
above do not compile it. Typst (scientific notation) is a hard dependency,
always compiled. See
[`docs/architecture.md`](docs/architecture.md#features).

## License and contributions

Copyright © 2026 Aliocha Skrzypczak. This branch is under
**[GPL-3.0-or-later](LICENSE)**, with no additional permission.

**Use not covered by the GPL** (for example a proprietary connector for an
instrument): an additional permission or a separate license can be requested
from the author, who considers it case by case and grants it, if at all, in
writing. None is granted by default. The third-party notices are described in
[`docs/license-and-extensions.md`](docs/license-and-extensions.md).

The distributions ship `LICENSE`,
`THIRD-PARTY-LICENSES.txt` and a `licenses/` folder (fonts, icons, patches,
CPython, numpy and scipy). The tools that check and generate the notices
must be installed before running the distribution scripts; see
the license page. The `licenses` CI job checks the policy and generates
the notices.

External contributions follow [`CONTRIBUTING.md`](CONTRIBUTING.md)
and the [DCO 1.1](DCO) until a CLA exists (#348). This license applies to `main`;
the old Python/Qt implementation (`legacy-python` tag) keeps its own status.
Version 1.0.0 carried an extension exception for `pyst`; it was not
kept after 1.0.0.

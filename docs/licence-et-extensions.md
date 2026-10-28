# License and extension interfaces

Copyright © 2026 Aliocha Skrzypczak. The original StokesForge code of the
`main` branch is distributed under the **GNU GPL version 3 or later**, with no
additional permission (#348). The official text of the GPLv3 is reproduced
without modification in [`LICENSE`](../LICENSE). Third-party components keep
their licenses. This decision does not change the status of the `legacy-python`
tag, the old Python/Qt version, nor that of the versions before this change.

The GPL allows commercial distributions; the distribution of the program or of
a modified version must respect its conditions, notably the supply of the
corresponding source code.

## Additional permission: withdrawn, can be requested

Version 1.0.0 carried an extension exception for `pyst` (#129, section 7 of
the GPLv3). It is withdrawn (#348): no request justified it, and it has
drafting defects (reference to a document that is not delivered, a moving
list). A recipient of 1.0.0 keeps the rights of that version; the following
versions are under pure GPL.

A use that the GPL does not cover, for example a proprietary connector from an
instrument maker, is handled by an additional permission or a separate license,
granted in writing on request. It assumes that the author holds or can grant
the rights on all the code concerned, so a **license CLA** (the contributor
keeps their copyright and allows redistribution under other conditions) before
any external contribution is merged. Its drafting and that of the permission go
through a lawyer.

## Cargo metadata

The workspace declares `GPL-3.0-or-later`; the twenty crates inherit
`license`. The text is `LICENSE`.

The Apache-2.0 licenses of `vendor/winit` and MIT of
`vendor/smithay-clipboard` remain intact, including for the local modifications
described in [`vendor/winit/STOKESFORGE.md`](../vendor/winit/STOKESFORGE.md).

## Third-party licenses and distribution

The policy [`deny.toml`](../deny.toml) checks all platforms and Cargo
features, including `python` and the test dependencies, without compiling the
interpreter. The accepted licenses
are explicit. MPL-2.0 (`option-ext` 0.2.0, and `colored` 2.2.0, pulled in only by the
`egui_kittest` → `dify` interface tests and never shipped) and Ubuntu-font-1.0
(`epaint_default_fonts` 0.36.2) are allowed only for these versions.
The six font families of `vendor/fonts` each carry their OFL-1.1;
the Noto Sans pair also includes its italic face. Phosphor carries its MIT notice.
The fonts embedded by egui also have supplementary notices
in `vendor/epaint-default-fonts/`, notably Bitstream Vera for Hack,
absent from the Cargo metadata. Their provenance is verified separately;
the check of the manifests does not replace that of the resources.
The manifest of `brotli-decompressor` 5.0.3 announces MIT, but its `LICENSE`
carries BSD-3-Clause: both tools clarify it with the fingerprint of this text.

Installation of the pinned tools, from the root:

```sh
source scripts/licenses-tools.env
cargo install cargo-deny --locked --version "$CARGO_DENY_VERSION"
cargo install cargo-about --locked --features cli --version "$CARGO_ABOUT_VERSION"
cargo deny --locked check licenses
```

`cargo-about` produces the texts from `Cargo.lock` and
[`about.toml`](../about.toml), with the template
[`scripts/licenses.hbs`](../scripts/licenses.hbs). The CI job `licenses`
is automatic and blocking: it runs the check then generates the notices,
kept as an artifact. It also exercises the refusal witnesses of
`scripts/licenses-test.sh` (missing/stale notices, different Cargo lock,
forbidden license).
It does not replace the Python tests.

[`scripts/licenses-rust.sh`](../scripts/licenses-rust.sh) produces the same
report in CI and in the distributions. It adds the notices/copyrights
missing from the crates.io packages, kept in
[`vendor/crate-license-notices/`](../vendor/crate-license-notices/README.md)
with their exact references and fingerprints. It refuses a different Cargo lock
as long as these supplements have not been reread and updated.

The three scripts `app-dist.sh`, `cli-dist.sh` and `pyhello-dist.sh` call
[`scripts/licenses-dist.sh`](../scripts/licenses-dist.sh) after assembly.
The tools must be installed before running them. The generation may
consult the upstream sources to retrieve a text missing from the Cargo package.
Each distribution contains:

- `LICENSE` for StokesForge;
- `THIRD-PARTY-LICENSES.txt`, texts of the transitive Rust dependencies,
  notably Typst (Apache-2.0), with their names and versions;
- `licenses/fonts/<famille>/OFL.txt` and `licenses/phosphor/LICENSE`;
- `licenses/epaint-default-fonts/`, copyrights and permissions of the egui
  fonts, including MIT and Bitstream Vera for Hack;
- `licenses/winit/` and `licenses/smithay-clipboard/`, notices of the patches;
- `licenses/python/`, PSF texts and history of CPython, notices of its
  native libraries, provenance and text of the runtime actually shipped;
- `licenses/numpy/`, all the texts shipped by the numpy wheel, including
  those of its native components. The `.dist-info/licenses` also remain
  in the runtime;
- `licenses/scipy/LICENSE.txt`, the text of the scipy wheel, which includes the
  licenses of its native components (OpenBLAS, libgfortran, libquadmath…).

The native notices of Python come from the complete archive matching the
lock, are versioned and their update is described in
[`vendor/python/licenses-linux/README.md`](../vendor/python/licenses-linux/README.md)
(Windows, #332: [`licenses-windows/`](../vendor/python/licenses-windows/README.md),
notices derived from those of Linux until the complete Windows archive is
cross-checked, which its README states).
The packaging refuses a set of notices that no longer matches the lock, or
a runtime without the texts of CPython, numpy and scipy. A delivery must also
provide the corresponding sources according to the applicable licenses; the
notices file is not a source archive.

## Contributions

See [`CONTRIBUTING.md`](../CONTRIBUTING.md): every external contribution
must be proposed under GPL-3.0-or-later and carry the `Signed-off-by` of the
[DCO 1.1](../DCO). Authors keep their rights. The DCO does not give the
right to grant an additional permission or to relicense: that requires
a license CLA, to be put in place **before** the first merged external
contribution (#348).

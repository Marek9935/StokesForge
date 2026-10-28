# Supplementary notices for Rust dependencies

`cargo-about` sometimes renders a generic text when a crates.io package does
not ship its license file, or ships only a pointer to that text. The
copyrights and permissions present in the upstream sources must also be kept.
`NOTICE.txt` complements the generated report, without replacing its license
texts. `scripts/licenses-rust.sh` concatenates the two in CI and in the
distributions.

`provenance.json` links the texts to the packages/versions, the commits
declared in their `.cargo_vcs_info.json`, the original URL and the SHA-256 of
the texts. Identical texts are grouped, with all their references and all
the packages. License files are kept in full; the License section of
Glidesort's README is an identified excerpt. For `dispatch` 0.2.0, the
snapshot contains no license file: its MIT declaration and its authors come
from the manifest, carried into the generated report.

Points noted during verification:

- `brotli-decompressor` 5.0.3 declares MIT in its manifest, but ships a
  BSD-3-Clause text from Dropbox: `deny.toml` and `about.toml` clarify it
  with the hash of its `LICENSE` file, kept unmodified;
- `dpi` ships the permissions of its `libm` code in `LICENSE-LIBM-MIT`;
- `r-efi` ships them in `AUTHORS`;
- `siphasher` and `zune-image` ship copyrights with a pointer to MIT/Apache;
  the generated report also provides their full terms;
- objc2's `LICENSE.md` files describe the conditions and limits flagged by
  that upstream for Apple SDKs; they are kept as they are.

The `repository` field of `zune-core` 0.4.12 is missing; its zune-image
repository is identified through the sibling crate `zune-jpeg`, then the text
is read at the exact commit declared by `zune-core`. No text is taken from a
moving branch.

## Updating

`checksums.sha256` ties these supplements to the **exact Cargo.lock** and to
their provenance files. A change to the lock blocks their generation until
the supplements have been reviewed. From the root:

1. Produce the detailed report with the pinned version of cargo-about:
   `cargo about generate --workspace --all-features --locked --fail --format json -o target/licenses/report.json`.
2. Examine the licenses whose `source_path` is missing. Keep the missing
   upstream texts and copyrights, at the reference published by each package;
   licenses/copyrights can also be found in `COPYING`, `AUTHORS`, a README, or
   binary resources. Compare any discrepancy with the manifest, and specify
   the exceptions in the tool configurations.
3. Refresh `NOTICE.txt` and `provenance.json`, then their fingerprints:
   `sha256sum Cargo.lock vendor/crate-license-notices/NOTICE.txt vendor/crate-license-notices/provenance.json > vendor/crate-license-notices/checksums.sha256`.
4. Replay the check, `scripts/licenses-test.sh` and
   `scripts/licenses-rust.sh target/licenses/THIRD-PARTY-LICENSES.txt`.

egui's fonts have their dedicated notice in
[`../epaint-default-fonts/`](../epaint-default-fonts/README.md). The
supplements are not an archive of the corresponding source code.

# Checks a change adds to the base ones

Read at step 3 of `stokesforge-verification`. CI: Linux, Windows and the publication all run on GitHub (`.github/workflows/ci.yml`); GitLab keeps only disabled Linux jobs, and statuses come back to GitLab through `gitlab-status.yml`. A missing report means reading the « Résultats GitLab » workflow and its secret, never concluding the tests are green. Details and job table: `docs/tests.md` § CI (GitHub Actions pour Linux et Windows).

| The change touches… | Also run |
|---|---|
| `stokesforge-python`, `stokesforge-script`, the `pyst` surface | `scripts/python-test.sh` (sets `PYO3_CONFIG_FILE`); the `fmt`, `clippy` and `test` jobs do not enable `python` |
| Python integration of `stokesforge-app-state` / `stokesforge-app`, or CLI `run` | `scripts/python-test.sh` covers these crates; apply its environment to Clippy; `scripts/cli-dist.sh` if CLI packaging changes |
| Typst formulas, `stokesforge-render::formula` | nothing more: `cargo test --workspace` covers them (firm dependency), exports included |
| an `#[ignore]` robustness reproduction | `cargo test -p stokesforge-robustesse -- --ignored` |
| formula recalculation (`stokesforge-session/src/recalc.rs`), data read by a formula, a column write | `b22` and `recalc_cible.rs` run in the workspace, but a green oracle proves nothing if the draw never produces the gesture: `docs/tests.md` § Recalcul ciblé (#193) — et ce qu'il faut tester en s'appuyant dessus |
| a dependency, a resource, the distributed runtime | `cargo deny --locked check licenses` and notice generation (`docs/licence-et-extensions.md` § Licences tierces et distribution); a changed `Cargo.lock` revalidates `vendor/crate-license-notices/` (`CLAUDE.md` § Conventions) |
| a version number, `CHANGELOG.md`, a release or distribution script, the tag jobs of the CI | `scripts/release-notes-test.sh`, `scripts/versions-alignees.sh` (all crates in one version; also run by `fmt` on every push) and `scripts/release-notes.sh v<version>` (versions of the crates against the tag and the changelog section), `cargo update --workspace` for `Cargo.lock`, `docs/criteres-v1.md`; the tag jobs themselves only run on a real tag |
| the **appearance** of the interface | a screenshot (`run-stokesforge`); an interaction scenario (menu, dialog, typing, undo) is replayed with its driver; a new text is captured again with `STOKESFORGE_LANG=en-XA` (a text without `[[ ]]` escaped the catalogue, a missing `]]` is truncated) |
| a reference screenshot (`tests/snapshots/*.png`) failing on Windows/macOS by a few pixels | tolerance per system in `kittest.toml`; never regenerate a reference outside Linux; delete the `*.new.png` / `*.diff.png` left behind (`docs/tests.md` § Ce qu'il faut savoir avant d'écrire un test) |
| a real window (native Wayland, `rfd`, multi-screen, drag, detached OS window) | nothing automatic: `run-stokesforge` runs X11 under Xwayland. Say so in the report, or ask the user for a screenshot |
| interface texts | keys in `en` **and** `fr` in the same commit; `aucun_libelle_en_dur` and `aucune_cle_du_catalogue_n_est_morte` green (`stokesforge-textes-interface`) |

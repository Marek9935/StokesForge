# Checks a change adds to the base ones

Read at step 3 of `stokesforge-verification`. CI: Linux, Windows and the publication all run on GitHub (`.github/workflows/ci.yml`); GitLab keeps only disabled Linux jobs, and statuses come back to GitLab through `gitlab-status.yml`. A missing report means reading the "GitLab results" workflow and its secret, never concluding the tests are green. Details and job table: `docs/tests.md` § CI (GitHub Actions for Linux and Windows).

| The change touches… | Also run |
|---|---|
| `stokesforge-python`, `stokesforge-script`, the `pyst` surface | `scripts/python-test.sh` (sets `PYO3_CONFIG_FILE`); the `fmt`, `clippy` and `test` jobs do not enable `python` |
| Python integration of `stokesforge-app-state` / `stokesforge-app`, or CLI `run` | `scripts/python-test.sh` covers these crates; apply its environment to Clippy; `scripts/cli-dist.sh` if CLI packaging changes |
| Typst formulas, `stokesforge-render::formula` | nothing more: `cargo test --workspace` covers them (firm dependency), exports included |
| any text file, comment, test data or doc | `python3 scripts/french-guard.py` (translate, or mark deliberate French with `lang: fr`; see `docs/tests.md`) and, if the guard changed, `python3 scripts/french-guard-test.py` |
| an `#[ignore]` robustness reproduction | `cargo test -p stokesforge-robustness -- --ignored` |
| formula recalculation (`stokesforge-session/src/recalc.rs`), data read by a formula, a column write | `b22` and `recalc_targeted.rs` run in the workspace, but a green oracle proves nothing if the draw never produces the gesture: `docs/tests.md` § Targeted recalculation (#193) — and what to test when relying on it |
| a dependency, a resource, the distributed runtime | `cargo deny --locked check licenses` and notice generation (`docs/license-and-extensions.md` § Third-party licenses and distribution); a changed `Cargo.lock` revalidates `vendor/crate-license-notices/` (`CLAUDE.md` § Conventions) |
| a version number, `CHANGELOG.md`, a release or distribution script, the tag jobs of the CI | `scripts/release-notes-test.sh`, `scripts/release-tag-test.sh`, `scripts/versions-aligned.sh` (all crates and internal dependencies in one version, `Cargo.lock` up to date, `FORMAT_VERSION` = N needs `tests/witnesses/vN/project.stf`; also run by `fmt` on every push), `scripts/release-tag-on-main.sh` / `release-tag-increasing.sh` (tag on main, increasing) and `scripts/release-notes.sh v<version>` (versions of the crates against the tag and the changelog section), `cargo update --workspace` for `Cargo.lock`, `docs/v1-criteria.md`; the tag jobs themselves only run on a real tag |
| the **appearance** of the interface | a screenshot (`run-stokesforge`); an interaction scenario (menu, dialog, typing, undo) is replayed with its driver; a new text is captured again with `STOKESFORGE_LANG=en-XA` (a text without `[[ ]]` escaped the catalogue, a missing `]]` is truncated) |
| a reference screenshot (`tests/snapshots/*.png`) failing on Windows/macOS by a few pixels | tolerance per system in `kittest.toml`; never regenerate a reference outside Linux; delete the `*.new.png` / `*.diff.png` left behind (`docs/tests.md` § What to know before writing a test) |
| a real window (native Wayland, `rfd`, multi-screen, drag, detached OS window) | nothing automatic: `run-stokesforge` runs X11 under Xwayland. Say so in the report, or ask the user for a screenshot |
| interface texts | keys in `en` **and** `fr` in the same commit; `no_hardcoded_label` and `no_catalog_key_is_dead` green (`stokesforge-ui-texts`) |

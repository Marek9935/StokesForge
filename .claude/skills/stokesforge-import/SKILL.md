---
name: stokesforge-import
description: Work on importing instrument files (stokesforge-import, stokesforge-importer) — recipes, delimiters, decimal comma, header and footer lines, sub-headers, live preview. Use for a badly read CSV/TXT/ASC, a new instrument (Jasco…), a column read as text, or a wrong import preview.
---

`stokesforge-import` reproduces OriginLab's ASCII import wizard: the user picks a **recipe** describing how an instrument writes its files, applies it, and may save it.

## Read first

The header doc of `crates/stokesforge-import/src/lib.rs`, then `docs/application/projet-et-import.md` § Import wizard. `docs/chantier-import-previsualisation.md` is a hand-over note: confront its "to do" with the code before reusing it.

## Rules

1. **The crate stays pure**: no dependency on UI, `stokesforge-core`, `stokesforge-commands` or `stokesforge-session`; its output vocabulary is `f64` with `NaN` for missing. Conversion to model columns is `stokesforge-importer` (`plan_import`, `make_import_column`), shared by the app, the CLI and `pyst.import_file`: never rewrite it in a consumer. Missing information is added to the crate's own vocabulary, never by importing the model.
2. **One layout, one line splitter**: `layout` (with `layout::split_lines`) is computed once and shared by `parse` and `preview`; `data_end.rs` is the single rule for where data stops. Never re-implement these steps in one of them.
3. **Extend a recipe without breaking saved ones**: every new field carries `#[serde(default)]`, and its default reproduces **exactly** the previous behaviour. The recipe has a mirror in the `.stf` format (`crates/stokesforge-persistence/src/recipe_record.rs`, #323): add the field there too, with the same default. Test that a JSON recipe from **before** the change reloads and splits identically.
4. **Respect the preview's cost budget** (header doc of `preview.rs`): a preview that rereads the whole file on each keystroke makes the window unusable.
5. **The app imports in the background**: menu and drag-and-drop both go through `AppState::start_import`, applied by `poll_import` (#266); never read or parse on the UI thread. Writing into a live project goes through undoable commands (`stokesforge-commande`); only `stokesforge-cli import` builds a new `Project` directly.
6. **Wizard texts** are `import-*` keys used by `tr!`, with the terms of `docs/textes-interface.md` § 2.4 Import (*Delimiter*, *Template* — never « recipe » on screen —, *Subheader*, *Footer*). Template names and file contents are data, never translated. Window: `app/import_wizard.rs`, a dialog of the shared building block (`stokesforge-feature`, sheet `windows-dialogs`); state in `stokesforge-app-state/src/state.rs`.
7. **No recipe in production code** (#378): the program carries no template; the user saves their own. Test templates live in `test/import/test_recipes.json` (a `RecipeStore` file), copied into a redirected config folder by the tests that need them.
8. **A mixed-type column is not an import failure** (invariant 10): keep it, never refuse the file.

Module map and symptom → field table: [references/modules-and-symptoms.md](references/modules-and-symptoms.md).

## Tests

Supporting an instrument means **adding its sample file** (reduced, anonymised) to `crates/stokesforge-import/tests/fixtures.rs` with the expected split; reuse `test/import/` samples first, and check a new one holds nothing the user does not want published. Preview/parse agreement: `tests/preview_import_agreement.rs`.

Done when no forbidden dependency was added, an old saved recipe reloads identically, the sample file and its test exist, and `stokesforge-verification` is green.

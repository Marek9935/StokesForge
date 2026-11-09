---
name: stokesforge-import
description: Work on importing instrument files (stokesforge-import, stokesforge-importer) — recipes, delimiters, decimal comma, header and footer lines, sub-headers, live preview. Use for a badly read CSV/TXT/ASC, a new instrument (Jasco…), a column read as text, or a wrong import preview.
---

`stokesforge-import` turns an instrument file into a **recipe** (how that instrument writes its files): the **detector** proposes it from the content, a saved template with matching **conditions** can replace it, the user corrects it on **one screen** (the Import Wizard, #488), and may save it.

## Read first

The header doc of `crates/stokesforge-import/src/lib.rs`, then `docs/application/project-and-import.md` § The import screen. `docs/import-preview-work-package.md` is a hand-over note (its wizard-step sections are history): confront its "to do" with the code before reusing it.

## Rules

1. **The crate stays pure**: no dependency on UI, `stokesforge-core`, `stokesforge-commands` or `stokesforge-session`; its output vocabulary is `f64` with `NaN` for missing. Conversion to model columns is `stokesforge-importer` (`plan_import`, `make_import_column`), shared by the app, the CLI and `pyst.import_file`: never rewrite it in a consumer. Missing information is added to the crate's own vocabulary, never by importing the model.
2. **One layout, one line splitter**: `layout` (with `layout::split_lines`) is computed once and shared by `parse` and `preview`; `data_end.rs` is the single rule for where data stops. Never re-implement these steps in one of them.
3. **Extend a recipe without breaking saved ones**: every new field carries `#[serde(default)]`, and its default reproduces **exactly** the previous behaviour. The recipe has a mirror in the `.stf` format (`crates/stokesforge-persistence/src/recipe_record.rs`, #323): add the field there too, with the same default. Test that a JSON recipe from **before** the change reloads and splits identically.
4. **Respect the preview's cost budget** (header doc of `preview.rs`): a preview that rereads the whole file on each keystroke makes the window unusable.
5. **The app imports in the background**: menu, window and drag-and-drop (nothing to check: no window; a point to check or a refusal: the window on all the files) go through `AppState::start_import`, applied by `poll_import` (#266); never read or parse on the UI thread. Writing into a live project goes through undoable commands (`stokesforge-command`); only `stokesforge-cli import` builds a new `Project` directly.
6. **One entry point for the recipe of a file**: `stokesforge_import::recipe_for_file` (named template, else a template whose conditions the file meets, else the detector `sniff`). The application (screen and drop), `stokesforge-cli import` and `pyst.import_file` all call it; never pick a recipe another way in a consumer.
7. **The detector never picks silently.** Several blocks of numbers, text rows in the data, a comma that may be a decimal, a JCAMP-DX problem, a tie between templates: each is a `Doubt` (data, no sentence) that the screen lists under **To check**. A new heuristic that chooses between readings must either be certain or raise a doubt. **NMR is out of scope** (refused with a card; an NMR block of a mixed file is left out and flagged).
8. **Where the detector lives**: `sniff` (the whole configuration), `instrument` (fingerprints from the content, never the extension), `jcamp` (JCAMP-DX reader), `thousands` (strict grouping guess), `props` (`KEY;value` lines into Long Name / Units / Comments / sheet name), `templates` (conditions, `RecipeStore::matching`, `recipe_for_file`). Pure and without sentences; the app-state layer (`state/import_*.rs`) turns them into sentences and decisions, the shell (`app/import_wizard/`) only paints. Answers to doubts are remembered per kind of file in the store.
9. **A corpus scores the detector, outside the repo**: `../_stokesforge/corpus488/` (local files, scripts `run_score.sh`, `classify*.py`, `SCORE.md`). Rerun it after any change to `sniff`, `jcamp`, `instrument` or `thousands`: the figure to watch is « wrong silently », which must stay 0. The repository holds only reduced public or synthetic fixtures (`test/import/`), never the user's private files.
10. **Refresh from Source** reads the stored recipe as it is, with no detection, and is not promised to reproduce an import across versions (`docs/persistence.md`). The new recipe fields are stored additively (rule 3).
11. **Wizard texts** are `import-*` keys used by `tr!`, with the terms of `docs/ui-texts.md` § 2.4 Import (*Delimiter*, *Template* — never « recipe » on screen —, *Subheader*, *Footer*). Template names and file contents are data, never translated. Window: `app/import_wizard.rs` and `import_wizard/` (grid, panel, guide, templates), a dialog of the shared building block (`stokesforge-feature`, sheet `windows-dialogs`); state in `stokesforge-app-state/src/state/import_*.rs`. Terms: *To check*, *Mark as Checked*, *Check Now*, *Answers remembered*, *Advanced*, *Show Guide*, *Don't show again*. A long text wraps or moves under its sentence, never cut at the panel edge (test `long_texts_of_the_left_panel_are_never_cut`).
7. **No recipe in production code** (#378): the program carries no template; the user saves their own. Test templates live in `test/import/test_recipes.json` (a `RecipeStore` file), copied into a redirected config folder by the tests that need them.
8. **A mixed-type column is not an import failure** (invariant 10): keep it, never refuse the file.

Module map and symptom → field table: [references/modules-and-symptoms.md](references/modules-and-symptoms.md).

## Tests

Supporting an instrument means **adding its sample file** (reduced, anonymised) to `crates/stokesforge-import/tests/fixtures.rs` with the expected split; reuse `test/import/` samples first, and check a new one holds nothing the user does not want published. Preview/parse agreement: `tests/preview_import_agreement.rs`.

Done when no forbidden dependency was added, an old saved recipe reloads identically, the sample file and its test exist, and `stokesforge-verification` is green.

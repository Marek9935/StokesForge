# Work package "import preview" — handover note

**This document is not a reference page.** Unlike the rest of `docs/`, which
describes the code as it is, this one describes a *work package*: what it
aimed at, what it found along the way, and what it leaves open. Its lifetime
is limited — when the "what remains" column is empty, it moves to
`docs/archive/`. The full specification, verbatim, lives in
`docs/work-packages/import-preview-plan.md`.

The *what* and the *why* of the result live closest to the code:
`crates/stokesforge-import/src/preview.rs` (module head) for the cost budget
of the preview, `crates/stokesforge-import/src/recipe.rs` for the model
(`HeaderLayout`, `SubheaderRoles`), `crates/stokesforge-app/src/app/import_wizard.rs`
(`import_window`, moved out of `app.rs` by #118) for the panel, and
`crates/stokesforge-app/src/app/grid.rs` (`grid_contents`) for the text field
bug.

## The goal

A reminder of the non-negotiable point of the specification: while the user
sets the import parameters, they see **in real time** how the file is split —
not only once the import is confirmed. The rest (exact widgets, OriginLab
vocabulary) was negotiable.

## What is in place

| Crate | What changed |
|---|---|
| `stokesforge-import` | `ImportRecipe` gains `header_layout` (`HeaderLayout`: separate main header/subheader lines, auto detection), `subheader_roles` (`SubheaderRoles`: ranges to `long_name`/`unit`/`comment`, same-line name/unit option), `skip_chars`, `preview_lines` — all `#[serde(default)]`. `parse()` applies the roles and keeps, for each non-numeric cell, the original token (`ParsedColumn::texts`). New module `preview`: `preview()` classifies each shown line by role without re-parsing the whole file; `detect_header_layout()` is the equivalent of OriginLab's automatic re-evaluation button. |
| `stokesforge-app` | `import_window`: controls for the structured split, role assignment, characters to remove, and a line-by-line preview panel colored by role (replaces the old frozen value grid). `AppState::import_line_preview`, `AppState::auto_detect_import_header`. `AppState::import_files` builds a `Mixed` column rather than a `Numeric` one as soon as a cell holds a kept text, and fills `unit`/`comment` of `ColumnMeta` in addition to `long_name`. `grid_contents` no longer consumes Backspace/Delete while the import window is open (the text field bug, see below). |
| `stokesforge-core`, `stokesforge-commands`, `stokesforge-render` | **nothing** — the work package uses `ColumnType::Mixed`/`Column::from_mixed`, already in place since the "mixed-type columns" work package; it does not extend them. |

## The text field bug: what it really was

It was not in the import window itself. `grid_contents`
(`crates/stokesforge-app/src/app.rs` at the time, `src/app/grid.rs` since #120) consumes Backspace/Delete via
`ui.input_mut(|input| input.consume_key(...))` for its own shortcut
"clear the selection". `input_mut` acts on the `InputState` of the **whole**
`Context`, not on that of one `Ui` — so not only on this grid.
This block was already guarded by `state.editing().is_none() &&
state.header_editing().is_none()`, but not by
`state.import_state().is_none()`.

Now `import_window` (a floating `egui::Window`) is painted *after* the dock
in `StokesForgeApp::update` (today `ui`) — so *after* `grid_contents` (which keeps
running underneath, on every frame, as long as a workbook is open) has already
consumed the keyboard event. Result: typing Backspace or Delete in
a text field of the import window did nothing, because the key had
already been removed from the event queue before the `TextEdit` of the
import window saw it. Selecting text and then typing a replacement character
worked (it is an `Event::Text`, never consumed here) —
hence the exact symptom reported: "impossible to delete text there without
selecting it first".

Fix: `grid_contents` now includes `state.import_state().is_none()`
in the guard. There is no automated test for this specific fix — `AppState`
does not depend on egui and real keyboard input is tested nowhere
in this repository (see the head doc of `shell.rs`, criterion 13: « ce qu'elle
ajoute n'est pas testable automatiquement »), so this is not a coverage
regression specific to this work package.

## Decisions on the open questions

### OriginLab `System Parameters`/`User Parameters`

**Ignored, no field added.** `ColumnMeta` already carries `role`, `unit`,
`long_name`, `comment`, `display_format`, `formula` — nothing closer
to a free "system/user parameter" than `comment`, and merging it into
that would have mixed two distinct pieces of information (a column comment
vs. an acquisition parameter of the instrument) with no clear benefit.
Extending `ColumnMeta` with two new fields would have touched `.stf`
persistence for a need nobody has yet expressed on the StokesForge side — not
checked in detail in `docs/persistence.md` because it was not
necessary: `ImportRecipe`/`RecipeStore` never touch
the `.stf` format anyway (see below), so the question only arose for
`ColumnMeta`, decided upstream. To reopen if a real use asks for these two
roles.

### Cost of the live preview on a large file

**Two-level bound**, see the head doc of
`crates/stokesforge-import/src/preview.rs`:

- splitting the file into lines (`layout::split_lines`: `\r\n`, `\n` or a lone `\r`) stays `O(taille du
  fichier)` — incompressible, and already the case of the old `import_preview`
  (which, for its part, analyzed **all** the data lines cell by
  cell, on every frame);
- building an owned line and analyzing it cell by cell is bounded to the
  header/footer (small counts, set by the user) plus
  `recipe.preview_lines` data lines — never proportional to the rest
  of the file, however long it is.

Not done, documented as a remaining limit: the line splitting itself
is not lazy (it materializes a `Vec<&str>` for the whole file,
without copying content but with one allocation per line for the vector of
pointers). On a file of several million lines, this `Vec`
would remain the dominant cost of the preview. Not measured, not handled here — a lazy
`str::lines().skip(n).take(m)` would be the natural next step if a
real case asks for it.

### Automatic detection of header lines

A two-pass heuristic (see `detect_header_layout`, head of
`preview.rs`): main header = initial lines whose number of
fields differs from that of the last line of the file (assumed to be a data
line); subheader = following lines that have the right number of fields
but where at least one token does not parse as a number. Capped at the 500
first non-blank lines — a cap documented in the code, not a
bug, in the same spirit as the six caps of
`docs/archive/rustrewrite/questions-ouvertes.md` §11 (not modified: this
cap is new, specific to this work package, and had no place in a
page that only documents the six already decided elsewhere).

Known and uncorrected limit: a file with no data line at all
among the 500 examined makes the detection degenerate to "everything main
header, nothing subheader". Not a panic, a barely useful detection —
to correct by hand like any automatic detection that gets
it wrong, consistent with the spirit of OriginLab's « Réévaluer » button (an
explicit action, never applied continuously).

`HeaderLayout::auto_detect` (the checkbox) triggers nothing by itself
— it is `AppState::auto_detect_import_header`, called on a click of the button
"Re-evaluate Now", that reads the file and rewrites
`main_header_lines`/`subheader_lines`. A deliberate decision: recomputing on
every frame would have continuously overwritten a value the user had just
corrected by hand, preventing them from ever departing from it — the same flaw as
if OriginLab applied its detection continuously rather than on a button.

### Persistence of the new settings in `ImportRecipe`

**Not concerned by the `.stf` format** — `ImportRecipe`/`RecipeStore`
persist in a separate JSON file
(`ProjectDirs::config_dir()/import_recipes.json`, see
`crates/stokesforge-import/src/store.rs`), never in the project file.
`docs/persistence.md` does not mention them and did not need to be read
in detail for that reason. The only compatibility that mattered was
JSON→JSON: all the new fields carry `#[serde(default)]`
(`header_layout`, `subheader_roles`, `skip_chars`, `preview_lines`), checked
by a dedicated test
(`json_recipe_older_than_the_work_package_loads_with_the_defaults`,
`crates/stokesforge-import/src/recipe.rs`) that deserializes a JSON written before this
work package and asserts that the new fields take their default values
rather than making the load fail.

## Decisions not to reopen without a new reason

- **The structured model (`header_layout`) replaces `HeaderSkip` only if
  the user uses it** (`main_header_lines + subheader_lines > 0`).
  A recipe that never touches it keeps the `HeaderSkip` behavior
  from before this work package, identically — minimize breakage, the same policy
  as the "mixed-type columns" work package.
- **A subheader role range is read only on its first line**
  (`SubheaderRoles::long_name`/`unit`/`comment`, field `from`). A range
  `18` to `18` (OriginLab's reference example) does not suffer from it; a
  real multi-line range spread across different columns is not
  implemented — no instrument format met in the test fixtures
  needed it. To extend if a real case requires it, not before.
- **The specification's `count_from_bottom` ("Line number start from
  bottom") was not implemented.** Unlike the other fields, an
  incorrect implementation would have been worse than its absence (counting from
  the end of a file whose header is supposed to be at the top only makes sense
  for a use that OriginLab does not clearly document in the screenshot
  provided). Absent from `HeaderLayout` rather than present and not wired — not
  a dead field in the model.
- **A fully numeric column stays `Numeric`, never promoted to
  `Mixed`.** Only a column that actually meets a non-numeric token
  becomes `Mixed` — checked by
  `importing_a_non_numeric_cell_produces_a_mixed_column_with_the_original_text`
  (`crates/stokesforge-app-state/tests/shell.rs`), which also asserts the opposite on the
  neighboring numeric column in the same file.
- **The fix for the text field bug lives in `grid_contents`, not in
  `import_window`.** That is where the problem was born (`consume_key` on the
  global `InputState`) — fixing it in the import window itself
  (for example by trying to re-consume/republish the event) would have been
  a local fix to a structural problem, fragile at the next floating widget
  that opens a text field on top of the grid.

## What remains

- The `Vec<&str>` allocation of the line splitting in `preview()` is not
  lazy — see "Cost of the live preview" above. Not a problem
  met in practice (no fixture of several million lines
  in this repository), documented out of caution.
- No automated test for the disappearance of the text field bug
  itself (the fix is structural and checked by reading +
  build/clippy, not by a simulated typing scenario — see the dedicated
  section above).
- OriginLab's `System Parameters`/`User Parameters` remain without a
  StokesForge equivalent, by choice (see above) — to revisit if a real use asks
  for them.
- The six caps of `docs/archive/rustrewrite/questions-ouvertes.md` §11
  were re-read before this work package; none is concerned (`MAX_COLUMN_ROWS`
  keeps applying to the mixed columns imported by the same
  generic function as before, nothing new to check here).


## End of data (issue #4)

`ImportRecipe::data_end` (`DataEnd`, `#[serde(default)]` = `None` since the
"n/a" fix in X; `Auto` stays selectable, the Jasco templates
ask for it) decides where
the data stops; the rest is the footer (`LineRole::Footer`,
`ParsedTable::footer_lines`). `Auto` arms at the first line whose
first column is a number, then cuts at the first one whose X is not
one (an empty line included); a text in another column does not cut.
Other options: no cut, first empty line, marker, N lines after
the header, N lines before the end. The rule lives in `data_end.rs`, called by the
shared `layout` split, so that `parse` **and** `preview` cut at the
same place. `footer_skip_lines` applies first. The options and the right-click
on the cut line are available in the wizard; see
`docs/application/project-and-import.md` § "Import wizard". The footer can be kept
in the metadata with `keep_footer_metadata`.

## File metadata per column (issue #5)

`ImportRecipe::keep_header_metadata` (true), `keep_footer_metadata` (false) and
`excluded_metadata` (excluded keys or sections) drive `ParsedTable::metadata()`
(`crate::metadata`, key / value lines, `[...]` sections, lines without a key
kept as they are). `stokesforge-app` copies them into `ColumnMeta::file_metadata`
of each imported column — in the "Import" macro, with no
edit command: read-only. Display: "Metadata" row of the header ("📋 N
keys"), click → modal viewing window (`column_metadata_window`).
The row can be hidden (Window menu); selection by key is in the
"Metadata" step of the wizard; no automatic filling of units
(decided).

## Step-by-step wizard (issue #6)

`import_window` is now a six-step wizard (see
`docs/application/project-and-import.md` § "Import wizard"). On the `stokesforge-import` side:
`ImportRecipe::column_roles` (`ColumnRoleChoice`, Ignore removes the column
at import), `unify_header`, `detect_encoding`. The header setting no longer
exposes the "Automatic detection" checkbox: only the button "Detect Header" acts (`HeaderLayout::auto_detect` stays in the recipe, with no effect).

Numbers: a non-finite result (`inf`, `Infinity`, `1e400`) reads as `NaN` and the token stays visible as text (`Mixed` column), like `nan`; the automatic end of data and the subheader detection nevertheless count it as a number (`NumberFormat::is_numeric`).

## Space thousands separator, date order, mixed encoding (issue #314)

Decisions of 2026-10-20. "Whitespace" delimiter (chosen or resolved from Auto)
+ space thousands separator: `ImportError::WhitespaceWithSpaceThousands`, rendered by
`analyze` so identical to the preview and to the import; keeping U+00A0/U+202F
(`keep_nbsp`) is removed. `ImportRecipe::date_order` (`DayFirst` by default,
`MonthFirst`) governs the slash dates of the import (`parse_datetime_with`);
`stokesforge-core` keeps day first (#217). `Auto` encoding: mixed
UTF-8 + Windows-1252 decoding byte by byte, label "UTF-8 (+ Windows-1252)".

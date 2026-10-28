# Specification — work package 2: Origin-style import preview

## Goal

**The important point of this work package, and the only one that must not get
lost along the way**: while the user sets the import parameters, they must
see **in real time how the file is actually split** — which lines are header,
which feed which column role, where the data starts — not only once the import
is confirmed. Everything else in this document (precise widgets, roles named
as in OriginLab, detailed configuration options) is secondary to this
principle and negotiable; this principle is not.

Concretely: rebuild the preview panel of the import window
(`import_window`, today `crates/stokesforge-app/src/app/import_wizard.rs`) so that it shows
this split live, as the user adjusts the import parameters, modeled on the
OriginLab ASCII import wizard.

This is the most important point of the two work packages noted on 2026-09-15: it
includes and fixes along the way the known bug of the text field of the import
module (impossible to delete text there without first selecting it),
since this area is entirely rebuilt here.

## Reference: screenshot of the OriginLab ASCII import wizard

Screenshot provided by the user on 2026-09-15 (OriginLab ASCII import wizard),
described below because it cannot be embedded as is in the versioned
documentation.

The panel it shows is divided into three areas:

1. **Header line counts**, at the top:
   - `Number of main header lines (exclude subheader lines)` — number of
     "main" header lines, in a drop-down menu, with an automatic
     re-evaluation button next to it (cycle icon) and a
     `Line number start from bottom` checkbox to count from the end of the file.
   - `Number of subheader lines` — number of "subheader" lines
     (those that carry column metadata: long name, unit,
     comment…), the same kind of drop-down menu + re-evaluation button.
   - `Auto determine header lines` — checkbox that lets the tool
     deduce these two counts from the file content rather than typing them
     by hand.

2. **Line role assignment** ("Column Label Assignment from Subheader Lines"):
   for each of the roles `Long Names`, `Units`,
   `Comments`, `System Parameters`, `User Parameters`, a pair of
   "from … to …" drop-down menus that chooses **which range of subheader
   lines** feeds this role (a single line in the screenshot: `18` to
   `18` for `Comments`, for example), each with its own re-evaluation button.
   An `Extract Long Names and Units from Same Line` checkbox
   reads the two roles on a shared line rather than on two separate
   lines. Next to it, `Characters to skip on each line` removes a fixed
   prefix from each data line before parsing.

3. **Live text preview**, at the bottom, with two settings that drive it
   (`Preview Font`, `Preview Lines` — number of lines displayed): the
   raw content of the file, numbered, with for each line a **prefix
   code** that says how this line will be interpreted with the current
   settings — legend given in full above the panel:
   `S=Short Name, L=Long Name, U=Units, P=Parameters, C=Comment,
   MH=Main Header, SH=Subheader`. The lines are colored according to their role
   (blue for the recognized header roles, magenta for a comment,
   gray for data lines not yet reached by `Preview
   Lines`…). **Any change in the two areas above re-renders this
   preview immediately**, with no intermediate confirmation step — this is the
   central behavior to reproduce, more than any specific widget.

## What already exists on the StokesForge side (starting point, not to reject wholesale)

- `crates/stokesforge-import`: a pure crate (zero UI/core dependency), which already models a
  **recipe** (`ImportRecipe`) inspired by the same OriginLab wizard
  — see the head comment of `crates/stokesforge-import/src/lib.rs`. Current fields:
  `separator` (`Separator`), `decimal` (`DecimalStyle`), `header`
  (`HeaderSkip`: no skip, `N` fixed lines, or up to a marker),
  `footer_skip_lines`, `has_column_header_row`, `name_source` (file name
  or key read from the header).
- `crates/stokesforge-core/src/column.rs`: `ColumnMeta { unit, long_name,
  comment }` already exists per column — the OriginLab roles `Long Names`/`Units`/
  `Comments` therefore have a natural destination in the current
  model. Nothing equivalent to OriginLab's `System Parameters`/`User Parameters`
  exists: to decide (ignore them, or land them in
  `comment` for lack of better — see "Open questions").
- `crates/stokesforge-app/src/app.rs` (today `src/app/import_wizard.rs`), function `import_window`: the current
  import window (transient dialog as long as `AppState::import_state`
  is `Some`), with its simple text fields (including the noted bug) and no
  live preview of the split.

The main gap with OriginLab: the current `HeaderSkip` model treats the
header as a block to skip, without distinguishing a "main header"
from a "subheader", and without assigning **line ranges** to semantic
roles per column — this is the whole contribution expected from this work package.

## Expected features

1. **Two distinct header line counters** (main / subheader)
   with a count-from-the-bottom option and a reasonable automatic
   detection (heuristic to document — e.g. first line where the number
   of fields per `separator` becomes stable and numeric again).
2. **Assignment of subheader ranges to the roles** `long_name`, `unit`,
   `comment` of `ColumnMeta` (plus, if retained, a treatment for the
   OriginLab system/user parameters — see open questions),
   with an option to read long name and unit on the same line.
3. **Number of characters to remove at the start of each line** before parsing
   (useful for the fixed prefixes of some instrument formats).
4. **Live preview, raw text colored/annotated by line role**,
   recomputed at every parameter change, with a configurable number of
   displayed lines. It must stay usable on large files (no
   costly full re-parsing at every keystroke — see "Points of
   attention").
5. **Fix of the text editing bug**: all text fields of the
   panel (recipe name, header marker, header key…) must
   allow normal deletion/insertion with the keyboard, not only a
   replacement by selection.

## What must *not* be done here

- Do not touch work package 1 (mixed-type columns) beyond
  consuming its result: this panel must reflect in its preview the
  per-cell typing now that this work package is merged (a non-numeric value
  in an otherwise numeric column must be annotated in the
  preview, not make the parsing fail).
- Do not copy OriginLab's vocabulary as is if a role has no
  equivalent in the StokesForge model (`System Parameters`/`User Parameters`
  in particular) without an explicit decision — see below.
- None of this goes through an undoable command of
  `stokesforge-commands` **as long as the import is not confirmed**: the preview
  panel works on a transient local state of the import
  window, as today — only the final application of the result to the
  workbook must go through a command (this is already the case, do not
  regress).

## Open questions to decide before/during implementation

- **OriginLab's `System Parameters`/`User Parameters`**: ignore them (no
  equivalent field in `ColumnMeta`), merge them into `comment`, or
  extend `ColumnMeta`? Impact on the `.stf` format if extended — read
  `docs/persistence.md` before deciding.
- **Cost of the live preview** on a large file (tens of thousands of
  lines): re-parse the whole file at every keystroke in a field, or only the visible window (`Preview Lines`) + the header lines?
  OriginLab seems to limit explicitly via `Preview Lines`.
- **Automatic detection of header lines** (`Auto determine header
  lines`): heuristic to define and document, with its limits
  (`docs/archive/rustrewrite/questions-ouvertes.md` §11 already lists similar
  caps/heuristics elsewhere in the project — check that an
  odd-looking edge behavior is not already an accepted choice).
- **Persistence of these new settings in `ImportRecipe`**: backward
  compatibility of the JSON format of the recipes already saved by
  `RecipeStore` (`crates/stokesforge-import/src/store.rs`) to check.

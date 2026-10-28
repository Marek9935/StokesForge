# Application: grid

Continuation of the index [`README.md`](README.md). Contents: Metadata and x' rows, context menus and multiple selection of the grid, gutter, Tab and arrows, alignment, input.

### The "Metadata" and x' header rows

Two optional rows, **set per workbook**: Workbook › Show Metadata Row / Show x'
Row, the right-click of the "#" cell (under the selection section), and, for
Metadata, the right-click of its label. The setting of a workbook
(`grid::HeaderRows`, `header_rows` / `set_header_rows`) is kept in egui's
memory under the workbook's identifier, shared by its floating and detached
windows, never in the model or the `.stf`; it is lost when the application
closes.

The setting for **new** workbooks is a preference (Preferences › Display;
`preferences.json`, `workbook_show_metadata` shown by default,
`workbook_show_x_prime` hidden by default; `Action::SetWorkbookHeaderDefaults`).
`app.rs` publishes it in the `Context` on every frame
(`grid::publish_header_row_defaults`): a workbook the user has not set follows
it, a workbook that was set keeps its own. The header height loses one
sub-row for each hidden row (`stacked_header_height`). Known limit: keyboard
navigation can still reach the hidden x' row.


### Grid context menus

The workbook no longer has an action strip above the grid. A right-click
in the empty space to the right of the last column offers "Add Column"; a right-click on a sheet tab offers "Add Sheet", "Rename", "Duplicate" and "Delete", and on the empty part of
its strip only "Add Sheet" — directly, without a submenu (#158). The strip stays below the grid in the
floating and detached windows. "Import…" is in the File menu.
Its dialog chooses between the active workbook, one new workbook per file
and a single new workbook for the whole batch. Each valid file creates its
own sheet; the new workbooks are created only after a successful
analysis. The batch is a single undoable history entry.

The five zones of the grid — data cell, header field of a single
column, row of letters, row-number gutter, whole-field labels
in that same gutter — carry the **same** editing block on right-click
(`app::grid::grid_edit_menu`): a clipboard group (Cut, Copy,
Copy with Header, Paste, Paste in Place), then Clear and Delete
and Shift Up. Undo and Redo are no longer there (Edit and the keyboard, #158); each
entry keeps its keyboard equivalent, a mirror of the shortcut block of
`grid_contents`. The row of letters surrounds this block with what is specific
to it: Plot ▸ and Add Column before, Move ▸, Type ▸ and Role ▸
after the clipboard (they describe the column rather than act on
its content).

**Move ▸** (#319, `grid::move_columns_menu`, painted by
`column_type_role_menus` so also in Workbook › Columns): four entries,
Move to First · Move Left · Move Right · Move to Last, which move the
selected columns **within their sheet** (never from one sheet to another).
Left and Right move each column by one place, the gap between non-contiguous
columns stays (`A [B] C [D] E` → `B A D C E`); First and Last gather the
selection at the start or at the end, in its order. All or nothing: an entry is
**disabled**, with the reason in a tooltip, when the gesture would change nothing (Left if the
first selected column is already in A, Right symmetrically, First and Last if
the order does not change); a selection of whole rows, which covers all the
columns, disables all four. One column: one `Move “B”` entry; several: one
`MacroKind::MoveColumns` macro, a single Ctrl+Z. **No** "Column in use" window: since #321 a formula follows its column. Graphs already plotted do not
move; only later plots pair the Ys with the Xs that are to their
new left. The ordering logic is pure (`stokesforge-app-state`,
`column_move.rs`) and serves both the disabling and the execution; an `Unknown` child
under the sheet keeps its place (the index is translated into the full list of
children). The width of a column follows its identifier, so its column
(#325).

The entries are **bare** verbs: "Copy", not "Copy the cell";
"Undo", not "Undo Edit Cells in Column A". The right-click has just
landed on its target, and the shortcut is already shown on the right of
the entry — naming the target as well made every line longer without teaching
anything. `GridMenu::target_label`/`copy_full_columns_label` and
`AppState::undo_label`/`redo_label` remain **available and tested** for the
surfaces where you do *not* see what you are aiming at (the menu bar already uses them
for Undo/Redo, and a status line or a confirmation
will need them); they are simply not called by the context menu.

The **decision** (which gestures make sense here, which to disable, how to
name the target the day it is useful) lives in `AppState::grid_menu` →
`GridMenu`, not in the egui code: that is what makes it verifiable without a
window in `stokesforge-app-state/tests/shell.rs`, like the rest of the copy/paste matrix.
Two contextualizations are played out there:

- the number agreement of `target_label` ("the cell" / "the fields" /
  "the columns" / "the rows"), with two distinct plurals — that of the
  **cells** (`GridMenu::plural`) and that of the **columns**
  (`GridMenu::multi_column`): three stacked cells are plural without
  "Copy the whole column" having to become plural.
  `GridZone::Rows` (the gutter) is an exception to `plural`: its labels
  speak of **rows**, not cells, so a single selected row stays
  "the row" even on a sheet with several columns — `plural` counts there
  only the number of rows covered, never `multi_column`;
- "Delete and Shift Up" **does not appear in the header**: a field is not
  repeated per row, Del just clears there, and offering it would make two
  entries for a single effect — true both for a field of a single
  column and for a whole field across all columns (`GridZone::
  Header` covers both, see below). It does appear, however, for a
  range of whole rows: deleting a row moves what follows up
  in all columns, exactly like a rectangle of cells.

### Multiple selection in the grid (#159)

`selection.rs` holds the selection of each sheet — pure interface state,
outside the history. It is of **one kind only** at a time (columns, rows
or cells), and the header/data boundary stays uncrossable; each
kind is a **set** and no longer a range `{ ancre, curseur }`:

| Kind | Content | Gestures |
|---|---|---|
| columns (`Columns`) | `base` (columns set one by one) ∪ range `ancre..=curseur` | click · Ctrl+click toggles the column · Shift+click replaces everything with the range anchor → target · Ctrl+Shift+click and drag add the range |
| rows (`FullRows`) | intervals of rows (same model) | same gestures on the row numbers |
| cells (`Rows`) | rectangles already set (`extra`) + last rectangle `ancre`–`curseur` | Ctrl+click adds or removes a cell (the rectangle is split around it) · Ctrl+drag adds a rectangle · Shift+click extends the last rectangle |

A Ctrl+click of another kind, or on the other side of the
header/data boundary, sets a simple selection again. The whole header fields
(`HeaderRows`) stay simple ranges. Removing the last element clears the
selection.

The consumers reason in **blocks** (`Selection::blocks`, one block =
columns of the sheet × range of ranks); the order is always that of the
sheet (left → right, top → bottom), never that of the clicks.

- **Corner cell "#"** and **Ctrl+A** (grid focused, outside editing;
  Edit › Select All): `Action::SelectAll`, all the visible columns,
  header included, even with no rows; a second click on the cell, or Esc in
  the grid, clears the selection. The cell has a tooltip and the "hand" cursor.
- **Copy / Cut**: non-contiguous columns end to end in the order of the
  sheet; blocks of cells that share their rows (side by side) or their
  columns (stacked) — otherwise Copy and Cut are disabled (`GridMenu::copy_enabled`,
  `Selection::copy_layout`). Copy with Header follows the columns touched.
- **Paste / Paste in Place**: disabled on a non-contiguous selection
  (`GridMenu::paste_enabled`, ambiguous target); pasting with the keyboard is refused
  with a message.
- **Clipboard format (#259)**: the system text is TSV in the
  Excel/LibreOffice format, written and read by `stokesforge_core::tsv` (`encode`,
  `decode`, `encode_cell`), without egui: a cell that holds a tab, a
  line break or a quote is wrapped in quotes (quote doubled),
  so a multiline text shifts nothing on paste, here as in a
  spreadsheet. On reading, `\r\n` and a trailing line end are accepted
  (pasting from a spreadsheet on Windows). A **missing** cell is copied
  empty: `cell_copy_text_in` handles it before the conversion to text, never by a
  `replace` of `MISSING_TEXT_DISPLAY` afterwards, which would also erase the
  non-breaking spaces that were really typed. The app keeps only the choice of cells.
- **Input and paste (#258)**: a single "text → cell value" reading rule,
  `ValueBlock::from_texts(kind, &[&str])` (`stokesforge-core`,
  `column.rs`), called by `commit_cell` (one cell) and by
  `value_block_from_tsv_column` (a pasted column): a typed or pasted date
  gives the same cell. Each cell is trimmed; empty = missing value;
  `Numeric` reads a finite number (decimal comma accepted), `DateTime` reads first
  a date or a time; `inf`, `nan` and `1e400` stay **text** (in all
  three types, an already open `Mixed` column included — only the type conversion
  by the header keeps a written `inf` = infinity, rule C2). An unreadable text in
  `Numeric`/`DateTime` returns a `Mixed` block and the "widens" flag:
  written by `SetCellsPromoting` (unchanged), the cells read staying
  numbers, the date included. After a paste that overwrites a `Mixed` column,
  if all its present cells are numbers, it goes back to `Numeric`
  (`SetColumnType`, as after an input); the repromotion happens **before**
  the `PasteCells` group is closed, and a paste of a single column opens this
  group only when a repromotion takes place: a paste stays one history
  entry, a single Undo. A `DateTime` column widened to `Mixed`
  then shows the date as a numeric timestamp, as for a
  typed date (existing behavior, not modified).
- **Clear** empties each block, in a single undoable group. **Delete and Shift
  Up** is offered only for a single block (holes would make the
  shift ambiguous); Del then just clears.
- **Right-click**: inside the selection, the menu acts on the whole selection, even
  non-contiguous; outside the selection, it replaces it with the targeted cell (#152).
- **Plot** receives the ordered list of selected columns (A as X, C and
  E as Y…). Type ▸ and Role ▸ go through `Action::SetColumnsType`/`SetColumnsRole`: a single
  undo step (`MacroKind::SetColumnsType`/`SetColumnsRole`) for all the columns.

During a drag, the extension on hover applies only to the cells where
the press did **not** start (`press_started_in`): otherwise a Ctrl+click, which
toggles on release, would already have extended the selection during the press.


### Column width: per column, not per position (#325)

The grid is an `egui_extras::TableBuilder`, whose memory keeps the
widths **by position** and drops them as soon as the number of columns changes:
deleting or adding a column used to reset the whole sheet, and a
move would not have followed it. The width is therefore a fact of the
**column**: `NodeId -> f32` in the actor (`SessionHandle::set_column_width`,
`Snapshot::column_widths`, `AppState::column_width` / `Action::SetColumnWidth`),
saved in the `.stf` (`docs/persistance.md`). Outside the model, outside the
history: resizing **cannot be undone**, but Ctrl+Z that brings back a
deleted column gives it back its width (the identifier is never reused,
invariant 7, and the table is not purged on mutation).

On every frame, `grid_contents` gives egui back `TableColumn::initial(largeur de
la colonne ou DEFAULT_COLUMN_WIDTH)` for each column, and calls
`TableBuilder::reset()` when the ordered list of identifiers (with the epoch
of the project) differs from that of the previous frame — never a variable
`id_salt`, from which the identifier of the vertical scroll area would drift (the
scroll position would be lost). A new column (added, pasted, imported, analysis)
takes the default width. No fit-to-content by double-click.

The width is pushed only at the **end of a resize done by hand**
(`finished_column_resizes`: handle released, width read from the extent of the
header): pushing the rendered width on every frame would mark the project
as modified on every opening. The detached view has its own `AppState`; its
width also goes through the actor, and the main state rereads it on its
refresh.

**Same default width floating / maximized (#337).** Reported under
#330: maximized, the workbook window made the never-set columns go from 57 to 62 px of spacing between letters. Cause: the first frame of an
`egui::Window` is a sizing pass (`Ui::is_sizing_pass`) during which
egui_extras shrinks each column to the width its content used (50
instead of 54), then keeps that value; a maximized window, painted in the
`CentralPanel`, has no sizing pass and kept the 54. Neither the values nor
the size of the window are to blame (the direct bench, without `Window`, saw
nothing). `grid_contents` forgets the table state on the frame that follows a
sizing pass, and does not retain the gutter width during it.

**Reset to default (#330).** Right-click on a letter or Workbook ▸ Columns:
"Reset Width" (after Role ▸), on the whole selection; Workbook ▸ Sheet and
the right-click of the tab: "Reset Column Widths", all the columns of the
sheet. `Action::ResetColumnWidths { columns }` →
`SessionHandle::reset_column_widths` **removes the keys** from the table (the default
is the application constant `DEFAULT_COLUMN_WIDTH`, nothing is written to
the `.stf`). Not undoable, no confirmation, like setting it. Entry disabled
("Columns are already at the default width") when no targeted column has a
retained width. The layout key of `grid_contents` contains the retained
widths of the visible columns: without them, `TableColumn::initial`
is read only after a `reset` and the reset would change nothing on screen.
`TableBuilder::reset` resets **all** the columns, gutter included: the
rendered width of the gutter is memorized per sheet in egui's temporary memory
(`gutter_width_id`) and given back as `initial`, otherwise each drag would
bring it back to 90. Resetting one column does not touch it; resetting the whole
sheet forgets it (`forget_gutter_width`): back to 90, never saved.

### A column has two identities, only one is visible

Its **node name** (`Node::name` — "Temps", "Signal", or the automatic "A"/"B"
of `Project::next_column_name`) is a key of the model: it resolves
a column from a script (`pyst.resolve("[Book1]Sheet1!Temps")`), addresses it
in a graph spec, finds it again in a `.stf`. The grid shows it
**nowhere**: it shows the **position** letter (A, B, C…) and the
four fields of `ColumnMeta` (Long Name, Units, Comments, Formula), which are
yet another thing.

**Formula row (#321).** A formula remembers the columns it reads **by
identifier**; the letters that the cell shows and that one types are only
a display, reread on every frame: inserting, deleting or moving columns
(including the outputs of an analysis) never changes what the formula reads,
the displayed letter follows the column (`a*2` is written `A*2`). If a column read is no
longer in the sheet, the cell shows `#REF` and paints a red frame with the
tooltip "Column deleted. Replace #REF with a column letter". Three
inputs are **refused**: a letter that designates no column of the
sheet, the column itself, and a modified `#REF`. Enter or Tab on a refused
text leave the field open (red frame, tooltip that says why); a click
elsewhere closes it without writing anything. Validating the displayed text as is (even with
`#REF`) writes nothing. A **syntax error** is not a refusal: it is
saved and the column fills with `---`. When pasting a rectangle, the refused
formula is the only one not pasted: the other fields are, and
the status bar states the refusal after the writes. The decision is made in
`stokesforge-app-state` (`header_edit_refusal`, `formula_issue`), the grid only
paints it.

`stokesforge_commands::Command::label` used to build its history labels on
the node name, and so announced "Input in Temps" for a column that
the user only knows as "A" — a word not to be found on screen, and
downright misleading when the two diverge (a node named `T` in third
position is displayed "C"). The label now designates the column by its
position (`ColumnRef`, since #46 a datum and not a sentence): "Edit Cells
in Column A". The other kinds of node
(sheet, workbook, graph, note) keep their real name — that one *is*
displayed, in the Project Manager.

The position → letter mapping is `stokesforge_core::spreadsheet_name`,
made public for the occasion: it is the same function that names the
columns created without an explicit name, that paints the header of the grid
(`app::grid::column_letter` is now only an alias for it) and that writes the history
labels. Three copies of this algorithm meant three ways of calling
the same column.

The gutter (`app::grid::row_number_widget`) is the exact mirror of the row of
letters (`app::grid::column_header_widget`): its whole width selects, on click,
**the whole row** — the visible columns of the sheet, at this row, whatever
their own length (a column shorter than the targeted row
is still touched by the selection, only its really written portion is
affected by Clear/Delete). `AppState::select_row`/`extend_row_selection`/
`is_row_selected` are the counterparts of `select_column`/
`extend_column_selection`/`is_column_selected`, and `SelectionKind::FullRows`
(`stokesforge_app_state::selection`) the counterpart of `SelectionKind::Columns` — same rule of
not crossing the boundary: a drag that starts in the gutter and crosses
a cell or the row of letters sets a simple selection again rather than
silently extending with no effect, and conversely.

### The header-side gutter: uniform, and also selectable

On the stacked header side (`app::grid::row_number_header_widget`), the gutter carried
a decorative "#" on a `header_letter_row_fill` background, then four rows of
labels ("Long Name", "Units", "Comments", "F(x)=") that stayed
**transparent** and inert — the left column was therefore uniform neither
visually (the background stopped at the "#") nor functionally (nothing in it
was clickable). Both are fixed together: each label row
now carries the same background as the "#" (or the selection highlight),
and a click on a label (`app::grid::header_row_label_widget`) selects that
field for **all** the visible columns of the sheet — symmetrical to
`row_number_widget` on the data side. `AppState::select_header_row`/
`extend_header_row_selection`/`is_header_row_selected` are the counterparts
of `select_row`/`extend_row_selection`/`is_row_selected`, and
`SelectionKind::HeaderRows` (`stokesforge_app_state::selection`) the counterpart of `FullRows`
for the header zone: it touches all the columns and, symmetrically,
never touches the data (`row_range()` is always `None` there).

This is **not** the same thing as a field selected on a single column
(`app::grid::header_field_widget`, `Selection::header_field`): that one stays
strictly on one column, dragging can extend it to other columns but
never in a single gesture to *all* the columns of the sheet — the two
widgets, the two constructors and the two gestures stay distinct, even
if both end up in the same menu zone (`GridZone::Header`) once
set: `cleared_header_fields`/`copy_selection`/`paste` are all three
written in terms of `full_rank_range()`/`columns()`, which already return
the right answer for both kinds of selection with no additional dedicated
code.

The "#" itself selects the whole table on click (`AppState::
select_all`), like the corner cell of most spreadsheets: a
`SelectionKind::FullRows` selection covering `0..=row_count - 1` (`Selection::
all_rows`), which already touches all the visible columns without needing a
dedicated kind of selection — `is_all_selected` tests its exact coverage
to tint the "#" in return.

The gutter labels stay on a single line, even in French,
in pseudo-locale or after its width is reduced (#125). If room
is lacking, they are truncated with an ellipsis; hovering shows
the full text. They do not overflow onto the neighboring rows.

### A three-level palette for the gutter, and a tint of its own for its text

The gutter (row of letters, "#", row numbers, field labels)
used the same green as the rest of the brand accent, at full
saturation for its text (`Visuals::strong_text_color()`, which equals
`widgets.active.fg_stroke.color`, so literally `ACCENT`) — too
heavy for a zone meant to stay in the background of the data. `theme::
gutter_text_color` gives it a tint of its own, clearly darker,
independent of the global color of "active" elements so as not to
rub off on unrelated buttons or tabs.

The background, for its part, gains a third level. `app::grid::gutter_fill(selected,
touched)` chooses between `theme::header_letter_row_fill` (rest),
`header_letter_row_touched_fill` (the column or row of a selected data
cell, without itself being clicked) and
`header_letter_row_selected_fill` (selected directly) — the "your row/column is
concerned" marker of a spreadsheet, carried by `Selection::
touches_column`/`touches_row` (`AppState::column_touched_by_selection`/
`row_touched_by_selection`), written in terms of `columns()`/`row_range()`
so valid for **every** kind of selection with no additional dedicated
code. The 3 gutter rows that benefit from it:
`column_header_widget`, `row_number_widget`, and the "#" of
`row_number_header_widget` (`touched` always false for it — nothing
"touches" the whole table without selecting it entirely).

Finally, `ui.spacing_mut().item_spacing.y = 0.0` before building the table
and in the two vertical stackings of the header-side gutter
(`column_header_widget`, `row_number_header_widget`): without it, egui's default
spacing between stacked rows left a thin white line between
each cell painted with a flat fill, which made it look like it was floating rather than
really filling its cell.

### Tab in the last column: same gesture as Enter in the last row

Enter on the last row of a sheet adds a row and moves the
selection there (`Action::CommitCellAndAdvance` while editing, or the direct
keyboard shortcut outside editing) — Tab on the last column of a row of
data now does the same for columns rather than going back to the
start of the next row (the old behavior of `move_selection_tab`
at the edges).

The asymmetry between the two axes comes from the identity of the target: a row
is only an index, so selecting `(colonne existante, row + 1)` *before*
the add command is applied (it is applied only after the frame is rendered,
see `StokesForgeApp::ui`, `app.rs`) is safe — the row will exist in time.
A new column, on the other hand, has no `NodeId` yet until `Command::
AddColumn` has run: selecting it in advance makes no sense.
`Action::AddColumnAndSelect { sheet, row }` therefore does both at once
in its own handling (`AppState::add_column_and_select`): adds the
column, *then* — once `self.snapshot` is refreshed by `execute` — selects
`row` in the last visible column. Wired in the same two places as normal
Tab: the direct keyboard shortcut of `grid_contents`, and the editing
validation of `cell_widget` — `app::grid::is_last_visible_column` factors out the
condition common to both.

Also wired **on the header side** (`Action::AddColumnAndSelectHeaderField`,
`AppState::add_column_and_select_header_field`), for **each** of the four
fields (Long Name, Units, Comments, Formula) of the last column, while
editing as well as outside editing (#52): Tab adds a column there and selects the
same field in the new column. Until #52, only Formula did so — Tab
first went column by column for the same field; the user lifted
that decision, at the price of the automatic return to the next field of the first
column when filling in the header field by field.

### Tab and arrows belong to the grid (#38)

egui handles Tab and the arrows **at the start of the frame** (`Focus::begin_pass`)
to move its own focus from widget to widget, unless the focused widget
claims them through an `EventFilter`. The grid read them with
`key_pressed`, which removes nothing: egui gave the focus to the first
focusable widget it found (often outside the grid), and as soon as a widget had
the focus the shortcut guard (`egui_wants_keyboard_input`) cut the grid's
selection short.
Same while editing: the `TextEdit` of a cell or a header field
lost focus on Tab.

The rule, in `grid_contents` (so the same for the floating workbook and the
detached workbook, both through `book_contents`):

- **One focus anchor per grid** (`grid_focus_id`): a widget placed on the
  rectangle of the grid, focusable *only while it has the focus* (otherwise
  a Tab with no focus would enter it by itself), which carries the filter
  `GRID_KEYS` (Tab, arrows; not Esc, which gives the focus back). The grid
  claims it (`claim_grid_focus`) on a press or a release in its
  rectangle, and at the end of an edit that gave the focus to nobody.
  egui removes it by itself on a click elsewhere: Tab then takes back its
  usual role, without touching the selection.
- **While editing**, the cell and header fields set the same filter
  (`TextEdit::event_filter(GRID_KEYS)`): Tab no longer makes them lose
  the focus, they read it themselves (`tab_pressed`), validate, give the focus back
  and let the grid take it again.
- **Shortcut guard**: `focus_free` (no focus, or that of this
  grid). The rest of the application, which tested `focused().is_some()` to
  know whether one is typing in a field (Enter in a dialog, Del on an
  annotation), goes through `keyboard_focus_outside_grid`.

F2 opens the same editor as a double-click, with the initial content kept
(#143), only when the grid's anchor has the focus. On a range
of data or header fields, it edits the top-left cell, in the visible
order of the columns. Long Name, Units, Comments and Formula use
their existing editor; whole columns do not trigger this edit.
Renaming with F2 stays specific to the focused managers.

Limit: the filter applies only to the frame that follows the focus being taken
(`set_focus_lock_filter` requires `had_focus_last_frame`) — a key falling
in the very frame of the click is not a realistic keystroke. Locked by
the tests of `app/grid.rs` (`app::grid::tests`). egui's focus does not show in a capture
(`STOKESFORGE_APP_SCREENSHOT`): to be confirmed on a real window
(`run-stokesforge` on X11 helps, without being worth native Wayland).

### Body alignment: numbers right, text left

`cell_widget` aligns each cell of the **body** of the grid like a
spreadsheet — never the gutter or the header, which do not display a column
value. `AppState::cell_is_numeric` decides per cell, not per column:
on a `Mixed` column, two neighboring rows can align
differently, one to the right (typed number), the other to the left (text). A
missing value (`NaN` or empty/absent text) counts as non-numeric and
stays on the left — an empty cell has no reason to be pushed to the right, and that
avoids a sparse column looking like it jumps from one edge to the other
from one row to the next.

`Label::halign` alone is **not** enough here, despite what its doc
suggests: `ui.put` places the `Label` in a `Layout::centered_and_justified`,
which justifies the text (stretches it to fill the width) before `halign`
has a say — the result stayed visually centered, whatever
`halign` was. A real directional `Layout` (`right_to_left`/
`left_to_right`, `Align::Center` for the vertical axis), set via
`ui.scope_builder` on a slightly shrunken `max_rect` — the same mechanism
as the one already set on the whole table (`cell_layout` of the `TableBuilder`) —
really aligns the text against the wanted edge.

A right-click **outside** the selection first reduces it to the targeted cell, a
right-click **inside** it leaves it intact — so that the menu always acts on
what was just designated — same guard (`response.secondary_clicked() &&
!selected`) in the four zones. `column_header_widget` did not have it:
its right-click **always** reduced the range to the targeted letter, which
made it impossible to change the type or role of several selected
columns at once, even though the comment of its `target_columns`
already assumed the opposite. Fixed at the same time as the arrival of the
gutter, which made the gap visible.

Two deliberate differences between the menu and the keyboard:

- **"Delete and Shift Up" deletes right away**, whereas the Del key
  clears on the first press and deletes only on the second. The double
  press exists only to protect against a reflex keystroke on an
  easy-to-reach key; a context menu is an explicit, designated gesture.
- **"Paste" is never disabled.** egui exposes no API to *read* the
  clipboard (Ctrl+V works only because `egui-winit` reads the
  clipboard itself before pushing the `Event::Paste`), so `app::grid::
  system_clipboard_text` reads it directly via `arboard` — the library
  that `egui-winit` already uses internally. Knowing whether there is anything to
  paste would require this read on *every frame* where the menu is open: we
  try on click, and show a notice if it gives nothing. "Paste in Place", for its part, can be disabled at no cost — it replays the **internal**
  clipboard, whose state is known.

Finally, the five constants `SHORTCUT_COPY`/`SHORTCUT_CUT`/`SHORTCUT_PASTE`/
`SHORTCUT_COPY_FULL_COLUMNS`/`SHORTCUT_PASTE_ABSOLUTE` are **purely
decorative**: they exist only to write the shortcut reminder on the
right of a menu entry. Nothing consumes them, and one must not try —
`egui-winit` intercepts Ctrl+C/X/V when translating the window events
and never lets an `Event::Key` through (an earlier attempt,
based on the opposite assumption, produced shortcuts that never
fired).

### Accepted limits of the bench

- **Shortcuts are consumed at the context level**, so *before* the
  widgets. Ctrl+Z while typing in a cell undoes the last **model
  command**, not the last keystroke: this is the expected behavior of a
  spreadsheet, and it is also the limit — the bench cannot tell the two
  intentions apart.
- **Only one editable cell at a time**, across all **docked** grids
  (`AppState::editing`). The grid is virtualized
  (`TableBuilder::body().rows()`): a sheet can have a million
  rows, there is no question of keeping a buffer per cell. The same
  principle holds for editing a note (`AppState::NoteEdit`), for the same
  reason of consistency rather than strict necessity. A **detached** tab
  has its own buffer, independent of that of the dock and of the other detached
  tabs (see "Generic detaching of content tabs") —
  a technical constraint of the `'static` closure of `show_viewport_deferred`,
  not a deliberate extension of the limit.
- **Open windows and their mode are not restored**: startup
  opens the first workbook and the first graph. The saved positions
  and sizes are reused for the contents, but maximization and
  detachment are not a persisted layout plan.

### A detail that looks like a detail

`MISSING_TEXT_DISPLAY` is `\u{00A0}` — a non-breaking space — to display an
**absent** text value. It fills the cell like an ordinary blank (nothing
catches the eye) while remaining, in memory, a string different from `""`, so
distinct from a **present** empty string. A visible character (`—`, `∅`, `?`)
would have been a legitimate answer to "what might a user want to
type here?"; a non-breaking space is not reachable by typing on any usual
keyboard.

It holds only for **text** columns, where a present empty string exists
and must stay indistinguishable to the eye. A **mixed** column carries
the information in the type of its cell: `cell_text_in` reads it through
`Column::cell` (and not `Column::get`, which would confuse the two cases) and displays
an empty cell like an empty numeric cell, `""`.

### Typing what is not a number

A non-numeric input in a numeric column **widens the column**
rather than being refused: `commit_cell` sends `Command::SetCellsPromoting`,
the column becomes `ColumnType::Mixed`, the cell holds the text, the others
keep their numbers ([`commandes-et-historique.md`](../commandes-et-historique.md)).
Conversion and write fit in a single history entry: one
`Ctrl+Z` brings back the type *and* the previous value.

Before mixed-type columns, the input was refused with an error
message, and noting an "n/a" on one row forced converting the whole
column to text — thus making all the rest unplottable. On an already
mixed column, nothing is promoted any more: each cell decides for itself, and
a typed number stays a number there.

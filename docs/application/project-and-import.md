# Application: new project, file drop, import screen

Continuation of the index [`README.md`](README.md). Contents: new project and empty graph, file drag and drop and its backend, the import screen.

### New project and empty graph (#73)

**File › New › New Project** ("File › New Project", Ctrl+N) replaces the
current project in the same window. The File bar offers the same gesture.
A project with unsaved work, including an entry still being typed (committed first, see below), a refused entry
or a text edit left open, asks "Discard Changes · Cancel · Save" (#229; Enter = Save, which
saves then continues). Cancel, Esc or the close cross keep the project.
Entries still open in the main window (cell, header field, note) are committed
first, by `AppState` itself (`commit_open_entries`, called by `save` and
`request_unsaved`, #465, #466), through their usual undoable commands: the
file holds what was typed, and Cancel leaves the value committed (Ctrl+Z
undoes it). A refused entry (a number out of range) stays open and is left out
of the save. Autosave and the quit after a lost session do not commit.
The entries open in detached windows (their own `view_state`) are committed
by `StokesForgeApp::commit_view_entries` before Save, Save As, New Project,
Open and Quit, then the main state refreshes. The same call closes the graph
text editors (titles, legends, group names, free texts: egui-side, in egui temp
data) of the floating and detached graphs and sends their usual undoable
actions (`graph_text_edit::commit_open_text_editors`); they no longer raise an
Unsaved Changes question of their own. A click elsewhere that opens a
blocking dialog (row drop, export overwrite, alert) ends a cell or note entry
through the state, so the frame's `actions.clear()` cannot drop it (T1).
File › Open…, File › Quit and closing the window ask the same question
(`Action::OpenProject`, `Action::Quit`, `app/confirmations.rs`). An unreadable
project or a failed save is announced by a blocking dialog (S-M8,
`AppState::alert`) and keeps the current project and path. The marker is
conservative: any revision since the save, including an undo, or a modified
geometry asks for confirmation. A command that is applied counts as a
modification **even during a long recalculation**, before the snapshot is read
again (`AppState::applied_revision`, #239): closing or opening asks the
question. It keeps no old tree in memory.
A new project that is still empty asks for no confirmation.

If the session actor dies (panic, #310), `AppState::check_session` (called at
each frame by `poll_recalc` and by `refresh`) detects it only once: the
recalculation indicator is dropped, a backup copy of the last snapshot is
written in `recovery/` of the data folder (never over the user's `.stf`, and it
may predate the user's last command), and a blocking dialog
(`AlertKind::SessionLost`) leaves only Close, which closes the application.
Quit then asks nothing.

**Autosave and recovery (#317)** — `state/autosave.rs`, `recovery.rs`. On by
default, every 5 minutes (preferences `autosave`, `autosave_minutes`, bounds 1
to 60; File › Preferences › General, Files section).
`AppState::poll_autosave(now)`, called by `logic()` of the root window, writes
the project through `SessionHandle::save` into **a single file per process** of
`recovery/` (`<nom|Untitled>-<pid>.stf`, overwritten, with its `.json` card
and its `.lock` lock), only if the project has changed since the last copy, the
last save or the start. Neither the history, nor `saved_revision`, nor the
path, nor the "modified" title moves, and the user's `.stf` is never written.
The deadline is skipped (and starts again 5 s later) during a macro, a script,
an import or a recalculation; a write failure gives a single warning. The copy
is deleted on save, on clean close, and for New, Open of another project and
Discard; a lost session keeps it. At startup (`main`, root window),
`scan_recovery` lists the copies with no held lock and newer than their `.stf`
(including the copies from #310) in the recovery window (Open, Ignore, Delete
with confirmation; `docs/windows.md`). Open loads into the current window
(behind "Save?" if there is work), under the original path (empty for
Untitled), as unsaved work, and the list stays open. Ignore leaves the copy,
offered again at the next startup.

The replacement goes through `SessionHandle::new_project`, empties the history
and the geometries and changes the epoch to invalidate the old Python
references. The content windows, caches and selections of the previous project
are forgotten, and its detached windows close. The visibility of the bars and
the language are kept. The gesture is disabled during a script, an import in
progress or an open macro. The first **Save** ("Save") asks for a
destination, then the following ones use that file.

**File › New › Add Graph** ("File › Add Graph") and the File bar
create an empty graph in the folder of the Project Manager selection (#425; the root when nothing or several rows are selected): one plot area, no series or title.
Its window opens in the foreground. It goes through the existing `AddGraph`
command: Undo removes it, Redo brings back the same identity, and the `.stf`
keeps it. This gesture has no extra shortcut; it stays reachable from the menu.

### File drag and drop

Dropping files on the main window emits `Action::DropFiles`. The hover shows
through the application's **single** drop stroke, `theme::drop_target_stroke`
(light blue, S-X5, #386), the same one as the graph hovered by columns or rows,
painted on the **border of the one target window**. The workbook under the cursor
claims the drop (`app::file_drop::claim_file_drop`) and records its layer in the
frame's marker (`file_drop::set_drop_target`): its window frame, or the
maximized area, or the detached viewport, paints the stroke in place of the
active outline. Without a claim, `finish_file_drop` outlines the application.
The claimed workbook it receives one sheet per file, otherwise
a new workbook is created. `.stf` files are never imported: `AppState` queues
them (`take_projects_to_open`) and `app.rs` launches a second process
(`file_drop::open_project_in_new_window`)
`stokesforge-app <chemin>`. Each file is read with the recipe that
`stokesforge_import::recipe_for_file` picks (a saved template whose conditions
the file meets, else the detector, #488): the same entry point as the import
screen, the CLI and `pyst.import_file`. Reading, template choice and analysis
happen on the background thread of the menu import (#266): `drop_files` keeps
on the interface thread only the refusals and the queuing of `.stf` files,
then goes through `start_import`, the single entry point of import (indicator
and Stop as for File › Import; a drop during an import in progress is
refused). When the thread returns, `poll_import` decides: if **no** file needs
a look, it applies the result — a single "Import" entry — and posts the
message of the templates used; if **any** file has a point left to check (a
doubt not answered yet for that kind of file), a refusal or a table that does
not read, nothing is imported and the import screen opens on **all** the
dropped files, each with the recipe chosen for it. An answer given earlier
(Mark as Checked, a correction) is remembered per kind of file, so the same
doubt does not open the window for the next file of that kind
(`import_check.rs`, `RecipeStore::remember_answer`). Pure decision:
`state::file_drop_plan`. The drop always creates new sheets, even if the recipe
of the last import replaced or merged a sheet. The File › Import › Reopen Last
Import… link reopens every file of the last import, each with its own recipe.

The same gesture is wired in the detached windows: on a workbook, the
destination stays that workbook; on a graph, a note, a matrix or the
Project Manager, a new workbook. Drops are isolated per viewport and their
actions go back to the main session. A drop is refused as long as the graph
properties are open, to keep their history macro independent of the import; if
they open while the thread is reading, `poll_import` keeps the result and
applies it after they close. The drop events are tested without a window. On Windows winit
gives no pointer position during a drag from Explorer, so `begin_file_drop`
reads the real cursor with `GetCursorPos` (converted to window points,
#383) and `claim_file_drop` tests that position instead of egui's pointer;
confirmed by the user on Windows on 2026-10-29 (drop on a workbook, outline
following the cursor, detached workbook; before #386, which moved the outline onto the target window's border). The real drop from Files on native Wayland was confirmed by the user on 2026-10-02;
the detached OS windows and the other cases of the protocol below remain to be
validated manually.

### Import wizard

"Import…" opens **one screen that creates** (`import_window`, module
`src/app/import_wizard.rs`, the only entry point called by `app.rs`; window
title "Import window"), a **modal dialog of the common building block**
(`modal_dialog_with`, `Draft`, free size 1320 x 840 shrunk to the screen). It
replaced the six-step wizard (files, format, header and end of data, columns,
metadata, destination): nothing of those steps is left, and there is no
fallback to them. The module `//!` docs carry the design; this page keeps the
map.

Layout, from top to bottom: the **file tabs** (one per file; a strip that stays
usable with about 50 files: filter, previous / next, "+ n more"); at the left,
the **left panel** (35 % of the width, 380 points at least) and, at the right,
the **grid** with its File / Result tabs; under them the line "You get"; the
footer with the **Destination** and **Template** menus, Cancel and Import.
Import is never greyed out by a doubt: the footer only says how many points are
left to check.

- **The detector reads every file once** (`stokesforge_import::sniff`, state
  in `state/import_auto.rs`) and proposes the whole configuration: encoding,
  delimiter, decimal, where the data start and stop, the x' line, column names,
  units and titles written as `KEY;value` lines. A saved template whose
  conditions the file meets is applied at once and its name shown ("Auto ·
  name"); when two meet as many conditions, one is used and a point to check
  offers the other. A correction made on one file is kept per **kind** of file
  (instrument fingerprint), so it does not leak to a file of another kind.
- **The left panel** (`grid/panel.rs`, decisions in `state/import_check.rs`):
  the format sentence (delimiter, decimal, encoding), the **To check** block, **What was found** (rows grouped Data /
  Columns / Sheet, each with the source line, a long value cut with an ellipsis
  and its whole text on hover), and the **Advanced** fold, closed by default
  (date order, leading characters removed, blocks, missing values, comment
  prefix, skipped data lines: `state/import_advanced.rs`; a setting not at its
  default also reads as a sentence above the fold). Nothing of the panel is
  ever cut at its edge: a long text wraps, and "Mark as Checked" moves under
  its sentence when the sentence would get too narrow.
- **To check** (`state/import_tocheck.rs`): the findings that are doubts, the
  detector's own doubts (text rows inside the data, several blocks of numbers,
  a comma that may be a decimal, a JCAMP-DX problem) and a tie between two
  templates. A sentence shows where it comes from in the file (it settles
  nothing). **Mark as Checked** answers one point without touching the recipe;
  **Check Now** walks through the doubts one at a time. An answer is remembered
  for that kind of file in the template store and listed under **Answers
  remembered** in Manage Templates (**Forget** brings the doubt back). The
  detector never picks a block silently: several blocks are read whole with
  their titles listed, split into one table each, or flagged.
- **The grid** (`grid.rs`, `state/import_grid.rs`): painted by hand, only the
  visible rows and columns are laid out (a kinetic of 630 lines x 241 columns
  stays smooth). Clicking a **line number** (or a value) gives the line a role
  (Long Name, Units, x', data start / end); column letters give a column a role
  or read it as text; the zones are colored; the data boundaries are handles.
  Each decision is a `GridCmd` applied to the recipe by
  `AppState::import_grid_cmd`. The **File** tab shows the raw file, the
  **Result** tab the worksheet that will be created. An "auto" pill takes an
  automatic choice back; it never deletes the choice by itself.
- **The first-use guide** (`grid/guide.rs`, `import_guide_*`): three short
  steps over the grid, shown every time until **Don't show again** is ticked
  (stored in the preferences); **Show Guide** brings it back.
- **Templates** (`templates.rs`, `state/import_template.rs`): after a first
  import of a kind of file, the screen offers "Save these settings for JASCO
  files?". **Save as Template…** proposes a name and shows **what triggers the
  template** (conditions, each can be unchecked: the instrument, the data type,
  the marker the data come after, the numbers; the user adds the number of
  columns, a header line equal to / containing a value, the file name, the
  extension). **Manage Templates…** lists the templates with their conditions,
  the answers remembered and Forget Template (with a confirmation). The
  Template menu of the footer lists them too (Auto first). One template store
  (`import_recipes.json`, `RecipeStore`): the templates saved before #488 are
  kept, their Auto-select criteria became conditions.
- **Texts**: `import-*` keys of the `en` catalog written by `docs/ui-texts.md`
  § 2.4; template names and file contents stay data, never translated.

The file bytes are read once (cache by path, `ImportState::bytes`, the
`FileMap` cached while neither the file nor the recipe changes); each file
shows before the import its lines read or its error. A file the reader refuses
(binary, empty, no numbers, NMR) is marked on its tab and skipped; it never
stops the others. The header is a single setting (`ImportRecipe::unify_header`
brings `HeaderSkip::Lines` back to the structured split); the column roles
live in the recipe (`column_roles`). A file drop while the screen is open adds
the files to its list. *Import* launches `Action::StartImportEach`: the
analysis of the files runs on a background thread (with progress "Importing…
k of n", `AppState::poll_import` called at each frame) and the sheets are
created at the end, in a single "Import" entry.
Shortcut: **Ctrl+"** (`SHORTCUT_IMPORT`: the key of the 3 without Shift on
AZERTY, `SHORTCUT_IMPORT_QWERTY`: Ctrl+Shift+' on QWERTY). Recent files and
remembered folder: `stokesforge_import::RecentFiles` (`import_recent.json`, next
to the recipes). "File › Import › Reopen Last Import…" (`Action::ReopenLastImport`)
reopens the screen on the files of the last import, each with its recipe —
undoing it first if it is still at the top of the history, never later work.

**What the engine reads.** `stokesforge-import` is split as follows (details
in each module's `//!`): `sniff` (the detector, pure, returns data and
`Doubt`s, never sentences), `instrument` (fingerprints from the content, never
the extension), `jcamp` (JCAMP-DX tables in every ASDF form, `(XY..XY)` pairs,
LINK blocks, simple NTUPLES; **NMR is out of scope**: an NMR file is refused
with a clear card, an NMR block of a mixed file is left out and flagged),
`thousands` (strict guess of a grouping character), `props` (`KEY;value` lines
that fill Long Name, Units or Comments of the columns; a sheet takes the name
of a `TITLE` key), `templates` (conditions and `recipe_for_file`, the one entry
point). `recipe_for_file` picks, in order, the named template, a matching
template, then the detector; the application, the CLI (`import` skips an
unreadable file and lists it) and `pyst.import_file` all call it.

The "modified" marker of the template compares the settings with the loaded
template or the last successful save (`AppState::import_recipe_modified`).
Saving the template removes this marker; a write failure keeps the previous
reference and reports the error.

**Where the columns go.** In the active workbook, the import mode
(`ImportMode`) is: new sheet, replace the active sheet, add columns, or add rows
(column *j* of the file extends column *j* of the sheet). Several files are
spread according to `FilesLayout`: one sheet per file, a single sheet side by
side (X Y X Y…), or a single sheet with the X of the first file shared. The plan
(`plan_import`) is computed then executed in the same "Import" macro. By
default, the first column of each file gets the X role and the following ones
the Y role; these roles are kept when the import is redone after an undo.

**End of data.** By default there is no cut (the data go to the end of the
file): the automatic cut silently lost the lines that followed a missing
measurement ("n/a") in X. It remains a choice of the recipe, and the Jasco
templates ask for it. It arms at the first numeric X, then cuts at the first
non-numeric X (empty line included). The text and empty lines that precede that
first number are kept; text in Y does not cut. The `skip_chars` prefix is
removed before this test; with the Whitespace delimiter, X is the first non-empty
token. The recipe also offers "No cut",
first empty line, marker, N lines after the header or N lines before the end.
On the import screen the cut is the pair of handles of the grid; the preview
and the import use the same split (`stokesforge_import::layout`, `data_end`).

**Number format, dates and encoding (#314).** A "Whitespace" delimiter (chosen, or
resolved from Auto) with the space as thousands separator is refused, in the
preview (message in the window) as in the import: `1 234,5` would be split into
two columns; `;` and tab accept these thousands. The "Date order" setting
(day first by default, or month first) settles `03/04/2026`; ISO does not
change. With Auto encoding, an almost-UTF-8 file is decoded in a mixed way:
valid UTF-8 sequences are kept, each invalid byte is read as Windows-1252; a
Windows-1252 file in which two bytes happen to form a UTF-8 sequence (`Ã©`) is
read as `é` — force the encoding in this case.

**Several blocks.** `BlockSplit` splits a file that chains several spectra (at
each line starting with a text such as `TITLE`, or at empty lines):
`parse_blocks` returns one table per block, each with its header, and the import
creates one sheet per block (`fichier_1`, `fichier_2`…) or spreads them
according to `FilesLayout`. The preview shows the first block and the number of
blocks.

**Refresh.** Each imported column keeps its source
(`ColumnMeta::import_source`: path, recipe as JSON in the template, column
index; in the `.stf` file, the recipe has its own types, `docs/persistence.md`).
"File › Import › Refresh from Source" (`Action::RefreshFromSource`)
rereads the files **with the stored recipe, without running the detector**, and rewrites the values **in place**. It is not promised to reproduce an import from one version to the next (a newer reader may read the same recipe differently; `docs/persistence.md`). The columns keep their
identity, the graphs stay valid. The rows of the sheet follow the longest of the
new contents; they shrink only if all the columns are refreshed.

### Drag and drop: the window backend

On Linux, winit now chooses **native Wayland by default** when it is available
(#72). `STOKESFORGE_X11=1` forces X11/XWayland as a fallback.
`STOKESFORGE_WAYLAND=1` is no longer needed. eframe/egui-winit stay at
0.36.2; the Cargo patches select winit 0.30.13 and smithay-clipboard
0.7.3 in `vendor/` while waiting for stable upstream support.

The transitional module receives `text/uri-list` on **the same receiver as the
clipboard**. This worker is shared by all viewports: creating a second receiver
steals the drop from the first on Mutter, as the first prototype showed. A weak
channel hands winit the paths and the position, with no Wayland object and no
new thread. It emits the usual events for each native window, so the drop path
of the application and of the detached windows stays common. The pipe is read
without blocking wait, with a URI list limited to 1 MiB and a timeout of 10
seconds. The position at release stays the one used to choose the workbook. See
[`vendor/winit/STOKESFORGE.md`](../../vendor/winit/STOKESFORGE.md) for the
technical choice, the alternatives, the manual GNOME/KDE/wlroots protocol and
the removal on moving to winit 0.31.

The fault of the first prototype was reproduced with a real drop from
Files on GNOME. After the fix, the user confirms that the drop works on native
Wayland on 2026-10-02.
**The detached windows and KDE/wlroots remain to be validated manually**;
the automatic tests cover the parser, the channel, its lifetime and the import
decision, not the compositor.

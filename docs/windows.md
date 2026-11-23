# Catalog of windows and dialogs (#300)

Inventory of **all** the windows of `stokesforge-app`: what exists, of
which type, with which building block, which size, which pages, where it is opened from.
It is a starting point before creating a new window: find the
twin, open it, imitate it. The **behavior** (button order,
Esc, focus, disabled state…) is in [`interface-standard.md`](interface-standard.md),
§ Windows and dialogs and § Confirmations; this page does not copy it.

Maintenance rules:

- **E1: this catalog is updated with every window.** Any window or
  dialog added, removed, resized or whose pages change updates its
  row in the same commit.
- **E2: mockup approved by the user before any code for a new
  window.** Always, with no exception for size. The mockup is presented as
  a decision (decision form,
  `.claude/skills/stokesforge-bug-report/references/decision-form.md`);
  code starts only after agreement.

The code is authoritative: the sizes below are the **default** sizes
passed to the building block; the size and position used afterwards are those
the user left (S-F8, see below); a small question or alert dialog always opens
centred on the whole window and keeps only its size (#562). Two guards hold this
page (S-F11): `the_window_catalog_and_the_code_agree` (every
`stokesforge_…` identifier passed to a `modal_*` appears here, and every row has its
call; **it does not read the sizes**) and `no_free_size_without_justification`.

## Natures and size classes (S-F11)

A modal window has a **nature** (`DialogNature`, `app/dialogs.rs`): it
fixes the footer, the default size, the initial focus and the default button.
Every `match` on the nature is exhaustive, without `_`: a new nature does not
compile until it has stated them. The size is a **field** of the variant,
whose default value is a class:

| Class | Size | For |
|---|---|---|
| Small | 480 x 170 | a question, a message, a few buttons |
| Medium | 480 x 420 | settings or a form with a few pages |
| Large | 640 x 500 | a list and its editor side by side |

- **Interactive window** (settings, live settings, draft, wizard: `InteractiveSize`):
  the default class of the nature, another class, or a **free size**
  (`InteractiveSize::Free`) that requires `// FREE-SIZE(#300): <justification>`
  in the twelve lines before it (15 characters at least). Free sizes
  today: the analysis (1000 x 660), Apply Theme (700 x 560), the import window (1000 x 620) and the
  `preferences_proto` bench (560 x 470, reference screenshots).
- **Window without rich interaction** (choice, read-only, confirmation,
  save, alert: `PlainSize`): may take the **size of its
  content** (`PlainSize::Content`) **without justification** — there is no form to
  lay out, a badly chosen size costs little there. The type does not offer it to
  interactive windows.

| Nature | Building block | Default class | Initial focus |
|---|---|---|---|
| `Settings` | `modal_dialog` | Medium | first field (S-F9) |
| `Draft` | `modal_dialog_with` | Medium | first field |
| `Choice` | `modal_dialog_with` | Small | first field (if there is one) |
| `Wizard` | `modal_wizard` | Large | first field |
| `LiveSettings` | `modal_live_settings` | Medium | first field (S-F9) |
| `ReadOnly` | `modal_read_only` | Medium | none |
| `Confirm` | `modal_confirm` | Small | Cancel (S-F5) |
| `SaveChanges` | `modal_save_changes` | Small | Save |
| `Alert` | `modal_alert` | Small | Close |

`modal_dialog_with` takes the nature (settings, draft or choice); the other
building blocks have a fixed nature and take the size field of that nature.
The main button of a footer is either **always enabled**
(`DialogFooter::always(libellé, bulle)`), or **can be disabled**
(`DialogFooter::gated(libellé, bulle, actif, raison)`: tooltip and reason
mandatory, S-F10).

## The types

| Type | Building block (`app/dialogs.rs`) | Nature | Footer | When |
|---|---|---|---|---|
| **Live settings** (S-F1) | `modal_dialog` | `Settings` | `[Cancel] [OK]` | a visible object is adjusted, OK keeps, Cancel restores |
| **Creation on a draft** (S-F1) | `modal_dialog_with` | `Draft` | `[secondaires] [message] [Cancel] [verbe]` | an action that creates; the button carries its verb (Create, Import) |
| **Choice** | `modal_dialog_with` | `Choice` | same, secondary = other answers | a question with no "yes" answer |
| **Wizard** | `modal_wizard` | `Wizard` | `[Cancel] [message] [Back · Next] [Finish]` | several ordered steps |
| **Settings applied live** (S-F2) | `modal_live_settings` | `LiveSettings` | `[message] [Close]` | adjust with nothing to confirm or cancel: read-only footer, but with fields (focus, size by class) |
| **Read-only** (S-F2) | `modal_read_only` | `ReadOnly` | `[secondaires] [message] [Close]` | view only |
| **Confirmation** (S-F3 to S-F5) | `modal_confirm` | `Confirm` | `[message] [Cancel] [verbe]` | only what Ctrl+Z cannot recover |
| **Save?** | `modal_save_changes` | `SaveChanges` | `[Discard Changes] [message] [Cancel] [Save]` | unsaved project |
| **Serious failure** (S-M8) | `modal_alert` | `Alert` | `[action] [message] [Close]` | failure with a possible action |
| **Content window** (non-modal) | `window_chrome::show_floating_window` | none | none | workbook, graph, matrix, note, Script Settings |

The first five building block rows all go through the same body
`modal_frame` (same frame, same title bar, same blocking of the rest of
the interface); the content window goes through `show_window_in_layer`, an
`egui::Area` and not an `egui::Window`. A raw `egui::Window` is forbidden
(guard `no_window_is_a_raw_egui_window`).

## Modal dialogs

All are painted in `StokesForgeApp::show_edit_dialogs` (`app.rs`), as long as
their state is open in `AppState`, except the blocking ones (last section).
They are always painted by the root, whatever window the gesture came from
(#472). See "Where a dialog is painted" below.

| Window | Nature | Size | Pages or content | Opened from | Identifier, file |
|---|---|---|---|---|---|
| Graph properties | `Settings` | Medium | 5 tabs (`tab_strip`): Graph (General: also Text color and Axes color, #542), Plot Area, Axes, Legend (its last section, a folded « Color bar », is `color_bar_settings.rs`, #499, #510: Show color bar, then four blocks: Groups (table: on the bar, gradient, range), Breaks (two groups or more), Value, Look), Theme (the theme manager, #542: the picker scrolls, the controls stay at the foot: Save Theme As…, Rename…, Update From This Graph, Reset, Delete, Use for new graphs, Apply Theme… into the draft; the Apply Theme, Save Theme As, Rename Theme windows and the delete confirmation open over it); on Axes, the sides Bottom, Left, Right, Top are always the four vertical tabs at the left (fixed strip, the page of the axis scrolls, S-L9); a side held by another layer of the cell is greyed with its reason, an overlaid layer edits its X axis on Top and its Y axis on Right with an "Own scale" box (#396, `cell_sides`) | right-click on a graph component, Graph menu, Properties button of the toolbar (`Action::OpenPlotSettings`) | `stokesforge_plot_settings` (built), `plot_settings.rs` |
| Series properties | `Settings` | Large | list of series and groups on the left; on the right the page the selection chooses (#510): the group page (name, gradient, line and symbol of all curves) or the curve page (`curve_settings_page`) | double-click on a curve, Object Manager, Graph menu (`Action::OpenObjectProperties` on a series or a group) | `stokesforge_curve_settings` (built), `plot_settings.rs` (window), `curve_settings.rs` (page) |
| Properties of an object (rectangle, arrow…) | `Settings` | Medium | coordinates X1 Y1 X2 Y2, line and fill colors | double-click on the object, Object Manager (`open_shape_properties`) | derived from the object, `graph_window.rs` |
| Analysis (new or edit) | `Draft` | free 1000 x 660 | two columns: settings (330 wide, scrolling) and clickable preview with tools and values | Analysis menu, right-click, results panel (`Action::OpenAnalysis`) | `stokesforge_analysis` (built), `analysis_dialog.rs`, `analysis_forms.rs`, `analysis_preview.rs` |
| Import | `Draft` | free 1320 x 840 (a smaller screen shrinks it) | one screen that creates (#488): file tabs, the format sentence, the points to check (Check Now, Mark as Checked), what was found, the Advanced fold (closed by default) at the left; the file as a grid with its File / Result tabs at the right, a short guide (Show Guide) over the grid; footer To:, Folder: and Template menus (To: and Folder: are painted by `import_to.rs`, the same painter as Import from Origin; Folder: is greyed out for Active Workbook, #537), Cancel, Import (never greyed out by a doubt: a warning in the footer; greyed with its reason for a bad new-folder name). Three windows nested over it: Save as Template (`Draft`, default size), Import Templates (`LiveSettings`, Large: templates, their conditions, remembered answers) and the confirmation of Forget Template (`Confirm`) | File > Import, Ctrl+3 (`open_import_dialog`), a file dropped on the window | `stokesforge_import_wizard`, `stokesforge_import_template_save`, `stokesforge_import_templates`, `stokesforge_import_template_forget`; `import_wizard.rs`, `import_wizard/templates.rs`, `import_to.rs` |
| Import from Origin | `Draft` | Large | a line To: / Folder: above the tree (the menus of the Import window, `import_to.rs`: One Workbook per Origin Workbook or New Workbook, the three sheet layouts, the project folders and New Folder…); scrolling tree file, workbooks (collapsible, three-state box), sheets (collapsed by default, three-state box and "N columns, M rows"), columns of an open sheet (box, X/Y role, long name, unit; Shift+click range, right-click Check/Uncheck in All Sheets; only the rows on screen are painted); footer Check All, Check None, Cancel, Import (greyed out with its reason while nothing is checked or the new folder has no usable name); orange line for the sheets that keep their own X; summary line under the list; shown only while the Import window is closed (#490) | an Origin file (extension `.opj`/`.opju`, or the `CPYA`/`CPYUA` signature in the first bytes, so a renamed file counts) dropped, File > Import or Add Files of the Import window (read in the background, `AppState::origin_import_open`); Reopen Last Import after an Origin import | `stokesforge_origin_import`, `origin_import.rs` |
| Preferences | `LiveSettings` (immediate settings) | Large | 4 rubrics as tabs on top (`TabStripSide::Top`): General (language, maximum file size at opening, autosave: checkbox and period in minutes, disabled if the checkbox is unchecked, #317; analysis: default of Values Show / Keep in Results, low R2 warning), Display with vertical sub-tabs at the left (S-L9) Interface (formula syntax help icon) · Object Manager (lists, expand-by-default groups, Reset) · New (rows of new workbooks, decimal comma of new graphs, Default theme: None, the built-ins or a user theme, the same setting as Use for new graphs of the Theme tab, #542, legend text from, with a Template row under Custom: the Legend tab's own field and validation, committed when left, Esc gives the saved template back, #502) · Graphs (group Automatic axis range: Left, Right, Bottom, Top margins in %, a user preference read when a graph writes its ranges, #529), Export (SVG text, PNG resolution, Copy Graph formats), Folders (the configuration folder with Open Folder and its files, the recovery copies folder, and the default folders Open and Save, Import, Export, each typed or picked with Browse…, with a "Folder not found" note #148) | File > Preferences (`Action::OpenPreferences`) | `stokesforge_preferences_rubrics`, `preferences_window.rs` |
| Apply Theme | `Draft` | free 700 x 560 (the picker's three rows of tiles, the boxes under them and a preview beside them) | creation on a draft: `[Cancel] [Apply]`, the verb is Apply, one undo step on every target. Left: the theme picker (`theme_picker.rs`: rows Project and Mine, tiles that draw the specimen of the theme, 2 columns x 3 rows per row group, then the drop-down More themes, the large view on hover) then « What to apply » (Style with its six parts under a fold, Size, Texts, Axis ranges, Add missing layers; each with its « ? »); right: the preview of the first target with the theme applied (real data, natural size scaled whole), Before under the pointer, « Preview of the first of N graphs ». Hovering a tile or a box previews it, only a click changes the choice. Apply is greyed with its reason while no box is ticked. State (targets, chosen theme, ticked families, hovered override) in `AppState::apply_theme_window` | right-click on a graph or on graphs of the Project Manager, Graph menu (`Action::OpenApplyTheme`) | `stokesforge_apply_theme`, `apply_theme_window.rs`, `theme_picker.rs` |
| Save Theme As | `Draft` | Small | creation on a draft: `[Cancel] [Save]`; the name, and « Save in » Mine (default) or Project through a `state_toggle`; Save greyed while the name is empty; a refused name (taken, library full) is said in the footer and the window stays open; Mine is forced to Project, with its reason, while the themes file is read-only (written by a newer version). State in `AppState::save_theme_window` | Graph menu (`Action::OpenSaveTheme`) | `stokesforge_save_theme`, `save_theme_window.rs` |
| Rename Theme | `Draft` | Small | creation on a draft, twin of Save Theme As: `[Cancel] [Rename]`, the name prefilled with the current one; Rename greyed while the name is empty; a refused name (taken, empty) is said in the footer and the window stays open. State in `AppState::rename_theme_window` | Theme tab of Graph Properties (`Action::OpenRenameTheme`) | `stokesforge_rename_theme`, `rename_theme_window.rs` |
| Delete Theme (confirmation) | `Confirm` | Small | `[Cancel] [Delete]`, Cancel default (S-R4): only for a user theme, whose library has no history; a project theme is undoable and asks nothing. State in `AppState::pending_theme_delete`; painted before Graph Properties so that its Escape cancels only the question | Theme tab (`Action::DeleteTheme`) | `stokesforge_delete_theme`, `confirmations.rs` |
| Keyboard shortcuts | `ReadOnly` | content 480 x 460 | scrolling list by area, generated from `shortcuts::HELP` | Help > Keyboard Shortcuts (`Action::OpenKeyboardShortcuts`) | `stokesforge_keyboard_shortcuts`, `keyboard_shortcuts_window.rs` |
| Formula Functions | `ReadOnly` | content 900 x 560 (wide: each description on one to three lines) | the functions of an `F(x)=` formula, one family at a time (#402): families as fixed vertical tabs at the left (`TabStripSide::Left`, S-L9), at the right a scrolling table Function · Example · What it does; the intro line on top; state (open, chosen family) in `AppState` | the "?" of the `F(x)=` cell while it is edited (`Action::OpenFormulaFunctions`) | `stokesforge_formula_functions`, `formula_functions_window.rs` |
| About | `ReadOnly` | content 360 x 140, 360 x 252 with the logo | the logo (96 pt, centred, only in an official build, #388), version, license, project page link, Copy button (version, system and architecture line for bug reports) | Help > About StokesForge (`Action::OpenAbout`) | `stokesforge_about`, `about_window.rs` |
| Status bar history | `ReadOnly` | content 520 x 360 | the last 50 messages, empty state if none | click on the status line (`Action::OpenStatusHistory`) | `stokesforge_status_history`, `status_history.rs` |
| List of low R² | `ReadOnly` | Small (480 x 170) | counters by type, normal text | status bar badge (`Action::OpenLowR2List`) | `stokesforge_low_r2_list`, `status_history.rs` |
| Column metadata | `ReadOnly` | Medium | scrolling key / value grid, "Use as x'" button per numeric row | click on the metadata cell of a column header, when it is not empty (`Action::ViewColumnMetadata`) | `stokesforge_column_metadata`, `plot_settings.rs` |
| Deleting a calculation | `Choice` | Small | Delete, Delete with Columns, Cancel | deleting a calculation (`Action::RequestDeleteCalculation`, results panel or analysis menu) | `stokesforge_delete_calculation`, `analysis_dialog.rs` |
| Export of stale series | `Choice` | content 560 x 170 | Export Anyway, Export Without Warning, Recalculate and Export, Cancel (four long labels: wider than Small) | export or copy of a graph that shows an out-of-date calculation | `stokesforge_stale_export`, `stale_export.rs` |
| Column used by a calculation | `ReadOnly` | Small (480 x 170) | Undo, Close; the list of formula columns and calculations that read what was just deleted | deleting columns, a sheet or a workbook read by a calculation (#306), after the gesture | `stokesforge_column_use`, `column_use.rs` |
| Copy with header of formula columns | `Choice` | Small | Values, Formulas (default), Cancel | Ctrl+Shift+C or Copy with Header when a copied column carries a formula (#306) | `stokesforge_full_copy`, `column_use.rs` |
| Project recovery | `ReadOnly` | content 560 x 230 | one row per backup copy (name or Untitled, UTC date) with Open, Ignore, Delete; introduction message; scrolling list | at startup, if copies from vanished processes remain that are newer than their `.stf` (`AppState::scan_recovery`, #317); closes by itself when the list is empty | `stokesforge_recovery`, `recovery.rs` |
| Replacing a shared analysis | `Confirm` | Small | a message | applying an analysis whose source feeds other curves | `stokesforge_replace_analysis`, `analysis_dialog.rs` |
| Refreshing a modified transposed sheet | `Confirm` | Small | a message | Relink (band of the linked transposed sheet) when a value of a column created by the link has been edited (#253) | `stokesforge_transpose_relink`, `transpose_ui.rs` |

The "built" identifiers are chosen before the call (a `&str`, not a
literal in its arguments): the catalog guard looks for them as
literals (`BUILT_IDENTIFIERS` in `tests/guards.rs`).

## Blocking dialogs

`confirmations::show` (`app/confirmations.rs`) paints them after everything else;
`AppState::blocking_dialog` (unsaved project, row drop, alert,
overwrite, overwrite of a file read with discrepancies) makes the other actions of the frame be
emptied (`blocking_confirmations`). All of Small class (480 x 170).

| Window | Nature | Identifier | Opened from |
|---|---|---|---|
| Unsaved project | `SaveChanges` | `stokesforge_unsaved` | New Project, Open…, Quit, closing the window (`AppState::unsaved_pending`) |
| Save / open failure | `Alert` | `stokesforge_alert` | a save or an open that fails (`AlertKind::SaveFailed`, `OpenFailed`) |
| Low R² (warning), lost session | `ReadOnly` (Small) | `stokesforge_alert` | end of a calculation whose R² is low, if the preference allows it (`AlertKind::LowRSquared`); dead session (`AlertKind::SessionLost`) |
| No x' Values (warning, #493) | `ReadOnly` (Small) with a secondary "Cancel Analysis" | `stokesforge_alert` | every row analysis that runs on the column numbers 1, 2, 3... because no Y column has an x', over the analysis window (`AlertKind::NoXPrime`); and the Recalculate of a transposed sheet whose source lost all its x' (Close alone, no analysis window) |
| Overwrite a file | `Confirm` | `stokesforge_overwrite` | export whose added extension targets an existing file (`pending_overwrite`) |
| Overwrite a file read with discrepancies | `Confirm` | `stokesforge_discarded_save` | Save that would overwrite the source file of a load that discarded calculations or traces not yet saved (`pending_discarded_save`, #309); not for Save As elsewhere |
| Delete a backup copy | `Confirm` | `stokesforge_recovery_delete` | Delete on a row of the recovery window (`pending_recovery_delete`, #317); the recovery window stays underneath |
| Axis of a row drop | `Choice` | `stokesforge_row_drop` | dragging rows onto a graph that already has an X axis (`pending_row_drop`) |

The alerts (failure, low R², lost session) share the identifier
`stokesforge_alert`: only one at a time.

## Content windows (non-modal)

Workbook, graph, matrix, note, Script Settings: tabs of the workspace, painted as
confined floating windows (`show_floating_window`). Only the workbook and the graph
detach into an OS window (`window_detach.rs`; `Tab::is_detachable`, the single
source of the header icon and of the guard of `detach_tab`, #473); a note, a
matrix and a Script Settings object stay in the main window. The header has an icon header (collapse, detach,
maximize, close; `HeaderPlacement::Floating` / `Detached`). No footer, no
imposed initial focus. Default sizes (`window_placement.rs`): workbook
373 x 420 (two thirds of 560), graph 560 x 460, matrix, note and Script Settings 560 x 420;
minimum 220 x 140. Opening: `open_tab` (`window_stack.rs`), from the Project Manager; a
second opening on the same entry brings the window to the front without
duplicating it. Opening a project (startup, File > Open, Recover Projects) restores
its saved windows (#513). Each new window is offset in a cascade.

**Script Settings (#548).** The twin of the note, for the TOML a script reads
(`app/settings_contents.rs`, opened with a double-click or Enter in the Project
Manager, or created by File ▸ New ▸ Script Settings and File ▸ Import ▸ Script
Settings…, which open it). Two tabs on top (`tab_strip`, `TabStripSide::Top`):
**Form**, one scrolling page with fixed bold section headings (S-L5: never
collapsible), a label column and one field per kind of value, and **Text**, the
raw text. Form fields apply on Enter, Tab or leaving the field and Esc cancels
(S-N3); a typed number out of its bounds is brought back to the bound, orange for 2 s
(S-N7), an unreadable one is refused, red, with the old value back (S-N6); a
stored value outside its hint is orange with a bubble and never rewritten
(S-R8); a list has a red bin per element (the analysis lists' twin) and an Add
Value button (a copy of the last one, greyed with a reason when the list is empty). The Text tab applies when the editor
loses focus (`SettingsEdit`, committed by a save like a note) and Esc abandons;
a text that is not exact TOML is kept and saved, its line shows in red under the
editor, and the Form tab freezes on the last readable form (greyed, a red banner
names the line). Parsed once per text (`AppState::settings_view`), never per
frame. One validation is one history entry.

**Graph in the Page Layout Mode (#426).** The same window, same code
(`graph_view_contents`), with two ruler strips (`RULER_WIDTH`) around the figure and
a settings strip under it (`app/page_layout.rs`). The strip's height is reserved from
what it used at the last frame (`controls_height`, whole points, at least
`LAYOUT_CONTROLS_HEIGHT`), so the figure's rectangle never moves from frame to frame
and the cache stays valid; its rows wrap and `keep_together` keeps a label with its
field or button; its two-state toggles (Coarse | Fine, Regular | Free, Grows with the axes | Fixed) are drawn by `app/state_toggle.rs`. The mode adds no window and no dialog.
It **maximises** a floating window while it lasts (`app/layout_window.rs`: `layout_entries` keeps
the tab that was maximised before; `reconcile_layout_modes` leaves the mode of a graph
whose window is no longer the maximised one, which covers Restore, the Window menu,
close, delete and project load). A detached window maximises its OS window the same way
(`follow_mode_maximize`, unless it was maximised at entry), and its header's Restore leaves the mode;
a restore done from outside the application (an OS shortcut) cannot be told from a maximise the
compositor has not applied yet, so it leaves the mode alone. This is safe on Wayland because a
detached window no longer needs the root (#475): it locks the application in its own pass,
paints the shared state and applies its actions at once, so a root covered by it, which gets no
frame callback, delays nothing. There is no occlusion report and no 100 ms throttle any more (the
frame-starvation patch of #457 was removed in L5). The saved window views report the state from before the mode
(`reported_maximized_tab`). Esc and the other keys of the mode are read by the root at
the start of the frame (`begin_layout_frame`) and at the top of a detached viewport, never
inside the contents: with a modal open (`modal_shown`) or a text field that had the keyboard
a frame ago (`note_field_focus`) they stand aside.

## Moving and resizing a detached window (#457)

A detached window has no OS decoration: its title (`window_chrome.rs`,
`panel_header`, `window_header`) and eight edge zones (`detached_resize_handles`)
hand the gesture to the compositor. That goes only through `app/os_gesture.rs`
(`start_drag`, `begin_resize`; guard `no_os_gesture_command_outside_the_helper`).
On Wayland the compositor keeps the button release, so the helper plays it for
egui at the head of the next pass of the same viewport (a pointer move too far
for a click, then the releases), otherwise the title stays "dragged" and every
second grab does nothing.

## Where a dialog is painted (#472)

Every dialog is painted by the **root** (`show_edit_dialogs`, `confirmations::show`),
through the same code and the same `AppState`, whichever window made the gesture
that opened it. #457 had a detached window paint its own dialogs; at the trial it
made too many cases (the main window pulled the dialog back, the reverse did not
exist), and #472 removed it: no `AppHost`, no host rule, no veil.

- **The rule.** A content feature works everywhere (a detached window offers what
  the docked one does); a global feature stays in the main window. A dialog is a
  global feature: it opens in the main window.
- **The window of the gesture.** When a gesture made in a detached window (or a
  message that arrived while the person worked there) opens a dialog,
  `announce_opened_dialog` (`app.rs`) shows the orange band of that window for
  5 seconds ("Graph Properties is open in the main window", or "A question is
  waiting in the main window"), sends `ViewportCommand::Focus` and
  `RequestUserAttention` to the root **once**, never on each pass, and the command
  wakes the root, which paints the dialog on its next pass. `Focus` raises the
  window on Windows and X11 and does nothing on Wayland (`vendor/winit`, empty
  `focus_window`): the band, the system's attention request and Alt+Tab are the
  way there. A refused gesture never brings the main window forward.
- **While a dialog is open** a gesture made in a detached window is refused (#471),
  and so is a key (#460): `window_band::stop_keys` is the one stop, called first
  in each of the four detached closures (tab viewport, Object Manager, console,
  Project Manager), before any key reader. It removes key, text and clipboard
  events from the viewport's input and notes the refusal for the root, which
  posts the band (same path as a refused action). It stands aside for a text
  field that has or had the keyboard (typing, and the Esc that gives a field
  back), an open popup, and a modal of the same viewport (Object properties).
  A key release, a key repeat and an Alt combination (Alt+Tab) never trigger
  the band. Ctrl+Z, Ctrl+Y and Ctrl+S are stopped by the same call when the
  cursor is not in a text field; with the cursor in a field they are read by
  `read_detached_history_and_save` (next section), refused by the root, and
  the field's text stays.
- **Covered root.** A dialog opened while the root is covered runs unpainted, like
  the rest of the root (above), and paints in full, with its focus, when the root
  is uncovered.
- **The single exception: object properties** (rectangle, arrow, `graph_window.rs`)
  stays in the window of its graph. It is a small panel of the graph, not a dialog
  of the application, and it raises no signal.
- Not covered: `rfd` pickers (system windows), local popups.

## Ctrl+Z, Ctrl+Y, Ctrl+S in a detached window (#459)

The detached graph, the detached workbook and the three detached panels read
exactly three keys, Ctrl+Z, Ctrl+Y (with Ctrl+Shift+Z) and Ctrl+S, through
`shortcuts::read_detached_history_and_save`. The note, the matrix and Script Settings, which
#473 keeps in the main window, never detach. Everything else global (Ctrl+N, Ctrl+O, Ctrl+E,
Ctrl+", Ctrl+Shift+S, Ctrl+Q) stays in the main window; Help › Keyboard Shortcuts
and the manual say so.

- **Read before painting.** The call comes first in the window's frame (after
  `stop_keys`), so a cell, the console, a name field or the Page Layout Mode
  never sees the key: one press is one action on one history stack, nothing is
  undone twice, and a typed text stays as typed (Ctrl+Z undoes the last command,
  as in the main window).
- **Applied after the window's own actions** (#398), in the same pass:
  Ctrl+Z pressed in the frame of a click undoes that click. The dialog refusal
  of #471 applies, so a dialog open in the main window refuses them, shows the
  band, and leaves the field untouched.
- **Ctrl+Shift+S is consumed first and dropped**: Ctrl+S would match it and Save
  As would save silently.
- **Ctrl+S with no file yet** dispatches `Action::Save`; the system
  file picker opens (no band).
- **Ctrl+Q closes nothing.** egui closes the window where its built-in shortcut
  (`Options::quit_shortcuts`) is pressed, which lost a typed cell;
  `refuse_quit_in_detached` consumes it and the window shows the orange band
  "Quit is in the main window". In the main window Ctrl+Q still asks to quit.
- **A name field on an object that is gone closes**: the Project Manager prunes
  `memory.rename` entries whose object left the tree (Ctrl+Z of Add Folder).

## Outside the catalog

- **In-place edits** (title, legend, text of a graph, grid cell,
  rename): no window, see the standard (S-T4) and
  `inplace_edit.rs`.
- **Native OS windows**: file picker (`file_picker.rs`),
  never an egui window.
- **Color picker**: a popup window anchored to the swatch button
  (`color_picker.rs`), not a dialog of the catalog.
- **Test prototype**: `preferences_proto.rs`, compiled in test only,
  bench of the advanced form of #148 (five pages including Folders and Paths and
  Analysis, not yet delivered). Its reference screenshots are in
  `crates/stokesforge-app/tests/snapshots/preferences_proto_*.png`; it is
  not a window of the application.

## Size, position, Esc, focus: what the building block does by itself

None of this is rewritten in a new window (S-F7 to S-F9, details in the
standard):

- the size and position are remembered **per identifier** of dialog
  (dialog key = `egui::Id`), in the preferences, 32 kinds at most; a small
  question or alert dialog (`DialogNature::opens_centred`: confirmation, save,
  choice, alert, read-only of class Small) ignores the remembered position and
  opens centred on the whole window;
  give a new window a stable and unique identifier `stokesforge_<nom>`;
- Esc: a text field then the dialog; the cross is the same as Cancel. While a
  modal is shown, no workspace keyboard (grid, Project Manager) acts: they test
  `dialogs::modal_shown` (#406);
- initial focus: set by the nature (S-F11): the first text field, found
  by the building block, for settings, drafts, choices and wizards; the
  default button (Cancel, Save, Close) for a confirmation, a
  save or an alert; nothing for a read-only window;
- footer message area: the error (`AppState::fail` as long as a content
  dialog is open) or the reason for the disabled main button
  (`DialogFooter::gated`, S-F10).

## Anatomy of a new window

1. **Type and nature.** Choose from the table of types: the row gives the
   nature (`DialogNature`), hence the footer, the focus and the default size
   class. When in doubt between confirmation and choice: a confirmation
   exists only for what Ctrl+Z cannot recover (standard S-F3).
   **Size**: the default class of the nature, otherwise another class;
   outside the classes, an interactive window requires a justified free size
   (`// FREE-SIZE(#300):`), a window without rich interaction takes the
   size of its content.
2. **Mockup (E2).** Have it approved before any code: size (class), pages or
   sections, content of each page, footer, opening point. Attach, when
   useful, a screenshot of the twin (see below).
3. **Twin.** Open the reference window of the type (next table) and copy it,
   without inventing its structure.
4. **Texts.** Fluent `en` keys first, glossary of
   [`ui-texts.md`](ui-texts.md) (skill
   `stokesforge-ui-texts`): title, buttons, `hint` tooltips, `disabled-*`
   disable reason.
5. **State.** A flag or an `Option` in `AppState` and two actions
   `Open…` / `Close…` (the dialog does not mutate the model; every mutation goes through
   an undoable command, see `stokesforge-command`).
6. **Painting.** A function `xxx_window(ctx, &state, actions)` that returns
   immediately if the state is closed, then add it to `show_edit_dialogs`
   (or to `confirmations::show` if it must block the rest).
7. **Content building blocks.** `form`/`choice` (`form.rs`), `number_field`,
   `tab_strip`, `hover_help`, `empty_state`, `theme::SPACE_*`. No bare
   tooltip, no `DragValue`, no hard-coded color: the guards of
   `tests/guards.rs` refuse them.
8. **Tests.** Paint the window without a real window (`test_support.rs`) and
   read the painted texts; the guard `no_window_is_a_raw_egui_window`
   and `tests/labels.rs` already cover the building block and the labels.
9. **Catalog (E1).** Add the row of this page in the same commit,
   with the identifier `stokesforge_<nom>` in backticks: the guard
   `the_window_catalog_and_the_code_agree` refuses an identifier
   missing from the page, or a row without a call.

### Reference window by type

| Type | Reference window | Why this one |
|---|---|---|
| Live settings | `plot_settings_window` (`plot_settings.rs`); `shape_properties_ui` (`graph_window.rs`) for the smallest | draft, single confirmation, tabs |
| Creation on a draft | `analysis_window` (`analysis_dialog.rs`) | button verb, disable reason, preview |
| Choice | `stale_export_window` (`stale_export.rs`) | secondary + main, the shortest |
| Wizard | `modal_wizard` (`dialogs.rs`) | none today: the import became a `Draft` (#488); the nature waits for the next multi-step window |
| Live settings | `preferences_window.rs` (tabs, immediate settings) | Close only, focus on the first field |
| Read-only | `keyboard_shortcuts_window.rs` (the simplest) | Close only |
| Confirmation | `analysis_replace_window` (`analysis_dialog.rs`); `confirmations.rs` for the blocking ones | Cancel by default |

## Taking a screenshot

Do not make them by hand: produce them with the `run-stokesforge` skill
(`.claude/skills/run-stokesforge/SKILL.md`).

```sh
cargo build -p stokesforge-app -p stokesforge-cli
D=.claude/skills/run-stokesforge/driver.py
python3 $D start [project.stf]      # no argument: demo project
python3 $D click 20 11             # File menu; then the entry that opens the window
python3 $D ss /tmp/fenetre.png     # screenshot, to read with Read
python3 $D stop
```

Useful opening gestures: File > Preferences, File > Import (or Ctrl+3),
Help > Keyboard Shortcuts, click on the status line for the history, right-click
on a graph or the Graph menu for the properties. Menu coordinates
are in fixed pixels (see the skill); reread the screenshot before
comparing it to the twin. English is the reference language: switch back to English
after a trial in French (the language is saved in the preferences).
For a layout alone, the test benches (`egui_kittest`, see
`preferences_proto.rs`) render the window without launching the application.

## Platforms

Requirements stay GNOME (Wayland) and Windows. X11 is not supported: say "it works, but is untested" (user decision, #475). No card, trial entry or probe is written for X11.

# Application: the shell and the code split

Continuation of the contents page [`README.md`](README.md). Contents: the bench principle, the two crates (`stokesforge-app-state` / `stokesforge-app`) and the module → role table of `src/app/`.

## `stokesforge-app` — the UI shell

**What this crate is not**: a draft of the final interface. The editable grid
with delegates, the properties panel and a complete graph system (multiple
selection, reordering, persisted closing…) will be taken up again entirely
later; nothing here must preempt them.

The bench is deliberately poor. Its role remains to prove that the end-to-end
integration holds — model → command → undo → save → reload, in a real window
— **as long as a failed integration costs only a rewrite of this crate**, and
not a rewrite of everything built on top of it. Two technical foundations are
laid on top of this principle (see below): the layout uses **confined floating
windows** (`egui::Area`), and Save/Load go through a real native file picker
(`rfd`) — these are only scaffolding, not the final panels they will host.

Toolkit: `eframe` / `egui` (layout and rendering), `rfd` (native file
picker).

### Two crates: the state and the shell (#263)

The application is split into two crates, and the compiler holds the
boundary. **`stokesforge-app-state`** (no egui) receives everything the
application decides without drawing it: `state.rs` + `state/` (`AppState`,
`Action`), `selection.rs`, `preferences.rs`, `analysis.rs`,
`analysis_recent.rs`, `object_manager.rs`, `object_drag.rs`,
`history_text.rs`, `quantities.rs`, `user_colors.rs`, `object_format.rs`,
`text_format.rs`, `errors.rs`, `i18n.rs` (with the `i18n/` catalogs) and
`file_picker.rs`. **`stokesforge-app`** keeps `main.rs`, `app.rs`, `app/`,
`theme.rs`, `icons.rs`, `field_state.rs`, `number_field.rs`, `toolbar.rs`,
`list_frame.rs`, `tree_lines.rs` and `color_picker.rs`: everything that names an
egui type, and the offer of the figure to the clipboard (`stokesforge-clipboard`
pulls in winit, so it cannot move down). In the paths of this document, a module
without a prefix is in `stokesforge-app/src/` if it is under `app/` or named
`theme`, `icons`, `field_state`, `number_field`, `toolbar`, `list_frame`,
`tree_lines`, `color_picker`, and in `stokesforge-app-state/src/` otherwise.

### Code split: `app.rs` and `src/app/` (#115)

`app.rs` keeps only the **orchestration** of the frame; each interface surface
is a real module of `src/app/` (work package #115, #116–#123): explicit
imports, minimal visibility (`pub(super)` between `app` modules), no
`use …::*` outside test code (guard `aucun_import_global_hors_des_tests`), a
head doc per module. Direction of dependencies: `app.rs` → surface modules →
shared building blocks (`dialogs`, `tab_strip`, `list_row`, `window_chrome`,
`crate::icons`…). Internal tests follow the code they test: a test lives in
the module of its surface, and the `tests` module of `app.rs` tests only the
orchestration. The guards that read the code walk all of `src/`,
subfolders included, and look for their functions by name (#116): see
`docs/tests.md`.

| File | Role |
|---|---|
| `app.rs` | orchestration: `StokesForgeApp` and its fields, `new` (initial layout, restoring the window state), `ui()` and the **panel order**, `collect_shortcuts` (global shortcuts, consumed before the widgets), deferred application of actions and openings after painting, blocking confirmations (new project, sheet deletion), draining of detached windows, capture and auto-close for verification. No module of `src/app/` borrows a helper from it: what they share lives in `tab.rs`, `window_detach.rs` or `crate::color_picker` (#128) |
| `app/menus.rs` | menu bar (`menu_bar` calls `file_menu`, `edit_menu`, `workbook_menu`, `graph_menu`, `view_menu`, `window_menu`) and status bar (`status_bar`, `status_line`) |
| `app/status_history.rs` | read-only window of the last 50 status bar messages, timestamped in UTC (#230) |
| `app/analysis_menu.rs` | Analysis menu (#181): `analysis_menu_contents` (menu bar), `analysis_open_menu` / `analysis_entry` (right-click: `Analysis ▸` submenu, one entry per category, #248), `recent_entries` (last three analyses, #249; state in `analysis_recent.rs`), `calculation_section` (Edit / Recalculate / Detach / Delete on a calculated column) |
| `app/transpose_ui.rs` | linked transposed sheet (#253): the band at the top of the grid (origin, state, Recalculate / Relink) and the confirmation before overwriting edited values |
| `app/stale_export.rs` | confirmation of an export or a figure copy that shows out-of-date series (#198) |
| `app/analysis_dialog.rs` | analysis window (`modal_dialog_with`, two columns: form + preview) and confirmation of the deletion of a calculation (`calculation_delete_window`) |
| `app/analysis_forms.rs` | the forms per family (`form_body`, `value_row`, parameter table) |
| `app/analysis_handles.rs` | draggable handles of a calculation on the project graph (#199): visible when the calculation is selected (its curve, or its row in the Results pane), one drag = one `MoveCalculationHandle` |
| `app/analysis_preview.rs` | the hand-painted preview (painting only): Point / Zone gestures, navigation like the graph (Ctrl + wheel, Ctrl + drag, double-click), magnet (Shift), average (Shift + wheel), residuals band, cache in egui's temporary memory; its geometry and ticks come from `stokesforge-render` (#261) |
| `app/results_pane.rs` | Results pane under the grid of a workbook: counter, Sheet / Workbook scope, list and series × value table |
| `app/format.rs` | text and object format bars, contextual bar: `format_target`, `text_format_state`, `object_format_state`, `tools_context`, `format_zone`, `format_text`, `format_object`, `run_toolbar_request` (the bars themselves: `crate::toolbar`) |
| `app/project_manager.rs` | Project Manager: `project_manager_panel` (fixed panel) and the tree `project_manager_contents_from_tree`, shared by the panel and the detached window |
| `app/curve_settings.rs` | series properties window (#177–#179): list, group, common style, swatch sets (`curve_settings_page`); opened as the `PlotSettingsTab::Series` tab of `plot_settings_window` |
| `app/object_manager_ui.rs` | painted Object Manager: `object_manager_panel`, `run_object_manager_request`, rows, checkboxes, menus, dragging of series, groups and free objects by the handle (the model: `crate::object_manager`) |
| `app/script_console.rs` | script console: `script_panel`, `ScriptConsoleModel` reading, content shared by attached/detached, input, Run/Stop, output |
| `app/tab.rs` | `Tab`, the tab of a content window (workbook, graph, matrix, note), its translations to the model (`tab_for_window`, `tab_node`), its title (`tab_title`, `tab_title_from_tree`, deleted node included) and its mirror readable by `crate::state` (`active_tab_kind`) |
| `app/contents.rs` | contents shared by floating/detached: `tab_contents`, `note_contents`, `matrix_contents` (placeholder) |
| `app/workbook.rs` | workbook: `book_contents` (content shared by floating/detached), `sheet_tabs`, `workbook_menu` |
| `app/grid.rs` | grid of a sheet: `grid_contents` prepares the table, paints its headers and its virtualized body, then handles the keyboard with `grid_keyboard` (after the cells: the editors consume Tab/Enter first and take the focus back), stacked header, gutter, cells, `grid_edit_menu`, keyboard focus (`GRID_KEYS`, `keyboard_focus_outside_grid`), offset bars, `column_grip_bands` (grip for dragging columns), type and role labels |
| `app/figure_copy.rs` | copy the figure of the active graph to an external application (#149): the Ctrl+C gesture (`request_figure_copy`, `take_figure_copy_gesture`), the menu entry (`request_graph_figure_copy`) and the offer to the system (`offer_figure`) |
| `app/graph_targets.rs` | "Send to": right-click entries of a column selection and of the Workbook menu that add the selection to an open graph, attached or detached (#165); the open graphs are published there in the `Context` at each frame |
| `app/graph_window.rs` | graph window: `graph_view_contents` (shared by floating/detached), `graph_drop_target`, context menu of the figure, `annotation_interactions`, order and properties of objects, `request_graph_export` |
| `app/graph_text_edit.rs` | in-place editors (free text, title, legend name) and pending formatting requested by the format bar |
| `app/file_drop.rs` | file drop: `begin_file_drop`, `claim_file_drop`, `finish_file_drop` (one slot per viewport), `open_project_in_new_window` |
| `app/inplace_edit.rs` | **the** contract of in-place editing (#224): opening selected, Enter/Tab/click elsewhere confirm, Escape cancels; pure decision `resolve` |
| `app/import_wizard.rs` | import wizard: `import_window`, its six steps; choice of template/files and format settings in sections, the preview colored by role, the labels of the choices |
| `app/plot_settings.rs` | graph properties: `plot_settings_window`, its tabs, one editor per page, the formula syntax help (`formula_field`, `formula_help`); `column_metadata_window` |
| `app/hover_help.rs` | shared building block of the hover help of a field (#197): `hover_help`, the greyed icon to the right of the field whose bubble shows the reminder |
| `field_state.rs` (crate root, like `color_picker.rs`) | shared building block of the state of a field (#219): `FieldIssue` (refused red / brought back orange), `show_issue`, 2 s memory (`flag`, `active`), cycle of a validated text field (`edit_text`, `decide`, `Verdict`) |
| `number_field.rs` (crate root) and `quantities.rs` (in `stokesforge-app-state`) | "number" building block (#220): `number_field` (text field, full arrow in the frame if the quantity has values, keyboard arrows, hover wheel that follows the list, list in an `egui::Area`) and the table of quantities (`Quantity`); built on `field_state::edit_text_with_arrow`; `list_frame.rs`: arrow and backgrounds shared with the drop-down lists (#234) |
| `app/confirmations.rs` | blocking dialogs of the application (#229): "Discard Changes · Cancel · Save", serious failure (S-M8), overwriting an export, axis choice of a row drop; confirmation / choice classification at the top of the module |
| `app/dialogs.rs` | shared building block of modal dialogs: `modal_dialog`, `modal_dialog_with`, `modal_wizard` (wizard), `modal_read_only` (Close only), `modal_confirm` (Cancel by default, destructive verb), `modal_save_changes` (trio, Enter = Save), `modal_alert` (S-M8), shared body `modal_frame`; ordered footer (S-F6), message area (error of `AppState::dialog_error`, otherwise reason for the greyed primary button), two-level Escape, geometry per kind of dialog in the preferences and initial focus on the first field (#228); `sync_dialogs`, called by `show_edit_dialogs`, links footer, preferences and state |
| `app/form.rs` | shared building block of the form (#227): `form` and `Form` (two-column grid: `row`, `field`, `checkbox`, `color`, `note`, `section`, `enabled`), fixed exclusive choice `choice` / `choice_by` (radios for 2–3 options, list from 4, `choice_layout`) |
| `app/tab_strip.rs` | shared building block of the tab strip: `tab_strip`, `tab_strip_with` (`TabOps`, `TabEvent`) |
| `app/menu_sections.rs` | shared building block of menus: `MenuSections::section` (only separator line, #155), `menu_choice` (check mark alone for an exclusive choice, S-C3, #227) |
| `app/list_row.rs` | shared building block of the selectable list row (#169): `list_row`, `ListRowBackground`, `list_row_label`, `list_row_text`, `LIST_ROW_MARK`, `LIST_ROW_HEIGHT` (single height, 24 pt, #170) and `with_list_row_height` / `raise_to_list_row_height` / `restore_row_height` (placing it in a `ui.horizontal` or around a `show_header`); Project Manager, Object Manager, series list of the properties |
| `app/shortcuts.rs` | **single registry of keys** (#231): `SHORTCUT_*` and `KEY_*` constants, their reminders, `HELP` table that generates Help › Keyboard Shortcuts, consumption of undo/redo in the active viewport (`collect_undo_redo_shortcuts`) |
| `app/preferences_window.rs` | File › Preferences (#148): General (language), Display (Metadata and x' rows of new workbooks, `Action::SetWorkbookHeaderDefaults`) and Export (Copy Graph formats: drag order, checkboxes, `Action::SetCopyFormats`, `copy_formats` preference) sections, applied immediately, `modal_read_only` |
| `app/object_dnd.rs` | drag state of series, groups and free objects, shared by the Object Manager (preview, one action on release) and the series window (draft modified live); pure decisions in `stokesforge_app_state::object_drag` (`step`, `finish`): enter a group, leave it, swap single series, move a group as a block, reorder text or shape within its category; "end of list" zone during the drag |
| `app/reorder_list.rs` | shared building block of a list reordered by dragging: `Grip` handle, row that follows the pointer, rows that move apart live (`reorder_row`, `reorder_move`, `move_before`); only the handle can be grabbed, so as not to steal the click of the checkboxes |
| `app/keyboard_shortcuts_window.rs` | Help › Keyboard Shortcuts (#231, S-K5): read-only window, generated from `HELP` |
| `app/busy.rs`, `app/empty_state.rs` | "work in progress" building blocks (indicator after 0.5 s, progress, Stop after 3 s; #232; background import and formula recalculation #239) and "empty state" (centered greyed sentence that says the gesture) |
| `app/cursors.rs` | only module that names a `CursorIcon` (#231, S-X2): open/closed hand, resize arrows, finger of clickable elements that are not buttons |
| `app/window_chrome.rs` | common frame of windows and panels: `window_header`, `panel_header`, `floating_window_frame`, active outline, handles, `show_window_in_layer`, `show_window_contents_bounded` |
| `app/window_placement.rs` | placement: default sizes, cascade, fixed workspace (`WORKSPACE_SIZE`), `clamp_to_workspace`, logical → screen conversion |
| `app/window_stack.rs` | stack of floating windows: `open_tab` (the only place that decides to open or focus), egui order and layers, hiding, collapsing, maximizing, `show_workspace` |
| `app/test_support.rs` | (compiled in test only) test helpers shared by the surfaces: `contexte`, `image`/`image_de`, `touche`, `cliquer`, `painted`, `textes`/`texte_exact`, `distincts`, `colonnes_xy` (#128) |
| `app/window_detach.rs` | detaching into OS windows: `show_detached_project_manager`, `show_detached_object_manager`, `show_detached_script_console`, `detach_tab`/`reattach_tab`, `show_detached_tabs` and the publication of the snapshot that wakes the viewports (#104, `snapshot_stamp` is its fingerprint), `sync_window_views` (window state sent to the session, #16) |

A new large surface is written in its own module, on this model; `app.rs`
gains only the call. Since #124, `ui()` describes the steps of the frame:
background jobs, shortcuts, panels in the order above, workspace, dialogs,
window requests, confirmations and dispatch. The menus, the grid (headers,
keyboard, body and drag), the Object Manager (headers, layers, series, axes and
objects), the properties pages and the files/format steps of the import are
split in place. Detaching separates publication, content and requests; the
workspace separates maximizing, scrolling and painting of a window; the tab
strip separates menus, renaming, scrolling and callbacks.

An extraction preserves the nesting of the `Ui`s, the persisted identifiers and
the order of event consumption. The small private painting contexts group only
the disjoint data or borrows needed for one frame; they add neither business
state nor sub-`Ui`. Openings and closings remain applied after the walk of the
stack. A future re-split stays in a commit separate from a move.

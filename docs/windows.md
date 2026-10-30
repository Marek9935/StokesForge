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
the user left (S-F8, see below). Two guards hold this
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
  today: the analysis (1000 x 660), the import wizard (1000 x 620) and the
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
| **Content window** (non-modal) | `window_chrome::show_floating_window` | none | none | workbook, graph, matrix, note |

The first five building block rows all go through the same body
`modal_frame` (same frame, same title bar, same blocking of the rest of
the interface); the content window goes through `show_window_in_layer`, an
`egui::Area` and not an `egui::Window`. A raw `egui::Window` is forbidden
(guard `no_window_is_a_raw_egui_window`).

## Modal dialogs

All are painted in `StokesForgeApp::show_edit_dialogs` (`app.rs`), as long as
their state is open in `AppState`, except the blocking ones (last section).

| Window | Nature | Size | Pages or content | Opened from | Identifier, file |
|---|---|---|---|---|---|
| Graph properties | `Settings` | Medium | 5 tabs (`tab_strip`): Graph, Plot Area, Axes, Legend, Theme; on Axes, the sides Bottom, Left, Right, Top are always the four vertical tabs at the left (fixed strip, the page of the axis scrolls, S-L9); a side held by another layer of the cell is greyed with its reason, an overlaid layer edits its X axis on Top and its Y axis on Right with an "Own scale" box (#396, `cell_sides`) | right-click on a graph component, Graph menu, Properties button of the toolbar (`Action::OpenPlotSettings`) | `stokesforge_plot_settings` (built), `plot_settings.rs` |
| Series properties | `Settings` | Large | list of series on the left, editor on the right (`curve_settings_page`) | double-click on a curve, Object Manager, Graph menu (`Action::OpenObjectProperties` on a series or a group) | `stokesforge_curve_settings` (built), `plot_settings.rs` (window), `curve_settings.rs` (page) |
| Properties of an object (rectangle, arrow…) | `Settings` | Medium | coordinates X1 Y1 X2 Y2, line and fill colors | double-click on the object, Object Manager (`open_shape_properties`) | derived from the object, `graph_window.rs` |
| Analysis (new or edit) | `Draft` | free 1000 x 660 | two columns: settings (330 wide, scrolling) and clickable preview with tools and values | Analysis menu, right-click, results panel (`Action::OpenAnalysis`) | `stokesforge_analysis` (built), `analysis_dialog.rs`, `analysis_forms.rs`, `analysis_preview.rs` |
| Import wizard | `Wizard` | free 1000 x 620 | 6 steps: Files, Format, Header, Columns, Metadata, Destination; file preview on the right at every step | File > Import, Ctrl+3 (`open_import_dialog`) | `stokesforge_import_wizard`, `import_wizard.rs` |
| Preferences | `LiveSettings` (immediate settings) | Large | 4 rubrics as tabs on top (`TabStripSide::Top`): General (language, maximum file size at opening, autosave: checkbox and period in minutes, disabled if the checkbox is unchecked, #317; analysis: default of Values Show / Keep in Results, low R2 warning), Display with vertical sub-tabs at the left (S-L9) Interface (formula syntax help icon) · Object Manager (lists, expand-by-default groups, Reset) · New (rows of new workbooks, decimal comma of new graphs), Export (SVG text, PNG resolution, Copy Graph formats), Folders (the configuration folder with Open Folder and its files, the recovery copies folder, and the default folders Open and Save, Import, Export, each typed or picked with Browse…, with a "Folder not found" note #148) | File > Preferences (`Action::OpenPreferences`) | `stokesforge_preferences_rubrics`, `preferences_window.rs` |
| Keyboard shortcuts | `ReadOnly` | content 480 x 460 | scrolling list by area, generated from `shortcuts::HELP` | Help > Keyboard Shortcuts (`Action::OpenKeyboardShortcuts`) | `stokesforge_keyboard_shortcuts`, `keyboard_shortcuts_window.rs` |
| Formula Functions | `ReadOnly` | content 900 x 560 (wide: each description on one to three lines) | the functions of an `F(x)=` formula, one family at a time (#402): families as fixed vertical tabs at the left (`TabStripSide::Left`, S-L9), at the right a scrolling table Function · Example · What it does; the intro line on top; state (open, chosen family) in `AppState` | the "?" of the `F(x)=` cell while it is edited (`Action::OpenFormulaFunctions`) | `stokesforge_formula_functions`, `formula_functions_window.rs` |
| About | `ReadOnly` | content 360 x 140 | version, license, project page link, Copy button (version, system and architecture line for bug reports) | Help > About StokesForge (`Action::OpenAbout`) | `stokesforge_about`, `about_window.rs` |
| Status bar history | `ReadOnly` | content 520 x 360 | the last 50 messages, empty state if none | click on the status line (`Action::OpenStatusHistory`) | `stokesforge_status_history`, `status_history.rs` |
| List of low R² | `ReadOnly` | content 480 x 170 | counters by type, normal text | status bar badge (`Action::OpenLowR2List`) | `stokesforge_low_r2_list`, `status_history.rs` |
| Column metadata | `ReadOnly` | Medium | scrolling key / value grid, "Use as x'" button per numeric row | click on the metadata cell of a column header, when it is not empty (`Action::ViewColumnMetadata`) | `stokesforge_column_metadata`, `plot_settings.rs` |
| Deleting a calculation | `Choice` | Small | Delete, Delete with Columns, Cancel | deleting a calculation (`Action::RequestDeleteCalculation`, results panel or analysis menu) | `stokesforge_delete_calculation`, `analysis_dialog.rs` |
| Export of stale series | `Choice` | content 560 x 170 | Export Anyway, Export Without Warning, Recalculate and Export, Cancel (four long labels: wider than Small) | export or copy of a graph that shows an out-of-date calculation | `stokesforge_stale_export`, `stale_export.rs` |
| Column used by a calculation | `ReadOnly` | content 480 x 170 | Undo, Close; the list of formula columns and calculations that read what was just deleted | deleting columns, a sheet or a workbook read by a calculation (#306), after the gesture | `stokesforge_column_use`, `column_use.rs` |
| Copy with header of formula columns | `Choice` | Small | Values, Formulas (default), Cancel | Ctrl+Shift+C or Copy with Header when a copied column carries a formula (#306) | `stokesforge_full_copy`, `column_use.rs` |
| Project recovery | `ReadOnly` | content 560 x 230 | one row per backup copy (name or Untitled, UTC date) with Open, Ignore, Delete; introduction message; scrolling list | at startup, if copies from vanished processes remain that are newer than their `.stf` (`AppState::scan_recovery`, #317); closes by itself when the list is empty | `stokesforge_recovery`, `recovery.rs` |
| Replacing a shared analysis | `Confirm` | Small | a message | applying an analysis whose source feeds other curves | `stokesforge_replace_analysis`, `analysis_dialog.rs` |
| Refreshing a modified transposed sheet | `Confirm` | Small | a message | Relink (band of the linked transposed sheet) when a value of a column created by the link has been edited (#253) | `stokesforge_transpose_relink`, `transpose_ui.rs` |
| Deleting an import template | `Confirm` | Small | a message | delete button of a template, at the Files step; painted **inside** the wizard | `stokesforge_import_template_delete`, `import_wizard.rs` |

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
| Overwrite a file | `Confirm` | `stokesforge_overwrite` | export whose added extension targets an existing file (`pending_overwrite`) |
| Overwrite a file read with discrepancies | `Confirm` | `stokesforge_discarded_save` | Save that would overwrite the source file of a load that discarded calculations or traces not yet saved (`pending_discarded_save`, #309); not for Save As elsewhere |
| Delete a backup copy | `Confirm` | `stokesforge_recovery_delete` | Delete on a row of the recovery window (`pending_recovery_delete`, #317); the recovery window stays underneath |
| Axis of a row drop | `Choice` | `stokesforge_row_drop` | dragging rows onto a graph that already has an X axis (`pending_row_drop`) |

The alerts (failure, low R², lost session) share the identifier
`stokesforge_alert`: only one at a time.

## Content windows (non-modal)

Workbook, graph, matrix, note: tabs of the workspace, painted as
confined floating windows (`show_floating_window`) or detached into an OS
window (`window_detach.rs`), with an icon header (collapse, detach,
maximize, close; `HeaderPlacement::Floating` / `Detached`). No footer, no
imposed initial focus. Default sizes (`window_placement.rs`): workbook
373 x 420 (two thirds of 560), graph 560 x 460, matrix and note 560 x 420;
minimum 220 x 140. Opening: `open_tab` (`window_stack.rs`), from the Project Manager; a
second opening on the same entry brings the window to the front without
duplicating it. Each new window is offset in a cascade.

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
  (dialog key = `egui::Id`), in the preferences, 32 kinds at most;
  give a new window a stable and unique identifier `stokesforge_<nom>`;
- Esc: a text field then the dialog; the cross is the same as Cancel;
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
| Wizard | `import_window` (`import_wizard.rs`) | the only one; Back · Next · Finish |
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

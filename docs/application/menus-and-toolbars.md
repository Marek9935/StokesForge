# Application: menus, toolbars, status bar

Continuation of the table of contents [`README.md`](README.md). Contents: themed menus, toolbars, status bar. The menu grammar is in [`../menus.md`](../menus.md).

### Themed menus (#114)

The top menus and the right-clicks file the shared actions under
the same names. Shortcuts and toolbars keep their actions.
Checkboxes stay reserved for display toggles; they leave the
menu chain open. An action or an exclusive choice (language, type,
role, resolution) closes the whole chain. `MenuBar` uses
`CloseOnClickOutside`; the Object Manager background and its settings submenus
use the same behavior. Actions close explicitly
through `Ui::close`, whose request egui propagates to the parent menus.

| Menu | Organization |
|---|---|
| File | New → New Project, Add Workbook, Add Graph, Add Folder (#326, at the root) ; Import → Import…, Refresh from Source, Reopen Last Import… ; Open…, Save, Save As… ; Language ; Quit |
| Edit | Undo, Redo ; Selection → Copy, Paste, Rename, Duplicate, Delete of the Project Manager objects |
| Workbook (active workbook) | Sheet → Add Sheet, Rename, Duplicate, Delete ; Columns → Add Column, Type, Role ; Plot → Line, Scatter, Line + Symbol, Bar ; Show Metadata Row, Show x' Row (this workbook only) |
| Graph (active graph) | Show Data ; Add Text ; New Layer → Top X, Right Y, Top X Right Y ; New Layer Beside → Right, Below (new cell of the grid, #357) ; Selection → the menu of the selected object (disabled without selection) ; Reset Text Positions ; Top / Right Mirror Axis (checkboxes, ordinary active layer, #176) ; Reset Zoom ; Export → Export Graph…, Copy Graph, SVG Text, PNG Resolution ; Properties → Graph Properties…, Plot Area…, Axes…, Series…, Legend…, Theme… |
| View | Object Manager → the background settings (Show Axes, Show Objects, Expand by Default, Reset Settings) ; no Panels or Toolbars entries: toolbars are fixed, panels collapse or detach (#214) |
| Window | Maximize Window / Restore ; Detach / Reattach Project Manager |

The grammar of sections, separators and submenus is in
[`docs/menus.md`](../menus.md) (#152); each menu files its entries through
`MenuSections::section` (`app/menu_sections.rs`), which draws a line
only between two present sections. The shared paintings paint the
**content**; the top menu wraps it in a submenu that names the
target, the right-click displays it flat.

Right-clicks: the Project Manager reuses `object_menu` flat (a window:
Open · Close ─ Copy · Paste ─ Rename · Duplicate ─ Delete; a folder: Add Folder ─
Paste ─ Rename ─ Delete, the renaming being edited in place; Add Folder creates in
this folder, expands it and opens the name of the new folder for editing, #326). A sheet tab
offers Add Sheet ─ Rename · Duplicate ─ Delete, the empty strip Add
Sheet. The column letters: Plot ▸ · Add Column ─ clipboard ─
Type ▸ · Role ▸ ─ Clear · Delete (the columns themselves, `AppState::delete_columns`), at the first level (`column_properties_menu`),
the empty grid Add Column; the cells, rows and header fields a single
clipboard group then Clear · Delete, without Undo/Redo.

The Object Manager background displays flat the settings of
`object_manager_settings_contents` (also under View → Object Manager); its
nodes and the figure share `object_node_menu`, **the** menu of an object
(curve: Show Data ─ Rename, Move Up/Down, Hide ─ Delete ─ Properties…;
text, axis, object: see `docs/menus.md`). The right-click on the figure opens this
menu for the targeted object — a curve is selected by the right-click — and,
on the background or the plot area, Add Text, Insert Object ▸ (the shape is placed
at the clicked point, `GraphView::insert_shape_at`), Export ▸ and Properties…; Reset
Zoom no longer appears there. The contents and targets are identical in floating
and detached windows.

The tree filters do not change the visibility of the graph objects.
The checkboxes directly in the tree remain model visibility controls,
independent of these filters. The inventory of global settings and the
persistence questions for the future Preferences window are tracked in
issue #148; this rework does not add that window.


### Toolbars (#59, #60)

`src/toolbar.rs`. Bars of icon buttons, **fixed** (#108): their
content and place are frozen in the software, in the
`stokesforge_toolbars` panel under the menu bar. No handle, no drag-and-drop,
no floating or vertical bar, no narrow-window logic. Hard-coded
layout (`ToolbarId::row`): row 0 = File (new project, workbook, graph, folder, open…), Edit (Undo/Redo, #238: history label and shortcut in the tooltip, disabled = "Nothing to undo/redo"), Tools; row 1 = Text, wide,
alone on its row. The Object Manager bar is removed (#109): the panel
is collapsed or detached from its header. No bar is hidden
through a menu. An old `placement` field
is ignored on reading. The `//!` of the module says why the panel is an
`egui::Panel::top` (neither a raw `Window` nor `show_window_in_layer`).

- `ToolbarLayout` (pure, tested): visibility per bar and visible rows.
  `ToolbarState` is only the egui glue.
- A button is a `ToolbarButton`: icon of the common component, tooltip
  (label + shortcut + `.hint`), disabling. It pushes the same `Action` as the
  menu — never a second write. The content of each bar is
  `toolbar::bar_contents`; a gesture that is not an `Action` (file
  picker, import wizard, export) goes up as a `ToolbarRequest`, executed by
  `StokesForgeApp::run_toolbar_request` (`app/format.rs`) with the very
  methods of the menus. File
  bar (#61, #73): new project, workbook, empty graph, import,
  open, save, save as,
  export (disabled without a foreground graph).
- `Tools` contextual bar (#62), **always displayed** (#93): its content
  follows the foreground window (`ToolsContext`, computed by
  `StokesForgeApp::tools_context`, `app/format.rs`). Graph: the Pointer / Zoom / Text tools
  (`GraphTool` of the view, button pressed for the active tool), reset the
  zoom, properties. Workbook: add sheet, add column, plot the
  selection (curve, scatter, bars; disabled without a selected column). Matrix,
  note, no window: the graph tools stay there, **disabled**, with
  the tooltip `toolbar-tools.disabled` (the bar does not change shape).
  After a separator, it carries the **format of the selected object** (#64,
  merged into it by #93; `object_format_contents`), disabled without a selected
  object.
- `Text` bar (text format, #63), **always displayed** (#108) on its
  own row: without a foreground graph its buttons are disabled, with
  the tooltip `toolbar-tools.disabled` (like `Tools`, #93). Font, size, color, bold,
  italic, subscript, superscript, formula. Since the graph texts are already
  Typst markup, bold, italic, subscript, superscript, formula, color and size
  of a passage **write tags into the text** (`text_format::apply`,
  pure). The **font** is no longer a tag (#78): it is a field of the
  whole zone (`TextFont`, `AnnotationSpec::font`), set by `StokesForgeApp::format_zone`
  (`app/format.rs`) — editor draft for a free text being edited
  (`pending_zone_font_id`), `Action::SetTextFont` for a title or the legend,
  `Action::SetAnnotation` for a selected free text. A **selected** title, axis
  title or legend (`FormatTarget::Zone`, #77) takes only the
  font, the size and the **color of the whole zone** (`Action::SetTextColor`, #90;
  the other buttons are disabled). Target (`FormatTarget`, `StokesForgeApp::format_target`): a field being
  edited in place (free text, title, axis title, legend) → the request
  waits in the egui memory (`pending_format_id`) and the field applies it at
  its next paint on the highlight that egui kept, or inserts the tags
  at the cursor (`apply_pending_format`); otherwise a selected free text →
  the whole text, in **one** `Action::SetAnnotation` (color and size are
  fields of the model); otherwise the buttons are disabled. Bold, italic, subscript,
  superscript and formula are undone by applying them again. A click on the bar does not close
  the editor (`toolbar::over_format_bar`). Bold and italic also act
  on a whole **selected** title, axis title or layer title outside editing
  (#243, `format_zone`: one `Action::SetTextContent`, toggle like
  `text_format::apply`); the legend, having no text of its own, takes none.
  Width of a free text (#244): without a width, the text fits on one line;
  the two handles of the selected text (middles of the left and right edges,
  `GraphView::width_handles`) set `AnnotationSpec::width`, as a fraction of the
  figure, and the editor sets the alignment (`TextAlign`). A selected title, layer
  title or axis title has the same handles (`TextFont::width`,
  `GraphEvent::TextResized` → `Action::SetTextFont`); the legend has none. The
  three alignment buttons of the format bar (`TextFormat::Align`, a field of the box
  or of the zone, never a tag) act on the free text (draft while
  editing, otherwise `SetAnnotation`) and on the zone of a title.
  The lists display the font and size of the selected zone (#97),
  or those of the draft during editing, including in a detached window.
  An inherited size displays its effective size from the shared pipeline;
  the "Auto" entry is then checked. Choosing it removes the explicit size
  of a title or the legend, and restores the default size of a free text.
  The "Graph font" entry is checked for an inherited font, while the
  field displays the name of the graph font. "Auto" reuses the common
  key `common-auto` of both languages.
  A local font or size setting in the markup displays the neutral label
  "Font" or "Size": the bar summarizes the zone without interpreting the
  computed Typst expressions. The parser distinguishes these settings from plain
  text, comments and raw code blocks.
  The selector also displays the color of this zone (#99), never the last
  choice made on another zone. A passage carrying a local color gives
  a neutral swatch. While editing, the selector draft lasts only
  until the gesture is confirmed, which sets a single tag on the passage.
- Visibility: no menu hides a bar (View carries only the Object Manager settings, #214). The `toolbars` field of `preferences.json` (never the `.stf`) is still read; reading
  ignores what it does not understand. `StokesForgeApp::new` does not read the
  file (the tests), `main` calls `load_toolbar_layout`.
- The bars are painted only in the main window, not in the detached
  OS windows.


## Status bar (#230)

`AppState::notice` (information), `warn` (warning) and `fail` (error,
except in a modal dialog: S-M7) set a leveled `StatusMessage`
(`StatusLevel`) and add it to a history of `STATUS_HISTORY_LEN` = 50
messages. An information message clears after `INFO_LIFETIME` (8 s):
`AppState::expire_status`, called on every frame by
`sync_window_title_and_status` (`app.rs`), clears the expired one and returns the remaining
delay — only this instant schedules a repaint, no wake-up when idle.
Warning and error stay until the next message. A click on the
bar (`Action::OpenStatusHistory`) opens `status_history_window`. A
script exception is an error ("Script failed. See the console"). The
bar no longer displays "Revision N" or "Reopen Last Import…".

The window title is `AppState::window_title` (file name, "•" as
long as there is unsaved work), sent through
`ViewportCommand::Title` when it changes. The history is timestamped in UTC
(no local time zone without a calendar dependency).

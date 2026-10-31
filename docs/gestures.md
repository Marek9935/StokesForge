# Mouse gestures: the matrix (#299)

The mouse equivalent of [`menus.md`](menus.md): **what each gesture does on
each surface**. A new surface, or a new gesture on an existing surface, is
filed here; a gesture that differs from the same action elsewhere is
reported against this page. Keyboard shortcuts are in
`app/shortcuts.rs` (registry) and `docs/application/README.md`, not here.

This page describes the **real state of the code** (surveyed on 2026-10-20, then replayed
in the real application: see "Verification"). The analysis preview is
aligned with the graph since #270 (D3 of #299).

## The model

- **The object first**: you click or drag the object itself (a curve,
  a cell, a handle), you do not go through a mode.
- **Tools live in the Tools bar** (Pointer, Text, Zoom, shapes) and
  nowhere else. The only justified exception: the analysis tools of the
  analysis window (see "Guard" below).
- **The same gesture, the same action**, from one surface to another: Ctrl + wheel
  zooms, double-click opens the settings or reframes, right-click opens the menu
  of the targeted object (and selects it if it was not, cf. `menus.md`).

## Legend

`Ctrl` means `Command` on macOS (`modifiers.command`). "—": the
gesture does nothing on this surface.

## The graph (figure)

Source: `render-egui/lib.rs` (navigation, objects), `render-egui/shape_tools.rs`
(shapes); the menu is set by `app/graph_window.rs`. A plain drag depends on the
**active tool** of the Tools bar.

| Gesture | On | Effect |
|---|---|---|
| Click | curve, free text, shape, title, legend | selects the object (only the Pointer tool selects a curve) |
| Click | empty area | deselects; activates the clicked layer; Text tool: places a text |
| Drop of columns | right or bottom band of the figure | new cell beside the nearest one (right; bottom-right corner: right), with the series (#357) |
| Drag | left or top edge of the figure (#405) | sets **Left** / **Top**, the distance from the image edge to the first plot area (never below what the axes and the title need); the graph window grows to the left or upward so the plot area stays still on screen; live preview, one undo step at release; top-left corner does both. The strip straddles the edge, 4 screen points each way, so it can be taken from the margin the window leaves around the figure (`FIGURE_GRAB_MARGIN`), where the window's own resize border never reaches; Undo and Redo of the step move the window back the same way. Dragging inward below what the axes need, or a start from a graph of an old file, changes nothing and sends no step. A click on the axis line still selects the axis |
| Drag | outer boundary of a cell (beyond the numbers and the axis title; right and bottom boundaries only, #394) | changes the **plot area** size of that column and/or row by the distance dragged: the graph grows or shrinks by it and its window follows; live preview, one undo step at release. The boundary two cells share is the handle of the left column (the upper row); the right edge of the figure is the last column's, the bottom edge the last row's, so a one-cell graph has handles too; the corner pulls both. Resize cursor on hover. A click on the axis line still selects the axis |
| Drag | any edge, with **Layer placement = Free** (#398) | — (no handle in Free until the mouse gestures of the free mode; the sizes are set in Graph Properties › General › Size). A click still activates the layer drawn under the cursor |
| Drag | plot area, Pointer or Text tool | — (nothing: no more pan on a plain drag, #318) |
| Drag | an axis | — (nothing; panning a scale is done with Ctrl or middle click) |
| Drag | free text, shape, title or legend, Pointer tool | moves the object (a width handle resizes it) |
| Drag | plot area, Zoom tool | zoom rectangle (beyond 4 px) |
| Drag | plot area, shape tool | draws the shape; a click without movement places the default size |
| Double-click | free text, title, legend entry (one series name) | in-place editing; the legend frame between entries does nothing |
| Double-click | curve, shape, axis (its numbers or its line, #385) | opens its settings; on the axis line the axis beats a curve lying against the edge |
| Double-click | empty area (away from an axis line), margins | reframes (resets the zoom) |
| Wheel | — | — (the wheel alone does not zoom, #62) |
| Ctrl + wheel (or pinch) | plot area | zoom anchored under the pointer |
| Ctrl + wheel | an axis | zoom of this scale only |
| Ctrl + drag, or middle click + drag | plot area, any tool, including over a text, a title, a shape, the legend or an analysis handle | moves the view (pan); no zoom rectangle, no shape drawn, no object moved; outside the graph frame: — |
| Ctrl + drag, or middle click + drag | an axis | moves only the scale of this axis (same gesture as the analysis preview, #318) |
| Ctrl + click | plot area, Text or shape tool | — (no text placed, no shape, no return to the Pointer) |
| Right-click | any object or the area | selects the target and opens its menu |
| Shift + wheel | — | — |

## The analysis window preview

Source: `app/analysis_preview.rs` (preview), `app/analysis_handles.rs`
(handles on the graph). The gesture of a plain click and drag comes from the
analysis: the window shows a **Point / Zone** selector (two outlined texts
that toggle, a justified exception of #299) only for peak
detection; the other analyses have a single gesture, with nothing to choose. There is
no longer a Zoom tool or a reframe button.

| Gesture | On | Effect |
|---|---|---|
| Click | preview, Point gesture | adds a point, a peak seed or a bound depending on the analysis (Shift: snapped to the curve) |
| Drag | preview, Zone gesture | draws a zone or a range |
| Double-click | preview | reframes (original frame) |
| Wheel | — | — (the wheel alone no longer zooms) |
| Ctrl + wheel (or pinch) | plot | zoom anchored under the pointer, same factor as the graph |
| Ctrl + wheel | an axis (X ticks under the plot, Y ticks on the left) | zoom of this scale only |
| Ctrl + drag | plot | moves the view (the content follows the cursor, inverted axis included); does not draw a zone; the modifier is read at the start of the drag |
| Ctrl + drag | an axis | moves only this scale |
| Ctrl + click | — | — (nothing) |
| Shift + wheel | plot, whatever the gesture | changes the snap average (1 point per notch, one raw wheel event per notch); the snap circle grows with the averaged area; the frame does not move |
| Right-click | preview | removes the nearest point or zone |
| Drag | a handle on the graph (`app/analysis_handles.rs`) | moves the handle of the computation; immediate preview |

## The grid (workbook)

Source: `app/grid.rs`. The wheel scrolls the sheet (egui bars);
Ctrl + wheel is not taken.

| Gesture | On | Effect |
|---|---|---|
| Click | cell | selects the cell |
| Ctrl + click | cell | adds or removes the cell |
| Shift + click | cell | extends the last rectangle |
| Drag | cell | rectangle selection |
| Ctrl + drag | cell | adds a rectangle, the previous ones stay |
| Double-click | cell or header field | in-place editing |
| Click, Ctrl, Shift, drag | column letter | selects, toggles, extends or adds columns (same modifiers) |
| Click, Shift + click, drag | row number | selects the rows |
| Click | corner box "#" | selects all |
| Drag | left or right edge of a column selection, under the row of letters (or top or bottom edge of a block of rows, if the sheet has an x' row) | drops a plot on a graph |
| Right-click | cell, header, letter, row | if outside the selection, replaces it with the target; opens the selection menu |
| Right-click › Reset Width | column letter (or Workbook › Columns) | resets the selected columns to the default width (54); not undoable, disabled without a stored width (#330) |
| Right-click › Reset Column Widths | sheet tab (or Workbook › Sheet) | resets all the columns of the sheet, and the row-number gutter, to their default width (#330) |
| Right-click › Move ▸ | column letter (or Workbook › Columns) | moves the selected columns within their sheet: to the start, to the end, or one place left or right each (the gap between non-contiguous columns stays); a single Ctrl+Z; no drag (dragging on the letters already selects) |

## The trees: Object Manager and Project Manager

Sources: `app/object_manager_ui.rs`, `app/list_row.rs`, `app/reorder_list.rs`,
`app/project_manager.rs`. The whole row responds, except the visibility box.

| Gesture | On | Effect |
|---|---|---|
| Click | row (OM) | selects the object (and activates it, for a layer) |
| Delete | layer node (OM) | deletes the cell and its overlay layers (an overlay layer alone); the last cell is refused (#357) |
| Click | row (PM) | window: selects and opens; folder: selects and expands it if closed, without collapsing it if open (#170) |
| Ctrl + click | row (PM) | toggles the row in the selection |
| Shift + click | row (PM) | range anchor → target in tree order, collapsed folders included: the folder and its windows enter the selection and the action (Delete, Open, Close; Copy refuses a folder) (Ctrl + Shift: adds the range) |
| Slow click (click on the entry already alone selected, then delay elapsed without a second click) | row (PM) | renames, like F2; a quick double-click does not rename |
| ↑, ↓ | row (OM, PM focused) | selects the previous or next visible row, without opening (S-K9); Shift+↑/↓ and Alt+↓ have no effect |
| Enter | row (OM, PM focused) | PM: opens the window, expands or collapses the folder; OM: opens the settings of the selected series or object (like the double-click) |
| Double-click | row (OM) | opens the settings of the object |
| Double-click | row (PM) | window: activates or restores it, never closes it; folder: collapses it (without a modifier) |
| Drag | row (OM) | reorders; on a graph, moves the object |
| Drag | row (PM) | — (PM rows cannot be dragged: click only) |
| Right-click | row | if outside the selection, replaces it; menu of the object (a single menu, `menus.md`) |
| Right-click › Add Folder | folder (PM) | creates a folder "FolderN" in this folder, expands it and puts the name of the new folder in in-place editing (creation and name = two history entries) |
| Right-click | OM background | panel settings |
| Wheel, Ctrl + wheel | — | scrolls the list; Ctrl + wheel is not taken |

## Tabs (tab strips)

Source: `app/tab_strip.rs`.

| Gesture | On | Effect |
|---|---|---|
| Click | tab | activates |
| Double-click | tab (renaming strip) | renames in place |
| Right-click | tab | tab menu |
| Right-click | empty strip or add button | add menu |

## Windows

Sources: `app/window_chrome.rs`, `app/dialogs.rs`, `app/window_detach.rs`,
`app/window_stack.rs`.

| Gesture | On | Effect |
|---|---|---|
| Drag | title bar | moves the window; outside the application, detaches it |
| Drag | edge or corner | resizes |
| Click | title bar icon | minimizes, detaches, closes (depending on the icon) |
| Click | window behind | brings it to the front (the click also acts on what it targets in the window) |
| Click outside a modal dialog | background | blocked; the outline blinks |

## Fields and selectors

| Gesture | On | Effect |
|---|---|---|
| Wheel | hovered number field (`number_field.rs`) | changes the value by one step, or by one entry of the proposed list; the wheel is consumed (the window does not scroll) |
| Ctrl + wheel | number field | not taken (reserved for zoom) |
| Click | field arrow | opens the list |
| Click, drag | square, wheel, bar of the color picker (`color_picker.rs`) | chooses the color |
| Right-click | swatch of a saved color | removes it |
| Right-click | row number of the import preview (`app/import_wizard.rs`) | row menu |

## The guard

Two guards of `crates/stokesforge-app/tests/guards.rs` hold this page.

- **`no_tool_picker_outside_the_tools_bar`** (#299, D1, D5): a
  choice (`choice`, selectable button, radio) whose arguments name a
  tool (`tool`) is refused outside `toolbar.rs`, unless it carries, in the
  twelve lines that precede it, the exemption marker
  `// TOOL-IN-WINDOW(#299): <justification>` with a non-empty
  justification. A departure from the guideline is therefore justified in the code, and here.
- **`no_gesture_outside_the_matrix`** (#299, D4): every production file
  that reads a wheel (`scroll_delta`, `zoom_delta`, `MouseWheel`) or a
  double-click (`double_clicked`) must be cited below; a cited file that
  no longer exists also fails the guard. It does not judge the gesture, it
  forces it to be filed in the matrix.

Surfaces read (paths relative to `src/`; `render-egui/` for
`stokesforge-render-egui`, `app-state/` for `stokesforge-app-state`):

- `render-egui/lib.rs`, `render-egui/shape_tools.rs`: the graph;
- `app/analysis_preview.rs`, `app/analysis_handles.rs`: the analysis preview;
- `app/grid.rs`: the grid;
- `app/object_manager_ui.rs`, `app/project_manager.rs`: the trees;
- `app/tab_strip.rs`: the tabs;
- `app/window_chrome.rs`: the windows;
- `number_field.rs`, `color_picker.rs`: the fields;
- `app/import_wizard.rs`: the import preview.

## Verification (2026-10-20)

**Graph, before #318**: the lines below that say "drag = pan" and
"drag an axis" describe the behavior **from before #318** (the Pointer
moved the view on a plain drag, without reading Ctrl). Today: Ctrl or
middle click + drag, covered by the interaction and session tests of
stokesforge-render-egui (see docs/tests.md), replayed in the application after #318. The gesture is read
at press time: Ctrl released along the way, the pan continues; pressed along the way, nothing.

Replayed in the real application (`run-stokesforge` driver, X11 under
Xwayland, mouse and wheel injected through XTEST). One line per gesture of the
matrix; "read in the code" = not replayed.

**Analysis preview**: the lines below that concern it (wheel = zoom,
Zoom tool, Ctrl + drag = zone) describe the behavior **from before #270**;
the preview table above is today's, covered by the tests
of `app/analysis_preview.rs` (`zone_tests`) and verified in the application after
#270. Shift + wheel: egui files the wheel under Shift in the horizontal axis
(`smooth_scroll_delta.x`); the preview reads both axes of the raw events (one per notch; `smooth_scroll_delta` spread one notch over several frames), otherwise the average
never changed for real.

- **Confirmed**: click on a window behind; graph: Wheel alone without
  zoom, Ctrl + wheel (area, anchored), Ctrl + wheel on an axis (this scale
  only), drag = pan (before #318), drag an axis (before #318), Zoom tool (rectangle), double-click
  curve (settings), double-click empty area (reframes), click and drag of the
  legend; grid: click, Ctrl + click, Shift + click, drag, Ctrl + drag,
  column letter (click, Shift, Ctrl), row number (click, Shift), box "#",
  double-click cell (editing), right-click, drag the edge of a column
  to a graph (series added); PM: click, Ctrl + click, slow click
  (renames); tabs: click, double-click (renames); windows: drag the
  title bar, resize by the corner, click outside a modal dialog
  (blocked; the blinking of the outline was not observed); analysis
  preview (Integral window): Wheel (zoom), Ctrl + wheel (no
  effect), drag in Zone tool, drag in Zoom tool, double-click (reframes),
  right-click (removes the zone).
- **Fixed**: Ctrl + drag of the graph (pan, not "—"); Ctrl + drag of
  the preview (draws a zone); drag a PM row (nothing, no
  drag and drop); drag edge of the grid (left/right edges under the
  letters); slow click (triggered by the delay, not by a second click).
- **Not replayed, read in the code**: Shift + wheel of the graph; click
  Text and shape tools; double-click of a text, the title, a legend entry, a shape, an axis;
  width handle; drag of the analysis
  handles on the graph; preview: Point tool (disabled in Integral),
  Shift + snapped click, Shift + wheel; Ctrl + wheel of the grid (sheet
  too short to scroll); double-click and right-click of the PM rows;
  the whole Object Manager (drag, double-click, right-click); right-click on
  tab, strip, add button; title bar icons, detaching;
  PM Shift + click; number fields (wheel); color picker; import
  preview.

**Re-read against the code (#381, 2026-10-29), not replayed in the application**:
the double-click of a legend entry edits that series name in place
(`render-egui/lib.rs` `GraphEvent::EditLegendEntry`, handled in
`app/graph_window.rs`); a double-click on the legend text target itself returns
without an event. The Object Manager rows match the code: click and
right-click select, double-click and Enter open the properties
(`app/object_manager_ui.rs`), arrows move the selection and Shift+arrows and
Alt+Down are swallowed (`app/list_row.rs` `read_navigation`), the background
right-click opens the settings. Not confirmed: an actual drag-and-drop
reorder of OM rows with the mouse (read only through the unit tests), the
Ctrl + wheel of the grid, and the right-click menu of the import preview row
number (the code attaches a context menu to the row number, `app/import_wizard.rs`,
but its content was not read).

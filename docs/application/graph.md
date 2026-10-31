# Application: graphs

Continuation of the table of contents [`README.md`](README.md). Contents: plotting from a workbook, dragging columns or rows to a graph, graph properties, rules shared with rendering.

### Plotting from a workbook

In addition to the types (Numeric, Mixed) and the "Role" submenu, the row of
letters offers a "Plot" submenu (Line, Scatter, Line + Symbol, Bar) that creates a graph from
the current column selection. It has the same target as the other two
submenus (`target_columns`: the whole selected range if the clicked column is
part of it, otherwise that column alone). The new graph is added without a
title in the folder that contains the workbook (`Command::AddGraph`, no
separate entry for the sheet) and opens at once in its own tab.

The pairing of columns into series follows the Origin convention rather than an
arbitrary choice: among the targeted columns, each column with the Y role
(`ColumnRole::Y`, `ColumnMeta::role`) becomes a series, paired with the
nearest X column to its **left** in the sheet. Unselected X columns count
too, since the X shared by several curves usually has no reason to be
clicked again at each plot. A Y column with no X to its left keeps an implicit
X (the row index). A column of any other role (X, Z, error bar, label,
none) never produces a series on its own. A selection with no Y column
therefore creates no graph: a right-click on a range of purely X columns, for
example, does nothing rather than opening an empty graph. Each series takes the
palette color of its rank (`SeriesSpec::from_palette`).

**Error columns** (#360): the first column with the Y Error role
(`ColumnRole::YError`) to the **right** of a Y column (Y2, Y3, Y4 too), before
the next ordinate column (any of Y, Y2, Y3, Y4), fills `SeriesSpec::y_error`;
the same for X Error and `x_error`. An unselected error column counts too, like
X; an error column to the left of its Y pairs with nothing. The bars are drawn
on **every** plot type that has points (Line, Scatter, Line + Symbol, Bar,
Error Bars) as soon as the series carries an error column; a Histogram has no
point to put a whisker on and ignores it. Two Y columns in a row each need
their own error column right after them.

This rule lives in `stokesforge_core::compose::series_from_columns` (#257:
`pyst.plot` and the appendix of an analysis apply it too, `x_for` and
`errors_for`);
`AppState::plot_columns` (`stokesforge-app-state/src/state.rs`) only emits the
command and the failure messages, and the context menu only chooses the plot
type and pushes `Action::PlotColumns`. A second
action, `Action::AddSeriesToGraph`, applies the same pairing rule but
re-reads the current `PlotSpec` of an existing graph to **add** series to it
(`Command::SetPlotSpec`) without touching those already there. It serves
the drag and drop from a workbook to an open graph, described
below.

### Dragging a column selection to a graph

A second path to `Action::AddSeriesToGraph`, complementary to the "Plot"
submenu: you **grab the edge** of a block of selected columns, like an Excel
selection, and drop it on an open graph (#160).

**Precondition.** A selection of **whole columns** (row of letters,
corner box, Ctrl+A; non-contiguous included, #159):
`AppState::column_drag_payload` returns `None` for cells, rows or a
header field. The gesture carries **all** the selected columns, in
sheet order.

**Grip.** `app::grid::column_grip_bands` (a pure function, tested alone) places
a thin band (`COLUMN_GRIP_HALF_WIDTH`, 3 pt on each side) on each outer
edge of a block of consecutive selected columns (two
pairs for two separate blocks), from the first header field to the bottom of the
grid. The **row of letters is excluded**: its column edges are used
for resizing (`egui_extras`), which keeps priority. The bands are
placed after the table (so above the cells) but exist only on these
edges: the selection-extending drag of `cell_widget` keeps the whole
surface of the cells. The cursor is `Grab` on hover and `Grabbing` during the drag;
a marker (icon from the icon component + number of columns) follows the
pointer; Esc cancels (`egui::DragAndDrop::clear_payload`).

**Payload.** A typed `state::ColumnDragPayload { sheet, columns }` (not a
string), carried by the native mechanism of egui 0.36
(`egui::Response::dnd_set_drag_payload`, `egui::DragAndDrop`).

**Target.** Each `Tab::Graph` is wrapped by `app::graph_window::graph_drop_target`
(called by `tab_contents`, the only entry point shared by the dock and the
detached tabs): a hover with an active `ColumnDragPayload` draws a
discreet outline around the graph; a release above it converts the payload
into `Action::AddSeriesToGraph` via `state::drop_payload_action`. Releasing
elsewhere does nothing. The action carries the targeted **layer** (`panel`) and
**does not impose a plot type** (`plot_type: None`): the series take
the type of the **last series** of the layer, `Line` for an empty layer
(`compose::panel_plot_type`). A single undoable command (`SetPlotSpec`).
The targeted layer is the graph's **active layer** (#161, below):
`graph_drop_target` and "Send to" read `published_active_panel`.

### The x' row and dragging rows to a graph (#164, #166)

**The x' row.** Fifth field of the stacked header, **above** the long name
(`HeaderField::XPrime`, rank -5; `HeaderField::COUNT` gives the number of
fields, `rank`/`from_rank` the mapping with the header/data axis): one
**numeric** value per column (`ColumnMeta::x_prime`), entered like the
other fields. An empty text clears it, and a text that is not a finite number
is refused (status bar, nothing is written). `Action::SetColumnXPrime`, copy,
paste, clear and Tab follow the other fields; the mutation remains
`Command::SetColumnMeta` (one undo step). **Import**: subheader role
`x_prime` (`SubheaderRoles::x_prime`, row "x'" of the wizard,
code `X'` of the preview), read with the numeric format of the recipe.
**Metadata**: the metadata window of a column offers, in front of each numeric
value of the file, "Use as x'" (the mini selector: the chosen key
feeds the x' row of this column).

**Dragging rows.** The same gesture as for columns, on **whole rows**
(row-number gutter): `AppState::row_drag_payload` offers it only
if the sheet has at least one x' value. The grip is on the top or bottom edge of a
block (`app::grid::row_grip_bands`, pure, the edges are read from the selection
and not from the painted rows; the gutter stays free), `Grab` /
`Grabbing` cursors, "n rows" marker, Esc. The payload is `state::RowDragPayload`, the target is
`graph_drop_target` → `Action::AddRowsToGraph { axis: RowAxis::Ask }`. **One
row = one series** (`SeriesSpec::row`), named "Row n" on creation. Axis:
a graph **without X** (no series with a column X and no row series) receives
the series on the main X axis of the active layer, without a question; a graph
**with X** opens the choice dialog of `app/confirmations.rs`
(`modal_dialog_with`, called by `StokesForgeApp::blocking_confirmations`): "Create Axis" (default) adds a layer overlaid with
an X axis at the top (`OverlaySpec { x_top, y_right: false }`), "Use Present Axis"
keeps the targeted layer, "Cancel" abandons (`Action::ConfirmRowDrop`). A
single `SetPlotSpec`: a single undo step, layer included. **Menus**: on a
selection of whole rows of a sheet with x' (right-click on a row number,
Workbook menu), Plot creates a graph of rows (`Action::PlotRows`, same plot
types) and Send to sends the rows (same axis question as on drop);
the column menu is no longer offered in this case. Without x', a selection of
rows keeps the menu of the columns they cross.

**Analyzing a row (#253).** `Analysis ▸` is active on a "row" curve (right-click
on the curve, Object Manager, Graph › Selection, same menu everywhere) and on
whole rows of the gutter (Analysis menu of the workbook, right-click on the numbers). The
gesture does not analyze the row: it creates or completes the **linked transposed sheet**
("Sheet1ᵀ"), then opens the window on its "Row N" column. The result follows a column
(results workbook, "Replace original data" acts on the column of the transposed sheet,
no series is added to the graph of rows). A group that mixes column curves and
row curves is disabled (`analysis-needs-same-curves`). A Y column of the sheet without x', or
a text column, makes the gesture refuse, naming it by its letter. The transposed sheet carries
a band at the top of the grid (`app/transpose_ui.rs`): origin, "Out of date" when the
source has changed, "Values edited" when a value of a linked column has been edited, and
Recalculate (Relink with confirmation in the latter case). Model details:
[`../data-model.md`](../data-model.md), "Linked transposed sheets (#253)".

**Overlaid layers (#167).** Graph › New Layer (Top X, Right Y, Top X Right Y)
adds, through `Action::AddLayer`, a `PanelSpec::overlay_on` to the base layer of the
active layer. The columns with role **Y2, Y3, Y4** (paired with the nearest **X2, X3, X4** to
their left) go, when the graph is created (Plot) as well as on drop or "Send
to", each to its own numbered layer (X axis at the top, Y axis on the right, own
scales; the axes stack outward, the oldest against the plot
area, #168), reused if it exists. "New Layer" is disabled at three
overlaid layers per cell; a layer with two axes takes the first free number. Zoom, pan and the zoom rectangle act on the
**scales**: the gesture, expressed in pixels of the plot area (shared by the
layers of a cell), applies to all the own scales of all the
layers of the cell (X, Y and y2 of the ordinary layer; top X / right Y of
an overlaid layer), so the ticks stay aligned. A
linked scale has no framing of its own (`Framing` per scale, `Gesture`,
`GraphView::commit_gesture`). **A single axis**: the wheel (Ctrl) and the drag
over an axis (line and ticks, not its title) frame only that scale
(`commit_axis_gesture`); the target of a drag is frozen at its first
frame (`NavDrag`), so the cursor can leave the axis. The other scales, linked
or not, do not move. The zoom rectangle stays a plot-area gesture. Rendering: `docs/rendering.md`.

**Active layer (#161).** `GraphView::active_panel(spec)`: the last layer
selected, an interface state outside the `.stf` and outside history, carried by the view
(so identical in a detached window), revalidated on read (layer gone:
the first one). It changes on a click, a right-click or the start of a drag in
the plot area of a layer, on the selection of an object of the layer (figure or
Object Manager, `follow_selection`), on a click on the node of the layer, of its
group or of its axes in the Object Manager, and on the **numbered marker** painted
at the top left of the layer's own **cell** (`layer_badges`, #395: the outer cell
rectangle drawn by the render, `GraphView::cell_rect_screen`, so it follows live
handle previews and the contained scale; overlays of one cell side by side in
layer order; a layer not drawn keeps the window corner; never in the export or
the copy). The window publishes the active layer in egui's memory
(`published_active_panel`) for the menus that do not have the view. These follow the
active layer: the drop of columns, "Send to" and Graph › Add Text; the
gestures that designate a point (right-click, tools) target their own layer.

**Between two system windows: no drag (#165).** The drop works within one
OS window (dock or detached window), not from one to the other: under
Wayland the source window keeps the pointer while the button is held, the
graph of the other window sees neither hover nor release, and a real
`wl_data_device` drag and drop would cost several days. Decision: no
cross-window drag, replaced by **"Send to <graph>"**
(`app/graph_targets.rs`), the same `Action::AddSeriesToGraph` as the drop, to
**every open graph**, attached or detached. It sits next to "Plot",
in the right-click of the column letters, in that of any cell of a
selection of **whole columns or rows** (cell, row number,
header field, corner: same menu as the letters, applied to the columns of the
selection, `selection_context_menu`) and in the Workbook menu. From three
graphs on, a "Send to Graph" submenu; with no open graph, the entry is
absent.

### Graph properties

The settings of a graph live in a **floating window with tabs**
(`app::plot_settings::plot_settings_window`), a **modal dialog** (`modal_dialog`, `src/app/dialogs.rs`):
in the foreground, the rest of the interface no longer receives any click, and a
click elsewhere makes its outline flash dark red
(`theme::blocked_flash_color`). Two buttons at the bottom: **OK** (default,
Enter outside text input) and **Cancel** (also the cross and Esc); their
texts come from the catalog (`dialog-ok`, `dialog-cancel`). The
gestures apply live to the graph (preview) but inside a **history
macro** opened when the window opens: OK closes it, giving a
single entry, so one Ctrl+Z undoes everything (`AppState::close_plot_settings`);
Cancel aborts it and restores the previous graph
(`AppState::cancel_plot_settings`). During the macro, undo/redo and
save are refused (`stokesforge-commands`), as are the writes of a
detached window (another sender identity: refused by the session) and the
actions it sends up to the root (`reject_detached_actions`). Every
grouped operation ends with `AppState::end_grouped_macro`, which refreshes
the snapshot (otherwise `can_undo`/the label stay stale, issue #22). The window is built
from the same building block as the content windows (`show_window_in_layer`,
`window_header`, `floating_resize_handles`), with the same frame and the same title
bar, and not from a raw `egui::Window`; only the layer order and the
full-screen layer that captures clicks set it apart. This layer is
**invisible**: no dark veil over the rest of the application, so the
graph rendering stays visible during the adjustment (issue #20). Its tabs are the
same strip as those of the sheets (`tab_strip`, issue #19): this is the project
standard for any set of tabs. The detached OS
windows, other viewports, are not covered by this blocking. The window
takes no space while it is closed.
It replaces the former fixed panel on the right of the workspace, displayed
permanently as soon as a graph was active. Its state (which graph, which
tab, which axis) is `state::PlotSettings`, in `AppState`; it is opened
by `Action::OpenPlotSettings { graph, tab, axis }`.

**Double-click on the figure** (#113): on an axis, a curve or a graphic
object, it opens their settings (`GraphEvent::OpenAxisProperties`,
`OpenSeriesProperties`, `OpenShapeProperties`, translated by
`graph_window::annotation_interactions`); on a text, it keeps in-place editing;
in the bare plot area, it resets the zoom. In the Object Manager,
every row (graph, layer, groups, series, axes, objects) opens its
settings on double-click. On a graph with several layers, a double-click
on an axis and its context menu open the Axes tab of the targeted layer,
with the same `Action::OpenObjectProperties` as the Object Manager. The address
is that of the layer in the grid, independent of the rank of the plotted panels.

**Axes tab: four sides (#396).** The strip at the left always lists Bottom,
Left, Right, Top, on an ordinary layer as on an overlaid one: an axis is named
**where it is drawn**. Which `PanelSpec` field a side edits, and whether another
layer holds the side, is one rule, `stokesforge_app_state::cell_sides`
(`side_use`, `sides`, `place_of`, `usable_place`), shared with the Object
Manager (`AxisRef::is_mirror` / `blocked` ask it; its visible behavior did not
change). An address in the spec is a **field** (`AxisRef`, `TextTarget`, hit
test of the figure); every opener turns it into a side with `place_of`
(`Action::OpenObjectProperties`, `graph_window::plot_target` for the header of the
context menu).
- Ordinary layer: each side edits its own field; Top is always a mirror, Right
  a mirror while no series is attached to it (a real axis otherwise).
- Overlaid layer (with an ordinary layer to cover): **Top** edits its X axis
  (`bottom`), **Right** its Y axis (`left`); **Bottom** and **Left** belong to the
  covered layer and are greyed out. Its `top` and `right` fields are never drawn
  and never edited. These two tabs carry the layer's **Own scale** box
  (`OverlaySpec::x_top` / `y_right`) beside **Show axis** (`AxisSpec::visible`):
  unticked, the scale fields and Show axis are greyed (tooltip
  `disabled-axis-own-scale-off`) because the layer follows the covered layer.
  Hiding the axis never moves the curves: the scale is kept.
- A side is **taken** when another layer draws there: the Top / Right of the
  ordinary layer while an overlay draws its axis (`x_top && bottom.visible`,
  `y_right && left.visible`) on a mirror; the overlay's Top / Right while the
  ordinary layer displays its mirror; the overlay's Bottom / Left always. Two
  overlays never take a side from each other (their axes stack outward, #168).
  The tab is greyed with a `disabled-axis-side-*` bubble that says who holds the
  side and what to do (S-G1, S-G2). A real right axis is never taken.
- The chosen tab falls back, when greyed, to the other side of the same axis
  (Bottom/Top, Left/Right), then to the first available tab
  (`usable_place`, applied by `axes_settings_page` on every frame).
- Conflict: creating an overlay axis at the top or on the right while the
  mirror is displayed (Graph › New Layer, drop of rows with "Create Axis",
  Y2/Y3/Y4 plotting) switches the mirror off **in the same command**
  (`switch_off_mirrors_under_overlays`): one Undo gives both back. An old file
  with both keeps the overlay drawn and the mirror blocked; the model is not
  changed on load, and the mirror tab is available again once the overlay hides
  its axis.
- The hit test reports the field it drew: `hit::AxisSide::Top` for the top mirror
  of an ordinary layer (so a click on it opens the Top tab), `Bottom` / `Left` for
  an overlay's X / Y axis, which `place_of` turns into Top / Right.
- In the Object Manager and the Graph menu, a mirror row is greyed while an overlay
  draws its axis there, and an overlay's axis row is greyed while the mirror is
  displayed; both use `VisibilityTarget::blocked_reason` (the tab bubble's text).
  The Graph menu mirror toggles already went through the same `visibility`, so they
  cannot turn a mirror on under a displayed overlay axis.
- Each axis of an overlay keeps its own colors on the figure: the top axis those of
  `bottom`, the right axis those of `left` (`pipeline.rs`).

**Titles, axis titles and legend** (issue #53): they are targeted like a free
text. Double-click on a title or an axis title → in-place editor
(`app::graph_text_edit::text_editor_ui`, one field; Enter or a click elsewhere confirms, Esc
cancels); on a legend entry → renames the series (`SetLegendEntryName`,
empty = name derived from the column); on the legend frame, nothing (it has no
text of its own). A drag moves the text (anchored to the data: it follows
the zoom); a right-click offers "Edit Text" and "Reset Position" (disabled while
the text is in place), and "Graph" → "Text" → "Reset Text Positions" resets everything
at once. Each confirmed gesture is **one** `Action` (`SetTextAnchor`,
`ResetTextAnchors`, `SetTextContent`, `SetLegendEntryName`), so one
`SetPlotSpec`, one history entry; the position is saved in the
`.stf` and applies on export.

**Free texts** (issue #35): right-click in a plot area →
"Add Text" at the clicked point, or "Graph" → "Text" → "Add Text"
(at the center of the first panel). The text is edited **in place**: a field placed
over the text, and above it a color / size / rotation bar
(`app::graph_text_edit::annotation_editor_ui`); Enter or a click elsewhere confirms, Esc cancels.
The field is written **in the rendering frame** (issue #37): centered on the center
of the rendered text, at the font size of the text (body × display scale of
the figure), upright even if the text is rotated. Its placement is the pure
function `stokesforge_render_egui::annotation_field`. Since an emptied text is no longer plotted,
the editor keeps the last known frame. While you type, the field has a light
background; as soon as it loses focus (you adjust the rotation or the size), it becomes
transparent and the **rotated** preview is visible (issue #36); a click in its frame
gives control back to the input. The final size differs slightly after
recompilation by Typst (a `$…$` formula is typeset differently from its source).
Double-click or right-click → "Edit Text" reopen the editor; a click
selects (blue outline), Del or "Delete" removes it; a drag
moves it. The preview lives in the view (`GraphView::set_annotation_preview`) and
the editor state in egui's memory, under the identifier of the view: the same
code for the floating window and the detached window, nothing in
`AppState`. Each confirmed gesture is **one** `Action::SetAnnotation` or
`RemoveAnnotation`, so one `SetPlotSpec` or `EditPlotSpec`, one history entry. The saved
colors are published every frame in egui's memory
(`color_picker::publish_saved_colors`, read through `color_picker::saved_colors_id`) so that the editor's picker offers them too
in a detached window.

**Graphic objects** (issue #76): four tools of the "Tools" bar
(`GraphTool::Rectangle`, `Ellipse`, `Line`, `Arrow`); a click inserts a
shape of default size (#153): square rectangle and circular ellipse of
48 × 48 px, horizontal line or arrow of 80 px, in figure pixels from
the clicked point. A drag draws the object freely (preview in the view); the
click or release gives control back to the Pointer tool and selects the object.
With the Pointer: a click selects (outline and handles),
dragging the body moves it (Ctrl + drag or middle click + drag moves the view
instead of the object, #318), dragging a handle resizes, Del removes it.
Holding Shift during the resize keeps the on-screen proportions
at the start of the gesture: the opposite corner stays fixed, a side handle keeps
the opposite side and the center of the other dimension. A line or arrow keeps
its initial direction. Without Shift, the dimensions stay free; a shape
that is initially flat stays free because it has no defined width/height ratio.
These gestures are shared by floating and detached graphs. The
"Graph" menu and the right-click on the object: "Bring to Front", "Send to Back",
"Delete Object" (`graph_window::shape_order_buttons`). Each confirmed gesture is **one**
`Action::SetShape` / `RemoveShape` / `ReorderShape`, so one `SetPlotSpec` or
`EditPlotSpec`. The
"object format" part of the `Tools` bar (issue #64, merged by #93;
`object_format.rs`) sets the selected object: stroke color, fill color (disabled for
a line or an arrow; opacity is the alpha of the picker, `#RRGGBBAA`),
thickness. Each setting is a model field set by an `Action::SetShape`
(`StokesForgeApp::format_object`, `app/format.rs`). On the script side: `pyst.add_shape` / `pyst.remove_shape` (`docs/python-and-scripting.md`).

**Exporting the graph** (issue #34): "Graph" → "Export" → "Export
Graph…" menu, Ctrl+E (active graph only), or right-click on the figure (including
in a detached window). An `rfd` picker (SVG, PDF, PNG) proposes
the name of the graph (`state::export_file_name`); the format follows the extension,
`.svg` if it is missing. `app::graph_window::request_graph_export` reads the zoom of the view
(`GraphView::framed_spec`) and pushes `Action::ExportGraph { path, spec }`;
`AppState` writes the file (`stokesforge-render-svg`, Typst text included) and says so
in the status bar. No command: the model is not touched. The
PNG resolution (×1, ×2, ×4; ×4 by default, i.e. 1920 × 1440 px) is set in
"Graph" → "Export" → "PNG Resolution" or in Preferences › Export
(`Action::SetPngExportScale`, kept from one launch to the next:
`preferences.json`, `png_export_scale`, read leniently, a value outside the
list gives ×4). The text of exported SVGs (#162) is set in
"Graph" → "Export" → "SVG Text": **Text** (default; selectable
`<text>`, font named but not embedded) or **Outlines**; the choice is
kept from one launch to the next (`preferences.json`, `Action::SetSvgTextMode`,
no command, no history, no `.stf`) and applies to SVG only; Preferences ›
Export sets the same two preferences.

**Copying the graph to another application** (issue #149): "Graph"
→ "Export" → "Copy Graph" menu, or **Ctrl+C** when a graph window is active in the
foreground (floating or detached), **no object** is selected in it
(series, free text, shape, title or legend: `selected_manager_target`, the
selection of the Object Manager is that of the view) and no input field
has focus (`keyboard_focus_outside_grid`). The clipboard receives the
**same figure in three representations**, offered together: SVG, PNG, PDF,
in this order of preference; the receiving application chooses, and
many prefer PNG. The framing is that of the displayed view (zoom and
pan, `GraphView::framed_spec`); the size, the fonts and the background are those of
the export to a file, and the PNG follows "PNG Resolution". Ctrl+C in a
cell, a sheet or the Project Manager is unchanged: the Project Manager
removes its own `Event::Copy` when it has focus, and `figure_copy` runs
before the panels and the contents, so that a grid visible behind the
graph window does not receive the same event.

Chain: `app::figure_copy::request_figure_copy` (the root, from `ui()`,
or the viewport of a detached graph) pushes `Action::CopyGraphFigure { spec }`;
`AppState::copy_graph_figure` composes the figure **once**
(`stokesforge_render_svg::export_bytes_many`) and prepares the representations
(`take_figure_clipboard`); the root window then offers them
(`StokesForgeApp::offer_figure`, which alone knows its display through
`eframe::Frame`) through `stokesforge-clipboard`. A format that fails alone is omitted
and named in the message; if none is ready, or if the system refuses, the status
bar gives the cause. No command: neither the model nor the history
changes.

Why a separate crate: neither egui (`Context::copy_text`) nor `arboard` can
offer `image/svg+xml` or `application/pdf`, nor several types at once.
`stokesforge-clipboard` carries one backend per display: **Wayland** through
`Clipboard::store_formats` of the `vendor/smithay-clipboard` patch (the worker already
shared with egui-winit, see `vendor/winit/STOKESFORGE.md`), **X11** through an
owner of the `CLIPBOARD` selection (`TARGETS`, one type per
representation, no `INCR`: a representation larger than an X request
is not offered), **Windows** (one registered format per representation:
`PNG`, `image/svg+xml`, `application/pdf`) and **macOS** (`NSPasteboard`, UTI
`public.svg-image`, `public.png`, `com.adobe.pdf`). Under Wayland the
compositor accepts the selection only from the focused window that has just
received an input: the offer therefore starts from the gesture, and otherwise it is lost without
an error, like the text of egui-winit.

Verified: the composition and order of the three formats, the gesture (selection,
focus, active tab, event consumed) and the menu entry by tests;
X11 against a real server (`cargo test -p stokesforge-clipboard --test x11 --
--ignored`, XWayland is enough). **Not run by the CI or here**: real Wayland,
Windows and macOS (the last two only compile,
`cargo check -p stokesforge-clipboard --target x86_64-pc-windows-gnu` and
`aarch64-apple-darwin`). Manual Wayland check: launch the application,
select a graph in the foreground with nothing selected in it, Ctrl+C, then
`wl-paste -l` must list `image/svg+xml`, `image/png` and `application/pdf`
(`wl-paste -t image/png > figure.png` to read a representation).

**Tabs** (`state::PlotSettingsTab`, `plot-tab-*` keys, in English):
"General" (title and its color, background), "Plot Area" (layer title,
grid lines), "Axes" (one side of the cell at a time, chosen in the vertical
strip at the left of the page: axis title (quantity and units), scale, display,
axis color; see "Axes tab: four sides" below),
"Legend" and "Theme". The series no longer have a tab (#179): they have their
own window, **Series Properties**, described below. The texts of the whole window are `plot-*` keys
of the `en` catalog (tooltip in a `.hint` attribute), following the glossary
`docs/ui-texts.md`; the names that are typed in (titles, series names)
are data, never translated. The theme names and the texts the figure draws by itself
(broken-reference placeholder, « … and N more » line, row series name) are
catalog keys (`plot-theme-*`, `graph-series-*`, `graph-legend-more`, `graph-row-series`).

**Graph texts**: title, panel title, axis label and unit, series name
are composed by Typst and read as **markup** (`docs/rendering.md`
§ Typst (R8)). Each of these fields (`formula_field`) carries on its right a small
greyed help icon (#197, `app/hover_help.rs`) whose bubble, on hover, recalls
the syntax (`formula_help`, issue #33): `$lambda$`, `$x_2$`, `$"cm"^(-1)$`,
`*bold*`, `\$` for a dollar, and names Typst with the address of its reference.
It replaces the collapsible "Math syntax" memo: the link with the field is
visible and the help no longer takes up height. Any future field that accepts
Typst text (interface side: "Math", #322) goes through `formula_field`; any other hover help, through `hover_help`.
The test `each_reminder_example_compiles` checks that each example compiles. A
series name left empty takes that of the column, displayed as is.

**"Theme" tab**: two predefined themes (`stokesforge_plot::THEMES`, identified by `Theme::id`
`light` and `dark`; named `plot-theme-light`/`plot-theme-dark`) and three global colors: background, text, axes. A global
color is **written into all the local fields** (graph title,
title and scale of each axis of each panel;
`PlotSpec::set_text_color`/`set_axes_color`/`apply_theme`): the last
setting overwrites what had been set one by one, and nothing more is remembered
(the `.stf` format does not change). Everything stays in the draft of the
window: a single history step on confirmation. The themes leave the
text and the axes on automatic, so they follow the background.

**Color picker** (`crate::color_picker`): every color of the
window is chosen with a swatch button (`color_field`/`color_button`)
that opens a pop-up window, **collapsed**: palette, "Auto"
when the color can be automatic, saved colors and hexadecimal input
(`#RRGGBB`, or `#RRGGBBAA` with the opacity). The **"Custom color"** box
(a checkbox since S-C5, #227) expands the rest: Save Color and its destination, swatch (saturation ×
lightness square with a hue bar, or **color wheel** with a
value bar; the "Square | Wheel" choice is a set of underlined tabs, `tab_strip`
(#170, point 7) and is kept in egui's memory, `stokesforge_picker_shape`), **opacity**, RGB and HSL input. The picker always reopens
collapsed (`color_picker::custom_id`). This window is an `egui::Area` managed
by `color_button` (and not an `egui::Popup`, of which egui allows only one
open instance: the list of a dropdown menu would close it); it
closes on Esc or on a click outside it and its lists. A transparent color shows on a
checkerboard (in the swatch of the picker). **One palette per use** (`stokesforge_plot::ColorUsage`):

- **curves**: a tool of its own, with a choice of named palettes
  (`stokesforge_plot::CURVE_PALETTES`: StokesForge, Tableau 10, Okabe-Ito readable by
  the colorblind, ColorBrewer Set1/Dark2/Paired, gradients Viridis, Plasma,
  grays), each with its dark and light shades. The palettes live
  in `stokesforge-plot`, so that a future change can color several curves
  at once (palette or gradient);
- **text**: readable inks;
- **backgrounds**: first **"No color"** (`#ffffff00`,
  `stokesforge_plot::TRANSPARENT`), which is also the **default background** of a new
  graph (`PlotSpec::default`): the exported figure has no background,
  to blend into another presentation; on screen, it shows on
  a **white** background painted by the application under the figure
  (`app::graph_window::display_backdrop`), never exported. Then light backgrounds, and dark
  backgrounds (the automatic text turns white, see `docs/rendering.md`). An
  empty background (files written before this default) stays white.

**Saved colors**: "Save Color" keeps the current color
**in the project** (by default: undoable command `SetSavedColors`,
saved in the `.stf`) or **for the user** (file
`couleurs.json` in the StokesForge configuration folder (`STOKESFORGE_CONFIG_DIR`
or the system folder, `docs/application/theme-i18n-system.md`), next to the
import recipes, outside history: `stokesforge_app_state::user_colors`), as chosen
(`state::SaveTarget`). A saved color reappears only in the
palette of its use; right-click on it to remove it
(`Action::SaveColor`/`RemoveSavedColor`). The picker touches neither the
model nor the disk: it leaves requests in a `PickerEnv`, which
`plot_settings_contents` translates into actions.

The color is confirmed at the end of the gesture (click on a swatch, end of
drag, field confirmed, closing), never pixel by pixel. Every new
color setting goes through this picker, with the use that matches it.

**Three ways to open it**:

- **right-click on a component of the graph** (`app::graph_window::graph_context_menu`):
  title, plot area, axis, axis title, curve, legend. The menu is that
  of the targeted object (`object_node_menu`), whose "Properties…" opens the page that
  sets this component (`state::PlotTarget::settings_page`); on the background and the
  plot area: Add Text, Insert Object, Export, Properties…. The component
  under the pointer is found by `stokesforge_render::hit::component_at`, on the
  geometry actually allocated by Plotters (plot area, tick
  areas, title band), not on a reconstruction. The axis line (a 4-point strip
  inside the edge of the plot area, #385) picks the axis like its tick
  band, before a curve. The right-click
  **without** a drag opens the menu; the secondary drag stays the rubber-band
  zoom. Same path in floating and detached windows
  (`graph_view_contents`);
- the **"Graph" menu** of the menu bar, present only when
  the active tab is a graph (`state::active_graph`, tested without a window):
  a "Properties" submenu with one entry per tab, and "Reset Zoom". So no setting
  is reachable **only** by right-click;
- reopening the window on another graph first confirms the draft of the
  previous one.

Closing the window confirms whatever still differs from the model
(`AppState::close_plot_settings`), for example a text field that has not
yet lost focus.

**It edits the `PlotSpec` through a draft, never directly.**
`state::PlotEdit { graph, spec, selected_series }` is a whole `PlotSpec`
held outside history until confirmation. It is the same principle as
`NoteEdit`/`HeaderEdit` (deferred input, local buffer, command sent
only on confirmation), generalized to a whole object rather than a single
field: the panel mixes a dozen small widgets (title, axes,
style of each series, legend) that must all see and modify the **same**
draft. `AppState::begin_plot_edit(graph)`, called every frame by
`app::plot_settings::plot_settings_window` before painting anything, overwrites the
draft only if there is none yet or it was about another graph.
Repainting the same frame on the same graph must never erase input
in progress, and changing graph must never let the draft of the
previous one leak. A draft **with no input in progress** (equal to
`PlotEdit::base`, the model spec it started from) does, however, follow the
model: after Undo, Redo, a script or a column drag and drop,
the window shows the up-to-date spec instead of stale values
that a later confirmation would write back over the undo. `AppState::commit_plot_edit` sends
`Action::SetPlotSpec { graph, spec }` → `Command::SetPlotSpec`, but
only if the draft differs from the current spec of the model (`PartialEq`
comparison on `PlotSpec`, which derives cleanly down to the leaves):
without this guard, releasing a slider that has not moved or losing the focus
of an unmodified text field would stack a history entry identical to the
previous one.

**Anti-inflation strategy for history**, three families of widgets each tied
to a different confirmation trigger, rather than a commit at every
keystroke/pixel:

- **text** (titles, axis labels, colors as `#RRGGBB`): `lost_focus()`.
  The field is modified live in the draft at each keystroke (immediate
  visual feedback), but `commit_plot_edit` is called only on loss of
  focus, exactly like `note_contents`/`cell_widget`;
- **numeric drag** (line thickness, marker size, manual axis
  limits, `egui::DragValue`): `drag_stopped() || lost_focus()`.
  The value changes at each pixel of the drag in the draft, but is
  sent only on mouse release or keyboard confirmation;
- **checkbox / dropdown menu** (scale, visibility, grid, plot
  type, legend position...): a single discrete event per click,
  confirmed at once. There is nothing to debounce, since by construction there is never
  a burst of `changed()` events for these widgets.

Each widget of `app::plot_settings::plot_settings_contents` (and of its helper functions
`axis_editor`/`series_editor`/`legend_editor`) accumulates into a simple
local boolean `commit`, set to `true` by any widget that has just
finished an interaction; `commit_plot_edit` is called only once, at the
very end of the function, never widget by widget.

**What the window edits**, touching only fields that really exist
in `stokesforge-plot` (read before writing, as requested):

- **Graph**: `PlotSpec::title`, `background`;
- **Plot area** (`PlotSpec::panels`[0] only, see below):
  `PanelSpec::title`, grid of the bottom and left axes;
- **Axes**: the four sides of the cell, Bottom, Left, Right, Top, on every
  layer (#396, "Axes tab: four sides" below): `AxisSpec::label`,
  `unit`, `scale` (`AxisScale::Linear`/`Log`), `visible`, `grid`, the
  "From" / "to" limits (#344: **neither an "Inverted axis" box nor an "Auto range" box**;
  "From" and "to" are always editable, show the computed limits as long as
  `auto_range` is true (in display order, the upper limit first
  on an inverted axis), an input freezes both limits (`auto_range` false,
  `inverted = De > À`), the "Auto" button makes `auto_range` true again keeping
  the direction), then the "Ticks" section (tick marks, direction, minor ticks)
  and its collapsible "Advanced" block (`tick_length`, `minor_tick_length`,
  `tick_width`, `tick_mark_color`, closed by default, `Form::collapsible`);
  `app/plot_settings.rs`, `axis_editor` and `mirror_axis_editor` (same
  sections, each for itself);
- **Series** (#179: separate window, `app/curve_settings.rs`; `PlotSettingsTab::Series`
  is no longer in `PlotSettingsTab::ALL` but selects this page in
  `plot_settings_window`: same draft, another title `curve-window-title`, another
  egui identity and another size; opened by the double-click on a curve of the
  figure or on its row of the Object Manager, `Properties…` and Graph › Properties ›
  Series…). A list on the left arranged like the Object Manager (#197, `PanelSpec::slots`):
  a single series is a `list_row` row, a group is a **collapsible node**
  (collapsing hides no curve) whose members are in a `tree_lines` branch;
  Move Up / Move Down change the rank in the group (or jump over a whole
  group). Settings on the right: the series' **group**
  (dropdown list None / group name / New group, and "Leave Group" which
  detaches it; group created on simultaneous add, #177; entering a group places
  the series after its members), then the group gestures (`group_editor`):
  its **name** (field confirmed on loss of focus, empty = "Group n"), Move Group
  Up / Down, **Ungroup** (the series stay, as single series, with their style) and
  **Delete Group** (the group and its series); and, for a grouped
  series, the **scope** (`EditScope`) in plain words: "This series only" or
  "Whole group (n series)" (default), with a sentence that says what
  applies. Group scope: line style and thickness, symbol shape and size
  are copied into each member (`propagate_style`); the settings
  specific to one series (name, visibility, legend) are **disabled**
  with the tooltip `curve-scope-locked`, its **colors are hidden** (the
  gradient gives them, #213) and the **plot type** stays settable, for the
  chosen series only; the sentence that says what the scope applies is under a
  "?" next to the choice, no longer in a note; the **gradient** (#178) is active.
  Series scope: only the series moves (this is the exception), its colors
  come back, the gradient is disabled (tooltip). Series without a group: everything is
  active, its colors are there and the **gradient is not shown**. Gradients:
  provided (`GRADIENT_PRESETS`), saved (project or user, suffix
  "(Project)" / "(User)", Delete button disabled outside a saved gradient) or
  custom with N colors (name + target + Save). Each entry of the list
  shows its colors (`gradient_swatch`) before its name, and the dot can be clicked like the name; the band below
  shows that of the group. It is applied **as soon as it is chosen** (or as soon as a
  stop changes), without a button: it spreads the colors in the order of the
  draft's list, which OK confirms and Cancel undoes. The **group remembers** its
  stops (`PanelSpec::group_gradients`, in the `.stf`): the list re-reads the
  choice from it (`identify_choice`), **empty** (and the band disabled) as long as none has
  been chosen; a series that joins the group (`set_series_group`) receives its
  color. In the Object Manager, the node under the layer is called "Series"; a
  group there is a **parent node like the others** (#197): collapse triangle
  (initial collapse = that of "Series", #139), mixed visibility box that
  controls only its members (`VisibilityTarget::SeriesGroup`), name editable in
  place (F2 or Rename, Enter confirms, Esc cancels), selection (the `GraphView`
  keeps `selected_group`, valid as long as the series that designates it is
  selected; the handles of all its curves are painted), double-click
  = series window opened on its first series, right-click
  (`object_node_menu`: Select Members, Rename, Move Up / Down, Ungroup, Hide,
  Delete, Properties…). In the series window, the group header follows the
  same rule (#208): highlighted when the group is being set (scope "whole
  group"), a click chooses it (first series, group scope). Del deletes the group **and its series** (the
  gesture distinct from dissolving it); Ctrl+↑/↓ moves it as a block. Actions:
  `RenameSeriesGroup`, `UngroupSeries`, `MoveSeriesGroup`, `MoveSeries`,
  `RemovePlotObject`.
  Former description of the list: (`egui::ScrollArea` bounded in height,
  `selectable_label` per series, label `state::series_list_label`: the name
  of the series if set, otherwise the long name of the Y column, otherwise
  its node name, otherwise the `graph-series-broken-reference` placeholder:
  `stokesforge_core::series_label` returns data (`SeriesLabel`: `Text`, `Row`,
  `Missing`) and `figure_texts::series_label_text` phrases it), then, for the chosen
  series: `SeriesSpec::name`,
  `plot_type` (`PlotType`, all variants except `Unknown`, see
  below), `visible`, `show_in_legend`, `line` (`LineSpec::color`, `style`,
  `width`) and `symbol` (`SymbolSpec::shape`, `size`, `color`); under the plot
  type, **Bins** (`histogram_bins`, whole number 1 to `MAX_HISTOGRAM_BINS`,
  written through `BinCount::new`) for a Histogram series only, and **Width
  (X units)** (`bar_width`) for a Bar series only (#359). Width follows S-C4:
  an emptied field or the word `auto` gives `None`, shown in gray as
  "Auto (<width>)" with the value of `stokesforge_core::resolve::bar_width`;
  a finite value above 0.000001 fixes it. In "whole group" scope both are
  copied to the members of the matching type (`propagate_style`); the plot
  type stays per series. Both go through the draft and `commit_plot_edit`
  (one history entry per validated entry); then **Y axis: Left / Right**
  (`SeriesSpec::axis`, #358), applied through `PanelSpec::set_series_axis`
  (Right makes the right axis visible, same rule as `pyst` `axis`; going back
  to Left hides nothing). In "whole group" scope the axis is copied to every
  member. Grayed out, with its reason, on an overlaid layer (the rendering
  ignores Right there, `pipeline.rs`). Known limit: the data cursor skips
  series on the right axis (`render/src/interact.rs`); then **Y error** and
  **X error** (#360, `SeriesSpec::y_error` / `x_error`): "None" first, then the
  other columns of the Y column's worksheet
  (`stokesforge_app_state::analysis::error_candidates`, plus the column
  already in use if it sits elsewhere). Choosing one, or None, goes through the
  draft like any field (one history entry). **Per series, never copied to the
  group**: an error column is data, not style, so "whole group" scope does not
  apply. Not shown for a "row" series (#166) nor for a Histogram. Deleting a
  column that serves as error breaks the whole series (`MissingRole::YError`),
  exactly like deleting its X column does (invariant 10: the series degrades,
  it does not crash);
- **Legend**: `LegendSpec::visible`, `position`.

**`PlotType::Unknown` never appears in the dropdown menu**, but a
series that carries it is still displayed: the plot type field becomes a
read-only text ("type unknown to this version, not editable"),
the rest (name, visibility, style...) stays editable as usual. This is
invariant 10 ("a data defect degrades, it does not crash") applied
to this window: offering `Unknown` in the list would suggest that one can go back to
it, whereas nothing can rebuild a variant of which this reader
knows only the name and raw bytes (C5, level 2,
`guide-de-correction-robustesse.md` §F3).

**What is not done**, explicitly out of scope for this work package:

- **no adding/removing of a series from the window**:
  that remains the role of the "Plot" submenu and of the drag and drop already
  described above (cells of the grid have their own gestures, below);
- **zoom/pan are not part of it, and must never be**:
  `stokesforge_render_egui::ViewState` lives on `GraphView`, outside the `PlotSpec`
  (`docs/invariants.md`, `.claude/rules.md`): this window has and will have
  no navigation field; confusing the two would break the rendering/model
  boundary that the whole rendering work package upholds.


## Rules shared with rendering

Two rules are **not** rewritten in the interface (issue #25):
the syntax of the `#RRGGBB[AA]` colors (`stokesforge_plot::parse_hex`, of which
`color_picker::parse_hex` is only an adapter) and the displayed name of a series
(`stokesforge_core::series_label`, called by the legend **and** by the list of
series of the graph properties through `series_list_label`). The RGB/HSL
transformations remain specific to the picker.

## Grid of cells (#357)

A graph is a grid of **cells** (`PanelSpec::row`/`column`); a cell holds an
ordinary layer and its overlay layers. Two gestures add a cell, one deletes it;
all three are a single `Command::EditPlotSpec` (one undo step), with the reasons
`PlotEditKind::AddCell` and `RemoveCell`.

- **Graph > New Layer Beside > Right / Below** (`Action::AddCell`, empty
  `columns`), on the **active layer**'s cell. Greyed out when the grid would pass
  `compose::MAX_GRID_CELLS` (256, the size the renderer lays out as declared).
- **Columns dropped on the right or bottom band** of the figure
  (`graph_window::edge_drop_side`, band `EDGE_BAND_WIDTH`, painted during the
  drag with `theme::drop_edge_band_*`): the new cell goes beside the cell
  **nearest the pointer** (`GraphView::nearest_cell`); bottom-right corner =
  Right. The series are planned like a drop on the graph
  (`AppState::add_columns_to_spec`, plot type of the reference layer); the Y2-Y4
  role layers go on the **new** cell. Without a Y column: `error-add-no-y`, no cell.
  Detached windows go through the same `graph_drop_target`.
- **Object Manager > layer > Delete** (and the Delete key after choosing the
  node) removes the cell and its overlay layers; on an overlay layer, that layer
  only. The last cell is refused (entry greyed, `error-last-layer`).

`compose::insert_cell` (#394): Right inserts a **whole grid column** after the
reference cell's (every cell at a greater column moves +1, whatever its row, with
its overlays; the other rows get a hole where the new cell is), Below a whole row;
the new column (row) takes the **frame** width (height) of the reference cell, so
the new plot area is exactly the reference's (4/3 stays 4/3) and the figure grows
by that frame plus the new cell's bands. The new layer is appended last, so
existing indices do not move. `compose::remove_cell` removes the column (row) a
deleted cell leaves empty, with its frame, so the figure shrinks by that column's
outer width; a column (row) that still holds a cell stays, with a hole. It resets
the graph `title_anchor` when layer 0 goes (it is in the frame of the first
layer).

### Graph size (#394)

A graph stores the **plot area** (the frame) of each grid column and row
(`PlotSpec::frame_widths` / `frame_heights`, points); the margins, the axis bands
and the title band are added around it. Adding an axis or a title grows the
image, never the frame; see `docs/rendering.md`, "Figure size, frames and bands".

- **Graph Properties > General > Size**: one field per column ("Plot area width
  (cm)", "Plot area width, column 2 (cm)"…) and per row ("Plot area height…"),
  in the unit of Preferences > General > Length unit (cm by default, mm, pt, in;
  `length_unit::LengthUnit`, the file keeps points), and the read-only "Graph
  size" (the frames plus the bands the figure has now,
  `FigureLayout::size_with_frames`). The fields edit the draft of the window like
  the others (one undo step for the window); a graph from an old file is first
  given the frames it is drawn with (`frame_sizes::set_size_in_draft`, from the
  published `FigureLayout::frame_widths`), so the other plot areas keep their
  size. The fields are `quantities::frame_size` (36 to 2000 pt, wide enough for
  six digits).
- **`Action::SetFrameSize { graph, axis: FrameAxis, index, size }`**
  (`PlotEditKind::ResizeCell`, "Resize Plot Area", one `EditPlotSpec`, one undo
  step) is the path of the Size fields and (with `SetFrameSizes`) of the drag handles: it fixes a legacy graph,
  clamps the size to `[MIN_FRAME_SIZE, MAX_FRAME_SIZE]` and sends nothing if the
  size is unchanged or outside the grid. `Action::SetFrameSizes { graph, column,
  row }` commits a column and/or a row in one step (a corner drag); all or
  nothing.
- **Drag handles (#394).** The outer boundary of a cell (right and bottom ones;
  `hit::cell_edge_at`, `docs/rendering.md`) is pulled with the Pointer tool:
  live preview in `GraphView`, at release `GraphEvent::FrameResized` becomes
  `Action::SetFrameSizes` (`graph_window.rs`). Dragging by Δ changes the stored
  frame by Δ.
- **Advanced › Figure size, Left, Top (#405).** General has a closed **Advanced**
  block (S-L8, `Form::collapsible`) at the bottom with **Figure size**: Grows
  with the axes (default) / Fixed (`PlotSpec::fixed_figure`; reserve of one axis
  per side, see `docs/rendering.md`). **Size** has **Plot area, from the left
  edge** and **from the top edge** (`quantities::figure_offset`, Auto when
  empty, never below `FigureLayout::min_left` / `min_top`: a smaller value is
  raised and the field's bubble says why). While a Fixed figure is bigger than
  its reserve, a note under Size says so (`status-figure-grew`, also put in the
  status bar when the change is made). All edits go through the draft
  (`frame_sizes::set_fixed_in_draft`, `set_offset_in_draft`: a legacy draft is
  given its frames in the same edit). Dragging the **left or top edge of the
  figure** (Pointer tool) sets Left / Top: `GraphEvent::FramePlaced` becomes
  `Action::SetFigureOffsets` (`PlotEditKind::PlaceFigure`, one undo step) and,
  while the gesture lasts (and for the Undo / Redo of its step,
  `AppState::placement_step_revision`), `GraphView::placing_edges` makes the floating window
  grow to the left / upward (`FigureFollow::anchor_shift` moves its corner) so the
  plot area stays still on screen. The handles of the figure's four outer edges
  straddle the edge by `FIGURE_GRAB_MARGIN` and the figure's interaction area and
  clip rectangle reach into the window's frame margin (`graph_window.rs`), so they
  win over nothing: the window's resize zones lie outside its frame. The status
  warning of a Fixed figure that grew in the window is given again when the
  window closes (`figure_grew_in_window`), the macro's own notice would hide it.
- **The window follows the figure.** `FigureFollow` (`window_placement.rs`, shell
  only, invariant 2) records the figure size each graph window published
  (`published_figure_size`) and the display scale of its view. When the size
  changes (a cell added or removed, Undo and Redo, the Size fields, a handle drag
  in progress, an axis or a title that makes the image grow, `pyst`), a floating
  window grows or shrinks by the difference at the scale it had, top-left fixed,
  to the right and down, clamped to the **visible** part of the workspace (the scroll area's viewport, never the virtual canvas, so the title bar and buttons stay reachable; beyond it the window stops and the figure is contained, and the window follows the CONTAINED figure on the other side too (no blank band; `FigureFollow::resize`, with the scale from before the clamp kept so Undo restores it); `floating_room`, `viewport_room`); a
  detached OS window does the same through `ViewportCommand::InnerSize`, clamped
  to the monitor (`follow_figure_in_viewport`). A maximized window does not
  resize (the figure is contained; it keeps its record so that restoring resizes
  nothing). The first frame of a graph after opening or loading a project has
  no previous value: no resize (`figure_follow` is cleared with the windows).
- **Legacy graphs (G7).** Opening one changes nothing and writes nothing. Its
  first edit, whatever it is (a field, a title, an axis, a layer, a curve), fixes
  the frames it was drawn with in the very command of the edit
  (`AppState::with_fixed_frames`, `frame_sizes::fix_from`); where the cells of a
  column or row had different frames, the largest is kept. Add Layer and Delete
  fix them before they edit the grid.
- A new graph is created legacy and fixed at once (`frame_sizes::fix_new`), so
  its figure is exactly 480 × 360 pt, explicit; a title added later grows the
  figure by its band (`pyst.plot` included). Beside / Below layers have the frame
  of their reference layer. `pyst` has no size setter yet.

**Indices that follow a removal** (and its undo): `ViewState::remap_panels`
(framings), `GraphView::remap_layers` (active layer, selection, gestures in
progress), driven by `StokesForgeApp::follow_layer_changes` with the exact map of a
deletion (`compose::remap_after_removal`) or, for Undo/Redo, the content match
`compose::remap_between`. The new cell becomes the active layer.

### Free layer placement (#398)

**Graph Properties > General > Size > Layer placement** chooses **Regular grid**
(default, everything above) or **Free** (`PlotSpec::layer_placement`,
`frame_sizes::set_placement_in_draft`, in the draft like every field: one undo
step for the window). Grid to Free leaves every plot area where it is drawn and
the image as it is (`render_svg::free_frames_from_grid`, a legacy graph is given
its frames first). Free to grid is `compose::set_grid_placement`: layers whose
vertical extents overlap share a row, left to right inside a row, then each
column takes its widest frame and each row its tallest.

In Free, **Size** shows **Position, left**, **Position, top**, **Plot area
width** and **Plot area height** of the layer chosen in the layer list (which
stays live on this page, greyed out only on the grid), the read-only **Graph
size**, and a note. Width and Height are the plot area
(`quantities::frame_size`, 36 to 2000 pt). The two positions are **stable page
coordinates** (`frame_sizes::free_fields`): exactly the stored
`FreeFrame::left` / `top`, in the Preferences unit, so a value is read back as
typed, a layer that does not move keeps its numbers whatever the others do, and
they can be negative (`quantities::free_position`, the `±MAX_FIGURE_EXTENT / 2`
range of `FreeFrame::sanitized`). The origin is the top-left corner of the image
at the moment the graph went Free (`free_frames_from_grid`), so right after
switching the numbers are the former distances from the image edge. Top does not
include the graph title band, so showing or hiding the title changes neither the
stored nor the shown numbers (with a title, the first Top reads the band less
than the old distance). The image still surrounds the layers (it may grow on the
left or above). The figure Left / Top and the per-column and per-row fields are
hidden and stay stored for the way back. **Figure size = Fixed** applies per
layer. There is no handle on any edge in Free until the mouse gestures come
(`GraphView::layout_is_free`); a click still activates the layer drawn under the
cursor, found from the drawn plot areas, never from grid breakpoints.

**Add Layer Beside** in Free writes the frame of the new layer in the same
`EditPlotSpec(AddCell)` (`frame_sizes::place_beside`, `render::place_beside`):
the active layer's frame size, the same top (Right) or the same left (Below),
and never over another layer (further along if the place is taken). **Delete
Layer** moves no other layer; the image shrinks around what is left. Neither
`AppState` nor `with_fixed_frames` ever puts a Free spec back on the grid
(`frame_sizes::is_legacy` is false in Free). `pyst` refuses the size keys on a
Free graph (`docs/python-and-scripting.md`).

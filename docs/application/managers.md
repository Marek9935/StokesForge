# Application: Project Manager and Object Manager

Continuation of the table of contents [`README.md`](README.md). Contents: Project Manager and windows by content, graph pasted between projects, Object Manager.

### Project Manager and windows by content

The context menu of windows (flat) and the *Edit* › Selection menu offer
Copy, Paste, Rename, Duplicate and Delete (#105); a folder offers
Add Folder, Paste, Rename (in place) and Delete (#157), and its Copy and
Duplicate are disabled (see below). A click selects and opens the object; Ctrl
toggles a target without opening it, Shift selects a range in tree order,
including the windows of collapsed folders (#327).

**Selectable folders (#170, point 4).** The Project Manager selection
is a set of `PmEntry` entries (`app/project_manager.rs`): a folder or
a window (its `Tab`). A folder is painted like a window
(`list_row`: pale background, accent rule, hover). A click **selects** it and
expands it if it is closed, without opening a window (`open_requests` stays reserved
for windows); a click on an open folder selects it **without
collapsing it**; a double-click collapses it, and so does the triangle. Delete and F2 act
on it (one history entry, `DeleteObjects` / `RenameObjects`). Paste
with only this folder selected pastes **into** this folder (`paste_parent`),
otherwise at the root. A deleted folder leaves the selection. **Copy and
Duplicate of a folder are refused**: a local copy would keep its references
to the original workbooks, which are outside the folder. The menu disables both
entries (`disabled-copy-folder`), and `AppState::refuse_folder_copy` refuses the
keyboard gesture with the same sentence in the status bar. What remains: a separate
issue to copy a folder with its dependencies.
**Keyboard (#170, point 6, S-K9).** With the panel focused, Up and Down move the
selection from one visible row to the next (`PmMemory::visible`, collected during
painting; `order`, which keeps collapsed folders, is used only for Shift ranges),
Enter opens the selected windows and toggles the folders. The cursor row (the
anchor of the last gesture) carries the focus outline
(`list_row::paint_focus_outline`); the area scrolls to it. The keys
are read and consumed by `list_row::read_navigation` (Shift+arrow and Alt+Down
first, without effect) before a workbook grid; the detached window has its
own memory per viewport. The Object Manager follows the same contract
(`object_manager_contents`): Up and Down select the neighbouring visible series,
group or object, Enter asks for its Properties; Ctrl+Up/Down stays "move the
series".

F2, Ctrl+D, Ctrl+C, Ctrl+V and Delete apply to this selection when the
Project Manager has focus, also in a detached window. Multiple renames
input the names one after another, then commit a single undoable entry;
Esc abandons the whole set. The field is committed by Enter or by losing focus.
Open and Close are in the context menu of an entry and act on
the whole selection: Open opens or activates (like a click), Close closes by the
same path as the cross (floating and detached windows). A closed window
is dimmed in the tree; a click on its row opens it. The tree starts directly under the panel header,
with no counter banner or intermediate buttons.

The selection uses the rounded shape of the buttons, with no rectangular background
added around the entries (#144), also in the detached Project Manager.

The single selection follows changes of the active content window (#146),
whether it is floating, maximized or detached. A multiple selection is
kept during passive changes and remains the target of the panel actions.
A single click on an entry, or a click in a workspace window
(floating or maximized), replaces the multiple selection with the targeted window
and re-enables following (#154), even if that window is already active. Hovering,
a click on empty space and panels covering the content do not replace it.
When the active window takes over the selection, it also becomes the anchor of
Shift-click in the fixed and detached panel: the next range therefore starts from
this window, and not from the last entry clicked in the Project Manager.
`follow_project_manager_active_tab` ensures passive following before the menus;
`window_stack` handles the explicit click and asks the detached Project
Manager to republish.

Names are unique in the parent folder. Copies get the first free
suffix "copy N" / "copie N" and new identifiers for their whole
descendants. A graph copied within its project keeps its references to the
original columns. A duplicated workbook keeps its open or closed state;
the graphs that use it are not duplicated. Delete closes the floating
and detached windows; undo restores the objects without reopening the windows.
Nothing asks for confirmation (a single Ctrl+Z gives everything back, #229); the message
names the remaining graphs that used the data.

Copy captures the content at the time of the gesture, then writes it to the system
clipboard; paste reads that, without rereading the source objects. Workbooks,
notes and matrices transfer between projects and between processes; graphs
between projects carry their columns in a hidden workbook (#107).
Paste in a folder's menu places the copies there. The grid refuses this object transport instead of
turning it into cells. `Action::MoveObjects` prepares the move between
future folders in a single undoable group; the Cut gesture remains to be wired when
they are introduced.



The Project Manager shows the project tree **stopped** at `Workbook`,
`Graph`, `Matrix`, `Note`: these four kinds are the clickable leaves of
this panel, never expanded further (no descent into the sheets
of a workbook, no descent into the content of a graph); folders
are expandable nodes, following the real hierarchy of the tree.
`AppState::project_tree` builds this view by generalizing the principle already
set by `AppState::graphs`/`AppState::sheets` (query the snapshot, never
duplicate access to the tree) rather than reinventing it: it returns
a light structure (`ProjectTreeNode`, folders and typed windows with their
`NodeId` and name), with no dependency on egui.

Clicking a window opens or focuses it, for the four kinds:

- **`Tab::Book`** replaces the old single global `Tab::Grid`: one window
  per open workbook, with an internal sub-navigation between its sheets
  (`AppState::worksheets_of`, `AppState::selected_sheet_in` — a
  `Workbook -> Worksheet` map rather than a single global selection, so that
  two workbooks open at the same time never interfere with each
  other). The grid itself (virtualization, a single editable cell at a
  time, `MISSING_TEXT_DISPLAY`…) is unchanged in principle; it is
  only scoped to "the sheet currently chosen in this workbook"
  rather than to a global selection — `AppState::visible_columns` and
  `AppState::row_count` now take an explicit sheet `NodeId`.
- **`Tab::Note`** is truly editable: `stokesforge-core` already exposes the text
  of a note (`NodeKind::Note::text`) and an undoable command
  (`Command::SetNoteText`). The editor follows the same deferred input
  principle as a grid cell (`AppState::NoteEdit`, symmetric to
  `CellEdit`: the buffer reaches the model only when focus is lost).
- **`Tab::Matrix`** is a **documented placeholder**: the window opens and
  is focused like the other three, but shows only an explicit message
  — `stokesforge-core::NodeKind::Matrix` has no content to edit today, and
  the full matrix editor is explicitly out of scope for this
  task.

### Data of a graph pasted between projects (#107)

The Copy gesture also freezes the X, Y, X error and Y error columns referenced
by all series and all panels. A paste into another project
creates a separate hidden workbook per graph and per paste, without merging;
the source sheets are kept with distinct names if necessary.
Unused columns are not carried over. Computed columns
carry their frozen results, without their formula: the new layout
of the columns and the omitted dependencies must not recalculate these values. A paste into the same
project keeps its original references and creates no workbook.

Graph › Show Data, also in the right-click menu of the
graph, reveals and opens the referenced workbooks. Hiding the embedded
data is persisted workbook metadata, distinct from the hidden
windows of the interface; the reveal is undoable. A whole paste
is undone in one gesture. A reference that is already broken stays broken, even if the
destination has a column with the same identifier.

The clipboard text is limited to 64 MiB (hexadecimal included); the
serializer stops before the overflow and the error is shown without
replacing the clipboard. Pasting a transport that is too large is refused.


## Object Manager (#81–85)

View › Object Manager › Show opens the tree of the
active graph: graph › layers (`PlotSpec::panels`) › plot group ›
series, as well as the rare objects, free texts and graphic objects
(rectangle, ellipse, line, arrow), when there are any. The **linked
texts** (title of the graph and of the layers, axis titles, legend) are
never listed there (#214): they are edited by double-click on the figure or in the
properties window, and do not count in the parent checkboxes. The group
"Objects" lists only free objects, is collapsed by default and disappears
when it is empty. "Objects" has its toggle "Show Objects" (`show_objects`, listed by default); when unchecked, its objects do not count in the parent checkboxes. The **axes** (#111, #147) follow the same
pattern with their own toggle, "Show Axes"
(`Action::SetObjectManagerShowScales`, preference `object_manager_show_scales`,
same View submenu and in the background menu): not listed by default, they
add under each layer, after the plot group, an "Axes" group
(`VisibilityTarget::Axes`, `LayerRow::axes`) that gathers one row per axis — the
axes the figure draws: bottom, left, right and top, the last two
being, on an ordinary layer, **mirror axes** (#176) — with
a single checkbox that controls `AxisSpec::visible` (axis line, ticks and
labels at once; the title keeps its `title_visible`); for the top, and the right
when no series is attached to it, it controls `AxisSpec::mirror`
(`AxisRef::is_mirror`), and is disabled when an overlaid layer of the checkbox
occupies this side (`AxisRef::blocked`). The checkbox of the
"Axes" group controls its axes and stays mixed as long as a mirror is unchecked; its
Properties open the Axes tab (Top and Right pages: `mirror_axis_editor`,
"Show mirror axis", "Show tick labels", "Show
ticks", their direction, the subticks and the "Advanced" block of the
tick marks, no title or axis color). **Closed frame** (#344):
`PanelSpec::frame_closed` / `set_frame_closed`, `Action::SetClosedFrame`, history
pattern `PlotEditKind::ClosedFrame` (**one** undoable entry) still
exist, but **no interface command** triggers them any more (neither the
layer menu, nor the Axes page: an axis is set by itself, mirrors
included). State update only: removing the action and its history
pattern is deferred (É5, `state.rs`). `set_frame_closed` sets the top, and the
right as long as no series is attached to it, as a mirror without numbers and copies
**once** the presence, direction and subticks of the bottom and left ticks; it has
no effect on an overlaid layer. An **overlaid layer** (#171)
lists only the axes it displays (`object_manager::axis_sides`): its X axis
named "X (top)" if `x_top`, its Y axis named "Y (right)" if `y_right`,
each controlling the `AxisSpec` `bottom` / `left` that carries it; its dead
`right` / `top` rows do not exist. The group is **collapsed by
default** (#138): a click on its arrow expands the axis rows, without touching
the model. The checkboxes of the graph and of the
layer count only the **listed children** (#173), gathered in `Shown`:
the axes without "Show Axes"; an unlisted child keeps its state. When axes are listed, they count
(layer and graph mixed if a mirror is unchecked); the checkbox of the plot
group has only its series (`VisibilityTarget::contents_shown`,
`visibility_shown`, `set_shown`). Activating a toggle recalculates the parents. `Action::SetPlotVisibility` reads the toggle from the state. The panel is resizable on the right;
its Detach button opens a system window, Reattach puts it back on the right. It is
visible by default; the chosen opening or closing is remembered in
`preferences.json`, outside the project and the history.

The view settings (#139) are grouped in **View › Object Manager**,
also available flat in the right-click menu on the background (#152), even with no
active graph. The two filters are at the first level; **Expand by
Default** sets graph, layers, plot groups, axes and objects. Objects
are grouped by graph or layer, without changing their visibility.
The menu is disabled when the panel is closed. **Reset** restores the hidden
filters, the collapsed axes and the other expanded groups; the collapses already
chosen stay free and are kept. The initial values apply
only to groups without egui state, never to a group already painted or manipulated.
`preferences::ObjectManagerSettings` is the single table of defaults; the
settings go to `preferences.json`, outside `.stf` and history. The old
fields of the two filters are still read and written for compatibility.
`object_manager_settings_menu` and `object_manager_contents` also serve the
detached viewport; a change of settings wakes it even if the
project and the selection have not changed.

Each series keeps its column-derived name and its legend sample
(line, color, thickness, marker or bar). The parent checkboxes change
their contents together; the group controls only the series. An indeterminate checkbox
signals partial visibility. Hiding keeps data,
style, text, position and rank; a hidden series leaves the plot, the legend
and the automatic bounds. The checkboxes of the graph and of the layer always control the full content,
hidden texts included. Titles have their own `title_visible`:
hiding an axis title leaves the axis in place. A toggle writes a single
`EditPlotSpec` command, with the history pattern `PlotEditKind::Visibility`.

Clicking a content item in the tree or on the plot synchronizes the selection, in
bold in the tree; the series is highlighted on the figure. A hidden item
stays selectable in the tree. The object format part of the `Tools` bar sets the line
of a series or of an object; the text part sets the selected text. The
panel and these bars also follow a detached graph window.

The right-click in the tree or on the figure and Graph › Selection share
the same menu (`object_node_menu`, #152): Show Data (curve),
Rename (F2), Move Up/Down (Ctrl+Up/Down), Show/Hide,
Delete (Delete) and Properties… last, depending on the target. A single click on a row gives
keyboard focus to the panel: F2, Ctrl+arrows and Delete act from this click,
without a prior drag (#126). The colored line and the name of the series offer
the same gestures (selection, right-click, double-click and drag, #151).
F2 on a group selected in the figure opens the editor of its name in the figure (on the legend), tree hidden or collapsed included (#206).
Deleting a series deletes no data column and none of the other
curves; Undo/Redo restores its place and its settings. A click elsewhere
gives this focus back.

Drag and drop of a series changes its rank in the group of its layer, with a before/after marker on the targeted
row; a drop on another layer is refused. The release sends a
single action. The selection follows the moved content, also after
Undo/Redo, and a deletion shifts the addresses of the selected neighbours.

The properties choose the exact series and layer, even hidden; the
layer selector is shared by the pages of the window; it is disabled, with
a tooltip, on "Graph" and "Theme", which apply to the whole graph (#180). A free text
reuses its in-place editor. A graphic object opens a draft of
properties (coordinates, line, fill): confirming sends an action,
Cancel changes nothing. Parents have their properties and their visibility;
deletion applies to the leaves, and renaming to the series.

`object_manager.rs` carries the projection, the visibility and the
reordering rules; `app/object_manager_ui.rs` (#121) paints the shared
docked/floating content (`object_manager_contents`, menus, checkboxes, drag, shortcuts)
and carries the selection glue to `GraphView` (`select_manager_target`,
`selected_manager_target`); what it asks of the view goes up as
`ObjectManagerRequest`, executed after painting by
`StokesForgeApp::run_object_manager_request` (same module, #122, which also carries
the `object_manager_panel` panel) — the "Graph › Selection" menu goes through the same path. Mutation decisions stay in
`AppState::dispatch`, with the session commands.

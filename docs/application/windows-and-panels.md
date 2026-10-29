# Application: windows and panels

Continuation of the contents page [`README.md`](README.md). Contents: confined floating windows, overflow, tabs/lists/selection, geometry, maximizing and detaching, tree connectors, collapsible and detached panels. The catalog of windows is in [`../windows.md`](../windows.md).

### Confined floating windows

The Project Manager, the Object Manager and the Python console are fixed
panels on the left, on the right and at the bottom (#100). No menu hides them (View holds only the Object Manager settings) and no header carries a Close button; they collapse or detach. The order of the
panels makes up the egui layout (the first one placed is the outermost);
it is decided in `StokesForgeApp::ui`, and nowhere else: menu bar
(`Panel::top`, `app/menus.rs`), toolbars (`toolbar.rs`), status bar
(`Panel::bottom`, `app/menus.rs`), script console (`Panel::bottom`,
`app/script_console.rs`, placed before the Project Manager so that it runs under it
over the full width), Project Manager (`Panel::left`,
`app/project_manager.rs`, unless detached), Object Manager (`Panel::right`,
`app/object_manager_ui.rs`, unless detached), then the `CentralPanel`. The test
`fixed_panels_are_placed_in_the_order_of_ui` (`app.rs`) holds this order
through the panel rectangles. The four other
kinds of content window — `Tab::Book` (workbook), `Tab::Graph`,
`Tab::Matrix`, `Tab::Note`, each carrying a `NodeId` — open by default
as floating windows inside the `CentralPanel`, one per
`StokesForgeApp::floating_tabs` (stack order: `app/window_stack.rs`). Each is a bare `egui::Area`
(`show_floating_window`), not an `egui::Window`: the application alone owns
its geometry, and the frame, the active outline, the clipping, the handles and the
persisted geometry all come from the same rectangle. The earlier `egui::Window`s
had all their functions cut off and then redone by hand, and each
frame had to reconcile the requested rectangle with the one egui re-adjusted —
hence the bugs of shifted outline and of overflow onto the panels
(feedback of 2026-09-24, points 1 and 2 — report deleted, in the git history).

The area that carries them has a **fixed** logical size (`WORKSPACE_SIZE`,
1920×1080), independent of the size of the main window: below that size,
it scrolls (`StokesForgeApp::workspace_scroll`) rather than crushing the windows
it contains against its edges. `window_positions`/`window_sizes` and the
geometry persisted in the project are expressed in these logical
coordinates, bounded to the workspace by `clamp_to_workspace` (the only
bound), then converted to screen coordinates when painting. The rendering is
clipped to the visible part of the workspace (outside the fixed panels and
the scroll bars), for drawing as for clicks: a window
straddling the edge of the view paints and captures nothing under the Project
Manager or the console. Entirely out of view, it is not painted at
all, and scrolling back makes it reappear at its unchanged logical position. Reopening from the Project Manager an entry that is already open
but scrolled out of view scrolls the workspace to it. A
newly opened window appears cascaded relative to the previous one
rather than stacked on top of it. The fixed size is for now a constant
of the code, not yet adjustable by the user (persistence, settings UI) —
to do.

One tab per **open** window, whatever its kind: `StokesForgeApp::
open_tab` is the only place that decides "open" vs "focus" (clicking
the same Project Manager entry a second time never duplicates its
tab, it is brought to the front). One `Tab::Graph` tab per open
graph keeps its own `stokesforge_render_egui::GraphView` (hence its own
shape cache — `StokesForgeApp::graph_views` is a table indexed by
`NodeId`). Its content reserves the height of the controls under the figure: the rendered
rectangle stays stable on hover, so the
cache is not rebuilt and the window does not drift in height. A
new graph window starts large enough to hold the reference figure
(480×360), these controls and its header; if the user
shrinks it, the figure keeps its size and its geometry. It stays centered,
and its parts outside the allocated area are truncated by egui's
clipping, for drawing as for interactions. It therefore covers
neither the header nor the controls placed under it. The cache counters and
the technical summary of the figure are no longer displayed.

The custom title bar centers the name of the content; it is clipped only
symmetrically, by the larger of the reserves actually occupied — collapse arrow on
the left, painted icons on the right (a dialog reserves only the cross,
#50). The one of a graph keeps on
the left the collapse arrow of an egui window: it hides or shows again only
the content of this graph, without changing the project, the saved geometry
or the other windows.

Each floating window can also be maximized (full frame of the
`CentralPanel`, `StokesForgeApp::maximized_tab`) or detached into a real
separate OS window (`StokesForgeApp::detach_tab`/`reattach_tab`, for multi-screen).

The File menu offers creating a new workbook (Add Workbook), a folder
at the root (Add Folder, #326) and importing files. The position and
size of the content windows are saved in the project (see
"Window geometry and state"). The matrix editor remains a placeholder.


### Overflow of content windows

All floating and detached content windows share a single
outer rectangle: their content is painted in a bounded subspace that cannot
enlarge the frame beyond the resize handles. The inner rule
depends on the kind of content. In a workbook, the grid and the
sheet tab strip each scroll horizontally, independently,
when they overflow. From top to bottom: grid, sheet tabs,
horizontal bar of the grid; the vertical bar occupies the right edge over
the full height. The tab strip is carved out of the bottom of the workbook
before the grid is built, so the grid can no longer push it out of the
frame (`book_contents`); this split does not use `egui::Panel`, which
replaces the clipping of its parent instead of combining it and drew the
tabs under the view of a cut window. The tabs
keep their natural size (`sheet_tabs`, wrapper of `tab_strip`, `crates/stokesforge-app/src/app/workbook.rs`) —
they do not shrink and do not fold into a "…" menu. The strip has
**no** scroll bar: when tabs are hidden, two buttons
"<" and ">" appear on its left and bring in the next hidden tab
on that side (each disabled if there is nothing to show on its side); they
disappear when everything fits. Changing sheet (by click, keyboard or
script) scrolls the strip **by the smallest offset** that shows
the selected tab and its two neighbors entirely — clicking the last
visible tab reveals only one more tab (`scroll_tab_strip_to_selected`,
issue #1). Look (#169): flat tabs, without a frame; the active one is written
in bold and in the accent and carries an accent line on the content side (`TabStripSide`:
under the label for the properties pages, above for the sheets),
a grey rule separates the strip from the content. The axes of the "Axes" page of the
properties are also a set of tabs (`tab_strip`, `Top`).

#### Tabs, lists and selection: four aspects, four roles (#169)

| Role | Building block | Look |
|---|---|---|
| set of tabs (sheets, pages, axes) | `tab_strip` / `tab_strip_with` + `TabStripSide` | flat, active in bold and accent, line on the content side |
| list or tree entry (Project Manager, Object Manager, series list of the properties) | `app/list_row.rs`: `list_row`, or `ListRowBackground` + `list_row_label` for a composed row | full-width row of `LIST_ROW_HEIGHT` (24 pt, the same for all, #170), pale accent background, accent rule on the left, bold |
| exclusive choice in a menu, a drop-down list or the color picker (its "Square | Wheel" views are `tab_strip` tabs, not buttons) | egui's `selectable_label` / `selectable_value` | selected-button background |
| toolbar tool toggle | `Button::selected` (`toolbar::tool_button`) | pressed button |

Guard `no_selectable_button_outside_a_menu` (`tests/guards.rs`): an
egui selectable button outside a menu, a `show_ui` or a
`*picker*`/`*popup*` function makes the tests fail.

Bold is **always** `theme::bold` (`wght` axis of Noto Sans Variable,
weight 600): egui's `RichText::strong` makes nothing bold, it is forbidden
(`no_egui_strong_outside_the_theme`). A form section title
(graph properties, import wizard) goes through `theme::section_title`
(6 pt space then bold) and alone separates its groups, without a `ui.separator()`
right before (`no_separator_before_a_section_title`); the current step
of the import wizard and the selected item of a list are also in
bold. A font without a `wght` axis (Fira or Libertinus pair, system font)
stays at normal weight, without error.

Three named text levels (#170, S-L6), in `theme.rs`: the section
title (`section_heading`, bold at normal size; `section_title` places it with
its space), the column header of a table (`table_header`, bold at small
size: import preview, results, formula memo) and the secondary text
(`secondary_text`, muted at small size). A bold at small size written
in place (`bold(…).small()`) is refused (`no_small_bold_outside_the_theme`). A
list row (`list_row`) and any composed row (checkbox, swatch,
label) have the same height, `list_row::LIST_ROW_HEIGHT` (S-L7): the
`ui.horizontal` row receives it through `with_list_row_height` (interaction
height raised for the duration of the row), so the checkbox there measures 24 pt on a side.
The separator of the step trail of the import wizard is the muted `CaretRight`
icon, without a tooltip (decorative, no gesture). Every state or chrome color
comes from `theme.rs` (`accent`, `tab_rule_color`, `tab_hover_mark_color`,
`list_row_selected_fill`, `list_row_hover_fill`, `drop_target_stroke`…)
or from the `Visuals` it sets (`error_fg_color`, `warn_fg_color`,
`selection`): no `Color32::from_…` and no named tint outside `theme.rs` and
`color_picker.rs` (`no_hardcoded_color_outside_the_theme`). Notes
and matrices stay bounded to the common rectangle. In a
graph (`graph_view_contents`,
`crates/stokesforge-app/src/app/graph_window.rs`), the figure keeps the ratio of
`DEFAULT_FIGURE_SIZE` (`stokesforge-render::pipeline`, 4:3) but is scaled
"contain" in the available space — maximized by taking the limiting
dimension, width or height, both when enlarging and when shrinking
(F11). No clipping or scrolling for the graph: the whole figure always stays
visible. It is a **zoom**: the layout (ticks, margins,
legend, text measurements) is always computed at the reference size
`DEFAULT_FIGURE_SIZE` (480×360), then the whole result, text and line
widths included, is enlarged or reduced uniformly (`GraphView::paint_scaled`,
`stokesforge-render-egui`). Resizing the window therefore lays out nothing again, and
does not rebuild the shape cache. The rule is identical in a floating
and in a detached window, which share the same function.

Gestures on a sheet (#106, removal of F2 per #130): double-click on the tab or the
Rename menu (in-place editing, `AppState::renaming_sheet`, confirmed by Enter or
by leaving the field, abandoned by Esc); Ctrl+D or the Duplicate menu ("… copy
N", first free number, the copy is displayed); Delete through the menu only,
without confirmation (undoable, #229; the message names the graphs that
plotted columns of it), disabled on the last sheet. Empty or already used name: old name kept and a message,
no history entry. Same gestures in the *Workbook* menu (mirror of the
right-click, `workbook_menu`); the strip offers them through `tab_strip_with`
(`TabOps`/`TabEvent`), which never touches the model.

For a new kind of window, keep this common bound, then
define with the user which areas scroll and which areas are
clipped before wiring up its content.

The `Frame` of these windows has no vertical margin
(`WINDOW_CONTENT_MARGIN`, `crates/stokesforge-app/src/app/window_chrome.rs`): the header band
and the separators of the content already provide their own spacing, and a
default margin only added a dead white band above and below.
The horizontal margin stays. All scroll bars follow the
standard of `theme::scroll_style` (see § Theme).

The horizontal bar of the grid is **moved under the sheet
tabs**, as in a spreadsheet: the scroll area of the grid
(`show_horizontally_scrollable_grid`) hides its own bar, and
`show_grid_horizontal_bar` places in the band reserved at the bottom of the window
an empty horizontal `ScrollArea` that mirrors it; a drag on this bar
is applied to the grid on the next frame.

The vertical bar of the grid is **moved**: the table scrolls itself
(which freezes its header) but its bar is hidden, because inside the
horizontal scroll area it would follow the width of the columns and leave
the view. `book_contents` reserves a band on the right edge of the window, over
its full height (header and sheet tabs included), and
`show_grid_vertical_bar` places in it an empty vertical `ScrollArea` that mirrors the
table, content and offset scaled to the bar
(`grid_vertical_bar_scale`); a drag on this bar is applied to the
table on the next frame (`grid_contents`, `pending_vertical_offset`).

### Window geometry and state

For each content window, the session remembers (and the `.stf`
saves) its **floating geometry** — position and size in the
workspace — and its **state**: open, maximized or closed, collapsed, stacking
rank (`WindowGeometry::view`, see `docs/persistence.md`). Both
are sent together at the end of each frame (`StokesForgeApp::sync_window_views`),
only when they change. The floating geometry never moves when the
window is closed, maximized or detached: `StokesForgeApp::floating_rects` keeps it
and takes precedence over the snapshot, which `AppState::set_window_geometry` does not
refresh.

**Detaching is not remembered**: a detached window is saved as
open and comes back floating on reload; the position and size of its OS window
are neither written nor read back. At startup, `StokesForgeApp::new`
restores the open windows (in stacking order), the maximized one and
the collapsed ones; with no saved state, it opens the first workbook and the
first graph.


### Maximizing and detaching a window

The content windows offer in their header icons to
maximize, detach and close, with tooltips. The workbook and the
graph use the same commands. Maximizing fills the
`CentralPanel`; the Project Manager and the console stay accessible.
Restoring gives the window back its floating layout. The Window menu
also allows maximizing or restoring the active window.

### L-shaped tree connectors (#112)

The trees of the Project Manager and the Object Manager no longer use egui's
indentation line (which went down to the bottom of a node's body). They
go through a single component, `tree_lines.rs`: `begin_tree` cuts
`indent_has_left_vline` at the root, each node body is written in
`tree_children`, and each direct child declares the center of its row
(`TreeBranch::mark`). The vertical line goes from the top of the body to the last direct
child (`└`), each child gets a horizontal line (`├`/`└`), and the line
continues through the subtree of a child as long as a sibling follows. The
lines are painted with the `Painter` (color `noninteractive.bg_stroke`, aligned
on the pixel). A node that contains a tree must return the center of its
row to its parent; limit: the vertical starts at the top of the body, not at the bottom
of the parent's arrow.

### Collapsible and detached panels (#100)

The three panels share `panel_header` (`app/window_chrome.rs`): compact
title, Detach, Collapse/Expand. The order of the buttons depends on the side:
Collapse/Expand toward the workspace, Detach toward the outer edge
for the managers. The console keeps Collapse/Expand then Detach
side by side on the left when it is expanded (#142). The three collapsed
panels show only Expand: Detach is hidden and its place is given back
to the title. The panel must be expanded to reach its Detach button.
The collapsed band of the console carries a top separation over the full width, of the same style as those
of the manager bands.
The collapsed vertical bands keep Expand at the top and the title below.
PM and OM collapse into a side band with a vertical title centered in
the space available under the button; the console collapses into a
horizontal bar. The three bands are 16 points thick, like their icons,
centered with no extra panel margin. The expanded panel stays resizable
(width of PM/OM, height of the console). The collapse bands have distinct
egui identifiers: their size does not overwrite the expanded dimension.
Collapsing leaves the panel visible; no menu hides it. The opening of the
OM is a preference; the collapse of the three panels is remembered too (see
below). The internal floating mode of the OM is removed.

**Remembered layout (#148).** The collapsed state of the three panels, whether
each is detached, and the size and position of its window outlive the session
(`preferences.json` › `panels`, never the `.stf`, nothing visible in
Preferences). `app/panel_memory.rs` does the shell's part, the pure rules are
`stokesforge_app_state::panel_layout` (`startup_panels`). At startup
(`StokesForgeApp::apply_startup_panels`, once, before the first frame) the collapsed
state is always restored; a detached panel is re-detached only if the machine has
the **same number of screens** as at the save, at the saved size and, where the
system can place a window (X11, Windows, macOS), at the saved position. Wayland
(winit) can neither read nor set a position: the key is then absent and the
window opens at its size, the compositor placing it. A different or unknown
number of screens, or an unusable size, re-attaches the panel (the collapsed state is
kept). While running, `sync_panel_layout` compares the collapsed and detached flags
after each frame and sends `Action::RememberPanelLayout` on any change, whatever
gesture caused it (header buttons, OS close, a script run expanding the console);
the detached windows report their live geometry (`LiveWindow`), flushed at exit
(`eframe::App::on_exit`). The builders of the viewports take the saved size and
position only when the window opens (constant while it stays detached: a value
renewed on each frame would fight the user's drag), and a manual Detach reuses
them. Not touched: the meaning of `object_manager_open`, the console expanding on
run, the viewport identifiers, the OS close re-attaching.

Detach opens a real system window without native decoration, with the
same custom frame and a single Reattach button. Its title allows dragging,
its edges resizing. A close request from the compositor
is canceled and then handled as a reattachment; it does not hide the
panel. The Window menu also keeps Detach / Reattach Project Manager.

The separate windows use `show_viewport_deferred`: their rendering is
independent of that of the main window. The callback exchanges a
read snapshot and requests with it, through two
`Arc<Mutex<_>>`, without moving `AppState` or creating a second
business mutation path. The requests explicitly wake the main
window so they are handled even when it is idle.
In the other direction, publishing a changed snapshot explicitly wakes the
reader viewport (#104): project epoch, model revision or revision
of the geometries. The Project Manager is also woken if its selection, its
hiding or the active tab changes. The OM receives the active graph, its selection
and the filters at each publication: it also follows a detached graph and
wakes without interaction in its own window. The gestures go back up
with the identifier of the graph that was painted. The console receives the input,
the output and the execution state (`ScriptConsoleModel`); new keystrokes
are drained before publication. The engine and the Python work stay in
the main `AppState`: detaching, reattaching, collapsing or hiding the console
does not interrupt them. The root polls the result while a script runs,
including when the console is hidden. A command applied at the end of a frame
schedules the root frame that will publish its result; no periodic wake-up
is added at rest. The windowless tests check the targeted request
and the return to rest; the cross-cutting tests of the three panels are in
`app/panel_tests.rs` (clicks, sizes, deferred callbacks, console under Python).
The real redrawing of the OS windows remains to be checked manually.

### Generic detaching of content tabs

`StokesForgeApp::detach_tab`/`reattach_tab`/`show_detached_tabs` generalize the
mechanism above to `Tab::Book`, `Tab::Graph`, `Tab::Matrix` and `Tab::Note`:
the detach icon opens an OS window; that window offers a
reattach icon. Closing a detached content window closes only its
view, without reattaching it or deleting the project data. It stays
reopenable from the Project Manager. Several contents can be
detached at the same time, each in its own `ViewportId`.

**Default sizes of the detached windows** (`show_detached_tabs`):
`Tab::Graph` opens at 560×540, which holds the reference figure, its
controls and its header. The rest (`Book`/`Matrix`/`Note`) share a
uniform size of 420×360. The workbook grid keeps the same widths and
headers in floating and detached windows; a narrow window
scrolls horizontally. The note and the matrix keep the same size for
consistency with the other contents.

**A single `ViewModel` structure, not one per kind of content.** The PM
has only one form of content (a read-only tree); the four kinds
detachable here are very different (virtualized editable grid,
interactive graph with shape cache, editable text, placeholder).
`DetachedTabViewModel` carries one field per kind (`view_state` for a
workbook or a note, `graph_view` for a graph) rather than four disjoint types to make coexist in the table of
detached tabs — the fields unused for the kind of `Tab` served
stay at their default value.

**Reading the model is factored out, transient editing is not.**
`tab_contents` (`app/contents.rs`, with `note_contents` and `matrix_contents`;
shared by the floating windows and the maximization area)
takes `&mut AppState`, which the `'static`, `Send + Sync` closure of
`show_viewport_deferred` cannot capture — the same problem already solved
for the PM (putting the whole `AppState` behind a lock would shift the
problem to solve, for one tab as for the other). The workaround:
`DetachedTabViewModel::snapshot`, a clone of `Snapshot` (cheap),
resynchronized from `self.state` at each frame before the call — same pattern
as `ProjectManagerViewModel::tree`. Every pure read goes through
free functions of `stokesforge_app_state::state`, factored out over a `&Tree` for
the occasion (`cell_text_in`, `visible_columns_in`, `row_count_in`,
`worksheets_of_in`, `note_text_in`, `node_title_in`): the exact body of the
corresponding `AppState` method, not a rewrite — the dock and a
detached window thus literally display the same rule.

The detached workbook has a local view `AppState`, synchronized with the snapshot
of the root. It calls the same `book_contents`/`grid_contents` as the
floating workbook; layout, header, selection, menus and grid shortcuts
thus share the same code. Its selection and its input stay local
to the window. Before handling its actions, the root takes over their grid
context, then applies the same undoable commands. The note follows the same
pattern: its detached window is a local `AppState` view (built by
`fork_for_detached_view`) and calls `note_contents`, like the floating
window; only the input buffer (`note_editing`) is local to the window,
and validation goes out as `Action::SetNoteText`, hence as an undoable command.
The matrix placeholder, `matrix_contents`, reads only the tree and serves
as is for both windows: no content function is specific to the
detached window any more.

`Tab::Graph` escapes this limit: `GraphView` has no global lock on
`AppState` to respect (its shape cache and its zoom/pan navigation are
already scoped per graph, in `StokesForgeApp::graph_views`) — `graph_view_contents`
is therefore **literally** the same function for a floating window and a
detached window; the view itself simply migrates out of `graph_views` for the time
of the detachment so it stays capturable by the closure without duplicating the
cache. `Tab::Matrix` has no state to carry (placeholder).

`Tab::Script` is not detachable: its input (`AppState::script_source`)
is a direct field of `AppState`, not a deferred buffer like
`CellEdit`/`NoteEdit`, and its life cycle (detached thread, `poll_script` to
call frame after frame) is much more coupled to `AppState` than the four
other kinds — generalizing it would require carrying the execution state of the script
too, out of scope for this task.


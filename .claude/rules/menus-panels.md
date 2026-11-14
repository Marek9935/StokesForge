---
paths:
  - "crates/stokesforge-app/src/**"
  - "crates/stokesforge-app-state/src/object_manager.rs"
  - "crates/stokesforge-app-state/src/panel_layout.rs"
  - "docs/menus.md"
---

## Object Manager, fixed panels, menus

Accessibility (#170): a hand-painted target publishes its label and its
state through `Response::widget_info`, like egui widgets. The layer
badges announce their number and their selection; the handles of floating
and detached windows announce their resize direction. The
tests read the AccessKit nodes actually published, as close to the surface as possible.

Object Manager (#81–85): `object_manager.rs` projects the graph,
`app/object_manager_ui.rs` draws the tree (#121), the selection
stays in `GraphView` (same view in a detached window). Linked texts
(titles, legend) are **never** listed there (#214): they are edited by
double-click on the figure or in the properties; "Objects" contains only
free objects (free text, shapes), collapsed by default, absent when empty.
Scales ("Axes" group under the layer, collapsed by default #138, one row per axis, check box = `AxisSpec::visible`, #111)
have their own twin toggle (`Action::SetObjectManagerShowScales`) and are
not driven by the graph/layer check boxes. The filters live in
`object_tree_with_selection`. The view settings (#139) go through the
common menu `object_manager_settings_menu` (display and background, disabled if the panel is
closed), the `Action`s and `preferences::ObjectManagerSettings`, the single table of
defaults. Objects have their collapsible section per graph/layer; the initial
collapse never replaces the current egui state, even after Reset.
The detached model carries all the settings and wakes its viewport if
only those change. The toggles reuse
`EditPlotSpec`, whose history reason stays a data item. The swatches
call `series_swatch`, shared with the legend.

Fixed panels (#100): PM on the left, OM on the right, console at the bottom; visibility
through View, collapse distinct from hiding, no internal floating. The
`panel_header` frame (`app/window_chrome.rs`) shares the icon cells of
content windows with a compact font. The horizontal order of the buttons
depends on the side: collapse toward the workspace, detach toward the outside.
The expanded console keeps Collapse then Detach side by side on the left (#142). The collapsed and detached state of the three panels and the size and position of their window are remembered between launches (#148: `app/panel_memory.rs`, pure rules in `stokesforge-app-state/src/panel_layout.rs`); a viewport builder takes the saved size or position only when the window opens, never a per-frame value.
Collapsed: thickness `ICON_SIZE` with no extra margin, only Expand is
visible, with no target or room reserved for Detach; title centered in the remaining
space (vertical for PM/OM). Attached and expanded: Detach and Collapse;
detached: Reattach only, with no system decoration. An OS close
reattaches. Reuse the contents of each surface and the reads/queries
of `window_detach.rs`; the Python engine stays in the main AppState.

- Project Manager (#170, point 4): its selection is a set of `PmEntry` (`Folder(NodeId)` or `Window(Tab)`), never `Tab`s alone; a row gesture goes through `select_gestures` (folder and window), a single click (window or folder) **only selects** (#423, S-K3/S-K4: the rule holds for every tree): opening is the double-click, Enter or the menu, folders toggle on the triangle, the double-click (one toggle), Enter and → / ←, no slow-click rename (F2 or the menu), `open_requests` stays reserved for windows, paste targets `paste_parent` (the single selected folder, otherwise the root; the target then expands). Cut (Ctrl+X, menu) + Paste moves through `Action::CutObjects` / `MoveObjects` (mark `AppState::cut_objects`, rows dimmed through `PmInteraction::cut`). A drag moves too (#425): payload and feedback in `app/project_dnd.rs`, decisions pure in `stokesforge-app-state/src/tree_drop.rs`, checks shared with Paste in `AppState::check_move_into` (never a second copy). Every folder is shown sorted until a drop between rows puts it in manual order (`Tree::displayed_children`, `Command::SetFolderOrder`); anything that lists the project tree for the user goes through `project_tree()` / `displayed_children`, anything that mutates uses the model order (`children`). A folder copies and duplicates like any node (#391).
- Keyboard navigation of lists (#170, point 6, S-K9): ↑ ↓ Enter of a manager go through `list_row::read_navigation` (registry `KEY_ARROW_UP`/`DOWN`/`ENTER`, Shift and Alt+↓ variants consumed first, with no effect) and `list_row::neighbour` over the **visible** rows recorded at paint time, never over an order that counts the rows of a collapsed folder; the focus outline of a row is `list_row::paint_focus_outline` (never a hand-written `rect_stroke`). A new list manager follows this contract, and a keyboard shortcut read outside the registry stays forbidden.
- Menus (#114, #152): the grammar of `docs/menus.md` is the single source
  — sections in canonical order (main, create, clipboard,
  organize, delete, exit, `Properties…` last), separators placed
  **only** by `MenuSections::section` (`app/menu_sections.rs`, guard
  `no_raw_separator_in_a_menu`, no exemption), submenu
  only for an exclusive choice or ≥ 3 entries (never for 1 entry; 2 entries stay flat),
  depth 2, entry *absent* (not applicable) or *disabled with `.disabled`*,
  bare verbs, disabled header reserved for the graph, shortcuts through `SHORTCUT_*`,
  no icon and no Undo/Redo on right-click. The shared painters (`object_menu`,
  `object_node_menu`, `object_manager_settings_contents`,
  `columns_menu_contents`/`column_type_role_menus`/`plot_menu`,
  `graph_properties_menu`) paint the **content**: the top menu
  wraps it in a submenu that names the target, the right-click shows it
  flat. Check boxes are limited to real display toggles and leave the
  menus open; an action or an exclusive choice closes the chain. Do not
  create a parallel mutation path.
- Menu bar (#343): switching on hover from one title to another is a single
  helper, `toggle_on_hover` (`app/menus.rs`), called after all the titles
  (each `*_menu` returns the `Response` of its title); egui does not do it for
  the titles of the bar. An open title is recognized by the identifier of its
  popup (`Popup::is_id_open(ctx, Popup::default_response_id(titre))`), never
  by `Popup::is_any_open` (an open context menu would make the
  bar switch); no state in `AppState`. Do not leave the diagonal tolerance
  to `Rect::intersects_ray` alone: it ignores the other axis when the
  direction is exactly horizontal or vertical (guard in the helper).

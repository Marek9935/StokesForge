# Menus and right-clicks: the grammar (#152, #155)

The **single** source of the rules that organize the menu-bar menus and the
right-clicks. A new entry goes into a section of this grammar; a menu that
violates it is reported against this page. The *how* (building block, shared
paints) is in `crates/stokesforge-app/src/app/menu_sections.rs`
and `docs/application/menus-et-barres.md` (§ Themed menus (#114)); the terms and capitalization
are in `docs/textes-interface.md`.

## The principle: content and envelope

A submenu that **bears the name of the target** (*Selection*, *Sheet*,
*Columns*, *Text*, *Objects*) is useful in the menu bar, which must say what it
acts on, and one level too many in the right-click, which has already pointed
at the target. The shared paints (`object_menu`, `object_node_menu`,
`object_manager_settings_contents`, `columns_menu_contents` /
`column_type_role_menus` / `plot_menu`, `graph_properties_menu`) therefore paint
**the content**; the top menu wraps it in its submenu, the right-click shows it
flat. There is no second path: only the packaging changes.
**An object has a single menu** (`object_node_menu`), whether it is pointed at
in the figure or in the Object Manager; Graph › Selection wraps it.

## Canonical order of sections

An empty section disappears with its separators.

1. **Main action** of the target: Show Data, Edit Text, Open / Close of a window…
2. **Create / insert**: Add Text, Insert Object ▸, Add Column, Add Worksheet, Plot ▸.
3. **Clipboard**, a single group: Cut, Copy and variants, Paste and variants.
4. **Organize**: Rename, Duplicate, Move Up / Down, Bring to Front…, Hide (graph objects), Move ▸ (columns), Type ▸, Role ▸.
5. **Delete**: Clear, Delete…
6. **Output**: Export ▸.
7. **Properties…**, always last, a direct action that opens the existing window
   on the target's page.

## Rules

- **Separators**: a thin line between two present sections, never at the start,
  at the end or doubled. It is placed **only** by `MenuSections::section(ui,
  présente, contenu)`; the caller says whether the section exists for the
  target, the line follows. Guard `aucun_separateur_brut_dans_un_menu` (`tests/gardes.rs`),
  with no exemption: no `ui.separator()` in a menu.
- **Submenu** only for an exclusive choice (Type, Role, plot type)
  or at least **3** secondary entries of the same family. Never 1 entry;
  2 entries go flat. Important or frequent actions are direct.
- **Exclusive choice** (S-C3, #227): a **lone check mark** in front of the
  current option, never a radio button nor the background of a selected button;
  it is painted by `menu_sections::menu_choice`. Check boxes stay for display
  toggles.
- **Depth 2**: the menu and one submenu. In the right-click this is strict;
  in the bar, the top menu counts as the menu. Accepted exceptions, all of them
  the envelope of a shared content that keeps its depth 2 in the right-click:
  View › Object Manager › Expand by Default, and Workbook › Columns › Move ▸ /
  Type ▸ / Role ▸ (three levels in the bar: Workbook, Columns, the
  submenu; #319 for Move ▸).
- **Absent or disabled**: not applicable to this target → **absent** (Show Data
  outside a curve); relevant but temporarily
  impossible → **disabled with a `.disabled` tooltip** that says why.
- **Target**: a right-click outside the selection replaces it with the element
  pointed at (a curve right-clicked is selected); inside it, it acts
  on the whole selection.
- **Labels**: bare verbs ("Copy", "Rename", "Delete"). The greyed header
  that names the target (`PlotTarget::label`) is **reserved to the graph**,
  where the target is not obvious.
- **Shortcut** displayed on the right as soon as one exists, always through
  `format_shortcut` of a `SHORTCUT_*` constant, never hard-coded.
- **No icon** in the menus.
- **Undo / Redo absent from the right-clicks**: Edit and the keyboard only.
- **Background of a panel**: its settings at the first level, without a
  "Settings ▸" envelope.
- **Nothing is reachable only by right-click**: every entry has its
  equivalent in the menu bar, a shortcut or a toolbar.
  **Accepted exception (#326)**: *Add Folder* in the right-click of a folder
  creates *in* that folder, a destination that File ▸ New ▸ Add Folder and the
  button of the File bar (which create at the root, like Add Workbook and Add
  Graph) cannot target. Creating in a subfolder goes only through the
  right-click.
- **Detached windows**: same paints, same targets; the actions
  go up to the root like the others.

## Tree by surface

### Graph (greyed header = target)

| Target | Menu |
|---|---|
| Background, plot area | Add Text · Insert Object ▸ (Rectangle, Ellipse, Line, Arrow, placed at the clicked point at the default size) ─ Export ▸ (Export Graph…, Copy Graph) ─ Properties… |
| Curve | Show Data (the workbook of **this** curve) ─ Rename · Move Up · Move Down · Hide ─ Delete ─ Properties… |
| Title, axis title | Edit Text · Reset Position ─ Hide ─ Properties… |
| Legend entry | Edit Text ─ Properties… |
| Legend | Reset Position ─ Hide ─ Properties… |
| Axis | Hide ─ Properties… |
| Free text | Edit Text ─ Delete ─ Properties… |
| Object | Bring to Front · Send to Back ─ Delete ─ Properties… |

Reset Zoom is not in the right-click: it stays in the toolbar, the Graph menu,
the button of the zoomed graph and the double-click. Export ▸ is only on the
background and the plot area, never on an object (consistent with Ctrl+C, which
copies the figure only with no selection, #149).

### Object Manager

| Target | Menu |
|---|---|
| Node | the menu of the figure's object (`object_node_menu`); layer and group "Axes": no more Closed Frame (#344: removed from the interface, `Action::SetClosedFrame` no longer has a menu entry) |
| Background | Show Axes ☐ · Show Objects ☑ · Expand by Default ▸ (5 check boxes) ─ Reset Settings |

View contains only "Object Manager ▸" (no more Panels nor Toolbars: panels and bars are not hidden through a menu; the settings of the Project Manager and of the console will be added there); it wraps the same content (`object_manager_settings_contents`).
The check boxes keep the menu open (#114).

### Project Manager

| Target | Menu |
|---|---|
| Window(s) | Open · Close ─ Copy · Paste ─ Rename · Duplicate ─ Delete |
| Folder | Add Folder (in this folder) ─ Copy (disabled) · Paste (in this folder) ─ Rename (in place) · Duplicate (disabled) ─ Delete |

Selection ▸ remains only in Edit.

### Grid

| Target | Menu |
|---|---|
| Cell, row, header field | Cut · Copy · Copy with Header · Paste · Paste in Place ─ Remove Formula, Keep Data (Formula field holding a formula, #236; also Workbook › Columns) · Clear · Delete and Shift Up |
| Column header | Plot ▸ · Add Column ─ clipboard ─ Move ▸ · Type ▸ · Role ▸ · Reset Width (default width of the selection, disabled with no stored width, #330) ─ Clear · Delete (removes the columns; a single Undo restores them); Move ▸ = Move to First · Move Left · Move Right · Move to Last, disabled with their reason when the gesture would change nothing (#319); same entries in Workbook › Columns |
| Empty grid, blank area | Add Column |
| Sheet tab, empty strip | Add Worksheet ─ Rename · Duplicate · Reset Column Widths (the whole sheet, disabled with no stored width, #330) ─ Delete |

### Columns and curves: Analysis (#181, #210)

The top menu (`analysis_menu_contents`) arranges the methods in
**categories** (#248), one window per category: Data Manipulation ▸
(Constant… · Linear… · Polynomial… · Anchor Points… · Reference Correction…,
flat) · Fitting ▸ (models, Custom Model…) · Peaks and Deconvolution ▸
(Decompose Peaks… · Reconvolution… · Direct Deconvolution…) · Integrate… ─
Recalculate Out of Date ─ the **last three analyses run** (#249),
numbered, under a rule. A click on a last analysis opens the window
pre-filled; Ctrl+click re-runs it on the current selection (the window
opens if a setting is missing). The right-click of a column header has an
`Analysis ▸` submenu with four entries, one per category (which reopens the
last method run in that category), in the "create" section (after `Plot ▸` and
Add Column) — one entry per method would have exceeded depth 2. On a column
that a calculation produces, a section of calculation gestures (Edit ·
Recalculate · Detach · Delete) follows Type/Role. The entries are **disabled**
(`.disabled` tooltip) as long as no Y column is targeted, never absent.

The right-click of a curve or of a group, in the graph as in
the Object Manager, offers the same `Analysis ▸` submenu, among its main actions.
The top Analysis menu takes the selected curve or group when a
graph is active. A group analyzes all its members, in plot order; the series built from a sheet row are disabled.
The X/Y are those of the plot, including in a hidden workbook: the pairing
of the sheet's columns does not come into play. The preview takes over the
framing and the styles of the source; Add to graph is checked and the results
join its layer and its group.

### Menu bar

The top menus arrange their sections in the same order; what they
wrap is the content of the right-clicks: Edit › Selection (menu of the
Project Manager's windows), Graph › Selection (menu of the object),
Workbook › Sheet (including Reset Column Widths) / Columns (including Reset Width) / Plot, View › Object Manager.

**Switch on hover (#343).** When a menu of the bar is already open, moving the
pointer over the title of another menu opens that one and closes the first,
without a click. With the bar at rest, hovering opens nothing. If the pointer
heads toward the open popup (a diagonal toward an entry that crosses the
neighboring title), no switch happens; the switch happens as soon as the
pointer no longer aims at the popup (`is_moving_towards_rect` tolerance, as for
egui's submenus). A single piece of code, `bascule_au_survol` (`app/menus.rs`),
shared by the active tab and the detached window. A click on the title that is
already open closes it.

## Adding an entry

1. Choose its section in the canonical order; if none fits, the
   grammar is to be discussed, not worked around.
2. Paint in the surface's **shared paint** (a single write
   for the bar and the right-click), behind `MenuSections::section`.
3. Decide *absent* or *disabled* (`.disabled` tooltip for a disabled entry).
4. Label: bare verb, key in the `en` **and** `fr` catalog; shortcut through
   `SHORTCUT_*`.
5. Test in the surface's module: the entry, its order, its absence
   on a target that does not want it.

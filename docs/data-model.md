# Data model

Three crates: `stokesforge-id` (identity), `stokesforge-plot` (the description of
a graph) and `stokesforge-core` (the tree, the columns, and the resolution that
brings the two together).

None of the three knows about a graphical interface, and none knows about a
renderer.

## The project tree

```text
Project
├── identifier allocator  (outside the tree — invariant 7)
└── Arc<Tree>                  (flat map NodeId → Arc<Node>, one root,
        │                          and the project's saved colors)
        └── Node { id, name, parent, kind }
              └── NodeKind
                  ├── Folder     { children }   folders and windows
                  ├── Workbook   { children }   sheets
                  ├── Worksheet  { children }   columns
                  ├── Column(Column)            the values
                  ├── Graph      { spec, … }    a PlotSpec
                  ├── Matrix     { … }
                  ├── Note       { … }
                  └── Unknown(RawVariant)       read from a newer file
```

The project's **saved colors** (`stokesforge_plot::SavedColors`, filed by use:
curves, text, backgrounds) are not a node but are carried by `Tree`
(`Tree::saved_colors`). They travel with the session snapshot, the rollback and
the `.stf` file without a separate path, and are changed by the `SetSavedColors`
command. The colors saved "for the user" are not in the model (see
`docs/application/graph.md`).

**x' row (#164).** `ColumnMeta::x_prime: Option<f64>` carries, per column, its
own abscissa (typically the time of a spectrum within a kinetic run). Always
finite; it is written like the other `ColumnMeta` fields, so through
`Command::SetColumnMeta` (no new command). `ColumnMeta` is no longer `Eq` (an
`f64`).

**"Row" series (#166).** `SeriesSpec::row = Some(r)`: the series is no longer a
column but row `r` of the sheet of `y` (which only serves as an anchor). Its
points are, for each column of the sheet that carries an x', the cell `r` of that
column at the abscissa x' (`NaN` if the column is shorter), in the order of the
sheet. `x` is ignored. It is resolved on the fly by `stokesforge_core::resolve`
(so it is live: editing the sheet or an x' updates the series); a sheet without
x' gives an empty series, and losing the anchor gives a broken reference. **The
row follows row insertions and deletions** of its sheet (#307): `InsertRows`
pushes `r >= at` to `r + count`, `DeleteRows` brings `r >= at + count` back to
`r - count`, in the same delta (field `plots`, so undoable as one block). A row
`r` inside the deleted range does not change: the series then plots the row that
took its place. **Deliberate choice (#307, G1, 2026-10-21)**: a row has no
identity other than its number, so there is neither deletion of the series nor a
"broken reference" state; a calculation already done keeps its old values until
Recalculate. A re-import that shortens a sheet falls under the same choice.
`DeleteCells` moves no sheet row and does not touch the series: deleting cells in
only some of the columns (G2) leaves a "row" series mixing cells from two
original rows, **left as is**, a rare case. Do not "fix" it without reopening
G1/G2. `NodeCopy::capture` embeds all the x' columns of the sheet when a copied
graph has such a series.

**Series group (#177).** `SeriesSpec::group: Option<u32>`: the series of a layer
that carry the same number form a group. It is born when two or more series are
**added at the same time** (`PanelSpec::push_series_batch`: Plot ▸ on several
columns, dropping columns or rows, via `plot_columns`, `add_series_to_graph`,
`add_rows_to_graph`); a series added alone has none. The number only makes sense
within its layer (`next_group_id`, `group_members`, `group_ids`). A group is
**not** a node nor a command: it is a field of the `PlotSpec`, changed by
`EditPlotSpec`/`SetPlotSpec` like the rest. Its common style is not stored
separately: the series window writes it into each member (a series set alone
afterwards keeps its exception). The Object Manager node under a layer
(`VisibilityTarget::Group`, formerly "Plot Group", shown as "Series") lists all
the series of the layer; each series group is a parent node there (see below).

**Palette of a group (#213).** `PanelSpec::group_gradients`
(`BTreeMap<u32, Vec<String>>`) keeps the stops of the last palette applied to a
group (`set_group_gradient`); with no entry, no palette has been chosen. A series
that joins the group (`set_series_group`) is recolored; dissolving or removing
the group forgets the entry. Neither the name nor the origin of the palette is
kept: the series window recognizes it by its stops.

**Name, order and gestures of a group (#197).** `PanelSpec::group_names`
(`BTreeMap<u32, String>`) gives a group a name; with no entry, the automatic name
"Group n" (`object_manager::group_title`, wherever a group is named). A name only
makes sense for a group that exists: `group_label` ignores it otherwise, and
`next_group_id` does not reuse the number of a name left alone, so a name never
passes to a new group. **A group is always contiguous** in `PanelSpec::series`
(the plot and legend order): the gestures are methods of `PanelSpec`, which
arrange the series by `slots()` (a single series or a group and all its members —
the projection that the Object Manager and the series window share):
`set_series_group` (join after the members, leave just behind the group, create
one), `dissolve_group` (the series stay, style kept), `remove_group`,
`rename_group`, `move_series` (stays in its group, jumps over a whole group),
`move_group`, `apply_group_gradient`. A group is still not a node nor a command:
the interface goes through `EditPlotSpec` (patterns `RenameGroup`,
`DissolveGroup`, `RemoveGroup`, `ReorderGroup`, `ReorderSeries`), `pyst` through
`SetPlotSpec` (`group_series` gathers series into a new group, #196; surface:
`docs/python-and-scripting.md`, § Series groups).

**Palettes (#178).** `stokesforge_plot::gradient`: `GRADIENT_PRESETS` (blue →
red, black → red…) and `gradient_colors(arrêts, n)`, `#RRGGBB` colors evenly
spaced between the first and the last stop, whatever `n` is. Nothing is stored at
the time of applying: choosing a palette (or changing a stop) immediately writes
the colors into the series of the draft (#213). A **custom** palette is saved
under a name: `SavedColors::gradients` (`SavedGradient { name, stops }`, bounded
like the colors, `with_gradient_added` replaces by name, `with_gradient_removed`),
so in the project (`SetSavedColors`, undoable, `.stf`) or for the user
(`couleurs.json`, outside the history), through `Action::SaveGradient` /
`RemoveSavedGradient`.

The lists of children live **inside** the kind of the node: a column, a graph, a
matrix or a note has none, and this is guaranteed at compile time rather than by
a check.

**Window** = workbook, graph, matrix or note — a node that can be shown as a
subwindow. Their names are unique within their **parent folder** (#105). A
canonical path includes that folder; a bare name is refused if it is ambiguous.

### The tree is persistent, in the functional sense

Mutating a tree means `Arc::make_mut` on the tree — which clones the map, pairs
of 16 bytes, **not the nodes** — then `Arc::make_mut` on the touched nodes only.
A client that holds an old `Arc<Tree>` therefore sees a frozen snapshot, and all
the untouched nodes stay physically shared between the two versions.

This is the mechanism that makes structure undo free: keeping the tree from
before and the one from after costs one pointer.

The `Tree` mutations are `pub(crate)`: `Project` is the only public mutation
facade. Each one **validates everything before writing anything**, so that a
failure leaves the tree unchanged.

**To read**: `stokesforge-core/src/tree.rs`, `src/node.rs`, `src/project.rs`.

### Names and paths

A valid name is non-empty, has no border whitespace, has at most 255 Unicode
characters, and has no reserved character (`[`, `]`, `!`, `/`), no control
character, and no invisible formatting or direction mark — `U+202E` RIGHT-TO-LEFT
OVERRIDE would visually reverse any path that quotes it.

The path grammar is that of Origin: `[Book]Sheet!Col`, with optional folders as a
prefix (`/Manips/[Book1]Sheet1!ColA`). There is a single implementation in the
repository — `Tree::resolve` — and it is the one the Python bridge calls too.

**To read**: `stokesforge-core/src/name.rs`, `src/path.rs`.

## Columns

A column is a vector of values of free length, plus its metadata. Its **name
lives in the node**, not in the column.

Three decisions structure the module:

- **Missing value** — `f64::NAN` for `Numeric` and `DateTime`, `None` for
  `Text` (distinct from `Some(String::new())`, which is a deliberate empty
  string), `MixedCell::Empty` for `Mixed`.
- **Contiguous buffers, never chunked**, shared behind an `Arc` and mutated
  by `Arc::make_mut`. This is what lets the Python bridge hold a numpy view
  **without copy** as long as it holds a clone of the `Arc`.
- **Modification counter** (`Column::version`): to know that a column has
  changed without rereading its values. It is also the optimistic condition that
  `Command::SetCells { expected_version }` checks, and that closes the TOCTOU
  between the snapshot a script read and the command it sends.

Types: `Numeric`, `DateTime` (POSIX timestamp in seconds, shown and edited in ISO 8601 by `datetime::format_datetime`, #218), `Text`, `Mixed`. Plot roles: `X`, `Y`
(default), `X2`, `Y2`, `X3`, `Y3`, `X4`, `Y4`, `Z`, `XError`, `YError`, `Label`, none
(`ColumnRole::ALL`: the menu order; `layer()`: the layer number;
`paired_x()`: the X of a Yn). `X2`…`Y4` (#167, #168) each feed an **overlay
layer** of their own (`PanelSpec::overlay`, `OverlaySpec { x_top,
y_right, layer }` where `layer` is 2, 3 or 4: the number, not the position, says
where a column goes; 0 for a layer without a role): each Yn pairs with the
nearest Xn to its left, like Y with X (`stokesforge_core::compose::series_from_columns`,
`PlannedSeries::overlays`).
A layer is identified by its **index** in `PlotSpec::panels`; `(row, column)`
only serves the grid, as several layers can share a cell.

**To read**: `stokesforge-core/src/column.rs`.

### `Mixed` — the type carried by the cell

The first three types say "the whole column is like this". `Mixed` says "each
row decides": its buffer is a `Vec<MixedCell>`, and a `MixedCell` is
independently `Empty`, `Number(f64)` or `Text(String)`.

The need comes from a real import. A column of measurements that carries
`1,2 / 1,3 / n/a / 1,5` had only two outcomes, both bad: everything as numbers,
and the `n/a` becomes a missing value indistinguishable from an absent
measurement; everything as text, and nothing can be plotted anymore.

Four things to know before touching it.

**The three homogeneous types remain fast paths.** Making *every* column a
`Vec<MixedCell>` would cost 32 bytes per row instead of 8 (a `MixedCell` has the
size of a `String` plus its discriminant) and, above all, would remove the
contiguous `Arc<Vec<f64>>` buffer on which the copy-free numpy view of the Python
bridge and the zero-copy resolution of a series rest. `Mixed` is **one more**
type, paid for by whoever uses it.

**Three distinct "missing" values in storage, one in display.**
`Empty`, `Number(f64::NAN)` and `Text("")` are stored distinctly and compare
distinctly; all three make `is_missing()` true. It is the same asymmetry as
`None` versus `Some("")` for a text column.

**A mixed column can be plotted.** `Column::numeric_view` draws from it an `f64`
view where the text and empty cells are `NaN` — which the renderer already
discards. The result is that a mixed column plots exactly its numeric cells, and
that no backend learned anything (invariant 5). It is the **only** read in the
module that copies: a `Vec<MixedCell>` is not a `Vec<f64>`, there is nothing to
share. The cost is `O(n)` per resolution and per mixed column *referenced by a
series*, never per column of the project.

**Conversions are destructive, like the others, and `Text → Mixed`
promotes.** A text that parses as a number becomes `Number` — through
`parse_f64_text`, the only parsing in the module, so never in disagreement with
`Text → Numeric`. This is what makes the type change useful: the column of the
example becomes three numbers and one text.

On the calculation side, a mixed column is **not** "non-numeric": `formula`
accepts it and its holes are `NaN` row by row. Only an entirely text column is
refused outright.

**Formulas by identifier (#321).** `ColumnMeta::formula` is a
`ColumnFormula`: a sequence of `Text` fragments (operators, numbers,
parentheses) and `Column(NodeId)` — the column read, kept by its identifier,
never by a position letter. The letters (`A`, `B`… by position in the sheet)
exist only at the boundaries: `Tree::formula_from_letters` (input, paste, `pyst`)
converts them to identifiers and refuses an unknown letter or the column itself;
`Tree::formula_letters` gives them back for display, `#REF` for a column that is
no longer in the sheet (deleted or moved elsewhere: it stays stored, Undo brings
it back). Inserting, deleting or moving columns (including the outputs of an
analysis, #320) therefore never changes what a formula reads. A formula
duplicated with its sheet reads the columns **of the copy**
(`Project::insert_copy`); an identifier that was read is never reassigned
(`Project::from_tree`).

## Analysis calculations (#181)

`stokesforge-analysis` is the pure calculation (no workspace dependency, `NaN`
means "empty"). `stokesforge-core` takes only its data types and makes the link
with the tree: `Tree::calculations()` (`BTreeMap<NodeId, Arc<Calculation>>`, in
`calc.rs`). A `Calculation` is **not a node**: no name, parent or window, it
travels with the snapshot, the rollback and the `.stf` next to the saved colors,
with a `NodeId` from the same allocator (never reused).

- It links an `Operation` (baseline, correction, integral, fit, peaks,
  deconvolution), its input series (`SeriesRef`, `AuxRef`), its output columns
  (`outputs`) and its results (values, standard errors, warnings).
- The trace "this column comes from that calculation" is read in `outputs`
  (`Tree::calculation_of_column`), **never** on the column: a copy or a
  move cannot duplicate it.
- `compute(tree, op, series, aux) -> Computed` is the pure function that the
  preview, the command, the session recalculation and `pyst.analysis` share.
- Staleness (`Staleness`): `Manual` (automatic recalculation turned off),
  `InputMissing` (an input has disappeared), `Failed` (the calculation fails). A
  deleted output stays dangling and reads back as "output deleted"; deleting an
  input does not edit the calculations that cite it.
- A calculated column is **read-only** for cells, type and cell deletion
  (`CoreError::CalculatedColumnReadOnly`); its metadata stays editable, except
  the formula.
- Writing: commands ([`commands-and-history.md`](commands-and-history.md));
  targeted recalculation by fingerprint, like formulas (invariant 11); file:
  [`persistence.md`](persistence.md).

## Linked transposed sheets (#253)

Analyzing a **row** (a "row" series of a graph, whole rows of the gutter) is done
on a **transposed sheet**: the analysis engine, the calculations and the
targeted recalculation stay in columns. `Tree::transposes()`
(`BTreeMap<NodeId, Arc<TransposeLink>>`, `transpose.rs`), indexed by the
transposed sheet, travels with the tree like `calculations()` (snapshot,
rollback, `.stf`). It is not a node.

- The sheet is named "`<Source>ᵀ`" (`TRANSPOSE_MARK`), placed right after the
  source; the name taken by a sheet that is not linked to this source gives
  "`<Source>ᵀ 2`", "… 3". It carries an "x'" column (the x' of the columns of the
  source) then a "Row N" column per requested row, on demand; a row requested
  again reuses its column. Long names "Row N of Source"; the unit of the "Row N"
  columns is that of the columns of the source when it is common. The model has
  no unit for x': the x' column carries none.
- **Recalculable snapshot, no live link.** The link keeps what it read
  (`TransposeSourceColumn`: identifier, version and x' of each column of the
  source, in order) and the fingerprint of the content of each column it wrote
  (`content_stamp`, FNV-1a, stable). Two states are **derived** from the tree,
  without being stored: *stale* (the source is no longer what was read: value,
  x', column added, deleted or moved) and *modified* (the content of a column
  created by the link no longer has its fingerprint: only a value edit counts;
  renaming, typing without changing the numbers, adding its own columns cut
  nothing). Neither enters the targeted recalculation.
- Only the columns of role **Y** are transposed (approved on 2026-10-22): an X
  column, an error bar or a label is skipped, with no x' required; a sheet
  without a Y column is refused. Changing the role of a column makes the
  transposed sheet stale.
- Refusals: a Y column of the source without x' (`CoreError::TransposeNoXPrime`)
  or containing text (`TransposeNotNumeric`); both name the column by its
  **letter** and its name. A shorter column gives `NaN`.
- Life of the link: source deleted or transposed sheet deleted, the link
  disappears (`Project::prune_transposes`) and the sheet that remains is
  ordinary, without loss; source renamed, the transposed sheet follows ("Newᵀ"),
  unless the user renamed it their own way; source or transposed sheet duplicated
  or copied, the copy is ordinary.

## `PlotSpec` — what to plot

```text
PlotSpec
├── title, background
└── panels : PanelSpec           (row/column grid, at least one panel)
    ├── title
    ├── bottom / left / right / top → AxisSpec  (scale, limits, grid)
    ├── legend : LegendSpec
    ├── series : SeriesSpec
    │   ├── y : NodeId, x / x_error / y_error : Option<NodeId>
    │   ├── row : Option<usize>   ("row" series, #166: see below)
    │   ├── plot_type : PlotType
    │   ├── line : LineSpec, symbol : SymbolSpec
    │   └── axis : SeriesAxis    (left or right)
    └── annotations : AnnotationSpec   (free texts, issue #35)
        ├── text                  (markup Typst)
        ├── x, y + axis : SeriesAxis   (center, in data units)
        └── color, size, rotation (empty = automatic; degrees, counter-clockwise)
```

A series carries **no data** and **no column name** — only the identity
(`NodeId`) of the columns it plots. This is why renaming or moving a column
breaks nothing.

- `PlotType`: `Scatter`, `Line`, `LineScatter`, `Bar`, `Histogram`,
  `ErrorBar`.
- `AxisScale`: `Linear`, `Log`. `AxisSide`: `Bottom`, `Left`, `Right`, `Top`.
- The style borrows its vocabulary from no engine: named enumerations
  (`LineStyle`, `SymbolShape`), whose translation table lives in the
  renderer.
- `AnnotationSpec`: a free text **anchored to the data** — its center is the
  point `(x, y)` of the ordinate `axis`, so it follows the zoom; it is not
  bounded to the view framing (margins included, #163), only a point outside the
  scale (≤ 0 in log) is not plotted. `AnnotationRef { panel, index }` addresses it in a
  `PlotSpec` (`get`, `set` which replaces or adds, `remove`): no dedicated
  command, everything goes through `SetPlotSpec`. `PlotSpec::set_text_color`
  (theme) also writes their color.
- `ShapeSpec` (issue #76): a **graphic object** — rectangle, ellipse, line
  or arrow (`ShapeKind`) — anchored to the data by two points `(x1, y1)` /
  `(x2, y2)` (opposite corners, or tail and tip) of the ordinate `axis`.
  Stroke color (empty = automatic ink), thickness, fill color
  (empty = none; opacity is the alpha of `#RRGGBBAA`).
  `PanelSpec::shapes` is **a single ordered list**: the stacking order
  is the rank (the last is on top); `ShapeRef { panel, index }`
  addresses it (`get`, `set`, `remove`, `bring_to_front`, `send_to_back`). Like
  the free texts, no dedicated command: everything goes through `SetPlotSpec`.
- `TextAnchor { x, y, axis }` (issue #53): the position of a moved title,
  **center in data** like a free text but not bounded to the view framing. It
  lives in `PlotSpec::title_anchor`, `PanelSpec::title_anchor`,
  `AxisSpec::title_anchor` and `LegendSpec::anchor`; `None` = the default
  place. `TextTarget` (`Title`, `PanelTitle`, `AxisTitle`, `Legend`) addresses it
  in a `PlotSpec` (`anchor`, `set_anchor`, `text`, `set_text`);
  `PlotSpec::reset_text_anchors` removes them all. No dedicated command:
  everything goes through `SetPlotSpec`.
- `TextFont { family, size }` (issues #77, #78): the font and size of a whole
  text **zone** — never a tag inside its content. Empty / `None`
  = the font of the graph and the size of the pipeline. Fields `PlotSpec::title_font`,
  `PanelSpec::title_font`, `AxisSpec::title_font`, `LegendSpec::font`; a free
  text keeps its size in `AnnotationSpec::size` and gets `font`.
  `TextTarget::font` / `set_font` address them. No dedicated command:
  `SetPlotSpec` (`Action::SetTextFont` on the application side).
- **Color of a zone** (#90): `PlotSpec::title_color`, `AxisSpec::title_color`,
  `PanelSpec::title_color`, `LegendSpec::color` (empty = automatic).
  `TextTarget::color` / `set_color` address them; `Action::SetTextColor`
  on the application side. While editing the text, the color stays a Typst tag on
  the highlighted passage.
- `AxisSpec::has_explicit_range`: an explicit framing takes precedence over the
  computed data. It is the lever — already present, nothing was invented for it —
  on which the on-screen zoom/pan relies ([`rendering.md`](rendering.md)).

**To read**: `stokesforge-plot/src/lib.rs` (top), then the module of the sub-type concerned.

## `resolve` — the only point of contact

`stokesforge_core::resolve` translates a spec against the current tree: `NodeId` →
column → data ready to plot. Alone with `compose` (below), it knows
`stokesforge-plot`; everything else in the crate ignores even the existence of
`PlotSpec`.

Two properties to know before touching it:

- **Zero copy.** The columns are already `Arc<Vec<f64>>`. Resolution
  clones the `Arc` — a counter increment — never the buffer. A
  `ResolvedValues` is a view on the same memory as the column,
  possibly truncated to the common prefix of a series whose columns have
  different lengths (battery `b16`). **One exception, and only
  one**: a `Mixed` column has no `f64` buffer to share, its view is
  materialized (see above).
- **A broken reference is a first-class state, not an exception.**
  `ResolvedSeries` always exists, with `missing` non-empty and `ok()` false.
  Nothing crashes, the series is not removed from the spec, it stays visible and
  marked (invariant 10).

**To read**: `stokesforge-core/src/resolve.rs` (top).

## `compose` — building the series of a graph (#257)

`stokesforge_core::compose` is the reverse direction of `resolve`: it reads the `Tree`
and builds `SeriesSpec`s, or reads a `PlotSpec`. A single rule for
the interface (`AppState::plot_columns`, in `stokesforge-app-state`, `add_series_to_graph`, `add_rows_to_graph`,
`plot_rows`, the analysis window), `pyst.plot` and the appendix of an analysis:

- **pairing** (`series_from_columns`, `x_for`): each Y is paired with the nearest X
  to its **left** in the sheet, unselected X included; a
  Yn (Y2…Y4) likewise takes an Xn and goes to the layer of its number
  (`PlannedSeries::overlays`).
  With no X on the left, the abscissa stays implicit (row number);
- **palette**: every series goes through `SeriesSpec::from_palette(y, rang)` (line
  and markers), the rank being that of the series in its layer; never
  `palette_color` written by hand;
- **layers**: `overlay_base` (the ordinary layer that an overlay layer covers),
  `role_layer_of` (the layer numbered 2 to 4, `OverlaySpec::role_layer`),
  `overlays_full` / `free_role_layer` (at most three overlay layers per
  cell for "New Layer", #168);
  the layers are named in `stokesforge-plot` (`OverlaySpec::role_layer`, `SECOND_AXES`,
  `TOP_X`, `new`);
- **rows** (`row_series`, `x_prime_columns`, `spec_has_x`): the name of the series
  is a parameter (an interface sentence that this crate cannot write).

Nothing in it mutates: the caller sends the result through an undoable command
(`AddGraph`, `SetPlotSpec`). Removing a series from the panel goes through
`PanelSpec::remove_series`, like `remove_group` for a group.

## What to know before modifying

- Data read by an analysis calculation enters the recalculation fingerprint,
  as for formulas: see « Recalcul des formules » in `CLAUDE.md`.
- Adding a mutating `pub` method to `Project` without a matching command
  in `stokesforge-commands` opens a non-undoable write path
  (invariant 1).
- Adding a dependency to `stokesforge-core`, `stokesforge-plot` or `stokesforge-id` makes
  their `tests/no_ui_dependency.rs` fail until it is added to the
  allowlist — deliberately.
- Adding a field to a model type is not enough: the `.stf` format has its
  **own** transport structures and hand-written conversions
  ([`persistence.md`](persistence.md)). Every new field or variant follows the
  "Version rule" of that document and updates the witness file.
- Some caps look like bugs and are not (name length,
  number of histogram bins, `MAX_NODE_ID`, `MAX_CELLS`…). They are
  documented where they are declared, in a named constant, and
  listed in `docs/archive/rustrewrite/questions-ouvertes.md` §11.

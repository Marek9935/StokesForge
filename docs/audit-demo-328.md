# Audit #328: the demo figures and their interface gestures

**Reading** audit (2026-10-23), without running the application: for each
element of the six demo graphs
(`crates/stokesforge-app/src/demo/graphs.rs`), we look for the interface
gesture that produces it. The ✗ are established by the absence of any call in
`app/`, `app-state/` and `render-egui` (text search with no result). No hidden
gesture in the context menus of the Object Manager was searched for beyond
those cited.

## Table by graph

The numbers in brackets refer to the gaps numbered below.

| Graph | Element | Interface gesture |
|---|---|---|
| `Graph1` (:66) | curve, axis labels, color and width | ✓ Graph › Plot (menu/drag), Graph Properties › Axes / Series |
| `Panels` (:90) | 2×2 grid of 4 cells (`PanelSpec::at(l,c)`) | **✗ [1]** |
| | one series per cell, type Line/Scatter/Histogram/Bar | ✓ per cell, but only in 1×1 |
| | Histogram 20 bins (:92) | **✗ [2]** |
| `Axes` (:144) | log scale, grid | ✓ Graph Properties › Axes (`plot_settings.rs`) |
| | series on the right Y axis of the same panel (`SeriesAxis::Right`, :165) | **✗ [3]** |
| | title of the right axis | ✓ Axes › Right tab (`PlotAxis::Right`, state.rs:2213) |
| | layer with secondary axes (:181) | ✓ Graph › New Layer (menus.rs:456-505), or X2/Y2 columns (`ColumnRole`, column.rs:205) |
| | titles and units of the layer axes | ✓ Axes tab (Top and Right) |
| `PlotTypes` (:197) | 5 cells in a 2×3 grid | **✗ [1]** |
| | the 5 plot types | ✓ type list (plot_settings.rs:771-776) |
| | ErrorBar with `y_error` (:205) | **✗ [4]** |
| | Histogram 30 bins | **✗ [2]** |
| | cell title (:258) | ✓ `panel.title` (plot_settings.rs:379) |
| `Annotations` (:265) | text with a Typst formula | ✓ Graph › Add Text, or the Text tool |
| | rectangle, ellipse, line, arrow | ✓ Tools bar (`shape_tools.rs`), drag on the plot area |
| | legend in a corner | ✓ Legend tab (`legend_editor`) |
| `Groups` (:302) | six curves in a group | ✓ plotting several columns together (state.rs:9400) |
| | color gradient of the group | ✓ `curve_settings.rs` (`set_group_gradient`) |
| `Rows` (:336) | "row" series | ✓ dragging rows from the gutter (gestes.md:96), `Action::PlotRows`, row menu (grid.rs:915) |

## Elements without a gesture

Classification: **A** = the gesture exists, the documentation is missing; **B** =
missing feature.

1. **Grid of several non-overlapping cells: B.** Nothing in the application
   adds a `PanelSpec::at(l, c)` to an existing graph. The only calls are
   `app.rs:2678` (new graph in 1×1), and `analysis_preview` / `object_drag` /
   `state.rs:9413` for layers. `graphe.md:594` explicitly rules it out
   ("no adding/removing of a series or panel from the window").
   A work package in its own right. It is not a duplicate of #168, which covers
   only the X3/Y3 layers and following.
2. **Number of bins of a histogram: B.** `SeriesSpec::histogram_bins` has
   no widget. `graphe.md:592` acknowledges it as "left for a later iteration".
   Small work package: one more field in Series.
3. **Series on the right Y axis of the same panel: B.** `SeriesAxis::Right`
   is assigned only by the demo, `pyst` and persistence. The only neighboring
   widget is the Axes › Right tab, which sets the axis without placing a series
   on it. The need is partly covered by a layer with Y on the right (New Layer,
   or X2/Y2 columns), but it is a different structure: the demo puts two series
   in the same panel, a layer is a separate panel. To decide: is the layer
   enough, or do we want a "series axis" choice?
4. **Error bars (`y_error`): B.** The ErrorBar type can be chosen, but no
   column can be designated as error: no error column role
   (`ColumnRole` has only X, Y, X2…Y4 and their variants), no widget. Only
   `analysis.rs` reads `y_error` (as `sigma`). Without an error, the ErrorBar
   plot shows nothing. This is the most annoying one: a plot type that is
   offered but cannot be fed.

## Documentation (case A, to handle right away)

- `gestes.md` cites neither New Layer, nor adding shapes, nor grouping by
  multiple plotting, nor the color gradient. These gestures exist and are found in
  `graphe.md:131` and `menus-et-barres.md:21`, but not in the gesture
  catalog.
- The Typst syntax of texts (`$lambda_1$`) is documented only on the rendering
  side. Not verified: does an interface text or `graphe.md` explain it to the
  user?

## Proposed split into sub-issues (not created)

- **S1 (A, small)**: complete `gestes.md` and the tooltips: New Layer,
  shapes, groups and color gradient, Typst in Add Text. Check: redo `Annotations`,
  `Groups` and `Rows` by following the documentation.
- **S2 (B, small)**: "Bins" widget (`histogram_bins`) in Series. It
  covers `Panels` and `PlotTypes`.
- **S3 (B, medium)**: designate the error column of an ErrorBar series. To
  decide between a column role (`yEr`, like X2/Y2) and a field in Series.
- **S4 (B, to arbitrate)**: assign a series to the right Y axis of the panel, or
  declare that the layer is enough and rewrite the `Axes` demo with a layer.
- **S5 (B, big, decision first)**: grid of several cells (adding,
  removing, row/column placement). Depends on the product choice "multi-panel
  figures in V1". Before the code, a decision form: gesture
  (Graph menu, drag, Object Manager), number of cells, independence of axes.
- **Option**: if S5 is postponed after V1, rewrite the `Panels` and
  `PlotTypes` demos as separate graphs and say so in the demo documentation.
  Issue #328 explicitly accepts a demo graph that cannot be reproduced.

**Consistency**: `docs/application/etat-et-console.md` (description of the demo),
`docs/gestes.md`, `docs/application/graphe.md`.

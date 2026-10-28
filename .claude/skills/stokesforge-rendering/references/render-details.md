# Rendering details

Read from `stokesforge-rendering` when the task touches the drawing order, layers, Typst, a continuous gesture, or the tests.

## Panel drawing order

Each step depends on what the previous one learnt (`docs/rendering.md` § The order in which a panel is plotted):

1. prepare each series as geometry (axis scale applied); framing and drawing read **the same** geometry;
2. frame the axes on it;
3. build the plotting area — the pixel width exists only here;
4. decimate (LTTB), which needs that width;
5. paint in pixels;
6. the legend last, measured by the same path as the axes.

Two decisions already arbitrated: the log scale transforms **values**, not the coordinate type (`docs/rendering.md` § The logarithmic scale transforms the data, not the type); clipping is done **in pixels**, after projection (`docs/rendering.md` § Clipping: in pixels, not in data).

## Overlaid layers (#167, #168)

A `PanelSpec` with `overlay: Some(OverlaySpec)` covers the last ordinary layer of its cell; the pipeline reserves the **union** of the cell's graduation areas (`CellPlan`). A layer is identified by its **index in `PlotSpec::panels`** (`DrawnPanel::index`), never by its rank in `DrawnPlot::panels` nor by `(row, column)`. Several layers' axes on one side stack in bands. Tests: `stokesforge-render/tests/geometry.rs`, `stokesforge-render-svg/tests/layers.rs`.

## Selection handles (#98)

Placed and drawn by `stokesforge_render::series_selection` (generic over the backend), laid by `GraphView` **over** the cached figure: selecting rebuilds nothing, and export never sees them.

## Typst (firm dependency)

Always compiled; `cargo test --workspace` covers it (`docs/rendering.md` § Typst (R8)). Backends switch on `formula::is_rich`, at measurement **and** drawing, never on the presence of `$`. `collect_shapes` / `render_to_string` draw everything as plain text — the tests' witness; `collect_shapes_with_typst`, `render_to_string_with_typst` and `with_typst()` go through Typst. `GraphView` always does, with a bounded `FormulaCache`. Text colour (`formula::ink`) enters compilation and cache key: a formula is never tinted afterwards. SVG text stays `<text>` by default (`SvgTextMode`), outlines being an option. A formula that fails to compile shows as raw text, `$…$` included (invariant 10). Interface word: Typst text between `$…$` is « Math » on screen and in keys (`plot-math-*`); « Formula » is reserved for a column computation.

## Preview on the view, one write in the model

A continuous gesture (dragging free text, editing in place, moving a title or the legend) never edits the model each frame: the preview sits on the view (`GraphView::set_annotation_preview`, `set_text_preview`, counted in `view_generation`), and the app sends **one** action at the end — one history entry. `hold_preview` keeps it until the model catches up. App side: `app::graph_window::annotation_interactions`, editors in `app/graph_text_edit.rs` (`commit_annotation_editor`). Pan is Ctrl + drag or middle-click drag, decided on press (`decide_nav`), never a plain drag (#318).

## Errors

`RenderError` lives in `stokesforge-render` (#255); `stokesforge-render-egui` re-exports it. Never redefine it in a backend: `stokesforge-app-state/src/errors.rs` reads it without egui.

## Tests

The architecture guards of rendering are listed in `docs/tests.md` § Guards not to break (`measurement.rs`, `agreement.rs`, `feathering.rs`, `no_ui_dependency.rs`, `view`/`interact` modules, `interaction.rs`, `cache_session.rs`). Also: `stokesforge-render-egui/tests/shapes.rs` (each backend's anchoring conventions). Touching Typst: the plain-text witness stays intact, and an unchanged formula title is not recompiled (`cache_session.rs`).

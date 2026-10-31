---
name: stokesforge-rendering
description: Work on StokesForge rendering — plots, axes, legend, zoom/pan, SVG/PDF/PNG export, Typst. Use when touching stokesforge-render* or for a visual symptom (« figure décalée » (figure shifted), « texte tronqué » (text cut off), screen differs from export, figure not refreshing).
---

Rendering is where a bug can be silent: the rules below guard against a wrong figure that looks right, not against a compile error.

## Read and load

`docs/rendering.md` § What to know before modifying, then load `stokesforge-render` **first** and only then the target backend (`.claude/rules.md` § Routing). The silent traps are written once: `.claude/rules.md` § The most expensive pitfalls ; `.claude/conventions.md` § Rendering: silent pitfalls. Read both before writing.

```text
stokesforge-render            all rendering, generic over DrawingBackend
   ↑                       ↑
stokesforge-render-egui   stokesforge-render-svg
   (screen)                (SVG → PDF, PNG)
```

## Steps

1. **Place the feature in `stokesforge-render`**, generic over the backend (invariant 5). The only living exception is `stokesforge-render-egui::feathering`. A data translation (a mixed cell becoming `NaN`) happens upstream, once.
2. **Measure text only through `stokesforge_render::text`** (invariant 4). Plotters' `configure_series_labels` and `MultiLineText` bypass it: that is why the legend is hand-written in `stokesforge_render::legend`.
2b. **Size the figure from `pipeline::figure_layout` only** (#394): stored frames (plot areas) + axis bands + title band, frames aligned across a column and a row, rounded prefix-sum breakpoints; a new reader of the figure size (export, copy, label, ratio) calls it (`GraphView::figure_layout` on screen, `render_svg::figure_size` for files), never a constant. "Fixed" figure size, Left and Top (#405) are floors on its gutters, explicit path only; the overflow flag is `FigureLayout::fixed_overflow`.
2d. **Total image size** (#407): `pipeline::fit_figure_size` sets the width or height of the whole image by scaling the frames in proportion (bands as `figure_layout` gives them, no iteration); fit the width, then call again for the height (title bands are measured at the figure width). Free: scale about the top-left of the frames, bisection on the bounding box. A target outside `range` comes back at the nearest end with `raised`; never clamp elsewhere (`docs/rendering.md`, "Setting the total size").
2c. **Free placement** (#398, `layer_placement = Free`): `FreeLayout::build` is the one copy, like `ExplicitGrid`, checked before `explicit_frames`; cells keyed by base layer index (`PlotSpec::cell_bases`), never `row * columns + column`; each outer edge rounded once in `FreeLayout::place`; `cell_rect`/`frame_rect` answer by `(row, column)`. Drawing order is not spec order in Free (#398): cells go by `(FreeFrame::stack, base index)`, `DrawnPlot::panels` is re-sorted to spec order, `DrawnPanel::cell_rank` carries the order, an Auto layer background fills the outer rectangle only over an earlier cell, and hit-testing / `panel_at` ignore anything under a higher-ranked cell's outer rectangle (a forward scan over `panels` is a bug). A change to the grid layout checks the free one too (`docs/rendering.md`, "Free placement of the cells").
3. **Leave the cache key alone** — `(revision, rect, pixels_per_point, view_generation)`: feathering never enters it (invariant 6); a selection enters neither the key nor the pipeline; zoom and pan stay in `ViewState`, never in the `PlotSpec`.
4. **No `f64` rule in `stokesforge-render-egui`**: framing, zoom, pan, nearest point live in `stokesforge-render` (`view`, `interact`); egui keeps only the `Pos2` adapter. A screen painted outside `GraphView` (analysis preview) paints in the app but takes all geometry from `stokesforge-render`.
5. **Choose a figure text's style in the pipeline**: `rich_style` (Typst) for titles, axis titles and legend entries, `literal_style` for text taken from a column; graduations never go through Typst.
5b. **Number of ticks** (#397): `scale::tick_count` once per axis in `draw_panel`, passed to every label, tick mark, minor tick, decimals and "Label every" call. Never `DEFAULT_LABELS` or a recomputed count at a call site (`docs/rendering.md`, "Tick count follows the axis length").
6. **Add the feature's witness** to `stokesforge-render-egui/tests/agreement.rs`: a variant of the graph that changes the feature must give a different figure. Read the existing witnesses of `agreement.rs` and `stokesforge-render/tests/measurement.rs` first (`docs/tests.md` § The witness, without which an equality test proves nothing).

Panel drawing order, overlaid layers, Typst paths, preview-then-one-write gestures and the test map: [references/render-details.md](references/render-details.md).

Done when the feature lives in `stokesforge-render`, its witness is in `agreement.rs`, the cache key is unchanged (or the change is justified in the report), and `stokesforge-verification` is green; a visual change is also **looked at** with `run-stokesforge`.

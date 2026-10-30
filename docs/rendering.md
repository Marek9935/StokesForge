# Rendering

Three crates, and one rule that explains how they are split: **a plotting
feature never goes into a single backend** (invariant 5).

```text
stokesforge-render          all the rendering, generic over DrawingBackend
   ↑                        ↑
stokesforge-render-egui    stokesforge-render-svg
  (screen)              (SVG → PDF, PNG)
```

The two backends differ only in **how they put pixels down**. Everything
else — series types, scales, primitives, legend, decimation, clipping,
formulas — is in `stokesforge-render`.

**Who does what for interaction (#262).** Every `f64` computation — pixel ↔
data, zoom, pan, rubber-band rectangle, framing from a gesture, nearest
point — lives in `stokesforge-render` (`interact`, `view`, `hit`,
`series_selection`) and is tested without egui. `stokesforge-render-egui` keeps
only what touches egui: reading the wheel and the drag (`handle_navigation`),
converting `Pos2` ↔ `(f64, f64)` (`nearest_point` is an adapter there: it
applies `HIT_RADIUS` and returns a `PointHit` as a `Pos2`), putting down the
`Shape`s (`backend`, `scaled`, `cache`, `shape_tools`, `typst_screen`) and
*feathering*. `GraphView` holds a `ViewState` as a field and calls
`ViewState::commit_gesture` with the panels of the last plotted figure.

## `stokesforge-render` — the common base

| Module | What it holds |
|---|---|
| `text` | **the single source of text measurement** (invariant 4) |
| `fonts` | font choice, three embedded pairs or one system font |
| `pipeline` | the sequence of Plotters calls, written once |
| `series` | the six `PlotType`s translated into geometry |
| `scale` | the axis scales, including the logarithmic one |
| `paint` | the pixel primitives: sub-pixel strokes, markers, bars, whiskers |
| `clip` | geometric clipping of a polyline against the plot rectangle |
| `legend` | the legend, laid out by hand |
| `decimate` | LTTB |
| `interact` | pixel ↔ data mapping of a plotted panel (`PanelTransform`, `Gesture`) and the point nearest the cursor (`nearest_point`, cursor as `(f64, f64)`, #262). For a caller that paints by itself (analysis preview, #261): `PanelTransform::new` (fractional area in `f64`, bounds in display order), `fit_range` / `axis_frame` (initial frame: 5 % margin in the transformed space, remembered bounds, inversion) and `snap_to_curve` (nearest point in X, average over *n* points; `snap_to_curve_zone` also returns the averaged zone, min and max abscissas and the real number of points, #334). `scale::axis_ticks` gives the ticks and their label through the pipeline path (`linear_key_points` + `label_count` + `TickFormat`) |
| `view` | the view state — zoom and pan (`ViewState`, `Framing`) — and the framing a gesture gives (`commit_gesture`, `commit_axis_gesture`), without egui (#262) |
| `hit` | component (title, plot area, axis, axis title) under a pixel, on the geometry allocated by Plotters (`DrawnPlot::title_height`, `DrawnPanel::axis_areas`) |
| `series_selection` | series under a pixel, on its actually plotted pixels, and handles of the selected series (#98) |
| `color` | reading of the spec's `#RRGGBB` colors (adapter of `stokesforge_plot::parse_hex`, the **single source** shared with the interface's picker), and the automatic ink contrasting with the background (`auto_ink`) |
| `formula` | Typst (a firm dependency, not a feature): the "rich text" style of titles, axis titles and legend, compilation, the cache |
| `sample` | two witness figures with explicit titles |

Curves keep their fractional coordinates after projection and clipping, then
are plotted to one sixteenth of a pixel on screen and in the SVG/PDF/PNG
exports (#150). Dashes follow the same precision. This avoids artificial
steps when zooming out, without smoothing the data; LTTB decimation is
unchanged. `stokesforge-render-egui/tests/jasco_zoom_out.rs` checks the two
framings of the real file `test/import/JascoUV_vis.csv`, fidelity to the
measurements and agreement of the screen/SVG vertices. To look at the same
figures:

```sh
STOKESFORGE_JASCO_CAPTURE=/tmp/jasco150 cargo test -p stokesforge-render-egui --test jasco_zoom_out
```

The titles of `sample::minimal()` and `sample::realistic()` are kept
to exercise the title bands and text measurement in the screen/export
agreement tests and the rendering benches (#103). These samples do not install
a default title: graphs created by the application, demo included,
or by `pyst.plot` without a `title` option have no title. The pipeline reserves
no band when `PlotSpec::title` is empty.

### Transparency

Any color of the spec can carry an opacity (`#RRGGBBAA`,
`color::rgba_or`): strokes, markers, bars, text, scales, background. A graph's
default background has no color (`stokesforge_plot::TRANSPARENT`).
A background of zero opacity is **not painted**: the SVG, PNG or PDF export has
no background (`stokesforge-render-svg`, test `a_colorless_background_gives_a_transparent_png`).
On screen, `stokesforge-app` paints a white under the figure
(`app::graph_window::display_backdrop`):
this white lives in the application, never in this crate, or it would be exported.
To choose the automatic ink, a (semi-)transparent background is judged
composited on white (`color::over_white`).

### Text and scale colors: automatic ink

The graph title (`PlotSpec::title_color`), the title of each axis
(`AxisSpec::title_color`) and its scale (`AxisSpec::tick_color`: axis line,
ticks, labels, and the grid, taken at 20 % / 10 % opacity)
have their own color. An empty string means **automatic**:
`color::auto_ink(fond)`, white on a dark background, dark on a light one
(the choice with the best WCAG contrast ratio). The same ink is used for the
panel title, the frame and text of the legend, and the outline of bars. A dark
background therefore turns everything in automatic color to white at once —
on screen as in the export, since the pipeline is shared
(`stokesforge-render-svg/tests/colors.rs`).

Width of a free text, a title or an axis title (#244): `AnnotationSpec::width` / `TextFont::width` is a fraction of the
figure (bounded to `[MIN_WIDTH, 1]`, `effective_width`), `align` a `TextAlign`. The
pipeline derives a width in backend pixels from it (rounded: the family name
is kept forever), which travels in the **style family**, after the
font (`formula::family_with_layout`, separator `\u{1f}`); `formula::source`
then wraps the text in `#box(width: …pt)[#align(…)[…]]` (inside the rotation
if there is one). The whole box is the page: the size rendered at selection
is that of the box, the anchor stays its center. Both backends go
through `source`. For an area, `pipeline::font_style` takes the width of the figure (also for a vertical title); the legend does not use it. Tests: `the_width_of_an_axis_title_wraps_it`, `a_fixed_width_wraps_the_text`
(`render-egui/tests/annotations.rs`), `pulling_the_handle_fixes_the_width`
(`cache_session.rs`).

Font of a text area (issues #77, #78): the font and size of a
title, an axis title, the legend and a free text are fields of the
area (`stokesforge_plot::TextFont`), not content. The pipeline derives the style from it
(`pipeline::font_style`, size bounded to 4-200) and the font travels in the
**style family** (`formula::rich_style_font`: `stokesforge-typst|Fira Sans`, like
the angle of a rotated text); `formula::source` then puts `#set text(font: …)`
and `#show math.equation: set text(font: …)` before the text, and both backends
ignore it. The Math font follows (`math_family_for`): Libertinus Sans →
Libertinus Math, Fira Sans → Fira Math, Noto Sans → Noto Sans Math, a
system font → Noto Sans Math (fallback). Tests: `formula.rs`
(`an_areas_font_is_set_in_the_source_with_its_math`), `geometry.rs`
(`the_font_size_of_a_text_box_changes_its_size`), `stokesforge-render-svg/tests/typst.rs`.

The title and the legend are **selected** by a click (`GraphView::selected_text`,
blue frame like a free text, issue #77); a legend entry designates the
whole legend, which can also be moved by dragging it from an entry.

Fonts in text (issue #63, **replaced** for the choice of font by what
precedes — the bar no longer puts down a font tag; a `#text(font: …)`
typed by hand is still read): a text can name its font,
`#text(font: "Fira Sans")[…]`. The Typst `World` (`formula::requested_fonts`)
reads these names in the markup and loads the requested family, embedded
(Libertinus Sans, Fira Sans, Noto Sans) otherwise installed on the machine
(`fonts::available_families`, `fontdb` database scanned once). Not found:
Typst falls back to the document font, the text does not disappear — but the
figure changes from one machine to another, since only the embedded fonts
travel. Bold comes from the `wght` axis of Noto Sans; italic from the
italic face of Noto Sans embedded with it (Typst does not fake italics);
Fira and Libertinus have only the regular.

`formula::diagnose(texte)` returns the Typst error message of a rich text (or
`None`), memoized per text: rendering falls back to plain text by itself, and the interface
(`plot_settings::formula_field`, #240) uses it to paint the field red.

A Typst formula follows the same color (issue #30): the backends pass
`style.color()` to the compilation (`formula::ink`), which puts
`#set text(fill: rgb(…))`. The color is therefore **in the rasterized image** or
in the SVG fragment, not a tint applied afterwards: a color written in the
formula itself (`#text(fill: red)[…]`) keeps priority. A text of
zero opacity draws nothing, formula included.

Plotters' `configure_mesh` has only one axis-title style and one
axis-line style for the two main axes: the bottom axis and the left one
are therefore plotted in **two passes**, each cutting out the other axis
(`pipeline::AxisColors`). The geometry does not depend on it.

### The order in which a panel is plotted

This order is not arbitrary; each step depends on what the previous one
learned.

1. **prepare** each series into geometry (`series`) — the scale of its axis is
   applied, histograms are binned. Framing and plotting will then read
   **the same** geometry, so they cannot read it differently;
2. **frame** the axes on this geometry;
3. **build** the plot area — only here does the width in
   pixels exist;
4. **decimate** (`decimate`), which needs this width;
5. **paint**, in pixels (`paint`);
6. **the legend** last, on top, and measured by the same path as the
   axes.

None of this knows the type of the plotted columns: the pipeline receives
`ResolvedValues`, that is `f64`s, and it already discards non-finite points.
This is what let **mixed-type columns** come in
without a line of rendering moving: `stokesforge_core::Column::numeric_view` gives `NaN`
for a text or empty cell, so a mixed column plots exactly its
numeric cells (`stokesforge-render/tests/geometry.rs`,
`a_mixed_column_draws_only_its_numeric_cells`). The right place
for this kind of translation is always that one — upstream, once — and
never a backend (invariant 5).

### Two Plotters traps, to know before touching rendering

`plotters` is taken **without its fonts** (`default-features = false`).

- A `DrawingBackend` that does not override `draw_text` **panics** at the
  first tick.
- One that does not override `estimate_text_size` measures text by the
  **number of UTF-8 bytes** — silently.

And the blind spot that follows: `configure_series_labels` and `MultiLineText`
measure the text **themselves**, through `FontData::estimate_layout`, hence by the
naive heuristic, **never** through `estimate_text_size`. This is the precise
reason why `legend` is written by hand rather than delegated.

Where measurement enters the geometry, concretely: `DrawingArea::titled` —
called as soon as a graph or panel has a title — asks `estimate_text_size` for the
height of the title, then **lowers the top of the plot area** by that
height. A measurement that is wrong by a few pixels moves the whole area, axes
included, without raising any error.

### Why plotting goes down to pixels

Three needs do not go through `ChartContext::draw_series`:

- **stroke styles** — a dash is measured in pixels, not in data
  units, and `DashedLineSeries` knows only a two-beat pattern;
- **markers** — Plotters has no ready-made element for the square, the
  diamond or the straight cross;
- **the legend**, for the reason above.

Going down to pixels costs neither fidelity nor agreement: it is the same code, in
the shared pipeline, that computes the integer coordinates; the two backends
see only a `draw_path`, a `draw_rect` or a `draw_circle` already resolved.

### The logarithmic scale transforms the data, not the type

Plotters can do a log axis (`LogRange`) and that is **not** what is
done: `LogRange` changes the *type* of the coordinate, hence the type of the
`ChartContext`. With two axes, two scales and a secondary axis, the function
that plots a panel would exist in **eight** monomorphizations — exactly the
divergence the single pipeline exists to prevent, coming back in through the
type system's door.

Here: a single coordinate type, linear `f64`, and the scale applied **to the
values**. Only two places know there is a scale: the input
(`forward`) and the labeling (`format_tick`). The price, in one line: the
ticks are **decades**, not the minor ticks that `LogRange`
knows how to put down.

On a log scale, zero and negatives have no image: they are removed from the
plot, as `NaN`s already are.

### Clipping: in pixels, not in data

A logarithmic axis is linear only in the transformed space; a straight line
in data coordinates is therefore not necessarily a straight line there — but it
**becomes** a straight line again once projected to pixels, the
pixel ↔ transformed projection always being affine. A segment/rectangle
intersection is an operation on straight lines: it must be done after the projection.

Algorithm: Liang-Barsky on an axis-aligned rectangle (the plot
rectangle is never tilted).

### LTTB: what it fixes, and what it does not

This is written here rather than diluted elsewhere, because the measurement contradicts
intuition:

| series | points/series | total | tessellation | what LTTB removes |
|---|---|---|---|---|
| 50 | 6,000 | 300 k | 18.3 ms | ~73 % of the points |
| 500 | 600 | 300 k | 22.7 ms | **nothing** |

600 points spread over 1,600 px are already undersampled. The document's load
case — 500 spectra of 600 points — gets its load from the **number of
series**, not their length; the lever that covers it is *feathering*,
not this one. LTTB serves **the other case**: a few very long series, the one
of acquisition.

*Largest Triangle Three Buckets* picks in each slice the point forming
the largest triangle with the point kept on the left and the average of the
right slice: concretely, **it keeps the peaks**, where "one point in
N" loses them.

### Large-curve bench (#141)

The manual test `stokesforge-render-egui/tests/curve_bench.rs` compares
100,000, 1,000,000 and 10,000,000 points **in total**, spread over one or ten
series. It uses the shared pipeline, `ShapeCache`, `ViewState` and the
real egui tessellation, without a window or a GPU renderer. The kept bench calls
`pipeline::draw_on`, with normal LTTB. The experimental mode without LTTB was
removed after the measurements: no public API dedicated to the bench is added.
Its historical results stay recorded below; they are no longer
replayed by this command. The existing screen/SVG agreement tests cover
production decimation. The bench is `#[ignore]`, outside ordinary CI.

```sh
cargo test -p stokesforge-render-egui --release --test curve_bench curve_bench_large_volumes -- --ignored --nocapture --test-threads=1
```

`STOKESFORGE_BENCH_POINTS=100000,1000000` limits the volumes;
`STOKESFORGE_BENCH_FRAMES=5` sets the number of frames per regime (at least
3). Each `IMAGE` line gives the medians of the pipeline (resolution,
preparation, framing, LTTB, projection, clipping, Plotters and collection), of
the egui frame before tessellation, of the tessellation and of the CPU total, then
the maximum of the total, the mesh bytes and the removed points. The medians
per phase do not necessarily add up to the median of the total.

Protocol: 800 × 600 figure, scale 1, numeric columns, shared
abscissas, deterministic signal with two frequencies, no title or legend
(hence no Typst compilation). One warm-up frame is excluded. "Cold"
means the **shape cache invalidated**, not the fonts or the CPU caches.
The zoom varies a central framing of about 80 % with full anti-aliasing;
the pan moves this framing with the cut fringe, as during a drag.
The "warm" frames repeat the last framing without changing the generation;
they correspond to a frame repainted during a pause of the gesture.
`window_warm` keeps the initial framing and the key (cost of the graph when
its window is moved; the application chrome is excluded).

The `PHASES` lines measure `series::prepare` and the isolated LTTB separately
on its geometry, at the real threshold of the plot area. They help identify
the CPU cost, but **must not be subtracted** from the pipeline: allocations
and memory locality differ. The mesh bytes are
a quantity to upload, **not a GPU time**: neither submission, nor driver,
nor rasterization, nor presentation/vsync is measured. The total also excludes
freeing the meshes after measurement, the model and startup.

#### Measurements of 6 October 2026

Intel Core Ultra 7 155H (22 logical processors), Linux x86-64, Rust 1.98.1,
egui 0.36.2, `release` profile, five frames per regime. Non-dedicated
work machine, CPU frequency and affinity not fixed. The tables below
come from the second pass, after our builds finished; they are not
guarantees for other machines nor a native window throughput.

With LTTB (median CPU times in ms; isolated preparation and LTTB outside the total):

| Total points | Series | Isolated preparation | Isolated LTTB | Zoom pipeline | Zoom tessellation | Cold zoom total | Cold pan total | Window, warm cache |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100,000 | 1 | 0.27 | 0.17 | 1.03 | 0.04 | 1.08 | 1.06 | 0.05 |
| 100,000 | 10 | 0.28 | 0.36 | 1.42 | 0.53 | 1.97 | 1.91 | 0.75 |
| 250,000 | 1 | 1.84 | 0.78 | 4.78 | 0.10 | 4.94 | 3.43 | 0.11 |
| 250,000 | 10 | 1.37 | 1.21 | 5.57 | 1.52 | 7.21 | 3.44 | 1.53 |
| 500,000 | 1 | 1.81 | 1.22 | 7.33 | 0.08 | 7.44 | 7.71 | 0.10 |
| 500,000 | 10 | 2.22 | 1.66 | 8.25 | 0.88 | 9.20 | 9.53 | 1.48 |
| 1,000,000 | 1 | 3.03 | 1.74 | 10.48 | 0.05 | 10.55 | 10.36 | 0.05 |
| 1,000,000 | 10 | 2.90 | 2.08 | 10.95 | 0.61 | 11.59 | 11.40 | 0.68 |
| 2,000,000 | 1 | 6.75 | 4.35 | 23.93 | 0.06 | 24.02 | 24.20 | 0.07 |
| 2,000,000 | 10 | 6.56 | 4.11 | 23.20 | 0.86 | 24.09 | 24.22 | 0.89 |
| 5,000,000 | 1 | 34.82 | 9.01 | 80.70 | 0.07 | 80.83 | 79.43 | 0.06 |
| 5,000,000 | 10 | 34.15 | 9.67 | 79.73 | 0.75 | 80.54 | 81.20 | 0.87 |
| 10,000,000 | 1 | 66.97 | 17.43 | 159.85 | 0.06 | 159.96 | 153.72 | 0.05 |
| 10,000,000 | 10 | 71.69 | 18.92 | 163.00 | 0.93 | 163.94 | 163.88 | 0.86 |

The witness without LTTB confirms its usefulness (median CPU times in ms):

| Total points | Series | Cold zoom | Cold pan | Window, warm cache | Window meshes (decimal MB) |
|---:|---:|---:|---:|---:|---:|
| 100,000 | 1 | 7.24 | 3.66 | 5.20 | 15.58 |
| 100,000 | 10 | 11.10 | 4.54 | 10.47 | 19.03 |
| 1,000,000 | 1 | 82.03 | 55.58 | 53.97 | 152.36 |
| 1,000,000 | 10 | 80.85 | 80.28 | 63.36 | 155.74 |
| 10,000,000 | 1 | 971.67 | 821.00 | 633.80 | 1520.35 |
| 10,000,000 | 10 | 821.90 | 732.42 | 525.63 | 1523.50 |

**Reading for the CPU/GPU decision.** The 60 frames/s budget is
16.7 ms for the **whole** frame, of which the bench measures only the CPU part of the
graph. The million points stay under this budget in the reference pass;
the cases of two million and above exceed it. The observed crossing
is therefore bracketed between one and two million points in total on this
machine, for these curves and this width. It does not define a cap
of the model: the data stay plottable, the interaction slows down.

At ten million, more than 99 % of the CPU total during zoom/pan with LTTB belongs
to the frame before tessellation. The threshold is 1,424 points per series: the final
mesh is small, but preparing, framing and decimating still walk over the whole
data at each generation. The warm-cache frames with LTTB
stay small; without LTTB the cache keeps huge paths that egui
re-tessellates and clones at each frame (up to about 1.52 GB of meshes at the
initial framing).

A first pass during builds gave about 30–32 ms at
a million points (one series) and 191–410 ms at ten million; the second gives
about 10–11 ms and 154–160 ms. This spread justifies keeping manual
measurements and medians, rather than a timing assertion in CI.

**Decision:** the GPU path is dropped at the end of this experiment (#141).
These measurements do not justify developing Vello or a direct wgpu
path for the already-decimated long curves: replacing only
the tessellation removes almost nothing of the measured CPU cost. The lead to study
together is the reuse of the prepared geometry and/or a multi-level
decimation, keeping scientific fidelity and backend agreement.
No optimization or GPU path is implemented here.

The real GPU upload remains **unmeasured**; separating it requires a bench with a
native renderer and a synchronization protocol. The pipeline still groups
preparation/framing/projection/clipping and Plotters calls: the side measurements
isolate the algorithms, not exactly their share in each frame. The
conclusion concerns a few long curves; it does not settle the case
of hundreds of small series, nor the cost of Typst, of mixed columns or
of high-resolution rendering.

### Fonts

Three embedded text + Math pairs (Libertinus, Fira, Noto — SIL OFL 1.1,
provenance in `vendor/fonts/README.md`), or a system font by its name
with fallback on Noto.

`text::font_definitions` stays the single source of measurement: this module gives it
a **parameter** (`FontChoice`) instead of freezing it. Hence a single entry
point `fonts::resolve`, and not two functions "for the screen" / "for
the export" — a caller that changed font on one side only would reproduce
exactly the defect `text.rs` exists to prevent.

## `stokesforge-render-egui` — the screen

`GraphView` is what the UI really holds: a cache, plus the link between a
session `Snapshot` and a `Graph` node.

### The backend collects, it does not paint

`EguiBackend` holds no `Painter`: it writes into a `&mut Vec<Shape>`
that is lent to it. egui is in immediate mode — it repaints at every mouse
movement. A backend that drew into a `Painter` would replay the whole
Plotters pipeline at **every frame**, even a still one. With a collector, the
result is cached and a frame without change costs one
`painter.extend(...)`.

### The cache key has exactly four terms

`(revision, rect, pixels_per_point, view_generation)`

- **`revision`** — the counter of `stokesforge-session`; the renderer becomes
  one more client of the same protocol, there was nothing to invent;
- **`rect`** — the logical rectangle passed to the pipeline; a direct call can
  change its size, whereas the application keeps the figure's own size
  (`figure_layout`, below) and transforms the shapes on output to resize the
  window without re-laying out the axes and texts. A new cell size is a new
  revision, hence a miss; a window resize is a hit;
- **`pixels_per_point`** — the exception that `epaint` itself documents:
  `Shape::Text` depends on it and must be rebuilt when it changes;
- **`view_generation`** — the counter of `ViewState`, which goes up at each
  committed zoom, pan or rubber-band rectangle. Zoom **changes the plotted geometry**,
  so it must invalidate. A counter rather than the bounds: an integer is enough to
  invalidate, the framing value does not have to pass through there.

- the **language of the figure texts** (#369) — the placeholder of a broken
  reference, the « … and N more » line and the name of a row series are not in
  the graph description: the application supplies a `FigureTexts`
  (`stokesforge-app-state/src/figure_texts.rs`) and `GraphView::set_figure_texts`
  folds a change of its language **tag** into `preview_generation` (same
  counter as the previews; never compare closures, never rebuild every frame).
  The plain `pipeline::draw`, `export_*` and `render_to_string` keep the English
  default (script, CLI); `*_with_texts` variants take the struct. Both backends
  must receive the same texts, since the legend text is measured once and
  places the axes (invariants 4 and 5). A broken series is drawn as literal
  text (never Typst markup), in every language (invariant 10).

What is deliberately **not** in it: *feathering* (invariant 6).

### Zoom and pan are not model mutations

A wheel zoom is an **ephemeral view framing**. Sending it through an
undoable command would make it a history entry just like a
rename — and nobody wants to undo a zoom with `Ctrl+Z`.

`ViewState` (`stokesforge_render::view`) therefore lives next to the model, on `GraphView`, and never touches the
`PlotSpec` stored in the `Tree`. The mechanism reuses what already existed:
`AxisSpec::has_explicit_range` makes an explicit framing win over the computed
data; `ViewState::apply` sets `auto_range = false` and the two bounds on
a **clone** of the spec, just before resolving. Only the trigger is
new.

**Overlaid layers (#167).** A `PanelSpec` carrying `overlay: Some(OverlaySpec
{ x_top, y_right })` covers the last ordinary layer of its cell (same
`row`/`column`). For each cell, the pipeline reserves the **union** of the tick
areas requested by its layers (`CellPlan`), so that all of them have the same
plot area (linked dimension); the overlaid layer inherits the area of the
covered layer (`BaseFrame`), and has neither title nor background. Its `bottom` axis
is its X axis, displayed **on top** with its own scale if `x_top` (otherwise the X scale
is that of the covered layer), likewise `left` / `y_right` for the Y axis on the right.
Its axes are plotted by Plotters' secondary axes; its series
have no secondary ordinate. `DrawnPanel::axis_areas.top` holds the top
area; `PlotComponent::Axis` keeps the side **of the spec** (`Bottom`/`Left`) even
when the axis is displayed on top or on the right. Test: `geometry.rs`,
`stokesforge-render-svg/tests/layers.rs`.

**Stacked axes (#168).** Several overlaid layers of the same cell can
display an axis on the same side: each has its **band** (area of the cell divided
by the number of layers displaying an axis there, `CellPlan::top_levels` /
`right_slots`, capped like any area at a third of the cell), placed outward in
**layer order** — the oldest against the plot area. The rank comes from this order (`ranks` in `draw_on`), never from a stored
level: a layer with no displayed axis, or a deleted one, leaves no empty band.
Since Plotters sticks the secondary axis to the plot area, each layer axis is
plotted in an "axis only" area (`area.shrink`) that takes the edges of the
plot area along the axis direction and is placed at its band.
`AxisArea::offset` is the distance from the band to the plot area, which
`hit::axis_component_at` counts.

**Axis line as a target (#385).** The edge of the plot area also picks the axis
(`Axis { panel, side }`, like the tick band): a strip inward from the edge,
`AXIS_LINE_TOLERANCE_POINTS` (4) **screen points** wide, so the caller passes
`4 / display_scale` figure pixels to `hit::component_at_with_tolerance`
(`GraphView::component_at` does; HiDPI and enlarged figures keep the same
strip). Clamped to a quarter of the plot area; none for a band with
`offset > 0` or an edge with no axis area (`size == 0`); nearest edge wins in a
corner. All axis gestures go through `component_at`, so they work there too.
The axis beats the curve on the strip (double-click: shape, then axis, then
curve; right-click looks for a curve only on the plot area); shapes, texts and
annotations keep their priority. Known limit: a data right axis
(`SeriesAxis::Right`) of the ordinary layer keeps its title at the outer edge of
the whole area. Test: `geometry.rs`, `layers.rs`, `hit.rs`.

**Cell handles (#394).** `hit::cell_edge_at(plot, x, y, tolerance)` returns the
`CellEdge { column, row }` under a pixel: the **outer** boundary of a cell
(`DrawnPanel::cell_origin` + `cell_size`, beyond the tick labels and the axis
title, never the axis line). Every cell's right boundary is the handle of its
grid column and its bottom boundary the handle of its row, so the boundary two
cells share belongs to the left column / the upper row and the figure's right and
bottom edges to the last column and row; left and top edges have none. The strip
is `CELL_EDGE_TOLERANCE_POINTS` (4) screen points either side
(`GraphView::cell_edge_at` passes `4 / display_scale`), bounded by
`CELL_EDGE_MAX_TOLERANCE` (half the 12 px margin) so that it never reaches an axis
line. The left and top edges of the figure are `hit::figure_edge_at`
(`FigureEdge { left, top }`, #405): they set Left and Top of the plot area
(`GraphEvent::FramePlaced`), never the width of a cell. `GraphView` pulls it on the preview pattern (`FramePreview`, applied to the
spec clone by `figure_layout` and the cache build, so the figure and the window
that follows it see the new layout live; model untouched; the distance is
measured in screen points over the display scale of the press because the figure
grows under the pointer); at release, one `GraphEvent::FrameResized`, then
`Action::SetFrameSizes` (one undo step). Never in the exports.

**Shared legend (#172).** The legend of an ordinary layer lists its
curves first, then, layer by layer in the order of `PlotSpec::panels`, those of the
overlaid layers that cover it (`overlays_of` in `draw_on`, prepared
by `draw_panel` with the own scales of each layer). The legend of the
overlaid layer stays hidden when it is created; showing it produces a
second one, limited to its curves. The added rows have no rank in the
ordinary layer: `DrawnPlot::legend_rows` does not designate them (no drag
or selection through them). Test: `geometry.rs`.

**Mirror axes (#176).** On an ordinary layer, `AxisSpec::mirror` of the top
(`top`) and the right (`right`, when no series is attached there) plots
an axis that takes the scale, bounds and ticks of the bottom and
left, **without a title**, through Plotters' secondary axes. An overlaid
layer of the cell that occupies this side wins (`CellPlan::mirror_top` /
`mirror_right`). `AxisSpec::tick_labels` false keeps the line and the tick marks without
the numbers (empty formatter); the tick area of a mirror without labels
is only `BARE_LABEL_AREA`. Test: `stokesforge-render-svg/tests/mirrors.rs`.

**Tick marks and tick direction (#344).** `AxisSpec::ticks` (plotted tick marks) and
`AxisSpec::tick_direction` (`TickDirection`: `Out` default, `In`, `Both`)
apply to the four sides of an ordinary layer (not to the axes of an overlaid
layer). The default (`ticks` true, `Out`) stays **entirely with Plotters**
(`custom_ticks` false): existing figures are pixel-identical. Otherwise:
Plotters plots tick marks only with the axis line and places the numbers at twice
their length (`tick_size < 0` even moves the line); the axis therefore goes through
two mesh passes (`mesh_passes`): the numbers alone (line of zero alpha,
original gap), then the line alone (tick marks of zero length, no tick
requested). The pipeline then plots the tick marks itself
(`draw_ticks`, `PathElement`, hence identical in egui and SVG) at the ticks
of `scale::linear_key_points`, projected by the primary frame (bottom, left)
or secondary frame (top, right: rounding there sometimes differs by a pixel). Length =
that of Plotters (`tick_length`, 5 px at most). In a "home-made" mesh, a
neighboring axis left at default has its `Out` tick marks plotted by the pipeline too, at the
same pixel. **Minor ticks (#344).** `AxisSpec::minor_ticks` (**true** for a new axis;
an old file that lacks the field reads back false; independent of `ticks`:
they can be plotted alone) plots tick marks of half the length of the
major ones (`draw_minor_ticks`, rounded up), in the same direction, even
when the major tick marks stay with Plotters (`custom_ticks` false). Their number between two
major ones is `AxisSpec::minor_tick_count` (`None` = automatic; no effect on a
logarithmic scale). `AxisSpec::label_every` writes only one number in n, the
first always (`TickFormat::with_label_every`, formatter per axis: the top has
its own). Positions
by `scale::minor_key_points`, in values then projected like the major
ticks: linear, the major step cut in 5 (in 4 for mantissa 2:
0.2 → 0.05), extended to the bounds; logarithmic, the multiples 2 to 9 of
each decade, or the interleaved decades when the step exceeds a decade;
none under a decade of step. Never on a major tick; none
on the axes of an overlaid layer. Plotters' light grid is not
aligned with them. Tests: `scale.rs`, `mirrors.rs`, `agreement.rs`.

**"Advanced" settings for tick marks (#344).** `AxisSpec::tick_length` (major
tick marks), `minor_tick_length` (minor ones), `tick_width` and `tick_mark_color`
(`None` / empty = Auto). Units: figure pixels for lengths, points
for the thickness (rounded to the integer by Plotters). Auto: major tick marks
keep Plotters' length (`tick_length`, 5 px at most), minor ones are
half of **the effective major tick mark** (major at 10 → minor at 5), 1 pt, the
scale color (`tick_color`, distinct from the tick mark color: line
and numbers keep theirs). The style is set **in `draw_marks` only**
(`MarkStyle`, invariant 5); a non-finite length reads as Auto, a negative
length is 0 (no tick mark) and `MAX_TICK_LENGTH` (50 px) bounds it, the thickness
goes through `paint::stroke_width` (1 to 64): the raw value stays in the file,
rendering bounds it (invariant 10). A setting moves the axis to home-made tick marks
(`custom_ticks`), like a direction other than "Outward"; the numbers
then keep their original gap, so a long tick mark (outward) can
touch them. Tests: `mirrors.rs` (length, Auto minor, color, thickness,
out-of-bounds values), `agreement.rs` (screen = export; the color is invisible there,
it is guarded by `mirrors.rs`).

**Axis direction.** `axis_range` always reads "one or the other" (`inverted`
or `range_min > range_max`), without adding them. `AxisSpec::orient` (crate
`stokesforge-plot`) brings an axis to its canonical form without changing the rendering:
`inverted` true as soon as the **used** bounds (non-Auto) are decreasing,
and an inverted axis puts its remembered bounds back in decreasing order; test
`an_oriented_axis_is_drawn_like_the_original_axis` (`geometry.rs`).

Tests: `mirrors.rs` (SVG), `agreement.rs`
(`the_custom_ticks_agree_and_change_the_figure`).

The own linear scales of an overlaid layer are **joined** to those of the
covered layer (#174): without a hand-fixed extent, `scale::aligned_range`
chooses a round step and a position such that the interval encloses the data
of the layer and its ticks fall at the same fractions of the axis as
those of the covered layer (the top 0 facing the bottom 0); each candidate
is checked against the ticks Plotters will really put down. With no solution,
a logarithmic scale or an explicit extent (zoom included): independent
framing. Linear ticks are written without decimals when they
are all integers, with the same number of decimals otherwise
(`scale::TickFormat`, #175); all axes go through a formatter.

**Tick count follows the axis length (#397).** `scale::tick_count` is the **only**
source of the number of ticks asked of Plotters. `draw_panel` computes it **once
per axis** (x, y, y2) from the plot area's pixel length (a throwaway
`build_cartesian_2d` on the same builder, before any range is known) and hands it to
everything that places or writes a tick: the labels of every mesh (bottom, left,
secondary, mirrors, overlay axes), the home-made tick marks, the minor ticks
(`minor_key_points`), the decimals and the "Label every" origin
(`TickFormat::new` / `with_label_every`), `aligned_range` (the covered layer's
count, kept in `BaseFrame`) and the analysis preview (`axis_ticks`). A caller that
rebuilt its own count would put the marks and the numbers at different places:
there is no count-less entry point left (`max_label_count` is only the upper bound).
The rule: one tick per `TICK_SPACING_X` (29 px, horizontal) or `TICK_SPACING_Y`
(20 px, vertical), at most `max_label_count` (11, or one per decade on a log
axis); then, while two neighbouring labels, **measured through the backend**
(`estimate_text_size`: width on a horizontal axis, height on a vertical one) and
kept `MIN_LABEL_GAP` (6 px) apart, would touch, one tick fewer (never below 2).
The spacings are calibrated so that a default one-cell graph (frame
392 x 284 px) keeps its 11 ticks on both axes: raising one changes every
default figure. "Label every" applies on top, and the safety net ignores it
(it measures all the labels). An overlaid layer on a cell keeps the covered axis'
count for the axis it shares. Tests: `scale::tests`, `stokesforge-render-svg/tests/tick_density.rs`,
`the_two_backends_agree_on_the_ticks_of_a_narrow_cell`. The corners where two plotted axes
meet are filled (1 px L-shaped segment per corner, `draw_panel`, test
`the_axes_meet_at_the_corners`).

A layer is identified by its **index in `PlotSpec::panels`**
(`DrawnPanel::index`), **not by its rank in `DrawnPlot::panels`**: the latter
omits the panels too small to be plotted (`MIN_CELL`), which shifts its
ranks. `(row, column)` serves only for the grid: several layers can
overlay in the same cell (#167), so they do not identify them.

The pan is the same for all tools (#318): Ctrl + drag or middle click +
drag, a single scale if the gesture starts from an axis; it is decided at
press (`NavDrag`, `decide_nav`) and takes precedence over the drags of objects,
zoom rectangle and plotting. Feathering is turned off only during a
real gesture (pan, rectangle, moved or plotted object), not for a drag without effect.

Wired gestures: Ctrl+wheel (zoom; the wheel alone does nothing), double-click
(reset), plus crosshair and point designation (`PointHit`, radius
`HIT_RADIUS`). The main drag depends on the view's **tool**
(`GraphTool`, issue #62, a view setting like the framing, never undoable):
`Pointer` (default) moves texts and objects (a plain drag no longer
scrolls the view), `ZoomBox` plots the rubber-band zoom rectangle, `Text` puts down a text on click
(`GraphEvent::PlaceText`, then back to the pointer). The right click is no longer a
zoom gesture: it opens the context menu.

### The anti-aliasing lever

*Feathering* — the one-pixel fringe that `epaint` adds to each stroke — weighs
**78 % of the tessellation cost**: 22.1 ms per frame with it, **4.88 ms without**, on
the same graph of 300,000 points. It is the most profitable lever of the work package,
and it was not in the plan before the measurement.

It is turned off when the drag starts and restored on release: aliasing
is not visible on a moving image, and it is exactly during the
movement that the 22.1 ms hurt. So it is not a setting exposed to the
user, it is a **state of the rendering**.

`stokesforge-render-egui::feathering` is the only living exception to invariant 5,
and it is not really one: `TessellationOptions` is a setting of the
egui stack, downstream of the collection of `Shape`s, and an SVG triangulates nothing. There
is nothing to put on the other side.

It is a free function taking the `Context` — and not a method of
`GraphView` — because `TessellationOptions` applies to the **whole window**:
two views that each set it would set two contradictory values.

## `stokesforge-render-svg` — the export

> « L'export est au `PlotSpec` ce que la persistance était au modèle : la seule
> chose qui prouve que la couche neutre est réellement neutre. »

That is why it is written **at the same time** as the screen backend, not after.

This crate knows neither `egui`, nor `eframe`, nor `winit` (`tests/no_ui_dependency.rs`),
and yet measures text exactly like the screen — the measurement comes
from `epaint`, pulled in by `stokesforge-render`. Practical consequence: **rendering is tested
without a graphics server**, hence in continuous integration.

`MeasuredSvgBackend` is a **wrapper** around `plotters-svg::SVGBackend`,
not a rewritten backend: ~800 lines maintained upstream already do everything
needed; they lack only `estimate_text_size`, and that is precisely the one that
matters.

PDF and PNG are **not backends**: they are conversions of the
`String` that `render_to_string` already returns — `usvg` + `svg2pdf` for one,
`usvg` + `resvg` for the other. The PNG takes a multiplicative scale factor,
not a size in pixels: the document stays vector until
rasterization.

A decision not announced by the plans, taken while writing the PDF: **load
the system fonts at conversion**. Without `fontdb`, `usvg` reports no
error and **silently drops all the text** — measured: 599 `Tj` operators
become zero. An export without a single title or axis label is not a
minor fidelity defect, it is a mute graph. Fallback on the embedded Noto safety net
if no system font fits; the PNG reuses the same
policy as is.

**Export from the application** (issue #34) goes through `export::export_to_file`
(`ExportFormat` read from the extension: `.svg`, `.pdf`, `.png` at a given
enlargement), **always with Typst**: `render_to_string_with_typst`, then
`document_to_pdf` or `document_to_png` on this SVG. The application passes it the
spec framed by the view (`GraphView::framed_spec`): the current zoom is exported,
without touching the model. `export::export_bytes_many` returns several formats of the
same figure while composing it only once (the copy to the clipboard,
#149, asks for three); each conversion fails alone. The size is `render_svg::figure_size(spec, …)` (#394), the one at which the screen lays
out the figure (see "Figure size and cells"). The display white of a transparent background stays on screen.
The inlined Typst fragment redeclares `xmlns:xlink` (its glyphs are
`<use xlink:href>`): without it `usvg` refused the document, hence any PDF or PNG
of a graph with Typst text (`a_typst_graph_exports_to_svg_pdf_and_png`).

## Typst (R8)

**Every** graph or panel title, every axis title and every legend entry goes through Typst, with or without `$`; tick labels never (issue #32).
Two text engines side by side did not align. The role is decided by
the pipeline, in the style: `formula::rich_style` sets the reserved family
`stokesforge-typst`, and the two backends branch on `formula::is_rich`, for
measurement as for drawing. No field is added to `stokesforge_plot`: **no
migration of the `.stf` format**. The text is therefore compiled as **Typst
markup**: `$…$` for math mode, `*gras*`, `\$` for a dollar.

So that neighboring labels align, `compile` puts before the text an invisible
**strut** of zero width, bounded from the font's ascent to its descent: ordinary text, subscripts and simple superscripts all have the same box and
the same baseline (19.07 pt high at 14 pt for Noto); a taller formula (fraction,
sum) enlarges the box without being cut. A mixed legend therefore has
rows of the same height.

**Free texts** (issue #35): plotted last, on top of the legend,
centered (`HPos`/`VPos::Center`) on their data point projected like a
series (`scale::forward`, same secondary axis); a point ≤ 0 on a log axis
is not plotted, but a point outside the framing is
(#163, lifting of the 2026-10-01 decision): the text can be put in the
margins, title band or tick area, on screen as in the export. A free angle does not
go through Plotters (`FontTransform`: quarter turns only): it travels
in the style (`formula::rotated_family`, family `stokesforge-typst@<degrés>`) and
`formula::source` wraps the text in `#stokesforge-rotate` (Typst
`rotate(reflow: true)`, the strut inside the rotation), whose page matches the
rotated box — the centered anchor stays the center of the text. `DrawnPanel::annotations`
reports center, size of the upright text and rotation: `hit` designates the **rotated**
rectangle (`PlotComponent::Annotation`, priority). On the screen side,
`GraphView` holds the preview of an edit or a drag
(`set_annotation_preview`, applied to the clone like the zoom, counter added to
`view_generation`), the selection and the `GraphEvent` signals (double-click,
text released); the validated preview stays displayed until the model changes
(`hold_preview`). The editor's input field (`annotation_field`, issue
#37) is put on the center and size of this rendered text, never rotated: a free
`DragValue` angle (fractional, tiny, negative) remains a valid Typst
duration (`formula::source`, issue #36 — the assumption of a `1e-7deg` format
was wrong: Rust's decimal format has no exponent). Without Typst (witness, fallback), the text is plain and not rotated.

**Graphic objects** (issue #76): rectangle, ellipse, line, arrow
(`stokesforge_render::shapes`), plotted by the shared pipeline between the legend and
the free texts (step "6 bis"), hence identical on screen and in the export.
The two points are projected **without** a framing membership test
(`Ranges::forward`) then clipped in pixels: stroke by `clip_polyline`
(closed outline), fill by Sutherland-Hodgman (`clip_polygon`), arrowhead
omitted if it is outside the frame. The ellipse is a convex polygon whose number of sides follows the radius in pixels (`ellipse_segments`: chord less than 0.1 px from the arc, #91).
The outline (rectangle, ellipse, line, arrow shaft) is plotted **at sub-pixel precision**: Plotters accepts only integer coordinates, which aliased the circle whatever the number of sides. Convention read by both backends (`stokesforge_render::subpixel`): fixed points ×16 and a flag in the thickness; the screen decodes in `draw_path`, the export rewrites the marked `<polyline>`s (`stokesforge-render-svg::subpixel::resolve`, called by each export entry point). `SubpixelPath` goes through a `CoordMapper` without `Rect::truncate`, which would crush the fixed points. A backend that ignores the convention plots these paths sixteen times too big.
`DrawnPanel::shapes` (`DrawnShape`, unclipped pixels) serves `hit`:
`PlotComponent::Shape`, designated **on its stroke** (or inside it if filled)
in the plot area, under the free texts. On the screen side, `GraphView`
(`shape_tools.rs`) holds the preview (`set_shape_preview`, same mechanism as
texts), the selection, the eight handles of a rectangle or an ellipse (two
ends for a line), the plotting tools (`GraphTool::Rectangle`…) and
the single signal `GraphEvent::ShapeChanged` on release. A click inserts a
shape of default size in figure pixels; Shift constrains the
resizing to the ratio of the projected dimensions at the start of the gesture,
including on logarithmic axes (#153, `shape_tools::resized_points`).
The preview is converted to data coordinates before validation:
the export finds the same geometry through the shared pipeline. A move on
linear axes is done **in data** (the plotted points are rounded to the
pixel).

**Movable titles and legend** (issue #53): graph title, panel title,
axis titles and legend frame carry an optional `TextAnchor`
(`stokesforge-plot::text_anchor`, center in panel **data**, not bounded to the
framing: a title lives outside the plot area). Without an anchor, Plotters puts the
text down (title band, tick area, legend corner); with an
anchor, the pipeline plots it itself, centered on the projected point
(`Projection::pixel`, without requiring it to be inside the framing), on top of the
series — **both backends, hence the SVG/PDF/PNG export**. **The layout does not
change**: a moved title keeps its band, empty (`reserve_title_band`,
same height as `DrawingArea::titled`, locked by an equality test of the
plot areas), and a moved axis title keeps its tick area —
otherwise the plot area would change on release and the title, anchored to the data,
would jump. A non-finite anchor is ignored (`TextAnchor::is_finite`); if the
projection fails (≤ 0 on a log axis), the text falls back to the center of the plot
area. `DrawnPanel::texts`, `DrawnPlot::title` and `DrawnPanel::legend_rows`
report center, size and rotation of each text — **approximate** for a
text at its default place (Plotters puts it down), exact for an anchored text —
and `hit::component_at` returns `PlotComponent::Text(TextTarget)` /
`LegendEntry`, before the plot area that they cover. On the screen side,
`GraphView::handle_text_drag` moves the center of the text with the pointer
(preview `set_text_preview`, same pattern as free texts) and the
double-click emits `GraphEvent::EditText` / `EditLegendEntry`. On a graphic object, a curve or an axis (#113), it
emits `OpenShapeProperties` / `OpenSeriesProperties` / `OpenAxisProperties` — the application
(`app::graph_window::annotation_interactions`) translates axes and curves into
`Action::OpenObjectProperties`, like the
double-click of a row of the Object Manager; elsewhere, it resets the zoom.
For an axis, the application reads `DrawnPanel::index` in the designated `DrawnPanel`
and opens the properties of the matching layer, even if other panels
were omitted from the rendering. The context menu keeps this address at the click.

**Automatically taken text** (issue #33): a series name left empty
takes the long name or the column name, often imported from an instrument
(`Abs_280 #1`). Read as markup, `_` would open italics and `#` code.
The legend therefore gives it `formula::literal_style` (family
`stokesforge-typst-literal`), like the « … and N more » line: the Typst branch
of the backends passes it through `formula::source`, which wraps it in a Typst string
`#"…"` (`formula::escape`) — composed by Typst, displayed as is, without
typographic quotes or `...` turned into an ellipsis. The path
without Typst (test witness, fallback) displays the original text, never
its escaped form. A **typed** name stays intended markup.

The split follows the same rule as the rest:

| Where | What |
|---|---|
| `stokesforge-render/formula.rs` | the minimal Typst `World`, the compilation, the cache that amortizes it between two frames |
| `stokesforge-render-egui/typst_screen.rs` | `typst-render` → RGBA → texture → `Shape::Mesh` |
| `stokesforge-render-svg/typst_export.rs` | witnesses, `splice`, `SvgTextMode`; `typst-svg` fragments (Outlines mode) |
| `stokesforge-render-svg/typst_text.rs` | `<text>` emitter of a Typst `Frame` (Text mode, #162) |

On the export side, the mechanism deserves to be known before touching it: `SVGBackend`
writes directly into its target `String` and its field is private, so
it is impossible to inject XML at drawing time. The workaround is a
**witness rectangle** — a unique color per witness (red fixed to an
improbable value, green and blue encoding a counter), put in place of the text,
then substituted by `splice` once the whole document is written, after
rechecking the position and the size.

**Text or outlines** (#162, decision of 2026-10-07; it closes open point
no. 6 of `architecture-choisie.md`). `typst-svg` always writes its glyphs as
outlines; to keep a figure editable (Inkscape, Illustrator), the
`SvgTextMode::Text` mode — **the default** — replaces its fragment with that of
`typst_text::frame_to_svg`, a home-made emitter that walks the compiled `Frame`:
one `<text>` per run, one character per glyph with the position (`x`/`y`) that
Typst computed, `font-family` naming the family followed by a generic
family (`'Noto Sans', sans-serif`), never an `@font-face` nor a subset —
the reader must have the font, otherwise their software substitutes. A glyph that the
`cmap` table does not link exactly to its character (stretched delimiter, math
variant, ligature) goes back **to outline, that glyph alone**; shapes
(fraction bar, root) stay `<path>`s. The witnesses and `splice` are
unchanged: only the content of the fragment changes, so the geometry as little
as possible (checked by overlaying the two renderings). The **ticks** carry
the same family as the Typst texts (`MeasuredSvgBackend::with_font`; the
measurement stays that of `epaint` on the same font). `SvgTextMode::Outlines` keeps
the `typst-svg` behavior and also passes the ticks through Typst (escaped
literal text): no `<text>` in the document.

The option lives in `ExportFormat::Svg { text }` and applies **only to SVG**:
the PDF is derived from an SVG in Text mode (selectable text; `usvg` must therefore
find the embedded fonts, which `pdf::ensure_embedded_fallback` loads all
into the `fontdb`), the PNG from an SVG in outlines, independent of the installed
fonts. `export_bytes_many` composes one SVG per requested mode. On the
application side: "Graph › Export › SVG Text" (`Action::SetSvgTextMode`),
preference kept in `preferences.json` (`svg_text`), never in the `.stf` (also set in
Preferences › Export);
the copy to the clipboard (#149) always takes Text. `svg2pdf` does not
number its fonts reproducibly when the PDF carries text:
two PDFs of the same SVG have the same lengths but not the same bytes.

Typst is no longer a Cargo feature (issue #29): `typst`, `typst-layout`,
`typst-render` and `typst-svg` (all `=0.15.1`) are firm dependencies of the
three crates, always compiled, and `cargo test --workspace` exercises these
paths. What remains optional is the **backend's execution path**:
`collect_shapes` (screen) and `render_to_string` (SVG export, hence PDF and PNG which
rely on it) plot everything as `Shape::Text` / plain text;
`collect_shapes_with_typst`, `render_to_string_with_typst` and `with_typst()`
pass formulas through Typst. The plain-text path serves as the **witness** of the
tests. The application exports through `export::export_to_file`, hence with Typst
(issue #34); `render_to_string`, `render_to_pdf` and `render_to_png` remain the
raw path of the tests and of the `figure` example.

A rich text that does not compile never makes the rendering fail: the plain
text, `$…$` included, is displayed as is by `epaint`, measurement included
(invariant 10). It is the only case where a title or a legend entry is
not composed by Typst.

### Screen side: `GraphView` always goes through Typst

`GraphView` (hence `stokesforge-app`) always calls `collect_shapes_with_typst`. It
resolves the Typst fonts **once**, at its creation (`TypstScreen::with_fonts`);
if this resolution fails, everything stays plain text. It holds a
`FormulaCache<FormulaTexture>` between two rebuilds of the figure, bounded by
`FORMULA_CACHE_LIMIT` (256): beyond it, the cache is emptied **before** the next
rebuild. Textures are therefore recompiled only if the text
of a formula, its size, its color (issue #30) or the `pixels_per_point`
changes, not at every rebuild.

`TypstScreen::measure` goes through this same cache, on **the same entry** as
drawing: measuring then drawing a formula compiles it only once (before,
each measurement recompiled).

Two R8 defects, which stayed invisible as long as the application did not display
Typst, were seen at the first capture and fixed:

- **the mesh sheared the texture**: `typst_screen::mesh` replaces the
  vertices of `Mesh::add_rect_with_uv` and must follow **its** order
  (top-left, top-right, bottom-left, bottom-right), otherwise each
  formula is displayed as noise in a box that is nonetheless right — the
  bounding box does not see it, the test
  `every_mesh_vertex_carries_the_texture_of_its_corner` does;
- **the page cut off subscripts and descenders**: Typst bounds a line from
  the cap height to the baseline; `compile` sets
  `top-edge`/`bottom-edge: "bounds"` so that the page goes to the bottom of
  the ink (`a_subscript_or_descender_is_not_cut_off_by_the_page`). The height
  of a label therefore varies with its content; the alignment of baselines
  between neighboring labels belongs to issue #32.

A formula enlarged with the window (`paint_scaled`) stays sharp (issue #31):
see "Enlarging a graph without blurring the text" below.

## What to know before modifying

- **The only place in the work package where a bug is silent is text
  measurement.** Before touching it, read `stokesforge-render/src/text.rs` and invariant 4.
- The net is `stokesforge-render-egui/tests/agreement.rs`. If it changes, read its two
  families of **witnesses** first: without them, an equality test between two
  paths passes just as well because both are right as because both
  are wrong in the same way.
- The sample graphs live in `stokesforge-render::sample`, hence in a production
  crate, because the agreement test (in `render-egui`) and the visual
  example (in `render-svg`) must cover **the same** graph and the
  two crates cannot depend on each other. To look at it:
  `cargo run -p stokesforge-render-svg --example figure`.
- The tests: `render/tests/{measurement,geometry,no_ui_dependency}.rs`,
  `render-egui/tests/{agreement,shapes,cache_session,feathering,interaction,typst}.rs`,
  `render-svg/tests/{export,no_ui_dependency,pdf,typst}.rs`. On the Typst side:
  `cache_session.rs` (`graph_view_passe_un_titre_formule_par_typst`,
  `rebuilding_the_figure_does_not_recompile_an_unchanged_title`) locks that
  `GraphView` goes through Typst and does not recompile an unchanged title;
  `typst_screen.rs` (`measuring_then_drawing_shares_a_single_entry`,
  `every_mesh_vertex_carries_the_texture_of_its_corner`) locks the
  sharing of the cache entry between measurement and drawing and the vertex order;
  `formula.rs` (`a_subscript_or_descender_is_not_cut_off_by_the_page`) the
  page height; `render-egui/tests/typst.rs`
  (`the_texture_of_a_formula_takes_the_text_color`) and
  `render-svg/tests/colors.rs` (`a_typst_title_takes_the_automatic_ink`,
  `a_typst_title_follows_title_color`) the color, texture pixels and
  SVG fragment included; `render-egui/tests/typst.rs`
  (`a_title_without_dollar_is_measured_and_drawn_by_typst`,
  `screen_and_export_place_the_typst_texts_at_the_same_place`,
  `a_mixed_legend_has_lines_of_the_same_height`) and
  `formula.rs` (`simple_labels_all_have_the_same_height`) the routing
  by role and the strut (#32).

### Figure size, frames and bands (#394)

What a graph stores is the **plot area**, the *frame*: `PlotSpec::frame_widths`
(one per grid column) and `frame_heights` (one per grid row), in points (1 pt =
1 SVG unit = 1 PDF point = 1 PNG pixel at ×1). The margins, the axis bands and
the title bands are added **around** the frame, so adding or removing an axis or
the graph title grows or shrinks the *image*, never the frame.

- `PlotSpec::sanitized_frame_widths()` / `sanitized_frame_heights()` read them
  **sanitized**: a missing, non-finite or non-positive entry reads
  `FALLBACK_FRAME` (392 × 284 pt), each entry is clamped to
  `[MIN_FRAME_SIZE, MAX_FRAME_SIZE]` (36 to 2000 pt), a sum beyond
  `MAX_FIGURE_EXTENT` (20000 pt) is scaled down. `MIN_CELL` (48 × 36) stays as
  the omission threshold of *legacy* and fallback grids only: a stored frame
  plus its margins is never below it.
- **Empty vectors are legacy, for good** (invariant 12): 480 × 360 pt shared
  equally between the cells, axis bands and title band **inside**, the label
  areas capped at a third of the cell. That path is `main`'s algorithm, byte for
  byte (the goldens guard it). A grid beyond `MAX_CELLS` also reads it.
- **`pipeline::figure_layout(spec, probe)` is the one place** where frames,
  bands and page meet; `ExplicitGrid::build` does the arithmetic and
  `draw_on_with_texts` reads the same structure to cut the cells. Per cell, the
  bands are `MARGIN` plus what its plan asks (`CellPlan::gutters`: left
  `Y_LABEL_AREA`, right `right_wanted`, top `top_wanted` plus the measured panel
  title, bottom `X_LABEL_AREA`; **no cap**). The plan reads only the `PanelSpec`s
  (`cell_plans`), so no data tree is needed. **Frames are aligned**: column `j`
  takes the largest left and right bands of its cells, row `i` the largest top
  (title included) and bottom bands; outer width = `left + frame + right`. A cell
  that asks for less keeps the difference empty (`Padding`, applied after its
  panel title), an empty cell takes its column's and row's bands. Order: widths,
  figure width, graph and panel title bands measured at that width (the title
  box is a fraction of it), heights, figure height = title band + Σ(top + frame +
  bottom). Plotters receives the outer cell, so the plot area it carves equals
  the frame (±1 px of prefix-sum rounding). It returns `FigureLayout { size,
  title_band, column_breaks, row_breaks, column_gutters, row_gutters,
  frame_widths, frame_heights }`; breakpoints are **rounded prefix sums** of the
  outer sizes (never each rounded alone, or screen ≠ export). `FigureLayout::
  cell_rect` is the outer cell, `frame_rect` the frame, `size_with_frames` the
  size a draft would give. `probe` is any drawing area of the backend that will
  draw the figure: only its text measurement matters, Typst titles included
  (invariant 4). Overlay layers: union of the plans, as before.
- **Fixed figure size, Left and Top (#405).** `PlotSpec::fixed_figure`,
  `figure_left`, `figure_top` (`Placement` in `pipeline.rs`) only act on the
  explicit path, so the legacy algorithm and `drawn_legacy_frames` ignore them
  (goldens untouched). In `ExplicitGrid::build` the gutters are floored: in
  Fixed mode every column has left and right `>= MARGIN + Y_LABEL_AREA` (76) and
  every row top and bottom `>= MARGIN + X_LABEL_AREA` (64), the panel title band
  coming **on top** of the top floor and the graph title band above the rows
  (F2: no reserve for a title). Left (Top), when set, **replaces** the reserve of
  column 0 (row 0) in Fixed mode and is a plain floor in Grows mode; Top counts
  from the image top, title band included (`slot.0 >= Top - title_band`). A value
  below what the axes need is raised (`FigureLayout::min_left`, `min_top`).
  When the floors are not enough (a second right axis, a title, a small Left) the
  image grows anyway and `FigureLayout::fixed_overflow` is true; the frame keeps
  its size, and its distance to the right and bottom edge. Growing at the left
  or the top moves it inside the image, which is why the window follows the
  figure on that side while those edges are dragged. The title band is measured
  once (`ExplicitGrid::title_band`) and used by the layout and the drawing path.
  Hit test: `hit::figure_edge_at` (left and top edges of the figure, same
  bounded strip as `cell_edge_at`).
- Everyone reads it: `draw`, `GraphView::figure_layout` (screen, memoized by
  graph, revision, pixels per point and preview generation; published per graph
  by `graph_view_contents` for the menus, the size fields and the window), the
  exports through `render_svg::figure_layout` / `figure_size`, the figure copy,
  the `pyst` export, the PNG resolution labels and the "contain" ratio.
- `DrawnPanel::cell_origin` / `cell_size` publish the **outer** cell in backend
  pixels (an overlay shares its base layer's cell): the future drag handles sit
  on its boundary, after the tick numbers and the axis title. They are screen
  markers and never enter the pipeline. `pixel_origin` / `pixel_size` stay the
  frame.
- **A legacy graph is fixed before it is replaced (G7).**
  `pipeline::legacy_frames` computes the frames a legacy graph is drawn with (the
  largest per column and row when the cells of one column differ) and
  `render_svg::fix_legacy_frames(reference, target)` writes them into the new
  spec, the reference being the spec as it is *now*. The application
  (`AppState::execute`: `SetPlotSpec`, `EditPlotSpec`, `AddGraph`; Add Layer and
  Delete before they edit the grid) and `pyst` (`bridge::set_graph_spec`,
  `plot`) call it, so the edit moves the bands and never the plot areas; nothing
  is written at load, nothing is added to the history. A new graph is created
  legacy and fixed at once: its figure is exactly 480 × 360 pt, explicit
  (frame 392 × 284 pt without a title).

### Enlarging a graph without blurring the text

`GraphView::paint_scaled` lays the figure out at its own size (`GraphView::figure_layout`, #394) then scales it to
the window. epaint's `Shape::transform` stretches the mesh of a text already
rasterized at the logical size: enlarged, it becomes blurry (issue
#18). `stokesforge_render_egui::scaled` therefore re-lays out each `Shape::Text` at the
displayed size (font × scale) and keeps the result by
`(clé du cache, rectangle affiché)`. Positions stay those of the logical
layout: the text measurement (invariant 4) is not redone.

A Typst formula is a textured `Shape::Mesh`: its mesh is transformed
like any shape (same place, same box), but designates **another texture**,
the same formula rasterized at `pixels_per_point × échelle` by
`FormulaTexture::rerender` from the already compiled document (issue #31) — no
recompilation, no new measurement. These enlarged textures live in
`ScaledShapes`, outside the view's `FormulaCache` (they do not count in
`FORMULA_CACHE_LIMIT`), and are reused as long as the scale does not change. An
image never exceeds `max_texture_side`: beyond it, the resolution is
capped. Rasterizing directly at the displayed scale during collection was
ruled out: the displayed rectangle would have entered the main cache key, hence
the whole pipeline replayed at each step of a resize. Tests:
`scaled.rs` (`an_enlarged_formula_is_rasterized_at_the_displayed_size`,
`moving_without_enlarging_rasterizes_nothing`) and `cache_session.rs`
(`an_enlarged_graph_paints_its_formulas_at_the_displayed_size`).

The mesh of a formula is also **snapped to the physical pixel grid**
(`scaled::snap_to_pixels`, issue #86): top-left corner on a whole
pixel, box at the exact size of the texture. Placed or stretched at a fraction of a
pixel, the image was blended with its neighbors by linear filtering — blur
mostly visible on a small text (reduced figure), barely on a large one. The
shift stays under one pixel; the layout is not touched
(`a_reduced_formula_is_snapped_to_the_pixels`). Outside the `scaled` path
(`paint` with logical rectangle = displayed rectangle) the snapping is not
applied.

## Out-of-date series (#198)

The rendering knows nothing about the model: the core sets `ResolvedSeries.stale`
(`resolve_series`, cancelled by `resolve_spec` when `PlotSpec::stale_warning`
is false) and `series::Prepared.stale` carries it down to drawing. A marked
series is plotted like a broken series (reuses the `broken` parameter of
`paint`, grey stroke `STALE_FILL`/`STALE_EDGE`) and gets a warning triangle
made of primitives (`paint::stale_marker`, without text measurement) just above
its last point (`pipeline::stale_marker_at`, brought back into the plot area). Both
backends inherit it; the end-to-end test is in
`stokesforge-app/src/app/stale_export.rs`.

## Visibility and selection (#81–85)

The `SeriesSpec::visible` filter precedes preparation, autoscale and legend.
Hidden free texts, shapes and titles are omitted along with their designation geometry. `title_visible` applies to the title of the graph, the panel and
the axis, with or without an anchor; the content and font stay remembered. A
hidden graph or panel title no longer reserves its band. Axes
keep their tick area when only their title is hidden. This
rule lives in the shared pipeline, so it applies in SVG, PDF and PNG too.

**Error bars on every type** (#360): `series::prepare` builds the whiskers
(`Geometry::Curve` and `Geometry::Bars`, `whiskers`) from the resolved
`x_error` / `y_error` columns alone, whatever the `PlotType`: Line, Scatter,
Line + Symbol, Bar and Error Bars all carry them, and a series without an error
column carries none. A Histogram ignores them (its bars are counts, not
measured points). `pipeline::paint_whiskers` draws them for curves and bars;
the Y extent of the framing includes them. The `ErrorBar` type keeps its
meaning (line and markers); it no longer is the only one that shows whiskers.

`DrawnPanel::series_geometry` keeps the pixels of the series after decimation,
projection onto the left or right axis and clipping (line runs, whiskers,
markers, bars and free end of each bar). `series_selection` designates
them; `GraphView` paints the selection there without modifying the figure cache or
the model. Ranks stay those of the spec, even when a series is hidden.

A selected series is **not highlighted** (#98): its plot stays
intact, and small filled squares mark it, as in OriginLab —
`HANDLE_SIDE` (5 px side) every `HANDLE_SPACING` (50 px), measured **along
the plot** and **in display pixels**, hence independent of the data density,
of LTTB, of the zoom and of the window size
(`series_selection::along_path`, start and end of each run marked).
A cloud without a line gets its squares on a regular subset of its
markers, bars at the middle of their free end, value side
(`thinned`, same spacing); whiskers carry none. Color:
`pipeline::figure_ink`, the automatic ink of the figure background (black on
a light background, white on a dark one). The placement and drawing
(`paint_handles`, generic over `DrawingBackend`) live in
`stokesforge-render`; `GraphView` calls them on top of the cached figure,
with an `EguiBackend` set on the displayed rectangle. The handles are therefore
neither in the cache key (selecting rebuilds nothing) nor in the pipeline
(the SVG/PDF/PNG export does not see them): the selection is a view state,
like the zoom. Tests: `series_selection.rs` (spacing, end, display scale,
cloud and bars), `geometry.rs` (free end of a negative bar, whiskers
outside the runs, ink), `render-egui/tests/interaction.rs`
(`selected_series_carries_handles_without_changing_its_plot`). The
selection of a graphic object keeps its own frame and handles
(`shape_tools.rs`).
Object Manager uses `series_swatch`, which calls the shared legend swatch
(`legend::series_key`), without redrawing the style in the application.

`GraphView::select_annotation` / `select_text` complete the selection of
series and object from the tree, including for hidden contents; only one
kind is selected at a time. Revalidation discards the addresses
absent from the spec. The properties and the format bars reuse this
same view in a detached window.

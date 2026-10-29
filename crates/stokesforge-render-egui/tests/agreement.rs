//! **The agreement test** — task R4.5, widened by R5.6, and the completion
//! criterion of the whole rendering plan.
//!
//! > A column drawn by `stokesforge-render-egui`, the same figure exported to
//! > SVG by `stokesforge-render-svg`, **axes at the same place** — checked by
//! > a test, not by eye.
//!
//! # What is compared, and why not less
//!
//! Not "the two backends do not crash", not "the two render a similar
//! number of shapes": **all the primitives**, to the whole pixel,
//! as sorted multisets. R4 compared the segments — axes,
//! ticks, grid, curve. R5 adds what it introduced:
//! the **circles** and the **polygons** (markers), the **rectangles**
//! (bars, histograms, legend frame) and the **text**. A weaker comparison
//! would let through precisely the defect that this test exists
//! to catch — an offset of a few pixels due to a diverging text
//! measurement, which never raises an error (`architecture-choisie.md` §4.4).
//!
//! The text is compared by its **content** and not by its position: on the
//! SVG side, `<text x y dy text-anchor>` carries the raw anchor point that
//! Plotters gave; on the egui side, `Shape::Text` carries the top-left
//! corner of the galley, already shifted by the anchor. Both are correct and
//! incomparable — this is what `tests/shapes.rs` checks separately. The
//! *position* of the ticks, however, is compared: these are their
//! tick strokes, which are segments.
//!
//! # The witnesses, without which the test would prove nothing
//!
//! An equality test between two paths can pass because both are
//! right, or because both are wrong in the same way. Two families
//! of witnesses rule that out:
//!
//! - the R4 one: the same figure drawn with the **bare** `SVGBackend` — the one
//!   that falls back to the UTF-8 byte heuristic — must be placed
//!   **elsewhere**. Without it, a wrapper that mistakenly delegated
//!   `estimate_text_size` would pass for correct;
//! - the R5 one: for **each** feature that the phase adds, a
//!   variant of the realistic graph that changes it must produce a
//!   **different** figure. Without them, a comparison that did not look at
//!   the circles, or a stroke style that was not applied, would pass
//!   for correct too — and the agreement would be that of two equally
//!   amputated figures.
//!
//! This is the method of spike R0 0.1, which already carried a positive witness (a
//! deliberately different font family) to prove that the measured equality
//! was not an artifact of the harness.
//!
//! # Where the test lives, and why not in `stokesforge-render-svg`
//!
//! It needs both backends. Putting it on the SVG side would make `egui` a
//! development dependency of that crate, and `cargo tree -p
//! stokesforge-render-svg` would stop demonstrating what it demonstrates
//! (`stokesforge-render-svg/tests/no_ui_dependency.rs`). It therefore lives on
//! the egui side, which already has egui.

use egui::{Pos2, Rect, Shape};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project, Tree};
use stokesforge_plot::{
    AxisScale, LegendPosition, LineStyle, PanelSpec, PlotSpec, PlotType, SeriesAxis, SeriesSpec,
    SymbolShape,
};
use stokesforge_render::pipeline::{self, DrawnPlot};
use stokesforge_render::sample::{self, Sample};
use stokesforge_render::FigureTexts;
use stokesforge_render::TextMeasurer;
use stokesforge_render_egui::collect_shapes_with_texts;

/// Dimensions of the compared figure, in backend units.
const SIZE: (u32, u32) = (800, 600);
/// The realistic graph has five panels: it needs room.
const LARGE: (u32, u32) = (1280, 860);

// -- fixtures --

/// The minimal graph of R4 and the realistic graph of R5 both come from
/// `stokesforge_render::sample`, and not from here: the example that one looks at
/// (`cargo run -p stokesforge-render-svg --example figure`) must be exactly
/// the figure that this test compares. The full reason is in the header of
/// that module.
fn minimal() -> Sample {
    sample::minimal().expect("minimal graph")
}

fn realistic() -> Sample {
    sample::realistic().expect("realistic graph")
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("valid numeric column")
}

// -- extraction --

/// A point in whole backend coordinates.
type Point = (i32, i32);

/// What a figure has laid down, reduced to what the two backends describe
/// the same way.
///
/// The coordinates are rounded to the integer: the egui backend works in
/// `f32` and the SVG in decimal text; comparing otherwise would amount to
/// comparing two representations, not two geometries. Plotters itself
/// produces only integers (`BackendCoord = (i32, i32)`), so the rounding
/// loses nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Primitive {
    /// A line segment.
    Segment(Point, Point),
    /// A filled polygon (a triangular or diamond marker).
    Polygon(Vec<Point>),
    /// A rectangle — bar, legend frame, background.
    Rect(Point, Point),
    /// A disc, filled or not.
    Circle(Point, i32),
    /// A text, by its content only (see the header doc).
    Text(String),
}

/// The primitives of a figure rendered by the egui backend.
fn primitives_egui(shapes: &[Shape]) -> Vec<Primitive> {
    let mut out = Vec::new();
    let point = |p: Pos2| (p.x.round() as i32, p.y.round() as i32);
    for shape in shapes {
        match shape {
            Shape::LineSegment { points, .. } => {
                out.push(Primitive::Segment(point(points[0]), point(points[1])));
            }
            Shape::Path(path) if path.closed => {
                out.push(Primitive::Polygon(
                    path.points.iter().map(|p| point(*p)).collect(),
                ));
            }
            Shape::Path(path) => {
                for pair in path.points.windows(2) {
                    out.push(Primitive::Segment(point(pair[0]), point(pair[1])));
                }
            }
            Shape::Rect(rect) => {
                out.push(Primitive::Rect(point(rect.rect.min), point(rect.rect.max)));
            }
            Shape::Circle(circle) => {
                out.push(Primitive::Circle(
                    point(circle.center),
                    circle.radius.round() as i32,
                ));
            }
            Shape::Text(text) => {
                out.push(Primitive::Text(text.galley.job.text.clone()));
            }
            _ => {}
        }
    }
    out.sort();
    out
}

/// The primitives of an SVG document produced by `plotters-svg`.
///
/// Parsed by hand rather than with an XML parser: the output of
/// `plotters-svg` is a tiny and stable subset of the format (no
/// `defs`, no `id`, no `clipPath` — which is incidentally what makes the
/// Typst merge of R8 easy, `architecture-choisie.md` §5.2), and one more
/// dependency in a boundary test would be misplaced.
fn primitives_svg(document: &str) -> Vec<Primitive> {
    let mut out = Vec::new();

    for tag in document.split('<').skip(1) {
        if let Some(rest) = tag.strip_prefix("polyline") {
            for pair in points_of(rest).windows(2) {
                out.push(Primitive::Segment(pair[0], pair[1]));
            }
        } else if let Some(rest) = tag.strip_prefix("polygon") {
            out.push(Primitive::Polygon(points_of(rest)));
        } else if let Some(rest) = tag.strip_prefix("line") {
            let get = |name| attribute(rest, name).and_then(|value| round(&value));
            if let (Some(x1), Some(y1), Some(x2), Some(y2)) =
                (get("x1"), get("y1"), get("x2"), get("y2"))
            {
                out.push(Primitive::Segment((x1, y1), (x2, y2)));
            }
        } else if let Some(rest) = tag.strip_prefix("rect") {
            let get = |name| attribute(rest, name).and_then(|value| round(&value));
            if let (Some(x), Some(y), Some(w), Some(h)) =
                (get("x"), get("y"), get("width"), get("height"))
            {
                out.push(Primitive::Rect((x, y), (x + w, y + h)));
            }
        } else if let Some(rest) = tag.strip_prefix("circle") {
            let get = |name| attribute(rest, name).and_then(|value| round(&value));
            if let (Some(cx), Some(cy), Some(r)) = (get("cx"), get("cy"), get("r")) {
                out.push(Primitive::Circle((cx, cy), r));
            }
        } else if let Some(rest) = tag.strip_prefix("text") {
            if let Some(content) = rest.split_once('>').map(|(_, after)| after) {
                out.push(Primitive::Text(unescape(content.trim())));
            }
        }
    }
    out.sort();
    out
}

fn points_of(tag: &str) -> Vec<Point> {
    let Some(points) = attribute(tag, "points") else {
        return Vec::new();
    };
    points
        .split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some((round(x)?, round(y)?))
        })
        .collect()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = start + tag[start..].find('"')?;
    Some(tag[start..end].to_string())
}

fn round(value: &str) -> Option<i32> {
    value.trim().parse::<f64>().ok().map(|v| v.round() as i32)
}

/// The inverse of the escaping of `plotters-svg` (`svg.rs::escape_and_push`).
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// -- renderings --

fn render_egui(tree: &Tree, spec: &PlotSpec, size: (u32, u32)) -> (Vec<Primitive>, DrawnPlot) {
    render_egui_with_texts(tree, spec, size, &FigureTexts::default())
}

fn render_egui_with_texts(
    tree: &Tree,
    spec: &PlotSpec,
    size: (u32, u32),
    texts: &FigureTexts,
) -> (Vec<Primitive>, DrawnPlot) {
    let plot = resolve_spec(tree, spec);
    let measurer = TextMeasurer::default();
    let mut shapes = Vec::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(size.0 as f32, size.1 as f32));
    let drawn = collect_shapes_with_texts(&mut shapes, rect, &measurer, &plot, texts)
        .expect("egui drawing");
    (primitives_egui(&shapes), drawn)
}

fn render_svg(tree: &Tree, spec: &PlotSpec, size: (u32, u32)) -> (Vec<Primitive>, DrawnPlot) {
    render_svg_with_texts(tree, spec, size, &FigureTexts::default())
}

fn render_svg_with_texts(
    tree: &Tree,
    spec: &PlotSpec,
    size: (u32, u32),
    texts: &FigureTexts,
) -> (Vec<Primitive>, DrawnPlot) {
    let plot = resolve_spec(tree, spec);
    // Deliberately a **second** `TextMeasurer`, not the one of the egui rendering:
    // task R4.1 says that sharing the `FontDefinitions` is enough, and that is
    // precisely what this test must put to the proof.
    let measurer = TextMeasurer::default();
    let (document, drawn) = stokesforge_render_svg::render_to_string_with_geometry_and_texts(
        &plot, size, &measurer, texts,
    )
    .expect("SVG export");
    (primitives_svg(&document), drawn)
}

/// The same rendering, but with the **bare** `SVGBackend`: the R4 witness.
fn render_svg_without_override(tree: &Tree, spec: &PlotSpec, size: (u32, u32)) -> Vec<Primitive> {
    let plot = resolve_spec(tree, spec);
    let mut buffer = String::new();
    {
        let backend = stokesforge_render_svg::plain_backend(&mut buffer, size);
        pipeline::draw(backend, &plot).expect("bare SVG drawing");
    }
    primitives_svg(&buffer)
}

/// The frame of the figure: the extremes of the **segments**, which gives a
/// readable failure message where comparing two lists of 500 primitives would
/// give none.
///
/// The segments and only them: the figure background is a rectangle that
/// covers the whole area by construction, so including it would make this frame
/// equal to the requested dimensions whatever happens — and the §4.4 witness,
/// which relies on the shift of the plot area, would never bite
/// again.
fn frame_of(primitives: &[Primitive]) -> (i32, i32, i32, i32) {
    let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for primitive in primitives {
        let Primitive::Segment(a, b) = primitive else {
            continue;
        };
        for (x, y) in [a, b] {
            bounds.0 = bounds.0.min(*x);
            bounds.1 = bounds.1.min(*y);
            bounds.2 = bounds.2.max(*x);
            bounds.3 = bounds.3.max(*y);
        }
    }
    bounds
}

/// Compares the two backends on a given graph, and returns the geometry.
fn agree(
    tree: &Tree,
    spec: &PlotSpec,
    size: (u32, u32),
    what: &str,
) -> (Vec<Primitive>, DrawnPlot) {
    agree_with_texts(tree, spec, size, what, &FigureTexts::default())
}

/// [`agree`] with the same [`FigureTexts`] given to both backends.
fn agree_with_texts(
    tree: &Tree,
    spec: &PlotSpec,
    size: (u32, u32),
    what: &str,
    texts: &FigureTexts,
) -> (Vec<Primitive>, DrawnPlot) {
    let (egui_primitives, egui_drawn) = render_egui_with_texts(tree, spec, size, texts);
    let (svg_primitives, svg_drawn) = render_svg_with_texts(tree, spec, size, texts);

    assert!(
        !egui_primitives.is_empty(),
        "{what}: the egui backend produced no primitive, the test compares \
         nothing"
    );
    // The frame first, so that the failure is readable.
    assert_eq!(
        frame_of(&egui_primitives),
        frame_of(&svg_primitives),
        "{what}: the plot area is not at the same place — this is \
         exactly the silent defect of §4.4: a diverging text measurement \
         shifts the figure without raising an error"
    );
    // Then the full equality, primitive by primitive.
    assert_eq!(
        egui_primitives,
        svg_primitives,
        "{what}: {} primitives on the egui side, {} on the SVG side — the two backends \
         do not draw the same figure",
        egui_primitives.len(),
        svg_primitives.len()
    );
    // And the logical geometry, which cannot be deduced from the pixels.
    assert_eq!(egui_drawn, svg_drawn, "{what}: distinct logical geometries");
    (egui_primitives, egui_drawn)
}

// -- R4: the minimal graph --

#[test]
fn the_two_backends_place_the_axes_at_the_same_place() {
    let fixture = minimal();
    let (_, drawn) = agree(fixture.project.tree(), &fixture.spec, SIZE, "minimal graph");
    assert_eq!(drawn.drawn_series, 1);
    assert_eq!(drawn.broken_series, 0);
    assert_eq!(drawn.panels.len(), 1);
}

#[test]
fn the_agreement_holds_at_several_figure_sizes() {
    // A single size could come out right by chance — two margins that cancel
    // each other, a rounding that hides an offset.
    let fixture = minimal();
    for size in [(400_u32, 300_u32), (800, 600), (1280, 720), (640, 900)] {
        agree(
            fixture.project.tree(),
            &fixture.spec,
            size,
            &format!("minimal graph at {size:?}"),
        );
    }
}

#[test]
fn witness_without_the_estimate_text_size_override_the_axes_move() {
    let fixture = minimal();
    let (with, _) = render_svg(fixture.project.tree(), &fixture.spec, SIZE);
    let without = render_svg_without_override(fixture.project.tree(), &fixture.spec, SIZE);

    assert_ne!(
        frame_of(&with),
        frame_of(&without),
        "the bare SVGBackend places the figure exactly like ours: either \
         the wrapper mistakenly delegates estimate_text_size, or the witness \
         no longer bites — in both cases the agreement test no longer proves \
         anything (architecture §4.4)"
    );
}

#[test]
fn a_broken_reference_breaks_neither_the_rendering_nor_the_agreement() {
    // `architecture-choisie.md` §11.1: a broken reference is a
    // first-class state, never a crash nor a silently empty series.
    // The two backends must cope with it *the same way*.
    let mut fixture = minimal();
    let ghost = NodeId::from_raw(u64::MAX);
    if let Some(panel) = fixture.spec.panels.get_mut(0) {
        panel.series.push(SeriesSpec::new(ghost));
    }

    let (primitives, drawn) = agree(
        fixture.project.tree(),
        &fixture.spec,
        SIZE,
        "broken reference",
    );
    assert_eq!(drawn.broken_series, 1, "the broken reference is counted");
    assert_eq!(drawn.drawn_series, 1, "the other series is drawn anyway");
    // R5 goes further than R4, which was content to count: the broken
    // series **is visible**, in the legend, annotated.
    assert!(
        primitives.iter().any(|p| matches!(
            p,
            Primitive::Text(text) if text.contains(&FigureTexts::default().broken_reference)
        )),
        "the broken reference appears nowhere in the figure: it is \
         silenced, which §11.1 forbids"
    );
    assert_eq!(drawn.panels[0].legend_entries, 2);
}

/// #369: the texts the renderer draws by itself come from the caller. Both
/// backends given the same non-English texts agree, and draw them: a
/// legend of 26 entries (« … and N more ») with one broken reference.
#[test]
fn the_two_backends_agree_on_the_texts_they_are_given() {
    let texts = FigureTexts::new(
        "xx",
        "(xx-placeholder)",
        |name| format!("{name} xx-named"),
        |count| format!("xx-more {count}"),
        |number| format!("xx-row {number}"),
    );
    let mut fixture = minimal();
    let ghost = NodeId::from_raw(u64::MAX);
    let column = fixture.spec.panels[0].series[0].y;
    if let Some(panel) = fixture.spec.panels.get_mut(0) {
        panel.series.push(SeriesSpec::new(ghost));
        for _ in 0..25 {
            panel.series.push(SeriesSpec::new(column));
        }
    }

    let (primitives, drawn) = agree_with_texts(
        fixture.project.tree(),
        &fixture.spec,
        SIZE,
        "custom texts",
        &texts,
    );
    let has = |needle: &str| {
        primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text(text) if text.contains(needle)))
    };
    assert_eq!(drawn.broken_series, 1);
    assert!(has("xx-more 3"), "the omitted-entries line is not drawn");
    assert!(!has("and 3 more"), "the English default leaked");
}

// -- R5: the realistic graph --

#[test]
fn the_realistic_graph_really_exercises_what_it_claims_to() {
    // An agreement test on a graph that exercised nothing would pass by
    // itself. This one first checks that the reference figure really
    // contains each of the features of the phase.
    let fixture = realistic();
    let (_, drawn) = render_egui(fixture.project.tree(), &fixture.spec, LARGE);

    assert_eq!(drawn.panels.len(), 5, "five panels, in a 2 × 3 grid");
    assert_eq!(drawn.omitted_panels, 0, "no panel must be omitted");
    assert_eq!(drawn.broken_series, 1, "one broken reference, on purpose");
    assert_eq!(
        drawn.drawn_series, 7,
        "seven series drawn: two spectra, two in kinetics, \
         the calibration, the histogram and the bars"
    );

    let calibration = drawn
        .panels
        .iter()
        .find(|panel| panel.y_scale == AxisScale::Log)
        .expect("a panel with a logarithmic ordinate");
    assert!(calibration.y_range.0 > 0.0, "{:?}", calibration.y_range);

    let kinetics = drawn
        .panels
        .iter()
        .find(|panel| panel.y2_range.is_some())
        .expect("a panel with a secondary axis");
    assert_ne!(
        kinetics.y2_range.map(|r| r.0),
        Some(kinetics.y_range.0),
        "the right axis has the same framing as the left one: it is not \
         really secondary"
    );

    assert!(
        drawn.panels.iter().all(|panel| panel.legend_entries > 0),
        "each panel must carry a legend"
    );
}

#[test]
fn the_agreement_holds_on_the_realistic_graph() {
    let fixture = realistic();
    for size in [LARGE, (900, 600), (1600, 1000)] {
        agree(
            fixture.project.tree(),
            &fixture.spec,
            size,
            &format!("realistic graph at {size:?}"),
        );
    }
}

#[test]
fn witness_the_bare_svgbackend_also_moves_the_realistic_graph() {
    let fixture = realistic();
    let (with, _) = render_svg(fixture.project.tree(), &fixture.spec, LARGE);
    let without = render_svg_without_override(fixture.project.tree(), &fixture.spec, LARGE);
    assert_ne!(frame_of(&with), frame_of(&without));
}

/// A feature of R5, and what is needed to unplug it: the name serves the
/// failure message, the function modifies the realistic graph.
type Variant = (&'static str, fn(&mut PlotSpec));

/// **The R5 witness.** For each feature added by the phase, a
/// variant of the realistic graph that changes it must produce a different
/// figure — and the two backends must stay in agreement *on the
/// variant too*.
///
/// Without this test, a feature that was not really applied
/// (an ignored stroke style, a log scale without effect) would leave the
/// agreement test green: two identically amputated figures agree
/// perfectly.
#[test]
fn witness_each_r5_feature_changes_the_figure() {
    let fixture = realistic();
    let (reference, _) = agree(
        fixture.project.tree(),
        &fixture.spec,
        LARGE,
        "realistic graph (reference)",
    );

    let variants: Vec<Variant> = vec![
        ("stroke style", |spec| {
            if let Some(panel) = spec.panels.get_mut(0) {
                panel.series[1].line.style = LineStyle::Solid;
            }
        }),
        ("marker shape", |spec| {
            if let Some(panel) = spec.panels.get_mut(1) {
                panel.series[1].symbol.shape = SymbolShape::Cross;
            }
        }),
        ("logarithmic scale", |spec| {
            if let Some(panel) = spec.panels.get_mut(2) {
                panel.left.scale = AxisScale::Linear;
            }
        }),
        ("secondary axis", |spec| {
            if let Some(panel) = spec.panels.get_mut(1) {
                panel.series[1].axis = SeriesAxis::Left;
            }
        }),
        ("legend", |spec| {
            for index in 0..5 {
                if let Some(panel) = spec.panels.get_mut(index) {
                    panel.legend.visible = false;
                }
            }
        }),
        ("legend position", |spec| {
            if let Some(panel) = spec.panels.get_mut(0) {
                panel.legend.position = LegendPosition::BottomLeft;
            }
        }),
        ("plot type", |spec| {
            if let Some(panel) = spec.panels.get_mut(4) {
                panel.series[0].plot_type = PlotType::Line;
            }
        }),
        ("histogram bin count", |spec| {
            if let Some(panel) = spec.panels.get_mut(3) {
                panel.series[0].histogram_bins =
                    stokesforge_plot::BinCount::new(7).expect("7 bins");
            }
        }),
        ("bar width", |spec| {
            // Batches are 1 apart: the deduced width is 0.8.
            if let Some(panel) = spec.panels.get_mut(4) {
                panel.series[0].bar_width = Some(0.2);
            }
        }),
        ("free text", |spec| {
            // At the top of spectrum A (λ = 520 nm, absorbance ≈ 0.9), within the
            // framing of the spectra panel.
            if let Some(panel) = spec.panels.get_mut(0) {
                panel
                    .annotations
                    .push(stokesforge_plot::AnnotationSpec::new("Peak", 520.0, 0.5));
            }
        }),
        ("panel grid", |spec| {
            if let Some(panel) = spec.panels.get_mut(4) {
                panel.column = 2;
            }
        }),
    ];

    for (what, modify) in variants {
        let mut variant = realistic();
        modify(&mut variant.spec);
        let (primitives, _) = agree(
            variant.project.tree(),
            &variant.spec,
            LARGE,
            &format!("variant « {what} »"),
        );
        assert_ne!(
            primitives, reference,
            "changing « {what} » changed nothing in the figure: the \
             feature is not applied, or the test does not look at it"
        );
    }
}

// -- R5.5: decimation --

#[test]
fn decimation_applies_and_the_two_backends_decimate_alike() {
    // LTTB depends on the **width in pixels** of the plot area: this is
    // the only place of the pipeline where the geometry depends on the size of
    // the figure other than by a scaling. If the two backends
    // did not get the same width, they would not decimate the same
    // way — and nothing else would tell.
    let mut project = Project::new("Acquisition").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let n = 40_000usize;
    let xs: Vec<f64> = (0..n).map(|i| i as f64 * 1e-4).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| (x * 37.0).sin() + 0.2 * (x * 311.0).sin())
        .collect();
    let x = project
        .add_column(sheet, None, Some("t"), numeric(&xs))
        .expect("t");
    let y = project
        .add_column(sheet, None, Some("signal"), numeric(&ys))
        .expect("signal");

    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(SeriesSpec::new_xy(x, y));
    let spec = PlotSpec::empty("Acquisition").with_panels(panel);

    let (_, drawn) = agree(project.tree(), &spec, SIZE, "long series");
    assert!(
        drawn.decimated_points > 0,
        "40,000 points over 800 px should be decimated"
    );
    assert!(
        drawn.decimated_points > n / 2,
        "only {} points removed out of {n}: the threshold is not the one \
         we believe",
        drawn.decimated_points
    );

    // The framing, for its part, stays that of the whole data: it is computed
    // before the decimation, and LTTB only picks real points — so
    // the two would stay consistent even if the order changed one day.
    let min = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let max = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let margin = (max - min) * 0.05;
    let (low, high) = drawn.y_range();
    assert!(
        (low - (min - margin)).abs() < 1e-9 && (high - (max + margin)).abs() < 1e-9,
        "the framing {:?} no longer matches the extent of the 40,000 points \
         ({min}..{max}): the decimation lost an extremum",
        drawn.y_range()
    );
}

#[test]
fn hidden_objects_and_series_agreement_and_visible_witness() {
    use stokesforge_plot::{AnnotationSpec, ShapeKind, ShapeSpec};
    let fixture = minimal();
    let tree = fixture.project.tree();
    let mut visible = fixture.spec.clone();
    let panel = visible.panels.get_mut(0).unwrap();
    let y = panel.series[0].y;
    panel.series.push(SeriesSpec::new(y));
    panel.series[1].name = "Witness".into();
    panel
        .annotations
        .push(AnnotationSpec::new("Witness text", 1.0, 1.0));
    panel
        .shapes
        .push(ShapeSpec::new(ShapeKind::Rectangle, 0.2, 0.3, 0.7, 0.8));
    // Fix the axes so that texts and shapes are well within the frame.
    panel.bottom.auto_range = false;
    panel.bottom.range_min = Some(0.0);
    panel.bottom.range_max = Some(2.0);
    panel.left.auto_range = false;
    panel.left.range_min = Some(0.0);
    panel.left.range_max = Some(2.0);
    let (before, drawn_before) = agree(tree, &visible, SIZE, "visible objects");
    assert_eq!(drawn_before.panels[0].annotations.len(), 1);
    assert_eq!(drawn_before.panels[0].shapes.len(), 1);
    let mut hidden = visible.clone();
    hidden.panels.get_mut(0).unwrap().series[1].visible = false;
    hidden.panels.get_mut(0).unwrap().annotations[0].visible = false;
    hidden.panels.get_mut(0).unwrap().shapes[0].visible = false;
    let (after, drawn_after) = agree(tree, &hidden, SIZE, "hidden objects");
    assert_ne!(before, after, "visibility must change the pixels");
    assert!(drawn_after.panels[0].annotations.is_empty());
    assert!(drawn_after.panels[0].shapes.is_empty());
    assert_eq!(
        drawn_after.panels[0].legend_entries + 1,
        drawn_before.panels[0].legend_entries
    );
    assert_eq!(drawn_after.panels[0].series_geometry.len(), 1);
    let mut removed = visible;
    removed.panels.get_mut(0).unwrap().series.pop();
    removed.panels.get_mut(0).unwrap().annotations.clear();
    removed.panels.get_mut(0).unwrap().shapes.clear();
    assert_eq!(
        after,
        agree(tree, &removed, SIZE, "removed objects").0,
        "hiding adds no primitive"
    );
}

/// Hidden titles have neither drawing nor selection area any more; the two
/// backends follow this rule with or without an anchor (#85).
#[test]
fn hidden_titles_agreement_and_witnesses_with_and_without_anchor() {
    use stokesforge_plot::{AxisSide, TextAnchor, TextTarget};
    let fixture = minimal();
    let tree = fixture.project.tree();
    let mut spec = fixture.spec.clone();
    spec.title = "Global title".into();
    spec.panels.get_mut(0).unwrap().title = "Layer title".into();
    spec.panels.get_mut(0).unwrap().bottom.label = "Time".into();
    spec.panels.get_mut(0).unwrap().left.label = "Signal".into();
    for anchored in [false, true] {
        let mut visible = spec.clone();
        if anchored {
            visible.title_anchor = Some(TextAnchor::new(0.5, 0.5));
            let panel = visible.panels.get_mut(0).unwrap();
            panel.title_anchor = Some(TextAnchor::new(0.5, 0.5));
            panel.bottom.title_anchor = Some(TextAnchor::new(0.5, 0.5));
            panel.left.title_anchor = Some(TextAnchor::new(0.5, 0.5));
        }
        let (baseline, _) = agree(tree, &visible, SIZE, "visible titles");
        for target in [
            TextTarget::Title,
            TextTarget::PanelTitle { panel: 0 },
            TextTarget::AxisTitle {
                panel: 0,
                side: AxisSide::Bottom,
            },
            TextTarget::AxisTitle {
                panel: 0,
                side: AxisSide::Left,
            },
        ] {
            let mut hidden = visible.clone();
            assert!(target.set_visible(&mut hidden, false));
            let (pixels, drawn) = agree(tree, &hidden, SIZE, "hidden title");
            assert_ne!(pixels, baseline, "{target:?}, anchored={anchored}");
            assert_ne!(drawn.title.as_ref().map(|t| t.target), Some(target));
            assert!(!drawn.panels[0]
                .texts
                .iter()
                .any(|text| text.target == target));
            assert_eq!(
                target.text(&hidden),
                target.text(&visible),
                "hiding keeps the text"
            );
        }
    }
}

/// Ticks inward, on both sides or cut (#344): drawn by the
/// pipeline, so the two backends agree, on the four sides and
/// with or without numbers. Witness: each setting changes the figure.
#[test]
fn the_custom_ticks_agree_and_change_the_figure() {
    use stokesforge_plot::TickDirection;
    let fixture = minimal();
    let tree = fixture.project.tree();
    let mut base = fixture.spec.clone();
    {
        let panel = base.panels.get_mut(0).unwrap();
        panel.top.mirror = true;
        panel.right.mirror = true;
    }
    let (baseline, _) = agree(tree, &base, SIZE, "frame closed by default");
    // (name, direction, mirror numbers, ticks)
    let rules: [(&str, TickDirection, bool, bool); 6] = [
        ("inward", TickDirection::In, true, true),
        ("both sides", TickDirection::Both, true, true),
        ("inward without numbers", TickDirection::In, false, true),
        (
            "both sides without numbers",
            TickDirection::Both,
            false,
            true,
        ),
        ("cut", TickDirection::Out, true, false),
        ("cut without numbers", TickDirection::Out, false, false),
    ];
    let mut views = vec![baseline];
    for (what, direction, mirror_numbers, ticks) in rules {
        let mut spec = base.clone();
        let panel = spec.panels.get_mut(0).unwrap();
        for axis in [
            &mut panel.bottom,
            &mut panel.left,
            &mut panel.top,
            &mut panel.right,
        ] {
            axis.tick_direction = direction;
            axis.ticks = ticks;
        }
        panel.top.tick_labels = mirror_numbers;
        panel.right.tick_labels = mirror_numbers;
        let (pixels, _) = agree(tree, &spec, SIZE, what);
        assert!(
            !views.contains(&pixels),
            "{what}: the figure does not change"
        );
        if !ticks {
            // Without ticks: the four lines remain, the tick strokes
            // (from bottom to top, the graduations) are gone.
            let strokes = |p: &[Primitive]| {
                p.iter()
                    .filter(|p| matches!(p, Primitive::Segment(..)))
                    .count()
            };
            assert!(
                strokes(&pixels) + 20 < strokes(&views[0]),
                "{what}: the ticks must disappear"
            );
        }
        views.push(pixels);
    }

    // #344: the minor ticks (on by default), on the four sides,
    // agree; off, another count, or a number every other tick, the
    // figure changes.
    let mut changes = Vec::new();
    for (what, rule) in [
        ("without minor ticks", 0),
        ("a single minor tick", 1),
        ("a number every other tick", 2),
    ] {
        let mut spec = base.clone();
        let panel = spec.panels.get_mut(0).unwrap();
        for axis in [
            &mut panel.bottom,
            &mut panel.left,
            &mut panel.top,
            &mut panel.right,
        ] {
            match rule {
                0 => axis.minor_ticks = false,
                1 => axis.minor_tick_count = Some(1),
                _ => axis.label_every = 2,
            }
        }
        let (pixels, _) = agree(tree, &spec, SIZE, what);
        assert!(
            !views.contains(&pixels) && !changes.contains(&pixels),
            "{what}: the figure does not change"
        );
        changes.push(pixels);
    }

    // #344 « Avancé »: length and thickness of the ticks, on the four
    // sides, agree (drawn by the pipeline) and change the figure;
    // the color, which this comparison does not see, is guarded by
    // `render-svg/tests/mirrors.rs`.
    for (what, rule) in [
        ("long ticks at 10", 0),
        ("short ticks at 8", 1),
        ("thick ticks", 2),
    ] {
        let mut spec = base.clone();
        let panel = spec.panels.get_mut(0).unwrap();
        for axis in [
            &mut panel.bottom,
            &mut panel.left,
            &mut panel.top,
            &mut panel.right,
        ] {
            match rule {
                0 => axis.tick_length = Some(10.0),
                1 => axis.minor_tick_length = Some(8.0),
                _ => axis.tick_width = Some(3.0),
            }
        }
        let (pixels, _) = agree(tree, &spec, SIZE, what);
        assert!(
            !views.contains(&pixels) && !changes.contains(&pixels),
            "{what}: the figure does not change"
        );
        changes.push(pixels);
    }
}

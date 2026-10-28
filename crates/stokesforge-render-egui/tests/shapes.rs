//! What the agreement test cannot see: the `Shape::Text`s.
//!
//! `tests/agreement.rs` compares the segments — thus the axes, the grid and
//! the curve — between the two backends. It is blind to text, for a
//! fundamental reason: on the SVG side the text is an anchored `<text>`
//! element, on the egui side it is a positioned and rotated `Galley`. Both
//! are correct and incomparable.
//!
//! Yet this is exactly where the silliest class of bug of this backend
//! lives: a badly translated anchor or a rotation applied to the wrong
//! vector, and the left axis title goes out of the frame — without any
//! error, and without the agreement test flinching. Hence these checks, which
//! replace the glance that the window would give.

use egui::{Pos2, Rect, Shape};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::{DrawnPlot, TextMeasurer};
use stokesforge_render_egui::collect_shapes;

const SIZE: (f32, f32) = (800.0, 600.0);
const POINTS: usize = 64;

fn figure() -> Vec<Shape> {
    figure_with(|_panel| {})
}

/// The same figure as [`figure`], with `customize` applied to the panel
/// before drawing — for task R7 (`a_narrow_zoomed_framing_stays_bounded_to_the_rectangle`),
/// which needs an explicit framing without duplicating the construction of
/// the project and the series.
fn figure_with(customize: impl FnOnce(&mut PanelSpec)) -> Vec<Shape> {
    figure_with_drawn(customize).0
}

/// Like [`figure_with`], but also returns the [`DrawnPlot`] — for a test that
/// needs the real plot rectangle in pixels (`pixel_origin`/
/// `pixel_size`) rather than guessing it again.
fn figure_with_drawn(customize: impl FnOnce(&mut PanelSpec)) -> (Vec<Shape>, DrawnPlot) {
    let mut project = Project::new("Shapes").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let xs: Vec<f64> = (0..POINTS).map(|i| i as f64).collect();
    let ys: Vec<f64> = xs.iter().map(|x| (x / 8.0).sin()).collect();
    let x = project
        .add_column(sheet, None, Some("X"), numeric(&xs))
        .expect("X");
    let y = project
        .add_column(sheet, None, Some("Y"), numeric(&ys))
        .expect("Y");

    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "Time".into(),
        unit: "s".into(),
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Amplitude".into(),
        unit: "V".into(),
        ..AxisSpec::default()
    };
    let mut series = SeriesSpec::new_xy(x, y);
    series.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(series);
    customize(&mut panel);
    let spec = PlotSpec {
        title: "Sinusoïde".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel);

    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let mut shapes = Vec::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(SIZE.0, SIZE.1));
    let drawn = collect_shapes(&mut shapes, rect, &measurer, &plot).expect("plot");
    (shapes, drawn)
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("column")
}

#[test]
fn nothing_overflows_the_allocated_rectangle() {
    let frame = Rect::from_min_size(Pos2::ZERO, egui::vec2(SIZE.0, SIZE.1));
    // A margin of one point: the axis strokes sit on the border and
    // their thickness overflows by half a pixel on each side.
    let tolerance = 1.5;
    for shape in figure() {
        let bounds = shape.visual_bounding_rect();
        if !bounds.is_finite() {
            continue;
        }
        assert!(
            bounds.min.x >= frame.min.x - tolerance
                && bounds.min.y >= frame.min.y - tolerance
                && bounds.max.x <= frame.max.x + tolerance
                && bounds.max.y <= frame.max.y + tolerance,
            "{bounds:?} leaves the frame {frame:?} — an anchor or a rotation \
             of `draw_text` is badly translated, and this text would be invisible on \
             screen"
        );
    }
}

#[test]
fn the_left_axis_title_is_rotated_and_on_the_left() {
    let texts: Vec<_> = figure()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect();
    assert!(!texts.is_empty(), "no text: the tick labels are missing");

    let vertical = texts
        .iter()
        .find(|text| text.galley.job.text.contains("Amplitude"))
        .expect("the left axis title must be drawn");
    assert!(
        (vertical.angle - 3.0 * std::f32::consts::FRAC_PI_2).abs() < 1e-4,
        "vertical axis title at {} rad instead of 3π/2: the conversion \
         FontTransform → TextShape::angle is wrong",
        vertical.angle
    );
    assert!(
        vertical.visual_bounding_rect().max.x < SIZE.0 * 0.15,
        "the left axis title is not on the left: {:?}",
        vertical.visual_bounding_rect()
    );

    let title = texts
        .iter()
        .find(|text| text.galley.job.text.contains("Sinusoïde"))
        .expect("the graph title must be drawn");
    let bounds = title.visual_bounding_rect();
    assert!(
        title.angle == 0.0 && bounds.min.y < 40.0,
        "the graph title should be upright and at the top: {bounds:?}, angle {}",
        title.angle
    );
    // Centered: `titled` anchors at the horizontal center, so the anchor
    // offset must have been applied.
    assert!(
        (bounds.center().x - SIZE.0 / 2.0).abs() < 2.0,
        "title not centered: {bounds:?}"
    );
}

#[test]
fn the_series_is_a_single_polyline() {
    // This is not a detail of form: the whole economy of the cache rests
    // on it. One `Shape::Path` per series costs 64 bytes plus its points;
    // N `LineSegment`s — what the default implementation of `draw_path`
    // would produce — would cost 64 each, and measurement R0 0.3 (2.432 MB
    // for 300,000 points) would no longer hold.
    let blue = egui::Color32::from_rgb(0x4c, 0x78, 0xa8);
    let series: Vec<_> = figure()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(blue) => {
                Some(path)
            }
            _ => None,
        })
        .collect();
    // Two since R5: the curve, and the series thumbnail in the
    // legend — which is painted with the same style, that is the point of a
    // thumbnail. What this test locks is that the **curve** stays a
    // single shape, not that the figure carries only one.
    assert_eq!(
        series.len(),
        2,
        "expected: the curve and its legend thumbnail"
    );
    let curve = series
        .iter()
        .max_by_key(|path| path.points.len())
        .expect("the curve");
    assert_eq!(
        curve.points.len(),
        POINTS,
        "the polyline should carry all the points of the column"
    );
    assert_eq!(
        curve.stroke.width, 2.0,
        "the spec width is not carried over"
    );
    let thumbnail = series
        .iter()
        .min_by_key(|path| path.points.len())
        .expect("the thumbnail");
    assert_eq!(
        thumbnail.points.len(),
        2,
        "the legend thumbnail is a single segment"
    );
}

#[test]
fn a_narrow_zoomed_framing_stays_bounded_to_the_rectangle() {
    // The series covers X = 0..64; this framing shows only a tenth of it. The
    // majority of the points are therefore extrapolated by `map_coordinate` to
    // very distant pixel coordinates, which Plotters brings back to the edge of
    // the panel per component (`rect.truncate` — see the module header of
    // `stokesforge_render::paint`, section "Overflow was mishandled by Plotters").
    // This test does not claim that the rendering is *correct* in this case —
    // it is precisely the defect that `paint.rs` documents and that R7 defers —
    // but checks what must stay true despite it: no shape escapes the
    // rectangle allocated to it, and no coordinate becomes `NaN`
    // or infinite. It is the equivalent, for an extreme zoom, of
    // `nothing_overflows_the_allocated_rectangle` above.
    let frame = Rect::from_min_size(Pos2::ZERO, egui::vec2(SIZE.0, SIZE.1));
    let tolerance = 1.5;
    let zoomed = figure_with(|panel| {
        panel.bottom.auto_range = false;
        panel.bottom.range_min = Some(28.0);
        panel.bottom.range_max = Some(32.0);
        panel.left.auto_range = false;
        panel.left.range_min = Some(-0.1);
        panel.left.range_max = Some(0.1);
    });
    assert!(!zoomed.is_empty(), "the zoomed figure is empty");
    for shape in zoomed {
        let bounds = shape.visual_bounding_rect();
        assert!(
            bounds.is_finite(),
            "a shape has non-finite coordinates under a narrow framing: {bounds:?}"
        );
        assert!(
            bounds.min.x >= frame.min.x - tolerance
                && bounds.min.y >= frame.min.y - tolerance
                && bounds.max.x <= frame.max.x + tolerance
                && bounds.max.y <= frame.max.y + tolerance,
            "{bounds:?} leaves the frame {frame:?} under a narrow framing: the \
             coarse clipping of Plotters (`rect.truncate`) no longer bounds \
             correctly, which would be worse than the documented defect"
        );
    }
}

/// The defect described above, now fixed by
/// `stokesforge_render::clip`: this same narrow framing must no longer produce
/// a stray segment pressed against an edge of the panel.
///
/// Before geometric clipping, most points of the series fell outside the
/// panel under this framing and Plotters brought them back **per
/// component** to the edge (`rect.truncate`) — a long run of consecutive
/// vertices then shared exactly the same x or the same y, over the whole
/// width or height of the panel. With the real clipping
/// (Liang-Barsky, in pixels — `clip::clip_polyline`), an out-of-frame segment
/// is omitted and a crossing segment is cut at the exact point of the
/// crossing: no long run of consecutive vertices may any longer
/// coincide with an edge of the plot rectangle, only an isolated contact
/// point (the intersection itself) being possible.
#[test]
fn a_narrow_framing_no_longer_presses_the_curve_against_an_edge() {
    let (zoomed, drawn) = figure_with_drawn(|panel| {
        panel.bottom.auto_range = false;
        panel.bottom.range_min = Some(28.0);
        panel.bottom.range_max = Some(32.0);
        panel.left.auto_range = false;
        panel.left.range_min = Some(-0.1);
        panel.left.range_max = Some(0.1);
    });
    assert!(!zoomed.is_empty(), "the zoomed figure is empty");
    let panel = drawn.panel(0).expect("a drawn panel");
    let (ox, oy) = panel.pixel_origin;
    let (w, h) = panel.pixel_size;
    let (x_min, x_max) = (ox as f32, ox as f32 + w as f32);
    let (y_min, y_max) = (oy as f32, oy as f32 + h as f32);
    // A tolerance of one pixel for the integer rounding of the coordinates, not
    // for a real deviation.
    let epsilon = 0.6f32;
    // A single contact point with the edge (the Liang-Barsky intersection
    // itself) is legitimate; a longer run is not.
    const MAX_TOLERATED: usize = 2;

    let on_an_edge = |point: &Pos2| -> bool {
        (point.x - x_min).abs() < epsilon
            || (point.x - x_max).abs() < epsilon
            || (point.y - y_min).abs() < epsilon
            || (point.y - y_max).abs() < epsilon
    };

    let mut worst_run = 0usize;
    for shape in &zoomed {
        let Shape::Path(path) = shape else { continue };
        let mut current = 0usize;
        for point in &path.points {
            if on_an_edge(point) {
                current += 1;
                worst_run = worst_run.max(current);
            } else {
                current = 0;
            }
        }
    }
    assert!(
        worst_run <= MAX_TOLERATED,
        "a run of {worst_run} consecutive vertices sticks exactly to \
         an edge of the panel ({x_min}..{x_max} × {y_min}..{y_max}): the \
         curve is still pressed against the edge instead of being cut at \
         the real geometric intersection"
    );
}

/// #91: the ellipse is a subpixel path — its vertices are not all
/// at whole pixels, and none stayed in fixed point (sixteen times too
/// far) for lack of being decoded.
#[test]
fn an_ellipse_is_drawn_at_subpixel_on_screen() {
    use stokesforge_plot::{ShapeKind, ShapeSpec};
    let with_ellipse = figure_with(|panel| {
        panel.shapes = vec![ShapeSpec::new(ShapeKind::Ellipse, 10.0, -0.5, 50.0, 0.5)];
    });
    let without = figure();
    let points: Vec<Pos2> = with_ellipse
        .iter()
        .skip(without.len().saturating_sub(1))
        .filter_map(|shape| match shape {
            Shape::Path(path) if path.points.len() > 40 => Some(path.points.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!points.is_empty(), "no ellipse outline");
    let fractional = points
        .iter()
        .any(|p| p.x.fract().abs() > 0.01 || p.y.fract().abs() > 0.01);
    assert!(fractional, "all the vertices are whole");
    for p in &points {
        assert!(
            (0.0..=SIZE.0).contains(&p.x) && (0.0..=SIZE.1).contains(&p.y),
            "vertex outside the figure: {p:?}"
        );
    }
}

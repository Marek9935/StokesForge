//! #397: the number of ticks of an axis follows its length in pixels, so the
//! numbers of a narrow cell never touch, and the labels, the tick marks and
//! the minor ticks of an axis all use the same count. Screen and export
//! share the pipeline: the agreement on a narrow cell is in
//! `stokesforge-render-egui/tests/agreement.rs`.

use plotters::style::IntoFont;
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec, TickDirection};
use stokesforge_render::{estimate, TextMeasurer};
use stokesforge_render_svg::render_to_string;

/// A one-cell graph whose plot area is `frame` points, on `x`.
fn export(x: &[f64], frame: (f64, f64), rule: impl FnOnce(&mut PanelSpec)) -> String {
    let mut project = Project::new("Ticks").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let mut add_column = |name: &str, values: Vec<f64>| -> NodeId {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values),
            ColumnMeta::default(),
        )
        .expect("column");
        project
            .add_column(sheet, None, Some(name), column)
            .expect("column")
    };
    let xs = add_column("X", x.to_vec());
    let ys = add_column("Y", (0..x.len()).map(|i| i as f64 * 10.0).collect());
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(SeriesSpec::new_xy(xs, ys));
    rule(&mut panel);
    let mut spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    spec.frame_widths = vec![frame.0];
    spec.frame_heights = vec![frame.1];
    let measurer = TextMeasurer::default();
    let size = stokesforge_render_svg::figure_size(&spec, &measurer, &Default::default());
    let plot = resolve_spec(project.tree(), &spec);
    render_to_string(&plot, size, &measurer).expect("export")
}

/// The numbers written under the bottom axis: `(centre x, text)`.
fn bottom_labels(svg: &str) -> Vec<(f64, String)> {
    let bottom = bottom_line(svg);
    let mut out = Vec::new();
    let mut lines = svg.lines();
    while let Some(line) = lines.next() {
        if !line.starts_with("<text") {
            continue;
        }
        let number = |name: &str| -> Option<f64> {
            line.split(&format!(" {name}=\""))
                .nth(1)?
                .split('"')
                .next()?
                .parse()
                .ok()
        };
        let (Some(x), Some(y)) = (number("x"), number("y")) else {
            continue;
        };
        let text = lines.next().unwrap_or_default();
        if y > f64::from(bottom) && text.parse::<f64>().is_ok() {
            out.push((x, text.to_owned()));
        }
    }
    out
}

/// A stroke of two points.
type Segment = (f64, f64, f64, f64);

fn segments(svg: &str) -> Vec<Segment> {
    svg.lines()
        .filter(|line| line.starts_with("<polyline"))
        .filter_map(|line| {
            let points = line.split("points=\"").nth(1)?.split('"').next()?;
            let n: Vec<f64> = points
                .split([' ', ','])
                .filter_map(|t| t.parse().ok())
                .collect();
            (n.len() == 4).then(|| (n[0], n[1], n[2], n[3]))
        })
        .collect()
}

/// Ordinate of the bottom axis line (the lowest long horizontal stroke).
fn bottom_line(svg: &str) -> i32 {
    segments(svg)
        .into_iter()
        .filter(|s| s.1 == s.3 && (s.2 - s.0).abs() > 50.0)
        .map(|s| s.1 as i32)
        .max()
        .expect("bottom line")
}

/// The vertical strokes touching the bottom line from below (ticks),
/// split into long (major) and short (minor) ones: their abscissae.
fn bottom_marks(svg: &str) -> (Vec<f64>, Vec<f64>) {
    let bottom = f64::from(bottom_line(svg));
    let marks: Vec<(f64, f64)> = segments(svg)
        .into_iter()
        .filter(|s| s.0 == s.2 && s.1.min(s.3) <= bottom + 0.6 && s.1.max(s.3) >= bottom - 0.6)
        .filter(|s| (s.3 - s.1).abs() < 30.0)
        .map(|s| (s.0, (s.3 - s.1).abs()))
        .collect();
    let longest = marks.iter().map(|m| m.1).fold(0.0, f64::max);
    let mut majors: Vec<f64> = marks
        .iter()
        .filter(|m| m.1 >= longest - 0.5)
        .map(|m| m.0)
        .collect();
    let mut minors: Vec<f64> = marks
        .iter()
        .filter(|m| m.1 < longest - 0.5)
        .map(|m| m.0)
        .collect();
    majors.sort_by(f64::total_cmp);
    minors.sort_by(f64::total_cmp);
    (majors, minors)
}

fn width_of(text: &str) -> f64 {
    let style = ("sans-serif", 14.0)
        .into_font()
        .color(&plotters::style::BLACK);
    f64::from(estimate(&TextMeasurer::default(), text, &style).0)
}

fn assert_no_overlap(labels: &[(f64, String)]) {
    let mut sorted = labels.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    for pair in sorted.windows(2) {
        let reach = (width_of(&pair[0].1) + width_of(&pair[1].1)) / 2.0;
        assert!(
            pair[1].0 - pair[0].0 >= reach,
            "{:?} and {:?} touch: {labels:?}",
            pair[0],
            pair[1]
        );
    }
}

/// The range of the issue (-67.5..317.5) on a 120 px axis.
const RANGE: [f64; 2] = [-67.5, 317.5];

#[test]
fn a_narrow_axis_writes_fewer_numbers_that_do_not_touch() {
    let narrow = bottom_labels(&export(&RANGE, (120.0, 200.0), |_| {}));
    assert!(narrow.len() >= 2, "{narrow:?}");
    assert!(narrow.len() < 11, "witness: still 11 numbers: {narrow:?}");
    assert_no_overlap(&narrow);
    // A wide axis keeps the usual eleven or so.
    let wide = bottom_labels(&export(&RANGE, (392.0, 284.0), |_| {}));
    assert!(
        wide.len() > narrow.len() + 3,
        "{} vs {}",
        wide.len(),
        narrow.len()
    );
}

#[test]
fn the_tick_marks_and_the_minor_ticks_follow_the_labels() {
    let svg = export(&RANGE, (160.0, 200.0), |p| {
        p.bottom.tick_direction = TickDirection::Both;
        p.bottom.minor_ticks = true;
    });
    let labels = bottom_labels(&svg);
    let (majors, minors) = bottom_marks(&svg);
    assert!(labels.len() < 11 && labels.len() >= 2, "{labels:?}");
    assert_eq!(majors.len(), labels.len(), "{majors:?} {labels:?}");
    for ((x, _), mark) in labels.iter().zip(&majors) {
        assert!((x - mark).abs() <= 1.5, "label at {x}, tick mark at {mark}");
    }
    // Minor ticks cut each step of the major ticks (4 or 5 divisions) and
    // never sit on a major one.
    let first = majors[0];
    let last = majors[majors.len() - 1];
    let between: Vec<f64> = minors
        .iter()
        .copied()
        .filter(|m| *m > first && *m < last)
        .collect();
    let steps = majors.len() - 1;
    assert!(
        !between.is_empty() && between.len().is_multiple_of(steps),
        "{between:?}"
    );
    let per_step = between.len() / steps;
    assert!((3..=4).contains(&per_step), "{per_step}");
    assert!(between
        .iter()
        .all(|m| majors.iter().all(|k| (k - m).abs() > 1.0)));
}

#[test]
fn long_numbers_lose_ticks_even_on_a_long_axis() {
    // "1234.55" is wide: the density alone would keep the eleven ticks.
    let svg = export(&[1234.5, 1234.7, 1234.9], (392.0, 284.0), |_| {});
    let labels = bottom_labels(&svg);
    assert!(labels.len() >= 2, "{labels:?}");
    assert_no_overlap(&labels);
    let plain = bottom_labels(&export(&RANGE, (392.0, 284.0), |_| {}));
    assert!(
        labels.len() < plain.len(),
        "{} vs {}",
        labels.len(),
        plain.len()
    );
}

/// The numbers written left of the left axis: their ordinates.
fn left_label_ordinates(svg: &str) -> Vec<f64> {
    let left = segments(svg)
        .into_iter()
        .filter(|s| s.0 == s.2 && (s.3 - s.1).abs() > 50.0)
        .map(|s| s.0)
        .fold(f64::MAX, f64::min);
    let mut out = Vec::new();
    let mut lines = svg.lines();
    while let Some(line) = lines.next() {
        if !line.starts_with("<text") {
            continue;
        }
        let number = |name: &str| -> Option<f64> {
            line.split(&format!(" {name}=\""))
                .nth(1)?
                .split('"')
                .next()?
                .parse()
                .ok()
        };
        let text = lines.next().unwrap_or_default();
        if let (Some(x), Some(y)) = (number("x"), number("y")) {
            if x < left && text.parse::<f64>().is_ok() {
                out.push(y);
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out
}

#[test]
fn a_short_axis_writes_fewer_numbers_one_text_height_apart() {
    let short = left_label_ordinates(&export(&RANGE, (300.0, 90.0), |_| {}));
    assert!(short.len() >= 2 && short.len() < 11, "{short:?}");
    for pair in short.windows(2) {
        assert!(pair[1] - pair[0] >= 14.0, "numbers touch: {short:?}");
    }
    let tall = left_label_ordinates(&export(&RANGE, (300.0, 284.0), |_| {}));
    assert!(
        tall.len() > short.len() + 3,
        "{} vs {}",
        tall.len(),
        short.len()
    );
}

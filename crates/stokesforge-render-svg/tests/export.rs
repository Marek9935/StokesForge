//! What the export must produce, independently of the agreement test.
//!
//! The agreement test (`stokesforge-render-egui/tests/agreement.rs`) proves that the
//! two backends *agree*. It does not prove that what they agree on
//! is a valid and complete SVG document — two empty figures would
//! agree perfectly. Hence this file.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project, Tree};
use stokesforge_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::{naive_estimate, render_spec, render_to_file, render_to_string};

struct Fixture {
    project: Project,
    spec: PlotSpec,
}

fn fixture() -> Fixture {
    let mut project = Project::new("Export").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let xs: Vec<f64> = (0..40).map(|i| i as f64 * 0.25).collect();
    let ys: Vec<f64> = xs.iter().map(|x| x.exp() / 10.0).collect();
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
        grid: true,
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Réponse".into(),
        unit: "µV".into(),
        grid: true,
        ..AxisSpec::default()
    };
    let mut series = SeriesSpec::new_xy(x, y);
    series.line = LineSpec {
        color: "#e45756".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(series);

    let spec = PlotSpec {
        title: "Impulse response".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel);
    Fixture { project, spec }
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("column")
}

fn tree(fixture: &Fixture) -> &Tree {
    fixture.project.tree()
}

#[test]
fn the_document_carries_the_whole_figure() {
    let fixture = fixture();
    let document = render_spec(
        tree(&fixture),
        &fixture.spec,
        (800, 600),
        &TextMeasurer::default(),
    )
    .expect("export");

    assert!(document.starts_with("<svg"), "SVG header missing");
    assert!(
        document.trim_end().ends_with("</svg>"),
        "document not closed"
    );
    assert!(document.contains("width=\"800\""));
    assert!(document.contains("height=\"600\""));

    // The title, the axis labels and the series color. Non-ASCII
    // is checked as is: `plotters-svg` escapes `<`, `>` and `&`, not
    // UTF-8, and the day it did we would need to notice.
    assert!(document.contains("Impulse response"));
    assert!(document.contains("Réponse (µV)"));
    assert!(document.contains("Time (s)"));
    assert!(
        document.contains("#E45756"),
        "the series color is not in the document"
    );
    assert!(
        document.matches("<polyline").count() > 5,
        "too few polylines: the grid or the curve is missing"
    );
}

#[test]
fn a_broken_series_does_not_empty_the_document() {
    // §11.1: a broken reference is a state, not a failure. The export must
    // output the figure all the same.
    let mut fixture = fixture();
    if let Some(panel) = fixture.spec.panels.get_mut(0) {
        panel
            .series
            .push(SeriesSpec::new(NodeId::from_raw(u64::MAX)));
    }
    let plot = resolve_spec(tree(&fixture), &fixture.spec);
    assert_eq!(plot.broken_series().count(), 1);

    let document = render_to_string(&plot, (640, 480), &TextMeasurer::default()).expect("export");
    assert!(document.contains("Impulse response"));
    assert!(document.contains("#E45756"), "the valid series is plotted");
}

#[test]
fn writing_to_disk() {
    let fixture = fixture();
    let plot = resolve_spec(tree(&fixture), &fixture.spec);
    let dir = tempfile::tempdir().expect("temporary directory");
    let target = dir.path().join("figure.svg");
    render_to_file(&target, &plot, (400, 300), &TextMeasurer::default()).expect("write");
    let content = std::fs::read_to_string(&target).expect("read back");
    assert!(content.starts_with("<svg"));
    assert!(content.len() > 500, "suspiciously short document");
}

#[test]
fn the_naive_witness_does_reproduce_plotters_heuristic() {
    // `naive_estimate` exists only to serve as a witness. A wrong witness
    // is worse than no witness: it would suggest a difference where there
    // is none. The two properties that characterize it are therefore
    // checked here — width proportional to the **number of bytes**, and
    // insensitivity to the actual drawing of the glyphs.
    assert_eq!(naive_estimate("111", 20.0), naive_estimate("WWW", 20.0));
    assert_eq!(
        naive_estimate("λ", 20.0),
        naive_estimate("ab", 20.0),
        "\"λ\" is two bytes: the heuristic must count it as two \
         characters, that is its whole flaw"
    );
    let (width, height) = naive_estimate("0,5", 20.0);
    assert!(width > 0 && height > 0);
}

#[test]
fn the_graphs_decimal_separator_applies_to_the_export() {
    let mut fixture = fixture();
    // An axis from 0 to 1: decimal ticks (0.2, 0.4…) for sure.
    if let Some(panel) = fixture.spec.panels.get_mut(0) {
        panel.left.auto_range = false;
        panel.left.range_min = Some(0.0);
        panel.left.range_max = Some(1.0);
    }
    let render = |fixture: &Fixture| {
        render_spec(
            tree(fixture),
            &fixture.spec,
            (800, 600),
            &TextMeasurer::default(),
        )
        .expect("export")
    };
    // Witness: with a point, decimal ticks exist and no comma
    // appears in a number.
    let with_point = render(&fixture);
    assert!(with_point.contains("\n0.2\n"), "decimal tick with a point");
    assert!(!with_point.contains("\n0,2\n"));

    fixture.spec.decimal_comma = true;
    let with_comma = render(&fixture);
    assert!(with_comma.contains("\n0,2\n"), "tick with a comma");
    assert!(!with_comma.contains("\n0.2\n"));
}

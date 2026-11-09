//! The colour of a curve on a colour bar (#499, step C) is computed at each
//! draw: it reaches the document, and the stored style is never touched.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{ColorBarSegment, ColorBarSpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

const STORED: &str = "#112233";

fn spec_with(project: &mut Project, bar: Option<ColorBarSpec>, x_primes: [f64; 2]) -> PlotSpec {
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let mut add = |name: &str, x_prime: Option<f64>| -> NodeId {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(vec![0.0, 1.0, 2.0]),
            ColumnMeta {
                x_prime,
                ..ColumnMeta::default()
            },
        )
        .expect("column");
        project
            .add_column(sheet, None, Some(name), column)
            .expect("column")
    };
    let x = add("X", None);
    let mut panel = PanelSpec::at(0, 0);
    for (name, x_prime) in ["A", "B"].into_iter().zip(x_primes) {
        let y = add(name, Some(x_prime));
        let mut series = SeriesSpec::new_xy(x, y);
        series.group = Some(1);
        series.line.color = STORED.to_string();
        series.symbol.color = STORED.to_string();
        panel.series.push(series);
    }
    panel
        .group_gradients
        .insert(1, vec!["#10ff20".to_string(), "#ff1020".to_string()]);
    panel.color_bar = bar;
    PlotSpec::empty("").with_panels(Panels::new(vec![panel]))
}

fn export(spec: &PlotSpec, project: &Project) -> String {
    let plot = resolve_spec(project.tree(), spec);
    render_to_string(&plot, (480, 360), &TextMeasurer::default())
        .expect("export")
        .to_uppercase()
}

fn bar() -> ColorBarSpec {
    ColorBarSpec {
        segments: vec![ColorBarSegment::new(1)],
        ..ColorBarSpec::default()
    }
}

/// Not baked: the curves are painted in the colours their values take
/// (the two ends of the gradient), the stored colour is not painted, and the
/// spec still holds it. Witness: without the bar the stored colour is
/// painted and the gradient ends are not.
#[test]
fn the_value_colour_is_painted_and_never_stored() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(bar()), [0.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#10FF20"), "start colour missing");
    assert!(document.contains("#FF1020"), "end colour missing");
    assert!(
        !document.contains("#112233"),
        "the stored colour is painted"
    );
    assert_eq!(spec.panels[0].series[0].line.color, STORED);
    assert_eq!(spec.panels[0].series[1].symbol.color, STORED);

    let mut other = Project::new("Plain").expect("project");
    let plain = spec_with(&mut other, None, [0.0, 57.0]);
    let document = export(&plain, &other);
    assert!(document.contains("#112233"), "witness: stored colour");
    assert!(!document.contains("#10FF20"), "witness: no value colour");
}

/// Editing a value recolours at the next draw, the spec staying as it was
/// (T1): the same spec with x' = 57 / 57 paints both curves in the start
/// colour (all values equal).
#[test]
fn equal_values_paint_the_start_colour_for_everyone() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(bar()), [57.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#10FF20"));
    assert!(!document.contains("#FF1020"), "no end colour when equal");
}

/// A hidden bar acts as no bar: the stored colours come back.
#[test]
fn a_hidden_bar_gives_the_stored_colours_back() {
    let mut project = Project::new("Bar").expect("project");
    let mut hidden = bar();
    hidden.visible = false;
    let spec = spec_with(&mut project, Some(hidden), [0.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#112233"));
    assert!(!document.contains("#10FF20"));
}

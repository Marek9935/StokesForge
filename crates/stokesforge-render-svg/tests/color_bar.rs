//! The colour of a curve on a colour bar (#499, step C) is computed at each
//! draw: it reaches the document, and the stored style is never touched.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{ColorBarSegment, ColorBarSpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

const STORED: &str = "#112233";

fn spec_with(project: &mut Project, bar: Option<ColorBarSpec>, x_primes: &[f64]) -> PlotSpec {
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
    for (name, x_prime) in ["A", "B", "C", "D"]
        .into_iter()
        .zip(x_primes.iter().copied())
    {
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
    let spec = spec_with(&mut project, Some(bar()), &[0.0, 57.0]);
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
    let plain = spec_with(&mut other, None, &[0.0, 57.0]);
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
    let spec = spec_with(&mut project, Some(bar()), &[57.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#10FF20"));
    // The bar's own stripes may reach the end colour: compare with a spread
    // of values, where a curve is painted in it as well.
    let mut other = Project::new("Spread").expect("project");
    let spread = export(&spec_with(&mut other, Some(bar()), &[0.0, 57.0]), &other);
    assert!(
        document.matches("#FF1020").count() < spread.matches("#FF1020").count(),
        "no curve in the end colour when equal"
    );
}

/// A hidden bar acts as no bar: the stored colours come back.
#[test]
fn a_hidden_bar_gives_the_stored_colours_back() {
    let mut project = Project::new("Bar").expect("project");
    let mut hidden = bar();
    hidden.visible = false;
    let spec = spec_with(&mut project, Some(hidden), &[0.0, 57.0]);
    let document = export(&spec, &project);
    assert!(document.contains("#112233"));
    assert!(!document.contains("#10FF20"));
}

/// The tab reports the curves of the bar that have no value (C8): both have
/// an x', neither has a comment; a hidden bar reports nothing.
/// Witness: counting every member (not only those without a color) fails the
/// first assertion.
#[test]
fn the_curves_without_a_value_are_counted() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(bar()), &[0.0, 57.0]);
    let count = |spec: &PlotSpec| {
        let plot = resolve_spec(project.tree(), spec);
        stokesforge_render::color_bar::missing_value_count(&plot.panels[0])
    };
    assert_eq!(count(&spec), 0);
    let mut comments = spec.clone();
    comments
        .panels
        .get_mut(0)
        .expect("panel")
        .color_bar
        .as_mut()
        .expect("bar")
        .value = stokesforge_plot::ColorBarValue::Comments;
    assert_eq!(count(&comments), 2);
    comments
        .panels
        .get_mut(0)
        .expect("panel")
        .color_bar
        .as_mut()
        .expect("bar")
        .visible = false;
    assert_eq!(count(&comments), 0);
}

fn log_bar() -> ColorBarSpec {
    ColorBarSpec {
        scale: stokesforge_plot::ColorBarScale::Log,
        ..bar()
    }
}

/// One value <= 0 on a log scale (#499, C8/C9): only that curve has no value
/// (it keeps its own colour); the others keep the colours of the range
/// 1..100 and the bar keeps its ticks. Witness: the old rule (every curve of
/// the segment loses its value) paints no gradient end at all.
#[test]
fn a_zero_on_a_log_bar_only_costs_its_own_curve() {
    let mut project = Project::new("Bar").expect("project");
    let spec = spec_with(&mut project, Some(log_bar()), &[0.0, 1.0, 10.0, 100.0]);
    let document = export(&spec, &project);
    assert!(
        document.contains("#112233"),
        "the zero curve keeps its colour"
    );
    // The curves at 1 and 100 are the two ends of the gradient: count them in
    // the curve paths (stroke), not only in the bar stripes.
    let curves = |colour: &str| document.matches(&format!("STROKE=\"{colour}\"")).count();
    assert!(curves("#10FF20") >= 1 && curves("#FF1020") >= 1);
    let has_tick = |label: &str| document.lines().any(|line| line == label);
    assert!(has_tick("100"), "end tick missing");
    assert!(has_tick("1"), "start tick missing");
    let plot = resolve_spec(project.tree(), &spec);
    assert_eq!(
        stokesforge_render::color_bar::missing_value_count(&plot.panels[0]),
        1
    );
}

fn font_sizes(document: &str) -> std::collections::BTreeSet<String> {
    document
        .split("FONT-SIZE=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

/// The bar's own font (title, ticks): size 30 in the bar reaches the SVG,
/// the default bar has no such size. Witness: a bar font not read leaves 24.19 (30 scaled)
/// out.
#[test]
fn the_bar_font_size_reaches_the_export() {
    let mut project = Project::new("Bar").expect("project");
    let mut big = bar();
    big.font.size = Some(30.0);
    big.title = "Time".to_string();
    let spec = spec_with(&mut project, Some(big), &[0.0, 57.0]);
    let sized = font_sizes(&export(&spec, &project));
    assert!(
        sized.iter().any(|size| size.starts_with("24.19")),
        "{sized:?}"
    );
    let mut other = Project::new("Default").expect("project");
    let plain = spec_with(&mut other, Some(bar()), &[0.0, 57.0]);
    assert!(!font_sizes(&export(&plain, &other))
        .iter()
        .any(|size| size.starts_with("24.19")));
}

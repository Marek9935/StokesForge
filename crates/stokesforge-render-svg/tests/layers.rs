//! Overlaid layers (#167) seen in the export: the second layer carries its
//! X axis at the top and its Y axis on the right, with their own ticks.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{OverlaySpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

fn export(overlay: Option<OverlaySpec>) -> String {
    let mut project = Project::new("Layers").expect("project");
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
    let x = add_column("X", vec![0.0, 1.0, 2.0]);
    let y = add_column("Y", vec![10.0, 20.0, 30.0]);
    let x2 = add_column("X2", vec![400.0, 600.0, 800.0]);
    let y2 = add_column("Y2", vec![0.0, 0.5, 1.0]);
    let mut base = PanelSpec::at(0, 0);
    base.series.push(SeriesSpec::new_xy(x, y));
    let mut panels = vec![base.clone()];
    if let Some(overlay) = overlay {
        let mut top = PanelSpec::overlay_on(&base, overlay);
        top.series.push(SeriesSpec::new_xy(x2, y2));
        panels.push(top);
    }
    let spec = PlotSpec::empty("").with_panels(Panels::new(panels));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    if let Some(dir) = std::env::var_os("STOKESFORGE_DUMP") {
        let name = if overlay.is_some() { "layers" } else { "base" };
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.svg")), &svg).ok();
    }
    svg
}

/// The 400…800 ticks (written without decimals, #175) of the overlaid layer
/// exist only in the export that contains it: witness without a layer.
#[test]
fn the_overlaid_layer_plots_its_ticks_at_the_top() {
    let without = export(None);
    assert!(!without.contains("\n800\n"), "witness: no 800 tick");
    let with = export(Some(OverlaySpec::new(true, true)));
    assert!(with.contains("\n800\n"), "X2 tick missing: {with}");
}

/// #166: a "row" series is plotted against the columns' x' — the
/// X axis ticks cover the instants (0…240), not the indices.
#[test]
fn a_row_series_is_plotted_against_the_x_primes() {
    let mut project = Project::new("Kinetics").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let mut anchor = None;
    for (time, value) in [(0.0, 1.0), (120.0, 2.0), (240.0, 3.0)] {
        let id = project
            .add_column(sheet, None, None, Column::from_f64(&[value, value * 2.0]))
            .expect("column");
        let mut meta = project
            .tree()
            .node(id)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .clone();
        meta.x_prime = Some(time);
        project.set_column_meta(id, meta).expect("x'");
        anchor.get_or_insert(id);
    }
    let mut series = SeriesSpec::new(anchor.unwrap());
    series.row = Some(1);
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(series);
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    assert!(svg.contains("\n200\n"), "x' tick missing: {svg}");
    assert!(
        svg.contains("<polyline") || svg.contains("<path"),
        "curve missing"
    );
}

/// The plotted axes meet at the corners: Plotters leaves them one pixel
/// apart, the pipeline fills that empty corner.
#[test]
fn the_axes_meet_at_the_corners() {
    let corners = |svg: &str| {
        svg.lines()
            .filter(|l| l.contains("<polyline"))
            .filter_map(|l| l.split("points=\"").nth(1))
            .filter(|p| {
                let n: Vec<f64> = p
                    .split([',', ' ', '"'])
                    .filter_map(|v| v.parse().ok())
                    .collect();
                n.len() == 6
                    && (n[0] - n[2]).abs() == 1.0
                    && n[1] == n[3]
                    && n[2] == n[4]
                    && (n[3] - n[5]).abs() == 1.0
            })
            .count()
    };
    assert_eq!(corners(&export(None)), 1, "bottom-left only");
    let with = export(Some(OverlaySpec::new(true, true)));
    assert_eq!(corners(&with), 4);
}

/// #168: three overlaid layers, axes stacked outward. Returns
/// the figure's export.
fn export_three() -> String {
    let mut project = Project::new("Layers").expect("project");
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
    let datasets = [
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("X2", vec![400.0, 600.0, 800.0]),
        ("Y2", vec![0.0, 0.5, 1.0]),
        ("X3", vec![4000.0, 6000.0, 8000.0]),
        ("Y3", vec![100.0, 200.0, 300.0]),
        ("X4", vec![40000.0, 60000.0, 80000.0]),
        ("Y4", vec![1000.0, 2000.0, 3000.0]),
    ];
    let ids: Vec<NodeId> = datasets
        .into_iter()
        .map(|(name, values)| add_column(name, values))
        .collect();
    let mut base = PanelSpec::at(0, 0);
    base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
    let mut panels = vec![base.clone()];
    for rank in 1..4 {
        let mut layer = PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES);
        layer
            .series
            .push(SeriesSpec::new_xy(ids[2 * rank], ids[2 * rank + 1]));
        panels.push(layer);
    }
    let spec = PlotSpec::empty("").with_panels(Panels::new(panels));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    if let Some(dir) = std::env::var_os("STOKESFORGE_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join("three.svg"), &svg).ok();
    }
    svg
}

#[test]
fn the_ticks_of_overlaid_layers_stack_up() {
    let svg = export_three();
    for tick in ["\n850\n", "\n8500\n", "\n85000\n", "\n3000\n"] {
        assert!(svg.contains(tick), "{tick:?} missing: {svg}");
    }
    // The X3 and X4 ticks are above those of X2 (smaller y).
    let ordinate = |text: &str| -> f64 {
        let end = svg.find(&format!("\n{text}\n")).expect("tick");
        let tag = &svg[svg[..end].rfind("<text").expect("tag")..end];
        let y = tag.split("y=\"").nth(1).expect("y");
        y.split('"').next().unwrap().parse().expect("number")
    };
    assert!(ordinate("8500") < ordinate("850"));
    assert!(ordinate("85000") < ordinate("8500"));
}

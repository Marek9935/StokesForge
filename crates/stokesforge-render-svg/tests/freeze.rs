//! #529: `freeze_auto_ranges` writes the ranges a graph is drawn with into the
//! axes still on Auto. It draws (the overlay alignment needs the tick count).

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project, Tree};
use stokesforge_plot::{OverlaySpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::{AutoMargins, FigureTexts, FontChoice, TextMeasurer};
use stokesforge_render_svg::{freeze_auto_ranges, render_to_string_with_geometry_and_texts};

fn project_with(columns: &[(&str, Vec<f64>)]) -> (Project, Vec<NodeId>) {
    let mut project = Project::new("Freeze").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let ids = columns
        .iter()
        .map(|(name, values)| {
            let column = Column::from_block(
                ColumnType::Numeric,
                ValueBlock::Float(values.clone()),
                ColumnMeta::default(),
            )
            .expect("column");
            project
                .add_column(sheet, None, Some(name), column)
                .expect("column")
        })
        .collect();
    (project, ids)
}

fn freeze(tree: &Tree, spec: &mut PlotSpec, margins: AutoMargins) -> bool {
    freeze_auto_ranges(
        tree,
        spec,
        margins,
        &TextMeasurer::default(),
        &FontChoice::default(),
    )
    .expect("freeze")
}

/// The ranges `spec` is drawn with: `(x, y)` of each layer.
fn drawn(tree: &Tree, spec: &PlotSpec, margins: AutoMargins) -> Vec<((f64, f64), (f64, f64))> {
    let measurer = TextMeasurer::default();
    let size = stokesforge_render_svg::figure_size(spec, &measurer, &FontChoice::default());
    let plot = resolve_spec(tree, spec);
    let texts = FigureTexts::default().with_margins(margins);
    let (_, geometry) =
        render_to_string_with_geometry_and_texts(&plot, size, &measurer, &texts).expect("draw");
    geometry
        .panels
        .iter()
        .map(|p| (p.x_range, p.y_range))
        .collect()
}

fn bounds(axis: &stokesforge_plot::AxisSpec) -> (f64, f64) {
    (axis.range_min.expect("From"), axis.range_max.expect("To"))
}

const TOP_10: AutoMargins = AutoMargins {
    left: 0.0,
    right: 0.0,
    bottom: 1.0,
    top: 10.0,
};

/// A layer with its own top X is aligned on the covered layer's ticks (#174):
/// the frozen bounds are the drawn ones, which only a draw can give. Redrawn
/// from the written ranges, the figure is the same.
#[test]
fn an_overlay_with_its_own_top_x_freezes_the_drawn_aligned_ranges() {
    let (project, ids) = project_with(&[
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("X2", vec![401.0, 600.0, 803.0]),
        ("Y2", vec![0.0, 0.5, 1.0]),
    ]);
    let mut base = PanelSpec::at(0, 0);
    base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
    let mut top = PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES);
    top.series.push(SeriesSpec::new_xy(ids[2], ids[3]));
    let mut spec = PlotSpec::empty("").with_panels(Panels::new(vec![base, top]));

    let before = drawn(project.tree(), &spec, TOP_10);
    assert!(freeze(project.tree(), &mut spec, TOP_10));
    let layer = &spec.panels[1];
    assert!(!layer.bottom.auto_range && !layer.left.auto_range);
    assert_eq!(bounds(&layer.bottom), before[1].0, "x of the overlay");
    assert_eq!(bounds(&layer.left), before[1].1, "y of the overlay");
    assert_eq!(bounds(&spec.panels[0].bottom), before[0].0);
    // The written spec draws the same figure.
    assert_eq!(drawn(project.tree(), &spec, TOP_10), before);
    // The aligned range is not the plain padded extent (witness that a draw
    // was needed): 401..803 would not be extended to the tick grid.
    assert_ne!(bounds(&layer.bottom), (401.0, 803.0));
}

/// A layer without data keeps Auto (no `0..1` written), and the axes of the
/// layer that has some are frozen; a second call has nothing to do.
#[test]
fn an_axis_without_data_stays_auto() {
    let (project, ids) = project_with(&[("X", vec![1.0, 2.0, 3.0]), ("Y", vec![4.0, 5.0, 6.0])]);
    let mut base = PanelSpec::at(0, 0);
    base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
    let empty = PanelSpec::at(0, 1);
    let mut spec = PlotSpec::empty("").with_panels(Panels::new(vec![base, empty]));
    assert!(freeze(project.tree(), &mut spec, TOP_10));
    assert!(spec.panels[1].bottom.auto_range && spec.panels[1].left.auto_range);
    assert!(!spec.panels[0].bottom.auto_range);
    assert!(!freeze(project.tree(), &mut spec, TOP_10), "nothing left");
}

/// The direction is kept: an inverted axis is written in display order, so
/// the drawing does not flip twice.
#[test]
fn an_inverted_axis_keeps_its_direction() {
    let (project, ids) = project_with(&[("X", vec![1.0, 2.0, 3.0]), ("Y", vec![4.0, 5.0, 6.0])]);
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
    panel.bottom.inverted = true;
    let mut spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    let before = drawn(project.tree(), &spec, TOP_10);
    assert!(freeze(project.tree(), &mut spec, TOP_10));
    let bottom = &spec.panels[0].bottom;
    assert!(bottom.inverted && !bottom.auto_range);
    let (from, to) = bounds(bottom);
    assert!(from > to, "display order: {from} {to}");
    assert_eq!(drawn(project.tree(), &spec, TOP_10), before);
}

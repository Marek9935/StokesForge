//! The demo graphs: `Graph1` (the long-standing one) and six graphs that each
//! show a family of plot settings.
//!
//! | Graph | What it shows |
//! |---|---|
//! | `Graph1` | one curve, two linear axes |
//! | `Panels` | a 2×2 grid of panels |
//! | `Axes` | right Y axis, logarithmic scale, layer with secondary axes |
//! | `PlotTypes` | scatter, line and points, bars, histogram, error bars |
//! | `Annotations` | Typst text, the four graphic objects, placed legend |
//! | `Groups` | a group of six curves and its color gradient |
//! | `Rows` | "row" series from the kinetics of `Book1` |
//!
//! No graph carries a title (#103): only panel, axis and series titles are
//! set. A new graph is built **here** on the `Project` before `Session::spawn`
//! (invariant 1); series groups go through the `PanelSpec` methods
//! (`push_series_batch`, `set_group_gradient`), never through a group number
//! written by hand. The X role of the columns is set before the graphs cite
//! them (`workbooks.rs`).

use anyhow::Context as _;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{
    AnnotationSpec, AxisScale, AxisSpec, BinCount, LegendPosition, LineSpec, OverlaySpec,
    PanelSpec, PlotSpec, PlotType, SeriesAxis, SeriesSpec, ShapeKind, ShapeSpec,
};

use super::workbooks::{Book1, Measures};

/// A titled axis, with its unit.
fn axis(title: &str, unit: &str) -> AxisSpec {
    AxisSpec {
        label: title.into(),
        unit: unit.into(),
        ..AxisSpec::default()
    }
}

/// A series of the default palette: `rank` chooses the color.
fn curve(x: Option<NodeId>, y: NodeId, name: &str, rank: usize, kind: PlotType) -> SeriesSpec {
    SeriesSpec {
        x,
        name: name.into(),
        plot_type: kind,
        ..SeriesSpec::from_palette(y, rank)
    }
}

/// A panel at `(row, column)` with its two titled axes.
fn panel_with(
    place: (usize, usize),
    bottom: (&str, &str),
    left: (&str, &str),
    series: Vec<SeriesSpec>,
) -> PanelSpec {
    let mut panel = PanelSpec::at(place.0, place.1);
    panel.bottom = axis(bottom.0, bottom.1);
    panel.left = axis(left.0, left.1);
    panel.series = series;
    panel
}

/// `Graph1`: one curve and two linear axes to judge the rendering by eye.
/// Like the graphs the user creates, the demo adds no title by default
/// (#103); the axis labels and the series name stay explicit.
pub(super) fn demo_spec(x: NodeId, y: NodeId) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "λ".into(),
        unit: "nm".into(),
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Measured absorbance".into(),
        unit: "".into(),
        ..AxisSpec::default()
    };
    let mut series_spec = SeriesSpec::new_xy(x, y);
    series_spec.name = "Sample µ1".into();
    series_spec.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(series_spec);
    PlotSpec::empty("").with_panels(panel)
}

/// A 2×2 grid: spectrum, scatter, histogram, bars.
fn panels(book1: &Book1, m: &Measures) -> PlotSpec {
    let mut histogram = curve(None, m.noise, "Noise", 2, PlotType::Histogram);
    histogram.histogram_bins = BinCount::new(20).unwrap_or(histogram.histogram_bins);
    PlotSpec::empty("").with_panels(vec![
        panel_with(
            (0, 0),
            ("λ", "nm"),
            ("Absorbance", ""),
            vec![curve(
                Some(book1.spectrum_x),
                book1.spectrum_y,
                "Spectrum",
                0,
                PlotType::Line,
            )],
        ),
        panel_with(
            (0, 1),
            ("Time", "s"),
            ("Signal", "mV"),
            vec![curve(Some(m.time), m.decay, "Decay", 1, PlotType::Scatter)],
        ),
        panel_with((1, 0), ("Voltage", "mV"), ("Count", ""), vec![histogram]),
        panel_with(
            (1, 1),
            ("Sample", ""),
            ("Yield", "%"),
            vec![curve(Some(m.bars.0), m.bars.1, "Yield", 3, PlotType::Bar)],
        ),
    ])
}

/// Two cells side by side. On the left: the decay on a logarithmic scale (its
/// data are strictly positive, invariant 10) and the growth on the right
/// axis. On the right: a layer overlaid with secondary axes, with its own X
/// axis at the top and its own Y axis on the right.
fn axes(m: &Measures) -> PlotSpec {
    let mut left_panel = panel_with(
        (0, 0),
        ("Time", "s"),
        ("Decay", "mV"),
        vec![curve(
            Some(m.time),
            m.decay,
            "Decay",
            0,
            PlotType::LineScatter,
        )],
    );
    left_panel.left.scale = AxisScale::Log;
    left_panel.left.grid = true;
    let mut right_series = curve(Some(m.time), m.growth, "Growth", 1, PlotType::LineScatter);
    right_series.axis = SeriesAxis::Right;
    left_panel.series.push(right_series);
    left_panel.right = axis("Growth", "mV");

    let base = panel_with(
        (0, 1),
        ("Time", "s"),
        ("Growth", "mV"),
        vec![curve(Some(m.time), m.growth, "Growth", 1, PlotType::Line)],
    );
    let mut layer = PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES);
    layer.bottom.label = "Relative time".into();
    layer.bottom.unit = "min".into();
    layer.left.label = "Temperature".into();
    layer.left.unit = "°C".into();
    layer.series.push(curve(
        Some(m.time),
        m.oscillation,
        "Oscillation",
        3,
        PlotType::Line,
    ));
    PlotSpec::empty("").with_panels(vec![left_panel, base, layer])
}

/// One panel per plot type.
fn plot_types(m: &Measures) -> PlotSpec {
    let mut errors = curve(Some(m.time), m.decay, "Decay ± σ", 4, PlotType::ErrorBar);
    errors.y_error = Some(m.error);
    let mut histogram = curve(None, m.noise, "Noise", 2, PlotType::Histogram);
    histogram.histogram_bins = BinCount::new(30).unwrap_or(histogram.histogram_bins);
    let mut cells = vec![
        panel_with(
            (0, 0),
            ("Time", "s"),
            ("Signal", "mV"),
            vec![curve(
                Some(m.time),
                m.decay,
                "Scatter",
                0,
                PlotType::Scatter,
            )],
        ),
        panel_with(
            (0, 1),
            ("Time", "s"),
            ("Temperature", "°C"),
            vec![curve(
                Some(m.time),
                m.oscillation,
                "Line + Symbol",
                1,
                PlotType::LineScatter,
            )],
        ),
        panel_with(
            (0, 2),
            ("Sample", ""),
            ("Yield", "%"),
            vec![curve(Some(m.bars.0), m.bars.1, "Bars", 3, PlotType::Bar)],
        ),
        panel_with((1, 0), ("Voltage", "mV"), ("Count", ""), vec![histogram]),
        panel_with((1, 1), ("Time", "s"), ("Signal", "mV"), vec![errors]),
    ];
    for (cell, name) in
        cells
            .iter_mut()
            .zip(["Scatter", "LineScatter", "Bar", "Histogram", "ErrorBar"])
    {
        cell.title = name.into();
    }
    PlotSpec::empty("").with_panels(cells)
}

/// The `Book1` spectrum annotated: Typst text, rectangle, ellipse, line,
/// arrow, and a legend moved into a corner.
fn annotations(book1: &Book1) -> PlotSpec {
    let mut panel = panel_with(
        (0, 0),
        ("λ", "nm"),
        ("Absorbance", ""),
        vec![curve(
            Some(book1.spectrum_x),
            book1.spectrum_y,
            "Spectrum",
            0,
            PlotType::Line,
        )],
    );
    panel.legend.position = LegendPosition::TopLeft;
    panel
        .annotations
        .push(AnnotationSpec::new("Maximum $lambda_1$", 580.0, 0.8));
    panel
        .annotations
        .push(AnnotationSpec::new("Shoulder $lambda_2$", 668.0, 0.74));

    let mut zone = ShapeSpec::new(ShapeKind::Rectangle, 500.0, 0.0, 540.0, 0.9);
    zone.line_color = "#4c78a8".into();
    zone.fill_color = "#4c78a833".into();
    let mut circle = ShapeSpec::new(ShapeKind::Ellipse, 585.0, 0.15, 635.0, 0.5);
    circle.line_color = "#e45756".into();
    let mut background = ShapeSpec::new(ShapeKind::Line, 400.0, 0.04, 700.0, 0.04);
    background.line_color = "#888888".into();
    let mut arrow = ShapeSpec::new(ShapeKind::Arrow, 662.0, 0.7, 618.0, 0.36);
    arrow.line_color = "#54a24b".into();
    arrow.line_width = 2.0;
    panel.shapes = vec![zone, circle, background, arrow];
    PlotSpec::empty("").with_panels(panel)
}

/// Six curves added **together**: they form a group, to which a color
/// gradient is applied.
fn groups(m: &Measures) -> anyhow::Result<PlotSpec> {
    let mut panel = panel_with(
        (0, 0),
        ("Reaction time", "h"),
        ("Conversion", "mol/L"),
        Vec::new(),
    );
    let series = m
        .families_y
        .iter()
        .enumerate()
        .map(|(rank, y)| {
            curve(
                Some(m.families_x),
                *y,
                &format!("Curve {}", rank + 1),
                rank,
                PlotType::Line,
            )
        })
        .collect();
    panel.push_series_batch(series);
    let group = *panel
        .group_ids()
        .first()
        .context("the batch of curves forms a group")?;
    anyhow::ensure!(
        panel.set_group_gradient(group, &["#2c7bb6", "#fdae61", "#d7191c"]),
        "color gradient of the curve group"
    );
    Ok(PlotSpec::empty("").with_panels(panel))
}

/// Three rows of the kinetics, plotted against the columns' x'.
fn rows(book1: &Book1) -> PlotSpec {
    let series = [0usize, 3, 7]
        .into_iter()
        .enumerate()
        .map(|(rank, row)| SeriesSpec {
            row: Some(row),
            ..curve(
                None,
                book1.kinetics,
                &format!("Row {}", row + 1),
                rank,
                PlotType::LineScatter,
            )
        })
        .collect();
    PlotSpec::empty("").with_panels(panel_with(
        (0, 0),
        ("Time", "s"),
        ("Absorbance", ""),
        series,
    ))
}

/// `Graph1`, right after `Book1` in the tree.
pub(super) fn graph1(project: &mut Project, book1: &Book1) -> anyhow::Result<()> {
    let root = project.root();
    project
        .add_graph(
            root,
            Some("Graph1"),
            demo_spec(book1.spectrum_x, book1.spectrum_y),
        )
        .context("demo graph")?;
    Ok(())
}

/// The six other graphs, at the root.
pub(super) fn others(project: &mut Project, book1: &Book1, m: &Measures) -> anyhow::Result<()> {
    let root = project.root();
    for (name, spec) in [
        ("Panels", panels(book1, m)),
        ("Axes", axes(m)),
        ("PlotTypes", plot_types(m)),
        ("Annotations", annotations(book1)),
        ("Groups", groups(m)?),
        ("Rows", rows(book1)),
    ] {
        project
            .add_graph(root, Some(name), spec)
            .with_context(|| format!("graph {name}"))?;
    }
    Ok(())
}

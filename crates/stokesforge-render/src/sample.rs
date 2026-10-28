//! Two demonstration graphs, built in code.
//!
//! # Why they live in a production crate
//!
//! The exit of phase R5 requires the end criterion to hold for a
//! **realistic** graph, and the agreement test to be about that one. It also
//! requires, since R4, that the figure can be *looked at*
//! (`cargo run -p stokesforge-render-svg --example figure`). These two needs
//! are about the **same** graph: "what one looks at is exactly what the test
//! compares" is what R4 had noted at the top of its example.
//!
//! Now the agreement test lives in `stokesforge-render-egui` (it needs both
//! backends) and the example in `stokesforge-render-svg` (it must not see
//! egui). Two crates that cannot depend on each other: the common graph
//! therefore goes down one level, exactly like the text measurement in R4
//! and `NodeId` in R3. Duplicating it would have guaranteed that it
//! diverges, and the divergence would have been about precisely what the
//! test claims to verify.
//!
//! It is not a development dependency because a `[dev-dependency]` does not
//! cross crates; it is public code, at no cost for whoever does not call it,
//! and there is **no example `.stf` file in the repository** it could be
//! taken from instead.
//!
//! # What the realistic graph exercises
//!
//! Everything R5 adds, in a single spec: five panels in a 2 × 3 grid (hence
//! one empty cell), the six plot types, a logarithmic scale, a secondary
//! axis, three line styles, four marker shapes, error bars, a legend in each
//! corner, and a **broken reference** — which must be visible, not
//! disappear.
//!
//! The titles are explicit on these witness figures: they exercise their
//! bands and the measurement of non-ASCII text in both backends. They are not
//! defaults of graph creation; the application's demo follows the
//! no-title default of user creations (#103).

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{CoreResult, NodeId, Project};
use stokesforge_plot::{
    AxisScale, AxisSpec, LegendPosition, LegendSpec, LineSpec, LineStyle, PanelSpec, Panels,
    PlotSpec, PlotType, SeriesAxis, SeriesSpec, SymbolShape, SymbolSpec,
};

/// A project and the graph it carries.
pub struct Sample {
    /// The project, which owns the columns referenced by the spec.
    pub project: Project,
    /// The graph.
    pub spec: PlotSpec,
}

/// The minimal graph of phase R4: one curve, two linear axes.
///
/// Kept as is — it remains the simplest case, the one that must keep
/// passing whatever the following phases add.
pub fn minimal() -> CoreResult<Sample> {
    let mut project = Project::new("Agreement")?;
    let sheet = worksheet(&mut project, "Book1", "Sheet1")?;

    let xs: Vec<f64> = (0..64).map(|i| 400.0 + i as f64 * 5.0).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| gaussian(*x, 520.0, 40.0, 0.9) + 0.05)
        .collect();
    let x = column(&mut project, sheet, "Lambda", &xs)?;
    let y = column(&mut project, sheet, "Absorbance", &ys)?;

    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = axis("λ", "nm");
    panel.left = axis("Measured absorbance", "u.a.");
    let mut series = SeriesSpec::new_xy(x, y);
    series.name = "Sample µ1".to_string();
    series.line = LineSpec {
        color: "#4c78a8".to_string(),
        ..LineSpec::default()
    };
    panel.series.push(series);

    let spec = PlotSpec {
        title: "Absorption spectrum — λ_max".to_string(),
        background: "#ffffff".to_string(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel);

    Ok(Sample { project, spec })
}

/// The realistic graph of phase R5 — five panels, six plot types.
///
/// The labels are deliberately non-ASCII ("λ", "⁻¹", "µ", "é"): that is where
/// Plotters' byte-based heuristic is most wrong, hence where the witness of
/// the agreement test bites best (`architecture-choisie.md` §4.4).
pub fn realistic() -> CoreResult<Sample> {
    let mut project = Project::new("Campaign")?;
    let sheet = worksheet(&mut project, "Measurements", "Spectra")?;

    // -- (0,0) two superimposed spectra, lines of different styles.
    let lambda: Vec<f64> = (0..=140).map(|i| 400.0 + i as f64 * 2.5).collect();
    let spectrum_a: Vec<f64> = lambda
        .iter()
        .map(|x| gaussian(*x, 520.0, 38.0, 0.85) + gaussian(*x, 610.0, 22.0, 0.32) + 0.04)
        .collect();
    let spectrum_b: Vec<f64> = lambda
        .iter()
        .map(|x| gaussian(*x, 545.0, 55.0, 0.62) + 0.06)
        .collect();
    let id_lambda = column(&mut project, sheet, "Lambda", &lambda)?;
    let id_a = column(&mut project, sheet, "AbsA", &spectrum_a)?;
    let id_b = column(&mut project, sheet, "AbsB", &spectrum_b)?;

    let mut spectra = PanelSpec::at(0, 0);
    spectra.title = "Spectra".to_string();
    spectra.bottom = axis("λ", "nm");
    spectra.left = axis("Absorbance", "u.a.");
    spectra.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopRight,
        anchor: None,
        ..LegendSpec::default()
    };
    spectra.series.push(SeriesSpec {
        name: "Rosé sample µ1".to_string(),
        line: line_spec("#4c78a8", LineStyle::Solid, 2.0),
        ..SeriesSpec::new_xy(id_lambda, id_a)
    });
    spectra.series.push(SeriesSpec {
        name: "Control".to_string(),
        line: line_spec("#f58518", LineStyle::Dash, 2.0),
        ..SeriesSpec::new_xy(id_lambda, id_b)
    });

    // -- (0,1) kinetics: a curve on the left, a scatter on the right.
    let time: Vec<f64> = (0..=60).map(|i| i as f64 * 0.5).collect();
    let concentration: Vec<f64> = time.iter().map(|t| 1.0 * (-t / 8.0).exp()).collect();
    let temperature: Vec<f64> = time
        .iter()
        .map(|t| 20.0 + 6.0 * (t / 4.0).sin() + t / 12.0)
        .collect();
    let id_time = column(&mut project, sheet, "Time", &time)?;
    let id_conc = column(&mut project, sheet, "Concentration", &concentration)?;
    let id_temp = column(&mut project, sheet, "Temperature", &temperature)?;

    let mut kinetics = PanelSpec::at(0, 1);
    kinetics.title = "Kinetics".to_string();
    kinetics.bottom = axis("t", "min");
    kinetics.left = axis("c", "mol·L⁻¹");
    kinetics.right = axis("θ", "°C");
    kinetics.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopRight,
        anchor: None,
        ..LegendSpec::default()
    };
    kinetics.series.push(SeriesSpec {
        name: "Concentration".to_string(),
        line: line_spec("#54a24b", LineStyle::Solid, 2.0),
        ..SeriesSpec::new_xy(id_time, id_conc)
    });
    kinetics.series.push(SeriesSpec {
        name: "Temperature".to_string(),
        plot_type: PlotType::Scatter,
        axis: SeriesAxis::Right,
        symbol: symbol_spec("#e45756", SymbolShape::Diamond, 6.0),
        line: line_spec("#e45756", LineStyle::None, 1.0),
        ..SeriesSpec::new_xy(id_time, id_temp)
    });

    // -- (0,2) calibration: logarithmic ordinate and error bars.
    let dose: Vec<f64> = (0..9).map(|i| 1.0 + i as f64 * 2.0).collect();
    let response: Vec<f64> = dose.iter().map(|d| 2.0 * 10f64.powf(d / 8.0)).collect();
    let uncertainty: Vec<f64> = response.iter().map(|r| r * 0.18).collect();
    let id_dose = column(&mut project, sheet, "Dose", &dose)?;
    let id_response = column(&mut project, sheet, "Response", &response)?;
    let id_uncertainty = column(&mut project, sheet, "Uncertainty", &uncertainty)?;

    let mut calibration = PanelSpec::at(0, 2);
    calibration.title = "Calibration".to_string();
    calibration.bottom = axis("dose", "µg");
    calibration.left = AxisSpec {
        scale: AxisScale::Log,
        ..axis("response", "cps")
    };
    calibration.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopLeft,
        anchor: None,
        ..LegendSpec::default()
    };
    calibration.series.push(SeriesSpec {
        name: "Series A".to_string(),
        plot_type: PlotType::ErrorBar,
        y_error: Some(id_uncertainty),
        line: line_spec("#72b7b2", LineStyle::Solid, 1.0),
        symbol: symbol_spec("#72b7b2", SymbolShape::Square, 7.0),
        ..SeriesSpec::new_xy(id_dose, id_response)
    });

    // -- (1,0) distribution: a histogram.
    let residuals: Vec<f64> = (0..400)
        .map(|i| {
            // Three equidistributed sequences with **distinct** irrational
            // steps, summed: a rough bell, perfectly deterministic. No
            // random generator may enter a graph whose output a test
            // compares to the pixel. The steps must not complete each other
            // to an integer either, otherwise their fractional parts cancel
            // out and the whole sample falls into a single bin — a mistake
            // made then fixed here, and visible at a glance on the
            // histogram.
            let i = i as f64;
            let a = (i * 0.754_877_666_2) % 1.0;
            let b = (i * 0.569_840_290_9) % 1.0;
            let c = (i * 0.324_717_957_2) % 1.0;
            (a + b + c - 1.5) * 2.5
        })
        .collect();
    let id_residuals = column(&mut project, sheet, "Residuals", &residuals)?;

    let mut distribution = PanelSpec::at(1, 0);
    distribution.title = "Distribution".to_string();
    distribution.bottom = axis("deviation", "u.a.");
    distribution.left = axis("count", "");
    distribution.legend = LegendSpec {
        visible: true,
        position: LegendPosition::TopLeft,
        anchor: None,
        ..LegendSpec::default()
    };
    distribution.series.push(SeriesSpec {
        name: "Residuals".to_string(),
        plot_type: PlotType::Histogram,
        line: line_spec("#b279a2", LineStyle::Solid, 1.0),
        ..SeriesSpec::new(id_residuals)
    });

    // -- (1,1) yield: bars, a connected scatter, and a broken reference
    //    that must be visible.
    let batch: Vec<f64> = (1..=7).map(|i| i as f64).collect();
    let yield_values: Vec<f64> = [72.0, 81.0, 68.0, 90.0, 77.0, 85.0, 79.0].to_vec();
    let id_batch = column(&mut project, sheet, "Batch", &batch)?;
    let id_yield_values = column(&mut project, sheet, "Yield", &yield_values)?;
    let vanished = column(&mut project, sheet, "Vanished", &[1.0, 2.0])?;
    project.remove(vanished)?;

    let mut yields = PanelSpec::at(1, 1);
    yields.title = "Yield per batch".to_string();
    yields.bottom = axis("batch", "");
    yields.left = axis("yield", "%");
    yields.legend = LegendSpec {
        visible: true,
        position: LegendPosition::BottomRight,
        anchor: None,
        ..LegendSpec::default()
    };
    yields.series.push(SeriesSpec {
        name: "Yield".to_string(),
        plot_type: PlotType::Bar,
        line: line_spec("#eeca3b", LineStyle::Solid, 1.0),
        ..SeriesSpec::new_xy(id_batch, id_yield_values)
    });
    yields.series.push(SeriesSpec {
        name: "Previous campaign".to_string(),
        plot_type: PlotType::LineScatter,
        line: line_spec("#9d755d", LineStyle::DashDot, 1.0),
        symbol: symbol_spec("#9d755d", SymbolShape::Triangle, 7.0),
        ..SeriesSpec::new(vanished)
    });

    let spec = PlotSpec {
        title: "Campaign of September 12 — series µ".to_string(),
        background: "#ffffff".to_string(),
        ..PlotSpec::empty("")
    }
    .with_panels(Panels::new([
        spectra,
        kinetics,
        calibration,
        distribution,
        yields,
    ]));

    Ok(Sample { project, spec })
}

/// A Gaussian, to make spectra that look like spectra.
fn gaussian(x: f64, center: f64, width: f64, height: f64) -> f64 {
    let u = (x - center) / width;
    height * (-u * u).exp()
}

fn axis(label: &str, unit: &str) -> AxisSpec {
    AxisSpec {
        label: label.to_string(),
        unit: unit.to_string(),
        grid: true,
        ..AxisSpec::default()
    }
}

fn line_spec(color: &str, style: LineStyle, width: f64) -> LineSpec {
    LineSpec {
        color: color.to_string(),
        style,
        width,
    }
}

fn symbol_spec(color: &str, shape: SymbolShape, size: f64) -> SymbolSpec {
    SymbolSpec {
        shape,
        size,
        color: color.to_string(),
        edge_color: String::new(),
    }
}

fn worksheet(project: &mut Project, book: &str, sheet: &str) -> CoreResult<NodeId> {
    let root = project.root();
    let book = project.add_empty_workbook(root, Some(book))?;
    project.add_worksheet(book, Some(sheet))
}

fn column(project: &mut Project, sheet: NodeId, name: &str, values: &[f64]) -> CoreResult<NodeId> {
    let column = Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )?;
    project.add_column(sheet, None, Some(name), column)
}

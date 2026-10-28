//! The demo workbooks: `Book1` (the example script's: its letters and values
//! do not change) and the `Workbooks` folder (the column types, the
//! measurements that feed the graphs, a matrix, a note).
//!
//! Everything is built on a fresh `Project`, **before** `Session::spawn`
//! (invariant 1: this is not a mutation of a session's model). Formula columns
//! are only *declared* here; their values, never written by hand (invariant
//! 11), come from the recalculation that `demo_session` requests at startup.

use anyhow::Context as _;
use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, NodeId, Project, ValueBlock,
};

use super::analysis::titled_column;
use super::noise::Noise;
use super::set_x_role;

/// The columns of `Book1` that the graphs plot.
pub(super) struct Book1 {
    pub(super) spectrum_x: NodeId,
    pub(super) spectrum_y: NodeId,
    /// A column of `Kinetics`: the "row" series designate the sheet by any of
    /// its columns.
    pub(super) kinetics: NodeId,
}

/// The columns of the `Measurements` workbook, which the `Types`, `Panels`,
/// `Axes` and `Groups` graphs plot.
pub(super) struct Measures {
    pub(super) time: NodeId,
    /// Strictly positive: it goes to a logarithmic scale (invariant 10).
    pub(super) decay: NodeId,
    /// Strictly positive too.
    pub(super) growth: NodeId,
    pub(super) error: NodeId,
    pub(super) noise: NodeId,
    pub(super) oscillation: NodeId,
    pub(super) bars: (NodeId, NodeId),
    pub(super) families_x: NodeId,
    pub(super) families_y: Vec<NodeId>,
}

/// `Book1`, as it has been since the early days of the demo.
///
/// It is built so that the model's twisted cases are visible right at
/// opening, not only in the tests: a missing numeric value (`NaN`), the two
/// text values that nothing tells apart on screen (`None` and `Some("")`),
/// and above all **columns of different lengths** — a sheet is not a
/// rectangular array.
/// The column names stay automatic (A, B, C…) so that the example script
/// designates the same column as the letter visible in the grid.
pub(super) fn book1(project: &mut Project) -> anyhow::Result<Book1> {
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .context("Book1 workbook")?;
    let sheet1 = project
        .tree()
        .child_by_name(book, "Sheet1")
        .context("reading Book1")?
        .context("Sheet1 created with the workbook")?;
    let a1 = project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_f64(&[0.0, 0.5, 1.0, 1.5, 2.0]),
        )
        .context("column A of Sheet1")?;
    set_x_role(project, a1)?;
    project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_f64(&[10.0, 12.5, f64::NAN, 18.0, 21.5]),
        )
        .context("column B of Sheet1")?;
    project
        .add_column(
            sheet1,
            None,
            None,
            Column::from_text(&[Some("a".to_string()), None, Some(String::new())]),
        )
        .context("column C of Sheet1")?;
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .context("Sheet2 sheet")?;
    project
        .add_column(sheet2, None, None, Column::from_f64(&[7.0, 8.0]))
        .context("column A of Sheet2")?;
    // Two more empty sheets — Book1 showed only three, not enough to test by
    // eye the scrolling of the tab strip (F8) once it no longer shrinks its
    // tabs.
    project
        .add_worksheet(book, Some("Sheet3"))
        .context("Sheet3 sheet")?;
    project
        .add_worksheet(book, Some("Sheet4"))
        .context("Sheet4 sheet")?;

    // The graph of phase R4. Its columns are **its own** and not A/B of
    // Sheet1: those deliberately carry a `NaN` and unequal lengths to test
    // the grid, which would give a curve that is hard to read. A bench must
    // show the model's twisted cases *and* a rendering that can be judged by
    // eye.
    let spectrum = project
        .add_worksheet(book, Some("Spectrum"))
        .context("Spectrum sheet")?;
    let xs: Vec<f64> = (0..=120).map(|i| 400.0 + i as f64 * 2.5).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| {
            let a = (x - 520.0) / 38.0;
            let b = (x - 610.0) / 22.0;
            0.85 * (-a * a).exp() + 0.32 * (-b * b).exp() + 0.04
        })
        .collect();
    let x = project
        .add_column(spectrum, None, None, Column::from_f64(&xs))
        .context("column A of Spectrum")?;
    set_x_role(project, x)?;
    let y = project
        .add_column(spectrum, None, None, Column::from_f64(&ys))
        .context("column B of Spectrum")?;
    // A kinetics to test row dragging (#166): one column per instant (row x' =
    // time in s), one row per wavelength. Selecting row numbers then grabbing
    // the block edge plots one curve per row, against the x'.
    let kinetics = project
        .add_worksheet(book, Some("Kinetics"))
        .context("Kinetics sheet")?;
    let mut last = None;
    for time in [0.0, 30.0, 60.0, 120.0, 240.0] {
        let values: Vec<f64> = (0..8)
            .map(|row| {
                let amplitude = 0.2 + 0.1 * row as f64;
                amplitude * (1.0 - (-time / (60.0 + 20.0 * row as f64)).exp())
            })
            .collect();
        let column = project
            .add_column(kinetics, None, None, Column::from_f64(&values))
            .context("column of Kinetics")?;
        let mut meta = project
            .tree()
            .node(column)
            .and_then(|node| node.as_column())
            .map(|column| column.meta().clone())
            .unwrap_or_default();
        meta.x_prime = Some(time);
        meta.unit = String::new();
        project
            .set_column_meta(column, meta)
            .context("x' of Kinetics")?;
        last = Some(column);
    }
    Ok(Book1 {
        spectrum_x: x,
        spectrum_y: y,
        kinetics: last.context("Kinetics has columns")?,
    })
}

/// Sets the metadata on `column` through `modify`, without touching the rest.
fn apply_meta(
    project: &mut Project,
    column: NodeId,
    modify: impl FnOnce(&mut ColumnMeta),
) -> anyhow::Result<()> {
    let mut meta = project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(|c| c.meta().clone())
        .context("column to annotate")?;
    modify(&mut meta);
    project
        .set_column_meta(column, meta)
        .context("column metadata")?;
    Ok(())
}

/// A formula column: the column is created empty, then the formula (typed in
/// **letters**) is resolved against the columns already present in the sheet.
fn formula_column(
    project: &mut Project,
    sheet: NodeId,
    text: &str,
    title: &str,
) -> anyhow::Result<NodeId> {
    let column = project
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .context("formula column")?;
    let formula = project
        .tree()
        .formula_from_letters(column, text)
        .with_context(|| format!("formula {text}"))?;
    apply_meta(project, column, |meta| {
        meta.formula = Some(formula);
        meta.long_name = title.to_string();
    })?;
    Ok(column)
}

/// The `Types` workbook: one example of each column type and some formula
/// columns.
fn types(project: &mut Project, folder: NodeId) -> anyhow::Result<()> {
    let workbook = project
        .add_workbook(folder, Some("Types"))
        .context("Types workbook")?;
    let sheet = project
        .tree()
        .children(workbook)?
        .first()
        .copied()
        .context("sheet of the Types workbook")?;
    let time: Vec<f64> = (0..12).map(f64::from).collect();
    titled_column(project, sheet, &time, ("Time", "s"), ColumnRole::X)?;

    // Numeric, with missing values (`NaN`) and complete metadata: unit, long
    // name, comment and format.
    let nan = f64::NAN;
    let temperature = [
        20.5, 21.0, nan, 22.4, 23.1, nan, 24.0, 24.6, 25.3, 25.1, 26.0, 26.4,
    ];
    let column = titled_column(
        project,
        sheet,
        &temperature,
        ("Temperature", "°C"),
        ColumnRole::Y,
    )?;
    apply_meta(project, column, |meta| {
        meta.comment = "Sensor 2: two missing measurements (NaN).".into();
        meta.display_format = "0.00".into();
    })?;

    // Mixed, text only: `None` and `Some("")` display the same.
    let labels = [
        Some("sample A".to_string()),
        None,
        Some(String::new()),
        Some("sample D".to_string()),
        Some("A12".to_string()),
    ];
    let column = project
        .add_column(sheet, None, None, Column::from_text(&labels))
        .context("mixed text column")?;
    apply_meta(project, column, |meta| {
        meta.long_name = "Label".into();
        meta.role = ColumnRole::Label;
    })?;

    // Mixed, blended: numbers, texts and empty cells in the same column.
    let readings = [
        MixedCell::Number(1.5),
        MixedCell::Text("n.d.".into()),
        MixedCell::Empty,
        MixedCell::Number(4.0),
        MixedCell::Text("<LOD".into()),
        MixedCell::Number(6.5),
        MixedCell::Empty,
        MixedCell::Number(8.0),
    ];
    let reading = project
        .add_column(sheet, None, None, Column::from_mixed(&readings))
        .context("blended mixed column")?;
    apply_meta(project, reading, |meta| {
        meta.long_name = "Manual reading".into();
        meta.unit = "mg/L".into();
    })?;

    // Date and time: POSIX seconds, one measurement per hour.
    let start = 1_790_841_600.0; // 2026-10-01 08:00 UTC
    let timestamp: Vec<f64> = (0..12).map(|i| start + 3600.0 * f64::from(i)).collect();
    let column = project
        .add_column(
            sheet,
            None,
            None,
            Column::from_block(
                ColumnType::DateTime,
                ValueBlock::Float(timestamp),
                ColumnMeta {
                    long_name: "Timestamp".into(),
                    ..ColumnMeta::default()
                },
            )
            .context("date and time column")?,
        )
        .context("date and time column")?;
    apply_meta(project, column, |meta| meta.role = ColumnRole::None)?;

    // The formulas read letters (A = Time, B = Temperature, D = reading).
    formula_column(project, sheet, "A*2+1", "2·t + 1")?;
    formula_column(project, sheet, "(B-A)/2", "(T − t) / 2")?;
    formula_column(project, sheet, "D+1", "Reading + 1")?;
    Ok(())
}

/// The `Measurements` workbook: the graphs' data (decay and growth strictly
/// positive for the logarithmic scale, error bars, histogram, bars, family of
/// six curves).
fn measures(project: &mut Project, folder: NodeId) -> anyhow::Result<Measures> {
    let mut noise = Noise::new(7);
    let workbook = project
        .add_workbook(folder, Some("Measurements"))
        .context("Measurements workbook")?;
    let sheet = project
        .tree()
        .children(workbook)?
        .first()
        .copied()
        .context("sheet of the Measurements workbook")?;
    let t: Vec<f64> = (1..=60).map(f64::from).collect();
    let decay: Vec<f64> = t
        .iter()
        .map(|t| 100.0 * (-t / 15.0).exp() * (1.0 + noise.gauss(0.03)))
        .collect();
    let growth: Vec<f64> = t
        .iter()
        .map(|t| 0.5 * t.powf(1.3) * (1.0 + noise.gauss(0.03)))
        .collect();
    let uncertainty: Vec<f64> = decay.iter().map(|v| 0.06 * v).collect();
    let histogram: Vec<f64> = (0..300).map(|_| 5.0 + noise.gauss(1.0)).collect();
    let oscillation: Vec<f64> = t.iter().map(|t| 25.0 + 20.0 * (t / 6.0).sin()).collect();

    let time = titled_column(project, sheet, &t, ("Time", "s"), ColumnRole::X)?;
    let decay = titled_column(project, sheet, &decay, ("Decay", "mV"), ColumnRole::Y)?;
    let growth = titled_column(project, sheet, &growth, ("Growth", "mV"), ColumnRole::Y)?;
    let error = titled_column(
        project,
        sheet,
        &uncertainty,
        ("Uncertainty", "mV"),
        ColumnRole::YError,
    )?;
    let noise_col = titled_column(
        project,
        sheet,
        &histogram,
        ("Detector noise", "mV"),
        ColumnRole::Y,
    )?;
    let oscillation = titled_column(
        project,
        sheet,
        &oscillation,
        ("Oscillation", "°C"),
        ColumnRole::Y,
    )?;

    let bars = project
        .add_worksheet(workbook, Some("Bars"))
        .context("Bars sheet")?;
    let numbers: Vec<f64> = (1..=8).map(f64::from).collect();
    let yields = [62.0, 71.0, 55.0, 80.0, 68.0, 90.0, 47.0, 76.0];
    let bars_x = titled_column(project, bars, &numbers, ("Sample", ""), ColumnRole::X)?;
    let bars_y = titled_column(project, bars, &yields, ("Yield", "%"), ColumnRole::Y)?;

    let families = project
        .add_worksheet(workbook, Some("Families"))
        .context("Families sheet")?;
    let x: Vec<f64> = (0..=50).map(|i| f64::from(i) * 0.2).collect();
    let families_x = titled_column(project, families, &x, ("Reaction time", "h"), ColumnRole::X)?;
    let mut families_y = Vec::new();
    for (rank, tau) in [0.8, 1.5, 2.5, 4.0, 6.0, 9.0].into_iter().enumerate() {
        let y: Vec<f64> = x
            .iter()
            .map(|x| (1.0 + 0.2 * rank as f64) * (1.0 - (-x / tau).exp()))
            .collect();
        families_y.push(titled_column(
            project,
            families,
            &y,
            ("Conversion", "mol/L"),
            ColumnRole::Y,
        )?);
    }
    Ok(Measures {
        time,
        decay,
        growth,
        error,
        noise: noise_col,
        oscillation,
        bars: (bars_x, bars_y),
        families_x,
        families_y,
    })
}

/// The text of the note: what the demo contains and where to look for it.
const NOTE: &str = "\
StokesForge demo project.\n\
\n\
Book1 / Graph1: the example script's workbook (Sheet1!A, a missing value, \
columns of different lengths, a kinetics sheet to drag rows from).\n\
Workbooks: Types (one example of each column type, formulas), Measurements \
(the graphs' data), a matrix, this note.\n\
Analysis: one workbook per computation family (baseline, correction, integral, \
fits, peaks, reconvolution, deconvolution); Integrate, FitPolynomial and \
FitGaussian already have their calculation, visible in the Results pane.\n\
Graphs: Panels, Axes, PlotTypes, Annotations, Groups, Rows.\n\
\n\
Everything is built by the code (no file); the noise has a fixed seed, so it \
is the same at every launch.";

/// The `Workbooks` folder: `Types`, `Measurements`, a matrix and the note.
pub(super) fn folder(project: &mut Project) -> anyhow::Result<Measures> {
    let root = project.root();
    let folder = project
        .add_folder(root, "Workbooks")
        .context("Workbooks folder")?;
    types(project, folder)?;
    let measures = measures(project, folder)?;
    project
        .add_matrix(folder, Some("Matrix"))
        .context("demo matrix")?;
    project
        .add_note(folder, Some("ReadMe"), NOTE)
        .context("demo note")?;
    Ok(measures)
}

//! Battery 16 — what a series becomes when its columns do not have the same
//! length.
//!
//! The rule is decided and documented (`resolve.rs`): "the plot covers the
//! **common prefix** instead of being refused". It is carried over as is from
//! `legacy-python/src/stokesforge/plot/resolve.py`, whose comment even names the
//! targeted case: "this is the common case of an error column shorter than the
//! signal". **Nothing below is therefore a regression of the port** — it is an
//! inherited decision whose consequence was never quantified.
//!
//! What this battery sets against that rule is the other promise of the same
//! module: a series must never be "silently empty", and the degraded state must
//! be **detectable**. The two do not hold together.

use stokesforge_core::resolve::{resolve_series, MissingRole};
use stokesforge_core::{Column, ColumnType, MixedCell, NodeId, Project};
use stokesforge_plot::{PlotType, SeriesSpec};

/// `[Book1]Sheet1` with `Y` of `points` rows, plus the error columns that the
/// test asks for.
struct Bench {
    project: Project,
    sheet: NodeId,
    y: NodeId,
}

impl Bench {
    fn new(points: usize) -> Self {
        let mut project = Project::new("Demo").expect("valid project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("workbook");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
        let values: Vec<f64> = (0..points).map(|i| i as f64).collect();
        let y = project
            .add_column(sheet, None, Some("Y"), Column::from_f64(&values))
            .expect("column Y");
        Self { project, sheet, y }
    }

    fn column(&mut self, name: &str, values: &[f64]) -> NodeId {
        self.project
            .add_column(self.sheet, None, Some(name), Column::from_f64(values))
            .expect("column")
    }

    fn empty(&mut self, name: &str) -> NodeId {
        self.project
            .add_column(
                self.sheet,
                None,
                Some(name),
                Column::new(ColumnType::Numeric),
            )
            .expect("empty column")
    }

    fn empty_mixed(&mut self, name: &str) -> NodeId {
        self.project
            .add_column(self.sheet, None, Some(name), Column::new(ColumnType::Mixed))
            .expect("empty mixed column")
    }

    /// Mixed column: lets numeric and non-plottable cells be mixed in the same
    /// column.
    fn mixed_column(&mut self, name: &str, cells: &[MixedCell]) -> NodeId {
        self.project
            .add_column(self.sheet, None, Some(name), Column::from_mixed(cells))
            .expect("mixed column")
    }

    fn series_with_error(&self, y_error: NodeId) -> SeriesSpec {
        SeriesSpec {
            plot_type: PlotType::ErrorBar,
            y_error: Some(y_error),
            ..SeriesSpec::new(self.y)
        }
    }
}

// -- guards ---------------------------------------------------------------

#[test]
fn b16_a_complete_series_resolves_entirely() {
    let mut bench = Bench::new(1000);
    let error = bench.column("dY", &vec![0.1; 1000]);
    let series = bench.series_with_error(error);
    let resolved = resolve_series(bench.project.tree(), &series);
    assert!(resolved.ok());
    assert!(!resolved.truncated);
    assert_eq!(resolved.point_count(), 1000);
}

#[test]
fn b16_a_deleted_error_column_is_a_detectable_broken_reference() {
    let mut bench = Bench::new(1000);
    let error = bench.column("dY", &vec![0.1; 1000]);
    let series = bench.series_with_error(error);
    bench.project.remove(error).expect("deletion");
    let resolved = resolve_series(bench.project.tree(), &series);
    assert!(resolved.broken(), "a broken reference must be detectable");
    assert_eq!(resolved.missing, vec![MissingRole::YError]);
    assert!(!resolved.ok());
}

#[test]
fn b16_a_complete_mixed_error_column_resolves_entirely() {
    // Mixed error column, but entirely numeric in practice (no text or empty
    // cell): it must behave like any numeric column once it has gone through
    // `numeric_view`.
    let mut bench = Bench::new(1000);
    let cells: Vec<MixedCell> = (0..1000)
        .map(|i| MixedCell::Number(i as f64 * 0.1))
        .collect();
    let error = bench.mixed_column("dY", &cells);
    let series = bench.series_with_error(error);
    let resolved = resolve_series(bench.project.tree(), &series);
    assert!(resolved.ok());
    assert!(!resolved.truncated);
    assert_eq!(resolved.point_count(), 1000);
}

#[test]
fn b16_a_mixed_column_with_text_resolves_to_the_common_prefix_with_nans() {
    // The motivating use case of the work package: a mixed column like X, half
    // as short as Y, with text and an empty cell mixed in with the numbers.
    // `numeric_view` translates text/empty into `NaN` (the renderer already
    // discards them); the truncation to the common prefix follows the same
    // rule as a homogeneous column.
    let mut bench = Bench::new(1000);
    let mut cells: Vec<MixedCell> = (0..500).map(|i| MixedCell::Number(i as f64)).collect();
    cells[10] = MixedCell::Text("n/a".to_string());
    cells[20] = MixedCell::Empty;
    let x = bench.mixed_column("X", &cells);
    let series = SeriesSpec {
        plot_type: PlotType::Scatter,
        x: Some(x),
        ..SeriesSpec::new(bench.y)
    };
    let resolved = resolve_series(bench.project.tree(), &series);
    assert!(
        resolved.ok(),
        "the series stays plottable despite the text/the empty cell"
    );
    assert!(
        resolved.truncated,
        "X (500 rows) is shorter than Y (1000): the common prefix must be reported"
    );
    assert_eq!(resolved.point_count(), 500);
}

#[test]
fn b16_an_entirely_text_error_column_resolves_without_a_single_point() {
    // There is no longer a non-plottable column type (`Text` has disappeared in
    // favor of `Mixed`, see `stokesforge_core::resolve`): an entirely text
    // column resolves its series normally, just without any numeric point —
    // it is no longer a broken reference.
    let mut bench = Bench::new(1000);
    let text = bench
        .project
        .add_column(
            bench.sheet,
            None,
            Some("dY"),
            Column::from_text(&[Some("0.1".to_string())]),
        )
        .expect("text column");
    let series = bench.series_with_error(text);
    let resolved = resolve_series(bench.project.tree(), &series);
    assert!(!resolved.broken());
    assert!(resolved.missing.is_empty());
    let y_error = resolved.y_error.expect("error ordinates resolved");
    assert!(y_error.as_slice().iter().all(|value| value.is_nan()));
}

// -- reproductions --------------------------------------------------------

#[test]
fn b16_a_short_error_column_does_not_truncate_the_data() {
    let mut bench = Bench::new(1000);
    let error = bench.column("dY", &[0.1, 0.2, 0.3]);
    let series = bench.series_with_error(error);
    let resolved = resolve_series(bench.project.tree(), &series);

    println!(
        "Y has 1000 points, dY has 3 → resolved series: {} points, ok = {}, truncated = {}, \
         missing = {:?}",
        resolved.point_count(),
        resolved.ok(),
        resolved.truncated,
        resolved.missing
    );
    assert_eq!(
        resolved.point_count(),
        1000,
        "error columns are per-point annotations, not coordinates: `x` and `y` go in \
         pairs and are truncated together, but a missing error bar should be `NaN` \
         (the model's missing value) and not shorten the signal. Here the curve is \
         drawn with 3 points out of 1000, and `ok()` returns `true`"
    );
}

#[test]
fn b16_a_short_mixed_error_column_does_not_truncate_the_data() {
    // Same anomaly as `b16_a_short_error_column_does_not_truncate_the_data`,
    // but with a mixed error column (text + numbers) rather than a homogeneous
    // one: going through `numeric_view` changes nothing about the common-prefix
    // truncation rule, good or bad.
    let mut bench = Bench::new(1000);
    let cells = vec![
        MixedCell::Number(0.1),
        MixedCell::Text("n/a".to_string()),
        MixedCell::Number(0.3),
    ];
    let error = bench.mixed_column("dY", &cells);
    let series = bench.series_with_error(error);
    let resolved = resolve_series(bench.project.tree(), &series);

    println!(
        "Y has 1000 points, dY (mixed) has 3 → resolved series: {} points, ok = {}, \
         truncated = {}, missing = {:?}",
        resolved.point_count(),
        resolved.ok(),
        resolved.truncated,
        resolved.missing
    );
    assert_eq!(
        resolved.point_count(),
        1000,
        "a mixed error column that is too short truncates the signal to 3 points out \
         of 1000, exactly like its homogeneous equivalent — the conversion by \
         `numeric_view` does not set this case apart from the already documented \
         anomaly"
    );
}

#[test]
fn b16_an_empty_mixed_error_column_does_not_make_the_series_disappear() {
    let mut bench = Bench::new(1000);
    let error = bench.empty_mixed("dY");
    let series = bench.series_with_error(error);
    let resolved = resolve_series(bench.project.tree(), &series);

    println!(
        "Y has 1000 points, dY (mixed) is empty → {} points, ok = {}, broken = {}, \
         missing = {:?}, truncated = {}",
        resolved.point_count(),
        resolved.ok(),
        resolved.broken(),
        resolved.missing,
        resolved.truncated
    );
    assert!(
        resolved.broken() || resolved.point_count() > 0,
        "same anomaly as the empty homogeneous error column, here with an empty mixed \
         column: the series silently disappears (zero points, `broken()` false)"
    );
}

#[test]
fn b16_an_empty_error_column_does_not_make_the_series_disappear() {
    let mut bench = Bench::new(1000);
    let error = bench.empty("dY");
    let series = bench.series_with_error(error);
    let resolved = resolve_series(bench.project.tree(), &series);

    println!(
        "Y has 1000 points, dY is empty → {} points, ok = {}, broken = {}, missing = {:?}, \
         truncated = {}",
        resolved.point_count(),
        resolved.ok(),
        resolved.broken(),
        resolved.missing,
        resolved.truncated
    );
    assert!(
        resolved.broken() || resolved.point_count() > 0,
        "the series disappeared — zero points — and `broken()` returns `false` with an \
         empty `missing`: `ResolvedPlot::broken_series()`, the only diagnostic the API \
         offers, will not list it. This is exactly the state that the top-level \
         documentation of `resolve.rs` promises to avoid: \"never a silently empty \
         series\". Adding an error column to a series and forgetting to fill it is \
         enough"
    );
}

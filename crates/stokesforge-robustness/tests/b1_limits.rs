//! Battery 1: boundary values of the business logic (huge allocations excluded;
//! they live in b6 so as not to kill the test machine).

use std::num::NonZeroU32;
use stokesforge_core::{Cell, Column, ColumnType, ColumnValues, MixedCell, Project, ValueBlock};
use stokesforge_plot::{PanelSpec, PlotSpec};

/// Builds a project with [Book1]Sheet1 and a column A holding `values`.
fn project_with_column(values: &[f64]) -> (Project, stokesforge_core::NodeId) {
    let mut project = Project::new("T").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let col = project
        .add_column(sheet, None, Some("A"), Column::from_f64(values))
        .unwrap();
    (project, col)
}

// -- histogram --

#[test]
fn b1_histogram_infinite_interval_produces_no_nan() {
    // Two finite values whose gap overflows f64: width = +inf.
    let values = [-f64::MAX, f64::MAX];
    let (edges, counts) = stokesforge_core::histogram(&values, NonZeroU32::new(4).unwrap());
    println!("edges = {edges:?}");
    println!("counts = {counts:?}");
    assert!(
        edges.iter().all(|e| e.is_finite()),
        "non-finite histogram edges: {edges:?}"
    );
    assert_eq!(
        counts.iter().sum::<f64>(),
        values.len() as f64,
        "points were lost or counted twice: {counts:?}"
    );
}

#[test]
fn b1_histogram_counts_all_finite_points() {
    let values = [1.0, 2.0, 3.0, 4.0, f64::NAN, f64::INFINITY];
    let (_edges, counts) = stokesforge_core::histogram(&values, NonZeroU32::new(3).unwrap());
    assert_eq!(counts.iter().sum::<f64>(), 4.0, "counts = {counts:?}");
}

#[test]
fn b1_histogram_empty_values() {
    let (edges, counts) = stokesforge_core::histogram(&[], NonZeroU32::new(5).unwrap());
    println!("empty -> edges={edges:?} counts={counts:?}");
    // Documented: (vec![0.0], vec![]). We only check that nothing panics and
    // that edges.len() == counts.len() + 1, as expected of a histogram.
    assert_eq!(
        edges.len(),
        counts.len() + 1,
        "histogram invariant broken: edges={edges:?}, counts={counts:?}"
    );
}

#[test]
fn b1_histogram_large_number_of_bins_does_not_panic() {
    // Huge bins: the count vector is allocated at that size.
    let values = [1.0, 2.0];
    let (edges, counts) = stokesforge_core::histogram(&values, NonZeroU32::new(1_000_000).unwrap());
    assert_eq!(edges.len(), 1_000_001);
    assert_eq!(counts.iter().sum::<f64>(), 2.0);
}

// -- bar_width --

#[test]
fn b1_bar_width_absurd_request_is_filtered() {
    // The doc says "the requested one if positive, else 80% of the median
    // step". We check that the documented contract holds.
    let x = [0.0, 1.0, 2.0, 3.0];
    for requested in [-5.0, 0.0, f64::NAN, f64::INFINITY] {
        let w = stokesforge_core::bar_width(Some(&x), Some(requested));
        println!("bar_width(requested={requested}) = {w}");
        assert!(
            w.is_finite() && w > 0.0,
            "unusable bar width ({w}) for a request of {requested}"
        );
    }
}

// -- PlotSpec grid --

#[test]
fn b1_plotspec_rows_columns_at_extreme_position() {
    let spec = PlotSpec {
        panels: vec![PanelSpec::at(usize::MAX, usize::MAX)].into(),
        ..PlotSpec::default()
    };
    // B2: `saturating_add(1)`: neither a panic nor an overflow to 0.
    println!("rows = {}", spec.rows());
    println!("columns = {}", spec.columns());
    assert_eq!(spec.rows(), usize::MAX);
    assert_eq!(spec.columns(), usize::MAX);
}

// -- column --

#[test]
fn b1_text_to_numeric_conversion_and_back() {
    let (mut project, col) = project_with_column(&[]);
    let texts: Vec<Option<String>> = vec![
        Some("1.5".into()),
        Some(" 2.5 ".into()),
        Some("".into()),
        None,
        Some("not a number".into()),
        Some("1e400".into()), // overflows f64
        Some("-0".into()),
        Some("inf".into()),
        Some("NaN".into()),
    ];
    project.set_column_type(col, ColumnType::Mixed).unwrap();
    let cells: Vec<MixedCell> = texts
        .iter()
        .cloned()
        .map(|value| match value {
            None => MixedCell::Empty,
            Some(text) => MixedCell::Text(text),
        })
        .collect();
    project
        .set_values(col, 0, ValueBlock::Mixed(cells))
        .unwrap();
    project.set_column_type(col, ColumnType::Numeric).unwrap();
    let tree = project.tree();
    let column = tree.node(col).unwrap().as_column().unwrap();
    let vals: Vec<f64> = match column.values() {
        ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!("unexpected type"),
    };
    println!("text -> numeric: {vals:?}");
    assert_eq!(vals.len(), texts.len(), "rows disappeared");
    // C2, decided: a finite text in full letters whose value overflows f64
    // ("1e400") is refused, never silently +inf; it falls back to the
    // model's missing value, exactly like "not a number".
    assert!(
        vals[5].is_nan(),
        "\"1e400\" overflows and must become the missing value (NaN), not {}",
        vals[5]
    );
    // A non-finiteness that is *explicitly requested* is not an overflow:
    // it is taken at its word.
    assert_eq!(
        vals[6].to_bits(),
        (-0.0_f64).to_bits(),
        "\"-0\" must stay -0.0"
    );
    assert_eq!(vals[7], f64::INFINITY, "\"inf\" must stay +inf");
    assert!(vals[8].is_nan(), "\"NaN\" must become the missing value");
}

// `b1_round_trip_numeric_text_numeric_preserves_values` was removed: it
// checked that a `Numeric → Text → Numeric` round trip (formatting by
// `format_f64_for_display`, then reparsing by `parse_f64_text`) was
// bit-for-bit faithful. There is no longer any conversion to a formatted
// text form: `Numeric → Mixed` is lossless (`MixedCell::Number`, never a
// string), so `Numeric → Mixed → Numeric` preserves the values *trivially*,
// without any formatting. The invariant that made this test worthwhile (the
// fidelity of a text round trip) no longer has a code path to cover.

#[test]
fn b1_empty_text_cell_is_distinct_from_missing() {
    let (mut project, col) = project_with_column(&[]);
    project.set_column_type(col, ColumnType::Mixed).unwrap();
    project
        .set_values(
            col,
            0,
            ValueBlock::Mixed(vec![MixedCell::Text(String::new()), MixedCell::Empty]),
        )
        .unwrap();
    let tree = project.tree();
    let column = tree.node(col).unwrap().as_column().unwrap();
    assert_eq!(column.get(0), Cell::Text(Some(String::new())));
    assert_eq!(column.get(1), Cell::Text(None));
    // Documented: both count as "missing" for display.
    assert!(column.is_missing(0));
    assert!(column.is_missing(1));
}

#[test]
fn b1_out_of_bounds_read_returns_missing_never_error() {
    let (project, col) = project_with_column(&[1.0]);
    let tree = project.tree();
    let column = tree.node(col).unwrap().as_column().unwrap();
    assert_eq!(column.get(usize::MAX), Cell::Float(f64::NAN));
    assert!(column.is_missing(usize::MAX));
}

#[test]
fn b1_delete_rows_count_max_does_not_overflow() {
    let mut project = Project::new("T").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let removed = project.delete_rows(sheet, 1, usize::MAX).unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].1.len(), 2);
    assert_eq!(project.tree().row_count(sheet).unwrap(), 1);
}

#[test]
fn b1_insert_rows_at_equal_to_length_appends_at_end_of_sheet() {
    // B5 (`guide-de-correction-robustesse.md` §F5): `at == len` is now a
    // valid target for `insert_rows` (not for `delete_rows`, which names no
    // existing row at that position): it is the only way to add a row at the
    // end of a sheet, the range that `Column::insert_rows` already accepts
    // per column (`at ∈ 0..=len`). This reproduction used to cover the
    // opposite bug ("ignored"); it now covers the guarantee that replaces it.
    let mut project = Project::new("T").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let touched = project.insert_rows(sheet, 2, 3).unwrap();
    assert_eq!(touched, vec![a]);
    assert_eq!(project.tree().row_count(sheet).unwrap(), 5);
}

#[test]
fn b1_version_counter_advances_on_every_write() {
    let (mut project, col) = project_with_column(&[1.0]);
    let v0 = project
        .tree()
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    project
        .set_values(col, 0, ValueBlock::Float(vec![2.0]))
        .unwrap();
    let v1 = project
        .tree()
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    assert_ne!(v0, v1, "the counter did not move after a write");
    // A refusal must not increment it.
    let _ = project.set_values(col, 0, ValueBlock::Mixed(vec![MixedCell::Empty]));
    let v2 = project
        .tree()
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    assert_eq!(v1, v2, "a refused write advanced the counter");
}

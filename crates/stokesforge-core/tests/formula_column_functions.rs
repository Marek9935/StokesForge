//! Tests of the column formula evaluator, part 2: the column functions
//! (`at`, `max`, `min`, `mean`, `sum`, `std`, `integral` and their range
//! forms).
//!
//! * (a) NIST StRD univariate summary statistics (`tests/data/nist-strd/`):
//!   `mean`, `std`, `sum` on the whole column and through the range path (the
//!   segment tree and Chan's merge);
//! * (b) `integral` against analytic references and edge cases;
//! * (c) `at`;
//! * (d) the statistics with ranges, empty cells, overflow.

use std::collections::BTreeMap;
use std::sync::Arc;

use proptest::prelude::*;
use stokesforge_core::formula::{evaluate_with_x, ColumnFormula, FormulaError};
use stokesforge_core::{ColumnValues, MixedCell, NodeId, NumericStatus};

// -- helpers --

fn col(index: u64) -> NodeId {
    NodeId::from_raw(index + 1)
}

const OWN: NodeId = NodeId::from_raw(999);

fn sheet() -> Vec<NodeId> {
    (0..30).map(col).collect()
}

fn float_column(values: &[f64]) -> ColumnValues {
    ColumnValues::Float(Arc::new(values.to_vec()))
}

/// Evaluates `src` over the columns `A`, `B`, ... of `data`; `pairs` lists
/// `(column, its X)` by position (`(1, 0)`: B reads its X in A).
fn eval(
    src: &str,
    data: &[&[f64]],
    pairs: &[(u64, u64)],
    rows: usize,
) -> (Vec<f64>, Vec<NumericStatus>) {
    let values: Vec<(NodeId, ColumnValues)> = data
        .iter()
        .enumerate()
        .map(|(i, v)| (col(i as u64), float_column(v)))
        .collect();
    eval_values(src, &values, pairs, rows)
}

fn eval_values(
    src: &str,
    values: &[(NodeId, ColumnValues)],
    pairs: &[(u64, u64)],
    rows: usize,
) -> (Vec<f64>, Vec<NumericStatus>) {
    let formula = ColumnFormula::from_letters(src, &sheet(), OWN)
        .and_then(|f| f.compile())
        .unwrap_or_else(|e: FormulaError| panic!("`{src}` refused: {e:?}"));
    let paired: BTreeMap<NodeId, NodeId> = pairs.iter().map(|(b, a)| (col(*b), col(*a))).collect();
    let result = evaluate_with_x(&formula, values, &paired, rows);
    assert_eq!(result.error, None, "`{src}`");
    (result.values, result.statuses)
}

/// `Some(value)` of row 0, `None` for an empty cell.
fn scalar(src: &str, data: &[&[f64]], pairs: &[(u64, u64)]) -> Option<f64> {
    let (values, _) = eval(src, data, pairs, 1);
    (!values[0].is_nan()).then_some(values[0])
}

/// B read against X = A.
fn on_xy(src: &str, x: &[f64], y: &[f64]) -> Option<f64> {
    scalar(src, &[x, y], &[(1, 0)])
}

fn ordered(x: f64) -> i64 {
    let bits = x.to_bits() as i64;
    if bits < 0 {
        i64::MIN - bits
    } else {
        bits
    }
}

fn ulps(a: f64, b: f64) -> u64 {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => 0,
        (true, false) | (false, true) => u64::MAX,
        _ if a == b => 0,
        _ => ordered(a).abs_diff(ordered(b)),
    }
}

fn rel_close(a: f64, b: f64, tolerance: f64) -> bool {
    a == b || (a - b).abs() <= tolerance * a.abs().max(b.abs())
}

/// Correct significant digits of `got` against `certified` (NIST "LRE"),
/// capped at 16.
fn digits(got: f64, certified: f64) -> f64 {
    if got == certified {
        return 16.0;
    }
    if certified == 0.0 {
        return (-got.abs().log10()).clamp(0.0, 16.0);
    }
    (-((got - certified).abs() / certified.abs()).log10()).clamp(0.0, 16.0)
}

/// Neumaier compensated sum: an accurate reference for sums.
fn exact_sum(values: &[f64]) -> f64 {
    let (mut sum, mut c) = (0.0, 0.0);
    for &v in values {
        let t = sum + v;
        c += if sum.abs() >= v.abs() {
            (sum - t) + v
        } else {
            (v - t) + sum
        };
        sum = t;
    }
    sum + c
}

/// Two-pass sample standard deviation with compensated sums.
fn reference_std(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let mean = exact_sum(values) / n;
    let dev: Vec<f64> = values.iter().map(|v| (v - mean) * (v - mean)).collect();
    (exact_sum(&dev) / (n - 1.0)).sqrt()
}

// -- (a) NIST StRD --

struct Nist {
    name: &'static str,
    mean: f64,
    std: f64,
    data: Vec<f64>,
}

const NIST_FILES: [&str; 9] = [
    "PiDigits", "Lottery", "Lew", "Mavro", "Michelso", "NumAcc1", "NumAcc2", "NumAcc3", "NumAcc4",
];

/// Exact (correctly rounded) sums of the data, computed with rational
/// arithmetic in CPython (`fractions.Fraction`), same order as `NIST_FILES`.
const NIST_SUMS: [f64; 9] = [
    22674.0,
    113133.0,
    -35487.0,
    100.0928,
    29985.24,
    30000006.0,
    1201.2,
    1001000200.2,
    10010000200.2,
];

/// Exact sample standard deviation of the **stored doubles** (not of the
/// decimal text): CPython `fractions.Fraction` for the sums and squares, 50
/// digit `decimal` for the root. It differs from the certified value by what
/// the conversion of the decimal data to doubles costs (`NumAcc4`: 5.6e-9).
const NIST_STD_OF_DOUBLES: [f64; 9] = [
    2.867339060288708,
    291.6997274709691,
    277.3321680443161,
    0.0004291234540030854,
    0.07901054781905066,
    1.0,
    0.09999999999999998,
    0.1000000000349246,
    0.10000000055879354,
];

fn load_nist(name: &'static str) -> Nist {
    let path = format!(
        "{}/tests/data/nist-strd/{name}.dat",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).expect("NIST file");
    let lines: Vec<&str> = text.lines().collect();
    let certified = |prefix: &str| -> f64 {
        lines[..60]
            .iter()
            .find(|l| l.starts_with(prefix))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("{name}: no `{prefix}`"))
    };
    let data: Vec<f64> = lines[60..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().parse().expect("data line"))
        .collect();
    Nist {
        name,
        mean: certified("Sample Mean"),
        std: certified("Sample Standard Deviation"),
        data,
    }
}

fn row_index(n: usize) -> Vec<f64> {
    (1..=n).map(|i| i as f64).collect()
}

/// Digits obtained, per path, against the certified values and against the
/// exact statistics of the stored doubles (`reference_std`).
struct NistResult {
    name: &'static str,
    /// Digits against the certified value, whole column.
    whole: [f64; 3],
    /// Same, range with constant bounds.
    constant_range: [f64; 3],
    /// Same, range with bounds read from columns (index queried per row).
    column_range: [f64; 2],
    /// `std` against the compensated two-pass reference on the doubles.
    std_vs_doubles: [f64; 3],
    /// Relative difference whole column vs range, for mean, std, sum.
    whole_vs_range: [f64; 3],
}

fn nist_results() -> Vec<NistResult> {
    NIST_FILES
        .iter()
        .zip(NIST_SUMS)
        .map(|(name, sum)| {
            let nist = load_nist(name);
            let n = nist.data.len();
            let x = row_index(n);
            let y = &nist.data;
            let whole = |f: &str| scalar(&format!("{f}(B)"), &[&x, y], &[]).unwrap();
            let hi = format!("{}", n + 10);
            let ranged = |f: &str| on_xy(&format!("{f}(B, 0, {hi})"), &x, y).unwrap();
            let zeros = vec![0.0; n];
            let highs = vec![(n + 10) as f64; n];
            let by_column = |f: &str| {
                scalar(&format!("{f}(B, C, D)"), &[&x, y, &zeros, &highs], &[(1, 0)]).unwrap()
            };
            let exact_std = NIST_STD_OF_DOUBLES[NIST_FILES.iter().position(|n| *n == *name).unwrap()];
            assert!(rel_close(exact_std, reference_std(y), 1e-9), "{name}");
            let rel = |a: f64, b: f64| ((a - b) / b).abs();
            let result = NistResult {
                name: nist.name,
                whole: [
                    digits(whole("mean"), nist.mean),
                    digits(whole("std"), nist.std),
                    digits(whole("sum"), sum),
                ],
                constant_range: [
                    digits(ranged("mean"), nist.mean),
                    digits(ranged("std"), nist.std),
                    digits(ranged("sum"), sum),
                ],
                column_range: [
                    digits(by_column("mean"), nist.mean),
                    digits(by_column("std"), nist.std),
                ],
                std_vs_doubles: [
                    digits(whole("std"), exact_std),
                    digits(ranged("std"), exact_std),
                    digits(by_column("std"), exact_std),
                ],
                whole_vs_range: [
                    rel(whole("mean"), ranged("mean")),
                    rel(whole("std"), ranged("std")),
                    rel(whole("sum"), ranged("sum")),
                ],
            };
            eprintln!(
                "{:9} whole [mean {:.2} std {:.2} sum {:.2}] const-range [{:.2} {:.2} {:.2}] column-range [{:.2} {:.2}] std vs doubles {:.2?} whole-vs-range {:?}",
                result.name,
                result.whole[0], result.whole[1], result.whole[2],
                result.constant_range[0], result.constant_range[1], result.constant_range[2],
                result.column_range[0], result.column_range[1],
                result.std_vs_doubles, result.whole_vs_range
            );
            result
        })
        .collect()
}

/// Best certified-`std` digits a double can give: the data are decimal
/// numbers rounded to a double (`NumAcc4`: 1e7 + 0.1 steps, a representation
/// error of 1e-9 on a deviation of 0.1, about 8 digits). Pinned just below
/// what the application gets.
const NIST_STD_FLOOR: [f64; 9] = [14.0, 15.0, 15.0, 12.0, 13.0, 15.0, 14.0, 9.0, 8.0];

#[test]
fn nist_statistics_reach_the_digits_a_double_allows_on_every_path() {
    for (r, floor) in nist_results().iter().zip(NIST_STD_FLOOR) {
        assert!(
            r.whole[0] >= 14.0 && r.whole[2] >= 14.0,
            "{}: whole mean/sum {:?}",
            r.name,
            r.whole
        );
        assert!(r.whole[1] >= floor, "{}: whole std {:?}", r.name, r.whole);
        assert!(
            r.constant_range
                .iter()
                .take(1)
                .chain(r.constant_range.iter().skip(2))
                .all(|d| *d >= 15.0),
            "{}: range mean/sum {:?}",
            r.name,
            r.constant_range
        );
        assert!(
            r.constant_range[1] >= floor - 0.5,
            "{}: range std {:?}",
            r.name,
            r.constant_range
        );
        assert!(
            r.column_range[0] >= 15.0 && r.column_range[1] >= floor - 0.5,
            "{}",
            r.name
        );
    }
}

/// Against the exact `std` of the stored doubles the application keeps 12
/// digits on every path (pinned at what it gets today).
#[test]
fn nist_std_stays_within_12_digits_of_the_exact_statistic_of_the_stored_doubles() {
    for r in nist_results() {
        assert!(
            r.std_vs_doubles.iter().all(|d| *d >= 12.0),
            "{}: {:?}",
            r.name,
            r.std_vs_doubles
        );
    }
}

/// A double allows 15 to 16 digits against the exact `std` of the doubles. The
/// whole-column path takes its mean from a naive running sum (NumAcc4: 12.3
/// digits), the range path merges rounded means with Chan's formula (Mavro
/// 12.6, Michelso 13.3 digits: an error of about 1e-16 * mean / std).
#[test]
#[ignore = "bug: #429: std loses up to 4 digits: naive mean in the whole path (NumAcc4 12.3), Chan merge of rounded means in the range path (Mavro 12.6), column_values.rs:76-86,111-117"]
fn nist_std_keeps_15_digits_of_the_exact_statistic_of_the_stored_doubles() {
    for r in nist_results() {
        assert!(
            r.std_vs_doubles.iter().all(|d| *d >= 15.0),
            "{}: {:?}",
            r.name,
            r.std_vs_doubles
        );
    }
}

#[test]
fn nist_whole_column_and_range_paths_agree_closely() {
    for r in nist_results() {
        assert!(
            r.whole_vs_range.iter().all(|d| *d < 1e-12),
            "{}: {:?}",
            r.name,
            r.whole_vs_range
        );
    }
}

/// `mean(B)` and `sum(B)` add left to right (`Agg::of_slice`): 14 digits on
/// NumAcc2/NumAcc4 where the range path (pairwise in the tree) gives 16.
#[test]
#[ignore = "bug: #429: whole-column mean/sum use a naive running sum, 14 digits on NumAcc2 and NumAcc4 where the range path gives 16, column_values.rs:76"]
fn nist_whole_column_mean_and_sum_have_15_digits() {
    for r in nist_results() {
        assert!(
            r.whole[0] >= 15.0 && r.whole[2] >= 15.0,
            "{}: {:?}",
            r.name,
            r.whole
        );
    }
}

/// A constant column: mean is the constant and std is 0.
#[test]
#[ignore = "bug: #429: naive running sum, mean of 1000 x 0.1 is not 0.1 and std of a constant column is not 0, column_values.rs:76"]
fn constant_column_has_its_value_as_mean_and_zero_std() {
    for (value, n) in [(0.1, 10usize), (0.1, 1000), (0.7, 1000), (1.1, 100_000)] {
        let y = vec![value; n];
        let mean = scalar("mean(B)", &[&y, &y], &[]).unwrap();
        assert_eq!(mean, value, "mean of {n} x {value}");
        let std = scalar("std(B)", &[&y, &y], &[]).unwrap();
        assert_eq!(std, 0.0, "std of {n} x {value}");
    }
}

/// Same through the range path (tree of partial sums).
#[test]
fn constant_column_through_the_range_path_stays_close() {
    for (value, n) in [(0.1, 10usize), (0.1, 1000), (0.7, 1000), (1.1, 100_000)] {
        let y = vec![value; n];
        let x = row_index(n);
        let mean = on_xy("mean(B, 0, 1e9)", &x, &y).unwrap();
        assert!(
            ulps(mean, value) <= 2,
            "range mean of {n} x {value} = {mean:e}"
        );
        let std = on_xy("std(B, 0, 1e9)", &x, &y).unwrap();
        assert!(std <= value * 1e-15, "range std of {n} x {value} = {std:e}");
    }
}

// -- (b) integral --

/// Independent trapezoid on the finite pairs, sorted by X (stable), clipped
/// to `[lo, hi]` with interpolated ends; `None` if fewer than two points or an
/// empty range. Compensated sum.
fn reference_integral(x: &[f64], y: &[f64], range: Option<(f64, f64)>) -> Option<f64> {
    let mut pairs: Vec<(f64, f64)> = x
        .iter()
        .zip(y)
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .map(|(a, b)| (*a, *b))
        .collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    if pairs.len() < 2 {
        return None;
    }
    let first = pairs[0].0;
    let last = pairs[pairs.len() - 1].0;
    let (lo, hi) = match range {
        Some((a, b)) => (a.min(b).max(first), a.max(b).min(last)),
        None => (first, last),
    };
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let at = |t: f64| -> f64 {
        let k = pairs.partition_point(|p| p.0 < t);
        if k == 0 {
            return pairs[0].1;
        }
        let (p0, p1) = (pairs[k - 1], pairs[k]);
        if p1.0 == t {
            p1.1
        } else {
            p0.1 + (p1.1 - p0.1) * (t - p0.0) / (p1.0 - p0.0)
        }
    };
    let mut pts = vec![(lo, at(lo))];
    pts.extend(pairs.iter().copied().filter(|p| p.0 > lo && p.0 < hi));
    pts.push((hi, at(hi)));
    let terms: Vec<f64> = pts
        .windows(2)
        .map(|w| 0.5 * (w[0].1 + w[1].1) * (w[1].0 - w[0].0))
        .collect();
    Some(exact_sum(&terms))
}

#[test]
fn integral_of_x_squared_on_a_uniform_grid_has_the_known_trapezoid_error() {
    let n = 101;
    let h = 0.01;
    let x: Vec<f64> = (0..n).map(|i| f64::from(i) * h).collect();
    let y: Vec<f64> = x.iter().map(|v| v * v).collect();
    let expected = 1.0 / 3.0 + h * h / 6.0;
    let got = on_xy("integral(B)", &x, &y).unwrap();
    assert!(rel_close(got, expected, 1e-13), "{got:e} vs {expected:e}");
    let got = on_xy("integral(B, 0, 1)", &x, &y).unwrap();
    assert!(rel_close(got, expected, 1e-13), "{got:e} vs {expected:e}");
}

#[test]
fn integral_starting_on_a_grid_point_uses_the_stored_value() {
    // at(1) over y = [1e20, 1, 1] must be 1, not 0 (#427).
    let got = on_xy("integral(B, 1, 2)", &[0.0, 1.0, 2.0], &[1e20, 1.0, 1.0]).unwrap();
    assert_eq!(got, 1.0);
}

#[test]
fn integral_of_sine_matches_the_reference_trapezoid_and_the_analytic_value() {
    let n = 1001;
    let x: Vec<f64> = (0..n)
        .map(|i| std::f64::consts::PI * f64::from(i) / 1000.0)
        .collect();
    let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
    let got = on_xy("integral(B)", &x, &y).unwrap();
    let reference = reference_integral(&x, &y, None).unwrap();
    assert!(rel_close(got, reference, 1e-13), "{got:e} vs {reference:e}");
    assert!((got - 2.0).abs() < 2e-6, "analytic 2, got {got}");
}

#[test]
fn integral_of_exp_on_a_non_uniform_grid_matches_the_reference_trapezoid() {
    let n = 400;
    let x: Vec<f64> = (0..=n)
        .map(|i| (f64::from(i) / f64::from(n)).powi(2))
        .collect();
    let y: Vec<f64> = x.iter().map(|v| v.exp()).collect();
    let got = on_xy("integral(B)", &x, &y).unwrap();
    let reference = reference_integral(&x, &y, None).unwrap();
    assert!(rel_close(got, reference, 1e-13), "{got:e} vs {reference:e}");
    assert!((got - (std::f64::consts::E - 1.0)).abs() < 1e-4);
}

#[test]
fn integral_of_a_line_is_exact_with_bounds_between_the_points() {
    // y = 2x + 1 on a non-uniform grid: the trapezoid is exact, also with
    // interpolated bounds.
    let x: Vec<f64> = [0.0, 0.1, 0.35, 0.4, 1.0, 1.7, 2.0, 3.5, 5.0].to_vec();
    let y: Vec<f64> = x.iter().map(|v| 2.0 * v + 1.0).collect();
    let primitive = |t: f64| t * t + t;
    for (a, b) in [
        (0.0, 5.0),
        (0.05, 0.2),
        (0.2, 4.4),
        (1.1, 1.2),
        (0.0, 0.1),
        (4.9, 5.0),
        (0.36, 0.39),
    ] {
        let expected = primitive(b) - primitive(a);
        let text = format!("integral(B, {a:e}, {b:e})");
        let got = on_xy(&text, &x, &y).unwrap();
        assert!(
            rel_close(got, expected, 1e-13),
            "{text}: {got:e} vs {expected:e}"
        );
        // Reversed bounds.
        let text = format!("integral(B, {b:e}, {a:e})");
        let got = on_xy(&text, &x, &y).unwrap();
        assert!(
            rel_close(got, expected, 1e-13),
            "{text}: {got:e} vs {expected:e}"
        );
    }
}

#[test]
fn integral_ignores_the_recording_order() {
    let x = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let y = [1.0, 4.0, 9.0, 16.0, 25.0, 36.0];
    let sorted = on_xy("integral(B)", &x, &y).unwrap();
    let rx: Vec<f64> = x.iter().rev().copied().collect();
    let ry: Vec<f64> = y.iter().rev().copied().collect();
    assert_eq!(on_xy("integral(B)", &rx, &ry).unwrap(), sorted);
    let order = [3, 0, 5, 1, 4, 2];
    let sx: Vec<f64> = order.iter().map(|&i| x[i]).collect();
    let sy: Vec<f64> = order.iter().map(|&i| y[i]).collect();
    assert_eq!(on_xy("integral(B)", &sx, &sy).unwrap(), sorted);
    assert_eq!(
        on_xy("integral(B, 0.5, 3.5)", &sx, &sy),
        on_xy("integral(B, 0.5, 3.5)", &x, &y)
    );
    assert_eq!(
        on_xy("integral(B, 3.5, 0.5)", &sx, &sy),
        on_xy("integral(B, 0.5, 3.5)", &x, &y)
    );
}

#[test]
fn integral_with_duplicate_x_counts_the_vertical_step_as_zero_area() {
    let x = [0.0, 1.0, 1.0, 2.0];
    let y = [0.0, 1.0, 5.0, 2.0];
    // 0.5 + 0 + 0.5 * (5 + 2).
    assert_eq!(on_xy("integral(B)", &x, &y), Some(4.0));
    assert_eq!(on_xy("integral(B, 0, 2)", &x, &y), Some(4.0));
    let zeros = [0.0; 4];
    let twos = [2.0; 4];
    assert_eq!(
        scalar("integral(B, C, D)", &[&x, &y, &zeros, &twos], &[(1, 0)]),
        Some(4.0)
    );
}

#[test]
fn integral_skips_rows_with_empty_or_non_finite_x_or_y() {
    let x = [0.0, 1.0, f64::NAN, 2.0, 3.0, f64::INFINITY, 4.0];
    let y = [0.0, 1.0, 5.0, f64::NAN, 3.0, 7.0, 4.0];
    // Left: (0,0) (1,1) (3,3) (4,4): the line y = x, area 8.
    assert_eq!(on_xy("integral(B)", &x, &y), Some(8.0));
    assert_eq!(on_xy("integral(B, 0, 4)", &x, &y), Some(8.0));
    assert_eq!(on_xy("integral(B, 1.5, 3.5)", &x, &y), Some(5.0));
}

#[test]
fn integral_with_text_cells_in_y_ignores_them() {
    let x = float_column(&[0.0, 1.0, 2.0, 3.0]);
    let y = ColumnValues::Mixed(Arc::new(vec![
        MixedCell::Number(0.0),
        MixedCell::Text("n/a".into()),
        MixedCell::Number(2.0),
        MixedCell::Empty,
    ]));
    let values = [(col(0), x), (col(1), y)];
    let (v, _) = eval_values("integral(B)", &values, &[(1, 0)], 1);
    assert_eq!(v[0], 2.0);
}

#[test]
fn integral_bounds_outside_the_data_are_clipped_and_empty_ranges_are_empty() {
    let x = [1.0, 2.0, 3.0];
    let y = [1.0, 2.0, 3.0];
    let whole = on_xy("integral(B)", &x, &y).unwrap();
    assert_eq!(whole, 4.0);
    assert_eq!(on_xy("integral(B, -5, 100)", &x, &y), Some(4.0));
    assert_eq!(on_xy("integral(B, 100, -5)", &x, &y), Some(4.0));
    assert_eq!(on_xy("integral(B, 2.5, 100)", &x, &y), Some(1.375));
    assert_eq!(on_xy("integral(B, 10, 20)", &x, &y), None);
    assert_eq!(on_xy("integral(B, -20, -10)", &x, &y), None);
    // A zero-width range holds no area: an empty cell, like an empty range.
    assert_eq!(on_xy("integral(B, 2, 2)", &x, &y), None);
    // Infinite bounds are refused (empty), not read as "to the end".
    assert_eq!(on_xy("integral(B, 0, 1/0)", &x, &y), None);
}

#[test]
fn integral_needs_two_points() {
    assert_eq!(on_xy("integral(B)", &[1.0], &[1.0]), None);
    assert_eq!(on_xy("integral(B, 0, 2)", &[1.0], &[1.0]), None);
    assert_eq!(on_xy("integral(B)", &[], &[]), None);
    assert_eq!(
        on_xy("integral(B)", &[f64::NAN, f64::NAN], &[1.0, 2.0]),
        None
    );
    assert_eq!(
        on_xy("integral(B)", &[1.0, 1.0], &[1.0, 2.0]),
        None,
        "two points at the same X: zero width"
    );
    assert_eq!(on_xy("integral(B)", &[0.0, 2.0], &[1.0, 3.0]), Some(4.0));
}

#[test]
fn integral_overflow_is_an_empty_cell() {
    assert_eq!(on_xy("integral(B)", &[0.0, 1e300], &[1e300, 1e300]), None);
}

#[test]
fn integral_on_a_descending_x_column() {
    let x = [5.0, 4.0, 3.0, 2.0, 1.0, 0.0];
    let y: Vec<f64> = x.iter().map(|v| v * v).collect();
    let ascending: Vec<f64> = x.iter().rev().copied().collect();
    let ay: Vec<f64> = y.iter().rev().copied().collect();
    assert_eq!(
        on_xy("integral(B)", &x, &y),
        on_xy("integral(B)", &ascending, &ay)
    );
}

/// `integral(B, a, b)` with literal bounds goes through the Integration
/// tool, with bounds read per row through the cumulative index: both must
/// give the same area.
fn random_xy() -> impl Strategy<Value = (Vec<f64>, Vec<f64>)> {
    prop::collection::vec(
        (
            prop_oneof![
                8 => (0i32..200).prop_map(|v| f64::from(v) / 10.0),
                1 => Just(f64::NAN),
            ],
            prop_oneof![
                8 => (-1000i32..1000).prop_map(|v| f64::from(v) / 10.0),
                1 => Just(f64::NAN),
                1 => Just(f64::INFINITY),
            ],
        ),
        0..40,
    )
    .prop_map(|rows| rows.into_iter().unzip())
}

fn bound() -> impl Strategy<Value = f64> {
    (-50i32..250).prop_map(|v| f64::from(v) / 10.0 + 0.013)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1500, failure_persistence: None, rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha, rng_seed: proptest::test_runner::RngSeed::Fixed(412), ..ProptestConfig::default() })]

    #[test]
    fn integral_with_literal_bounds_equals_integral_with_column_bounds(
        (x, y) in random_xy(), lo in bound(), hi in bound()
    ) {
        let n = x.len().max(1);
        let los = vec![lo; n];
        let his = vec![hi; n];
        let literal = on_xy(&format!("integral(B, {lo:e}, {hi:e})"), &x, &y);
        let by_column = scalar("integral(B, C, D)", &[&x, &y, &los, &his], &[(1, 0)]);
        let reference = reference_integral(&x, &y, Some((lo, hi)));
        match (literal, by_column, reference) {
            (None, None, None) => {}
            (Some(a), Some(b), Some(r)) => {
                let scale = 1e-12 * (r.abs() + a.abs() + 1.0e3);
                prop_assert!((a - r).abs() <= scale, "literal {a:e} reference {r:e}");
                prop_assert!((b - r).abs() <= scale, "column {b:e} reference {r:e}");
            }
            other => prop_assert!(false, "literal / column / reference disagree on emptiness: {other:?}"),
        }
    }

    #[test]
    fn whole_column_integral_equals_the_reference((x, y) in random_xy()) {
        let got = on_xy("integral(B)", &x, &y);
        let reference = reference_integral(&x, &y, None);
        match (got, reference) {
            (None, None) => {}
            (Some(a), Some(r)) => prop_assert!((a - r).abs() <= 1e-12 * (r.abs() + 1.0e3), "{a:e} vs {r:e}"),
            other => prop_assert!(false, "emptiness differs: {other:?}"),
        }
    }

    #[test]
    fn statistics_in_a_range_equal_a_brute_force_filter(
        (x, y) in random_xy(), lo in bound(), hi in bound()
    ) {
        let (a, b) = (lo.min(hi), lo.max(hi));
        let picked: Vec<f64> = x.iter().zip(&y)
            .filter(|(x, y)| x.is_finite() && y.is_finite() && **x >= a && **x <= b)
            .map(|(_, y)| *y).collect();
        let n = x.len().max(1);
        let los = vec![hi; n];
        let his = vec![lo; n];
        for (name, expected) in [
            ("max", picked.iter().copied().reduce(f64::max)),
            ("min", picked.iter().copied().reduce(f64::min)),
            ("sum", (!picked.is_empty()).then(|| exact_sum(&picked))),
            ("mean", (!picked.is_empty()).then(|| exact_sum(&picked) / picked.len() as f64)),
            ("std", (picked.len() >= 2).then(|| reference_std(&picked))),
        ] {
            let literal = on_xy(&format!("{name}(B, {lo:e}, {hi:e})"), &x, &y);
            let by_column = scalar(&format!("{name}(B, C, D)"), &[&x, &y, &los, &his], &[(1, 0)]);
            for got in [literal, by_column] {
                match (got, expected) {
                    (None, None) => {}
                    (Some(g), Some(e)) => prop_assert!(
                        (g - e).abs() <= 1e-9 * (e.abs() + 1e-3),
                        "{name}: {g:e} vs {e:e} over {picked:?}"
                    ),
                    other => prop_assert!(false, "{name}: emptiness differs {other:?}"),
                }
            }
        }
    }
}

// -- (c) at --

#[test]
fn at_returns_the_data_at_grid_points_and_interpolates_between() {
    let x = [0.0, 1.0, 2.0, 4.0];
    let y = [10.0, 20.0, 15.0, 35.0];
    for (at, expected) in [
        (0.0, 10.0),
        (1.0, 20.0),
        (2.0, 15.0),
        (4.0, 35.0),
        (0.5, 15.0),
        (1.5, 17.5),
        (3.0, 25.0),
        (3.5, 30.0),
    ] {
        assert_eq!(
            on_xy(&format!("at(B, {at:e})"), &x, &y),
            Some(expected),
            "at {at}"
        );
    }
}

#[test]
fn at_outside_the_x_domain_is_an_empty_cell() {
    let x = [0.0, 1.0, 2.0];
    let y = [1.0, 2.0, 3.0];
    for at in ["-0.001", "2.001", "1e300", "-1e300", "1/0", "-1/0"] {
        assert_eq!(on_xy(&format!("at(B, {at})"), &x, &y), None, "at {at}");
    }
}

#[test]
fn at_with_one_point_answers_only_at_that_x() {
    assert_eq!(on_xy("at(B, 5)", &[5.0], &[7.0]), Some(7.0));
    assert_eq!(on_xy("at(B, 5.0000001)", &[5.0], &[7.0]), None);
    assert_eq!(on_xy("at(B, 5)", &[], &[]), None);
}

#[test]
fn at_averages_duplicate_x() {
    let x = [0.0, 1.0, 1.0, 1.0, 2.0];
    let y = [0.0, 3.0, 6.0, 9.0, 12.0];
    assert_eq!(on_xy("at(B, 1)", &x, &y), Some(6.0));
    assert_eq!(on_xy("at(B, 0.5)", &x, &y), Some(3.0));
    assert_eq!(on_xy("at(B, 1.5)", &x, &y), Some(9.0));
}

#[test]
fn at_treats_minus_zero_and_zero_as_the_same_x() {
    let x = [-0.0, 0.0, 1.0];
    let y = [2.0, 4.0, 10.0];
    assert_eq!(on_xy("at(B, 0)", &x, &y), Some(3.0));
    assert_eq!(on_xy("at(B, -0)", &x, &y), Some(3.0));
}

#[test]
fn at_with_unsorted_x_and_ignored_rows() {
    let x = [3.0, f64::NAN, 1.0, 2.0, 0.0, 9.0];
    let y = [30.0, 99.0, 10.0, 20.0, 0.0, f64::NAN];
    assert_eq!(on_xy("at(B, 2.5)", &x, &y), Some(25.0));
    assert_eq!(on_xy("at(B, 0.5)", &x, &y), Some(5.0));
    assert_eq!(
        on_xy("at(B, 9)", &x, &y),
        None,
        "the row of x = 9 has an empty y"
    );
}

#[test]
fn at_reads_the_abscissa_row_by_row_and_reports_the_row_status() {
    let x = [0.0, 1.0, 2.0];
    let y = [0.0, 10.0, 20.0];
    let query = ColumnValues::Mixed(Arc::new(vec![
        MixedCell::Number(0.5),
        MixedCell::Text("oops".into()),
        MixedCell::Empty,
        MixedCell::Number(1.5),
        MixedCell::Number(7.0),
    ]));
    let values = [
        (col(0), float_column(&x)),
        (col(1), float_column(&y)),
        (col(2), query),
    ];
    let (v, s) = eval_values("at(B, C)", &values, &[(1, 0)], 5);
    assert_eq!(v[0], 5.0);
    assert!(v[1].is_nan() && v[2].is_nan());
    assert_eq!(s[0], NumericStatus::Ok);
    assert_eq!(s[1], NumericStatus::Failed, "text abscissa");
    assert_eq!(s[2], NumericStatus::Missing, "empty abscissa");
    assert_eq!(v[3], 15.0);
    assert!(v[4].is_nan());
    assert_eq!(
        s[4],
        NumericStatus::Ok,
        "outside the domain is an empty cell, not a failure"
    );
}

#[test]
fn at_with_an_explicit_x_column_ignores_the_pairing() {
    // B is paired with A, but `at(B, 2.5, C)` reads X in C.
    let a = [100.0, 200.0, 300.0];
    let b = [1.0, 2.0, 3.0];
    let c = [0.0, 2.0, 4.0];
    assert_eq!(scalar("at(B, 3, C)", &[&a, &b, &c], &[(1, 0)]), Some(2.5));
    assert_eq!(scalar("at(B, 3, C)", &[&a, &b, &c], &[]), Some(2.5));
}

#[test]
fn at_without_a_paired_x_degrades_the_formula() {
    let formula = ColumnFormula::from_letters("at(B, 1)", &sheet(), OWN)
        .unwrap()
        .compile()
        .unwrap();
    let values = [
        (col(0), float_column(&[0.0])),
        (col(1), float_column(&[1.0])),
    ];
    let result = evaluate_with_x(&formula, &values, &BTreeMap::new(), 1);
    assert_eq!(result.error, Some(FormulaError::NoPairedX));
}

/// At a grid point the value must be the data: `at(B, x_i) = y_i`, bit for
/// bit ("1 on the 340 row" in the manual).
fn grid() -> impl Strategy<Value = (Vec<f64>, Vec<f64>)> {
    prop::collection::vec((1u32..1000, -1.0e6f64..1.0e6), 2..30).prop_map(|rows| {
        let mut rows = rows;
        rows.sort_by_key(|r| r.0);
        rows.dedup_by_key(|r| r.0);
        let x: Vec<f64> = rows.iter().map(|r| f64::from(r.0) * 0.37).collect();
        let y: Vec<f64> = rows.iter().map(|r| r.1).collect();
        (x, y)
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 500, failure_persistence: None, rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha, rng_seed: proptest::test_runner::RngSeed::Fixed(412), ..ProptestConfig::default() })]

    #[test]
    fn at_a_random_grid_point_returns_the_stored_value_exactly((x, y) in grid(), k in 0usize..29) {
        let k = k % x.len();
        let got = on_xy(&format!("at(B, {:e})", x[k]), &x, &y).unwrap();
        prop_assert_eq!(got, y[k]);
    }

    #[test]
    fn at_between_points_stays_between_the_neighbours((x, y) in grid(), t in 0.0f64..1.0, k in 0usize..29) {
        prop_assume!(x.len() >= 2);
        let k = k % (x.len() - 1);
        let at = x[k] + t * (x[k + 1] - x[k]);
        let got = on_xy(&format!("at(B, {at:e})"), &x, &y).unwrap();
        let (low, high) = (y[k].min(y[k + 1]), y[k].max(y[k + 1]));
        let slack = 1e-12 * (low.abs() + high.abs() + 1.0);
        prop_assert!(got >= low - slack && got <= high + slack, "{got} not in [{low}, {high}]");
    }
}

#[test]
fn at_a_grid_point_returns_the_stored_value_exactly() {
    // y_k + (y_{k+1} - y_k) * 1 is not y_{k+1} when the difference rounds.
    let x = [0.0, 1.0];
    for (y0, y1) in [(1e20, 1.0), (0.1, 0.3), (1e16, 1.0), (3.0, 1e-20)] {
        let got = on_xy("at(B, 1)", &x, &[y0, y1]).unwrap();
        assert_eq!(got, y1, "at(B, 1) over y = [{y0:e}, {y1:e}]");
    }
}

// -- (d) statistics --

#[test]
fn range_bounds_equal_reversed_and_partially_overlapping() {
    let x = [1.0, 2.0, 3.0, 4.0, 5.0];
    let y = [10.0, 40.0, 20.0, 30.0, 50.0];
    // Equal bounds: the points at that X only.
    assert_eq!(on_xy("max(B, 2, 2)", &x, &y), Some(40.0));
    assert_eq!(on_xy("sum(B, 3, 3)", &x, &y), Some(20.0));
    assert_eq!(on_xy("std(B, 3, 3)", &x, &y), None, "one point");
    assert_eq!(on_xy("mean(B, 2.5, 2.5)", &x, &y), None, "no point");
    // Inclusive bounds, any order.
    assert_eq!(on_xy("sum(B, 2, 4)", &x, &y), Some(90.0));
    assert_eq!(on_xy("sum(B, 4, 2)", &x, &y), Some(90.0));
    assert_eq!(on_xy("min(B, 2.5, 4.5)", &x, &y), Some(20.0));
    // Partial overlap with the data, and outside it.
    assert_eq!(on_xy("sum(B, 4, 100)", &x, &y), Some(80.0));
    assert_eq!(on_xy("sum(B, -100, 1)", &x, &y), Some(10.0));
    assert_eq!(on_xy("sum(B, 6, 100)", &x, &y), None);
    assert_eq!(on_xy("max(B, -100, 100)", &x, &y), Some(50.0));
    assert_eq!(on_xy("mean(B, -100, 100)", &x, &y), Some(30.0));
}

#[test]
fn infinite_or_empty_bounds_give_an_empty_cell() {
    let x = [1.0, 2.0, 3.0];
    let y = [1.0, 2.0, 3.0];
    assert_eq!(on_xy("sum(B, 0, 1/0)", &x, &y), None);
    assert_eq!(on_xy("sum(B, -1/0, 5)", &x, &y), None);
    // A NaN bound read per row: the row is empty (Missing), not a failure.
    let nan_bound = [f64::NAN, 0.0, 0.0];
    let hi = [5.0, 5.0, 5.0];
    let (v, s) = eval("sum(B, C, D)", &[&x, &y, &nan_bound, &hi], &[(1, 0)], 3);
    assert!(v[0].is_nan());
    assert_eq!(s[0], NumericStatus::Missing);
    assert_eq!(v[1], 6.0);
    // An infinite bound read per row: empty, status Ok.
    let inf_bound = [0.0, 0.0, f64::INFINITY];
    let (v, s) = eval("sum(B, C, D)", &[&x, &y, &inf_bound, &hi], &[(1, 0)], 3);
    assert_eq!(v[0], 6.0);
    assert!(v[2].is_nan());
    assert_eq!(s[2], NumericStatus::Ok);
}

#[test]
fn statistics_ignore_text_empty_and_infinite_cells() {
    let y = ColumnValues::Mixed(Arc::new(vec![
        MixedCell::Number(1.0),
        MixedCell::Text("n/a".into()),
        MixedCell::Number(3.0),
        MixedCell::Empty,
        MixedCell::Number(f64::INFINITY),
        MixedCell::Number(5.0),
    ]));
    let values = [(col(1), y)];
    for (src, expected) in [
        ("sum(B)", 9.0),
        ("mean(B)", 3.0),
        ("max(B)", 5.0),
        ("min(B)", 1.0),
        ("std(B)", 2.0),
    ] {
        let (v, s) = eval_values(src, &values, &[], 1);
        assert_eq!(v[0], expected, "{src}");
        assert_eq!(s[0], NumericStatus::Ok);
    }
    // inf - the infinite value is also ignored by max/min.
    let (v, _) = eval_values("max(B)", &values, &[], 1);
    assert!(v[0].is_finite());
}

#[test]
fn a_column_without_a_point_gives_an_empty_cell_never_zero() {
    let nan = [f64::NAN; 3];
    for f in ["sum", "mean", "max", "min", "std"] {
        assert_eq!(scalar(&format!("{f}(B)"), &[&nan, &nan], &[]), None, "{f}");
    }
    assert_eq!(scalar("sum(B)", &[&[], &[]], &[]), None);
    let (v, s) = eval("sum(B)", &[&nan, &nan], &[], 1);
    assert!(v[0].is_nan());
    assert_eq!(
        s[0],
        NumericStatus::Ok,
        "no point is an empty cell, not a failure"
    );
}

#[test]
fn std_is_the_sample_deviation_and_needs_two_points() {
    let y = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
    // Population deviation 2, sample deviation sqrt(32 / 7).
    assert!(
        ulps(
            scalar("std(B)", &[&y, &y], &[]).unwrap(),
            (32.0f64 / 7.0).sqrt()
        ) <= 1
    );
    assert_eq!(scalar("std(B)", &[&[5.0], &[5.0]], &[]), None);
    assert_eq!(
        scalar("std(B)", &[&[5.0, 5.0], &[5.0, 5.0]], &[]),
        Some(0.0)
    );
    assert_eq!(
        scalar("std(B)", &[&[1.0, f64::NAN], &[1.0, f64::NAN]], &[]),
        None
    );
}

#[test]
fn std_does_not_lose_digits_on_a_large_offset() {
    let y: Vec<f64> = (0..1000).map(|i| 1e9 + f64::from(i % 3)).collect();
    let x = row_index(1000);
    let expected = reference_std(&y);
    for got in [
        scalar("std(B)", &[&x, &y], &[]).unwrap(),
        on_xy("std(B, 0, 1e9)", &x, &y).unwrap(),
    ] {
        assert!(rel_close(got, expected, 1e-9), "{got} vs {expected}");
    }
}

#[test]
fn sum_overflow_is_an_empty_cell() {
    assert_eq!(scalar("sum(B)", &[&[1.0], &[1e308, 1e308]], &[]), None);
}

/// The mean of two finite numbers is finite: `mean` computes `sum / n`, so
/// the intermediate sum overflows and the cell is empty.
#[test]
#[ignore = "bug: #429: mean(B) of [1.7e308, 1.7e308] is empty (sum overflows before the division), column_values.rs:111,119"]
fn mean_of_huge_values_is_not_lost_to_an_overflowing_sum() {
    let y = [1.7e308, 1.7e308];
    assert_eq!(scalar("mean(B)", &[&y, &y], &[]), Some(1.7e308));
    let x = [1.0, 2.0];
    assert_eq!(on_xy("mean(B, 0, 3)", &x, &y), Some(1.7e308));
}

/// `std` of finite values whose squared deviations overflow is empty even
/// when the deviation itself is representable (1e200 scale).
#[test]
#[ignore = "bug: #429: std of [-1e200, 1e200] is empty although 1.41e200 is representable (squared deviations overflow), column_values.rs:76-86"]
fn std_of_values_with_large_squares_is_not_lost() {
    let y = [-1e200, 1e200];
    let got = scalar("std(B)", &[&y, &y], &[]);
    assert!(
        got.is_some_and(|g| rel_close(g, 2f64.sqrt() * 1e200, 1e-12)),
        "{got:?}"
    );
}

#[test]
fn a_column_function_nests_in_the_bounds_of_another() {
    let x = [1.0, 2.0, 3.0, 4.0];
    let y = [10.0, 20.0, 30.0, 40.0];
    assert_eq!(on_xy("max(B, 1, max(A))", &x, &y), Some(40.0));
    assert_eq!(on_xy("sum(B, 2, 1 + 1)", &x, &y), Some(20.0));
}

#[test]
fn normalisation_formula_has_one_at_the_reference_row() {
    let x: Vec<f64> = (0..50).map(|i| 300.0 + f64::from(i) * 2.0).collect();
    let y: Vec<f64> = x.iter().map(|v| 0.001 * v * v + 0.37).collect();
    let (v, _) = eval("B / at(B, 340)", &[&x, &y], &[(1, 0)], 50);
    let row = x.iter().position(|v| *v == 340.0).unwrap();
    assert_eq!(v[row], 1.0);
}

// -- performance bench --

/// Timings of a 1e6-row sheet; prints, never fails. Run with
/// `cargo test -p stokesforge-core --test formula_column_functions --release -- --ignored --nocapture perf`.
#[test]
#[ignore = "bench: prints timings, run with --release"]
fn per_row_bounds_scale_like_n_log_n() {
    for n in [100_000usize, 1_000_000] {
        let x: Vec<f64> = (0..n).map(|i| i as f64 * 0.001).collect();
        let y: Vec<f64> = x.iter().map(|v| v.sin() + 2.0).collect();
        for src in [
            "B * 2 + 1",
            "B / max(B)",
            "mean(B, A - 1, A + 1)",
            "std(B, A - 1, A + 1)",
            "integral(B, A - 1, A + 1)",
            "at(B, A + 0.0005)",
            "B - mean(B, A - 0.5, A + 0.5)",
        ] {
            let start = std::time::Instant::now();
            let (v, _) = eval(src, &[&x, &y], &[(1, 0)], n);
            eprintln!(
                "n = {n:>8}  {src:<32} {:>9.1?}  (row 1000 = {:.6})",
                start.elapsed(),
                v[1000]
            );
        }
    }
}

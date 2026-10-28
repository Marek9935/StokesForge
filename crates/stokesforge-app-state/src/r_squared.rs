//! The alert level of an R² (#288), **without egui**: the state says
//! "orange", "red" or "dialog box"; `stokesforge-app` picks the shade
//! (`theme::r_squared_color`) and paints the box.
//!
//! The levels accumulate: an R² below [`DIALOG_BELOW`] is also red and also
//! orange. The bounds are strict (exactly `0.95` does not warn); a negative
//! R² — a fit worse than a horizontal line — falls under the lowest level.
//! `NaN` (no value) warns of nothing: we do not alert on what we cannot
//! measure.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_core::{Calculation, Tree};

use crate::tr;

/// The key of the results that carries R² (fits and peaks).
pub const R_SQUARED_KEY: &str = "stat.r_squared";

/// Below this value (strict), R² is written in orange.
pub const ORANGE_BELOW: f64 = 0.95;
/// Below this value (strict), R² is written in red.
pub const RED_BELOW: f64 = 0.8;
/// Below this value (strict), R² also opens the dialog box, unless neither the
/// application preference nor the calculation's setting has turned it off.
pub const DIALOG_BELOW: f64 = 0.6;

/// The level reached by an R², from the mildest to the strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RSquaredLevel {
    /// No warning (good R², or `NaN`).
    Good,
    /// `R² < 0.95`.
    Orange,
    /// `R² < 0.8`.
    Red,
    /// `R² < 0.6`: red, and dialog box.
    Dialog,
}

impl RSquaredLevel {
    /// The level of `r_squared`.
    pub fn of(r_squared: f64) -> Self {
        if r_squared.is_nan() {
            Self::Good
        } else if r_squared < DIALOG_BELOW {
            Self::Dialog
        } else if r_squared < RED_BELOW {
            Self::Red
        } else if r_squared < ORANGE_BELOW {
            Self::Orange
        } else {
            Self::Good
        }
    }

    /// Is the value painted in color (orange or above)?
    pub fn is_warning(self) -> bool {
        self >= Self::Orange
    }

    /// Is the value painted red (red or above)?
    pub fn is_red(self) -> bool {
        self >= Self::Red
    }
}

/// The level of a computed value: that of its R² if `key` is
/// [`R_SQUARED_KEY`], otherwise [`RSquaredLevel::Good`].
pub fn level_of_value(key: &str, value: f64) -> RSquaredLevel {
    if key == R_SQUARED_KEY {
        RSquaredLevel::of(value)
    } else {
        RSquaredLevel::Good
    }
}

/// The R² of the calculation that cross the box level, per series, as bits
/// (an exact fingerprint: two recalculations with the same result do not warn
/// twice). Empty: no fault.
pub fn dialog_fingerprint(calc: &Calculation) -> Vec<(usize, u64)> {
    let mut out = Vec::new();
    for (rank, result) in calc.results.iter().enumerate() {
        for value in &result.values {
            if level_of_value(&value.key, value.value) == RSquaredLevel::Dialog {
                out.push((rank, value.value.to_bits()));
            }
        }
    }
    out
}

/// The number of **series** of the calculation whose R² crosses the box level
/// (#290): the unit of count is the series, one curve = one R².
pub fn low_series_count(calc: &Calculation) -> usize {
    calc.results
        .iter()
        .filter(|result| {
            result
                .values
                .iter()
                .any(|v| level_of_value(&v.key, v.value) == RSquaredLevel::Dialog)
        })
        .count()
}

/// The faulty series of a calculation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LowRSquaredGroup {
    pub kind: OperationKind,
    /// Always at least 1.
    pub count: usize,
}

/// Groups `(type, number of series)` by type, in the fixed order of
/// [`OperationKind::ALL`] (stable from one snapshot to the next); types with
/// no faulty series disappear.
pub fn group_by_kind(
    items: impl IntoIterator<Item = (OperationKind, usize)>,
) -> Vec<LowRSquaredGroup> {
    let mut totals = [0usize; OperationKind::ALL.len()];
    for (kind, count) in items {
        if let Some(rank) = OperationKind::ALL.iter().position(|k| *k == kind) {
            totals[rank] += count;
        }
    }
    OperationKind::ALL
        .iter()
        .zip(totals)
        .filter(|(_, count)| *count > 0)
        .map(|(kind, count)| LowRSquaredGroup { kind: *kind, count })
        .collect()
}

/// The low R² of the whole project (#290), grouped by type: all the
/// calculations that keep their values, whatever their `low_r2_dialog`
/// setting (the setting turns off the box, not the information). A "Show"
/// calculation keeps nothing: it does not count there.
pub fn project_groups(tree: &Tree) -> Vec<LowRSquaredGroup> {
    group_by_kind(
        tree.calculations()
            .values()
            .map(|calc| (calc.operation.kind(), low_series_count(calc))),
    )
}

/// The total of series of the groups.
pub fn total(groups: &[LowRSquaredGroup]) -> usize {
    groups.iter().map(|g| g.count).sum()
}

fn count_i64(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// "12 reconvolutions": the counter of a type, with its plural.
pub fn group_line(group: &LowRSquaredGroup) -> String {
    let count = count_i64(group.count);
    match group.kind {
        OperationKind::Baseline => tr!("low-r2-count-baseline", count = count),
        OperationKind::Correct => tr!("low-r2-count-correct", count = count),
        OperationKind::Integrate => tr!("low-r2-count-integrate", count = count),
        OperationKind::Fit => tr!("low-r2-count-fit", count = count),
        OperationKind::Peaks => tr!("low-r2-count-peaks", count = count),
        OperationKind::Reconvolution => tr!("low-r2-count-reconvolution", count = count),
        OperationKind::DirectDeconvolution => {
            tr!("low-r2-count-direct-deconvolution", count = count)
        }
        OperationKind::Smooth => tr!("low-r2-count-smooth", count = count),
    }
}

/// The text of the box and of the badge list: "R² < 0.6 for:", one line
/// "• N type" per group, then the pointer to the Results.
pub fn summary_text(groups: &[LowRSquaredGroup]) -> String {
    let mut text = tr!("low-r2-summary-head");
    for group in groups {
        text.push_str("\n• ");
        text.push_str(&group_line(group));
    }
    text.push('\n');
    text.push_str(&tr!("low-r2-summary-see-results"));
    text
}

/// The label of the status bar badge: "13 low R²".
pub fn badge_text(total: usize) -> String {
    tr!("status-low-r2-badge", count = count_i64(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_levels_have_strict_bounds() {
        use RSquaredLevel::*;
        let cases = [
            (1.0, Good),
            (0.95, Good),
            (0.9499, Orange),
            (0.8, Orange),
            (0.7999, Red),
            (0.6, Red),
            (0.5999, Dialog),
            (0.0, Dialog),
            (-3.0, Dialog),
            (f64::NEG_INFINITY, Dialog),
            (f64::NAN, Good),
        ];
        for (r2, expected) in cases {
            assert_eq!(RSquaredLevel::of(r2), expected, "R² = {r2}");
        }
    }

    #[test]
    fn the_levels_accumulate() {
        let dialog = RSquaredLevel::of(0.1);
        assert!(dialog.is_warning() && dialog.is_red());
        let orange = RSquaredLevel::of(0.9);
        assert!(orange.is_warning() && !orange.is_red());
        assert!(!RSquaredLevel::of(0.99).is_warning());
    }

    #[test]
    fn only_the_r_squared_key_is_judged() {
        assert_eq!(level_of_value("param.tau", 0.1), RSquaredLevel::Good);
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.1), RSquaredLevel::Dialog);
    }

    fn next(x: f64) -> f64 {
        f64::from_bits(x.to_bits() + 1)
    }

    fn previous(x: f64) -> f64 {
        f64::from_bits(x.to_bits() - 1)
    }

    /// The three thresholds, exactly and one ulp on each side: the bound is strict.
    #[test]
    fn each_level_is_strict_to_the_ulp() {
        use RSquaredLevel::*;
        let cases = [
            (ORANGE_BELOW, Good),
            (previous(ORANGE_BELOW), Orange),
            (next(ORANGE_BELOW), Good),
            (ORANGE_BELOW - f64::EPSILON, Orange),
            (ORANGE_BELOW + f64::EPSILON, Good),
            (RED_BELOW, Orange),
            (previous(RED_BELOW), Red),
            (next(RED_BELOW), Orange),
            (RED_BELOW - f64::EPSILON, Red),
            (RED_BELOW + f64::EPSILON, Orange),
            (DIALOG_BELOW, Red),
            (previous(DIALOG_BELOW), Dialog),
            (next(DIALOG_BELOW), Red),
            (DIALOG_BELOW - f64::EPSILON, Dialog),
            (DIALOG_BELOW + f64::EPSILON, Red),
        ];
        for (r2, expected) in cases {
            assert_eq!(RSquaredLevel::of(r2), expected, "R² = {r2:e}");
        }
        // The thresholds themselves are those of the issue.
        assert_eq!((ORANGE_BELOW, RED_BELOW, DIALOG_BELOW), (0.95, 0.8, 0.6));
    }

    #[test]
    fn extreme_and_special_values() {
        use RSquaredLevel::*;
        let cases = [
            (1.0, Good),
            (1.0 + f64::EPSILON, Good),
            (0.0, Dialog),
            (-0.0, Dialog),
            (-1.0, Dialog),
            (f64::MIN, Dialog),
            (f64::NEG_INFINITY, Dialog),
            (f64::INFINITY, Good),
            (f64::MAX, Good),
            (f64::NAN, Good),
            (-f64::NAN, Good),
            (f64::MIN_POSITIVE, Dialog),
        ];
        for (r2, expected) in cases {
            assert_eq!(RSquaredLevel::of(r2), expected, "R² = {r2:e}");
        }
    }

    /// The levels accumulate, level by level: `is_warning` and `is_red`
    /// for each of the four levels.
    #[test]
    fn the_accumulation_of_levels_is_exact() {
        use RSquaredLevel::*;
        let table = [
            (Good, false, false),
            (Orange, true, false),
            (Red, true, true),
            (Dialog, true, true),
        ];
        for (level, warns, red) in table {
            assert_eq!(level.is_warning(), warns, "{level:?}");
            assert_eq!(level.is_red(), red, "{level:?}");
        }
        assert!(Good < Orange && Orange < Red && Red < Dialog);
    }

    #[test]
    fn other_keys_are_ignored_even_with_a_very_low_value() {
        for key in ["param.tau", "area", "stat.r_squared_adj", "R_SQUARED", ""] {
            for value in [-5.0, 0.0, 0.3, f64::NEG_INFINITY] {
                assert_eq!(
                    level_of_value(key, value),
                    RSquaredLevel::Good,
                    "{key:?} = {value}"
                );
            }
        }
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.9), RSquaredLevel::Orange);
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.7), RSquaredLevel::Red);
        assert_eq!(level_of_value(R_SQUARED_KEY, 0.99), RSquaredLevel::Good);
        assert_eq!(R_SQUARED_KEY, "stat.r_squared");
    }

    fn result_value(key: &str, value: f64) -> stokesforge_core::ResultValue {
        stokesforge_core::ResultValue {
            key: key.to_string(),
            index: None,
            value,
            error: None,
        }
    }

    fn calculation(series: Vec<Vec<(&str, f64)>>) -> Calculation {
        use stokesforge_analysis::correct::CorrectOp;
        use stokesforge_analysis::ops::Operation;
        Calculation {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: Vec::new(),
            aux: None,
            outputs: Vec::new(),
            results: series
                .into_iter()
                .map(|values| stokesforge_core::SeriesResult {
                    values: values
                        .into_iter()
                        .map(|(k, v)| result_value(k, v))
                        .collect(),
                    ..Default::default()
                })
                .collect(),
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: true,
            stale: None,
        }
    }

    #[test]
    fn the_fingerprint_is_empty_without_an_r2_below_the_box_level() {
        let good = calculation(vec![
            vec![(R_SQUARED_KEY, 0.99), ("param.tau", 0.1)],
            vec![(R_SQUARED_KEY, DIALOG_BELOW)],
            vec![(R_SQUARED_KEY, f64::NAN)],
            vec![("param.tau", -9.0)],
            vec![(R_SQUARED_KEY, 0.7)],
        ]);
        assert!(dialog_fingerprint(&good).is_empty());
        assert_eq!(low_series_count(&good), 0);
        assert!(dialog_fingerprint(&calculation(Vec::new())).is_empty());
        assert!(dialog_fingerprint(&calculation(vec![Vec::new()])).is_empty());
    }

    #[test]
    fn the_fingerprint_carries_the_series_rank_and_the_bits_of_the_r2() {
        let calc = calculation(vec![
            vec![(R_SQUARED_KEY, 0.99)],
            vec![(R_SQUARED_KEY, 0.2), ("param.tau", 0.1)],
            vec![(R_SQUARED_KEY, -1.0)],
        ]);
        assert_eq!(
            dialog_fingerprint(&calc),
            vec![(1, 0.2f64.to_bits()), (2, (-1.0f64).to_bits())]
        );
        assert_eq!(low_series_count(&calc), 2);
    }

    #[test]
    fn the_fingerprint_is_stable_and_changes_on_the_slightest_bit() {
        let a = calculation(vec![vec![(R_SQUARED_KEY, 0.3)]]);
        let b = calculation(vec![vec![(R_SQUARED_KEY, 0.3)]]);
        assert_eq!(dialog_fingerprint(&a), dialog_fingerprint(&b));
        // Pin: the fingerprint is exact, a difference of 1e-12 counts as a new
        // result (the box reopens).
        let c = calculation(vec![vec![(R_SQUARED_KEY, 0.3 + 1e-12)]]);
        assert_ne!(dialog_fingerprint(&a), dialog_fingerprint(&c));
        // Same R² on another series: another fingerprint.
        let d = calculation(vec![Vec::new(), vec![(R_SQUARED_KEY, 0.3)]]);
        assert_ne!(dialog_fingerprint(&a), dialog_fingerprint(&d));
    }

    #[test]
    fn low_series_count_counts_series_not_values() {
        let calc = calculation(vec![
            vec![(R_SQUARED_KEY, 0.1), (R_SQUARED_KEY, 0.2)],
            vec![(R_SQUARED_KEY, 0.9)],
            vec![(R_SQUARED_KEY, 0.3)],
        ]);
        assert_eq!(low_series_count(&calc), 2);
        assert_eq!(dialog_fingerprint(&calc).len(), 3, "one entry per value");
    }
}

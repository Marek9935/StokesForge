//! One value computed over a whole column (#402): the engine of the formula
//! functions `at(B, x)`, `max(B)`, `mean(B, x1, x2)`, `integral(B)`...
//!
//! Pure calculation on plain `f64` (`NaN` = empty, text or invalid), like the
//! rest of the crate. Three pieces:
//!
//! - [`column_stat`]: a statistic of the finite values of one column, no X;
//! - [`AtTable`]: the (X, Y) pairs sorted by X, equal X merged by the mean of
//!   their Y, for the linear interpolation of `at`;
//! - [`ColumnIndex`]: the pairs sorted by X with a segment tree, so that a
//!   statistic or an integral over `[lo, hi]` costs `O(log n)` per query. A
//!   formula whose bounds are columns (`max(B, D, E)`) builds it **once** and
//!   queries it per row, never `O(n²)`.
//!
//! Rules shared by all of them: only **finite** values count (`NaN` and `±inf`
//! are ignored, like the Integration tool); no point means `None` (an empty
//! cell, `sum` and `integral` included, never 0); the bounds of a range come in
//! any order and are inclusive.
//!
//! The standard deviation is the **sample** one (divided by n - 1), computed
//! in two passes and merged with Chan's formula in the tree: never a
//! difference of sums of squares, which loses all its digits on data such as
//! 1e9 + 1, 1e9 + 2, 1e9 + 3. It needs two points.
//!
//! The integral over a range returns what [`crate::integrate::integrate`]
//! returns for the same range on the raw cleaned series (not deduplicated;
//! sorted by increasing X, so the recording direction does not matter): the
//! index reproduces its bounds exactly (interpolated ends, points strictly
//! inside) from a cumulative trapezoid sum, to rounding.

use crate::series::interpolate;

/// A statistic of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    Max,
    Min,
    Mean,
    Sum,
    /// Sample standard deviation (n - 1).
    Std,
}

/// Count, sum, mean, sum of squared deviations, extremes: what one segment of
/// values tells, and what two segments merge into.
#[derive(Debug, Clone, Copy)]
struct Agg {
    n: usize,
    sum: f64,
    mean: f64,
    m2: f64,
    min: f64,
    max: f64,
}

impl Agg {
    const EMPTY: Agg = Agg {
        n: 0,
        sum: 0.0,
        mean: 0.0,
        m2: 0.0,
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
    };

    fn leaf(y: f64) -> Agg {
        Agg {
            n: 1,
            sum: y,
            mean: y,
            m2: 0.0,
            min: y,
            max: y,
        }
    }

    /// Two passes over the values (mean, then squared deviations).
    fn of_slice(values: &[f64]) -> Agg {
        if values.is_empty() {
            return Agg::EMPTY;
        }
        let n = values.len();
        let sum: f64 = values.iter().sum();
        let mean = sum / n as f64;
        let m2 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
        Agg {
            n,
            sum,
            mean,
            m2,
            min: values.iter().copied().fold(f64::INFINITY, f64::min),
            max: values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        }
    }

    /// Chan's parallel formula: stable, no sum of squares.
    fn merge(self, other: Agg) -> Agg {
        if self.n == 0 {
            return other;
        }
        if other.n == 0 {
            return self;
        }
        let n = self.n + other.n;
        let (na, nb, nt) = (self.n as f64, other.n as f64, n as f64);
        let delta = other.mean - self.mean;
        Agg {
            n,
            sum: self.sum + other.sum,
            mean: self.mean + delta * nb / nt,
            m2: self.m2 + other.m2 + delta * delta * na * nb / nt,
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn value(&self, stat: Stat) -> Option<f64> {
        let value = match stat {
            _ if self.n == 0 => return None,
            Stat::Max => self.max,
            Stat::Min => self.min,
            Stat::Sum => self.sum,
            Stat::Mean => self.sum / self.n as f64,
            Stat::Std if self.n < 2 => return None,
            Stat::Std => (self.m2 / (self.n - 1) as f64).sqrt(),
        };
        value.is_finite().then_some(value)
    }
}

/// `stat` of the finite values of `ys`; `None` if there is none (or, for
/// `Std`, fewer than two).
pub fn column_stat(stat: Stat, ys: &[f64]) -> Option<f64> {
    let finite: Vec<f64> = ys.iter().copied().filter(|y| y.is_finite()).collect();
    Agg::of_slice(&finite).value(stat)
}

/// The finite `(x, y)` pairs of two parallel columns (the common length only),
/// sorted by increasing X (stable, `total_cmp`, like [`crate::series::Series::sorted`]).
fn sorted_pairs(x: &[f64], y: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut pairs: Vec<(f64, f64)> = x
        .iter()
        .zip(y)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(&x, &y)| (x, y))
        .collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    pairs.into_iter().unzip()
}

/// The points of `at(B, x)`: sorted by X, equal X (`==`, so -0 and +0) merged
/// into one point holding the mean of their Y.
#[derive(Debug, Clone, Default)]
pub struct AtTable {
    xs: Vec<f64>,
    ys: Vec<f64>,
}

impl AtTable {
    pub fn new(x: &[f64], y: &[f64]) -> Self {
        let (sx, sy) = sorted_pairs(x, y);
        let mut table = AtTable::default();
        let mut k = 0;
        while k < sx.len() {
            let mut end = k + 1;
            while end < sx.len() && sx[end] == sx[k] {
                end += 1;
            }
            table.xs.push(sx[k]);
            table
                .ys
                .push(sy[k..end].iter().sum::<f64>() / (end - k) as f64);
            k = end;
        }
        table
    }

    /// Number of distinct X.
    pub fn len(&self) -> usize {
        self.xs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }

    /// Linear interpolation at `at`; `None` outside the X domain (no
    /// extrapolation), on an empty table or for a non-finite `at`.
    pub fn at(&self, at: f64) -> Option<f64> {
        interpolate(&self.xs, &self.ys, at).filter(|v| v.is_finite())
    }
}

/// The finite pairs sorted by X, indexed for range queries.
#[derive(Debug, Clone, Default)]
pub struct ColumnIndex {
    xs: Vec<f64>,
    ys: Vec<f64>,
    /// Iterative segment tree: leaves at `n..2n`.
    tree: Vec<Agg>,
    /// `cum[k]`: trapezoid area from the first point to point `k`.
    cum: Vec<f64>,
}

impl ColumnIndex {
    pub fn new(x: &[f64], y: &[f64]) -> Self {
        let (xs, ys) = sorted_pairs(x, y);
        let n = xs.len();
        let mut tree = vec![Agg::EMPTY; 2 * n];
        for (k, &y) in ys.iter().enumerate() {
            tree[n + k] = Agg::leaf(y);
        }
        for k in (1..n).rev() {
            tree[k] = tree[2 * k].merge(tree[2 * k + 1]);
        }
        let mut cum = vec![0.0; n];
        for k in 1..n {
            cum[k] = cum[k - 1] + 0.5 * (ys[k] + ys[k - 1]) * (xs[k] - xs[k - 1]);
        }
        ColumnIndex { xs, ys, tree, cum }
    }

    /// Number of indexed points.
    pub fn len(&self) -> usize {
        self.xs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }

    /// Segment tree query over the leaves `[from, to)`.
    fn query(&self, from: usize, to: usize) -> Agg {
        let n = self.xs.len();
        let (mut lo, mut hi) = (from + n, to + n);
        let (mut left, mut right) = (Agg::EMPTY, Agg::EMPTY);
        while lo < hi {
            if lo & 1 == 1 {
                left = left.merge(self.tree[lo]);
                lo += 1;
            }
            if hi & 1 == 1 {
                hi -= 1;
                right = self.tree[hi].merge(right);
            }
            lo >>= 1;
            hi >>= 1;
        }
        left.merge(right)
    }

    /// `stat` of the Y of the points whose X is in `[lo, hi]` (either order,
    /// bounds included); `None` when there is no point, a non-finite bound, or
    /// (`Std`) a single point.
    pub fn stat(&self, stat: Stat, lo: f64, hi: f64) -> Option<f64> {
        if !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let from = self.xs.partition_point(|&x| x < lo);
        let to = self.xs.partition_point(|&x| x <= hi);
        if from >= to {
            return None;
        }
        self.query(from, to).value(stat)
    }

    /// Area under the curve over `[lo, hi]` (either order), clipped to the
    /// data domain, by the trapezoid method: the result of
    /// [`crate::integrate::integrate`] with `Under::Zero` and that range.
    /// `None` for fewer than two points, a non-finite bound, an empty range
    /// or a non-finite area.
    pub fn integral(&self, lo: f64, hi: f64) -> Option<f64> {
        let n = self.xs.len();
        if n < 2 || !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let (lo, hi) = (lo.max(self.xs[0]), hi.min(self.xs[n - 1]));
        if !(hi > lo) {
            return None;
        }
        let ylo = interpolate(&self.xs, &self.ys, lo)?;
        let yhi = interpolate(&self.xs, &self.ys, hi)?;
        // Points strictly inside `(lo, hi)`: `first..end`.
        let first = self.xs.partition_point(|&x| x <= lo);
        let end = self.xs.partition_point(|&x| x < hi);
        let area = if first >= end {
            0.5 * (ylo + yhi) * (hi - lo)
        } else {
            let last = end - 1;
            0.5 * (ylo + self.ys[first]) * (self.xs[first] - lo)
                + (self.cum[last] - self.cum[first])
                + 0.5 * (self.ys[last] + yhi) * (hi - self.xs[last])
        };
        area.is_finite().then_some(area)
    }

    /// Area under the whole curve.
    pub fn integral_all(&self) -> Option<f64> {
        Some(self.integral(*self.xs.first()?, *self.xs.last()?)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrate::{integrate, IntegrateSpec, Under};
    use crate::series::Series;

    /// Small deterministic generator (no dependency).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
    }

    /// Unsorted X with duplicates and gaps, Y with NaN, +-inf and negatives.
    fn random_data(rng: &mut Rng, n: usize) -> (Vec<f64>, Vec<f64>) {
        let mut x = Vec::new();
        let mut y = Vec::new();
        for _ in 0..n {
            let r = rng.next();
            x.push(if r < 0.05 {
                f64::NAN
            } else {
                // A coarse grid makes duplicates frequent; a jump makes gaps.
                (rng.next() * 20.0).floor() * 0.5 + if rng.next() < 0.1 { 50.0 } else { 0.0 }
            });
            let r = rng.next();
            y.push(if r < 0.05 {
                f64::NAN
            } else if r < 0.08 {
                f64::INFINITY
            } else if r < 0.1 {
                f64::NEG_INFINITY
            } else {
                rng.next() * 10.0 - 4.0
            });
        }
        (x, y)
    }

    fn naive_stat(stat: Stat, x: &[f64], y: &[f64], lo: f64, hi: f64) -> Option<f64> {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let ys: Vec<f64> = x
            .iter()
            .zip(y)
            .filter(|(x, y)| x.is_finite() && y.is_finite() && **x >= lo && **x <= hi)
            .map(|(_, &y)| y)
            .collect();
        if ys.is_empty() {
            return None;
        }
        let n = ys.len() as f64;
        let mean = ys.iter().sum::<f64>() / n;
        match stat {
            Stat::Max => Some(ys.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
            Stat::Min => Some(ys.iter().copied().fold(f64::INFINITY, f64::min)),
            Stat::Sum => Some(ys.iter().sum()),
            Stat::Mean => Some(mean),
            Stat::Std if ys.len() < 2 => None,
            Stat::Std => {
                Some((ys.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt())
            }
        }
    }

    fn close(a: Option<f64>, b: Option<f64>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).abs() <= 1e-10 * (1.0 + a.abs().max(b.abs())),
            _ => false,
        }
    }

    const STATS: [Stat; 5] = [Stat::Max, Stat::Min, Stat::Mean, Stat::Sum, Stat::Std];

    #[test]
    fn range_index_matches_a_brute_force_oracle() {
        let mut rng = Rng(7);
        for round in 0..40 {
            let (x, y) = random_data(&mut rng, 5 + round * 7);
            let index = ColumnIndex::new(&x, &y);
            for _ in 0..30 {
                // Bounds in any order, some outside the data, some equal.
                let a = rng.next() * 70.0 - 5.0;
                let b = if rng.next() < 0.1 {
                    a
                } else {
                    rng.next() * 70.0 - 5.0
                };
                for stat in STATS {
                    let got = index.stat(stat, a, b);
                    let want = naive_stat(stat, &x, &y, a, b);
                    assert!(close(got, want), "{stat:?} [{a}, {b}]: {got:?} vs {want:?}");
                }
            }
        }
    }

    #[test]
    fn whole_column_stat_matches_the_oracle() {
        let mut rng = Rng(11);
        for _ in 0..30 {
            let (x, y) = random_data(&mut rng, 40);
            let finite_x = vec![0.0; y.len()];
            for stat in STATS {
                let want = naive_stat(stat, &finite_x, &y, -1.0, 1.0);
                assert!(close(column_stat(stat, &y), want), "{stat:?}");
            }
            let _ = x;
        }
    }

    #[test]
    fn std_is_the_sample_one() {
        let y = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let got = column_stat(Stat::Std, &y).unwrap();
        assert!((got - 2.138089935299395).abs() < 1e-12, "{got}");
    }

    #[test]
    fn std_is_stable_on_large_offsets() {
        let y = [1e9 + 1.0, 1e9 + 2.0, 1e9 + 3.0];
        let got = column_stat(Stat::Std, &y).unwrap();
        assert!((got - 1.0).abs() < 1e-9, "{got}");
        // Through the tree too.
        let x = [1.0, 2.0, 3.0];
        let got = ColumnIndex::new(&x, &y).stat(Stat::Std, 1.0, 3.0).unwrap();
        assert!((got - 1.0).abs() < 1e-9, "{got}");
    }

    #[test]
    fn no_point_is_none_for_every_stat_sum_included() {
        for stat in STATS {
            assert_eq!(column_stat(stat, &[]), None);
            assert_eq!(column_stat(stat, &[f64::NAN, f64::INFINITY]), None);
            assert_eq!(ColumnIndex::new(&[1.0], &[1.0]).stat(stat, 5.0, 6.0), None);
        }
        assert_eq!(ColumnIndex::new(&[], &[]).integral_all(), None);
        // One point is a std of nothing.
        assert_eq!(column_stat(Stat::Std, &[3.0]), None);
        assert_eq!(column_stat(Stat::Sum, &[3.0]), Some(3.0));
    }

    #[test]
    fn at_interpolates_without_extrapolating() {
        let t = AtTable::new(&[339.5, 340.5], &[0.80, 0.84]);
        assert!((t.at(340.0).unwrap() - 0.82).abs() < 1e-12);
        assert_eq!(t.at(339.5), Some(0.80));
        assert_eq!(t.at(900.0), None);
        assert_eq!(t.at(300.0), None);
        assert_eq!(t.at(f64::NAN), None);
    }

    #[test]
    fn at_sorts_the_pairs() {
        let t = AtTable::new(&[3.0, 1.0, 2.0], &[30.0, 10.0, 20.0]);
        assert_eq!(t.at(2.5), Some(25.0));
    }

    #[test]
    fn at_merges_equal_x_by_the_mean() {
        let t = AtTable::new(&[1.0, 2.0, 2.0, 3.0], &[0.0, 1.0, 3.0, 4.0]);
        assert_eq!(t.len(), 3);
        assert_eq!(t.at(2.0), Some(2.0));
        assert_eq!(t.at(1.5), Some(1.0));
        // -0 and +0 are the same X.
        let t = AtTable::new(&[-0.0, 0.0, 1.0], &[2.0, 4.0, 5.0]);
        assert_eq!(t.len(), 2);
        assert_eq!(t.at(0.0), Some(3.0));
    }

    #[test]
    fn at_ignores_non_finite_pairs() {
        let t = AtTable::new(&[1.0, f64::NAN, 3.0, 4.0], &[1.0, 2.0, f64::INFINITY, 5.0]);
        assert_eq!(t.len(), 2);
        assert_eq!(t.at(2.5), Some(3.0));
    }

    fn tool_integral(x: &[f64], y: &[f64], range: Option<(f64, f64)>) -> Option<f64> {
        let cleaned = crate::series::clean(x, y).series;
        let spec = IntegrateSpec {
            ranges: range.into_iter().collect(),
            under: Under::Zero,
            cumulative: false,
            normalize: false,
        };
        integrate(&Series::new(cleaned.x, cleaned.y), &spec)
            .ok()
            .map(|r| r.total)
            .filter(|t| t.is_finite())
    }

    #[test]
    fn integral_matches_the_integration_tool() {
        let mut rng = Rng(23);
        for round in 0..40 {
            let (x, y) = random_data(&mut rng, 4 + round * 5);
            let index = ColumnIndex::new(&x, &y);
            assert!(close(index.integral_all(), tool_integral(&x, &y, None)));
            for _ in 0..30 {
                let a = rng.next() * 70.0 - 5.0;
                let b = rng.next() * 70.0 - 5.0;
                let got = index.integral(a, b);
                let want = tool_integral(&x, &y, Some((a, b)));
                assert!(close(got, want), "[{a}, {b}]: {got:?} vs {want:?}");
            }
            // Bounds exactly on the points, where duplicates sit.
            for &p in x.iter().filter(|p| p.is_finite()).take(10) {
                let got = index.integral(p, p + 3.0);
                let want = tool_integral(&x, &y, Some((p, p + 3.0)));
                assert!(close(got, want), "[{p}, +3]: {got:?} vs {want:?}");
            }
        }
    }

    #[test]
    fn integral_keeps_duplicate_x_and_ignores_the_recording_direction() {
        // Not deduplicated: 1 + 4 = 5 (a mean of the duplicates would give 4.5).
        let index = ColumnIndex::new(&[0.0, 1.0, 1.0, 3.0], &[0.0, 2.0, 4.0, 0.0]);
        assert_eq!(index.integral_all(), Some(5.0));
        let down = ColumnIndex::new(&[3.0, 2.0, 1.0], &[1.0, 1.0, 1.0]);
        assert_eq!(down.integral_all(), Some(2.0));
    }

    #[test]
    fn integral_of_nothing_is_none() {
        let one = ColumnIndex::new(&[1.0], &[1.0]);
        assert_eq!(one.integral_all(), None);
        let two = ColumnIndex::new(&[1.0, 2.0], &[1.0, 1.0]);
        assert_eq!(two.integral(5.0, 6.0), None);
        assert_eq!(two.integral(1.5, 1.5), None);
        assert_eq!(two.integral(f64::NAN, 2.0), None);
        assert_eq!(two.integral(2.0, 1.0), Some(1.0));
    }
}

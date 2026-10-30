//! Axis scales — task R5.2, "logarithmic" half.
//!
//! # The decision: transform the data, not the coordinate type
//!
//! Plotters can make a logarithmic axis by itself
//! ([`plotters::coord::combinators::LogRange`]), and that is **not** what is
//! done here. The reason lies in the work package's single pipeline
//! (`architecture-choisie.md` §8): `LogRange` changes the *type* of the
//! coordinate, hence the type of the `ChartContext`. With two axes, two
//! scales, and a secondary axis that has its own, the function that draws a
//! panel would have to exist in **eight** monomorphizations — or be cut into
//! eight branches each of which would redraw the same thing. This is exactly
//! the divergence the single pipeline exists to prevent, and it would come
//! in through the door of the type system.
//!
//! Here, a single coordinate type — linear `f64` — and the scale is applied
//! **to the values**: the plot area reasons in log₁₀, and only two places
//! know there is a scale, the input ([`forward`]) and the labeling
//! ([`format_tick`]). The price paid is real and fits in one line: the ticks
//! are **decades** (and their submultiples if Plotters sees fit to
//! interleave some), not the logarithmic minor ticks that `LogRange` can
//! place — those are drawn by the pipeline ([`minor_key_points`], #344). No
//! other piece of the rendering — series, bars, legend, secondary axis — has
//! to know which of the two scales is in force.
//!
//! # Unrepresentable values disappear, and it is said
//!
//! On a logarithmic scale, zero and negatives have no image. They are
//! **removed** from the plot, as `NaN`s already are (the base's missing
//! value). What matters is that it is done in the same place for both
//! backends, and that the framing ignores them: an entirely negative series
//! on a log axis draws nothing rather than making the figure fail.

use stokesforge_core::number_format::format_tick as format_linear_tick;
use stokesforge_plot::AxisScale;

/// Image of a value in the space where the plot area works.
///
/// `None` when the value has no image: non-finite, or zero/negative on a
/// logarithmic scale.
pub fn forward(scale: AxisScale, value: f64) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    match scale {
        AxisScale::Linear => Some(value),
        AxisScale::Log => {
            if value > 0.0 {
                let t = value.log10();
                t.is_finite().then_some(t)
            } else {
                None
            }
        }
    }
}

/// Back to the data space. Inverse of [`forward`] where the latter is
/// defined.
pub fn inverse(scale: AxisScale, t: f64) -> f64 {
    match scale {
        AxisScale::Linear => t,
        AxisScale::Log => 10f64.powf(t),
    }
}

/// Number of ticks Plotters aims for by default on an axis.
pub const DEFAULT_LABELS: usize = 11;

/// The most ticks an axis can ask for: one per decade on a logarithmic scale
/// ([`decade_label_count`]), Plotters' default otherwise. This is only the
/// upper bound of [`tick_count`], which is what every caller must use: the
/// count of a drawn axis depends on its length in pixels (#397).
pub fn max_label_count(scale: AxisScale, range: (f64, f64)) -> usize {
    match scale {
        AxisScale::Linear => DEFAULT_LABELS,
        AxisScale::Log => decade_label_count(range),
    }
}

/// Pixels an axis gives to one tick (#397). A horizontal axis is calibrated
/// so that the plot area of a default one-cell graph (480 x 360 pt) still
/// asks for the maximum count of ticks ([`DEFAULT_LABELS`]) on both axes.
pub const TICK_SPACING_X: f64 = 29.0;
/// As [`TICK_SPACING_X`] for a vertical axis, whose labels are one text height
/// tall and so can be packed closer.
pub const TICK_SPACING_Y: f64 = 20.0;
/// Free space kept between two neighbouring labels, in pixels.
pub const MIN_LABEL_GAP: f64 = 6.0;

/// The direction of an axis, which decides what a label measures along it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisDirection {
    /// Labels side by side: their width counts.
    Horizontal,
    /// Labels one above the other: their height counts.
    Vertical,
}

/// How many ticks to ask Plotters for on an axis of `length` pixels (#397),
/// the **single** count shared by the labels, the tick marks, the minor ticks,
/// the decimals and the "Label every" origin of that axis.
///
/// Starts from the density (one tick per [`TICK_SPACING_X`] or
/// [`TICK_SPACING_Y`] pixels, never above [`max_label_count`]), then removes
/// ticks as long as two neighbouring labels, measured by `extent` (their size
/// along the axis, in pixels) and kept [`MIN_LABEL_GAP`] apart, would touch.
/// A degenerate interval or length leaves the upper bound.
pub fn tick_count(
    scale: AxisScale,
    range: (f64, f64),
    length: f64,
    direction: AxisDirection,
    decimal_comma: bool,
    extent: &dyn Fn(&str) -> f64,
) -> usize {
    let cap = max_label_count(scale, range);
    let span = range.1 - range.0;
    if !length.is_finite() || length <= 0.0 || !span.is_finite() || span == 0.0 {
        return cap;
    }
    let spacing = match direction {
        AxisDirection::Horizontal => TICK_SPACING_X,
        AxisDirection::Vertical => TICK_SPACING_Y,
    };
    let mut count = cap.min(((length / spacing).floor() as usize).max(2));
    while count > 2 {
        let format = TickFormat::new(scale, range, count).with_decimal_comma(decimal_comma);
        let mut marks: Vec<(f64, f64)> = linear_key_points(range, count)
            .into_iter()
            .map(|t| {
                (
                    ((t - range.0) / span * length).abs(),
                    extent(&format.format(t)),
                )
            })
            .collect();
        marks.sort_by(|a, b| a.0.total_cmp(&b.0));
        let fits = marks
            .windows(2)
            .all(|pair| pair[1].0 - pair[0].0 >= (pair[0].1 + pair[1].1) / 2.0 + MIN_LABEL_GAP);
        if fits {
            break;
        }
        count -= 1;
    }
    count
}

/// The ticks Plotters will place on a linear interval.
pub fn linear_key_points(range: (f64, f64), labels: usize) -> Vec<f64> {
    use plotters::coord::ranged1d::{BoldPoints, Ranged};
    use plotters::coord::types::RangedCoordf64;
    if !range.0.is_finite() || !range.1.is_finite() {
        return Vec::new();
    }
    RangedCoordf64::from(range.0..range.1).key_points(BoldPoints(labels))
}

/// The minor ticks of an axis (#344), in the transformed space: those drawn
/// between the major ticks of [`linear_key_points`], never overlapping one.
///
/// Linear: `count` minor ticks between two major ones; without `count`, the
/// major step is cut in 5, or in 4 when its mantissa is 2 (0.2 → 0.05);
/// Plotters only produces the mantissas 1, 2 and 5, and a step of 0.4
/// between two minor ticks would be ugly. The minor ticks also extend beyond
/// the outermost major ticks, up to the axis bounds. Logarithmic (`count`
/// has no effect): the multiples 2 to 9 of each decade; when the major step
/// exceeds a decade, the interleaved decades. Empty below a decade of step
/// (narrow interval: no "round" mark to place), or if the interval is not
/// finite or is zero.
pub fn minor_key_points(
    scale: AxisScale,
    range: (f64, f64),
    count: Option<u8>,
    labels: usize,
) -> Vec<f64> {
    let (low, high) = if range.0 <= range.1 {
        range
    } else {
        (range.1, range.0)
    };
    if !low.is_finite() || !high.is_finite() || high <= low {
        return Vec::new();
    }
    let majors = linear_key_points(range, labels);
    let [first, second, ..] = majors[..] else {
        return Vec::new();
    };
    let step = (second - first).abs();
    if step <= 0.0 || !step.is_finite() {
        return Vec::new();
    }
    let tolerance = step * 1e-6;
    let is_major = |value: f64| {
        majors
            .iter()
            .any(|major| (major - value).abs() <= tolerance)
    };
    let inside = |value: f64| value >= low - tolerance && value <= high + tolerance;
    let mut points = Vec::new();
    match scale {
        AxisScale::Linear => {
            let exponent = step.log10().floor();
            let mantissa = step / 10f64.powf(exponent);
            let divisions = match count {
                Some(count) => f64::from(count.clamp(1, 99)) + 1.0,
                None if (mantissa - 2.0).abs() < 0.01 => 4.0,
                None => 5.0,
            };
            let sub = step / divisions;
            let origin = first.min(second);
            let from = ((low - origin) / sub).floor() as i64;
            let to = ((high - origin) / sub).ceil() as i64;
            for index in from..=to {
                let value = origin + index as f64 * sub;
                if inside(value) && !is_major(value) {
                    points.push(value);
                }
            }
        }
        AxisScale::Log => {
            if step < 0.99 {
                return Vec::new();
            }
            let lowest = low.floor() as i64 - 1;
            let highest = high.ceil() as i64;
            for decade in lowest..=highest {
                let decade = decade as f64;
                if step < 1.5 {
                    for multiple in 2..=9 {
                        let value = decade + f64::from(multiple).log10();
                        if inside(value) && !is_major(value) {
                            points.push(value);
                        }
                    }
                } else if inside(decade) && !is_major(decade) {
                    points.push(decade);
                }
            }
        }
    }
    points
}

/// The smallest number of decimals that writes all the ticks of an axis
/// without altering them: 0 when they are all integers.
fn decimals_for(points: &[f64]) -> usize {
    (0..=10)
        .find(|&decimals| {
            let factor = 10f64.powi(decimals as i32);
            points.iter().all(|value| {
                let scaled = value * factor;
                (scaled - scaled.round()).abs() <= 1e-6 * scaled.abs().max(1.0)
            })
        })
        .unwrap_or(10)
}

/// How to write the ticks of an axis: the same number of decimals
/// everywhere, none when all are integers ("0, 100, 200", not
/// "0.0, 100.0, 200.0" as Plotters' default formatting gives).
#[derive(Debug, Clone, Copy)]
pub struct TickFormat {
    scale: AxisScale,
    decimals: usize,
    comma: bool,
    /// One number in `every` is written, from `origin` in steps of `step`
    /// (#344); `every` at 1 writes them all.
    every: u32,
    origin: f64,
    step: f64,
}

impl TickFormat {
    /// The format of the axis of this interval (in the transformed space).
    pub fn new(scale: AxisScale, range: (f64, f64), labels: usize) -> Self {
        let decimals = match scale {
            AxisScale::Linear => decimals_for(&linear_key_points(range, labels)),
            AxisScale::Log => 0,
        };
        Self {
            scale,
            decimals,
            comma: false,
            every: 1,
            origin: 0.0,
            step: 0.0,
        }
    }

    /// Writes only one number in `every` (#344): the first of the major
    /// ticks of `range`, then one every `every`. 0 and 1 write them all.
    pub fn with_label_every(mut self, every: u8, range: (f64, f64), labels: usize) -> Self {
        let majors = linear_key_points(range, labels);
        if let [first, second, ..] = majors[..] {
            self.every = u32::from(every.max(1));
            self.origin = first;
            self.step = second - first;
        }
        self
    }

    /// Writes the comma as the decimal separator (#223, graph setting).
    pub fn with_decimal_comma(mut self, comma: bool) -> Self {
        self.comma = comma;
        self
    }

    /// The label of a tick, in the transformed space.
    pub fn format(&self, t: f64) -> String {
        if self.every > 1 && self.step != 0.0 {
            let index = ((t - self.origin) / self.step).round() as i64;
            if index.rem_euclid(i64::from(self.every)) != 0 {
                return String::new();
            }
        }
        let text = match self.scale {
            AxisScale::Linear => format_linear_tick(t, self.decimals),
            AxisScale::Log => format_decade(t),
        };
        if self.comma {
            text.replace('.', ",")
        } else {
            text
        }
    }
}

/// The ticks of an axis, in data space, with their label: what the graph
/// places, for a caller that paints by itself (the preview of the analysis
/// window, #261).
///
/// `range` is in data space, in display order (an inverted axis is given
/// `(top, bottom)`). The step comes from [`linear_key_points`] with
/// [`tick_count`] in the transformed space (`length` pixels long, `extent`
/// measuring a label along the axis), the format from [`TickFormat`]: the same
/// path as the pipeline, hence the same ticks as a graph with the same
/// framing. Empty if a bound has no image on the scale ([`forward`]).
pub fn axis_ticks(
    scale: AxisScale,
    range: (f64, f64),
    decimal_comma: bool,
    length: f64,
    direction: AxisDirection,
    extent: &dyn Fn(&str) -> f64,
) -> Vec<(f64, String)> {
    let (Some(low), Some(high)) = (forward(scale, range.0), forward(scale, range.1)) else {
        return Vec::new();
    };
    let transformed = (low, high);
    let count = tick_count(scale, transformed, length, direction, decimal_comma, extent);
    let format = TickFormat::new(scale, transformed, count).with_decimal_comma(decimal_comma);
    linear_key_points(transformed, count)
        .into_iter()
        .map(|t| (inverse(scale, t), format.format(t)))
        .collect()
}

/// Frames a linear axis of an overlaid layer (#174) so that its ticks fall
/// **opposite** those of the covered layer's axis.
///
/// `base` is the interval of the covered axis, `wanted` the one that would
/// only wrap the layer's data; both in the same direction. We look for a
/// "round" step and a position of the first tick such that the resulting
/// interval contains `wanted`, has as many ticks as `base` and places them
/// at the same fractions of the axis. Each candidate is checked against the
/// ticks Plotters will actually place: we do not guess its algorithm. `labels`
/// is the tick count of the covered axis ([`tick_count`]). `None`
/// if nothing fits (the caller then keeps `wanted`).
pub fn aligned_range(base: (f64, f64), wanted: (f64, f64), labels: usize) -> Option<(f64, f64)> {
    let inverted = base.0 > base.1;
    if (wanted.0 > wanted.1) != inverted {
        return None;
    }
    let (b0, b1) = if inverted { (base.1, base.0) } else { base };
    let (d0, d1) = if inverted {
        (wanted.1, wanted.0)
    } else {
        wanted
    };
    if ![b0, b1, d0, d1].iter().all(|v| v.is_finite()) || b1 <= b0 || d1 <= d0 {
        return None;
    }
    let base_points = linear_key_points((b0, b1), labels);
    if base_points.len() < 2 {
        return None;
    }
    let base_step = base_points[1] - base_points[0];
    if base_step <= 0.0 {
        return None;
    }
    // The base ticks, as a fraction of the axis.
    let fractions: Vec<f64> = base_points.iter().map(|t| (t - b0) / (b1 - b0)).collect();
    let steps_in_axis = (b1 - b0) / base_step;
    let first_offset = (base_points[0] - b0) / base_step;

    let smallest = (d1 - d0) / steps_in_axis;
    let decade = 10f64.powf(smallest.log10().floor());
    const MANTISSAS: [f64; 11] = [1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.5, 8.0];
    for exponent in [1.0, 10.0, 100.0] {
        for mantissa in MANTISSAS {
            let step = decade * exponent * mantissa;
            let span = steps_in_axis * step;
            if span < d1 - d0 {
                continue;
            }
            // First tick `j · step`, placed `first_offset` steps from the low
            // edge: the low edge is `(j − first_offset) · step`.
            let lowest = (d1 - span) / step + first_offset;
            let highest = d0 / step + first_offset;
            let (lo_j, hi_j) = ((lowest - 1e-9).ceil(), (highest + 1e-9).floor());
            if lo_j > hi_j {
                continue;
            }
            let j = ((lowest + highest) / 2.0).round().clamp(lo_j, hi_j);
            let low = (j - first_offset) * step;
            let high = low + span;
            let points = linear_key_points((low, high), labels);
            let aligned = points.len() == fractions.len()
                && points
                    .iter()
                    .zip(&fractions)
                    .all(|(t, f)| ((t - low) / (high - low) - f).abs() < 1e-6);
            if aligned {
                return Some(if inverted { (high, low) } else { (low, high) });
            }
        }
    }
    None
}

/// Widens a logarithmic interval to the whole decades that enclose it.
///
/// This is what makes the ticks fall on 1, 10, 100 rather than on 3.16 and
/// 31.6: Plotters chooses its key points in the space where it works — here
/// log₁₀ — and an interval with integer bounds naturally makes it place
/// integer steps.
pub fn snap_to_decades((lo, hi): (f64, f64)) -> (f64, f64) {
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    let (low, high) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let mut low = low.floor();
    let mut high = high.ceil();
    if high <= low {
        high = low + 1.0;
    }
    // A file can carry oversized bounds; beyond that, the ticks describe
    // nothing anymore and `powf` leaves `f64`.
    low = low.clamp(-300.0, 300.0);
    high = high.clamp(low + 1.0, 301.0);
    if lo <= hi {
        (low, high)
    } else {
        (high, low)
    }
}

/// How many ticks to ask for on a given logarithmic interval.
///
/// One per decade, capped at the number Plotters aims for by default (11):
/// beyond that it chooses a step of several decades by itself, which is the
/// wanted behavior.
pub fn decade_label_count((lo, hi): (f64, f64)) -> usize {
    let decades = (hi - lo).abs().round() as usize;
    decades.saturating_add(1).clamp(2, 11)
}

/// Label of a tick, in the transformed space.
///
/// For a linear scale, returns the value as is — the function exists so the
/// caller does not have to wonder which one to call, but the pipeline only
/// places a formatter for the scales that need one ([`needs_formatter`]), to
/// leave Plotters' default formatting intact where it suits.
pub fn format_tick(scale: AxisScale, t: f64) -> String {
    match scale {
        AxisScale::Linear => format_plain(t),
        AxisScale::Log => format_decade(t),
    }
}

/// `10^t`, written as a reader would write it.
///
/// Decimal as long as it stays readable (from 0.0001 to 1,000,000), power of
/// ten beyond. A non-integer exponent — Plotters can produce some if the
/// interval is narrow — returns the value itself.
fn format_decade(t: f64) -> String {
    if !t.is_finite() {
        return String::new();
    }
    let rounded = t.round();
    if (t - rounded).abs() > 1e-9 {
        return format_plain(inverse(AxisScale::Log, t));
    }
    let exponent = rounded as i32;
    if (-4..=6).contains(&exponent) {
        format_plain(10f64.powi(exponent))
    } else {
        format!("1e{exponent}")
    }
}

/// A number without useless zeros, and without surprise scientific notation.
fn format_plain(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let text = format!("{value}");
    // `{}` on an `f64` switches to scientific notation beyond 1e16; the
    // values this path sees are bounded by `snap_to_decades`.
    text
}

#[cfg(test)]
mod tests {

    #[test]
    fn one_number_in_n_is_written_starting_with_the_first() {
        let range = (0.0, 10.0);
        let all = TickFormat::new(AxisScale::Linear, range, DEFAULT_LABELS);
        let thin = all.with_label_every(3, range, DEFAULT_LABELS);
        let shown: Vec<f64> = linear_key_points(range, DEFAULT_LABELS)
            .into_iter()
            .filter(|&t| !thin.format(t).is_empty())
            .collect();
        assert_eq!(shown, vec![0.0, 3.0, 6.0, 9.0]);
        assert_eq!(
            all.with_label_every(1, range, DEFAULT_LABELS).format(1.0),
            "1"
        );
        assert_eq!(
            all.with_label_every(0, range, DEFAULT_LABELS).format(1.0),
            "1"
        );
    }

    #[test]
    fn a_number_of_minor_ticks_forces_the_subdivisions() {
        let minor = minor_key_points(AxisScale::Linear, (0.0, 10.0), Some(1), DEFAULT_LABELS);
        assert_eq!(minor.len(), 10, "one in the middle of each step");
        assert!((minor[0] - 0.5).abs() < 1e-9);
        let nine = minor_key_points(AxisScale::Linear, (0.0, 10.0), Some(9), DEFAULT_LABELS);
        assert_eq!(nine.len(), 90);
    }

    fn approx(points: &[f64], wanted: &[f64]) -> bool {
        points.len() == wanted.len() && points.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-9)
    }

    #[test]
    fn linear_minor_ticks_cut_the_step_in_five() {
        let minor = minor_key_points(AxisScale::Linear, (0.0, 10.0), None, DEFAULT_LABELS);
        assert_eq!(minor.len(), 40);
        assert!(approx(&minor[..4], &[0.2, 0.4, 0.6, 0.8]));
        let majors = linear_key_points((0.0, 10.0), DEFAULT_LABELS);
        assert!(minor
            .iter()
            .all(|m| majors.iter().all(|major| (major - m).abs() > 1e-9)));
    }

    #[test]
    fn a_step_with_mantissa_two_is_cut_in_four() {
        let majors = linear_key_points((0.0, 1.6), DEFAULT_LABELS);
        assert!((majors[1] - majors[0] - 0.2).abs() < 1e-9, "{majors:?}");
        let minor = minor_key_points(AxisScale::Linear, (0.0, 1.6), None, DEFAULT_LABELS);
        assert_eq!(minor.len(), 24);
        assert!(approx(&minor[..3], &[0.05, 0.1, 0.15]));
    }

    #[test]
    fn minor_ticks_extend_to_the_bounds() {
        let minor = minor_key_points(AxisScale::Linear, (-0.5, 10.5), None, DEFAULT_LABELS);
        assert!(minor.iter().any(|m| (m + 0.4).abs() < 1e-9));
        assert!(minor.iter().any(|m| (m - 10.4).abs() < 1e-9));
        assert!(minor.iter().all(|m| (-0.5..=10.5).contains(m)));
    }

    #[test]
    fn an_inverted_axis_has_the_same_minor_ticks() {
        let mut direct = minor_key_points(AxisScale::Linear, (0.0, 10.0), None, DEFAULT_LABELS);
        let mut inverse = minor_key_points(AxisScale::Linear, (10.0, 0.0), None, DEFAULT_LABELS);
        direct.sort_by(f64::total_cmp);
        inverse.sort_by(f64::total_cmp);
        assert!(approx(&direct, &inverse));
    }

    #[test]
    fn logarithmic_minor_ticks_are_the_multiples_from_two_to_nine() {
        let minor = minor_key_points(AxisScale::Log, (0.0, 3.0), None, 4);
        assert_eq!(minor.len(), 24);
        assert!((minor[0] - 2f64.log10()).abs() < 1e-9);
        assert!((minor[7] - 9f64.log10()).abs() < 1e-9);
        assert!((minor[8] - (1.0 + 2f64.log10())).abs() < 1e-9);
    }

    #[test]
    fn beyond_a_decade_of_step_the_minor_ticks_are_the_interleaved_decades() {
        let majors = linear_key_points((0.0, 20.0), max_label_count(AxisScale::Log, (0.0, 20.0)));
        assert!(majors[1] - majors[0] >= 1.5, "{majors:?}");
        let minor = minor_key_points(AxisScale::Log, (0.0, 20.0), None, DEFAULT_LABELS);
        assert!(!minor.is_empty());
        assert!(minor.iter().all(|m| (m - m.round()).abs() < 1e-9));
        assert!(minor
            .iter()
            .all(|m| majors.iter().all(|major| (major - m).abs() > 1e-9)));
    }

    #[test]
    fn a_degenerate_interval_has_no_minor_ticks() {
        assert!(minor_key_points(AxisScale::Linear, (5.0, 5.0), None, DEFAULT_LABELS).is_empty());
        assert!(
            minor_key_points(AxisScale::Linear, (f64::NAN, 1.0), None, DEFAULT_LABELS).is_empty()
        );
    }
    use super::*;

    #[test]
    fn linear_is_the_identity() {
        assert_eq!(forward(AxisScale::Linear, -3.5), Some(-3.5));
        assert_eq!(inverse(AxisScale::Linear, -3.5), -3.5);
    }

    #[test]
    fn log_rejects_what_has_no_image() {
        assert_eq!(forward(AxisScale::Log, 0.0), None);
        assert_eq!(forward(AxisScale::Log, -1.0), None);
        assert_eq!(forward(AxisScale::Log, f64::NAN), None);
        assert_eq!(forward(AxisScale::Log, f64::INFINITY), None);
        assert_eq!(forward(AxisScale::Log, 100.0), Some(2.0));
    }

    #[test]
    fn the_non_finite_also_disappears_in_linear() {
        // Same rule on both sides: what has no place on the axis does not go
        // on it, and it is not an error.
        assert_eq!(forward(AxisScale::Linear, f64::NAN), None);
        assert_eq!(forward(AxisScale::Linear, f64::NEG_INFINITY), None);
    }

    #[test]
    fn log_bounds_snap_to_decades() {
        assert_eq!(snap_to_decades((0.3, 2.7)), (0.0, 3.0));
        // A degenerate interval must remain an interval.
        assert_eq!(snap_to_decades((2.0, 2.0)), (2.0, 3.0));
        // An inverted axis keeps its direction.
        assert_eq!(snap_to_decades((2.7, 0.3)), (3.0, 0.0));
    }

    #[test]
    fn oversized_bounds_are_capped() {
        let (lo, hi) = snap_to_decades((-1e300, 1e300));
        assert!(lo >= -300.0 && hi <= 301.0 && lo < hi, "{lo}..{hi}");
        assert!(inverse(AxisScale::Log, hi).is_finite() || hi > 300.0);
    }

    #[test]
    fn a_log_tick_reads_as_a_number() {
        assert_eq!(format_tick(AxisScale::Log, 0.0), "1");
        assert_eq!(format_tick(AxisScale::Log, 2.0), "100");
        assert_eq!(format_tick(AxisScale::Log, -2.0), "0.01");
        assert_eq!(format_tick(AxisScale::Log, 9.0), "1e9");
    }

    #[test]
    fn a_non_integer_log_tick_returns_its_value() {
        let label = format_tick(AxisScale::Log, 0.5);
        assert!(label.starts_with("3.16"), "{label}");
    }

    #[test]
    fn the_number_of_ticks_follows_the_decades() {
        assert_eq!(decade_label_count((0.0, 3.0)), 4);
        assert_eq!(decade_label_count((0.0, 40.0)), 11);
        assert_eq!(decade_label_count((1.0, 1.0)), 2);
    }

    fn ticks(range: (f64, f64)) -> Vec<String> {
        let format = TickFormat::new(AxisScale::Linear, range, DEFAULT_LABELS);
        linear_key_points(range, DEFAULT_LABELS)
            .into_iter()
            .map(|t| format.format(t))
            .collect()
    }

    #[test]
    fn integer_ticks_are_written_without_decimals() {
        let labels = ticks((-40.0, 840.0));
        assert!(labels.contains(&"0".to_owned()), "{labels:?}");
        assert!(labels.contains(&"200".to_owned()), "{labels:?}");
        assert!(labels.iter().all(|l| !l.contains('.')), "{labels:?}");
    }

    #[test]
    fn fractional_ticks_keep_the_same_number_of_decimals() {
        let labels = ticks((-0.6, 0.6));
        assert!(labels.contains(&"0.0".to_owned()), "{labels:?}");
        assert!(labels.contains(&"-0.2".to_owned()), "{labels:?}");
        assert!(labels
            .iter()
            .all(|l| l.ends_with(|c: char| c.is_ascii_digit())));
    }

    #[test]
    fn a_zero_has_no_sign() {
        assert_eq!(format_linear_tick(-0.0, 0), "0");
        assert_eq!(format_linear_tick(-1e-12, 1), "0.0");
        assert_eq!(format_linear_tick(-2.0, 0), "-2");
    }

    #[test]
    fn an_aligned_axis_places_its_ticks_opposite() {
        let base = (-40.0, 840.0);
        let wanted = (-6.0, 126.0);
        let range = aligned_range(base, wanted, DEFAULT_LABELS).expect("an aligned framing");
        assert!(range.0 <= wanted.0 && range.1 >= wanted.1, "{range:?}");
        let fractions = |r: (f64, f64)| -> Vec<f64> {
            linear_key_points(r, DEFAULT_LABELS)
                .iter()
                .map(|t| (t - r.0) / (r.1 - r.0))
                .collect()
        };
        let (a, b) = (fractions(base), fractions(range));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-6, "{a:?} / {b:?}");
        }
    }

    #[test]
    fn alignment_respects_the_axis_direction() {
        let inverted =
            aligned_range((840.0, -40.0), (126.0, -6.0), DEFAULT_LABELS).expect("aligned");
        assert!(inverted.0 > inverted.1);
        assert_eq!(
            aligned_range((-40.0, 840.0), (126.0, -6.0), DEFAULT_LABELS),
            None
        );
    }

    #[test]
    fn the_comma_replaces_the_point_in_ticks() {
        let linear_format = TickFormat::new(AxisScale::Linear, (0.0, 1.0), DEFAULT_LABELS);
        let comma_format = linear_format.with_decimal_comma(true);
        assert_eq!(linear_format.format(0.5), "0.5");
        assert_eq!(comma_format.format(0.5), "0,5");
        // A log scale also writes its decimal powers with the comma.
        let log = TickFormat::new(AxisScale::Log, (-2.0, 2.0), 5).with_decimal_comma(true);
        assert_eq!(log.format(-1.0), "0,1");
    }

    #[test]
    fn the_ticks_of_a_linear_axis_are_the_pipeline_ones() {
        let range = (0.0, 10.0);
        let ticks = axis_ticks(
            AxisScale::Linear,
            range,
            false,
            2000.0,
            AxisDirection::Horizontal,
            &|s| 7.0 * s.len() as f64,
        );
        let expected = linear_key_points(range, DEFAULT_LABELS);
        assert!(expected.len() > 2, "{expected:?}");
        assert_eq!(ticks.iter().map(|t| t.0).collect::<Vec<_>>(), expected);
        assert_eq!(ticks.first().map(|t| t.1.as_str()), Some("0"));
        assert_eq!(ticks.last().map(|t| t.1.as_str()), Some("10"));
    }

    #[test]
    fn an_inverted_axis_has_the_same_ticks() {
        let sorted = |range| {
            let mut values: Vec<f64> = axis_ticks(
                AxisScale::Linear,
                range,
                false,
                2000.0,
                AxisDirection::Horizontal,
                &|s| 7.0 * s.len() as f64,
            )
            .iter()
            .map(|t| t.0)
            .collect();
            values.sort_by(f64::total_cmp);
            values
        };
        let (forward_ticks, reversed_ticks) = (sorted((0.0, 10.0)), sorted((10.0, 0.0)));
        assert!(forward_ticks.len() > 2);
        assert_eq!(forward_ticks, reversed_ticks);
    }

    #[test]
    fn logarithmic_ticks_are_the_decades() {
        let ticks = axis_ticks(
            AxisScale::Log,
            (1.0, 1000.0),
            false,
            2000.0,
            AxisDirection::Horizontal,
            &|s| 7.0 * s.len() as f64,
        );
        let data: Vec<f64> = ticks.iter().map(|t| t.0.round()).collect();
        assert_eq!(data, vec![1.0, 10.0, 100.0, 1000.0]);
        let labels: Vec<&str> = ticks.iter().map(|t| t.1.as_str()).collect();
        assert_eq!(labels, ["1", "10", "100", "1000"]);
    }

    #[test]
    fn the_decimal_comma_follows_the_setting() {
        let range = (0.0, 1.0);
        let dotted = axis_ticks(
            AxisScale::Linear,
            range,
            false,
            2000.0,
            AxisDirection::Horizontal,
            &|s| 7.0 * s.len() as f64,
        );
        let comma = axis_ticks(
            AxisScale::Linear,
            range,
            true,
            2000.0,
            AxisDirection::Horizontal,
            &|s| 7.0 * s.len() as f64,
        );
        assert!(dotted.iter().any(|t| t.1 == "0.5"), "{dotted:?}");
        assert!(comma.iter().any(|t| t.1 == "0,5"), "{comma:?}");
        assert!(comma.iter().all(|t| !t.1.contains('.')), "{comma:?}");
    }

    #[test]
    fn a_bound_without_image_gives_no_ticks() {
        assert!(axis_ticks(
            AxisScale::Log,
            (0.0, 100.0),
            false,
            2000.0,
            AxisDirection::Horizontal,
            &|s| 7.0 * s.len() as f64
        )
        .is_empty());
        assert!(axis_ticks(
            AxisScale::Log,
            (-5.0, 100.0),
            false,
            2000.0,
            AxisDirection::Horizontal,
            &|s| 7.0 * s.len() as f64
        )
        .is_empty());
        assert!(axis_ticks(
            AxisScale::Linear,
            (0.0, f64::NAN),
            false,
            2000.0,
            AxisDirection::Horizontal,
            &|s| 7.0 * s.len() as f64
        )
        .is_empty());
    }

    /// Width of a label for these tests: 7 px a character.
    fn wide(text: &str) -> f64 {
        7.0 * text.chars().count() as f64
    }

    /// Height of a label for these tests.
    fn tall(_: &str) -> f64 {
        14.0
    }

    /// The labels of `count` ticks of `range`, as `(pixel, size along the axis)`.
    fn placed(
        scale: AxisScale,
        range: (f64, f64),
        count: usize,
        length: f64,
        extent: &dyn Fn(&str) -> f64,
    ) -> Vec<(f64, f64)> {
        let format = TickFormat::new(scale, range, count);
        let mut marks: Vec<(f64, f64)> = linear_key_points(range, count)
            .into_iter()
            .map(|t| {
                (
                    (t - range.0) / (range.1 - range.0) * length,
                    extent(&format.format(t)),
                )
            })
            .collect();
        marks.sort_by(|a, b| a.0.total_cmp(&b.0));
        marks
    }

    fn assert_apart(marks: &[(f64, f64)]) {
        for pair in marks.windows(2) {
            assert!(
                pair[1].0 - pair[0].0 >= (pair[0].1 + pair[1].1) / 2.0 + MIN_LABEL_GAP,
                "{marks:?}"
            );
        }
    }

    #[test]
    fn a_narrow_horizontal_axis_asks_for_fewer_ticks_that_do_not_touch() {
        let range = (-67.5, 317.5);
        let count = tick_count(
            AxisScale::Linear,
            range,
            120.0,
            AxisDirection::Horizontal,
            false,
            &wide,
        );
        assert!(count < DEFAULT_LABELS, "{count}");
        assert_apart(&placed(AxisScale::Linear, range, count, 120.0, &wide));
    }

    #[test]
    fn the_default_one_cell_graph_keeps_its_eleven_ticks() {
        // The frame of a default graph (`FALLBACK_FRAME`: 392 x 284).
        for range in [(0.0, 10.0), (-67.5, 317.5), (0.0, 1.0), (-5.0, 105.0)] {
            let x = tick_count(
                AxisScale::Linear,
                range,
                392.0,
                AxisDirection::Horizontal,
                false,
                &wide,
            );
            let y = tick_count(
                AxisScale::Linear,
                range,
                284.0,
                AxisDirection::Vertical,
                false,
                &tall,
            );
            assert_eq!((x, y), (DEFAULT_LABELS, DEFAULT_LABELS), "{range:?}");
        }
        // And the legacy page (480 x 360 with a title): its plot area is shorter.
        let legacy = tick_count(
            AxisScale::Linear,
            (0.0, 10.0),
            240.0,
            AxisDirection::Vertical,
            false,
            &tall,
        );
        assert_eq!(legacy, DEFAULT_LABELS);
    }

    #[test]
    fn long_labels_take_ticks_away_after_the_density() {
        // "1234.55" is 49 px wide: with eleven ticks on a 392 px axis (39 px
        // each) the numbers touch although the density alone allows them.
        let range = (1234.5, 1234.9);
        let count = tick_count(
            AxisScale::Linear,
            range,
            392.0,
            AxisDirection::Horizontal,
            false,
            &wide,
        );
        assert!(count < DEFAULT_LABELS, "{count}");
        assert_apart(&placed(AxisScale::Linear, range, count, 392.0, &wide));
    }

    #[test]
    fn a_short_vertical_axis_uses_the_text_height() {
        let range = (0.0, 100.0);
        let count = tick_count(
            AxisScale::Linear,
            range,
            90.0,
            AxisDirection::Vertical,
            false,
            &tall,
        );
        assert!(count < DEFAULT_LABELS, "{count}");
        assert_apart(&placed(AxisScale::Linear, range, count, 90.0, &tall));
    }

    #[test]
    fn a_dense_logarithmic_axis_labels_fewer_decades() {
        let range = (-6.0, 6.0);
        assert_eq!(max_label_count(AxisScale::Log, range), 11);
        let count = tick_count(
            AxisScale::Log,
            range,
            150.0,
            AxisDirection::Horizontal,
            false,
            &wide,
        );
        assert!(count < 11, "{count}");
        assert_apart(&placed(AxisScale::Log, range, count, 150.0, &wide));
        // A long axis keeps one tick per decade, as before.
        let few = (0.0, 3.0);
        assert_eq!(
            tick_count(
                AxisScale::Log,
                few,
                392.0,
                AxisDirection::Horizontal,
                false,
                &wide
            ),
            4
        );
    }

    #[test]
    fn a_degenerate_axis_keeps_the_upper_bound() {
        for (range, length) in [
            ((1.0, 1.0), 200.0),
            ((0.0, 10.0), 0.0),
            ((0.0, f64::NAN), 200.0),
        ] {
            assert_eq!(
                tick_count(
                    AxisScale::Linear,
                    range,
                    length,
                    AxisDirection::Horizontal,
                    false,
                    &wide
                ),
                DEFAULT_LABELS
            );
        }
    }
}

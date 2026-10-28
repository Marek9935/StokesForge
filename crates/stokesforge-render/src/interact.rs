//! Pixel ↔ data mapping of a drawn panel — task R7.
//!
//! # Why here, and not in `stokesforge-render-egui`
//!
//! Nothing below depends on egui: it is pure geometry, just like
//! [`crate::scale`]. A drawn panel has a plot area in pixels
//! ([`DrawnPanel::pixel_origin`], [`DrawnPanel::pixel_size`], added by this
//! task) and a framing in data ([`DrawnPanel::x_range`],
//! [`DrawnPanel::y_range`]); going from one to the other concerns no backend
//! in particular, and putting it here avoids exactly the duplication that
//! the rule of `roadmap-rendu.md` (« aucune fonctionnalité n'entre dans un
//! seul backend ») exists to prevent. `stokesforge-render-svg` has no use
//! for it today (a static SVG has no cursor), but nothing would prevent it
//! from having one some day — an interactive SVG viewer, for example —
//! without having to move this module down a level.
//!
//! # The transformation, in one sentence
//!
//! An axis works in a **transformed** space ([`crate::scale::forward`]) that
//! is linear by construction — including on a logarithmic scale, where it is
//! log₁₀(value) that is. The pixel ↔ transformed mapping is thus a plain
//! linear interpolation; going to data space is done by
//! [`crate::scale::inverse`] on either side. This is what keeps a wheel zoom
//! correct on a log axis without any caller having to know it.
//!
//! # The Y axis is inverted, and this module carries the only correction
//!
//! A pixel grows downward; a datum grows upward. `pixel_to_data`
//! and `data_to_pixel` carry this flip once and for all — the exact
//! derivation (and the direction of the resulting drag) is verified by this
//! module's tests rather than asserted in a comment.

use crate::pipeline::DrawnPanel;
use crate::scale::{forward, inverse};
use crate::series;
use stokesforge_core::resolve::resolve_panel;
use stokesforge_core::Tree;
use stokesforge_plot::{AxisScale, AxisSpec, PlotSpec, SeriesAxis};

/// The pixel ↔ data mapping of an already drawn panel.
///
/// A framing gesture, expressed in the plot area's coordinate space (pixels):
/// the same for all layers of a cell, which share this area (#167).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Zoom by a `factor` centered on pixel `anchor` (see [`PanelTransform::zoom`]).
    Zoom {
        /// Pixel that keeps its datum.
        anchor: (f64, f64),
        /// `< 1` zooms in, `> 1` zooms out.
        factor: f64,
    },
    /// Drag by `(dx, dy)` pixels (see [`PanelTransform::pan`]).
    Pan {
        /// Horizontal displacement.
        dx: f64,
        /// Vertical displacement.
        dy: f64,
    },
    /// Zoom rectangle between two corners in pixels (see
    /// [`PanelTransform::range_between`]).
    Box {
        /// First corner.
        a: (f64, f64),
        /// Opposite corner.
        b: (f64, f64),
    },
}

/// Built from a [`DrawnPanel`]; rebuilt identically as long as nothing has
/// moved, hence cheap to redo on every frame rather than to cache.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelTransform {
    // As `f64`: the rectangle of an interface panel is fractional, and rounding
    // to an integer would shift by half a pixel everything placed in it.
    origin: (f64, f64),
    size: (f64, f64),
    x_range: (f64, f64),
    y_range: (f64, f64),
    x_scale: AxisScale,
    y_scale: AxisScale,
}

impl PanelTransform {
    /// The transformation of a drawn panel, or `None` if its plot area
    /// is degenerate (zero width or height — should not happen for
    /// a panel that passed [`crate::pipeline::MIN_CELL`], but a
    /// division by zero should not rely on this indirect guarantee).
    pub fn of(panel: &DrawnPanel) -> Option<Self> {
        if panel.pixel_size.0 == 0 || panel.pixel_size.1 == 0 {
            return None;
        }
        Some(Self {
            origin: (
                f64::from(panel.pixel_origin.0),
                f64::from(panel.pixel_origin.1),
            ),
            size: (f64::from(panel.pixel_size.0), f64::from(panel.pixel_size.1)),
            x_range: panel.x_range,
            y_range: panel.y_range,
            x_scale: panel.x_scale,
            y_scale: panel.y_scale,
        })
    }

    /// The transformation of a plot area that does not come from a
    /// [`DrawnPanel`] — the analysis window preview (#261), painted by the
    /// application in a fractional rectangle. `origin` is the top-left
    /// corner and `size` the size, in pixels; `None` if the size is not
    /// finite and strictly positive.
    ///
    /// The bounds are in data space **in display order**: an inverted
    /// axis is given as `(top, bottom)`, like [`DrawnPanel::x_range`]. On a
    /// logarithmic scale they must be strictly positive, otherwise
    /// [`Self::pixel_to_data`] and [`Self::data_to_pixel`] return `None`.
    pub fn new(
        origin: (f64, f64),
        size: (f64, f64),
        x_range: (f64, f64),
        y_range: (f64, f64),
        x_scale: AxisScale,
        y_scale: AxisScale,
    ) -> Option<Self> {
        let valid = origin.0.is_finite()
            && origin.1.is_finite()
            && size.0.is_finite()
            && size.1.is_finite()
            && size.0 > 0.0
            && size.1 > 0.0;
        valid.then_some(Self {
            origin,
            size,
            x_range,
            y_range,
            x_scale,
            y_scale,
        })
    }

    /// The transformation of a panel for a given ordinate: the right-hand
    /// one if `right` and the panel has one (`y2_range`), otherwise the
    /// left-hand one — the rule for series and free texts (issue
    /// #35).
    pub fn of_axis(panel: &DrawnPanel, right: bool) -> Option<Self> {
        let mut transform = Self::of(panel)?;
        if let (true, Some(range)) = (right, panel.y2_range) {
            transform.y_range = range;
            transform.y_scale = panel.y2_scale;
        }
        Some(transform)
    }

    /// True if the given pixel falls inside this panel's plot area.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let ((ox, oy), (w, h)) = (self.origin, self.size);
        (ox..=ox + w).contains(&x) && (oy..=oy + h).contains(&y)
    }

    /// Datum under a pixel. `None` if one of the two bounds of either
    /// axis has no image on its scale (e.g. a negative bound on a
    /// log axis) — a case already possible for an inconsistent explicit
    /// framing (`pipeline::axis_range`), hence to be passed up rather than
    /// panicked on.
    pub fn pixel_to_data(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let fx = (x - self.origin.0) / self.size.0;
        // The pixel grows downward, the datum upward: `1.0 - fy`.
        let fy = 1.0 - (y - self.origin.1) / self.size.1;
        let data_x = axis_from_frac(self.x_scale, self.x_range, fx)?;
        let data_y = axis_from_frac(self.y_scale, self.y_range, fy)?;
        Some((data_x, data_y))
    }

    /// Pixel of a datum. Reciprocal of [`Self::pixel_to_data`] where
    /// the latter is defined.
    pub fn data_to_pixel(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let fx = axis_to_frac(self.x_scale, self.x_range, x)?;
        let fy = axis_to_frac(self.y_scale, self.y_range, y)?;
        let px = self.origin.0 + fx * self.size.0;
        let py = self.origin.1 + (1.0 - fy) * self.size.1;
        Some((px, py))
    }

    /// The `(x, y)` framing that this gesture gives to these two axes.
    pub fn apply(&self, gesture: &Gesture) -> Option<((f64, f64), (f64, f64))> {
        match *gesture {
            Gesture::Zoom { anchor, factor } => self.zoom(anchor, factor),
            Gesture::Pan { dx, dy } => self.pan(dx, dy),
            Gesture::Box { a, b } => self.range_between(a, b),
        }
    }

    /// New framing after a zoom centered on `anchor` (in pixels), by a
    /// `factor` (`< 1` zooms in, `> 1` zooms out). The datum under `anchor`
    /// stays under `anchor` after the zoom — this is the property that
    /// [`tests::zooming_pins_the_point_under_the_cursor`] checks.
    pub fn zoom(&self, anchor: (f64, f64), factor: f64) -> Option<((f64, f64), (f64, f64))> {
        let (ax, ay) = self.pixel_to_data(anchor.0, anchor.1)?;
        let x_range = zoom_range(self.x_scale, self.x_range, ax, factor)?;
        let y_range = zoom_range(self.y_scale, self.y_range, ay, factor)?;
        Some((x_range, y_range))
    }

    /// New framing after a drag of `(dx, dy)` pixels — the datum that
    /// was under the cursor at the start of the gesture stays under the cursor
    /// at the end (`tests::dragging_makes_the_content_follow_the_cursor`).
    pub fn pan(&self, dx: f64, dy: f64) -> Option<((f64, f64), (f64, f64))> {
        let fx = dx / self.size.0;
        // The pixel Y is inverted relative to the datum: the sign changes.
        let fy = -dy / self.size.1;
        let x_range = pan_range(self.x_scale, self.x_range, fx)?;
        let y_range = pan_range(self.y_scale, self.y_range, fy)?;
        Some((x_range, y_range))
    }

    /// The framing delimited by two corners in pixels — the rubber-band
    /// zoom rectangle of task 7.4. The order of the corners does not matter: each
    /// pair is put back in the direction of the displayed axis, since a framing
    /// "from 800 to 400" draws the axis upside down (`pipeline::axis_range`).
    pub fn range_between(
        &self,
        corner_a: (f64, f64),
        corner_b: (f64, f64),
    ) -> Option<((f64, f64), (f64, f64))> {
        let (ax, ay) = self.pixel_to_data(corner_a.0, corner_a.1)?;
        let (bx, by) = self.pixel_to_data(corner_b.0, corner_b.1)?;
        let oriented = |(a, b): (f64, f64), shown: (f64, f64)| {
            if (a < b) == (shown.0 < shown.1) {
                (a, b)
            } else {
                (b, a)
            }
        };
        Some((
            oriented((ax, bx), self.x_range),
            oriented((ay, by), self.y_range),
        ))
    }
}

/// The fraction (0 = `range.0`, 1 = `range.1`) of value `v` on the axis,
/// in the transformed space.
fn axis_to_frac(scale: AxisScale, (r0, r1): (f64, f64), v: f64) -> Option<f64> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    let t = forward(scale, v)?;
    if t1 == t0 {
        return None;
    }
    Some((t - t0) / (t1 - t0))
}

/// The datum at fraction `frac` (0 = `range.0`, 1 = `range.1`) of the axis.
fn axis_from_frac(scale: AxisScale, (r0, r1): (f64, f64), frac: f64) -> Option<f64> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    Some(inverse(scale, t0 + frac * (t1 - t0)))
}

/// A framing zoomed by a `factor` around value `anchor`, in the
/// transformed space — the derivation that guarantees `anchor` keeps the
/// same fraction before and after (hence the same pixel) is at the top of the module.
fn zoom_range(
    scale: AxisScale,
    (r0, r1): (f64, f64),
    anchor: f64,
    factor: f64,
) -> Option<(f64, f64)> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    let ta = forward(scale, anchor)?;
    let n0 = ta + (t0 - ta) * factor;
    let n1 = ta + (t1 - ta) * factor;
    Some((inverse(scale, n0), inverse(scale, n1)))
}

/// A framing shifted by a fraction `frac` of its own extent, in the
/// transformed space. `frac > 0` moves the framing toward the top of
/// the axis (hence, in pixels, makes the content appear to move down);
/// [`PanelTransform::pan`] chooses the sign of `frac` for each axis.
fn pan_range(scale: AxisScale, (r0, r1): (f64, f64), frac: f64) -> Option<(f64, f64)> {
    let t0 = forward(scale, r0)?;
    let t1 = forward(scale, r1)?;
    let shift = frac * (t1 - t0);
    Some((inverse(scale, t0 - shift), inverse(scale, t1 - shift)))
}

/// The point of an already projected curve closest to the cursor, in pixels.
///
/// The equivalent of the `ReverseCoordTranslate` + `min_by_key` pair
/// of `architecture-choisie.md` §6 — without the former, of which this module is already
/// the implementation. Returns the index in `points` and the distance in pixels.
pub fn nearest_on_curve(
    transform: &PanelTransform,
    points: &[(f64, f64)],
    cursor: (f64, f64),
) -> Option<(usize, f64)> {
    points
        .iter()
        .enumerate()
        .filter_map(|(index, &(x, y))| {
            let (px, py) = transform.data_to_pixel(x, y)?;
            let squared = (px - cursor.0).powi(2) + (py - cursor.1).powi(2);
            Some((index, squared))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, squared)| (index, squared.sqrt()))
}

/// The automatic framing of an axis on `values`, in data space and in
/// increasing direction.
///
/// Values with no image on the scale ([`forward`]: non-finite, or ≤ 0 on a
/// logarithmic scale) are ignored. The 5% margin is taken in the
/// **transformed** space — hence proportionally on a log axis —, a frame of
/// zero extent widens by 0.5 on each side, and with no values the frame is
/// `(0, 1)` transformed (`(1, 10)` on log). This is the initial frame of the
/// analysis window preview (#261).
pub fn fit_range(values: &[f64], scale: AxisScale) -> (f64, f64) {
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in values.iter().filter_map(|&value| forward(scale, value)) {
        low = low.min(t);
        high = high.max(t);
    }
    let (low, high) = if low > high {
        (0.0, 1.0)
    } else if low == high {
        (low - 0.5, high + 0.5)
    } else {
        let margin = (high - low) * 0.05;
        (low - margin, high + margin)
    };
    (inverse(scale, low), inverse(scale, high))
}

/// The initial frame of a graph axis: [`fit_range`] on `values`,
/// replaced by the axis's stored bounds when its framing is not
/// automatic, then put in display direction (`(top, bottom)` if the axis is
/// inverted). Same rule as the graph (`pipeline::axis_range`, #269): a single
/// flip, whether the axis is inverted by the "Inverted" checkbox, by
/// bounds `min > max`, or by both.
pub fn axis_frame(values: &[f64], axis: &AxisSpec) -> (f64, f64) {
    let mut range = fit_range(values, axis.scale);
    let mut reversed = false;
    if !axis.auto_range {
        if let (Some(low), Some(high)) = (axis.range_min, axis.range_max) {
            reversed = low > high;
            range = (low.min(high), low.max(high));
        }
    }
    if axis.inverted || reversed {
        range = (range.1, range.0);
    }
    range
}

/// The curve point closest to `x` in abscissa, with the ordinate
/// averaged over `n` points centered on it (for even `n`, one more on the right)
/// (the analysis preview's snapping, #181 M4 b). `None` for an empty curve, or if `ys` is
/// shorter than `xs`.
///
/// Non-finite abscissas are never chosen; non-finite ordinates are
/// dropped from the average (`None` if none remain). At the edge,
/// the window is truncated instead of overflowing. Named `snap_to_curve` so as
/// not to be confused with [`crate::scale::snap_to_decades`].
pub fn snap_to_curve(xs: &[f64], ys: &[f64], x: f64, n: usize) -> Option<(f64, f64)> {
    snap_to_curve_zone(xs, ys, x, n).map(|snap| (snap.x, snap.y))
}

/// The snapping of [`snap_to_curve`] with the area that enters the average
/// (#334): enough to show on screen what N averages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapZone {
    /// Abscissa of the closest curve point.
    pub x: f64,
    /// Mean ordinate of the window.
    pub y: f64,
    /// Smallest finite abscissa of the window (unsorted abscissas: the
    /// minimum of the window, not its first point).
    pub x_min: f64,
    /// Largest finite abscissa of the window.
    pub x_max: f64,
    /// Actual number of averaged points: the window truncated at the edge,
    /// without the non-finite ordinates.
    pub count: usize,
}

/// [`snap_to_curve`] and the averaged area. Same rules, same `None`s.
pub fn snap_to_curve_zone(xs: &[f64], ys: &[f64], x: f64, n: usize) -> Option<SnapZone> {
    if ys.len() < xs.len() {
        return None;
    }
    let nearest = xs
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))?
        .0;
    // Window of `n` points; for even `n`, the extra point is on the right.
    let n = n.max(1);
    let low = nearest.saturating_sub((n - 1) / 2);
    let high = (nearest + n / 2 + 1).min(ys.len());
    let (sum, count) = ys[low..high]
        .iter()
        .filter(|value| value.is_finite())
        .fold((0.0, 0_usize), |(sum, count), value| {
            (sum + value, count + 1)
        });
    if count == 0 {
        return None;
    }
    let (x_min, x_max) = xs[low..high]
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), value| {
            (lo.min(value), hi.max(value))
        });
    Some(SnapZone {
        x: xs[nearest],
        y: sum / count as f64,
        x_min,
        x_max,
        count,
    })
}

/// A point picked on a curve: what [`nearest_point`] returns, as
/// `f64` and with no interface type (the conversion to egui pixels is that of
/// `stokesforge-render-egui`, which makes its `PointHit` from it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestPoint {
    /// Index of the series in `PanelSpec::series`, resolved in the same
    /// order (`ResolvedPanel::series`).
    pub series: usize,
    /// Index of the picked point in its series.
    pub point_index: usize,
    /// Position of the point, in data (never the exponents of a
    /// logarithmic axis).
    pub data: (f64, f64),
    /// Position of the point, in figure pixels.
    pub pixel: (f64, f64),
    /// Distance from the cursor to the point, in pixels.
    pub distance: f64,
}

/// The picked point of the drawn panel `drawn_panel` under the cursor `cursor`
/// (figure pixels) — task R7.2, the local equivalent of
/// `ReverseCoordTranslate` + `min_by_key` of `architecture-choisie.md` §6
/// (`min_by_key` lives in [`nearest_on_curve`], reused series by series).
/// `None` if no point is within `radius` pixels or less.
///
/// **Limited to the main axis** (bottom, left): a series of the secondary
/// axis would live on a second [`PanelTransform`] (`of_axis`) that would have
/// to be built alongside, and nothing uses it today. A deliberate gap, not
/// a silent omission. Hidden, broken or unknown-type series, and
/// those that are not a curve (histogram, bars), are ignored.
///
/// Recomputes the geometry rather than keeping it in memory: a resolved
/// series only carries `Arc<Vec<f64>>`s cloned by counter
/// (`stokesforge-core::resolve`), so redoing it on every hover costs
/// one traversal, not a copy — cheaper than keeping it up to date across
/// each zoom.
pub fn nearest_point(
    tree: &Tree,
    spec: &PlotSpec,
    drawn_panel: &DrawnPanel,
    transform: &PanelTransform,
    cursor: (f64, f64),
    radius: f64,
) -> Option<NearestPoint> {
    let spec_panel = spec.panels.get(drawn_panel.index)?;
    let resolved = resolve_panel(tree, spec_panel);

    let mut best: Option<NearestPoint> = None;
    for (series_index, resolved_series) in resolved.series.iter().enumerate() {
        let series_spec = resolved_series.spec;
        if !series_spec.visible || series_spec.axis == SeriesAxis::Right {
            continue;
        }
        let prepared = series::prepare(
            resolved_series,
            spec_panel.bottom.scale,
            spec_panel.left.scale,
        );
        if prepared.broken || prepared.unknown_type {
            continue;
        }
        let series::Geometry::Curve { points, .. } = &prepared.geometry else {
            continue;
        };
        // The prepared geometry is in the transformed space (log10 on a log
        // axis); `PanelTransform` expects data and applies `forward`
        // itself: without this return, the logarithm would be taken twice (#268).
        let data_points: Vec<(f64, f64)> = points
            .iter()
            .map(|&(x, y)| {
                (
                    inverse(spec_panel.bottom.scale, x),
                    inverse(spec_panel.left.scale, y),
                )
            })
            .collect();
        let Some((point_index, distance)) = nearest_on_curve(transform, &data_points, cursor)
        else {
            continue;
        };
        if best.is_some_and(|hit| hit.distance <= distance) {
            continue;
        }
        let Some(pixel) =
            transform.data_to_pixel(data_points[point_index].0, data_points[point_index].1)
        else {
            continue;
        };
        best = Some(NearestPoint {
            series: series_index,
            point_index,
            data: data_points[point_index],
            pixel,
            distance,
        });
    }
    best.filter(|hit| hit.distance <= radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::panel;
    use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
    use stokesforge_core::{NodeId, Project};
    use stokesforge_plot::{PanelSpec, SeriesSpec};

    /// A 100×100 px panel, framed on `0..100` on both sides, linear —
    /// enough to check the direction of both axes without the log scale
    /// muddling the reading of the numbers.
    fn square() -> PanelTransform {
        PanelTransform {
            origin: (0.0, 0.0),
            size: (100.0, 100.0),
            x_range: (0.0, 100.0),
            y_range: (0.0, 100.0),
            x_scale: AxisScale::Linear,
            y_scale: AxisScale::Linear,
        }
    }

    #[test]
    fn the_top_left_corner_is_x_min_y_max() {
        let t = square();
        assert_eq!(t.pixel_to_data(0.0, 0.0), Some((0.0, 100.0)));
        assert_eq!(t.pixel_to_data(100.0, 100.0), Some((100.0, 0.0)));
    }

    #[test]
    fn pixel_to_data_round_trip_is_the_identity() {
        let t = square();
        for (x, y) in [(0.0, 0.0), (37.0, 61.0), (100.0, 100.0)] {
            let (dx, dy) = t.pixel_to_data(x, y).expect("pixel inside the panel");
            let (px, py) = t.data_to_pixel(dx, dy).expect("representable datum");
            assert!((px - x).abs() < 1e-9 && (py - y).abs() < 1e-9, "{px},{py}");
        }
    }

    #[test]
    fn dragging_makes_the_content_follow_the_cursor() {
        // The datum under pixel (20, 30) must end up under pixel
        // (20 + 15, 30 + 8) after a drag of (15, 8) px — this is the very
        // definition of "dragging the graph".
        let t = square();
        let (dx, dy) = t.pixel_to_data(20.0, 30.0).expect("inside the panel");
        let (x_range, y_range) = t.pan(15.0, 8.0).expect("linear pan always defined");
        let after = PanelTransform {
            x_range,
            y_range,
            ..t
        };
        let (px, py) = after
            .data_to_pixel(dx, dy)
            .expect("the same datum stays representable");
        assert!((px - 35.0).abs() < 1e-9, "px = {px}");
        assert!((py - 38.0).abs() < 1e-9, "py = {py}");
    }

    #[test]
    fn zooming_pins_the_point_under_the_cursor() {
        let t = square();
        let anchor = (70.0, 20.0);
        let anchor_data = t
            .pixel_to_data(anchor.0, anchor.1)
            .expect("inside the panel");
        let (x_range, y_range) = t.zoom(anchor, 0.5).expect("linear zoom always defined");
        let after = PanelTransform {
            x_range,
            y_range,
            ..t
        };
        let (px, py) = after
            .data_to_pixel(anchor_data.0, anchor_data.1)
            .expect("the point under the cursor stays representable");
        assert!((px - anchor.0).abs() < 1e-9, "px = {px}");
        assert!((py - anchor.1).abs() < 1e-9, "py = {py}");
        // And the framing did shrink by half.
        assert!((x_range.1 - x_range.0 - 50.0).abs() < 1e-9, "{x_range:?}");
    }

    #[test]
    fn zooming_on_a_log_scale_stays_consistent() {
        let t = PanelTransform {
            origin: (0.0, 0.0),
            size: (100.0, 100.0),
            x_range: (1.0, 1_000.0),
            y_range: (0.0, 100.0),
            x_scale: AxisScale::Log,
            y_scale: AxisScale::Linear,
        };
        // The middle of the panel, in x, is the geometric mean of 1..1000,
        // about 31.6 — not 500.5 as in linear.
        let (mid_x, _) = t.pixel_to_data(50.0, 100.0).expect("inside the panel");
        assert!((mid_x - 31.622_776_6).abs() < 1e-4, "{mid_x}");

        let (x_range, _) = t.zoom((50.0, 50.0), 0.5).expect("log zoom always defined");
        // Still centered on the same point after the zoom.
        let after = PanelTransform { x_range, ..t };
        let (px, _) = after.data_to_pixel(mid_x, 50.0).expect("representable");
        assert!((px - 50.0).abs() < 1e-6, "px = {px}");
    }

    #[test]
    fn the_rubber_band_rectangle_reads_both_corners() {
        let t = square();
        let (x_range, y_range) = t.range_between((10.0, 10.0), (60.0, 40.0)).unwrap();
        assert_eq!(x_range, (10.0, 60.0));
        // Y grows upward: the top corner (pixel 10) is the largest datum,
        // but the pair stays in the direction of the displayed axis (bottom → top).
        assert_eq!(y_range, (60.0, 90.0));
        // The corners taken in the other order give the same framing.
        let swapped = t.range_between((60.0, 40.0), (10.0, 10.0)).unwrap();
        assert_eq!(swapped, (x_range, y_range));
    }

    #[test]
    fn outside_the_panel_is_no_longer_in_the_panel() {
        let t = square();
        assert!(t.contains(0.0, 0.0));
        assert!(t.contains(100.0, 100.0));
        assert!(!t.contains(-1.0, 50.0));
        assert!(!t.contains(50.0, 101.0));
    }

    #[test]
    fn the_closest_point_really_is_the_closest() {
        let t = square();
        let points = [(0.0, 0.0), (50.0, 50.0), (100.0, 0.0)];
        // Cursor near the pixel of (50, 50) (data), i.e. pixel (50, 50).
        let (index, distance) = nearest_on_curve(&t, &points, (52.0, 51.0)).unwrap();
        assert_eq!(index, 1);
        assert!(distance < 3.0, "{distance}");
    }

    /// A project with one worksheet whose columns are `columns`, and their
    /// identifiers in the same order.
    fn project_with(columns: &[Vec<f64>]) -> (Project, Vec<NodeId>) {
        let mut project = Project::new("Picking").expect("project");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("workbook");
        let sheet = project.add_worksheet(book, None).expect("worksheet");
        let ids = columns
            .iter()
            .map(|values| {
                let column = Column::from_block(
                    ColumnType::Numeric,
                    ValueBlock::Float(values.clone()),
                    ColumnMeta::default(),
                )
                .expect("column");
                project
                    .add_column(sheet, None, None, column)
                    .expect("added")
            })
            .collect();
        (project, ids)
    }

    /// Picks the point under `cursor` of a 100×100 px panel framed on
    /// `0..100`, whose curves are `series`.
    fn pick(
        columns: &[Vec<f64>],
        series: impl Fn(&[NodeId]) -> Vec<SeriesSpec>,
        cursor: (f64, f64),
        radius: f64,
    ) -> Option<NearestPoint> {
        let (project, ids) = project_with(columns);
        let mut spec_panel = PanelSpec::at(0, 0);
        spec_panel.series.extend(series(&ids));
        let spec = PlotSpec::empty("Picking").with_panels(spec_panel);
        let drawn = panel(0, 0, 0);
        let transform = PanelTransform::of(&drawn).expect("plot area");
        nearest_point(project.tree(), &spec, &drawn, &transform, cursor, radius)
    }

    fn diagonal() -> Vec<Vec<f64>> {
        vec![vec![0.0, 50.0, 100.0], vec![0.0, 50.0, 100.0]]
    }

    #[test]
    fn the_picked_point_is_the_closest_to_the_cursor() {
        // Point (50, 50) is at pixel (50, 50): the ordinate is flipped.
        let hit = pick(
            &diagonal(),
            |ids| vec![SeriesSpec::new_xy(ids[0], ids[1])],
            (52.0, 51.0),
            12.0,
        )
        .expect("a point in range");
        assert_eq!((hit.series, hit.point_index), (0, 1));
        assert_eq!(hit.data, (50.0, 50.0));
        assert_eq!(hit.pixel, (50.0, 50.0));
        assert!((hit.distance - 5f64.sqrt()).abs() < 1e-9, "{hit:?}");
    }

    #[test]
    fn a_point_is_picked_on_logarithmic_axes() {
        // #268: the logarithm must be taken only once.
        let columns = vec![
            vec![1.0, 10.0, 100.0, 1000.0],
            vec![10.0, 100.0, 1000.0, 10_000.0],
        ];
        let (project, ids) = project_with(&columns);
        let mut spec_panel = PanelSpec::at(0, 0);
        spec_panel.bottom.scale = AxisScale::Log;
        spec_panel.left.scale = AxisScale::Log;
        spec_panel.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
        let spec = PlotSpec::empty("Log").with_panels(spec_panel);
        let drawn = panel(0, 0, 0);
        let transform = PanelTransform {
            origin: (0.0, 0.0),
            size: (300.0, 400.0),
            x_range: (1.0, 1000.0),
            y_range: (10.0, 10_000.0),
            x_scale: AxisScale::Log,
            y_scale: AxisScale::Log,
        };
        for (index, (&x, &y)) in columns[0].iter().zip(&columns[1]).enumerate() {
            let cursor = transform.data_to_pixel(x, y).unwrap();
            let hit = nearest_point(project.tree(), &spec, &drawn, &transform, cursor, 3.0)
                .unwrap_or_else(|| panic!("point {index} not picked"));
            assert_eq!(hit.point_index, index);
            assert!(hit.distance < 1e-6, "{hit:?}");
            assert!((hit.data.0 / x - 1.0).abs() < 1e-9 && (hit.data.1 / y - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn out_of_range_nothing_is_picked() {
        let columns = diagonal();
        let series = |ids: &[NodeId]| vec![SeriesSpec::new_xy(ids[0], ids[1])];
        // The top-left corner is more than 12 px from the diagonal (the
        // closest point, (0, 0), is at pixel (0, 100)).
        assert_eq!(pick(&columns, series, (10.0, 10.0), 12.0), None);
        // 20 px from point (50, 50): outside a radius of 12, inside one of 25.
        assert_eq!(pick(&columns, series, (70.0, 50.0), 12.0), None);
        let hit = pick(&columns, series, (70.0, 50.0), 25.0).expect("in range");
        assert_eq!(hit.point_index, 1);
    }

    #[test]
    fn a_hidden_or_right_hand_series_is_not_picked() {
        let columns = vec![
            vec![0.0, 50.0, 100.0],
            vec![0.0, 50.0, 100.0],
            // A second curve, away from the first (offset by 10 px).
            vec![0.0, 40.0, 90.0],
        ];
        let cursor = (50.0, 50.0);
        let two = |modify: fn(&mut SeriesSpec)| {
            move |ids: &[NodeId]| {
                let mut near = SeriesSpec::new_xy(ids[0], ids[1]);
                modify(&mut near);
                vec![near, SeriesSpec::new_xy(ids[0], ids[2])]
            }
        };
        let free = pick(&columns, two(|_| {}), cursor, 12.0).expect("in range");
        assert_eq!((free.series, free.point_index), (0, 1));

        let hidden = pick(&columns, two(|s| s.visible = false), cursor, 12.0)
            .expect("the other curve is in range");
        assert_eq!((hidden.series, hidden.point_index), (1, 1));
        assert_eq!(hidden.data, (50.0, 40.0));

        let right = pick(
            &columns,
            two(|s| s.axis = stokesforge_plot::SeriesAxis::Right),
            cursor,
            12.0,
        )
        .expect("the other curve is in range");
        assert_eq!(right.series, 1);

        // Alone, a hidden series picks nothing.
        let alone = |ids: &[NodeId]| {
            let mut s = SeriesSpec::new_xy(ids[0], ids[1]);
            s.visible = false;
            vec![s]
        };
        assert_eq!(pick(&columns, alone, cursor, 12.0), None);
    }

    #[test]
    fn a_panel_unknown_to_the_spec_picks_nothing() {
        let (project, _) = project_with(&[]);
        let spec = PlotSpec::empty("Empty").with_panels(PanelSpec::at(0, 0));
        let drawn = panel(5, 0, 0);
        let transform = PanelTransform::of(&drawn).expect("plot area");
        assert_eq!(
            nearest_point(project.tree(), &spec, &drawn, &transform, (1.0, 1.0), 12.0),
            None
        );
    }

    #[test]
    fn the_picked_panel_is_that_of_the_drawn_layer() {
        // The drawn layer is the second of the spec: it is its series that are
        // read, not those of the first.
        let (project, ids) = project_with(&diagonal());
        let mut second = PanelSpec::at(0, 1);
        second.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
        let spec = PlotSpec::empty("Two").with_panels(stokesforge_plot::Panels::new(vec![
            PanelSpec::at(0, 0),
            second,
        ]));
        let drawn = panel(1, 0, 1);
        let transform = PanelTransform::of(&drawn).expect("plot area");
        let hit = nearest_point(
            project.tree(),
            &spec,
            &drawn,
            &transform,
            (50.0, 50.0),
            12.0,
        )
        .expect("a point in range");
        assert_eq!(hit.point_index, 1);
    }

    // --- `PanelTransform::new`, initial frame and snapping (#261) ---

    fn assert_pixel(got: Option<(f64, f64)>, want: (f64, f64)) {
        let (x, y) = got.expect("representable");
        assert!(
            (x - want.0).abs() < 1e-9 && (y - want.1).abs() < 1e-9,
            "{x},{y} instead of {want:?}"
        );
    }

    #[test]
    fn a_fractional_origin_gives_the_exact_corners() {
        let t = PanelTransform::new(
            (10.5, 20.25),
            (200.0, 100.0),
            (0.0, 10.0),
            (-1.0, 1.0),
            AxisScale::Linear,
            AxisScale::Linear,
        )
        .expect("valid area");
        assert_pixel(t.data_to_pixel(0.0, -1.0), (10.5, 120.25));
        assert_pixel(t.data_to_pixel(10.0, 1.0), (210.5, 20.25));
        let (x, y) = t.pixel_to_data(10.5, 120.25).expect("inside the area");
        assert!((x - 0.0).abs() < 1e-9 && (y + 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_degenerate_area_has_no_transformation() {
        let new = |size| {
            PanelTransform::new(
                (0.0, 0.0),
                size,
                (0.0, 1.0),
                (0.0, 1.0),
                AxisScale::Linear,
                AxisScale::Linear,
            )
        };
        assert!(new((0.0, 10.0)).is_none());
        assert!(new((10.0, -1.0)).is_none());
        assert!(new((f64::NAN, 10.0)).is_none());
        assert!(new((10.0, f64::INFINITY)).is_none());
        assert!(new((10.0, 10.0)).is_some());
    }

    #[test]
    fn an_inverted_log_scale_and_zoom_keep_the_gesture_mapping() {
        // Inverted log X (100 on the left, 1 on the right), increasing log Y.
        let t = PanelTransform::new(
            (0.0, 0.0),
            (200.0, 100.0),
            (100.0, 1.0),
            (10.0, 1000.0),
            AxisScale::Log,
            AxisScale::Log,
        )
        .expect("valid area");
        let center = t.data_to_pixel(10.0, 100.0);
        assert_pixel(center, (100.0, 50.0));
        let (x, y) = t.pixel_to_data(100.0, 50.0).expect("inside the area");
        assert!((x - 10.0).abs() < 1e-6 && (y - 100.0).abs() < 1e-6);
        assert_pixel(t.data_to_pixel(100.0, 10.0), (0.0, 100.0));
        assert_pixel(t.data_to_pixel(1.0, 1000.0), (200.0, 0.0));
        let (x_range, y_range) = t.zoom((100.0, 50.0), 0.5).expect("log zoom defined");
        let zoomed = PanelTransform {
            x_range,
            y_range,
            ..t
        };
        assert_pixel(zoomed.data_to_pixel(10.0, 100.0), (100.0, 50.0));
        // The direction of the axes survives the zoom: X stays decreasing, Y increasing.
        assert!(x_range.0 > x_range.1 && y_range.0 < y_range.1);
    }

    #[test]
    fn a_rectangle_on_an_inverted_axis_keeps_the_display_direction() {
        let t = PanelTransform::new(
            (0.0, 0.0),
            (100.0, 100.0),
            (100.0, 0.0),
            (0.0, 100.0),
            AxisScale::Linear,
            AxisScale::Linear,
        )
        .expect("valid area");
        let (x_range, _) = t.range_between((10.0, 10.0), (60.0, 40.0)).unwrap();
        assert_eq!(x_range, (90.0, 40.0));
    }

    #[test]
    fn a_zoom_or_rectangle_on_an_invalid_log_bound_gives_nothing() {
        let t = PanelTransform::new(
            (0.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (0.0, 100.0),
            AxisScale::Log,
            AxisScale::Linear,
        )
        .expect("valid area");
        assert!(t.zoom((50.0, 50.0), 0.5).is_none());
        assert!(t.range_between((10.0, 10.0), (60.0, 40.0)).is_none());
    }

    #[test]
    fn the_frame_surrounds_the_finite_values_with_a_margin() {
        let close = |got: (f64, f64), want: (f64, f64)| {
            assert!(
                (got.0 - want.0).abs() < 1e-12 && (got.1 - want.1).abs() < 1e-12,
                "{got:?} instead of {want:?}"
            );
        };
        close(
            fit_range(&[0.0, 10.0, f64::NAN], AxisScale::Linear),
            (-0.5, 10.5),
        );
        close(fit_range(&[1.0, 3.0], AxisScale::Linear), (0.9, 3.1));
        close(fit_range(&[2.0, 2.0], AxisScale::Linear), (1.5, 2.5));
        close(fit_range(&[], AxisScale::Linear), (0.0, 1.0));
        close(
            fit_range(&[1.0, 10.0, 100.0], AxisScale::Log),
            (10f64.powf(-0.1), 10f64.powf(2.1)),
        );
        // Only the positives count in log; without them, one decade.
        close(
            fit_range(&[-5.0, 0.0, 1.0, 100.0], AxisScale::Log),
            (10f64.powf(-0.1), 10f64.powf(2.1)),
        );
        close(fit_range(&[-5.0, 0.0], AxisScale::Log), (1.0, 10.0));
    }

    #[test]
    fn an_axis_frame_follows_its_framing_and_direction() {
        let mut axis = AxisSpec::default();
        assert_eq!(axis_frame(&[0.0, 10.0], &axis).0, -0.5);
        axis.inverted = true;
        assert_eq!(axis_frame(&[0.0, 10.0], &axis), (10.5, -0.5));
        axis.auto_range = false;
        axis.range_min = Some(2.0);
        axis.range_max = Some(8.0);
        assert_eq!(axis_frame(&[0.0, 10.0], &axis), (8.0, 2.0));
        // Non-automatic explicit framing but incomplete: keep the data.
        axis.range_max = None;
        assert_eq!(axis_frame(&[0.0, 10.0], &axis), (10.5, -0.5));
        // Explicit framing ignored as long as the framing is automatic.
        axis.range_max = Some(8.0);
        axis.auto_range = true;
        axis.inverted = false;
        assert_eq!(axis_frame(&[0.0, 10.0], &axis), (-0.5, 10.5));
    }

    /// #269: the four combinations "bounds in order or not" ×
    /// "Inverted", like the graph (a single flip).
    #[test]
    fn a_decreasing_axis_frame_is_flipped_only_once() {
        let mut axis = AxisSpec {
            auto_range: false,
            ..AxisSpec::default()
        };
        for (min, max, inverted, expected) in [
            (400.0, 4000.0, false, (400.0, 4000.0)),
            (4000.0, 400.0, false, (4000.0, 400.0)),
            (400.0, 4000.0, true, (4000.0, 400.0)),
            (4000.0, 400.0, true, (4000.0, 400.0)),
        ] {
            axis.range_min = Some(min);
            axis.range_max = Some(max);
            axis.inverted = inverted;
            assert_eq!(axis_frame(&[500.0, 3000.0], &axis), expected);
        }
    }

    #[test]
    fn snapping_takes_the_closest_point_and_averages_around_it() {
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0];
        let ys = [10.0, 20.0, 60.0, 20.0, 10.0];
        assert_eq!(snap_to_curve(&xs, &ys, 2.2, 1), Some((2.0, 60.0)));
        let (x, y) = snap_to_curve(&xs, &ys, 2.2, 3).unwrap();
        assert_eq!(x, 2.0);
        assert!((y - 100.0 / 3.0).abs() < 1e-12);
        // Even n: exactly n points, the surplus on the right.
        let (_, y) = snap_to_curve(&xs, &ys, 2.2, 2).unwrap();
        assert!((y - 40.0).abs() < 1e-12);
        let (_, y) = snap_to_curve(&xs, &ys, 2.2, 4).unwrap();
        assert!((y - 110.0 / 4.0).abs() < 1e-12);
        // At the edge, the window is truncated instead of overflowing.
        assert_eq!(snap_to_curve(&xs, &ys, -5.0, 3), Some((0.0, 15.0)));
        assert_eq!(snap_to_curve(&[], &[], 1.0, 3), None);
    }

    #[test]
    fn snapping_does_not_panic_on_inconsistent_data() {
        // `ys` shorter than `xs`: nothing, and above all no panic.
        assert_eq!(snap_to_curve(&[0.0, 1.0, 2.0], &[1.0], 2.0, 3), None);
        // A non-finite abscissa is never chosen; a non-finite ordinate
        // is dropped from the average.
        let xs = [f64::NAN, 1.0, 2.0];
        let ys = [5.0, f64::NAN, 7.0];
        assert_eq!(snap_to_curve(&xs, &ys, 1.1, 3), Some((1.0, 6.0)));
        assert_eq!(snap_to_curve(&[f64::NAN], &[1.0], 0.0, 1), None);
        assert_eq!(snap_to_curve(&[1.0], &[f64::NAN], 1.0, 1), None);
    }

    #[test]
    fn the_snapping_area_follows_the_averaged_window() {
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0];
        let ys = [10.0, 20.0, 60.0, 20.0, 10.0];
        let area = |x, n| {
            let s = snap_to_curve_zone(&xs, &ys, x, n).unwrap();
            (s.x_min, s.x_max, s.count)
        };
        assert_eq!(area(2.2, 1), (2.0, 2.0, 1));
        assert_eq!(area(2.2, 3), (1.0, 3.0, 3));
        // Even n: the surplus on the right.
        assert_eq!(area(2.2, 4), (1.0, 4.0, 4));
        // At the edge, the area follows the truncated window, not n.
        assert_eq!(area(-5.0, 3), (0.0, 1.0, 2));
        assert_eq!(area(10.0, 3), (3.0, 4.0, 2));
        // Unsorted abscissas: minimum and maximum of the window.
        let xs = [5.0, 1.0, 3.0, 9.0, 2.0];
        let s = snap_to_curve_zone(&xs, &ys, 3.1, 5).unwrap();
        assert_eq!((s.x_min, s.x_max, s.count), (1.0, 9.0, 5));
        // Window 3, 9, 2: neither its first nor its last point are the bounds.
        let s = snap_to_curve_zone(&xs, &ys, 9.1, 3).unwrap();
        assert_eq!((s.x_min, s.x_max), (2.0, 9.0));
        let s = snap_to_curve_zone(&xs, &ys, 3.1, 1).unwrap();
        assert_eq!((s.x_min, s.x_max), (3.0, 3.0));
    }
}

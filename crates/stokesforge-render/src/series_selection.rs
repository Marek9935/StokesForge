//! Picking series by the pixels actually drawn (#83), and handles of the
//! selected series (#98).
//!
//! The pipeline supplies these paths after decimation and clipping, with the
//! original rank. No second axis computation, no resolution on mouse
//! movement: right axis, histogram and curve follow the same path as the
//! rendering.
//!
//! # Selection handles (#98)
//!
//! A selected series used to be repainted in blue over its drawing, which
//! hid its color and style. It is now flagged as in OriginLab: small solid
//! squares ([`HANDLE_SIDE`]) placed **along the drawn path** at a fixed
//! interval ([`HANDLE_SPACING`]), the drawing itself being left untouched.
//! The spacing is measured in **display** pixels, along the path actually
//! drawn — not per data point —, to stay readable whatever the data density,
//! the LTTB decimation or the window enlargement; hence the `to_display`
//! conversion that the caller supplies to [`DrawnSeries::handle_centers`].
//!
//! What has no path follows the same rule on discrete positions
//! ([`thinned`]): a scatter plot gets its squares on a regular subset of its
//! markers, bars at the middle of their free end (value side). Whiskers
//! carry no handle: they serve to pick the series, not to flag it.
//!
//! The color is the automatic ink of the figure's background
//! ([`crate::pipeline::figure_ink`]): black on a light background, white on
//! a dark one, independent of the series color.
//!
//! The handles are a **view** state, not a model one: they do not go through
//! the pipeline (the figure, its cache and the SVG/PDF/PNG export know
//! nothing of them), but through [`paint_handles`], generic over the backend,
//! which the screen calls over the cached figure.

use crate::{DrawnPlot, PanelTransform};
use plotters::style::RGBColor;
use plotters_backend::{DrawingBackend, DrawingErrorKind};

type Pixel = (i32, i32);

/// Side of a selection handle, in display pixels — independent of the zoom
/// and of the window size.
pub const HANDLE_SIDE: f64 = 5.0;

/// Spacing of the handles along the path, in display pixels: tight enough
/// to follow the shape of a curve, loose enough not to cover it (a 5 px
/// square every 50 px hides 10% of the line).
pub const HANDLE_SPACING: f64 = 50.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawnSeries {
    pub index: usize,
    /// The segments of the drawn line, after clipping, at a sixteenth of a pixel.
    pub paths: Vec<Vec<(f64, f64)>>,
    /// The whiskers (error bars), after clipping: pickable, without a
    /// handle.
    pub whiskers: Vec<(Pixel, Pixel)>,
    /// The visible markers.
    pub points: Vec<Pixel>,
    /// The visible bars, as normalized rectangles (top-left corner,
    /// bottom-right corner), clipped.
    pub bars: Vec<(Pixel, Pixel)>,
    /// For each bar of `bars`, the middle of its free end (value side, not
    /// baseline side), clipped: the place of its handle.
    pub bar_tips: Vec<Pixel>,
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length
    }
    .clamp(0.0, 1.0);
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

impl DrawnSeries {
    pub fn distance(&self, pixel: (f64, f64)) -> f64 {
        let paths = self
            .paths
            .iter()
            .flat_map(|path| path.windows(2))
            .map(|s| segment_distance(pixel, s[0], s[1]));
        let whiskers = self.whiskers.iter().map(|&(a, b)| {
            segment_distance(pixel, (a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64))
        });
        let points = self
            .points
            .iter()
            .map(|&p| segment_distance(pixel, (p.0 as f64, p.1 as f64), (p.0 as f64, p.1 as f64)));
        let bars = self.bars.iter().map(|&(a, b)| {
            let x = pixel.0.clamp(a.0 as f64, b.0 as f64);
            let y = pixel.1.clamp(a.1 as f64, b.1 as f64);
            (pixel.0 - x).hypot(pixel.1 - y)
        });
        paths
            .chain(whiskers)
            .chain(points)
            .chain(bars)
            .fold(f64::INFINITY, f64::min)
    }

    /// Centers of the selection handles, in display pixels.
    ///
    /// `to_display` converts a figure pixel into a display pixel (the screen
    /// enlarges or shrinks the figure): the [`HANDLE_SPACING`] spacing is
    /// measured **after** this conversion. A line carries its handles along
    /// its segments ([`along_path`]); without a line, the markers then the
    /// bars get a regular subset of them ([`thinned`]).
    pub fn handle_centers(&self, to_display: impl Fn((f64, f64)) -> (f64, f64)) -> Vec<(f64, f64)> {
        if !self.paths.is_empty() {
            return self
                .paths
                .iter()
                .flat_map(|path| {
                    let path: Vec<_> = path.iter().map(|&p| to_display(p)).collect();
                    along_path(&path, HANDLE_SPACING)
                })
                .collect();
        }
        if !self.points.is_empty() {
            return thinned(
                self.points
                    .iter()
                    .map(|&p| to_display((p.0 as f64, p.1 as f64))),
                HANDLE_SPACING,
            );
        }
        thinned(
            self.bar_tips
                .iter()
                .map(|&p| to_display((p.0 as f64, p.1 as f64))),
            HANDLE_SPACING,
        )
    }
}

/// Points placed along a polyline, every `spacing` pixels of distance
/// traveled: the first at the start, then one at each `spacing`, and the end
/// in addition if the last handle is more than half an interval away from
/// it — both ends of a segment are thus always marked. A segment of a
/// single point (or of zero length) gets one.
pub fn along_path(path: &[(f64, f64)], spacing: f64) -> Vec<(f64, f64)> {
    let Some(&first) = path.first() else {
        return Vec::new();
    };
    let mut handles = vec![first];
    // Distance traveled since the last handle.
    let mut since = 0.0;
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        if !length.is_finite() || length == 0.0 {
            continue;
        }
        // Abscissa, on this segment, of the next handle.
        let mut next = spacing - since;
        while next <= length {
            let t = next / length;
            handles.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
            next += spacing;
        }
        since = length - (next - spacing);
    }
    let last = path[path.len() - 1];
    if since > spacing / 2.0 {
        handles.push(last);
    }
    handles
}

/// Regular subset of discrete positions (markers, bar vertices), in order:
/// the first is kept, then every position at least `spacing` pixels from the
/// last one kept.
pub fn thinned(positions: impl IntoIterator<Item = (f64, f64)>, spacing: f64) -> Vec<(f64, f64)> {
    let mut kept: Vec<(f64, f64)> = Vec::new();
    for p in positions {
        let far = kept
            .last()
            .is_none_or(|last| (p.0 - last.0).hypot(p.1 - last.1) >= spacing);
        if far {
            kept.push(p);
        }
    }
    kept
}

/// The square of a handle centered on `center`, with whole-number corners
/// (Plotters reasons in integers): side exactly [`HANDLE_SIDE`].
pub fn handle_square((x, y): (f64, f64)) -> (Pixel, Pixel) {
    let half = HANDLE_SIDE / 2.0;
    let left = (x - half).round() as i32;
    let top = (y - half).round() as i32;
    let side = HANDLE_SIDE as i32;
    ((left, top), (left + side, top + side))
}

/// Paints the selection handles, solid squares of color `ink`, on any
/// backend — written once, here, like the rest of the rendering
/// (invariant 5). The centers are in the backend's frame.
pub fn paint_handles<DB: DrawingBackend>(
    backend: &mut DB,
    centers: &[(f64, f64)],
    ink: (u8, u8, u8),
) -> Result<(), DrawingErrorKind<DB::ErrorType>> {
    let color = RGBColor(ink.0, ink.1, ink.2);
    for &center in centers {
        let (upper_left, bottom_right) = handle_square(center);
        backend.draw_rect(upper_left, bottom_right, &color, true)?;
    }
    Ok(())
}

/// Layer index and original rank within the declared layer. On a tie, the last one drawn wins.
pub fn series_at(plot: &DrawnPlot, pixel: (f64, f64)) -> Option<(usize, usize)> {
    for panel in plot.panels.iter().rev() {
        if !PanelTransform::of(panel)?.contains(pixel.0, pixel.1) {
            continue;
        }
        let mut best = None;
        let mut distance = 5.0;
        for series in panel.series_geometry.iter().rev() {
            let candidate = series.distance(pixel);
            if candidate < distance {
                distance = candidate;
                best = Some((panel.index, series.index));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn segments_and_bars_are_picked_between_their_points() {
        let line = DrawnSeries {
            paths: vec![vec![(0.0, 0.0), (100.0, 100.0)]],
            ..Default::default()
        };
        assert!(line.distance((50.0, 50.0)) < 0.1);
        assert!(line.distance((20.0, 80.0)) > 30.0);
        let bars = DrawnSeries {
            bars: vec![((10, 20), (30, 70))],
            ..Default::default()
        };
        assert_eq!(bars.distance((20.0, 40.0)), 0.0);
        assert_eq!(bars.distance((40.0, 40.0)), 10.0);
        let whiskers = DrawnSeries {
            whiskers: vec![((10, 0), (10, 40))],
            ..Default::default()
        };
        assert_eq!(whiskers.distance((10.0, 20.0)), 0.0);
        assert!(DrawnSeries::default().distance((0.0, 0.0)).is_infinite());
    }

    #[test]
    fn handles_are_spaced_along_the_path_not_per_point() {
        // A bend: 0 → 120 in x, then 80 downward. Length 200.
        let path = [(0.0, 0.0), (120.0, 0.0), (120.0, 80.0)];
        let handles = along_path(&path, 50.0);
        let expected = [
            (0.0, 0.0),
            (50.0, 0.0),
            (100.0, 0.0),
            // 20 px remaining of the first segment + 30 px on the second.
            (120.0, 30.0),
            (120.0, 80.0),
        ];
        assert_eq!(handles.len(), expected.len(), "{handles:?}");
        for (got, want) in handles.iter().zip(expected) {
            assert!(close(*got, want), "{got:?} ≠ {want:?}");
        }
        // The same path split into a hundred points gives the same handles:
        // the spacing does not depend on the data density.
        let dense: Vec<_> = (0..=60)
            .map(|i| (i as f64 * 2.0, 0.0))
            .chain((1..=40).map(|i| (120.0, i as f64 * 2.0)))
            .collect();
        let dense_handles = along_path(&dense, 50.0);
        assert_eq!(dense_handles.len(), handles.len());
        for (got, want) in dense_handles.iter().zip(&handles) {
            assert!(close(*got, *want), "{got:?} ≠ {want:?}");
        }
    }

    #[test]
    fn the_end_is_marked_only_if_it_is_far_from_the_last_handle() {
        // 110 px: handles at 0, 50, 100; the end (10 px further) does not
        // double the last one.
        assert_eq!(along_path(&[(0.0, 0.0), (110.0, 0.0)], 50.0).len(), 3);
        // 140 px: the end is 40 px from the handle at 100, it is marked.
        let handles = along_path(&[(0.0, 0.0), (140.0, 0.0)], 50.0);
        assert_eq!(handles.len(), 4);
        assert!(close(handles[3], (140.0, 0.0)));
        // A segment reduced to a point gets one; an empty path gets none.
        assert_eq!(along_path(&[(3.0, 4.0)], 50.0), vec![(3.0, 4.0)]);
        assert_eq!(
            along_path(&[(3.0, 4.0), (3.0, 4.0)], 50.0),
            vec![(3.0, 4.0)]
        );
        assert!(along_path(&[], 50.0).is_empty());
    }

    #[test]
    fn spacing_is_measured_in_display_pixels() {
        let series = DrawnSeries {
            paths: vec![vec![(0.0, 0.0), (200.0, 0.0)]],
            ..Default::default()
        };
        let native = series.handle_centers(|(x, y)| (x, y));
        let doubled = series.handle_centers(|(x, y)| (x * 2.0, y * 2.0));
        assert_eq!(native.len(), 5);
        // Figure displayed twice as large: twice as many handles,
        // still HANDLE_SPACING apart on the screen.
        assert_eq!(doubled.len(), 9);
        assert!(close(doubled[1], (HANDLE_SPACING, 0.0)));
    }

    #[test]
    fn a_scatter_and_bars_carry_a_regular_subset() {
        // Markers every 10 px: one in five.
        let scatter = DrawnSeries {
            points: (0..=20).map(|i| (i * 10, 0)).collect(),
            ..Default::default()
        };
        let handles = scatter.handle_centers(|(x, y)| (x, y));
        assert_eq!(
            handles,
            vec![
                (0.0, 0.0),
                (50.0, 0.0),
                (100.0, 0.0),
                (150.0, 0.0),
                (200.0, 0.0)
            ]
        );
        // Bars 60 px apart: each its own, on its free end.
        let bars = DrawnSeries {
            bars: vec![((0, 10), (40, 100)), ((60, 100), (100, 150))],
            bar_tips: vec![(20, 10), (80, 150)],
            ..Default::default()
        };
        assert_eq!(
            bars.handle_centers(|(x, y)| (x, y)),
            vec![(20.0, 10.0), (80.0, 150.0)]
        );
        // A line with markers: the handles follow the line, not the
        // markers.
        let line_and_symbols = DrawnSeries {
            paths: vec![vec![(0.0, 0.0), (100.0, 0.0)]],
            points: vec![(0, 0), (7, 0), (100, 0)],
            ..Default::default()
        };
        assert_eq!(
            line_and_symbols.handle_centers(|(x, y)| (x, y)),
            vec![(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)]
        );
        // Whiskers alone carry none.
        let whiskers = DrawnSeries {
            whiskers: vec![((0, 0), (0, 100))],
            ..Default::default()
        };
        assert!(whiskers.handle_centers(|(x, y)| (x, y)).is_empty());
    }

    #[test]
    fn a_handle_is_a_centered_square_of_fixed_side() {
        let ((left, top), (right, bottom)) = handle_square((100.0, 40.0));
        assert_eq!(right - left, HANDLE_SIDE as i32);
        assert_eq!(bottom - top, HANDLE_SIDE as i32);
        let center = ((left + right) as f64 / 2.0, (top + bottom) as f64 / 2.0);
        assert!((center.0 - 100.0).abs() <= 0.5 && (center.1 - 40.0).abs() <= 0.5);
    }
}

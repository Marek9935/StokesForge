//! Drawing primitives, in **pixels** — tasks R5.1 and R5.3.
//!
//! # Why drawing goes down to pixels rather than staying in data units
//!
//! R4 entrusted the curve to `ChartContext::draw_series(LineSeries::new(…))`,
//! which was enough for a solid line. Three R5 needs no longer go through
//! there:
//!
//! - **line styles** ([`stokesforge_plot::LineStyle`]): a dash is measured in
//!   pixels, not in data units. Plotters' `DashedLineSeries` only knows a
//!   two-beat pattern, so neither `DashDot`, nor a dotted line whose dot
//!   stays round whatever the scale;
//! - **markers** ([`stokesforge_plot::SymbolShape`]): Plotters has no ready-made
//!   element for the square, the diamond or the straight cross;
//! - **the legend**: Plotters' is sized on `FontData::estimate_layout`, that
//!   is — our Plotters fonts being cut down — on the UTF-8 byte heuristic of
//!   §4.4. The frame would be measured wrong **with nothing to signal it**,
//!   even though the text inside would be drawn right. See [`crate::legend`].
//!
//! Curves must not be rounded to the whole pixel: this rounding makes steps
//! when zooming out on smooth data (#150, real Jasco spectrum). The pipeline
//! keeps projection and clipping in floating point, then [`polyline_f64`]
//! uses the shared fixed-point of [`crate::subpixel`]. Both backends thus
//! receive the same path at a sixteenth of a pixel, dashes included; no
//! smoothing of the data is done.
//!
//! # Overflow was mishandled by Plotters — fixed on 2026-09-13
//!
//! Everything painted here goes through [`plotters::drawing::DrawingArea`],
//! whose coordinate mapping **truncates to the area's rectangle**
//! (`element/mod.rs`: `BackendCoordOnly::map` calls
//! `rect.truncate(coord_trans.translate(from))`, and this is the path that
//! `area.draw(&PathElement::new(…))` takes below). A point outside the
//! framing is thus brought back to the edge rather than painted over the
//! ticks, **per component**: a segment that crosses the area diagonally had
//! its end lie down on the edge instead of being cut at the real
//! intersection. Invisible in R4 as long as the framing followed the data
//! (the 5% margin of `pipeline::padded` always left some slack), the defect
//! became visible as soon as an explicit framing tightened the window (wheel
//! zoom, rubber-band rectangle — R7.1/7.4): most of a series' points then
//! fall outside the panel, `map_coordinate` extrapolates them linearly to
//! very distant pixel coordinates, and `rect.truncate` brought them back to
//! the edge one axis at a time rather than to the segment/rectangle
//! intersection. Documented and postponed twice (R5, then re-confirmed in R7
//! without being dealt with).
//!
//! **Fixed** by a real geometric clipping (Liang-Barsky, on the plot
//! rectangle, **in pixels** — the space where the projection stays affine
//! even for a logarithmic axis): [`crate::clip`]. It sits between the
//! data→pixel projection and the call to [`polyline`] below (wired in
//! `pipeline::paint_series`): a segment that crosses the rectangle is cut at
//! the exact crossing point, a segment entirely outside the rectangle is
//! omitted rather than flattened onto an edge, and a polyline that leaves and
//! re-enters several times becomes several disjoint sub-polylines instead of
//! a single one that would run along the edge. Without an explicit framing
//! that overflows the data (the common case), every segment stays entirely
//! inside the rectangle and this clipping is a visual no-op. Pure unit tests
//! in `crate::clip` (crossing segment, entirely outside, entirely inside,
//! tangent to an edge) and end-to-end test
//! `a_narrow_framing_no_longer_presses_the_curve_against_an_edge`
//! (`stokesforge-render-egui/tests/shapes.rs`), next to
//! `a_narrow_zoomed_framing_stays_bounded_to_the_rectangle`, which already
//! checked, before the fix, what remained true despite the defect.

use plotters::coord::Shift;
use plotters::element::{Circle, PathElement, Polygon, Rectangle};
use plotters::prelude::*;
use plotters_backend::{BackendCoord, DrawingBackend};
use stokesforge_plot::{LineSpec, LineStyle, SymbolShape, SymbolSpec};

use crate::color::rgba_or;

/// Fallback color of a line whose spec string is unreadable.
pub const SERIES_FALLBACK: (u8, u8, u8) = (0x4c, 0x78, 0xa8);
/// Color of text, axes and outlines on a light background — the automatic
/// ink of a light background ([`crate::color::auto_ink`]).
pub const INK: (u8, u8, u8) = crate::color::DARK_INK;
/// Fallback background when `PlotSpec::background` is empty or unreadable.
pub const PAPER: (u8, u8, u8) = (0xff, 0xff, 0xff);
/// Gray of series with a broken reference: present, and visibly inert.
pub const BROKEN: (u8, u8, u8) = (0x9a, 0x9a, 0x9a);

/// Fill and outline of the triangle of a stale series (#198): amber and brown.
pub const STALE_FILL: (u8, u8, u8) = (0xf2, 0xb1, 0x34);
pub const STALE_EDGE: (u8, u8, u8) = (0x5a, 0x3a, 0x00);

/// Beyond this number of dashes for a single polyline, the pattern is
/// abandoned in favor of a solid line.
///
/// A guard, not a setting: nothing bounds the number of points of a column
/// nor the length of the resulting path, and a three-pixel dotted line over a
/// path of a million pixels would produce hundreds of thousands of shapes —
/// enough to bring rendering down for a detail invisible at that density.
pub const MAX_DASHES: usize = 20_000;

/// A Plotters color from a triplet.
pub fn rgb((r, g, b): (u8, u8, u8)) -> RGBColor {
    RGBColor(r, g, b)
}

/// A Plotters color with its opacity, from a quadruplet.
pub fn rgba((r, g, b, a): (u8, u8, u8, u8)) -> RGBAColor {
    RGBAColor(r, g, b, a as f64 / 255.0)
}

/// An opaque triplet.
pub fn opaque((r, g, b): (u8, u8, u8)) -> (u8, u8, u8, u8) {
    (r, g, b, 255)
}

/// Line width: `LineSpec::width` is a free `f64` on the spec side (nothing
/// bounds it), `ShapeStyle::stroke_width` is a `u32`.
pub fn stroke_width(width: f64) -> u32 {
    if width.is_finite() && width >= 1.0 {
        // Capped: a width coming from a file can be anything, and Plotters'
        // `polygonize` allocates proportionally.
        (width.round() as u32).min(64)
    } else {
        1
    }
}

/// The same line in gray: a stale series (#198) reads as a series whose
/// reference is broken, without losing it.
pub fn grayed(line: &LineSpec) -> LineSpec {
    let (r, g, b) = BROKEN;
    LineSpec {
        color: format!("#{r:02x}{g:02x}{b:02x}"),
        ..line.clone()
    }
}

/// The marker of a stale series: a warning triangle carrying a « ! »,
/// centered on `at`. Drawn with primitives (no text, hence no measurement or
/// font) so it is identical on all backends.
pub fn stale_marker<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    at: BackendCoord,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    const HALF: i32 = 8;
    let (x, y) = at;
    let top = (x, y - HALF);
    let left = (x - HALF, y + HALF - 2);
    let right = (x + HALF, y + HALF - 2);
    let fill = ShapeStyle {
        color: rgba(opaque(STALE_FILL)),
        filled: true,
        stroke_width: 1,
    };
    let edge = ShapeStyle {
        color: rgba(opaque(STALE_EDGE)),
        filled: false,
        stroke_width: 1,
    };
    area.draw(&Polygon::new(vec![top, left, right], fill))?;
    area.draw(&PathElement::new(vec![top, left, right, top], edge))?;
    // The « ! »: a stroke and a dot.
    area.draw(&PathElement::new(vec![(x, y - 3), (x, y + 2)], edge))?;
    area.draw(&Circle::new((x, y + 5), 1, edge))?;
    Ok(())
}

/// A series' line style, ready for Plotters.
pub fn line_style(line: &LineSpec, broken: bool) -> ShapeStyle {
    let color = if broken {
        opaque(BROKEN)
    } else {
        rgba_or(&line.color, opaque(SERIES_FALLBACK))
    };
    ShapeStyle {
        color: rgba(color),
        filled: false,
        stroke_width: stroke_width(line.width),
    }
}

/// The pattern of a line style, in pixels: alternating solid/empty lengths.
/// `None` for a continuous line.
pub fn dash_pattern(style: LineStyle) -> Option<&'static [f64]> {
    match style {
        LineStyle::None | LineStyle::Solid => None,
        LineStyle::Dash => Some(&[6.0, 4.0]),
        LineStyle::Dot => Some(&[1.0, 3.0]),
        LineStyle::DashDot => Some(&[6.0, 3.0, 1.0, 3.0]),
    }
}

/// Splits a polyline according to a dash pattern, in pixels.
///
/// Returns the **solid** runs, in order. A pattern whose period is zero or
/// whose splitting would exceed [`MAX_DASHES`] returns the whole polyline, as
/// a single run: better a solid line than a rendering that collapses.
pub fn dashes(points: &[BackendCoord], pattern: &[f64]) -> Vec<Vec<BackendCoord>> {
    let points: Vec<_> = points.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
    dashes_f64(&points, pattern)
        .into_iter()
        .map(|run| {
            run.into_iter()
                .map(|(x, y)| (x.round() as i32, y.round() as i32))
                .collect()
        })
        .collect()
}

/// Splits the dashes without rounding their ends or the length traveled
/// (sub-pixel curves, #150). Same guard as [`dashes`].
pub fn dashes_f64(points: &[(f64, f64)], pattern: &[f64]) -> Vec<Vec<(f64, f64)>> {
    let period: f64 = pattern.iter().sum();
    if points.len() < 2 || pattern.is_empty() || !period.is_finite() || period <= 0.0 {
        return vec![points.to_vec()];
    }
    let total: f64 = points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum();
    if !total.is_finite() || total / period * pattern.len() as f64 > MAX_DASHES as f64 {
        return vec![points.to_vec()];
    }

    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut current: Vec<(f64, f64)> = vec![points[0]];
    let mut on = true;
    let mut index = 0usize;
    let mut remaining = pattern[0];

    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = distance(a, b);
        if length <= 0.0 {
            continue;
        }
        let mut travelled = 0.0;
        while length - travelled > remaining {
            travelled += remaining;
            let cut = lerp(a, b, travelled / length);
            if on {
                current.push(cut);
                if current.len() >= 2 {
                    runs.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            } else {
                current = vec![cut];
            }
            on = !on;
            index = (index + 1) % pattern.len();
            remaining = pattern[index];
        }
        remaining -= length - travelled;
        if on {
            current.push(b);
        }
    }
    if on && current.len() >= 2 {
        runs.push(current);
    }
    runs
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    dx.hypot(dy)
}

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Draws a polyline, taking the line style into account.
///
/// [`LineStyle::None`] draws nothing — that is what the variant says.
pub fn polyline<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    points: &[BackendCoord],
    line: &LineSpec,
    broken: bool,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if points.len() < 2 || line.style == LineStyle::None {
        return Ok(());
    }
    let style = line_style(line, broken);
    match dash_pattern(line.style) {
        None => area.draw(&PathElement::new(points.to_vec(), style)),
        Some(pattern) => {
            for run in dashes(points, pattern) {
                if run.len() >= 2 {
                    area.draw(&PathElement::new(run, style))?;
                }
            }
            Ok(())
        }
    }
}

/// Draws a curve without quantizing it to the whole pixel (#150). The shared
/// fixed path of both backends also keeps the ends of dashes.
pub fn polyline_f64<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    points: &[(f64, f64)],
    line: &LineSpec,
    broken: bool,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if points.len() < 2 || line.style == LineStyle::None {
        return Ok(());
    }
    let style = line_style(line, broken);
    match dash_pattern(line.style) {
        None => area.draw(&crate::subpixel::SubpixelPath::new(points, style)),
        Some(pattern) => {
            for run in dashes_f64(points, pattern) {
                if run.len() >= 2 {
                    area.draw(&crate::subpixel::SubpixelPath::new(&run, style))?;
                }
            }
            Ok(())
        }
    }
}

/// Radius of a marker, in pixels.
fn radius(size: f64) -> i32 {
    if size.is_finite() && size >= 2.0 {
        ((size / 2.0).round() as i32).min(64)
    } else {
        1
    }
}

/// Puts a marker on each point.
pub fn symbols<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    points: &[BackendCoord],
    symbol: &SymbolSpec,
    broken: bool,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if symbol.shape == SymbolShape::None {
        return Ok(());
    }
    for &point in points {
        symbol_at(area, point, symbol, broken)?;
    }
    Ok(())
}

/// Puts a marker at a given place — also what the legend draws in its
/// thumbnail, hence the separate function.
pub fn symbol_at<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    (x, y): BackendCoord,
    symbol: &SymbolSpec,
    broken: bool,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if symbol.shape == SymbolShape::None {
        return Ok(());
    }
    let r = radius(symbol.size);
    let fill_color = if broken {
        opaque(BROKEN)
    } else {
        rgba_or(&symbol.color, opaque(SERIES_FALLBACK))
    };
    let edge_color = if broken {
        opaque(BROKEN)
    } else {
        rgba_or(symbol.effective_edge_color(), fill_color)
    };
    let fill = ShapeStyle {
        color: rgba(fill_color),
        filled: true,
        stroke_width: 1,
    };
    let edge = ShapeStyle {
        color: rgba(edge_color),
        filled: false,
        stroke_width: 1,
    };

    match symbol.shape {
        SymbolShape::None => {}
        SymbolShape::Circle => {
            area.draw(&Circle::new((x, y), r as u32, fill))?;
            area.draw(&Circle::new((x, y), r as u32, edge))?;
        }
        SymbolShape::Square => {
            let corners = [(x - r, y - r), (x + r, y + r)];
            area.draw(&Rectangle::new(corners, fill))?;
            area.draw(&Rectangle::new(corners, edge))?;
        }
        SymbolShape::Triangle => {
            let vertices = vec![(x, y - r), (x + r, y + r), (x - r, y + r)];
            area.draw(&Polygon::new(vertices.clone(), fill))?;
            area.draw(&PathElement::new(closed(vertices), edge))?;
        }
        SymbolShape::Diamond => {
            let vertices = vec![(x, y - r), (x + r, y), (x, y + r), (x - r, y)];
            area.draw(&Polygon::new(vertices.clone(), fill))?;
            area.draw(&PathElement::new(closed(vertices), edge))?;
        }
        SymbolShape::Plus => {
            area.draw(&PathElement::new(vec![(x - r, y), (x + r, y)], edge))?;
            area.draw(&PathElement::new(vec![(x, y - r), (x, y + r)], edge))?;
        }
        SymbolShape::Cross => {
            area.draw(&PathElement::new(
                vec![(x - r, y - r), (x + r, y + r)],
                edge,
            ))?;
            area.draw(&PathElement::new(
                vec![(x - r, y + r), (x + r, y - r)],
                edge,
            ))?;
        }
    }
    Ok(())
}

fn closed(mut vertices: Vec<BackendCoord>) -> Vec<BackendCoord> {
    if let Some(&first) = vertices.first() {
        vertices.push(first);
    }
    vertices
}

/// Half-width of a whisker cap, in pixels.
const WHISKER_CAP: i32 = 3;

/// Draws the error-bar whiskers, caps included.
pub fn whiskers<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    segments: &[(BackendCoord, BackendCoord)],
    line: &LineSpec,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let style = ShapeStyle {
        color: rgba(rgba_or(&line.color, opaque(SERIES_FALLBACK))),
        filled: false,
        stroke_width: 1,
    };
    for &(a, b) in segments {
        area.draw(&PathElement::new(vec![a, b], style))?;
        // The cap is perpendicular to the whisker; the orientation is read
        // from the segment itself rather than carried over from the
        // preparation, where it would be one more field to keep right.
        let vertical = (b.0 - a.0).abs() <= (b.1 - a.1).abs();
        for end in [a, b] {
            let cap = if vertical {
                vec![(end.0 - WHISKER_CAP, end.1), (end.0 + WHISKER_CAP, end.1)]
            } else {
                vec![(end.0, end.1 - WHISKER_CAP), (end.0, end.1 + WHISKER_CAP)]
            };
            area.draw(&PathElement::new(cap, style))?;
        }
    }
    Ok(())
}

/// Draws solid bars, outline included.
///
/// The outline is not a frill: two neighboring bars of the same color are
/// indistinguishable without it, and that is the nominal case of a
/// histogram.
pub fn bars<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    rects: &[(BackendCoord, BackendCoord)],
    line: &LineSpec,
    edge_color: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let color = rgba_or(&line.color, opaque(SERIES_FALLBACK));
    let fill = ShapeStyle {
        color: rgba(color),
        filled: true,
        stroke_width: 1,
    };
    let edge = ShapeStyle {
        color: rgb(edge_color).into(),
        filled: false,
        stroke_width: 1,
    };
    for &(a, b) in rects {
        area.draw(&Rectangle::new([a, b], fill))?;
        area.draw(&Rectangle::new([a, b], edge))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solid_line_has_no_pattern() {
        assert!(dash_pattern(LineStyle::Solid).is_none());
        assert!(dash_pattern(LineStyle::None).is_none());
        assert_eq!(dash_pattern(LineStyle::Dash), Some(&[6.0, 4.0][..]));
    }

    #[test]
    fn dash_splitting_alternates_solid_and_empty() {
        // 100 px long, pattern 6 solid / 4 empty: ten periods, hence ten
        // solid runs (the last one falls exactly on the end).
        let line = [(0, 0), (100, 0)];
        let runs = dashes(&line, &[6.0, 4.0]);
        assert_eq!(runs.len(), 10, "{runs:?}");
        assert_eq!(runs[0], vec![(0, 0), (6, 0)]);
        assert_eq!(runs[1], vec![(10, 0), (16, 0)]);
        assert!(runs.iter().all(|run| run.len() == 2));
    }

    #[test]
    fn the_pattern_runs_through_the_polyline_vertices() {
        // The pattern must not reset at each vertex, otherwise a noisy
        // signal would become a solid line again.
        let line = [(0, 0), (3, 0), (6, 0), (100, 0)];
        let broken = dashes(&line, &[6.0, 4.0]);
        let straight = dashes(&[(0, 0), (100, 0)], &[6.0, 4.0]);
        assert_eq!(broken.len(), straight.len());
    }

    #[test]
    fn dashes_keep_fractional_coordinates() {
        let runs = dashes_f64(&[(0.25, 1.375), (100.25, 1.375)], &[6.0, 4.0]);
        assert_eq!(runs.len(), 10);
        assert_eq!(runs[0], vec![(0.25, 1.375), (6.25, 1.375)]);
        assert_eq!(runs[1], vec![(10.25, 1.375), (16.25, 1.375)]);
        // Changing the vertex density changes neither the step nor the phase.
        let dense = dashes_f64(
            &[(0.25, 1.375), (3.25, 1.375), (6.25, 1.375), (100.25, 1.375)],
            &[6.0, 4.0],
        );
        assert_eq!(dense.len(), runs.len());
        for (dense, sparse) in dense.iter().zip(&runs) {
            // Compare the coordinates sent to the backends: floating-point
            // computations may differ in the last bit before encoding.
            assert_eq!(
                dense.first().copied().map(crate::subpixel::encode),
                sparse.first().copied().map(crate::subpixel::encode)
            );
            assert_eq!(
                dense.last().copied().map(crate::subpixel::encode),
                sparse.last().copied().map(crate::subpixel::encode)
            );
        }
    }

    #[test]
    fn an_absurd_pattern_falls_back_to_a_solid_line() {
        let line = [(0, 0), (100, 0)];
        assert_eq!(dashes(&line, &[]).len(), 1);
        assert_eq!(dashes(&line, &[0.0, 0.0]).len(), 1);
    }

    #[test]
    fn a_huge_path_falls_back_to_a_solid_line() {
        // The MAX_DASHES guard: without it, a dotted line over a path of a
        // million pixels would produce hundreds of thousands of shapes.
        let line = [(0, 0), (10_000_000, 0)];
        assert_eq!(dashes(&line, &[1.0, 3.0]).len(), 1);
    }

    #[test]
    fn width_is_bounded_on_both_sides() {
        assert_eq!(stroke_width(0.0), 1);
        assert_eq!(stroke_width(f64::NAN), 1);
        assert_eq!(stroke_width(2.4), 2);
        assert_eq!(stroke_width(1e9), 64);
    }

    #[test]
    fn a_marker_radius_is_bounded_too() {
        assert_eq!(radius(7.0), 4);
        assert_eq!(radius(0.0), 1);
        assert_eq!(radius(f64::INFINITY), 1);
        assert_eq!(radius(1e6), 64);
    }

    #[test]
    fn a_broken_series_takes_the_gray_and_not_its_color() {
        let line = LineSpec {
            color: "#ff0000".into(),
            ..LineSpec::default()
        };
        assert_eq!(line_style(&line, true).color.rgb(), BROKEN);
        assert_eq!(line_style(&line, false).color.rgb(), (0xff, 0, 0));
    }
}

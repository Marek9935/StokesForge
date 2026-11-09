//! The colour bar of a layer (issue #499, step C).
//!
//! This module holds what the figure needs to **colour its curves by value**
//! ([`value_colors`]) and to **draw the bar** ([`draw`], [`band`]).
//!
//! The colour is computed at each draw and handed to the painting as a copy of
//! the style (`Prepared::value_color`); it is never written to the
//! `SeriesSpec`.
//!
//! # The bar
//!
//! The gradient is painted as **1-pixel stripes** (Plotters has no gradient
//! element, and a stripe is the same rectangle on screen and in the export).
//! A segment is as long as the span of its drawn range (the `chain` option
//! shifts the ranges, never the colours), segments are separated by a small
//! gap. Ticks are written by `scale::format_tick` through the epaint
//! measurement, **never** by Typst; the title and the break labels are
//! entered rich text (`formula::rich_style`). A break is marked by a small
//! triangle (a drawn polygon, not a glyph, so no font can lack it).
//!
//! # Where it sits
//!
//! Below / Right of the frame the bar owns a **band** outside the frame
//! ([`band`], read by `CellPlan`): the figure grows, the frame never changes.
//! Those two places force the orientation (Below is horizontal, Right is
//! vertical); the orientation setting only applies to the four corners and to
//! a dragged bar. The band is a function of the spec alone (fixed
//! thicknesses), so the layout probe and the drawing cannot disagree.

use plotters::coord::Shift;
use plotters::element::{PathElement, Polygon, Rectangle, Text};
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use plotters::style::TextStyle;
use plotters_backend::{BackendCoord, DrawingBackend};
use stokesforge_core::resolve::{ResolvedPanel, ResolvedSeries};
use stokesforge_plot::{
    gradient_at, value_color, ColorBarOrientation, ColorBarPosition, ColorBarScale, ColorBarSpec,
    ColorBarTicks, GRADIENT_PRESETS,
};

use crate::paint;
use crate::scale;

/// Gap between the frame (its axis areas) and the band of the bar.
const GAP: i32 = 6;
/// Thickness of the gradient strip.
const THICK: i32 = 12;
/// Height of the row of break markers and labels above a horizontal bar.
const MARKER_ROW: i32 = 18;
/// Distance between the strip and its tick labels.
const TICK_GAP: i32 = 6;
/// Height of the row of tick labels under a horizontal bar.
const TICK_ROW: i32 = 18;
/// Height of the title row under a horizontal bar (width beside a vertical one).
const TITLE_ROW: i32 = 22;
/// Width of the column of break labels left of a vertical bar.
const LABEL_COL: i32 = 84;
/// Width of the tick label column right of a vertical bar.
const TICK_COL: i32 = 40;
/// Gap between two segments.
const SEG_GAP: i32 = 6;
/// Distance between an inside bar and the frame.
const INSIDE_MARGIN: i32 = 8;
/// Dash and gap of the tick marks.
const DASH: i32 = 3;
const DASH_GAP: i32 = 2;
/// How far a tick mark goes past the strip.
const TICK_OVERSHOOT: i32 = 4;

/// One drawn segment of the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct BarSegment {
    /// Range of the values of its curves (typed ends completed).
    pub natural: (f64, f64),
    /// Range as drawn on the scale (shifted by `chain`).
    pub drawn: (f64, f64),
    /// Gradient stops of the group (the first preset if it retains none).
    pub stops: Vec<String>,
    /// Typst label of the break at its end.
    pub label: String,
}

/// A segment with the members that have a value.
struct Resolved {
    members: Vec<usize>,
    natural: (f64, f64),
    stops: Vec<String>,
    label: String,
}

/// The segments of the bar of `panel` that can be drawn, in bar order.
fn resolve_segments(panel: &ResolvedPanel<'_>, bar: &ColorBarSpec) -> Vec<Resolved> {
    let mut out = Vec::new();
    // First segment of a group wins; a group is never in two segments.
    let mut seen = Vec::new();
    for segment in &bar.segments {
        if seen.contains(&segment.group) {
            continue;
        }
        seen.push(segment.group);
        let members: Vec<usize> = panel
            .series
            .iter()
            .enumerate()
            .filter(|(_, series)| on_bar(series, segment.group))
            .map(|(index, _)| index)
            .collect();
        let values: Vec<f64> = members
            .iter()
            .filter_map(|&index| panel.series[index].value)
            .collect();
        let Some(natural) = ColorBarSpec::natural_range(segment, &values) else {
            continue;
        };
        let stops: Vec<String> = match panel.spec.group_gradient(segment.group) {
            Some(stops) if !stops.is_empty() => stops.to_vec(),
            _ => GRADIENT_PRESETS[0]
                .stops
                .iter()
                .map(|stop| (*stop).to_string())
                .collect(),
        };
        out.push(Resolved {
            members,
            natural,
            stops,
            label: segment.label.clone(),
        });
    }
    out
}

/// The segments to draw for the visible bar of `panel` (empty without one).
pub fn bar_segments(panel: &ResolvedPanel<'_>) -> Vec<BarSegment> {
    let Some(bar) = panel.spec.color_bar.as_ref().filter(|bar| bar.visible) else {
        return Vec::new();
    };
    let resolved = resolve_segments(panel, bar);
    let natural: Vec<(f64, f64)> = resolved.iter().map(|segment| segment.natural).collect();
    let drawn = bar.drawn_ranges(&natural);
    resolved
        .into_iter()
        .zip(drawn)
        .map(|(segment, drawn)| BarSegment {
            natural: segment.natural,
            drawn,
            stops: segment.stops,
            label: segment.label,
        })
        .collect()
}

/// The value colour of each series of `panel`, in `panel.series` order.
///
/// A series is on the bar when the layer has a visible bar, the series is
/// visible, not broken, belongs to a group with a segment, and has a value
/// (`ResolvedSeries::value`) that maps onto the segment's range. The range
/// of a segment is its typed ends completed by the smallest and largest value
/// of its members on the bar (hidden and broken members do not count). The
/// gradient is the one the group retains, the first preset if it retained
/// none.
pub fn value_colors(panel: &ResolvedPanel<'_>) -> Vec<Option<String>> {
    let mut colors = vec![None; panel.series.len()];
    let Some(bar) = panel.spec.color_bar.as_ref().filter(|bar| bar.visible) else {
        return colors;
    };
    for segment in resolve_segments(panel, bar) {
        for index in segment.members {
            let Some(value) = panel.series[index].value else {
                continue;
            };
            colors[index] = value_color(bar, &segment.stops, segment.natural, value);
        }
    }
    colors
}

/// How many curves of the groups on the visible bar of `panel` are left
/// without a value (no x', text in the cell, a value `<= 0` on a log scale):
/// the tab reports them (#499, C8). Hidden and broken curves do not count.
pub fn missing_value_count(panel: &ResolvedPanel<'_>) -> usize {
    let Some(bar) = panel.spec.color_bar.as_ref().filter(|bar| bar.visible) else {
        return 0;
    };
    let colors = value_colors(panel);
    panel
        .series
        .iter()
        .enumerate()
        .filter(|(index, series)| {
            bar.segments
                .iter()
                .any(|segment| on_bar(series, segment.group))
                && colors[*index].is_none()
        })
        .count()
}

/// True if `series` counts for the segment of `group`.
fn on_bar(series: &ResolvedSeries<'_>, group: u32) -> bool {
    series.spec.visible && !series.broken() && series.spec.group == Some(group)
}

/// Where the bar of a spec stands, once the options are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Outside the frame, under it.
    Below,
    /// Outside the frame, on its right.
    Right,
    /// Inside the frame, in a corner.
    Corner { left: bool, top: bool },
    /// Dragged: centred on a point.
    Free,
}

fn place_of(bar: &ColorBarSpec) -> Place {
    if bar.anchor.is_some_and(|anchor| anchor.is_finite()) {
        return Place::Free;
    }
    match bar.position.effective() {
        ColorBarPosition::Below => Place::Below,
        ColorBarPosition::Right => Place::Right,
        corner => Place::Corner {
            left: matches!(
                corner,
                ColorBarPosition::TopLeft | ColorBarPosition::BottomLeft
            ),
            top: matches!(
                corner,
                ColorBarPosition::TopLeft | ColorBarPosition::TopRight
            ),
        },
    }
}

/// True if the bar is laid out along the horizontal.
fn horizontal(bar: &ColorBarSpec, place: Place) -> bool {
    match place {
        Place::Below => true,
        Place::Right => false,
        _ => bar.orientation.effective() == ColorBarOrientation::Horizontal,
    }
}

/// Whether the bar carries break labels, and a title.
fn extras(bar: &ColorBarSpec) -> (bool, bool) {
    let labels = bar
        .segments
        .split_last()
        .is_some_and(|(_, rest)| rest.iter().any(|segment| !segment.label.is_empty()));
    (labels, !bar.title.is_empty())
}

/// Size across the bar (its height when horizontal, its width when
/// vertical): markers, strip, tick labels, title.
fn cross_size(bar: &ColorBarSpec, horizontal: bool) -> i32 {
    let (labels, title) = extras(bar);
    let title = if title { TITLE_ROW } else { 0 };
    if horizontal {
        i32::from(labels) * MARKER_ROW + THICK + TICK_GAP + TICK_ROW + title
    } else {
        i32::from(labels) * LABEL_COL + THICK + TICK_GAP + TICK_COL + title
    }
}

/// The band the bar asks outside the frame, in points: `(bottom, right)`.
///
/// Only a visible bar with segments, put Below or Right and not dragged, asks
/// for one. A function of the spec alone: the layout probe, the grid and the
/// free placement read it before anything is drawn.
pub fn band(bar: Option<&ColorBarSpec>) -> (u32, u32) {
    let Some(bar) = bar.filter(|bar| bar.visible && !bar.segments.is_empty()) else {
        return (0, 0);
    };
    match place_of(bar) {
        Place::Below => ((GAP + cross_size(bar, true)) as u32, 0),
        Place::Right => (0, (GAP + cross_size(bar, false)) as u32),
        _ => (0, 0),
    }
}

/// Text styles of the bar.
pub struct BarStyles {
    /// Break labels and title (`formula::rich_style`).
    pub rich: TextStyle<'static>,
    /// Tick labels (epaint, never Typst).
    pub tick: TextStyle<'static>,
    /// Ink of the marks.
    pub ink: (u8, u8, u8),
}

/// Where the bar goes.
pub struct BarFrame {
    /// Plot area: top-left corner and size.
    pub frame: ((i32, i32), (u32, u32)),
    /// `(x, y)` where the band on the right / under the frame starts (past
    /// the axis areas).
    pub outside: (i32, i32),
    /// The band was reserved in the layout (stored frames). Without it
    /// (legacy layout) an outside place falls back to the bottom right corner.
    pub reserved: bool,
    /// Centre of a dragged bar, in backend pixels.
    pub center: Option<BackendCoord>,
}

/// What was drawn: the box of the bar (strip, ticks, labels, title).
pub type DrawnBar = ((i32, i32), (u32, u32));

/// Draws the colour bar of a layer; `None` if there is nothing to draw.
pub fn draw<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    bar: &ColorBarSpec,
    segments: &[BarSegment],
    placing: &BarFrame,
    styles: &BarStyles,
) -> Result<Option<DrawnBar>, DrawingAreaErrorKind<DB::ErrorType>> {
    if !bar.visible || segments.is_empty() {
        return Ok(None);
    }
    let mut place = place_of(bar);
    if matches!(place, Place::Below | Place::Right) && !placing.reserved {
        place = Place::Corner {
            left: false,
            top: false,
        };
    }
    let along_x = horizontal(bar, place);
    let ((fx, fy), (fw, fh)) = placing.frame;
    let (fw, fh) = (fw as i32, fh as i32);
    let cross = cross_size(bar, along_x);
    let (labels, title) = extras(bar);

    // Length of the strip.
    let length = match place {
        Place::Below => fw,
        Place::Right => fh,
        _ if along_x => (fw * 2 / 5).clamp(120, 260),
        _ => (fh / 2).clamp(100, 260),
    }
    .max(8 * segments.len() as i32 + 12);
    let (bw, bh) = if along_x {
        (length, cross)
    } else {
        (cross, length)
    };
    let (bx, by) = match place {
        Place::Below => (fx, placing.outside.1 + GAP),
        Place::Right => (placing.outside.0 + GAP, fy),
        Place::Free => {
            let (cx, cy) = placing.center.unwrap_or((fx + fw / 2, fy + fh / 2));
            (cx - bw / 2, cy - bh / 2)
        }
        Place::Corner { left, top } => (
            if left {
                fx + INSIDE_MARGIN
            } else {
                (fx + fw - bw - INSIDE_MARGIN).max(fx)
            },
            if top {
                fy + INSIDE_MARGIN
            } else {
                (fy + fh - bh - INSIDE_MARGIN).max(fy)
            },
        ),
    };

    // `u` runs along the strip from its start (left, or bottom when
    // vertical), `v` across it from the box edge (top, or left).
    let at = |u: i32, v: i32| -> BackendCoord {
        if along_x {
            (bx + u, by + v)
        } else {
            (bx + v, by + length - u)
        }
    };
    let rect = |u0: i32, u1: i32, v0: i32, v1: i32| -> [BackendCoord; 2] {
        let (a, b) = (at(u0, v0), at(u1, v1));
        [(a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1))]
    };
    let strip_v = if along_x {
        i32::from(labels) * MARKER_ROW
    } else {
        i32::from(labels) * LABEL_COL
    };

    // Segment extents along the strip.
    let spans: Vec<f64> = segments
        .iter()
        .map(|segment| segment_span(&bar.scale.effective(), segment.drawn))
        .collect();
    let gaps = SEG_GAP * (segments.len() as i32 - 1);
    let usable = f64::from((length - gaps).max(segments.len() as i32));
    let total: f64 = spans.iter().sum();
    let mut starts = Vec::with_capacity(segments.len());
    let mut lens = Vec::with_capacity(segments.len());
    let mut cursor = 0.0f64;
    for span in &spans {
        let share = if total > 0.0 {
            span / total
        } else {
            1.0 / segments.len() as f64
        };
        let len = (usable * share).round().max(1.0);
        starts.push(cursor.round() as i32);
        lens.push(len as i32);
        cursor += len + f64::from(SEG_GAP);
    }

    // The strip: 1-pixel stripes.
    for (segment, (&start, &len)) in segments.iter().zip(starts.iter().zip(&lens)) {
        for pixel in 0..len {
            let t = (f64::from(pixel) + 0.5) / f64::from(len);
            let Some(color) = gradient_at(&segment.stops, t) else {
                continue;
            };
            let rgb = crate::color::rgb_or(&color, paint::SERIES_FALLBACK);
            area.draw(&Rectangle::new(
                rect(start + pixel, start + pixel + 1, strip_v, strip_v + THICK),
                ShapeStyle {
                    color: paint::rgb(rgb).into(),
                    filled: true,
                    stroke_width: 1,
                },
            ))?;
        }
    }

    // Ticks.
    let ink: ShapeStyle = paint::rgb(styles.ink).into();
    let ticks = tick_marks(bar, segments, &starts, &lens);
    for (u, text) in &ticks {
        let end = strip_v + THICK + TICK_OVERSHOOT;
        let mut v = strip_v;
        while v < end {
            let to = (v + DASH).min(end);
            area.draw(&PathElement::new(vec![at(*u, v), at(*u, to)], ink))?;
            v += DASH + DASH_GAP;
        }
        let label_v = strip_v + THICK + TICK_GAP;
        if along_x {
            let (x, y) = at(*u, label_v);
            area.draw(&Text::new(
                text.as_str(),
                (x, y),
                styles.tick.pos(Pos::new(HPos::Center, VPos::Top)),
            ))?;
        } else {
            let (x, y) = at(*u, label_v);
            area.draw(&Text::new(
                text.as_str(),
                (x, y),
                styles.tick.pos(Pos::new(HPos::Left, VPos::Center)),
            ))?;
        }
    }

    // Break markers and labels: at the end of every segment but the last.
    let rich = |pos: Pos| styles.rich.pos(pos);
    for (index, segment) in segments.iter().enumerate().take(segments.len() - 1) {
        let u = (starts[index] + lens[index] - 1).max(0);
        if along_x {
            let (x, y) = at(u, strip_v);
            area.draw(&Polygon::new(
                vec![(x - 4, y - 12), (x + 4, y - 12), (x, y - 2)],
                ink.filled(),
            ))?;
            if !segment.label.is_empty() {
                area.draw(&Text::new(
                    segment.label.as_str(),
                    (x + 7, y - 9),
                    rich(Pos::new(HPos::Left, VPos::Center)),
                ))?;
            }
        } else {
            let (x, y) = at(u, strip_v);
            area.draw(&Polygon::new(
                vec![(x - 12, y - 4), (x - 12, y + 4), (x - 2, y)],
                ink.filled(),
            ))?;
            if !segment.label.is_empty() {
                area.draw(&Text::new(
                    segment.label.as_str(),
                    (x - 15, y),
                    rich(Pos::new(HPos::Right, VPos::Center)),
                ))?;
            }
        }
    }

    // Title, centred along the strip.
    if title {
        if along_x {
            let (x, y) = (bx + length / 2, by + cross - TITLE_ROW / 2);
            area.draw(&Text::new(
                bar.title.as_str(),
                (x, y),
                rich(Pos::new(HPos::Center, VPos::Center)),
            ))?;
        } else {
            let (x, y) = (bx + cross - TITLE_ROW / 2, by + length / 2);
            crate::pipeline::draw_centered_text(area, &bar.title, &styles.rich, -90.0, (x, y))?;
        }
    }

    Ok(Some(((bx, by), (bw.max(1) as u32, bh.max(1) as u32))))
}

/// Length weight of a segment on the strip.
fn segment_span(scale: &ColorBarScale, (from, to): (f64, f64)) -> f64 {
    let span = match scale {
        ColorBarScale::Log if from > 0.0 && to > 0.0 => to.ln() - from.ln(),
        _ => to - from,
    };
    if span.is_finite() && span > 0.0 {
        span
    } else {
        0.0
    }
}

/// Position `t` (0 = start, 1 = end) of `value` in a segment, `None` when the
/// value is outside it or cannot be placed on the scale.
fn position(scale: &ColorBarScale, (from, to): (f64, f64), value: f64) -> Option<f64> {
    let t = match scale {
        ColorBarScale::Log => {
            if value <= 0.0 || from <= 0.0 || to <= 0.0 {
                return None;
            }
            let span = to.ln() - from.ln();
            if span == 0.0 {
                0.0
            } else {
                (value.ln() - from.ln()) / span
            }
        }
        _ => {
            let span = to - from;
            if span == 0.0 {
                0.0
            } else {
                (value - from) / span
            }
        }
    };
    (t.is_finite() && (-1e-9..=1.0 + 1e-9).contains(&t)).then_some(t.clamp(0.0, 1.0))
}

/// The tick marks `(position along the strip in pixels, label)`.
fn tick_marks(
    bar: &ColorBarSpec,
    segments: &[BarSegment],
    starts: &[i32],
    lens: &[i32],
) -> Vec<(i32, String)> {
    let scale = bar.scale.effective();
    let mut values: Vec<(usize, f64)> = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let (from, to) = segment.drawn;
        match bar.ticks.effective() {
            ColorBarTicks::Auto => {
                let mut points = match scale {
                    ColorBarScale::Log => decades(from, to),
                    _ => scale::linear_key_points((from, to), 4),
                };
                points.extend([from, to]);
                for value in points {
                    values.push((index, value));
                }
            }
            _ => {
                // The ends and the breaks: with a chain the start of a
                // segment is the break before it.
                if index == 0 || !bar.chain {
                    values.push((index, from));
                }
                values.push((index, to));
            }
        }
    }
    let mut out: Vec<(i32, String)> = Vec::new();
    for (index, value) in values {
        let Some(t) = position(&scale, segments[index].drawn, value) else {
            continue;
        };
        let len = lens[index];
        let pixel = starts[index] + (t * f64::from(len - 1)).round() as i32;
        if out.iter().any(|(other, _)| (other - pixel).abs() < 3) {
            continue;
        }
        out.push((
            pixel,
            scale::format_tick(stokesforge_plot::AxisScale::Linear, value),
        ));
    }
    out
}

/// Powers of ten inside `[from, to]`.
fn decades(from: f64, to: f64) -> Vec<f64> {
    if !(from > 0.0 && to > from) {
        return Vec::new();
    }
    let (low, high) = (from.log10().ceil() as i32, to.log10().floor() as i32);
    (low..=high).map(|k| 10f64.powi(k)).collect()
}

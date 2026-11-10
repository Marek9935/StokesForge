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
//! Each segment is a **rounded bar** (small corner radius: a cap and two
//! filled circles per end). Its body is painted in **stripes** of a few pixels
//! (Plotters has no gradient element, and a stripe is the same rectangle on
//! screen and in the export), each reaching under the next one and at least 3
//! points wide: egui's tessellator draws a rectangle of two points or less as a
//! faint line, and a figure shown at a fraction of its size would show light
//! seams between 1-pixel stripes. A segment is as long as the span of its drawn
//! range (the `chain` option shifts the ranges, never the colours); segments are
//! separated by an empty gap.
//!
//! A break is centred on its gap: a small triangle (a drawn polygon, not a
//! glyph) touching the top of the bar, a tick and its value label (centred on
//! the gap with a chain), and the break label under the bar just after the value
//! label, smaller. Ticks go from the bar's edge outwards, never over the
//! colours. Ticks are written by `scale::format_tick` through the epaint
//! measurement, **never** by Typst; the title and the break labels are
//! entered rich text (`formula::rich_style`). Vertical: the same, rotated
//! (marker on the left, labels on the right).
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
/// Height of the row of break markers above a horizontal bar (width of the
/// column of markers left of a vertical one): the marker touches the strip.
const MARKER_ROW: i32 = 10;
/// Height (width, vertical) of a break marker triangle.
const MARKER_SIZE: i32 = 8;
/// Width of a gradient stripe, and how far it reaches under the next one.
const STEP: i32 = 2;
const BRIDGE: i32 = 2;
/// Corner radius of a segment.
const RADIUS: f64 = 4.0;
/// Size of a break label against the other labels: it reads like a
/// subscript of the break value.
pub const BREAK_LABEL_SCALE: f64 = 0.8;
/// Distance between the strip and its tick labels.
const TICK_GAP: i32 = 8;
/// Height of the row of tick labels under a horizontal bar.
const TICK_ROW: i32 = 18;
/// Height of the title row under a horizontal bar (width beside a vertical one).
const TITLE_ROW: i32 = 22;
/// Gap between the tick labels and the break label of a vertical bar, and
/// between the last label column and the title.
const COL_GAP: i32 = 4;
/// Sample the tick column is at least as wide as: tick values come from the
/// data, unknown when the room is reserved (only typed ends are in the spec).
const TICK_SAMPLE: &str = "00.0";
/// Gap between two segments.
const SEG_GAP: i32 = 6;
/// Distance between an inside bar and the frame.
const INSIDE_MARGIN: i32 = 8;
/// Dash and gap of the tick marks.
const DASH: i32 = 2;
const DASH_GAP: i32 = 1;
/// Length of a tick mark, from the strip's edge outwards.
const TICK_LEN: i32 = 6;

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
        // On a log scale a value <= 0 has no colour: it takes no part in the
        // range, the others keep theirs (#499, C8/C9).
        let log = bar.scale.effective() == ColorBarScale::Log;
        let values: Vec<f64> = members
            .iter()
            .filter_map(|&index| panel.series[index].value)
            .filter(|value| !log || *value > 0.0)
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

/// Widths of the label columns right of a vertical bar `(ticks, breaks)`,
/// measured through the one text measure of the backend (invariant 4): the
/// widest tick label (typed ends, at least [`TICK_SAMPLE`]) and the widest
/// break label. Both are zero-free of any fixed room: a short label leaves no
/// gap.
fn label_columns<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    bar: &ColorBarSpec,
) -> Result<(i32, i32), DrawingAreaErrorKind<DB::ErrorType>> {
    let (small, tick) = label_styles(bar, (0, 0, 0, 255));
    let mut ticks = area.estimate_text_size(TICK_SAMPLE, &tick)?.0 as i32;
    for segment in &bar.segments {
        for value in [segment.from, segment.to].into_iter().flatten() {
            let text = scale::format_tick(stokesforge_plot::AxisScale::Linear, value);
            ticks = ticks.max(area.estimate_text_size(&text, &tick)?.0 as i32);
        }
    }
    let mut breaks = 0;
    if let Some((_, rest)) = bar.segments.split_last() {
        for segment in rest.iter().filter(|segment| !segment.label.is_empty()) {
            breaks = breaks.max(area.estimate_text_size(&segment.label, &small)?.0 as i32);
        }
    }
    Ok((ticks, breaks))
}

/// The text styles of the tick labels and the break labels `(small, tick)`.
pub fn label_styles(
    bar: &ColorBarSpec,
    ink: (u8, u8, u8, u8),
) -> (TextStyle<'static>, TextStyle<'static>) {
    let small = crate::formula::rich_style_layout(
        bar.font.effective_size(crate::pipeline::LABEL_SIZE) * BREAK_LABEL_SCALE,
        ink,
        bar.font.family(),
        None,
    );
    let tick = if bar.font.is_default() {
        crate::pipeline::text_style(crate::pipeline::LABEL_SIZE, ink)
    } else {
        crate::formula::literal_style_font(
            bar.font.effective_size(crate::pipeline::LABEL_SIZE),
            ink,
            bar.font.family(),
        )
    };
    (small, tick)
}

/// Size across the bar (its height when horizontal, its width when
/// vertical): markers, strip, tick labels, title. A vertical bar is as wide
/// as its labels, no wider.
fn cross_size<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    bar: &ColorBarSpec,
    horizontal: bool,
) -> Result<i32, DrawingAreaErrorKind<DB::ErrorType>> {
    let (labels, title) = extras(bar);
    let title = if title { grown(bar, TITLE_ROW) } else { 0 };
    Ok(if horizontal {
        i32::from(labels) * MARKER_ROW + THICK + TICK_GAP + grown(bar, TICK_ROW) + title
    } else {
        let (ticks, breaks) = label_columns(area, bar)?;
        i32::from(labels) * MARKER_ROW
            + THICK
            + TICK_GAP
            + ticks
            + if labels { COL_GAP + breaks } else { 0 }
            + if title > 0 { COL_GAP } else { 0 }
            + title
    })
}

/// A text row or column, grown with the bar's font size above the default
/// label size (never shrunk: a smaller font keeps today's room).
fn grown(bar: &ColorBarSpec, size: i32) -> i32 {
    let ratio = bar.font.effective_size(crate::pipeline::LABEL_SIZE) / crate::pipeline::LABEL_SIZE;
    if ratio > 1.0 {
        (f64::from(size) * ratio).round() as i32
    } else {
        size
    }
}

/// The band the bar asks outside the frame, in points: `(bottom, right)`.
///
/// Only a visible bar with segments, put Below or Right and not dragged, asks
/// for one. A function of the spec and the text measure: the layout probe, the grid and the
/// free placement read it before anything is drawn.
pub fn band<DB: DrawingBackend>(
    bar: Option<&ColorBarSpec>,
    probe: &DrawingArea<DB, Shift>,
) -> Result<(u32, u32), DrawingAreaErrorKind<DB::ErrorType>> {
    let Some(bar) = bar.filter(|bar| bar.visible && !bar.segments.is_empty()) else {
        return Ok((0, 0));
    };
    Ok(match place_of(bar) {
        Place::Below => ((GAP + cross_size(probe, bar, true)?) as u32, 0),
        Place::Right => (0, (GAP + cross_size(probe, bar, false)?) as u32),
        _ => (0, 0),
    })
}

/// Text styles of the bar.
pub struct BarStyles {
    /// Title (`formula::rich_style`).
    pub rich: TextStyle<'static>,
    /// Break labels: the same, smaller.
    pub small: TextStyle<'static>,
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
    let cross = cross_size(area, bar, along_x)?;
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
    let strip_v = i32::from(labels) * MARKER_ROW;

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

    // The strip: one rounded bar per segment. The rounded ends are a cap and
    // two filled circles per end (the corners), the body is painted in
    // stripes of STEP pixels, each reaching under the next one and never
    // narrower than the thin-rectangle limit of the screen's tessellator (a
    // rectangle under two pixels wide becomes a faint line there): a figure
    // shown at a fraction of its size would otherwise let the background shine
    // between stripes, as a grid of light seams (#499).
    let radius = RADIUS as i32;
    let fill = |color: &str| -> ShapeStyle {
        ShapeStyle {
            color: paint::rgb(crate::color::rgb_or(color, paint::SERIES_FALLBACK)).into(),
            filled: true,
            stroke_width: 1,
        }
    };
    for (segment, (&start, &len)) in segments.iter().zip(starts.iter().zip(&lens)) {
        let color_at =
            |pixel: f64| gradient_at(&segment.stops, (pixel / f64::from(len)).clamp(0.0, 1.0));
        let rounded = len >= 4 * radius;
        let (from, to) = if rounded {
            (radius, len - radius)
        } else {
            (0, len)
        };
        if rounded {
            for (end, edge) in [(0, 0), (len - radius, len)] {
                let Some(color) = color_at(f64::from(end) + f64::from(radius) / 2.0) else {
                    continue;
                };
                let (cap_from, cap_to) = if edge == 0 {
                    (0, radius + BRIDGE)
                } else {
                    (len - radius - BRIDGE, len)
                };
                area.draw(&Rectangle::new(
                    rect(
                        start + cap_from,
                        start + cap_to,
                        strip_v + radius,
                        strip_v + THICK - radius,
                    ),
                    fill(&color),
                ))?;
                for v in [strip_v + radius, strip_v + THICK - radius] {
                    let (x, y) = at(start + end + if edge == 0 { radius } else { 0 }, v);
                    area.draw(&Circle::new((x, y), radius, fill(&color)))?;
                }
            }
        }
        let mut pixel = from;
        while pixel < to {
            let width = if to - pixel - STEP < 3 {
                to - pixel
            } else {
                STEP
            };
            if let Some(color) = color_at(f64::from(pixel) + f64::from(width) / 2.0) {
                let reach = (pixel + width + BRIDGE).min(to);
                area.draw(&Rectangle::new(
                    rect(start + pixel, start + reach, strip_v, strip_v + THICK),
                    fill(&color),
                ))?;
            }
            pixel += width;
        }
    }

    // Ticks: from the strip's edge outwards, never over the colours.
    let ink: ShapeStyle = paint::rgb(styles.ink).into();
    let ticks = tick_marks(bar, segments, &starts, &lens);
    let label_v = strip_v + THICK + TICK_GAP;
    let mut widths = Vec::with_capacity(ticks.len());
    for (u, text) in &ticks {
        let end = strip_v + THICK + TICK_LEN;
        let mut v = strip_v + THICK;
        while v < end {
            let to = (v + DASH).min(end);
            area.draw(&PathElement::new(vec![at(*u, v), at(*u, to)], ink))?;
            v += DASH + DASH_GAP;
        }
        let (x, y) = at(*u, label_v);
        let pos = if along_x {
            Pos::new(HPos::Center, VPos::Top)
        } else {
            Pos::new(HPos::Left, VPos::Center)
        };
        area.draw(&Text::new(text.as_str(), (x, y), styles.tick.pos(pos)))?;
        let (w, _) = area.estimate_text_size(text, &styles.tick)?;
        widths.push(w as i32);
    }

    // Break markers and labels: centred on the gap after every segment but
    // the last. The marker sits on the strip's edge, the label under the bar
    // just after the break's value label, smaller (read like its subscript).
    for index in 0..segments.len() - 1 {
        let u = starts[index] + lens[index] + SEG_GAP / 2;
        let (x, y) = at(u, strip_v);
        // Room taken by the value labels of this break (past the gap centre).
        let reach = ticks
            .iter()
            .zip(&widths)
            .filter(|((other, _), _)| (other - u).abs() <= SEG_GAP)
            .map(|((other, _), w)| if along_x { other - u + w / 2 } else { *w })
            .max()
            .unwrap_or(0);
        if along_x {
            area.draw(&Polygon::new(
                vec![
                    (x - MARKER_SIZE / 2, y - MARKER_SIZE),
                    (x + MARKER_SIZE / 2, y - MARKER_SIZE),
                    (x, y),
                ],
                ink.filled(),
            ))?;
            if !segments[index].label.is_empty() {
                area.draw(&Text::new(
                    segments[index].label.as_str(),
                    (x + reach + 4, by + label_v + 2),
                    styles.small.pos(Pos::new(HPos::Left, VPos::Top)),
                ))?;
            }
        } else {
            area.draw(&Polygon::new(
                vec![
                    (x - MARKER_SIZE, y - MARKER_SIZE / 2),
                    (x - MARKER_SIZE, y + MARKER_SIZE / 2),
                    (x, y),
                ],
                ink.filled(),
            ))?;
            if !segments[index].label.is_empty() {
                area.draw(&Text::new(
                    segments[index].label.as_str(),
                    (bx + label_v + reach + COL_GAP, y + 2),
                    styles.small.pos(Pos::new(HPos::Left, VPos::Center)),
                ))?;
            }
        }
    }

    // Title, centred along the strip.
    let rich = |pos: Pos| styles.rich.pos(pos);
    if title {
        if along_x {
            let (x, y) = (bx + length / 2, by + cross - grown(bar, TITLE_ROW) / 2);
            area.draw(&Text::new(
                bar.title.as_str(),
                (x, y),
                rich(Pos::new(HPos::Center, VPos::Center)),
            ))?;
        } else {
            let (x, y) = (bx + cross - grown(bar, TITLE_ROW) / 2, by + length / 2);
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
///
/// A position is an edge coordinate: 0 is the strip's start, the strip's
/// length its end. With a chain the value that ends a segment is the break,
/// put at the centre of the gap (the next segment's start is the same value
/// and has no tick of its own); otherwise every segment has its own ends.
fn tick_marks(
    bar: &ColorBarSpec,
    segments: &[BarSegment],
    starts: &[i32],
    lens: &[i32],
) -> Vec<(i32, String)> {
    let scale = bar.scale.effective();
    let last = segments.len() - 1;
    let mut values: Vec<(usize, f64)> = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let (from, to) = segment.drawn;
        let mut points = match bar.ticks.effective() {
            ColorBarTicks::Auto => {
                let mut points = match scale {
                    ColorBarScale::Log => decades(from, to),
                    _ => scale::linear_key_points((from, to), 4),
                };
                points.extend([from, to]);
                points
            }
            // The ends and the breaks.
            _ => vec![from, to],
        };
        if bar.chain && index > 0 {
            points.retain(|value| *value != from);
        }
        for value in points {
            values.push((index, value));
        }
    }
    let mut out: Vec<(i32, String)> = Vec::new();
    for (index, value) in values {
        let Some(t) = position(&scale, segments[index].drawn, value) else {
            continue;
        };
        let pixel = if bar.chain && index < last && value == segments[index].drawn.1 {
            starts[index] + lens[index] + SEG_GAP / 2
        } else {
            starts[index] + (t * f64::from(lens[index])).round() as i32
        };
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

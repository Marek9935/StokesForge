//! From resolved data to a drawable geometry — task R5.1.
//!
//! # A separate step, and why
//!
//! R4 read the points at drawing time and, separately, when framing the axes:
//! two passes, two chances of not saying the same thing. With bars and
//! histograms, this is no longer tenable — a histogram's geometry cannot be
//! deduced from its data without binning, and a framing that ignored it
//! would cut off the last bin.
//!
//! Hence this step: each series is **prepared once**, into geometry, and
//! framing and drawing read the same thing. This is also what makes the
//! coverage of [`PlotType`] checkable: the plot type is interpreted only
//! here.
//!
//! # Transformed space
//!
//! Everything this module produces is already in the space where the plot
//! area works — log₁₀ included (see [`crate::scale`]). The rest of the
//! pipeline no longer needs to know the axis scale, except to label.

use std::num::NonZeroU32;

use stokesforge_core::resolve::{bar_width, histogram, ResolvedSeries, SeriesLabel};
use stokesforge_plot::{AxisScale, PlotType, SeriesAxis, SeriesSpec};

use crate::scale::forward;
use crate::texts::FigureTexts;

/// A point in plot space.
pub type Point = (f64, f64);

/// An error-bar whisker: two ends, already transformed.
pub type Whisker = (Point, Point);

/// What a series became, once translated into geometry.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// Nothing to draw: plot type unknown to this reader, empty column, or
    /// all values outside the chosen scale.
    Nothing,
    /// A sequence of points, to connect and/or mark depending on the plot type.
    Curve {
        /// The points, in column order.
        points: Vec<Point>,
        /// The error-bar whiskers, empty if the series has none.
        whiskers: Vec<Whisker>,
    },
    /// Vertical bars.
    Bars {
        /// `(left edge, right edge, top)` of each bar.
        bars: Vec<(f64, f64, f64)>,
        /// The error-bar whiskers (#360), empty if the series has none.
        whiskers: Vec<Whisker>,
    },
}

impl Geometry {
    /// True if nothing will be drawn.
    pub fn is_empty(&self) -> bool {
        match self {
            Geometry::Nothing => true,
            Geometry::Curve { points, .. } => points.is_empty(),
            Geometry::Bars { bars, .. } => bars.is_empty(),
        }
    }

    /// Number of points carried — what the LTTB can reduce.
    pub fn point_count(&self) -> usize {
        match self {
            Geometry::Nothing => 0,
            Geometry::Curve { points, .. } => points.len(),
            Geometry::Bars { bars, .. } => bars.len(),
        }
    }

    /// True if the geometry rests on a baseline (the bars), which must then
    /// enter the framing or the bars would float.
    pub fn needs_baseline(&self) -> bool {
        matches!(self, Geometry::Bars { .. })
    }
}

/// A series ready to draw: its geometry, and what is needed to draw it and
/// list it in the legend.
#[derive(Debug)]
pub struct Prepared<'s> {
    /// The original spec — the style is read from it at paint time.
    pub spec: &'s SeriesSpec,
    /// Displayed name, as the resolution established it.
    pub label: SeriesLabel,
    /// Y axis it is attached to.
    pub axis: SeriesAxis,
    /// The geometry, in plot space.
    pub geometry: Geometry,
    /// True if the series' reference is broken: nothing to draw, but the
    /// series must **show** (`architecture-choisie.md` §11.1).
    pub broken: bool,
    /// True if the plot type comes from a newer version of StokesForge
    /// (C5, level 2): nothing is drawn, and nothing is lost either.
    pub unknown_type: bool,
    /// Number of points removed by decimation, filled in afterwards by the
    /// pipeline.
    pub decimated: usize,
    /// A column of the series comes from a stale computation and the graph
    /// warns about it (#198): grayed line and « ! » marker.
    pub stale: bool,
}

impl Prepared<'_> {
    /// True if something will actually be painted in the plot area.
    pub fn draws(&self) -> bool {
        !self.broken && !self.geometry.is_empty()
    }

    /// True if the series has its place in the legend.
    ///
    /// A broken reference qualifies — it is even the only place where it can
    /// show, since it has nothing to draw.
    pub fn in_legend(&self) -> bool {
        self.spec.show_in_legend && (self.draws() || self.broken)
    }

    /// The text of the legend entry, in the language of `texts`.
    ///
    /// A broken reference says so. An unnamed one shows the placeholder; one
    /// that has a name keeps it and is annotated — otherwise a named series
    /// would silently vanish from the figure while keeping a normal-looking
    /// legend entry (invariant 10, in every language).
    pub fn legend_text(&self, texts: &FigureTexts) -> String {
        let name = match &self.label {
            SeriesLabel::Missing => return texts.broken_reference.clone(),
            SeriesLabel::Text(name) => name.clone(),
            SeriesLabel::Row(row) => texts.row_series(row.saturating_add(1)),
        };
        if self.broken {
            texts.broken_named(&name)
        } else {
            name
        }
    }

    /// True if the legend text was **typed** (`SeriesSpec::name`): intended
    /// Typst markup. False if taken from the column, hence shown as is
    /// (issue #33, `formula::literal_style`). Also false for a broken
    /// reference: the annotated text mixes the name with a sentence of the
    /// interface, which would be markup too, so it is all drawn literally.
    pub fn legend_is_typed(&self) -> bool {
        !self.spec.name.is_empty() && !self.broken
    }
}

/// Prepares a resolved series, on the axis scales that concern it.
///
/// `y_scale` is that of **its** attached axis: a right-hand series is
/// prepared on the right-hand scale.
pub fn prepare<'s>(
    resolved: &ResolvedSeries<'s>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Prepared<'s> {
    let spec = resolved.spec;
    let mut prepared = Prepared {
        spec,
        label: resolved.label.clone(),
        axis: spec.axis,
        geometry: Geometry::Nothing,
        broken: resolved.broken(),
        unknown_type: matches!(spec.plot_type, PlotType::Unknown(_)),
        decimated: 0,
        stale: resolved.stale,
    };
    if prepared.broken || prepared.unknown_type {
        return prepared;
    }

    prepared.geometry = match spec.plot_type {
        PlotType::Histogram => histogram_geometry(resolved, x_scale, y_scale),
        PlotType::Bar => bars_geometry(resolved, x_scale, y_scale),
        // The four others draw a sequence of points; what tells them apart —
        // line, markers, whiskers — is read at paint time, not here, because
        // the geometry is the same.
        _ => curve_geometry(resolved, x_scale, y_scale),
    };
    prepared
}

/// Points and whiskers, in a single pass.
///
/// A single pass and not two: the error columns are indexed **per point**,
/// and filtering the non-representable points on one side without filtering
/// the errors on the other would shift the whiskers by one from the first
/// `NaN` — a defect that raises nothing and is barely visible.
fn curve_geometry(
    resolved: &ResolvedSeries<'_>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Geometry {
    let Some(ys) = resolved.y.as_ref().map(|y| y.as_slice()) else {
        return Geometry::Nothing;
    };
    let xs = resolved.x.as_ref().map(|x| x.as_slice());
    // Whiskers on every plot type that draws points (#360): what makes the
    // bars appear is the error column, not the type.
    let x_errors = resolved.x_error.as_ref().map(|e| e.as_slice());
    let y_errors = resolved.y_error.as_ref().map(|e| e.as_slice());

    let mut points = Vec::with_capacity(ys.len());
    let mut whiskers = Vec::new();

    for (index, &raw_y) in ys.iter().enumerate() {
        let raw_x = match xs {
            Some(xs) => match xs.get(index) {
                Some(x) => *x,
                None => break,
            },
            // Implicit abscissa: the row index, as the resolution already
            // does when no X column is designated.
            None => index as f64,
        };
        let (Some(x), Some(y)) = (forward(x_scale, raw_x), forward(y_scale, raw_y)) else {
            continue;
        };
        points.push((x, y));

        if let Some(half) = x_errors.and_then(|e| e.get(index)).copied() {
            if let Some(whisker) = whisker_along_x(raw_x, half, y, x_scale) {
                whiskers.push(whisker);
            }
        }
        if let Some(half) = y_errors.and_then(|e| e.get(index)).copied() {
            if let Some(whisker) = whisker_along_y(raw_y, half, x, y_scale) {
                whiskers.push(whisker);
            }
        }
    }

    Geometry::Curve { points, whiskers }
}

/// Horizontal whisker around `center`, at an already transformed ordinate.
///
/// Returns `None` when the half-width is not a half-width (non-finite,
/// negative) or when neither end has an image on the scale. A single
/// representable end gives a half-whisker rather than nothing: on a
/// logarithmic axis, this is the common case near the origin.
fn whisker_along_x(center: f64, half: f64, y: f64, scale: AxisScale) -> Option<Whisker> {
    if !half.is_finite() || half <= 0.0 {
        return None;
    }
    let here = forward(scale, center)?;
    let low = forward(scale, center - half).unwrap_or(here);
    let high = forward(scale, center + half).unwrap_or(here);
    (low != high).then_some(((low, y), (high, y)))
}

/// Vertical whisker around `center`, at an already transformed abscissa.
fn whisker_along_y(center: f64, half: f64, x: f64, scale: AxisScale) -> Option<Whisker> {
    if !half.is_finite() || half <= 0.0 {
        return None;
    }
    let here = forward(scale, center)?;
    let low = forward(scale, center - half).unwrap_or(here);
    let high = forward(scale, center + half).unwrap_or(here);
    (low != high).then_some(((x, low), (x, high)))
}

/// Bars centered on the abscissas, with a deduced or imposed width.
///
/// The width comes from [`stokesforge_core::resolve::bar_width`] — from the
/// core, not from here: the deduction must not give two different widths
/// depending on the engine that paints, and the same reason gave birth to
/// `PlotSpec`.
fn bars_geometry(
    resolved: &ResolvedSeries<'_>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Geometry {
    let Some(ys) = resolved.y.as_ref().map(|y| y.as_slice()) else {
        return Geometry::Nothing;
    };
    let xs = resolved.x.as_ref().map(|x| x.as_slice());
    let implicit: Vec<f64>;
    let xs = match xs {
        Some(xs) => xs,
        None => {
            implicit = (0..ys.len()).map(|i| i as f64).collect();
            &implicit
        }
    };
    let width = bar_width(Some(xs), resolved.spec.bar_width);
    let half = width / 2.0;

    let x_errors = resolved.x_error.as_ref().map(|e| e.as_slice());
    let y_errors = resolved.y_error.as_ref().map(|e| e.as_slice());

    let mut bars = Vec::with_capacity(ys.len());
    let mut whiskers = Vec::new();
    // Indexed per point, like `curve_geometry`: the error columns follow the
    // rows, not the bars that survived the scale.
    for (index, (&raw_x, &raw_y)) in xs.iter().zip(ys.iter()).enumerate() {
        let (Some(left), Some(right), Some(top)) = (
            forward(x_scale, raw_x - half),
            forward(x_scale, raw_x + half),
            forward(y_scale, raw_y),
        ) else {
            continue;
        };
        bars.push((left.min(right), left.max(right), top));
        let (Some(x), Some(y)) = (forward(x_scale, raw_x), forward(y_scale, raw_y)) else {
            continue;
        };
        if let Some(half) = x_errors.and_then(|e| e.get(index)).copied() {
            if let Some(whisker) = whisker_along_x(raw_x, half, y, x_scale) {
                whiskers.push(whisker);
            }
        }
        if let Some(half) = y_errors.and_then(|e| e.get(index)).copied() {
            if let Some(whisker) = whisker_along_y(raw_y, half, x, y_scale) {
                whiskers.push(whisker);
            }
        }
    }
    Geometry::Bars { bars, whiskers }
}

/// Bins and counts, computed by the core.
///
/// [`stokesforge_core::resolve::histogram`] is the port of the Python
/// function and lives there for exactly this reason: two renderers must not
/// bin differently. A histogram consumes **only** its Y column —
/// `PlotType::needs_x` already says so on the spec side, and the resolution
/// gives it no abscissa.
fn histogram_geometry(
    resolved: &ResolvedSeries<'_>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Geometry {
    let Some(values) = resolved.y.as_ref().map(|y| y.as_slice()) else {
        return Geometry::Nothing;
    };
    let bins = NonZeroU32::new(resolved.spec.histogram_bins.get()).unwrap_or(NonZeroU32::MIN);
    let (edges, counts) = histogram(values, bins);
    if counts.is_empty() || edges.len() < 2 {
        return Geometry::Nothing;
    }

    let mut bars = Vec::with_capacity(counts.len());
    for (index, &count) in counts.iter().enumerate() {
        let (Some(&raw_left), Some(&raw_right)) = (edges.get(index), edges.get(index + 1)) else {
            break;
        };
        let (Some(left), Some(right), Some(top)) = (
            forward(x_scale, raw_left),
            forward(x_scale, raw_right),
            forward(y_scale, count),
        ) else {
            continue;
        };
        bars.push((left.min(right), left.max(right), top));
    }
    Geometry::Bars {
        bars,
        whiskers: Vec::new(),
    }
}

/// Extent of a geometry on the abscissa axis.
pub fn x_extent(geometry: &Geometry) -> Option<(f64, f64)> {
    match geometry {
        Geometry::Nothing => None,
        Geometry::Curve { points, .. } => extent(points.iter().map(|(x, _)| *x)),
        Geometry::Bars { bars, .. } => extent(bars.iter().flat_map(|(l, r, _)| [*l, *r])),
    }
}

/// Extent of a geometry on the ordinate axis, whiskers included.
///
/// The whiskers enter the framing: an error bar that leaves the plot area
/// describes an uncertainty that can no longer be seen.
pub fn y_extent(geometry: &Geometry) -> Option<(f64, f64)> {
    match geometry {
        Geometry::Nothing => None,
        Geometry::Curve { points, whiskers } => extent(
            points
                .iter()
                .map(|(_, y)| *y)
                .chain(whiskers.iter().flat_map(|(a, b)| [a.1, b.1])),
        ),
        Geometry::Bars { bars, whiskers } => extent(
            bars.iter()
                .map(|(_, _, top)| *top)
                .chain(whiskers.iter().flat_map(|(a, b)| [a.1, b.1])),
        ),
    }
}

fn extent(values: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values.filter(|v| v.is_finite()) {
        min = min.min(value);
        max = max.max(value);
    }
    (min.is_finite() && max.is_finite()).then_some((min, max))
}

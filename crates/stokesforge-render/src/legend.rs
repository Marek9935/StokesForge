//! The legend — task R5.3.
//!
//! # Why it is written here and not delegated to Plotters
//!
//! `ChartContext::configure_series_labels()` exists and almost does the job.
//! Almost: to size its frame, it calls `MultiLineText::estimate_dimension`,
//! which goes through `FontData::estimate_layout` — **not** through the
//! backend's `estimate_text_size`. Since our Plotters fonts are cut down
//! (`default-features = false`, `architecture-choisie.md` §4.4), this
//! estimate is the monospace heuristic by UTF-8 bytes: « λ » counts as two,
//! « Échantillon » as twelve characters and thirteen bytes.
//!
//! The frame would thus be measured wrong while the text inside it was drawn
//! right — an offset that raises no error, exactly the silent defect this
//! work package is built to prevent. And it would be wrong **in the same way
//! on both sides**, so the agreement test would not catch it: this is the
//! case that justifies not relying on that test alone.
//!
//! The legend is therefore laid out here, on
//! [`plotters::drawing::DrawingArea::estimate_text_size`], which is the
//! backend's path — hence [`crate::text::estimate`], hence the project's
//! single measurement.
//!
//! # What an entry shows
//!
//! Its thumbnail is the series': the line with its style, the marker with
//! its shape, or a solid fill for a bar series. A series with a **broken
//! reference** appears too, in gray and annotated — this is the only place
//! in the figure where it can show, since it has nothing to draw
//! (`architecture-choisie.md` §11.1: « à afficher comme telle et jamais
//! comme une série vide »).

use plotters::coord::Shift;
use plotters::element::{Rectangle, Text};
use plotters::prelude::*;
use plotters::style::TextStyle;
use plotters_backend::{BackendCoord, DrawingBackend};
use stokesforge_plot::{LegendPosition, LegendSpec, PlotType};

use crate::paint::{self, BROKEN, SERIES_FALLBACK};
use crate::series::{Geometry, Prepared};
use crate::texts::FigureTexts;

/// Margin between the legend frame and the edge of the plot area.
const MARGIN: i32 = 8;
/// Inner margin of the frame.
const PADDING: i32 = 6;
/// Width of the thumbnail (the line stub, or the solid fill).
pub const KEY_WIDTH: i32 = 26;
/// Gap between the thumbnail and the text.
const KEY_GAP: i32 = 6;
/// Vertical gap between two entries.
const ROW_GAP: i32 = 4;
/// Number of entries beyond which the legend is truncated.
///
/// A graph can carry hundreds of series (the load case of the document has
/// 500); a 500-line legend fits in no plot area and would cost 500 text
/// measurements per frame. Beyond the cap, the first entries are listed and
/// the last line says how many are missing.
const MAX_ENTRIES: usize = 24;

/// What the legend actually drew.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawnLegend {
    /// Number of entries drawn, not counting the overflow line.
    pub entries: usize,
    /// Number of entries left out for lack of room.
    pub omitted: usize,
    /// The drawn frame, in backend pixels: top-left corner and size
    /// (issue #53: pick the legend under the pointer).
    pub frame: Option<((i32, i32), (u32, u32))>,
    /// For each drawn entry, in order: vertical middle and height of its
    /// row, in backend pixels.
    pub rows: Vec<(f64, f64)>,
}

/// The legend's two text styles: that of a typed name (Typst markup) and
/// that of a text taken from a column or generated here, shown as is
/// (issue #33). Same size, same color.
#[derive(Clone)]
pub struct LegendText<'a> {
    /// Typed series name (`formula::rich_style`).
    pub typed: TextStyle<'a>,
    /// Name taken from the column, « … and N more » line
    /// (`formula::literal_style`).
    pub literal: TextStyle<'a>,
}

/// Legend colors: the background of its frame (the graph's) and the ink of
/// its outline and thumbnails (the background's automatic ink,
/// `crate::color::auto_ink`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegendColors {
    /// Frame background, opacity included (transparent if the graph has no
    /// background).
    pub background: (u8, u8, u8, u8),
    /// Outline of the frame and of the bar thumbnails.
    pub ink: (u8, u8, u8),
}

/// Draws a panel's legend in its plot area.
///
/// `area` must be the plot area in **screen** coordinates
/// (`plotting_area().use_screen_coord()`), and `origin`/`size` its position
/// and size: the legend is placed relative to the plot area, not to the
/// panel, so it does not overlap the ticks.
///
/// `center` (issue #53): the center where to put the frame, in backend
/// pixels, when the legend was moved; it takes precedence over the anchor
/// corner.
#[allow(clippy::too_many_arguments)]
pub fn draw<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    origin: BackendCoord,
    size: (u32, u32),
    spec: &LegendSpec,
    entries: &[&Prepared<'_>],
    styles: &LegendText<'_>,
    figure_texts: &FigureTexts,
    colors: LegendColors,
    center: Option<BackendCoord>,
) -> Result<DrawnLegend, DrawingAreaErrorKind<DB::ErrorType>> {
    let LegendColors { background, ink } = colors;
    if !spec.visible || entries.is_empty() {
        return Ok(DrawnLegend::default());
    }

    let omitted = entries.len().saturating_sub(MAX_ENTRIES);
    let shown = &entries[..entries.len().min(MAX_ENTRIES)];

    // The texts are measured once, and the same measurement decides the
    // frame and was used to place the axes.
    let mut texts: Vec<(String, &TextStyle<'_>)> = shown
        .iter()
        .map(|entry| {
            let style = if entry.legend_is_typed() {
                &styles.typed
            } else {
                &styles.literal
            };
            (entry.legend_text(figure_texts), style)
        })
        .collect();
    if omitted > 0 {
        texts.push((figure_texts.more_entries(omitted), &styles.literal));
    }

    let mut label_width = 0i32;
    let mut row_height = 1i32;
    for (text, style) in &texts {
        let (w, h) = area.estimate_text_size(text, style)?;
        label_width = label_width.max(w as i32);
        row_height = row_height.max(h as i32);
    }

    let rows = texts.len() as i32;
    let box_width = PADDING * 2 + KEY_WIDTH + KEY_GAP + label_width;
    let box_height = PADDING * 2 + rows * row_height + (rows - 1).max(0) * ROW_GAP;

    let (area_w, area_h) = (size.0 as i32, size.1 as i32);
    let left = match spec.position {
        LegendPosition::TopLeft | LegendPosition::BottomLeft => MARGIN,
        LegendPosition::TopRight | LegendPosition::BottomRight => area_w - box_width - MARGIN,
    };
    let top = match spec.position {
        LegendPosition::TopLeft | LegendPosition::TopRight => MARGIN,
        LegendPosition::BottomLeft | LegendPosition::BottomRight => area_h - box_height - MARGIN,
    };
    // A legend wider than its plot area stays anchored at the top left
    // rather than leaving through the opposite side: Plotters' clipping
    // handles the rest.
    let (x0, y0) = match center {
        Some((cx, cy)) => (cx - box_width / 2, cy - box_height / 2),
        None => (origin.0 + left.max(0), origin.1 + top.max(0)),
    };

    let frame = [(x0, y0), (x0 + box_width, y0 + box_height)];
    area.draw(&Rectangle::new(
        frame,
        ShapeStyle {
            color: paint::rgba(background),
            filled: true,
            stroke_width: 1,
        },
    ))?;
    area.draw(&Rectangle::new(
        frame,
        ShapeStyle {
            color: paint::rgb(ink).into(),
            filled: false,
            stroke_width: 1,
        },
    ))?;

    for (index, (text, style)) in texts.iter().enumerate() {
        let row_top = y0 + PADDING + index as i32 * (row_height + ROW_GAP);
        let middle = row_top + row_height / 2;
        let key_left = x0 + PADDING;
        if let Some(entry) = shown.get(index) {
            key(area, entry, (key_left, middle), ink)?;
        }
        area.draw(&Text::new(
            text.as_str(),
            (key_left + KEY_WIDTH + KEY_GAP, row_top),
            *style,
        ))?;
    }

    let row_boxes = (0..shown.len())
        .map(|index| {
            let row_top = y0 + PADDING + index as i32 * (row_height + ROW_GAP);
            (row_top as f64 + row_height as f64 / 2.0, row_height as f64)
        })
        .collect();
    Ok(DrawnLegend {
        entries: shown.len(),
        omitted,
        frame: Some(((x0, y0), (box_width as u32, box_height as u32))),
        rows: row_boxes,
    })
}

/// The thumbnail of an entry: what the series looks like.
fn key<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    entry: &Prepared<'_>,
    (left, middle): BackendCoord,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    series_key(
        area,
        entry.spec,
        entry.broken,
        matches!(entry.geometry, Geometry::Bars(_)),
        (left, middle),
        ink,
    )
}

/// Style thumbnail without preparing the values (#82): opening the panel
/// must neither walk a column nor compute its histogram on every frame.
pub fn series_key<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    spec: &stokesforge_plot::SeriesSpec,
    broken: bool,
    bars: bool,
    (left, middle): BackendCoord,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if bars {
        let color = if broken {
            BROKEN
        } else {
            crate::color::rgb_or(&spec.line.color, SERIES_FALLBACK)
        };
        let swatch = [(left, middle - 5), (left + KEY_WIDTH, middle + 5)];
        area.draw(&Rectangle::new(
            swatch,
            ShapeStyle {
                color: paint::rgb(color).into(),
                filled: true,
                stroke_width: 1,
            },
        ))?;
        return area.draw(&Rectangle::new(
            swatch,
            ShapeStyle {
                color: paint::rgb(ink).into(),
                filled: false,
                stroke_width: 1,
            },
        ));
    }

    let draws_line = spec.plot_type.draws_line() || broken;
    if draws_line {
        paint::polyline(
            area,
            &[(left, middle), (left + KEY_WIDTH, middle)],
            &spec.line,
            broken,
        )?;
    }
    if spec.plot_type.draws_symbols() && !matches!(spec.plot_type, PlotType::Unknown(_)) {
        paint::symbol_at(area, (left + KEY_WIDTH / 2, middle), &spec.symbol, broken)?;
    }
    Ok(())
}

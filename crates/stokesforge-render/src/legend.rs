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
/// Default width of the thumbnail (the line stub, or the solid fill): the
/// object-manager thumbnail keeps it whatever the legend's setting.
pub const KEY_WIDTH: i32 = 26;
/// Gap between the thumbnail and the text.
const KEY_GAP: i32 = 6;
/// Number of entries beyond which the legend is truncated.
///
/// A graph can carry hundreds of series (the load case of the document has
/// 500); a 500-line legend fits in no plot area and would cost 500 text
/// measurements per frame. Beyond the cap, the first entries are listed and
/// the last line says how many are missing.
const MAX_ENTRIES: usize = 24;

/// Where one entry was drawn, in backend pixels: the cell it picks on
/// (#507: a second column picks its own series).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnCell {
    /// Horizontal middle of the cell.
    pub center_x: f64,
    /// Width of the cell; a lone column spans the whole frame.
    pub width: f64,
    /// Vertical middle of the row.
    pub center_y: f64,
    /// Height of the row.
    pub height: f64,
}

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
    /// The cell of each drawn entry, in order.
    pub rows: Vec<DrawnCell>,
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
    /// Outline of the bar thumbnails (and of the frame when it has no color
    /// of its own).
    pub ink: (u8, u8, u8),
    /// Outline of the frame (#507): the spec's color, else the ink.
    pub frame: (u8, u8, u8),
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
    let LegendColors {
        background,
        ink,
        frame: frame_ink,
    } = colors;
    if !spec.visible || entries.is_empty() {
        return Ok(DrawnLegend::default());
    }

    let omitted = entries.len().saturating_sub(MAX_ENTRIES);
    let shown = &entries[..entries.len().min(MAX_ENTRIES)];

    // The texts are measured once, and the same measurement decides the
    // frame and was used to place the axes.
    let texts: Vec<(String, &TextStyle<'_>)> = shown
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
    let more = (omitted > 0).then(|| figure_texts.more_entries(omitted));

    let mut widths = Vec::with_capacity(texts.len());
    let mut row_height = 1i32;
    for (text, style) in &texts {
        let (w, h) = area.estimate_text_size(text, style)?;
        widths.push(w as i32);
        row_height = row_height.max(h as i32);
    }
    let mut more_width = 0i32;
    if let Some(text) = &more {
        let (w, h) = area.estimate_text_size(text, &styles.literal)?;
        more_width = w as i32;
        row_height = row_height.max(h as i32);
    }

    // Column-major grid (#507, D2): entries fill down first, then the next
    // column. The number of columns never exceeds the number of entries.
    let columns = spec.column_count().min(texts.len().max(1));
    let per_column = texts.len().div_ceil(columns).max(1);
    let row_gap = spec.row_gap_pt().round() as i32;
    let column_gap = spec.column_gap_pt().round() as i32;
    let key_width = spec.swatch_length_pt().round() as i32;
    let used_columns = texts.len().div_ceil(per_column).max(1);
    let column_text: Vec<i32> = (0..used_columns)
        .map(|c| {
            widths
                .iter()
                .skip(c * per_column)
                .take(per_column)
                .copied()
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut column_left = Vec::with_capacity(used_columns);
    let mut grid_width = 0i32;
    for (c, text) in column_text.iter().enumerate() {
        if c > 0 {
            grid_width += column_gap;
        }
        column_left.push(grid_width);
        grid_width += key_width + KEY_GAP + text;
    }
    let more_row = i32::from(more.is_some());
    let grid_rows = per_column as i32 + more_row;
    let box_width = PADDING * 2
        + grid_width.max(if more.is_some() {
            key_width + KEY_GAP + more_width
        } else {
            0
        });
    let box_height = PADDING * 2 + grid_rows * row_height + (grid_rows - 1).max(0) * row_gap;

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
    if spec.frame {
        let outline = ShapeStyle {
            color: paint::rgb(frame_ink).into(),
            filled: false,
            stroke_width: paint::stroke_width(spec.frame_width_pt()),
        };
        if outline.stroke_width == 1 {
            area.draw(&Rectangle::new(frame, outline))?;
        } else {
            // The SVG backend of Plotters writes no width for a rectangle
            // outline: a thick frame is a closed path, started in the middle
            // of the top edge so that no corner is left open.
            let (x1, y1) = (x0 + box_width, y0 + box_height);
            let middle = (x0 + box_width / 2, y0);
            area.draw(&PathElement::new(
                vec![middle, (x1, y0), (x1, y1), (x0, y1), (x0, y0), middle],
                outline,
            ))?;
        }
    }

    let row_top_of = |row: usize| y0 + PADDING + row as i32 * (row_height + row_gap);
    for (index, (text, style)) in texts.iter().enumerate() {
        let (column, row) = (index / per_column, index % per_column);
        let row_top = row_top_of(row);
        let middle = row_top + row_height / 2;
        let key_left = x0 + PADDING + column_left[column];
        if let Some(entry) = shown.get(index) {
            key(area, entry, (key_left, middle), key_width, ink)?;
        }
        area.draw(&Text::new(
            text.as_str(),
            (key_left + key_width + KEY_GAP, row_top),
            *style,
        ))?;
    }
    if let Some(text) = &more {
        // « … and N more » spans the whole width, under the grid.
        area.draw(&Text::new(
            text.as_str(),
            (x0 + PADDING + key_width + KEY_GAP, row_top_of(per_column)),
            &styles.literal,
        ))?;
    }

    // The pick cell of a column runs from the middle of the gap before it to
    // the middle of the gap after it; the first and last reach the frame, so
    // a lone column spans the whole frame like it always did.
    let cell_bounds = |column: usize| -> (f64, f64) {
        let start = if column == 0 {
            x0 as f64
        } else {
            (x0 + PADDING + column_left[column]) as f64 - column_gap as f64 / 2.0
        };
        let end = if column + 1 == used_columns {
            (x0 + box_width) as f64
        } else {
            (x0 + PADDING + column_left[column + 1]) as f64 - column_gap as f64 / 2.0
        };
        (start, end)
    };
    let row_boxes = (0..shown.len())
        .map(|index| {
            let (column, row) = (index / per_column, index % per_column);
            let (start, end) = cell_bounds(column);
            DrawnCell {
                center_x: (start + end) / 2.0,
                width: end - start,
                center_y: row_top_of(row) as f64 + row_height as f64 / 2.0,
                height: row_height as f64,
            }
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
    key_width: i32,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    series_key(
        area,
        entry.spec,
        entry.broken,
        matches!(entry.geometry, Geometry::Bars { .. }),
        (left, middle),
        key_width,
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
    key_width: i32,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if bars {
        let color = if broken {
            BROKEN
        } else {
            crate::color::rgb_or(&spec.line.color, SERIES_FALLBACK)
        };
        let swatch = [(left, middle - 5), (left + key_width, middle + 5)];
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
            &[(left, middle), (left + key_width, middle)],
            &spec.line,
            broken,
        )?;
    }
    if spec.plot_type.draws_symbols() && !matches!(spec.plot_type, PlotType::Unknown(_)) {
        paint::symbol_at(area, (left + key_width / 2, middle), &spec.symbol, broken)?;
    }
    Ok(())
}

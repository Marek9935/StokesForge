//! The Plotters pipeline, **written once for both backends**.
//!
//! # Why this module is not in each of the two backend crates
//!
//! The end criterion of the work package is "axes in the same place, checked
//! by a test" (`roadmap-rendu.md`). A test can notice a disagreement; it
//! cannot prevent one from arising. If the egui backend and the SVG backend
//! each called their own `ChartBuilder`, the slightest divergence in margin,
//! font size or call order would become a disagreement to find after the
//! fact. Here there is **only one** sequence of Plotters calls, generic over
//! `DB: DrawingBackend`, and the two backends now differ only in the way
//! they put pixels down.
//!
//! This is the transposition of the rule phase R5 gives itself — "no feature
//! enters a single backend" — to the level where it is free: structure,
//! rather than discipline. **R5 did not have to write a single line of
//! rendering in a backend**: everything the phase adds — plot types, scales,
//! legend, panels, decimation — is here.
//!
//! # Scope, phase R5
//!
//! The real `PlotSpec`: the six [`stokesforge_plot::PlotType`]s, the two
//! [`stokesforge_plot::AxisScale`]s, the secondary axis, the line and marker
//! styles, the legend, the panel grid, and LTTB decimation. What is still
//! missing is flagged by a `// R6:` or `// R7:` at the exact place where it
//! will be inserted, not by a list at the top of the file that would go
//! stale.
//!
//! # How a panel is drawn, in order
//!
//! The order is not free, and each step depends on what the previous one
//! learned:
//!
//! 1. **prepare** each series into geometry ([`crate::series`]), which
//!    applies the scale of its axis and bins the histograms. The framing and
//!    the drawing will read the same geometry, so they cannot read it
//!    differently;
//! 2. **frame** the axes on this geometry;
//! 3. **build** the plot area — only there does the width in pixels exist;
//! 4. **decimate** ([`crate::decimate`]), which needs this width;
//! 5. **paint**, in pixels ([`crate::paint`]);
//! 6. **the legend**, on top, and measured by the same path as the axes
//!    ([`crate::legend`]);
//! 7. **the free texts** (issue #35), on the very top: centered on their data
//!    point, projected like a series; a point outside the framing is not
//!    drawn, a text can overflow onto the ticks.
//!
//! # Where text measurement enters the geometry
//!
//! This is not anecdotal, and it is what makes the agreement test (R4.5,
//! extended in R5.6) more than a formality: `DrawingArea::titled` — called as
//! soon as the graph or a panel has a title — asks `estimate_text_size` for
//! the height of the title, then **moves the top of the plot area down** by
//! that height. A measurement wrong by a few pixels therefore shifts the
//! whole area, axes included, without any error being raised. This is
//! exactly the silent defect described in `architecture-choisie.md` §4.4,
//! and the agreement test catches it there. The legend goes through the same
//! path, for the same reason ([`crate::legend`]).
//!
//! # Two text styles, depending on the role
//!
//! The ticks take `text_style` (`epaint` layout); graph title, panel titles,
//! axis titles and legend take [`rich_style`], which the backends compile
//! through Typst (issue #32, see [`crate::formula`]). It is here, and only
//! here, that the role of a text is decided.

use plotters::coord::Shift;
use plotters::prelude::*;
use stokesforge_core::resolve::{ResolvedPanel, ResolvedPlot};
use stokesforge_plot::{
    AxisScale, AxisSide as PlotAxisSide, AxisSpec, PlotSpec, SeriesAxis, TextAnchor, TextTarget,
    TickDirection,
};

use crate::clip::{self, PixelBounds};
use crate::color::{auto_ink, over_white, parse_hex, rgba_or};
use crate::decimate;
use crate::formula::literal_style_font;
use crate::legend;
use crate::paint::{self, opaque, rgba, PAPER};
use crate::scale;
use crate::series::{self, Geometry, Prepared};
use crate::texts::FigureTexts;

/// Size (in Plotters' sense) of the graph title.
pub const TITLE_SIZE: f64 = 24.0;
/// Size (in Plotters' sense) of a panel title.
///
/// Smaller than the graph's: in a grid, the graph title must remain the one
/// read first.
pub const PANEL_TITLE_SIZE: f64 = 16.0;
/// Size (in Plotters' sense) of the tick numbers.
pub const LABEL_SIZE: f64 = 14.0;
/// Size (in Plotters' sense) of the axis titles.
pub const AXIS_DESC_SIZE: f64 = 16.0;
/// Outer margin of the graph, in backend units.
pub const MARGIN: u32 = 12;
/// Height reserved for the ticks and the title of the bottom axis.
pub const X_LABEL_AREA: u32 = 52;
/// Width reserved for the ticks and the title of a vertical axis.
pub const Y_LABEL_AREA: u32 = 64;
/// Tick area of a mirror axis without labels (#176): the tick marks alone.
pub const BARE_LABEL_AREA: u32 = 12;
/// Font family requested from Plotters. It is translated into
/// [`epaint::FontFamily::Proportional`] by [`crate::text::font_id`]; what
/// Plotters does with it by itself does not matter, its fonts being
/// disabled.
pub const FONT_FAMILY: &str = "sans-serif";

/// Number of cells beyond which the declared grid is abandoned in favor of a
/// compact grid.
///
/// `PlotSpec::rows()` and `columns()` are `max(position) + 1`: a panel
/// position coming from a file can be anything (nothing bounds it on
/// reading, and the spec documents it with `saturating_add`), so a declared
/// grid can be billions of cells for two panels. Splitting the area into so
/// many is impossible; the fallback is described in [`grid_of`].
pub const MAX_CELLS: usize = 256;

/// Dimensions below which a cell is no longer a plot area.
///
/// A panel smaller than this has room for neither its ticks nor its frame;
/// it is counted as omitted rather than drawn as a mess.
pub const MIN_CELL: (u32, u32) = (48, 36);

/// Default size of a StokesForge figure, in pixels — `(width, height)`.
///
/// Deliberate choice, not yet a setting. 4/3, as aimed — but provisional
/// (values to revisit). Any future configurability (per graph, per export)
/// goes through this single point rather than through a constant copied into
/// each backend — `stokesforge-render-svg` (`DEFAULT_SIZE`, export) uses it
/// as the fixed page size; `stokesforge-app` (screen view, F11) only uses it
/// for its 4/3 ratio, the figure being scaled "contain" into the window's
/// available space (`graph_view_contents`).
pub const DEFAULT_FIGURE_SIZE: (u32, u32) = (480, 360);

/// What a panel ended up drawing — enough to write a test without rereading
/// pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPanel {
    /// Index of the layer in [`PlotSpec::panels`]: the identity of a layer
    /// (several layers can be superimposed in the same cell).
    pub index: usize,
    /// The layer is superimposed on another (#167): its X axis is on top if
    /// `x_top`, its Y axis on the right if `y_right`.
    pub overlay: Option<stokesforge_plot::OverlaySpec>,
    /// Declared position of the panel in the grid.
    pub row: usize,
    /// Same, as a column.
    pub column: usize,
    /// Bounds kept on X, **in data space**: on a logarithmic scale, these are
    /// indeed values and not their logarithms.
    pub x_range: (f64, f64),
    /// Same for the left ordinate.
    pub y_range: (f64, f64),
    /// Bounds of the right ordinate, if a series is attached to it.
    pub y2_range: Option<(f64, f64)>,
    /// Top-left corner of the plot area, in backend pixels.
    ///
    /// Added in R7 (task 7.2/7.3): this is what, combined with
    /// [`Self::pixel_size`] and the bounds above, gives the pixel ↔ data
    /// correspondence that `interact::PanelTransform` builds — without
    /// rediscovering the grid computation or the tick areas, since it is the
    /// pixels **actually** allocated by Plotters
    /// (`plotting.get_base_pixel()`) that are copied here, not an
    /// approximation rebuilt on the side.
    pub pixel_origin: (i32, i32),
    /// Dimensions of the plot area, in backend pixels.
    pub pixel_size: (u32, u32),
    /// Scale of the abscissa axis, as it was applied.
    pub x_scale: AxisScale,
    /// Scale of the left ordinate.
    pub y_scale: AxisScale,
    /// Scale of the right ordinate (no effect if [`Self::y2_range`]
    /// is `None`).
    pub y2_scale: AxisScale,
    /// Number of series actually drawn.
    pub drawn_series: usize,
    /// Number of series set aside because their reference is broken. Never an
    /// error: `architecture-choisie.md` §11.1 makes it a first-class state,
    /// to display and not to hide — they appear in the legend, in grey and
    /// annotated.
    pub broken_series: usize,
    /// Number of series whose plot type comes from a more recent version of
    /// StokesForge (C5, level 2): nothing is drawn, nothing is lost.
    pub unknown_series: usize,
    /// Number of entries written in the legend.
    pub legend_entries: usize,
    /// Number of points removed by LTTB decimation.
    pub decimated_points: usize,
    /// Tick areas actually allocated around the plot area — enough to find,
    /// without rereading pixels, which axis (or which axis title) is under a
    /// click ([`crate::hit`]).
    pub axis_areas: AxisAreas,
    /// The free texts actually drawn (issue #35), in spec order; a text
    /// outside the visible framing is not listed.
    pub annotations: Vec<DrawnAnnotation>,
    /// The graphic objects actually drawn (issue #76), in spec order (from
    /// bottom to top). Unlike a free text, an object that overflows the
    /// visible framing is listed: it is drawn clipped, but can be picked by
    /// its visible part.
    pub shapes: Vec<crate::shapes::DrawnShape>,
    /// Visible geometry of the series, after decimation and clipping (#83).
    pub series_geometry: Vec<crate::series_selection::DrawnSeries>,
    /// The titles and the legend of this panel (issue #53): panel title, axis
    /// titles, legend frame. A text in its default place is listed too, with
    /// an **approximate** position (Plotters places it itself); this is
    /// enough to pick it and to start from there when it is moved.
    pub texts: Vec<DrawnText>,
    /// The rows of the legend, one per drawn entry.
    pub legend_rows: Vec<DrawnLegendRow>,
}

/// A title or a legend as it was drawn (issue #53).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnText {
    /// The designated text.
    pub target: TextTarget,
    /// Center, in backend pixels.
    pub center: (f64, f64),
    /// Size of the **unrotated** text, in backend pixels.
    pub size: (u32, u32),
    /// Rotation, in degrees, counterclockwise (vertical axis titles).
    pub rotation: f64,
    /// True if the text is placed on its anchor ([`TextAnchor`]), false if it
    /// is in its default place — then `center` is approximate.
    pub anchored: bool,
}

impl DrawnText {
    /// True if the pixel `(x, y)` falls in the rotated rectangle of the text.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        rotated_contains(self.center, self.size, self.rotation, x, y)
    }
}

/// A row of the legend: the entry it shows, and where.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnLegendRow {
    /// Rank of the series in `PanelSpec::series`.
    pub series: usize,
    /// Vertical middle of the row, in backend pixels.
    pub center_y: f64,
    /// Height of the row.
    pub height: f64,
}

/// A free text as it was drawn — enough to pick it under the pointer
/// ([`crate::hit`]), rotation included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnAnnotation {
    /// Index in `PanelSpec::annotations`.
    pub index: usize,
    /// Center of the text, in backend pixels.
    pub center: (f64, f64),
    /// Size of the **unrotated** text, in backend pixels.
    pub size: (u32, u32),
    /// Rotation, in degrees, counterclockwise.
    pub rotation: f64,
}

impl DrawnAnnotation {
    /// True if the pixel `(x, y)` falls in the rotated rectangle of the text.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        rotated_contains(self.center, self.size, self.rotation, x, y)
    }
}

/// Is the pixel `(x, y)` inside the rectangle `size` centered at `center` and
/// rotated by `rotation` degrees (counterclockwise)?
fn rotated_contains(center: (f64, f64), size: (u32, u32), rotation: f64, x: f64, y: f64) -> bool {
    let (dx, dy) = (x - center.0, y - center.1);
    // The text is rotated counterclockwise on screen (y axis downward); we
    // bring the point back into the frame of the straight text by the
    // inverse rotation.
    let (sin, cos) = rotation.to_radians().sin_cos();
    let (u, v) = (dx * cos - dy * sin, dx * sin + dy * cos);
    u.abs() <= size.0 as f64 / 2.0 && v.abs() <= size.1 as f64 / 2.0
}

/// A tick area: its thickness in pixels (0 if the axis is hidden or absent),
/// and whether an axis title is written in it — Plotters places it against
/// the **outer** edge of the area (at the bottom for the bottom axis, on the
/// left for the left one, on the right for the secondary axis).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AxisArea {
    /// Thickness, in backend pixels, perpendicular to the axis.
    pub size: u32,
    /// True if the axis carries a title (non-empty label or unit).
    pub titled: bool,
    /// Distance, in pixels, between the plot area and the inner edge of the
    /// area: 0 in general, a multiple of `size` for the axis of a
    /// superimposed layer stacked behind the axes of older layers (#168).
    pub offset: u32,
}

/// The tick areas of a panel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AxisAreas {
    /// Below the plot area (abscissa).
    pub bottom: AxisArea,
    /// On the left (main ordinate).
    pub left: AxisArea,
    /// On the right (secondary ordinate), if a series is attached to it, or
    /// the Y axis of a superimposed layer (#167).
    pub right: AxisArea,
    /// Above the plot area: the X axis of a superimposed layer (#167).
    pub top: AxisArea,
}

/// What a graph ended up drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPlot {
    /// The panels actually drawn, in spec order.
    pub panels: Vec<DrawnPanel>,
    /// Number of panels set aside for lack of room ([`MIN_CELL`]).
    pub omitted_panels: usize,
    /// Total number of series drawn, all panels together.
    pub drawn_series: usize,
    /// Total number of broken references.
    pub broken_series: usize,
    /// Total number of series with an unknown plot type.
    pub unknown_series: usize,
    /// Total number of points removed by decimation.
    pub decimated_points: usize,
    /// Height, in backend pixels, of the graph title band at the top of the
    /// figure (0 without a title).
    pub title_height: u32,
    /// The graph title as it was drawn (issue #53); `None` without a
    /// title.
    pub title: Option<DrawnText>,
}

impl DrawnPlot {
    /// The X framing of the first panel — the convenience of the common case,
    /// which has only one.
    pub fn x_range(&self) -> (f64, f64) {
        self.panels.first().map_or((0.0, 1.0), |p| p.x_range)
    }

    /// The Y framing of the first panel.
    pub fn y_range(&self) -> (f64, f64) {
        self.panels.first().map_or((0.0, 1.0), |p| p.y_range)
    }

    /// The panel with the given index.
    pub fn panel(&self, index: usize) -> Option<&DrawnPanel> {
        self.panels.get(index)
    }
}

/// The background of the figure, white if it is not readable.
fn figure_background(spec: &PlotSpec) -> (u8, u8, u8, u8) {
    rgba_or(&spec.background, opaque(PAPER))
}

/// Automatic ink of the figure: that of any text and any scale whose color is
/// left empty, and of the handles of a selected series (#98) — white on a
/// dark background, dark otherwise. A (semi-)transparent background is judged
/// composited on white, as the screen paints it.
pub fn figure_ink(spec: &PlotSpec) -> (u8, u8, u8) {
    auto_ink(over_white(figure_background(spec)))
}

/// Draws the resolved graph on the given backend.
///
/// Consumes the backend — this is Plotters' convention
/// ([`IntoDrawingArea`]) and it is right: a backend serves only one figure.
pub fn draw<DB: DrawingBackend>(
    backend: DB,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, DrawingAreaErrorKind<DB::ErrorType>> {
    draw_with_texts(backend, plot, &FigureTexts::default())
}

/// [`draw`] with the texts of the figure ([`FigureTexts`]) in the language of
/// the caller. [`draw`] uses the English default.
pub fn draw_with_texts<DB: DrawingBackend>(
    backend: DB,
    plot: &ResolvedPlot<'_>,
    texts: &FigureTexts,
) -> Result<DrawnPlot, DrawingAreaErrorKind<DB::ErrorType>> {
    let root = backend.into_drawing_area();
    draw_on_with_texts(&root, plot, texts)
}

/// Draws the resolved graph on an already built drawing area.
///
/// Separate from [`draw`] so that a caller that already owns a
/// [`DrawingArea`] does not have to make another.
pub fn draw_on<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, DrawingAreaErrorKind<DB::ErrorType>> {
    draw_on_with_texts(root, plot, &FigureTexts::default())
}

/// [`draw_on`] with the texts of the figure ([`FigureTexts`]).
pub fn draw_on_with_texts<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    plot: &ResolvedPlot<'_>,
    figure_texts: &FigureTexts,
) -> Result<DrawnPlot, DrawingAreaErrorKind<DB::ErrorType>> {
    let spec = plot.spec;
    let background = figure_background(spec);
    let ink = figure_ink(spec);
    // A background of zero opacity is not painted at all: the exported figure
    // (SVG, PNG, PDF) has no background, to blend into another
    // presentation.
    if background.3 > 0 {
        root.fill(&rgba(background))?;
    }

    // `titled` measures the title and lowers the top of the area by as much:
    // the point where `estimate_text_size` enters the geometry (see the head
    // doc).
    //
    // A moved title (issue #53) keeps its band, **empty**: the layout does not
    // depend on where the title is placed — otherwise the plot area would
    // change size on drop, and the title, anchored to the data, would jump. It
    // is drawn on its anchor once the panels are done, on top.
    let title_color = rgba_or(&spec.title_color, opaque(ink));
    let title_style = font_style(
        &spec.title_font,
        TITLE_SIZE,
        title_color,
        root.dim_in_pixel().0,
    );
    let title_anchor =
        usable_anchor(spec.title_anchor).filter(|_| spec.title_visible && !spec.title.is_empty());
    let area = if !spec.title_visible || spec.title.is_empty() {
        root.clone()
    } else if title_anchor.is_some() {
        reserve_title_band(root, &spec.title, &title_style)?
    } else {
        root.titled(&spec.title, title_style.clone())?
    };
    let title_height = (area.get_base_pixel().1 - root.get_base_pixel().1).max(0) as u32;
    let (root_width, _) = root.dim_in_pixel();
    let mut drawn_title = if !spec.title_visible || spec.title.is_empty() || title_anchor.is_some()
    {
        None
    } else {
        let size = root.estimate_text_size(&spec.title, &title_style)?;
        Some(DrawnText {
            target: TextTarget::Title,
            center: (root_width as f64 / 2.0, title_height as f64 / 2.0),
            size,
            rotation: 0.0,
            anchored: false,
        })
    };

    let mut drawn = DrawnPlot {
        panels: Vec::with_capacity(plot.panels.len()),
        omitted_panels: 0,
        drawn_series: 0,
        broken_series: 0,
        unknown_series: 0,
        decimated_points: 0,
        title_height,
        title: None,
    };
    let mut first_projection: Option<Projection> = None;

    // The whole figure, in screen coordinates: moved texts (titles, legend,
    // free texts) are drawn there without being brought back into the cell or
    // the plot area (`BackendCoordOnly` clips each point to the rectangle of
    // its area, #163).
    let canvas = root.use_screen_coord();
    let layout = grid_of(spec, plot.panels.len());
    let cells = area.split_evenly((layout.rows, layout.columns));

    // The tick areas of each cell: the union of what its layers ask for, so
    // that superimposed layers (#167) share an identical plot area. `bases`
    // keeps, per cell, the last ordinary layer drawn: the one a superimposed
    // layer covers.
    let mut plans: std::collections::HashMap<usize, CellPlan> = std::collections::HashMap::new();
    // The rank of each superimposed layer among those of its cell that show an
    // axis on the same side (#168): 0 against the plot area. It comes from the
    // order of the layers, never from a stored level: a deleted layer
    // therefore leaves no empty band.
    let mut ranks: Vec<(u32, u32)> = Vec::with_capacity(plot.panels.len());
    for (index, panel) in plot.panels.iter().enumerate() {
        if let Some(cell) = layout.cells.get(index) {
            let plan = plans.entry(*cell).or_default();
            ranks.push((plan.top_levels, plan.right_levels));
            plan.add(panel);
        } else {
            ranks.push((0, 0));
        }
    }
    let mut bases = std::collections::HashMap::new();
    // The legend of an ordinary layer also lists the curves of the
    // superimposed layers that cover it (#172): `overlays_of` gives, per
    // ordinary layer index, the superimposed layers that follow it in its
    // cell.
    let mut overlays_of: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    let mut last_base: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (index, panel) in plot.panels.iter().enumerate() {
        let Some(cell) = layout.cells.get(index).copied() else {
            continue;
        };
        if panel.spec.overlay.is_none() {
            last_base.insert(cell, index);
        } else if let Some(base) = last_base.get(&cell) {
            overlays_of.entry(*base).or_default().push(index);
        }
    }

    for (index, panel) in plot.panels.iter().enumerate() {
        let Some(cell_index) = layout.cells.get(index).copied() else {
            drawn.omitted_panels += 1;
            continue;
        };
        let Some(cell) = cells.get(cell_index) else {
            drawn.omitted_panels += 1;
            continue;
        };
        let (w, h) = cell.dim_in_pixel();
        if w < MIN_CELL.0 || h < MIN_CELL.1 {
            drawn.omitted_panels += 1;
            continue;
        }
        let plan = plans.get(&cell_index).copied().unwrap_or_default();
        let overlay_panels: Vec<&ResolvedPanel<'_>> = overlays_of
            .get(&index)
            .into_iter()
            .flatten()
            .filter_map(|&i| plot.panels.get(i))
            .collect();
        let (panel_drawn, projection, frame) = draw_panel(
            cell,
            &canvas,
            panel,
            index,
            background,
            plan,
            ranks.get(index).copied().unwrap_or_default(),
            bases.get(&cell_index),
            &overlay_panels,
            plot.spec.decimal_comma,
            figure_texts,
        )?;
        if panel.spec.overlay.is_none() {
            bases.insert(cell_index, frame);
        }
        if first_projection.is_none() {
            first_projection = Some(projection);
        }
        drawn.drawn_series += panel_drawn.drawn_series;
        drawn.broken_series += panel_drawn.broken_series;
        drawn.unknown_series += panel_drawn.unknown_series;
        drawn.decimated_points += panel_drawn.decimated_points;
        drawn.panels.push(panel_drawn);
    }

    if let Some(anchor) = title_anchor {
        // The graph title belongs to no panel: its anchor is read in the
        // frame of the first drawn panel. With no possible projection (omitted
        // panel, logarithmic axis ≤ 0), it falls back to the top center of
        // the figure rather than disappearing.
        let pixel = first_projection
            .as_ref()
            .and_then(|projection| projection.pixel(anchor))
            .unwrap_or(((root_width / 2) as i32, (MARGIN + 12) as i32));
        let size = draw_centered_text(root, &spec.title, &title_style, 0.0, pixel)?;
        drawn_title = Some(DrawnText {
            target: TextTarget::Title,
            center: (pixel.0 as f64, pixel.1 as f64),
            size,
            rotation: 0.0,
            anchored: true,
        });
    }
    drawn.title = drawn_title;

    root.present()?;
    Ok(drawn)
}

/// The area under the band a title would occupy, **without** writing the
/// title: the same height as Plotters' `DrawingArea::titled` (text height
/// plus two margins of at most 5 px), measured by the same path.
fn reserve_title_band<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    text: &str,
    style: &TextStyle<'_>,
) -> Result<DrawingArea<DB, Shift>, DrawingAreaErrorKind<DB::ErrorType>> {
    let (_, text_height) = area.estimate_text_size(text, style)?;
    let band = text_height + (text_height / 2).min(5) * 2;
    let (width, height) = area.dim_in_pixel();
    Ok(area
        .clone()
        .shrink((0, band as i32), (width, height.saturating_sub(band))))
}

/// The anchor, if usable ([`TextAnchor::is_finite`]): a damaged anchor leaves
/// the text in its default place.
fn usable_anchor(anchor: Option<TextAnchor>) -> Option<TextAnchor> {
    anchor.filter(TextAnchor::is_finite)
}

/// The plot area of a panel and its framings: enough to place a
/// [`TextAnchor`] in pixels — the same correspondence as
/// `crate::interact::PanelTransform`, which serves the drag.
struct Projection {
    origin: (i32, i32),
    size: (u32, u32),
    ranges: Ranges,
    right_active: bool,
}

impl Projection {
    /// The pixel of the anchor — **without** requiring that it fall within
    /// the framing: a title lives outside the plot area. `None` if the scale
    /// has no image of this value (≤ 0 on a logarithmic axis) or if the
    /// framing is degenerate.
    fn pixel(&self, anchor: TextAnchor) -> Option<(i32, i32)> {
        let right = anchor.axis == SeriesAxis::Right && self.right_active;
        let (y_scale, y_range) = if right {
            self.ranges.y2?
        } else {
            self.ranges.y
        };
        let fraction = |scale: AxisScale, (low, high): (f64, f64), value: f64| {
            let t = scale::forward(scale, value)?;
            (high != low).then(|| (t - low) / (high - low))
        };
        let fx = fraction(self.ranges.x.0, self.ranges.x.1, anchor.x)?;
        let fy = fraction(y_scale, y_range, anchor.y)?;
        Some((
            self.origin.0 + (fx * self.size.0 as f64).round() as i32,
            self.origin.1 + ((1.0 - fy) * self.size.1 as f64).round() as i32,
        ))
    }
}

/// Draws `text` (Typst) centered on `pixel`, rotated by `rotation` degrees
/// (counterclockwise), and returns the size of the **straight** text,
/// measured by the same path as everything else.
fn draw_centered_text<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    text: &str,
    style: &TextStyle<'static>,
    rotation: f64,
    pixel: plotters_backend::BackendCoord,
) -> Result<(u32, u32), DrawingAreaErrorKind<DB::ErrorType>> {
    use plotters::style::text_anchor::{HPos, Pos, VPos};
    let centered = Pos::new(HPos::Center, VPos::Center);
    let straight = style.pos(centered);
    // The style of an entered rich text carries the font of its area in its
    // family (`formula::rich_style_font`): the rotated style takes it over.
    let font = match style.font.get_family() {
        FontFamily::Name(name) => crate::formula::font_of_family(name),
        _ => None,
    };
    let family = crate::formula::family_with_layout(
        crate::formula::rotated_family_font(rotation, font),
        crate::formula::layout_of(style),
    );
    let turned = TextStyle {
        font: (family.as_str(), style.font.get_size()).into_font(),
        color: style.color,
        pos: centered,
    };
    let measured = area.estimate_text_size(text, &straight)?;
    area.draw(&Text::new(text, pixel, &turned))?;
    Ok(measured)
}

/// The panel grid: how many rows, how many columns, and in which cell each
/// panel goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Rows of the grid.
    pub rows: usize,
    /// Columns of the grid.
    pub columns: usize,
    /// Cell index of each panel, in spec order.
    pub cells: Vec<usize>,
}

/// Distributes the panels into a grid — task R5.4.
///
/// The nominal case is the one the roadmap describes: `spec.rows()` ×
/// `spec.columns()`, each panel in the cell its `row`/`column` designates.
/// Two panels at the same position are superimposed, the last one wins — this
/// is not a state the rendering has to arbitrate.
///
/// **The fallback exists because positions are not bounded.** `row` and
/// `column` are free `usize`s that nothing validates when a file is read
/// (`stokesforge-plot` documents it, and `PlotSpec::rows` already protects
/// itself with a `saturating_add`). Two panels at `0` and at `usize::MAX`
/// declare a grid of `usize::MAX` rows: impossible to split. Beyond
/// [`MAX_CELLS`], the panels are therefore laid out **in declaration order**
/// in the squarest grid that contains them. The figure is then not the one
/// the spec describes, but it is readable and deterministic, and both
/// backends produce it identically — which is the only property this module
/// has the right to promise.
pub fn grid_of(spec: &PlotSpec, panels: usize) -> Layout {
    let rows = spec.rows();
    let columns = spec.columns();
    let declared = rows
        .checked_mul(columns)
        .filter(|cells| *cells <= MAX_CELLS);
    if declared.is_some() {
        return Layout {
            rows,
            columns,
            cells: spec
                .panels
                .iter()
                .map(|panel| panel.row * columns + panel.column)
                .collect(),
        };
    }

    let count = panels.max(1);
    let columns = (count as f64).sqrt().ceil() as usize;
    let columns = columns.max(1);
    Layout {
        rows: count.div_ceil(columns),
        columns,
        cells: (0..panels).collect(),
    }
}

/// The tick areas reserved in a grid cell: the union of what all its layers
/// ask for, so that superimposed layers (#167) have exactly the same plot
/// area.
#[derive(Debug, Clone, Copy, Default)]
struct CellPlan {
    bottom: bool,
    top: bool,
    left: bool,
    right: bool,
    /// A superimposed layer of the cell has its own axis at the top / on the
    /// right: it takes precedence over a mirror axis (#176).
    top_overlay: bool,
    right_overlay: bool,
    /// Number of superimposed layers of the cell that show their axis at the
    /// top / on the right: each has its band, stacked outward in the order of
    /// the layers (#168).
    top_levels: u32,
    right_levels: u32,
    /// An ordinary layer of the cell asks for a mirror axis at the top / on
    /// the right, and whether one of them keeps its labels.
    top_mirror: bool,
    right_mirror: bool,
    top_labels: bool,
    right_labels: bool,
    /// A series is attached to the right axis: it is a real axis.
    right_data: bool,
}

impl CellPlan {
    fn add(&mut self, panel: &ResolvedPanel<'_>) {
        let spec = panel.spec;
        match spec.overlay {
            Some(overlay) => {
                self.top_overlay |= overlay.x_top && spec.bottom.visible;
                self.right_overlay |= overlay.y_right && spec.left.visible;
                self.top_levels += u32::from(overlay.x_top && spec.bottom.visible);
                self.right_levels += u32::from(overlay.y_right && spec.left.visible);
                self.top |= overlay.x_top && spec.bottom.visible;
                self.right |= overlay.y_right && spec.left.visible;
            }
            None => {
                if spec.mirrors_top() {
                    self.top_mirror = true;
                    self.top = true;
                    self.top_labels |= spec.top.tick_labels;
                }
                if spec.mirrors_right() {
                    self.right_mirror = true;
                    self.right = true;
                    self.right_labels |= spec.right.tick_labels;
                }
                self.bottom |= spec.bottom.visible;
                self.left |= spec.left.visible;
                let data = spec.right.visible
                    && panel
                        .series
                        .iter()
                        .any(|s| s.spec.visible && s.spec.axis == SeriesAxis::Right);
                self.right |= data;
                self.right_data |= data;
            }
        }
    }
}

impl CellPlan {
    /// Bands of the right area when superimposed layers show an axis there:
    /// one per layer, plus that of the right data axis (#168).
    fn right_slots(&self) -> u32 {
        (self.right_levels + u32::from(self.right_data)).max(1)
    }

    /// The top mirror is drawn: requested, and no own axis of a
    /// superimposed layer at that place.
    fn mirror_top(&self) -> bool {
        self.top_mirror && !self.top_overlay
    }

    /// The right mirror is drawn, same rule.
    fn mirror_right(&self) -> bool {
        self.right_mirror && !self.right_overlay
    }

    /// The wanted size for the top area: a mirror without labels only needs
    /// its tick marks.
    fn top_wanted(&self) -> u32 {
        if self.top_overlay {
            X_LABEL_AREA.saturating_mul(self.top_levels.max(1))
        } else if self.top_labels {
            X_LABEL_AREA
        } else {
            BARE_LABEL_AREA
        }
    }

    /// Same for the right area (a real right axis keeps its own there:
    /// `right` is then already true without a mirror).
    fn right_wanted(&self) -> u32 {
        if self.right_overlay {
            Y_LABEL_AREA.saturating_mul(self.right_slots())
        } else if self.right_data || self.right_labels {
            Y_LABEL_AREA
        } else {
            BARE_LABEL_AREA
        }
    }
}

/// What a superimposed layer (#167) inherits from the ordinary layer it covers.
struct BaseFrame<DB: DrawingBackend> {
    area: DrawingArea<DB, Shift>,
    x_range: (f64, f64),
    y_range: (f64, f64),
    x_scale: AxisScale,
    y_scale: AxisScale,
}

/// Draws a panel in its cell.
#[allow(clippy::too_many_arguments)]
fn draw_panel<DB: DrawingBackend>(
    cell: &DrawingArea<DB, Shift>,
    canvas: &DrawingArea<DB, Shift>,
    panel: &ResolvedPanel<'_>,
    index: usize,
    background: (u8, u8, u8, u8),
    plan: CellPlan,
    rank: (u32, u32),
    base: Option<&BaseFrame<DB>>,
    legend_overlays: &[&ResolvedPanel<'_>],
    decimal_comma: bool,
    figure_texts: &FigureTexts,
) -> Result<(DrawnPanel, Projection, BaseFrame<DB>), DrawingAreaErrorKind<DB::ErrorType>> {
    let spec = panel.spec;
    let figure_width = canvas.dim_in_pixel().0;
    // A superimposed layer without a layer to cover is drawn like an ordinary
    // layer.
    let overlay = spec.overlay.filter(|_| base.is_some());
    let base = base.filter(|_| overlay.is_some());
    let x_own = overlay.is_none_or(|o| o.x_top);
    let y_own = overlay.is_none_or(|o| o.y_right);
    // Automatic ink of the panel, see `draw_on`.
    let ink = auto_ink(over_white(background));
    let x_scale = match base {
        Some(base) if !x_own => base.x_scale,
        _ => spec.bottom.scale,
    };
    let y_scale = match base {
        Some(base) if !y_own => base.y_scale,
        _ => spec.left.scale,
    };
    let y2_scale = spec.right.scale;

    // 1 — preparation. An invisible series appears nowhere: neither drawn,
    // nor framed, nor in the legend.
    // `source` keeps the rank of each series in `PanelSpec::series`: the
    // legend designates its rows by this rank (issue #53).
    let (source, mut prepared): (Vec<usize>, Vec<Prepared<'_>>) = panel
        .series
        .iter()
        .enumerate()
        .filter(|(_, series)| series.spec.visible)
        .map(|(rank, series)| {
            let scale = if series.spec.axis == SeriesAxis::Right {
                y2_scale
            } else {
                y_scale
            };
            (rank, series::prepare(series, x_scale, scale))
        })
        .unzip();
    // A superimposed layer has no secondary ordinate: everything is drawn on
    // its Y axis.
    if overlay.is_some() {
        for series in &mut prepared {
            series.axis = SeriesAxis::Left;
        }
    }

    // Mirror axes (#176), except where a superimposed layer has its own axis.
    let mirror_top = overlay.is_none() && plan.mirror_top() && spec.mirrors_top();
    let mirror_right = overlay.is_none() && plan.mirror_right() && spec.mirrors_right();
    let right_active = overlay.is_none()
        && spec.right.visible
        && prepared
            .iter()
            .any(|series| series.axis == SeriesAxis::Right);

    // 2 — framing, in the transformed space.
    let x_range = match base {
        Some(base) if !x_own => base.x_range,
        _ => {
            let own = axis_range(
                &spec.bottom,
                x_scale,
                extent_of(&prepared, series::x_extent, None),
                false,
            );
            join_to_base(
                &spec.bottom,
                x_scale,
                own,
                base.map(|b| (b.x_scale, b.x_range)),
            )
        }
    };
    let y_range = match base {
        Some(base) if !y_own => base.y_range,
        _ => {
            let own = axis_range(
                &spec.left,
                y_scale,
                extent_of(&prepared, series::y_extent, Some(SeriesAxis::Left)),
                needs_baseline(&prepared, SeriesAxis::Left),
            );
            join_to_base(
                &spec.left,
                y_scale,
                own,
                base.map(|b| (b.y_scale, b.y_range)),
            )
        }
    };
    let y2_range = right_active.then(|| {
        axis_range(
            &spec.right,
            y2_scale,
            extent_of(&prepared, series::y_extent, Some(SeriesAxis::Right)),
            needs_baseline(&prepared, SeriesAxis::Right),
        )
    });

    // 3 — the plot area. The panel title lowers it, as the graph title lowers
    // the whole figure.
    // A moved title (issue #53) keeps its empty band, see `draw_on`.
    let panel_title_anchor =
        usable_anchor(spec.title_anchor).filter(|_| spec.title_visible && !spec.title.is_empty());
    let title_style = font_style(
        &spec.title_font,
        PANEL_TITLE_SIZE,
        rgba_or(&spec.title_color, opaque(ink)),
        canvas.dim_in_pixel().0,
    );
    let area = if let Some(base) = base {
        // Linked dimension: the plot area of the covered layer, title
        // included; a superimposed layer has no title of its own.
        base.area.clone()
    } else if !spec.title_visible || spec.title.is_empty() {
        cell.clone()
    } else if panel_title_anchor.is_some() {
        reserve_title_band(cell, &spec.title, &title_style)?
    } else {
        cell.titled(&spec.title, title_style.clone())?
    };
    let mut texts: Vec<DrawnText> = Vec::new();
    if base.is_none()
        && spec.title_visible
        && !spec.title.is_empty()
        && panel_title_anchor.is_none()
    {
        let band = (area.get_base_pixel().1 - cell.get_base_pixel().1).max(0);
        let cell_origin = cell.get_base_pixel();
        let (cell_w, _) = cell.dim_in_pixel();
        texts.push(DrawnText {
            target: TextTarget::PanelTitle { panel: index },
            center: (
                cell_origin.0 as f64 + cell_w as f64 / 2.0,
                cell_origin.1 as f64 + band as f64 / 2.0,
            ),
            size: cell.estimate_text_size(&spec.title, &title_style)?,
            rotation: 0.0,
            anchored: false,
        });
    }
    let (width, height) = area.dim_in_pixel();
    // The tick areas are capped at a third of the cell: in a grid, 64 px of
    // labels on a 150 px panel would leave nothing to draw.
    let x_label_area = label_area(plan.bottom, X_LABEL_AREA, height);
    let top_label_area = label_area(plan.top, plan.top_wanted(), height);
    let y_label_area = label_area(plan.left, Y_LABEL_AREA, width);
    let right_label_area = label_area(plan.right, plan.right_wanted(), width);

    let mut builder = ChartBuilder::on(&area);
    builder
        .margin(MARGIN)
        .x_label_area_size(x_label_area)
        .top_x_label_area_size(top_label_area)
        .y_label_area_size(y_label_area)
        .right_y_label_area_size(right_label_area);
    let chart = builder.build_cartesian_2d(x_range.0..x_range.1, y_range.0..y_range.1)?;

    // The secondary axis is **always** attached, even unused: it takes from
    // the main axis only the top and right areas, which do not exist when
    // they were not asked for. A single shape of `chart` for both cases,
    // hence a single drawing path — the price of a branch here would be the
    // same series code twice.
    let secondary = y2_range.unwrap_or(y_range);
    let mut chart = chart.set_secondary_coord(x_range.0..x_range.1, secondary.0..secondary.1);
    // The tick areas of this layer alone: an axis has an area only where it
    // is displayed (the cell plan reserves it for the other layers too).
    // For a superimposed layer, the band of its axis: the area of the cell
    // shared in equal parts between the axes stacked there, its own shifted
    // by `rank` bands outward (#168).
    let top_stride = top_label_area / plan.top_levels.max(1);
    let right_stride = right_label_area / plan.right_slots();
    let top_offset = rank.0 * top_stride;
    let right_offset = (rank.1 + u32::from(plan.right_data)) * right_stride;
    let (x_area, top_area, y_area, right_area) = match overlay {
        Some(o) => (
            0,
            if o.x_top && spec.bottom.visible {
                top_stride
            } else {
                0
            },
            0,
            if o.y_right && spec.left.visible {
                right_stride
            } else {
                0
            },
        ),
        None => (
            if spec.bottom.visible { x_label_area } else { 0 },
            if mirror_top { top_label_area } else { 0 },
            if spec.left.visible { y_label_area } else { 0 },
            if right_active || mirror_right {
                if plan.right_overlay {
                    right_stride
                } else {
                    right_label_area
                }
            } else {
                0
            },
        ),
    };

    // Pixel positions of the ticks, for the tick marks the pipeline draws itself
    // (#344): those Plotters gives to its labels, on the same interval,
    // projected by the plot area.
    let frame = {
        let plotting = chart.plotting_area();
        (plotting.get_base_pixel(), plotting.dim_in_pixel())
    };
    let ticks_of = |scale: AxisScale, range: (f64, f64)| -> Vec<f64> {
        scale::linear_key_points(range, scale::label_count(scale, range))
    };
    let x_ticks_px: Vec<i32> = ticks_of(x_scale, x_range)
        .into_iter()
        .map(|v| chart.plotting_area().map_coordinate(&(v, y_range.0)).0)
        .collect();
    let y_ticks_px: Vec<i32> = ticks_of(y_scale, y_range)
        .into_iter()
        .map(|v| chart.plotting_area().map_coordinate(&(x_range.0, v)).1)
        .collect();
    // The top and right axes are projected by the secondary frame, whose
    // rounding sometimes differs by a pixel from that of the main frame.
    let x2_ticks_px: Vec<i32> = ticks_of(x_scale, x_range)
        .into_iter()
        .map(|v| {
            chart
                .secondary_plotting_area()
                .map_coordinate(&(v, secondary.0))
                .0
        })
        .collect();
    let y2_ticks_px: Vec<i32> = ticks_of(
        if y2_range.is_some() {
            y2_scale
        } else {
            y_scale
        },
        y2_range.unwrap_or(y_range),
    )
    .into_iter()
    .map(|v| {
        chart
            .secondary_plotting_area()
            .map_coordinate(&(x_range.0, v))
            .1
    })
    .collect();

    // The minor ticks (#344): positions computed as values, then projected
    // like the major ticks (never interpolated between the pixels of those:
    // rounding, inverted axis, log scale).
    let minor_of = |axis: &AxisSpec, scale: AxisScale, range: (f64, f64)| -> Vec<f64> {
        if axis.minor_ticks {
            scale::minor_key_points(scale, range, axis.minor_tick_count)
        } else {
            Vec::new()
        }
    };
    let x_minor_px: Vec<i32> = minor_of(&spec.bottom, x_scale, x_range)
        .into_iter()
        .map(|v| chart.plotting_area().map_coordinate(&(v, y_range.0)).0)
        .collect();
    let y_minor_px: Vec<i32> = minor_of(&spec.left, y_scale, y_range)
        .into_iter()
        .map(|v| chart.plotting_area().map_coordinate(&(x_range.0, v)).1)
        .collect();
    let x2_minor_px: Vec<i32> = minor_of(&spec.top, x_scale, x_range)
        .into_iter()
        .map(|v| {
            chart
                .secondary_plotting_area()
                .map_coordinate(&(v, secondary.0))
                .0
        })
        .collect();
    let y2_minor_px: Vec<i32> = minor_of(
        &spec.right,
        if y2_range.is_some() {
            y2_scale
        } else {
            y_scale
        },
        y2_range.unwrap_or(y_range),
    )
    .into_iter()
    .map(|v| {
        chart
            .secondary_plotting_area()
            .map_coordinate(&(x_range.0, v))
            .1
    })
    .collect();

    // 4 — the axes. Each axis has its own colors (title, scale), and
    // Plotters' `configure_mesh` has only one title style and one axis line
    // style for both: the bottom axis and the left one are therefore drawn in
    // two passes, each cutting the other axis. The geometry does not depend
    // on it — the tick areas are fixed by the `ChartBuilder` above.
    // A moved axis title is no longer written by Plotters (the tick area
    // stays reserved): it is placed on its anchor further down.
    let axis_anchor = |axis: &AxisSpec| {
        usable_anchor(axis.title_anchor)
            .filter(|_| axis.title_visible && !axis.display_label().is_empty())
    };
    let bottom_anchor = axis_anchor(&spec.bottom);
    let left_anchor = axis_anchor(&spec.left);
    let right_anchor = axis_anchor(&spec.right);
    let plotters_desc = |axis: &AxisSpec, anchor: Option<TextAnchor>| {
        if !axis.title_visible || anchor.is_some() {
            String::new()
        } else {
            axis.display_label()
        }
    };
    let tick_x = scale::TickFormat::new(x_scale, x_range).with_decimal_comma(decimal_comma);
    let tick_y = scale::TickFormat::new(y_scale, y_range).with_decimal_comma(decimal_comma);
    let format_x = |value: &f64| tick_x.format(*value);
    let format_y = |value: &f64| tick_y.format(*value);
    // The numbers of an axis: none (#176, the tick marks stay), all, or one in n
    // (#344).
    let shown_x = |axis: &AxisSpec| -> Box<dyn Fn(&f64) -> String> {
        if axis.tick_labels {
            let format = tick_x.with_label_every(axis.label_every, x_range);
            Box::new(move |value| format.format(*value))
        } else {
            Box::new(|_| String::new())
        }
    };
    let shown_y = |axis: &AxisSpec| -> Box<dyn Fn(&f64) -> String> {
        if axis.tick_labels {
            let format = tick_y.with_label_every(axis.label_every, y_range);
            Box::new(move |value| format.format(*value))
        } else {
            Box::new(|_| String::new())
        }
    };
    let labels_bottom = shown_x(&spec.bottom);
    let labels_top = shown_x(&spec.top);
    let labels_left = shown_y(&spec.left);
    let labels_right = shown_y(&spec.right);
    // A superimposed layer has neither a bottom axis nor a left axis: its axes
    // are displayed at the top and on the right, through Plotters' secondary
    // axes (same frame as the layer itself, see below).
    if overlay.is_none() {
        let colors = AxisColors::of(&spec.bottom, ink, figure_width);
        let custom = custom_ticks(&spec.bottom);
        for pass in mesh_passes(custom, spec.bottom.tick_labels) {
            let mut mesh = chart.configure_mesh();
            colors.apply(&mut mesh);
            mesh.disable_y_mesh().disable_y_axis();
            if pass.rest {
                mesh.x_desc(plotters_desc(&spec.bottom, bottom_anchor));
            }
            if !pass.rest || !spec.bottom.grid {
                mesh.disable_x_mesh();
            }
            if !spec.bottom.visible {
                mesh.disable_x_axis();
            }
            if !pass.line {
                mesh.axis_style(NO_INK);
            }
            if pass.zero_ticks {
                mesh.set_all_tick_mark_size(0);
            }
            mesh.x_label_formatter(&*labels_bottom)
                .x_labels(pass.count(scale::label_count(x_scale, x_range)));
            mesh.draw()?;
        }
        if custom && spec.bottom.visible {
            draw_ticks(
                canvas,
                PlotAxisSide::Bottom,
                &spec.bottom,
                &x_ticks_px,
                frame,
                colors.ticks,
            )?;
        }
        if spec.bottom.visible {
            draw_minor_ticks(
                canvas,
                PlotAxisSide::Bottom,
                &spec.bottom,
                &x_minor_px,
                frame,
                colors.ticks,
            )?;
        }
    }
    if overlay.is_none() {
        let colors = AxisColors::of(&spec.left, ink, figure_width);
        let custom = custom_ticks(&spec.left);
        for pass in mesh_passes(custom, spec.left.tick_labels) {
            let mut mesh = chart.configure_mesh();
            colors.apply(&mut mesh);
            mesh.disable_x_mesh().disable_x_axis();
            if pass.rest {
                mesh.y_desc(plotters_desc(&spec.left, left_anchor));
            }
            if !pass.rest || !spec.left.grid {
                mesh.disable_y_mesh();
            }
            if !spec.left.visible {
                mesh.disable_y_axis();
            }
            if !pass.line {
                mesh.axis_style(NO_INK);
            }
            if pass.zero_ticks {
                mesh.set_all_tick_mark_size(0);
            }
            mesh.y_label_formatter(&*labels_left)
                .y_labels(pass.count(scale::label_count(y_scale, y_range)));
            mesh.draw()?;
        }
        if custom && spec.left.visible {
            draw_ticks(
                canvas,
                PlotAxisSide::Left,
                &spec.left,
                &y_ticks_px,
                frame,
                colors.ticks,
            )?;
        }
        if spec.left.visible {
            draw_minor_ticks(
                canvas,
                PlotAxisSide::Left,
                &spec.left,
                &y_minor_px,
                frame,
                colors.ticks,
            )?;
        }
    }

    if let Some(range) = y2_range {
        let tick_y2 = scale::TickFormat::new(y2_scale, range)
            .with_decimal_comma(decimal_comma)
            .with_label_every(spec.right.label_every, range);
        let format_y2 = |value: &f64| {
            if spec.right.tick_labels {
                tick_y2.format(*value)
            } else {
                String::new()
            }
        };
        let colors = AxisColors::of(&spec.right, ink, figure_width);
        // The top tick marks only count if the top mirror is drawn.
        let top_custom = mirror_top && custom_ticks(&spec.top);
        let custom = custom_ticks(&spec.right) || top_custom;
        for pass in mesh_passes(
            custom,
            spec.right.tick_labels || (mirror_top && spec.top.tick_labels),
        ) {
            let mut mesh = chart.configure_secondary_axes();
            mesh.label_style(text_style(LABEL_SIZE, colors.ticks))
                .axis_desc_style(colors.title_style.clone())
                .axis_style(rgba(colors.ticks));
            if pass.rest {
                mesh.y_desc(plotters_desc(&spec.right, right_anchor));
            }
            if !pass.line {
                mesh.axis_style(NO_INK);
            }
            if pass.zero_ticks {
                mesh.set_all_tick_mark_size(0);
            }
            mesh.y_label_formatter(&format_y2)
                .y_labels(pass.count(scale::label_count(y2_scale, range)));
            if mirror_top {
                mesh.x_label_formatter(&*labels_top)
                    .x_labels(pass.count(scale::label_count(x_scale, x_range)));
            }
            mesh.draw()?;
        }
        if custom {
            draw_ticks(
                canvas,
                PlotAxisSide::Right,
                &spec.right,
                &y2_ticks_px,
                frame,
                colors.ticks,
            )?;
            if mirror_top {
                draw_ticks(
                    canvas,
                    PlotAxisSide::Top,
                    &spec.top,
                    &x2_ticks_px,
                    frame,
                    colors.ticks,
                )?;
            }
        }
        draw_minor_ticks(
            canvas,
            PlotAxisSide::Right,
            &spec.right,
            &y2_minor_px,
            frame,
            colors.ticks,
        )?;
        if mirror_top {
            draw_minor_ticks(
                canvas,
                PlotAxisSide::Top,
                &spec.top,
                &x2_minor_px,
                frame,
                colors.ticks,
            )?;
        }
    }

    // The mirror axes (#176): same scale, bounds and ticks as the bottom and
    // the left, without a title. A right axis with data is already drawn
    // above; only the top mirror is then added.
    if (mirror_top || mirror_right) && y2_range.is_none() {
        let colors = AxisColors::of(&spec.bottom, ink, figure_width);
        let custom_top = mirror_top && custom_ticks(&spec.top);
        let custom_right = mirror_right && custom_ticks(&spec.right);
        let custom = custom_top || custom_right;
        let labels =
            (mirror_top && spec.top.tick_labels) || (mirror_right && spec.right.tick_labels);
        for pass in mesh_passes(custom, labels) {
            let mut mesh = chart.configure_secondary_axes();
            mesh.label_style(text_style(LABEL_SIZE, colors.ticks))
                .axis_style(rgba(colors.ticks));
            if !pass.line {
                mesh.axis_style(NO_INK);
            }
            if pass.zero_ticks {
                mesh.set_all_tick_mark_size(0);
            }
            if mirror_top {
                mesh.x_label_formatter(&*labels_top)
                    .x_labels(pass.count(scale::label_count(x_scale, x_range)));
            } else {
                mesh.x_labels(0);
            }
            if mirror_right {
                mesh.y_label_formatter(&*labels_right)
                    .y_labels(pass.count(scale::label_count(y_scale, y_range)));
            } else {
                mesh.y_labels(0);
            }
            mesh.draw()?;
        }
        if custom {
            if mirror_top {
                draw_ticks(
                    canvas,
                    PlotAxisSide::Top,
                    &spec.top,
                    &x2_ticks_px,
                    frame,
                    colors.ticks,
                )?;
            }
            if mirror_right {
                draw_ticks(
                    canvas,
                    PlotAxisSide::Right,
                    &spec.right,
                    &y2_ticks_px,
                    frame,
                    colors.ticks,
                )?;
            }
        }
        if mirror_top {
            draw_minor_ticks(
                canvas,
                PlotAxisSide::Top,
                &spec.top,
                &x2_minor_px,
                frame,
                colors.ticks,
            )?;
        }
        if mirror_right {
            draw_minor_ticks(
                canvas,
                PlotAxisSide::Right,
                &spec.right,
                &y2_minor_px,
                frame,
                colors.ticks,
            )?;
        }
    }

    // The axes of a superimposed layer: each in an "axis only" area that
    // takes the edges of the plot area along the axis direction and sits in
    // its band (Plotters sticks the secondary axis to the plot area; an axis
    // further out asks for another, #168). Each axis keeps its own colors
    // (#396): the top one those of the layer's X axis (`bottom`), the right
    // one those of its Y axis (`left`).
    if let Some(o) = overlay {
        let show_x = o.x_top && spec.bottom.visible && top_area > 0;
        let show_y = o.y_right && spec.left.visible && right_area > 0;
        let colors_x = AxisColors::of(&spec.bottom, ink, figure_width);
        let colors_y = AxisColors::of(&spec.left, ink, figure_width);
        let stub = 4;
        if show_x {
            let y0 = top_label_area.saturating_sub(top_offset + top_area);
            let h = (2 * MARGIN + top_area + stub).min(height.saturating_sub(y0));
            if h > 2 * MARGIN + top_area {
                let sub = area.clone().shrink((0, y0 as i32), (width, h));
                let mut b = ChartBuilder::on(&sub);
                b.margin(MARGIN)
                    .margin_left(MARGIN + y_label_area)
                    .margin_right(MARGIN + right_label_area)
                    .top_x_label_area_size(top_area);
                let mut axis = b
                    .build_cartesian_2d(x_range.0..x_range.1, y_range.0..y_range.1)?
                    .set_secondary_coord(x_range.0..x_range.1, y_range.0..y_range.1);
                let mut mesh = axis.configure_secondary_axes();
                mesh.label_style(text_style(LABEL_SIZE, colors_x.ticks))
                    .axis_desc_style(colors_x.title_style.clone())
                    .axis_style(rgba(colors_x.ticks))
                    .x_desc(plotters_desc(&spec.bottom, bottom_anchor))
                    .x_label_formatter(&format_x)
                    .x_labels(scale::label_count(x_scale, x_range))
                    .y_labels(0);
                mesh.draw()?;
            }
        }
        if show_y {
            // The right edge of the "axis only" area is that of the band.
            let right_edge = width.saturating_sub(right_label_area) + right_offset + right_area;
            let x0 = right_edge.saturating_sub(2 * MARGIN + right_area + stub);
            let w = right_edge - x0;
            if w > 2 * MARGIN + right_area {
                let sub = area.clone().shrink((x0 as i32, 0), (w, height));
                let mut b = ChartBuilder::on(&sub);
                b.margin(MARGIN)
                    .margin_top(MARGIN + top_label_area)
                    .margin_bottom(MARGIN + x_label_area)
                    .right_y_label_area_size(right_area);
                let mut axis = b
                    .build_cartesian_2d(x_range.0..x_range.1, y_range.0..y_range.1)?
                    .set_secondary_coord(x_range.0..x_range.1, y_range.0..y_range.1);
                let mut mesh = axis.configure_secondary_axes();
                mesh.label_style(text_style(LABEL_SIZE, colors_y.ticks))
                    .axis_desc_style(colors_y.title_style.clone())
                    .axis_style(rgba(colors_y.ticks))
                    .y_desc(plotters_desc(&spec.left, left_anchor))
                    .y_label_formatter(&format_y)
                    .y_labels(scale::label_count(y_scale, y_range))
                    .x_labels(0);
                mesh.draw()?;
            }
        }
    }

    // 5 — the series, in pixels.
    let plotting = chart.plotting_area();
    let origin = plotting.get_base_pixel();
    let size = plotting.dim_in_pixel();
    let screen = plotting.use_screen_coord();
    let threshold = decimate::threshold_for_width(size.0);

    // Plotters draws a vertical axis one pixel outside the area (on the left,
    // or on the right) and a horizontal axis one pixel outside it (at the
    // bottom, or at the top): the common corners of two drawn axes stay empty.
    // We fill them, for the whole cell (the plan gathers the axes of all its
    // layers), from the ordinary layer.
    if overlay.is_none() {
        let (left, right) = (origin.0 - 1, origin.0 + size.0 as i32);
        let (top, bottom) = (origin.1 - 1, origin.1 + size.1 as i32);
        let ink = rgba(AxisColors::of(&spec.bottom, ink, figure_width).ticks);
        for (vertical, horizontal, x, y) in [
            (plan.left, plan.bottom, left, bottom),
            (plan.right, plan.bottom, right, bottom),
            (plan.left, plan.top, left, top),
            (plan.right, plan.top, right, top),
        ] {
            if vertical && horizontal {
                // L-shaped, covering both axis lines: a simple corner point
                // would only join them diagonally.
                let along_x = if x == left { x + 1 } else { x - 1 };
                let along_y = if y == top { y + 1 } else { y - 1 };
                canvas.draw(&PathElement::new(
                    vec![(along_x, y), (x, y), (x, along_y)],
                    ink,
                ))?;
            }
        }
    }

    let mut drawn = DrawnPanel {
        index,
        overlay,
        row: spec.row,
        column: spec.column,
        x_range: data_range(x_scale, x_range),
        y_range: data_range(y_scale, y_range),
        y2_range: y2_range.map(|range| data_range(y2_scale, range)),
        pixel_origin: origin,
        pixel_size: size,
        x_scale,
        y_scale,
        y2_scale,
        drawn_series: 0,
        broken_series: 0,
        unknown_series: 0,
        legend_entries: 0,
        decimated_points: 0,
        axis_areas: AxisAreas {
            bottom: AxisArea {
                size: x_area,
                titled: overlay.is_none()
                    && spec.bottom.title_visible
                    && !spec.bottom.display_label().is_empty()
                    && bottom_anchor.is_none(),
                offset: 0,
            },
            left: AxisArea {
                size: y_area,
                titled: overlay.is_none()
                    && spec.left.title_visible
                    && !spec.left.display_label().is_empty()
                    && left_anchor.is_none(),
                offset: 0,
            },
            right: AxisArea {
                size: right_area,
                offset: if overlay.is_some() { right_offset } else { 0 },
                titled: match overlay {
                    Some(_) => {
                        spec.left.title_visible
                            && !spec.left.display_label().is_empty()
                            && left_anchor.is_none()
                    }
                    None => {
                        spec.right.title_visible
                            && !spec.right.display_label().is_empty()
                            && right_anchor.is_none()
                    }
                },
            },
            top: AxisArea {
                size: top_area,
                offset: if overlay.is_some() { top_offset } else { 0 },
                titled: spec.bottom.title_visible
                    && !spec.bottom.display_label().is_empty()
                    && bottom_anchor.is_none(),
            },
        },
        annotations: Vec::new(),
        shapes: Vec::new(),
        series_geometry: Vec::new(),
        texts: Vec::new(),
        legend_rows: Vec::new(),
    };

    // The axis titles in their default place: approximate position (Plotters
    // places them against the outer edge of the tick area), enough to pick
    // them and to start from there when they are moved (issue #53).
    let (plot_x, plot_y) = (origin.0 as f64, origin.1 as f64);
    let (plot_w, plot_h) = (size.0 as f64, size.1 as f64);
    // `side` is the axis of the spec, `place` the edge where it is displayed
    // (a superimposed layer puts its X axis at the top and its Y axis on the
    // right, #167).
    let placed: [(
        &AxisSpec,
        Option<TextAnchor>,
        PlotAxisSide,
        PlotAxisSide,
        u32,
    ); 3] = match overlay {
        Some(_) => [
            (
                &spec.bottom,
                bottom_anchor,
                PlotAxisSide::Bottom,
                PlotAxisSide::Top,
                top_area,
            ),
            (
                &spec.left,
                left_anchor,
                PlotAxisSide::Left,
                PlotAxisSide::Right,
                right_area,
            ),
            (
                &spec.right,
                None,
                PlotAxisSide::Right,
                PlotAxisSide::Right,
                0,
            ),
        ],
        None => [
            (
                &spec.bottom,
                bottom_anchor,
                PlotAxisSide::Bottom,
                PlotAxisSide::Bottom,
                x_area,
            ),
            (
                &spec.left,
                left_anchor,
                PlotAxisSide::Left,
                PlotAxisSide::Left,
                y_area,
            ),
            (
                &spec.right,
                right_anchor,
                PlotAxisSide::Right,
                PlotAxisSide::Right,
                if right_active { right_label_area } else { 0 },
            ),
        ],
    };
    for (axis, anchor, side, place, area_size) in placed {
        let label = axis.display_label();
        if !axis.title_visible || label.is_empty() || anchor.is_some() || area_size == 0 {
            continue;
        }
        let colors = AxisColors::of(axis, ink, figure_width);
        let text_size = area.estimate_text_size(&label, &colors.title_style)?;
        let half = text_size.1 as f64 / 2.0;
        let area_size = area_size as f64;
        // A stacked superimposed-layer axis (#168) is shifted outward.
        let shift = match (overlay.is_some(), place) {
            (true, PlotAxisSide::Top) => top_offset as f64,
            (true, PlotAxisSide::Right) => right_offset as f64,
            _ => 0.0,
        };
        let (center, rotation) = match place {
            PlotAxisSide::Left => ((plot_x - area_size + half, plot_y + plot_h / 2.0), 90.0),
            PlotAxisSide::Right => (
                (
                    plot_x + plot_w + shift + area_size - half,
                    plot_y + plot_h / 2.0,
                ),
                -90.0,
            ),
            PlotAxisSide::Top => (
                (plot_x + plot_w / 2.0, plot_y - shift - area_size + half),
                0.0,
            ),
            PlotAxisSide::Bottom => (
                (plot_x + plot_w / 2.0, plot_y + plot_h + area_size - half),
                0.0,
            ),
        };
        texts.push(DrawnText {
            target: TextTarget::AxisTitle { panel: index, side },
            center,
            size: text_size,
            rotation,
            anchored: false,
        });
    }

    // The plot rectangle, in pixels: it is against this that the segments of
    // a curve are clipped before `paint::polyline` (see `crate::clip`), and
    // not against the framing in data — a logarithmic axis would not be
    // linear there (head doc of `clip`).
    let bounds = PixelBounds::of_panel(origin, size);

    for (rank, series) in source.iter().copied().zip(&mut prepared) {
        if series.broken {
            drawn.broken_series += 1;
            continue;
        }
        if series.unknown_type {
            drawn.unknown_series += 1;
            continue;
        }
        if series.geometry.is_empty() {
            continue;
        }
        let right = series.axis == SeriesAxis::Right && right_active;
        let (removed, mut geometry) = paint_series(
            &screen,
            |point| {
                if right {
                    chart.secondary_plotting_area().map_coordinate(&point)
                } else {
                    chart.plotting_area().map_coordinate(&point)
                }
            },
            series,
            threshold,
            (x_range, if right { secondary } else { y_range }),
            bounds,
            ink,
        )?;
        geometry.index = rank;
        drawn.series_geometry.push(geometry);
        series.decimated = removed;
        drawn.decimated_points += removed;
        drawn.drawn_series += 1;
    }

    // 6 — the legend, on top, and measured like the axes.
    let projection = Projection {
        origin,
        size,
        ranges: Ranges {
            x: (x_scale, x_range),
            y: (y_scale, y_range),
            y2: y2_range.map(|range| (y2_scale, range)),
        },
        right_active,
    };
    let legend_center = usable_anchor(spec.legend.anchor)
        .and_then(|anchor| projection.pixel(anchor))
        .filter(|_| spec.legend.visible);
    let entry_ranks: Vec<usize> = prepared
        .iter()
        .enumerate()
        .filter(|(_, series)| series.in_legend())
        .map(|(index, _)| source[index])
        .collect();
    // The curves of the superimposed layers follow, layer by layer (#172);
    // they have no rank in this panel, hence no pickable row.
    let overlay_prepared: Vec<Prepared<'_>> = legend_overlays
        .iter()
        .flat_map(|layer| {
            let own = layer.spec.overlay.unwrap_or_default();
            let x = if own.x_top {
                layer.spec.bottom.scale
            } else {
                x_scale
            };
            let y = if own.y_right {
                layer.spec.left.scale
            } else {
                y_scale
            };
            layer
                .series
                .iter()
                .filter(|series| series.spec.visible)
                .map(move |series| series::prepare(series, x, y))
        })
        .collect();
    let entries: Vec<&Prepared<'_>> = prepared
        .iter()
        .chain(&overlay_prepared)
        .filter(|s| s.in_legend())
        .collect();
    let legend_ink = rgba_or(&spec.legend.color, opaque(ink));
    let inscribed = legend::draw(
        canvas,
        origin,
        size,
        &spec.legend,
        &entries,
        &legend::LegendText {
            typed: font_style(
                &spec.legend.font,
                LABEL_SIZE,
                legend_ink,
                canvas.dim_in_pixel().0,
            ),
            literal: literal_style_font(
                spec.legend.font.effective_size(LABEL_SIZE),
                legend_ink,
                spec.legend.font.family(),
            ),
        },
        figure_texts,
        legend::LegendColors { background, ink },
        legend_center,
    )?;
    drawn.legend_entries = inscribed.entries;
    if let Some(((left, top), (frame_w, frame_h))) = inscribed.frame {
        texts.push(DrawnText {
            target: TextTarget::Legend { panel: index },
            center: (
                left as f64 + frame_w as f64 / 2.0,
                top as f64 + frame_h as f64 / 2.0,
            ),
            size: (frame_w, frame_h),
            rotation: 0.0,
            anchored: legend_center.is_some(),
        });
        drawn.legend_rows = inscribed
            .rows
            .iter()
            .zip(&entry_ranks)
            .map(|(&(center_y, height), &series)| DrawnLegendRow {
                series,
                center_y,
                height,
            })
            .collect();
    }

    // The moved titles, placed on their anchor (issue #53): on top of the
    // series and the legend, like the free texts right after.
    let centered = |anchor: TextAnchor| {
        projection.pixel(anchor).unwrap_or((
            origin.0 + (size.0 / 2) as i32,
            origin.1 + (size.1 / 2) as i32,
        ))
    };
    if let Some(anchor) = panel_title_anchor {
        let pixel = centered(anchor);
        let measured = draw_centered_text(canvas, &spec.title, &title_style, 0.0, pixel)?;
        texts.push(DrawnText {
            target: TextTarget::PanelTitle { panel: index },
            center: (pixel.0 as f64, pixel.1 as f64),
            size: measured,
            rotation: 0.0,
            anchored: true,
        });
    }
    for (axis, anchor, side, rotation) in [
        (&spec.bottom, bottom_anchor, PlotAxisSide::Bottom, 0.0),
        (&spec.left, left_anchor, PlotAxisSide::Left, 90.0),
        (&spec.right, right_anchor, PlotAxisSide::Right, -90.0),
    ] {
        let Some(anchor) = anchor else { continue };
        let pixel = centered(anchor);
        let colors = AxisColors::of(axis, ink, figure_width);
        let measured = draw_centered_text(
            canvas,
            &axis.display_label(),
            &colors.title_style,
            rotation,
            pixel,
        )?;
        texts.push(DrawnText {
            target: TextTarget::AxisTitle { panel: index, side },
            center: (pixel.0 as f64, pixel.1 as f64),
            size: measured,
            rotation,
            anchored: true,
        });
    }
    drawn.texts = texts;

    let ranges = Ranges {
        x: (x_scale, x_range),
        y: (y_scale, y_range),
        y2: y2_range.map(|range| (y2_scale, range)),
    };

    // 6 bis — the graphic objects (issue #76), from bottom to top, under the
    // free texts. Projected without a test of membership in the framing: a
    // rectangle that overflows the zoom is clipped, not omitted.
    for (index, shape) in spec.shapes.iter().enumerate() {
        let right = shape.axis == SeriesAxis::Right && right_active;
        if !shape.visible || !shape.is_finite() {
            continue;
        }
        let pixel_of = |x: f64, y: f64| {
            let point = ranges.forward(x, y, right)?;
            Some(if right {
                chart.secondary_plotting_area().map_coordinate(&point)
            } else {
                chart.plotting_area().map_coordinate(&point)
            })
        };
        let (Some(a), Some(b)) = (pixel_of(shape.x1, shape.y1), pixel_of(shape.x2, shape.y2))
        else {
            continue;
        };
        let (p1, p2) = ((a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64));
        crate::shapes::draw_shape(&screen, shape, p1, p2, bounds, ink)?;
        drawn.shapes.push(crate::shapes::DrawnShape {
            index,
            kind: shape.kind,
            p1,
            p2,
            filled: shape.kind.is_closed() && parse_hex(&shape.fill_color).is_some(),
            width: shape.effective_width(),
        });
    }

    // 7 — the free texts, on the very top (issue #35). Like an anchored
    // title, they are not bounded to the framing: a text placed in the
    // margins is still drawn (#163).
    for (index, annotation) in spec.annotations.iter().enumerate() {
        if !annotation.visible {
            continue;
        }
        let right = annotation.axis == SeriesAxis::Right && right_active;
        let Some(point) = ranges.forward(annotation.x, annotation.y, right) else {
            continue;
        };
        let pixel = if right {
            chart.secondary_plotting_area().map_coordinate(&point)
        } else {
            chart.plotting_area().map_coordinate(&point)
        };
        if let Some(drawn_annotation) = draw_annotation(canvas, annotation, index, pixel, ink)? {
            drawn.annotations.push(drawn_annotation);
        }
    }

    // R7: the crosshair, the elastic zoom rectangle and point picking are
    // painted **on top of** what precedes, and above all on top of the
    // `Shape` cache — so not here, lest the whole pipeline be replayed at each
    // mouse move (`architecture-choisie.md` §4.2, task 7.3).
    let frame = BaseFrame {
        area,
        x_range,
        y_range,
        x_scale,
        y_scale,
    };
    Ok((drawn, projection, frame))
}

/// The framings of a panel, in the transformed space, with their scale —
/// enough to project a data point like a series.
struct Ranges {
    x: (AxisScale, (f64, f64)),
    y: (AxisScale, (f64, f64)),
    y2: Option<(AxisScale, (f64, f64))>,
}

impl Ranges {
    /// The point `(x, y)` in the transformed space, **without** a test of
    /// membership in the framing (graphic objects #76, free texts #163);
    /// `None` only if the scale cannot represent it (≤ 0 on a log axis).
    fn forward(&self, x: f64, y: f64, right: bool) -> Option<(f64, f64)> {
        let (y_scale, _) = if right { self.y2? } else { self.y };
        Some((scale::forward(self.x.0, x)?, scale::forward(y_scale, y)?))
    }
}

/// Draws a free text centered on `pixel`, and returns what was drawn.
///
/// Entered rich text (Typst), rotated by Typst at a free angle
/// ([`crate::formula::rotated_family`]). The reported size is that of the
/// **straight** text, measured by the same path: the pickable rectangle is
/// this rectangle, rotated around the center.
fn draw_annotation<DB: DrawingBackend>(
    screen: &DrawingArea<DB, Shift>,
    annotation: &stokesforge_plot::AnnotationSpec,
    index: usize,
    pixel: plotters_backend::BackendCoord,
    ink: (u8, u8, u8),
) -> Result<Option<DrawnAnnotation>, DrawingAreaErrorKind<DB::ErrorType>> {
    if annotation.text.is_empty() {
        return Ok(None);
    }
    let color = rgba_or(&annotation.color, opaque(ink));
    let size = annotation.effective_size();
    let rotation = annotation.effective_rotation();
    // A fixed width (fraction of the figure, capped to it) makes the text wrap
    // in a box of this width (#244).
    let layout = annotation.effective_width().map(|fraction| {
        (
            // Rounded to the pixel: the style's family name is retained
            // forever (`formula::interned`), one width per value.
            (fraction * f64::from(screen.dim_in_pixel().0))
                .round()
                .max(1.0),
            annotation.align,
        )
    });
    let (width, height) = draw_centered_text(
        screen,
        &annotation.text,
        &crate::formula::rich_style_layout(size, color, Some(annotation.font.as_str()), layout),
        rotation,
        pixel,
    )?;
    Ok(Some(DrawnAnnotation {
        index,
        center: (pixel.0 as f64, pixel.1 as f64),
        size: (width, height),
        rotation,
    }))
}

/// Paints the error-bar whiskers of a series (any type that carries them,
/// #360) and records their clipped pixel segments for selection.
fn paint_whiskers<DB: DrawingBackend>(
    screen: &DrawingArea<DB, Shift>,
    whiskers: &[crate::series::Whisker],
    map: &impl Fn((f64, f64)) -> plotters_backend::BackendCoord,
    series: &Prepared<'_>,
    bounds: Option<PixelBounds>,
    geometry: &mut crate::series_selection::DrawnSeries,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if whiskers.is_empty() {
        return Ok(());
    }
    let spec = series.spec;
    let segments: Vec<_> = whiskers
        .iter()
        .map(|(from, to)| (map(*from), map(*to)))
        .collect();
    if series.stale {
        paint::whiskers(screen, &segments, &paint::grayed(&spec.line))?;
    } else {
        paint::whiskers(screen, &segments, &spec.line)?;
    }
    for (a, b) in segments {
        match bounds {
            Some(bounds) => {
                if let Some((a, b)) =
                    clip::clip_segment(bounds, (a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64))
                {
                    geometry.whiskers.push((
                        (a.0.round() as i32, a.1.round() as i32),
                        (b.0.round() as i32, b.1.round() as i32),
                    ));
                }
            }
            None => geometry.whiskers.push((a, b)),
        }
    }
    Ok(())
}

/// Paints a prepared series, and returns the number of points that
/// decimation removed.
fn paint_series<DB: DrawingBackend>(
    screen: &DrawingArea<DB, Shift>,
    map: impl Fn((f64, f64)) -> plotters_backend::BackendCoord,
    series: &Prepared<'_>,
    threshold: usize,
    ranges: ((f64, f64), (f64, f64)),
    bounds: Option<PixelBounds>,
    ink: (u8, u8, u8),
) -> Result<(usize, crate::series_selection::DrawnSeries), DrawingAreaErrorKind<DB::ErrorType>> {
    let mut geometry = crate::series_selection::DrawnSeries::default();
    let spec = series.spec;
    match &series.geometry {
        Geometry::Nothing => Ok((0, geometry)),
        Geometry::Bars { bars, whiskers } => {
            // The baseline of a bar is zero when the axis can represent it,
            // and the bottom of the framing otherwise — on a logarithmic
            // scale, zero has no place.
            let baseline = scale_baseline(ranges.1);
            let rects: Vec<_> = bars
                .iter()
                .map(|(left, right, top)| (map((*left, *top)), map((*right, baseline))))
                .collect();
            if series.stale {
                paint::bars(screen, &rects, &paint::grayed(&spec.line), ink)?;
            } else {
                paint::bars(screen, &rects, &spec.line, ink)?;
            }
            // `a` is the corner on the value side, `b` the corner on the
            // baseline side: the free end of the bar (the place of its
            // selection handle, #98) is read before the rectangle is normalized.
            (geometry.bars, geometry.bar_tips) = rects
                .into_iter()
                .filter_map(|(a, b)| {
                    let (mut left, mut right) = (a.0.min(b.0), a.0.max(b.0));
                    let (mut top, mut bottom) = (a.1.min(b.1), a.1.max(b.1));
                    if let Some(bounds) = bounds {
                        left = left.max(bounds.x_min as i32);
                        right = right.min(bounds.x_max as i32);
                        top = top.max(bounds.y_min as i32);
                        bottom = bottom.min(bounds.y_max as i32);
                    }
                    // `clamp` only once the rectangle is judged non-empty: it
                    // panics on inverted bounds.
                    (left <= right && top <= bottom).then(|| {
                        let tip = ((left + right) / 2, a.1.clamp(top, bottom));
                        (((left, top), (right, bottom)), tip)
                    })
                })
                .unzip();
            paint_whiskers(screen, whiskers, &map, series, bounds, &mut geometry)?;
            if series.stale {
                if let Some(&tip) = geometry.bar_tips.last() {
                    paint::stale_marker(screen, stale_marker_at(tip, bounds))?;
                }
            }
            Ok((0, geometry))
        }
        Geometry::Curve { points, whiskers } => {
            // Decimating a series with whiskers would dissociate the error
            // bars from their points; decimating a scatter without a line
            // makes no sense either, LTTB reasoning along a path.
            let decimable =
                spec.plot_type.draws_line() && whiskers.is_empty() && points.len() > threshold;
            let decimated = decimable.then(|| decimate::lttb(points, threshold));
            let drawn_points: &[(f64, f64)] = decimated.as_deref().unwrap_or(points);
            let removed = points.len() - drawn_points.len();

            if spec.plot_type.draws_line() && spec.line.style != stokesforge_plot::LineStyle::None {
                // Plotters' integer endpoints fix the frame (last pixel
                // included, inverted Y axis). Interpolating in the already
                // transformed space avoids its per-vertex rounding: when
                // zooming out, this made steps on the Jasco spectrum (#150).
                let (xr, yr) = ranges;
                let a = map((xr.0, yr.0));
                let b = map((xr.1, yr.1));
                let precise: Vec<_> = drawn_points
                    .iter()
                    .map(|&(x, y)| {
                        (
                            a.0 as f64 + (x - xr.0) / (xr.1 - xr.0) * (b.0 - a.0) as f64,
                            a.1 as f64 + (y - yr.0) / (yr.1 - yr.0) * (b.1 - a.1) as f64,
                        )
                    })
                    .collect();
                // Geometric clipping against the plot rectangle, **before**
                // `paint::polyline_f64`: a segment that crosses the panel is
                // cut at the real intersection rather than brought back onto
                // the edge per component (the defect documented at the top of
                // `crate::paint`, fixed by `crate::clip`). With a framing
                // that does not overflow the data, each segment stays entirely
                // inside `bounds` and this clipping changes nothing:
                // `clip_segment` then returns the segment as is.
                match bounds {
                    Some(bounds) => {
                        for run in clip::clip_polyline_f64(bounds, &precise) {
                            paint::polyline_f64(screen, &run, &spec.line, series.stale)?;
                            geometry.paths.push(quantized_path(&run));
                        }
                    }
                    None => {
                        paint::polyline_f64(screen, &precise, &spec.line, series.stale)?;
                        geometry.paths.push(quantized_path(&precise));
                    }
                }
            }
            paint_whiskers(screen, whiskers, &map, series, bounds, &mut geometry)?;
            if spec.plot_type.draws_symbols()
                && spec.symbol.shape != stokesforge_plot::SymbolShape::None
            {
                let mapped: Vec<_> = drawn_points.iter().map(|point| map(*point)).collect();
                paint::symbols(screen, &mapped, &spec.symbol, series.stale)?;
                geometry.points = mapped
                    .into_iter()
                    .filter(|&(x, y)| {
                        bounds.is_none_or(|b| {
                            x as f64 >= b.x_min
                                && x as f64 <= b.x_max
                                && y as f64 >= b.y_min
                                && y as f64 <= b.y_max
                        })
                    })
                    .collect();
            }
            if series.stale {
                if let Some(&last) = drawn_points.last() {
                    paint::stale_marker(screen, stale_marker_at(map(last), bounds))?;
                }
            }
            Ok((removed, geometry))
        }
    }
}

/// Where to put the marker of an out-of-date series: just above its last
/// point (it does not hide it), brought back into the plot rectangle.
fn stale_marker_at(point: (i32, i32), bounds: Option<PixelBounds>) -> (i32, i32) {
    const INSET: i32 = 10;
    let (x, y) = (point.0, point.1 - 12);
    match bounds {
        Some(b) => (
            x.clamp(
                b.x_min as i32 + INSET,
                (b.x_max as i32 - INSET).max(b.x_min as i32 + INSET),
            ),
            y.clamp(
                b.y_min as i32 + INSET,
                (b.y_max as i32 - INSET).max(b.y_min as i32 + INSET),
            ),
        ),
        None => (x, y),
    }
}

/// Picking geometry identical to the vertices actually sent to the backend
/// (sixteenths of a pixel), rather than a second integer rounding.
fn quantized_path(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    points
        .iter()
        .copied()
        .map(|p| crate::subpixel::decode(crate::subpixel::encode(p)))
        .collect()
}

/// The baseline of the bars, in the transformed space.
fn scale_baseline((low, high): (f64, f64)) -> f64 {
    let (bottom, top) = if low <= high {
        (low, high)
    } else {
        (high, low)
    };
    0.0f64.clamp(bottom, top)
}

/// True if at least one series of this axis rests on a baseline.
fn needs_baseline(prepared: &[Prepared<'_>], axis: SeriesAxis) -> bool {
    prepared
        .iter()
        .filter(|series| series.axis == axis)
        .any(|series| series.geometry.needs_baseline())
}

/// Extent of a set of prepared series.
///
/// `axis` restricts to the series of a given axis (for the ordinates); `None`
/// takes everything (for the abscissas, common to both vertical axes).
fn extent_of(
    prepared: &[Prepared<'_>],
    extent: impl Fn(&Geometry) -> Option<(f64, f64)>,
    axis: Option<SeriesAxis>,
) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut seen = false;
    for series in prepared {
        if series.broken {
            continue;
        }
        if axis.is_some_and(|wanted| series.axis != wanted) {
            continue;
        }
        if let Some((low, high)) = extent(&series.geometry) {
            min = min.min(low);
            max = max.max(high);
            seen = true;
        }
    }
    seen.then_some((min, max))
}

/// Bounds of an axis, in the transformed space.
///
/// The explicit framing of the spec takes precedence if it is complete **and**
/// representable on the chosen scale; otherwise the extent of the data;
/// otherwise `0..1`. A framing "from 800 to 400" (`range_min > range_max`)
/// draws the axis reversed, like [`AxisSpec::inverted`]: the two do not add up.
fn axis_range(
    spec: &AxisSpec,
    scale: AxisScale,
    data: Option<(f64, f64)>,
    with_baseline: bool,
) -> (f64, f64) {
    let mut reversed = false;
    let explicit = spec
        .has_explicit_range()
        .then(|| match (spec.range_min, spec.range_max) {
            (Some(min), Some(max)) if min.is_finite() && max.is_finite() && min != max => {
                reversed = min > max;
                let (low, high) = (min.min(max), min.max(max));
                match (scale::forward(scale, low), scale::forward(scale, high)) {
                    (Some(low), Some(high)) if low != high => Some((low, high)),
                    _ => None,
                }
            }
            _ => None,
        })
        .flatten();

    let (min, max) = match explicit {
        Some(range) => range,
        None => padded(scale, data, with_baseline),
    };
    if spec.inverted || reversed {
        (max, min)
    } else {
        (min, max)
    }
}

/// Joins the axis of a superimposed layer to that of the covered layer (#174):
/// same bounds at the corners, ticks opposite. No effect outside a
/// superimposed layer, on a logarithmic scale (decades already align) or with
/// an extent set by hand.
fn join_to_base(
    spec: &AxisSpec,
    scale: AxisScale,
    own: (f64, f64),
    base: Option<(AxisScale, (f64, f64))>,
) -> (f64, f64) {
    match base {
        Some((base_scale, base_range))
            if scale == AxisScale::Linear
                && base_scale == AxisScale::Linear
                && !spec.has_explicit_range() =>
        {
            scale::aligned_range(base_range, own).unwrap_or(own)
        }
        _ => own,
    }
}

/// The extent of the data, widened by what it takes to be a framing.
fn padded(scale: AxisScale, data: Option<(f64, f64)>, with_baseline: bool) -> (f64, f64) {
    // No data: `0..1`, which is worth "one decade" on a logarithmic scale
    // since the interval there is already that of the exponents.
    let Some((mut min, mut max)) = data else {
        return (0.0, 1.0);
    };
    if with_baseline {
        // A bar that does not show where it starts lies about its height.
        if let Some(baseline) = scale::forward(scale, 0.0) {
            min = min.min(baseline);
            max = max.max(baseline);
        }
    }
    if matches!(scale, AxisScale::Log) {
        // The decades stand in for a margin: widening before snapping to them
        // would change nothing most of the time, and would add an empty
        // decade the rest of the time.
        return scale::snap_to_decades((min, max));
    }
    if min == max {
        // A constant series, or a single point: without this spreading,
        // `build_cartesian_2d` would receive an empty interval and Plotters
        // would divide by zero when projecting.
        let pad = if min == 0.0 { 1.0 } else { min.abs() * 0.05 };
        return (min - pad, max + pad);
    }
    let pad = (max - min) * 0.05;
    (min - pad, max + pad)
}

/// Puts a transformed interval back into the data space, to be reported to
/// the caller.
fn data_range(scale: AxisScale, (low, high): (f64, f64)) -> (f64, f64) {
    (scale::inverse(scale, low), scale::inverse(scale, high))
}

/// Effective tick area: the requested one, capped at a third of the available
/// dimension, zero if the axis is hidden.
fn label_area(visible: bool, wanted: u32, available: u32) -> u32 {
    if !visible {
        return 0;
    }
    wanted.min(available / 3)
}

/// The style of the **ticks**, at the given size and in the given color
/// (opacity included): laid out by `epaint`, never by Typst. Titles, axis
/// titles and legend take [`rich_style`] (issue #32).
fn text_style(size: f64, color: (u8, u8, u8, u8)) -> TextStyle<'static> {
    (FONT_FAMILY, size).into_font().color(&rgba(color))
}

/// The style of an entered rich text (title, axis title, legend): the given
/// color, the size and font of its area (issues #77, #78), `default_size` and
/// the graph's font when they are not set.
///
/// `figure_width` is the width of the figure in backend pixels: an area width
/// (#244) is a fraction of it, rounded to the pixel (the style's family name
/// is retained forever), and also holds for a vertical title.
fn font_style(
    font: &stokesforge_plot::TextFont,
    default_size: f64,
    color: (u8, u8, u8, u8),
    figure_width: u32,
) -> TextStyle<'static> {
    let layout = font.effective_width().map(|fraction| {
        (
            (fraction * f64::from(figure_width)).round().max(1.0),
            font.align,
        )
    });
    crate::formula::rich_style_layout(
        font.effective_size(default_size),
        color,
        font.family(),
        layout,
    )
}

/// Opacity of the main grid lines, in the color of the scale — the one
/// Plotters gives by default on black.
const GRID_BOLD_OPACITY: f64 = 0.2;
/// Opacity of the secondary grid lines.
const GRID_LIGHT_OPACITY: f64 = 0.1;

/// A tint with no effect: Plotters skips drawings of zero alpha (egui and SVG alike).
const NO_INK: RGBAColor = RGBAColor(0, 0, 0, 0.0);

/// The tick marks of this axis are not Plotters' (#344): cut, of a direction
/// other than outward, or set in "Advanced" (length, thickness, color). The
/// default stays Plotters', so that existing figures do not move by a pixel.
fn custom_ticks(axis: &AxisSpec) -> bool {
    !axis.ticks
        || axis.tick_direction != TickDirection::Out
        || axis.tick_length.is_some()
        || axis.tick_width.is_some()
        || !axis.tick_mark_color.is_empty()
}

/// One pass of `configure_mesh` / `configure_secondary_axes` on an axis.
///
/// Plotters only draws the tick marks along with the axis line, and places the
/// numbers at twice the length of the tick marks: without tick marks, they stick to
/// the line. An axis with custom tick marks therefore takes two passes: the
/// numbers alone (line and tick marks of zero alpha, original gap), then the line
/// alone (tick marks of zero length, empty numbers). The pipeline then draws the
/// tick marks ([`draw_ticks`]).
#[derive(Debug, Clone, Copy)]
struct MeshPass {
    /// Grid and title: a single pass draws them.
    rest: bool,
    /// The numbers are written (otherwise no tick is requested).
    labels: bool,
    /// The axis line is drawn (otherwise of zero alpha).
    line: bool,
    /// Tick marks of zero length.
    zero_ticks: bool,
}

impl MeshPass {
    /// Number of ticks to request: none in the "line only" pass, which writes
    /// nothing (not even an empty text).
    fn count(self, count: usize) -> usize {
        if self.labels {
            count
        } else {
            0
        }
    }
}

fn mesh_passes(custom: bool, labels: bool) -> Vec<MeshPass> {
    match (custom, labels) {
        (false, _) => vec![MeshPass {
            rest: true,
            labels: true,
            line: true,
            zero_ticks: false,
        }],
        (true, true) => vec![
            MeshPass {
                rest: true,
                labels: true,
                line: false,
                zero_ticks: false,
            },
            MeshPass {
                rest: false,
                labels: false,
                line: true,
                zero_ticks: true,
            },
        ],
        (true, false) => vec![MeshPass {
            rest: true,
            labels: false,
            line: true,
            zero_ticks: true,
        }],
    }
}

/// Length of Plotters' tick marks: 5 % of the smaller side of the plot area,
/// capped at 5 px (`(5u32).percent().max(5)`: `max` bounds from above, it
/// does not guarantee a minimum).
fn tick_length(size: (u32, u32)) -> i32 {
    ((f64::from(size.0.min(size.1)) * 0.05).round() as i32).min(5)
}

/// Cap of a tick mark length requested by hand (#344): a value coming from a file
/// or a script can be anything (invariant 10).
const MAX_TICK_LENGTH: f64 = 50.0;

/// The tick marks of one side, "Advanced" settings resolved (#344): length in
/// pixels, thickness and color. The style is set only by [`draw_marks`].
#[derive(Clone, Copy)]
struct MarkStyle {
    len: i32,
    width: u32,
    color: (u8, u8, u8, u8),
}

impl MarkStyle {
    /// The major tick marks: the requested length if it is a finite number
    /// (bounded to `0..=MAX_TICK_LENGTH`), otherwise the automatic one.
    fn major(axis: &AxisSpec, plot_size: (u32, u32), ink: (u8, u8, u8, u8)) -> Self {
        let len = match axis.tick_length {
            Some(len) if len.is_finite() => len.clamp(0.0, MAX_TICK_LENGTH).round() as i32,
            _ => tick_length(plot_size),
        };
        Self::with_len(axis, len, ink)
    }

    /// The minor tick marks: the requested length, otherwise half (rounded up) of
    /// the effective major tick marks.
    fn minor(axis: &AxisSpec, plot_size: (u32, u32), ink: (u8, u8, u8, u8)) -> Self {
        let len = match axis.minor_tick_length {
            Some(len) if len.is_finite() => len.clamp(0.0, MAX_TICK_LENGTH).round() as i32,
            _ => (Self::major(axis, plot_size, ink).len + 1) / 2,
        };
        Self::with_len(axis, len, ink)
    }

    fn with_len(axis: &AxisSpec, len: i32, ink: (u8, u8, u8, u8)) -> Self {
        Self {
            len,
            width: axis.tick_width.map_or(1, paint::stroke_width),
            color: rgba_or(&axis.tick_mark_color, ink),
        }
    }
}

/// Draws the tick marks of one side: one segment per tick, on either side of the
/// axis line according to [`TickDirection`]. Same geometry as Plotters for
/// "outward" (the line is one pixel from the plot area).
fn draw_ticks<DB: DrawingBackend>(
    canvas: &DrawingArea<DB, Shift>,
    side: PlotAxisSide,
    axis: &AxisSpec,
    positions: &[i32],
    frame: ((i32, i32), (u32, u32)),
    ink: (u8, u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if !axis.ticks {
        return Ok(());
    }
    draw_marks(
        canvas,
        side,
        axis.tick_direction,
        positions,
        frame,
        MarkStyle::major(axis, frame.1, ink),
    )
}

/// Draws the minor ticks of one side (#344): half the length of the major
/// tick marks (rounded up) unless set in "Advanced", same direction. Nothing
/// when the minor ticks are off.
fn draw_minor_ticks<DB: DrawingBackend>(
    canvas: &DrawingArea<DB, Shift>,
    side: PlotAxisSide,
    axis: &AxisSpec,
    positions: &[i32],
    frame: ((i32, i32), (u32, u32)),
    ink: (u8, u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if !axis.minor_ticks {
        return Ok(());
    }
    draw_marks(
        canvas,
        side,
        axis.tick_direction,
        positions,
        frame,
        MarkStyle::minor(axis, frame.1, ink),
    )
}

/// A segment of length `style.len` per position, on either side of the axis
/// line according to `direction`. The only place that sets the style of the
/// tick marks (invariant 5: a single write for both backends).
fn draw_marks<DB: DrawingBackend>(
    canvas: &DrawingArea<DB, Shift>,
    side: PlotAxisSide,
    direction: TickDirection,
    positions: &[i32],
    frame: ((i32, i32), (u32, u32)),
    style: MarkStyle,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    let MarkStyle { len, width, color } = style;
    if len == 0 {
        return Ok(());
    }
    let ((x0, y0), size) = frame;
    let (w, h) = (size.0 as i32, size.1 as i32);
    // Outward direction on the perpendicular, and pixel of the line.
    let (outward, line) = match side {
        PlotAxisSide::Left => (-1, x0 - 1),
        PlotAxisSide::Right => (1, x0 + w),
        PlotAxisSide::Top => (-1, y0 - 1),
        PlotAxisSide::Bottom => (1, y0 + h),
    };
    let (from, to) = match direction {
        TickDirection::Out => (0, outward * len),
        TickDirection::In => (0, -outward * len),
        TickDirection::Both => (-len, len),
    };
    let (low, high) = match side {
        PlotAxisSide::Left | PlotAxisSide::Right => (y0, y0 + h - 1),
        PlotAxisSide::Top | PlotAxisSide::Bottom => (x0, x0 + w - 1),
    };
    let horizontal_axis = matches!(side, PlotAxisSide::Top | PlotAxisSide::Bottom);
    // "Both" is symmetric: a negative `from` is enough whichever the side.
    let (a, b) = (line + from, line + to);
    for &p in positions.iter().filter(|&&p| p >= low && p <= high) {
        let points = if horizontal_axis {
            vec![(p, a), (p, b)]
        } else {
            vec![(a, p), (b, p)]
        };
        canvas.draw(&PathElement::new(
            points,
            ShapeStyle {
                color: rgba(color).to_rgba(),
                filled: false,
                stroke_width: width,
            },
        ))?;
    }
    Ok(())
}

/// Effective colors of an axis: those of the spec, or the automatic ink when
/// they are empty.
struct AxisColors {
    ticks: (u8, u8, u8, u8),
    /// The style of the axis title: color, font and size of its area.
    title_style: TextStyle<'static>,
}

impl AxisColors {
    fn of(axis: &AxisSpec, ink: (u8, u8, u8), figure_width: u32) -> Self {
        let title = rgba_or(&axis.title_color, opaque(ink));
        Self {
            ticks: rgba_or(&axis.tick_color, opaque(ink)),
            title_style: font_style(&axis.title_font, AXIS_DESC_SIZE, title, figure_width),
        }
    }

    /// Sets these colors on a pass of `configure_mesh`: labels and axis line
    /// in the color of the scale, grid in the same, attenuated, axis title in
    /// its own.
    fn apply<X, Y, DB>(&self, mesh: &mut plotters::chart::MeshStyle<'_, '_, X, Y, DB>)
    where
        X: plotters::coord::ranged1d::Ranged<ValueType = f64>
            + plotters::coord::ranged1d::ValueFormatter<f64>,
        Y: plotters::coord::ranged1d::Ranged<ValueType = f64>
            + plotters::coord::ranged1d::ValueFormatter<f64>,
        DB: DrawingBackend,
    {
        let ticks = rgba(self.ticks);
        mesh.label_style(text_style(LABEL_SIZE, self.ticks))
            .axis_desc_style(self.title_style.clone())
            .axis_style(ticks)
            .bold_line_style(ticks.mix(GRID_BOLD_OPACITY))
            .light_line_style(ticks.mix(GRID_LIGHT_OPACITY));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::{PanelSpec, Panels};

    fn spec_with(panels: Vec<PanelSpec>) -> PlotSpec {
        PlotSpec::empty("Grid").with_panels(Panels::new(panels))
    }

    /// "From 800 to 400" draws the axis reversed; "from 400 to 800" the right
    /// way round; the "Reverse axis" box on top does not add up.
    #[test]
    fn a_framing_from_800_to_400_draws_the_axis_reversed() {
        let mut axis = AxisSpec {
            auto_range: false,
            range_min: Some(800.0),
            range_max: Some(400.0),
            ..AxisSpec::default()
        };
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (800.0, 400.0)
        );
        axis.range_min = Some(400.0);
        axis.range_max = Some(800.0);
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (400.0, 800.0)
        );
        axis.inverted = true;
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (800.0, 400.0)
        );
        axis.range_min = Some(800.0);
        axis.range_max = Some(400.0);
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (800.0, 400.0)
        );
    }

    #[test]
    fn the_declared_grid_is_the_spec_one() {
        let spec = spec_with(vec![
            PanelSpec::at(0, 0),
            PanelSpec::at(0, 1),
            PanelSpec::at(1, 0),
        ]);
        let layout = grid_of(&spec, 3);
        assert_eq!((layout.rows, layout.columns), (2, 2));
        // Row by row order, like `split_evenly`.
        assert_eq!(layout.cells, vec![0, 1, 2]);
    }

    #[test]
    fn a_hole_in_the_grid_stays_a_hole() {
        let spec = spec_with(vec![PanelSpec::at(0, 0), PanelSpec::at(1, 2)]);
        let layout = grid_of(&spec, 2);
        assert_eq!((layout.rows, layout.columns), (2, 3));
        assert_eq!(layout.cells, vec![0, 5]);
    }

    #[test]
    fn an_oversized_position_does_not_split_into_infinity() {
        // The case `PlotSpec::rows` already documents: a position coming from
        // a file is bounded by nothing.
        let spec = spec_with(vec![PanelSpec::at(0, 0), PanelSpec::at(usize::MAX, 0)]);
        let layout = grid_of(&spec, 2);
        assert!(
            layout.rows * layout.columns <= MAX_CELLS,
            "{layout:?} cannot be split"
        );
        assert_eq!(layout.cells, vec![0, 1]);
    }

    #[test]
    fn the_baseline_stays_within_the_framing() {
        assert_eq!(scale_baseline((-5.0, 5.0)), 0.0);
        // On a logarithmic scale, the framing does not contain zero: the bars
        // start from the bottom of the axis.
        assert_eq!(scale_baseline((1.0, 3.0)), 1.0);
        // Inverted axis: the bottom is the larger of the two bounds.
        assert_eq!(scale_baseline((5.0, -5.0)), 0.0);
        assert_eq!(scale_baseline((3.0, 1.0)), 1.0);
    }

    #[test]
    fn a_tick_area_does_not_eat_the_cell() {
        assert_eq!(label_area(true, 64, 600), 64);
        assert_eq!(label_area(true, 64, 90), 30);
        assert_eq!(label_area(false, 64, 600), 0);
    }
}

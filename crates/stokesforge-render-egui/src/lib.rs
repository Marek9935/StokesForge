//! Screen rendering of a StokesForge graph — phase R4, tasks 4.2 and 4.3.
//!
//! Three pieces, in the order in which they fit together:
//!
//! - [`backend`]: the Plotters `DrawingBackend` that collects `egui::Shape`s
//!   instead of painting;
//! - [`cache`]: the `(revision, rect, pixels_per_point, view_generation)` key
//!   that lets a frame without changes skip replaying the pipeline;
//! - [`feathering`]: the antialiasing lever of task R5.5 bis — the only piece
//!   of the rendering that has no equivalent on the export side, because
//!   tessellation exists only for the screen;
//! - [`GraphView`], here: what the UI actually holds, a cache plus the link
//!   between a session `Snapshot` and a `Graph` node.
//!
//! # The point of contact with the base
//!
//! This crate knows two things about the model, both built by the previous
//! phases: `stokesforge_core::resolve` (R3), which translates a `PlotSpec`
//! into data ready to plot while handling broken references first hand, and
//! `stokesforge_session::Snapshot` (base), whose `revision` is the cache key.
//! Nothing is re-routed, nothing is copied: the columns are
//! `Arc<Vec<f64>>`s that the resolution clones by counter.
//!
//! # Measuring and drawing do not go through the same object, on purpose
//!
//! [`ScreenText`] is the screen's [`TextSource`]: it **draws** through the
//! window's `egui::Context` and **measures** through an off-screen
//! [`TextMeasurer`]. This is not a duplication of the measurement source —
//! both start from the same [`font_definitions`] — but the conjunction of
//! two constraints that cannot be satisfied by the same call:
//!
//! - a `Galley` carries the UV coordinates of its glyphs **in the atlas that
//!   laid it out**; a `Shape::Text` built on a foreign `Galley` would be
//!   tessellated against the `Context`'s atlas and show garbage. The drawing
//!   must therefore come from the `Context`;
//! - the geometry of the figure must not depend on the screen, and the
//!   `egui::Context` always lays out at the current `pixels_per_point` —
//!   on which `epaint`'s line height depends (see
//!   `stokesforge_render::text`). The measurement must therefore come from
//!   elsewhere.
//!
//! This is the case the R0 0.1 measurement had to settle, and its answer is
//! what makes this split safe: **0.000000 px of difference** over 540
//! comparisons between a hand-built `Fonts` and the `Context` path. Sharing
//! the `FontDefinitions` is enough; sharing the `Fonts` object would not have
//! been possible anyway.
//!
//! So that this is not a coincidence, the application must set
//! [`font_definitions`] on its `Context` — `stokesforge-app` does it at
//! startup.
//!
//! # Typst — task R8.3, wired in by issue #29
//!
//! [`typst_screen`] rasterizes a formula compiled by `stokesforge-render` to
//! RGBA (`typst-render`), loads it as an `egui` texture, and
//! [`EguiBackend::draw_text`]/[`estimate_text_size`] use it in place of the
//! `Shape::Text` path for any rich text — titles, axis titles, legend,
//! marked by their style ([`stokesforge_render::formula::is_rich`], issue
//! #32). A text that does not compile degrades to the plain `Shape::Text`
//! rather than making the rendering fail (see `typst_screen`, module
//! header).
//!
//! Typst is always compiled (the `typst` feature was removed, issue #29),
//! and [`GraphView`] — hence the application — always enables it: it holds
//! the texture cache ([`FormulaCache`]) and the fonts resolved once and for
//! all. It stays optional at the **backend** level: [`collect_shapes`] plots
//! everything as plain text, [`collect_shapes_with_typst`] passes formulas
//! through Typst. The first is not a leftover: it is the witness of the
//! tests that prove Typst is really consulted (a figure with a formula must
//! differ from one path to the other), and the entry point of the tests that
//! have no egui `Context` to lend.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod backend;
pub mod cache;
pub mod feathering;
pub mod scaled;
mod shape_tools;
pub mod typst_screen;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use egui::{
    Color32, Galley, Id, Painter, PointerButton, Pos2, Rect, Response, Shape, Stroke, StrokeKind,
};
use plotters::drawing::IntoDrawingArea;
use stokesforge_core::node::NodeKind;
use stokesforge_core::resolve::{resolve_spec, ResolvedPlot};
use stokesforge_core::{NodeId, Tree};
use stokesforge_plot::{FreeFrame, PlotSpec, SeriesAxis};
use stokesforge_render::formula::FormulaCache;
use stokesforge_render::interact;
use stokesforge_render::pipeline::{self};
use stokesforge_render::view::ViewState;
// Moved down into `stokesforge-render` (#255): the app's `errors.rs` reads it without egui.
pub use stokesforge_render::{FigureTexts, RenderError};
use stokesforge_session::Revision;
use stokesforge_session::Snapshot;

pub use backend::{EguiBackend, EguiBackendError};
pub use cache::{CacheKey, Outcome, ShapeCache};
pub use feathering::Feathering;
// Re-exported so that the UI does not have to declare an edge to
// `stokesforge-render` to read what the figure drew: this crate is already
// its screen rendering facade.
pub use stokesforge_render::pipeline::{
    DrawnAnnotation, DrawnLegendRow, DrawnPanel, DrawnPlot, DrawnText, FigureLayout,
    AXIS_DESC_SIZE, DEFAULT_FIGURE_SIZE, LABEL_SIZE, PANEL_TITLE_SIZE, TITLE_SIZE,
};
// The address and model of a free text: the application handles them with
// the signals of the view ([`GraphEvent`]).
pub use stokesforge_plot::{
    AnnotationRef, AnnotationSpec, ShapeKind, ShapeRef, ShapeSpec, TextAnchor, TextTarget,
};
pub use stokesforge_render::text::{
    font_definitions, font_definitions_for, TextMeasurer, TextSource,
};
// The font choice (embed of 2026-09-13, `stokesforge_render::fonts`): a UI
// that wants to offer Fira/Libertinus/Noto or a system font does not have to
// declare its own edge to `stokesforge-render` for that either.
pub use stokesforge_render::{
    available_families, FamilyEntry, FontChoice, FontSet, FontSource, ResolvedFonts,
};
// The pixel ↔ data correspondence (task R7): `stokesforge-render` carries it
// (see `stokesforge_render::interact`, generic over the backend), this crate
// only consumes it with egui types.
pub use stokesforge_render::{
    component_at_with_tolerance, nearest_on_curve, AxisSide, Gesture, PanelTransform,
    PlotComponent, AXIS_LINE_TOLERANCE_POINTS,
};
pub use typst_screen::{formula_label, FormulaTexture, TypstScreen};
// Typst on the text model side (compilation, escaping): the interface needs
// it to check what it offers to write (property syntax memo), without
// declaring an edge to `stokesforge-render`.
pub use stokesforge_render::formula;

/// What the view signals to the application, to be translated into actions —
/// free texts (issue #35). Collected by [`GraphView::take_events`].
#[derive(Debug, Clone, PartialEq)]
pub enum GraphEvent {
    /// Click of the Text tool (issue #62): place a free text at the targeted
    /// data point and open its editor. The tool goes back to the pointer.
    PlaceText {
        /// Index of the layer in `PlotSpec::panels`.
        panel: usize,
        /// Data abscissa.
        x: f64,
        /// Data ordinate.
        y: f64,
    },
    /// Double click on a free text: edit it in place.
    EditAnnotation(AnnotationRef),
    /// A free text was dragged then released: its new version, to commit as
    /// **one** change of the model. The preview stays displayed until the
    /// model changes ([`GraphView::hold_preview`]).
    AnnotationMoved {
        /// The moved text.
        target: AnnotationRef,
        /// Its moved version.
        annotation: AnnotationSpec,
    },
    /// A graphic object was drawn, moved or resized (issue #76): its new
    /// version, to commit as **one** change of the model (`target.index`
    /// equal to the number of objects of the panel: an addition). The preview
    /// stays displayed until the model changes ([`GraphView::hold_preview`]).
    ShapeChanged {
        /// The object concerned.
        target: ShapeRef,
        /// Its new version.
        shape: ShapeSpec,
    },
    /// A resize handle on the outer boundary of a cell was pulled then released
    /// (#394): the new frame (plot area) size, in points, of a grid column
    /// and/or row, to commit as **one** change of the model. The preview stays
    /// displayed until the model changes ([`GraphView::hold_preview`]).
    FrameResized {
        /// Grid column and its new frame width, for a vertical boundary.
        column: Option<(usize, f64)>,
        /// Grid row and its new frame height, for a horizontal boundary.
        row: Option<(usize, f64)>,
    },
    /// The left or top edge of the figure was pulled then released (#405): the
    /// new distance in points from the edge of the image to the first frame
    /// (Left / Top), to commit as **one** change of the model. The preview stays
    /// displayed until the model changes ([`GraphView::hold_preview`]).
    FramePlaced {
        /// New Left, for the left edge.
        left: Option<f64>,
        /// New Top, for the top edge.
        top: Option<f64>,
    },
    /// A layer of a free figure was moved or resized with the mouse, then the
    /// button was released (#398): its new frame, to commit as **one** change
    /// of the model. Nothing is signalled during the gesture, nor for a click
    /// without movement. The preview stays displayed until the model changes
    /// ([`GraphView::hold_preview`]).
    LayerFramed {
        /// Base layer of the cell in `PlotSpec::panels`.
        panel: usize,
        /// The new frame in stored page coordinates, in points (the stacking
        /// key is the one it had).
        frame: FreeFrame,
        /// The size changed (an edge was pulled), else the layer only moved.
        resized: bool,
    },
    /// Double click on a title: edit it in place (issue #53).
    EditText(TextTarget),
    /// Double click on a legend entry: rename the series of layer `panel`
    /// (issue #53).
    EditLegendEntry {
        /// Index of the layer in `PlotSpec::panels`.
        panel: usize,
        /// Rank of the series in `PanelSpec::series`.
        series: usize,
    },
    /// Double click on an axis (line or ticks): open the settings of this
    /// axis (issue #113).
    OpenAxisProperties {
        /// Index of the panel in the drawn figure.
        panel: usize,
        /// Side of the axis.
        side: stokesforge_render::AxisSide,
    },
    /// Double click on a curve: open the settings of the series
    /// (issue #113).
    OpenSeriesProperties(stokesforge_plot::SeriesRef),
    /// Double click on a graphic object: open its properties
    /// (issue #113).
    OpenShapeProperties(stokesforge_plot::ShapeRef),
    /// A title or a legend was dragged then released: its new anchor, to
    /// commit as **one** change of the model. The preview stays displayed
    /// until the model changes ([`GraphView::hold_preview`]).
    TextMoved {
        /// The moved text.
        target: TextTarget,
        /// Its new anchor.
        anchor: TextAnchor,
    },
    /// A width handle of a title or an axis title was pulled then released
    /// (#244): its new width, as a fraction of the width of the figure, to
    /// commit as **one** change of the model.
    TextResized {
        /// The resized text.
        target: TextTarget,
        /// Its width, bounded to `[MIN_WIDTH, 1]`.
        width: f64,
    },
}

/// The active tool of a graph (issue #62), Origin style. A **view** setting,
/// like the framing: never in the model, never undoable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GraphTool {
    /// The default: a plain drag only moves objects (texts, shapes, title,
    /// legend); Ctrl + drag or middle click + drag scrolls the view (a single
    /// scale if the gesture starts from an axis, #318), Ctrl+wheel zooms.
    #[default]
    Pointer,
    /// Dragging draws a zoom rectangle.
    ZoomBox,
    /// A click in the plot area places a free text.
    Text,
    /// Dragging draws a rectangle (issue #76).
    Rectangle,
    /// Dragging draws an ellipse (issue #76).
    Ellipse,
    /// Dragging draws a line (issue #76).
    Line,
    /// Dragging draws an arrow, tip at release (issue #76).
    Arrow,
}

impl GraphTool {
    /// The kind of object the tool draws, if it draws one.
    pub fn shape_kind(self) -> Option<ShapeKind> {
        match self {
            Self::Rectangle => Some(ShapeKind::Rectangle),
            Self::Ellipse => Some(ShapeKind::Ellipse),
            Self::Line => Some(ShapeKind::Line),
            Self::Arrow => Some(ShapeKind::Arrow),
            Self::Pointer | Self::ZoomBox | Self::Text => None,
        }
    }
}

/// The frame sizes a cell handle is being pulled to (#394): applied to the
/// clone of the spec when the figure is laid out and drawn, never to the model.
/// `base` is the frames the figure was drawn with when the gesture began, so
/// that a legacy graph (no stored frame) previews exactly what the committing
/// action will fix.
#[derive(Debug, Clone, PartialEq)]
struct FramePreview {
    column: Option<(usize, f64)>,
    row: Option<(usize, f64)>,
    /// Left and Top the figure edges are being pulled to (#405).
    left: Option<f64>,
    top: Option<f64>,
    base: (Vec<f64>, Vec<f64>),
    /// The frame a free layer is being moved or resized to (#398): the base
    /// layer of its cell and the frame.
    free: Option<(usize, FreeFrame)>,
}

impl FramePreview {
    fn apply(&self, spec: &mut PlotSpec) {
        if let Some((base, frame)) = self.free {
            if let Some(panel) = spec.panels.get_mut(base) {
                panel.free_frame = Some(frame);
            }
            return;
        }
        let bounds = |size: f64| {
            size.clamp(
                stokesforge_plot::MIN_FRAME_SIZE,
                stokesforge_plot::MAX_FRAME_SIZE,
            )
        };
        let mut widths = spec
            .sanitized_frame_widths()
            .unwrap_or_else(|| self.base.0.clone());
        let mut heights = spec
            .sanitized_frame_heights()
            .unwrap_or_else(|| self.base.1.clone());
        if let Some((index, size)) = self.column {
            if let Some(slot) = widths.get_mut(index) {
                *slot = bounds(size);
            }
        }
        if let Some((index, size)) = self.row {
            if let Some(slot) = heights.get_mut(index) {
                *slot = bounds(size);
            }
        }
        spec.frame_widths = widths;
        spec.frame_heights = heights;
        if let Some(left) = self.left {
            spec.figure_left = Some(left);
        }
        if let Some(top) = self.top {
            spec.figure_top = Some(top);
        }
    }
}

/// True if the previewed size differs from the starting one by half a point
/// or more (the preview is rounded to whole points).
fn moved(preview: Option<(usize, f64)>, start: Option<(usize, f64)>) -> bool {
    match (preview, start) {
        (Some((_, now)), Some((_, before))) => (now - before).abs() >= 0.5,
        _ => false,
    }
}

/// True if a previewed Left or Top differs from the starting one by half a
/// point or more (#405).
fn moved_value(preview: Option<f64>, start: Option<f64>) -> bool {
    match (preview, start) {
        (Some(now), Some(before)) => (now - before).abs() >= 0.5,
        _ => false,
    }
}

/// How far outside the figure, in screen points, its edge handles can be
/// grabbed (#405): the strip straddles the edge. The host must leave at least
/// that much room around the figure and let its clip rectangle reach it.
pub const FIGURE_GRAB_MARGIN: f32 = stokesforge_render::CELL_EDGE_TOLERANCE_POINTS as f32;

/// A cell-handle drag in progress (#394).
#[derive(Debug, Clone)]
struct FrameDrag {
    /// Column and frame width at the start of the gesture.
    column: Option<(usize, f64)>,
    /// Row and frame height at the start of the gesture.
    row: Option<(usize, f64)>,
    /// Left and Top at the start of the gesture, when the left or top edge of
    /// the figure is pulled (#405).
    left: Option<f64>,
    top: Option<f64>,
    /// Press point, in screen coordinates, and the display scale at that
    /// moment: the distance is measured in screen points and divided by that
    /// scale, because the figure (and the window that follows it) grows under
    /// the pointer while the gesture goes on, so the live mapping of the
    /// position would be a frame behind.
    press: Pos2,
    scale: f32,
    /// The frames the figure was drawn with at the start.
    base: (Vec<f64>, Vec<f64>),
}

/// Which layer gesture a call of [`GraphView::handle_free_drag`] may start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FreeStart {
    /// A resize from an edge or a corner.
    Edges,
    /// A move from the plot area.
    Move,
}

/// A move or resize of a layer of a free figure in progress (#398).
#[derive(Debug, Clone)]
struct FreeDrag {
    /// What the press grabbed.
    handle: stokesforge_render::FreeHandle,
    /// The frame at the start of the gesture.
    start: FreeFrame,
    /// The frames of the other layers (the magnets' targets).
    others: Vec<FreeFrame>,
    /// Press point in screen coordinates and the display scale at that moment
    /// (frozen, as for [`FrameDrag`]: the figure grows under the pointer).
    press: Pos2,
    scale: f32,
    /// The dashed guides of the magnets, painted over the figure.
    guides: Vec<stokesforge_render::Guide>,
}

/// A title or legend drag in progress (issue #53).
#[derive(Debug, Clone)]
struct TextDrag {
    target: TextTarget,
    /// The anchor at the start of the gesture (`None`: the text was at its
    /// default place).
    original: Option<TextAnchor>,
    /// Drawn center of the text at the start of the gesture, in figure coordinates.
    start_center: Pos2,
    /// Press point of the gesture, in figure coordinates.
    press: Pos2,
    /// The gesture pulls a width handle (#244) instead of moving.
    resize: bool,
}

/// A free text drag in progress.
#[derive(Debug, Clone)]
struct AnnotationDrag {
    target: AnnotationRef,
    original: AnnotationSpec,
    /// Drawn center of the text at the start of the gesture, in figure coordinates.
    start_center: Pos2,
    /// Press point of the gesture, in figure coordinates.
    press: Pos2,
    /// The gesture pulls a width handle (#244) instead of moving.
    resize: bool,
}

/// Paints the two width handles of a selected text (#244).
fn paint_width_handles(painter: &Painter, handles: [Pos2; 2]) {
    for handle in handles {
        let square = Rect::from_center_size(handle, egui::vec2(7.0, 7.0));
        painter.rect_filled(square, 1.0, Color32::WHITE);
        painter.rect_stroke(
            square,
            1.0,
            Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
            StrokeKind::Inside,
        );
    }
}

/// Maximum distance, in egui points, at which a width handle is grabbed.
const WIDTH_HANDLE_RADIUS: f32 = 7.0;

/// Maximum distance, in pixels, at which a point is still "designated" by
/// the cursor (task R7.2). Beyond that, nobody is under the mouse.
pub const HIT_RADIUS: f32 = 12.0;

/// The screen's text source: drawing through the `Context`, off-screen
/// measurement.
///
/// Both halves start from the same [`font_definitions`]; the reason for
/// separating them is in the module header.
pub struct ScreenText<'a> {
    /// The context that owns the glyph atlas into which the text will be
    /// tessellated.
    pub context: &'a egui::Context,
    /// The measurement source, independent of the screen.
    pub measurer: &'a TextMeasurer,
}

impl<'a> ScreenText<'a> {
    /// Assembles the two halves.
    pub fn new(context: &'a egui::Context, measurer: &'a TextMeasurer) -> Self {
        Self { context, measurer }
    }
}

impl TextSource for ScreenText<'_> {
    fn measure(&self, text: &str, font_id: egui::FontId) -> egui::Vec2 {
        self.measurer.measure(text, font_id)
    }

    fn layout(&self, text: &str, font_id: egui::FontId, color: Color32) -> Arc<Galley> {
        self.context
            .fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font_id, color))
    }
}

/// Builds the shapes of a resolved graph into `shapes`.
///
/// The low-level entry point: no `Context`, no cache, no session. This is
/// what the tests call — the rendering is thus checked **without a graphics
/// server**, which is the argument of §9 of `roadmap-rendu.md` for building
/// the renderer before the UI.
pub fn collect_shapes(
    shapes: &mut Vec<Shape>,
    rect: Rect,
    text: &dyn TextSource,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, RenderError> {
    collect_shapes_with_texts(shapes, rect, text, plot, &FigureTexts::default())
}

/// Like [`collect_shapes`], with the texts of the figure in the caller's
/// language ([`FigureTexts`]); the export must be given the same ones.
pub fn collect_shapes_with_texts(
    shapes: &mut Vec<Shape>,
    rect: Rect,
    text: &dyn TextSource,
    plot: &ResolvedPlot<'_>,
    texts: &FigureTexts,
) -> Result<DrawnPlot, RenderError> {
    let backend = EguiBackend::in_rect(shapes, rect, text);
    pipeline::draw_with_texts(backend, plot, texts)
        .map_err(|error| RenderError::Drawing(error.to_string()))
}

/// Swatch shared with the legend, for the Object Manager (#82).
pub fn series_swatch(tree: &Tree, spec: &stokesforge_plot::SeriesSpec, rect: Rect) -> Vec<Shape> {
    use plotters::drawing::IntoDrawingArea;
    // Check the references through their metadata, without converting mixed
    // cells or allocating the series geometry for a thumbnail.
    let references = [
        Some(spec.y),
        spec.x.filter(|_| spec.plot_type.needs_x()),
        spec.x_error,
        spec.y_error,
    ];
    let broken = references
        .into_iter()
        .flatten()
        .any(|id| stokesforge_core::resolve::resolve_column(tree, id).is_none());
    let bars = !broken
        && matches!(
            spec.plot_type,
            stokesforge_plot::PlotType::Bar | stokesforge_plot::PlotType::Histogram
        );
    let mut shapes = Vec::new();
    static TEXT: std::sync::OnceLock<TextMeasurer> = std::sync::OnceLock::new();
    let text = TEXT.get_or_init(TextMeasurer::default);
    let area = EguiBackend::in_rect(&mut shapes, rect, text).into_drawing_area();
    let _ = stokesforge_render::legend::series_key(
        &area,
        spec,
        broken,
        bars,
        (0, rect.height() as i32 / 2),
        stokesforge_render::paint::INK,
    );
    drop(area);
    shapes
}

/// Curve of the analysis preview, already projected into local pixels. This
/// bridge interprets no style: strokes, dashes and symbols are those of the
/// shared rendering, as in the graph and its exports (#210).
pub fn analysis_curve(
    spec: &stokesforge_plot::SeriesSpec,
    rect: Rect,
    points: &[(f64, f64)],
) -> Vec<Shape> {
    use plotters::drawing::IntoDrawingArea;
    let mut shapes = Vec::new();
    static TEXT: std::sync::OnceLock<TextMeasurer> = std::sync::OnceLock::new();
    let text = TEXT.get_or_init(TextMeasurer::default);
    let area = EguiBackend::in_rect(&mut shapes, rect, text).into_drawing_area();
    let mut line = spec.line.clone();
    if matches!(
        spec.plot_type,
        stokesforge_plot::PlotType::Bar | stokesforge_plot::PlotType::Histogram
    ) && line.style == stokesforge_plot::LineStyle::None
    {
        line.style = stokesforge_plot::LineStyle::Solid;
    }
    for run in points.split(|(x, y)| !x.is_finite() || !y.is_finite()) {
        // Bars and histograms: the analysis works on the raw Y measurements,
        // the preview links them while keeping the style of the source stroke.
        if spec.plot_type.draws_line() || !spec.plot_type.draws_symbols() {
            let _ = stokesforge_render::paint::polyline_f64(&area, run, &line, false);
        }
        if spec.plot_type.draws_symbols() {
            let pixels: Vec<_> = run
                .iter()
                .map(|(x, y)| (x.round() as i32, y.round() as i32))
                .collect();
            let _ = stokesforge_render::paint::symbols(&area, &pixels, &spec.symbol, false);
        }
    }
    drop(area);
    shapes
}

/// Like [`collect_shapes`], with the Typst path of task R8.3 enabled.
///
/// Equivalent low-level entry point, to test the Typst screen path without
/// going through a complete [`GraphView`] or a session.
pub fn collect_shapes_with_typst(
    shapes: &mut Vec<Shape>,
    rect: Rect,
    text: &dyn TextSource,
    typst: typst_screen::TypstScreen<'_>,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, RenderError> {
    collect_shapes_with_typst_and_texts(shapes, rect, text, typst, plot, &FigureTexts::default())
}

/// Like [`collect_shapes_with_typst`], with the texts of the figure
/// ([`FigureTexts`]).
pub fn collect_shapes_with_typst_and_texts(
    shapes: &mut Vec<Shape>,
    rect: Rect,
    text: &dyn TextSource,
    typst: typst_screen::TypstScreen<'_>,
    plot: &ResolvedPlot<'_>,
    texts: &FigureTexts,
) -> Result<DrawnPlot, RenderError> {
    let backend = EguiBackend::in_rect(shapes, rect, text).with_typst(typst);
    pipeline::draw_with_texts(backend, plot, texts)
        .map_err(|error| RenderError::Drawing(error.to_string()))
}

/// A point designated on a curve — task R7.2.
///
/// **Limited to the main axis** (bottom, left): a series of the secondary
/// axis would live on a second `PanelTransform` (bounds `y2_range`, scale
/// `right.scale`) that would have to be built alongside, and nothing today
/// has a use for it (`stokesforge-app` still shows only a test bench, without a
/// visible secondary axis). An accepted gap, not a silent omission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointHit {
    /// Index of the designated panel in [`DrawnPlot::panels`] — thus among
    /// the panels actually drawn (R5.4), not in the original spec.
    pub panel: usize,
    /// Index of the series in `PanelSpec::series`, resolved in the same
    /// order (`ResolvedPanel::series`).
    pub series: usize,
    /// Index of the designated point in its series.
    pub point_index: usize,
    /// Position of the point, in data.
    pub data: (f64, f64),
    /// Position of the point, on screen.
    pub pixel: Pos2,
    /// Distance from the cursor to the point, in pixels.
    pub distance: f32,
}

/// Key of the memoized figure layout: graph, revision, pixels per point (bits),
/// preview generation.
type FigureMemoKey = (NodeId, Revision, u32, u64);

/// A displayed graph: its cache, its view state, and what it takes to repaint it.
///
/// One `GraphView` per view, not one per graph: two windows on the same
/// graph each have their own cache and, since R7, their own zoom/pan
/// framing, which is the expected behavior (zooming in one window must not
/// zoom the other).
#[derive(Debug)]
pub struct GraphView {
    cache: ShapeCache,
    /// The Typst formulas already rasterized (issue #29): lives as long as
    /// the view, so that a rebuild of the figure (zoom, revision) does not
    /// recompile unchanged titles. Bounded by [`FORMULA_CACHE_LIMIT`].
    formulas: FormulaCache<FormulaTexture>,
    /// The font choice of the formulas — the default value, like the view's
    /// [`TextMeasurer`].
    font_choice: FontChoice,
    /// The Typst fonts (text, Math) of `font_choice`, resolved once when the
    /// view is created. `None` if the resolution failed — then all the text
    /// stays plain, without making the rendering fail (invariant 10).
    typst_fonts: Option<(typst::text::Font, typst::text::Font)>,
    /// The cached shapes scaled to the display, text (issue #18) and Typst
    /// formulas (issue #31) included: see [`scaled`].
    scaled: scaled::ScaledShapes,
    last: Option<DrawnPlot>,
    /// The memoized [`GraphView::figure_layout`], with its key.
    figure_memo: Option<(FigureMemoKey, FigureLayout)>,
    measurer: TextMeasurer,
    view: ViewState,
    id: Id,
    hover: Option<PointHit>,
    /// Visible rectangle of the last rendering; the cache keeps the reference
    /// logical frame when the application enlarges or reduces the figure.
    display_rect: Option<Rect>,
    /// Corners of the rubber-band zoom rectangle in progress (task 7.4), in
    /// screen pixels. `None` outside a drag of the Zoom tool.
    drag_box: Option<(Pos2, Pos2)>,
    /// The active tool (issue #62).
    tool: GraphTool,
    /// Interaction response of the figure at the last rendering: the
    /// application hooks its context menu (right click) on it.
    response: Option<egui::Response>,
    /// Preview of a free text being edited or dragged (issue #35): set on the
    /// clone of the spec when the figure is built, like the zoom — never in
    /// the model until the application has committed.
    annotation_preview: Option<(AnnotationRef, AnnotationSpec)>,
    /// Counter of the preview, added to the view generation in the cache
    /// key.
    preview_generation: u64,
    /// The texts the figure draws by itself, in the application's language.
    figure_texts: FigureTexts,
    /// Revision of the model at which the preview was committed: it clears as
    /// soon as the model has changed, so as not to show an image of the old
    /// position between the commit and the update of the model.
    preview_until: Option<Revision>,
    annotation_drag: Option<AnnotationDrag>,
    /// Preview of a title or legend being dragged (issue #53), on the same
    /// principle as `annotation_preview`.
    text_preview: Option<(TextTarget, TextAnchor)>,
    /// Preview of the width of a title being resized (#244).
    text_width_preview: Option<(TextTarget, f64)>,
    text_drag: Option<TextDrag>,
    /// Preview of the frame sizes of a cell handle being pulled (#394).
    frame_preview: Option<FramePreview>,
    frame_drag: Option<FrameDrag>,
    /// Move or resize of a layer of a free figure in progress (#398).
    free_drag: Option<FreeDrag>,
    /// Selected free text (click), target of Delete.
    selected_annotation: Option<AnnotationRef>,
    /// The selected title, axis title or legend (issue #77).
    selected_text: Option<TextTarget>,
    /// Preview of a graphic object being drawn, moved or resized (issue #76),
    /// on the same principle as `annotation_preview`.
    shape_preview: Option<(ShapeRef, ShapeSpec)>,
    shape_drag: Option<shape_tools::ShapeDrag>,
    /// Selected graphic object (click), target of Delete and the handles.
    selected_shape: Option<ShapeRef>,
    /// Selected series, shared by the graph and the Object Manager (#83).
    selected_series: Option<stokesforge_plot::SeriesRef>,
    /// The selected series group (#197), with the series that designates it:
    /// it only holds as long as `selected_series` is that series, so a click
    /// on another curve lifts it with nothing to clean up elsewhere.
    selected_group: Option<(stokesforge_plot::SeriesRef, u32)>,
    /// Where the navigation drag in progress started: decided at the first
    /// frame of the gesture, so that the cursor can leave it without changing
    /// target.
    nav_drag: Option<NavDrag>,
    /// Active layer (#161): the last selected layer, default target of
    /// additions. Interface state, outside the model; never drawn in the
    /// figure. Read by [`Self::active_panel`], which revalidates it.
    active_panel: usize,
    /// The other layers selected with Shift + click in free placement (#398),
    /// as base layer indices in click order; the active layer is the
    /// reference of the selection and is never listed. Interface state like
    /// [`Self::active_panel`]; read through [`Self::extra_layers`].
    extra_layers: Vec<usize>,
    /// Selection seen at the previous frame: the active layer follows a
    /// change of selection, not the selection itself (a click on a layer
    /// marker therefore stays valid as long as nothing else is selected).
    seen_selection: SelectionKey,
    events: Vec<GraphEvent>,
}

/// What the drag in progress does, decided at the press (like `DragMode` of
/// the analysis preview): the whole plot area, a single scale (drag started
/// from an axis, #167), or no pan. The pan requires Ctrl or the middle button
/// at the press (#318); releasing Ctrl on the way does not interrupt it,
/// pressing it on the way does not trigger it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum NavDrag {
    Plot,
    Axis {
        panel: usize,
        side: AxisSide,
    },
    /// No pan: plain drag (tool, object or nothing).
    Inert,
}

impl NavDrag {
    fn is_pan(self) -> bool {
        !matches!(self, NavDrag::Inert)
    }
}

/// The four selections of a view, to detect that they changed.
type SelectionKey = (
    Option<stokesforge_plot::SeriesRef>,
    Option<AnnotationRef>,
    Option<ShapeRef>,
    Option<TextTarget>,
);

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}

/// Counter to give each `GraphView` its own `egui::Id` for interaction,
/// independent of the `Ui` path where it is painted.
///
/// Without it, two views painted from the same place in the code (a loop
/// over several graphs, for example) would share the same `Id` and therefore
/// the same drag gesture — the `Id` derived from `ui.id()` alone would not
/// tell them apart.
static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(0);

/// Number of formulas beyond which [`GraphView`] empties its texture cache
/// before rebuilding the figure.
///
/// Each keystroke in a formula title creates an entry (and a texture) that
/// will not be used again. A real figure has a few dozen texts at most — a
/// title, three axis titles, twenty-four legend entries
/// (`legend::MAX_ENTRIES`) —, so the limit only triggers after a long edit,
/// and then costs a recompilation of what is displayed (0.7 ms per formula).
pub const FORMULA_CACHE_LIMIT: usize = 256;

impl GraphView {
    /// A new view, with nothing in the cache and no zoom.
    ///
    /// Builds its own measurement source: this is the only notable cost of
    /// the view (decoding the embedded fonts), paid once when a graph window
    /// is opened.
    pub fn new() -> Self {
        let font_choice = FontChoice::default();
        Self {
            cache: ShapeCache::new(),
            formulas: FormulaCache::new(),
            typst_fonts: stokesforge_render::formula::resolve_fonts(&font_choice).ok(),
            font_choice,
            scaled: scaled::ScaledShapes::default(),
            last: None,
            figure_memo: None,
            measurer: TextMeasurer::default(),
            view: ViewState::new(),
            id: Id::new((
                "stokesforge_render_egui::GraphView",
                NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed),
            )),
            hover: None,
            display_rect: None,
            drag_box: None,
            tool: GraphTool::default(),
            response: None,
            annotation_preview: None,
            preview_generation: 0,
            figure_texts: FigureTexts::default(),
            preview_until: None,
            annotation_drag: None,
            text_preview: None,
            text_width_preview: None,
            text_drag: None,
            frame_preview: None,
            frame_drag: None,
            free_drag: None,
            selected_annotation: None,
            selected_text: None,
            shape_preview: None,
            shape_drag: None,
            selected_shape: None,
            selected_series: None,
            selected_group: None,
            nav_drag: None,
            active_panel: 0,
            extra_layers: Vec::new(),
            seen_selection: (None, None, None, None),
            events: Vec::new(),
        }
    }

    /// The active layer of `spec` (#161): the last selected one, or the
    /// first if it has disappeared. Always a valid index as long as the graph
    /// has a layer.
    pub fn active_panel(&self, spec: &PlotSpec) -> usize {
        if self.active_panel < spec.panels.len() {
            self.active_panel
        } else {
            0
        }
    }

    /// Makes layer `panel` active (no effect if it does not exist). Numbered
    /// marker of the window, layer node in the Object Manager.
    pub fn set_active_panel(&mut self, panel: usize, spec: &PlotSpec) {
        if panel < spec.panels.len() {
            self.active_panel = panel;
        }
    }

    /// The layers selected besides the active one (#398): base layers in click
    /// order, stale indices and the active layer's own cell left out. Only in
    /// free placement; empty on the grid.
    pub fn extra_layers(&self, spec: &PlotSpec) -> Vec<usize> {
        if spec.layer_placement != stokesforge_plot::LayerPlacement::Free {
            return Vec::new();
        }
        let bases = spec.cell_bases();
        let active = bases.get(self.active_panel(spec)).copied();
        let mut kept: Vec<usize> = Vec::new();
        for &panel in &self.extra_layers {
            if let Some(&base) = bases.get(panel) {
                if Some(base) != active && !kept.contains(&base) {
                    kept.push(base);
                }
            }
        }
        kept
    }

    /// Shift + click on a layer (#398): adds its cell to the selection, or
    /// removes it when already selected. The active layer stays the reference;
    /// clicking it, or any layer on the grid, does nothing.
    pub fn toggle_extra_layer(&mut self, panel: usize, spec: &PlotSpec) {
        if spec.layer_placement != stokesforge_plot::LayerPlacement::Free {
            return;
        }
        let bases = spec.cell_bases();
        let Some(&base) = bases.get(panel) else {
            return;
        };
        if bases.get(self.active_panel(spec)) == Some(&base) {
            return;
        }
        let mut current = self.extra_layers(spec);
        if let Some(at) = current.iter().position(|&b| b == base) {
            current.remove(at);
        } else {
            current.push(base);
        }
        self.extra_layers = current;
    }

    /// Drops the selection of other layers (a plain click on a layer).
    pub fn clear_extra_layers(&mut self) {
        self.extra_layers.clear();
    }

    /// Follows a change of the layers' indices (#357: a cell or a layer was
    /// deleted, or its deletion undone): `map[old]` is the new index of layer
    /// `old`, `None` if it is gone. The framings and the active layer move with
    /// their layer (the first one if the active layer is gone); the selection and
    /// the gestures in progress, which address layers by index, are dropped.
    pub fn remap_layers(&mut self, map: &[Option<usize>]) {
        self.view.remap_panels(map);
        self.active_panel = map.get(self.active_panel).copied().flatten().unwrap_or(0);
        self.extra_layers.clear();
        self.selected_annotation = None;
        self.selected_shape = None;
        self.selected_series = None;
        self.selected_group = None;
        self.selected_text = None;
        self.seen_selection = (None, None, None, None);
        self.nav_drag = None;
        self.drag_box = None;
        self.hover = None;
        self.annotation_drag = None;
        self.annotation_preview = None;
        self.text_drag = None;
        self.text_preview = None;
        self.frame_drag = None;
        self.free_drag = None;
        self.frame_preview = None;
        self.shape_drag = None;
        self.shape_preview = None;
    }

    /// The layer of the selected object, if there is one.
    fn selection_panel(&self, spec: &PlotSpec) -> Option<usize> {
        if let Some(series) = self.selected_series {
            return Some(series.panel);
        }
        let position = self
            .selected_annotation
            .map(|t| t.panel)
            .or_else(|| self.selected_shape.map(|t| t.panel))
            .or_else(|| self.selected_text.and_then(|t| t.panel_index_of()))?;
        (position < spec.panels.len()).then_some(position)
    }

    /// The active layer follows the selection when it changes.
    fn follow_selection(&mut self, spec: &PlotSpec) {
        let key = (
            self.selected_series,
            self.selected_annotation,
            self.selected_shape,
            self.selected_text,
        );
        if key != self.seen_selection {
            self.seen_selection = key;
            if let Some(panel) = self.selection_panel(spec) {
                self.active_panel = panel;
            }
        }
    }

    fn paint_series_selection(&self, painter: &egui::Painter, spec: &PlotSpec) {
        let Some(selected) = self.selected_series else {
            return;
        };
        // A selected group (#197) shows the handles of all its members.
        let members = self.selected_group().and_then(|group| {
            spec.panels
                .get(group.panel)
                .map(|panel| panel.group_members(group.group))
        });
        let targets = match members {
            Some(members) => members
                .into_iter()
                .map(|index| stokesforge_plot::SeriesRef::new(selected.panel, index))
                .collect(),
            None => vec![selected],
        };
        for target in targets {
            self.paint_series_handles(painter, spec, target);
        }
    }

    /// Centers, in display pixels from the top-left corner of the figure, of
    /// the selection handles of `target` (#98): placed by
    /// `stokesforge_render::series_selection`, in display pixels so that their
    /// spacing does not follow the enlargement of the figure. A handle where a
    /// layer in front of the curve's cell covers it is left out (#398). Empty
    /// for a series that was not drawn.
    pub fn series_handle_centers(&self, target: stokesforge_plot::SeriesRef) -> Vec<(f64, f64)> {
        let Some(plot) = self.last.as_ref() else {
            return Vec::new();
        };
        let Some(panel) = plot.panels.iter().find(|p| p.index == target.panel) else {
            return Vec::new();
        };
        let Some(series) = panel
            .series_geometry
            .iter()
            .find(|s| s.index == target.index)
        else {
            return Vec::new();
        };
        let Some(scale) = self.display_scale() else {
            return Vec::new();
        };
        let scale = f64::from(scale);
        series
            .handle_centers(|(x, y)| (x * scale, y * scale))
            .into_iter()
            .filter(|&(x, y)| {
                !stokesforge_render::hit::hidden_by_front(
                    plot,
                    panel.cell_rank,
                    x / scale,
                    y / scale,
                )
            })
            .collect()
    }

    fn paint_series_handles(
        &self,
        painter: &egui::Painter,
        spec: &PlotSpec,
        target: stokesforge_plot::SeriesRef,
    ) {
        if target.panel >= spec.panels.len() {
            return;
        }
        let Some(rect) = self.display_rect else {
            return;
        };
        // Painted over the cached figure, never inside it: selecting rebuilds
        // nothing, and the export knows nothing about them.
        let centers = self.series_handle_centers(target);
        let mut shapes = Vec::with_capacity(centers.len());
        let size = (rect.width().max(1.0) as u32, rect.height().max(1.0) as u32);
        let mut backend = EguiBackend::new(&mut shapes, rect.min, size, &self.measurer);
        // `draw_rect` of this backend never fails (`EguiBackendError` is
        // empty, and a square does not go through fonts).
        let _ = stokesforge_render::series_selection::paint_handles(
            &mut backend,
            &centers,
            stokesforge_render::pipeline::figure_ink(spec),
        );
        painter.extend(shapes);
    }

    /// The cache, to inspect it (counters of task R4.3).
    pub fn cache(&self) -> &ShapeCache {
        &self.cache
    }

    /// Number of Typst formulas kept in the cache by this view — for the
    /// tests (issue #29).
    pub fn formula_count(&self) -> usize {
        self.formulas.len()
    }

    /// The geometry of the last figure built.
    pub fn last_drawn(&self) -> Option<&DrawnPlot> {
        self.last.as_ref()
    }

    /// Rectangle actually occupied on screen at the last rendering.
    pub fn display_rect(&self) -> Option<Rect> {
        self.display_rect
    }

    /// The view state (zoom/pan) of this view — task R7.1.
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// The interaction identifier of this view — specific to the view, stable,
    /// something to hang an interface state on (the in-place editor of a free
    /// text, `stokesforge-app`).
    pub fn id(&self) -> Id {
        self.id
    }

    /// The signals of the view since the last collection (issue #35).
    pub fn take_events(&mut self) -> Vec<GraphEvent> {
        std::mem::take(&mut self.events)
    }

    /// The selected series, visible or hidden (#83).
    pub fn selected_series(&self) -> Option<stokesforge_plot::SeriesRef> {
        self.selected_series
    }

    /// Interface selection: it changes neither the model nor its history.
    pub fn select_series(&mut self, target: Option<stokesforge_plot::SeriesRef>, spec: &PlotSpec) {
        self.selected_group = None;
        self.selected_series = target.filter(|target| spec.series_at(*target).is_some());
        if self.selected_series.is_some() {
            self.selected_annotation = None;
            self.selected_shape = None;
            self.selected_text = None;
            self.tool = GraphTool::Pointer;
        }
    }

    /// The selected series group (#197), if there is one.
    pub fn selected_group(&self) -> Option<stokesforge_plot::SeriesGroupRef> {
        let (series, group) = self.selected_group?;
        (self.selected_series == Some(series))
            .then(|| stokesforge_plot::SeriesGroupRef::new(series.panel, group))
    }

    /// Selects a series group from the Object Manager (#197): the series that
    /// designates it is the first of its members.
    pub fn select_series_group(
        &mut self,
        target: Option<stokesforge_plot::SeriesGroupRef>,
        spec: &PlotSpec,
    ) {
        let first = target.and_then(|group| {
            let index = *spec
                .panels
                .get(group.panel)?
                .group_members(group.group)
                .first()?;
            Some((
                stokesforge_plot::SeriesRef::new(group.panel, index),
                group.group,
            ))
        });
        self.select_series(first.map(|(series, _)| series), spec);
        if self.selected_series.is_some() {
            self.selected_group = first;
        }
    }

    /// Selection of a free text from the Object Manager, hidden ones included.
    pub fn select_annotation(&mut self, target: Option<AnnotationRef>, spec: &PlotSpec) {
        self.select_series(None, spec);
        self.selected_shape = None;
        self.selected_text = None;
        self.selected_annotation = target.filter(|t| t.get(spec).is_some());
        self.tool = GraphTool::Pointer;
    }

    /// Selection of a title or a legend from the Object Manager.
    pub fn select_text(&mut self, target: Option<TextTarget>, spec: &PlotSpec) {
        self.select_series(None, spec);
        self.selected_shape = None;
        self.selected_annotation = None;
        self.selected_text = target.filter(|t| t.visible(spec).is_some());
        self.tool = GraphTool::Pointer;
    }

    /// Series under the pointer, by its geometry as actually drawn.
    pub fn series_at(&self, pos: Pos2, spec: &PlotSpec) -> Option<stokesforge_plot::SeriesRef> {
        let pos = self.figure_position(pos)?;
        let last = self.last.as_ref()?;
        let (panel, index) =
            stokesforge_render::series_selection::series_at(last, (pos.x as f64, pos.y as f64))?;
        (panel < spec.panels.len()).then(|| stokesforge_plot::SeriesRef::new(panel, index))
    }

    /// The selected free text, if there is one.
    pub fn selected_annotation(&self) -> Option<AnnotationRef> {
        self.selected_annotation
    }

    /// The selected title, axis title or legend, if there is one (issue #77):
    /// a click selects it, dragging moves it, the format bar sets its font
    /// and size.
    pub fn selected_text(&self) -> Option<TextTarget> {
        self.selected_text
    }

    /// The title or legend under the egui pixel `pos`. A legend entry
    /// designates the whole legend: a line is neither selected nor moved
    /// separately.
    pub fn text_target_at(&self, pos: Pos2) -> Option<TextTarget> {
        match self.component_at(pos)? {
            PlotComponent::Text(target) => Some(target),
            PlotComponent::LegendEntry { panel, .. } => {
                let drawn = self.last.as_ref()?.panels.get(panel)?;
                Some(TextTarget::Legend { panel: drawn.index })
            }
            _ => None,
        }
    }

    /// Deselects the free text (after its deletion, for example).
    pub fn clear_annotation_selection(&mut self) {
        self.selected_annotation = None;
    }

    /// The preview of a free text, if there is one.
    pub fn annotation_preview(&self) -> Option<&(AnnotationRef, AnnotationSpec)> {
        self.annotation_preview.as_ref()
    }

    /// Sets (or removes) the preview of a free text: the figure shows it at
    /// the next frame, without touching the model. An identical preview
    /// rebuilds nothing.
    pub fn set_annotation_preview(&mut self, preview: Option<(AnnotationRef, AnnotationSpec)>) {
        self.preview_until = None;
        if self.annotation_preview != preview {
            self.annotation_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// Keeps the preview displayed until the model changes — to call when
    /// the application has just committed the preview (a change of the model
    /// is on its way). With no known revision, the preview clears.
    pub fn hold_preview(&mut self) {
        match self.cache.key() {
            Some(key) => self.preview_until = Some(key.revision),
            None => self.clear_previews(),
        }
    }

    /// Clears the previews (free text and moved title).
    fn clear_previews(&mut self) {
        self.set_annotation_preview(None);
        self.set_text_preview(None);
        self.set_text_width_preview(None);
        self.set_shape_preview(None);
        self.set_frame_preview(None);
    }

    /// Sets (or removes) the preview of the frame sizes of a cell handle
    /// (#394): the figure is laid out and drawn with them at the next frame,
    /// without touching the model.
    fn set_frame_preview(&mut self, preview: Option<FramePreview>) {
        self.preview_until = None;
        if self.frame_preview != preview {
            self.frame_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// The preview of a moved title or legend, if there is one.
    pub fn text_preview(&self) -> Option<&(TextTarget, TextAnchor)> {
        self.text_preview.as_ref()
    }

    /// Sets (or removes) the preview of a moved title or legend: the figure
    /// shows it at the next frame, without touching the model.
    pub fn set_text_preview(&mut self, preview: Option<(TextTarget, TextAnchor)>) {
        self.preview_until = None;
        if self.text_preview != preview {
            self.text_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// Sets the texts the figure draws by itself (legend placeholders, #369).
    ///
    /// The cache is invalidated only when the language tag changes, so the
    /// application can call this every frame.
    pub fn set_figure_texts(&mut self, texts: &FigureTexts) {
        if self.figure_texts.language != texts.language {
            self.figure_texts = texts.clone();
            self.preview_generation += 1;
        }
    }

    /// Sets (or removes) the width preview of a title (#244).
    fn set_text_width_preview(&mut self, preview: Option<(TextTarget, f64)>) {
        self.preview_until = None;
        if self.text_width_preview != preview {
            self.text_width_preview = preview;
            self.preview_generation += 1;
        }
    }

    /// The free text under the egui pixel `pos`, looked up in the last drawn
    /// figure.
    pub fn annotation_at(&self, pos: Pos2) -> Option<AnnotationRef> {
        match self.component_at(pos)? {
            PlotComponent::Annotation { panel, index } => {
                let drawn = self.last.as_ref()?.panels.get(panel)?;
                Some(AnnotationRef {
                    panel: drawn.index,
                    index,
                })
            }
            _ => None,
        }
    }

    /// Where a title or the legend is drawn on screen: center, size of the
    /// unrotated text and rotation (degrees, counterclockwise) — to place the
    /// in-place editor (issue #53).
    pub fn text_screen_box(&self, target: TextTarget) -> Option<(Pos2, egui::Vec2, f32)> {
        let drawn = self.drawn_text(target)?;
        let (center, size) = self.figure_box_to_screen(drawn.center, drawn.size)?;
        Some((center, size, drawn.rotation as f32))
    }

    /// Where the legend line of series `series` (rank in
    /// `PanelSpec::series`) of layer `panel` is drawn on screen: center and
    /// size of the line, over the whole width of the frame.
    pub fn legend_entry_screen_box(
        &self,
        panel: usize,
        series: usize,
    ) -> Option<(Pos2, egui::Vec2)> {
        let panel = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|drawn| drawn.index == panel)?;
        let frame = panel
            .texts
            .iter()
            .find(|text| matches!(text.target, TextTarget::Legend { .. }))?;
        let line = panel
            .legend_rows
            .iter()
            .find(|line| line.series == series)?;
        self.figure_box_to_screen(
            (frame.center.0, line.center_y),
            (frame.size.0, line.height.round() as u32),
        )
    }

    fn figure_box_to_screen(
        &self,
        center: (f64, f64),
        size: (u32, u32),
    ) -> Option<(Pos2, egui::Vec2)> {
        let display = self.display_rect?;
        let scale = self.display_scale()?;
        Some((
            display.min + egui::vec2(center.0 as f32, center.1 as f32) * scale,
            egui::vec2(size.0 as f32, size.1 as f32) * scale,
        ))
    }

    /// The title or legend drawn for `target`, in the last figure.
    fn drawn_text(&self, target: TextTarget) -> Option<&DrawnText> {
        let last = self.last.as_ref()?;
        match target.panel_index_of() {
            None => last.title.as_ref(),
            Some(index) => last
                .panels
                .iter()
                .find(|panel| panel.index == index)?
                .texts
                .iter()
                .find(|text| text.target == target),
        }
    }

    /// The active tool.
    pub fn tool(&self) -> GraphTool {
        self.tool
    }

    /// Changes the tool; a rectangle gesture in progress is abandoned.
    pub fn set_tool(&mut self, tool: GraphTool) {
        if self.tool != tool {
            self.tool = tool;
            self.drag_box = None;
            self.abort_shape_drag();
        }
    }

    /// The data point under the egui pixel `pos` (left ordinate), and the
    /// index of its layer.
    pub fn data_at(&self, pos: Pos2) -> Option<(usize, f64, f64)> {
        let (index, transform) = self.panel_at(pos)?;
        let figure = self.figure_position(pos)?;
        let (x, y) = transform.pixel_to_data(figure.x as f64, figure.y as f64)?;
        let panel = self.last.as_ref()?.panels.get(index)?;
        Some((panel.index, x, y))
    }

    /// The egui pixel of a data point `(x, y)` of layer `panel` (#199) —
    /// inverse of [`Self::data_at`]. Looked up in the **last** drawn figure.
    pub fn data_to_screen(&self, panel: usize, x: f64, y: f64) -> Option<Pos2> {
        let (transform, display, scale) = self.panel_screen(panel)?;
        let (px, py) = transform.data_to_pixel(x, y)?;
        Some(display.min + egui::vec2(px as f32, py as f32) * scale)
    }

    /// The plot area of layer `panel` on screen (#199).
    pub fn plot_rect_screen(&self, panel: usize) -> Option<Rect> {
        let drawn = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|p| p.index == panel)?;
        let (display, scale) = (self.display_rect?, self.display_scale()?);
        let min = display.min
            + egui::vec2(drawn.pixel_origin.0 as f32, drawn.pixel_origin.1 as f32) * scale;
        let size = egui::vec2(drawn.pixel_size.0 as f32, drawn.pixel_size.1 as f32) * scale;
        Some(Rect::from_min_size(min, size))
    }

    /// The outer rectangle of the cell layer `panel` lives in, on screen (#395):
    /// the rectangle the layer badges and the cell handles are built from.
    /// Layers of the same cell (overlays) share it; `None` for a layer not
    /// drawn in the last figure.
    pub fn cell_rect_screen(&self, panel: usize) -> Option<Rect> {
        let drawn = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|p| p.index == panel)?;
        let (display, scale) = (self.display_rect?, self.display_scale()?);
        let min = display.min
            + egui::vec2(drawn.cell_origin.0 as f32, drawn.cell_origin.1 as f32) * scale;
        let size = egui::vec2(drawn.cell_size.0 as f32, drawn.cell_size.1 as f32) * scale;
        Some(Rect::from_min_size(min, size))
    }

    /// The ordinary layer (a cell) whose plot area is nearest to the egui pixel
    /// `pos`, in the **last** drawn figure (#357): where a drop on the edge of
    /// the figure puts its new cell beside. `None` before the first draw.
    pub fn nearest_cell(&self, pos: Pos2) -> Option<usize> {
        let drawn = self.last.as_ref()?;
        drawn
            .panels
            .iter()
            .filter(|panel| panel.overlay.is_none())
            .filter_map(|panel| {
                let rect = self.plot_rect_screen(panel.index)?;
                Some((panel.index, rect.distance_to_pos(pos)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }

    fn panel_screen(&self, panel: usize) -> Option<(PanelTransform, Rect, f32)> {
        let drawn = self
            .last
            .as_ref()?
            .panels
            .iter()
            .find(|p| p.index == panel)?;
        Some((
            PanelTransform::of(drawn)?,
            self.display_rect?,
            self.display_scale()?,
        ))
    }

    /// The data point at the center of the plot area of the active layer
    /// (#161), or of the first drawn panel if the former is not — where to
    /// place a text added by the menu bar.
    pub fn panel_center_data(&self, spec: &PlotSpec) -> Option<(usize, f64, f64)> {
        let last = self.last.as_ref()?;
        let active_index = self.active_panel(spec);
        let panel = last
            .panels
            .iter()
            .find(|p| p.index == active_index)
            .or_else(|| last.panels.first())?;
        let transform = PanelTransform::of(panel)?;
        let center = (
            panel.pixel_origin.0 as f64 + panel.pixel_size.0 as f64 / 2.0,
            panel.pixel_origin.1 as f64 + panel.pixel_size.1 as f64 / 2.0,
        );
        let (x, y) = transform.pixel_to_data(center.0, center.1)?;
        Some((panel.index, x, y))
    }

    /// Where a free text is drawn on screen: center, size of the unrotated
    /// text and rotation (degrees, counterclockwise) — to place the in-place
    /// editor.
    pub fn annotation_screen_box(&self, target: AnnotationRef) -> Option<(Pos2, egui::Vec2, f32)> {
        let drawn = self.drawn_annotation(target)?;
        let display = self.display_rect?;
        let scale = self.display_scale()?;
        let center = display.min + egui::vec2(drawn.center.0 as f32, drawn.center.1 as f32) * scale;
        let size = egui::vec2(drawn.size.0 as f32, drawn.size.1 as f32) * scale;
        Some((center, size, drawn.rotation as f32))
    }

    /// The two width handles of free text `target`, at the middle of its
    /// left and right edges (rotated frame), in egui points (#244).
    fn width_handles(&self, target: AnnotationRef) -> Option<[Pos2; 2]> {
        let (center, size, rotation) = self.annotation_screen_box(target)?;
        let (sin, cos) = rotation.to_radians().sin_cos();
        let half = size.x / 2.0 + 3.0;
        let axis = egui::vec2(cos, -sin);
        Some([center - axis * half, center + axis * half])
    }

    /// The width handles of a selected title or axis title; no width for the
    /// legend (#244).
    fn text_width_handles(&self, target: TextTarget) -> Option<[Pos2; 2]> {
        if matches!(target, TextTarget::Legend { .. }) {
            return None;
        }
        let (center, size, rotation) = self.text_screen_box(target)?;
        let (sin, cos) = rotation.to_radians().sin_cos();
        let half = size.x / 2.0 + 3.0;
        let axis = egui::vec2(cos, -sin);
        Some([center - axis * half, center + axis * half])
    }

    fn text_width_handle_at(&self, target: TextTarget, pos: Pos2) -> bool {
        self.text_width_handles(target).is_some_and(|handles| {
            handles
                .iter()
                .any(|handle| handle.distance(pos) <= WIDTH_HANDLE_RADIUS)
        })
    }

    /// True if `pos` (egui points) falls on a width handle.
    fn width_handle_at(&self, target: AnnotationRef, pos: Pos2) -> bool {
        self.width_handles(target).is_some_and(|handles| {
            handles
                .iter()
                .any(|handle| handle.distance(pos) <= WIDTH_HANDLE_RADIUS)
        })
    }

    fn drawn_annotation(&self, target: AnnotationRef) -> Option<&DrawnAnnotation> {
        self.last
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.index == target.panel)?
            .annotations
            .iter()
            .find(|annotation| annotation.index == target.index)
    }

    /// `spec` as this view shows it: a clone, framed by the current zoom
    /// ([`ViewState::apply`], the same call as the construction of the
    /// figure). This is what the application exports (issue #34) — we export
    /// what we see — without the zoom touching the model's `PlotSpec`.
    /// Without zoom, `spec` unchanged.
    pub fn framed_spec(&self, spec: &PlotSpec) -> PlotSpec {
        let mut framed = spec.clone();
        self.view.apply(&mut framed);
        framed
    }

    /// Clears any explicit framing: the view goes back to framing on the
    /// data, for all panels.
    pub fn reset_view(&mut self) {
        self.view.reset();
    }

    /// Forgets the shapes and frames tied to the surface where the view was
    /// painted.
    ///
    /// The same view may migrate between a floating window and a separate OS
    /// viewport. Even if their rectangles coincide by chance, their screen
    /// coordinates do not designate the same surface: keeping the previous
    /// shapes or transformations would shift the rendering and the crosshair.
    pub fn invalidate_surface(&mut self) {
        self.cache.invalidate();
        self.last = None;
        self.hover = None;
        self.display_rect = None;
        self.drag_box = None;
        self.response = None;
    }

    /// Interaction response of the figure at the last rendering, to hang a
    /// context menu on (`Response::context_menu`). Reuse this one rather than
    /// creating a second one on the same rectangle: another superimposed
    /// response would compete with navigation for the drag.
    pub fn response(&self) -> Option<&egui::Response> {
        self.response.as_ref()
    }

    /// The graph component (title, plot area, axis, axis title) under the
    /// egui pixel `pos`, looked up in the **last** drawn figure — same
    /// imperceptible one-frame lag as [`Self::panel_at`].
    pub fn component_at(&self, pos: Pos2) -> Option<PlotComponent> {
        let pos = self.figure_position(pos)?;
        // The axis-line strip keeps the same width on screen (#385).
        let tolerance = AXIS_LINE_TOLERANCE_POINTS / self.display_scale().unwrap_or(1.0) as f64;
        component_at_with_tolerance(self.last.as_ref()?, pos.x as f64, pos.y as f64, tolerance)
    }

    /// The cell resize handle under the screen position `pos` (#394): the outer
    /// boundary of a cell, with a strip of the same width on screen whatever
    /// the enlargement of the figure.
    pub fn cell_edge_at(&self, pos: Pos2) -> Option<stokesforge_render::CellEdge> {
        if self.layout_is_free() {
            return None;
        }
        let pos = self.figure_position(pos)?;
        let tolerance = stokesforge_render::CELL_EDGE_TOLERANCE_POINTS
            / self.display_scale().unwrap_or(1.0) as f64;
        stokesforge_render::cell_edge_at(self.last.as_ref()?, pos.x as f64, pos.y as f64, tolerance)
    }

    /// The figure edge (left, top) under the screen position `pos` (#405).
    pub fn figure_edge_at(&self, pos: Pos2) -> Option<stokesforge_render::FigureEdge> {
        if self.layout_is_free() {
            return None;
        }
        let pos = self.figure_position(pos)?;
        let tolerance = stokesforge_render::CELL_EDGE_TOLERANCE_POINTS
            / self.display_scale().unwrap_or(1.0) as f64;
        stokesforge_render::figure_edge_at(
            self.last.as_ref()?,
            pos.x as f64,
            pos.y as f64,
            tolerance,
        )
    }

    /// True while the left edge or the top edge of the figure is being pulled
    /// (#405), as `(left, top)`: the window that follows the figure then grows
    /// towards that side, so the frame stays put on screen.
    pub fn placing_edges(&self) -> (bool, bool) {
        self.frame_drag.as_ref().map_or((false, false), |drag| {
            (drag.left.is_some(), drag.top.is_some())
        })
    }

    /// The point designated by the cursor at the last painted frame, if
    /// there is one within [`HIT_RADIUS`] pixels — task R7.2.
    pub fn hovered_point(&self) -> Option<&PointHit> {
        self.hover.as_ref()
    }

    /// Paints graph `graph` of `snapshot` in `rect`, and handles the
    /// navigation gestures (wheel, drag, rubber-band rectangle — R7.1 to
    /// R7.4) that occurred on this area since the last frame.
    ///
    /// Returns `true` if the frame was served from the cache — that is, if
    /// the Plotters pipeline was **not** replayed. `ui` rather than a bare
    /// `Painter` (as before R7): a `Response` is now needed to receive the
    /// wheel and the drag, hence a `Sense` wider than `hover()`.
    pub fn paint(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<bool, RenderError> {
        self.paint_with_layout(ui, rect, rect, snapshot, graph)
    }

    /// The layout of the figure of `graph` ([`pipeline::figure_layout`], #394):
    /// the size the screen paints, the exports write and the window keeps the
    /// ratio of. Measured with the screen's text (Typst titles included) and
    /// memoized by (graph, revision, pixels per point, preview generation): a
    /// window resize or a zoom does not measure again; a title edit or a cell
    /// size edit does.
    pub fn figure_layout(
        &mut self,
        context: &egui::Context,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<FigureLayout, RenderError> {
        let key = (
            graph,
            snapshot.revision,
            context.pixels_per_point().to_bits(),
            self.preview_generation,
        );
        if let Some((cached, layout)) = &self.figure_memo {
            if *cached == key {
                return Ok(layout.clone());
            }
        }
        let mut spec = graph_spec(&snapshot.tree, graph)?;
        if let Some((target, width)) = self.text_width_preview {
            if let Some(mut font) = target.font(&spec) {
                font.width = Some(width);
                target.set_font(&mut spec, font);
            }
        }
        if let Some(preview) = &self.frame_preview {
            preview.apply(&mut spec);
        }
        let text = ScreenText::new(context, &self.measurer);
        let mut shapes = Vec::new();
        let probe_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1.0, 1.0));
        let backend = EguiBackend::in_rect(&mut shapes, probe_rect, &text);
        let layout = match &self.typst_fonts {
            Some(fonts) => {
                let typst = TypstScreen::with_fonts(
                    context,
                    &self.formulas,
                    &self.font_choice,
                    fonts.clone(),
                );
                let probe = backend.with_typst(typst).into_drawing_area();
                pipeline::figure_layout(&spec, &probe)
            }
            None => pipeline::figure_layout(&spec, &backend.into_drawing_area()),
        }
        .map_err(|error| RenderError::Drawing(error.to_string()))?;
        self.figure_memo = Some((key, layout.clone()));
        Ok(layout)
    }

    /// Lays out the figure at its own size ([`GraphView::figure_layout`]), then
    /// applies a uniform transformation to its shapes. The density of the ticks,
    /// the placement of the legends and the text measurements therefore do not
    /// change with the window size; only their displayed size changes.
    pub fn paint_scaled(
        &mut self,
        ui: &egui::Ui,
        display_rect: Rect,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<bool, RenderError> {
        let (width, height) = self.figure_layout(ui.ctx(), snapshot, graph)?.size;
        let logical_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(width as f32, height as f32));
        if display_rect.width() <= 0.0 || display_rect.height() <= 0.0 {
            self.hover = None;
            return Ok(true);
        }
        self.paint_with_layout(ui, logical_rect, display_rect, snapshot, graph)
    }

    fn paint_with_layout(
        &mut self,
        ui: &egui::Ui,
        logical_rect: Rect,
        display_rect: Rect,
        snapshot: &Snapshot,
        graph: NodeId,
    ) -> Result<bool, RenderError> {
        let spec = graph_spec(&snapshot.tree, graph)?;
        let context = ui.ctx().clone();
        // The strip of the figure's edge handles straddles the edge (#405):
        // the interaction area reaches that far outside the figure, into the
        // margin the window leaves around it, so the outer edges are grabbed
        // before the window's own border is.
        let response = ui.interact(
            display_rect.expand(FIGURE_GRAB_MARGIN),
            self.id,
            egui::Sense::click_and_drag(),
        );
        self.display_rect = Some(display_rect);
        self.response = Some(response.clone());

        // The wiring planned by R5.5 bis, give or take the call
        // (`architecture-choisie.md` §4.2, `feathering` R7.3): off during any
        // drag — pan or zoom rectangle, both are a `dragged()` —, restored at
        // rest. A `Context` setting, so for the whole window: no effect as
        // long as a single graph is displayed at a time (`stokesforge-app`
        // today), to be reviewed if several views ever coexist.
        // Only a real gesture turns it off (#318): a plain drag with no effect
        // (Pointer on the background) leaves feathering in place; it is
        // applied further down, once the navigation has been handled.

        // A committed preview clears as soon as the model has caught up with it.
        if self
            .preview_until
            .is_some_and(|revision| revision != snapshot.revision)
        {
            self.clear_previews();
        }

        self.selected_series = self
            .selected_series
            .filter(|target| spec.series_at(*target).is_some());
        self.selected_annotation = self.selected_annotation.filter(|t| t.get(&spec).is_some());
        self.selected_shape = self.selected_shape.filter(|t| t.get(&spec).is_some());
        self.selected_text = self.selected_text.filter(|t| t.visible(&spec).is_some());
        self.follow_selection(&spec);
        if spec.layer_placement != stokesforge_plot::LayerPlacement::Free {
            self.extra_layers.clear();
        }
        self.handle_navigation(&response, &context, &spec);
        let real_gesture = response.dragged()
            && (self.nav_drag.is_some_and(NavDrag::is_pan)
                || self.drag_box.is_some()
                || self.shape_drag.is_some()
                || self.annotation_drag.is_some()
                || self.text_drag.is_some());
        feathering::apply(&context, Feathering::during_drag(real_gesture));

        let key = CacheKey {
            revision: snapshot.revision,
            rect: logical_rect,
            pixels_per_point: context.pixels_per_point(),
            // Two counters that only go up: their sum changes at each zoom
            // as at each free-text preview.
            view_generation: self.view.generation() + self.preview_generation,
        };
        let tree = snapshot.tree.clone();
        let measurer = &self.measurer;
        let view = &self.view;
        let preview = &self.annotation_preview;
        let text_preview = &self.text_preview;
        let width_preview = self.text_width_preview;
        let shape_preview = &self.shape_preview;
        let frame_preview = &self.frame_preview;
        let formulas = &self.formulas;
        let font_choice = &self.font_choice;
        let figure_texts = &self.figure_texts;
        let typst_fonts = &self.typst_fonts;
        let outcome = self.cache.get_or_build(key, |shapes| {
            // R7.1: the framing override lives on a clone, never on the
            // original `PlotSpec` — see `crate::view` for the reason.
            let mut framed = spec.clone();
            view.apply(&mut framed);
            if let Some(frames) = frame_preview {
                frames.apply(&mut framed);
            }
            if let Some((target, annotation)) = preview {
                target.set(&mut framed, annotation.clone());
            }
            if let Some((target, shape)) = shape_preview {
                target.set(&mut framed, shape.clone());
            }
            if let Some((target, anchor)) = text_preview {
                target.set_anchor(&mut framed, Some(*anchor));
            }
            if let Some((target, width)) = width_preview {
                if let Some(mut font) = target.font(&framed) {
                    font.width = Some(width);
                    target.set_font(&mut framed, font);
                }
            }
            let plot = resolve_spec(&tree, &framed);
            let text = ScreenText::new(&context, measurer);
            let Some(fonts) = typst_fonts.clone() else {
                return collect_shapes_with_texts(shapes, logical_rect, &text, &plot, figure_texts);
            };
            // Cleared here only: the shapes this cache designated are
            // precisely being replaced (see `FormulaCache::clear`).
            if formulas.len() > FORMULA_CACHE_LIMIT {
                formulas.clear();
            }
            let typst = TypstScreen::with_fonts(&context, formulas, font_choice, fonts);
            collect_shapes_with_typst_and_texts(
                shapes,
                logical_rect,
                &text,
                typst,
                &plot,
                figure_texts,
            )
        })?;
        if let Some(drawn) = outcome.built {
            self.last = Some(drawn);
        }
        // The core of task R4.3: a frame without changes is exactly this
        // line — no resolution, no `ChartBuilder`, no walk over the points.
        let painter = ui.painter().with_clip_rect(display_rect);
        let scale = display_rect.width() / logical_rect.width();
        if logical_rect == display_rect {
            painter.extend(self.cache.shapes().iter().cloned());
        } else {
            let transform = egui::emath::TSTransform::new(
                display_rect.min.to_vec2() - logical_rect.min.to_vec2() * scale,
                scale,
            );
            let scaled_key = scaled::ScaledKey {
                source: key,
                display: display_rect,
            };
            painter.extend(
                self.scaled
                    .get(
                        &context,
                        scaled_key,
                        self.cache.shapes(),
                        transform,
                        &self.formulas,
                    )
                    .iter()
                    .cloned(),
            );
        }

        // R7.2/7.3: crosshair, point designation and rubber-band rectangle,
        // painted **over** what precedes — never in the `Vec<Shape>` cached
        // above, on pain of replaying the whole pipeline at each mouse move
        // (`crate::cache`, module header).
        self.hover = self.paint_overlay(&painter, &response, &snapshot.tree, &spec);

        Ok(outcome.cached)
    }

    /// Coordinate of `pos` in the local frame of the cached figure.
    ///
    /// The rendering pipeline produces the [`DrawnPanel`]s in this frame;
    /// egui, for its part, delivers pointer positions in the window frame.
    /// A figure may be centered in a floating window, so the two origins do
    /// not coincide in general.
    fn figure_position(&self, pos: Pos2) -> Option<Pos2> {
        let display = self.display_rect?;
        let logical = self.cache.key()?.rect;
        let scale = display.width() / logical.width();
        (scale > 0.0).then(|| Pos2::ZERO + (pos - display.min) / scale)
    }

    /// The ratio between a figure pixel and an egui point on screen
    /// (1 as long as it is neither enlarged nor reduced). `None` before the
    /// first drawn figure.
    pub fn display_scale(&self) -> Option<f32> {
        Some(self.display_rect?.width() / self.cache.key()?.rect.width())
    }

    /// The panel (and its pixel ↔ data transformation) under the egui pixel
    /// `pos`, if there is one — looked up in the **last** drawn figure.
    ///
    /// One frame behind the current figure: the geometry only moves on a
    /// resize or an already committed zoom, so it is imperceptible, and it is
    /// the normal price of immediate mode (interpreting this frame's input
    /// with the geometry of the previous one).
    fn panel_at(&self, pos: Pos2) -> Option<(usize, PanelTransform)> {
        let pos = self.figure_position(pos)?;
        let last = self.last.as_ref()?;
        let hit = |(index, panel): (usize, &DrawnPanel)| {
            let transform = PanelTransform::of(panel)?;
            transform
                .contains(pos.x as f64, pos.y as f64)
                .then_some((index, transform))
        };
        // Free cells may overlap (#398): the front-most cell under the cursor
        // takes the gesture, a cell hidden by the outer rectangle of a cell in
        // front of it does not. On the grid every rank is 0.
        let hidden = |panel: &DrawnPanel| {
            stokesforge_render::hit::hidden_by_front(
                last,
                panel.cell_rank,
                pos.x as f64,
                pos.y as f64,
            )
        };
        let front = last
            .panels
            .iter()
            .enumerate()
            .filter(|(_, panel)| !hidden(panel))
            .filter_map(|(index, panel)| hit((index, panel)).map(|_| panel.cell_rank))
            .max()?;
        let in_front = |(_, panel): &(usize, &DrawnPanel)| panel.cell_rank == front;
        // Superimposed layers overlap: the active layer takes the gesture
        // (#161), failing that the first drawn one.
        last.panels
            .iter()
            .enumerate()
            .filter(in_front)
            .filter(|(_, panel)| panel.index == self.active_panel)
            .find_map(hit)
            .or_else(|| {
                last.panels
                    .iter()
                    .enumerate()
                    .filter(in_front)
                    .find_map(hit)
            })
    }

    /// Applies a framing gesture (zoom, drag, rectangle) to all the own
    /// scales of the box of layer `panel_index` (in [`DrawnPlot::panels`]) —
    /// the computation is `ViewState::commit_gesture` (`stokesforge-render`,
    /// without egui).
    fn commit_gesture(&mut self, panel_index: usize, gesture: Gesture) {
        let Some(last) = &self.last else { return };
        self.view.commit_gesture(&last.panels, panel_index, gesture);
    }

    /// The axis (line and ticks) under the egui pixel `pos`, by the index of
    /// the panel in [`DrawnPlot::panels`] and the side **of the spec**.
    fn axis_at(&self, pos: Pos2) -> Option<(usize, AxisSide)> {
        match self.component_at(pos)? {
            PlotComponent::Axis { panel, side } => Some((panel, side)),
            _ => None,
        }
    }

    /// Applies a gesture to a single scale: that of axis `side` of panel
    /// `panel_index` (in [`DrawnPlot::panels`]) — the computation is
    /// `ViewState::commit_axis_gesture` (`stokesforge-render`, without egui).
    fn commit_axis_gesture(&mut self, panel_index: usize, side: AxisSide, gesture: Gesture) {
        let Some(last) = &self.last else { return };
        self.view
            .commit_axis_gesture(&last.panels, panel_index, side, gesture);
    }

    /// Decides, at the press, what the drag in progress will do (#318): a pan
    /// if Ctrl is held (primary button) or if it is the middle button, on a
    /// single scale when the press starts from an axis; otherwise no pan.
    /// Returns the state of this gesture, including at the release frame (a
    /// Ctrl + click must place nothing). `None` at rest.
    fn decide_nav(&mut self, response: &Response, context: &egui::Context) -> Option<NavDrag> {
        let (primary, middle, command, press) = context.input(|input| {
            (
                input.pointer.button_down(PointerButton::Primary),
                input.pointer.button_down(PointerButton::Middle),
                input.modifiers.command,
                input.pointer.press_origin(),
            )
        });
        let down =
            (response.is_pointer_button_down_on() && (primary || middle)) || response.dragged();
        if !down {
            let ended = self.nav_drag.take();
            // A full click within a single frame left no state.
            return ended.or_else(|| {
                (response.clicked_by(PointerButton::Primary) && command).then_some(NavDrag::Plot)
            });
        }
        if self.nav_drag.is_none() {
            let origin = press.or_else(|| {
                response
                    .interact_pointer_pos()
                    .map(|pos| pos - response.drag_delta())
            });
            let wants_pan = middle || (primary && command);
            self.nav_drag = Some(match (wants_pan, origin) {
                (true, Some(origin)) => match self.axis_at(origin) {
                    Some((panel, side)) => NavDrag::Axis { panel, side },
                    None => NavDrag::Plot,
                },
                _ => NavDrag::Inert,
            });
        }
        self.nav_drag
    }

    /// Ctrl+wheel (zoom), primary drag (pan with the Pointer tool, rubber-band
    /// zoom rectangle with the Zoom tool), click of the Text tool and double
    /// click (reset) — tasks 7.1 and 7.4, issue #62. Does nothing as long as
    /// no figure has been drawn yet.
    fn handle_navigation(&mut self, response: &Response, context: &egui::Context, spec: &PlotSpec) {
        if self.last.is_none() {
            return;
        }
        let nav = self.decide_nav(response, context);
        let pan = nav.is_some_and(NavDrag::is_pan);
        if self.tool == GraphTool::Pointer && !pan {
            self.frame_handle_cursor(response, context);
        }

        // Issue #35: a click picks the free text under the pointer (or none),
        // a double click on a text edits it instead of resetting the zoom.
        let pointer = response.interact_pointer_pos();
        // A gesture started in the plot area of a layer (click, right click,
        // start of a tool drag) activates it (#161), even with no object
        // targeted.
        if response.clicked() || response.secondary_clicked() || response.drag_started() {
            let hit = pointer
                .and_then(|pos| self.panel_at(pos))
                .and_then(|(i, _)| Some(self.last.as_ref()?.panels.get(i)?.index));
            // Shift + click on a layer in free placement (#398) adds it to, or
            // removes it from, the selection of layers: the active layer stays
            // the reference, the selected object is left alone and no move
            // starts.
            if let (true, true, Some(panel)) = (
                response.clicked() && self.layout_is_free() && context.input(|i| i.modifiers.shift),
                self.tool == GraphTool::Pointer,
                hit,
            ) {
                self.toggle_extra_layer(panel, spec);
                return;
            }
            if let Some(panel) = hit {
                self.set_active_panel(panel, spec);
                if response.clicked() {
                    self.clear_extra_layers();
                }
            }
        }
        if response.clicked() {
            self.selected_series = None;
            self.selected_annotation = pointer.and_then(|pos| self.annotation_at(pos));
            self.selected_shape = match self.selected_annotation {
                Some(_) => None,
                None => pointer.and_then(|pos| self.shape_at(pos)),
            };
            // Issue #77: a title or the legend is selected on click, when
            // neither a free text nor an object is targeted.
            self.selected_text = match (self.selected_annotation, self.selected_shape) {
                (None, None) => pointer.and_then(|pos| self.text_target_at(pos)),
                _ => None,
            };
            if self.tool == GraphTool::Pointer
                && self.selected_annotation.is_none()
                && self.selected_shape.is_none()
                && self.selected_text.is_none()
            {
                self.selected_series = pointer.and_then(|pos| self.series_at(pos, spec));
            }
        }
        if response.double_clicked() {
            let component = pointer.and_then(|pos| self.component_at(pos));
            if let Some(PlotComponent::Text(target)) = component {
                self.selected_text = Some(target);
            }
            match (pointer.and_then(|pos| self.annotation_at(pos)), component) {
                (Some(target), _) => {
                    self.selected_annotation = Some(target);
                    self.events.push(GraphEvent::EditAnnotation(target));
                }
                // Issue #53: a title is edited in place, like a free text.
                (None, Some(PlotComponent::Text(target))) => {
                    self.events.push(match target {
                        // The legend has no text of its own: it is the series
                        // names, one per line.
                        TextTarget::Legend { .. } => return,
                        _ => GraphEvent::EditText(target),
                    });
                }
                (None, Some(PlotComponent::LegendEntry { panel, series })) => {
                    if let Some(drawn) = self.last.as_ref().and_then(|last| last.panels.get(panel))
                    {
                        self.events.push(GraphEvent::EditLegendEntry {
                            panel: drawn.index,
                            series,
                        });
                    }
                }
                // Issue #113: on an object, a curve or an axis, a double click
                // opens their settings. Elsewhere (bare plot area, margins),
                // it always resets the zoom.
                _ => {
                    let shape = pointer.and_then(|pos| self.shape_at(pos));
                    let series = pointer.and_then(|pos| self.series_at(pos, spec));
                    // The axis beats the curve (#385): on its line a curve
                    // glued to the edge must not take the gesture.
                    if let Some(shape) = shape {
                        self.events.push(GraphEvent::OpenShapeProperties(shape));
                    } else if let Some(PlotComponent::Axis { panel, side }) = component {
                        self.events
                            .push(GraphEvent::OpenAxisProperties { panel, side });
                    } else if let Some(series) = series {
                        self.selected_series = Some(series);
                        self.events.push(GraphEvent::OpenSeriesProperties(series));
                    } else {
                        self.view.reset();
                    }
                }
            }
        }
        if self.tool == GraphTool::Pointer
            && !pan
            && (self.handle_frame_drag(response, context)
                // A layer edge beats every object; a gesture in progress is
                // always continued here, before another handler can pick it up
                // from the layer sliding under the pointer (#398).
                || self.handle_free_drag(response, context, spec, FreeStart::Edges)
                || self.handle_annotation_drag(response, spec)
                || self.handle_shape_drag(response, spec)
                || self.handle_text_drag(response, spec)
                // Nothing else claimed the drag: the plot area moves its layer.
                || self.handle_free_drag(response, context, spec, FreeStart::Move))
        {
            return;
        }
        self.handle_shape_draw(response, spec, pan);

        // Text tool: a click in the plot area places the text.
        if self.tool == GraphTool::Text && !pan && response.clicked() {
            if let Some((panel, x, y)) = pointer.and_then(|pos| self.data_at(pos)) {
                self.events.push(GraphEvent::PlaceText { panel, x, y });
            }
        }

        if response.hovered() {
            if let Some(pos) = response
                .hover_pos()
                .filter(|pos| response.interact_rect.contains(*pos))
                .and_then(|pos| self.figure_position(pos).map(|position| (pos, position)))
            {
                // Ctrl+wheel (or pinch): egui files this gesture under
                // `zoom_delta`, a multiplicative factor (> 1: forward, we
                // zoom in). The wheel alone no longer zooms (issue #62). The
                // framing factor is the inverse, bounded so that continuous
                // scrolling (touchpad) does not collapse the framing at once.
                let zoom = context.input(|input| input.zoom_delta());
                if zoom != 1.0 {
                    let factor = (1.0 / zoom as f64).clamp(0.5, 2.0);
                    let gesture = Gesture::Zoom {
                        anchor: (pos.1.x as f64, pos.1.y as f64),
                        factor,
                    };
                    if let Some((panel_index, side)) = self.axis_at(pos.0) {
                        // Above an axis, the zoom only frames that scale.
                        self.commit_axis_gesture(panel_index, side, gesture);
                    } else if let Some((panel_index, _)) = self.panel_at(pos.0) {
                        self.commit_gesture(panel_index, gesture);
                    }
                }
            }
        }

        // Pan: Ctrl + drag or middle click + drag, anywhere in the graph,
        // whatever the tool (#318). The target is the one decided at the
        // press: once dragging, the cursor can leave the plot area or the axis
        // without the target changing.
        let panning = response.dragged_by(PointerButton::Primary)
            || response.dragged_by(PointerButton::Middle);
        if let (true, Some(target)) = (panning, nav.filter(|n| n.is_pan())) {
            let delta = response.drag_delta();
            if let Some(pos) = response.interact_pointer_pos() {
                let origin = pos - delta;
                let scale = self.display_scale().unwrap_or(1.0);
                let gesture = Gesture::Pan {
                    dx: (delta.x / scale) as f64,
                    dy: (delta.y / scale) as f64,
                };
                match target {
                    NavDrag::Axis { panel, side } => self.commit_axis_gesture(panel, side, gesture),
                    _ => {
                        if let Some((panel_index, _)) =
                            self.panel_at(origin).or_else(|| self.panel_at(pos))
                        {
                            self.commit_gesture(panel_index, gesture);
                        }
                    }
                }
            }
        }

        if self.tool == GraphTool::ZoomBox && !pan && response.dragged_by(PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                // **Do not** rely on `drag_started_by` to capture the start
                // point: on a widget sensitive to click *and* drag, `egui`
                // postpones the "click or drag" decision as long as the
                // movement does not settle it
                // (`egui::interaction::interact`, comment *"the mouse
                // first is pressed, it could be either [...] so we postpone
                // the decision"*) — `drag_started_by` only becomes true at the
                // frame where the decision falls, and `interact_pointer_pos()`
                // already carries the **current** position at that moment, not
                // that of the initial press. `drag_delta()` stays reliable
                // (it is the movement of **this** frame, whatever the date of
                // the decision): `pos - drag_delta()` thus recovers the real
                // start point the first time `dragged_by` is seen, without
                // depending on `drag_started_by` at all.
                let start = self
                    .drag_box
                    .map_or_else(|| pos - response.drag_delta(), |(start, _)| start);
                self.drag_box = Some((start, pos));
            }
        }
        if self.tool == GraphTool::ZoomBox && response.drag_stopped_by(PointerButton::Primary) {
            if let Some((start, end)) = self.drag_box.take() {
                // Below a few pixels, it is a hand tremor, not a rectangle: a
                // zoom rectangle one pixel wide would produce a nearly
                // degenerate framing, which the pipeline would silently
                // straighten by going back to the data
                // (`pipeline::axis_range`) — correct behavior but surprising
                // for a gesture that meant to be a zoom.
                if start.distance(end) > 4.0 {
                    if let Some((panel_index, _)) = self.panel_at(start) {
                        if let (Some(start), Some(end)) =
                            (self.figure_position(start), self.figure_position(end))
                        {
                            self.commit_gesture(
                                panel_index,
                                Gesture::Box {
                                    a: (start.x as f64, start.y as f64),
                                    b: (end.x as f64, end.y as f64),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    /// The resize cursor over a cell handle or a figure edge, and during its
    /// drag (#394, #405).
    fn frame_handle_cursor(&self, response: &Response, context: &egui::Context) {
        if let Some(icon) = self.free_cursor(response) {
            context.set_cursor_icon(icon);
            return;
        }
        let (mut horizontal, mut vertical) = (false, false);
        match &self.frame_drag {
            Some(drag) => {
                horizontal = drag.column.is_some() || drag.left.is_some();
                vertical = drag.row.is_some() || drag.top.is_some();
            }
            None if response.hovered() => {
                if let Some(pos) = response.hover_pos() {
                    if let Some(edge) = self.cell_edge_at(pos).filter(|e| self.edge_is_editable(e))
                    {
                        horizontal |= edge.column.is_some();
                        vertical |= edge.row.is_some();
                    }
                    if let Some(edge) = self
                        .figure_edge_at(pos)
                        .filter(|_| self.figure_is_placeable())
                    {
                        horizontal |= edge.left;
                        vertical |= edge.top;
                    }
                }
            }
            None => {}
        }
        let icon = match (horizontal, vertical) {
            (true, true) => egui::CursorIcon::ResizeNwSe,
            (true, false) => egui::CursorIcon::ResizeHorizontal,
            (false, true) => egui::CursorIcon::ResizeVertical,
            (false, false) => return,
        };
        context.set_cursor_icon(icon);
    }

    /// True if the figure was drawn with free layer placement (#398): its
    /// layers have no grid column or row to pull, so no handle exists (they
    /// come with the mouse gestures of free placement).
    fn layout_is_free(&self) -> bool {
        self.figure_memo
            .as_ref()
            .is_some_and(|(_, layout)| layout.placement == stokesforge_plot::LayerPlacement::Free)
    }

    /// True if the layout the figure was drawn with has frames to place (a
    /// compact fallback grid has none; a free figure has no handle yet).
    fn figure_is_placeable(&self) -> bool {
        self.figure_memo.as_ref().is_some_and(|(_, layout)| {
            layout.placement == stokesforge_plot::LayerPlacement::Grid
                && !layout.frame_widths.is_empty()
                && !layout.frame_heights.is_empty()
        })
    }

    /// True if the layout the figure was drawn with has a frame for the column
    /// or the row of `edge` (a compact fallback grid has none to edit).
    fn edge_is_editable(&self, edge: &stokesforge_render::CellEdge) -> bool {
        let Some((_, layout)) = &self.figure_memo else {
            return false;
        };
        if layout.placement == stokesforge_plot::LayerPlacement::Free {
            return false;
        }
        edge.column
            .is_some_and(|index| index < layout.frame_widths.len())
            || edge
                .row
                .is_some_and(|index| index < layout.frame_heights.len())
    }

    /// Pulling a cell handle (#394): a primary drag **starting on the outer
    /// boundary of a cell** changes the frame size of its column and/or row by
    /// the distance the pointer travels. The preview follows the pointer; at
    /// release, a single [`GraphEvent::FrameResized`]. Returns `true` if the
    /// gesture is a handle drag (the other drags must then do nothing with it).
    fn handle_frame_drag(&mut self, response: &Response, context: &egui::Context) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            let Some(drag) = self.frame_drag.take() else {
                return false;
            };
            match self.frame_preview.clone() {
                Some(preview)
                    if moved(preview.column, drag.column)
                        || moved(preview.row, drag.row)
                        || moved_value(preview.left, drag.left)
                        || moved_value(preview.top, drag.top) =>
                {
                    if preview.column.is_some() || preview.row.is_some() {
                        self.events.push(GraphEvent::FrameResized {
                            column: preview.column,
                            row: preview.row,
                        });
                    } else {
                        self.events.push(GraphEvent::FramePlaced {
                            left: preview.left,
                            top: preview.top,
                        });
                    }
                    self.hold_preview();
                }
                _ => self.set_frame_preview(None),
            }
            return true;
        }
        if !response.dragged_by(PointerButton::Primary) {
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.frame_drag.is_some();
        };
        if self.frame_drag.is_none() {
            let origin = context
                .input(|input| input.pointer.press_origin())
                .unwrap_or(pos - response.drag_delta());
            let cell_edge = self.cell_edge_at(origin);
            let figure_edge = self.figure_edge_at(origin);
            if cell_edge.is_none() && figure_edge.is_none() {
                return false;
            }
            let Some((_, layout)) = &self.figure_memo else {
                return false;
            };
            let base = (layout.frame_widths.clone(), layout.frame_heights.clone());
            let column = cell_edge
                .and_then(|edge| edge.column)
                .and_then(|index| Some((index, *base.0.get(index)?)));
            let row = cell_edge
                .and_then(|edge| edge.row)
                .and_then(|index| Some((index, *base.1.get(index)?)));
            // Where the first frame is now, measured from the image edge: read
            // from what was drawn, so that a legacy graph (whose layout stores
            // no frame) starts from the plot area it is drawn with.
            let first = self.last.as_ref().and_then(|plot| {
                plot.panels
                    .iter()
                    .find(|panel| panel.row == 0 && panel.column == 0)
                    .map(|panel| panel.pixel_origin)
            });
            let placeable = !base.0.is_empty() && !base.1.is_empty() && first.is_some();
            let left = figure_edge
                .filter(|edge| edge.left && placeable)
                .and_then(|_| first.map(|origin| f64::from(origin.0)));
            let top = figure_edge
                .filter(|edge| edge.top && placeable)
                .and_then(|_| first.map(|origin| f64::from(origin.1)));
            let Some(scale) = self.display_scale().filter(|scale| *scale > 0.0) else {
                return false;
            };
            if column.is_none() && row.is_none() && left.is_none() && top.is_none() {
                return false;
            }
            self.frame_drag = Some(FrameDrag {
                column,
                row,
                left,
                top,
                press: origin,
                scale,
                base,
            });
        }
        let Some(drag) = self.frame_drag.clone() else {
            return false;
        };
        let delta = (pos - drag.press) / drag.scale;
        let bounds = |size: f64| {
            size.round().clamp(
                stokesforge_plot::MIN_FRAME_SIZE,
                stokesforge_plot::MAX_FRAME_SIZE,
            )
        };
        // Pulling the left or top edge of the image outwards (towards smaller
        // coordinates) gives the frame more room on that side, and the other
        // way round.
        let (min_left, min_top) = self.figure_memo.as_ref().map_or((0.0, 0.0), |(_, layout)| {
            (f64::from(layout.min_left), f64::from(layout.min_top))
        });
        let offset = |start: f64, moved: f32, minimum: f64| {
            (start - f64::from(moved))
                .round()
                .clamp(minimum, stokesforge_plot::MAX_FRAME_SIZE)
        };
        self.set_frame_preview(Some(FramePreview {
            column: drag
                .column
                .map(|(index, start)| (index, bounds(start + f64::from(delta.x)))),
            row: drag
                .row
                .map(|(index, start)| (index, bounds(start + f64::from(delta.y)))),
            left: drag.left.map(|start| offset(start, delta.x, min_left)),
            top: drag.top.map(|start| offset(start, delta.y, min_top)),
            base: drag.base,
            free: None,
        }));
        true
    }

    /// The dashed guides of the magnets painted over the figure while a layer
    /// is moved or resized (#398); empty otherwise.
    pub fn magnet_guides(&self) -> &[stokesforge_render::Guide] {
        self.free_drag
            .as_ref()
            .map_or(&[], |drag| drag.guides.as_slice())
    }

    /// The cursor of the layer gestures of a free figure (#398): the move
    /// cursor while a layer moves, a resize cursor while an edge is pulled and
    /// over an edge or a corner. `None` elsewhere.
    fn free_cursor(&self, response: &Response) -> Option<egui::CursorIcon> {
        let handle = match &self.free_drag {
            Some(drag) => drag.handle,
            None if response.hovered() && self.layout_is_free() => {
                let pos = self.figure_position(response.hover_pos()?)?;
                self.free_handle_at(pos)
                    .filter(|handle| !handle.is_move())?
            }
            None => return None,
        };
        Some(
            match (handle.left || handle.right, handle.top || handle.bottom) {
                (false, false) => egui::CursorIcon::Move,
                (true, false) => egui::CursorIcon::ResizeHorizontal,
                (false, true) => egui::CursorIcon::ResizeVertical,
                (true, true) if handle.left == handle.top => egui::CursorIcon::ResizeNwSe,
                (true, true) => egui::CursorIcon::ResizeNeSw,
            },
        )
    }

    /// The handle of a layer of the free figure under the figure position
    /// `pos` ([`stokesforge_render::free_handle_at`]), the edge strip being
    /// the same width on screen whatever the enlargement.
    fn free_handle_at(&self, pos: Pos2) -> Option<stokesforge_render::FreeHandle> {
        let (_, layout) = self.figure_memo.as_ref()?;
        let tolerance = stokesforge_render::CELL_EDGE_TOLERANCE_POINTS
            / f64::from(self.display_scale().filter(|scale| *scale > 0.0)?);
        stokesforge_render::free_handle_at(layout, f64::from(pos.x), f64::from(pos.y), tolerance)
    }

    /// Moving or resizing a layer of a free figure (#398): a primary drag
    /// **starting on an edge of a layer** (`FreeStart::Edges`) resizes its
    /// frame, one starting in its plot area (`FreeStart::Move`, tried after the
    /// handlers of texts, shapes, titles and legends) moves it. The preview
    /// follows the pointer, with the magnets (Shift held: none); at release a
    /// single [`GraphEvent::LayerFramed`]. A gesture in progress is continued
    /// by the first call whatever its kind. Returns `true` if the gesture is a
    /// layer gesture.
    fn handle_free_drag(
        &mut self,
        response: &Response,
        context: &egui::Context,
        spec: &PlotSpec,
        start: FreeStart,
    ) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            let Some(drag) = self.free_drag.take() else {
                return false;
            };
            match self.frame_preview.clone().and_then(|preview| preview.free) {
                Some((panel, frame))
                    if (frame.left - drag.start.left).abs() >= 0.005
                        || (frame.top - drag.start.top).abs() >= 0.005
                        || (frame.width - drag.start.width).abs() >= 0.005
                        || (frame.height - drag.start.height).abs() >= 0.005 =>
                {
                    self.events.push(GraphEvent::LayerFramed {
                        panel,
                        frame,
                        resized: !drag.handle.is_move(),
                    });
                    self.hold_preview();
                }
                _ => self.set_frame_preview(None),
            }
            return true;
        }
        if !response.dragged_by(PointerButton::Primary) || !self.layout_is_free() {
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.free_drag.is_some();
        };
        if self.free_drag.is_none() {
            let origin = context
                .input(|input| input.pointer.press_origin())
                .unwrap_or(pos - response.drag_delta());
            let Some(handle) = self
                .figure_position(origin)
                .and_then(|figure| self.free_handle_at(figure))
            else {
                return false;
            };
            let wanted = match start {
                FreeStart::Edges => !handle.is_move(),
                // The axis line lies in the frame: a drag from it does nothing.
                FreeStart::Move => {
                    handle.is_move()
                        && matches!(
                            self.component_at(origin),
                            Some(PlotComponent::PlotArea { .. })
                        )
                }
            };
            let Some(scale) = self.display_scale().filter(|scale| *scale > 0.0 && wanted) else {
                return false;
            };
            let Some(first) = spec.panels.get(handle.base) else {
                return false;
            };
            let others = spec
                .free_draw_order()
                .into_iter()
                .filter(|base| *base != handle.base)
                .filter_map(|base| spec.panels.get(base))
                .map(|panel| panel.sanitized_free_frame())
                .collect();
            self.free_drag = Some(FreeDrag {
                handle,
                start: first.sanitized_free_frame(),
                others,
                press: origin,
                scale,
                guides: Vec::new(),
            });
        }
        let Some(drag) = self.free_drag.clone() else {
            return false;
        };
        let scale = f64::from(drag.scale);
        let delta = (pos - drag.press) / drag.scale;
        let shift = context.input(|input| input.modifiers.shift);
        let result = stokesforge_render::magnetized(
            drag.start,
            &drag.handle,
            (f64::from(delta.x), f64::from(delta.y)),
            &drag.others,
            stokesforge_render::MAGNET_THRESHOLD_POINTS / scale,
            !shift,
        );
        if let Some(active) = self.free_drag.as_mut() {
            active.guides = result.guides;
        }
        self.set_frame_preview(Some(FramePreview {
            column: None,
            row: None,
            left: None,
            top: None,
            base: (Vec::new(), Vec::new()),
            free: Some((drag.handle.base, result.frame)),
        }));
        true
    }

    /// Dragging a free text (issue #35): a primary drag **starting from a
    /// text** moves it instead of scrolling the view. The preview follows the
    /// pointer; at release, a single [`GraphEvent::AnnotationMoved`].
    /// Returns `true` if the gesture is a text drag (navigation must then do
    /// nothing with it).
    fn handle_annotation_drag(&mut self, response: &Response, spec: &PlotSpec) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            if let Some(drag) = self.annotation_drag.take() {
                if let Some((target, annotation)) = self.annotation_preview.clone() {
                    if annotation != drag.original {
                        self.events
                            .push(GraphEvent::AnnotationMoved { target, annotation });
                        self.hold_preview();
                    } else {
                        self.set_annotation_preview(None);
                    }
                }
                return true;
            }
            return false;
        }
        if !response.dragged_by(PointerButton::Primary) {
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.annotation_drag.is_some();
        };
        if self.annotation_drag.is_none() {
            // The real start point, as for the rubber-band rectangle.
            let origin = pos - response.drag_delta();
            let handle = self
                .selected_annotation
                .filter(|target| self.width_handle_at(*target, origin));
            let Some(target) = handle.or_else(|| self.annotation_at(origin)) else {
                return false;
            };
            let (Some(original), Some(drawn), Some(press)) = (
                target.get(spec).cloned(),
                self.drawn_annotation(target).copied(),
                self.figure_position(origin),
            ) else {
                return false;
            };
            self.selected_series = None;
            self.selected_annotation = Some(target);
            self.selected_text = None;
            self.selected_shape = None;
            self.annotation_drag = Some(AnnotationDrag {
                target,
                original,
                start_center: Pos2::new(drawn.center.0 as f32, drawn.center.1 as f32),
                press,
                resize: handle.is_some(),
            });
        }
        let Some(drag) = self.annotation_drag.clone() else {
            return false;
        };
        let Some(current) = self.figure_position(pos) else {
            return true;
        };
        if drag.resize {
            // The pulled handle sets the half-width: its distance to the
            // center, measured along the (rotated) text, as a fraction of the figure.
            let Some(figure) = self.cache.key().map(|key| key.rect.width()) else {
                return true;
            };
            let (sin, cos) = drag.original.effective_rotation().to_radians().sin_cos();
            let along = (current - drag.start_center).dot(egui::vec2(cos as f32, -sin as f32));
            let fraction =
                f64::from(2.0 * along.abs() / figure).clamp(AnnotationSpec::MIN_WIDTH, 1.0);
            self.set_annotation_preview(Some((
                drag.target,
                AnnotationSpec {
                    width: Some(fraction),
                    ..drag.original
                },
            )));
            return true;
        }
        let center = drag.start_center + (current - drag.press);
        let right = drag.original.axis == SeriesAxis::Right;
        let moved = self
            .last
            .as_ref()
            .and_then(|last| {
                last.panels
                    .iter()
                    .find(|panel| panel.index == drag.target.panel)
            })
            .and_then(|panel| PanelTransform::of_axis(panel, right))
            .and_then(|transform| transform.pixel_to_data(center.x as f64, center.y as f64));
        if let Some((x, y)) = moved {
            let annotation = AnnotationSpec {
                x,
                y,
                ..drag.original.clone()
            };
            self.set_annotation_preview(Some((drag.target, annotation)));
        }
        true
    }

    /// Dragging a title or the legend (issue #53): a primary drag **starting
    /// from a title** moves it instead of scrolling the view. The preview
    /// follows the pointer; at release, a single
    /// [`GraphEvent::TextMoved`]. Same principle as
    /// [`Self::handle_annotation_drag`] — the center of the text follows the
    /// pointer, converted to panel data (so anchored to the data, it then
    /// follows the zoom).
    fn handle_text_drag(&mut self, response: &Response, spec: &PlotSpec) -> bool {
        if response.drag_stopped_by(PointerButton::Primary) {
            if let Some(drag) = self.text_drag.take() {
                if drag.resize {
                    match self.text_width_preview {
                        Some((target, width)) => {
                            self.events.push(GraphEvent::TextResized { target, width });
                            self.hold_preview();
                        }
                        None => self.set_text_width_preview(None),
                    }
                    return true;
                }
                match self.text_preview {
                    Some((target, anchor)) if drag.original != Some(anchor) => {
                        self.events.push(GraphEvent::TextMoved { target, anchor });
                        self.hold_preview();
                    }
                    _ => self.set_text_preview(None),
                }
                return true;
            }
            return false;
        }
        if !response.dragged_by(PointerButton::Primary) {
            return false;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return self.text_drag.is_some();
        };
        if self.text_drag.is_none() {
            let origin = pos - response.drag_delta();
            let handle = self
                .selected_text
                .filter(|target| self.text_width_handle_at(*target, origin));
            let Some(target) = handle.or_else(|| self.text_target_at(origin)) else {
                return false;
            };
            self.selected_series = None;
            self.selected_text = Some(target);
            self.selected_annotation = None;
            self.selected_shape = None;
            let (Some(drawn), Some(press)) = (
                self.drawn_text(target).copied(),
                self.figure_position(origin),
            ) else {
                return false;
            };
            self.text_drag = Some(TextDrag {
                target,
                original: target.anchor(spec),
                start_center: Pos2::new(drawn.center.0 as f32, drawn.center.1 as f32),
                press,
                resize: handle.is_some(),
            });
        }
        let Some(drag) = self.text_drag.clone() else {
            return false;
        };
        let Some(current) = self.figure_position(pos) else {
            return true;
        };
        if drag.resize {
            let (Some(figure), Some(drawn)) = (
                self.cache.key().map(|key| key.rect.width()),
                self.drawn_text(drag.target).copied(),
            ) else {
                return true;
            };
            let (sin, cos) = drawn.rotation.to_radians().sin_cos();
            let along = (current - drag.start_center).dot(egui::vec2(cos as f32, -sin as f32));
            let fraction =
                f64::from(2.0 * along.abs() / figure).clamp(AnnotationSpec::MIN_WIDTH, 1.0);
            self.set_text_width_preview(Some((drag.target, fraction)));
            return true;
        }
        let center = drag.start_center + (current - drag.press);
        // The frame of the anchor: the panel of the text, or the first drawn
        // panel for the graph title; the right ordinate if the existing
        // anchor was already on it.
        let right = drag
            .original
            .is_some_and(|anchor| anchor.axis == SeriesAxis::Right);
        let panel = self
            .last
            .as_ref()
            .and_then(|last| match drag.target.panel_index_of() {
                None => last.panels.first(),
                Some(index) => last.panels.iter().find(|panel| panel.index == index),
            });
        let moved = panel
            .and_then(|panel| PanelTransform::of_axis(panel, right))
            .and_then(|transform| transform.pixel_to_data(center.x as f64, center.y as f64));
        if let Some((x, y)) = moved {
            let axis = if right {
                SeriesAxis::Right
            } else {
                SeriesAxis::Left
            };
            self.set_text_preview(Some((drag.target, TextAnchor { x, y, axis })));
        }
        true
    }

    /// Crosshair, point designation and rubber-band zoom rectangle —
    /// tasks 7.2 to 7.4. Painted directly on `painter`, **never** in the
    /// cache's `Vec<Shape>`: this is what replaces the `copy_from_bbox`
    /// blitting of the Python version (see `crate::cache`).
    fn paint_overlay(
        &self,
        painter: &Painter,
        response: &Response,
        tree: &Tree,
        spec: &PlotSpec,
    ) -> Option<PointHit> {
        if let Some((start, end)) = self.drag_box {
            let rect = Rect::from_two_pos(start, end);
            painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(90, 150, 255, 40));
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
                StrokeKind::Outside,
            );
        }

        // The magnets' guides of a layer being moved or resized (#398): dashed
        // lines over the figure, screen only like the zoom rectangle.
        if let (Some(drag), Some((_, layout)), Some(display), Some(scale)) = (
            &self.free_drag,
            &self.figure_memo,
            self.display_rect,
            self.display_scale(),
        ) {
            let (origin_x, origin_y) = layout.free_origin;
            let band = f64::from(layout.title_band);
            let to_screen = |x: f64, y: f64| {
                display.min
                    + egui::vec2((x - origin_x) as f32, (y - origin_y + band) as f32) * scale
            };
            let stroke = Stroke::new(1.0, Color32::from_rgb(232, 62, 140));
            for guide in &drag.guides {
                let ends = if guide.vertical {
                    [
                        to_screen(guide.at, guide.from),
                        to_screen(guide.at, guide.to),
                    ]
                } else {
                    [
                        to_screen(guide.from, guide.at),
                        to_screen(guide.to, guide.at),
                    ]
                };
                painter.extend(egui::Shape::dashed_line(&ends, stroke, 5.0, 3.0));
            }
        }

        // The selected free text: its rotated rectangle (issue #35).
        if let Some((center, size, rotation)) = self
            .selected_annotation
            .and_then(|target| self.annotation_screen_box(target))
        {
            let (sin, cos) = rotation.to_radians().sin_cos();
            let half = size / 2.0 + egui::vec2(3.0, 3.0);
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
                let (u, v) = (sx * half.x, sy * half.y);
                // Counterclockwise rotation on screen (y axis pointing down).
                center + egui::vec2(u * cos + v * sin, -u * sin + v * cos)
            });
            painter.add(Shape::closed_line(
                corners.to_vec(),
                Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
            ));
            // The width handles (#244), at the middles of the left and right
            // edges.
            if let Some(handles) = self
                .selected_annotation
                .and_then(|target| self.width_handles(target))
            {
                paint_width_handles(painter, handles);
            }
        }
        if let Some(handles) = self
            .selected_text
            .and_then(|target| self.text_width_handles(target))
        {
            paint_width_handles(painter, handles);
        }

        if let Some((center, size, rotation)) = self
            .selected_text
            .and_then(|target| self.text_screen_box(target))
        {
            let (sin, cos) = rotation.to_radians().sin_cos();
            let half = size / 2.0 + egui::vec2(3.0, 3.0);
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
                let (u, v) = (sx * half.x, sy * half.y);
                center + egui::vec2(u * cos + v * sin, -u * sin + v * cos)
            });
            painter.add(Shape::closed_line(
                corners.to_vec(),
                Stroke::new(1.0, Color32::from_rgb(90, 150, 255)),
            ));
        }

        self.paint_shape_selection(painter);
        self.paint_series_selection(painter, spec);

        let pos = response.hover_pos()?;
        let figure_pos = self.figure_position(pos)?;
        let (panel_index, transform) = self.panel_at(pos)?;
        let last = self.last.as_ref()?;
        let panel = last.panels.get(panel_index)?;
        let figure_rect = self.display_rect?;
        let scale = self.display_scale()?;
        let panel_rect = Rect::from_min_size(
            figure_rect.min
                + egui::vec2(panel.pixel_origin.0 as f32, panel.pixel_origin.1 as f32) * scale,
            egui::vec2(panel.pixel_size.0 as f32, panel.pixel_size.1 as f32) * scale,
        );
        if !panel_rect.contains(pos) {
            return None;
        }

        let stroke = Stroke::new(1.0, Color32::from_gray(150));
        painter.line_segment(
            [
                Pos2::new(pos.x, panel_rect.top()),
                Pos2::new(pos.x, panel_rect.bottom()),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(panel_rect.left(), pos.y),
                Pos2::new(panel_rect.right(), pos.y),
            ],
            stroke,
        );

        let mut hit = nearest_point(tree, spec, panel_index, panel, &transform, figure_pos)?;
        hit.pixel = figure_rect.min + hit.pixel.to_vec2() * scale;
        hit.distance *= scale;
        painter.circle_stroke(
            hit.pixel,
            5.0 * scale,
            Stroke::new(1.5 * scale, Color32::from_rgb(220, 90, 90)),
        );
        Some(hit)
    }
}

/// The designated point of panel `panel_index` under the pixel `pos`: the egui
/// adapter of `stokesforge_render::interact::nearest_point`, which carries
/// the `f64` computation (#262). Only the `Pos2` ↔ `(f64, f64)` conversions
/// and the designation radius ([`HIT_RADIUS`]) remain here.
fn nearest_point(
    tree: &Tree,
    spec: &PlotSpec,
    panel_index: usize,
    drawn_panel: &DrawnPanel,
    transform: &PanelTransform,
    pos: Pos2,
) -> Option<PointHit> {
    let hit = interact::nearest_point(
        tree,
        spec,
        drawn_panel,
        transform,
        (pos.x as f64, pos.y as f64),
        HIT_RADIUS as f64,
    )?;
    Some(PointHit {
        panel: panel_index,
        series: hit.series,
        point_index: hit.point_index,
        data: hit.data,
        pixel: Pos2::new(hit.pixel.0 as f32, hit.pixel.1 as f32),
        distance: hit.distance as f32,
    })
}

/// The input field of a free text being edited (issue #37): where it is
/// placed and at what writing size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnnotationField {
    /// The rectangle of the field, on screen. Always **straight**: the
    /// rotation of the text does not enter into it (see [`annotation_field`]).
    pub rect: egui::Rect,
    /// The font size of the field, in egui points — that of the rendered text.
    pub font_size: f32,
}

/// Width reserve of a field on screen, in multiples of its body size, so that
/// a very short (or empty) text leaves room for input.
const FIELD_MIN_WIDTH_EM: f32 = 6.0;
/// Margin around the text, in multiples of the body size: the height of an
/// input line, a little more than the text alone.
const FIELD_PADDING_EM: f32 = 0.3;

/// Where to place the input field of a free text, so that one writes **inside
/// the frame of the rendering** (issue #37).
///
/// - **centered** on `center`, the center of the rendered text;
/// - at the **font size** of the text: its body size (`annotation_size`, in
///   the Plotters sense, like `AnnotationSpec::effective_size`) brought back
///   to the epaint body size, times `scale`, the display scale of the figure;
/// - **without rotation**: the field stays straight, whatever that of the
///   text, so nothing here takes it as a parameter;
/// - at least as large as the rendered straight text (`text_size`, on
///   screen), plus a margin and a minimum width so that an empty text stays
///   editable.
///
/// Pure function: the final size differs a little after recompilation by
/// Typst (a `$…$` formula is composed differently from its source), the field
/// is not meant to predict it.
pub fn annotation_field(
    center: Pos2,
    text_size: egui::Vec2,
    annotation_size: f64,
    scale: f32,
) -> AnnotationField {
    let font_size = stokesforge_render::text::font_size(annotation_size) * scale;
    let padding = font_size * FIELD_PADDING_EM;
    let size = egui::vec2(
        (text_size.x + 2.0 * padding).max(font_size * FIELD_MIN_WIDTH_EM),
        (text_size.y + 2.0 * padding).max(font_size + 2.0 * padding),
    );
    AnnotationField {
        rect: egui::Rect::from_center_size(center, size),
        font_size,
    }
}

/// The `PlotSpec` of a node, cloned.
///
/// Cloned and not borrowed: the spec only carries identities and style
/// (`stokesforge-plot`: "no data"), so the copy is cheap, and borrowing
/// would force the `Arc<Tree>` to outlive the closure of the cache's
/// `FnOnce`.
fn graph_spec(tree: &Tree, graph: NodeId) -> Result<PlotSpec, RenderError> {
    let node = tree.node(graph).ok_or(RenderError::UnknownNode(graph))?;
    match node.kind() {
        NodeKind::Graph { spec, .. } => Ok(spec.clone()),
        _ => Err(RenderError::NotAGraph(graph)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::{PanelSpec, Panels};
    use stokesforge_render::view::Framing;

    /// #357: after layer 0 is deleted, the zoom of layer 1 stays on that layer
    /// (now 0), is never applied to another one, and the active layer follows.
    #[test]
    fn remapping_layers_moves_the_zoom_and_the_active_layer_with_their_layer() {
        let spec = PlotSpec {
            panels: Panels::new(vec![
                PanelSpec::at(0, 0),
                PanelSpec::at(0, 1),
                PanelSpec::at(0, 2),
            ]),
            ..PlotSpec::default()
        };
        let mut view = GraphView::new();
        let zoom = Framing::xy((1.0, 2.0), (3.0, 4.0));
        view.view.set_framing(2, zoom);
        view.set_active_panel(2, &spec);

        view.remap_layers(&[None, Some(0), Some(1)]);
        assert_eq!(view.view.framing(1), Some(zoom));
        assert_eq!(view.view.framing(2), None);
        let two = PlotSpec {
            panels: Panels::new(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]),
            ..PlotSpec::default()
        };
        assert_eq!(
            view.active_panel(&two),
            1,
            "the active layer moved with its layer"
        );

        // The active layer deleted: back to the first.
        view.remap_layers(&[Some(0), None]);
        assert_eq!(view.active_panel(&two), 0);
    }

    /// #398: the selection of other layers keeps click order, toggles, never
    /// lists the active layer's cell, stands for the base of an overlay, is
    /// empty on the grid and dropped when the layers are renumbered. Witness:
    /// letting the grid keep a selection, or listing the active layer.
    #[test]
    fn the_layer_selection_toggles_and_follows_the_placement() {
        let mut panels: Vec<PanelSpec> = (0..3).map(|column| PanelSpec::at(0, column)).collect();
        panels.push(PanelSpec::overlay_on(
            &panels[2],
            stokesforge_plot::OverlaySpec::TOP_X,
        ));
        let mut free = PlotSpec::empty("Layers").with_panels(Panels::new(panels));
        free.layer_placement = stokesforge_plot::LayerPlacement::Free;
        let mut view = GraphView::new();
        view.set_active_panel(1, &free);
        view.toggle_extra_layer(2, &free);
        view.toggle_extra_layer(0, &free);
        assert_eq!(view.extra_layers(&free), vec![2, 0], "click order");
        view.toggle_extra_layer(1, &free);
        assert_eq!(
            view.extra_layers(&free),
            vec![2, 0],
            "the reference is not listed"
        );
        view.toggle_extra_layer(2, &free);
        assert_eq!(
            view.extra_layers(&free),
            vec![0],
            "a second click removes it"
        );
        view.toggle_extra_layer(3, &free);
        assert_eq!(
            view.extra_layers(&free),
            vec![0, 2],
            "an overlay stands for its base cell"
        );
        view.set_active_panel(2, &free);
        assert_eq!(
            view.extra_layers(&free),
            vec![0],
            "the new reference leaves it"
        );

        let mut grid = free.clone();
        grid.layer_placement = stokesforge_plot::LayerPlacement::Grid;
        assert!(view.extra_layers(&grid).is_empty(), "nothing on the grid");
        let mut other = GraphView::new();
        other.toggle_extra_layer(1, &grid);
        assert!(
            other.extra_layers(&free).is_empty(),
            "no toggling on the grid"
        );

        view.remap_layers(&[Some(0), Some(1), Some(2), Some(3)]);
        assert!(view.extra_layers(&free).is_empty(), "renumbering drops it");
        view.toggle_extra_layer(0, &free);
        view.clear_extra_layers();
        assert!(view.extra_layers(&free).is_empty());
    }

    /// Issue #37: the input field is centered on the rendered text, at its
    /// body size (proportional to the size and to the display scale), and at
    /// least as large as it — the rotation does not enter into it.
    #[test]
    fn the_input_field_is_centered_and_sized_to_the_text() {
        let center = Pos2::new(240.0, 130.0);
        let text = egui::vec2(90.0, 20.0);
        let field = annotation_field(center, text, 16.0, 1.0);
        assert_eq!(field.rect.center(), center);
        assert!(field.rect.width() >= text.x && field.rect.height() >= text.y);
        assert_eq!(field.font_size, stokesforge_render::text::font_size(16.0));

        // The body size follows the size of the text…
        let large = annotation_field(center, text, 32.0, 1.0);
        assert!((large.font_size - 2.0 * field.font_size).abs() < 1e-4);
        // … and the display scale of the figure.
        let enlarged = annotation_field(center, text * 2.0, 16.0, 2.0);
        assert!((enlarged.font_size - 2.0 * field.font_size).abs() < 1e-4);
        assert_eq!(enlarged.rect.center(), center);
    }

    /// An empty (or very short) text leaves an input width; a long text makes
    /// the field grow on both sides, always centered.
    #[test]
    fn the_field_stays_editable_when_empty_and_follows_a_long_text() {
        let center = Pos2::new(100.0, 50.0);
        let empty = annotation_field(center, egui::Vec2::ZERO, 16.0, 1.0);
        assert!(empty.rect.width() >= 6.0 * empty.font_size);
        assert!(empty.rect.height() > empty.font_size);
        let long = annotation_field(center, egui::vec2(600.0, 20.0), 16.0, 1.0);
        assert!(long.rect.width() >= 600.0);
        assert_eq!(long.rect.center(), center);
    }

    /// Issue #34: `framed_spec` returns the spec framed by the zoom — what
    /// the figure shows — on a clone; the original spec, the model's, does
    /// not change. Without zoom, the spec is returned as is.
    #[test]
    fn framed_spec_applies_the_zoom_on_a_clone() {
        let spec = PlotSpec::empty("Zoom").with_panels(Panels::new(vec![PanelSpec::at(0, 0)]));
        let mut view = GraphView::new();
        assert_eq!(
            view.framed_spec(&spec),
            spec,
            "without zoom, nothing changes"
        );

        view.view
            .set_framing(0, Framing::xy((2.0, 3.0), (-1.0, 1.0)));
        let framed = view.framed_spec(&spec);
        let bottom = &framed.panels[0].bottom;
        assert_eq!((bottom.range_min, bottom.range_max), (Some(2.0), Some(3.0)));
        assert!(!bottom.auto_range);
        assert!(
            spec.panels[0].bottom.auto_range,
            "the original spec changed"
        );
    }
}

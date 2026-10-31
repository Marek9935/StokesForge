//! SVG export of a StokesForge graph — task R4.4.
//!
//! "Export is to `PlotSpec` what persistence was to the model: the only
//! thing that proves the neutral layer is really neutral"
//! (`roadmap-rendu.md`). This crate is that proof, which is why it is
//! written **at the same time** as the screen backend and not after.
//!
//! # Boundary
//!
//! This crate knows neither `egui`, nor `eframe`, nor `winit` — a test
//! checks it (`tests/no_ui_dependency.rs`), because it is the kind of
//! property that gets lost without anyone noticing. Yet it measures text
//! exactly like the screen: the measurement comes from `epaint`, pulled in by
//! `stokesforge-render`, the only UI stack an export needs.
//!
//! Practical consequence, and the argument of §9 of `roadmap-rendu.md`:
//! **rendering is tested without a graphics server**. The whole test suite of
//! this crate runs in continuous integration without a display.
//!
//! # PDF and PNG
//!
//! PDF export (`svg2pdf`, module [`pdf`]) is phase R6: a conversion of the
//! `String` that [`render_to_string`] already returns, not a third
//! backend — see the module documentation for what that implies.
//! PNG export (`resvg`, module [`png`], added after the roadmap) follows
//! exactly the same pattern: see the module documentation.
//!
//! # Typst — task R8.4
//!
//! [`typst_export`] compiles all rich text — titles, axis titles,
//! legend ([`stokesforge_render::formula::is_rich`], issue #32) —, places a
//! witness that is replaced afterwards by an inlined `typst-svg` fragment —
//! see the doc of [`typst_export`] for the full mechanism, detailed at the
//! top of that module. [`render_to_string_with_typst`] is the entry point
//! equivalent to [`render_to_string`], with Typst enabled.
//!
//! Typst has always been compiled since issue #29 (the `typst` feature was
//! removed). [`render_to_string`] and the PDF/PNG conversions built on it
//! still plot everything as plain text: wiring them to Typst is the job of
//! the export from the application (issue #34). The plain-text path
//! remains, as on the screen side, the witness of the Typst tests.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod backend;
pub mod export;
pub mod pdf;
pub mod png;
pub mod subpixel;
pub mod typst_export;
mod typst_text;

use std::path::Path;

use plotters::prelude::{DrawingAreaErrorKind, SVGBackend};
use stokesforge_core::resolve::{resolve_spec, ResolvedPlot};
use stokesforge_core::Tree;
use stokesforge_plot::PlotSpec;
use stokesforge_render::pipeline::{self, DrawnPlot};
use stokesforge_render::text::TextMeasurer;

pub use backend::MeasuredSvgBackend;
pub use export::{
    export_bytes, export_bytes_many, export_bytes_many_with_texts, export_bytes_with_texts,
    export_to_file, export_to_file_with_texts, ExportError, ExportFormat,
};
pub use typst_export::SvgTextMode;
// The export's font choice, so that a caller without egui (the application
// state) does not have to declare an edge to `stokesforge-render`.
pub use pdf::{document_to_pdf, render_to_pdf, render_to_pdf_file, PdfError};
pub use png::{document_to_png, render_to_png, render_to_png_file, PngError};
pub use stokesforge_render::pipeline::{FittedFigure, FittedFrames, SizeAxis};
pub use stokesforge_render::text::{font_definitions, TextMeasurer as SvgTextMeasurer};
pub use stokesforge_render::{FigureTexts, FontChoice};

/// Size of the figure of a graph that records no cell size and has no title,
/// in SVG units: [`pipeline::DEFAULT_FIGURE_SIZE`]. **Not the size to export
/// with** since #394: the figure's size comes from its cells, use
/// [`figure_layout`] / [`figure_size`]. Every exporting function of this module
/// still takes `size` as a parameter.
pub const DEFAULT_SIZE: (u32, u32) = pipeline::DEFAULT_FIGURE_SIZE;

/// Runs `run` on a throwaway backend that measures text the way the export
/// does, Typst included (invariant 4). Typst is only built when a title is
/// there to measure.
fn with_probe<R, E>(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
    run: impl for<'b> FnOnce(
        &plotters::drawing::DrawingArea<MeasuredSvgBackend<'b>, plotters::coord::Shift>,
    ) -> Result<R, E>,
) -> Result<R, SvgError>
where
    SvgError: From<E>,
{
    use plotters::drawing::IntoDrawingArea;
    let mut buffer = String::new();
    let titled = (spec.title_visible && !spec.title.is_empty())
        || spec
            .panels
            .iter()
            .any(|panel| panel.title_visible && !panel.title.is_empty());
    if !titled {
        let probe =
            MeasuredSvgBackend::with_string(&mut buffer, (1, 1), measurer).into_drawing_area();
        return Ok(run(&probe)?);
    }
    let cache = stokesforge_render::formula::FormulaCache::new();
    let mode = SvgTextMode::Text;
    let typst = typst_export::TypstExport::new(&cache, choice, mode).map_err(SvgError::Typst)?;
    let probe = MeasuredSvgBackend::with_string(&mut buffer, (1, 1), measurer)
        .with_typst(&typst)
        .with_font(choice, mode)
        .into_drawing_area();
    Ok(run(&probe)?)
}

/// The layout of the figure of `spec` (#394), the titles measured with the
/// export's own measurement, Typst included (invariant 4): the page of an
/// export is sized by this, on a throwaway backend, before the figure is drawn.
/// Screen and export agree because both read [`pipeline::figure_layout`].
pub fn figure_layout(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
) -> Result<pipeline::FigureLayout, SvgError> {
    with_probe(spec, measurer, choice, |probe| {
        pipeline::figure_layout(spec, probe)
    })
}

/// The frame sizes a legacy graph (no stored frame) is drawn with, in points
/// (see [`pipeline::legacy_frames`]), measured like the export: what the
/// application and `pyst` store before they replace a legacy spec (#394, G7).
/// `None` for a graph that already stores its frames.
pub fn legacy_frames(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
) -> Result<Option<pipeline::FrameSizes>, SvgError> {
    with_probe(spec, measurer, choice, |probe| {
        pipeline::legacy_frames(spec, probe)
    })
}

/// Gives a legacy `target` (no stored frame) the frames `reference` is drawn
/// with (#394, G7): called before a legacy spec is replaced, `reference` being
/// the spec as it is now, so that the plot areas stay exactly what they were
/// and only what the edit changes moves. Nothing happens, and `false` comes
/// back, if `target` already stores frames, if `reference` has none to give (it
/// stores its own) or if the two do not have the same grid (a cell added or
/// deleted: the caller fixes the frames before editing the grid). A graph
/// created legacy fixes itself: `reference` is a copy of `target`.
pub fn fix_legacy_frames(
    reference: &PlotSpec,
    target: &mut PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
) -> Result<bool, SvgError> {
    if target.layer_placement == stokesforge_plot::LayerPlacement::Free
        || !(target.frame_widths.is_empty() && target.frame_heights.is_empty())
        || (target.rows(), target.columns()) != (reference.rows(), reference.columns())
    {
        return Ok(false);
    }
    let Some((widths, heights)) = legacy_frames(reference, measurer, choice)? else {
        return Ok(false);
    };
    target.frame_widths = widths;
    target.frame_heights = heights;
    Ok(true)
}

/// The free frames a graph on the grid would keep in free placement (#398),
/// measured like the export; see [`pipeline::free_frames_from_grid`]. `None`
/// for a graph that is free already, legacy or too large a grid.
pub fn free_frames_from_grid(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
) -> Result<Option<Vec<Option<stokesforge_plot::FreeFrame>>>, SvgError> {
    with_probe(spec, measurer, choice, |probe| {
        pipeline::free_frames_from_grid(spec, probe)
    })
}

/// Switches `spec` between the regular grid and free placement (#398, #411),
/// the one rule of Graph Properties and `pyst.set_graph`.
///
/// To free: a legacy graph is given its frames first, then every layer takes
/// the frame the grid draws it with, so nothing moves. To the grid:
/// [`stokesforge_core::compose::set_grid_placement`] (L7 then C4). `false`,
/// with `spec` unchanged, if it is already in that placement or the
/// conversion is refused.
pub fn set_layer_placement(
    spec: &mut PlotSpec,
    free: bool,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
) -> bool {
    use stokesforge_plot::LayerPlacement;
    let mut candidate = spec.clone();
    if free {
        if candidate.layer_placement == LayerPlacement::Free {
            return false;
        }
        let reference = spec.clone();
        let _ = fix_legacy_frames(&reference, &mut candidate, measurer, choice);
        let Ok(Some(frames)) = free_frames_from_grid(&candidate, measurer, choice) else {
            return false;
        };
        if !stokesforge_core::compose::set_free_placement(&mut candidate, &frames) {
            return false;
        }
    } else if !stokesforge_core::compose::set_grid_placement(&mut candidate) {
        return false;
    }
    *spec = candidate;
    true
}

/// The frame of a layer added beside layer `panel` of a free graph (#398, L8),
/// measured like the export; see [`pipeline::place_beside`].
pub fn place_beside(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
    panel: usize,
    side: pipeline::BesideSide,
) -> Result<Option<stokesforge_plot::FreeFrame>, SvgError> {
    with_probe(spec, measurer, choice, |probe| {
        pipeline::place_beside(spec, probe, panel, side)
    })
}

/// The new frames of the cells `targets` spread with equal gaps between their
/// outer rectangles (#398, L4), measured like the export; see
/// [`pipeline::distribute_frames`].
pub fn distribute_frames(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
    targets: &[usize],
    axis: pipeline::DistributeAxis,
) -> Result<Option<pipeline::CellFrames>, SvgError> {
    with_probe(spec, measurer, choice, |probe| {
        pipeline::distribute_frames(spec, probe, targets, axis)
    })
}

/// The frames that give the figure of `spec` a total size along `axis`
/// (#407), measured like the export; see [`pipeline::fit_figure_size`].
pub fn fit_figure_size(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
    axis: pipeline::SizeAxis,
    target_pt: f64,
) -> Result<Option<pipeline::FittedFigure>, SvgError> {
    with_probe(spec, measurer, choice, |probe| {
        pipeline::fit_figure_size(spec, probe, axis, target_pt)
    })
}

/// [`figure_layout`]'s size, or the legacy default if the measurement fails
/// (it practically cannot).
pub fn figure_size(
    spec: &PlotSpec,
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
) -> (u32, u32) {
    figure_layout(spec, measurer, choice).map_or(DEFAULT_SIZE, |layout| layout.size)
}

/// What can go wrong during an export.
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    /// Plotters refused to draw. Practically unreachable with the SVG
    /// backend, which writes into a `String` — but the error exists in
    /// Plotters' signature and silencing it with an `unwrap` would be worse.
    #[error("drawing failed: {0}")]
    Drawing(String),
    /// The file could not be written.
    #[error("could not write {path}: {source}")]
    Write {
        /// Target path.
        path: String,
        /// System cause.
        source: std::io::Error,
    },
    /// Typst font resolution failed — task R8.4. Affects only
    /// [`render_to_string_with_typst`]: the embedded fonts being checked
    /// valid by the tests of `stokesforge_render::fonts`, it is practically
    /// reachable only for a requested system font whose reading would fail
    /// in a way that [`stokesforge_render::fonts::resolve`] does not already
    /// meet itself (silent fallback to Noto).
    #[error("could not resolve Typst fonts: {0}")]
    Typst(String),
}

impl<E: std::error::Error + Send + Sync> From<DrawingAreaErrorKind<E>> for SvgError {
    fn from(error: DrawingAreaErrorKind<E>) -> Self {
        Self::Drawing(error.to_string())
    }
}

/// Exports an **already resolved** graph and returns the SVG document.
///
/// Takes the [`TextMeasurer`] as a parameter rather than building one:
/// building it decodes the embedded fonts, and above all the caller must be
/// able to prove — this is the whole point of the agreement test — that it
/// measures with the same thing as the screen.
pub fn render_to_string(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<String, SvgError> {
    render_to_string_with_texts(plot, size, measurer, &FigureTexts::default())
}

/// [`render_to_string`] with the texts of the figure in the caller's language
/// ([`FigureTexts`]); the screen must be given the same ones (invariant 5).
pub fn render_to_string_with_texts(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    texts: &FigureTexts,
) -> Result<String, SvgError> {
    let mut buffer = String::new();
    {
        let backend = MeasuredSvgBackend::with_string(&mut buffer, size, measurer);
        pipeline::draw_with_texts(backend, plot, texts)?;
    }
    Ok(subpixel::resolve(buffer))
}

/// Like [`render_to_string`], with the Typst path of task R8.4 enabled
/// for rich text ([`stokesforge_render::formula::is_rich`]: titles,
/// axis titles, legend).
///
/// Additive entry point — [`render_to_string`] is untouched, task
/// 8.6 — builds its own [`stokesforge_render::formula::FormulaCache`]
/// internally: a caller that exports the same figure several times (for
/// example to compare sizes) therefore does not share this cache from one
/// call to the next, unlike [`stokesforge_render_egui::GraphView`] on the
/// screen side, which lives longer than an export. Not a problem for an
/// export, which is one-off by nature.
pub fn render_to_string_with_typst(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
    mode: SvgTextMode,
) -> Result<String, SvgError> {
    render_to_string_with_typst_and_texts(
        plot,
        size,
        measurer,
        choice,
        mode,
        &FigureTexts::default(),
    )
}

/// [`render_to_string_with_typst`] with the texts of the figure
/// ([`FigureTexts`]).
pub fn render_to_string_with_typst_and_texts(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    choice: &stokesforge_render::FontChoice,
    mode: SvgTextMode,
    texts: &FigureTexts,
) -> Result<String, SvgError> {
    let cache = stokesforge_render::formula::FormulaCache::new();
    let typst = typst_export::TypstExport::new(&cache, choice, mode).map_err(SvgError::Typst)?;
    let mut buffer = String::new();
    {
        let backend = MeasuredSvgBackend::with_string(&mut buffer, size, measurer)
            .with_typst(&typst)
            .with_font(choice, mode);
        pipeline::draw_with_texts(backend, plot, texts)?;
    }
    Ok(typst_export::splice(&subpixel::resolve(buffer), &typst))
}

/// Exports an already resolved graph, and also returns the geometry used.
///
/// Same as [`render_to_string`], but keeping the [`DrawnPlot`] that the
/// pipeline computed — what a test needs to compare something other than
/// text.
pub fn render_to_string_with_geometry(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<(String, DrawnPlot), SvgError> {
    render_to_string_with_geometry_and_texts(plot, size, measurer, &FigureTexts::default())
}

/// [`render_to_string_with_geometry`] with the texts of the figure
/// ([`FigureTexts`]).
pub fn render_to_string_with_geometry_and_texts(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    texts: &FigureTexts,
) -> Result<(String, DrawnPlot), SvgError> {
    let mut buffer = String::new();
    let drawn = {
        let backend = MeasuredSvgBackend::with_string(&mut buffer, size, measurer);
        pipeline::draw_with_texts(backend, plot, texts)?
    };
    Ok((subpixel::resolve(buffer), drawn))
}

/// Resolves a spec against a tree, then exports it.
///
/// The shortcut expected by a caller that has a project at hand rather
/// than a `ResolvedPlot`.
pub fn render_spec(
    tree: &Tree,
    spec: &PlotSpec,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<String, SvgError> {
    let plot = resolve_spec(tree, spec);
    render_to_string(&plot, size, measurer)
}

/// Writes the export to a file.
pub fn render_to_file(
    path: impl AsRef<Path>,
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<(), SvgError> {
    let path = path.as_ref();
    let document = render_to_string(plot, size, measurer)?;
    std::fs::write(path, document).map_err(|source| SvgError::Write {
        path: path.display().to_string(),
        source,
    })
}

/// What `plotters-svg` would have returned without the `estimate_text_size`
/// override — the naive heuristic of §4.4.
///
/// Exists only to serve as a **witness** in tests: a wrapper that
/// mistakenly delegated `estimate_text_size` to `SVGBackend` would return
/// exactly this value, and without a point of comparison the test would see
/// nothing. It is the same precaution as the positive witness of spike R0
/// 0.1 (a deliberately different font family, to prove that the measured
/// equality was not an artifact).
///
/// Reproduces `plotters/src/style/font/naive.rs`: monospace width estimated
/// from the **number of UTF-8 bytes**.
pub fn naive_estimate(text: &str, plotters_size: f64) -> (u32, u32) {
    let em = plotters_size / 1.24 / 1.24;
    let width = (em * 0.7 * text.len() as f64).round() as i32;
    let top = -(em.round() as i32);
    let bottom = (em * 0.24).round() as i32;
    (width.max(0) as u32, (bottom - top).max(0) as u32)
}

/// A bare SVG backend, without the override — **to be used only in a test**.
///
/// Exposed because the witness above is not enough on its own: comparing
/// two numbers does not prove that the whole layout moves. A test can thus
/// plot the same figure with the bare backend and see that the axes really
/// move — that is, that task R4.4 had a point.
pub fn plain_backend(buffer: &mut String, size: (u32, u32)) -> SVGBackend<'_> {
    SVGBackend::with_string(buffer, size)
}

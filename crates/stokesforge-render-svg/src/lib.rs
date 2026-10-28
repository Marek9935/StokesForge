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
pub use export::{export_bytes, export_bytes_many, export_to_file, ExportError, ExportFormat};
pub use typst_export::SvgTextMode;
// The export's font choice, so that a caller without egui (the application
// state) does not have to declare an edge to `stokesforge-render`.
pub use pdf::{document_to_pdf, render_to_pdf, render_to_pdf_file, PdfError};
pub use png::{document_to_png, render_to_png, render_to_png_file, PngError};
pub use stokesforge_render::text::{font_definitions, TextMeasurer as SvgTextMeasurer};
pub use stokesforge_render::FontChoice;

/// Default size of an exported figure, in SVG units.
///
/// Reuses [`pipeline::DEFAULT_FIGURE_SIZE`] rather than its own value:
/// the export must produce the same figure as the default screen. Every
/// exporting function of this module takes `size` as a parameter — that is
/// already the extension point for a configurable size, just not yet wired
/// to a UI.
pub const DEFAULT_SIZE: (u32, u32) = pipeline::DEFAULT_FIGURE_SIZE;

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
    let mut buffer = String::new();
    {
        let backend = MeasuredSvgBackend::with_string(&mut buffer, size, measurer);
        pipeline::draw(backend, plot)?;
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
    let cache = stokesforge_render::formula::FormulaCache::new();
    let typst = typst_export::TypstExport::new(&cache, choice, mode).map_err(SvgError::Typst)?;
    let mut buffer = String::new();
    {
        let backend = MeasuredSvgBackend::with_string(&mut buffer, size, measurer)
            .with_typst(&typst)
            .with_font(choice, mode);
        pipeline::draw(backend, plot)?;
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
    let mut buffer = String::new();
    let drawn = {
        let backend = MeasuredSvgBackend::with_string(&mut buffer, size, measurer);
        pipeline::draw(backend, plot)?
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

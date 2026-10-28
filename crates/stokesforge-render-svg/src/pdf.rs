//! PDF export of a StokesForge graph — task R6.1.
//!
//! "The PDF is a conversion of the SVG, not a third backend"
//! (`roadmap-rendu.md`, phase R6). This module draws nothing: it takes the
//! document already produced by [`crate::render_to_string`] — hence already
//! measured with the shared `epaint::Fonts`, hence already in agreement with
//! the screen — and runs it through `usvg` then `svg2pdf`, which comes from
//! the Typst organization (`architecture-choisie.md` §3 and §5).
//!
//! # A decision made while writing this module, not announced by the source
//! docs: loading the system fonts at conversion time
//!
//! Without any `fontdb`, `usvg` (hence `svg2pdf`) reports no error but
//! **silently drops all the text**: measured on the realistic graph of R5
//! (110 `<text>` tags), the PDF's content stream goes from 599 `Tj`
//! operators to zero, without any `Result` saying so. A PDF export without a
//! single title, axis label or tick is not a minor fidelity defect: it is a
//! mute graph. `document_to_pdf` therefore calls
//! `fontdb_mut().load_system_fonts()`, a simple walk of the file system that
//! assumes neither a graphics server nor a screen — the same constraint as
//! the rest of the work package.
//!
//! This **does not settle** open point no. 6 of `architecture-choisie.md`
//! §5.2 (whether to outline the export's text). The asymmetry this module
//! adds is new, and distinct from the one already known on the SVG side:
//! there, the reader of the file needs a compatible font; here, it is
//! the machine that **produces** the PDF that must have one, once, at
//! export time — after which `svg2pdf` embeds a subset of the chosen font
//! in the file, which then becomes self-contained. The PDF is thus **more**
//! portable than the SVG once produced (embedded, not referenced), but the
//! choice of font depends on the export machine: two exports of the same
//! graph on two machines may pick two different fonts for "sans-serif", with
//! visually different glyphs — without effect on the geometry, since the axes
//! are already placed by the `epaint` measurement before `svg2pdf` steps
//! in. An export machine with no font installed at all (a bare CI container,
//! for example) gets the silent behavior above: no error, no text.
//! Inlining outlined Typst fragments, the only way to make this choice
//! unnecessary, is phase R8.
//!
//! # Addition of 2026-09-13 — the embedded fonts as a safety net, below the
//! system fonts
//!
//! `load_system_fonts()` remains the first source: a normal export machine
//! keeps the behavior already described above, glyphs included.
//! But **if no system font satisfies the generic `sans-serif` family** — the
//! bare CI container of the previous paragraph —
//! [`ensure_embedded_fallback`] loads Noto Sans, embedded in the binary
//! (`stokesforge_render::fonts`, the same task that gave a choice to
//! [`stokesforge_render::text::font_definitions`] on the screen side), and
//! declares it as the generic `sans-serif` font of the `fontdb` database. An
//! export produced on a machine totally devoid of system fonts becomes
//! readable again, without depending on what is installed there.
//!
//! This moves no axis: the geometry is already fixed by the `epaint`
//! measurement before `document_to_pdf` runs (see the previous paragraph
//! and the R4.5/R5.6 agreement test) — only the rendering of the glyphs
//! depends on it, exactly as the choice between two system fonts already
//! did.
//!
//! `fontdb` is not added to the direct dependencies of this crate
//! (`tests/no_ui_dependency.rs`): it is already reached via
//! `svg2pdf::usvg::fontdb`, which `usvg` publicly re-exports — a path rather
//! than one more edge.

use std::path::Path;

use stokesforge_core::resolve::ResolvedPlot;
use stokesforge_render::text::TextMeasurer;
use stokesforge_render::FontSet;

use crate::{render_to_string, SvgError};

/// What can go wrong during a PDF export, in addition to what can already
/// go wrong during the SVG export it starts from.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// The SVG stage failed before even reaching the conversion.
    #[error(transparent)]
    Svg(#[from] SvgError),
    /// `usvg` could not re-parse the document that [`render_to_string`] just
    /// wrote.
    ///
    /// Should never happen in practice: the document comes from our own
    /// `SVGBackend`. If it did, it would be the sign that the SVG export
    /// has stopped being valid SVG — a more serious fact than the one this
    /// module exists to check, and one that deserves not to be swallowed.
    #[error("could not re-read the SVG with usvg: {0}")]
    Parse(String),
    /// `svg2pdf` refused to convert the tree.
    #[error("PDF conversion failed: {0}")]
    Conversion(String),
    /// The file could not be written.
    #[error("could not write {path}: {source}")]
    Write {
        /// Target path.
        path: String,
        /// System cause.
        source: std::io::Error,
    },
}

/// Re-parses an already rendered SVG document and converts it to PDF.
///
/// Separate from [`render_to_pdf`] because it is the point where a caller
/// that already has the SVG at hand (for example to also write it alongside,
/// or to compare the two as task 6.2 does) avoids having the figure plotted a
/// second time.
///
/// Loads the export machine's system fonts before converting — see the
/// module documentation: without that, `usvg` drops all the text without
/// the slightest error. Then completes with [`ensure_embedded_fallback`]
/// for the case where no system font would fit.
pub fn document_to_pdf(document: &str) -> Result<Vec<u8>, PdfError> {
    let mut options = svg2pdf::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    ensure_embedded_fallback(options.fontdb_mut());
    convert(document, &options)
}

/// The conversion itself, once the `Options` (and thus the font database)
/// are already decided.
///
/// Separate from [`document_to_pdf`] so that the tests of
/// [`ensure_embedded_fallback`] can choose a font database without going
/// through `load_system_fonts()` — thus reproducing, deterministically, an
/// export machine that would have no system font.
fn convert(document: &str, options: &svg2pdf::usvg::Options<'_>) -> Result<Vec<u8>, PdfError> {
    let tree = svg2pdf::usvg::Tree::from_str(document, options)
        .map_err(|err| PdfError::Parse(err.to_string()))?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|err| PdfError::Conversion(err.to_string()))
}

/// Guarantees that a `font-family="sans-serif"` text (the one written by
/// `stokesforge_render::pipeline::FONT_FAMILY`, the only family this crate
/// produces) always finds a font, even on a machine with no system font
/// installed.
///
/// Acts only if the generic `sans-serif` family does not already resolve to
/// anything in `fontdb`: a normal export machine, whose `load_system_fonts()`
/// has already found a suitable system font, is not affected — this
/// function only adds a safety net, never replaces a choice that already
/// works.
///
/// `pub(crate)`: [`crate::png`] reuses it as is rather than duplicating the
/// font policy — `resvg` (its dependency) pulls in the same version of
/// `usvg`, hence the same `fontdb::Database`, as `svg2pdf` here (see
/// `Cargo.lock` and `tests/no_ui_dependency.rs`).
pub(crate) fn ensure_embedded_fallback(fontdb: &mut svg2pdf::usvg::fontdb::Database) {
    ensure_generic_sans(fontdb);
    // SVGs in text mode (#162) name Typst's family
    // (`font-family="'Noto Sans', sans-serif"`): without these fonts in the
    // database, `usvg` silently drops the text instead of falling back.
    for set in FontSet::ALL {
        fontdb.load_font_data(set.sans_bytes().to_vec());
        fontdb.load_font_data(set.math_bytes().to_vec());
    }
}

fn ensure_generic_sans(fontdb: &mut svg2pdf::usvg::fontdb::Database) {
    let already_resolved = fontdb
        .query(&svg2pdf::usvg::fontdb::Query {
            families: &[svg2pdf::usvg::fontdb::Family::SansSerif],
            ..svg2pdf::usvg::fontdb::Query::default()
        })
        .is_some();
    if already_resolved {
        return;
    }
    // Noto Sans: the same default and fallback as on the screen side
    // (`stokesforge_render::fonts::FontSet::Noto`), so that the choice is not
    // arbitrary a second time.
    fontdb.load_font_data(FontSet::Noto.sans_bytes().to_vec());
    fontdb.set_sans_serif_family(FontSet::Noto.sans_family_name());
}

/// Exports an **already resolved** graph and returns the PDF document.
///
/// Same signature as [`render_to_string`], on the same path: it is not a
/// third backend, only what follows the existing SVG export.
pub fn render_to_pdf(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<Vec<u8>, PdfError> {
    let document = render_to_string(plot, size, measurer)?;
    document_to_pdf(&document)
}

/// Writes the PDF export to a file.
pub fn render_to_pdf_file(
    path: impl AsRef<Path>,
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<(), PdfError> {
    let path = path.as_ref();
    let bytes = render_to_pdf(plot, size, measurer)?;
    std::fs::write(path, bytes).map_err(|source| PdfError::Write {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sans_serif_query() -> svg2pdf::usvg::fontdb::Query<'static> {
        svg2pdf::usvg::fontdb::Query {
            families: &[svg2pdf::usvg::fontdb::Family::SansSerif],
            ..svg2pdf::usvg::fontdb::Query::default()
        }
    }

    /// Number of `Tj` operators (string display) in the content stream of
    /// the first page — the same measure as the one cited at the head of
    /// the module for the R6 bug (599 → 0).
    fn count_tj(pdf: &[u8]) -> usize {
        let document = lopdf::Document::load_mem(pdf).expect("the PDF must open without error");
        let (_, page_id) = document
            .get_pages()
            .into_iter()
            .next()
            .expect("at least one page");
        let content = document.get_page_content(page_id);
        content.windows(3).filter(|f| *f == b" Tj").count()
    }

    /// Witness, as in the module documentation: an empty font database
    /// (no `load_system_fonts()`) does not resolve the generic `sans-serif`
    /// family. Without this fact, [`ensure_embedded_fallback`] would have
    /// nothing to guarantee.
    #[test]
    fn an_empty_font_database_does_not_resolve_sans_serif() {
        let fontdb = svg2pdf::usvg::fontdb::Database::new();
        assert!(
            fontdb.query(&sans_serif_query()).is_none(),
            "invalid witness: a fresh database already resolves `sans-serif`"
        );
    }

    #[test]
    fn the_fallback_guarantees_a_sans_serif_resolution() {
        let mut fontdb = svg2pdf::usvg::fontdb::Database::new();
        ensure_embedded_fallback(&mut fontdb);
        assert!(
            fontdb.query(&sans_serif_query()).is_some(),
            "the fallback must guarantee a resolution of `sans-serif`"
        );
    }

    #[test]
    fn the_fallback_does_not_replace_a_system_font_already_present() {
        let mut fontdb = svg2pdf::usvg::fontdb::Database::new();
        fontdb.load_font_data(FontSet::Fira.sans_bytes().to_vec());
        fontdb.set_sans_serif_family(FontSet::Fira.sans_family_name());

        ensure_embedded_fallback(&mut fontdb);

        let id = fontdb
            .query(&sans_serif_query())
            .expect("must resolve after the fallback too");
        let face = fontdb.face(id).expect("face info");
        assert!(
            face.families
                .iter()
                .any(|(name, _)| name == FontSet::Fira.sans_family_name()),
            "the fallback replaced a system font that already worked"
        );
    }

    /// End to end, the reason for this module: an export produced on a
    /// machine with **no system font** — simulated here by never calling
    /// `load_system_fonts()` — keeps its text thanks to the fallback, whereas
    /// it would lose it entirely without it.
    #[test]
    fn an_export_with_no_system_font_keeps_its_text() {
        let sample = stokesforge_render::sample::minimal().expect("minimal graph");
        let plot = stokesforge_core::resolve::resolve_spec(sample.project.tree(), &sample.spec);
        let measurer = TextMeasurer::default();
        let document = crate::render_to_string(&plot, (400, 300), &measurer).expect("SVG export");

        // Without fallback: the font database contains nothing, exactly the
        // bug documented at the head of the module.
        let options_without_fallback = svg2pdf::usvg::Options::default();
        let pdf_without_fallback =
            convert(&document, &options_without_fallback).expect("conversion (without fallback)");
        assert_eq!(
            count_tj(&pdf_without_fallback),
            0,
            "invalid witness: the PDF carries text with no fallback and no \
             system font loaded"
        );

        // With fallback: the same document, the same absence of system font,
        // but with `ensure_embedded_fallback` in addition.
        let mut options_with_fallback = svg2pdf::usvg::Options::default();
        ensure_embedded_fallback(options_with_fallback.fontdb_mut());
        let pdf_with_fallback =
            convert(&document, &options_with_fallback).expect("conversion (with fallback)");
        assert!(
            count_tj(&pdf_with_fallback) > 0,
            "the fallback did not prevent the loss of the text"
        );
    }
}

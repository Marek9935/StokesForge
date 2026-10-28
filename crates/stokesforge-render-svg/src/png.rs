//! PNG export of a StokesForge graph — same pattern as [`crate::pdf`].
//!
//! "The PDF is a conversion of the SVG, not a third backend"
//! (`roadmap-rendu.md`, phase R6) holds word for word for the PNG: this module
//! draws nothing. It takes the document already produced by
//! [`crate::render_to_string`] — hence already measured with the shared
//! `epaint::Fonts` — and runs it through `usvg` then `resvg`, which comes from
//! the same family as `svg2pdf` (`resvg` is in fact already pulled in,
//! internally, by `svg2pdf` — see `Cargo.toml` and `Cargo.lock`, a single
//! version of `usvg` in the tree for both exports).
//!
//! # Resolution: a scale factor, not a pixel size
//!
//! [`document_to_png`] and [`render_to_png`] take a multiplicative
//! `scale: f32` applied to the size of the SVG document (itself set by
//! the `size` parameter of [`crate::render_to_string`], in SVG units). The
//! document stays vector until the moment of rasterization: asking for
//! `scale: 2.0` or `scale: 4.0` produces an image whose text and
//! strokes are sharp at that resolution, not a blurry enlargement of a
//! rendering frozen at a hard-wired pixel size. It is `resvg::render`,
//! called with a `Transform::from_scale(scale, scale)`, that does this
//! work — the graph's geometry (already laid out by the `epaint`
//! measurement) is not affected, only the pixel density of the final raster
//! changes.
//!
//! # Fonts: exactly the same policy as the PDF
//!
//! [`crate::pdf::ensure_embedded_fallback`] is reused as is
//! (`pub(crate)`) rather than duplicated: system fonts via
//! `fontdb::load_system_fonts()` first, then the embedded Noto safety net
//! (`stokesforge_render::fonts`) if no system font satisfies the generic
//! `sans-serif` family. See the documentation of [`crate::pdf`] for the
//! detail and history of this decision — it does not depend on the output
//! format, only on the use of `usvg` upstream.
//!
//! `resvg` is added to the direct dependencies of this crate
//! (`tests/no_ui_dependency.rs`), unlike `fontdb` for the PDF: here, it is
//! indeed `resvg`'s rasterization API (`resvg::render`,
//! `resvg::tiny_skia`) that is used directly, not just a type passed through
//! on the way.

use std::path::Path;

use stokesforge_core::resolve::ResolvedPlot;
use stokesforge_render::text::TextMeasurer;

use crate::pdf::ensure_embedded_fallback;
use crate::{render_to_string, SvgError};

/// What can go wrong during a PNG export, in addition to what can already
/// go wrong during the SVG export it starts from.
#[derive(Debug, thiserror::Error)]
pub enum PngError {
    /// The SVG stage failed before even reaching the conversion.
    #[error(transparent)]
    Svg(#[from] SvgError),
    /// `usvg` could not re-parse the document that [`render_to_string`] just
    /// wrote.
    ///
    /// Should never happen in practice — see [`crate::pdf::PdfError::Parse`],
    /// same reasoning.
    #[error("could not re-read the SVG with usvg: {0}")]
    Parse(String),
    /// The requested scale is unusable (not finite, zero or
    /// negative): `resvg` does not rasterize an image with no dimension.
    #[error("invalid scale: {0}")]
    InvalidScale(f32),
    /// The resulting size, once the scale is applied, does not fit
    /// in a rasterization canvas (zero width or height once
    /// rounded).
    #[error("invalid canvas size: {width}x{height}")]
    InvalidSize {
        /// Computed width, in pixels.
        width: u32,
        /// Computed height, in pixels.
        height: u32,
    },
    /// The PNG encoding of the raster failed.
    #[error("PNG encoding failed: {0}")]
    Encode(String),
    /// The file could not be written.
    #[error("could not write {path}: {source}")]
    Write {
        /// Target path.
        path: String,
        /// System cause.
        source: std::io::Error,
    },
}

/// Re-parses an already rendered SVG document and rasterizes it to PNG.
///
/// Separate from [`render_to_png`] for the same reason that
/// [`crate::pdf::document_to_pdf`] is separate from [`crate::pdf::render_to_pdf`]:
/// a caller that already has the SVG at hand avoids having the figure
/// plotted a second time.
///
/// `scale` is a multiplicative factor applied to the size of the SVG
/// document (see the module documentation) — not a fixed pixel size.
pub fn document_to_png(document: &str, scale: f32) -> Result<Vec<u8>, PngError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(PngError::InvalidScale(scale));
    }

    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    ensure_embedded_fallback(options.fontdb_mut());

    let tree = resvg::usvg::Tree::from_str(document, &options)
        .map_err(|err| PngError::Parse(err.to_string()))?;

    let doc_size = tree.size();
    let pixel_width = (doc_size.width() * scale).round().max(0.0) as u32;
    let pixel_height = (doc_size.height() * scale).round().max(0.0) as u32;

    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(pixel_width, pixel_height).ok_or(PngError::InvalidSize {
            width: pixel_width,
            height: pixel_height,
        })?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    pixmap
        .encode_png()
        .map_err(|err| PngError::Encode(err.to_string()))
}

/// Exports an **already resolved** graph and returns the PNG raster.
///
/// Same signature as [`render_to_string`], plus `scale` — see the module
/// documentation.
pub fn render_to_png(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    scale: f32,
) -> Result<Vec<u8>, PngError> {
    let document = render_to_string(plot, size, measurer)?;
    document_to_png(&document, scale)
}

/// Writes the PNG export to a file.
pub fn render_to_png_file(
    path: impl AsRef<Path>,
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    scale: f32,
) -> Result<(), PngError> {
    let path = path.as_ref();
    let bytes = render_to_png(plot, size, measurer, scale)?;
    std::fs::write(path, bytes).map_err(|source| PngError::Write {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PNG signature, RFC 2083 §3.1 — the same check any image reader
    /// makes before even trying to decode anything.
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

    fn resolved_graph() -> (stokesforge_core::Project, stokesforge_plot::PlotSpec) {
        let sample = stokesforge_render::sample::minimal().expect("minimal graph");
        (sample.project, sample.spec)
    }

    /// Reads the width/height of a PNG directly from the IHDR header
    /// (bytes 16..24, big-endian) rather than pulling in an image-decoding
    /// dependency just for this test.
    fn png_dimensions(png: &[u8]) -> (u32, u32) {
        assert!(png.len() >= 24, "PNG too short to carry an IHDR");
        let width = u32::from_be_bytes(png[16..20].try_into().expect("4 bytes"));
        let height = u32::from_be_bytes(png[20..24].try_into().expect("4 bytes"));
        (width, height)
    }

    #[test]
    fn a_png_export_produces_non_empty_bytes_with_a_valid_header() {
        let (project, spec) = resolved_graph();
        let plot = stokesforge_core::resolve::resolve_spec(project.tree(), &spec);
        let measurer = TextMeasurer::default();

        let png = render_to_png(&plot, (400, 300), &measurer, 1.0).expect("export PNG");

        assert!(!png.is_empty(), "the PNG export must not be empty");
        assert!(
            png.starts_with(&PNG_SIGNATURE),
            "the export must carry the standard PNG signature"
        );
    }

    /// A colorless background (zero opacity) gives a transparent PNG, to
    /// embed as is in another presentation; an opaque background, an
    /// opaque PNG.
    #[test]
    fn a_colorless_background_gives_a_transparent_png() {
        let corner_alpha = |background: &str| {
            let (project, mut spec) = resolved_graph();
            spec.background = background.to_string();
            let plot = stokesforge_core::resolve::resolve_spec(project.tree(), &spec);
            let png = render_to_png(&plot, (400, 300), &TextMeasurer::default(), 1.0)
                .expect("export PNG");
            let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).expect("PNG decoding");
            pixmap.pixel(1, 1).expect("corner pixel").alpha()
        };
        assert_eq!(corner_alpha("#ffffff00"), 0, "transparent background");
        assert_eq!(corner_alpha("#ffffff"), 255, "opaque background");
    }

    #[test]
    fn two_different_scales_produce_different_dimensions() {
        let (project, spec) = resolved_graph();
        let plot = stokesforge_core::resolve::resolve_spec(project.tree(), &spec);
        let measurer = TextMeasurer::default();
        let document = render_to_string(&plot, (400, 300), &measurer).expect("export SVG");

        let png_1x = document_to_png(&document, 1.0).expect("export PNG 1x");
        let png_2x = document_to_png(&document, 2.0).expect("export PNG 2x");

        let (width_1x, height_1x) = png_dimensions(&png_1x);
        let (width_2x, height_2x) = png_dimensions(&png_2x);

        assert_ne!(
            (width_1x, height_1x),
            (width_2x, height_2x),
            "a different scale must change the raster's dimensions, \
             proof that the parameter is taken into account"
        );
        assert_eq!(width_2x, width_1x * 2, "2x must double the width");
        assert_eq!(height_2x, height_1x * 2, "2x must double the height");
    }

    #[test]
    fn a_zero_or_negative_scale_is_refused() {
        let document = {
            let (project, spec) = resolved_graph();
            let plot = stokesforge_core::resolve::resolve_spec(project.tree(), &spec);
            let measurer = TextMeasurer::default();
            render_to_string(&plot, (400, 300), &measurer).expect("export SVG")
        };

        assert!(matches!(
            document_to_png(&document, 0.0),
            Err(PngError::InvalidScale(_))
        ));
        assert!(matches!(
            document_to_png(&document, -1.0),
            Err(PngError::InvalidScale(_))
        ));
        assert!(matches!(
            document_to_png(&document, f32::NAN),
            Err(PngError::InvalidScale(_))
        ));
    }
}

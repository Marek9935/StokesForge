//! Export a graph to an SVG, PDF or PNG file — the application's entry
//! point (issue #34).
//!
//! Always **with Typst**: titles, axis titles and legend are composed
//! as on the screen (`render_to_string_with_typst`), then the PDF and the PNG
//! are derived from that SVG (`document_to_pdf`, `document_to_png`), like the
//! rest of this crate. The format is read from the chosen file's extension.
//!
//! This module knows nothing about zoom: the caller passes the `PlotSpec`
//! already framed (`stokesforge_render_egui::GraphView::framed_spec`), to
//! export what is seen without touching the model.

use std::path::Path;

use stokesforge_core::resolve::ResolvedPlot;
use stokesforge_render::text::TextMeasurer;
use stokesforge_render::FontChoice;

use crate::{
    document_to_pdf, document_to_png, render_to_string_with_typst, PdfError, PngError, SvgError,
    SvgTextMode,
};

/// The format of an export.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportFormat {
    /// SVG; `text` says whether the text composed by Typst stays SVG text or
    /// comes out as outlines (#162).
    Svg {
        /// Text or outlines.
        text: SvgTextMode,
    },
    /// PDF, derived from the SVG in text mode: the text is selectable.
    Pdf,
    /// PNG, derived from the SVG as outlines (independent of the installed
    /// fonts), enlarged by `scale` (1 = the size of the figure).
    Png {
        /// Factor applied to the figure's size.
        scale: f32,
    },
}

impl ExportFormat {
    /// The format designated by the extension of `path`, case-insensitive; a
    /// PNG is rasterized at `png_scale`, an SVG writes its text according to
    /// `svg_text`. `None` for an unknown or missing extension.
    pub fn from_path(path: &Path, png_scale: f32, svg_text: SvgTextMode) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "svg" => Some(Self::Svg { text: svg_text }),
            "pdf" => Some(Self::Pdf),
            "png" => Some(Self::Png { scale: png_scale }),
            _ => None,
        }
    }
}

/// What can go wrong during an export to a file.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The SVG drawing itself.
    #[error(transparent)]
    Svg(#[from] SvgError),
    /// The conversion to PDF.
    #[error(transparent)]
    Pdf(#[from] PdfError),
    /// The rasterization to PNG.
    #[error(transparent)]
    Png(#[from] PngError),
    /// Writing the file.
    #[error("could not write {path}: {source}")]
    Write {
        /// Target path.
        path: String,
        /// System cause.
        source: std::io::Error,
    },
}

impl ExportFormat {
    /// The text mode of the SVG this format is derived from.
    fn text_mode(self) -> SvgTextMode {
        match self {
            Self::Svg { text } => text,
            Self::Pdf => SvgTextMode::Text,
            Self::Png { .. } => SvgTextMode::Outlines,
        }
    }
}

/// The composed SVG document (Typst included) converted to `format`.
fn convert(document: &str, format: ExportFormat) -> Result<Vec<u8>, ExportError> {
    Ok(match format {
        ExportFormat::Svg { .. } => document.as_bytes().to_vec(),
        ExportFormat::Pdf => document_to_pdf(document)?,
        ExportFormat::Png { scale } => document_to_png(document, scale)?,
    })
}

/// The content of the export file, without writing it.
pub fn export_bytes(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    choice: &FontChoice,
    format: ExportFormat,
) -> Result<Vec<u8>, ExportError> {
    let document = render_to_string_with_typst(plot, size, measurer, choice, format.text_mode())?;
    convert(&document, format)
}

/// Several formats of the **same** figure (the clipboard, #149), in the
/// order of `formats`. The figure is plotted and composed by Typst only once
/// **per requested text mode**, the conversions being derived from that
/// SVG; each conversion succeeds or fails alone, so that a failed format
/// does not take the others down. `Err` only if the drawing itself fails.
pub fn export_bytes_many(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    choice: &FontChoice,
    formats: &[ExportFormat],
) -> Result<Vec<Result<Vec<u8>, ExportError>>, ExportError> {
    let mut documents: Vec<(SvgTextMode, String)> = Vec::new();
    for format in formats {
        let mode = format.text_mode();
        if !documents.iter().any(|(known, _)| *known == mode) {
            documents.push((
                mode,
                render_to_string_with_typst(plot, size, measurer, choice, mode)?,
            ));
        }
    }
    Ok(formats
        .iter()
        .map(|format| {
            let mode = format.text_mode();
            documents
                .iter()
                .find(|(known, _)| *known == mode)
                .map_or_else(
                    || Ok(Vec::new()),
                    |(_, document)| convert(document, *format),
                )
        })
        .collect())
}

/// Exports the resolved graph to `path`.
pub fn export_to_file(
    path: impl AsRef<Path>,
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    choice: &FontChoice,
    format: ExportFormat,
) -> Result<(), ExportError> {
    let path = path.as_ref();
    let bytes = export_bytes(plot, size, measurer, choice, format)?;
    std::fs::write(path, bytes).map_err(|source| ExportError::Write {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_is_read_from_the_extension() {
        let png = |name: &str| ExportFormat::from_path(Path::new(name), 4.0, SvgTextMode::Text);
        assert_eq!(
            png("a.svg"),
            Some(ExportFormat::Svg {
                text: SvgTextMode::Text
            })
        );
        assert_eq!(png("a.PDF"), Some(ExportFormat::Pdf));
        assert_eq!(png("a.png"), Some(ExportFormat::Png { scale: 4.0 }));
        assert_eq!(png("a.jpg"), None);
        assert_eq!(png("no_extension"), None);
    }
}

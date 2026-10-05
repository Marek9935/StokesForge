//! Exporter un graphe dans un fichier SVG, PDF ou PNG — le point d'entrée de
//! l'application (issue #34).
//!
//! Toujours **avec Typst** : titres, titres d'axe et légende sont composés
//! comme à l'écran (`render_to_string_with_typst`), puis le PDF et le PNG
//! sont tirés de ce SVG (`document_to_pdf`, `document_to_png`), comme le
//! reste de ce crate. Le format se lit sur l'extension du fichier choisi.
//!
//! Ce module ne sait rien du zoom : l'appelant passe le `PlotSpec` déjà
//! cadré (`stokesforge_render_egui::GraphView::framed_spec`), pour exporter ce
//! qu'on voit sans toucher au modèle.

use std::path::Path;

use stokesforge_core::resolve::ResolvedPlot;
use stokesforge_render::text::TextMeasurer;
use stokesforge_render::FontChoice;

use crate::{
    document_to_pdf, document_to_png, render_to_string_with_typst, PdfError, PngError, SvgError,
};

/// Le format d'un export.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportFormat {
    /// SVG, texte Typst en contours.
    Svg,
    /// PDF, tiré du SVG.
    Pdf,
    /// PNG, tiré du SVG, agrandi de `scale` (1 = la taille de la figure).
    Png {
        /// Facteur appliqué à la taille de la figure.
        scale: f32,
    },
}

impl ExportFormat {
    /// Le format que désigne l'extension de `path`, sans casse ; un PNG est
    /// rastérisé à `png_scale`. `None` pour une extension inconnue ou
    /// absente.
    pub fn from_path(path: &Path, png_scale: f32) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "svg" => Some(Self::Svg),
            "pdf" => Some(Self::Pdf),
            "png" => Some(Self::Png { scale: png_scale }),
            _ => None,
        }
    }
}

/// Ce qui peut rater pendant un export vers un fichier.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// Le tracé SVG lui-même.
    #[error(transparent)]
    Svg(#[from] SvgError),
    /// La conversion en PDF.
    #[error(transparent)]
    Pdf(#[from] PdfError),
    /// La rastérisation en PNG.
    #[error(transparent)]
    Png(#[from] PngError),
    /// L'écriture du fichier.
    #[error("could not write {path}: {source}")]
    Write {
        /// Chemin visé.
        path: String,
        /// Cause système.
        source: std::io::Error,
    },
}

/// Le document SVG composé (Typst compris) converti vers `format`.
fn convert(document: &str, format: ExportFormat) -> Result<Vec<u8>, ExportError> {
    Ok(match format {
        ExportFormat::Svg => document.as_bytes().to_vec(),
        ExportFormat::Pdf => document_to_pdf(document)?,
        ExportFormat::Png { scale } => document_to_png(document, scale)?,
    })
}

/// Le contenu du fichier d'export, sans l'écrire.
pub fn export_bytes(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    choice: &FontChoice,
    format: ExportFormat,
) -> Result<Vec<u8>, ExportError> {
    let document = render_to_string_with_typst(plot, size, measurer, choice)?;
    convert(&document, format)
}

/// Plusieurs formats d'une **même** figure (le presse-papiers, #149), dans
/// l'ordre de `formats`. La figure n'est tracée et composée par Typst qu'une
/// fois, le PDF et le PNG étant tirés du même SVG ; chaque conversion réussit
/// ou échoue seule, de sorte qu'un format raté n'emporte pas les autres.
/// `Err` seulement si le tracé lui-même échoue : aucun format n'existe alors.
pub fn export_bytes_many(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    choice: &FontChoice,
    formats: &[ExportFormat],
) -> Result<Vec<Result<Vec<u8>, ExportError>>, ExportError> {
    let document = render_to_string_with_typst(plot, size, measurer, choice)?;
    Ok(formats
        .iter()
        .map(|format| convert(&document, *format))
        .collect())
}

/// Exporte le graphe résolu dans `path`.
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
    fn le_format_se_lit_sur_lextension() {
        let png = |nom: &str| ExportFormat::from_path(Path::new(nom), 4.0);
        assert_eq!(png("a.svg"), Some(ExportFormat::Svg));
        assert_eq!(png("a.PDF"), Some(ExportFormat::Pdf));
        assert_eq!(png("a.png"), Some(ExportFormat::Png { scale: 4.0 }));
        assert_eq!(png("a.jpg"), None);
        assert_eq!(png("sans_extension"), None);
    }
}

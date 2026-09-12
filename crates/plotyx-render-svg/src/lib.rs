//! Export SVG d'un graphe PlotYx — tâche R4.4.
//!
//! « L'export est au `PlotSpec` ce que la persistance était au modèle : la
//! seule chose qui prouve que la couche neutre est réellement neutre »
//! (`roadmap-rendu.md`). Ce crate est cette preuve, et c'est pour cela qu'il
//! est écrit **en même temps** que le backend écran et non après.
//!
//! # Frontière
//!
//! Ce crate ne connaît ni `egui`, ni `eframe`, ni `winit` — un test le
//! vérifie (`tests/frontiere.rs`), parce que c'est le genre de propriété qui
//! se perd sans qu'on s'en aperçoive. Il mesure pourtant le texte
//! exactement comme l'écran : la mesure vient d'`epaint`, tiré par
//! `plotyx-render`, qui est la seule pile UI dont un export a besoin.
//!
//! Conséquence pratique, et c'est l'argument du §9 de `roadmap-rendu.md` :
//! **le rendu se teste sans serveur graphique**. Toute la suite de tests de
//! ce crate tourne en intégration continue sans display.
//!
//! # PDF
//!
//! L'export PDF (`svg2pdf`, module [`pdf`]) est la phase R6 : une conversion
//! de la `String` que [`render_to_string`] rend déjà, pas un troisième
//! backend — voir la documentation du module pour ce que ça implique.
//!
//! # Ce qui n'est pas ici
//!
//! L'inlining de fragments Typst outlinés dans l'export est la phase R8.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod backend;
pub mod pdf;

use std::path::Path;

use plotters::prelude::{DrawingAreaErrorKind, SVGBackend};
use plotyx_core::resolve::{resolve_spec, ResolvedPlot};
use plotyx_core::Tree;
use plotyx_plot::PlotSpec;
use plotyx_render::pipeline::{self, DrawnPlot};
use plotyx_render::text::TextMeasurer;

pub use backend::MeasuredSvgBackend;
pub use pdf::{document_to_pdf, render_to_pdf, render_to_pdf_file, PdfError};
pub use plotyx_render::text::{font_definitions, TextMeasurer as SvgTextMeasurer};

/// Taille par défaut d'une figure exportée, en unités SVG.
pub const DEFAULT_SIZE: (u32, u32) = (800, 600);

/// Ce qui peut rater pendant un export.
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    /// Plotters a refusé de tracer. En pratique inatteignable avec le backend
    /// SVG, qui écrit dans une `String` — mais l'erreur existe dans la
    /// signature de Plotters et la taire par un `unwrap` serait pire.
    #[error("échec du tracé : {0}")]
    Drawing(String),
    /// Le fichier n'a pas pu être écrit.
    #[error("écriture de {path} : {source}")]
    Write {
        /// Chemin visé.
        path: String,
        /// Cause système.
        source: std::io::Error,
    },
}

impl<E: std::error::Error + Send + Sync> From<DrawingAreaErrorKind<E>> for SvgError {
    fn from(error: DrawingAreaErrorKind<E>) -> Self {
        Self::Drawing(error.to_string())
    }
}

/// Exporte un graphe **déjà résolu** et rend le document SVG.
///
/// Prend le [`TextMeasurer`] en paramètre plutôt que d'en construire un :
/// sa construction décode les polices embarquées, et surtout l'appelant doit
/// pouvoir prouver — c'est tout l'objet du test d'accord — qu'il mesure avec
/// la même chose que l'écran.
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
    Ok(buffer)
}

/// Exporte un graphe déjà résolu, et rend en plus la géométrie retenue.
///
/// Même chose que [`render_to_string`], mais en conservant le [`DrawnPlot`]
/// que le pipeline a calculé — ce dont un test a besoin pour comparer autre
/// chose que du texte.
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
    Ok((buffer, drawn))
}

/// Résout un spec contre un arbre, puis l'exporte.
///
/// Le raccourci qu'attend un appelant qui a un projet sous la main plutôt
/// qu'un `ResolvedPlot`.
pub fn render_spec(
    tree: &Tree,
    spec: &PlotSpec,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<String, SvgError> {
    let plot = resolve_spec(tree, spec);
    render_to_string(&plot, size, measurer)
}

/// Écrit l'export dans un fichier.
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

/// Ce que `plotters-svg` aurait rendu sans la surcharge de
/// `estimate_text_size` — l'heuristique naïve du §4.4.
///
/// N'existe que pour servir de **témoin** dans les tests : une enveloppe qui
/// délèguerait par erreur `estimate_text_size` à `SVGBackend` rendrait
/// exactement cette valeur, et sans point de comparaison le test ne verrait
/// rien. C'est la même précaution que le témoin positif du spike R0 0.1
/// (une famille de police volontairement différente, pour prouver que
/// l'égalité mesurée n'était pas un artefact).
///
/// Reproduit `plotters/src/style/font/naive.rs` : largeur monospace estimée
/// sur le **nombre d'octets UTF-8**.
pub fn naive_estimate(text: &str, plotters_size: f64) -> (u32, u32) {
    let em = plotters_size / 1.24 / 1.24;
    let width = (em * 0.7 * text.len() as f64).round() as i32;
    let top = -(em.round() as i32);
    let bottom = (em * 0.24).round() as i32;
    (width.max(0) as u32, (bottom - top).max(0) as u32)
}

/// Un backend SVG nu, sans la surcharge — **à n'utiliser que dans un test**.
///
/// Exposé parce que le témoin ci-dessus ne suffit pas tout seul : comparer
/// deux nombres ne prouve pas que la mise en page entière bouge. Un test peut
/// ainsi tracer la même figure avec le backend nu et constater que les axes
/// se déplacent réellement — c'est-à-dire que la tâche R4.4 avait un objet.
pub fn plain_backend(buffer: &mut String, size: (u32, u32)) -> SVGBackend<'_> {
    SVGBackend::with_string(buffer, size)
}

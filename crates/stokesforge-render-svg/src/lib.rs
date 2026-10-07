//! Export SVG d'un graphe StokesForge — tâche R4.4.
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
//! `stokesforge-render`, qui est la seule pile UI dont un export a besoin.
//!
//! Conséquence pratique, et c'est l'argument du §9 de `roadmap-rendu.md` :
//! **le rendu se teste sans serveur graphique**. Toute la suite de tests de
//! ce crate tourne en intégration continue sans display.
//!
//! # PDF et PNG
//!
//! L'export PDF (`svg2pdf`, module [`pdf`]) est la phase R6 : une conversion
//! de la `String` que [`render_to_string`] rend déjà, pas un troisième
//! backend — voir la documentation du module pour ce que ça implique.
//! L'export PNG (`resvg`, module [`png`], ajout post-roadmap) suit exactement
//! le même patron : voir la documentation du module.
//!
//! # Typst — tâche R8.4
//!
//! [`typst_export`] compile tout texte riche — titres, titres d'axe,
//! légende ([`stokesforge_render::formula::is_rich`], issue #32) —, pose un témoin substitué après
//! coup par un fragment `typst-svg` inliné — voir la doc de
//! [`typst_export`] pour le mécanisme complet, détaillé en tête de ce
//! module. [`render_to_string_with_typst`] est le point d'entrée équivalent
//! à [`render_to_string`], Typst activé.
//!
//! Typst est toujours compilé depuis l'issue #29 (la feature `typst` a été
//! supprimée). [`render_to_string`] et les conversions PDF/PNG qui s'appuient
//! sur lui tracent encore tout en texte brut : les brancher sur Typst est le
//! travail de l'export depuis l'application (issue #34). Le chemin en texte
//! brut reste, comme côté écran, le témoin des tests Typst.

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
// Le choix de police de l'export, pour qu'un appelant sans egui (l'état de
// l'application) n'ait pas à déclarer d'arête vers `stokesforge-render`.
pub use pdf::{document_to_pdf, render_to_pdf, render_to_pdf_file, PdfError};
pub use png::{document_to_png, render_to_png, render_to_png_file, PngError};
pub use stokesforge_render::text::{font_definitions, TextMeasurer as SvgTextMeasurer};
pub use stokesforge_render::FontChoice;

/// Taille par défaut d'une figure exportée, en unités SVG.
///
/// Reprend [`pipeline::DEFAULT_FIGURE_SIZE`] plutôt que sa propre valeur :
/// l'export doit produire la même figure que l'écran par défaut. Toute
/// fonction de ce module qui exporte prend `size` en paramètre — c'est déjà
/// le point d'extension pour une taille configurable, seulement pas encore
/// câblé à une UI.
pub const DEFAULT_SIZE: (u32, u32) = pipeline::DEFAULT_FIGURE_SIZE;

/// Ce qui peut rater pendant un export.
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    /// Plotters a refusé de tracer. En pratique inatteignable avec le backend
    /// SVG, qui écrit dans une `String` — mais l'erreur existe dans la
    /// signature de Plotters et la taire par un `unwrap` serait pire.
    #[error("drawing failed: {0}")]
    Drawing(String),
    /// Le fichier n'a pas pu être écrit.
    #[error("could not write {path}: {source}")]
    Write {
        /// Chemin visé.
        path: String,
        /// Cause système.
        source: std::io::Error,
    },
    /// La résolution des polices Typst a échoué — tâche R8.4. N'affecte que
    /// [`render_to_string_with_typst`] : les polices embarquées étant
    /// vérifiées valides par les tests de `stokesforge_render::fonts`, ce n'est
    /// atteignable en pratique que pour une police système demandée dont la
    /// lecture échouerait d'une façon que
    /// [`stokesforge_render::fonts::resolve`] ne rencontre déjà pas lui-même
    /// (repli silencieux sur Noto).
    #[error("could not resolve Typst fonts: {0}")]
    Typst(String),
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
    Ok(subpixel::resolve(buffer))
}

/// Comme [`render_to_string`], avec le chemin Typst de la tâche R8.4 activé
/// pour les textes riches ([`stokesforge_render::formula::is_rich`] : titres,
/// titres d'axe, légende).
///
/// Point d'entrée additif — [`render_to_string`] n'est pas touché, tâche
/// 8.6 — construit son propre [`stokesforge_render::formula::FormulaCache`] en
/// interne : un appelant qui exporte la même figure plusieurs fois (par
/// exemple pour comparer des tailles) ne partage donc pas ce cache d'un
/// appel à l'autre, contrairement à [`stokesforge_render_egui::GraphView`] côté
/// écran, qui vit plus longtemps qu'un export. Pas un problème pour un
/// export, par nature ponctuel.
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
    Ok((subpixel::resolve(buffer), drawn))
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

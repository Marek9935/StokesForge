//! Export PDF d'un graphe PlotYx — tâche R6.1.
//!
//! « Le PDF est une conversion du SVG, pas un troisième backend »
//! (`roadmap-rendu.md`, phase R6). Ce module ne dessine rien : il prend le
//! document déjà produit par [`crate::render_to_string`] — donc déjà mesuré
//! avec l'`epaint::Fonts` partagée, donc déjà d'accord avec l'écran — et le
//! fait traverser `usvg` puis `svg2pdf`, qui vient de l'organisation Typst
//! (`architecture-choisie.md` §3 et §5).
//!
//! # Une décision prise en écrivant ce module, non annoncée par les docs
//! source : charger les polices système à la conversion
//!
//! Sans le moindre `fontdb`, `usvg` (donc `svg2pdf`) ne signale aucune
//! erreur mais **abandonne silencieusement tout le texte** : mesuré sur le
//! graphe réaliste de R5 (110 balises `<text>`), le flux de contenu du PDF
//! passe de 599 opérateurs `Tj` à zéro, sans qu'aucun `Result` ne le dise. Un
//! export PDF sans un seul titre, libellé d'axe ni graduation n'est pas un
//! défaut de fidélité mineur : c'est un graphe muet. `document_to_pdf`
//! appelle donc `fontdb_mut().load_system_fonts()`, un simple parcours du
//! système de fichiers qui ne suppose ni serveur graphique ni écran — la
//! même contrainte que le reste du chantier.
//!
//! Ceci **ne tranche pas** le point ouvert n°6 d'`architecture-choisie.md`
//! §5.2 (outliner ou non le texte de l'export). Ce que ce module ajoute
//! comme asymétrie est nouveau, et distinct de celle déjà connue côté SVG :
//! là, le lecteur du fichier a besoin d'une police compatible ; ici, c'est
//! la machine qui **produit** le PDF qui doit en avoir une, une fois, au
//! moment de l'export — après quoi `svg2pdf` embarque un sous-ensemble de
//! la police choisie dans le fichier, qui devient alors autonome. Le PDF
//! est donc **plus** portable que le SVG une fois produit (embarqué, pas
//! référencé), mais le choix de la police dépend de la machine d'export :
//! deux exports du même graphe sur deux machines peuvent choisir deux
//! polices différentes pour « sans-serif », avec des glyphes visuellement
//! différents — sans effet sur la géométrie, puisque les axes sont déjà
//! placés par la mesure `epaint` avant que `svg2pdf` n'intervienne. Une
//! machine d'export sans aucune police installée (un conteneur CI nu, par
//! exemple) retrouve le comportement silencieux ci-dessus : aucune erreur,
//! aucun texte. L'inlining de fragments Typst outlinés, seule façon de
//! rendre ce choix inutile, est la phase R8.

use std::path::Path;

use plotyx_core::resolve::ResolvedPlot;
use plotyx_render::text::TextMeasurer;

use crate::{render_to_string, SvgError};

/// Ce qui peut rater pendant un export PDF, en plus de ce qui peut déjà
/// rater pendant l'export SVG dont il part.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// L'étage SVG a échoué avant même d'atteindre la conversion.
    #[error(transparent)]
    Svg(#[from] SvgError),
    /// `usvg` n'a pas pu reparser le document que [`render_to_string`] vient
    /// d'écrire.
    ///
    /// Ne devrait jamais arriver en pratique : le document part de notre
    /// propre `SVGBackend`. S'il arrivait, ce serait le signe que l'export
    /// SVG a cessé d'être un SVG valide — un fait plus grave que celui que
    /// ce module existe pour vérifier, et qui mérite de ne pas être avalé.
    #[error("relecture du SVG par usvg : {0}")]
    Parse(String),
    /// `svg2pdf` a refusé de convertir l'arbre.
    #[error("conversion en PDF : {0}")]
    Conversion(String),
    /// Le fichier n'a pas pu être écrit.
    #[error("écriture de {path} : {source}")]
    Write {
        /// Chemin visé.
        path: String,
        /// Cause système.
        source: std::io::Error,
    },
}

/// Reparse un document SVG déjà rendu et le convertit en PDF.
///
/// Séparé de [`render_to_pdf`] parce que c'est le point où un appelant qui a
/// déjà le SVG sous la main (par exemple pour l'écrire aussi à côté, ou pour
/// comparer les deux comme le fait la tâche 6.2) évite de faire tracer la
/// figure une seconde fois.
///
/// Charge les polices système de la machine d'export avant de convertir —
/// voir la documentation du module : sans ça, `usvg` abandonne tout le texte
/// sans la moindre erreur.
pub fn document_to_pdf(document: &str) -> Result<Vec<u8>, PdfError> {
    let mut options = svg2pdf::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(document, &options)
        .map_err(|err| PdfError::Parse(err.to_string()))?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|err| PdfError::Conversion(err.to_string()))
}

/// Exporte un graphe **déjà résolu** et rend le document PDF.
///
/// Même signature que [`render_to_string`], sur le même chemin : ce n'est
/// pas un troisième backend, seulement ce qui suit l'export SVG existant.
pub fn render_to_pdf(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
) -> Result<Vec<u8>, PdfError> {
    let document = render_to_string(plot, size, measurer)?;
    document_to_pdf(&document)
}

/// Écrit l'export PDF dans un fichier.
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

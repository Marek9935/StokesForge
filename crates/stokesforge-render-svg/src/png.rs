//! Export PNG d'un graphe StokesForge — même patron que [`crate::pdf`].
//!
//! « Le PDF est une conversion du SVG, pas un troisième backend »
//! (`roadmap-rendu.md`, phase R6) vaut mot pour mot pour le PNG : ce module
//! ne dessine rien. Il reprend le document déjà produit par
//! [`crate::render_to_string`] — donc déjà mesuré avec l'`epaint::Fonts`
//! partagée — et le fait traverser `usvg` puis `resvg`, qui vient de la même
//! famille que `svg2pdf` (`resvg` est d'ailleurs déjà tiré, en interne, par
//! `svg2pdf` — voir `Cargo.toml` et `Cargo.lock`, une seule version d'`usvg`
//! dans l'arbre pour les deux exports).
//!
//! # Résolution : un facteur d'échelle, pas une taille de pixels
//!
//! [`document_to_png`] et [`render_to_png`] prennent un `scale: f32`
//! multiplicatif appliqué à la taille du document SVG (elle-même fixée par
//! le paramètre `size` de [`crate::render_to_string`], en unités SVG). Le
//! document reste vectoriel jusqu'au moment de la rasterisation : demander
//! `scale: 2.0` ou `scale: 4.0` produit une image dont le texte et les
//! traits sont nets à cette résolution, pas un agrandissement flou d'un
//! rendu figé à une taille de pixels câblée en dur. C'est `resvg::render`,
//! appelé avec une `Transform::from_scale(scale, scale)`, qui fait ce
//! travail — la géométrie du graphe (déjà posée par la mesure `epaint`) n'en
//! est pas affectée, seule la densité de pixels du raster final change.
//!
//! # Polices : exactement la même politique que le PDF
//!
//! [`crate::pdf::ensure_embedded_fallback`] est réutilisée telle quelle
//! (`pub(crate)`) plutôt que dupliquée : polices système via
//! `fontdb::load_system_fonts()` d'abord, puis filet Noto embarqué
//! (`stokesforge_render::fonts`) si aucune police système ne satisfait la famille
//! générique `sans-serif`. Voir la documentation de [`crate::pdf`] pour le
//! détail et l'historique de cette décision — elle ne dépend pas du format
//! de sortie, seulement de l'usage d'`usvg` en amont.
//!
//! `resvg` est ajouté aux dépendances directes de ce crate
//! (`tests/frontiere.rs`), à la différence de `fontdb` pour le PDF : ici,
//! c'est bien l'API de rasterisation de `resvg` (`resvg::render`,
//! `resvg::tiny_skia`) qui est utilisée directement, pas seulement un type
//! traversé en chemin.

use std::path::Path;

use stokesforge_core::resolve::ResolvedPlot;
use stokesforge_render::text::TextMeasurer;

use crate::pdf::ensure_embedded_fallback;
use crate::{render_to_string, SvgError};

/// Ce qui peut rater pendant un export PNG, en plus de ce qui peut déjà
/// rater pendant l'export SVG dont il part.
#[derive(Debug, thiserror::Error)]
pub enum PngError {
    /// L'étage SVG a échoué avant même d'atteindre la conversion.
    #[error(transparent)]
    Svg(#[from] SvgError),
    /// `usvg` n'a pas pu reparser le document que [`render_to_string`] vient
    /// d'écrire.
    ///
    /// Ne devrait jamais arriver en pratique — voir [`crate::pdf::PdfError::Parse`],
    /// même raisonnement.
    #[error("could not re-read the SVG with usvg: {0}")]
    Parse(String),
    /// L'échelle demandée n'est pas exploitable (non finie, nulle ou
    /// négative) : `resvg` ne rasterise pas une image sans dimension.
    #[error("invalid scale: {0}")]
    InvalidScale(f32),
    /// La taille résultante, une fois l'échelle appliquée, ne tient pas
    /// dans un canevas de rasterisation (largeur ou hauteur nulle une fois
    /// arrondie).
    #[error("invalid canvas size: {width}x{height}")]
    InvalidSize {
        /// Largeur calculée, en pixels.
        width: u32,
        /// Hauteur calculée, en pixels.
        height: u32,
    },
    /// L'encodage PNG du raster a échoué.
    #[error("PNG encoding failed: {0}")]
    Encode(String),
    /// Le fichier n'a pas pu être écrit.
    #[error("could not write {path}: {source}")]
    Write {
        /// Chemin visé.
        path: String,
        /// Cause système.
        source: std::io::Error,
    },
}

/// Reparse un document SVG déjà rendu et le rasterise en PNG.
///
/// Séparé de [`render_to_png`] pour la même raison que
/// [`crate::pdf::document_to_pdf`] est séparée de [`crate::pdf::render_to_pdf`] :
/// un appelant qui a déjà le SVG sous la main évite de faire tracer la
/// figure une seconde fois.
///
/// `scale` est un facteur multiplicatif appliqué à la taille du document
/// SVG (voir la documentation du module) — pas une taille de pixels fixe.
pub fn document_to_png(document: &str, scale: f32) -> Result<Vec<u8>, PngError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(PngError::InvalidScale(scale));
    }

    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    ensure_embedded_fallback(options.fontdb_mut());

    let tree = resvg::usvg::Tree::from_str(document, &options)
        .map_err(|err| PngError::Parse(err.to_string()))?;

    let taille = tree.size();
    let largeur = (taille.width() * scale).round().max(0.0) as u32;
    let hauteur = (taille.height() * scale).round().max(0.0) as u32;

    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(largeur, hauteur).ok_or(PngError::InvalidSize {
            width: largeur,
            height: hauteur,
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

/// Exporte un graphe **déjà résolu** et rend le raster PNG.
///
/// Même signature que [`render_to_string`], avec en plus `scale` — voir la
/// documentation du module.
pub fn render_to_png(
    plot: &ResolvedPlot<'_>,
    size: (u32, u32),
    measurer: &TextMeasurer,
    scale: f32,
) -> Result<Vec<u8>, PngError> {
    let document = render_to_string(plot, size, measurer)?;
    document_to_png(&document, scale)
}

/// Écrit l'export PNG dans un fichier.
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

    /// Signature PNG, RFC 2083 §3.1 — la même vérification que fait tout
    /// lecteur d'image avant même de tenter de décoder quoi que ce soit.
    const SIGNATURE_PNG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

    fn graphe_resolu() -> (stokesforge_core::Project, stokesforge_plot::PlotSpec) {
        let echantillon = stokesforge_render::sample::minimal().expect("graphe minimal");
        (echantillon.project, echantillon.spec)
    }

    /// Lit la largeur/hauteur d'un PNG directement dans l'en-tête IHDR
    /// (octets 16..24, gros-boutiste) plutôt que de tirer une dépendance de
    /// décodage d'image rien que pour ce test.
    fn dimensions_png(png: &[u8]) -> (u32, u32) {
        assert!(png.len() >= 24, "PNG trop court pour porter un IHDR");
        let largeur = u32::from_be_bytes(png[16..20].try_into().expect("4 octets"));
        let hauteur = u32::from_be_bytes(png[20..24].try_into().expect("4 octets"));
        (largeur, hauteur)
    }

    #[test]
    fn un_export_png_produit_des_octets_non_vides_avec_un_en_tete_valide() {
        let (project, spec) = graphe_resolu();
        let plot = stokesforge_core::resolve::resolve_spec(project.tree(), &spec);
        let measurer = TextMeasurer::default();

        let png = render_to_png(&plot, (400, 300), &measurer, 1.0).expect("export PNG");

        assert!(!png.is_empty(), "l'export PNG ne doit pas être vide");
        assert!(
            png.starts_with(&SIGNATURE_PNG),
            "l'export doit porter la signature PNG standard"
        );
    }

    /// Un fond sans couleur (opacité nulle) donne un PNG transparent, à
    /// intégrer tel quel dans une autre présentation ; un fond opaque, un
    /// PNG opaque.
    #[test]
    fn un_fond_sans_couleur_donne_un_png_transparent() {
        let alpha_du_coin = |fond: &str| {
            let (project, mut spec) = graphe_resolu();
            spec.background = fond.to_string();
            let plot = stokesforge_core::resolve::resolve_spec(project.tree(), &spec);
            let png = render_to_png(&plot, (400, 300), &TextMeasurer::default(), 1.0)
                .expect("export PNG");
            let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).expect("décodage PNG");
            pixmap.pixel(1, 1).expect("pixel du coin").alpha()
        };
        assert_eq!(alpha_du_coin("#ffffff00"), 0, "fond transparent");
        assert_eq!(alpha_du_coin("#ffffff"), 255, "fond opaque");
    }

    #[test]
    fn deux_echelles_differentes_produisent_des_dimensions_differentes() {
        let (project, spec) = graphe_resolu();
        let plot = stokesforge_core::resolve::resolve_spec(project.tree(), &spec);
        let measurer = TextMeasurer::default();
        let document = render_to_string(&plot, (400, 300), &measurer).expect("export SVG");

        let png_1x = document_to_png(&document, 1.0).expect("export PNG 1x");
        let png_2x = document_to_png(&document, 2.0).expect("export PNG 2x");

        let (largeur_1x, hauteur_1x) = dimensions_png(&png_1x);
        let (largeur_2x, hauteur_2x) = dimensions_png(&png_2x);

        assert_ne!(
            (largeur_1x, hauteur_1x),
            (largeur_2x, hauteur_2x),
            "une échelle différente doit changer les dimensions du raster, \
             preuve que le paramètre est bien pris en compte"
        );
        assert_eq!(largeur_2x, largeur_1x * 2, "2x doit doubler la largeur");
        assert_eq!(hauteur_2x, hauteur_1x * 2, "2x doit doubler la hauteur");
    }

    #[test]
    fn une_echelle_nulle_ou_negative_est_refusee() {
        let document = {
            let (project, spec) = graphe_resolu();
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

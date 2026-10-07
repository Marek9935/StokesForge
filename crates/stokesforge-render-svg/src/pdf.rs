//! Export PDF d'un graphe StokesForge — tâche R6.1.
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
//!
//! # Ajout du 2026-09-13 — les polices embarquées comme filet, sous les
//! polices système
//!
//! `load_system_fonts()` reste la première source : une machine d'export
//! normale garde le comportement déjà décrit ci-dessus, glyphes compris.
//! Mais **si aucune police système ne satisfait la famille générique
//! `sans-serif`** — le conteneur CI nu du paragraphe précédent —
//! [`ensure_embedded_fallback`] charge Noto Sans, embarquée dans le binaire
//! (`stokesforge_render::fonts`, la même tâche que celle qui a donné un choix à
//! [`stokesforge_render::text::font_definitions`] côté écran), et la déclare
//! comme police générique `sans-serif` de la base `fontdb`. Un export produit
//! sur une machine totalement dépourvue de police système redevient donc
//! lisible, sans dépendre de ce qui s'y trouve installé.
//!
//! Ceci ne déplace aucun axe : la géométrie est déjà figée par la mesure
//! `epaint` avant que `document_to_pdf` ne s'exécute (voir le paragraphe
//! ci-dessus et le test d'accord R4.5/R5.6) — seul le rendu des glyphes en
//! dépend, exactement comme le choix entre deux polices système en
//! dépendait déjà.
//!
//! `fontdb` n'est pas ajouté aux dépendances directes de ce crate
//! (`tests/frontiere.rs`) : il est déjà atteint via `svg2pdf::usvg::fontdb`,
//! que `usvg` réexporte publiquement — un chemin plutôt qu'une arête de
//! plus.

use std::path::Path;

use stokesforge_core::resolve::ResolvedPlot;
use stokesforge_render::text::TextMeasurer;
use stokesforge_render::FontSet;

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
    #[error("could not re-read the SVG with usvg: {0}")]
    Parse(String),
    /// `svg2pdf` a refusé de convertir l'arbre.
    #[error("PDF conversion failed: {0}")]
    Conversion(String),
    /// Le fichier n'a pas pu être écrit.
    #[error("could not write {path}: {source}")]
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
/// sans la moindre erreur. Complète ensuite avec [`ensure_embedded_fallback`]
/// pour le cas où aucune police système ne conviendrait.
pub fn document_to_pdf(document: &str) -> Result<Vec<u8>, PdfError> {
    let mut options = svg2pdf::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    ensure_embedded_fallback(options.fontdb_mut());
    convert(document, &options)
}

/// La conversion elle-même, une fois les `Options` (et donc la base de
/// polices) déjà décidées.
///
/// Séparée de [`document_to_pdf`] pour que les tests de
/// [`ensure_embedded_fallback`] puissent choisir une base de polices sans
/// passer par `load_system_fonts()` — donc reproduire, de façon
/// déterministe, une machine d'export qui n'aurait aucune police système.
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

/// Garantit qu'un texte `font-family="sans-serif"` (celui qu'écrit
/// `stokesforge_render::pipeline::FONT_FAMILY`, seule famille que ce crate
/// produit) trouve toujours une police, même sur une machine sans aucune
/// police système installée.
///
/// N'agit que si la famille générique `sans-serif` ne résout déjà sur rien
/// dans `fontdb` : une machine d'export normale, dont `load_system_fonts()`
/// a déjà trouvé une police système convenable, n'est pas affectée — cette
/// fonction ne fait qu'ajouter un filet, jamais remplacer un choix qui
/// fonctionne déjà.
///
/// `pub(crate)` : [`crate::png`] la réutilise telle quelle plutôt que de
/// dupliquer la politique de police — `resvg` (sa dépendance) tire la même
/// version d'`usvg`, donc le même `fontdb::Database`, que `svg2pdf` ici (voir
/// `Cargo.lock` et `tests/frontiere.rs`).
pub(crate) fn ensure_embedded_fallback(fontdb: &mut svg2pdf::usvg::fontdb::Database) {
    ensure_generic_sans(fontdb);
    // Les SVG en mode texte (#162) nomment la famille de Typst
    // (`font-family="'Noto Sans', sans-serif"`) : sans ces polices dans la
    // base, `usvg` abandonne silencieusement le texte au lieu de le replier.
    for set in FontSet::ALL {
        fontdb.load_font_data(set.sans_bytes().to_vec());
        fontdb.load_font_data(set.math_bytes().to_vec());
    }
}

fn ensure_generic_sans(fontdb: &mut svg2pdf::usvg::fontdb::Database) {
    let deja_resolue = fontdb
        .query(&svg2pdf::usvg::fontdb::Query {
            families: &[svg2pdf::usvg::fontdb::Family::SansSerif],
            ..svg2pdf::usvg::fontdb::Query::default()
        })
        .is_some();
    if deja_resolue {
        return;
    }
    // Noto Sans : le même défaut et repli que côté écran
    // (`stokesforge_render::fonts::FontSet::Noto`), pour que le choix ne soit
    // pas arbitraire une deuxième fois.
    fontdb.load_font_data(FontSet::Noto.sans_bytes().to_vec());
    fontdb.set_sans_serif_family(FontSet::Noto.sans_family_name());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn requete_sans_serif() -> svg2pdf::usvg::fontdb::Query<'static> {
        svg2pdf::usvg::fontdb::Query {
            families: &[svg2pdf::usvg::fontdb::Family::SansSerif],
            ..svg2pdf::usvg::fontdb::Query::default()
        }
    }

    /// Nombre d'opérateurs `Tj` (affichage d'une chaîne) dans le flux de
    /// contenu de la première page — la même mesure que celle citée en tête
    /// de module pour le bogue de R6 (599 → 0).
    fn compte_tj(pdf: &[u8]) -> usize {
        let document = lopdf::Document::load_mem(pdf).expect("le PDF doit s'ouvrir sans erreur");
        let (_, page_id) = document
            .get_pages()
            .into_iter()
            .next()
            .expect("au moins une page");
        let contenu = document.get_page_content(page_id);
        contenu.windows(3).filter(|f| *f == b" Tj").count()
    }

    /// Témoin, comme dans la documentation du module : une base de polices
    /// vide (aucun `load_system_fonts()`) ne résout pas la famille générique
    /// `sans-serif`. Sans ce fait, [`ensure_embedded_fallback`] n'aurait
    /// rien à garantir.
    #[test]
    fn une_base_de_polices_vide_ne_resout_pas_sans_serif() {
        let fontdb = svg2pdf::usvg::fontdb::Database::new();
        assert!(
            fontdb.query(&requete_sans_serif()).is_none(),
            "témoin invalide : une base neuve résout déjà `sans-serif`"
        );
    }

    #[test]
    fn le_repli_garantit_une_resolution_de_sans_serif() {
        let mut fontdb = svg2pdf::usvg::fontdb::Database::new();
        ensure_embedded_fallback(&mut fontdb);
        assert!(
            fontdb.query(&requete_sans_serif()).is_some(),
            "le repli doit garantir une résolution de `sans-serif`"
        );
    }

    #[test]
    fn le_repli_ne_remplace_pas_une_police_systeme_deja_presente() {
        let mut fontdb = svg2pdf::usvg::fontdb::Database::new();
        fontdb.load_font_data(FontSet::Fira.sans_bytes().to_vec());
        fontdb.set_sans_serif_family(FontSet::Fira.sans_family_name());

        ensure_embedded_fallback(&mut fontdb);

        let id = fontdb
            .query(&requete_sans_serif())
            .expect("doit résoudre après le repli aussi");
        let face = fontdb.face(id).expect("info de face");
        assert!(
            face.families
                .iter()
                .any(|(nom, _)| nom == FontSet::Fira.sans_family_name()),
            "le repli a remplacé une police système qui fonctionnait déjà"
        );
    }

    /// Bout en bout, la raison d'être de ce module : un export produit sur
    /// une machine **sans aucune police système** — ici simulée en
    /// n'appelant jamais `load_system_fonts()` — garde son texte grâce au
    /// repli, alors qu'il le perdrait entièrement sans lui.
    #[test]
    fn un_export_sans_aucune_police_systeme_garde_son_texte() {
        let echantillon = stokesforge_render::sample::minimal().expect("graphe minimal");
        let plot =
            stokesforge_core::resolve::resolve_spec(echantillon.project.tree(), &echantillon.spec);
        let measurer = TextMeasurer::default();
        let document = crate::render_to_string(&plot, (400, 300), &measurer).expect("export SVG");

        // Sans repli : la base de polices ne contient rien, exactement le
        // bogue documenté en tête de module.
        let options_sans_repli = svg2pdf::usvg::Options::default();
        let pdf_sans_repli =
            convert(&document, &options_sans_repli).expect("conversion (sans repli)");
        assert_eq!(
            compte_tj(&pdf_sans_repli),
            0,
            "témoin invalide : le PDF porte du texte sans le moindre repli ni \
             la moindre police système chargée"
        );

        // Avec repli : le même document, la même absence de police système,
        // mais `ensure_embedded_fallback` en plus.
        let mut options_avec_repli = svg2pdf::usvg::Options::default();
        ensure_embedded_fallback(options_avec_repli.fontdb_mut());
        let pdf_avec_repli =
            convert(&document, &options_avec_repli).expect("conversion (avec repli)");
        assert!(
            compte_tj(&pdf_avec_repli) > 0,
            "le repli n'a pas empêché la perte du texte"
        );
    }
}

//! Chemin export de Typst — tâche R8.4.
//!
//! # Le mécanisme, et pourquoi il ne touche pas `SVGBackend`
//!
//! `plotters-svg::SVGBackend` écrit directement dans sa `String` cible à
//! chaque appel de `draw_*`, sans repasser par un document qu'on pourrait
//! modifier après coup — et son champ `target` est privé, donc
//! [`crate::backend::MeasuredSvgBackend`] ne peut pas y injecter du XML
//! brut au moment où il dessine un label Typst. `architecture-choisie.md`
//! §8.1 suggère la sortie : noter des **rectangles-témoins**, à substituer
//! une fois le document entier écrit.
//!
//! Concrètement : [`TypstExport::record`] compile la formule (ou reprend le
//! cache, tâche 8.2), retient sa position, sa rotation et son fragment SVG
//! déjà rendu, puis [`crate::backend::MeasuredSvgBackend::draw_text`] pose
//! un simple `<rect>` de couleur [`crate::backend::WITNESS_COLOR`] à la
//! place du `<text>` ordinaire. Une fois tout le graphe tracé et la
//! `String` complète obtenue, [`splice`] retrouve chaque témoin **par sa
//! couleur**, dans l'ordre où ils ont été posés, et le remplace par le
//! fragment enregistré.
//!
//! **Limite assumée** : si un style de tracé du spec utilisait exactement
//! la même couleur `RGBColor(1, 2, 3)` que [`crate::backend::WITNESS_COLOR`]
//! pour un vrai rectangle (barre, légende), `splice` le prendrait à tort
//! pour un témoin. Une couleur aussi spécifique n'a aucune raison d'être
//! choisie par un utilisateur ou un défaut du projet ; le risque est jugé
//! négligeable plutôt qu'éliminé par construction (un vrai identifiant
//! coûterait de faire porter un attribut `id` par `plotters-svg`, ce qu'il
//! ne fait justement pas — voir `architecture-choisie.md` §5.2).
//!
//! # L'unité du fragment imbriqué
//!
//! `typst_svg::svg` déclare son `<svg>` racine avec des `width`/`height` en
//! points CSS (`"12.5pt"`) — une unité **absolue**, différente de la
//! convention déjà en vigueur dans ce chantier où un nombre nu dans le SVG
//! de Plotters représente un pixel (`plotyx_render::text`,
//! `PLOTTERS_SIZE_PER_POINT`). Imbriquer tel quel le `<svg>` de Typst
//! ferait donc redimensionner son contenu d'un facteur ~1,33 (1 pt CSS =
//! 1,3333 px) par rapport au reste du document. [`inner_svg`] retire cette
//! balise racine ; [`splice`] la remplace par la sienne, en nombres nus
//! égaux à `size_pt` — cohérents avec le reste du document, puisque
//! `viewBox` de Typst est déjà en nombres nus dans la même échelle.

use std::cell::RefCell;
use std::sync::Arc;

use plotters_backend::{text_anchor::Pos, FontTransform};
use plotyx_render::formula::{self, Compiled, FormulaCache};
use plotyx_render::FontChoice;
use typst::text::Font;
use typst_svg::SvgOptions;

/// `pixels_per_point` fixe utilisé pour toutes les compilations de l'export
/// — voir la doc de [`plotyx_render::formula::FormulaCache`] : ce terme de
/// la clé de cache ne varie jamais côté export, un fragment SVG étant
/// vectoriel et donc indépendant de toute résolution d'écran.
const EXPORT_PIXELS_PER_POINT: f32 = 1.0;

/// Un fragment SVG déjà compilé pour l'export — la valeur mise en cache
/// (tâche 8.2), l'analogue export de `FormulaTexture` côté écran.
///
/// Public malgré ses champs privés : c'est le paramètre de type du
/// [`FormulaCache`] qu'un appelant ([`crate::render_to_string_with_typst`])
/// doit pouvoir nommer pour construire son propre cache.
pub struct FormulaFragment {
    /// Contenu intérieur d'un document `typst_svg::svg`, sans sa balise
    /// `<svg>` racine — voir la doc de tête pour la raison.
    inner_svg: String,
    /// Taille en points, le même repère que le reste du texte.
    size_pt: (f64, f64),
}

/// Un emplacement à substituer — un témoin déjà posé par `draw_rect`,
/// remplacé par [`splice`] une fois le document entier écrit.
struct PendingFormula {
    /// Coin supérieur gauche **non tourné** du fragment, dans le repère du
    /// document — c'est aussi le pivot de la rotation, comme
    /// `epaint::TextShape::pos` (voir `crate::typst_screen::mesh` côté
    /// écran, le même calcul).
    origin: (f64, f64),
    /// Rotation horaire en degrés — l'unité que `rotate(deg, cx, cy)` de
    /// SVG attend directement.
    angle_deg: f64,
    size_pt: (f64, f64),
    inner_svg: String,
}

/// Ce qu'il faut pour poser des témoins Typst dans l'export — construit une
/// fois par appel à [`crate::render_to_string_with_typst`].
pub struct TypstExport<'a> {
    cache: &'a FormulaCache<FormulaFragment>,
    choice: &'a FontChoice,
    sans: Font,
    math: Font,
    pending: RefCell<Vec<PendingFormula>>,
}

impl<'a> TypstExport<'a> {
    /// Résout les polices une fois (`choice`) — comme
    /// `plotyx_render_egui::TypstScreen::new`.
    pub fn new(
        cache: &'a FormulaCache<FormulaFragment>,
        choice: &'a FontChoice,
    ) -> Result<Self, String> {
        let (sans, math) = formula::resolve_fonts(choice)?;
        Ok(Self {
            cache,
            choice,
            sans,
            math,
            pending: RefCell::new(Vec::new()),
        })
    }

    /// Le fragment compilé (ou repris du cache) pour `text` à `size_pt` de
    /// corps. `None` si la formule ne compile pas.
    fn fragment(&self, text: &str, size_pt: f32) -> Option<Arc<FormulaFragment>> {
        self.cache
            .get_or_try_insert(
                text,
                size_pt,
                self.choice,
                EXPORT_PIXELS_PER_POINT,
                self.sans.clone(),
                self.math.clone(),
                |compiled| Ok(build_fragment(compiled)),
            )
            .ok()
    }

    /// La taille (arrondie en pixels de backend) qu'occuperait `text`
    /// compilé par Typst — l'invariant de la tâche 8.5, côté export.
    pub fn measure(&self, text: &str, size_pt: f32) -> Option<(u32, u32)> {
        let fragment = self.fragment(text, size_pt)?;
        Some(formula::round_size_pt(fragment.size_pt))
    }

    /// Enregistre un témoin à substituer par [`splice`] : compile la
    /// formule, calcule son ancrage et sa rotation exactement comme
    /// `EguiBackend::draw_text` le fait à l'écran (mêmes fonctions
    /// partagées de `plotyx_render::formula`), et retient le fragment.
    /// Rend la taille de la boîte (pour dimensionner le rectangle-témoin),
    /// `None` si la formule ne compile pas.
    pub fn record(
        &self,
        text: &str,
        size_pt: f32,
        anchor: Pos,
        transform: FontTransform,
        pos: (f64, f64),
    ) -> Option<(f64, f64)> {
        let fragment = self.fragment(text, size_pt)?;
        let offset = formula::anchor_offset(fragment.size_pt, anchor.h_pos, anchor.v_pos);
        let offset = formula::rotate_offset(offset, transform.clone());
        let origin = (pos.0 + offset.0, pos.1 + offset.1);
        self.pending.borrow_mut().push(PendingFormula {
            origin,
            angle_deg: formula::rotation_degrees(transform),
            size_pt: fragment.size_pt,
            inner_svg: fragment.inner_svg.clone(),
        });
        Some(fragment.size_pt)
    }
}

/// Compile une page en fragment SVG — tâche 8.4.
fn build_fragment(compiled: &Compiled) -> FormulaFragment {
    let size_pt = compiled.size_pt();
    let Some(page) = compiled.page() else {
        return FormulaFragment {
            inner_svg: String::new(),
            size_pt: (0.0, 0.0),
        };
    };
    let document = typst_svg::svg(page, &SvgOptions::default());
    FormulaFragment {
        inner_svg: inner_svg(&document).to_owned(),
        size_pt,
    }
}

/// Le contenu intérieur d'un document SVG autonome produit par
/// `typst_svg::svg` — entre la balise `<svg ...>` d'ouverture et
/// `</svg>`. Voir la doc de tête du module pour la raison.
fn inner_svg(document: &str) -> &str {
    let start = document.find('>').map_or(0, |i| i + 1);
    let end = document.rfind("</svg>").unwrap_or(document.len());
    document.get(start..end.max(start)).unwrap_or("").trim()
}

/// Substitue chaque témoin posé par [`TypstExport::record`] dans `buffer`
/// par son fragment Typst, dans l'ordre où ils ont été enregistrés — tâche
/// R8.4.
///
/// `buffer` est le document SVG complet, obtenu **après** que le backend
/// qui a construit `typst` a été abandonné (le retirer de portée relâche
/// l'emprunt sur la `String` cible que `SVGBackend` détenait) — voir
/// `crate::render_to_string_with_typst`.
pub fn splice(buffer: &str, typst: &TypstExport<'_>) -> String {
    let pending = typst.pending.borrow();
    let witness = format!("fill=\"{}\"", make_hex(crate::backend::WITNESS_COLOR));
    let mut result = String::with_capacity(buffer.len());
    let mut rest = buffer;
    for formula in pending.iter() {
        // Un témoin manquant ou mal formé ne doit pas faire échouer tout
        // l'export : le rectangle-témoin reste visible tel quel dans ce
        // cas plutôt que de paniquer sur un document par ailleurs valide.
        let Some(marker) = rest.find(&witness) else {
            break;
        };
        let Some(tag_start) = rest[..marker].rfind("<rect") else {
            break;
        };
        let Some(tag_end) = rest[marker..].find("/>").map(|i| marker + i + 2) else {
            break;
        };
        result.push_str(&rest[..tag_start]);
        result.push_str(&group_for(formula));
        rest = &rest[tag_end..];
    }
    result.push_str(rest);
    result
}

/// Le `<g>` de remplacement d'un témoin : rotation puis translation vers
/// `origin`, un `<svg>` imbriqué dimensionné en nombres nus (voir la doc de
/// tête pour l'unité), et le fragment intérieur de Typst.
fn group_for(formula: &PendingFormula) -> String {
    let (ox, oy) = formula.origin;
    let (w, h) = formula.size_pt;
    format!(
        "<g transform=\"rotate({deg}, {ox}, {oy}) translate({ox}, {oy})\">\
         <svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" \
         xmlns=\"http://www.w3.org/2000/svg\">{inner}</svg></g>",
        deg = formula.angle_deg,
        inner = formula.inner_svg,
    )
}

/// La même représentation hexadécimale que `plotters-svg::make_svg_color`
/// (module privé de ce crate tiers) — reproduite ici pour reconnaître la
/// couleur-témoin dans le document produit, sans dépendre d'une fonction
/// que `plotters-svg` n'expose pas.
fn make_hex(color: plotters::style::RGBColor) -> String {
    format!("#{:02X}{:02X}{:02X}", color.0, color.1, color.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotters_backend::text_anchor::{HPos, VPos};

    fn export() -> (FormulaCache<FormulaFragment>, FontChoice) {
        (FormulaCache::new(), FontChoice::default())
    }

    /// Une formule triviale s'enregistre et rend une taille non nulle.
    #[test]
    fn enregistrer_une_formule_triviale_rend_une_taille_non_nulle() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        let taille = typst
            .record("$lambda$", 14.0, anchor, FontTransform::None, (10.0, 20.0))
            .expect("la formule aurait dû compiler");
        assert!(taille.0 > 0.0 && taille.1 > 0.0);
    }

    /// Une formule invalide ne s'enregistre pas — dégradation, pas panique.
    #[test]
    fn une_formule_invalide_ne_senregistre_pas() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        assert!(typst
            .record("$ (unclosed", 14.0, anchor, FontTransform::None, (0.0, 0.0))
            .is_none());
    }

    /// `splice` retrouve le témoin et injecte le fragment Typst — en
    /// laissant le reste du document intact.
    #[test]
    fn splice_remplace_le_temoin_par_le_fragment() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        typst
            .record("$lambda$", 14.0, anchor, FontTransform::None, (10.0, 20.0))
            .expect("compilation échouée");

        let hex = make_hex(crate::backend::WITNESS_COLOR);
        let document = format!(
            "<svg><rect x=\"0\" y=\"0\" width=\"1\" height=\"1\" fill=\"{hex}\"/><circle/></svg>"
        );
        let result = splice(&document, &typst);
        assert!(!result.contains(&hex), "le témoin est encore présent");
        assert!(
            result.contains("<svg width="),
            "le fragment imbriqué est absent"
        );
        assert!(
            result.contains("<circle/>"),
            "le reste du document a été perdu"
        );
    }

    /// Un document sans témoin ne change pas — cas normal quand aucun
    /// label de la figure n'est une formule.
    #[test]
    fn sans_temoin_le_document_ne_change_pas() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let document = "<svg><circle/></svg>".to_string();
        assert_eq!(splice(&document, &typst), document);
    }
}

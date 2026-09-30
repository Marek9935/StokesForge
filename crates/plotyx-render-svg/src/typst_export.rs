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
//! déjà rendu, s'attribue une couleur-témoin **qui n'a encore servi à aucun
//! autre témoin de cet export** (voir [`TypstExport::next_witness_color`]),
//! et rend cette couleur avec la taille attendue du rectangle à
//! [`crate::backend::MeasuredSvgBackend::draw_text`], qui pose un simple
//! `<rect>` de cette couleur et cette taille à la place du `<text>`
//! ordinaire. Une fois tout le graphe tracé et la `String` complète
//! obtenue, [`splice`] retrouve chaque témoin **par sa couleur propre**, en
//! revérifiant que le `<rect>` trouvé a bien la position et la taille
//! attendues avant de le remplacer par le fragment enregistré.
//!
//! **Limite assumée** (corrigée depuis la version à couleur unique, dont
//! ceci documentait le défaut) : la couleur-témoin varie par témoin — la
//! composante rouge reste fixée à une valeur qu'un utilisateur ou un
//! défaut du projet n'a raisonnablement aucune raison de choisir pour un
//! vrai élément du graphe, vert et bleu encodent un compteur (65536
//! couleurs distinctes par export). [`splice`] exige en plus que le
//! `<rect>` trouvé ait la position et la taille exactes du témoin posé.
//! Pour provoquer une collision, un vrai élément du graphe devrait donc
//! partager *à la fois* une couleur précise et une géométrie précise avec
//! un témoin — un risque jugé négligeable plutôt qu'éliminé par
//! construction : un vrai identifiant coûterait de faire porter un
//! attribut `id` par `plotters-svg`, ce qu'il ne fait justement pas (voir
//! `architecture-choisie.md` §5.2). Reste aussi le cas, plus théorique que
//! pratique, d'un export à plus de 65536 témoins : le compteur de couleurs
//! boucle et deux témoins du même export pourraient à nouveau se partager
//! une couleur, la vérification de géométrie restant alors le seul filet
//! entre eux.
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

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use plotters::style::RGBColor;
use plotters_backend::{text_anchor::Pos, FontTransform};
use plotyx_render::formula::{self, Compiled, FormulaCache, Ink};
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
    /// Couleur-témoin propre à ce témoin — voir
    /// [`TypstExport::next_witness_color`]. Distincte de celle de tout
    /// autre témoin du même export.
    color: RGBColor,
    /// Position **brute** (avant décalage d'ancrage) du `<rect>`-témoin
    /// réellement posé par `draw_rect`, dans le repère entier du backend —
    /// distincte de [`Self::origin`], qui sert au placement du fragment
    /// final et inclut déjà ce décalage. C'est ce que [`splice`] revérifie
    /// avant de substituer.
    witness_pos: (i32, i32),
    /// Taille en pixels de backend du `<rect>`-témoin réellement posé —
    /// `size_pt` arrondi comme
    /// [`crate::backend::MeasuredSvgBackend::draw_text`] arrondit pour son
    /// appel à `draw_rect`. Revérifiée elle aussi par [`splice`].
    witness_size: (i32, i32),
}

/// Ce que [`TypstExport::record`] rend à l'appelant : la couleur-témoin
/// assignée à cet appel et la taille en pixels de backend que le
/// rectangle-témoin doit avoir — exactement ce que
/// [`crate::backend::MeasuredSvgBackend::draw_text`] doit passer à
/// `draw_rect`, et ce que [`splice`] revérifie ensuite avant de substituer.
pub(crate) struct WitnessRect {
    pub(crate) color: RGBColor,
    pub(crate) size: (i32, i32),
}

/// Ce qu'il faut pour poser des témoins Typst dans l'export — construit une
/// fois par appel à [`crate::render_to_string_with_typst`].
pub struct TypstExport<'a> {
    cache: &'a FormulaCache<FormulaFragment>,
    choice: &'a FontChoice,
    sans: Font,
    math: Font,
    pending: RefCell<Vec<PendingFormula>>,
    /// Compteur du prochain appel à [`Self::next_witness_color`] — voir sa
    /// doc pour comment il devient une couleur.
    next_witness: Cell<u16>,
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
            next_witness: Cell::new(0),
        })
    }

    /// La composante rouge fixe de toute couleur-témoin de cet export —
    /// une valeur qu'un utilisateur ou un défaut du projet n'a
    /// raisonnablement aucune raison de choisir pour un vrai élément du
    /// graphe (c'était déjà, seule, toute la couleur-témoin avant cette
    /// correction : voir la doc de tête, section « Limite assumée »).
    const WITNESS_RED: u8 = 1;

    /// La couleur-témoin du prochain témoin de cet export : le rouge reste
    /// fixé à [`Self::WITNESS_RED`], vert et bleu encodent un compteur —
    /// 65536 couleurs distinctes par export, largement au-delà de ce
    /// qu'une figure réaliste peut contenir de labels Typst. C'est ce qui
    /// rend deux témoins du même export non confondables par couleur, là où
    /// l'ancien mécanisme (une seule couleur partagée) les rendait
    /// indiscernables entre eux.
    ///
    /// Au-delà de 65536 appels, le compteur boucle et deux témoins
    /// pourraient à nouveau partager une couleur — la vérification de
    /// géométrie de [`splice`] resterait alors le seul filet entre eux, un
    /// volume qu'une figure réaliste ne devrait pas atteindre.
    fn next_witness_color(&self) -> RGBColor {
        let n = self.next_witness.get();
        self.next_witness.set(n.wrapping_add(1));
        RGBColor(Self::WITNESS_RED, (n >> 8) as u8, n as u8)
    }

    /// Le fragment compilé (ou repris du cache) pour `text` à `size_pt` de
    /// corps, en `ink` (issue #30 : `typst-svg` écrit la couleur dans le
    /// fragment). `None` si la formule ne compile pas.
    fn fragment(&self, text: &str, size_pt: f32, ink: Ink) -> Option<Arc<FormulaFragment>> {
        self.cache
            .get_or_try_insert(
                text,
                size_pt,
                ink,
                self.choice,
                EXPORT_PIXELS_PER_POINT,
                self.sans.clone(),
                self.math.clone(),
                |compiled| Ok(build_fragment(compiled)),
            )
            .ok()
    }

    /// La taille (arrondie en pixels de backend) qu'occuperait `text`
    /// compilé par Typst — l'invariant de la tâche 8.5, côté export. `ink`
    /// ne change pas la taille : il fait tomber la mesure sur l'entrée que
    /// [`Self::record`] réutilise.
    pub fn measure(&self, text: &str, size_pt: f32, ink: Ink) -> Option<(u32, u32)> {
        let fragment = self.fragment(text, size_pt, ink)?;
        Some(formula::round_size_pt(fragment.size_pt))
    }

    /// Enregistre un témoin à substituer par [`splice`] : compile la
    /// formule, calcule son ancrage et sa rotation exactement comme
    /// `EguiBackend::draw_text` le fait à l'écran (mêmes fonctions
    /// partagées de `plotyx_render::formula`), lui attribue une couleur qui
    /// n'a encore servi à aucun autre témoin de cet export (voir
    /// [`Self::next_witness_color`]), et retient le fragment. Rend la
    /// couleur et la taille en pixels de backend du rectangle-témoin à
    /// dessiner — exactement ce qu'un appelant (`MeasuredSvgBackend`) doit
    /// passer à `draw_rect`, et ce que [`splice`] revérifiera. `None` si la
    /// formule ne compile pas.
    pub(crate) fn record(
        &self,
        text: &str,
        size_pt: f32,
        ink: Ink,
        anchor: Pos,
        transform: FontTransform,
        pos: (f64, f64),
    ) -> Option<WitnessRect> {
        let fragment = self.fragment(text, size_pt, ink)?;
        let offset = formula::anchor_offset(fragment.size_pt, anchor.h_pos, anchor.v_pos);
        let offset = formula::rotate_offset(offset, transform.clone());
        let origin = (pos.0 + offset.0, pos.1 + offset.1);
        let color = self.next_witness_color();
        let witness_pos = (pos.0.round() as i32, pos.1.round() as i32);
        // Le même arrondi que `MeasuredSvgBackend::draw_text` appliquait
        // jusqu'ici lui-même avant d'appeler `draw_rect` — rapatrié ici pour
        // que le rectangle réellement dessiné et celui que `splice`
        // attend soient calculés par le même code, jamais deux fois.
        let witness_size = (
            fragment.size_pt.0.ceil().max(1.0) as i32,
            fragment.size_pt.1.ceil().max(1.0) as i32,
        );
        self.pending.borrow_mut().push(PendingFormula {
            origin,
            angle_deg: formula::rotation_degrees(transform),
            size_pt: fragment.size_pt,
            inner_svg: fragment.inner_svg.clone(),
            color,
            witness_pos,
            witness_size,
        });
        Some(WitnessRect {
            color,
            size: witness_size,
        })
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
/// par son fragment Typst — tâche R8.4, corrigée par la couleur propre à
/// chaque témoin et la vérification de géométrie (voir la doc de tête,
/// section « Limite assumée »).
///
/// `buffer` est le document SVG complet, obtenu **après** que le backend
/// qui a construit `typst` a été abandonné (le retirer de portée relâche
/// l'emprunt sur la `String` cible que `SVGBackend` détenait) — voir
/// `crate::render_to_string_with_typst`.
pub fn splice(buffer: &str, typst: &TypstExport<'_>) -> String {
    let pending = typst.pending.borrow();
    let mut result = buffer.to_string();
    for formula in pending.iter() {
        // Un témoin manquant, ou trouvé mais de géométrie différente de
        // celle attendue (la signature d'une collision de couleur avec un
        // vrai élément du graphe), reste tel quel : dégradation plutôt que
        // panique ou export corrompu, sur un document par ailleurs valide.
        if let Some(range) = matching_witness_range(&result, formula) {
            result.replace_range(range, &group_for(formula));
        }
    }
    result
}

/// Cherche dans `haystack` le `<rect>`-témoin de `formula` : sa couleur
/// propre **et** sa géométrie (position, taille) doivent correspondre à ce
/// que [`TypstExport::record`] a enregistré. Un `<rect>` de la bonne
/// couleur mais d'une géométrie différente n'est pas retenu — c'est
/// exactement le cas d'un vrai élément du graphe qui porterait la même
/// couleur par malchance, que la vérification est censée écarter.
fn matching_witness_range(
    haystack: &str,
    formula: &PendingFormula,
) -> Option<std::ops::Range<usize>> {
    let witness = format!("fill=\"{}\"", make_hex(formula.color));
    let marker = haystack.find(&witness)?;
    let tag_start = haystack[..marker].rfind("<rect")?;
    let tag_end = haystack[marker..].find("/>").map(|i| marker + i + 2)?;
    let tag = &haystack[tag_start..tag_end];
    geometry_matches(tag, formula.witness_pos, formula.witness_size).then_some(tag_start..tag_end)
}

/// La valeur d'un attribut `name="..."` dans un fragment de balise XML —
/// assez pour lire `x`, `y`, `width`, `height` sur le `<rect>` émis par
/// `plotters-svg::SVGBackend::draw_rect` (voir sa source pour le format
/// exact des attributs, reproduit ici en lecture comme [`make_hex`]
/// reproduit son écriture des couleurs).
fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Vrai si le `<rect>` `tag` a la position et la taille attendues, à
/// 0,5 px près — les coordonnées émises par `plotters-svg` sont des
/// entiers, la tolérance n'absorbe que l'arrondi du parsing en `f64`, pas
/// une vraie marge géométrique.
fn geometry_matches(tag: &str, pos: (i32, i32), size: (i32, i32)) -> bool {
    const EPS: f64 = 0.5;
    let parse = |name| attr_value(tag, name).and_then(|v| v.parse::<f64>().ok());
    let (Some(x), Some(y), Some(w), Some(h)) =
        (parse("x"), parse("y"), parse("width"), parse("height"))
    else {
        return false;
    };
    (x - pos.0 as f64).abs() < EPS
        && (y - pos.1 as f64).abs() < EPS
        && (w - size.0 as f64).abs() < EPS
        && (h - size.1 as f64).abs() < EPS
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

    const NOIR: Ink = [0, 0, 0, 255];

    fn export() -> (FormulaCache<FormulaFragment>, FontChoice) {
        (FormulaCache::new(), FontChoice::default())
    }

    /// Une formule triviale s'enregistre et rend une taille non nulle.
    #[test]
    fn enregistrer_une_formule_triviale_rend_une_taille_non_nulle() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        let witness = typst
            .record(
                "$lambda$",
                14.0,
                NOIR,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("la formule aurait dû compiler");
        assert!(witness.size.0 > 0 && witness.size.1 > 0);
    }

    /// Une formule invalide ne s'enregistre pas — dégradation, pas panique.
    #[test]
    fn une_formule_invalide_ne_senregistre_pas() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        assert!(typst
            .record(
                "$ (unclosed",
                14.0,
                NOIR,
                anchor,
                FontTransform::None,
                (0.0, 0.0)
            )
            .is_none());
    }

    /// `splice` retrouve le témoin et injecte le fragment Typst — en
    /// laissant le reste du document intact.
    #[test]
    fn splice_remplace_le_temoin_par_le_fragment() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        let witness = typst
            .record(
                "$lambda$",
                14.0,
                NOIR,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("compilation échouée");

        let hex = make_hex(witness.color);
        let (w, h) = witness.size;
        let document = format!(
            "<svg><rect x=\"10\" y=\"20\" width=\"{w}\" height=\"{h}\" fill=\"{hex}\"/><circle/></svg>"
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

    /// Deux formules différentes dans le même export ne se mélangent pas :
    /// chacune reçoit son propre fragment, jamais celui de l'autre. Avec
    /// l'ancien mécanisme à couleur unique, ce test aurait pu passer par
    /// accident (les deux témoins étaient indiscernables par couleur) ;
    /// une couleur propre à chaque témoin le rend vérifiable pour de bon.
    #[test]
    fn deux_formules_differentes_ne_se_melangent_pas() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);

        let lambda = typst
            .record(
                "$lambda$",
                14.0,
                NOIR,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("compilation échouée (lambda)");
        let alpha = typst
            .record(
                "$alpha$",
                14.0,
                NOIR,
                anchor,
                FontTransform::None,
                (100.0, 200.0),
            )
            .expect("compilation échouée (alpha)");

        assert_ne!(
            make_hex(lambda.color),
            make_hex(alpha.color),
            "les deux témoins partagent une couleur"
        );

        let (inner_lambda, inner_alpha) = {
            let pending = typst.pending.borrow();
            (pending[0].inner_svg.clone(), pending[1].inner_svg.clone())
        };
        assert!(!inner_lambda.is_empty() && !inner_alpha.is_empty());
        assert_ne!(
            inner_lambda, inner_alpha,
            "les deux formules compilent au même fragment, le test ne peut rien prouver"
        );

        let document = format!(
            "<svg><rect x=\"10\" y=\"20\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\
             <rect x=\"100\" y=\"200\" width=\"{}\" height=\"{}\" fill=\"{}\"/></svg>",
            lambda.size.0,
            lambda.size.1,
            make_hex(lambda.color),
            alpha.size.0,
            alpha.size.1,
            make_hex(alpha.color),
        );

        let result = splice(&document, &typst);

        // Chaque fragment Typst compilé (lambda, alpha) contient lui-même
        // des `<g>` internes (Typst structure ses glyphes ainsi) : borner
        // un bloc au premier `</g>` rencontré couperait donc dans le
        // fragment plutôt qu'à la fin du groupe de substitution. On borne
        // plutôt chaque bloc par le début de l'autre — comme les deux
        // groupes sont écrits dans l'ordre où `record` a été appelé, celui
        // de lambda précède forcément celui d'alpha dans le document.
        let lambda_group = result
            .find("translate(10, 20)")
            .expect("groupe de lambda absent");
        let alpha_group = result
            .find("translate(100, 200)")
            .expect("groupe d'alpha absent");
        assert!(
            lambda_group < alpha_group,
            "le groupe de lambda devrait précéder celui d'alpha dans le document"
        );

        let lambda_block = &result[lambda_group..alpha_group];
        let alpha_block = &result[alpha_group..];
        assert!(
            lambda_block.contains(&inner_lambda),
            "le fragment de lambda n'est pas dans son groupe"
        );
        assert!(
            alpha_block.contains(&inner_alpha),
            "le fragment d'alpha n'est pas dans son groupe"
        );
        assert!(
            !lambda_block.contains(&inner_alpha),
            "le fragment d'alpha a fui dans le groupe de lambda"
        );
        assert!(
            !alpha_block.contains(&inner_lambda),
            "le fragment de lambda a fui dans le groupe d'alpha"
        );
    }

    /// Un `<rect>` de la bonne couleur-témoin mais d'une taille différente
    /// de celle attendue — la signature d'une collision avec un vrai
    /// élément du graphe qui porterait la même couleur par malchance —
    /// n'est pas substitué : c'est la vérification de géométrie qui doit
    /// l'écarter, pas seulement en théorie.
    #[test]
    fn temoin_de_mauvaise_taille_nest_pas_substitue() {
        let (cache, choice) = export();
        let typst = TypstExport::new(&cache, &choice).expect("résolution des polices échouée");
        let anchor = Pos::new(HPos::Left, VPos::Top);
        let witness = typst
            .record(
                "$lambda$",
                14.0,
                NOIR,
                anchor,
                FontTransform::None,
                (10.0, 20.0),
            )
            .expect("compilation échouée");

        let hex = make_hex(witness.color);
        let (w, h) = witness.size;
        let document = format!(
            "<svg><rect x=\"10\" y=\"20\" width=\"{}\" height=\"{}\" fill=\"{hex}\"/></svg>",
            w + 50,
            h + 50,
        );

        let result = splice(&document, &typst);
        assert_eq!(
            result, document,
            "un témoin de mauvaise taille n'aurait pas dû être substitué"
        );
    }
}

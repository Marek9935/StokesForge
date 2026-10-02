//! Mise à l'échelle des formes en cache, sans flouter le texte (issue #18).
//!
//! `Shape::transform` d'epaint agrandit le maillage d'un `Shape::Text` : les
//! glyphes restent ceux rastérisés à la taille **logique**, donc un graphe
//! agrandi montre un bitmap étiré, flou. Ici le texte est remis en page à la
//! taille **affichée** (police × échelle) pour que l'atlas rastérise à la
//! bonne résolution ; les positions, elles, restent celles de la mise en page
//! logique (la mesure de texte, invariant 4, n'est pas touchée : le pipeline
//! a déjà décidé de tout avant cette étape).
//!
//! # Les formules Typst (issue #31)
//!
//! Une formule est un `Shape::Mesh` texturé (`crate::typst_screen`), dont
//! l'image est rastérisée à la taille logique : étirée, elle floutait comme
//! le texte avant #18. Le maillage garde ici sa géométrie — transformée
//! comme toute forme, donc à la même place et de la même taille — mais
//! désigne une **autre texture** : la même formule rastérisée à
//! `pixels_per_point × échelle`, par [`FormulaTexture::rerender`], à partir
//! du document déjà compilé (pas de recompilation, pas de nouvelle mesure).
//! Même boîte, même couverture de texture (0..1) : seule la finesse change.
//!
//! Une variante rastérisait directement à l'échelle affichée pendant la
//! collecte. Écartée : il aurait fallu mettre le rectangle affiché dans la
//! clé du cache principal, donc rejouer tout le pipeline Plotters à chaque
//! pas d'un redimensionnement.
//!
//! Les textures agrandies vivent dans [`ScaledShapes`], hors du
//! `FormulaCache` de la vue : elles ne comptent pas dans sa limite et
//! disparaissent avec la version mise à l'échelle qui les désigne. Une
//! texture déjà faite à la bonne résolution est reprise d'une version à
//! l'autre (déplacer le graphe sans le redimensionner ne rastérise rien).

use std::collections::HashMap;
use std::sync::Arc;

use egui::emath::TSTransform;
use egui::epaint::TextShape;
use egui::{Shape, TextureId};
use plotyx_render::formula::FormulaCache;

use crate::typst_screen::FormulaTexture;

/// Transforme `shape` par `transform` (mise à l'échelle uniforme + décalage),
/// avec un texte re-rastérisé à la taille affichée.
///
/// `retexture` donne, pour la texture d'un maillage, celle qui doit la
/// remplacer à l'échelle affichée — `None` pour la garder (maillage qui
/// n'est pas une formule, ou échelle 1).
pub fn transform_shape(
    context: &egui::Context,
    shape: &mut Shape,
    transform: TSTransform,
    retexture: &mut dyn FnMut(TextureId) -> Option<TextureId>,
) {
    match shape {
        Shape::Text(text) => transform_text(context, text, transform),
        Shape::Vec(shapes) => {
            for inner in shapes {
                transform_shape(context, inner, transform, retexture);
            }
        }
        Shape::Mesh(mesh) => {
            let formula = retexture(mesh.texture_id);
            if let Some(id) = formula {
                Arc::make_mut(mesh).texture_id = id;
            }
            shape.transform(transform);
            if let (Some(id), Shape::Mesh(mesh)) = (formula, &mut *shape) {
                let size = context.tex_manager().read().meta(id).map(|meta| meta.size);
                if let Some(size) = size {
                    snap_to_pixels(Arc::make_mut(mesh), size, context.pixels_per_point());
                }
            }
        }
        other => other.transform(transform),
    }
}

/// Cale le maillage d'une formule sur la grille des pixels physiques, à la
/// taille exacte de sa texture (issue #86).
///
/// Une formule est une image : posée à une fraction de pixel, ou étirée
/// d'une fraction de pixel, le filtrage linéaire la mélange avec ses voisins
/// et les traits fins deviennent flous — sensible surtout sur un petit texte
/// (une figure réduite dans une petite fenêtre), peu visible sur un grand.
/// Le coin haut-gauche de la boîte tombe donc sur un pixel entier et la
/// boîte mesure exactement `size` pixels (la texture est rastérisée à
/// l'arrondi supérieur, d'où l'écart). Le décalage est inférieur à un pixel.
///
/// Sans effet sur un maillage qui n'est pas un quadrilatère aligné sur les
/// axes (une formule tournée d'un angle quelconque ne se cale pas).
fn snap_to_pixels(mesh: &mut egui::epaint::Mesh, size: [usize; 2], pixels_per_point: f32) {
    let bounds = mesh.calc_bounds();
    if mesh.vertices.len() != 4 || !bounds.is_positive() || pixels_per_point <= 0.0 {
        return;
    }
    let axis_aligned = mesh.vertices.iter().all(|vertex| {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        (near(vertex.pos.x, bounds.min.x) || near(vertex.pos.x, bounds.max.x))
            && (near(vertex.pos.y, bounds.min.y) || near(vertex.pos.y, bounds.max.y))
    });
    if !axis_aligned {
        return;
    }
    // Un quart de tour échange largeur et hauteur de la texture.
    let (w, h) = (size[0] as f32, size[1] as f32);
    let (physical_w, physical_h) = (
        bounds.width() * pixels_per_point,
        bounds.height() * pixels_per_point,
    );
    let (target_w, target_h) = if (physical_w - w).abs() + (physical_h - h).abs()
        <= (physical_w - h).abs() + (physical_h - w).abs()
    {
        (w, h)
    } else {
        (h, w)
    };
    let min = egui::pos2(
        (bounds.min.x * pixels_per_point).round() / pixels_per_point,
        (bounds.min.y * pixels_per_point).round() / pixels_per_point,
    );
    let max = min + egui::vec2(target_w, target_h) / pixels_per_point;
    let middle = bounds.center();
    for vertex in &mut mesh.vertices {
        vertex.pos = egui::pos2(
            if vertex.pos.x < middle.x {
                min.x
            } else {
                max.x
            },
            if vertex.pos.y < middle.y {
                min.y
            } else {
                max.y
            },
        );
    }
}

fn transform_text(context: &egui::Context, text: &mut TextShape, transform: TSTransform) {
    let scale = transform.scaling;
    if (scale - 1.0).abs() < f32::EPSILON {
        text.transform(transform);
        return;
    }
    let mut job = (*text.galley.job).clone();
    for section in &mut job.sections {
        section.format.font_id.size *= scale;
        section.format.line_height = section.format.line_height.map(|height| height * scale);
        section.format.extra_letter_spacing *= scale;
    }
    text.galley = context.fonts_mut(|fonts| fonts.layout_job(job));
    text.pos = transform * text.pos;
    text.underline.width *= scale;
}

/// Ce qui identifie une version mise à l'échelle des formes du cache.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaledKey {
    /// La figure logique dont elle vient.
    pub source: crate::cache::CacheKey,
    /// Rectangle affiché (échelle **et** position).
    pub display: egui::Rect,
}

/// Formes mises à l'échelle, gardées d'une image à l'autre tant que la figure
/// et le rectangle affiché ne changent pas.
#[derive(Default)]
pub struct ScaledShapes {
    key: Option<ScaledKey>,
    shapes: Vec<Shape>,
    /// Les formules rastérisées à l'échelle affichée, par texture d'origine
    /// (celle du maillage en cache) — voir la tête de module.
    formulas: HashMap<TextureId, Arc<FormulaTexture>>,
}

impl std::fmt::Debug for ScaledShapes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScaledShapes")
            .field("key", &self.key)
            .field("shapes", &self.shapes.len())
            .field("formulas", &self.formulas.len())
            .finish()
    }
}

impl ScaledShapes {
    /// Les formes de `source` à l'échelle de `key.display`, recalculées si la
    /// clé a changé. `formulas` est le cache qui a produit les textures des
    /// formules de `source` : c'est là que se retrouve le document d'une
    /// formule à rastériser de nouveau.
    pub fn get(
        &mut self,
        context: &egui::Context,
        key: ScaledKey,
        source: &[Shape],
        transform: TSTransform,
        formulas: &FormulaCache<FormulaTexture>,
    ) -> &[Shape] {
        if self.key != Some(key) {
            let pixels_per_point = key.source.pixels_per_point * transform.scaling;
            let unscaled = (transform.scaling - 1.0).abs() < f32::EPSILON;
            let previous = std::mem::take(&mut self.formulas);
            let mut kept = HashMap::new();
            // Construit au premier maillage rencontré seulement : une
            // figure sans formule ne parcourt pas le cache.
            let mut by_id: Option<HashMap<TextureId, Arc<FormulaTexture>>> = None;
            let mut retexture = |id: TextureId| -> Option<TextureId> {
                let by_id = by_id.get_or_insert_with(|| {
                    formulas
                        .values()
                        .into_iter()
                        .map(|texture| (texture.id(), texture))
                        .collect()
                });
                if unscaled {
                    // Pas de nouvelle texture, mais la formule est reconnue :
                    // son maillage est calé sur les pixels (#86).
                    return by_id.contains_key(&id).then_some(id);
                }
                let sharp = match previous.get(&id) {
                    Some(texture) if texture.pixels_per_point() == pixels_per_point => {
                        Arc::clone(texture)
                    }
                    _ => Arc::new(by_id.get(&id)?.rerender(context, pixels_per_point)?),
                };
                let sharp_id = sharp.id();
                kept.insert(id, sharp);
                Some(sharp_id)
            };
            self.shapes.clear();
            self.shapes.extend(source.iter().cloned().map(|mut shape| {
                transform_shape(context, &mut shape, transform, &mut retexture);
                shape
            }));
            self.formulas = kept;
            self.key = Some(key);
        }
        &self.shapes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, FontId, Pos2};

    fn contexte() -> egui::Context {
        let context = egui::Context::default();
        let mut sortie = context.run_ui(egui::RawInput::default(), |_| {});
        sortie.textures_delta.clear();
        context
    }

    fn texte(context: &egui::Context, size: f32) -> Shape {
        let galley = context.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                "Tension (V)".to_owned(),
                FontId::proportional(size),
                Color32::BLACK,
            )
        });
        Shape::galley(Pos2::new(10.0, 20.0), galley, Color32::BLACK)
    }

    fn taille_de_police(shape: &Shape) -> f32 {
        let Shape::Text(text) = shape else {
            panic!("un texte attendu");
        };
        text.galley.job.sections[0].format.font_id.size
    }

    /// #18 : agrandir un graphe re-rastérise le texte à la taille affichée
    /// au lieu d'étirer le maillage des glyphes (flou).
    #[test]
    fn le_texte_agrandi_est_remis_en_page_a_la_taille_affichee() {
        let context = contexte();
        let transform = TSTransform::new(egui::vec2(5.0, 7.0), 2.5);
        let mut shape = texte(&context, 12.0);
        let logique = match &shape {
            Shape::Text(text) => text.galley.rect.width(),
            _ => unreachable!(),
        };
        transform_shape(&context, &mut shape, transform, &mut |_| None);
        assert!((taille_de_police(&shape) - 30.0).abs() < 1e-4);
        let Shape::Text(text) = &shape else {
            unreachable!()
        };
        assert_eq!(text.pos, Pos2::new(30.0, 57.0));
        // La largeur reste proportionnelle : la mise en page ne dérive pas.
        let ratio = text.galley.rect.width() / logique;
        assert!((ratio - 2.5).abs() < 0.15, "ratio de largeur {ratio}");
    }

    #[test]
    fn un_texte_imbrique_est_aussi_remis_en_page() {
        let context = contexte();
        let mut shape = Shape::Vec(vec![texte(&context, 10.0)]);
        transform_shape(
            &context,
            &mut shape,
            TSTransform::new(egui::Vec2::ZERO, 2.0),
            &mut |_| None,
        );
        let Shape::Vec(inner) = &shape else {
            unreachable!()
        };
        assert!((taille_de_police(&inner[0]) - 20.0).abs() < 1e-4);
    }

    fn cle(display: f32) -> ScaledKey {
        ScaledKey {
            source: crate::cache::CacheKey {
                revision: Default::default(),
                rect: egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0)),
                pixels_per_point: 1.0,
                view_generation: 0,
            },
            display: egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(display, display)),
        }
    }

    #[test]
    fn les_formes_mises_a_l_echelle_sont_gardees_tant_que_la_cle_ne_change_pas() {
        let context = contexte();
        let source = [texte(&context, 12.0)];
        let key = cle(200.0);
        let transform = TSTransform::new(egui::Vec2::ZERO, 2.0);
        let formules = FormulaCache::new();
        let mut cache = ScaledShapes::default();
        let premier = cache
            .get(&context, key, &source, transform, &formules)
            .as_ptr();
        let second = cache
            .get(&context, key, &source, transform, &formules)
            .as_ptr();
        assert_eq!(premier, second, "pas de recalcul à clé identique");
        assert!(
            (taille_de_police(&cache.get(&context, key, &source, transform, &formules)[0]) - 24.0)
                .abs()
                < 1e-4
        );
    }

    /// Une formule `$lambda$` rastérisée à `pixels_per_point` 1, et son
    /// maillage ancré en (10, 20).
    fn formule(
        context: &egui::Context,
        cache: &FormulaCache<FormulaTexture>,
    ) -> (Arc<FormulaTexture>, Shape) {
        let choice = plotyx_render::FontChoice::default();
        let screen = crate::typst_screen::TypstScreen::new(context, cache, &choice)
            .expect("résolution des polices échouée");
        let texture = screen
            .render("$lambda$", 14.0, [0, 0, 0, 255], 1.0)
            .expect("rendu");
        let anchor = plotters_backend::text_anchor::Pos::new(
            plotters_backend::text_anchor::HPos::Left,
            plotters_backend::text_anchor::VPos::Top,
        );
        let shape = crate::typst_screen::mesh(
            &texture,
            Pos2::new(10.0, 20.0),
            anchor,
            plotters_backend::FontTransform::None,
        );
        (texture, shape)
    }

    fn maillage(shape: &Shape) -> &egui::epaint::Mesh {
        let Shape::Mesh(mesh) = shape else {
            panic!("un maillage attendu");
        };
        mesh
    }

    /// Issue #31 : agrandie deux fois, une formule désigne une texture de
    /// deux fois plus de pixels de côté, dans **la même boîte** que le
    /// maillage simplement étiré — la finesse change, pas la mise en page.
    #[test]
    fn une_formule_agrandie_est_rasterisee_a_la_taille_affichee() {
        let context = contexte();
        let formules = FormulaCache::new();
        let (base, source) = formule(&context, &formules);
        let transform = TSTransform::new(egui::vec2(5.0, 7.0), 2.0);

        let mut etire = source.clone();
        etire.transform(transform);
        let mut cache = ScaledShapes::default();
        let agrandi = &cache.get(&context, cle(200.0), &[source], transform, &formules)[0];

        let (avant, apres) = (maillage(&etire), maillage(agrandi));
        // Le maillage est calé sur les pixels (#86) : la boîte ne bouge que
        // de moins d'un pixel physique (ppp 1 ici), pas de mise en page.
        let (avant, apres_boite) = (avant.calc_bounds(), apres.calc_bounds());
        assert!(
            (avant.min - apres_boite.min).abs().max_elem() <= 1.0
                && (avant.size() - apres_boite.size()).abs().max_elem() <= 1.0,
            "la boîte a bougé : {avant:?} -> {apres_boite:?}"
        );
        assert_ne!(apres.texture_id, base.id(), "texture d'origine étirée");
        let fine = context
            .tex_manager()
            .read()
            .meta(apres.texture_id)
            .expect("texture agrandie absente")
            .size;
        let [w, h] = base.size_pixels();
        assert!(
            fine[0].abs_diff(2 * w) <= 1 && fine[1].abs_diff(2 * h) <= 1,
            "{fine:?} n'est pas le double de {:?}",
            [w, h]
        );
        assert_eq!(
            formules.len(),
            1,
            "la texture agrandie est entrée dans le cache de la vue"
        );
    }

    /// Le témoin : à l'échelle 1 (un simple déplacement), rien n'est
    /// rastérisé de nouveau — le maillage garde sa texture. Et une texture
    /// agrandie déjà faite est reprise quand seule la position change.
    #[test]
    fn deplacer_sans_agrandir_ne_rasterise_rien() {
        let context = contexte();
        let formules = FormulaCache::new();
        let (base, source) = formule(&context, &formules);
        let mut cache = ScaledShapes::default();

        let deplace = TSTransform::new(egui::vec2(30.0, 0.0), 1.0);
        let shapes = cache.get(
            &context,
            cle(100.0),
            std::slice::from_ref(&source),
            deplace,
            &formules,
        );
        assert_eq!(maillage(&shapes[0]).texture_id, base.id());

        let double = TSTransform::new(egui::Vec2::ZERO, 2.0);
        let premier = maillage(
            &cache.get(
                &context,
                cle(200.0),
                std::slice::from_ref(&source),
                double,
                &formules,
            )[0],
        )
        .texture_id;
        let decale = TSTransform::new(egui::vec2(40.0, 0.0), 2.0);
        let mut ailleurs = cle(200.0);
        ailleurs.display = ailleurs.display.translate(egui::vec2(40.0, 0.0));
        let second =
            maillage(&cache.get(&context, ailleurs, &[source], decale, &formules)[0]).texture_id;
        assert_eq!(
            premier, second,
            "même échelle : la texture agrandie est reprise"
        );
    }

    /// Issue #86 : à l'écran, le maillage d'une formule tombe sur un pixel
    /// physique entier et mesure exactement sa texture, y compris quand la
    /// figure est réduite à une échelle qui ne tombe pas juste.
    #[test]
    fn une_formule_reduite_est_calee_sur_les_pixels() {
        let context = contexte();
        let formules = FormulaCache::new();
        let (_, source) = formule(&context, &formules);
        let transform = TSTransform::new(egui::vec2(5.3, 7.7), 0.63);
        let mut cache = ScaledShapes::default();
        let formes = cache.get(&context, cle(63.0), &[source], transform, &formules);
        let boite = maillage(&formes[0]).calc_bounds();
        let ppp = context.pixels_per_point();
        let entier = |v: f32| ((v * ppp) - (v * ppp).round()).abs() < 1e-3;
        assert!(entier(boite.min.x) && entier(boite.min.y), "{boite:?}");
        let taille = context
            .tex_manager()
            .read()
            .meta(maillage(&formes[0]).texture_id)
            .expect("texture")
            .size;
        assert!((boite.width() * ppp - taille[0] as f32).abs() < 1e-3);
        assert!((boite.height() * ppp - taille[1] as f32).abs() < 1e-3);
    }
}

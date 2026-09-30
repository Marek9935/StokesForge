//! Mise à l'échelle des formes en cache, sans flouter le texte (issue #18).
//!
//! `Shape::transform` d'epaint agrandit le maillage d'un `Shape::Text` : les
//! glyphes restent ceux rastérisés à la taille **logique**, donc un graphe
//! agrandi montre un bitmap étiré, flou. Ici le texte est remis en page à la
//! taille **affichée** (police × échelle) pour que l'atlas rastérise à la
//! bonne résolution ; les positions, elles, restent celles de la mise en page
//! logique (la mesure de texte, invariant 4, n'est pas touchée : le pipeline
//! a déjà décidé de tout avant cette étape).

use egui::emath::TSTransform;
use egui::epaint::TextShape;
use egui::Shape;

/// Transforme `shape` par `transform` (mise à l'échelle uniforme + décalage),
/// avec un texte re-rastérisé à la taille affichée.
pub fn transform_shape(context: &egui::Context, shape: &mut Shape, transform: TSTransform) {
    match shape {
        Shape::Text(text) => transform_text(context, text, transform),
        Shape::Vec(shapes) => {
            for inner in shapes {
                transform_shape(context, inner, transform);
            }
        }
        other => other.transform(transform),
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
#[derive(Debug, Default)]
pub struct ScaledShapes {
    key: Option<ScaledKey>,
    shapes: Vec<Shape>,
}

impl ScaledShapes {
    /// Les formes de `source` à l'échelle de `key.display`, recalculées si la
    /// clé a changé.
    pub fn get(
        &mut self,
        context: &egui::Context,
        key: ScaledKey,
        source: &[Shape],
        transform: TSTransform,
    ) -> &[Shape] {
        if self.key != Some(key) {
            self.shapes.clear();
            self.shapes.extend(source.iter().cloned().map(|mut shape| {
                transform_shape(context, &mut shape, transform);
                shape
            }));
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
        transform_shape(&context, &mut shape, transform);
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
        );
        let Shape::Vec(inner) = &shape else {
            unreachable!()
        };
        assert!((taille_de_police(&inner[0]) - 20.0).abs() < 1e-4);
    }

    #[test]
    fn les_formes_mises_a_l_echelle_sont_gardees_tant_que_la_cle_ne_change_pas() {
        let context = contexte();
        let source = [texte(&context, 12.0)];
        let key = ScaledKey {
            source: crate::cache::CacheKey {
                revision: Default::default(),
                rect: egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0)),
                pixels_per_point: 1.0,
                view_generation: 0,
            },
            display: egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 200.0)),
        };
        let transform = TSTransform::new(egui::Vec2::ZERO, 2.0);
        let mut cache = ScaledShapes::default();
        let premier = cache.get(&context, key, &source, transform).as_ptr();
        let second = cache.get(&context, key, &source, transform).as_ptr();
        assert_eq!(premier, second, "pas de recalcul à clé identique");
        assert!(
            (taille_de_police(&cache.get(&context, key, &source, transform)[0]) - 24.0).abs()
                < 1e-4
        );
    }
}

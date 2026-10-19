//! Brique « état vide » (#232, S-P3) : une phrase grisée, centrée, qui dit ce
//! qui manque **et** le geste pour le créer (« No columns in this sheet. Use
//! Workbook › Add Column »). Une surface vide ne reste jamais muette ni ne
//! pose son propre `ui.weak`.

use eframe::egui;

use crate::theme;

/// Peint la phrase `text` au centre de la place disponible.
pub(super) fn empty_state(ui: &mut egui::Ui, text: impl Into<String>) {
    let text = text.into();
    ui.vertical_centered(|ui| {
        ui.add_space(theme::SPACE_SECTION);
        ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La phrase est peinte, grisée (couleur de texte faible), au milieu de
    /// la largeur disponible.
    #[test]
    fn la_phrase_est_grisee_et_centree() {
        let context = egui::Context::default();
        crate::theme::install(&context);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| empty_state(ui, "Rien ici"),
        );
        output.textures_delta.clear();
        let (center, color) = output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == "Rien ici" => Some((
                    painted
                        .clip_rect
                        .center()
                        .x
                        .min(shape.visual_bounding_rect().center().x),
                    shape.galley.job.sections[0].format.color,
                )),
                _ => None,
            })
            .expect("phrase peinte");
        assert!((center - 200.0).abs() < 20.0, "centre {center}");
        assert_ne!(color, egui::Color32::PLACEHOLDER);
    }
}

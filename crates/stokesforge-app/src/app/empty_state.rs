//! "Empty state" building block (#232, S-P3): a greyed-out, centered sentence
//! that says what is missing **and** the gesture to create it ("No columns in
//! this sheet. Use Workbook › Add Column"). An empty surface is never silent
//! and never places its own `ui.weak`.

use eframe::egui;

use crate::theme;

/// Paints the sentence `text` at the center of the available space.
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

    /// The sentence is painted, greyed out (weak text color), in the middle of
    /// the available width.
    #[test]
    fn the_sentence_is_greyed_out_and_centered() {
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
            |ui| empty_state(ui, "Nothing here"),
        );
        output.textures_delta.clear();
        let (center, color) = output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == "Nothing here" => Some((
                    painted
                        .clip_rect
                        .center()
                        .x
                        .min(shape.visual_bounding_rect().center().x),
                    shape.galley.job.sections[0].format.color,
                )),
                _ => None,
            })
            .expect("sentence painted");
        assert!((center - 200.0).abs() < 20.0, "center {center}");
        assert_ne!(color, egui::Color32::PLACEHOLDER);
    }
}

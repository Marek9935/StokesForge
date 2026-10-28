//! The common look of a list field (#234, S-N1/S-N9): a single frame, a
//! **filled arrow inside**, on the right — the one of egui's `ComboBox`.
//!
//! The two families differ in one deliberate way: the **fill**.
//! [`editable_fill`] (the theme's input field fill, white in light mode) for
//! what is typed on the keyboard — the number field with a list —,
//! [`list_only_fill`] (the button fill, gray) for a closed choice. Both come
//! from the `Visuals` set by `theme::install`, never from a hard-coded color
//! (`no_hardcoded_color_outside_the_theme`).
//!
//! The `ComboBox` keeps its own drawing (egui); this module takes over its
//! arrow and fills so that `number_field` no longer diverges from it.

use eframe::egui;

/// Room reserved to the right of the text for the arrow, in points.
pub const ARROW_ROOM: f32 = 18.0;

/// Fill of a field editable from the keyboard.
pub fn editable_fill(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.extreme_bg_color
}

/// Fill of a list that only picks (the one of a button or a `ComboBox`).
pub fn list_only_fill(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.widgets.inactive.weak_bg_fill
}

/// The arrow's rectangle, inside `field` on the right.
pub fn arrow_rect(field: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(field.right() - ARROW_ROOM, field.top()),
        field.right_bottom(),
    )
}

/// Paints the filled arrow (downward triangle) in `rect`, hover tint
/// included.
pub fn paint_arrow(ui: &egui::Ui, rect: egui::Rect, response: &egui::Response) {
    let color = ui.style().interact(response).fg_stroke.color;
    let triangle = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(rect.width() * 0.45, rect.height() * 0.3),
    );
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            triangle.left_top(),
            triangle.right_top(),
            triangle.center_bottom(),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_fills_differ_and_come_from_the_theme() {
        for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
            assert_ne!(editable_fill(&visuals), list_only_fill(&visuals));
            assert_eq!(editable_fill(&visuals), visuals.extreme_bg_color);
            assert_eq!(
                list_only_fill(&visuals),
                visuals.widgets.inactive.weak_bg_fill
            );
        }
    }

    #[test]
    fn the_arrow_is_inside_the_field_frame() {
        let field = egui::Rect::from_min_size(egui::pos2(10.0, 5.0), egui::vec2(80.0, 20.0));
        let arrow = arrow_rect(field);
        assert!(field.contains_rect(arrow));
        assert_eq!(arrow.right(), field.right());
    }
}

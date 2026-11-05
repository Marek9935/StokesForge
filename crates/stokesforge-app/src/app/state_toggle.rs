//! **The** two-state toggle button of the application (#426): ONE button with
//! one hit area that shows both of its states. Both labels are always visible
//! and never change; the part for the current state is highlighted in the
//! theme's selection colour. A click anywhere on the button, on either part,
//! switches to the other state, so the same spot toggles back and forth.
//!
//! It exists for a setting with two states that must show which one is on
//! without a button that renames itself (Coarse | Fine, Regular | Free, Grows
//! with the axes | Fixed). Not for a choice among more options, not for a
//! menu.
//!
//! Look: on hover the **whole** button is outlined with the stroke of a hovered
//! button of the theme (`Visuals::widgets.hovered.bg_stroke`, the accent colour
//! the `Auto` button of the fit window gets from the same `Visuals`); a grey
//! on/off button gets the same outline through [`paint_hover_outline`]. The
//! segments touch and only the outer corners are rounded. Tab reaches it once
//! and Space or Enter toggles it. Greyed (`disabled` carries the reason shown
//! on hover), it neither reacts nor outlines. Every colour comes from the
//! `Visuals`; egui's selected button is not used, so the guard
//! `no_selectable_button_outside_a_menu` stays as it is.

use eframe::egui;

/// Width one label takes: its text and the padding of a button.
fn part_width(ui: &egui::Ui, label: &str, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(label.to_owned(), font.clone(), egui::Color32::WHITE)
        .size()
        .x
        + 2.0 * ui.spacing().button_padding.x
}

/// Width the whole toggle takes, for `keep_together`.
pub(super) fn state_toggle_width(ui: &egui::Ui, labels: [&str; 2]) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    labels
        .iter()
        .map(|label| part_width(ui, label, &font))
        .sum()
}

/// The outline of a hovered button: the stroke of the theme, around the whole
/// `rect`. Shared by the toggle and by the on/off buttons beside it, whose
/// selected look otherwise hides egui's own hover outline.
pub(super) fn paint_hover_outline(ui: &egui::Ui, rect: egui::Rect) {
    let visuals = ui.visuals();
    ui.painter().rect_stroke(
        rect,
        visuals.widgets.hovered.corner_radius,
        visuals.widgets.hovered.bg_stroke,
        egui::StrokeKind::Inside,
    );
}

/// Paints the toggle and returns whether it was clicked (or activated with
/// Space or Enter): the caller then switches to the other state. `selected` is
/// the index of the current state (0 or 1), `hint` the one tooltip, `disabled`
/// the reason when the setting has no effect now.
pub(super) fn state_toggle(
    ui: &mut egui::Ui,
    labels: [&str; 2],
    selected: usize,
    hint: &str,
    disabled: Option<&str>,
) -> bool {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widths = labels.map(|label| part_width(ui, label, &font));
    let size = egui::vec2(widths[0] + widths[1], ui.spacing().interact_size.y);
    let sense = if disabled.is_some() {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            disabled.is_none(),
            format!("{} | {}", labels[0], labels[1]),
        )
    });
    let round = ui.visuals().widgets.inactive.corner_radius;
    let parts = [
        (
            egui::Rect::from_min_size(rect.min, egui::vec2(widths[0], size.y)),
            egui::CornerRadius {
                nw: round.nw,
                sw: round.sw,
                ne: 0,
                se: 0,
            },
        ),
        (
            egui::Rect::from_min_size(
                egui::pos2(rect.min.x + widths[0], rect.min.y),
                egui::vec2(widths[1], size.y),
            ),
            egui::CornerRadius {
                nw: 0,
                sw: 0,
                ne: round.ne,
                se: round.se,
            },
        ),
    ];
    let visuals = ui.visuals();
    let rest = visuals.widgets.inactive.bg_stroke;
    let painter = ui.painter();
    for (index, (part, corners)) in parts.into_iter().enumerate() {
        let (fill, color) = if disabled.is_some() {
            (
                visuals.widgets.inactive.weak_bg_fill,
                visuals.weak_text_color(),
            )
        } else if index == selected {
            (visuals.selection.bg_fill, visuals.selection.stroke.color)
        } else {
            (
                visuals.widgets.inactive.weak_bg_fill,
                visuals.widgets.inactive.text_color(),
            )
        };
        painter.rect_filled(part, corners, fill);
        painter.text(
            part.center(),
            egui::Align2::CENTER_CENTER,
            labels[index],
            font.clone(),
            color,
        );
    }
    painter.rect_stroke(rect, round, rest, egui::StrokeKind::Inside);
    if disabled.is_none() && (response.hovered() || response.has_focus()) {
        paint_hover_outline(ui, rect);
    }
    match disabled {
        Some(reason) => {
            response.on_hover_text(reason);
            false
        }
        None => response.on_hover_text(hint).clicked(),
    }
}

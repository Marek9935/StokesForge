//! **The** two-or-more-part choice of the application (#426): a row of touching
//! segments, all always visible, the active one in the theme's selection
//! colour. It is for a setting that has two states and must show which one is
//! on without changing its own label (a toggle button that renames itself
//! hides the state it is not in).
//!
//! One click on an inactive segment selects it; a click on the active one does
//! nothing and reports nothing. The segments touch, only the outer corners of
//! the group are rounded, and Tab reaches each one like any other button. The
//! colours come from the `Visuals` only; egui's selected button is not used,
//! so the guard `no_selectable_button_outside_a_menu` stays as it is.

use eframe::egui;

/// One segment: its label (never changed by the state) and its tooltip.
pub(super) struct Segment<'a> {
    pub label: &'a str,
    pub hint: &'a str,
}

/// Width the group takes: the labels and the padding of every segment.
pub(super) fn segmented_width(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    labels
        .iter()
        .map(|label| segment_width(ui, label, &font))
        .sum()
}

fn segment_width(ui: &egui::Ui, label: &str, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(label.to_owned(), font.clone(), egui::Color32::WHITE)
        .size()
        .x
        + 2.0 * ui.spacing().button_padding.x
}

/// Paints the choice and returns the index of the segment the user just
/// selected, `None` when nothing changed (no click, or a click on the active
/// segment). `disabled` is the reason shown, instead of the hints, when the
/// choice has no effect now: the segments are then dimmed and do not react.
pub(super) fn segmented_choice(
    ui: &mut egui::Ui,
    id_salt: &str,
    segments: &[Segment<'_>],
    selected: usize,
    disabled: Option<&str>,
) -> Option<usize> {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widths: Vec<f32> = segments
        .iter()
        .map(|segment| segment_width(ui, segment.label, &font))
        .collect();
    let height = ui.spacing().interact_size.y;
    let (total, _) = ui.allocate_exact_size(
        egui::vec2(widths.iter().sum(), height),
        egui::Sense::hover(),
    );
    let base_id = ui.id().with(id_salt);
    let round = ui.visuals().widgets.inactive.corner_radius;
    let sense = if disabled.is_some() {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let mut chosen = None;
    let mut left = total.left();
    for (index, segment) in segments.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            egui::pos2(left, total.top()),
            egui::vec2(widths[index], height),
        );
        left += widths[index];
        let response = ui.interact(rect, base_id.with(index), sense);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                disabled.is_none(),
                index == selected,
                segment.label,
            )
        });
        let corners = egui::CornerRadius {
            nw: if index == 0 { round.nw } else { 0 },
            sw: if index == 0 { round.sw } else { 0 },
            ne: if index + 1 == segments.len() {
                round.ne
            } else {
                0
            },
            se: if index + 1 == segments.len() {
                round.se
            } else {
                0
            },
        };
        let visuals = ui.visuals();
        let active = index == selected;
        let (fill, text_color) = if disabled.is_some() {
            (
                visuals.widgets.inactive.weak_bg_fill,
                visuals.weak_text_color(),
            )
        } else if active {
            (visuals.selection.bg_fill, visuals.selection.stroke.color)
        } else if response.hovered() || response.has_focus() {
            (
                visuals.widgets.hovered.weak_bg_fill,
                visuals.widgets.hovered.text_color(),
            )
        } else {
            (
                visuals.widgets.inactive.weak_bg_fill,
                visuals.widgets.inactive.text_color(),
            )
        };
        let outline = visuals.widgets.inactive.bg_stroke;
        let focus = visuals.selection.stroke;
        let painter = ui.painter();
        painter.rect_filled(rect, corners, fill);
        painter.rect_stroke(rect, corners, outline, egui::StrokeKind::Inside);
        if response.has_focus() {
            painter.rect_stroke(rect, corners, focus, egui::StrokeKind::Inside);
        }
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            segment.label,
            font.clone(),
            text_color,
        );
        match disabled {
            Some(reason) => {
                response.on_hover_text(reason);
            }
            None => {
                if response.on_hover_text(segment.hint).clicked() && !active {
                    chosen = Some(index);
                }
            }
        }
    }
    chosen
}

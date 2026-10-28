//! The shared building block for menu sections (#155, grammar of
//! `docs/menus.md`): every menu, menu bar or right-click, arranges its
//! entries in **sections** (main action, create, clipboard, organize, delete,
//! exit, `Properties…`) and places a thin line only **between two present
//! sections** — never at the start, at the end, or doubled.
//!
//! Why a building block rather than hand-written `ui.separator()` calls: a
//! menu whose section depends on the target ("Show Data" only on a curve) left
//! an orphan line as soon as the section disappeared. Here the caller only
//! says whether the section **exists** for this target; the line follows. The
//! only `ui.separator()` of a menu is the one in [`section_separator`] (guard
//! `no_raw_separator_in_a_menu`, `tests/guards.rs`, no exemption).
//!
//! "Absent" (not applicable to the target) is not "greyed out" (relevant but
//! momentarily impossible, with a `.disabled` tooltip): an absent section is
//! not called at all, a greyed-out section is painted with its entries greyed
//! out.
//!
//! An exclusive choice in a menu (Type ▸, Role ▸, language, SVG text) is
//! painted by [`menu_choice`]: a **lone check mark** in front of the current
//! option, with no radio and no selection background (S-C3, #227).

use eframe::egui;

/// Counts the sections already painted in a menu to separate the next one.
#[derive(Debug, Default)]
pub(super) struct MenuSections {
    painted: usize,
    separators: usize,
}

impl MenuSections {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Paints a section if it is `present` for the target, preceded by a thin
    /// line when a section has already been painted. `paint` is not called for
    /// an absent section.
    pub(super) fn section(
        &mut self,
        ui: &mut egui::Ui,
        present: bool,
        paint: impl FnOnce(&mut egui::Ui),
    ) {
        if !present {
            return;
        }
        if self.painted > 0 {
            section_separator(ui);
            self.separators += 1;
        }
        paint(ui);
        self.painted += 1;
    }
}

/// The thin line between two sections: the only separator of a menu.
fn section_separator(ui: &mut egui::Ui) {
    ui.add(egui::Separator::default().spacing(4.0));
}

/// An option of an exclusive choice in a menu (S-C3, `docs/menus.md`): a
/// check mark in the margin in front of the current option, nothing in front
/// of the others — never a radio nor the background of a selected button. The
/// margin is reserved for all options, which stay aligned. On hover, the
/// background of a menu entry. Accessibility describes it as a radio button,
/// checked or not.
pub(super) fn menu_choice(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let spacing = ui.spacing();
    let (padding, mark, gap) = (
        spacing.button_padding,
        spacing.icon_width,
        spacing.icon_spacing,
    );
    let galley = text.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let size = egui::vec2(
        padding.x * 2.0 + mark + gap + galley.size().x,
        galley.size().y.max(mark) + padding.y * 2.0,
    );
    let width = size.x.max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, size.y), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            ui.is_enabled(),
            selected,
            galley.text(),
        )
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        }
        let box_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + padding.x + mark / 2.0, rect.center().y),
            egui::Vec2::splat(mark),
        )
        .shrink(mark / 5.0);
        if selected {
            ui.painter().add(egui::Shape::line(
                vec![
                    egui::pos2(box_rect.left(), box_rect.center().y),
                    egui::pos2(box_rect.center().x - 1.0, box_rect.bottom()),
                    egui::pos2(box_rect.right(), box_rect.top()),
                ],
                visuals.fg_stroke,
            ));
        }
        let text_pos = egui::pos2(
            rect.left() + padding.x + mark + gap,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(text_pos, galley, visuals.text_color());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{context, frame_of, image, texts};
    use super::*;

    /// S-C3: a check mark (a three-point line) in front of the current option
    /// only, no radio circle or selection background, and the option labels
    /// aligned. Witness: with no current option, no check mark.
    #[test]
    fn a_menu_choice_checks_only_the_current_option() {
        let context = context();
        let checks = |current: Option<usize>| {
            let shapes = image(&context, Vec::new(), |ui| {
                for (index, text) in ["One", "Two"].into_iter().enumerate() {
                    menu_choice(ui, current == Some(index), text);
                }
            });
            let lines = shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Path(path) if path.points.len() == 3)
                })
                .count();
            let circles = shapes
                .iter()
                .filter(|shape| matches!(shape.shape, egui::Shape::Circle(_)))
                .count();
            (lines, circles, texts(&shapes))
        };
        let (lines, circles, painted) = checks(Some(1));
        assert_eq!(lines, 1, "one check mark, in front of the current option");
        assert_eq!(circles, 0, "no radio");
        let one = painted.iter().find(|t| t.text == "One").unwrap().center;
        let two = painted.iter().find(|t| t.text == "Two").unwrap().center;
        assert!(two.y > one.y);
        assert_eq!(
            checks(None).0,
            0,
            "witness: with no current option, no check mark"
        );
    }

    fn separators(present: &[bool]) -> (usize, usize) {
        let context = context();
        let mut sections = MenuSections::new();
        frame_of(&context, egui::vec2(300.0, 300.0), Vec::new(), |ui| {
            sections = MenuSections::new();
            for &is_present in present {
                sections.section(ui, is_present, |ui| {
                    ui.label("x");
                });
            }
        });
        (sections.painted, sections.separators)
    }

    #[test]
    fn a_line_only_between_two_present_sections() {
        assert_eq!(separators(&[true, true, true]), (3, 2));
        assert_eq!(separators(&[true]), (1, 0));
        assert_eq!(separators(&[]), (0, 0));
    }

    #[test]
    fn an_absent_section_leaves_no_line_at_the_start_the_end_or_doubled() {
        assert_eq!(separators(&[false, true, false]), (1, 0));
        assert_eq!(separators(&[true, false, false, true]), (2, 1));
        assert_eq!(separators(&[false, false]), (0, 0));
    }

    #[test]
    fn an_absent_section_does_not_call_its_content() {
        let context = context();
        let mut calls = 0;
        frame_of(&context, egui::vec2(300.0, 300.0), Vec::new(), |ui| {
            calls = 0;
            let mut sections = MenuSections::new();
            sections.section(ui, false, |_| calls += 1);
            sections.section(ui, true, |_| calls += 1);
        });
        assert_eq!(calls, 1);
    }
}

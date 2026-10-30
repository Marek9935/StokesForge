//! The "?" help of the `F(x)=` cell of the workbook grid (#404, S-T6).
//!
//! While the cell is edited, a "?" icon sits at its right; hovering it opens a
//! bubble that lists the callable functions by family, one example line
//! each. The list comes from `stokesforge_app_state::formula_help`, itself
//! built from the callees of the formula parser: nothing is copied here.
//!
//! Not [`super::plot_settings`]'s `formula_help`, which is the Typst syntax
//! reminder of the title fields: another content, another preference
//! ([`AppState::show_column_formula_help`]), the same icon through
//! [`hover_help`].
//!
//! The icon only senses the hover, so it never takes the keyboard focus: the
//! cell closes when it loses it (`FocusLoss::Ends`), and the help must not be
//! the thing that closes it.
//!
//! [`AppState::show_column_formula_help`]: stokesforge_app_state::state::AppState::show_column_formula_help

use eframe::egui;
use stokesforge_app_state::formula_help::{self, HelpGroup, HelpRow};
use stokesforge_app_state::tr;

use super::hover_help::hover_help;
use crate::icons::ICON_SIZE;
use crate::theme;

/// The width the icon takes at the right of the field, spacing included: the
/// text field is shortened by it so that a narrow column never clips the icon.
pub(super) fn reserved_width(ui: &egui::Ui) -> f32 {
    ICON_SIZE + ui.spacing().item_spacing.x
}

/// Paints the "?" icon at the current position, with the bubble of the
/// functions. Returns the icon's response.
pub(super) fn icon(ui: &mut egui::Ui) -> egui::Response {
    hover_help(ui, bubble)
}

/// The groups split into two side-by-side columns at the group boundary
/// nearest to half the rows, so that the bubble stays low enough for a small
/// screen. The order of the families is kept.
fn two_columns(groups: &[HelpGroup]) -> (&[HelpGroup], &[HelpGroup]) {
    let total: usize = groups.iter().map(|group| group.rows.len()).sum();
    let mut before = 0usize;
    let mut best = (usize::MAX, 0);
    for (index, group) in groups.iter().enumerate() {
        let gap = (2 * before).abs_diff(total);
        if gap < best.0 {
            best = (gap, index);
        }
        before += group.rows.len();
    }
    groups.split_at(best.1)
}

/// One line of a column of the bubble: a family name, or a function.
enum Line<'a> {
    Heading(&'a str),
    Function(&'a HelpRow),
}

fn lines(groups: &[HelpGroup]) -> Vec<Line<'_>> {
    groups
        .iter()
        .flat_map(|group| {
            std::iter::once(Line::Heading(group.title.as_str()))
                .chain(group.rows.iter().map(Line::Function))
        })
        .collect()
}

/// A cell with nothing in it, to complete a grid row.
fn empty_cell(ui: &mut egui::Ui) {
    ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover());
}

/// The three cells (signature, example, description) of a line, or three
/// empty cells to complete the shorter column. The texts come from variables:
/// the examples are formula code, the rest was translated by `app-state`.
fn paint_line(ui: &mut egui::Ui, line: Option<&Line<'_>>) {
    match line {
        Some(Line::Heading(title)) => {
            ui.label(theme::bold(*title));
            empty_cell(ui);
            empty_cell(ui);
        }
        Some(Line::Function(row)) => {
            ui.monospace(row.signature);
            ui.monospace(row.example);
            ui.label(&row.description);
        }
        None => {
            for _ in 0..3 {
                empty_cell(ui);
            }
        }
    }
}

/// The bubble: a title, a note, then the families in **one** grid of two
/// side-by-side blocks (three cells each, a gap between), so that the bubble
/// stays low enough for a small screen and the layout has no nested
/// unbounded width.
fn bubble(ui: &mut egui::Ui) {
    ui.label(theme::bold(tr!("formula-help-title")));
    ui.label(tr!("formula-help-note"));
    let groups = formula_help::groups();
    let (left, right) = two_columns(&groups);
    let (left, right) = (lines(left), lines(right));
    egui::Grid::new("column_formula_help")
        .num_columns(7)
        .show(ui, |ui| {
            for index in 0..left.len().max(right.len()) {
                paint_line(ui, left.get(index));
                empty_cell(ui);
                paint_line(ui, right.get(index));
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(rows: usize) -> HelpGroup {
        let row = || HelpRow {
            callee: stokesforge_core::Callee::If,
            signature: "f()",
            example: "f()",
            description: String::new(),
        };
        HelpGroup {
            title: String::new(),
            rows: (0..rows).map(|_| row()).collect(),
        }
    }

    /// The split keeps the order and falls on a group boundary near the
    /// middle: 8 + 10 rows then 12, not 8 then 22.
    #[test]
    fn the_columns_split_at_the_group_boundary_nearest_to_half() {
        let groups = [group(8), group(10), group(2), group(6), group(3), group(1)];
        let (left, right) = two_columns(&groups);
        assert_eq!(left.len(), 2);
        assert_eq!(right.len(), 4);
        let one = [group(5)];
        let (left, right) = two_columns(&one);
        assert_eq!(left.len() + right.len(), 1);
        assert_eq!(two_columns(&[]).0.len(), 0);
    }
}

//! The **Formula Functions** window (#402): the functions an `F(x)=` formula can
//! call, **one family at a time**. Opened by the "?" of the cell
//! ([`super::column_formula_help`]).
//!
//! Read-only, hence `modal_read_only` (Close only, Enter and Escape close, S-F2),
//! Large class (a list and its content side by side). The families are the
//! fixed vertical tabs at the left (S-L9, [`TabStripSide::Left`]); the table of
//! the chosen family (Function, Example, What it does) is on the right and is
//! the only part that scrolls. The rows come from
//! `stokesforge_app_state::formula_help`; the open state and the chosen family
//! are in `AppState`.

use eframe::egui;
use stokesforge_app_state::formula_help::{self, HelpGroup};
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize, SizeClass};
use super::tab_strip::{tab_strip, TabStripSide};
use crate::theme;

/// Paints the window while it is open.
pub(super) fn formula_functions_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    if !state.formula_functions_open() {
        return;
    }
    let groups = formula_help::groups();
    let chosen = state.formula_functions_family().min(groups.len() - 1);
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_formula_functions"),
        &tr!("formula-help-title"),
        PlainSize::Class(SizeClass::Large),
        |ui| {
            ui.label(tr!("formula-help-note"));
            ui.add_space(theme::SPACE_ITEM);
            let tabs: Vec<(usize, String)> = groups
                .iter()
                .enumerate()
                .map(|(index, group)| (index, group.title.clone()))
                .collect();
            // The row takes the whole height left; the strip stays fixed.
            let size = ui.available_size();
            ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Min), |ui| {
                tab_strip(
                    ui,
                    TabStripSide::Left,
                    &tabs,
                    chosen,
                    |picked| actions.push(Action::SelectFormulaFunctionsFamily(picked)),
                    None,
                    || {},
                );
                ui.vertical(|ui| family_table(ui, &groups[chosen]));
            });
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseFormulaFunctions);
    }
}

/// The table of a family: Function, Example, What it does. The signatures and
/// the examples are formula code (not translated, monospace).
fn family_table(ui: &mut egui::Ui, group: &HelpGroup) {
    theme::page_scroll_area().show(ui, |ui| {
        egui::Grid::new("formula_functions_table")
            .num_columns(3)
            .spacing(egui::vec2(10.0, 6.0))
            .striped(true)
            .show(ui, |ui| {
                ui.label(theme::bold(tr!("formula-help-column-function")));
                ui.label(theme::bold(tr!("formula-help-column-example")));
                ui.label(theme::bold(tr!("formula-help-column-meaning")));
                ui.end_row();
                for row in &group.rows {
                    ui.monospace(row.signature);
                    ui.monospace(row.example);
                    ui.add(egui::Label::new(&row.description).wrap());
                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texts painted by two frames of the window in the given state.
    fn texts(open: bool, family: usize) -> Vec<String> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Help").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        if open {
            state.dispatch(Action::OpenFormulaFunctions);
            state.dispatch(Action::SelectFormulaFunctionsFamily(family));
        }
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut actions = Vec::new();
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |_| formula_functions_window(&ctx, &state, &mut actions),
            );
            output.textures_delta.clear();
        }
        session.shutdown().unwrap();
        output
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Closed, nothing; open, the title, the families as tabs and **only** the
    /// rows of the chosen one.
    #[test]
    fn the_window_shows_the_chosen_family_only() {
        stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
        assert!(texts(false, 0).is_empty());
        let groups = formula_help::groups();
        let shown = texts(true, 1);
        assert!(shown.contains(&tr!("formula-help-title")), "{shown:?}");
        for group in &groups {
            assert!(
                shown.contains(&group.title),
                "tab {}: {shown:?}",
                group.title
            );
        }
        for row in &groups[1].rows {
            assert!(shown.contains(&row.signature.to_string()), "{shown:?}");
        }
        for row in &groups[0].rows {
            assert!(!shown.contains(&row.signature.to_string()), "{shown:?}");
        }
    }
}

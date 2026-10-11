//! Help › Keyboard Shortcuts (#231, K5) : la liste des raccourcis, générée de
//! [`HELP`](super::shortcuts::HELP) pour qu'elle ne puisse pas diverger du
//! registre. Lecture seule, donc `modal_read_only` (Close seul, F2).

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome};
use super::shortcuts::{HelpArea, HELP};
use crate::state::{Action, AppState};
use crate::theme;
use crate::tr;

/// Peint la fenêtre tant qu'elle est ouverte.
pub(super) fn keyboard_shortcuts_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    if !state.keyboard_shortcuts_open() {
        return;
    }
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_keyboard_shortcuts"),
        &tr!("keyboard-shortcuts-title"),
        egui::vec2(480.0, 460.0),
        |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for area in HelpArea::ALL {
                    theme::section_title(ui, area.title());
                    egui::Grid::new(("keyboard_shortcuts", area as u8))
                        .num_columns(2)
                        .show(ui, |ui| {
                            for entry in HELP.iter().filter(|entry| entry.area == area) {
                                ui.label((entry.label)());
                                ui.weak(entry.keys_text(ui.ctx()));
                                ui.end_row();
                            }
                        });
                }
            });
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseKeyboardShortcuts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::shortcuts::HELP;

    /// Une image de la fenêtre ouverte ; rend les textes peints.
    fn textes(open: bool) -> Vec<String> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Aide").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        if open {
            state.dispatch(Action::OpenKeyboardShortcuts);
        }
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                ..Default::default()
            },
            |_| keyboard_shortcuts_window(&ctx, &state, &mut actions),
        );
        output.textures_delta.clear();
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

    /// Fermée, la fenêtre ne peint rien ; ouverte, elle montre son titre, ses
    /// rubriques et au moins une ligne du registre.
    #[test]
    fn la_fenetre_montre_le_registre_quand_elle_est_ouverte() {
        assert!(textes(false).is_empty());
        let texts = textes(true);
        assert!(
            texts.contains(&tr!("keyboard-shortcuts-title")),
            "{texts:?}"
        );
        assert!(texts.contains(&tr!("help-area-general")), "{texts:?}");
        let first = (HELP[0].label)();
        assert!(texts.contains(&first), "{first} absent : {texts:?}");
    }
}

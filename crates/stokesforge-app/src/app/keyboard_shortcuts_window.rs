//! Help › Keyboard Shortcuts (#231, S-K5): the list of shortcuts, generated
//! from [`HELP`](super::shortcuts::HELP) so that it cannot diverge from the
//! registry. Read-only, hence `modal_read_only` (Close only, S-F2).

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize};
use super::shortcuts::{HelpArea, HELP};
use crate::theme;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Paints the window while it is open.
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
        PlainSize::Content(egui::vec2(480.0, 460.0)),
        |ui| {
            theme::page_scroll_area().show(ui, |ui| {
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

    /// A frame of the open window; returns the painted texts.
    fn texts(open: bool) -> Vec<String> {
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Help").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        if open {
            state.dispatch(Action::OpenKeyboardShortcuts);
        }
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut actions = Vec::new();
        // Two frames: a window is painted on the second (the first one
        // measures it); a read-only window no longer has a discarded initial
        // focus pass to give it that (S-F11).
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
                |_| keyboard_shortcuts_window(&ctx, &state, &mut actions),
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

    /// Closed, the window paints nothing; open, it shows its title, its
    /// sections and at least one line of the registry.
    #[test]
    fn the_window_shows_the_registry_when_it_is_open() {
        assert!(texts(false).is_empty());
        let texts = texts(true);
        assert!(
            texts.contains(&tr!("keyboard-shortcuts-title")),
            "{texts:?}"
        );
        assert!(texts.contains(&tr!("help-area-general")), "{texts:?}");
        let first = (HELP[0].label)();
        assert!(texts.contains(&first), "{first} missing: {texts:?}");
    }
}

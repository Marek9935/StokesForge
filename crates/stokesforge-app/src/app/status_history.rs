//! Fenêtre d'historique de la barre d'état (#230, M6) : les 50 derniers
//! messages, horodatés, du plus récent au plus ancien. Lecture seule, donc
//! `modal_read_only` (Close seul, F2) ; un message d'échec reste dans la barre
//! d'état, jamais ici seulement. L'heure est l'heure UTC : la bibliothèque
//! standard ne connaît pas le fuseau local et le projet n'a pas de dépendance
//! de calendrier.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome};
use super::empty_state::empty_state;
use super::menus::status_line;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// « HH:MM:SS » (UTC) d'un instant.
fn clock(at: std::time::SystemTime) -> String {
    let seconds = at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// Peint la fenêtre tant que l'historique est ouvert.
pub(super) fn status_history_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    if !state.status_history_open() {
        return;
    }
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_status_history"),
        &tr!("status-history-title"),
        egui::vec2(520.0, 360.0),
        |ui| {
            let mut messages = state.status_history().peekable();
            if messages.peek().is_none() {
                empty_state(ui, tr!("status-history-empty"));
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for message in state.status_history().collect::<Vec<_>>().into_iter().rev() {
                    ui.horizontal(|ui| {
                        ui.weak(tr!("status-history-time", time = clock(message.wall)));
                        status_line(ui, message);
                    });
                }
            });
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseStatusHistory);
    }
}

/// Peint la liste des R² faibles du projet par type (#290), ouverte par la
/// pastille de la barre d'état. Lecture seule : Close seul. Les mêmes
/// compteurs que la boîte (`AppState::low_r2_list_text`).
pub(super) fn low_r2_list_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    if !state.low_r2_list_open() {
        return;
    }
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_low_r2_list"),
        &tr!("alert-low-r2-title"),
        egui::vec2(480.0, 170.0),
        |ui| {
            ui.label(egui::RichText::new(state.low_r2_list_text()).weak());
        },
    );
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseLowR2List);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_heure_est_celle_du_jour_en_utc() {
        let at = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(86_400 * 3 + 13 * 3600 + 5 * 60 + 9);
        assert_eq!(clock(at), "13:05:09");
        assert_eq!(clock(std::time::UNIX_EPOCH), "00:00:00");
    }
}

//! Status bar history window (#230, S-M6): the last 50 messages, timestamped,
//! from newest to oldest. Read-only, hence `modal_read_only` (Close only,
//! S-F2); a failure message stays in the status bar, never only here. The time
//! is UTC: the standard library does not know the local time zone and the
//! project has no calendar dependency.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize};
use super::empty_state::empty_state;
use super::menus::status_line;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// "HH:MM:SS" (UTC) of a point in time.
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

/// Paints the window while the history is open.
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
        PlainSize::Content(egui::vec2(520.0, 360.0)),
        |ui| {
            let mut messages = state.status_history().peekable();
            if messages.peek().is_none() {
                empty_state(ui, tr!("status-history-empty"));
                return;
            }
            crate::theme::page_scroll_area().show(ui, |ui| {
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

/// Paints the list of the project's weak R² by type (#290), opened by the
/// status bar badge. Read-only: Close only. The same counters as the box
/// (`AppState::low_r2_list_text`).
pub(super) fn low_r2_list_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    if !state.low_r2_list_open() {
        return;
    }
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_low_r2_list"),
        &tr!("alert-low-r2-title"),
        PlainSize::Content(egui::vec2(480.0, 170.0)),
        |ui| {
            // Normal text: the counters are the content, not a detail (#290).
            ui.label(state.low_r2_list_text());
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
    fn the_time_is_the_time_of_day_in_utc() {
        let at = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(86_400 * 3 + 13 * 3600 + 5 * 60 + 9);
        assert_eq!(clock(at), "13:05:09");
        assert_eq!(clock(std::time::UNIX_EPOCH), "00:00:00");
    }
}

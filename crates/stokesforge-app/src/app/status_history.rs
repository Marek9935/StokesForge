//! Status bar history window (#230, S-M6): the last 50 messages, timestamped,
//! from newest to oldest. Read-only, hence `modal_read_only` (Close only,
//! S-F2); a failure message stays in the status bar, never only here. The time
//! is UTC: the standard library does not know the local time zone and the
//! project has no calendar dependency.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize};
use super::empty_state::empty_state;
use super::menus::status_line_text;
use stokesforge_app_state::state::{Action, AppState, StatusMessage};
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

/// The sentence of an entry: its text, with the count of the identical
/// messages in a row it stands for (#453).
fn entry_text(message: &StatusMessage) -> String {
    if message.count > 1 {
        tr!(
            "status-history-repeated",
            text = message.text.as_str(),
            count = message.count
        )
    } else {
        message.text.clone()
    }
}

/// One line of the list: the time stays on the left of the first line and the
/// sentence wraps in the width that is left (#453).
fn history_row(ui: &mut egui::Ui, message: &StatusMessage) -> egui::Response {
    ui.horizontal_top(|ui| {
        ui.weak(tr!("status-history-time", time = clock(message.wall)));
        status_line_text(ui, message.level, &entry_text(message), true);
    })
    .response
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
                    history_row(ui, message);
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
    use stokesforge_app_state::state::StatusLevel;

    fn message(text: &str, level: StatusLevel, count: u32) -> StatusMessage {
        StatusMessage {
            text: text.to_owned(),
            level,
            at: std::time::Instant::now(),
            wall: std::time::UNIX_EPOCH,
            gesture: None,
            seq: 0,
            count,
        }
    }

    /// Height and width of `paint` in a column `width` points wide.
    fn measure(width: f32, paint: impl Fn(&mut egui::Ui)) -> egui::Vec2 {
        let ctx = egui::Context::default();
        let mut size = egui::Vec2::ZERO;
        // Two passes: the first one loads the fonts.
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let response = ui.allocate_ui(egui::vec2(width, 600.0), |ui| paint(ui));
                size = response.response.rect.size();
            });
            output.textures_delta.clear();
        }
        size
    }

    /// #453 (F1): a long sentence wraps in the width left on the row, the time
    /// stays on the first line, and the status bar's own line is not wrapped.
    /// Witness: `wrap` passed as `false` in `history_row`.
    #[test]
    fn a_long_sentence_wraps_in_the_history_and_not_in_the_status_bar() {
        let long = "word ".repeat(40);
        let long = long.trim();
        let long_message = message(long, StatusLevel::Warning, 1);
        let short = message("short", StatusLevel::Warning, 1);
        let one_line = measure(240.0, |ui| {
            history_row(ui, &short);
        });
        let wrapped = measure(240.0, |ui| {
            history_row(ui, &long_message);
        });
        assert!(
            wrapped.y > one_line.y * 3.0,
            "several lines: {wrapped:?} against {one_line:?}"
        );
        assert!(wrapped.x <= 240.5, "stays in the window: {wrapped:?}");
        // The shared line of the status bar does not change.
        let bar = measure(240.0, |ui| {
            ui.horizontal(|ui| status_line_text(ui, StatusLevel::Warning, long, false));
        });
        assert!(
            bar.x > 240.0 && bar.y < one_line.y * 2.0,
            "one line, not wrapped: {bar:?} against {one_line:?}"
        );
    }

    /// #453 (F2): the count is part of the sentence, through the catalogs; one
    /// occurrence shows no count.
    #[test]
    fn a_merged_entry_shows_its_count_and_a_single_one_does_not() {
        let english = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let once = message("Nothing to undo", StatusLevel::Info, 1);
        let many = message("Nothing to undo", StatusLevel::Info, 13);
        assert_eq!(entry_text(&once), "Nothing to undo");
        let text = entry_text(&many);
        assert!(
            text.starts_with("Nothing to undo (") && text.contains("13)"),
            "{text:?}"
        );
        drop(english);
        let _french = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::French,
        );
        let text = entry_text(&message("Rien à annuler", StatusLevel::Info, 13));
        assert!(
            text.starts_with("Rien à annuler (") && text.contains("13)"),
            "{text:?}"
        );
    }

    #[test]
    fn the_time_is_the_time_of_day_in_utc() {
        let at = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(86_400 * 3 + 13 * 3600 + 5 * 60 + 9);
        assert_eq!(clock(at), "13:05:09");
        assert_eq!(clock(std::time::UNIX_EPOCH), "00:00:00");
    }
}

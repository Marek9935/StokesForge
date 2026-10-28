//! "Work in progress" building block (#232, S-P1/S-P2): the same indicator
//! everywhere a task takes time.
//!
//! - **S-P1**: below [`SHOW_AFTER`] (≈ 0.5 s) nothing is shown — a short task
//!   must not flicker; beyond it, a spinning indicator and a word
//!   ("Importing…"), with a progress bar when the number of steps is known;
//! - **S-P2**: beyond [`STOP_AFTER`] (a few seconds), a Stop button that
//!   pushes the given `Action`. The task itself says what Stop undoes
//!   (see `AppState::stop_import`).
//!
//! The building block measures nothing: the caller gives the elapsed time,
//! read from the task (`AppState::import_work`). It requests the next frame
//! itself as long as it is visible or about to be.

use std::time::Duration;

use eframe::egui;

use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;

/// Duration below which a task stays invisible (S-P1).
pub(super) const SHOW_AFTER: Duration = Duration::from_millis(500);
/// Duration from which Stop is offered (S-P2).
pub(super) const STOP_AFTER: Duration = Duration::from_secs(3);

/// A task in progress, as the building block paints it.
pub(super) struct Work {
    /// The word: "Importing…".
    pub(super) label: String,
    /// How long it has been running.
    pub(super) elapsed: Duration,
    /// `(done, total)` when the number of steps is known.
    pub(super) progress: Option<(usize, usize)>,
    /// What Stop pushes; `None` if the task cannot be interrupted.
    pub(super) stop: Option<Action>,
}

/// The indicator alone: spinner and word. For the surfaces that already have
/// their own Stop (the script console).
pub(super) fn indicator(ui: &mut egui::Ui, label: &str) {
    ui.spinner();
    ui.weak(label);
}

/// Paints the task if it has been running long enough; returns true if
/// something was painted.
pub(super) fn work_in_progress(ui: &mut egui::Ui, work: &Work, actions: &mut Vec<Action>) -> bool {
    if work.elapsed < SHOW_AFTER {
        // Nothing to see yet, but the frame where it will appear must come.
        ui.ctx().request_repaint_after(SHOW_AFTER - work.elapsed);
        return false;
    }
    indicator(ui, &work.label);
    if let Some((done, total)) = work.progress {
        ui.add(
            egui::ProgressBar::new(if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            })
            .desired_width(120.0),
        );
    }
    match (&work.stop, work.elapsed >= STOP_AFTER) {
        (Some(action), true) => {
            if ui
                .button(tr!("busy-stop"))
                .on_hover_text(tr!("busy-stop", "hint"))
                .clicked()
            {
                actions.push(action.clone());
            }
        }
        (Some(_), false) => {
            ui.ctx().request_repaint_after(STOP_AFTER - work.elapsed);
        }
        (None, _) => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paints `work` and returns (painted?, texts, actions).
    fn paint(work: &Work) -> (bool, Vec<String>, Vec<Action>) {
        let context = egui::Context::default();
        crate::theme::install(&context);
        let mut actions = Vec::new();
        let mut drawn = false;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| drawn = work_in_progress(ui, work, &mut actions),
        );
        output.textures_delta.clear();
        let texts = output
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) => Some(shape.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        (drawn, texts, actions)
    }

    fn work(seconds: f32) -> Work {
        Work {
            label: "Importing…".into(),
            elapsed: Duration::from_secs_f32(seconds),
            progress: Some((1, 4)),
            stop: Some(Action::StopImport),
        }
    }

    /// S-P1: nothing before 0.5 s; the word afterwards. S-P2: Stop only after
    /// a few seconds, and only if the task can be interrupted.
    #[test]
    fn the_indicator_and_stop_each_arrive_at_their_own_time() {
        let (drawn, texts, _) = paint(&work(0.2));
        assert!(!drawn && texts.is_empty(), "{texts:?}");

        let (drawn, texts, _) = paint(&work(1.0));
        assert!(drawn);
        assert!(texts.contains(&"Importing…".to_string()), "{texts:?}");
        assert!(!texts.contains(&tr!("busy-stop")), "Stop too early");

        let (_, texts, _) = paint(&work(4.0));
        assert!(texts.contains(&tr!("busy-stop")), "{texts:?}");

        let mut without_stop = work(4.0);
        without_stop.stop = None;
        let (_, texts, _) = paint(&without_stop);
        assert!(!texts.contains(&tr!("busy-stop")), "{texts:?}");
    }
}

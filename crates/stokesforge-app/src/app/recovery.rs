//! The recovery window (#317), opened at startup when backup copies left by
//! dead processes are newer than their `.stf` files.
//!
//! Read-only (`[Close]`, S-F2); each row carries its own buttons: `Open`
//! loads the copy into the current window (behind "Save?" if there is work)
//! and the list stays open for the others, `Ignore` leaves the copy on disk
//! (offered again at the next startup), `Delete` asks for confirmation
//! (`confirmations.rs`, a deletion cannot be undone). `Close` ignores what is
//! left. The list comes from `AppState::recovery_rows`; the window decides
//! nothing. Reference twin: `column_use.rs`.

use eframe::egui;

use super::dialogs::{modal_read_only, DialogOutcome, PlainSize};
use stokesforge_app_state::state::{Action, AppState, RecoveryRow};
use stokesforge_app_state::tr;

/// Paints the window while copies remain to offer, and pushes the answer.
pub(super) fn recovery_window(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    let Some(rows) = state.recovery_rows() else {
        return;
    };
    let mut requested = Vec::new();
    let outcome = modal_read_only(
        ctx,
        egui::Id::new("stokesforge_recovery"),
        &tr!("recovery-title"),
        PlainSize::Content(egui::vec2(560.0, 230.0)),
        |ui| {
            ui.label(tr!("recovery-message"));
            crate::theme::page_scroll_area()
                .max_height(260.0)
                .show(ui, |ui| {
                    for row in &rows {
                        row_buttons(ui, row, &mut requested);
                    }
                });
        },
    );
    actions.extend(requested);
    if outcome != DialogOutcome::Open {
        actions.push(Action::CloseRecovery);
    }
}

/// One copy: its name and date, then `Open · Ignore · Delete` on the right.
fn row_buttons(ui: &mut egui::Ui, row: &RecoveryRow, requested: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(&row.text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Placed right to left: on screen, Open · Ignore · Delete.
            if ui
                .button(tr!("recovery-delete"))
                .on_hover_text(tr!("recovery-delete", "hint"))
                .clicked()
            {
                requested.push(Action::RecoveryDelete(row.path.clone()));
            }
            if ui
                .button(tr!("recovery-ignore"))
                .on_hover_text(tr!("recovery-ignore", "hint"))
                .clicked()
            {
                requested.push(Action::RecoveryIgnore(row.path.clone()));
            }
            if ui
                .button(tr!("recovery-open"))
                .on_hover_text(tr!("recovery-open", "hint"))
                .clicked()
            {
                requested.push(Action::RecoveryOpen(row.path.clone()));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    struct Bench {
        _session: stokesforge_session::Session,
        _dir: tempfile::TempDir,
        state: AppState,
    }

    /// Two copies from dead processes (one with an original path, one
    /// without), listed at startup.
    fn bench() -> egui_kittest::Harness<'static, Bench> {
        let dir = tempfile::tempdir().unwrap();
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Recovery").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new_with_preferences(
            session.handle(),
            String::new(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        let snapshot = session.handle().snapshot().unwrap();
        let recovery = dir.path().join("recovery");
        stokesforge_app_state::recovery::write_copy(&recovery, "/x/measurements.stf", &snapshot)
            .unwrap();
        stokesforge_app_state::recovery::write_copy(&recovery, "", &snapshot).unwrap();
        state.set_recovery_dir(Some(recovery));
        state.scan_recovery();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut actions = Vec::new();
                    recovery_window(ui.ctx(), &bench.state, &mut actions);
                    for action in actions {
                        bench.state.dispatch(action);
                    }
                },
                Bench {
                    _session: session,
                    _dir: dir,
                    state,
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn files(harness: &egui_kittest::Harness<'static, Bench>) -> usize {
        let dir = harness.state()._dir.path().join("recovery");
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "stf"))
            .count()
    }

    /// The window paints its title, its two rows (name or Untitled, date) and
    /// their three buttons.
    #[test]
    fn the_window_paints_its_rows_and_buttons() {
        let harness = bench();
        harness.get_by_label(&tr!("recovery-message"));
        let rows = harness.state().state.recovery_rows().expect("two copies");
        assert_eq!(rows.len(), 2);
        for row in &rows {
            harness.get_by_label(&row.text);
        }
        assert!(
            rows.iter().any(|r| r.text.contains("measurements.stf")),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .any(|r| r.text.contains(&tr!("recovery-untitled"))),
            "{rows:?}"
        );
        for button in [
            tr!("recovery-open"),
            tr!("recovery-ignore"),
            tr!("recovery-delete"),
        ] {
            assert_eq!(
                harness.get_all_by_label(&button).count(),
                2,
                "one {button} per row"
            );
        }
    }

    /// Open loads the copy and the list stays open for the other one.
    #[test]
    fn open_loads_the_copy_and_keeps_the_list_open() {
        let mut harness = bench();
        harness
            .get_all_by_label(&tr!("recovery-open"))
            .next()
            .unwrap()
            .click();
        harness.run();
        assert_eq!(
            harness.state().state.recovery_rows().map(|r| r.len()),
            Some(1),
            "the other copy is still offered"
        );
        assert!(harness.state().state.alert().is_none());
    }

    /// Ignore removes the row without touching the file; Delete goes through
    /// the confirmation (nothing is erased before it).
    #[test]
    fn ignore_keeps_the_file_and_delete_waits_for_its_confirmation() {
        let mut harness = bench();
        assert_eq!(files(&harness), 2);
        harness
            .get_all_by_label(&tr!("recovery-ignore"))
            .next()
            .unwrap()
            .click();
        harness.run();
        assert_eq!(harness.state().state.recovery_rows().unwrap().len(), 1);
        assert_eq!(files(&harness), 2, "Ignore keeps the file");
        harness
            .get_all_by_label(&tr!("recovery-delete"))
            .next()
            .unwrap()
            .click();
        harness.run();
        assert!(harness.state().state.pending_recovery_delete().is_some());
        assert_eq!(
            files(&harness),
            2,
            "nothing is erased before the confirmation"
        );
    }
}

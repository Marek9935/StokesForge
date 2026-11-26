//! The window `Formulas with Different Values` (#556): opens over the Origin
//! chooser after Import is clicked, only when a formula gives other values
//! than Origin's. A **modal** `Draft` window (`modal_dialog_with`, Large class,
//! like the chooser it comes from): a sentence, then one row per column, a box
//! and `Book · Sheet · Column = formula as Origin wrote it`, and under it one
//! example row `row 1: Origin 0.1215, formula 45.85`. All boxes are unchecked at
//! first; a checked column takes the formula, the others keep Origin's values.
//! Footer `[Check All] [Check None] [Cancel] [Import]`. Cancel imports nothing
//! and the chooser stays open. The state and the rows live in `AppState`
//! (`origin_formulas_*`); the window decides nothing. A long formula wraps
//! instead of being cut.

use eframe::egui;

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize, SizeClass,
};
use crate::theme;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

/// Paints the window while it is open and pushes the answer.
pub(super) fn origin_formulas_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    if !state.origin_formulas_open() {
        return;
    }
    let rows = state.origin_formulas_rows();
    let accept_label = tr!("origin-formulas-accept");
    let accept_hint = tr!("origin-formulas-accept", "hint");
    let mut requested = Vec::new();
    let mut footer_requested = Vec::new();
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_origin_formulas"),
        &tr!("origin-formulas-title"),
        DialogNature::Draft(InteractiveSize::Class(SizeClass::Large)),
        DialogFooter::always(&accept_label, &accept_hint),
        |ui| {
            if ui
                .button(tr!("origin-formulas-check-all"))
                .on_hover_text(tr!("origin-formulas-check-all", "hint"))
                .clicked()
            {
                footer_requested.push(Action::CheckAllOriginFormulas(true));
            }
            if ui
                .button(tr!("origin-formulas-check-none"))
                .on_hover_text(tr!("origin-formulas-check-none", "hint"))
                .clicked()
            {
                footer_requested.push(Action::CheckAllOriginFormulas(false));
            }
        },
        |ui| {
            ui.label(tr!("origin-formulas-intro"));
            ui.add_space(theme::SPACE_ITEM);
            theme::page_scroll_area()
                .id_salt("stokesforge_origin_formulas_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (index, row) in rows.iter().enumerate() {
                        let mut ticked = row.ticked;
                        let response = ui.checkbox(&mut ticked, row.label.as_str());
                        if response.changed() {
                            requested.push(Action::SetOriginFormulaTick { index, ticked });
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(ui.spacing().icon_width + ui.spacing().icon_spacing);
                            ui.label(theme::secondary_text(&row.example));
                        });
                        ui.add_space(theme::SPACE_ITEM);
                    }
                });
        },
    );
    actions.extend(footer_requested);
    actions.extend(requested);
    match outcome {
        DialogOutcome::Accept => actions.push(Action::ImportOriginFormulas),
        DialogOutcome::Cancel => actions.push(Action::CancelOriginFormulas),
        DialogOutcome::Open => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable};
    use stokesforge_import::origin::synthetic;
    use stokesforge_import::origin::OriginDesignation::Y;

    /// A very long book name: the row must wrap, never be cut.
    const LONG_BOOK: &str = "A workbook whose name is very long, as the Horiba labels of a spectrometer run can be, to see the row wrap";

    struct Bench {
        _session: stokesforge_session::Session,
        _dir: tempfile::TempDir,
        state: AppState,
    }

    /// The chooser on a file with one stale formula column, Import clicked: the
    /// window is open.
    fn open_window() -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Origin").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new_with_preferences(
            session.handle(),
            String::new(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        let mut stale = synthetic::Column::numbers("C", Y, &[1.5, 1.5]);
        stale.formula = Some("col(B)*2".into());
        let bytes = synthetic::write_project(&synthetic::Project {
            books: vec![synthetic::Book {
                short_name: "A".into(),
                long_name: LONG_BOOK.into(),
                sheets: vec![synthetic::Sheet {
                    name: "Data".into(),
                    columns: vec![synthetic::Column::numbers("B", Y, &[10.0, 20.0]), stale],
                }],
                horiba_xml: None,
            }],
            ..synthetic::Project::default()
        });
        let path = dir.path().join("f.opj");
        std::fs::write(&path, bytes).unwrap();
        state.dispatch(Action::DropFiles {
            paths: vec![path],
            target_book: None,
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while state.import_progress().is_some() {
            state.poll_import();
            assert!(std::time::Instant::now() < deadline, "import too long");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        state.dispatch(Action::CheckAllOriginSheets);
        state.dispatch(Action::ImportOrigin);
        assert!(state.origin_formulas_open());
        Bench {
            _session: session,
            _dir: dir,
            state,
        }
    }

    fn harness(bench: Bench) -> egui_kittest::Harness<'static, Bench> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut actions = Vec::new();
                    origin_formulas_window(ui.ctx(), &bench.state, &mut actions);
                    for action in actions {
                        bench.state.dispatch(action);
                    }
                },
                bench,
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    /// One row per stale column with its example, all boxes unchecked; Check
    /// All and Check None flip them; Cancel closes the window and imports
    /// nothing.
    #[test]
    fn the_window_lists_the_column_and_an_example_and_its_buttons_answer() {
        let mut harness = harness(open_window());
        harness.get_by_label_contains(&tr!("origin-formulas-intro"));
        let row = harness.get_by_label_contains("col(B)*2");
        assert_eq!(
            row.accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::False)
        );
        harness.get_by_label_contains("row 1: Origin 1.5, formula 20");
        harness
            .get_by_label(&tr!("origin-formulas-check-all"))
            .click();
        harness.run();
        assert!(harness.state().state.origin_formulas_rows()[0].ticked);
        harness
            .get_by_label(&tr!("origin-formulas-check-none"))
            .click();
        harness.run();
        assert!(!harness.state().state.origin_formulas_rows()[0].ticked);
        harness.get_by_label(&tr!("dialog-cancel")).click();
        harness.run();
        assert!(!harness.state().state.origin_formulas_open());
        assert!(
            harness.state().state.origin_import_open(),
            "the chooser is back"
        );
    }

    /// Run in a child process whose language is the pseudo-locale: nothing is
    /// cut, the buttons stay in the window and apart.
    #[test]
    #[ignore = "run in a subprocess by long_texts_of_the_formulas_window_are_never_cut"]
    fn long_texts_of_the_formulas_window_are_never_cut_in_the_process_language() {
        use stokesforge_app_state::i18n::{self, Language};
        assert_eq!(i18n::language(), Language::PseudoEnglish);
        println!("formulas window checked in {}", i18n::language().tag());
        let harness = harness(open_window());
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        let row = harness.get_by_label_contains("col(B)*2");
        assert!(window.contains_rect(row.rect()), "cut: {:?}", row.rect());
        // The long book name wraps inside the row: its whole text is there.
        assert!(
            row.accesskit_node()
                .label()
                .is_some_and(|label| label.contains(LONG_BOOK)),
            "the long name is not cut"
        );
        let shown = harness.state().state.origin_formulas_rows()[0]
            .example
            .clone();
        let example = harness.get_by_label_contains(&shown);
        assert!(window.contains_rect(example.rect()), "{:?}", example.rect());
        let labels = [
            tr!("origin-formulas-check-all"),
            tr!("origin-formulas-check-none"),
            tr!("dialog-cancel"),
            tr!("origin-formulas-accept"),
        ];
        let mut rects = Vec::new();
        for label in &labels {
            let rect = harness.get_by_label_contains(label).rect();
            assert!(window.contains_rect(rect), "{label}: {rect:?}");
            rects.push(rect);
        }
        for pair in rects.windows(2) {
            assert!(!pair[0].intersects(pair[1]), "{pair:?}");
        }
    }

    /// The check above, in a child process (witness: a child that ran nothing
    /// would not say so).
    #[test]
    fn long_texts_of_the_formulas_window_are_never_cut() {
        use stokesforge_app_state::i18n::{Language, LANGUAGE_VARIABLE};
        let executable = std::env::current_exe().unwrap();
        let module = module_path!()
            .split_once("::")
            .map_or(module_path!(), |(_, rest)| rest);
        let name = format!(
            "{module}::long_texts_of_the_formulas_window_are_never_cut_in_the_process_language"
        );
        let output = std::process::Command::new(&executable)
            .args([
                name.as_str(),
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(LANGUAGE_VARIABLE, Language::PseudoEnglish.tag())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}\n{stderr}");
        assert!(
            stdout.contains(&format!(
                "formulas window checked in {}",
                Language::PseudoEnglish.tag()
            )),
            "the child did not run in the pseudo-locale:\n{stdout}"
        );
    }
}

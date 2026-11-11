//! The Origin chooser (#490): the window that lists the workbooks and sheets
//! of one or several Origin `.opj` projects and creates the ticked ones.
//!
//! A **modal** `Draft` window (`modal_dialog_with`, Large class): a scrolling
//! tree, file then workbooks then sheets, and the footer
//! `[Check All] [Check None] [Cancel] [Import]`. A workbook is a collapsible
//! row with a three-state check box (it ticks or unticks its sheets); a sheet
//! is a check box with its size ("3 columns, 120 rows"). A sheet with no column
//! has its box greyed out with its reason. With a single file the file level
//! is a plain heading line. The state, the ticks and the reasons live in
//! `AppState` (`origin_import_*`); the window decides nothing and pushes only
//! actions. A long name (Horiba labels run to 60 characters) is cut with an
//! ellipsis and shows in full on hover, rather than widening the window.
//! Reference twins: `analysis_dialog.rs` (footer), `recovery.rs` (list).

use eframe::egui;

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize, SizeClass,
};
use crate::theme;
use stokesforge_app_state::state::{Action, AppState, OriginBookView, OriginSheetView};
use stokesforge_app_state::tr;

/// Beyond this many characters a name may be cut: it then gets a tooltip.
const LONG_NAME: usize = 36;

/// Paints the chooser while it is open and pushes the answer.
pub(super) fn origin_import_window(
    ctx: &egui::Context,
    state: &AppState,
    actions: &mut Vec<Action>,
) {
    if !state.origin_import_open() {
        return;
    }
    let files = state.origin_import_files();
    let summary = state.origin_import_summary();
    let blocker = state.origin_import_blocker();
    let accept_label = tr!("origin-import-accept");
    let accept_hint = tr!("origin-import-accept", "hint");
    let reason = blocker.clone().unwrap_or_default();
    let mut footer_requested = Vec::new();
    let mut requested = Vec::new();
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_origin_import"),
        &tr!("origin-import-title"),
        DialogNature::Draft(InteractiveSize::Class(SizeClass::Large)),
        DialogFooter::gated(&accept_label, &accept_hint, blocker.is_none(), &reason),
        |ui| {
            if ui
                .button(tr!("origin-import-check-all"))
                .on_hover_text(tr!("origin-import-check-all", "hint"))
                .clicked()
            {
                footer_requested.push(Action::CheckAllOriginSheets);
            }
            if ui
                .button(tr!("origin-import-check-none"))
                .on_hover_text(tr!("origin-import-check-none", "hint"))
                .clicked()
            {
                footer_requested.push(Action::CheckNoOriginSheets);
            }
        },
        |ui| {
            let list_height = (ui.available_height() - 26.0).max(80.0);
            theme::page_scroll_area()
                .id_salt("stokesforge_origin_import_list")
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (file_index, file) in files.iter().enumerate() {
                        if files.len() == 1 {
                            ui.label(theme::bold(&file.file_name))
                                .on_hover_text(file.path.display().to_string());
                            books(ui, file_index, &file.books, &mut requested);
                        } else {
                            egui::CollapsingHeader::new(theme::bold(&file.file_name))
                                .id_salt(("stokesforge_origin_file", file_index))
                                .default_open(true)
                                .show(ui, |ui| {
                                    books(ui, file_index, &file.books, &mut requested);
                                })
                                .header_response
                                .on_hover_text(file.path.display().to_string());
                        }
                    }
                });
            ui.add_space(theme::SPACE_ITEM);
            if let Some(summary) = &summary {
                ui.label(theme::dimmed_text(summary));
            }
        },
    );
    actions.extend(footer_requested);
    actions.extend(requested);
    match outcome {
        DialogOutcome::Accept => actions.push(Action::ImportOrigin),
        DialogOutcome::Cancel => actions.push(Action::CancelOriginImport),
        DialogOutcome::Open => {}
    }
}

fn books(ui: &mut egui::Ui, file: usize, books: &[OriginBookView], requested: &mut Vec<Action>) {
    for (book_index, book) in books.iter().enumerate() {
        book_node(ui, file, book_index, book, requested);
    }
}

/// One workbook: its three-state box and name on the header row, its sheets
/// below. Open by default when a sheet of it is ticked.
fn book_node(
    ui: &mut egui::Ui,
    file: usize,
    index: usize,
    book: &OriginBookView,
    requested: &mut Vec<Action>,
) {
    let importable = book.sheets.iter().filter(|s| s.columns > 0).count();
    let ticked = book.sheets.iter().filter(|s| s.ticked).count();
    let mut checked = importable > 0 && ticked == importable;
    let mixed = ticked > 0 && ticked < importable;
    let id = ui.make_persistent_id(("stokesforge_origin_book", file, index));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, ticked > 0)
        .show_header(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !book.short_name.is_empty() && book.short_name != book.name {
                    ui.label(theme::secondary_text(&book.short_name));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // A long name is cut with an ellipsis where the row ends.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    let response = ui
                        .add_enabled(
                            importable > 0,
                            egui::Checkbox::new(&mut checked, book.name.as_str())
                                .indeterminate(mixed),
                        )
                        .on_disabled_hover_text(tr!("disabled-origin-sheet-empty"));
                    let response = if book.name.chars().count() > LONG_NAME {
                        response.on_hover_text(&book.name)
                    } else {
                        response.on_hover_text(tr!("origin-import-book-hint"))
                    };
                    if response.changed() {
                        requested.push(Action::SetOriginBookTicked {
                            file,
                            book: index,
                            ticked: checked,
                        });
                    }
                });
            });
        })
        .body(|ui| {
            for (sheet_index, sheet) in book.sheets.iter().enumerate() {
                sheet_row(ui, file, index, sheet_index, sheet, requested);
            }
        });
}

/// One sheet: its check box and name, its size on the right.
fn sheet_row(
    ui: &mut egui::Ui,
    file: usize,
    book: usize,
    index: usize,
    sheet: &OriginSheetView,
    requested: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(theme::secondary_text(tr!(
                "origin-import-sheet-size",
                columns = i64::try_from(sheet.columns).unwrap_or(i64::MAX),
                rows = i64::try_from(sheet.rows).unwrap_or(i64::MAX)
            )));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // A long name is cut with an ellipsis where the row ends.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                let mut ticked = sheet.ticked;
                let response = ui
                    .add_enabled(
                        sheet.columns > 0,
                        egui::Checkbox::new(&mut ticked, sheet.name.as_str()),
                    )
                    .on_disabled_hover_text(tr!("disabled-origin-sheet-empty"));
                let response = if sheet.name.chars().count() > LONG_NAME {
                    response.on_hover_text(&sheet.name)
                } else {
                    response
                };
                if response.changed() {
                    requested.push(Action::ToggleOriginSheet {
                        file,
                        book,
                        sheet: index,
                    });
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable};
    use std::path::PathBuf;
    use stokesforge_import::origin::synthetic;
    use stokesforge_import::origin::OriginDesignation::{X, Y};

    struct Bench {
        _session: stokesforge_session::Session,
        _dir: tempfile::TempDir,
        state: AppState,
    }

    fn sheet(name: &str, columns: Vec<synthetic::Column>) -> synthetic::Sheet {
        synthetic::Sheet {
            name: name.into(),
            columns,
        }
    }

    fn book(short: &str, long: &str, sheets: Vec<synthetic::Sheet>) -> synthetic::Book {
        synthetic::Book {
            short_name: short.into(),
            long_name: long.into(),
            sheets,
            horiba_xml: None,
        }
    }

    /// `Book1` (short `A`): sheets Graph, Data (ticked by default), Empty
    /// (no column). `Kinetics` (short `B`): one sheet.
    fn books() -> Vec<synthetic::Book> {
        vec![
            book(
                "A",
                "Book1",
                vec![
                    sheet("Graph", vec![synthetic::Column::numbers("G", Y, &[1.0])]),
                    sheet(
                        "Data",
                        vec![
                            synthetic::Column::numbers("X", X, &[1.0, 2.0, 3.0]),
                            synthetic::Column::numbers("Y", Y, &[10.0, 20.0, 30.0]),
                        ],
                    ),
                    sheet("Empty", Vec::new()),
                ],
            ),
            book(
                "B",
                "Kinetics",
                vec![sheet(
                    "Sheet1",
                    vec![
                        synthetic::Column::numbers("T", X, &[0.0, 1.0]),
                        synthetic::Column::numbers("S", Y, &[4.0, 5.0]),
                    ],
                )],
            ),
        ]
    }

    fn start(books: Vec<synthetic::Book>) -> (Bench, PathBuf) {
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
        let path = dir.path().join("kinetics.opj");
        let bytes = synthetic::write_project(&synthetic::Project {
            books,
            ..synthetic::Project::default()
        });
        std::fs::write(&path, bytes).unwrap();
        state.dispatch(Action::DropFiles {
            paths: vec![path.clone()],
            target_book: None,
        });
        wait(&mut state);
        (
            Bench {
                _session: session,
                _dir: dir,
                state,
            },
            path,
        )
    }

    fn wait(state: &mut AppState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while state.import_progress().is_some() {
            state.poll_import();
            assert!(std::time::Instant::now() < deadline, "import too long");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn harness(bench: Bench) -> egui_kittest::Harness<'static, Bench> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut actions = Vec::new();
                    origin_import_window(ui.ctx(), &bench.state, &mut actions);
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

    fn ticks(harness: &egui_kittest::Harness<'static, Bench>) -> Vec<Vec<bool>> {
        harness.state().state.origin_import_files()[0]
            .books
            .iter()
            .map(|b| b.sheets.iter().map(|s| s.ticked).collect())
            .collect()
    }

    fn root_names(state: &AppState) -> Vec<String> {
        let tree = state.tree();
        tree.node(tree.root())
            .unwrap()
            .children()
            .iter()
            .map(|id| tree.node(*id).unwrap().name().to_string())
            .collect()
    }

    /// The tree paints the file heading, the workbooks (long name, short
    /// name greyed when different) and the sheets with their sizes.
    #[test]
    fn the_tree_paints_file_workbooks_sheets_and_sizes() {
        let (bench, _) = start(books());
        let harness = harness(bench);
        harness.get_by_label("kinetics.opj");
        harness.get_by_label("Book1");
        harness.get_by_label("Kinetics");
        for sheet in ["Graph", "Data", "Empty", "Sheet1"] {
            harness.get_by_label(sheet);
        }
        harness.get_by_label("A");
        harness.get_by_label("2 columns, 3 rows");
        harness.get_by_label("2 columns, 2 rows");
        harness.get_by_label("0 columns, 0 rows");
        harness.get_by_label("1 column, 1 row");
        harness.get_by_label("To create: 2 sheets in 2 workbooks");
    }

    /// Check None greys Import out and says why; Check All brings it back,
    /// leaving the sheet without a column unticked.
    #[test]
    fn check_none_greys_import_out_with_its_reason_and_check_all_restores_it() {
        let (bench, _) = start(books());
        let mut harness = harness(bench);
        harness.get_by_label("Import");
        assert!(!harness
            .get_by_label("Import")
            .accesskit_node()
            .is_disabled());
        harness.get_by_label("Check None").click();
        harness.run();
        assert!(ticks(&harness).iter().flatten().all(|t| !t));
        assert!(harness
            .get_by_label("Import")
            .accesskit_node()
            .is_disabled());
        harness.get_by_label(&tr!("disabled-origin-import-nothing-ticked"));
        harness.get_by_label("Check All").click();
        harness.run();
        assert_eq!(ticks(&harness), vec![vec![true, true, false], vec![true]]);
        assert!(!harness
            .get_by_label("Import")
            .accesskit_node()
            .is_disabled());
    }

    /// A click on a sheet box toggles that sheet only, and the summary follows.
    #[test]
    fn a_click_on_a_sheet_checkbox_toggles_it() {
        let (bench, _) = start(books());
        let mut harness = harness(bench);
        assert_eq!(ticks(&harness), vec![vec![false, true, false], vec![true]]);
        harness.get_by_label("Graph").click();
        harness.run();
        assert_eq!(ticks(&harness), vec![vec![true, true, false], vec![true]]);
        harness.get_by_label("To create: 3 sheets in 2 workbooks");
        harness.get_by_label("Graph").click();
        harness.run();
        assert_eq!(ticks(&harness), vec![vec![false, true, false], vec![true]]);
        // A sheet that is not the first one: the click reaches its own index.
        harness.get_by_label("Data").click();
        harness.run();
        assert_eq!(ticks(&harness), vec![vec![false, false, false], vec![true]]);
    }

    /// The box of a workbook ticks all its sheets with columns when only some
    /// are ticked, and unticks them all once every one is.
    #[test]
    fn the_workbook_checkbox_is_three_state() {
        let (bench, _) = start(books());
        let mut harness = harness(bench);
        harness.get_by_label("Book1").click();
        harness.run();
        assert_eq!(ticks(&harness)[0], vec![true, true, false]);
        harness.get_by_label("Book1").click();
        harness.run();
        assert_eq!(ticks(&harness)[0], vec![false, false, false]);
    }

    /// A sheet with no column cannot be ticked: its box is greyed out.
    #[test]
    fn a_sheet_without_column_is_greyed_out() {
        let (bench, _) = start(books());
        let harness = harness(bench);
        assert!(harness.get_by_label("Empty").accesskit_node().is_disabled());
        assert!(!harness.get_by_label("Data").accesskit_node().is_disabled());
    }

    /// Import creates one workbook per ticked workbook, named after its long
    /// name, and closes the window.
    #[test]
    fn import_creates_the_workbooks_and_closes_the_window() {
        let (bench, _) = start(books());
        let mut harness = harness(bench);
        harness.get_by_label("Import").click();
        harness.run();
        assert!(!harness.state().state.origin_import_open());
        let names = root_names(&harness.state().state);
        assert!(names.contains(&"Book1".to_string()), "{names:?}");
        assert!(names.contains(&"Kinetics".to_string()), "{names:?}");
    }

    /// Cancel closes the window and creates nothing.
    #[test]
    fn cancel_closes_the_window_and_creates_nothing() {
        let (bench, _) = start(books());
        let mut harness = harness(bench);
        let before = root_names(&harness.state().state);
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(!harness.state().state.origin_import_open());
        assert_eq!(root_names(&harness.state().state), before);
    }

    /// A long name stays on one row: it is cut, not wrapped, and the window
    /// does not widen.
    #[test]
    fn a_long_name_is_cut_on_one_row() {
        let long = "Horiba spectrum of the sample number twelve measured on the long arm of the instrument with the second grating and the slow scan, after the night run";
        let (bench, _) = start(vec![book(
            "A",
            long,
            vec![sheet(
                "Data",
                vec![synthetic::Column::numbers("X", X, &[1.0])],
            )],
        )]);
        let harness = harness(bench);
        let row = harness.get_by_label(long).rect();
        let sheet = harness.get_by_label("Data").rect();
        assert!(
            row.width() < SizeClass::Large.size().x,
            "cut inside the window: {row:?}"
        );
        assert!(
            row.height() < 2.0 * sheet.height(),
            "one row, not wrapped: {row:?} vs {sheet:?}"
        );
    }

    /// The shell opens the created workbooks like it does after a text import.
    #[test]
    fn the_shell_opens_the_workbooks_that_import_created() {
        let (bench, _) = start(books());
        let Bench {
            _session,
            _dir,
            state,
        } = bench;
        let mut app = super::super::StokesForgeApp::new(state);
        let ctx = egui::Context::default();
        app.dispatch_actions(&ctx, vec![Action::ImportOrigin]);
        let tree = app.state.tree().clone();
        let root = tree.root();
        let new_books: Vec<_> = tree
            .node(root)
            .unwrap()
            .children()
            .iter()
            .filter(|id| {
                matches!(
                    tree.node(**id).unwrap().kind(),
                    stokesforge_core::node::NodeKind::Workbook { .. }
                )
            })
            .filter(|id| matches!(tree.node(**id).unwrap().name(), "Book1" | "Kinetics"))
            .copied()
            .collect();
        assert_eq!(new_books.len(), 2);
        for id in new_books {
            assert!(
                app.open_tabs().contains(&super::super::Tab::Book(id)),
                "workbook {id:?} is not open"
            );
        }
    }
}

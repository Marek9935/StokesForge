//! The Origin chooser (#490, #537): the window that lists the workbooks, sheets
//! and columns of one or several Origin projects and creates the ticked ones.
//!
//! A **modal** `Draft` window (`modal_dialog_with`, Large class): a scrolling
//! tree, file then workbooks then sheets then columns, a line `[To: …] [Folder: …]`
//! above it and the footer `[Check All] [Check None] [Cancel] [Import]`. The To:
//! and Folder: menus are the ones of the Import window (`import_to.rs`, one
//! painter): where the workbooks go (one per Origin workbook, or a new one),
//! how the Origin sheets share worksheets, and the project folder. A workbook
//! and a sheet are collapsible rows with a three-state check box (all, none,
//! some of their columns); sheets are collapsed, and the column rows of an
//! open sheet are painted only when they are on screen (a file can hold a
//! thousand columns). A column row is a check box, its X/Y role, its long name
//! and its unit; click toggles it, Shift+click toggles the range from the last
//! one clicked in that sheet, right-click offers the same column in all
//! sheets. Under the workbooks of each file a **Graphs** heading lists its
//! graphs (#557): a three-state box over the columns the graph plots, its name
//! and curve count, and its curves (`Y vs X · Book / Sheet`, no box) once
//! opened; a graph with no usable curve is greyed with its reason. Three
//! buttons `Show: All / Workbooks / Graphs` above the list hide one kind or
//! the other and never untick. Nothing is ticked at opening. A sheet with no column has its box greyed out with its reason. With
//! a single file the file level is a plain heading line. The state, the ticks
//! and the reasons live in `AppState` (`origin_import_*`); the window decides
//! nothing and pushes only actions. A long name (Horiba labels run to 60
//! characters) is cut with an ellipsis and shows in full on hover, rather than
//! widening the window. The orange line above the footer says which sheets
//! keep an X of their own. Reference twins: `analysis_dialog.rs` (footer),
//! `recovery.rs` (list).

use eframe::egui;

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize, SizeClass,
};
use super::import_to::{
    folder_label, folder_menu, menu_button, to_menu, LayoutOptions, ToPick, WhereOption,
};
use super::tab_strip::{tab_strip, TabStripSide};
use crate::icons::Icon;
use crate::theme;
use stokesforge_app_state::state::{
    Action, AppState, OriginBookView, OriginColumnView, OriginFileView, OriginGraphView,
    OriginSheetView, OriginShow, OriginTick, OriginWhere,
};
use stokesforge_app_state::tr;
use stokesforge_import::origin::OriginDesignation;

/// Beyond this many characters a name may be cut: it then gets a tooltip.
const LONG_NAME: usize = 36;

/// Where a sheet lives in the chooser: file, book and sheet ranks.
#[derive(Clone, Copy)]
struct SheetAt {
    file: usize,
    book: usize,
    sheet: usize,
}

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
    let own_x = state.origin_import_x_warning();
    let show = state.origin_import_show();
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
            // The two menus of the Import window, above the tree: their labels
            // are too long for the footer.
            ui.horizontal(|ui| to_and_folder(ui, state, &mut requested));
            ui.horizontal(|ui| show_buttons(ui, show, &mut requested));
            ui.add_space(theme::SPACE_ITEM);
            let reserved = if own_x.is_some() { 70.0 } else { 26.0 };
            let list_height = (ui.available_height() - reserved).max(80.0);
            theme::page_scroll_area()
                .id_salt("stokesforge_origin_import_list")
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (file_index, file) in files.iter().enumerate() {
                        let partial_note = |ui: &mut egui::Ui| {
                            if file.partial {
                                ui.label(
                                    egui::RichText::new(tr!("origin-import-partial"))
                                        .color(theme::warning_color(ui.visuals().dark_mode)),
                                );
                            }
                        };
                        if files.len() == 1 {
                            ui.label(theme::bold(&file.file_name))
                                .on_hover_text(file.path.display().to_string());
                            partial_note(ui);
                            file_body(ui, file_index, file, show, &mut requested);
                        } else {
                            egui::CollapsingHeader::new(theme::bold(&file.file_name))
                                .id_salt(("stokesforge_origin_file", file_index))
                                .default_open(true)
                                .show(ui, |ui| {
                                    partial_note(ui);
                                    file_body(ui, file_index, file, show, &mut requested);
                                })
                                .header_response
                                .on_hover_text(file.path.display().to_string());
                        }
                    }
                });
            ui.add_space(theme::SPACE_ITEM);
            if let Some(line) = &own_x {
                ui.label(
                    egui::RichText::new(line).color(theme::warning_color(ui.visuals().dark_mode)),
                );
            }
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

/// The `To:` and `Folder:` buttons of the footer, with the menus the Import
/// window has (`import_to.rs`).
fn to_and_folder(ui: &mut egui::Ui, state: &AppState, requested: &mut Vec<Action>) {
    let (Some(current), Some(layout)) = (state.origin_import_where(), state.origin_import_layout())
    else {
        return;
    };
    let wheres = [
        (
            OriginWhere::PerOriginWorkbook,
            tr!("origin-import-where-per-book"),
        ),
        (OriginWhere::NewWorkbook, tr!("import-destination-new")),
    ]
    .map(|(value, label)| WhereOption {
        value,
        selected: value == current,
        label,
        unavailable: None,
    });
    let layouts = LayoutOptions {
        heading: tr!("origin-import-sheets"),
        labels: [
            tr!("origin-import-layout-per-sheet"),
            tr!("origin-import-layout-side-by-side"),
            tr!("origin-import-layout-shared-x"),
        ],
        current: layout,
    };
    let shown = wheres
        .iter()
        .find(|option| option.selected)
        .map(|option| option.label.clone())
        .unwrap_or_default();
    let button = menu_button(ui, tr!("import-footer-to", value = shown), Icon::CaretDown);
    let pick = egui::Popup::from_toggle_button_response(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| to_menu(ui, &wheres, None, Some(&layouts)))
        .and_then(|menu| menu.inner);
    match pick {
        Some(ToPick::Where(value)) => requested.push(Action::SetOriginWhere(value)),
        Some(ToPick::Layout(value)) => requested.push(Action::SetOriginLayout(value)),
        None => {}
    }
    if let Some(folder) = state.import_folder() {
        let choices = state.import_folder_choices();
        let button = menu_button(
            ui,
            tr!(
                "import-footer-folder",
                value = folder_label(&choices, folder)
            ),
            Icon::CaretDown,
        );
        let picked = egui::Popup::from_toggle_button_response(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| folder_menu(ui, &choices, folder))
            .and_then(|menu| menu.inner);
        if let Some(picked) = picked {
            requested.push(Action::SetImportFolder(picked));
        }
    }
}

/// The tabs `Show: All / Workbooks / Graphs` (#557).
fn show_buttons(ui: &mut egui::Ui, current: OriginShow, requested: &mut Vec<Action>) {
    ui.label(tr!("origin-import-show"));
    let tabs = [
        (OriginShow::All, tr!("origin-import-show-all")),
        (OriginShow::Workbooks, tr!("origin-import-show-workbooks")),
        (OriginShow::Graphs, tr!("origin-import-show-graphs")),
    ];
    tab_strip(
        ui,
        TabStripSide::Top,
        &tabs,
        current,
        |picked| {
            if picked != current {
                requested.push(Action::SetOriginShow(picked));
            }
        },
        None,
        || {},
    );
}

/// The books of a file, then its graphs under a heading, as `show` says.
fn file_body(
    ui: &mut egui::Ui,
    file_index: usize,
    file: &OriginFileView,
    show: OriginShow,
    requested: &mut Vec<Action>,
) {
    if show != OriginShow::Graphs {
        for (book_index, book) in file.books.iter().enumerate() {
            book_node(ui, file_index, book_index, book, requested);
        }
    }
    if show != OriginShow::Workbooks && !file.graphs.is_empty() {
        ui.add_space(theme::SPACE_ITEM);
        ui.label(theme::bold(tr!("origin-import-graphs-heading")));
        for (graph_index, graph) in file.graphs.iter().enumerate() {
            graph_node(ui, file_index, graph_index, graph, requested);
        }
    }
}

/// One graph: its three-state box, name and curve count on the header row, its
/// curves below once opened (closed by default). A curve row has no box.
fn graph_node(
    ui: &mut egui::Ui,
    file: usize,
    index: usize,
    graph: &OriginGraphView,
    requested: &mut Vec<Action>,
) {
    let mut checked = graph.tick == OriginTick::All;
    let mixed = graph.tick == OriginTick::Some;
    let id = ui.make_persistent_id(("stokesforge_origin_graph", file, index));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(theme::secondary_text(tr!(
                    "origin-import-graph-curves",
                    count = i64::try_from(graph.curves.len()).unwrap_or(i64::MAX)
                )));
                if !graph.short_name.is_empty() && graph.short_name != graph.name {
                    ui.label(theme::secondary_text(&graph.short_name));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // A long name is cut with an ellipsis where the row ends.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    let response = ui
                        .add_enabled(
                            graph.unavailable.is_none(),
                            egui::Checkbox::new(&mut checked, graph.name.as_str())
                                .indeterminate(mixed),
                        )
                        .on_disabled_hover_text(graph.unavailable.clone().unwrap_or_default());
                    let response = if graph.name.chars().count() > LONG_NAME {
                        response.on_hover_text(&graph.name)
                    } else {
                        response.on_hover_text(tr!("origin-import-graph-hint"))
                    };
                    if response.changed() {
                        requested.push(Action::ToggleOriginGraph { file, graph: index });
                    }
                });
            });
        })
        .body(|ui| {
            for curve in &graph.curves {
                // One line per curve, cut where the row ends; full on hover.
                ui.add(egui::Label::new(curve.as_str()).truncate())
                    .on_hover_text(curve);
            }
        });
}

/// One workbook: its three-state box and name on the header row, its sheets
/// below. Open by default when something of it is ticked.
fn book_node(
    ui: &mut egui::Ui,
    file: usize,
    index: usize,
    book: &OriginBookView,
    requested: &mut Vec<Action>,
) {
    let importable = book.sheets.iter().any(|s| s.columns > 0);
    let mut checked = book.tick == OriginTick::All;
    let mixed = book.tick == OriginTick::Some;
    let id = ui.make_persistent_id(("stokesforge_origin_book", file, index));
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        book.tick != OriginTick::None,
    )
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
                        importable,
                        egui::Checkbox::new(&mut checked, book.name.as_str()).indeterminate(mixed),
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
                        ticked: book.tick != OriginTick::All,
                    });
                }
            });
        });
    })
    .body(|ui| {
        for (sheet_index, sheet) in book.sheets.iter().enumerate() {
            let at = SheetAt {
                file,
                book: index,
                sheet: sheet_index,
            };
            sheet_node(ui, at, sheet, requested);
        }
    });
}

/// One sheet: its three-state box and name, its size on the right, its columns
/// below once opened (closed by default).
fn sheet_node(
    ui: &mut egui::Ui,
    at: SheetAt,
    sheet: &OriginSheetView,
    requested: &mut Vec<Action>,
) {
    let mut checked = sheet.tick == OriginTick::All;
    let mixed = sheet.tick == OriginTick::Some;
    let id = ui.make_persistent_id(("stokesforge_origin_sheet", at.file, at.book, at.sheet));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(theme::secondary_text(tr!(
                    "origin-import-sheet-size",
                    columns = i64::try_from(sheet.columns).unwrap_or(i64::MAX),
                    rows = i64::try_from(sheet.rows).unwrap_or(i64::MAX)
                )));
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // A long name is cut with an ellipsis where the row ends.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    let response = ui
                        .add_enabled(
                            sheet.columns > 0,
                            egui::Checkbox::new(&mut checked, sheet.name.as_str())
                                .indeterminate(mixed),
                        )
                        .on_disabled_hover_text(tr!("disabled-origin-sheet-empty"));
                    let response = if sheet.name.chars().count() > LONG_NAME {
                        response.on_hover_text(&sheet.name)
                    } else {
                        response.on_hover_text(tr!("origin-import-sheet-hint"))
                    };
                    if response.changed() {
                        requested.push(Action::ToggleOriginSheet {
                            file: at.file,
                            book: at.book,
                            sheet: at.sheet,
                        });
                    }
                });
            });
        })
        .body(|ui| column_rows(ui, at, &sheet.column_list, requested));
}

/// The designation of a column as its short badge.
fn designation_text(designation: OriginDesignation) -> String {
    match designation {
        OriginDesignation::X => tr!("workbook-role-x"),
        OriginDesignation::Y => tr!("workbook-role-y"),
        OriginDesignation::Z => tr!("workbook-role-z"),
        OriginDesignation::XError => tr!("column-role-x-error"),
        OriginDesignation::YError => tr!("column-role-y-error"),
        OriginDesignation::Label => tr!("workbook-role-label"),
        OriginDesignation::Disregard => tr!("workbook-role-none"),
    }
}

/// The columns of an open sheet. Only the rows on screen are painted: the
/// others only take their height, so a thousand columns cost a few dozen
/// widgets, and nothing is cut off.
fn column_rows(
    ui: &mut egui::Ui,
    at: SheetAt,
    columns: &[OriginColumnView],
    requested: &mut Vec<Action>,
) {
    let row_height = ui.spacing().interact_size.y;
    let last_id = egui::Id::new(("stokesforge_origin_last_column", at.file, at.book, at.sheet));
    for index in 0..columns.len() {
        let top = ui.cursor().top();
        let clip = ui.clip_rect();
        if top + row_height < clip.top() || top > clip.bottom() {
            ui.allocate_space(egui::vec2(1.0, row_height));
            if !columns[index].reads_unticked.is_empty() {
                ui.allocate_space(egui::vec2(1.0, row_height));
            }
            continue;
        }
        column_row(ui, at, index, columns, last_id, requested);
    }
}

fn column_row(
    ui: &mut egui::Ui,
    at: SheetAt,
    index: usize,
    columns: &[OriginColumnView],
    last_id: egui::Id,
    requested: &mut Vec<Action>,
) {
    let column = &columns[index];
    let mut clicked = false;
    let mut menu_targets: Vec<egui::Response> = Vec::new();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !column.unit.is_empty() {
                ui.label(theme::secondary_text(&column.unit));
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // A long name is cut with an ellipsis where the row ends.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                let mut ticked = column.ticked;
                let hint = tr!("origin-import-column-hint");
                let boxed = ui.add(egui::Checkbox::without_text(&mut ticked));
                let badge = ui
                    .add_sized(
                        egui::vec2(64.0, ui.spacing().interact_size.y),
                        egui::Label::new(theme::secondary_text(designation_text(
                            column.designation,
                        )))
                        .truncate(),
                    )
                    .on_hover_text(designation_text(column.designation));
                let name =
                    ui.add(egui::Label::new(column.long_name.as_str()).sense(egui::Sense::click()));
                let name = if column.long_name.chars().count() > LONG_NAME {
                    name.on_hover_text(format!("{}\n{hint}", column.long_name))
                } else {
                    name.on_hover_text(hint)
                };
                clicked = boxed.clicked() || name.clicked();
                menu_targets.extend([boxed, badge, name]);
            });
        });
    });
    if !column.reads_unticked.is_empty() {
        // Orange line under the column (#556): its formula reads columns that
        // are not checked; they are never checked for the user.
        ui.horizontal(|ui| {
            ui.set_height(ui.spacing().interact_size.y);
            ui.add_space(ui.spacing().icon_width + ui.spacing().icon_spacing);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(tr!(
                        "origin-import-reads-unchecked",
                        column = column.short_name.as_str(),
                        inputs = column.reads_unticked.join(", ")
                    ))
                    .color(theme::warning_color(ui.visuals().dark_mode)),
                )
                .truncate(),
            );
        });
    }
    let shift = ui.input(|input| input.modifiers.shift);
    let target = !column.ticked;
    if clicked {
        let last = ui.data(|data| data.get_temp::<usize>(last_id));
        match last {
            Some(last) if shift && last < columns.len() => {
                requested.push(Action::SetOriginColumnRange {
                    file: at.file,
                    book: at.book,
                    sheet: at.sheet,
                    from: last,
                    to: index,
                    ticked: target,
                });
            }
            _ => requested.push(Action::ToggleOriginColumn {
                file: at.file,
                book: at.book,
                sheet: at.sheet,
                column: index,
            }),
        }
        ui.data_mut(|data| data.insert_temp(last_id, index));
    }
    for response in menu_targets {
        response.context_menu(|ui| {
            for (key_ticked, text) in [
                (
                    true,
                    tr!(
                        "origin-import-column-check-in-all",
                        name = column.long_name.as_str()
                    ),
                ),
                (
                    false,
                    tr!(
                        "origin-import-column-uncheck-in-all",
                        name = column.long_name.as_str()
                    ),
                ),
            ] {
                if ui.button(text).clicked() {
                    requested.push(Action::SetOriginColumnInAllSheets {
                        file: at.file,
                        book: at.book,
                        sheet: at.sheet,
                        column: index,
                        ticked: key_ticked,
                    });
                    ui.close();
                }
            }
        });
    }
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

    /// The chooser as the tests of #490 and #537 knew it: the `Data` sheets
    /// ticked (all non-empty sheets of a book with no `Data` sheet). Since
    /// #557 the chooser opens with nothing ticked, see [`start_unticked`].
    fn start(books: Vec<synthetic::Book>) -> (Bench, PathBuf) {
        let (mut bench, path) = start_unticked(books);
        let files = bench.state.origin_import_files();
        for (book_index, book) in files[0].books.iter().enumerate() {
            let has_data = book
                .sheets
                .iter()
                .any(|sheet| sheet.name.eq_ignore_ascii_case("Data"));
            for (sheet_index, sheet) in book.sheets.iter().enumerate() {
                let ticked = if has_data {
                    sheet.name.eq_ignore_ascii_case("Data")
                } else {
                    sheet.columns > 0
                };
                if ticked {
                    bench.state.dispatch(Action::ToggleOriginSheet {
                        file: 0,
                        book: book_index,
                        sheet: sheet_index,
                    });
                }
            }
        }
        (bench, path)
    }

    fn start_unticked(books: Vec<synthetic::Book>) -> (Bench, PathBuf) {
        start_project(synthetic::Project {
            books,
            ..synthetic::Project::default()
        })
    }

    fn start_project(project: synthetic::Project) -> (Bench, PathBuf) {
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
        let bytes = synthetic::write_project(&project);
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
        eprintln!("open after: {}", harness.state().state.origin_import_open());
        harness.run();
        eprintln!(
            "open after2: {}",
            harness.state().state.origin_import_open()
        );
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

    // ---- Columns, folder and layouts (#537) ---------------------------------

    fn named(
        short: &str,
        designation: OriginDesignation,
        long: &str,
        unit: &str,
        values: &[f64],
    ) -> synthetic::Column {
        let mut column = synthetic::Column::numbers(short, designation, values);
        column.long_name = long.into();
        column.unit = unit.into();
        column
    }

    /// `Book1`: sheets `Alpha` (Time/s, Abs, Sig) and `Beta` (Time/s, Abs, Sig).
    fn two_sheets() -> Vec<synthetic::Book> {
        let sheet_of = |name: &str| {
            sheet(
                name,
                vec![
                    named("T", X, "Time", "s", &[1.0, 2.0]),
                    named("A", Y, "Abs", "", &[10.0, 20.0]),
                    named("B", Y, "Sig", "", &[30.0, 40.0]),
                ],
            )
        };
        vec![book(
            "A",
            "Book1",
            vec![sheet_of("Alpha"), sheet_of("Beta")],
        )]
    }

    /// A click on the arrow of a sheet row opens it (the arrow sits left of the box).
    fn expand(harness: &mut egui_kittest::Harness<'static, Bench>, sheet: &str) {
        let rect = harness.get_by_label(sheet).rect();
        let at = egui::pos2(rect.left() - 10.0, rect.center().y);
        harness.event(egui::Event::PointerMoved(at));
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        harness.run();
    }

    fn column_ticks(harness: &egui_kittest::Harness<'static, Bench>, sheet: usize) -> Vec<bool> {
        harness.state().state.origin_import_files()[0].books[0].sheets[sheet]
            .column_list
            .iter()
            .map(|c| c.ticked)
            .collect()
    }

    /// An open sheet lists its columns (role, long name, unit); a closed one
    /// paints none. Witness: the closed sheet shows no "Time".
    #[test]
    fn an_open_sheet_lists_its_columns_with_role_name_and_unit() {
        let (bench, _) = start(two_sheets());
        let mut harness = harness(bench);
        assert!(harness.query_by_label("Abs").is_none(), "closed by default");
        expand(&mut harness, "Alpha");
        harness.get_by_label("Time");
        harness.get_by_label("Abs");
        harness.get_by_label("Sig");
        harness.get_by_label("s");
    }

    /// A click on a column row toggles that column only. Witness: with the
    /// click not pushed, the ticks stay.
    #[test]
    fn a_click_on_a_column_toggles_it_and_the_sheet_box_shows_three_states() {
        let (bench, _) = start(two_sheets());
        let mut harness = harness(bench);
        expand(&mut harness, "Alpha");
        assert_eq!(column_ticks(&harness, 0), vec![true, true, true]);
        harness.get_by_label("Abs").click();
        harness.run();
        assert_eq!(column_ticks(&harness, 0), vec![true, false, true]);
        assert_eq!(column_ticks(&harness, 1), vec![true, true, true]);
        // The sheet box is mixed now, the other one is full.
        assert_eq!(
            harness.get_by_label("Alpha").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::Mixed)
        );
        assert_eq!(
            harness.get_by_label("Beta").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
    }

    /// Shift+click checks the range from the last column clicked in the sheet.
    #[test]
    fn shift_click_sets_the_range_from_the_last_click() {
        let (bench, _) = start(two_sheets());
        let mut harness = harness(bench);
        expand(&mut harness, "Alpha");
        harness.get_by_label("Check None").click();
        harness.run();
        assert_eq!(column_ticks(&harness, 0), vec![false, false, false]);
        harness.get_by_label("Time").click();
        harness.run();
        harness
            .get_by_label("Sig")
            .click_modifiers(egui::Modifiers::SHIFT);
        harness.run();
        assert_eq!(column_ticks(&harness, 0), vec![true, true, true]);
        assert_eq!(column_ticks(&harness, 1), vec![false, false, false]);
    }

    /// The context menu of a column checks or unchecks the same column in all
    /// sheets.
    #[test]
    fn the_context_menu_of_a_column_acts_on_all_sheets() {
        let (bench, _) = start(two_sheets());
        let mut harness = harness(bench);
        expand(&mut harness, "Alpha");
        harness.get_by_label("Abs").click_secondary();
        harness.run();
        harness
            .get_by_label(&tr!("origin-import-column-uncheck-in-all", name = "Abs"))
            .click();
        harness.run();
        assert_eq!(column_ticks(&harness, 0), vec![true, false, true]);
        assert_eq!(column_ticks(&harness, 1), vec![true, false, true]);
        harness.get_by_label("Abs").click_secondary();
        harness.run();
        harness
            .get_by_label(&tr!("origin-import-column-check-in-all", name = "Abs"))
            .click();
        harness.run();
        assert_eq!(column_ticks(&harness, 1), vec![true, true, true]);
    }

    fn with_folder(books: Vec<synthetic::Book>) -> (Bench, PathBuf) {
        let (mut bench, path) = start(books);
        let root = bench.state.tree().root();
        bench.state.dispatch(Action::AddFolder { parent: root });
        (bench, path)
    }

    /// The Folder: menu lists the project folders and a pick is applied.
    /// Witness: before the pick the folder is the root.
    #[test]
    fn the_folder_menu_lists_the_folders_and_picks_one() {
        use stokesforge_app_state::state::ImportFolder;
        let (bench, _) = with_folder(books());
        let mut harness = harness(bench);
        let root = harness.state().state.tree().root();
        assert_eq!(
            harness.state().state.import_folder(),
            Some(&ImportFolder::Existing(root))
        );
        harness.get_by_label_contains("Folder: Project").click();
        harness.run();
        harness.get_by_label("Folder1").click();
        harness.run();
        let picked = harness.state().state.import_folder().cloned();
        assert!(
            matches!(picked, Some(ImportFolder::Existing(id)) if id != root),
            "{picked:?}"
        );
        harness.get_by_label_contains("Folder: Folder1");
    }

    /// "New Folder…" with no name greys Import, with the reason; a name frees it.
    #[test]
    fn a_new_folder_without_a_name_greys_import_out() {
        use stokesforge_app_state::state::ImportFolder;
        let (bench, _) = start(books());
        let mut harness = harness(bench);
        harness.get_by_label_contains("Folder: Project").click();
        harness.run();
        harness.get_by_label("New Folder…").click();
        harness.run();
        assert!(matches!(
            harness.state().state.import_folder(),
            Some(ImportFolder::New { name, .. }) if name.is_empty()
        ));
        assert!(harness
            .get_by_label("Import")
            .accesskit_node()
            .is_disabled());
        harness.get_by_label(&tr!("disabled-import-folder-name-empty"));
        harness
            .get_by_role(egui::accesskit::Role::TextInput)
            .type_text("Series");
        harness.run();
        assert!(!harness
            .get_by_label("Import")
            .accesskit_node()
            .is_disabled());
    }

    /// The To: menu offers the two destinations and the three layouts, no
    /// Active Workbook; a pick reaches the state.
    #[test]
    fn the_to_menu_offers_the_origin_destinations_and_layouts() {
        use stokesforge_app_state::state::OriginWhere;
        use stokesforge_import::FilesLayout;
        let (bench, _) = start(books());
        let mut harness = harness(bench);
        harness
            .get_by_label_contains("To: One Workbook per Origin Workbook")
            .click();
        harness.run();
        assert!(harness
            .query_by_label(&tr!("import-destination-active"))
            .is_none());
        harness
            .get_by_label(&tr!("origin-import-layout-side-by-side"))
            .click();
        harness.run();
        assert_eq!(
            harness.state().state.origin_import_layout(),
            Some(FilesLayout::SideBySide)
        );
        harness.get_by_label_contains("To:").click();
        harness.run();
        harness.get_by_label(&tr!("import-destination-new")).click();
        harness.run();
        assert_eq!(
            harness.state().state.origin_import_where(),
            Some(OriginWhere::NewWorkbook)
        );
    }

    /// The orange line names the sheets that keep an X of their own; it is
    /// absent when no X differs.
    #[test]
    fn the_orange_line_shows_when_a_sheet_keeps_its_own_x() {
        use stokesforge_import::FilesLayout;
        let (bench, _) = start(vec![book(
            "A",
            "Book1",
            vec![
                sheet(
                    "One",
                    vec![
                        named("T", X, "Time", "s", &[1.0, 2.0]),
                        named("A", Y, "Abs", "", &[1.0, 2.0]),
                    ],
                ),
                sheet(
                    "Two",
                    vec![
                        named("T", X, "Time", "s", &[5.0, 6.0]),
                        named("A", Y, "Abs", "", &[1.0, 2.0]),
                    ],
                ),
            ],
        )]);
        let mut harness = harness(bench);
        harness.get_by_label("Check All").click();
        harness.run();
        assert!(harness.state().state.origin_import_x_warning().is_none());
        harness.state_mut().state.dispatch(Action::SetOriginWhere(
            stokesforge_app_state::state::OriginWhere::NewWorkbook,
        ));
        harness
            .state_mut()
            .state
            .dispatch(Action::SetOriginLayout(FilesLayout::SharedX));
        harness.run();
        let line = harness
            .state()
            .state
            .origin_import_x_warning()
            .expect("an own X");
        harness.get_by_label(&line);
    }

    const LONG_GRAPH: &str = "Graph of the absorbance of the sample number twelve against the wavelength, measured with the second grating and the slow scan, after the night run";

    /// `books()` with two graphs: `G1` (long name, Y of `Data` against its X)
    /// and `Empty` (a layer with no curve). Ranks: G 1, X 2, Y 3, T 4, S 5.
    fn with_graphs() -> synthetic::Project {
        let window =
            |short: &str, long: &str, curves: Vec<synthetic::GraphCurve>| synthetic::GraphWindow {
                short_name: short.into(),
                long_name: long.into(),
                layers: vec![synthetic::GraphLayer {
                    name: "Layer1".into(),
                    curves,
                }],
            };
        synthetic::Project {
            books: books(),
            graph_windows: vec![
                window("G1", LONG_GRAPH, vec![synthetic::GraphCurve::new(3, 2)]),
                window("Empty", "Nothing plotted", vec![]),
            ],
            ..synthetic::Project::default()
        }
    }

    /// The graphs are listed under the books with their curve count, a click
    /// on the box ticks the columns, a Show button hides rows and unticks
    /// nothing, and a graph with no curve is greyed out.
    #[test]
    fn graphs_are_listed_ticked_by_their_box_and_hidden_by_the_show_buttons() {
        let (bench, _) = start_project(with_graphs());
        let mut harness = harness(bench);
        assert!(
            ticks(&harness).iter().flatten().all(|t| !t),
            "nothing ticked"
        );
        harness.get_by_label(LONG_GRAPH);
        harness.get_by_label("1 curve");
        harness.get_by_label("0 curves");
        assert!(harness
            .get_by_label("Nothing plotted")
            .accesskit_node()
            .is_disabled());
        harness.get_by_label(LONG_GRAPH).click();
        harness.run();
        assert_eq!(ticks(&harness), vec![vec![false, true, false], vec![false]]);
        assert_eq!(
            harness.get_by_label(LONG_GRAPH).accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        harness.get_by_label("To create: 1 sheet in 1 workbook");
        // Workbooks only: no graph row, the ticks stay.
        harness.get_by_label("Workbooks").click();
        harness.run();
        assert!(harness.query_by_label(LONG_GRAPH).is_none());
        harness.get_by_label("Book1");
        assert_eq!(ticks(&harness), vec![vec![false, true, false], vec![false]]);
        // Back to All: the graph is there, still ticked.
        harness.get_by_label("All").click();
        harness.run();
        assert_eq!(
            harness.get_by_label(LONG_GRAPH).accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        // Graphs only: no workbook row.
        harness.get_all_by_label("Graphs").next().unwrap().click();
        harness.run();
        assert!(harness.query_by_label("Book1").is_none());
        harness.get_by_label(LONG_GRAPH);
    }

    /// The curves of an open graph read `Y vs X · Book / Sheet`, with no box.
    #[test]
    fn an_open_graph_lists_its_curves_without_a_box() {
        let (bench, _) = start_project(with_graphs());
        let mut harness = harness(bench);
        expand(&mut harness, LONG_GRAPH);
        harness.get_by_label("Y vs X · Book1 / Data");
    }

    /// In the pseudo-language the buttons of the window stay inside it and
    /// apart from each other. Runs in a child process (the language is the
    /// process's), see `long_texts_of_the_footer_and_menus_row_are_never_cut`.
    #[test]
    #[ignore = "run in a subprocess by long_texts_of_the_footer_and_menus_row_are_never_cut"]
    fn long_texts_of_the_footer_and_menus_row_are_never_cut_in_the_process_language() {
        use stokesforge_app_state::i18n::{self, Language};
        assert_eq!(i18n::language(), Language::PseudoEnglish);
        println!("origin chooser checked in {}", i18n::language().tag());
        let (bench, _) = start_project(with_graphs());
        let harness = harness(bench);
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        // The Show buttons and the graph row stay inside the window.
        let show = [
            tr!("origin-import-show-all"),
            tr!("origin-import-show-workbooks"),
            tr!("origin-import-show-graphs"),
        ];
        let mut show_rects = Vec::new();
        for label in &show {
            // The first match: the tab (the heading of the graphs comes later).
            let rect = harness
                .get_all_by_label_contains(label)
                .next()
                .unwrap()
                .rect();
            assert!(window.contains_rect(rect), "{label}: {rect:?}");
            show_rects.push(rect);
        }
        for pair in show_rects.windows(2) {
            assert!(!pair[0].intersects(pair[1]), "{pair:?}");
        }
        // The heading and the Show button have the same text.
        for graph in harness.get_all_by_label_contains(&tr!("origin-import-graphs-heading")) {
            assert!(window.contains_rect(graph.rect()), "{:?}", graph.rect());
        }
        let row = harness.get_by_label_contains("Graph of the absorbance");
        assert!(window.contains_rect(row.rect()), "cut: {:?}", row.rect());
        let labels = [
            tr!("origin-import-check-all"),
            tr!("origin-import-check-none"),
            tr!("dialog-cancel"),
            tr!("origin-import-accept"),
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
        let to = tr!(
            "import-footer-to",
            value = tr!("origin-import-where-per-book")
        );
        let folder = tr!("import-footer-folder", value = tr!("import-folder-root"));
        let (to, folder) = (
            harness.get_by_label_contains(to.trim()).rect(),
            harness.get_by_label_contains(folder.trim()).rect(),
        );
        assert!(window.contains_rect(to) && window.contains_rect(folder));
        assert!(!to.intersects(folder), "{to:?} {folder:?}");
    }

    /// The pseudo-language check above, run in a child process whose language
    /// is the pseudo-locale (witness: a child that ran nothing would not say so).
    #[test]
    fn long_texts_of_the_footer_and_menus_row_are_never_cut() {
        use stokesforge_app_state::i18n::{Language, LANGUAGE_VARIABLE};
        let executable = std::env::current_exe().unwrap();
        let module = module_path!()
            .split_once("::")
            .map_or(module_path!(), |(_, rest)| rest);
        let name = format!(
            "{module}::long_texts_of_the_footer_and_menus_row_are_never_cut_in_the_process_language"
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
                "origin chooser checked in {}",
                Language::PseudoEnglish.tag()
            )),
            "the child did not run in the pseudo-locale:\n{stdout}"
        );
    }

    /// The real application in a kittest harness, with the actions of the next
    /// frame (#536 timing).
    struct TimedApp {
        app: super::super::StokesForgeApp,
        frame: eframe::Frame,
        pending: Vec<Action>,
        in_frame: std::time::Duration,
        _session: stokesforge_session::Session,
    }

    /// Timing (#536), measurement only: with the largest private `.opju`, the
    /// frame in which `Action::ImportOrigin` runs (the import is synchronous in
    /// the paint pass) and the frames after it (tabs opening, grids painting).
    /// The harness lays out and tessellates but does not rasterize on a GPU.
    /// Prints counts and durations only; a missing folder skips the test.
    #[test]
    #[ignore = "needs the private Origin files; measurement only"]
    fn private_origin_import_frame_timing() {
        use eframe::App as _;
        let folder = std::env::var_os("STOKESFORGE_PRIVATE_OPJ_DIR").map_or_else(
            || PathBuf::from("/home/aliocha/dev_project/StokesForge/test/private_file/"),
            PathBuf::from,
        );
        let Ok(entries) = std::fs::read_dir(&folder) else {
            eprintln!("no private Origin folder: nothing measured");
            return;
        };
        let Some(path) = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("opju"))
            .max_by_key(|p| std::fs::metadata(p).map_or(0, |m| m.len()))
        else {
            eprintln!("no .opju file: nothing measured");
            return;
        };
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
        state.dispatch(Action::DropFiles {
            paths: vec![path],
            target_book: None,
        });
        wait(&mut state);
        let books: usize = state
            .origin_import_files()
            .iter()
            .map(|f| f.books.len())
            .sum();
        let timed = TimedApp {
            app: super::super::StokesForgeApp::new(state),
            frame: eframe::Frame::_new_kittest(),
            pending: Vec::new(),
            in_frame: std::time::Duration::ZERO,
            _session: session,
        };
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(
                |ui, timed: &mut TimedApp| {
                    let started = std::time::Instant::now();
                    let ctx = ui.ctx().clone();
                    let actions = std::mem::take(&mut timed.pending);
                    if !actions.is_empty() {
                        timed.app.dispatch_actions(&ctx, actions);
                    }
                    timed.app.logic(&ctx, &mut timed.frame);
                    timed.app.ui(ui, &mut timed.frame);
                    timed.in_frame = started.elapsed();
                },
                timed,
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        egui_extras::install_image_loaders(&harness.ctx);
        crate::theme::install(&harness.ctx);
        let frame_line = |harness: &mut egui_kittest::Harness<'static, TimedApp>, label: &str| {
            let started = std::time::Instant::now();
            harness.step();
            let wall = started.elapsed();
            let state = harness.state();
            eprintln!(
                "timing {label}: app closure {:.2?}, whole step {wall:.2?}, {} tabs open",
                state.in_frame,
                state.app.open_tabs().len()
            );
        };
        frame_line(&mut harness, "frame before (chooser open)");
        frame_line(&mut harness, "frame before again");
        harness.state_mut().pending = vec![Action::ImportOrigin];
        frame_line(&mut harness, "frame where the import runs");
        for n in 1..=3 {
            frame_line(&mut harness, &format!("frame {n} after the import"));
        }
        eprintln!("timing chooser held {books} books");
    }
}

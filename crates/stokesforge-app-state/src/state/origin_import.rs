//! The Origin chooser (#490): the state behind the window that lists the
//! workbooks and sheets of one or several Origin `.opj` projects and creates
//! the ticked ones.
//!
//! The routing is by extension only ([`stokesforge_importer::is_origin_path`],
//! no I/O on the interface thread): an `.opj` reached by a drop, by the
//! Import Wizard or by a menu import is read on the background import thread
//! (`start_import`), and its parsed project (shared by `Arc`) comes back to
//! [`AppState::add_origin_reads`]. The chooser holds one entry per file, with
//! its ticks. It is open as soon as one file was read, but it is painted
//! ([`AppState::origin_import_open`]) only while the Import Wizard is closed:
//! two modal dialogs never show together, and an Origin project that arrives
//! while the wizard is open waits for it to close.
//!
//! Import creates one workbook per ticked book, its sheets and columns, all
//! inside one `MacroKind::Import` group (one history entry, one undo). A
//! name the model would refuse (invalid, or already used among the siblings)
//! is never sent: the workbook or sheet keeps its automatic name, silently.
//! The columns carry no import source, so "Refresh from Source" does not
//! apply to them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{check_name, same_name_ignoring_case, Node, NodeId};
use stokesforge_import::origin::OriginProject;
use stokesforge_importer::{
    default_origin_selection, make_origin_column, origin_book_name, plan_origin_import,
    ImportFailure, OriginSelection,
};

use super::{AppState, ImportDestination, ImportSource};
use crate::errors;
use crate::tr;

/// What the background thread returns for one Origin file.
pub(super) type OriginRead = (PathBuf, Result<Arc<OriginProject>, ImportFailure>);

/// One Origin file of the chooser: the parsed project and its ticks.
#[derive(Debug, Clone)]
pub(super) struct OriginFile {
    path: PathBuf,
    project: Arc<OriginProject>,
    selection: OriginSelection,
}

impl OriginFile {
    /// A sheet is ticked by default as `default_origin_selection` says, but a
    /// sheet with no column can never be ticked: there is nothing to create.
    fn new(path: PathBuf, project: Arc<OriginProject>) -> Self {
        let mut selection = default_origin_selection(&project);
        for (ticks, book) in selection.iter_mut().zip(&project.books) {
            for (tick, sheet) in ticks.iter_mut().zip(&book.sheets) {
                *tick &= !sheet.columns.is_empty();
            }
        }
        Self {
            path,
            project,
            selection,
        }
    }

    fn sheet_ticked(&self, book: usize, sheet: usize) -> bool {
        self.selection
            .get(book)
            .and_then(|ticks| ticks.get(sheet))
            .copied()
            .unwrap_or(false)
    }

    /// Ticks (or unticks) a sheet that has columns.
    fn set_sheet(&mut self, book: usize, sheet: usize, ticked: bool) {
        let has_columns = self
            .project
            .books
            .get(book)
            .and_then(|b| b.sheets.get(sheet))
            .is_some_and(|s| !s.columns.is_empty());
        if let Some(tick) = self
            .selection
            .get_mut(book)
            .and_then(|ticks| ticks.get_mut(sheet))
        {
            *tick = ticked && has_columns;
        }
    }
}

/// The open chooser: its files, in the order they arrived.
#[derive(Debug, Clone)]
pub(super) struct OriginChooser {
    files: Vec<OriginFile>,
}

/// A sheet as the chooser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginSheetView {
    /// The Origin sheet name.
    pub name: String,
    /// Number of columns; a sheet with none cannot be ticked.
    pub columns: usize,
    /// Number of rows: the longest column.
    pub rows: usize,
    /// Ticked for the import.
    pub ticked: bool,
}

/// A book (a future workbook) as the chooser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginBookView {
    /// The name shown: the Origin long name, else the short name.
    pub name: String,
    /// The Origin short name.
    pub short_name: String,
    /// Its sheets.
    pub sheets: Vec<OriginSheetView>,
}

/// A file as the chooser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginFileView {
    /// Full path.
    pub path: PathBuf,
    /// Last component of the path.
    pub file_name: String,
    /// Its books.
    pub books: Vec<OriginBookView>,
}

impl AppState {
    /// The Origin chooser is open and must be painted: it holds files and the
    /// Import Wizard is not on screen.
    #[must_use]
    pub fn origin_import_open(&self) -> bool {
        self.origin_import.is_some() && self.import_state.is_none()
    }

    /// The files of the chooser, with their books, sheets and ticks. Empty
    /// when the chooser is not open.
    #[must_use]
    pub fn origin_import_files(&self) -> Vec<OriginFileView> {
        let Some(chooser) = &self.origin_import else {
            return Vec::new();
        };
        chooser
            .files
            .iter()
            .map(|file| OriginFileView {
                path: file.path.clone(),
                file_name: file_name(&file.path),
                books: file
                    .project
                    .books
                    .iter()
                    .enumerate()
                    .map(|(book_index, book)| OriginBookView {
                        name: origin_book_name(book).to_string(),
                        short_name: book.short_name.clone(),
                        sheets: book
                            .sheets
                            .iter()
                            .enumerate()
                            .map(|(sheet_index, sheet)| OriginSheetView {
                                name: sheet.name.clone(),
                                columns: sheet.columns.len(),
                                rows: sheet
                                    .columns
                                    .iter()
                                    .map(|column| column.values.len())
                                    .max()
                                    .unwrap_or(0),
                                ticked: file.sheet_ticked(book_index, sheet_index),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Workbooks and sheets that Import would create.
    fn origin_import_counts(&self) -> (usize, usize) {
        let Some(chooser) = &self.origin_import else {
            return (0, 0);
        };
        let (mut books, mut sheets) = (0, 0);
        for file in &chooser.files {
            for plan in plan_origin_import(&file.project, &file.selection) {
                let importable = plan
                    .sheets
                    .iter()
                    .filter(|sheet| !self.origin_sheet_is_empty(file, plan.book, sheet.sheet))
                    .count();
                if importable > 0 {
                    books += 1;
                    sheets += importable;
                }
            }
        }
        (books, sheets)
    }

    fn origin_sheet_is_empty(&self, file: &OriginFile, book: usize, sheet: usize) -> bool {
        file.project
            .books
            .get(book)
            .and_then(|b| b.sheets.get(sheet))
            .is_none_or(|s| s.columns.is_empty())
    }

    /// Why Import is greyed out, translated: nothing is ticked. `None` when
    /// something is ticked (and when the chooser is not open).
    #[must_use]
    pub fn origin_import_blocker(&self) -> Option<String> {
        self.origin_import.as_ref()?;
        (self.origin_import_counts().1 == 0).then(|| tr!("disabled-origin-import-nothing-ticked"))
    }

    /// "To create: 3 sheets in 2 workbooks"; `None` when nothing is ticked.
    #[must_use]
    pub fn origin_import_summary(&self) -> Option<String> {
        let (books, sheets) = self.origin_import_counts();
        (sheets > 0).then(|| tr!("origin-import-summary", books = books, sheets = sheets))
    }

    pub(super) fn toggle_origin_sheet(&mut self, file: usize, book: usize, sheet: usize) {
        if let Some(entry) = self
            .origin_import
            .as_mut()
            .and_then(|chooser| chooser.files.get_mut(file))
        {
            let ticked = entry.sheet_ticked(book, sheet);
            entry.set_sheet(book, sheet, !ticked);
        }
    }

    pub(super) fn set_origin_book_ticked(&mut self, file: usize, book: usize, ticked: bool) {
        if let Some(entry) = self
            .origin_import
            .as_mut()
            .and_then(|chooser| chooser.files.get_mut(file))
        {
            let sheets = entry.project.books.get(book).map_or(0, |b| b.sheets.len());
            for sheet in 0..sheets {
                entry.set_sheet(book, sheet, ticked);
            }
        }
    }

    pub(super) fn check_all_origin_sheets(&mut self, ticked: bool) {
        let Some(chooser) = &mut self.origin_import else {
            return;
        };
        for entry in &mut chooser.files {
            for (book_index, book) in entry.project.clone().books.iter().enumerate() {
                for sheet in 0..book.sheets.len() {
                    entry.set_sheet(book_index, sheet, ticked);
                }
            }
        }
    }

    /// Takes the Origin projects out of `paths` meant for the Import Wizard:
    /// they are read in the background and go to the chooser, with a message.
    /// Returns the other paths and whether an Origin project was taken.
    pub(super) fn take_origin_paths_out_of_wizard(
        &mut self,
        paths: Vec<PathBuf>,
    ) -> (Vec<PathBuf>, bool) {
        if self.import_state.is_none() {
            return (paths, false);
        }
        let (origin, others): (Vec<PathBuf>, Vec<PathBuf>) = paths
            .into_iter()
            .partition(|path| stokesforge_importer::is_origin_path(path));
        if origin.is_empty() {
            return (others, false);
        }
        let count = origin.len();
        let had_job = self.import_job.is_some();
        self.route_origin_paths(origin);
        let routed = !had_job && self.import_job.is_some();
        if routed {
            self.notice(tr!("status-import-origin-left-wizard", count = count));
        }
        (others, routed)
    }

    /// A wizard that an Origin project left with no file is closed, so that
    /// the chooser is the only dialog. A wizard that still holds text files
    /// stays; the chooser waits behind it and shows when it closes.
    pub(super) fn close_wizard_if_emptied(&mut self, routed: bool) {
        let empty = self
            .import_state
            .as_ref()
            .is_some_and(|import| import.paths.is_empty());
        if routed && empty {
            self.import_state = None;
        }
    }

    /// Reads `paths` (Origin projects) on the background import thread.
    pub(super) fn route_origin_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let recipe = stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"));
        self.start_import(
            ImportDestination::NewBookForAll,
            ImportSource::Menu(recipe),
            paths,
        );
    }

    /// The Origin projects the background thread has read: the readable ones
    /// open the chooser (or join it, a path already there is not added
    /// twice), the others make one red message.
    pub(super) fn add_origin_reads(&mut self, reads: Vec<OriginRead>) {
        let mut failures = Vec::new();
        let mut files = Vec::new();
        for (path, read) in reads {
            match read {
                Ok(project) => files.push(OriginFile::new(path, project)),
                Err(failure) => failures.push(errors::import::failure_message(&failure)),
            }
        }
        if !failures.is_empty() {
            self.fail(failures.join(" ; "));
        }
        if files.is_empty() {
            return;
        }
        let chooser = self
            .origin_import
            .get_or_insert_with(|| OriginChooser { files: Vec::new() });
        for file in files {
            if !chooser.files.iter().any(|known| known.path == file.path) {
                chooser.files.push(file);
            }
        }
    }

    /// [`Action::ImportOrigin`](super::Action::ImportOrigin).
    pub(super) fn import_origin(&mut self) {
        if self.origin_import_blocker().is_some() {
            return;
        }
        let Some(files) = self.origin_import.as_ref().map(|c| c.files.clone()) else {
            return;
        };
        if self.handle.begin_macro(MacroKind::Import).is_err() {
            self.fail(tr!("error-import-group"));
            return;
        }
        // Closed before the first command: a failure is then said in the status
        // bar, not in the footer of a chooser that is about to disappear.
        self.origin_import = None;
        let mut failures: Vec<String> = Vec::new();
        let mut done_paths: Vec<PathBuf> = Vec::new();
        let (mut book_count, mut sheet_count) = (0, 0);
        for file in &files {
            let mut created = false;
            for plan in plan_origin_import(&file.project, &file.selection) {
                let Some(origin_book) = file.project.books.get(plan.book) else {
                    continue;
                };
                let sheets: Vec<_> = plan
                    .sheets
                    .iter()
                    .filter_map(|planned| {
                        let sheet = origin_book.sheets.get(planned.sheet)?;
                        (!sheet.columns.is_empty()).then_some((planned, sheet))
                    })
                    .collect();
                if sheets.is_empty() {
                    continue;
                }
                let Some(book) = self.create_import_book(None) else {
                    failures.push(tr!("error-import-create-book", name = plan.name.clone()));
                    continue;
                };
                self.rename_if_usable(book, &plan.name);
                book_count += 1;
                created = true;
                for (planned, sheet) in sheets {
                    let Some(target) = self.add_bare_sheet(book) else {
                        failures.push(tr!(
                            "error-import-create-sheet",
                            name = planned.name.clone()
                        ));
                        continue;
                    };
                    self.rename_if_usable(target, &planned.name);
                    sheet_count += 1;
                    for column in &sheet.columns {
                        self.execute(Command::AddColumn {
                            sheet: target,
                            index: None,
                            name: None,
                            column: make_origin_column(origin_book, column),
                        });
                    }
                }
            }
            if created {
                done_paths.push(file.path.clone());
            }
        }
        if self.handle.end_macro().is_err() {
            failures.push(tr!("error-import-close-group"));
        }
        self.refresh();
        if book_count > 0 {
            self.recent_to_record.extend(done_paths);
            self.last_origin_import = Some(files);
        }
        if failures.is_empty() {
            self.notice(tr!(
                "status-import-origin-done",
                books = book_count,
                sheets = sheet_count
            ));
        } else {
            self.fail(failures.join(" ; "));
        }
    }

    /// Gives `id` the Origin name `name` when the model would accept it: valid,
    /// and not the name of a sibling (case ignored). Otherwise `id` keeps its
    /// automatic name and nothing is said: a refused name is not a failure.
    fn rename_if_usable(&mut self, id: NodeId, name: &str) {
        let tree = &self.snapshot.tree;
        let Some(parent) = tree.node(id).and_then(Node::parent) else {
            return;
        };
        if check_name(name).is_err() {
            return;
        }
        let taken = tree.node(parent).is_some_and(|parent| {
            parent.children().iter().any(|&sibling| {
                sibling != id
                    && tree
                        .node(sibling)
                        .is_some_and(|node| same_name_ignoring_case(node.name(), name))
            })
        });
        if !taken {
            self.execute(Command::Rename {
                id,
                name: name.to_string(),
            });
        }
    }

    /// "Reopen Last Import" after an Origin import: undoes it if it is still
    /// the last history entry, then opens the chooser on the same files and
    /// ticks (the parsed projects were kept, nothing is read again).
    pub(super) fn reopen_origin_import(&mut self) {
        let Some(files) = self.last_origin_import.clone() else {
            return;
        };
        let undone = self.snapshot.history.undo_label
            == Some(stokesforge_commands::HistoryLabel::Macro(MacroKind::Import))
            && !self.snapshot.history.in_macro;
        if undone {
            self.undo();
        }
        self.import_state = None;
        self.origin_import = Some(OriginChooser { files });
        self.notice(if undone {
            tr!("status-import-reopened-undone")
        } else {
            tr!("status-import-reopened-kept")
        });
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

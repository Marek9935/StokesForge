//! The Origin chooser (#490): the state behind the window that lists the
//! workbooks and sheets of one or several Origin `.opj` projects and creates
//! the ticked ones.
//!
//! The routing is by extension or by the five-byte signature
//! ([`stokesforge_importer::is_origin_file`]; the interface-thread sites read
//! at most five bytes, the Import window's detector already reads whole
//! files there): an Origin file reached by a drop, by the
//! Import window or by a menu import is read on the background import thread
//! (`start_import`), and its parsed project (shared by `Arc`) comes back to
//! [`AppState::add_origin_reads`]. The chooser holds one entry per file, with
//! its ticks. It is open as soon as one file was read, but it is painted
//! ([`AppState::origin_import_open`]) only while the Import window is closed:
//! two modal dialogs never show together, and an Origin project that arrives
//! while the wizard is open waits for it to close.
//!
//! The selection is per column (#537): a sheet and a workbook box are
//! three-state (all, none, some). The chooser also holds the classic layouts
//! (where the workbooks go, how the Origin sheets share worksheets) and the
//! folder of the new workbooks; one Origin sheet is one "file" of the classic
//! layouts ([`stokesforge_importer::plan_origin_layout`]).
//!
//! Import creates the planned workbooks, sheets and columns, all inside one
//! `MacroKind::Import` group (one history entry, one undo; a new folder is made
//! inside it too). A name the model would refuse (invalid, or already used
//! among the siblings) is never sent: the workbook or sheet keeps its
//! automatic name, silently. The columns carry no import source, so "Refresh
//! from Source" does not apply to them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{check_name, same_name_ignoring_case, Node, NodeId};
use stokesforge_import::origin::OriginDesignation;
use stokesforge_import::origin::OriginProject;
use stokesforge_import::FilesLayout;
use stokesforge_importer::{
    default_origin_selection, make_origin_column_in, origin_book_name, origin_column_key,
    plan_origin_layout, ImportFailure, OriginColumnKey, OriginColumnRef, OriginLayoutPlan,
    OriginSelection, OriginSource, OriginWhere,
};

use super::import_plans::{BookApply, BookTarget, Naming, SheetApply};
use super::{AppState, ImportDestination, ImportFolder, ImportSource};
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
    /// Ticked as `default_origin_selection` says (a sheet with no column has
    /// nothing to tick).
    fn new(path: PathBuf, project: Arc<OriginProject>) -> Self {
        let selection = default_origin_selection(&project);
        Self {
            path,
            project,
            selection,
        }
    }

    fn column_ticked(&self, book: usize, sheet: usize, column: usize) -> bool {
        self.selection
            .get(book)
            .and_then(|b| b.get(sheet))
            .and_then(|s| s.get(column))
            .copied()
            .unwrap_or(false)
    }

    fn columns_in(&self, book: usize, sheet: usize) -> usize {
        self.project
            .books
            .get(book)
            .and_then(|b| b.sheets.get(sheet))
            .map_or(0, |s| s.columns.len())
    }

    fn sheet_tick(&self, book: usize, sheet: usize) -> OriginTick {
        let count = self.columns_in(book, sheet);
        let ticked = (0..count)
            .filter(|&c| self.column_ticked(book, sheet, c))
            .count();
        tick_of(ticked, count)
    }

    fn book_tick(&self, book: usize) -> OriginTick {
        let sheets = self.project.books.get(book).map_or(0, |b| b.sheets.len());
        let (mut ticked, mut count) = (0, 0);
        for sheet in 0..sheets {
            let columns = self.columns_in(book, sheet);
            count += columns;
            ticked += (0..columns)
                .filter(|&c| self.column_ticked(book, sheet, c))
                .count();
        }
        tick_of(ticked, count)
    }

    /// Sets one column (nothing happens for a column that does not exist).
    fn set_column(&mut self, book: usize, sheet: usize, column: usize, ticked: bool) {
        if let Some(tick) = self
            .selection
            .get_mut(book)
            .and_then(|b| b.get_mut(sheet))
            .and_then(|s| s.get_mut(column))
        {
            *tick = ticked;
        }
    }

    /// Ticks (or unticks) every column of a sheet.
    fn set_sheet(&mut self, book: usize, sheet: usize, ticked: bool) {
        for column in 0..self.columns_in(book, sheet) {
            self.set_column(book, sheet, column, ticked);
        }
    }
}

/// How much of a sheet or workbook is ticked (#537): the three states of its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginTick {
    /// No column ticked (also a sheet that has no column).
    None,
    /// Some columns ticked.
    Some,
    /// Every column ticked.
    All,
}

fn tick_of(ticked: usize, count: usize) -> OriginTick {
    match (ticked, count) {
        (0, _) => OriginTick::None,
        (t, c) if t == c => OriginTick::All,
        _ => OriginTick::Some,
    }
}

/// The open chooser: its files, in the order they arrived.
#[derive(Debug, Clone)]
pub(super) struct OriginChooser {
    files: Vec<OriginFile>,
    /// Where the workbooks go.
    where_: OriginWhere,
    /// How the Origin sheets share worksheets.
    layout: FilesLayout,
    /// The folder of the new workbooks.
    pub(super) folder: ImportFolder,
}

/// A column as the chooser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginColumnView {
    /// The Origin short name.
    pub short_name: String,
    /// The long name (the short name when blank).
    pub long_name: String,
    /// The unit, possibly empty.
    pub unit: String,
    /// X, Y, error…
    pub designation: OriginDesignation,
    /// Ticked for the import.
    pub ticked: bool,
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
    /// Something of it is ticked (it takes part in the import).
    pub ticked: bool,
    /// The three states of its box.
    pub tick: OriginTick,
    /// Its columns, in order.
    pub column_list: Vec<OriginColumnView>,
}

/// A book (a future workbook) as the chooser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginBookView {
    /// The name shown: the Origin long name, else the short name.
    pub name: String,
    /// The Origin short name.
    pub short_name: String,
    /// The three states of its box.
    pub tick: OriginTick,
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
    /// Part of the file could not be read (an `.opju` read as far as it goes):
    /// the chooser says so.
    pub partial: bool,
}

impl AppState {
    /// The Origin chooser is open and must be painted: it holds files and the
    /// Import window is not on screen.
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
                partial: file.project.partial,
                books: file
                    .project
                    .books
                    .iter()
                    .enumerate()
                    .map(|(book_index, book)| OriginBookView {
                        tick: file.book_tick(book_index),
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
                                ticked: file.sheet_tick(book_index, sheet_index)
                                    != OriginTick::None,
                                tick: file.sheet_tick(book_index, sheet_index),
                                column_list: sheet
                                    .columns
                                    .iter()
                                    .enumerate()
                                    .map(|(column_index, column)| OriginColumnView {
                                        short_name: column.short_name.clone(),
                                        long_name: origin_column_key(column).to_string(),
                                        unit: column.unit.clone(),
                                        designation: column.designation,
                                        ticked: file.column_ticked(
                                            book_index,
                                            sheet_index,
                                            column_index,
                                        ),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// The plan Import would run, for the ticks, layouts and files of the
    /// chooser.
    fn origin_import_plan(&self) -> Option<OriginLayoutPlan> {
        let chooser = self.origin_import.as_ref()?;
        Some(origin_plan(chooser))
    }

    /// Workbooks and sheets that Import would create.
    fn origin_import_counts(&self) -> (usize, usize) {
        self.origin_import_plan().map_or((0, 0), |plan| {
            (
                plan.books.len(),
                plan.books.iter().map(|book| book.sheets.len()).sum(),
            )
        })
    }

    /// Why Import is greyed out, translated: no column is ticked, or the new
    /// folder has no usable name. `None` when Import can run (and when the
    /// chooser is not open).
    #[must_use]
    pub fn origin_import_blocker(&self) -> Option<String> {
        let chooser = self.origin_import.as_ref()?;
        if self.origin_import_counts().1 == 0 {
            return Some(tr!("disabled-origin-import-nothing-ticked"));
        }
        self.import_folder_blocker(&chooser.folder)
    }

    /// Where the workbooks of the Origin import go.
    #[must_use]
    pub fn origin_import_where(&self) -> Option<OriginWhere> {
        self.origin_import.as_ref().map(|chooser| chooser.where_)
    }

    /// How the Origin sheets share worksheets.
    #[must_use]
    pub fn origin_import_layout(&self) -> Option<FilesLayout> {
        self.origin_import.as_ref().map(|chooser| chooser.layout)
    }

    /// The orange line before Import (#537): the sheets that keep an X of their
    /// own because its values differ from the first X. `None` when there is
    /// none, outside a shared-X layout included.
    #[must_use]
    pub fn origin_import_x_warning(&self) -> Option<String> {
        let chooser = self.origin_import.as_ref()?;
        let plan = self.origin_import_plan()?;
        if plan.own_x.is_empty() {
            return None;
        }
        let names: Vec<String> = plan
            .own_x
            .iter()
            .filter_map(|at| {
                let book = chooser.files.get(at.file)?.project.books.get(at.book)?;
                book.sheets.get(at.sheet).map(|sheet| sheet.name.clone())
            })
            .collect();
        Some(tr!("origin-import-own-x", names = names.join(", ")))
    }

    /// "To create: 3 sheets in 2 workbooks"; `None` when nothing is ticked.
    #[must_use]
    pub fn origin_import_summary(&self) -> Option<String> {
        let (books, sheets) = self.origin_import_counts();
        (sheets > 0).then(|| tr!("origin-import-summary", books = books, sheets = sheets))
    }

    fn origin_file_mut(&mut self, file: usize) -> Option<&mut OriginFile> {
        self.origin_import
            .as_mut()
            .and_then(|chooser| chooser.files.get_mut(file))
    }

    /// A sheet that is wholly ticked is unticked; otherwise it is ticked whole.
    pub(super) fn toggle_origin_sheet(&mut self, file: usize, book: usize, sheet: usize) {
        if let Some(entry) = self.origin_file_mut(file) {
            let ticked = entry.sheet_tick(book, sheet) != OriginTick::All;
            entry.set_sheet(book, sheet, ticked);
        }
    }

    pub(super) fn toggle_origin_column(
        &mut self,
        file: usize,
        book: usize,
        sheet: usize,
        column: usize,
    ) {
        if let Some(entry) = self.origin_file_mut(file) {
            let ticked = entry.column_ticked(book, sheet, column);
            entry.set_column(book, sheet, column, !ticked);
        }
    }

    /// The Shift+click: the columns between two ends, both included.
    pub(super) fn set_origin_column_range(
        &mut self,
        file: usize,
        book: usize,
        sheet: usize,
        from: usize,
        to: usize,
        ticked: bool,
    ) {
        if let Some(entry) = self.origin_file_mut(file) {
            for column in from.min(to)..=from.max(to) {
                entry.set_column(book, sheet, column, ticked);
            }
        }
    }

    /// "Check this column in all sheets": every column of the file with the
    /// long name (else the short name) of the given one.
    pub(super) fn set_origin_column_in_all_sheets(
        &mut self,
        file: usize,
        book: usize,
        sheet: usize,
        column: usize,
        ticked: bool,
    ) {
        let Some(entry) = self.origin_file_mut(file) else {
            return;
        };
        let project = Arc::clone(&entry.project);
        let Some(key) = project
            .books
            .get(book)
            .and_then(|b| b.sheets.get(sheet))
            .and_then(|s| s.columns.get(column))
            .map(origin_column_key)
        else {
            return;
        };
        for (book_index, origin_book) in project.books.iter().enumerate() {
            for (sheet_index, origin_sheet) in origin_book.sheets.iter().enumerate() {
                for (column_index, candidate) in origin_sheet.columns.iter().enumerate() {
                    if origin_column_key(candidate) == key {
                        entry.set_column(book_index, sheet_index, column_index, ticked);
                    }
                }
            }
        }
    }

    pub(super) fn set_origin_book_ticked(&mut self, file: usize, book: usize, ticked: bool) {
        if let Some(entry) = self.origin_file_mut(file) {
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

    pub(super) fn set_origin_where(&mut self, where_: OriginWhere) {
        if let Some(chooser) = &mut self.origin_import {
            chooser.where_ = where_;
        }
    }

    pub(super) fn set_origin_layout(&mut self, layout: FilesLayout) {
        if let Some(chooser) = &mut self.origin_import {
            chooser.layout = layout;
        }
    }

    /// Takes the Origin projects out of `paths` meant for the Import window:
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
            .partition(|path| stokesforge_importer::is_origin_file(path));
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
        let folder = self.import_default_folder();
        self.start_import(
            ImportDestination::NewBookForAll,
            folder,
            ImportSource::Menu(Box::new(recipe)),
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
        let folder = ImportFolder::Existing(self.creation_folder());
        let chooser = self.origin_import.get_or_insert_with(|| OriginChooser {
            files: Vec::new(),
            where_: OriginWhere::default(),
            layout: FilesLayout::SheetPerFile,
            folder,
        });
        for file in files {
            if !chooser.files.iter().any(|known| known.path == file.path) {
                chooser.files.push(file);
            }
        }
    }

    /// The columns the last Origin import created, keyed by the Origin column
    /// they come from (file rank, book, sheet, column; see
    /// [`OriginColumnKey`]). `None`: planned but not created. Emptied by the call.
    pub fn take_origin_created_columns(&mut self) -> BTreeMap<OriginColumnKey, Option<NodeId>> {
        std::mem::take(&mut self.origin_created_columns)
    }

    /// [`Action::ImportOrigin`](super::Action::ImportOrigin).
    pub(super) fn import_origin(&mut self) {
        if self.origin_import_blocker().is_some() {
            return;
        }
        let Some(chooser) = self.origin_import.clone() else {
            return;
        };
        if self.handle.begin_macro(MacroKind::Import).is_err() {
            self.fail(tr!("error-import-group"));
            return;
        }
        // Closed before the first command: a failure is then said in the status
        // bar, not in the footer of a chooser that is about to disappear.
        self.origin_import = None;
        let plan = origin_plan(&chooser);
        let files = &chooser.files;
        let mut failures: Vec<String> = Vec::new();
        let mut folder_gone = false;
        let mut used_files: Vec<usize> = Vec::new();
        let (mut book_count, mut sheet_count) = (0, 0);
        let mut created_columns = BTreeMap::new();
        let parent = self.import_parent(&chooser.folder, &mut folder_gone);
        for book_plan in &plan.books {
            let failed_name = if book_plan.name.is_empty() {
                book_plan
                    .sheets
                    .first()
                    .map_or_else(String::new, |sheet| sheet.name.clone())
            } else {
                book_plan.name.clone()
            };
            let books = vec![BookApply {
                target: BookTarget::New {
                    name: (!book_plan.name.is_empty()).then(|| book_plan.name.clone()),
                    failed_names: vec![failed_name],
                },
                sheets: book_plan
                    .sheets
                    .iter()
                    .map(|sheet| SheetApply {
                        name: sheet.name.clone(),
                        columns: sheet.columns.clone(),
                        // Raw Data is a classic-import rule, never Origin.
                        raw: false,
                    })
                    .collect(),
            }];
            let outcomes = self.apply_book_plans(
                parent,
                Naming::Silent,
                books,
                &mut failures,
                &mut |at: &OriginColumnRef| {
                    let file = files.get(at.file)?;
                    let origin_book = file.project.books.get(at.book)?;
                    let origin_sheet = origin_book.sheets.get(at.sheet)?;
                    let column = origin_sheet.columns.get(at.column)?;
                    Some(make_origin_column_in(
                        origin_book,
                        column,
                        at.tag_sheet.then_some(origin_sheet.name.as_str()),
                    ))
                },
            );
            for outcome in outcomes {
                for (planned, ids) in book_plan.sheets.iter().zip(&outcome.columns) {
                    for (at, id) in planned.columns.iter().zip(ids) {
                        created_columns.insert((at.file, at.book, at.sheet, at.column), *id);
                    }
                }
                if outcome.book.is_none() {
                    continue;
                }
                book_count += 1;
                for (planned, created) in book_plan.sheets.iter().zip(&outcome.sheets) {
                    if !created {
                        continue;
                    }
                    sheet_count += 1;
                    for at in &planned.columns {
                        if !used_files.contains(&at.file) {
                            used_files.push(at.file);
                        }
                    }
                }
            }
        }
        self.origin_created_columns = created_columns;
        used_files.sort_unstable();
        let done_paths: Vec<PathBuf> = used_files
            .iter()
            .filter_map(|&file| files.get(file).map(|f| f.path.clone()))
            .collect();
        let partial_imported = used_files
            .iter()
            .any(|&file| files.get(file).is_some_and(|f| f.project.partial));
        if self.handle.end_macro().is_err() {
            failures.push(tr!("error-import-close-group"));
        }
        self.refresh();
        if book_count > 0 {
            self.recent_to_record.extend(done_paths);
            self.last_origin_import = Some(chooser.clone());
        }
        if !failures.is_empty() {
            self.fail(failures.join(" ; "));
        } else if partial_imported {
            self.warn(tr!("status-import-origin-partial", books = book_count));
        } else {
            self.notice(tr!(
                "status-import-origin-done",
                books = book_count,
                sheets = sheet_count
            ));
        }
        if folder_gone {
            self.warn(tr!("status-import-folder-gone"));
        }
    }

    /// Gives `id` the Origin name `name` when the model would accept it: valid,
    /// and not the name of a sibling (case ignored). Otherwise `id` keeps its
    /// automatic name and nothing is said: a refused name is not a failure.
    pub(super) fn rename_if_usable(&mut self, id: NodeId, name: &str) {
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
        let Some(mut chooser) = self.last_origin_import.clone() else {
            return;
        };
        let undone = self.snapshot.history.undo_label
            == Some(stokesforge_commands::HistoryLabel::Macro(MacroKind::Import))
            && !self.snapshot.history.in_macro;
        if undone {
            self.undo();
        }
        chooser.folder = self.reopened_folder(&chooser.folder);
        self.import_state = None;
        self.origin_import = Some(chooser);
        self.notice(if undone {
            tr!("status-import-reopened-undone")
        } else {
            tr!("status-import-reopened-kept")
        });
    }
}

/// The plan of a chooser: its ticks, layout and files.
fn origin_plan(chooser: &OriginChooser) -> OriginLayoutPlan {
    let stems: Vec<String> = chooser
        .files
        .iter()
        .map(|file| {
            file.path
                .file_stem()
                .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
        })
        .collect();
    let sources: Vec<OriginSource<'_>> = chooser
        .files
        .iter()
        .zip(&stems)
        .map(|(file, stem)| OriginSource {
            project: &file.project,
            selection: &file.selection,
            file_stem: stem,
        })
        .collect();
    plan_origin_layout(&sources, chooser.where_, chooser.layout)
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

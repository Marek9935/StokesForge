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
//! Nothing is ticked at opening (#557): the user picks graphs (their plotted
//! columns are added to the ticks, shared with the books) or columns.
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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{check_name, same_name_ignoring_case, Node, NodeId};
use stokesforge_import::origin::OriginDesignation;
use stokesforge_import::origin::{OriginColumnAt, OriginGraph, OriginProject};
use stokesforge_import::FilesLayout;
use stokesforge_importer::{
    build_column_formula, decide_origin_formulas, judge_origin_formulas, make_origin_column_in,
    origin_book_name, origin_column_key, plan_origin_formulas, plan_origin_layout,
    resolve_origin_formula, stale_formulas, with_formula_note, FormulaDecision, ImportFailure,
    Judgement, OriginColumnKey, OriginColumnRef, OriginLayoutPlan, OriginSelection, OriginSource,
    OriginWhere, PlannedFormula, ResolvedFormula, ResolvedPiece,
};

use super::import_plans::{BookApply, BookTarget, Naming, SheetApply};
use super::{AppState, ImportDestination, ImportFolder, ImportSource};
use crate::errors;
use crate::tr;

/// What the background thread returns for one Origin file.
pub(super) type OriginRead = (PathBuf, Result<Arc<OriginProject>, ImportFailure>);

/// A column of a file by position: book, sheet, column.
type ColumnPos = (usize, usize, usize);

/// A graph of a file, resolved once when the file is added: the columns it
/// plots and the lines of its curves (never recomputed per frame).
#[derive(Debug, Clone)]
struct GraphInfo {
    /// The Origin long name, else the short name.
    name: String,
    short_name: String,
    /// One line per resolvable curve: `Y vs X · Book / Sheet`.
    curves: Vec<String>,
    /// The graph has no curve at all (not even an unresolved one).
    without_curve: bool,
    /// Columns it plots, without duplicates, in plotting order: X and Y of
    /// each curve, then the error columns standing right after a plotted Y.
    columns: Vec<ColumnPos>,
}

impl GraphInfo {
    fn new(project: &OriginProject, graph: &OriginGraph) -> Self {
        let mut columns: Vec<ColumnPos> = Vec::new();
        let mut curves = Vec::new();
        let mut total = 0;
        let add = |columns: &mut Vec<ColumnPos>, at: ColumnPos| {
            if !columns.contains(&at) {
                columns.push(at);
            }
        };
        for curve in graph.layers.iter().flat_map(|layer| &layer.curves) {
            total += 1;
            let Some(y) = curve.y.and_then(|at| resolve(project, at)) else {
                // Nothing to plot without a Y: the curve is ignored.
                continue;
            };
            let x = curve.x.and_then(|at| resolve(project, at));
            if let Some(x) = x {
                add(&mut columns, x);
            }
            add(&mut columns, y);
            let (book, sheet, y_index) = y;
            if let Some(origin_sheet) = project.books.get(book).and_then(|b| b.sheets.get(sheet)) {
                let following = origin_sheet.columns.iter().enumerate().skip(y_index + 1);
                for (error, column) in following {
                    if !matches!(
                        column.designation,
                        OriginDesignation::YError | OriginDesignation::XError
                    ) {
                        break;
                    }
                    add(&mut columns, (book, sheet, error));
                }
            }
            curves.push(curve_line(project, y, x));
        }
        let short_name = graph.short_name.clone();
        let name = if graph.long_name.trim().is_empty() {
            short_name.clone()
        } else {
            graph.long_name.clone()
        };
        Self {
            name,
            short_name,
            curves,
            without_curve: total == 0,
            columns,
        }
    }
}

/// The columns each translatable formula column reads, by position.
fn formula_reads(project: &OriginProject) -> BTreeMap<ColumnPos, Vec<ColumnPos>> {
    let mut out = BTreeMap::new();
    for (b, book) in project.books.iter().enumerate() {
        for (s, sheet) in book.sheets.iter().enumerate() {
            for (c, column) in sheet.columns.iter().enumerate() {
                if column.formula.is_none() {
                    continue;
                }
                let Some(ResolvedFormula::Pieces(pieces)) =
                    resolve_origin_formula(project, (b, s, c))
                else {
                    continue;
                };
                let mut reads: Vec<ColumnPos> = Vec::new();
                for piece in pieces {
                    if let ResolvedPiece::Column { at, .. } = piece {
                        if at != (b, s, c) && !reads.contains(&at) {
                            reads.push(at);
                        }
                    }
                }
                if !reads.is_empty() {
                    out.insert((b, s, c), reads);
                }
            }
        }
    }
    out
}

/// The position when it names a column that exists.
fn resolve(project: &OriginProject, at: OriginColumnAt) -> Option<ColumnPos> {
    project
        .books
        .get(at.book)?
        .sheets
        .get(at.sheet)?
        .columns
        .get(at.column)?;
    Some((at.book, at.sheet, at.column))
}

fn curve_line(project: &OriginProject, y: ColumnPos, x: Option<ColumnPos>) -> String {
    let column = |(b, s, c): ColumnPos| origin_column_key(&project.books[b].sheets[s].columns[c]);
    let book = &project.books[y.0];
    let sheet = book.sheets[y.1].name.as_str();
    match x {
        Some(x) => tr!(
            "origin-import-curve",
            y = column(y),
            x = column(x),
            book = origin_book_name(book),
            sheet = sheet
        ),
        None => tr!(
            "origin-import-curve-no-x",
            y = column(y),
            book = origin_book_name(book),
            sheet = sheet
        ),
    }
}

/// One Origin file of the chooser: the parsed project and its ticks.
#[derive(Debug, Clone)]
pub(super) struct OriginFile {
    path: PathBuf,
    pub(super) project: Arc<OriginProject>,
    selection: OriginSelection,
    graphs: Vec<GraphInfo>,
    /// The graphs picked by their box (Check All picks every graph that
    /// plots something). A graph that is not picked shows an empty box.
    graph_picked: Vec<bool>,
    /// For each formula column that translates, the columns it reads, resolved
    /// once when the file is added (#556); the chooser warns when one is not
    /// ticked.
    formula_reads: BTreeMap<ColumnPos, Vec<ColumnPos>>,
    /// The short names of the ticked formula columns that read an unticked
    /// column, resolved on the first read after a change of the ticks (the
    /// only write is `write_column`, which clears it), not at every frame.
    reads_warning: OnceLock<Vec<String>>,
    /// "Include the columns formulas read" (#556, #557), copied from the
    /// chooser: the ticks of `derived` add the columns read by the formulas of
    /// the ticked columns.
    include_reads: bool,
    /// What the overlay adds to `selection`, resolved like `reads_warning`.
    derived: OnceLock<Derived>,
}

/// The overlay of "Include the columns formulas read": the ticks Import uses,
/// and the columns only the overlay ticked.
#[derive(Debug, Clone)]
struct Derived {
    /// The ticks by hand and by graph, plus the columns the ticked formulas
    /// read, transitively. Equal to the ticks when the box is off.
    effective: OriginSelection,
    /// A column that only the overlay ticked, with the short name of the
    /// formula column that reads it: its box cannot be clicked.
    locked: BTreeMap<ColumnPos, String>,
}

impl OriginFile {
    /// Nothing ticked (#557): a graph or a column is picked by the user.
    fn new(path: PathBuf, project: Arc<OriginProject>) -> Self {
        let selection = project
            .books
            .iter()
            .map(|book| {
                book.sheets
                    .iter()
                    .map(|sheet| vec![false; sheet.columns.len()])
                    .collect()
            })
            .collect();
        let graphs: Vec<GraphInfo> = project
            .graphs
            .iter()
            .map(|graph| GraphInfo::new(&project, graph))
            .collect();
        let formula_reads = formula_reads(&project);
        Self {
            path,
            project,
            selection,
            graph_picked: vec![false; graphs.len()],
            graphs,
            formula_reads,
            reads_warning: OnceLock::new(),
            include_reads: false,
            derived: OnceLock::new(),
        }
    }

    /// The ticks of the overlay (see [`Derived`]), worked out once per change.
    fn derived(&self) -> &Derived {
        self.derived.get_or_init(|| {
            let mut effective = self.selection.clone();
            let mut locked = BTreeMap::new();
            if self.include_reads {
                let tick = |selection: &OriginSelection, (b, s, c): ColumnPos| {
                    selection
                        .get(b)
                        .and_then(|b| b.get(s))
                        .and_then(|s| s.get(c))
                        .copied()
                        .unwrap_or(true)
                };
                let mut pending: Vec<ColumnPos> = self
                    .formula_reads
                    .keys()
                    .filter(|&&at| tick(&self.selection, at))
                    .copied()
                    .collect();
                // A column is ticked once: a loop between two columns ends.
                while let Some(reader) = pending.pop() {
                    let Some(reads) = self.formula_reads.get(&reader) else {
                        continue;
                    };
                    for &(b, s, c) in reads {
                        if tick(&effective, (b, s, c)) {
                            continue;
                        }
                        if let Some(cell) = effective
                            .get_mut(b)
                            .and_then(|b| b.get_mut(s))
                            .and_then(|s| s.get_mut(c))
                        {
                            *cell = true;
                            let name = self.short_name(reader);
                            locked.insert((b, s, c), name);
                            pending.push((b, s, c));
                        }
                    }
                }
            }
            Derived { effective, locked }
        })
    }

    fn short_name(&self, (b, s, c): ColumnPos) -> String {
        self.project
            .books
            .get(b)
            .and_then(|b| b.sheets.get(s))
            .and_then(|s| s.columns.get(c))
            .map_or_else(String::new, |column| column.short_name.clone())
    }

    /// The ticks Import and the plan use: by hand, by graph, and by the overlay.
    fn effective(&self) -> &OriginSelection {
        &self.derived().effective
    }

    /// The short name of the formula column that makes the box of this column
    /// unclickable; `None` when the overlay did not tick it alone.
    fn locked_by(&self, book: usize, sheet: usize, column: usize) -> Option<&str> {
        self.derived()
            .locked
            .get(&(book, sheet, column))
            .map(String::as_str)
    }

    /// Every column of the sheet is ticked by the overlay alone.
    fn sheet_locked(&self, book: usize, sheet: usize) -> bool {
        let count = self.columns_in(book, sheet);
        count > 0 && (0..count).all(|c| self.locked_by(book, sheet, c).is_some())
    }

    fn book_locked(&self, book: usize) -> bool {
        let sheets = self.project.books.get(book).map_or(0, |b| b.sheets.len());
        let mut any = false;
        for sheet in 0..sheets {
            if self.columns_in(book, sheet) == 0 {
                continue;
            }
            if !self.sheet_locked(book, sheet) {
                return false;
            }
            any = true;
        }
        any
    }

    fn set_include_reads(&mut self, include: bool) {
        self.include_reads = include;
        self.reads_warning.take();
        self.derived.take();
    }

    /// The box is useful: some formula of the file reads another column.
    fn has_formula_reads(&self) -> bool {
        !self.formula_reads.is_empty()
    }

    /// The box of a graph: empty while it is not picked; picked, ticked when
    /// all its columns are, "some" when the user unticked one since.
    fn graph_tick(&self, graph: usize) -> OriginTick {
        let Some(info) = self.graphs.get(graph) else {
            return OriginTick::None;
        };
        if !self.graph_picked[graph] {
            return OriginTick::None;
        }
        let all = info
            .columns
            .iter()
            .all(|&(b, s, c)| self.column_ticked(b, s, c));
        if all {
            OriginTick::All
        } else {
            OriginTick::Some
        }
    }

    /// Picks a graph (or completes a "some" one): its columns are ticked.
    fn pick_graph(&mut self, graph: usize) {
        let Some(columns) = self.graphs.get(graph).map(|g| g.columns.clone()) else {
            return;
        };
        self.graph_picked[graph] = true;
        for (b, s, c) in columns {
            self.write_column(b, s, c, true);
        }
    }

    /// Unpicks a graph: its columns are unticked, except those another picked
    /// graph uses.
    fn unpick_graph(&mut self, graph: usize) {
        let Some(columns) = self.graphs.get(graph).map(|g| g.columns.clone()) else {
            return;
        };
        self.graph_picked[graph] = false;
        for at in columns {
            let used_elsewhere = self.graphs.iter().enumerate().any(|(other, info)| {
                other != graph && self.graph_picked[other] && info.columns.contains(&at)
            });
            if !used_elsewhere {
                self.write_column(at.0, at.1, at.2, false);
            }
        }
    }

    fn column_ticked(&self, book: usize, sheet: usize, column: usize) -> bool {
        self.effective()
            .get(book)
            .and_then(|b| b.get(sheet))
            .and_then(|s| s.get(column))
            .copied()
            .unwrap_or(false)
    }

    /// The short names of the unticked columns that the formula of a ticked
    /// column reads.
    fn reads_unticked(&self, book: usize, sheet: usize, column: usize) -> Vec<String> {
        if !self.column_ticked(book, sheet, column) {
            return Vec::new();
        }
        let Some(reads) = self.formula_reads.get(&(book, sheet, column)) else {
            return Vec::new();
        };
        reads
            .iter()
            .filter(|&&(b, s, c)| !self.column_ticked(b, s, c))
            .filter_map(|&(b, s, c)| {
                self.project
                    .books
                    .get(b)?
                    .sheets
                    .get(s)?
                    .columns
                    .get(c)
                    .map(|column| column.short_name.clone())
            })
            .collect()
    }

    /// The ticked formula columns that read an unticked column (see
    /// `reads_warning`).
    fn columns_reading_unticked(&self) -> &[String] {
        self.reads_warning.get_or_init(|| {
            self.formula_reads
                .keys()
                .filter(|&&(b, s, c)| !self.reads_unticked(b, s, c).is_empty())
                .filter_map(|&(b, s, c)| {
                    self.project
                        .books
                        .get(b)?
                        .sheets
                        .get(s)?
                        .columns
                        .get(c)
                        .map(|column| column.short_name.clone())
                })
                .collect()
        })
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
        self.write_column(book, sheet, column, ticked);
    }

    /// Writes the tick; `false` for a column that does not exist.
    fn write_column(&mut self, book: usize, sheet: usize, column: usize, ticked: bool) -> bool {
        self.reads_warning.take();
        self.derived.take();
        let tick = self
            .selection
            .get_mut(book)
            .and_then(|b| b.get_mut(sheet))
            .and_then(|s| s.get_mut(column));
        tick.map(|tick| *tick = ticked).is_some()
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
    pub(super) files: Vec<OriginFile>,
    /// Where the workbooks go.
    where_: OriginWhere,
    /// How the Origin sheets share worksheets.
    layout: FilesLayout,
    /// The folder of the new workbooks.
    pub(super) folder: ImportFolder,
    /// What the list shows (#557); hides, never unticks.
    show: OriginShow,
    /// "Include the columns formulas read" (#556, #557): off at every opening,
    /// kept by Reopen Last Import.
    include_reads: bool,
    /// The stale formulas the user ticked in the window of the last Import
    /// (#556): kept so that Reopen Last Import finds the same answers.
    pub(super) accepted_formulas: BTreeSet<OriginColumnKey>,
}

/// What the chooser lists (#557): the three Show buttons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OriginShow {
    /// Workbooks and graphs.
    #[default]
    All,
    /// Workbooks only.
    Workbooks,
    /// Graphs only.
    Graphs,
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
    /// Ticked, with a formula that reads columns that are not ticked (#556):
    /// their Origin short names. The chooser says so in orange; nothing is
    /// ticked for the user.
    pub reads_unticked: Vec<String>,
    /// Ticked only by "Include the columns formulas read" (#556): the Origin
    /// short name of the formula column that reads it. Its box cannot be
    /// clicked while the box is on.
    pub read_by: Option<String>,
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
    /// Every column is ticked only by the formulas that read it: the box
    /// cannot be clicked.
    pub locked: bool,
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
    /// Every column of it is ticked only by the formulas that read it.
    pub locked: bool,
    /// Its sheets.
    pub sheets: Vec<OriginSheetView>,
}

/// A graph as the chooser shows it (#557).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginGraphView {
    /// The name shown: the Origin long name, else the short name.
    pub name: String,
    /// The Origin short name.
    pub short_name: String,
    /// The three states of its box (over the columns it plots).
    pub tick: OriginTick,
    /// One line per resolvable curve: `Y vs X · Book / Sheet`.
    pub curves: Vec<String>,
    /// Why its box is greyed out (no curve, or none that resolves).
    pub unavailable: Option<String>,
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
    /// Its graphs.
    pub graphs: Vec<OriginGraphView>,
    /// Part of the file could not be read (an `.opju` read as far as it goes):
    /// the chooser says so.
    pub partial: bool,
}

impl AppState {
    /// The Origin chooser is open and must be painted: it holds files and the
    /// Import window is not on screen.
    #[must_use]
    pub fn origin_import_open(&self) -> bool {
        self.origin_import.is_some()
            && self.import_state.is_none()
            && self.origin_formulas.is_none()
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
                graphs: file
                    .graphs
                    .iter()
                    .enumerate()
                    .map(|(index, graph)| OriginGraphView {
                        name: graph.name.clone(),
                        short_name: graph.short_name.clone(),
                        tick: file.graph_tick(index),
                        curves: graph.curves.clone(),
                        unavailable: if graph.without_curve {
                            Some(tr!("disabled-origin-graph-no-curve"))
                        } else if graph.columns.is_empty() {
                            Some(tr!("disabled-origin-graph-unresolved"))
                        } else {
                            None
                        },
                    })
                    .collect(),
                books: file
                    .project
                    .books
                    .iter()
                    .enumerate()
                    .map(|(book_index, book)| OriginBookView {
                        tick: file.book_tick(book_index),
                        locked: file.book_locked(book_index),
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
                                locked: file.sheet_locked(book_index, sheet_index),
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
                                        reads_unticked: file.reads_unticked(
                                            book_index,
                                            sheet_index,
                                            column_index,
                                        ),
                                        read_by: file
                                            .locked_by(book_index, sheet_index, column_index)
                                            .map(str::to_string),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// What the list shows; `All` when the chooser is not open.
    #[must_use]
    pub fn origin_import_show(&self) -> OriginShow {
        self.origin_import
            .as_ref()
            .map_or(OriginShow::All, |chooser| chooser.show)
    }

    /// "Include the columns formulas read" is on (#556); off when the chooser
    /// is not open.
    #[must_use]
    pub fn origin_import_include_reads(&self) -> bool {
        self.origin_import
            .as_ref()
            .is_some_and(|chooser| chooser.include_reads)
    }

    /// Why that box is greyed out: no file has a formula that reads another
    /// column. `None` when it can be used (and when the chooser is not open).
    #[must_use]
    pub fn origin_import_include_reads_blocker(&self) -> Option<String> {
        let chooser = self.origin_import.as_ref()?;
        (!chooser.files.iter().any(OriginFile::has_formula_reads))
            .then(|| tr!("disabled-origin-include-reads-no-formula"))
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

    /// The orange line before Import (#556, F4): how many ticked formula
    /// columns read an unticked column and will keep Origin's values, with up
    /// to three of their names. `None` when there is none.
    #[must_use]
    pub fn origin_import_reads_warning(&self) -> Option<String> {
        let chooser = self.origin_import.as_ref()?;
        let names: Vec<&String> = chooser
            .files
            .iter()
            .flat_map(OriginFile::columns_reading_unticked)
            .collect();
        if names.is_empty() {
            return None;
        }
        let shown: Vec<&str> = names.iter().take(3).map(|name| name.as_str()).collect();
        let list = shown.join(", ");
        let list = if names.len() > shown.len() {
            tr!(
                "origin-import-reads-names-more",
                names = list,
                more = i64::try_from(names.len() - shown.len()).unwrap_or(i64::MAX)
            )
        } else {
            list
        };
        Some(tr!(
            "origin-import-reads-warning",
            count = i64::try_from(names.len()).unwrap_or(i64::MAX),
            names = list
        ))
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

    /// A column that only the overlay ticked (its box is greyed out) keeps its
    /// tick: unticking it by hand writes nothing the overlay does not redo.
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

    /// Check All / Check None: every sheet and every graph.
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
            for (picked, info) in entry.graph_picked.iter_mut().zip(&entry.graphs) {
                *picked = ticked && !info.columns.is_empty();
            }
        }
    }

    /// A click on a graph box always changes it: an empty box picks the graph
    /// (its columns are ticked), "some" ticks the missing columns, a ticked box
    /// unpicks it ([`OriginFile::unpick_graph`]).
    pub(super) fn toggle_origin_graph(&mut self, file: usize, graph: usize) {
        if let Some(entry) = self.origin_file_mut(file) {
            if entry.graphs.get(graph).is_none_or(|g| g.columns.is_empty()) {
                return;
            }
            if entry.graph_tick(graph) == OriginTick::All {
                entry.unpick_graph(graph);
            } else {
                entry.pick_graph(graph);
            }
        }
    }

    pub(super) fn set_origin_include_reads(&mut self, include: bool) {
        let Some(chooser) = &mut self.origin_import else {
            return;
        };
        chooser.include_reads = include;
        for file in &mut chooser.files {
            file.set_include_reads(include);
        }
    }

    pub(super) fn set_origin_show(&mut self, show: OriginShow) {
        if let Some(chooser) = &mut self.origin_import {
            chooser.show = show;
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
            show: OriginShow::All,
            include_reads: false,
            accepted_formulas: BTreeSet::new(),
        });
        for mut file in files {
            if !chooser.files.iter().any(|known| known.path == file.path) {
                file.set_include_reads(chooser.include_reads);
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

    /// [`Action::ImportOrigin`](super::Action::ImportOrigin): the formulas are
    /// judged first, before anything is created (#556). A formula that gives
    /// other values than Origin's opens the window `Formulas with Different
    /// Values`; with none, the import runs at once.
    pub(super) fn import_origin(&mut self) {
        if self.origin_import_blocker().is_some() {
            return;
        }
        let Some(chooser) = self.origin_import.clone() else {
            return;
        };
        let prep = prepare_formulas(&chooser);
        let stale = stale_formulas(&prep.judgements);
        if stale.is_empty() {
            self.run_origin_import(chooser, prep, BTreeSet::new());
            return;
        }
        self.origin_formulas = Some(super::origin_formulas::OriginFormulaForm::new(
            &chooser, prep, stale,
        ));
    }

    /// Creates the books, sheets and columns of the plan, then sets the
    /// formulas `accepted` and the equal ones, all in one `Import` entry.
    pub(super) fn run_origin_import(
        &mut self,
        mut chooser: OriginChooser,
        prep: FormulaPrep,
        accepted: BTreeSet<OriginColumnKey>,
    ) {
        if self.handle.begin_macro(MacroKind::Import).is_err() {
            self.fail(tr!("error-import-group"));
            return;
        }
        // Closed before the first command: a failure is then said in the status
        // bar, not in the footer of a chooser that is about to disappear.
        self.origin_import = None;
        self.origin_formulas = None;
        let FormulaPrep {
            plan,
            formulas,
            judgements,
        } = prep;
        let decision = decide_origin_formulas(&formulas, &judgements, &accepted);
        chooser.accepted_formulas = accepted;
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
                    let built = make_origin_column_in(
                        origin_book,
                        column,
                        at.tag_sheet.then_some(origin_sheet.name.as_str()),
                    );
                    Some(match decision.kept.get(&at.key()) {
                        Some(original) => with_formula_note(built, original),
                        None => built,
                    })
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
        let (formulas_set, formulas_kept) =
            self.set_origin_formulas(&formulas, &decision, &created_columns);
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
        } else {
            let mut sentence = if partial_imported {
                tr!("status-import-origin-partial", books = book_count)
            } else {
                tr!(
                    "status-import-origin-done",
                    books = book_count,
                    sheets = sheet_count
                )
            };
            if formulas_set + formulas_kept > 0 {
                sentence = tr!(
                    "status-import-origin-formulas",
                    done = sentence,
                    set = formulas_set,
                    kept = formulas_kept
                );
            }
            if partial_imported {
                self.warn(sentence);
            } else {
                self.notice(sentence);
            }
        }
        if folder_gone {
            self.warn(tr!("status-import-folder-gone"));
        }
    }

    /// Sets the formulas of `decision` on the columns just created, one
    /// `SetColumnMeta` each, inside the import entry. Returns how many formulas
    /// are set and how many columns kept their values (a column that was not
    /// created counts for neither).
    fn set_origin_formulas(
        &mut self,
        formulas: &BTreeMap<OriginColumnKey, PlannedFormula>,
        decision: &FormulaDecision,
        created: &BTreeMap<OriginColumnKey, Option<NodeId>>,
    ) -> (usize, usize) {
        let created_id = |key: &OriginColumnKey| created.get(key).copied().flatten();
        let mut kept = decision
            .kept
            .keys()
            .filter(|key| created_id(key).is_some())
            .count();
        let mut set = 0;
        for key in &decision.set {
            let Some(column) = created_id(key) else {
                continue;
            };
            let Some(stokesforge_importer::PlannedFormula {
                verdict: stokesforge_importer::FormulaVerdict::Translated(pieces),
                ..
            }) = formulas.get(key)
            else {
                continue;
            };
            let tree = &self.snapshot.tree;
            let sheet = tree.parent(column).ok().flatten();
            let locate = |k: OriginColumnKey| -> Option<(NodeId, NodeId)> {
                let id = created_id(&k)?;
                Some((id, tree.parent(id).ok().flatten()?))
            };
            let formula = sheet.and_then(|sheet| build_column_formula(pieces, sheet, &locate));
            let Some(formula) = formula else {
                kept += 1;
                continue;
            };
            let mut meta = self.column_meta(column);
            meta.formula = Some(formula);
            self.execute(Command::SetColumnMeta { column, meta });
            set += 1;
        }
        (set, kept)
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
        chooser.show = OriginShow::All;
        self.import_state = None;
        self.origin_import = Some(chooser);
        self.notice(if undone {
            tr!("status-import-reopened-undone")
        } else {
            tr!("status-import-reopened-kept")
        });
    }
}

/// What the formulas of a chooser come to, computed before anything is created
/// (#556): the plan, the verdict on each formula column, the comparison with
/// Origin's values.
pub(super) struct FormulaPrep {
    pub(super) plan: OriginLayoutPlan,
    pub(super) formulas: BTreeMap<OriginColumnKey, PlannedFormula>,
    pub(super) judgements: BTreeMap<OriginColumnKey, Judgement>,
}

fn prepare_formulas(chooser: &OriginChooser) -> FormulaPrep {
    let stems = file_stems(chooser);
    let sources = origin_sources(chooser, &stems);
    let plan = plan_origin_layout(&sources, chooser.where_, chooser.layout);
    let formulas = plan_origin_formulas(&sources, &plan);
    let judgements = judge_origin_formulas(&sources, &plan, &formulas);
    FormulaPrep {
        plan,
        formulas,
        judgements,
    }
}

fn file_stems(chooser: &OriginChooser) -> Vec<String> {
    chooser
        .files
        .iter()
        .map(|file| {
            file.path
                .file_stem()
                .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
        })
        .collect()
}

fn origin_sources<'a>(chooser: &'a OriginChooser, stems: &'a [String]) -> Vec<OriginSource<'a>> {
    chooser
        .files
        .iter()
        .zip(stems)
        .map(|(file, stem)| OriginSource {
            project: &file.project,
            selection: file.effective(),
            file_stem: stem,
        })
        .collect()
}

/// The plan of a chooser: its ticks, layout and files.
fn origin_plan(chooser: &OriginChooser) -> OriginLayoutPlan {
    let stems = file_stems(chooser);
    plan_origin_layout(
        &origin_sources(chooser, &stems),
        chooser.where_,
        chooser.layout,
    )
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

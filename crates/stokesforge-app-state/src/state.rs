//! The state of the test bench, **with no dependency on egui**.
//!
//! # Why this split
//!
//! A window cannot be tested in continuous integration. All the bench logic
//! therefore lives here, driven by an [`Action`] enum; the `eframe` layer
//! (module `stokesforge_app::app`) only translates keyboard and mouse into
//! actions and paints the state. This is what makes the complete loop — model →
//! command → undo → save → reload — checkable automatically
//! (`tests/shell.rs`).
//!
//! # Two rules of conduct
//!
//! 1. [`AppState::dispatch`] **never returns an error**: every error becomes
//!    a [`StatusMessage`] shown to the user. A test bench that dies on an
//!    invalid input proves nothing.
//! 2. The snapshot it holds is **frozen** (the model tree is persistent): it
//!    must be requested again from the actor after each mutation, which is
//!    what [`AppState::refresh`] does, called by `dispatch` after any action
//!    that may have changed the model.
//!
//! # The Python script (feature `python`)
//!
//! Three decisions, made here and nowhere else:
//!
//! 1. **The interpreter never starts when the window opens.**
//!    [`AppState::new`] neither builds nor touches `stokesforge_script::
//!    ScriptEngine`; only the first [`Action::RunScript`] does, through
//!    `AppState::run_script`. The interpreter startup time is one of the
//!    reasons for the port — losing it at opening would cancel the benefit.
//! 2. **`RunScript` never blocks the caller.** `ScriptEngine::run`, on the
//!    other hand, blocks until the script ends — calling it from the render
//!    loop would freeze the window, including the "Stop" button. `run_script`
//!    therefore starts (lazy engine creation included) on a **detached
//!    thread** and returns at once; [`AppState::poll_script`], to be called
//!    on **every frame** by `stokesforge_app::app` (and by a test that waits
//!    for a run), polls the result with `try_recv` and applies it as soon as
//!    it is ready — snapshot refresh included. This is what makes `dispatch`
//!    safe to call from the render loop for **all** actions, script included,
//!    with no special case on the `stokesforge_app::app` side.
//! 3. **A script exception is not an application error.** It lands in the
//!    output ([`AppState::script_output`]) with its call trace; the status
//!    message set by `poll_script` stays neutral (level `Info`), and the state
//!    remains usable.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use stokesforge_commands::{Command, HistoryLabel, MacroKind, PlotEditKind};
use stokesforge_core::compose::{
    current_plot_spec, free_role_layer, overlay_base, overlays_full, panel_plot_type,
    role_layer_of, row_series, series_from_columns, spec_has_x, x_prime_columns, OVERLAY_ROLES,
};
use stokesforge_core::node::NodeKind;
use stokesforge_core::number_format::{format_exact, format_grid};
use stokesforge_core::tsv;
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, Node, NodeCopy, NodeId, Tree,
    ValueBlock,
};
use stokesforge_importer::{import_items, plan_import, ImportItem, ImportPlan, PlannedColumn};
use stokesforge_plot::{
    AnnotationRef, AnnotationSpec, ColorUsage, OverlaySpec, PanelSpec, Panels, PlotSpec, PlotType,
    SavedColors, SeriesRef, ShapeRef, ShapeSpec, TextAnchor, TextTarget,
};
#[cfg(feature = "python")]
use stokesforge_python::interpreter::EmbeddedPython;
#[cfg(feature = "python")]
use stokesforge_script::{ScriptEngine, ScriptError, ScriptOutcome};
use stokesforge_session::{RecalcEnd, SessionHandle, SessionResult, Snapshot};

use crate::errors;
use crate::history_text::history_text;
use crate::preferences::{ObjectManagerGroup, ObjectManagerSettings, Preferences};
use crate::selection::{GridCursor, Selection};
use crate::tr;
use crate::user_colors::UserColors;

mod analysis;
mod autosave;
mod column_use;
mod transpose;
pub use analysis::{CalculationDelete, StaleExport};
pub use autosave::{RecoveryRow, AUTOSAVE_RETRY};
pub use column_use::FullColumnsCopy;

/// How long a command waits for the formula recalculation it started to
/// finish before handing control back to the interface (#239): a recalculation
/// shorter than that stays invisible and synchronous.
const RECALC_WAIT: std::time::Duration = std::time::Duration::from_millis(200);

/// What the user can ask for. The window layer only produces these values; it
/// never touches the actor directly.
// `ImportFiles` carries a whole import recipe, much bigger than the other
// variants: the `Action` is consumed at once, boxing would gain nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Undoes the last history entry.
    Undo,
    /// Redoes the last undone entry.
    Redo,
    /// Saves the project to the current file ([`AppState::path`]).
    Save,
    /// Saves as `path` and makes it the current file. On failure, the previous
    /// path is restored: the next Ctrl+S does not write to the file that just
    /// failed (#315).
    SaveAs {
        /// Chosen destination.
        path: String,
    },
    /// Reloads the project from the current file.
    Load,
    /// Asks for an empty project; waits for the answer "Discard Changes · Cancel ·
    /// Save" if work would be lost (S-F3, #229).
    NewProject {
        /// Input still local to a view, notably a detached window.
        pending_edits: bool,
    },
    /// Asks to open the project `path` (File › Open…); same wait as
    /// [`Action::NewProject`]. A read failure keeps the current project and
    /// path and is reported by a blocking dialog (S-M8).
    OpenProject {
        /// Chosen file.
        path: String,
        /// Input still local to a view.
        pending_edits: bool,
    },
    /// Asks to quit (Quit, window close); same wait. When nothing holds the
    /// application back any more, [`AppState::take_quit_ready`] tells the
    /// window.
    Quit {
        /// Input still local to a view.
        pending_edits: bool,
    },
    /// Answer to the "Discard Changes · Cancel · Save" dialog (S-F4).
    ConfirmUnsaved(UnsavedChoice),
    /// Closes the dialog of a serious failure (S-M8).
    DismissAlert,
    /// Answer to the overwrite confirmation of an exported file (S-F3).
    ConfirmOverwrite(bool),
    /// Answer to "Saving would overwrite the file that was read without what
    /// the read discarded" (#309, F3): `true` writes, `false` gives up (and
    /// forgets the gesture that was waiting for the save).
    ConfirmDiscardedSave(bool),
    /// Adds an empty graph at the root, undoable like a plot.
    AddGraph,
    /// Adds a workbook and its initial sheet at the root of the project.
    ///
    /// The File menu carries no ambiguous destination: a new workbook always
    /// belongs to the root folder of the open project.
    AddWorkbook,
    /// Adds an empty folder in `parent`, under the first free name "FolderN"
    /// among its children (#326). The File menu and the toolbar target the
    /// root; the right-click on a folder of the Project Manager targets that
    /// folder — the only gesture that creates in a subfolder.
    AddFolder {
        /// Host folder.
        parent: NodeId,
    },
    /// Selects the sheet displayed in the grid of the workbook that contains
    /// it (the workbook is deduced from the parent of `id` in the tree) — a
    /// workbook shown in its own tab `stokesforge_app::app`, selecting a sheet
    /// in one never affects that of another workbook open at the same time.
    SelectSheet(NodeId),
    /// Puts the tab of `sheet` into name editing (double-click, F2, context
    /// menu) and selects it.
    BeginRenameSheet(NodeId),
    /// Commits the name typed for `sheet`. Empty name or already taken in the
    /// workbook: the old name stays, a message explains it, no history entry.
    CommitRenameSheet {
        /// Renamed sheet.
        sheet: NodeId,
        /// Typed name.
        name: String,
    },
    /// Abandons the editing of a sheet name (Esc).
    CancelRenameSheet,
    /// Duplicates `sheet` (columns included) right after it, and shows the
    /// copy.
    DuplicateSheet(NodeId),
    /// Deletes `sheet`, without confirmation: Ctrl+Z gives it back (S-F3, #229);
    /// the message names the graphs that plotted columns of it. Refused for
    /// the last sheet of a workbook.
    DeleteSheet(NodeId),
    /// Renames a Project Manager object after in-place editing.
    RenameObject { id: NodeId, name: String },
    /// Commits all the names typed during a single rename gesture.
    RenameObjects(Vec<(NodeId, String)>),
    /// Copies the selection to the system transport.
    CopyObjects(Vec<NodeId>),
    /// Duplicates the selection in its original folders.
    DuplicateObjects(Vec<NodeId>),
    /// Pastes the system transport into a folder.
    PasteObjects { parent: NodeId, text: String },
    /// Reveals the embedded workbooks referenced by this graph.
    ShowGraphData { graph: NodeId },
    /// Reveals the workbook of the columns of a single curve (curve menu,
    /// #152): no ambiguous choice among the curves of a graph.
    ShowSeriesData {
        graph: NodeId,
        series: stokesforge_plot::SeriesRef,
    },
    /// Deletes the selection, without confirmation: a single Ctrl+Z gives it
    /// back (S-F3, #229); the message names the graphs that used its data.
    DeleteObjects(Vec<NodeId>),
    /// Moves the selection in a single undoable gesture.
    MoveObjects { ids: Vec<NodeId>, parent: NodeId },
    /// Commits the input of a cell: the text as typed, to be converted
    /// according to the column type.
    CommitCell {
        /// Target column.
        column: NodeId,
        /// Target row.
        row: usize,
        /// Typed text, not converted.
        text: String,
    },
    /// Adds a column at the end of `sheet`.
    AddColumn {
        /// Target sheet — explicit, since several workbooks can be displayed
        /// at the same time, each in its own tab.
        sheet: NodeId,
    },
    /// Adds a sheet at the end of `book`, with a name generated by
    /// `stokesforge-core` (`Command::AddWorksheet { name: None, .. }`, same
    /// convention as [`Action::AddWorkbook`] for the workbook name).
    AddWorksheet {
        /// Target workbook.
        book: NodeId,
    },
    /// Adds a column at the end of `sheet` then selects `row` in it — Tab
    /// on the last column of a data row, symmetric to
    /// [`Action::CommitCellAndAdvance`] (Enter on the last row of a column)
    /// but for the column axis: the target column does not exist yet when the
    /// action is pushed (unlike a row, which is only an index), so the
    /// selection can only be set once the column is really created, in the
    /// handling of this action — never before, as `AppState::select_cell`
    /// does for `CommitCellAndAdvance`.
    AddColumnAndSelect {
        /// Target sheet.
        sheet: NodeId,
        /// Row to select in the new column.
        row: usize,
    },
    /// Adds a column at the end of `sheet` then selects `field` in it —
    /// counterpart of [`Action::AddColumnAndSelect`] on the header side: Tab
    /// commits in the Formula field (the last of the four) of the last
    /// column, for the same reason the selection can only be set once the
    /// column is really created.
    AddColumnAndSelectHeaderField {
        /// Target sheet.
        sheet: NodeId,
        /// Header field to select in the new column.
        field: HeaderField,
    },
    /// Changes the type of a column — explicit user choice from the header,
    /// destructive conversion accepted (`Column::set_column_type`,
    /// `stokesforge-core`): a non-numeric text in a column that becomes
    /// `Numeric` disappears (`NaN`), as is already the case for a direct input.
    SetColumnType {
        /// Target column.
        column: NodeId,
        /// Requested type.
        column_type: ColumnType,
    },
    /// Changes the plot role of a column (`ColumnMeta::role`) — explicit
    /// choice from the "Role" submenu of the header context menu, next to the
    /// type. For now it only affects the display (`A(x)`…) and persistence:
    /// nothing on the series resolution side reads it yet.
    SetColumnRole {
        /// Target column.
        column: NodeId,
        /// Requested role.
        role: ColumnRole,
    },
    /// Moves columns within their sheet (#319, `Move ▸` submenu of the right-click
    /// on the letters and of the Columns menu): `columns` moves to `to`, in a
    /// single undoable gesture (one `Move` entry for one column, one
    /// `MoveColumns` macro for several). No "column in use" window: a
    /// formula follows its column (#321). No effect when the gesture would
    /// change nothing (the menu entry is then greyed out). The selection stays
    /// on the moved columns.
    MoveColumns {
        /// Sheet of the columns.
        sheet: NodeId,
        /// Columns to move (the selection).
        columns: Vec<NodeId>,
        /// Where they go.
        to: crate::column_move::ColumnMove,
    },
    /// Sets the width of a grid column, in logical points (#325), at the
    /// **end** of a manual resize. A view setting, like a window's geometry:
    /// no history and no `revision` (Ctrl+Z does not undo it), saved in the
    /// `.stf` per column. Emits no command; see [`AppState::set_column_width`].
    SetColumnWidth {
        /// Resized column.
        column: NodeId,
        /// New width.
        width: f32,
    },
    /// Resets columns to the default width (#330): their key leaves the table
    /// (the grid then applies its default). Same status as `SetColumnWidth`:
    /// no history and no `revision`, no command; see
    /// [`AppState::reset_column_widths`].
    ResetColumnWidths {
        /// Target columns (a selection is handled as a whole).
        columns: Vec<NodeId>,
    },
    /// Inserts `count` rows at `at` in `sheet`.
    InsertRows {
        /// Target sheet.
        sheet: NodeId,
        /// Insertion row.
        at: usize,
        /// Number of inserted rows.
        count: usize,
    },
    /// Deletes `count` rows at `at` in `sheet`.
    DeleteRows {
        /// Target sheet.
        sheet: NodeId,
        /// First deleted row.
        at: usize,
        /// Number of deleted rows.
        count: usize,
    },
    /// Replaces the text of a note as a block (editor loses focus, like
    /// [`Action::CommitCell`] for a grid cell).
    SetNoteText {
        /// Target note.
        note: NodeId,
        /// New text.
        text: String,
    },
    /// Runs the script currently in the input area
    /// ([`AppState::script_source`]).
    ///
    /// **Does not block**: see the section "The Python script" at the top of
    /// this module. Without the `python` feature, produces an explicit
    /// [`StatusMessage`] and sends no command.
    RunScript,
    /// Asks to stop the running script. No effect if there is none.
    /// Does not block.
    InterruptScript,
    /// Commits the input of a cell (like [`Action::CommitCell`]) then, if
    /// `row` was the last row of `column` and the commit succeeded, adds a
    /// row to `sheet` — Enter on the last cell of a column.
    CommitCellAndAdvance {
        /// Target column.
        column: NodeId,
        /// Target row.
        row: usize,
        /// Typed text, not converted.
        text: String,
        /// Sheet to add a row to if needed.
        sheet: NodeId,
    },
    /// Clears the content of the cells currently selected in `sheet`,
    /// without shifting what follows — Backspace or Ctrl+X.
    ClearSelectedCells {
        /// Target sheet.
        sheet: NodeId,
    },
    /// Removes the formula of the columns whose Formula field is selected in
    /// `sheet` **keeping** their values (right-click, #236). The ordinary
    /// removal (field emptied) also deletes the values.
    RemoveFormulaKeepData {
        /// Target sheet.
        sheet: NodeId,
    },
    /// Same removal, for whole columns (Workbook › Columns menu, #236):
    /// columns without a formula are ignored.
    RemoveColumnsFormulaKeepData {
        /// Target columns.
        columns: Vec<NodeId>,
    },
    /// Clears the content of the cells currently selected in `sheet`
    /// **and** moves the following cells of each touched column up,
    /// independently — Delete key.
    DeleteSelectedCells {
        /// Target sheet.
        sheet: NodeId,
    },
    /// Pastes `text` from the anchor of the current selection of `sheet`
    /// — Ctrl+V. `text` is the text received from egui (`egui::Event::Paste`),
    /// that is, the content of the **system** clipboard: [`AppState::
    /// paste`] compares `text` with what it pushed to the clipboard itself
    /// ([`AppState::copy_selection`]/[`AppState::copy_full_columns`]) to
    /// decide whether to replay its internal structure or to handle `text` as
    /// external text — see the behavior matrix at the top of this
    /// module.
    Paste {
        /// Target sheet.
        sheet: NodeId,
        /// Text received from the system clipboard.
        text: String,
    },
    /// Pastes at the absolute position from the internal clipboard — Ctrl+Shift+
    /// V. `egui-winit` translates it into the **same** `egui::Event::Paste` as
    /// Ctrl+V (`is_paste_command` never tests `shift`): `stokesforge_app::app`
    /// tells them apart by reading `modifiers.shift` at the time of the event,
    /// and throws away the text carried by the event — everything here comes
    /// from the last [`AppState::copy_selection`]/
    /// [`AppState::copy_full_columns`] of this session. No effect if nothing
    /// has been copied yet.
    PasteAbsolute {
        /// Target sheet.
        sheet: NodeId,
    },
    /// Changes the type of all the `columns` in **a single undo step**
    /// (columns menu on a multiple selection, #159).
    SetColumnsType {
        /// Target columns.
        columns: Vec<NodeId>,
        /// New type.
        column_type: ColumnType,
    },
    /// Changes the role of all the `columns` in a single undo step.
    SetColumnsRole {
        /// Target columns.
        columns: Vec<NodeId>,
        /// New role.
        role: ColumnRole,
    },
    /// Selects all the visible columns of `sheet`, header included,
    /// even without a row — Ctrl+A in the grid, Edit › Select All and corner
    /// box (#159). Pure interface state: no model, no history.
    SelectAll {
        /// Target sheet.
        sheet: NodeId,
    },
    /// Replaces the displayed long name of a column
    /// ([`stokesforge_core::ColumnMeta::long_name`]).
    SetColumnLongName {
        /// Target column.
        column: NodeId,
        /// New long name.
        text: String,
    },
    /// Replaces the displayed unit of a column
    /// ([`stokesforge_core::ColumnMeta::unit`]).
    SetColumnUnit {
        /// Target column.
        column: NodeId,
        /// New unit.
        text: String,
    },
    /// Replaces the displayed comment of a column
    /// ([`stokesforge_core::ColumnMeta::comment`]).
    SetColumnComment {
        /// Target column.
        column: NodeId,
        /// New comment.
        text: String,
    },
    /// Replaces the x' value of a column
    /// ([`stokesforge_core::ColumnMeta::x_prime`], #164): empty text =
    /// cleared; a text that is not a finite number is refused (status
    /// bar), nothing is written.
    SetColumnXPrime {
        /// Target column.
        column: NodeId,
        /// Typed value.
        text: String,
    },
    /// Replaces the formula of a column
    /// ([`stokesforge_core::ColumnMeta::formula`]) — evaluation stays that
    /// of `stokesforge_core::formula` / `stokesforge-session::actor`, outside
    /// the scope of this action. The text is typed in **letters**; it is
    /// converted into column identifiers by
    /// [`stokesforge_core::Tree::formula_from_letters`] (#321). A refusal
    /// (unknown letter, self-reference, `#REF`) is reported in the status bar
    /// and nothing is written.
    SetColumnFormula {
        /// Target column.
        column: NodeId,
        /// New formula in letters, or `None` for an uncomputed column.
        formula: Option<String>,
    },
    /// Imports the files to the destination chosen in the dialog.
    /// The whole batch (all sheets, all columns) fits in a single history
    /// entry.
    ImportFiles {
        /// Destination of the produced sheets.
        destination: ImportDestination,
        /// Analysis settings to apply to each file.
        recipe: stokesforge_import::ImportRecipe,
        /// Files to import, each in its own sheet.
        paths: Vec<std::path::PathBuf>,
    },
    /// Files dropped on the application (drag and drop): imported without
    /// going through the import window when they read correctly, see
    /// [`file_drop_plan`]. Same "Import" history macro as
    /// [`Action::ImportFiles`].
    DropFiles {
        /// Dropped files, in the order given by the system.
        paths: Vec<std::path::PathBuf>,
        /// Workbook under the cursor at the time of the drop, if there was one.
        target_book: Option<NodeId>,
    },
    /// Like [`Action::ImportFiles`], but the analysis of the files runs on
    /// a background thread, with progress: the interface does not freeze. The
    /// sheets are created at the end ([`AppState::poll_import`]).
    StartImport {
        /// Destination of the produced sheets.
        destination: ImportDestination,
        /// Analysis settings to apply to each file.
        recipe: stokesforge_import::ImportRecipe,
        /// Files to import.
        paths: Vec<std::path::PathBuf>,
    },
    /// Reopens the last import in the wizard, to correct it: if it is still
    /// the last history entry, it is undone first.
    ReopenLastImport,
    /// Refreshes the imported columns of `sheet` from their source files
    /// (reread with the stored recipe), **in place**: the columns keep
    /// their identity, so the graphs that plot them stay valid.
    RefreshFromSource {
        /// Sheet to refresh.
        sheet: NodeId,
    },
    /// Opens the (read-only) window of the file metadata of
    /// `column` — click on the "Metadata" row of the header.
    ViewColumnMetadata {
        /// Column whose metadata is viewed.
        column: NodeId,
    },
    /// Closes the metadata window.
    CloseColumnMetadata,
    /// Opens the window of the last 50 status bar messages (S-M6).
    OpenStatusHistory,
    /// Closes the message history window.
    CloseStatusHistory,
    /// Opens the list of the project's low R² by type (badge, #290).
    OpenLowR2List,
    /// Closes this list.
    CloseLowR2List,
    /// Stops the background import after the current file (#232, S-P2): nothing
    /// is created.
    StopImport,
    /// Stops the running formula recalculation (#239, S-P2): the command that
    /// triggered it is undone, as by Ctrl+Z.
    StopRecalc,
    /// Ouvre Help › Keyboard Shortcuts (#231, S-K5).
    OpenKeyboardShortcuts,
    /// Closes the keyboard shortcuts window.
    CloseKeyboardShortcuts,
    /// Ouvre File › Preferences (#148).
    OpenPreferences,
    /// Closes the preferences window.
    ClosePreferences,
    /// Opens the analysis window (#181) on the selected Y columns of
    /// `sheet`, each with the nearest X to its left. Nothing opens
    /// without a Y column.
    OpenAnalysis {
        /// The operation family and the pre-chosen method.
        preset: crate::analysis::Preset,
        /// Sheet containing the columns.
        sheet: NodeId,
        /// Columns selected at the time of the gesture.
        columns: Vec<NodeId>,
    },
    /// Opens the analysis on the exact pairs of the graph and its view framing.
    OpenGraphAnalysis {
        preset: crate::analysis::Preset,
        graph: NodeId,
        series: Vec<stokesforge_plot::SeriesRef>,
        spec: Box<PlotSpec>,
    },
    /// Opens the analysis on **rows** `(sheet, row)` (#253, 0 = the
    /// first): the linked transposed sheet is created or completed, in a
    /// single undoable group with the computation, then the window opens on its
    /// columns.
    OpenRowAnalysis {
        /// The operation family and the pre-chosen method.
        preset: crate::analysis::Preset,
        /// The target rows.
        rows: Vec<(NodeId, usize)>,
    },
    /// Updates a transposed sheet from its source (#253); asks for
    /// confirmation if edited values would be overwritten.
    RecalculateTranspose {
        /// The transposed sheet.
        sheet: NodeId,
    },
    /// Answer to this confirmation (`true`: overwrite and update).
    ConfirmTransposeRelink(bool),
    /// Reopens one of the last analyses run (#249) on the current
    /// selection: the window opens with its settings; with `run` (Ctrl+click)
    /// it is run at once if nothing is missing.
    OpenRecentAnalysis {
        /// Rank in [`AppState::analysis_recent`], 0 for the most recent.
        index: usize,
        run: bool,
        target: crate::analysis::RecentTarget,
    },
    /// "Restore Original Data" (#248): gives back to the columns that a correction
    /// replaced their earlier values, overwrites the input made since;
    /// `Ctrl+Z` undoes. Carries the trace of the correction (Results row).
    RestoreOriginalData(NodeId),
    /// Reopens an existing calculation in the analysis window.
    EditCalculation(NodeId),
    /// End of a handle drag on the graph (#199): the calculation receives the
    /// new position, in **one** history entry (`UpdateCalculation`).
    MoveCalculationHandle {
        calculation: NodeId,
        slot: crate::analysis::HandleSlot,
        x: f64,
        y: f64,
    },
    /// Closes the analysis window without saving anything.
    CloseAnalysis,
    /// Commits the analysis window: a new or modified calculation, a single
    /// history entry.
    ApplyAnalysis,
    /// Answer to the replacement of a source plotted by several curves.
    ConfirmAnalysisReplacement(bool),
    /// Recalculates calculations, outside history; empty list: all the stale
    /// calculations.
    RecalculateCalculations(Vec<NodeId>),
    /// Removes a calculation and lets its columns become ordinary.
    DetachCalculation(NodeId),
    /// Asks to delete a calculation: opens the confirmation.
    RequestDeleteCalculation(NodeId),
    /// Answer to the confirmation of [`Action::RequestDeleteCalculation`].
    ConfirmDeleteCalculation(CalculationDelete),
    /// Answer to the confirmation of a graph export that shows series that
    /// are not up to date (#198, mockup M9 a).
    ConfirmStaleExport(StaleExport),
    /// Closes the "Column in use" window without doing anything (#306).
    CloseColumnUse,
    /// Answer to the Ctrl+Shift+C question on columns with a formula (#306).
    ConfirmFullColumnsCopy(FullColumnsCopy),
    /// Undoes the deletion that the "Column in use" window just reported (#306).
    UndoColumnUse,
    /// Creates a graph from the selected columns of a workbook sheet —
    /// context menu of the column header. The new graph is added in the same
    /// parent folder as the workbook ([`Command::
    /// AddGraph`]), with one series per column of role
    /// [`stokesforge_core::ColumnRole::Y`] among `columns`, paired with the
    /// nearest X column to its left in the sheet (see [`AppState::
    /// plot_columns`] for the full rule). Sends no command if
    /// `columns` contains no Y: no empty graph.
    PlotColumns {
        /// Sheet containing the columns.
        sheet: NodeId,
        /// Columns selected at the time of the gesture.
        columns: Vec<NodeId>,
        /// Plot type requested for each created series.
        plot_type: PlotType,
    },
    /// Adds series to an existing graph, built from workbook
    /// columns — same X/Y pairing rule as
    /// [`Action::PlotColumns`], but by rereading the current
    /// [`stokesforge_plot::PlotSpec`] of the graph rather than creating a new one
    /// ([`Command::SetPlotSpec`]). Serves the drag of a column selection
    /// to a graph (#160): a single undo step.
    AddSeriesToGraph {
        /// Target graph.
        graph: NodeId,
        /// Target layer, by its index in the graph. The drop passes the
        /// active layer of the graph (#161). The Y2 columns go to the
        /// second layer overlaid on this one (#167), created if needed.
        panel: usize,
        /// Columns to add as series.
        columns: Vec<NodeId>,
        /// Plot type requested for each new series; `None` takes
        /// that of the target layer (see [`panel_plot_type`]).
        plot_type: Option<PlotType>,
    },
    /// Plots whole **rows** of a sheet to a graph (#166):
    /// one series per row, whose points are the values of the row in
    /// the columns that carry an x', placed according to that x' (the special
    /// x' row plays the role of a second X). `axis` says where: see [`RowAxis`]. A
    /// single undo step, new layer included.
    AddRowsToGraph {
        /// Target graph.
        graph: NodeId,
        /// Target layer (the active layer, #161).
        panel: usize,
        /// Sheet the rows come from.
        sheet: NodeId,
        /// Plotted rows (0 = the first).
        rows: Vec<usize>,
        /// X axis of the series.
        axis: RowAxis,
    },
    /// Creates a graph that plots whole **rows** of a sheet (Plot
    /// menu of a row selection, #166): one series per row, against the
    /// x' of the columns. Refused without an x' column. A single undo step.
    PlotRows {
        /// Sheet the rows come from.
        sheet: NodeId,
        /// Plotted rows (0 = the first).
        rows: Vec<usize>,
        /// Plot type of each series.
        plot_type: PlotType,
    },
    /// Answer to the question of [`RowAxis::Ask`]: `Some` chooses the axis,
    /// `None` abandons the drop.
    ConfirmRowDrop(Option<RowAxis>),
    /// Adds to a graph a layer overlaid on that of `panel` (#167), in the
    /// manner of Origin's "New Layer (Axes)": `x_top` shows its X axis
    /// at the top, `y_right` its Y axis on the right, each with its own scale; an
    /// axis not shown follows that of the covered layer. A single undo
    /// step.
    AddLayer {
        /// Target graph.
        graph: NodeId,
        /// Layer (ordinary or overlaid) whose base layer is covered.
        panel: usize,
        /// X axis of the new layer at the top, with its own scale.
        x_top: bool,
        /// Y axis of the new layer on the right, with its own scale.
        y_right: bool,
    },
    /// Opens or closes the panel; a user preference, outside history.
    SetObjectManagerOpen(bool),
    /// Lists or hides the scales (axes) in the Object Manager (#111).
    SetObjectManagerShowScales(bool),
    /// Lists or hides the "Objects" group (free objects) in the Object Manager.
    SetObjectManagerShowObjects(bool),
    /// Initial fold state of a family, outside project and history (#139).
    SetObjectManagerGroupOpen {
        group: ObjectManagerGroup,
        open: bool,
    },
    /// Restores the initial values without erasing the current fold states.
    ResetObjectManagerSettings,
    /// Visibility of a content or of a set of contents (#81–82).
    SetPlotVisibility {
        graph: NodeId,
        target: crate::object_manager::VisibilityTarget,
        visible: bool,
    },
    /// Closes or reopens the frame of a layer (#344): a single history
    /// entry, which Undo reverts as a block.
    SetClosedFrame {
        graph: NodeId,
        panel: usize,
        closed: bool,
    },
    /// Removes a content from the graph through the same path from menu and tree.
    RemovePlotObject {
        graph: NodeId,
        target: crate::object_manager::VisibilityTarget,
    },
    /// Places a series in a container (drag and drop): alone (`group`
    /// `None`) or in a group, at this rank of this container — see
    /// `PanelSpec::place_series`. A single history entry per gesture,
    /// whether it reorders, moves into a group or moves out of one.
    PlaceSeries {
        graph: NodeId,
        source: stokesforge_plot::SeriesRef,
        group: Option<u32>,
        position: usize,
    },
    /// Places a series group at rank `position` among the slots of
    /// its layer (drag and drop).
    PlaceSeriesGroup {
        graph: NodeId,
        group: stokesforge_plot::SeriesGroupRef,
        position: usize,
    },
    /// Puts a free text or a shape back at rank `index` of its category, in
    /// its layer (drag and drop): the order of the objects is the
    /// stacking order.
    ReorderObject {
        graph: NodeId,
        target: crate::object_manager::VisibilityTarget,
        index: usize,
    },
    /// Moves a series up or down one step (#197): it stays in its
    /// group, or jumps over a whole group.
    MoveSeries {
        graph: NodeId,
        series: stokesforge_plot::SeriesRef,
        up: bool,
    },
    /// Moves a series group up or down as a block (#197).
    MoveSeriesGroup {
        graph: NodeId,
        group: stokesforge_plot::SeriesGroupRef,
        up: bool,
    },
    /// Names a series group; an empty name gives back the automatic
    /// name (#197).
    RenameSeriesGroup {
        graph: NodeId,
        group: stokesforge_plot::SeriesGroupRef,
        name: String,
    },
    /// Dissolves a series group: its series stay, plain, with their
    /// style (#197). Deleting the group and its series goes through
    /// [`Action::RemovePlotObject`].
    UngroupSeries {
        graph: NodeId,
        group: stokesforge_plot::SeriesGroupRef,
    },
    /// Properties on the exact series or layer.
    OpenObjectProperties {
        graph: NodeId,
        target: crate::object_manager::VisibilityTarget,
    },
    /// Replaces the [`PlotSpec`] of a graph as a block — properties
    /// window of a graph ([`Command::SetPlotSpec`]). Pushed when a
    /// draft is committed ([`AppState::commit_plot_edit`]), never on
    /// each keystroke or each pixel of a drag: see the top-level doc of
    /// [`PlotEdit`].
    SetPlotSpec {
        /// Target graph.
        graph: NodeId,
        /// New description of the graph.
        spec: PlotSpec,
    },
    /// Opens (or brings to the wanted tab) the properties window of the
    /// graph `graph` — right-click on a graph component, or the
    /// "Graph" menu. `axis` chooses the axis shown by the "Axes" tab
    /// (`None`: keep the one already chosen).
    OpenPlotSettings {
        /// Target graph.
        graph: NodeId,
        /// Tab to show.
        tab: PlotSettingsTab,
        /// Axis to show in the "Axes" tab.
        axis: Option<PlotAxis>,
    },
    /// Saves a color for `usage`, in the project (undoable command
    /// [`Command::SetSavedColors`]) or for the user (configuration file,
    /// [`crate::user_colors`], outside history).
    SaveColor {
        /// Usage: curves, text or backgrounds.
        usage: ColorUsage,
        /// Color `#RRGGBB` or `#RRGGBBAA`.
        color: String,
        /// Where to save it.
        target: SaveTarget,
    },
    /// Removes a saved color — counterpart of [`Action::SaveColor`].
    RemoveSavedColor {
        /// Usage: curves, text or backgrounds.
        usage: ColorUsage,
        /// Color to remove.
        color: String,
        /// Where to remove it from.
        target: SaveTarget,
    },
    /// Saves a color gradient (#178) under `name`, in the project (undoable command
    /// [`Command::SetSavedColors`]) or for the user; a color gradient of the
    /// same name is replaced.
    SaveGradient {
        /// Name chosen by the user.
        name: String,
        /// `#RRGGBB` stops, at least two.
        stops: Vec<String>,
        /// Where to save it.
        target: SaveTarget,
    },
    /// Deletes the saved color gradient `name` — counterpart of
    /// [`Action::SaveGradient`].
    RemoveSavedGradient {
        /// Name of the color gradient.
        name: String,
        /// Where to delete it from.
        target: SaveTarget,
    },
    /// Opens a native file picker on a separate thread (issue #192);
    /// the window intercepts it, `AppState::dispatch` does nothing.
    PickFile(crate::file_picker::PickRequest),
    /// Exports a graph as SVG, PDF or PNG (issue #34), the format read from
    /// the extension of `path` (`.svg` if it is missing).
    ///
    /// `spec` is the spec **already framed** by the view — the current zoom,
    /// `GraphView::framed_spec`: the export shows what you see. The model
    /// is not touched, so no command (a zoom cannot be undone, nor can an
    /// export).
    ExportGraph {
        /// File to write.
        path: PathBuf,
        /// Framed spec to export.
        spec: Box<PlotSpec>,
    },
    /// Copies the graph to an external application (issue #149): SVG, PNG
    /// then PDF of the **same** figure, offered together to the system
    /// clipboard — the receiving application chooses.
    ///
    /// Like [`Action::ExportGraph`], `spec` is the spec **already framed** by the
    /// view (`GraphView::framed_spec`) and the model is not touched: no
    /// command, no history. The action prepares the bytes
    /// ([`AppState::take_figure_clipboard`]); the window offers them
    /// to the system, because only it knows its display.
    CopyGraphFigure {
        /// Framed spec to copy.
        spec: Box<PlotSpec>,
    },
    /// Sets the scale of PNG exports ([`PNG_EXPORT_SCALES`]).
    SetPngExportScale(f32),
    /// Remembers the size and position of a kind of dialog when it is
    /// closed (S-F8, #228). Application preference: no command, no
    /// history, no `.stf`.
    RememberDialogGeometry {
        /// The kind of dialog (key drawn from its identifier).
        kind: String,
        geometry: crate::preferences::DialogGeometry,
    },
    /// Sets how the text of SVG exports is written, text or outlines (#162).
    /// Application preference: no command, no history, no `.stf`.
    SetSvgTextMode(stokesforge_render_svg::SvgTextMode),
    /// Sets the size cap of a `.stf` at opening, in MB (A2, #316).
    /// Application preference: no command, no history, no `.stf`.
    SetMaxFileMb(u32),
    /// Global cell budget at opening, in millions (B1, #316).
    SetMaxCellsM(u32),
    /// Turns the automatic save on or off (#317). Application
    /// preference: no command, no history, no `.stf`.
    SetAutosave(bool),
    /// Sets the period of the automatic save, in minutes (#317);
    /// clamped to 1–60.
    SetAutosaveMinutes(u32),
    /// Recovery (#317): opens the copy `path` in the current window,
    /// behind "Save?" if there is work; the list stays open.
    RecoveryOpen(PathBuf),
    /// Recovery: leaves the copy on disk, it will be offered again at the
    /// next startup.
    RecoveryIgnore(PathBuf),
    /// Recovery: asks to delete the copy (confirmation).
    RecoveryDelete(PathBuf),
    /// Answer to the confirmation of the deletion of a copy.
    ConfirmRecoveryDelete(bool),
    /// Closes the recovery window: each remaining copy is ignored.
    CloseRecovery,
    /// Sets the order and state of the formats offered by Copy Graph (#148,
    /// #149). Application preference: no command, no history, no
    /// `.stf`.
    SetCopyFormats(Vec<crate::preferences::CopyFormatEntry>),
    /// Sets the header rows (Metadata, x') that **new** workbooks show.
    /// Application preference: no command, no history, no
    /// `.stf`; open workbooks keep their own setting.
    SetWorkbookHeaderDefaults { metadata: bool, x_prime: bool },
    /// Allows or turns off the "Very low R²" box (#288). Application
    /// preference, like [`Action::SetWorkbookHeaderDefaults`]; the setting
    /// of each calculation (`Calculation::low_r2_dialog`) turns it off too.
    SetAnalysisLowR2Dialog(bool),
    /// Sets a free text (issue #35): replaces that of `target`, or
    /// adds it if `target` is the rank after the last
    /// ([`AnnotationRef::set`]). **One** model change, hence one
    /// history entry — whatever the number of frames the in-place
    /// editing or the drag lasted, whose preview lives in the view.
    SetAnnotation {
        /// Target graph.
        graph: NodeId,
        /// Address of the text.
        target: AnnotationRef,
        /// Its new version.
        annotation: Box<AnnotationSpec>,
    },
    /// Removes a free text (Delete, context menu).
    RemoveAnnotation {
        /// Target graph.
        graph: NodeId,
        /// Address of the text.
        target: AnnotationRef,
    },
    /// Sets a graphic object (issue #76): replaces that of `target`, or
    /// adds it if `target` is the rank after the last
    /// ([`ShapeRef::set`]). Like [`Action::SetAnnotation`], **one** history
    /// entry whatever the number of frames of the gesture.
    SetShape {
        /// Target graph.
        graph: NodeId,
        /// Address of the object.
        target: ShapeRef,
        /// Its new version.
        shape: Box<ShapeSpec>,
    },
    /// Opens the history macro of the properties window of an object
    /// (S-F1): settings are applied live, a single Ctrl+Z undoes them.
    BeginShapeEdit,
    /// Closes the macro of [`Action::BeginShapeEdit`]: `keep` (OK) keeps it
    /// as a history entry, otherwise (Cancel) it is aborted and the
    /// object gets back its state at opening.
    EndShapeEdit {
        /// OK (`true`) or Cancel (`false`).
        keep: bool,
    },
    /// Removes a graphic object (Delete, menu).
    RemoveShape {
        /// Target graph.
        graph: NodeId,
        /// Address of the object.
        target: ShapeRef,
    },
    /// Changes the stacking rank of a graphic object.
    ReorderShape {
        /// Target graph.
        graph: NodeId,
        /// Address of the object.
        target: ShapeRef,
        /// Where to put it.
        order: ShapeOrder,
    },
    /// Sets the position of a title or of the legend (issue #53): the drag
    /// that ends, or, with `anchor: None`, "Reset Position". A
    /// model change ([`Command::SetPlotSpec`]), like
    /// [`Action::SetAnnotation`].
    SetTextAnchor {
        /// Target graph.
        graph: NodeId,
        /// The moved text.
        target: TextTarget,
        /// Its new anchor; `None` puts it back at its default place.
        anchor: Option<TextAnchor>,
    },
    /// Sets the font and size of a title or of the legend (issues #77,
    /// #78): an attribute of the whole area, not a tag in its text.
    /// A model change ([`Command::SetPlotSpec`]).
    SetTextFont {
        /// Target graph.
        graph: NodeId,
        /// The adjusted text.
        target: TextTarget,
        /// Its new font and its new size.
        font: stokesforge_plot::TextFont,
    },
    /// Sets the color of a title, an axis title or the legend (issue
    /// #90): an attribute of the whole area, like the font. Empty =
    /// automatic. A model change ([`Command::SetPlotSpec`]).
    SetTextColor {
        /// Target graph.
        graph: NodeId,
        /// The adjusted text.
        target: TextTarget,
        /// Its new color (`#RRGGBB[AA]`), empty for automatic.
        color: String,
    },
    /// Puts **all** the titles and the legend of the graph back at their
    /// default place (*Graph* menu).
    ResetTextAnchors {
        /// Target graph.
        graph: NodeId,
    },
    /// Replaces the typed text of a title (double-click, in-place editor).
    SetTextContent {
        /// Target graph.
        graph: NodeId,
        /// The edited title.
        target: TextTarget,
        /// Its new text (Typst markup).
        text: String,
    },
    /// Renames a series as it reads in the legend (double-click on
    /// an entry). An empty name gives the series back its name derived from the column.
    SetLegendEntryName {
        /// Target graph.
        graph: NodeId,
        /// The series.
        series: SeriesRef,
        /// Its new name in the legend.
        name: String,
    },
}

/// The magnifications offered for a PNG export: the figure is
/// 480 × 360 points (`DEFAULT_FIGURE_SIZE`), so ×4 gives 1920 × 1440 px,
/// about 300 dpi for a figure 16 cm wide.
pub const PNG_EXPORT_SCALES: [f32; 3] = [1.0, 2.0, 4.0];

/// The default PNG magnification: the resolution of a publication
/// figure.
pub const DEFAULT_PNG_EXPORT_SCALE: f32 = 4.0;

/// The file name offered to export `graph`: its name in the project,
/// as `.svg`, without path separator.
pub fn export_file_name(tree: &Tree, graph: NodeId) -> String {
    let name = tree
        .node(graph)
        .map_or("graph", |node| node.name())
        .replace(['/', '\\'], "_");
    format!("{name}.svg")
}

/// Where to save a color of the color picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SaveTarget {
    /// In the project: the color travels with the `.stf` file, and
    /// saving it is undone by Ctrl+Z. The default choice.
    #[default]
    Project,
    /// For the user: the color stays on this machine, offered
    /// in all projects.
    User,
}

impl SaveTarget {
    /// Label of the choice.
    pub fn label(self) -> String {
        match self {
            SaveTarget::Project => crate::tr!("color-target-project"),
            SaveTarget::User => crate::tr!("color-target-user"),
        }
    }
}

/// Severity of a status bar message (S-M4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    /// Normal information: fades by itself after [`INFO_LIFETIME`] (S-M5).
    Info,
    /// Warning: "impossible for now" (undo with nothing to undo), not a
    /// failure. Stays until the next message.
    Warning,
    /// Failure (unreadable path, script in error). Stays until the next
    /// message.
    Error,
}

/// How long an information is displayed before it fades (S-M5).
pub const INFO_LIFETIME: std::time::Duration = std::time::Duration::from_secs(8);

/// Number of messages kept for the status bar history (S-M6).
pub const STATUS_HISTORY_LEN: usize = 50;

/// One form of the copied figure (#149): its bytes and the export format that
/// produced them. Neutral toward the system: the MIME type and the offer to the
/// clipboard are the window's business (`app/figure_copy.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct FigureBytes {
    /// The export format that produced `bytes`.
    pub format: stokesforge_render_svg::ExportFormat,
    /// The content.
    pub bytes: Vec<u8>,
}

/// A message to show to the user, with its severity and its time.
///
/// The level separates three things that Qt told apart by greying out an
/// action: "impossible for now" (undo with nothing to undo) is a
/// warning, **not** an error, whereas an unreadable path is one.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusMessage {
    /// Displayed text.
    pub text: String,
    /// Severity.
    pub level: StatusLevel,
    /// Monotonic instant of emission (for fading).
    pub at: std::time::Instant,
    /// Wall-clock time of emission (for the history).
    pub wall: std::time::SystemTime,
}

impl StatusMessage {
    /// True if the message reports a failure.
    pub fn is_error(&self) -> bool {
        self.level == StatusLevel::Error
    }
}

/// What a script run produced, as the application state
/// presents it.
///
/// A copy **deliberately independent** of `stokesforge_script::ScriptOutcome`:
/// `state.rs` must stay readable, and its public surface stable, whether the
/// `python` feature is active or not — [`AppState::script_output`] returns this
/// type in both configurations rather than exposing a type that
/// would only exist under the feature.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptOutput {
    /// What the script wrote on standard output (`print`).
    pub stdout: String,
    /// What the script wrote on the error output.
    pub stderr: String,
    /// Error message **with its full Python call trace**, if the
    /// script raised an exception. The status bar marks it as an error
    /// (S-M10) and points here, to the console, for the detail.
    pub error: Option<String>,
    /// True if the run stopped on a requested interruption
    /// ([`Action::InterruptScript`]).
    pub interrupted: bool,
}

#[cfg(feature = "python")]
impl From<ScriptOutcome> for ScriptOutput {
    fn from(outcome: ScriptOutcome) -> Self {
        Self {
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            error: outcome.error,
            interrupted: outcome.interrupted,
        }
    }
}

/// What [`AppState::cell_text`] displays for an **absent** value
/// (`Cell::Text(None)`) — see its documentation (A9, option 2). A non-breaking
/// space rather than an empty string: it fills the cell like an
/// ordinary blank (nothing on screen "catches the eye"), but stays, in memory,
/// a string different from `""` — hence distinct from a **present** empty
/// string. Chosen rather than a visible character (`—`, `∅`, `?`) because
/// none of those is an illegitimate answer to "what might a user want to
/// type in a text cell?" — a non-breaking space, on the other hand, can be
/// typed on no usual keyboard.
pub const MISSING_TEXT_DISPLAY: &str = "\u{00A0}";

/// Cap on the identifiers named in the load warning: beyond it,
/// the list is abridged by "…" (the count stays exact).
const DISCARDED_IDS_SHOWN: usize = 5;

/// "2 calculations and 1 correction record": what a read discarded,
/// counted; `None` when nothing was.
fn discarded_what(calculations: usize, corrections: usize) -> Option<String> {
    let calculations_part = (calculations > 0)
        .then(|| crate::tr!("status-load-discarded-calculations", count = calculations));
    let corrections_part = (corrections > 0)
        .then(|| crate::tr!("status-load-discarded-corrections", count = corrections));
    match (calculations_part, corrections_part) {
        (Some(first), Some(second)) => Some(crate::tr!(
            "status-load-discarded-and",
            first = first,
            second = second
        )),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// The load warning for what the read discarded (#278,
/// #309): calculations **and** correction records in a single message that
/// counts them, `None` when nothing was discarded. The identifiers are those of the
/// file; calculations first, then records.
fn discarded_calculations_text(report: &stokesforge_session::LoadReport) -> Option<String> {
    let calculations = &report.discarded_calculations;
    let corrections = &report.discarded_corrections;
    let total = calculations.len() + corrections.len();
    if total == 0 {
        return None;
    }
    let what = discarded_what(calculations.len(), corrections.len())?;
    let mut ids = calculations
        .iter()
        .chain(corrections)
        .take(DISCARDED_IDS_SHOWN)
        .map(|d| format!("#{}", d.id))
        .collect::<Vec<_>>()
        .join(", ");
    if total > DISCARDED_IDS_SHOWN {
        ids.push_str(", …");
    }
    // In French the past participle agrees with records alone: two keys.
    Some(if calculations.is_empty() {
        crate::tr!(
            "status-load-discarded-corrections-only",
            what = what,
            total = total,
            ids = ids
        )
    } else {
        crate::tr!(
            "status-load-discarded",
            what = what,
            total = total,
            ids = ids
        )
    })
}

/// Builds the value block of a column pasted from a TSV: the
/// column of index `col_index` of each line of `rows` (a shorter line
/// provides an empty cell; pastes of data rows go
/// through [`value_block_keeping_absent_cells`], which keeps the existing value),
/// read according to `kind` by the single input rule, [`ValueBlock::from_texts`]
/// (#258) — dates of `DateTime` columns,
/// `inf` / `nan` kept as text, empty cell = missing value. A text that
/// does not fit in a `Numeric`/`DateTime` column returns a `Mixed` block
/// with `true`: the caller then writes with `Command::SetCellsPromoting` and
/// not `Command::SetCells`, exactly like a tolerant keyboard input.
fn value_block_from_tsv_column(
    kind: ColumnType,
    rows: &[Vec<String>],
    col_index: usize,
) -> (ValueBlock, bool) {
    let texts: Vec<&str> = rows
        .iter()
        .map(|row| row.get(col_index).map(String::as_str).unwrap_or(""))
        .collect();
    ValueBlock::from_texts(kind, &texts)
}

/// Block of a data-row paste (#311) for column `col_index`:
/// an **absent** cell (row shorter than the target column) keeps the
/// value the column already held, a present and empty cell stays a missing
/// value. The block stops at the last present cell: a
/// column is never lengthened by absent cells. `None` when no
/// cell is present. The kept cells are read by [`Column::cell`] (a
/// `MixedCell`, never converted back to text); the block itself comes from
/// [`value_block_from_tsv_column`], so `promoting` is that of the input.
fn value_block_keeping_absent_cells(
    column: &Column,
    start: usize,
    rows: &[Vec<String>],
    col_index: usize,
) -> Option<(ValueBlock, bool)> {
    let last = rows.iter().rposition(|row| row.get(col_index).is_some())?;
    let rows = &rows[..=last];
    let (block, promoting) = value_block_from_tsv_column(column.column_type(), rows, col_index);
    let kept = |i: usize| rows[i].get(col_index).is_none();
    if !(0..rows.len()).any(kept) {
        return Some((block, promoting));
    }
    let block = match block {
        ValueBlock::Float(mut values) => {
            for i in (0..values.len()).filter(|&i| kept(i)) {
                values[i] = match column.cell(start + i) {
                    MixedCell::Number(x) => x,
                    _ => f64::NAN,
                };
            }
            ValueBlock::Float(values)
        }
        ValueBlock::Mixed(mut cells) => {
            for i in (0..cells.len()).filter(|&i| kept(i)) {
                cells[i] = column.cell(start + i);
            }
            ValueBlock::Mixed(cells)
        }
    };
    Some((block, promoting))
}

/// True if `column` (assumed `Mixed`) would hold only numbers and
/// empty cells — at least one number — once `write` is written (`None`: as
/// it is). A text cell, even `""`, prevents it. Serves the
/// repromotion to `Numeric` ([`AppState::auto_promote_mixed_column_if_all_numeric`])
/// and to know, **before** writing a paste, whether there will be one.
fn column_all_numeric_after(column: &Column, write: Option<(usize, &ValueBlock)>) -> bool {
    let rows = column
        .len()
        .max(write.map_or(0, |(start, block)| start + block.len()));
    let mut any_present = false;
    for row in 0..rows {
        let cell = match write {
            Some((start, block)) if (start..start + block.len()).contains(&row) => match block {
                ValueBlock::Float(values) => MixedCell::from_f64(values[row - start]),
                ValueBlock::Mixed(cells) => cells[row - start].clone(),
            },
            _ => column.cell(row),
        };
        match cell {
            MixedCell::Empty => {}
            MixedCell::Number(_) => any_present = true,
            MixedCell::Text(_) => return false,
        }
    }
    any_present
}

/// Body of [`AppState::cell_text`], factored over a `&Tree` rather than over
/// `&self`: this is what lets a detached window
/// (`stokesforge_app::app::StokesForgeApp::show_detached_tabs`, which carries only a clone
/// of [`Snapshot`], never `AppState` itself — see its top-level doc) paint
/// a grid identical to that of the dock, without duplicating the display rule
/// (`MISSING_TEXT_DISPLAY`, scientific toggle A9/C4 — see
/// [`AppState::cell_text`] for the detail of these choices).
pub fn cell_text_in(tree: &Tree, column: NodeId, row: usize) -> String {
    cell_in(tree, column, row, format_exact, MISSING_TEXT_DISPLAY)
}

/// What copy writes for a cell (#259): like [`cell_text_in`], but
/// a **missing** value gives the empty string, never
/// [`MISSING_TEXT_DISPLAY`], which only makes sense on screen. The missing cell
/// is handled here, before the conversion to text: removing the non-breaking space
/// afterwards would also erase those a user really typed in a text.
pub(crate) fn cell_copy_text_in(tree: &Tree, column: NodeId, row: usize) -> String {
    cell_in(tree, column, row, format_exact, "")
}

/// What the grid **paints** (#222): like [`cell_text_in`], but a number
/// has only 6 significant digits. Editing, copy and input comparison
/// always read [`cell_text_in`], the whole value.
pub(crate) fn cell_display_in(tree: &Tree, column: NodeId, row: usize) -> String {
    cell_in(tree, column, row, format_grid, MISSING_TEXT_DISPLAY)
}

fn cell_in(
    tree: &Tree,
    column: NodeId,
    row: usize,
    number: fn(f64) -> String,
    missing_text: &str,
) -> String {
    let Some(column) = tree.node(column).and_then(Node::as_column) else {
        return String::new();
    };
    // A **mixed** column is read by `Column::cell`, not by `Column::get`:
    // only this path tells an empty cell (`Empty`) from a text cell
    // without a value — which `get` confuses, both as `Cell::Text(None)`.
    // `Empty` is the missing value of `Mixed`, whether it takes the place
    // of a `NaN` or of a `None`: there is no homogeneous `Text` type left
    // to decide between "numeric missing" (`""`) and "text
    // missing" (`MISSING_TEXT_DISPLAY`). A9 decides for the non-breaking space —
    // the same treatment as `Cell::Text(None)` below — because the
    // distinction that matters on screen is between "missing value" and
    // "present empty string" ([`MixedCell::Text(String::new())`]), not
    // between the possible origins of a missing cell.
    if column.column_type() == ColumnType::Mixed {
        return match column.cell(row) {
            MixedCell::Empty => missing_text.to_string(),
            MixedCell::Number(value) if value.is_nan() => missing_text.to_string(),
            MixedCell::Number(value) => number(value),
            MixedCell::Text(text) => text,
        };
    }
    match column.get(row) {
        Cell::Float(value) if value.is_nan() => String::new(),
        // Date/Time (#218): ISO 8601, the same form on screen, in editing and
        // in copy; outside year 0–9999, the raw number rather than a wrong date.
        Cell::Float(value) if column.column_type() == ColumnType::DateTime => {
            stokesforge_core::datetime::format_datetime(value).unwrap_or_else(|| number(value))
        }
        Cell::Float(value) => number(value),
        Cell::Text(None) => missing_text.to_string(),
        Cell::Text(Some(text)) => text,
    }
}

/// True if the cell `(column, row)` holds a **numeric** value — serves
/// to align the grid body like a spreadsheet (numbers right, text
/// left, see `app::grid::cell_widget`). For a `Mixed` column, the
/// type is read per cell (`Column::cell`), not per column: two neighboring
/// rows can align differently. `false` for an empty cell
/// (missing numeric or missing text): an empty cell has no sense being
/// pushed right rather than left, and leaving it left avoids an entirely
/// empty column looking random from one row to the next.
pub(crate) fn cell_is_numeric_in(tree: &Tree, column: NodeId, row: usize) -> bool {
    let Some(column) = tree.node(column).and_then(Node::as_column) else {
        return false;
    };
    if column.column_type() == ColumnType::Mixed {
        return matches!(column.cell(row), MixedCell::Number(value) if !value.is_nan());
    }
    matches!(column.get(row), Cell::Float(value) if !value.is_nan())
}

/// Body of [`AppState::visible_columns`], factored over a `&Tree` — see
/// [`cell_text_in`].
pub fn visible_columns_in(tree: &Tree, sheet: NodeId) -> Vec<(NodeId, String, ColumnType)> {
    let Some(node) = tree.node(sheet) else {
        return Vec::new();
    };
    node.children()
        .iter()
        .filter_map(|id| {
            let child = tree.node(*id)?;
            let column = child.as_column()?;
            Some((*id, child.name().to_string(), column.column_type()))
        })
        .collect()
}

/// The name of a "row" series (#166): "Row n", with the displayed number
/// (1 = the first row). The sentence is interface text; `compose` receives it.
fn row_series_name(row: usize) -> String {
    crate::tr!("graph-row-series", number = (row + 1))
}

/// Displayed name of a series in the list of the properties window: the
/// same rule as the graph legend, [`stokesforge_core::series_label`] (single
/// source, issue #25).
pub fn series_list_label(tree: &Tree, series: &stokesforge_plot::SeriesSpec) -> String {
    crate::figure_texts::series_label_text(&stokesforge_core::series_label(series, tree))
}

/// Payload of the drag of **whole rows** of a sheet to an open
/// graph (#166): offered only when the sheet carries an x' row
/// ([`AppState::row_drag_payload`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowDragPayload {
    /// Sheet the rows come from.
    pub sheet: NodeId,
    /// Dragged rows, ascending (0 = the first).
    pub rows: Vec<usize>,
}

/// The [`Action`] produced by dropping `payload` on layer `panel` of
/// graph `graph`: [`RowAxis::Ask`], the axis being decided by the state (question
/// asked only if the graph already has an X).
pub fn drop_rows_action(payload: &RowDragPayload, graph: NodeId, panel: usize) -> Action {
    Action::AddRowsToGraph {
        graph,
        panel,
        sheet: payload.sheet,
        rows: payload.rows.clone(),
        axis: RowAxis::Ask,
    }
}

/// X axis of the series of rows dropped on a graph (#166).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAxis {
    /// Let the state decide: the graph already has an X → dialog (new default
    /// axis, present axis, cancel); otherwise the main X axis, without a
    /// question.
    Ask,
    /// A new overlaid layer, with its own X axis (at the top).
    New,
    /// The X axis of the target layer.
    Present,
}

/// The gesture that would replace or close an unsaved project (S-F3,
/// #229): it waits for the answer "Discard Changes · Cancel · Save" (S-F4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsavedGesture {
    /// File › New Project.
    NewProject,
    /// File › Open…: the chosen project.
    Open(String),
    /// Quit, or closing the window.
    Quit,
    /// Recovery (#317): the backup copy to open.
    OpenRecovery(PathBuf),
}

/// The answer to the "Discard Changes · Cancel · Save" dialog (S-F4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChoice {
    /// Give up the changes and carry on with the gesture.
    Discard,
    /// Do nothing.
    Cancel,
    /// Save (asking where if the project has no file), then
    /// carry on with the gesture if the save succeeded.
    Save,
}

/// The source file of a load that discarded data (#309, F3).
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscardedSource {
    path: PathBuf,
    calculations: usize,
    corrections: usize,
}

impl DiscardedSource {
    /// `None` for a clean report.
    fn of(path: PathBuf, report: &stokesforge_session::LoadReport) -> Option<Self> {
        (!report.is_clean()).then_some(Self {
            path,
            calculations: report.discarded_calculations.len(),
            corrections: report.discarded_corrections.len(),
        })
    }
}

/// A save suspended on the answer to the overwrite confirmation
/// of a file whose read discarded data (#309, F3).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingDiscardedSave {
    /// The path to restore if the answer is Cancel (Save As to the source).
    restore_path: Option<String>,
}

/// A serious failure outside the window (S-M8, #229): reported by a blocking dialog,
/// with a possible action, never by the status bar alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// What failed; decides the title and the proposed action.
    pub kind: AlertKind,
    /// The error message, already put into a sentence.
    pub message: String,
}

/// The serious failures (S-M8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// Saving the project failed: Save As… is offered.
    SaveFailed,
    /// The chosen project could not be read: Open… is offered.
    OpenFailed,
    /// The R² of a calculation is very low (#288): a plain warning, which
    /// Close alone dismisses. The message names the calculations at fault.
    LowRSquared,
    /// The session actor is dead (#310): the model is lost. Close closes
    /// the application; the message says where the backup copy was written.
    SessionLost,
}

/// Row drop waiting for the answer to the axis dialog (#166).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRowDrop {
    graph: NodeId,
    panel: usize,
    sheet: NodeId,
    rows: Vec<usize>,
}

/// Payload of the drag and drop of a selection of workbook columns
/// to an open graph — work package "drag a selection to a graph"
/// (see [`Action::AddSeriesToGraph`]). Typed rather than a string: this is what
/// `egui::Response::dnd_set_drag_payload`/`dnd_hover_payload`/
/// `dnd_release_payload` require (`Any + Send + Sync`), and what [`drop_payload_action`]
/// rereads at the drop to build the action. `Clone`: `stokesforge_app::app` clones it
/// once per grid edge tested (left then right) before knowing
/// which one actually started the drag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDragPayload {
    /// Sheet the columns come from.
    pub sheet: NodeId,
    /// Dragged columns, in the order of [`AppState::column_drag_payload`].
    pub columns: Vec<NodeId>,
}

/// The [`Action`] produced by dropping `payload` on layer `panel` of
/// graph `graph` — always [`Action::AddSeriesToGraph`], with no imposed
/// plot type: the series take that of the layer (#160, see
/// [`panel_plot_type`]). A pure function, separate from the painting code of
/// `stokesforge_app::app::graph_window::graph_drop_target` so it is testable without a
/// window.
pub fn drop_payload_action(payload: &ColumnDragPayload, graph: NodeId, panel: usize) -> Action {
    Action::AddSeriesToGraph {
        graph,
        panel,
        columns: payload.columns.clone(),
        plot_type: None,
    }
}

/// Body of [`AppState::row_count`], factored over a `&Tree` — see
/// [`cell_text_in`].
pub fn row_count_in(tree: &Tree, sheet: NodeId) -> usize {
    tree.row_count(sheet).unwrap_or(0)
}

/// Body of [`AppState::worksheets_of`], factored over a `&Tree` — see
/// [`cell_text_in`].
pub fn worksheets_of_in(tree: &Tree, book: NodeId) -> Vec<(NodeId, String)> {
    let Some(node) = tree.node(book) else {
        return Vec::new();
    };
    node.children()
        .iter()
        .filter_map(|id| tree.node(*id).map(|sheet| (*id, sheet.name().to_string())))
        .collect()
}

/// Body of [`AppState::note_text`], factored over a `&Tree` — see
/// [`cell_text_in`].
pub fn note_text_in(tree: &Tree, note: NodeId) -> Option<String> {
    match tree.node(note)?.kind() {
        NodeKind::Note { text, .. } => Some(text.clone()),
        _ => None,
    }
}

/// Name of a node, or `"{missing} {id} (deleted)"` if it no longer exists in
/// `tree` — counterpart of `stokesforge_app::app::tab::node_title`, factored over a `&Tree`
/// for the same reason as [`cell_text_in`].
pub fn node_title_in(tree: &Tree, id: NodeId, missing: impl FnOnce() -> String) -> String {
    tree.node(id)
        .map(|node| node.name().to_string())
        .unwrap_or_else(|| crate::tr!("window-deleted", kind = missing(), id = id.to_string()))
}

/// Input in progress in a cell.
///
/// Only one cell is editable at a time: this is what lets the virtualized
/// grid keep a single buffer, not one per visible cell. The
/// buffer is sent to the model only at commit (focus loss or
/// Enter), never on each keystroke — otherwise history would count one entry
/// per character.
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit {
    /// Column being edited.
    pub column: NodeId,
    /// Row being edited.
    pub row: usize,
    /// Text typed so far.
    pub buffer: String,
    /// The editing opened on a keystroke (the buffer is what was just
    /// typed): the field then keeps its cursor instead of selecting everything on
    /// opening (S-T4, #224).
    pub typed: bool,
}

/// Field of [`stokesforge_core::ColumnMeta`] editable in the stacked header of a
/// column — see [`HeaderEdit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderField {
    /// Special x' row ([`stokesforge_core::ColumnMeta::x_prime`], #164):
    /// one numeric value per column, above the long name.
    XPrime,
    /// Long name ([`stokesforge_core::ColumnMeta::long_name`]).
    LongName,
    /// Unit ([`stokesforge_core::ColumnMeta::unit`]).
    Unit,
    /// Comment ([`stokesforge_core::ColumnMeta::comment`]).
    Comment,
    /// Formula ([`stokesforge_core::ColumnMeta::formula`]).
    Formula,
}

/// Value typed in the x' row: `Ok(None)` for an empty text, `Err`
/// when it is not a finite number (text, NaN, infinity). The error has no
/// content: the caller only has a message to set (`error-*`), hence the `allow`
/// that the `pub` visibility (#263) makes necessary.
#[allow(clippy::result_unit_err)]
pub fn parse_x_prime(text: &str) -> Result<Option<f64>, ()> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match MixedCell::from_text(Some(trimmed.to_string())) {
        MixedCell::Number(value) if value.is_finite() => Ok(Some(value)),
        _ => Err(()),
    }
}

/// Why a header field refuses a text ([`HeaderField::write_text`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HeaderTextRefused {
    /// x': it is not a finite number.
    NotANumber,
    /// Formula: unknown letter, self-reference or `#REF`.
    Formula(stokesforge_core::FormulaError),
}

impl HeaderField {
    /// Number of stacked fields. The rank of a field in the grid is
    /// `index() - COUNT` (`-COUNT..=-1`), data rows starting at 0.
    pub const COUNT: usize = 5;

    /// Position among the fields, in the order they are stacked on
    /// screen (top to bottom, 0 = `XPrime`) — serves to compare two
    /// fields to know whether one is "above" the other, in
    /// particular for the range of selected header fields (see
    /// [`crate::selection::Selection::is_header_field_selected`]).
    pub fn index(self) -> usize {
        match self {
            HeaderField::XPrime => 0,
            HeaderField::LongName => 1,
            HeaderField::Unit => 2,
            HeaderField::Comment => 3,
            HeaderField::Formula => 4,
        }
    }

    /// Inverse of [`HeaderField::index`] — an `index` beyond the last one gives
    /// `Formula`, never a panic: the callers ([`crate::selection::
    /// Selection::cleared_header_fields`]/[`AppState::copy_selection`]) only
    /// pass ranks already bounded to `-COUNT..=-1`, but a silent
    /// saturation is safer than an incomplete `match` if that changed.
    pub(crate) fn from_index(index: usize) -> Self {
        match index {
            0 => HeaderField::XPrime,
            1 => HeaderField::LongName,
            2 => HeaderField::Unit,
            3 => HeaderField::Comment,
            _ => HeaderField::Formula,
        }
    }

    /// Text of the field of `column` (whose metadata is `meta`), as it is
    /// displayed and copied. The formula is displayed in **current letters**
    /// (`Tree::formula_letters`, #321): they follow the columns, `#REF`
    /// for a read column that is no longer in the sheet.
    pub(crate) fn text_of(self, tree: &Tree, column: NodeId, meta: &ColumnMeta) -> String {
        match self {
            HeaderField::XPrime => meta.x_prime.map(format_exact).unwrap_or_default(),
            HeaderField::LongName => meta.long_name.clone(),
            HeaderField::Unit => meta.unit.clone(),
            HeaderField::Comment => meta.comment.clone(),
            HeaderField::Formula => tree.formula_letters(column).unwrap_or_default(),
        }
    }

    /// Writes `text` into the field of `column` (whose metadata is `meta`). Refusal
    /// (nothing is written) when the text is not a value of the field: x'
    /// refuses anything that is not a finite number, the formula refuses an
    /// unknown letter, the column itself or `#REF` — an empty text
    /// erases it. The letters-to-identifiers conversion is
    /// `Tree::formula_from_letters`, never a separate write.
    pub(crate) fn write_text(
        self,
        tree: &Tree,
        column: NodeId,
        meta: &mut ColumnMeta,
        text: String,
    ) -> Result<(), HeaderTextRefused> {
        match self {
            HeaderField::XPrime => match parse_x_prime(&text) {
                Ok(value) => {
                    meta.x_prime = value;
                    Ok(())
                }
                Err(()) => Err(HeaderTextRefused::NotANumber),
            },
            HeaderField::LongName => {
                meta.long_name = text;
                Ok(())
            }
            HeaderField::Unit => {
                meta.unit = text;
                Ok(())
            }
            HeaderField::Comment => {
                meta.comment = text;
                Ok(())
            }
            HeaderField::Formula => {
                meta.formula = if text.trim().is_empty() {
                    None
                } else {
                    Some(
                        tree.formula_from_letters(column, &text)
                            .map_err(HeaderTextRefused::Formula)?,
                    )
                };
                Ok(())
            }
        }
    }

    /// Field of rank `rank` (`-COUNT..=-1`) of the grid.
    pub fn from_rank(rank: isize) -> Self {
        Self::from_index((rank + Self::COUNT as isize) as usize)
    }

    /// Rank of the field in the grid (`-COUNT..=-1`).
    pub fn rank(self) -> isize {
        self.index() as isize - Self::COUNT as isize
    }
}

/// Grid area touched by the current selection — the header, the
/// data, or whole columns. The three are mutually
/// exclusive: a rectangle never crosses the header/data boundary
/// and a range of columns is a gesture of its own (see the top-level doc of
/// [`crate::selection`]).
///
/// Serves to contextualize the grid context menus
/// ([`AppState::grid_menu`]): it is the same list of gestures everywhere, but
/// named according to what is really selected — "Clear the field" on
/// a header field, "Clear the cells" on data, "Clear the
/// columns" on a range of columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridZone {
    /// Rectangle of header fields (Long Name, Unit, Comment,
    /// Formula).
    Header,
    /// Rectangle of data cells.
    Data,
    /// Range of whole columns (letters row).
    Columns,
    /// Range of whole rows (row number gutter).
    Rows,
}

/// What a grid context menu must offer for the current
/// selection — built by [`AppState::grid_menu`], rendered by
/// `app::grid::grid_edit_menu`.
///
/// This type carries **only the decision** (which gestures make sense here,
/// how to name them, which are greyed out) and never a widget: this is
/// what lets it be checked by the UI-free integration tests of
/// `tests/shell.rs`, like all the rest of the copy/paste matrix.
///
/// The listed gestures are exactly the keyboard shortcuts of the grid
/// (see the top-level doc of this module, copy/paste section, and the
/// keyboard block of `app::grid::grid_contents`): no menu offers here
/// an action that would have no keyboard equivalent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridMenu {
    /// Area touched by the selection.
    pub zone: GridZone,
    /// True if the selection covers more than one cell (several columns,
    /// several ranks, or both) — decides the plural of the labels.
    /// Exception for [`GridZone::Rows`]: its labels ("the row"/"the
    /// rows") speak of rows, not cells, so `plural` there counts
    /// only the number of rows covered — a single selected row on
    /// a sheet with several columns already touches several cells but
    /// stays "the row" in the singular.
    pub plural: bool,
    /// True if the selection touches more than one column. Distinct from
    /// [`GridMenu::plural`], which counts *cells*: three cells
    /// stacked in a single column are plural without being
    /// multi-column, and "Copy with Header" (which reasons in
    /// whole columns, never in cells) must stay singular.
    pub multi_column: bool,
    /// True if "Delete and Shift Up" (Delete, second press) makes sense
    /// here. False in the header: a field is not repeated per row, there is
    /// nothing to move up, and Delete just clears there — see
    /// [`AppState::delete_selected_cells`]. Offering it anyway would make
    /// two menu entries for one and the same effect.
    pub offers_delete_shift: bool,
    /// True if "Paste in Place" (Ctrl+Shift+V) has something to work with:
    /// it replays the **internal** clipboard, so it has no effect as long as
    /// nothing has been copied from the grid in this session (an external text
    /// has no original rank to replay).
    pub paste_absolute_enabled: bool,
    /// False if the selection is a non-contiguous set of columns (#159):
    /// the target of a paste would be ambiguous there, Paste and Paste in Place are
    /// greyed out.
    pub paste_enabled: bool,
    /// False if the multiple selection has no unambiguous copy (#159): blocks
    /// of cells that share neither their rows nor their columns.
    /// Copy and Cut are then greyed out.
    pub copy_enabled: bool,
    /// True if the Formula field of at least one selected column carries a
    /// formula: "Remove Formula, Keep Data" (#236) then makes sense.
    pub offers_keep_formula_data: bool,
}

impl GridMenu {
    /// Name of what is selected, agreed in number ("the cell",
    /// "the fields", "the rows"…).
    ///
    /// # Deliberately **not** used by the context menu
    ///
    /// The context menu entries say "Copy", not "Copy the
    /// cell": the right-click has just landed on what is
    /// selected, repeating it in each entry teaches nothing and lengthens
    /// the menu. This method (and [`GridMenu::copy_full_columns_label`])
    /// is kept for future UI surfaces where you do **not** see
    /// what you are aiming at — a menu bar, a tooltip, a status
    /// line, a confirmation — where naming the target becomes useful again. It
    /// stays locked by the tests of `tests/shell.rs` so that it
    /// follows the selection matrix in the meantime.
    pub fn target_label(&self) -> &'static str {
        match (self.zone, self.plural) {
            (GridZone::Header, false) => "the field",
            (GridZone::Header, true) => "the fields",
            (GridZone::Data, false) => "the cell",
            (GridZone::Data, true) => "the cells",
            (GridZone::Columns, false) => "the column",
            (GridZone::Columns, true) => "the columns",
            (GridZone::Rows, false) => "the row",
            (GridZone::Rows, true) => "the rows",
        }
    }

    /// Expanded label of "Copy with Header" (Ctrl+Shift+C), which
    /// always promotes the touched column(s) in full, whatever
    /// the shape of the selection — see [`AppState::copy_full_columns`].
    /// Like [`GridMenu::target_label`], kept for a future UI: the context
    /// menu simply says "Copy with Header".
    pub fn copy_full_columns_label(&self) -> &'static str {
        if self.multi_column {
            "Copy entire columns"
        } else {
            "Copy entire column"
        }
    }
}

/// What was copied in the grid since the last [`AppState::
/// copy_selection`]/[`AppState::copy_full_columns`], as
/// [`AppState::paste`]/[`AppState::paste_absolute`] need it — see the
/// behavior matrix at the top of this module (copy/paste section).
///
/// Replaces the old control marker (`HEADER_LINE_MARKER`, removed) that
/// polluted the system clipboard with a control character pasted as is
/// into an external spreadsheet: the **system** clipboard now receives
/// only a clean TSV text ([`grid_clipboard_to_text`]); this
/// structure lives only in [`AppState::grid_clipboard`] and is
/// replayed at paste only if the received text is identical to the one we
/// ourselves pushed to the clipboard ([`AppState::clipboard_text`]) — without
/// which an external copy in between (another program, or even a
/// Ctrl+C outside StokesForge) would silently replay a structure that no
/// longer matches what the clipboard actually holds.
#[derive(Debug, Clone, PartialEq)]
enum GridClipboard {
    /// A single copied cell (data cell or header field): its
    /// text only, nothing else — allows for example to copy the formula
    /// of a column into the Formula field of another.
    Text(String),
    /// Rectangle of several cells copied by Ctrl+C: pasted by
    /// shifting onto the anchor (see [`AppState::paste`]).
    Relative(RelativeCopy),
    /// Whole column(s) copied by Ctrl+Shift+C (header and body): pasted
    /// at its original ranks (see [`AppState::paste`]).
    FullColumns(Vec<FullColumnCopy>),
}

/// Relative rectangle copied by Ctrl+C ([`GridClipboard::Relative`]): either
/// header fields or data rows, never both — a
/// selection no longer crosses the header/data boundary (see the top-level doc of
/// [`crate::selection`]). `start` carries the original rank of the
/// first copied rank, so that [`AppState::paste_absolute`] can
/// replay each rank at its original rank rather than shifted onto the
/// anchor.
#[derive(Debug, Clone, PartialEq)]
enum RelativeCopy {
    HeaderFields {
        /// Original field of the first rank of `rows`.
        start: HeaderField,
        /// One rank per copied field, in the order they are stacked on
        /// screen; one value per touched column.
        rows: Vec<Vec<String>>,
    },
    DataRows {
        /// Original row of the first rank of `rows`.
        start: usize,
        /// One rank per copied row; one value per touched column.
        rows: Vec<Vec<String>>,
    },
}

/// A column copied in full by Ctrl+Shift+C — see
/// [`GridClipboard::FullColumns`].
#[derive(Debug, Clone, PartialEq)]
struct FullColumnCopy {
    x_prime: String,
    long_name: String,
    unit: String,
    comment: String,
    formula: Option<String>,
    /// Body of the column, row 0 first — the own length of
    /// this column, independent of the other columns touched by the same
    /// copy (a sheet allows columns of different lengths,
    /// see `demo_project` in `tests/shell.rs`).
    data: Vec<String>,
}

/// Clean text (TSV, no control character) for the **system**
/// clipboard — what an external spreadsheet pastes when it receives `clipboard`, in the
/// Excel/LibreOffice format of [`stokesforge_core::tsv`] (cell in
/// quotes if it carries a tab, a line break or a quote).
/// A [`GridClipboard::Text`] has no tabular form: a single cell.
fn grid_clipboard_to_text(clipboard: &GridClipboard) -> String {
    match clipboard {
        GridClipboard::Text(text) => tsv::encode_cell(text),
        GridClipboard::Relative(RelativeCopy::HeaderFields { rows, .. })
        | GridClipboard::Relative(RelativeCopy::DataRows { rows, .. }) => tsv::encode(rows),
        GridClipboard::FullColumns(columns) => {
            let mut rows: Vec<Vec<String>> = vec![
                columns.iter().map(|c| c.x_prime.clone()).collect(),
                columns.iter().map(|c| c.long_name.clone()).collect(),
                columns.iter().map(|c| c.unit.clone()).collect(),
                columns.iter().map(|c| c.comment.clone()).collect(),
                columns
                    .iter()
                    .map(|c| c.formula.clone().unwrap_or_default())
                    .collect(),
            ];
            let len = columns.iter().map(|c| c.data.len()).max().unwrap_or(0);
            for row in 0..len {
                rows.push(
                    columns
                        .iter()
                        .map(|c| c.data.get(row).cloned().unwrap_or_default())
                        .collect(),
                );
            }
            tsv::encode(&rows)
        }
    }
}

/// Rebuilds a [`GridClipboard`] from an external text (not our own:
/// see [`GridClipboard`]) — a third-party spreadsheet or text editor has
/// no notion of a header field, so never a
/// [`GridClipboard::FullColumns`] nor a [`RelativeCopy::HeaderFields`] by
/// this path. Without a tab or a newline, `text` becomes a
/// [`GridClipboard::Text`] (rule 1 of the matrix: written into the
/// anchored cell, whatever it is); otherwise a [`RelativeCopy::DataRows`] starting
/// at row 0 (rule 1 too: a tabbed or multi-line rectangle is
/// handled as a relative rectangle of body).
fn external_clipboard_from_text(text: &str) -> GridClipboard {
    let mut rows = tsv::decode(text);
    // A single cell (even quoted, even multiline): a text.
    if rows.len() <= 1 && rows.first().is_none_or(|row| row.len() <= 1) {
        let cell = rows.pop().and_then(|mut row| row.pop());
        return GridClipboard::Text(cell.unwrap_or_default());
    }
    GridClipboard::Relative(RelativeCopy::DataRows { start: 0, rows })
}

/// Column targeted by `cursor`, whatever its kind — [`GridCursor`]
/// does not expose this accessor outside [`crate::selection`]; its callers
/// in this module (the paste, see [`AppState::apply_paste`]) rebuild it
/// by this filtering rather than by an extra public method on a
/// type whose API otherwise stays minimal.
fn grid_cursor_column(cursor: GridCursor) -> NodeId {
    match cursor {
        GridCursor::Data(column, _) | GridCursor::Header(column, _) => column,
    }
}

/// Direction of a keyboard move of a cell in the grid (arrow
/// keys) — see [`AppState::move_selection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Input in progress in a header field (long name, unit, comment,
/// formula). The exact counterpart of [`CellEdit`] for the stacked header of a
/// column — a single editable field at a time, including across different
/// columns, sent to the model only at commit.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderEdit {
    /// Column being edited.
    pub column: NodeId,
    /// Field being edited.
    pub field: HeaderField,
    /// Text typed so far.
    pub buffer: String,
    /// See [`CellEdit::typed`].
    pub typed: bool,
}

/// Input in progress in a note. The exact counterpart of [`CellEdit`] for
/// `stokesforge_app::app::Tab::Note`: a single buffer at a time (see the doc of
/// [`AppState::note_editing`]), sent to the model only at commit.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteEdit {
    /// Note being edited.
    pub note: NodeId,
    /// Text typed so far.
    pub buffer: String,
}

/// Kind of active tab, as seen by [`AppState`] — a minimal mirror of the
/// private `Tab` enum of `stokesforge_app::app`, just enough for the
/// question "must the "Graph" menu be shown?"
/// ([`active_graph`]) to be asked without a window, in
/// `tests/shell.rs`. `stokesforge_app::app` translates its `Tab` into this one at the only
/// point of contact (`StokesForgeApp::ui`), nothing else reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveTabKind {
    /// No active tab (empty dock, or nothing focused this frame).
    #[default]
    None,
    /// A workbook.
    Book(NodeId),
    /// A graph.
    Graph(NodeId),
    /// A matrix.
    Matrix(NodeId),
    /// A note.
    Note(NodeId),
}

/// The selected graph, from the kind of the active tab — `None` except
/// for [`ActiveTabKind::Graph`]. Decides the presence of the "Graph" menu
/// in the menu bar. A pure function, separate from the painting code of
/// `stokesforge_app::app` so it is testable without a window.
pub fn active_graph(active: ActiveTabKind) -> Option<NodeId> {
    match active {
        ActiveTabKind::Graph(id) => Some(id),
        _ => None,
    }
}

/// Thematic tabs of the graph properties window, in their display
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotSettingsTab {
    /// The whole graph: title, background.
    Graph,
    /// The plot area: panel title, grids.
    PlotArea,
    /// An axis: title, scale, bounds, display.
    Axes,
    /// The plotted series: not a tab of the graph window but their
    /// own window (#179), opened by double-clicking a curve.
    Series,
    /// The legend.
    Legend,
    /// The global colors: background, text, axes, predefined themes.
    Theme,
}

impl PlotSettingsTab {
    /// The tabs of the graph window, in display order (the series
    /// have their own window, #179).
    pub const ALL: [PlotSettingsTab; 5] = [
        PlotSettingsTab::Graph,
        PlotSettingsTab::PlotArea,
        PlotSettingsTab::Axes,
        PlotSettingsTab::Legend,
        PlotSettingsTab::Theme,
    ];

    /// Label of the tab.
    pub fn label(self) -> String {
        match self {
            PlotSettingsTab::Graph => tr!("plot-tab-general"),
            PlotSettingsTab::PlotArea => tr!("plot-tab-plot-area"),
            PlotSettingsTab::Axes => tr!("plot-tab-axes"),
            PlotSettingsTab::Series => tr!("plot-tab-series"),
            PlotSettingsTab::Legend => tr!("plot-tab-legend"),
            PlotSettingsTab::Theme => tr!("plot-tab-theme"),
        }
    }
}

/// An editable axis of the (first) panel of a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotAxis {
    /// X axis, at the bottom.
    Bottom,
    /// Main Y axis, on the left.
    Left,
    /// Secondary Y axis, on the right.
    Right,
    /// Secondary X axis, at the top: the mirror axis of the bottom one (#176).
    Top,
}

impl PlotAxis {
    /// Label of the axis.
    pub fn label(self) -> String {
        match self {
            PlotAxis::Bottom => tr!("plot-axis-bottom"),
            PlotAxis::Left => tr!("plot-axis-left"),
            PlotAxis::Right => tr!("plot-axis-right"),
            PlotAxis::Top => tr!("plot-axis-top"),
        }
    }
}

/// Where to put a graphic object in the stacking order
/// ([`Action::ReorderShape`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeOrder {
    /// Above all the other objects of the panel.
    ToFront,
    /// Below all the other objects of the panel.
    ToBack,
}

/// A graph component targeted by a right-click — translated by
/// `stokesforge_app::app` from what the renderer designates under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotTarget {
    /// The figure background, outside any component.
    Background,
    /// The graph title.
    Title,
    /// The plot area.
    PlotArea,
    /// An axis (line and tick marks).
    Axis(PlotAxis),
    /// The title of an axis.
    AxisTitle(PlotAxis),
    /// A free text (issue #35).
    Annotation(AnnotationRef),
    /// The legend (frame or entry, issue #53).
    Legend,
    /// A graphic object: rectangle, ellipse, line or arrow (issue #76).
    Object(ShapeRef),
}

impl PlotTarget {
    /// Name of the component, at the top of the context menu.
    pub fn label(self) -> String {
        match self {
            PlotTarget::Background => tr!("plot-target-graph"),
            PlotTarget::Title => tr!("plot-target-title"),
            PlotTarget::PlotArea => tr!("plot-target-plot-area"),
            PlotTarget::Axis(axis) => tr!("plot-target-axis", axis = axis.label()),
            PlotTarget::AxisTitle(axis) => tr!("plot-target-axis-title", axis = axis.label()),
            PlotTarget::Annotation(_) => tr!("plot-target-text"),
            PlotTarget::Legend => tr!("plot-target-legend"),
            PlotTarget::Object(_) => tr!("plot-target-object"),
        }
    }

    /// The tab (and the axis) of the properties window that sets this
    /// component.
    pub fn settings_page(self) -> (PlotSettingsTab, Option<PlotAxis>) {
        match self {
            PlotTarget::Background | PlotTarget::Title => (PlotSettingsTab::Graph, None),
            PlotTarget::PlotArea | PlotTarget::Annotation(_) | PlotTarget::Object(_) => {
                (PlotSettingsTab::PlotArea, None)
            }
            PlotTarget::Legend => (PlotSettingsTab::Legend, None),
            PlotTarget::Axis(axis) | PlotTarget::AxisTitle(axis) => {
                (PlotSettingsTab::Axes, Some(axis))
            }
        }
    }

    /// The action that opens the properties of this component on `graph`.
    pub fn open_settings(self, graph: NodeId) -> Action {
        let (tab, axis) = self.settings_page();
        Action::OpenPlotSettings { graph, tab, axis }
    }
}

/// State of the graph properties window: which graph, which tab,
/// which axis in the "Axes" tab. Pure UI state; the edited values
/// live in the draft [`PlotEdit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlotSettings {
    /// Graph whose properties the window sets.
    pub graph: NodeId,
    /// Displayed tab.
    pub tab: PlotSettingsTab,
    /// Axis displayed in the "Axes" tab.
    pub axis: PlotAxis,
}

/// Edit draft of the graph properties window: a whole
/// [`PlotSpec`] held outside history until commit, on the
/// same principle as [`NoteEdit`]/[`HeaderEdit`] — except that here the edited object
/// is a whole graph rather than a single field. The reason: the panel
/// mixes a dozen small widgets (title, axes, style of each
/// series, legend) that must all see and modify the **same** draft
/// for "undoing a round trip in the panel" to make sense — one
/// buffer per field would be as many back-and-forth mechanisms with the
/// model to maintain separately.
///
/// [`AppState::commit_plot_edit`] sends [`Command::SetPlotSpec`] only if
/// `spec` actually differs from the current spec of the model: releasing a
/// slider that did not move, or losing the focus of an unmodified text field,
/// must not stack a useless history entry.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotEdit {
    /// Graph being edited.
    pub graph: NodeId,
    /// Description being typed, not yet committed.
    pub spec: PlotSpec,
    /// Series currently chosen in the list, for detailed editing —
    /// pure UI state, never sent to the model.
    pub selected_series: usize,
    /// Layer shown by the properties; independent of its series rank.
    pub selected_panel: usize,
    /// The spec of the model the draft started from (or just
    /// committed). As long as `spec` equals it, the draft has nothing in
    /// progress: if the model changes under it (Undo, Redo, script,
    /// drag and drop of a column), [`AppState::begin_plot_edit`] realigns
    /// it instead of showing stale values — which a following commit
    /// would write back over the undo.
    pub base: PlotSpec,
}

/// Destination of an import batch. Each file produces a sheet; new
/// workbooks are created only after a file is successfully analyzed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDestination {
    ActiveBook(NodeId),
    NewBookPerFile,
    NewBookForAll,
}

/// What a file drop asks to do — see [`file_drop_plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropPlan {
    /// `.stf` projects: each opens in a new StokesForge window.
    pub projects: Vec<PathBuf>,
    /// Data files to import.
    pub data: Vec<PathBuf>,
    /// Destination of the data files.
    pub destination: ImportDestination,
}

/// Pure decision of a drop: the drop location sets the destination (a
/// workbook under the cursor receives one sheet per file, anywhere
/// else a shared new workbook), the `.stf` extension makes a
/// file a project to open rather than data to import.
pub fn file_drop_plan(paths: &[PathBuf], target_book: Option<NodeId>) -> DropPlan {
    let is_project = |path: &PathBuf| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("stf"))
    };
    DropPlan {
        projects: paths.iter().filter(|p| is_project(p)).cloned().collect(),
        data: paths.iter().filter(|p| !is_project(p)).cloned().collect(),
        destination: target_book.map_or(
            ImportDestination::NewBookForAll,
            ImportDestination::ActiveBook,
        ),
    }
}

/// A file "reads well" when the recipe splits it into at least
/// one column and one row of which a value is a number. Otherwise the drop
/// opens the import window rather than creating an empty or
/// unreadable sheet.
fn reads_well(bytes: &[u8], recipe: &stokesforge_import::ImportRecipe) -> bool {
    stokesforge_import::parse(bytes, recipe).is_ok_and(|table| {
        table
            .columns
            .iter()
            .any(|column| column.values.iter().any(|value| !value.is_nan()))
    })
}

/// Last successful import: what is needed to reopen it in the wizard
/// to correct it ("Reopen Last Import…").
#[derive(Debug, Clone)]
struct LastImport {
    destination: ImportDestination,
    recipe: stokesforge_import::ImportRecipe,
    paths: Vec<PathBuf>,
}

/// Where a background import comes from: the wizard/menu (a recipe chosen
/// by the user) or a file drop (one template per file).
enum ImportSource {
    Menu(stokesforge_import::ImportRecipe),
    Drop {
        /// Template of the files that the automatic choice does not recognize.
        fallback: stokesforge_import::ImportRecipe,
        /// Workbook under the cursor at the time of the drop.
        target_book: Option<NodeId>,
    },
}

/// What the import thread returns: the items read, the files that cannot
/// be read (drop only) and the templates used.
struct ImportReply {
    items: Vec<ImportItem>,
    unreadable: Vec<PathBuf>,
    models: Vec<String>,
}

/// Import in progress on a background thread: reading and analyzing the files
/// (the slow part) do not block the interface; the sheets are created
/// by [`AppState::poll_import`] when everything is ready.
struct ImportJob {
    destination: ImportDestination,
    /// `Some(target workbook)` for a drop, `None` for the menu import.
    dropped: Option<Option<NodeId>>,
    total: usize,
    done: Arc<std::sync::atomic::AtomicUsize>,
    /// Set by Stop: the analysis thread stops between two files.
    stop: Arc<std::sync::atomic::AtomicBool>,
    started: std::time::Instant,
    reply: mpsc::Receiver<ImportReply>,
    /// Reply received but not applied: the graph properties are
    /// open and their history macro would absorb the import.
    ready: Option<ImportReply>,
}

/// Transient state of the import window (see `stokesforge_app::app` for its
/// rendering): never in history, never sent to the actor — same
/// family as [`HeaderEdit`]/[`CellEdit`]/[`NoteEdit`]. Carries everything the
/// window needs to remember between two frames, including the
/// [`stokesforge_import::store::RecipeStore`] loaded once at opening
/// (not reloaded each frame — `RecipeStore::load` touches the disk).
pub struct ImportState {
    /// Active workbook at opening, available even after a change of destination.
    pub active_book: Option<NodeId>,
    /// Chosen destination; the active workbook is offered if it exists.
    pub destination: ImportDestination,
    /// Recipe being edited in the window.
    pub recipe: stokesforge_import::ImportRecipe,
    /// Chosen files ("Choose Files to Import"), in the order of the
    /// picker.
    pub paths: Vec<std::path::PathBuf>,
    /// Saved templates, loaded once when the window opens.
    pub store: stokesforge_import::RecipeStore,
    /// Name under which to save the current recipe as a template (text
    /// field of the window, distinct from `recipe.name`: we do not want to
    /// overwrite the name of the loaded template until "Save Template"
    /// has been clicked).
    pub save_as_name: String,
    /// Current step of the wizard.
    pub step: ImportStep,
    /// File shown in the preview (index in `paths`).
    pub preview_index: usize,
    /// Template whose deletion awaits a confirmation.
    pub confirm_delete: Option<String>,
    /// Recently imported files, loaded at opening.
    pub recent: stokesforge_import::RecentFiles,
    /// Recipe as it was when the template was loaded: serves to say
    /// that it was modified without being saved.
    baseline: stokesforge_import::ImportRecipe,
    /// Preview held during typing (S-T1, #224): the recipe as it
    /// was before the typing and the instant of the last change; as long as it
    /// is there, previews are computed on this recipe, not on `recipe`.
    held_preview: Option<(stokesforge_import::ImportRecipe, f64)>,
    /// The recipe at the last frame, to recognize a change.
    seen_recipe: stokesforge_import::ImportRecipe,
    /// Bytes of the chosen files, read once per path (the file
    /// is no longer reread each frame).
    bytes: std::cell::RefCell<ImportBytesCache>,
}

/// Typing pause before a preview is recomputed (S-T1, #224), in seconds.
pub const PREVIEW_PAUSE_SECONDS: f64 = 0.3;

impl ImportState {
    /// The recipe on which previews are computed: the one from before the
    /// typing as long as the pause has not elapsed, otherwise the current recipe.
    pub fn preview_recipe(&self) -> &stokesforge_import::ImportRecipe {
        self.held_preview
            .as_ref()
            .map_or(&self.recipe, |(recipe, _)| recipe)
    }

    /// To call once per frame, **after** painting the fields. `typing`:
    /// a text field has the focus; `now`: the instant of the frame. During
    /// typing, a change of the recipe shows in the previews only after
    /// [`PREVIEW_PAUSE_SECONDS`] without another change; outside typing (list,
    /// box, button) it is immediate. Returns the time remaining before the
    /// update, to request as the next frame.
    pub fn debounce_preview(&mut self, typing: bool, now: f64) -> Option<f64> {
        if !typing {
            self.held_preview = None;
            self.seen_recipe = self.recipe.clone();
            return None;
        }
        if self.recipe != self.seen_recipe {
            let before = std::mem::replace(&mut self.seen_recipe, self.recipe.clone());
            let shown = self.held_preview.take().map_or(before, |(held, _)| held);
            self.held_preview = Some((shown, now));
        }
        let (_, changed_at) = self.held_preview.as_ref()?;
        let remaining = PREVIEW_PAUSE_SECONDS - (now - changed_at);
        if remaining <= 0.0 {
            self.held_preview = None;
            return None;
        }
        Some(remaining)
    }
}

/// Setting offered by the right-click on a line of the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportLineAction {
    /// "Start Data Here".
    DataStartsHere,
    /// "Use as Units Line".
    UnitsLine,
    /// "End Data Here" (the line is the first of the footer).
    DataEndsHere,
    /// "Disable Auto Data End".
    NoAutoCut,
}

/// Bytes read per path (or the read failure message).
type ImportBytesCache = std::collections::HashMap<PathBuf, Result<Arc<Vec<u8>>, ImportReadError>>;

/// Why the import wizard cannot show a file: data, without a
/// sentence — `errors::import::read_message` puts it into a sentence at display.
#[derive(Debug, Clone)]
pub enum ImportReadError {
    /// The wizard is not open.
    NoImport,
    /// The file `path` could not be read.
    Read {
        path: PathBuf,
        error: Arc<std::io::Error>,
    },
    /// The content of the file is not a readable table.
    Parse(stokesforge_import::ImportError),
}

/// The six steps of the import wizard, in the manner of Origin's ASCII wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportStep {
    /// File(s) and template.
    Files,
    /// Delimiter and decimal.
    Format,
    /// Header and end of the data.
    Header,
    /// Column-by-column designation.
    Columns,
    /// Metadata to keep.
    Metadata,
    /// Destination and saving of the template.
    Destination,
}

impl ImportStep {
    /// All the steps, in order.
    pub const ALL: [ImportStep; 6] = [
        ImportStep::Files,
        ImportStep::Format,
        ImportStep::Header,
        ImportStep::Columns,
        ImportStep::Metadata,
        ImportStep::Destination,
    ];

    /// Displayed title.
    pub fn title(self) -> String {
        match self {
            ImportStep::Files => tr!("import-step-files-title"),
            ImportStep::Format => tr!("import-step-format-title"),
            ImportStep::Header => tr!("import-step-header-title"),
            ImportStep::Columns => tr!("import-step-columns-title"),
            ImportStep::Metadata => tr!("common-metadata"),
            ImportStep::Destination => tr!("import-destination"),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap_or(0)
    }

    /// Next step (the last one stays itself).
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1).min(Self::ALL.len() - 1)]
    }

    /// Previous step (the first one stays itself).
    pub fn previous(self) -> Self {
        Self::ALL[self.index().saturating_sub(1)]
    }
}

/// A column of the import result, for the "Columns" step.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportColumnPreview {
    /// Name (column names line) or empty.
    pub name: String,
    /// Long name (subheader) or empty.
    pub long_name: String,
    /// Unit (subheader) or empty.
    pub unit: String,
    /// Mixed-type column (text kept).
    pub mixed: bool,
    /// First values, formatted.
    pub samples: Vec<String>,
}

/// Kind of window of the [`ProjectTreeNode`] shown by the Project
/// Manager panel — the subset of `NodeKind::is_window()` that has a window to
/// open on the UI side (`Unknown` has none: nothing to show for a node
/// kind that this reader does not understand, invariant 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// Workbook — `stokesforge_app::app` opens it as `Tab::Book`.
    Book,
    /// Graph — `Tab::Graph`.
    Graph,
    /// Matrix — `Tab::Matrix`.
    Matrix,
    /// Note — `Tab::Note`.
    Note,
}

/// A node as the Project Manager panel shows it: an expandable
/// folder, or a clickable leaf window. Built by
/// [`AppState::project_tree`], which **stops** at `Workbook`/`Graph`/
/// `Matrix`/`Note` — never descending into the sheets of a workbook nor
/// into the content of a graph, these kinds are leaves of that tree
/// even if they are not leaves of `Tree` (`Workbook` has `Worksheet`
/// children in the model, but none in this view).
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectTreeNode {
    /// Storage folder.
    Folder {
        /// Identifier of the folder.
        id: NodeId,
        /// Displayed name.
        name: String,
        /// Ordered children, folders and windows mixed — the order of the tree.
        children: Vec<ProjectTreeNode>,
    },
    /// Clickable window, never expanded further.
    Window {
        /// Identifier of the window.
        id: NodeId,
        /// Displayed name.
        name: String,
        /// Kind of the window.
        kind: WindowKind,
    },
}

/// The state of the test bench: a handle to the actor, the last snapshot
/// received, what is selected, the current file path, the last message
/// and the entry in progress.
pub struct AppState {
    /// Client handle to the actor that owns the `Project`.
    handle: SessionHandle,
    /// Last snapshot received: frozen, to be refreshed after any mutation.
    snapshot: Snapshot,
    /// Sheet shown in the grid of each **open** workbook —
    /// the `Workbook -> Worksheet` selected in its own tab
    /// `app::tab::Tab::Book`. A workbook with no entry here (or
    /// whose entry became invalid) falls back to its first sheet,
    /// see [`AppState::selected_sheet_in`] — so the map never needs
    /// to be pre-filled.
    selected_sheets: std::collections::HashMap<NodeId, NodeId>,
    /// Path of the working file (typed by hand, see `stokesforge_app::app`).
    path: String,
    /// Conservative mark of the last save, without pinning the buffers
    /// of an old tree. An undo also asks for confirmation.
    saved_revision: Option<(u64, u64)>,
    /// Autosave and recovery (#317, `state/autosave.rs`).
    autosave: autosave::AutosaveState,
    /// Gesture waiting for the answer "Discard Changes · Cancel · Save" (S-F4).
    unsaved_pending: Option<UnsavedGesture>,
    /// Gesture to continue once the project is saved (Save answer).
    after_save: Option<UnsavedGesture>,
    /// Nothing holds the application any more: the window can close.
    quit_ready: bool,
    /// Serious failure waiting to be read (S-M8).
    alert: Option<Alert>,
    /// Export that would overwrite a file the picker has not seen (S-F3).
    pending_overwrite: Option<(PathBuf, Box<PlotSpec>)>,
    /// The **source** file of the last load and what its read
    /// discarded, as long as that has not been saved (#309, F3): overwriting it
    /// would delete that data from disk, hence the confirmation. Set by
    /// [`AppState::mark_loaded`] and `load` (nothing if the file is clean);
    /// cleared by a successful save to this path, `new_project` or
    /// another load.
    discarded_source: Option<DiscardedSource>,
    /// The save is waiting for the answer to the confirmation above;
    /// distinct from a failure (no alert, `after_save` kept).
    pending_discarded_save: Option<PendingDiscardedSave>,
    /// Sheet whose tab is in name editing.
    renaming_sheet: Option<NodeId>,
    /// Row drop waiting for the choice of the X axis (#166).
    pending_row_drop: Option<PendingRowDrop>,
    object_clipboard_origin: String,
    object_clipboard_output: Option<String>,
    /// The representations of the figure to offer to the system, prepared by
    /// [`Action::CopyGraphFigure`] and picked up by the window (#149).
    figure_clipboard: Option<Vec<FigureBytes>>,
    copied_objects: Vec<(NodeId, NodeId, bool)>,
    /// Scale of PNG exports (issue #34), one of
    /// [`PNG_EXPORT_SCALES`]. Session preference, not part of the project.
    png_export_scale: f32,
    object_manager_open: bool,
    object_manager_preferences: Preferences,
    /// Last message meant for the user.
    status: Option<StatusMessage>,
    /// The last 50 status bar messages (S-M6).
    status_log: std::collections::VecDeque<StatusMessage>,
    /// The message history window is open.
    status_history_open: bool,
    /// The project's low R² values by type (#290), recomputed at each snapshot.
    low_r2_groups: Vec<crate::r_squared::LowRSquaredGroup>,
    /// The list of the "low R²" badge is open (#290): a separate
    /// state, which does not overwrite `alert`.
    low_r2_list_open: bool,
    keyboard_shortcuts_open: bool,
    /// The preferences window is open (#148).
    preferences_open: bool,
    /// Error raised while a modal dialog is open (S-M7, #228):
    /// it shows in the dialog footer, never in the status bar.
    dialog_error: Option<String>,
    /// Cell entry in progress, if any.
    editing: Option<CellEdit>,
    /// Note entry in progress, if any (see [`NoteEdit`]).
    note_editing: Option<NoteEdit>,
    /// Current draft of a graph's properties window, if there
    /// is one (see [`PlotEdit`]).
    plot_edit: Option<PlotEdit>,
    /// Graph properties window, if it is open (see
    /// [`PlotSettings`]).
    plot_settings: Option<PlotSettings>,
    /// A history macro is open for the properties window:
    /// its gestures apply live (preview) but form only one
    /// history entry, set on confirmation; cancelling rejects it.
    plot_macro_open: bool,
    /// Colors saved for the user, outside the project (see
    /// [`crate::user_colors`]).
    user_colors: UserColors,
    /// Current grid selection of each displayed sheet — pure UI
    /// state, never in the history (see the head doc of
    /// [`crate::selection`]). Missing key = no selection in this
    /// sheet.
    selection: std::collections::HashMap<NodeId, Selection>,
    /// Entry in progress in a column header field, if any.
    header_editing: Option<HeaderEdit>,
    /// Last structured grid clipboard, set by [`AppState::
    /// copy_selection`]/[`AppState::copy_full_columns`] — pure UI state, like
    /// [`AppState::selection`]: never in the history, never sent to
    /// the actor. `None` as long as nothing has been copied in this
    /// session (or after a paste that left it intact: copying never
    /// empties it, only a new `copy_selection`/`copy_full_columns`
    /// replaces it).
    grid_clipboard: Option<GridClipboard>,
    /// Exact text pushed to the **system** clipboard at the same time as
    /// [`AppState::grid_clipboard`] — lets [`AppState::paste`] decide
    /// whether the text received from egui is our own copy (replays the structure)
    /// or external text (see [`external_clipboard_from_text`]).
    clipboard_text: Option<String>,
    /// A local copy in the detached viewport must be transferred once to
    /// the root; without this flag, its stale copy would overwrite any more
    /// recent copy made in another window at every frame.
    clipboard_dirty: bool,
    /// Selection on which Delete has just **emptied** the cells (not yet
    /// deleted) — see [`AppState::consume_delete_key`]. `Some`
    /// between the two steps of a double press on Delete: a second
    /// press on *exactly* the same selection of the same sheet
    /// deletes (shifts what follows) rather than emptying again what is already
    /// empty. Reset as soon as the selection changes (it no longer compares
    /// equal to the remembered anchor) or explicitly by [`AppState::
    /// forget_delete_repeat`] before any other grid mutation on the
    /// same selection, so that a Delete after a Cut/Backspace/
    /// paste starts again at the first step rather than deleting by
    /// surprise.
    delete_repeat: Option<(NodeId, Selection)>,
    /// State of the import window, as long as it is open — see
    /// [`ImportState`].
    import_state: Option<ImportState>,
    /// Dropped `.stf` projects, each to open in a new StokesForge
    /// window: the state starts no process (testable without a window), the
    /// window layer empties them through [`AppState::take_projects_to_open`].
    projects_to_open: Vec<PathBuf>,
    /// Column whose file metadata window is open
    /// (read-only, transient: never in the history).
    metadata_view: Option<NodeId>,
    /// Form of the analysis window, if it is open (#181).
    analysis: Option<crate::analysis::AnalysisForm>,
    /// "Keep in Results": the last choice of the session (M5), reused at
    /// the next opening.
    analysis_keep_results: bool,
    /// For each calculation with a very low R², the fingerprint of that R² at the last
    /// snapshot: the box (#288) opens only when it changes.
    low_r2_seen: std::collections::BTreeMap<NodeId, Vec<(usize, u64)>>,
    /// The latest analyses run, the most recent first (#249).
    analysis_recent: Vec<crate::analysis_recent::RecentAnalysis>,
    /// Calculation whose deletion waits for confirmation (M8).
    pending_calculation_delete: Option<NodeId>,
    pending_analysis_replacement: bool,
    /// The `AnalyzeRows` group open for the current analysis window
    /// (#253): closed by Apply, aborted by Cancel.
    analysis_rows_macro: bool,
    /// Transposed sheet whose refresh waits for the confirmation
    /// to overwrite edited values (#253).
    pending_transpose_relink: Option<NodeId>,
    /// The export (or copy) of a graph that shows out-of-date series,
    /// waiting for the user's answer (#198).
    pending_stale_export: Option<Action>,
    /// "Column used by a calculation" window open after a
    /// deletion (#306).
    column_use: Option<column_use::ColumnUseNotice>,
    /// Sheet whose Ctrl+Shift+C waits for the answer "values or formulas"
    /// (#306): set only when a copied column carries a formula.
    pending_full_copy: Option<NodeId>,
    /// Import in progress on a background thread.
    import_job: Option<ImportJob>,
    /// Start of a formula recalculation the interface did not wait for (#239);
    /// `None` when none is running. As long as it is set, the snapshot is
    /// not refreshed (the actor would answer only at the end) and the status bar
    /// shows the indicator and Stop.
    recalc_since: Option<std::time::Instant>,
    /// Highest revision returned by the actor to a command from this
    /// interface. During a long recalculation the snapshot is not re-read
    /// (`recalc_since`): without this mark, [`AppState::has_unsaved_work`] would not
    /// see the command already applied and closing the window would lose
    /// the change without asking.
    applied_revision: u64,
    /// The session actor is dead (panic, #310): the model is lost, nothing
    /// can be saved or opened any more; one can only close.
    session_lost: bool,
    /// Close without asking "Discard Changes · Cancel · Save": the demo
    /// forced by `STOKESFORGE_DEMO=1` (development, captures) is disposable.
    /// Applies to Quit only; New and Open… always ask.
    quit_without_prompt: bool,
    /// Where to write the backup copy when the actor dies; `None`: none
    /// (tests). See [`crate::recovery`].
    recovery_dir: Option<PathBuf>,
    /// Last successful import, to resume it in the wizard.
    last_import: Option<LastImport>,
    /// Files imported successfully, to be added to the recent files by the
    /// window layer ([`AppState::take_imported_paths`]).
    recent_to_record: Vec<PathBuf>,
    /// Text of the script area.
    script_source: String,
    /// Output of the last script run, if there has been one.
    script_output: Option<ScriptOutput>,
    /// Descriptor of the embedded interpreter, set by [`AppState::
    /// set_python`]. `None` until it has been set — the interpreter
    /// itself is started neither here nor there, only at the first script.
    #[cfg(feature = "python")]
    python: Option<EmbeddedPython>,
    /// The script engine, once started (at the first successful [`Action::
    /// RunScript`]). Shared by `Arc` with the detached thread that
    /// runs the current script, if there is one.
    #[cfg(feature = "python")]
    engine: Option<Arc<ScriptEngine>>,
    /// Reply channel of a run in progress on a detached thread.
    /// `Some` until [`AppState::poll_script`] has received a reply.
    #[cfg(feature = "python")]
    pending_script: Option<mpsc::Receiver<PendingScriptResult>>,
}

/// What the detached run thread hands back to [`AppState::poll_script`]: the
/// engine (newly created or reused, so the caller can put it in
/// the cache) and the script result, already reduced to a message if the host
/// refused or failed — a [`stokesforge_script::ScriptError`] need not cross
/// this channel, `state.rs` does not need it beyond its message.
#[cfg(feature = "python")]
type PendingScriptResult = (
    Option<Arc<ScriptEngine>>,
    Result<ScriptOutcome, ScriptError>,
);

impl AppState {
    /// Builds the state from an actor handle. `Err` only if
    /// the actor does not answer — there is then nothing to display at all.
    ///
    /// Preselects no sheet: with one workbook per tab
    /// (`stokesforge_app::app::Tab::Book`), there is no longer "the" grid to prepare
    /// at opening — each workbook tab falls back to its first
    /// sheet by itself ([`AppState::selected_sheet_in`]) as soon as it
    /// opens.
    ///
    /// **Never** touches the Python interpreter: see the section "The
    /// Python script" at the top of this module.
    pub fn new(handle: SessionHandle, path: String) -> SessionResult<Self> {
        Self::new_with_preferences(handle, path, Preferences::load())
    }

    /// Storage injection, notably to test actions without writing
    /// the preferences of the user's machine.
    pub fn new_with_preferences(
        handle: SessionHandle,
        path: String,
        preferences: Preferences,
    ) -> SessionResult<Self> {
        let snapshot = handle.snapshot()?;
        let keep_results_default = preferences.analysis_keep_results();
        let analysis_recent = preferences.analysis_recent().to_vec();
        // Never deduced from `path`: a file that exists is not a loaded
        // project (#315). `main` sets the mark through [`AppState::mark_loaded`].
        let saved_revision = None;
        let autosave = autosave::AutosaveState::at((snapshot.revision, snapshot.geometry_revision));
        let mut state = Self {
            handle,
            snapshot,
            saved_revision,
            autosave,
            unsaved_pending: None,
            after_save: None,
            quit_ready: false,
            alert: None,
            pending_overwrite: None,
            discarded_source: None,
            pending_discarded_save: None,
            renaming_sheet: None,
            pending_row_drop: None,
            object_clipboard_origin: format!(
                "{}:{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            object_clipboard_output: None,
            figure_clipboard: None,
            copied_objects: Vec::new(),
            selected_sheets: std::collections::HashMap::new(),
            path,
            png_export_scale: DEFAULT_PNG_EXPORT_SCALE,
            object_manager_open: preferences.object_manager_open(),
            object_manager_preferences: preferences,
            status: None,
            status_log: std::collections::VecDeque::new(),
            status_history_open: false,
            low_r2_groups: Vec::new(),
            low_r2_list_open: false,
            keyboard_shortcuts_open: false,
            preferences_open: false,
            dialog_error: None,
            editing: None,
            note_editing: None,
            plot_edit: None,
            plot_settings: None,
            plot_macro_open: false,
            user_colors: UserColors::load(),
            selection: std::collections::HashMap::new(),
            header_editing: None,
            grid_clipboard: None,
            clipboard_text: None,
            clipboard_dirty: false,
            delete_repeat: None,
            import_state: None,
            projects_to_open: Vec::new(),
            metadata_view: None,
            analysis: None,
            analysis_keep_results: keep_results_default,
            low_r2_seen: std::collections::BTreeMap::new(),
            analysis_recent,
            pending_calculation_delete: None,
            pending_analysis_replacement: false,
            analysis_rows_macro: false,
            pending_transpose_relink: None,
            pending_stale_export: None,
            column_use: None,
            pending_full_copy: None,
            import_job: None,
            recalc_since: None,
            applied_revision: 0,
            session_lost: false,
            quit_without_prompt: false,
            recovery_dir: crate::recovery::default_dir(),
            last_import: None,
            recent_to_record: Vec::new(),
            script_source: String::new(),
            script_output: None,
            #[cfg(feature = "python")]
            python: None,
            #[cfg(feature = "python")]
            engine: None,
            #[cfg(feature = "python")]
            pending_script: None,
        };
        state.seed_low_r2();
        state.refresh_low_r2_groups();
        Ok(state)
    }

    /// Gives a detached viewport (workbook or note) its own transient
    /// state — grid, cell or note entry —, backed by the same
    /// actor. It can thus use exactly the same widgets as the
    /// floating window without locking the root's state.
    pub fn fork_for_detached_view(&self) -> SessionResult<Self> {
        // Sender identity specific to the detached view: with a plain `clone`,
        // its writes would be absorbed into a macro opened by the
        // root (graph properties) then carried away by its undo
        // (issue #21).
        let mut view = Self::new(self.handle.with_new_identity(), self.path.clone())?;
        view.snapshot = self.snapshot.clone();
        view.selected_sheets = self.selected_sheets.clone();
        view.renaming_sheet = self.renaming_sheet;
        view.selection = self.selection.clone();
        view.editing = self.editing.clone();
        view.header_editing = self.header_editing.clone();
        view.grid_clipboard = self.grid_clipboard.clone();
        view.clipboard_text = self.clipboard_text.clone();
        view.clipboard_dirty = false;
        Ok(view)
    }

    /// Refreshes a viewport's read without touching its selection or
    /// its entry buffers. The snapshot comes from the main state.
    pub fn sync_detached_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
        self.revalidate_selection();
    }

    /// Synchronizes the tab editor without overwriting the local selection.
    pub fn sync_detached_sheet_editor(&mut self, root: &Self) {
        self.renaming_sheet = root.renaming_sheet;
    }

    /// Joins the interaction of a detached workbook to the main command
    /// path before its deferred actions are dispatched.
    pub fn adopt_detached_book_view(&mut self, source: &mut Self, book: NodeId) {
        if let Some(sheet) = source.selected_sheet_in(book) {
            self.selected_sheets.insert(book, sheet);
        }
        for (sheet, _) in source.worksheets_of(book) {
            if let Some(selection) = source.selection.get(&sheet) {
                self.selection.insert(sheet, selection.clone());
            } else {
                self.selection.remove(&sheet);
            }
        }
        if source.clipboard_dirty {
            self.grid_clipboard = source.grid_clipboard.clone();
            self.clipboard_text = source.clipboard_text.clone();
            source.clipboard_dirty = false;
        }
        // The Ctrl+Shift+C question is asked in the main window (#306).
        if let Some(sheet) = source.pending_full_copy.take() {
            self.pending_full_copy = Some(sheet);
        }
    }

    /// Last snapshot received.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Tree of the last snapshot.
    pub fn tree(&self) -> &Arc<Tree> {
        &self.snapshot.tree
    }

    /// Sheet shown in the grid of workbook `book`: the last one
    /// selected by [`Action::SelectSheet`] for this workbook if it
    /// still exists and still belongs to it, otherwise its first sheet.
    /// `None` only if `book` has no sheet left (or no longer exists).
    ///
    /// Always recomputed, never cached: this is what makes the map
    /// [`AppState::selected_sheets`](Self) self-healing — a workbook
    /// never explicitly selected, or whose selection was made
    /// invalid by a deletion, falls back by itself to its first
    /// sheet at the next call, without going through [`AppState::refresh`].
    pub fn selected_sheet_in(&self, book: NodeId) -> Option<NodeId> {
        let tree = &self.snapshot.tree;
        if let Some(&sheet) = self.selected_sheets.get(&book) {
            if tree
                .node(sheet)
                .is_some_and(|node| node.kind().is_worksheet() && node.parent() == Some(book))
            {
                return Some(sheet);
            }
        }
        tree.node(book)?.children().iter().copied().find(|id| {
            tree.node(*id)
                .is_some_and(|node| node.kind().is_worksheet())
        })
    }

    /// Path of the working file.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Replaces the path of the working file.
    pub fn set_path(&mut self, path: String) {
        self.path = path;
    }

    /// Declares the current project **loaded from** [`AppState::path`]: it
    /// then has nothing to save. Called by the binary after a real
    /// load, never for a demo project nor because a file of the
    /// same name exists (#315). The `report` of this read is told
    /// like that of the File menu (#309): otherwise the first Save
    /// would silently delete what the read discarded.
    pub fn mark_loaded(&mut self, report: &stokesforge_session::LoadReport) {
        self.saved_revision = Some((self.snapshot.revision, self.snapshot.geometry_revision));
        self.autosave_mark_clean();
        self.discarded_source = DiscardedSource::of(PathBuf::from(&self.path), report);
        if let Some(text) = discarded_calculations_text(report) {
            self.warn(text);
        }
    }

    /// Records the geometry of content window `node` with the
    /// actor. This view setting does not go through the history: moving
    /// a window must not add a step to undo.
    ///
    /// Does not refresh the snapshot after each move: the actor
    /// already holds the geometry its next save will serialize, and
    /// a round trip per frame would make dragging needlessly costly.
    pub fn set_window_geometry(
        &mut self,
        node: NodeId,
        geometry: stokesforge_session::WindowGeometry,
    ) {
        if let Err(err) = self.handle.set_window_geometry(node, geometry) {
            self.fail(tr!(
                "error-window-geometry",
                cause = errors::session::error_message(&err)
            ));
        }
    }

    /// Records the width of column `column` with the actor (#325).
    /// Same status as [`AppState::set_window_geometry`]: a view setting
    /// that does not go through the history (nothing to undo) and that the
    /// save writes into the `.stf`, by `NodeId`.
    ///
    /// Does not refresh the snapshot — one more round trip for nothing: the
    /// local copy of the table is only kept up to date, so that
    /// [`AppState::column_width`] reads back what was just set before the
    /// next refresh (the `geometry_revision` counter, for its part, is
    /// the actor's, read at refresh: it is what then marks
    /// the project modified). A width that is not a strictly positive
    /// finite real is ignored by the actor.
    pub fn set_column_width(&mut self, column: NodeId, width: f32) {
        match self.handle.set_column_width(column, width) {
            Ok(_) => {
                if stokesforge_persistence::is_valid_column_width(width) {
                    Arc::make_mut(&mut self.snapshot.column_widths).insert(column, width);
                }
            }
            Err(err) => self.fail(tr!(
                "error-column-width",
                cause = errors::session::error_message(&err)
            )),
        }
    }

    /// Those of the columns `columns` that have a retained width (#330): what
    /// "reset to default width" can still change. Empty =
    /// the menu entry is greyed out.
    pub fn columns_with_width(&self, columns: &[NodeId]) -> Vec<NodeId> {
        columns
            .iter()
            .copied()
            .filter(|column| self.column_width(*column).is_some())
            .collect()
    }

    /// All the columns of `sheet` (#330), for "reset the whole sheet
    /// to default width".
    pub fn sheet_column_ids(&self, sheet: NodeId) -> Vec<NodeId> {
        self.visible_columns(sheet)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect()
    }

    /// Resets the columns `columns` to the default width with the actor
    /// (#330): their key is removed, here too in the local copy of
    /// the snapshot (like [`AppState::set_column_width`]), so that
    /// [`AppState::column_width`] returns `None` before the next refresh.
    pub fn reset_column_widths(&mut self, columns: &[NodeId]) {
        match self.handle.reset_column_widths(columns) {
            Ok(_) => {
                let widths = Arc::make_mut(&mut self.snapshot.column_widths);
                for column in columns {
                    widths.remove(column);
                }
            }
            Err(err) => self.fail(tr!(
                "error-column-width",
                cause = errors::session::error_message(&err)
            )),
        }
    }

    /// The retained width of column `column`, in logical points; `None`
    /// for a column never resized (the grid then applies its
    /// default width). Read from the snapshot: see
    /// [`AppState::set_column_width`].
    pub fn column_width(&self, column: NodeId) -> Option<f32> {
        self.snapshot.column_widths.get(&column).copied()
    }

    /// Last message meant for the user.
    pub fn status(&self) -> Option<&StatusMessage> {
        self.status.as_ref()
    }

    /// The error to show in the footer of the open modal dialog (S-M7, #228);
    /// none when no content dialog is open.
    pub fn dialog_error(&self) -> Option<&str> {
        if self.modal_dialog_open() {
            self.dialog_error.as_deref()
        } else {
            None
        }
    }

    /// Is a content modal dialog open (import, graph properties,
    /// analysis)? Its errors then go in its footer (S-M7).
    fn modal_dialog_open(&self) -> bool {
        self.import_state.is_some() || self.plot_settings.is_some() || self.analysis.is_some()
    }

    /// The remembered size and position of each kind of dialog (S-F8).
    pub fn dialog_geometries(
        &self,
    ) -> &std::collections::BTreeMap<String, crate::preferences::DialogGeometry> {
        self.object_manager_preferences.dialog_geometries()
    }

    /// Current text of the script area.
    pub fn script_source(&self) -> &str {
        &self.script_source
    }

    /// Replaces the text of the script area.
    pub fn set_script_source(&mut self, source: String) {
        self.script_source = source;
    }

    /// Output of the last script run, if there has been one.
    pub fn script_output(&self) -> Option<&ScriptOutput> {
        self.script_output.as_ref()
    }

    /// True if this binary embeds the Python bridge (feature `python`).
    ///
    /// Reflects only the **compilation**, not the location of an
    /// interpreter: a binary compiled with the feature but that did not
    /// find its embedded prefix (`AppState::set_python` never called)
    /// stays `true` here, and [`Action::RunScript`] explains it at run time —
    /// the same choice as for every other action of this bench (fail with a
    /// message rather than grey out in advance).
    #[must_use]
    pub fn script_available(&self) -> bool {
        cfg!(feature = "python")
    }

    /// True while a script is running (between [`Action::RunScript`] and the
    /// [`AppState::poll_script`] that collects its result).
    #[must_use]
    pub fn script_running(&self) -> bool {
        #[cfg(feature = "python")]
        {
            self.pending_script.is_some()
        }
        #[cfg(not(feature = "python"))]
        {
            false
        }
    }

    /// Sets the descriptor of the embedded interpreter to use at the first
    /// script. **Starts nothing**: `EmbeddedPython::locate`/`with_home` do not
    /// touch CPython, only `ScriptEngine::spawn` does — and it
    /// is called only from [`AppState::run_script`].
    #[cfg(feature = "python")]
    pub fn set_python(&mut self, python: EmbeddedPython) {
        self.python = Some(python);
    }

    /// Label of the "Undo" action: `Some("Undo Edit Cells in Column A")`
    /// (put into a sentence by [`history_text`]), or `None` if the stack is empty
    /// (the menu entry is then greyed out).
    pub fn undo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .undo_label
            .as_ref()
            .map(|label| tr!("history-undo-entry", label = history_text(label)))
    }

    /// Label of the "Redo" action, `None` if nothing to redo.
    pub fn redo_label(&self) -> Option<String> {
        self.snapshot
            .history
            .redo_label
            .as_ref()
            .map(|label| tr!("history-redo-entry", label = history_text(label)))
    }

    /// True if there is something to undo.
    pub fn can_undo(&self) -> bool {
        self.snapshot.history.can_undo
    }

    /// Current revision of the snapshot — used by `stokesforge_app::app` to detect,
    /// after an [`AppState::dispatch`], whether the action really produced a
    /// command (`Action::PlotColumns` on a selection without Y sends
    /// none, the revision does not move).
    pub fn revision(&self) -> stokesforge_session::Revision {
        self.snapshot.revision
    }

    /// True if there is something to redo.
    pub fn can_redo(&self) -> bool {
        self.snapshot.history.can_redo
    }

    /// Columns of `sheet`, in model order: `(NodeId, name, type)`.
    /// Empty if `sheet` is not (or no longer) a sheet of this project.
    pub fn visible_columns(&self, sheet: NodeId) -> Vec<(NodeId, String, ColumnType)> {
        visible_columns_in(&self.snapshot.tree, sheet)
    }

    /// The current [`PlotSpec`] of `graph`, or `None` if it is not (or
    /// no longer) a graph of this project.
    pub fn plot_spec(&self, graph: NodeId) -> Option<PlotSpec> {
        current_plot_spec(&self.snapshot.tree, graph)
    }

    /// Drag payload of a selection of columns of `sheet` to an
    /// open graph ([`ColumnDragPayload`]), or `None` if `sheet` has
    /// no selection or if its selection covers no column
    /// ([`Selection::columns`]) — no drag is possible without a column, even
    /// from the grab zone. `stokesforge_app::app::grid::grid_contents` calls it
    /// every frame to know whether there is anything at all to offer to the
    /// drag; having nothing selected disables the whole grab zone
    /// there rather than letting it start an empty drag.
    pub fn column_drag_payload(&self, sheet: NodeId) -> Option<ColumnDragPayload> {
        let selection = self.selection(sheet)?;
        // Only a selection of **whole** columns can be dragged (#160):
        // no cells or rows.
        if !selection.is_columns() {
            return None;
        }
        let columns = selection.columns(&self.snapshot.tree, sheet);
        if columns.is_empty() {
            None
        } else {
            Some(ColumnDragPayload { sheet, columns })
        }
    }

    /// Drag payload of rows of `sheet` to a graph
    /// ([`RowDragPayload`]), or `None` outside the two preconditions of #166:
    /// a selection of **whole rows** (row-number gutter), and the special
    /// x' row present — at least one column of the sheet
    /// carries a value in it. Without x', no gesture: a row has no abscissa.
    pub fn row_drag_payload(&self, sheet: NodeId) -> Option<RowDragPayload> {
        let selection = self.selection(sheet)?;
        if !selection.is_full_rows() || x_prime_columns(&self.snapshot.tree, sheet).is_empty() {
            return None;
        }
        let row_count = self.row_count(sheet);
        let rows: Vec<usize> = selection
            .full_row_indices()
            .into_iter()
            .filter(|row| *row < row_count)
            .collect();
        (!rows.is_empty()).then_some(RowDragPayload { sheet, rows })
    }

    /// The row drop waiting for the choice of the X axis (#166), with the
    /// number of rows concerned, or `None`.
    pub fn pending_row_drop(&self) -> Option<(NodeId, usize)> {
        self.pending_row_drop
            .as_ref()
            .map(|pending| (pending.graph, pending.rows.len()))
    }

    /// Number of rows of `sheet` — the **maximum** of the lengths of its
    /// columns, not the length of the first: a sheet is not a
    /// rectangular table. 0 if `sheet` is not (or no longer) a sheet.
    pub fn row_count(&self, sheet: NodeId) -> usize {
        row_count_in(&self.snapshot.tree, sheet)
    }

    /// Text to display in a cell: [`MISSING_TEXT_DISPLAY`] for an
    /// absent value (`Cell::Text(None)`, including out of bounds — `get`
    /// returns `None` past the end of a text column), `""` for a
    /// numeric missing value (`NaN`) or for an empty string that is
    /// **present** (`Cell::Text(Some(String::new()))`), otherwise the
    /// formatted value.
    ///
    /// # A9 (option 2, `guide-de-correction-robustesse.md` §F1/§F8) — two
    /// different things must not look the same
    ///
    /// The model distinguishes `Some("")` (an empty string that was written) from
    /// `None` (nothing was ever written) — the only missing value
    /// of text (see the head doc of `column.rs`, invariant F1). Before this
    /// fix, `cell_text` returned `""` in both cases: nothing, on
    /// screen, told which of the two was in front of
    /// the eyes — anomaly A9 (the display half; the other half, the survival of
    /// `Some("")` when crossing a cell, is already fixed in
    /// `commit_cell`, option 1).
    ///
    /// The convention chosen here: an **absent** value displays as a
    /// single non-breaking space U+00A0 ([`MISSING_TEXT_DISPLAY`]) — visible
    /// nowhere (it fills the cell like a blank), but distinct from the
    /// empty string in memory, and from a character a user never types
    /// when trying to write ordinary text (unlike a
    /// dash or a question mark, since "nothing" as well as "—" are
    /// genuine possible answers to a question). An empty string that is
    /// **present** keeps `""`: that is literally what it is.
    ///
    /// # Numeric formatting
    ///
    /// A number is written as its **exact value** (`format_exact`, from
    /// `stokesforge-core`, #222): it is the text for editing, copying and
    /// comparing entries, identical to the conversion
    /// `Column::set_column_type` (§F8). What the grid *paints* is
    /// [`AppState::cell_display`], rounded to 6 digits.
    pub fn cell_text(&self, column: NodeId, row: usize) -> String {
        cell_text_in(&self.snapshot.tree, column, row)
    }

    /// The text the grid paints for a cell (6 significant digits,
    /// #222). For everything else — editing, copying, comparing —
    /// [`AppState::cell_text`].
    pub fn cell_display(&self, column: NodeId, row: usize) -> String {
        cell_display_in(&self.snapshot.tree, column, row)
    }

    /// True if `(column, row)` carries a numeric value — see
    /// [`cell_is_numeric_in`].
    pub fn cell_is_numeric(&self, column: NodeId, row: usize) -> bool {
        cell_is_numeric_in(&self.snapshot.tree, column, row)
    }

    /// Display metadata of a column (role, unit, long name,
    /// comment, formula) — [`stokesforge_core::ColumnMeta::default`] if
    /// `column` is not (or no longer) a column of this project.
    pub fn column_meta(&self, column: NodeId) -> ColumnMeta {
        self.snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(|c| c.meta().clone())
            .unwrap_or_default()
    }

    /// Current grid selection of `sheet`, if there is one.
    pub fn selection(&self, sheet: NodeId) -> Option<&Selection> {
        self.selection.get(&sheet)
    }

    /// Sets a single-cell selection (the anchor of a future
    /// rectangle) — simple click. Transient mutation (like
    /// [`AppState::begin_edit`]): no effect on the model, so safe to
    /// call directly during rendering rather than through [`Action`].
    pub fn select_cell(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        self.selection.insert(sheet, Selection::cell(column, row));
    }

    /// Extends the current selection of `sheet` up to `(column, row)`,
    /// keeping the anchor already set — drag or Shift-click. Continues a
    /// rectangle already started in a header field, but stops at the
    /// boundary rather than crossing it (see [`Selection::
    /// extend_row_to`] and the head doc of [`crate::selection`]); sets a
    /// single-cell selection if `sheet` had none yet, or if
    /// the current selection is a range of columns (letter row),
    /// a range of rows (row-number gutter) or a range of
    /// whole header fields (field label in the gutter) — those
    /// drags remain distinct gestures, see [`AppState::
    /// extend_column_selection`]/[`AppState::extend_row_selection`]/
    /// [`AppState::extend_header_row_selection`].
    pub fn extend_selection(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection)
                if !selection.is_columns()
                    && !selection.is_full_rows()
                    && !selection.is_header_rows() =>
            {
                selection.extend_row_to(GridCursor::Data(column, row))
            }
            _ => self.select_cell(sheet, column, row),
        }
    }

    /// Selects `column` entirely — click on its header letter.
    pub fn select_column(&mut self, sheet: NodeId, column: NodeId) {
        self.selection.insert(sheet, Selection::column(column));
    }

    /// Ctrl+click on a letter: adds or removes `column` from the selection
    /// and moves the anchor there (#159). Sets a simple selection again if the
    /// current selection is not a set of columns — only one kind
    /// at a time.
    pub fn toggle_column_selection(&mut self, sheet: NodeId, column: NodeId) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_columns() => {
                selection.toggle_column(&self.snapshot.tree, sheet, column)
            }
            _ => self.select_column(sheet, column),
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Shift+click on a letter: the selection becomes the range anchor →
    /// `column` (the columns set by Ctrl+click disappear). Ctrl+Shift+click
    /// and the drag, which are additive, go through
    /// [`AppState::extend_column_selection`].
    pub fn extend_column_selection_replacing(&mut self, sheet: NodeId, column: NodeId) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_columns() => selection.extend_columns_replacing(column),
            _ => self.select_column(sheet, column),
        }
    }

    /// Extends the current column range of `sheet` up to `column`,
    /// keeping the anchor already set — drag or Shift-click on the letter
    /// row. Sets a single-column selection if `sheet` had none
    /// yet, or if the current selection is not already a range of
    /// columns — a drag that crosses the boundary from the data,
    /// the header fields or the row-number gutter to the letter
    /// row always sets a simple selection again, same
    /// policy as on the keyboard (see [`AppState::move_selection`]).
    pub fn extend_column_selection(&mut self, sheet: NodeId, column: NodeId) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_columns() => selection.extend_columns_to(column),
            _ => self.select_column(sheet, column),
        }
    }

    /// Removes from `sheet` a selection that the last gesture emptied (Ctrl+click
    /// that removes the last column, row or cell).
    fn drop_selection_if_empty(&mut self, sheet: NodeId) {
        let empty = self
            .selection
            .get(&sheet)
            .is_some_and(|selection| selection.columns(&self.snapshot.tree, sheet).is_empty());
        if empty {
            self.selection.remove(&sheet);
        }
    }

    /// Ctrl+click on a row number: adds or removes `row` (#159).
    pub fn toggle_row_selection(&mut self, sheet: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_full_rows() => selection.toggle_row(row),
            _ => self.select_row(sheet, row),
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Shift+click on a row number: the range anchor → `row` replaces
    /// everything; Ctrl+Shift+click and the drag go through
    /// [`AppState::extend_row_selection`].
    pub fn extend_row_selection_replacing(&mut self, sheet: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_full_rows() => {
                selection.extend_full_rows_replacing(row)
            }
            _ => self.select_row(sheet, row),
        }
    }

    /// Ctrl+click on a cell: adds or removes the cell (#159). Sets
    /// a simple selection again if the current selection is not a set of
    /// cells of the same zone.
    pub fn toggle_cell_selection(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        let target = GridCursor::Data(column, row);
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.toggle_cell(&self.snapshot.tree, sheet, target),
            None => false,
        };
        if !handled {
            self.select_cell(sheet, column, row);
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Ctrl+click on a header field: same rule as
    /// [`AppState::toggle_cell_selection`], header side.
    pub fn toggle_header_field_selection(
        &mut self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) {
        let target = GridCursor::Header(column, field);
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.toggle_cell(&self.snapshot.tree, sheet, target),
            None => false,
        };
        if !handled {
            self.select_header_field(sheet, column, field);
        }
        self.drop_selection_if_empty(sheet);
    }

    /// Ctrl+drag from a cell: opens a new rectangle while
    /// keeping the previous ones (#159).
    pub fn add_cell_selection(&mut self, sheet: NodeId, column: NodeId, row: usize) {
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.add_cell(GridCursor::Data(column, row)),
            None => false,
        };
        if !handled {
            self.select_cell(sheet, column, row);
        }
    }

    /// Ctrl+drag from a header field: counterpart of
    /// [`AppState::add_cell_selection`].
    pub fn add_header_field_selection(
        &mut self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) {
        let handled = match self.selection.get_mut(&sheet) {
            Some(selection) => selection.add_cell(GridCursor::Header(column, field)),
            None => false,
        };
        if !handled {
            self.select_header_field(sheet, column, field);
        }
    }

    /// Selects `row` entirely (all the visible columns of
    /// `sheet`) — click on its number in the gutter. Counterpart of
    /// [`AppState::select_column`].
    pub fn select_row(&mut self, sheet: NodeId, row: usize) {
        self.selection.insert(sheet, Selection::row(row));
    }

    /// Extends the current row range of `sheet` up to `row`,
    /// keeping the anchor already set — drag or Shift-click on the row-number
    /// gutter. Sets a single-row selection if `sheet`
    /// had none yet, or if the current selection is not already a
    /// range of rows — a drag that crosses the boundary from the
    /// data, the header fields or the letter row to the
    /// gutter always sets a simple selection again, counterpart of
    /// [`AppState::extend_column_selection`].
    pub fn extend_row_selection(&mut self, sheet: NodeId, row: usize) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_full_rows() => selection.extend_full_rows_to(row),
            _ => self.select_row(sheet, row),
        }
    }

    /// Selects a specific header field (long name/unit/comment/
    /// formula) — simple click, same behavior as [`AppState::
    /// select_cell`] for a data cell.
    pub fn select_header_field(&mut self, sheet: NodeId, column: NodeId, field: HeaderField) {
        self.selection
            .insert(sheet, Selection::header_field(column, field));
    }

    /// Extends the current range of `sheet` up to the field `(column, field)`,
    /// keeping the anchor already set — drag or Shift-click on one of the 4
    /// field rows. Continues a rectangle already started in the
    /// data, but stops at the boundary rather than crossing it
    /// (see [`Selection::extend_row_to`] and the head doc of [`crate::
    /// selection`]); sets a single-field selection if `sheet` had
    /// none yet, or if the current selection is a range of
    /// columns (letter row), a range of rows (row-number
    /// gutter) or a range of whole header fields (field label
    /// in the gutter) — those drags remain distinct
    /// gestures, see [`AppState::extend_column_selection`]/[`AppState::
    /// extend_row_selection`]/[`AppState::extend_header_row_selection`].
    pub fn extend_header_field_selection(
        &mut self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) {
        match self.selection.get_mut(&sheet) {
            Some(selection)
                if !selection.is_columns()
                    && !selection.is_full_rows()
                    && !selection.is_header_rows() =>
            {
                selection.extend_row_to(GridCursor::Header(column, field))
            }
            _ => self.select_header_field(sheet, column, field),
        }
    }

    /// Selects `field` entirely (all the visible columns of
    /// `sheet`) — click on its label in the gutter ("Long Name",
    /// "Units", "Comments", "F(x)="). Counterpart of [`AppState::
    /// select_row`], header side.
    pub fn select_header_row(&mut self, sheet: NodeId, field: HeaderField) {
        self.selection.insert(sheet, Selection::header_row(field));
    }

    /// Extends the current whole-header-field range of `sheet` up to
    /// `field`, keeping the anchor already set — drag or Shift-click on a
    /// field label in the gutter. Sets a single whole-field
    /// selection if `sheet` had none yet, or if the current selection
    /// is not already a range of this kind — a drag that crosses
    /// the boundary from the data, the header fields of a single
    /// column or the letter row to the gutter always sets
    /// a simple selection again, counterpart of [`AppState::extend_row_selection`].
    pub fn extend_header_row_selection(&mut self, sheet: NodeId, field: HeaderField) {
        match self.selection.get_mut(&sheet) {
            Some(selection) if selection.is_header_rows() => selection.extend_header_rows_to(field),
            _ => self.select_header_row(sheet, field),
        }
    }

    /// Clears the selection of `sheet` (no effect on the model).
    pub fn deselect_all(&mut self, sheet: NodeId) {
        self.selection.remove(&sheet);
    }

    /// Visible columns of `sheet`, in order — just the identifiers,
    /// for keyboard navigation ([`AppState::move_selection`]/
    /// [`AppState::move_selection_tab`]) as for [`AppState::paste`].
    fn column_order(&self, sheet: NodeId) -> Vec<NodeId> {
        self.visible_columns(sheet)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect()
    }

    /// Moves the selection of `sheet` by one cell in `direction` (arrow
    /// keys), keeping the anchor already set if `extend`
    /// (Shift+arrow extends the rectangle rather than moving the selected
    /// cell). Up/Down cross the boundary between the header (the
    /// 4 fields, see [`HeaderField`]) and the data: Up from the
    /// first data row reaches the Formula field (the lowest of the
    /// 4), Down from the Formula field reaches the first data
    /// row, Up from the Long Name field (the highest) does nothing —
    /// the letter row does not enter this navigation, it must be
    /// clicked to be selected (see [`Selection::grid_cursor`]).
    /// Crossing this boundary always sets a single-cell selection again,
    /// even under Shift: a range cannot mix data cells
    /// and header fields. Left/Right never change
    /// row. Clamped to the grid edges — never wraps to the next
    /// line (see [`AppState::move_selection_tab`] for that) nor adds a
    /// row (reserved to [`AppState::commit_cell_and_advance`], triggered by
    /// the confirmation of an entry, not by navigation alone). No effect
    /// if `sheet` has a range of columns selected (letter row)
    /// or no selection — transient mutation, like [`AppState::
    /// select_cell`].
    pub fn move_selection(&mut self, sheet: NodeId, direction: MoveDirection, extend: bool) {
        /// Row crossed by the arrow-key navigation: either one of the 4
        /// header fields, or a data row — see the doc of
        /// [`AppState::move_selection`].
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Row {
            Header(HeaderField),
            Data(usize),
        }
        impl Row {
            fn up(self) -> (Self, bool) {
                use HeaderField::{Comment, Formula, LongName, Unit, XPrime};
                match self {
                    Row::Data(0) => (Row::Header(Formula), true),
                    Row::Data(row) => (Row::Data(row - 1), false),
                    Row::Header(XPrime) => (self, false),
                    Row::Header(LongName) => (Row::Header(XPrime), false),
                    Row::Header(Unit) => (Row::Header(LongName), false),
                    Row::Header(Comment) => (Row::Header(Unit), false),
                    Row::Header(Formula) => (Row::Header(Comment), false),
                }
            }
            fn down(self, last_row: usize) -> (Self, bool) {
                use HeaderField::{Comment, Formula, LongName, Unit, XPrime};
                match self {
                    Row::Header(Formula) => (Row::Data(0), true),
                    Row::Header(XPrime) => (Row::Header(LongName), false),
                    Row::Header(LongName) => (Row::Header(Unit), false),
                    Row::Header(Unit) => (Row::Header(Comment), false),
                    Row::Header(Comment) => (Row::Header(Formula), false),
                    Row::Data(row) => (Row::Data((row + 1).min(last_row)), false),
                }
            }
        }

        let Some(from) = self.selection.get(&sheet).and_then(Selection::grid_cursor) else {
            return;
        };
        let (from_column, from_row) = match from {
            GridCursor::Data(column, row) => (column, Row::Data(row)),
            GridCursor::Header(column, field) => (column, Row::Header(field)),
        };
        let order = self.column_order(sheet);
        let Some(index) = order.iter().position(|&id| id == from_column) else {
            return;
        };
        let last_row = self.row_count(sheet).saturating_sub(1);
        let (to_index, to_row, crossed) = match direction {
            MoveDirection::Up => {
                let (row, crossed) = from_row.up();
                (index, row, crossed)
            }
            MoveDirection::Down => {
                let (row, crossed) = from_row.down(last_row);
                (index, row, crossed)
            }
            MoveDirection::Left => (index.saturating_sub(1), from_row, false),
            MoveDirection::Right => (
                (index + 1).min(order.len().saturating_sub(1)),
                from_row,
                false,
            ),
        };
        let Some(&to_column) = order.get(to_index) else {
            return;
        };
        // Crossing the header/data boundary always sets a selection reduced to a
        // single cell again, even under Shift — mixing data
        // cells and header fields in one range would make no
        // sense (see the head doc of this method).
        let extend = extend && !crossed;
        match to_row {
            Row::Data(row) => {
                if extend {
                    self.extend_selection(sheet, to_column, row);
                } else {
                    self.select_cell(sheet, to_column, row);
                }
            }
            Row::Header(field) => {
                if extend {
                    self.extend_header_field_selection(sheet, to_column, field);
                } else {
                    self.select_header_field(sheet, to_column, field);
                }
            }
        }
    }

    /// Moves the selection of `sheet` by one cell to the next column
    /// (`backward: false`) or the previous one (`true`), wrapping to the line
    /// at the edges — Tab/Shift+Tab, as in any spreadsheet: Tab at the end of a
    /// row restarts at the beginning of the next row, Shift+Tab at the start of a row
    /// goes back to the end of the previous one. Also crosses the boundary between
    /// the header (the 4 fields) and the data, unlike the arrow
    /// keys (see [`AppState::move_selection`]) which cross it
    /// but never come into play with Tab: Tab from the
    /// Formula field (the lowest of the 4 fields) of the **last** column enters
    /// the data at `Data(0)` of the **first** column; Shift+Tab
    /// from `Data(0)` of the first column does the reverse path and
    /// reaches Formula of the last column — the symmetric entry point
    /// into the header. Within the header or the data alone, Tab
    /// moves normally (next/previous column, next/previous field
    /// or next/previous row at the column edges),
    /// clamped with no other boundary crossing. Never extends the
    /// selection (Tab does not extend, unlike the arrows under Shift); the
    /// same guards as [`AppState::move_selection`] otherwise (no effect if
    /// `sheet` has a range of columns selected or no selection).
    pub fn move_selection_tab(&mut self, sheet: NodeId, backward: bool) {
        let Some(from) = self.selection.get(&sheet).and_then(Selection::grid_cursor) else {
            return;
        };
        let order = self.column_order(sheet);
        let row_count = self.row_count(sheet);
        if order.is_empty() || row_count == 0 {
            return;
        }
        let from_column = match from {
            GridCursor::Data(column, _) | GridCursor::Header(column, _) => column,
        };
        let Some(index) = order.iter().position(|&id| id == from_column) else {
            return;
        };
        let last_index = order.len() - 1;

        /// Next/previous field once all the columns have been traversed
        /// for the current field, `None` at the limits (Formula going forward,
        /// Long Name going back) — Tab first traverses the columns of a
        /// fixed field, see the doc of [`AppState::move_selection_tab`].
        fn step_field(field: HeaderField, backward: bool) -> Option<HeaderField> {
            use HeaderField::{Comment, Formula, LongName, Unit, XPrime};
            match (field, backward) {
                (XPrime, false) => Some(LongName),
                (LongName, true) => Some(XPrime),
                (XPrime, true) => None,
                (LongName, false) => Some(Unit),
                (Unit, false) => Some(Comment),
                (Comment, false) => Some(Formula),
                (Formula, false) => None,
                (Unit, true) => Some(LongName),
                (Comment, true) => Some(Unit),
                (Formula, true) => Some(Comment),
            }
        }

        let to = if backward {
            match from {
                GridCursor::Header(_, field) if index > 0 => {
                    GridCursor::Header(order[index - 1], field)
                }
                GridCursor::Header(_, field) => match step_field(field, true) {
                    Some(prev) => GridCursor::Header(order[last_index], prev),
                    None => GridCursor::Header(from_column, HeaderField::XPrime),
                },
                GridCursor::Data(_, 0) if index == 0 => {
                    GridCursor::Header(order[last_index], HeaderField::Formula)
                }
                GridCursor::Data(_, row) => {
                    if index > 0 {
                        GridCursor::Data(order[index - 1], row)
                    } else {
                        GridCursor::Data(order[last_index], row.saturating_sub(1))
                    }
                }
            }
        } else {
            match from {
                GridCursor::Header(_, field) if index < last_index => {
                    GridCursor::Header(order[index + 1], field)
                }
                GridCursor::Header(_, field) => match step_field(field, false) {
                    Some(next) => GridCursor::Header(order[0], next),
                    None => GridCursor::Data(order[0], 0),
                },
                GridCursor::Data(_, row) => {
                    if index < last_index {
                        GridCursor::Data(order[index + 1], row)
                    } else {
                        GridCursor::Data(order[0], (row + 1).min(row_count - 1))
                    }
                }
            }
        };
        match to {
            GridCursor::Data(column, row) => self.select_cell(sheet, column, row),
            GridCursor::Header(column, field) => self.select_header_field(sheet, column, field),
        }
    }

    /// True if `(column, row)` is part of the selection of `sheet`.
    pub fn is_selected(&self, sheet: NodeId, column: NodeId, row: usize) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.contains(&self.snapshot.tree, sheet, column, row))
    }

    /// True if `column` is selected entirely (click on its header
    /// letter).
    pub fn is_column_selected(&self, sheet: NodeId, column: NodeId) -> bool {
        self.selection.get(&sheet).is_some_and(|selection| {
            selection.is_column_selected(&self.snapshot.tree, sheet, column)
        })
    }

    /// True if `row` is selected entirely (click on its number
    /// in the gutter) — counterpart of [`AppState::is_column_selected`].
    pub fn is_row_selected(&self, sheet: NodeId, row: usize) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.is_row_selected(row))
    }

    /// True if `field` is selected entirely (click on its
    /// label in the gutter) — counterpart of [`AppState::is_row_selected`],
    /// header side.
    pub fn is_header_row_selected(&self, sheet: NodeId, field: HeaderField) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.is_header_row_selected(field))
    }

    /// True if `column` is touched by the current selection of `sheet`,
    /// whatever its kind (a single data cell of this
    /// column is enough) — see [`Selection::touches_column`]. Distinct from
    /// [`AppState::is_column_selected`], which holds only for a direct click
    /// on the letter row.
    pub fn column_touched_by_selection(&self, sheet: NodeId, column: NodeId) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.touches_column(&self.snapshot.tree, sheet, column))
    }

    /// True if `row` is touched by the current selection of `sheet` —
    /// counterpart of [`AppState::column_touched_by_selection`], row side.
    pub fn row_touched_by_selection(&self, sheet: NodeId, row: usize) -> bool {
        self.selection
            .get(&sheet)
            .is_some_and(|selection| selection.touches_row(row))
    }

    /// True if `sheet` is currently selected entirely: all
    /// its visible columns, through the corner cell or Ctrl+A (see
    /// [`AppState::select_all`]).
    pub fn is_all_selected(&self, sheet: NodeId) -> bool {
        let order = self.column_order(sheet);
        !order.is_empty()
            && self
                .selection
                .get(&sheet)
                .is_some_and(|selection| selection.columns(&self.snapshot.tree, sheet) == order)
            && self
                .selection
                .get(&sheet)
                .is_some_and(Selection::is_columns)
    }

    /// Selects **all** the visible columns of `sheet`, header
    /// included and even without rows (#159) — corner cell, Ctrl+A, Edit ›
    /// Select All. No effect on a sheet without columns.
    pub fn select_all(&mut self, sheet: NodeId) {
        let order = self.column_order(sheet);
        if let Some(selection) = Selection::all_columns(&order) {
            self.selection.insert(sheet, selection);
        }
    }

    /// True if `(column, field)` is part of the range of header fields
    /// currently selected in `sheet`.
    pub fn is_header_field_selected(
        &self,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) -> bool {
        self.selection.get(&sheet).is_some_and(|selection| {
            selection.is_header_field_selected(&self.snapshot.tree, sheet, column, field)
        })
    }

    /// Header field selected in `sheet`, if there is one — for the
    /// keyboard shortcuts (Ctrl+C/Delete) scoped to the grid.
    pub fn selected_header_field(&self, sheet: NodeId) -> Option<(NodeId, HeaderField)> {
        self.selection.get(&sheet)?.selected_header_field()
    }

    /// What the grid context menu must offer for the
    /// current selection of `sheet` — see [`GridMenu`]. `None` if `sheet`
    /// has no selection or if it no longer designates a valid column
    /// (column deleted in the meantime): there is then no menu to
    /// open, rather than a menu whose every entry would have no effect.
    ///
    /// A right click always sets a selection before arriving here (see
    /// `app::grid::cell_widget`/`app::grid::header_field_widget`/
    /// `app::grid::column_header_widget`, which reduce the selection to the
    /// targeted cell if the right click falls outside it): the `None` occurs
    /// in practice only on a sheet emptied under the cursor.
    pub fn grid_menu(&self, sheet: NodeId) -> Option<GridMenu> {
        let selection = self.selection.get(&sheet)?;
        let columns = selection.columns(&self.snapshot.tree, sheet);
        if columns.is_empty() {
            return None;
        }
        let multi_column = columns.len() > 1;
        let (zone, plural) = if selection.is_full_rows() {
            // A range of whole rows touches by construction all
            // the columns of the sheet: `multi_column` therefore has nothing to
            // do with the plural here, unlike the other zones (see
            // the doc of `GridMenu::plural`) — only the number of *rows*
            // covered decides between "the row" and "the rows".
            let rows_span = selection
                .row_range()
                .map(|(lo, hi)| hi.saturating_sub(lo))
                .unwrap_or(0);
            (GridZone::Rows, rows_span > 0)
        } else {
            // `full_rank_range` tells the two zones of a rectangle apart by
            // the sign of its ranks (negative = header, see
            // `crate::selection::GridCursor::rank`); `None` remains only
            // for a range of whole columns, which has no single
            // rank.
            let (zone, rank_span) = match selection.full_rank_range() {
                Some((lo, hi)) if hi < 0 => (GridZone::Header, hi - lo),
                Some((lo, hi)) => (GridZone::Data, hi - lo),
                None => (GridZone::Columns, 0),
            };
            (zone, multi_column || rank_span > 0)
        };
        Some(GridMenu {
            zone,
            plural,
            multi_column,
            offers_delete_shift: zone != GridZone::Header
                && (selection.is_columns()
                    || selection.blocks(&self.snapshot.tree, sheet).len() <= 1),
            copy_enabled: selection.is_columns()
                || selection.copy_layout(&self.snapshot.tree, sheet).is_some(),
            paste_absolute_enabled: self.grid_clipboard.is_some(),
            paste_enabled: selection.is_contiguous(&self.snapshot.tree, sheet),
            offers_keep_formula_data: !self.formula_columns_selected(sheet).is_empty(),
        })
    }

    /// Copies the current selection of `sheet` — Ctrl+C (and the first
    /// half of Ctrl+X, see `stokesforge_app::app`). `None` if `sheet` has no
    /// selection or if it no longer designates a valid column (nothing to
    /// push to the clipboard).
    ///
    /// Sets both [`AppState::grid_clipboard`] (replayed by
    /// [`AppState::paste`]/[`AppState::paste_absolute`]) and
    /// [`AppState::clipboard_text`] (to recognize this copy at
    /// paste time), and returns the text to push to the **system** clipboard —
    /// see the behavior matrix at the top of this module:
    ///
    /// - a single cell selected (cell or header field) →
    ///   [`GridClipboard::Text`], its text alone;
    /// - several cells, all in the header or all in the data
    ///   (never both, see the head doc of [`crate::selection`]) →
    ///   [`GridClipboard::Relative`];
    /// - a range of whole columns (letter row) →
    ///   [`GridClipboard::Relative`] too, over the whole body of each
    ///   column (no header: a range of columns is not a
    ///   rectangle of fields).
    ///
    /// A missing value is written as an empty string (never
    /// [`MISSING_TEXT_DISPLAY`], which makes sense only for the screen, not for
    /// an exchange with an external spreadsheet).
    pub fn copy_selection(&mut self, sheet: NodeId) -> Option<String> {
        let selection = self.selection.get(&sheet)?;
        let tree = &self.snapshot.tree;
        let columns = selection.columns(tree, sheet);
        if columns.is_empty() {
            return None;
        }
        let clipboard = if let Some((column, field)) = selection.selected_header_field() {
            GridClipboard::Text(self.header_field_display_text(column, field))
        } else if let Some((column, row)) = selection.single_cell() {
            GridClipboard::Text(cell_copy_text_in(tree, column, row))
        } else if !selection.is_columns() {
            // One or several blocks: end to end in the order of the sheet
            // (#159) when they share their rows or their columns,
            // otherwise nothing is copied (see `Selection::copy_layout`).
            let (columns, ranges) = selection.copy_layout(tree, sheet)?;
            let ranks: Vec<isize> = ranges.iter().flat_map(|&(lo, hi)| lo..=hi).collect();
            let first = *ranks.first()?;
            if first < 0 {
                let start = HeaderField::from_rank(first);
                let rows = ranks
                    .iter()
                    .map(|&rank| {
                        let field = HeaderField::from_rank(rank);
                        columns
                            .iter()
                            .map(|&column| self.header_field_display_text(column, field))
                            .collect()
                    })
                    .collect();
                GridClipboard::Relative(RelativeCopy::HeaderFields { start, rows })
            } else {
                let rows = ranks
                    .iter()
                    .map(|&rank| {
                        columns
                            .iter()
                            .map(|&column| cell_copy_text_in(tree, column, rank as usize))
                            .collect()
                    })
                    .collect();
                GridClipboard::Relative(RelativeCopy::DataRows {
                    start: first as usize,
                    rows,
                })
            }
        } else {
            // Range of whole columns (letter row):
            // `full_rank_range` makes no sense, there is no single rank
            // to cover — we copy the whole body of each touched column,
            // starting from its own length (columns of different
            // lengths are allowed).
            let len = columns
                .iter()
                .filter_map(|&column| tree.node(column).and_then(Node::as_column))
                .map(Column::len)
                .max()
                .unwrap_or(0);
            if len == 0 {
                return None;
            }
            let rows = (0..len)
                .map(|row| {
                    columns
                        .iter()
                        .map(|&column| cell_copy_text_in(tree, column, row))
                        .collect()
                })
                .collect();
            GridClipboard::Relative(RelativeCopy::DataRows { start: 0, rows })
        };
        let text = grid_clipboard_to_text(&clipboard);
        self.grid_clipboard = Some(clipboard);
        self.clipboard_text = Some(text.clone());
        self.clipboard_dirty = true;
        Some(text)
    }

    /// Copies entirely (header + body) the columns touched by the
    /// current selection of `sheet` — Ctrl+Shift+C, whatever the shape
    /// of the selection (a single cell, a rectangle, a range of
    /// columns): it is always the column(s) it touches that are
    /// promoted entirely. `None` if `sheet` has no selection or if it
    /// no longer designates a valid column. See the behavior matrix at the
    /// top of this module.
    ///
    /// When one of the columns carries a formula, nothing is copied yet: the
    /// question "values or formulas" is asked (#306,
    /// [`AppState::pending_full_copy`]) and the copy follows the answer
    /// ([`Action::ConfirmFullColumnsCopy`]). `None` in this case too.
    pub fn copy_full_columns(&mut self, sheet: NodeId) -> Option<String> {
        let selection = self.selection.get(&sheet)?;
        let tree = &self.snapshot.tree;
        let columns = selection.columns(tree, sheet);
        if columns.is_empty() {
            return None;
        }
        if columns
            .iter()
            .any(|&c| self.column_meta(c).formula.is_some())
        {
            self.pending_full_copy = Some(sheet);
            return None;
        }
        self.copy_full_columns_with(sheet, true)
    }

    /// The sheet whose Ctrl+Shift+C waits for the answer "values or formulas".
    pub fn pending_full_copy(&self) -> Option<NodeId> {
        self.pending_full_copy
    }

    /// Copies entirely the columns touched by the selection of `sheet`,
    /// with or without their formula (`keep_formula`); the header is always
    /// copied.
    fn copy_full_columns_with(&mut self, sheet: NodeId, keep_formula: bool) -> Option<String> {
        let selection = self.selection.get(&sheet)?;
        let tree = &self.snapshot.tree;
        let columns = selection.columns(tree, sheet);
        if columns.is_empty() {
            return None;
        }
        let full: Vec<FullColumnCopy> = columns
            .iter()
            .map(|&column| {
                let meta = self.column_meta(column);
                let len = tree
                    .node(column)
                    .and_then(Node::as_column)
                    .map(Column::len)
                    .unwrap_or(0);
                let data = (0..len)
                    .map(|row| cell_copy_text_in(tree, column, row))
                    .collect();
                FullColumnCopy {
                    x_prime: HeaderField::XPrime.text_of(tree, column, &meta),
                    long_name: meta.long_name,
                    unit: meta.unit,
                    comment: meta.comment,
                    // The clipboard carries **letters** (F8): read back
                    // in the destination sheet at paste time.
                    formula: tree.formula_letters(column).filter(|_| keep_formula),
                    data,
                }
            })
            .collect();
        let clipboard = GridClipboard::FullColumns(full);
        let text = grid_clipboard_to_text(&clipboard);
        self.grid_clipboard = Some(clipboard);
        self.clipboard_text = Some(text.clone());
        self.clipboard_dirty = true;
        Some(text)
    }

    /// Displayed text of a header field (outside editing) — same detail as
    /// `app::grid::header_field_widget`, factored out here to be shared
    /// with [`AppState::copy_selection`]/[`AppState::copy_full_columns`].
    pub fn header_field_display_text(&self, column: NodeId, field: HeaderField) -> String {
        field.text_of(&self.snapshot.tree, column, &self.column_meta(column))
    }

    /// Why the text being typed in the **Formula** field is
    /// refused: a letter that designates no column of the sheet, the column
    /// itself, or a modified `#REF` (#321). `None` for everything else:
    /// an empty text (clears), the displayed text as is (nothing is written, even
    /// with `#REF`) and a **syntax error**, which is saved (the column
    /// fills with `---`). This is the only place that decides; the grid
    /// keeps the field open and paints the refusal.
    pub fn header_edit_refusal(&self) -> Option<String> {
        let edit = self.header_editing.as_ref()?;
        if edit.field != HeaderField::Formula {
            return None;
        }
        if edit.buffer.trim().is_empty()
            || edit.buffer == self.header_field_display_text(edit.column, HeaderField::Formula)
        {
            return None;
        }
        self.snapshot
            .tree
            .formula_from_letters(edit.column, &edit.buffer)
            .err()
            .map(|error| errors::formula::error_message(&error))
    }

    /// The sentence saying why the formula of `column` no longer computes,
    /// when it reads a deleted column (`#REF`, #321): the grid
    /// paints it as a frame on the Formula cell.
    pub fn formula_issue(&self, column: NodeId) -> Option<String> {
        self.snapshot
            .tree
            .formula_has_deleted_reference(column)
            .then(|| errors::formula::error_message(&stokesforge_core::FormulaError::DeletedColumn))
    }

    /// Entry in progress in a column header field, if there is one.
    pub fn header_editing(&self) -> Option<&HeaderEdit> {
        self.header_editing.as_ref()
    }

    /// Entry in progress in a header field, mutable.
    pub fn header_editing_mut(&mut self) -> Option<&mut HeaderEdit> {
        self.header_editing.as_mut()
    }

    /// Opens the entry of a header field, buffer initialized to its current
    /// value. Replaces without committing any previous entry, and
    /// cancels along the way a cell entry in progress — only one
    /// editable thing at a time in the window (otherwise a cell and a header
    /// field could be in editing simultaneously, each
    /// claiming keyboard focus for itself).
    pub fn begin_header_edit(&mut self, column: NodeId, field: HeaderField) {
        self.editing = None;
        let buffer = self.header_field_display_text(column, field);
        self.header_editing = Some(HeaderEdit {
            column,
            field,
            buffer,
            typed: false,
        });
    }

    /// Abandons the header entry in progress without writing anything.
    pub fn cancel_header_edit(&mut self) {
        self.header_editing = None;
    }

    /// Commits the header entry in progress. No effect if there is no
    /// entry open.
    pub fn commit_header_edit(&mut self) {
        let Some(edit) = self.header_editing.take() else {
            return;
        };
        let action = match edit.field {
            HeaderField::LongName => Action::SetColumnLongName {
                column: edit.column,
                text: edit.buffer,
            },
            HeaderField::Unit => Action::SetColumnUnit {
                column: edit.column,
                text: edit.buffer,
            },
            HeaderField::Comment => Action::SetColumnComment {
                column: edit.column,
                text: edit.buffer,
            },
            HeaderField::Formula => {
                // Not touching the field writes nothing: committing as is the
                // displayed text (especially if it carries `#REF`, which cannot be
                // rewritten) leaves the stored formula as it is.
                if edit.buffer == self.header_field_display_text(edit.column, HeaderField::Formula)
                {
                    return;
                }
                Action::SetColumnFormula {
                    column: edit.column,
                    formula: (!edit.buffer.trim().is_empty()).then_some(edit.buffer),
                }
            }
            HeaderField::XPrime => Action::SetColumnXPrime {
                column: edit.column,
                text: edit.buffer,
            },
        };
        self.dispatch(action);
    }

    /// Cell entry in progress, if there is one.
    pub fn editing(&self) -> Option<&CellEdit> {
        self.editing.as_ref()
    }

    /// Entry in progress, mutable: the window layer plugs its
    /// `TextEdit` into it so that keystrokes land in the buffer without
    /// touching the model.
    pub fn editing_mut(&mut self) -> Option<&mut CellEdit> {
        self.editing.as_mut()
    }

    /// Opens the entry of a cell, buffer initialized to the displayed text.
    /// Replaces without committing any previous entry: it is up to
    /// the caller to commit first, if desired. Cancels along the way a
    /// header entry in progress — see the same remark on
    /// [`AppState::begin_header_edit`].
    pub fn begin_edit(&mut self, column: NodeId, row: usize) {
        self.header_editing = None;
        let buffer = self.cell_text(column, row);
        self.editing = Some(CellEdit {
            column,
            row,
            buffer,
            typed: false,
        });
    }

    /// Abandons the entry in progress without writing anything.
    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    /// Commits the entry in progress (equivalent to [`Action::CommitCell`] on the
    /// buffer). No effect if there is no entry open.
    pub fn commit_edit(&mut self) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        self.dispatch(Action::CommitCell {
            column: edit.column,
            row: edit.row,
            text: edit.buffer,
        });
    }

    /// Note entry in progress, if there is one. The exact counterpart of
    /// [`AppState::editing`] for a note — one buffer at a time, for
    /// the same reason of principle (even though nothing here forces the limit to a
    /// single buffer: a note is not virtualized like a grid of a
    /// million rows, but following the same convention keeps `stokesforge_app::app`
    /// simple, a single deferred-editing mechanism to understand).
    pub fn note_editing(&self) -> Option<&NoteEdit> {
        self.note_editing.as_ref()
    }

    /// Note entry in progress, mutable.
    pub fn note_editing_mut(&mut self) -> Option<&mut NoteEdit> {
        self.note_editing.as_mut()
    }

    /// Opens the entry of a note, buffer initialized to its current text (empty
    /// if `note` is not — or no longer — a note of this project).
    pub fn begin_note_edit(&mut self, note: NodeId) {
        let buffer = self.note_text(note).unwrap_or_default();
        self.note_editing = Some(NoteEdit { note, buffer });
    }

    /// Abandons the note entry in progress without writing anything.
    pub fn cancel_note_edit(&mut self) {
        self.note_editing = None;
    }

    /// Commits the note entry in progress (equivalent to [`Action::SetNoteText`]
    /// on the buffer). No effect if there is no entry open.
    pub fn commit_note_edit(&mut self) {
        let Some(edit) = self.note_editing.take() else {
            return;
        };
        self.dispatch(Action::SetNoteText {
            note: edit.note,
            text: edit.buffer,
        });
    }

    /// Current draft of a graph's properties window, if there
    /// is one.
    pub fn plot_edit(&self) -> Option<&PlotEdit> {
        self.plot_edit.as_ref()
    }

    /// Current draft, mutable — this is where the panel's widgets
    /// write, without going through a command.
    pub fn plot_edit_mut(&mut self) -> Option<&mut PlotEdit> {
        self.plot_edit.as_mut()
    }

    /// Opens (or keeps) the editing draft of graph `graph`. To be
    /// called every frame by the properties window, *before* painting
    /// anything: resets the draft only if there is none yet,
    /// or if it was about another graph — repainting the same
    /// frame on the same graph must never erase an entry in
    /// progress. A draft with no entry in progress does follow the model
    /// (see [`PlotEdit::base`]). A `graph` without
    /// spec (node deleted, or not a graph) leaves the draft unchanged:
    /// it is up to the caller to check `self.tree().node(graph)` before
    /// showing the panel (see `node_title` in `stokesforge_app::app::tab`).
    pub fn begin_plot_edit(&mut self, graph: NodeId) {
        let Some(model) = self.plot_spec(graph) else {
            return;
        };
        if let Some(edit) = self.plot_edit.as_mut().filter(|edit| edit.graph == graph) {
            if edit.spec == edit.base && edit.base != model {
                edit.spec = model.clone();
                edit.base = model;
            }
            return;
        }
        self.plot_edit = Some(PlotEdit {
            graph,
            spec: model.clone(),
            selected_series: 0,
            selected_panel: 0,
            base: model,
        });
    }

    /// Closes the current editing draft without committing anything.
    pub fn close_plot_edit(&mut self) {
        self.plot_edit = None;
    }

    /// The graph properties window, if it is open.
    pub fn plot_settings(&self) -> Option<PlotSettings> {
        self.plot_settings
    }

    /// Changes the tab or the axis shown by the properties window.
    pub fn plot_settings_mut(&mut self) -> Option<&mut PlotSettings> {
        self.plot_settings.as_mut()
    }

    /// **Applies** and closes the properties window (the "Apply" button):
    /// first commits what still differs from the model (an entry whose
    /// field has not lost focus), closes the history macro — all the
    /// gestures of the window form only **one** entry, which a single Ctrl+Z
    /// undoes — then drops the draft.
    pub fn close_plot_settings(&mut self) {
        self.commit_plot_edit();
        self.end_plot_macro();
        self.plot_settings = None;
        self.plot_edit = None;
    }

    /// **Cancels** and closes the properties window (the "Cancel" button,
    /// the close cross, Esc): aborts the history macro, which restores the
    /// graph as it was at opening, and throws the draft away. Without a
    /// macro (opening refused, see [`AppState::begin_plot_macro`]), the
    /// gestures already applied stay in the ordinary history.
    pub fn cancel_plot_settings(&mut self) {
        self.abort_plot_macro();
        self.plot_settings = None;
        self.plot_edit = None;
    }

    /// Aborts the properties macro if it is open: the targeted object
    /// returns to its opening state.
    fn abort_plot_macro(&mut self) {
        if self.plot_macro_open {
            self.plot_macro_open = false;
            if let Err(err) = self.handle.abort_macro() {
                self.fail(tr!(
                    "error-plot-settings-cancel",
                    cause = errors::session::error_message(&err)
                ));
            }
            self.refresh();
        }
    }

    /// Opens the history macro of the properties window. A refusal
    /// (macro already open by another sender, for example a script) is
    /// reported to the user; the window stays usable, without grouped
    /// undo.
    fn begin_plot_macro(&mut self) {
        if self.plot_macro_open {
            return;
        }
        match self.handle.begin_macro(MacroKind::GraphProperties) {
            Ok(_) => self.plot_macro_open = true,
            Err(err) => self.fail(tr!(
                "error-plot-settings-undo-group",
                cause = errors::session::error_message(&err)
            )),
        }
    }

    /// Message shown when actions of a detached window are refused
    /// because the modal graph properties window is open.
    pub fn reject_detached_actions(&mut self) {
        self.fail(tr!("error-plot-settings-block-detached"));
    }

    /// Closes the macro of the properties window (one history entry,
    /// none if nothing changed).
    fn end_plot_macro(&mut self) {
        if !self.plot_macro_open {
            return;
        }
        self.plot_macro_open = false;
        self.end_grouped_macro();
    }

    /// Commits the current draft ([`Action::SetPlotSpec`]) — but
    /// only if it differs from the spec currently in the model. Without
    /// this guard, releasing a slider that did not move or losing the focus
    /// of an unmodified text field would stack a history entry
    /// identical to the previous one; see the head doc of [`PlotEdit`].
    pub fn commit_plot_edit(&mut self) {
        let Some(edit) = self.plot_edit.clone() else {
            return;
        };
        if self.plot_spec(edit.graph).as_ref() == Some(&edit.spec) {
            return;
        }
        self.dispatch(Action::SetPlotSpec {
            graph: edit.graph,
            spec: edit.spec.clone(),
        });
        if let Some(current) = self.plot_edit.as_mut() {
            current.base = edit.spec;
        }
    }

    /// The sheets of `book`, in the order of the inner tabs
    /// `app::tab::Tab::Book`: `(NodeId, name)`. Empty if `book` no longer
    /// exists or is not a workbook.
    pub fn worksheets_of(&self, book: NodeId) -> Vec<(NodeId, String)> {
        worksheets_of_in(&self.snapshot.tree, book)
    }

    /// The workbooks of the project, as `(identifier, readable
    /// path)`. The exact counterpart of [`AppState::graphs`], for the
    /// initial layout of `stokesforge_app::app::StokesForgeApp::new`: opening the
    /// first workbook of the project, as the first graph already was.
    pub fn books(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id).is_some_and(
                    |node| matches!(node.kind(), NodeKind::Workbook { meta, .. } if !meta.hidden),
                )
            })
            .map(|id| (id, self.readable_path(id)))
            .collect()
    }

    /// Text of a note, or `None` if `note` is not (or no longer) a note of
    /// this project.
    pub fn note_text(&self, note: NodeId) -> Option<String> {
        note_text_in(&self.snapshot.tree, note)
    }

    /// The project tree as the Project Manager panel shows it:
    /// expandable folders, clickable sheets (`Workbook`, `Graph`,
    /// `Matrix`, `Note`) never expanded further — see the head doc
    /// of [`ProjectTreeNode`]. Root excluded (it has no name to display
    /// as a panel node): its direct children are the root of
    /// the rendered tree.
    pub fn project_tree(&self) -> Vec<ProjectTreeNode> {
        self.project_tree_children(self.snapshot.tree.root())
    }

    fn project_tree_children(&self, parent: NodeId) -> Vec<ProjectTreeNode> {
        let tree = &self.snapshot.tree;
        let Some(node) = tree.node(parent) else {
            return Vec::new();
        };
        node.children()
            .iter()
            .filter_map(|id| {
                let child = tree.node(*id)?;
                if matches!(child.kind(), NodeKind::Workbook { meta, .. } if meta.hidden) {
                    return None;
                }
                let name = child.name().to_string();
                match child.kind() {
                    NodeKind::Folder { .. } => Some(ProjectTreeNode::Folder {
                        id: *id,
                        name,
                        children: self.project_tree_children(*id),
                    }),
                    NodeKind::Workbook { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Book,
                    }),
                    NodeKind::Graph { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Graph,
                    }),
                    NodeKind::Matrix { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Matrix,
                    }),
                    NodeKind::Note { .. } => Some(ProjectTreeNode::Window {
                        id: *id,
                        name,
                        kind: WindowKind::Note,
                    }),
                    // `Worksheet`/`Column`: never met here, children
                    // of a `Workbook`/`Worksheet` that this walk never
                    // reaches (we stop at `Workbook`). `Unknown`: no
                    // window to open on the UI side for an unrecognized kind
                    // (invariant 10) — the node still exists and will be
                    // rewritten as is at save time, but the Project
                    // Manager has nothing to show for it.
                    NodeKind::Worksheet { .. } | NodeKind::Column(_) | NodeKind::Unknown(_) => None,
                }
            })
            .collect()
    }

    /// The sheets of the project, for tests and generic uses:
    /// `(NodeId, readable path)`, in prefix traversal order. The Project
    /// Manager panel does not use it (it stops at `Workbook`, see
    /// [`AppState::project_tree`]); this method remains the exact counterpart of
    /// [`AppState::graphs`] for whoever needs the flat list.
    ///
    /// The path is rebuilt here ("Book1 / Sheet1") rather than obtained from
    /// `Project::path_of`: that method lives on `Project`, which the UI does not
    /// hold — it has only an `Arc<Tree>`. This is a gap noted by
    /// this phase, not an aesthetic choice.
    pub fn sheets(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id)
                    .is_some_and(|node| node.kind().is_worksheet())
            })
            .map(|id| (id, self.readable_path(id)))
            .collect()
    }

    /// The graphs of the project, as `(identifier, readable path)`.
    ///
    /// The exact counterpart of [`AppState::sheets`], for the rendering panel of
    /// phase R4. Like it, it rebuilds the path by hand for lack of
    /// `Project::path_of` on the `Arc<Tree>` side.
    pub fn graphs(&self) -> Vec<(NodeId, String)> {
        let tree = &self.snapshot.tree;
        tree.walk_all()
            .into_iter()
            .filter(|id| {
                tree.node(*id)
                    .is_some_and(|node| matches!(node.kind(), NodeKind::Graph { .. }))
            })
            .map(|id| (id, self.readable_path(id)))
            .collect()
    }

    /// Readable path of a node: the names of its ancestors (root excluded)
    /// then its own, separated by " / ".
    fn readable_path(&self, id: NodeId) -> String {
        let tree = &self.snapshot.tree;
        let root = tree.root();
        let mut parts: Vec<String> = tree
            .ancestors(id)
            .unwrap_or_default()
            .into_iter()
            .rev()
            .filter(|ancestor| *ancestor != root)
            .filter_map(|ancestor| tree.node(ancestor).map(|node| node.name().to_string()))
            .collect();
        if let Some(node) = tree.node(id) {
            parts.push(node.name().to_string());
        }
        parts.join(" / ")
    }

    /// Opening of the panel, remembered outside the project.
    pub fn object_manager_open(&self) -> bool {
        self.object_manager_open
    }

    /// Opens or closes the panel for a test, without writing the
    /// machine's `preferences.json` (which
    /// `Action::SetObjectManagerOpen` does): a test thus neither depends on nor
    /// modifies the user's preference.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_object_manager_open_for_test(&mut self, open: bool) {
        self.object_manager_open = open;
    }

    /// Scales (axes) listed in the Object Manager (#111), outside the project.
    pub fn object_manager_show_scales(&self) -> bool {
        self.object_manager_settings().show_scales
    }

    pub fn object_manager_settings(&self) -> ObjectManagerSettings {
        self.object_manager_preferences.object_manager_settings()
    }

    fn save_object_manager_settings(&mut self, settings: ObjectManagerSettings) {
        if let Err(error) = self
            .object_manager_preferences
            .save_object_manager_settings(settings)
        {
            self.fail(tr!(
                "error-preferences-not-saved",
                cause = errors::io::error_message(&error)
            ));
        }
    }

    /// Applies an action. Never returns an error: every error becomes a
    /// [`StatusMessage`]. Refreshes the snapshot after any action that may have
    /// changed the model.
    ///
    /// **Never blocks**, `RunScript` included: see the section "The
    /// Python script" at the top of this module. This is what makes this method
    /// safe to call from the rendering loop for any action.
    pub fn dispatch(&mut self, action: Action) {
        // A dialog error does not outlive its dialog: the gesture that
        // opens another one starts from an empty footer.
        if !self.modal_dialog_open() {
            self.dialog_error = None;
        }
        match action {
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Save => {
                self.save();
            }
            Action::SaveAs { path } => self.save_as(path),
            Action::Load => {
                self.load();
            }
            Action::NewProject { pending_edits } => {
                self.request_unsaved(UnsavedGesture::NewProject, pending_edits);
            }
            Action::OpenProject {
                path,
                pending_edits,
            } => self.request_unsaved(UnsavedGesture::Open(path), pending_edits),
            Action::Quit { pending_edits } => {
                if self.session_lost {
                    // Nothing to save or to ask: the model no longer exists.
                    self.quit_ready = true;
                } else if self.quit_without_prompt {
                    self.perform_unsaved(UnsavedGesture::Quit);
                } else {
                    self.request_unsaved(UnsavedGesture::Quit, pending_edits);
                }
            }
            Action::ConfirmUnsaved(choice) => self.confirm_unsaved(choice),
            Action::DismissAlert => {
                if self
                    .alert
                    .as_ref()
                    .is_some_and(|a| a.kind == AlertKind::SessionLost)
                {
                    self.quit_ready = true;
                }
                self.alert = None;
            }
            Action::ConfirmOverwrite(accept) => {
                if let Some((path, spec)) = self.pending_overwrite.take() {
                    if accept {
                        self.write_graph_export(&path, &spec);
                    }
                }
            }
            Action::ConfirmDiscardedSave(accept) => self.confirm_discarded_save(accept),
            Action::AddGraph => self.execute(Command::AddGraph {
                parent: self.snapshot.tree.root(),
                name: None,
                spec: PlotSpec::empty(""),
            }),
            Action::AddWorkbook => self.add_workbook(),
            Action::AddFolder { parent } => self.add_folder(parent),
            Action::SelectSheet(id) => self.select_sheet(id),
            Action::BeginRenameSheet(sheet) => self.begin_rename_sheet(sheet),
            Action::CommitRenameSheet { sheet, name } => self.commit_rename_sheet(sheet, &name),
            Action::CancelRenameSheet => self.renaming_sheet = None,
            Action::DuplicateSheet(sheet) => self.duplicate_sheet(sheet),
            Action::DeleteSheet(sheet) => self.delete_sheet(sheet),
            Action::RenameObject { id, name } => self.rename_object(id, &name),
            Action::RenameObjects(changes) => self.rename_objects(changes),
            Action::CopyObjects(ids) => self.copy_objects(&ids),
            Action::DuplicateObjects(ids) => self.duplicate_objects(&ids),
            Action::PasteObjects { parent, text } => self.paste_objects(parent, &text),
            Action::ShowGraphData { graph } => {
                self.reveal_books(graph_data_books(self.tree(), graph));
            }
            Action::ShowSeriesData { graph, series } => {
                self.reveal_books(series_data_books(self.tree(), graph, series));
            }
            Action::DeleteObjects(ids) => self.delete_objects(&ids),
            Action::MoveObjects { ids, parent } => self.move_objects(&ids, parent),
            Action::CommitCell { column, row, text } => {
                self.commit_cell(column, row, text);
            }
            Action::AddColumn { sheet } => self.add_column(sheet),
            Action::AddWorksheet { book } => self.add_worksheet(book),
            Action::AddColumnAndSelect { sheet, row } => self.add_column_and_select(sheet, row),
            Action::AddColumnAndSelectHeaderField { sheet, field } => {
                self.add_column_and_select_header_field(sheet, field)
            }
            Action::SetColumnType {
                column,
                column_type,
            } => self.execute(Command::SetColumnType {
                column,
                column_type,
            }),
            Action::SetColumnsType {
                columns,
                column_type,
            } => self.set_columns_grouped(MacroKind::SetColumnsType, &columns, |column| {
                Command::SetColumnType {
                    column,
                    column_type,
                }
            }),
            Action::SetColumnsRole { columns, role } => {
                self.set_columns_role_grouped(&columns, role)
            }
            Action::SetColumnRole { column, role } => {
                self.set_column_meta_field(column, |meta| meta.role = role);
            }
            Action::MoveColumns { sheet, columns, to } => self.move_columns(sheet, &columns, to),
            Action::SetColumnWidth { column, width } => self.set_column_width(column, width),
            Action::ResetColumnWidths { columns } => self.reset_column_widths(&columns),
            Action::InsertRows { sheet, at, count } => self.rows(sheet, at, count, true),
            Action::DeleteRows { sheet, at, count } => self.rows(sheet, at, count, false),
            Action::SetNoteText { note, text } => self.set_note_text(note, text),
            Action::RunScript => self.run_script(),
            Action::InterruptScript => self.interrupt_script(),
            Action::CommitCellAndAdvance {
                column,
                row,
                text,
                sheet,
            } => self.commit_cell_and_advance(column, row, text, sheet),
            Action::ClearSelectedCells { sheet } => self.clear_selected_cells(sheet),
            Action::DeleteSelectedCells { sheet } => self.delete_selected_cells(sheet),
            Action::Paste { sheet, text } => self.paste(sheet, text),
            Action::PasteAbsolute { sheet } => self.paste_absolute(sheet),
            Action::SelectAll { sheet } => self.select_all(sheet),
            Action::SetColumnLongName { column, text } => {
                self.set_column_meta_field(column, |meta| meta.long_name = text);
            }
            Action::SetColumnUnit { column, text } => {
                self.set_column_meta_field(column, |meta| meta.unit = text);
            }
            Action::SetColumnComment { column, text } => {
                self.set_column_meta_field(column, |meta| meta.comment = text);
            }
            Action::SetColumnXPrime { column, text } => self.set_column_x_prime(column, text),
            Action::SetColumnFormula { column, formula } => {
                // Refusal: nothing is written, the status bar says why. The
                // conversion from letters to identifiers is done here, above
                // the command (`SetColumnMeta` never validates references).
                match formula {
                    None => self.set_column_meta_field(column, |meta| meta.formula = None),
                    Some(text) => match self.snapshot.tree.formula_from_letters(column, &text) {
                        Ok(formula) => {
                            self.set_column_meta_field(column, |meta| meta.formula = Some(formula));
                        }
                        Err(error) => self.fail(errors::formula::error_message(&error)),
                    },
                }
            }
            Action::RemoveFormulaKeepData { sheet } => {
                let columns = self.formula_columns_selected(sheet);
                self.remove_formulas_keep_data(&columns);
            }
            Action::RemoveColumnsFormulaKeepData { columns } => {
                self.remove_formulas_keep_data(&columns);
            }
            Action::ImportFiles {
                destination,
                recipe,
                paths,
            } => self.import_files(destination, recipe, paths),
            Action::DropFiles { paths, target_book } => self.drop_files(paths, target_book),
            Action::StartImport {
                destination,
                recipe,
                paths,
            } => self.start_import(destination, ImportSource::Menu(recipe), paths),
            Action::ReopenLastImport => self.reopen_last_import(),
            Action::RefreshFromSource { sheet } => self.refresh_from_source(sheet),
            Action::ViewColumnMetadata { column } => self.metadata_view = Some(column),
            Action::CloseColumnMetadata => self.metadata_view = None,
            Action::OpenStatusHistory => self.status_history_open = true,
            Action::CloseStatusHistory => self.status_history_open = false,
            Action::OpenLowR2List => self.low_r2_list_open = self.low_r2_total() > 0,
            Action::CloseLowR2List => self.low_r2_list_open = false,
            Action::StopImport => self.stop_import(),
            Action::StopRecalc => self.stop_recalc(),
            Action::OpenKeyboardShortcuts => self.keyboard_shortcuts_open = true,
            Action::CloseKeyboardShortcuts => self.keyboard_shortcuts_open = false,
            Action::OpenPreferences => self.preferences_open = true,
            Action::ClosePreferences => self.preferences_open = false,
            Action::OpenAnalysis {
                preset,
                sheet,
                columns,
            } => self.open_analysis(&preset, sheet, &columns),
            Action::OpenGraphAnalysis {
                preset,
                graph,
                series,
                spec,
            } => self.open_graph_analysis(&preset, graph, &series, *spec),
            Action::OpenRowAnalysis { preset, rows } => self.open_row_analysis(&preset, &rows),
            Action::RecalculateTranspose { sheet } => self.recalculate_transpose(sheet),
            Action::ConfirmTransposeRelink(accept) => self.confirm_transpose_relink(accept),
            Action::OpenRecentAnalysis { index, run, target } => {
                self.open_recent_analysis(index, run, target)
            }
            Action::RestoreOriginalData(trace) => {
                self.execute(Command::RestoreOriginalData { trace })
            }
            Action::EditCalculation(id) => self.edit_calculation(id),
            Action::MoveCalculationHandle {
                calculation,
                slot,
                x,
                y,
            } => self.move_calculation_handle(calculation, slot, x, y),
            Action::CloseAnalysis => {
                self.abort_row_macro();
                self.analysis = None;
                self.pending_analysis_replacement = false;
            }
            Action::ApplyAnalysis => self.apply_analysis(),
            Action::ConfirmAnalysisReplacement(accept) => self.confirm_analysis_replacement(accept),
            Action::RecalculateCalculations(ids) => self.recalculate_calculations(&ids),
            Action::DetachCalculation(id) => self.detach_calculation(id),
            Action::RequestDeleteCalculation(id) => self.request_delete_calculation(id),
            Action::ConfirmDeleteCalculation(choice) => self.confirm_delete_calculation(choice),
            Action::ConfirmStaleExport(choice) => self.confirm_stale_export(choice),
            Action::CloseColumnUse => self.close_column_use(),
            Action::ConfirmFullColumnsCopy(choice) => self.confirm_full_columns_copy(choice),
            Action::UndoColumnUse => self.undo_column_use(),
            Action::PlotColumns {
                sheet,
                columns,
                plot_type,
            } => self.plot_columns(sheet, columns, plot_type),
            Action::AddSeriesToGraph {
                graph,
                panel,
                columns,
                plot_type,
            } => self.add_series_to_graph(graph, panel, columns, plot_type),
            Action::AddLayer {
                graph,
                panel,
                x_top,
                y_right,
            } => self.add_layer(graph, panel, x_top, y_right),
            Action::AddRowsToGraph {
                graph,
                panel,
                sheet,
                rows,
                axis,
            } => self.add_rows_to_graph(graph, panel, sheet, rows, axis),
            Action::ConfirmRowDrop(choice) => self.confirm_row_drop(choice),
            Action::PlotRows {
                sheet,
                rows,
                plot_type,
            } => self.plot_rows(sheet, rows, plot_type),
            Action::SetObjectManagerOpen(open) => {
                self.object_manager_open = open;
                if let Err(error) = self
                    .object_manager_preferences
                    .save_object_manager_open(open)
                {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
            Action::SetObjectManagerShowObjects(show) => {
                let mut settings = self.object_manager_settings();
                settings.show_objects = show;
                self.save_object_manager_settings(settings);
            }
            Action::SetObjectManagerShowScales(show) => {
                let mut settings = self.object_manager_settings();
                settings.show_scales = show;
                self.save_object_manager_settings(settings);
            }
            Action::SetObjectManagerGroupOpen { group, open } => {
                let mut settings = self.object_manager_settings();
                settings.set_group_open(group, open);
                self.save_object_manager_settings(settings);
            }
            Action::ResetObjectManagerSettings => {
                self.save_object_manager_settings(ObjectManagerSettings::default());
            }
            Action::SetPlotVisibility {
                graph,
                target,
                visible,
            } => {
                if let Some(mut spec) = current_plot_spec(self.tree(), graph) {
                    let shown = crate::object_manager::Shown::from(self.object_manager_settings());
                    if target.set_shown(&mut spec, visible, shown) {
                        self.execute(Command::EditPlotSpec {
                            graph,
                            spec,
                            kind: PlotEditKind::Visibility,
                        });
                    }
                }
            }
            Action::SetClosedFrame {
                graph,
                panel,
                closed,
            } => self.edit_plot_object(graph, PlotEditKind::ClosedFrame, |spec| {
                spec.panels.get_mut(panel).is_some_and(|panel| {
                    panel.set_frame_closed(closed);
                    true
                })
            }),
            Action::PlaceSeries {
                graph,
                source,
                group,
                position,
            } => self.edit_plot_object(graph, PlotEditKind::ReorderSeries, |spec| {
                spec.panels.get_mut(source.panel).is_some_and(|panel| {
                    panel.place_series(source.index, group, position).is_some()
                })
            }),
            Action::PlaceSeriesGroup {
                graph,
                group,
                position,
            } => self.edit_plot_object(graph, PlotEditKind::ReorderGroup, |spec| {
                spec.panels
                    .get_mut(group.panel)
                    .is_some_and(|panel| panel.place_group(group.group, position).is_some())
            }),
            Action::ReorderObject {
                graph,
                target,
                index,
            } => self.edit_plot_object(graph, PlotEditKind::ReorderObject, |spec| {
                crate::object_drag::reorder_object(spec, target, index)
            }),
            Action::MoveSeries { graph, series, up } => {
                self.edit_plot_object(graph, PlotEditKind::ReorderSeries, |spec| {
                    spec.panels
                        .get_mut(series.panel)
                        .is_some_and(|panel| panel.move_series(series.index, up).is_some())
                })
            }
            Action::MoveSeriesGroup { graph, group, up } => {
                self.edit_plot_object(graph, PlotEditKind::ReorderGroup, |spec| {
                    spec.panels
                        .get_mut(group.panel)
                        .is_some_and(|panel| panel.move_group(group.group, up))
                })
            }
            Action::RenameSeriesGroup { graph, group, name } => {
                self.edit_plot_object(graph, PlotEditKind::RenameGroup, |spec| {
                    spec.panels
                        .get_mut(group.panel)
                        .is_some_and(|panel| panel.rename_group(group.group, &name))
                })
            }
            Action::UngroupSeries { graph, group } => {
                self.edit_plot_object(graph, PlotEditKind::DissolveGroup, |spec| {
                    spec.panels
                        .get_mut(group.panel)
                        .is_some_and(|panel| panel.dissolve_group(group.group))
                })
            }
            Action::RemovePlotObject { graph, target } => {
                use crate::object_manager::VisibilityTarget as T;
                match target {
                    T::SeriesGroup(group) => {
                        self.edit_plot_object(graph, PlotEditKind::RemoveGroup, |spec| {
                            spec.panels
                                .get_mut(group.panel)
                                .is_some_and(|panel| panel.remove_group(group.group))
                        })
                    }
                    T::Series(series) => {
                        self.edit_plot_object(graph, PlotEditKind::RemoveSeries, |spec| {
                            spec.panels
                                .get_mut(series.panel)
                                .is_some_and(|panel| panel.remove_series(series.index))
                        })
                    }
                    T::Annotation(target) => {
                        self.dispatch(Action::RemoveAnnotation { graph, target })
                    }
                    T::Shape(target) => self.dispatch(Action::RemoveShape { graph, target }),
                    T::Text(target) => {
                        self.edit_plot_object(graph, PlotEditKind::RemoveText, |spec| {
                            target.set_text(spec, String::new())
                        })
                    }
                    T::Graph | T::Panel(_) | T::Group(_) | T::Axis(_) | T::Axes(_) => {}
                }
            }
            Action::OpenObjectProperties { graph, target } => {
                use crate::object_manager::VisibilityTarget as T;
                let Some(spec) = self.plot_spec(graph) else {
                    return;
                };
                if !target.exists(&spec) {
                    return;
                }
                let (tab, axis, panel, series) = match target {
                    T::Series(s) => (PlotSettingsTab::Series, None, s.panel, Some(s.index)),
                    // A group opens on its first series (#197).
                    T::SeriesGroup(g) => (
                        PlotSettingsTab::Series,
                        None,
                        g.panel,
                        spec.panels
                            .get(g.panel)
                            .and_then(|panel| panel.group_members(g.group).first().copied()),
                    ),
                    T::Panel(i) => (PlotSettingsTab::PlotArea, None, i, None),
                    T::Group(i) => (PlotSettingsTab::Series, None, i, None),
                    T::Axes(i) => (PlotSettingsTab::Axes, None, i, None),
                    T::Text(TextTarget::Legend { panel }) => {
                        (PlotSettingsTab::Legend, None, panel, None)
                    }
                    T::Text(TextTarget::AxisTitle { panel, side }) => (
                        PlotSettingsTab::Axes,
                        Some(match side {
                            stokesforge_plot::AxisSide::Bottom => PlotAxis::Bottom,
                            stokesforge_plot::AxisSide::Top => PlotAxis::Top,
                            stokesforge_plot::AxisSide::Left => PlotAxis::Left,
                            stokesforge_plot::AxisSide::Right => PlotAxis::Right,
                        }),
                        panel,
                        None,
                    ),
                    T::Axis(axis) => (
                        PlotSettingsTab::Axes,
                        Some(match axis.side {
                            stokesforge_plot::AxisSide::Bottom => PlotAxis::Bottom,
                            stokesforge_plot::AxisSide::Top => PlotAxis::Top,
                            stokesforge_plot::AxisSide::Left => PlotAxis::Left,
                            stokesforge_plot::AxisSide::Right => PlotAxis::Right,
                        }),
                        axis.panel,
                        None,
                    ),
                    T::Text(TextTarget::PanelTitle { panel }) => {
                        (PlotSettingsTab::PlotArea, None, panel, None)
                    }
                    _ => (PlotSettingsTab::Graph, None, 0, None),
                };
                self.open_plot_settings(graph, tab, axis);
                if let Some(edit) = self.plot_edit.as_mut() {
                    edit.selected_panel = panel;
                    if let Some(series) = series {
                        edit.selected_series = series;
                    }
                }
            }
            Action::SetPlotSpec { graph, spec } => {
                self.execute(Command::SetPlotSpec { graph, spec })
            }
            Action::OpenPlotSettings { graph, tab, axis } => {
                self.open_plot_settings(graph, tab, axis);
            }
            Action::SaveColor {
                usage,
                color,
                target,
            } => {
                let next = self.saved_colors(target).with_added(usage, &color);
                self.replace_saved_colors(target, next);
            }
            Action::RemoveSavedColor {
                usage,
                color,
                target,
            } => {
                let next = self.saved_colors(target).with_removed(usage, &color);
                self.replace_saved_colors(target, next);
            }
            Action::SaveGradient {
                name,
                stops,
                target,
            } => {
                let next = self.saved_colors(target).with_gradient_added(&name, &stops);
                self.replace_saved_colors(target, next);
            }
            Action::RemoveSavedGradient { name, target } => {
                let next = self.saved_colors(target).with_gradient_removed(&name);
                self.replace_saved_colors(target, next);
            }
            Action::PickFile(_) => {}
            Action::ExportGraph { .. } | Action::CopyGraphFigure { .. } => {
                self.export_unless_stale(action)
            }
            Action::SetAnnotation {
                graph,
                target,
                annotation,
            } => self.edit_annotations(graph, |spec| target.set(spec, *annotation)),
            Action::RemoveAnnotation { graph, target } => {
                self.edit_plot_object(graph, PlotEditKind::RemoveText, |spec| target.remove(spec))
            }
            Action::SetShape {
                graph,
                target,
                shape,
            } => self.edit_annotations(graph, |spec| target.set(spec, *shape)),
            Action::BeginShapeEdit => self.begin_plot_macro(),
            Action::EndShapeEdit { keep: true } => self.end_plot_macro(),
            Action::EndShapeEdit { keep: false } => self.abort_plot_macro(),
            Action::RemoveShape { graph, target } => {
                self.edit_plot_object(graph, PlotEditKind::RemoveObject, |spec| {
                    target.remove(spec)
                })
            }
            Action::ReorderShape {
                graph,
                target,
                order,
            } => self.edit_plot_object(graph, PlotEditKind::ReorderObject, |spec| match order {
                ShapeOrder::ToFront => target.bring_to_front(spec).is_some(),
                ShapeOrder::ToBack => target.send_to_back(spec).is_some(),
            }),
            Action::SetTextAnchor {
                graph,
                target,
                anchor,
            } => self.edit_annotations(graph, |spec| target.set_anchor(spec, anchor)),
            Action::SetTextFont {
                graph,
                target,
                font,
            } => self.edit_annotations(graph, |spec| target.set_font(spec, font)),
            Action::SetTextColor {
                graph,
                target,
                color,
            } => self.edit_annotations(graph, |spec| target.set_color(spec, color)),
            Action::ResetTextAnchors { graph } => {
                self.edit_annotations(graph, |spec| spec.reset_text_anchors() > 0)
            }
            Action::SetTextContent {
                graph,
                target,
                text,
            } => self.edit_annotations(graph, |spec| target.set_text(spec, text)),
            Action::SetLegendEntryName {
                graph,
                series,
                name,
            } => self.edit_plot_object(graph, PlotEditKind::RenameSeries, |spec| {
                spec.series_at_mut(series)
                    .map(|series| series.name = name)
                    .is_some()
            }),
            Action::SetMaxFileMb(megabytes) => {
                if let Err(error) = self.object_manager_preferences.save_max_file_mb(megabytes) {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
            Action::SetMaxCellsM(millions) => {
                if let Err(error) = self.object_manager_preferences.save_max_cells_m(millions) {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
            Action::SetAutosave(on) => self.set_autosave(on),
            Action::SetAutosaveMinutes(minutes) => self.set_autosave_minutes(minutes),
            Action::RecoveryOpen(path) => self.recovery_open(path),
            Action::RecoveryIgnore(path) => self.recovery_ignore(&path),
            Action::RecoveryDelete(path) => self.recovery_request_delete(path),
            Action::ConfirmRecoveryDelete(accept) => self.recovery_confirm_delete(accept),
            Action::CloseRecovery => self.recovery_close(),
            Action::SetSvgTextMode(mode) => {
                if let Err(error) = self.object_manager_preferences.save_svg_text_mode(mode) {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
            Action::SetCopyFormats(entries) => {
                if let Err(error) = self.object_manager_preferences.save_copy_formats(&entries) {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
            Action::SetAnalysisLowR2Dialog(on) => {
                if let Err(error) = self
                    .object_manager_preferences
                    .save_analysis_low_r2_dialog(on)
                {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
            Action::SetWorkbookHeaderDefaults { metadata, x_prime } => {
                if let Err(error) = self
                    .object_manager_preferences
                    .save_workbook_header_rows(metadata, x_prime)
                {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
            Action::SetPngExportScale(scale) => {
                if PNG_EXPORT_SCALES.contains(&scale) {
                    self.png_export_scale = scale;
                }
            }
            Action::RememberDialogGeometry { kind, geometry } => {
                if let Err(error) = self
                    .object_manager_preferences
                    .save_dialog_geometry(&kind, geometry)
                {
                    self.fail(tr!(
                        "error-preferences-not-saved",
                        cause = errors::io::error_message(&error)
                    ));
                }
            }
        }
    }

    /// The colors saved in the project, or for the user.
    pub fn saved_colors(&self, target: SaveTarget) -> SavedColors {
        match target {
            SaveTarget::Project => self.snapshot.tree.saved_colors().clone(),
            SaveTarget::User => self.user_colors.colors().clone(),
        }
    }

    /// Makes the user's colors be read and written in `path` rather than
    /// in the configuration folder — for tests, which must not
    /// touch the real preferences of the machine.
    pub fn use_user_colors_file(&mut self, path: std::path::PathBuf) {
        self.user_colors = UserColors::load_from(Some(path));
    }

    fn replace_saved_colors(&mut self, target: SaveTarget, next: SavedColors) {
        if next == self.saved_colors(target) {
            return;
        }
        match target {
            SaveTarget::Project => self.execute(Command::SetSavedColors { colors: next }),
            SaveTarget::User => {
                if let Err(err) = self.user_colors.replace(next) {
                    self.fail(tr!(
                        "error-colors-session-only",
                        cause = errors::io::error_message(&err)
                    ));
                }
            }
        }
    }

    /// See [`Action::OpenPlotSettings`]. Switching to another graph first
    /// commits the draft of the previous one; a graph that no longer exists (or
    /// is not a graph) leaves everything as it is.
    fn open_plot_settings(&mut self, graph: NodeId, tab: PlotSettingsTab, axis: Option<PlotAxis>) {
        if self.plot_spec(graph).is_none() {
            return;
        }
        if self
            .plot_edit
            .as_ref()
            .is_some_and(|edit| edit.graph != graph)
        {
            self.commit_plot_edit();
            self.end_plot_macro();
            self.plot_edit = None;
        }
        self.begin_plot_macro();
        self.begin_plot_edit(graph);
        let current_axis = self
            .plot_settings
            .filter(|settings| settings.graph == graph)
            .map(|settings| settings.axis);
        self.plot_settings = Some(PlotSettings {
            graph,
            tab,
            axis: axis.or(current_axis).unwrap_or(PlotAxis::Bottom),
        });
    }

    /// Refreshes the snapshot from the actor and revalidates the selection.
    ///
    /// Revalidation is not reserved to reloading: it is the only
    /// place that guarantees the displayed sheet still exists, whatever
    /// the way the tree changed (including by another client of
    /// the actor). A refresh failure overwrites the current message —
    /// an unreachable actor is more serious than what we were about to say.
    pub fn refresh(&mut self) {
        self.check_session();
        if self.session_lost {
            return;
        }
        // A recalculation is running (#239): the actor would answer only at its end and
        // the interface would stay frozen. The old snapshot stays painted;
        // `poll_recalc` refreshes as soon as it is done.
        if self.recalc_since.is_some() && self.handle.recalc_running().is_some() {
            return;
        }
        match self.handle.snapshot() {
            Ok(snapshot) => {
                if self.snapshot.epoch != snapshot.epoch {
                    self.renaming_sheet = None;
                    self.pending_row_drop = None;
                    self.pending_calculation_delete = None;
                    self.pending_analysis_replacement = false;
                    self.pending_stale_export = None;
                    self.column_use = None;
                    self.pending_full_copy = None;
                    self.pending_overwrite = None;
                    self.pending_discarded_save = None;
                    self.object_clipboard_output = None;
                    self.copied_objects.clear();
                }
                let reopened = self.snapshot.epoch != snapshot.epoch;
                self.snapshot = snapshot;
                self.revalidate_selection();
                self.refresh_low_r2_groups();
                if reopened {
                    self.seed_low_r2();
                } else {
                    self.watch_low_r2();
                }
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    /// Polls the detached thread of a script run in progress, never
    /// blocking. To be called **every frame** (see `stokesforge_app::app`): without a
    /// notification pushed by the actor, it is the only place that refreshes
    /// the snapshot after a script — forgetting it would leave the grid frozen
    /// while the model has changed.
    ///
    /// No effect if no script is running (including without the `python`
    /// feature, where this method never does anything): safe to call
    /// every frame without a prior test.
    pub fn poll_script(&mut self) {
        #[cfg(feature = "python")]
        {
            let received = match &self.pending_script {
                Some(receiver) => match receiver.try_recv() {
                    Ok(reply) => Some(reply),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.pending_script = None;
                        self.fail(tr!("error-script-vanished"));
                        return;
                    }
                },
                None => None,
            };
            let Some((engine, result)) = received else {
                return;
            };
            self.pending_script = None;
            if let Some(engine) = engine {
                // `pyst.save_as(path)` may have changed the project path: the
                // window adopts it, as after a "Save as".
                if let Some(path) = engine.project_path() {
                    if Path::new(&self.path) != path {
                        self.path = path.to_string_lossy().into_owned();
                    }
                }
                // A `save`/`save_as` of the script saved the project: without
                // this the title would stay "modified".
                if let Some(saved) = engine.take_saved_revision() {
                    self.saved_revision = Some(saved);
                    self.autosave_mark_clean();
                }
                self.engine = Some(engine);
            }
            match result {
                Ok(outcome) => self.finish_script_run(outcome),
                Err(error) => self.fail(errors::script::error_message(&error)),
            }
        }
    }

    /// Current state of the import window, if it is open.
    pub fn import_state(&self) -> Option<&ImportState> {
        self.import_state.as_ref()
    }

    /// Current state of the import window, mutable — the window layer plugs
    /// its widgets (ComboBox, radio buttons, text fields) into it to mutate
    /// `recipe` directly, without going through [`Action`]: transient
    /// mutation, like [`AppState::header_editing_mut`].
    pub fn import_state_mut(&mut self) -> Option<&mut ImportState> {
        self.import_state.as_mut()
    }

    /// Opens the import window for `book`. Loads the [`stokesforge_import::
    /// RecipeStore`] from disk once for the whole time
    /// the window is open (never reloaded every frame) and
    /// starts on its first saved template if there is one, otherwise on
    /// a permissive default recipe ([`stokesforge_import::ImportRecipe::
    /// new`]). Replaces without confirmation an import window already open
    /// (same pattern as [`AppState::begin_edit`] for a cell
    /// entry).
    pub fn open_import(&mut self, book: impl Into<Option<NodeId>>) {
        let active_book = book.into();
        // Start on the first saved template, otherwise on the permissive
        // recipe.
        let store = stokesforge_import::RecipeStore::load();
        let recipe = store.recipes().first().cloned().unwrap_or_else(|| {
            stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"))
        });
        let mut recipe = recipe;
        recipe.unify_header();
        let save_as_name = recipe.name.clone();
        self.import_state = Some(ImportState {
            active_book,
            destination: active_book.map_or(
                ImportDestination::NewBookForAll,
                ImportDestination::ActiveBook,
            ),
            paths: Vec::new(),
            store,
            save_as_name,
            step: ImportStep::Files,
            preview_index: 0,
            confirm_delete: None,
            recent: stokesforge_import::RecentFiles::load(),
            baseline: recipe.clone(),
            held_preview: None,
            seen_recipe: recipe.clone(),
            recipe,
            bytes: Default::default(),
        });
    }

    /// Handles a file drop ([`Action::DropFiles`]). The `.stf` files are put
    /// in a queue for opening ([`AppState::take_projects_to_open`]). The
    /// other files go through [`AppState::start_import`]: read on the
    /// background thread with the recognized template, otherwise the last one used, failing
    /// that the permissive recipe; those that read correctly go off in a
    /// single cancellable import, the others open the import window
    /// pre-filled to be set by hand.
    fn drop_files(&mut self, paths: Vec<PathBuf>, target_book: Option<NodeId>) {
        // An OS drop is not blocked by egui's modal layer, notably
        // from the detached explorer. The root owns the graph's macro:
        // the import would be absorbed into it if this gesture were not refused here.
        if self.plot_settings.is_some() {
            self.fail(tr!("error-drop-plot-settings-open"));
            return;
        }
        let plan = file_drop_plan(&paths, target_book);
        self.projects_to_open.extend(plan.projects);
        // The wizard is open: the drop adds the files to it instead of
        // importing them on top.
        if self.import_state.is_some() {
            self.add_import_paths(plan.data);
            return;
        }
        if plan.data.is_empty() {
            return;
        }
        let fallback = self
            .last_import
            .as_ref()
            .map(|last| last.recipe.clone())
            .unwrap_or_else(|| {
                stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"))
            });
        self.start_import(
            plan.destination,
            ImportSource::Drop {
                fallback,
                target_book,
            },
            plan.data,
        );
    }

    /// Starts the reading and analysis of the files on a background thread: single
    /// entry point of the menu import ([`Action::StartImport`]) and of the drop.
    fn start_import(
        &mut self,
        destination: ImportDestination,
        source: ImportSource,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }
        if self.import_job.is_some() {
            self.fail(tr!("error-import-running"));
            return;
        }
        let total = paths.len();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (reply_tx, reply) = mpsc::channel();
        let worker_done = Arc::clone(&done);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let dropped = match &source {
            ImportSource::Menu(_) => None,
            ImportSource::Drop { target_book, .. } => Some(*target_book),
        };
        let spawned = thread::Builder::new()
            .name("stokesforge-app-import".to_owned())
            .spawn(move || {
                let mut reply = ImportReply {
                    items: Vec::new(),
                    unreadable: Vec::new(),
                    models: Vec::new(),
                };
                // One template per file for a drop: the automatic choice
                // (extension, name, content signature), otherwise the last
                // template actually used, otherwise the permissive recipe.
                let store = match &source {
                    ImportSource::Drop { .. } => Some(stokesforge_import::RecipeStore::load()),
                    ImportSource::Menu(_) => None,
                };
                for path in paths {
                    if worker_stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    match (&source, &store) {
                        (ImportSource::Drop { fallback, .. }, Some(store)) => {
                            let bytes = std::fs::read(&path).ok();
                            let mut recipe = bytes
                                .as_deref()
                                .and_then(|bytes| {
                                    stokesforge_import::choose_recipe(store.recipes(), &path, bytes)
                                        .cloned()
                                })
                                .unwrap_or_else(|| fallback.clone());
                            // The drop gesture adds one sheet per file:
                            // the replace or merge options of the
                            // last wizard must not change its
                            // destination nor overwrite an existing sheet.
                            recipe.import_mode = stokesforge_import::ImportMode::NewSheet;
                            recipe.files_layout = stokesforge_import::FilesLayout::SheetPerFile;
                            if bytes.as_deref().is_some_and(|b| reads_well(b, &recipe)) {
                                if !reply.models.contains(&recipe.name) {
                                    reply.models.push(recipe.name.clone());
                                }
                                reply.items.extend(import_items(&path, &recipe));
                            } else {
                                reply.unreadable.push(path);
                            }
                        }
                        (ImportSource::Menu(recipe), _) => {
                            reply.items.extend(import_items(&path, recipe));
                        }
                        (ImportSource::Drop { .. }, None) => {}
                    }
                    worker_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                let _ = reply_tx.send(reply);
            });
        match spawned {
            Ok(_) => {
                self.import_job = Some(ImportJob {
                    destination,
                    dropped,
                    total,
                    done,
                    stop,
                    started: std::time::Instant::now(),
                    reply,
                    ready: None,
                });
                self.notice(tr!("status-import-progress", done = 0, total = total));
            }
            Err(err) => self.fail(tr!(
                "error-import-start",
                cause = errors::io::error_message(&err)
            )),
        }
    }

    /// Progress of the import in progress: `(files analyzed, total)`.
    pub fn import_progress(&self) -> Option<(usize, usize)> {
        let job = self.import_job.as_ref()?;
        Some((
            job.done.load(std::sync::atomic::Ordering::Relaxed),
            job.total,
        ))
    }

    /// The background import seen as work to show (#232): how long
    /// it has been running, files analyzed, total.
    pub fn import_work(&self) -> Option<(std::time::Duration, usize, usize)> {
        let job = self.import_job.as_ref()?;
        Some((
            job.started.elapsed(),
            job.done.load(std::sync::atomic::Ordering::Relaxed),
            job.total,
        ))
    }

    /// Stop of the background import (#232, S-P2): the thread stops after the file
    /// in progress, nothing is created or undoable, and the partial result is
    /// ignored.
    pub fn stop_import(&mut self) {
        if let Some(job) = self.import_job.take() {
            job.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            self.notice(tr!("status-import-stopped"));
        }
    }

    /// To be called every frame: updates the progress and, when
    /// the analysis is done, creates the sheets. Returns true if the import has just
    /// been applied. Never blocks.
    pub fn poll_import(&mut self) -> bool {
        let Some(job) = &mut self.import_job else {
            return false;
        };
        if job.ready.is_none() {
            match job.reply.try_recv() {
                Ok(reply) => job.ready = Some(reply),
                Err(mpsc::TryRecvError::Empty) => {
                    let (done, total) = (
                        job.done.load(std::sync::atomic::Ordering::Relaxed),
                        job.total,
                    );
                    self.notice(tr!("status-import-progress", done = done, total = total));
                    return false;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.import_job = None;
                    self.fail(tr!("error-import-stopped"));
                    return false;
                }
            }
        }
        // The graph properties hold the root's history macro:
        // we keep the work and apply it when they close.
        if self.plot_settings.is_some() {
            return false;
        }
        let Some(mut job) = self.import_job.take() else {
            return false;
        };
        let Some(reply) = job.ready.take() else {
            return false;
        };
        let applied = job.dropped.is_none() || !reply.items.is_empty();
        if applied {
            let count = reply.items.len();
            self.apply_import(job.destination, reply.items);
            if job.dropped.is_some()
                && self
                    .status
                    .as_ref()
                    .is_some_and(|status| !status.is_error())
            {
                self.notice(tr!(
                    "status-import-dropped",
                    count = count,
                    models = reply.models.join(", ")
                ));
            }
        }
        if let Some(target_book) = job.dropped {
            if !reply.unreadable.is_empty() {
                if self.import_state.is_some() {
                    self.add_import_paths(reply.unreadable);
                } else {
                    self.open_import(target_book);
                    if let Some(import) = &mut self.import_state {
                        import.paths = reply.unreadable;
                    }
                }
            }
        }
        applied
    }

    /// Files imported successfully since the last call, for the
    /// recent files (emptied: each file is recorded only once).
    pub fn take_imported_paths(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.recent_to_record)
    }

    /// At least one column of `sheet` comes from a file that can be read again.
    pub fn can_refresh_from_source(&self, sheet: NodeId) -> bool {
        self.visible_columns(sheet)
            .iter()
            .any(|(column, ..)| self.column_meta(*column).import_source.is_some())
    }

    /// [`Action::RefreshFromSource`]: re-reads the source files of the columns of
    /// `sheet` and rewrites their values in place, in a single history
    /// entry. A column whose file has disappeared or can no longer be read is
    /// left as it is, with a message.
    fn refresh_from_source(&mut self, sheet: NodeId) {
        let sources: Vec<(NodeId, stokesforge_core::ImportSource)> = self
            .visible_columns(sheet)
            .iter()
            .filter_map(|(column, ..)| {
                self.column_meta(*column)
                    .import_source
                    .map(|source| (*column, source))
            })
            .collect();
        if sources.is_empty() {
            self.fail(tr!("error-refresh-no-imported-column"));
            return;
        }
        // A source file is re-read only once, even if it feeds several
        // columns.
        let mut items: Vec<ImportItem> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        let mut read_paths: Vec<String> = Vec::new();
        for (_, source) in &sources {
            if read_paths.contains(&source.path) {
                continue;
            }
            read_paths.push(source.path.clone());
            let recipe = match stokesforge_import::ImportRecipe::from_json(&source.recipe_json) {
                Ok(recipe) => recipe,
                Err(message) => {
                    failures.push(format!("{}: {message}", source.path));
                    continue;
                }
            };
            for item in import_items(&PathBuf::from(&source.path), &recipe) {
                if let Err(message) = &item.table {
                    failures.push(errors::import::failure_message(message));
                } else {
                    items.push(item);
                }
            }
        }
        // Each column finds its block (file, rank) then its column.
        let refreshed: Vec<(NodeId, PlannedColumn)> = sources
            .iter()
            .filter_map(|(column, source)| {
                let item = items.iter().position(|item| {
                    item.path.display().to_string() == source.path
                        && item.block.unwrap_or(0) == source.block
                })?;
                Some((
                    *column,
                    PlannedColumn {
                        item,
                        column: source.column,
                    },
                ))
            })
            .collect();
        if refreshed.is_empty() {
            self.fail(tr!("error-refresh-failed", failures = failures.join(" ; ")));
            return;
        }
        if self.handle.begin_macro(MacroKind::RefreshFromFile).is_err() {
            self.fail(tr!("error-refresh-group"));
            return;
        }
        // The rows of the sheet follow the longest of the new contents;
        // they shrink only if **all** the columns are refreshed
        // (a hand-typed column is never truncated).
        let new_len = refreshed
            .iter()
            .filter_map(|(_, planned)| stokesforge_importer::import_block(&items, *planned))
            .map(|block| match block {
                ValueBlock::Float(values) => values.len(),
                ValueBlock::Mixed(cells) => cells.len(),
            })
            .max()
            .unwrap_or(0);
        let rows = self.row_count(sheet);
        let all_refreshed = refreshed.len() == self.visible_columns(sheet).len();
        if new_len > rows {
            self.execute(Command::InsertRows {
                sheet,
                at: rows,
                count: new_len - rows,
            });
        } else if new_len < rows && all_refreshed {
            self.execute(Command::DeleteRows {
                sheet,
                at: new_len,
                count: rows - new_len,
            });
        }
        let mut updated = 0usize;
        for (column, planned) in &refreshed {
            let Some(mut block) = stokesforge_importer::import_block(&items, *planned) else {
                continue;
            };
            let length = self
                .snapshot
                .tree
                .node(*column)
                .and_then(Node::as_column)
                .map_or(0, Column::len);
            // Cells past the new content are emptied, and nothing is
            // written past the column.
            match &mut block {
                ValueBlock::Float(values) => {
                    values.resize(length, f64::NAN);
                }
                ValueBlock::Mixed(cells) => cells.resize(length, MixedCell::Empty),
            }
            self.execute(Command::SetCellsPromoting {
                column: *column,
                start: 0,
                values: block,
                expected_version: None,
            });
            updated += 1;
        }
        if self.handle.end_macro().is_err() {
            failures.push(tr!("error-refresh-close-group"));
        }
        self.refresh();
        if failures.is_empty() {
            self.notice(tr!("status-refresh-done", count = updated));
        } else {
            self.fail(tr!(
                "error-refresh-partial",
                count = updated,
                failures = failures.join(" ; ")
            ));
        }
    }

    /// A last import exists and can be resumed in the wizard.
    pub fn can_reopen_last_import(&self) -> bool {
        self.last_import.is_some() && self.import_job.is_none()
    }

    /// [`Action::ReopenLastImport`]: undoes the last import if it is still
    /// at the top of the history (otherwise the correction would be added instead of
    /// replacing it), then reopens the wizard on its files and its recipe.
    fn reopen_last_import(&mut self) {
        let Some(last) = self.last_import.clone() else {
            self.fail(tr!("error-import-nothing-to-reopen"));
            return;
        };
        let undone = self.snapshot.history.undo_label
            == Some(HistoryLabel::Macro(MacroKind::Import))
            && !self.snapshot.history.in_macro;
        if undone {
            self.undo();
        }
        let book = match last.destination {
            ImportDestination::ActiveBook(book) => Some(book),
            _ => None,
        };
        self.open_import(book);
        if let Some(import) = &mut self.import_state {
            import.recipe = last.recipe.clone();
            import.baseline = last.recipe;
            import.save_as_name = import.recipe.name.clone();
            if let ImportDestination::NewBookPerFile | ImportDestination::NewBookForAll =
                last.destination
            {
                import.destination = last.destination;
            }
            import.paths = last.paths;
        }
        self.notice(if undone {
            tr!("status-import-reopened-undone")
        } else {
            tr!("status-import-reopened-kept")
        });
    }

    /// Column whose metadata window is open, if there is one.
    pub fn column_metadata_view(&self) -> Option<NodeId> {
        self.metadata_view
    }

    /// Summary of the "Metadata" cell of the header ("23 keys"), empty if
    /// the column carries none. Section titles do not count.
    pub fn file_metadata_summary(&self, column: NodeId) -> String {
        let count = self
            .column_meta(column)
            .file_metadata
            .iter()
            .filter(|line| !line.is_section())
            .count();
        if count == 0 {
            return String::new();
        }
        crate::tr!("workbook-metadata-summary", count = count)
    }

    /// `.stf` projects dropped since the last call, each to open in
    /// a new StokesForge window (emptied: a project opens only once).
    pub fn take_projects_to_open(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.projects_to_open)
    }

    /// Closes the import window without importing anything.
    pub fn cancel_import(&mut self) {
        self.import_state = None;
    }

    /// Starts over from a new permissive recipe (the "Reset" button).
    /// No effect if the import window is not open.
    pub fn new_import_recipe(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.recipe = stokesforge_import::ImportRecipe::new(tr!("import-template-untitled"));
            state.baseline = state.recipe.clone();
            state.save_as_name = state.recipe.name.clone();
        }
    }

    /// Loads the template `name` of the `RecipeStore` into the recipe being
    /// edited (selection in the drop-down list). No effect if `name`
    /// is not a saved template, or if the window is not open.
    pub fn select_import_recipe(&mut self, name: &str) {
        if let Some(state) = &mut self.import_state {
            if let Some(recipe) = state.store.recipes().iter().find(|r| r.name == name) {
                let mut recipe = recipe.clone();
                recipe.unify_header();
                state.recipe = recipe.clone();
                state.baseline = recipe.clone();
                state.save_as_name = recipe.name.clone();
            }
        }
    }

    /// Replaces the chosen files ("Choose Files to Import", native picker
    /// opened by the window layer — `rfd` is a UI dependency,
    /// it has no place here). No effect if the window is not
    /// open.
    pub fn set_import_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        if let Some(state) = &mut self.import_state {
            state.paths = paths;
            state.preview_index = 0;
            state.bytes.borrow_mut().clear();
        }
    }

    /// Adds `paths` to the wizard's files (drop while it is
    /// open, "Add Files…"), without duplicates.
    ///
    /// At the **first** addition, if the current template has not been modified, the
    /// automatic choice of the template (extension, name, content signature)
    /// can replace it: the message says which one was chosen.
    pub fn add_import_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        let was_empty = self
            .import_state
            .as_ref()
            .is_some_and(|state| state.paths.is_empty());
        if let Some(state) = &mut self.import_state {
            for path in paths {
                if !state.paths.contains(&path) {
                    state.paths.push(path);
                }
            }
        }
        if was_empty && !self.import_recipe_modified() {
            self.choose_import_recipe_automatically();
        }
    }

    /// Chooses, from the file shown, the template that matches it
    /// (`stokesforge_import::choose_recipe`) and loads it as the current recipe.
    /// No effect if no template matches.
    fn choose_import_recipe_automatically(&mut self) {
        let Some(path) = self.import_preview_path() else {
            return;
        };
        let Ok(bytes) = self.import_bytes(&path) else {
            return;
        };
        let Some(state) = &self.import_state else {
            return;
        };
        let Some(chosen) =
            stokesforge_import::choose_recipe(state.store.recipes(), &path, &bytes).cloned()
        else {
            return;
        };
        let name = chosen.name.clone();
        self.select_import_recipe(&name);
        self.notice(tr!("status-template-chosen", name = name));
    }

    /// Writes the current template to `path` (JSON) to share it.
    pub fn export_import_recipe(&mut self, path: &std::path::Path) {
        let Some(state) = &self.import_state else {
            return;
        };
        let json = state.recipe.to_json();
        match std::fs::write(path, json) {
            Ok(()) => self.notice(tr!(
                "status-template-exported",
                path = path.display().to_string()
            )),
            Err(err) => self.fail(tr!(
                "error-template-export",
                cause = errors::io::error_message(&err)
            )),
        }
    }

    /// Reads an exported template from `path`, adds it to the list (replacing
    /// the one of the same name) and saves it, then makes it the current recipe.
    pub fn import_import_recipe(&mut self, path: &std::path::Path) {
        let recipe = match std::fs::read_to_string(path) {
            Ok(text) => stokesforge_import::ImportRecipe::from_json(&text).map_err(|detail| {
                tr!(
                    "error-template-import-invalid",
                    path = path.display().to_string(),
                    detail = detail
                )
            }),
            Err(err) => Err(tr!(
                "error-template-import-read",
                path = path.display().to_string(),
                cause = errors::io::error_message(&err)
            )),
        };
        let mut recipe = match recipe {
            Ok(recipe) => recipe,
            Err(message) => {
                self.fail(message);
                return;
            }
        };
        recipe.unify_header();
        let Some(state) = &mut self.import_state else {
            return;
        };
        let name = recipe.name.clone();
        state.store.upsert(recipe.clone());
        let saved = state.store.save();
        // Like `save_import_recipe`: without a successful write, keep the old
        // reference so that the imported template stays "modified" (unsaved).
        if saved.is_ok() {
            state.baseline = recipe.clone();
        }
        state.recipe = recipe;
        state.save_as_name = name.clone();
        match saved {
            Ok(()) => self.notice(tr!("status-template-imported", name = name)),
            Err(err) => self.fail(tr!(
                "error-template-import-not-saved",
                cause = errors::io::error_message(&err)
            )),
        }
    }

    /// Removes file `index` from the wizard's list.
    pub fn remove_import_path(&mut self, index: usize) {
        if let Some(state) = &mut self.import_state {
            if index < state.paths.len() {
                let path = state.paths.remove(index);
                state.bytes.borrow_mut().remove(&path);
                state.preview_index = state.preview_index.min(state.paths.len().saturating_sub(1));
            }
        }
    }

    /// Chooses the file shown in the preview.
    pub fn set_import_preview_index(&mut self, index: usize) {
        if let Some(state) = &mut self.import_state {
            if index < state.paths.len() {
                state.preview_index = index;
            }
        }
    }

    /// Moves to the next / previous step of the wizard.
    pub fn import_step_forward(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.step = state.step.next();
        }
    }

    /// See [`AppState::import_step_forward`].
    pub fn import_step_back(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.step = state.step.previous();
        }
    }

    /// Bytes of file `path`, read only once then kept: the preview
    /// is recomputed every frame, the disk must not be.
    fn import_bytes(&self, path: &std::path::Path) -> Result<Arc<Vec<u8>>, ImportReadError> {
        let state = self
            .import_state
            .as_ref()
            .ok_or(ImportReadError::NoImport)?;
        let mut cache = state.bytes.borrow_mut();
        cache
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                std::fs::read(path)
                    .map(Arc::new)
                    .map_err(|error| ImportReadError::Read {
                        path: path.to_path_buf(),
                        error: Arc::new(error),
                    })
            })
            .clone()
    }

    /// File shown in the preview.
    fn import_preview_path(&self) -> Option<PathBuf> {
        let state = self.import_state.as_ref()?;
        state
            .paths
            .get(state.preview_index)
            .or_else(|| state.paths.first())
            .cloned()
    }

    /// Encoding detected for the file shown in the preview.
    pub fn import_encoding(&self) -> Option<&'static str> {
        let path = self.import_preview_path()?;
        let bytes = self.import_bytes(&path).ok()?;
        stokesforge_import::detect_encoding(&bytes)
    }

    /// What the import would give for each chosen file, with the current
    /// recipe: `Ok` with the number of rows, or the error that awaits it —
    /// shown **before** the click on Import.
    pub fn import_file_statuses(&self) -> Vec<(PathBuf, Result<usize, ImportReadError>)> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        state
            .paths
            .iter()
            .map(|path| {
                let status = self.import_bytes(path).and_then(|bytes| {
                    stokesforge_import::parse(&bytes, &state.recipe)
                        .map(|table| table.row_count())
                        .map_err(ImportReadError::Parse)
                });
                (path.clone(), status)
            })
            .collect()
    }

    /// The current template differs from the one that was loaded or saved.
    pub fn import_recipe_modified(&self) -> bool {
        self.import_state
            .as_ref()
            .is_some_and(|state| state.recipe != state.baseline)
    }

    /// Requests the deletion of template `name` (it takes place only on
    /// confirmation, [`AppState::confirm_delete_import_recipe`]).
    pub fn request_delete_import_recipe(&mut self, name: &str) {
        if let Some(state) = &mut self.import_state {
            state.confirm_delete = Some(name.to_string());
        }
    }

    /// Cancels the deletion request.
    pub fn cancel_delete_import_recipe(&mut self) {
        if let Some(state) = &mut self.import_state {
            state.confirm_delete = None;
        }
    }

    /// Deletes the template whose deletion was requested.
    pub fn confirm_delete_import_recipe(&mut self) {
        let name = self
            .import_state
            .as_mut()
            .and_then(|state| state.confirm_delete.take());
        if let Some(name) = name {
            self.delete_import_recipe(&name);
        }
    }

    /// Columns of the import result of the file shown, with their first
    /// values — "Columns" step.
    pub fn import_column_previews(
        &self,
    ) -> Option<Result<Vec<ImportColumnPreview>, ImportReadError>> {
        let table = self.import_preview()?;
        Some(table.map(|table| {
            table
                .columns
                .iter()
                .map(|column| ImportColumnPreview {
                    name: column.name.clone().unwrap_or_default(),
                    long_name: column.long_name.clone().unwrap_or_default(),
                    unit: column.unit.clone().unwrap_or_default(),
                    mixed: column.texts.iter().any(Option::is_some),
                    samples: column
                        .values
                        .iter()
                        .zip(&column.texts)
                        .take(3)
                        .map(|(value, text)| match text {
                            Some(text) => text.clone(),
                            None if value.is_nan() => String::new(),
                            None => format!("{value}"),
                        })
                        .collect(),
                })
                .collect()
        }))
    }

    /// All the metadata the file shown could provide (header
    /// and footer, without exclusion) — "Metadata" step, to tick
    /// what is kept.
    pub fn import_available_metadata(&self) -> Vec<stokesforge_import::MetadataLine> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        let Some(path) = self.import_preview_path() else {
            return Vec::new();
        };
        let Ok(bytes) = self.import_bytes(&path) else {
            return Vec::new();
        };
        let mut recipe = state.recipe.clone();
        recipe.keep_header_metadata = true;
        recipe.keep_footer_metadata = true;
        recipe.excluded_metadata.clear();
        stokesforge_import::parse(&bytes, &recipe)
            .map(|table| table.metadata().to_vec())
            .unwrap_or_default()
    }

    /// Setting from a right-click on line `number` (1-based, number in the
    /// file) of the preview. Leading blank lines do not count in the header
    /// (as at import): the offset comes from the first line shown.
    pub fn import_line_action(&mut self, action: ImportLineAction, number: usize) {
        let Some(Ok(preview)) = self.import_line_preview() else {
            return;
        };
        let Some(first) = preview.lines.first().map(|line| line.number) else {
            return;
        };
        let Some(state) = &mut self.import_state else {
            return;
        };
        let recipe = &mut state.recipe;
        let header_total =
            recipe.header_layout.main_header_lines + recipe.header_layout.subheader_lines;
        let data_start = first + header_total + usize::from(recipe.has_column_header_row);
        let offset = number.saturating_sub(first);
        match action {
            ImportLineAction::DataStartsHere => {
                recipe.header = stokesforge_import::HeaderSkip::None;
                let sub = recipe.header_layout.subheader_lines.min(offset);
                recipe.header_layout.subheader_lines = sub;
                recipe.header_layout.main_header_lines = offset - sub;
                recipe.header_layout.auto_detect = false;
            }
            ImportLineAction::UnitsLine => {
                // The line and those after it, up to the data, form the
                // subheader; the line gives the units.
                recipe.header = stokesforge_import::HeaderSkip::None;
                let total = header_total.max(offset + 1);
                recipe.header_layout.main_header_lines = offset;
                recipe.header_layout.subheader_lines = total - offset;
                recipe.header_layout.auto_detect = false;
                recipe.subheader_roles.same_line_name_unit = false;
                recipe.subheader_roles.unit =
                    Some(stokesforge_import::SubheaderLineRange::single(1));
            }
            ImportLineAction::DataEndsHere => {
                if number >= data_start {
                    recipe.data_end =
                        stokesforge_import::DataEnd::LinesAfterHeader(number - data_start);
                }
            }
            ImportLineAction::NoAutoCut => recipe.data_end = stokesforge_import::DataEnd::None,
        }
    }

    /// Saves the recipe being edited as a template named `save_as_name` (text
    /// field of the window) in the `RecipeStore`, and persists the store to
    /// disk. A write failure (config folder unreachable) becomes a
    /// [`StatusMessage`], never a panic. No effect if the window is not open
    /// or the name is empty.
    pub fn save_import_recipe(&mut self) {
        let Some(state) = &mut self.import_state else {
            return;
        };
        let name = state.save_as_name.trim().to_string();
        if name.is_empty() {
            self.fail(tr!("error-template-name-required"));
            return;
        }
        state.recipe.name = name;
        state.store.upsert(state.recipe.clone());
        let save_result = state.store.save();
        let saved_name = state.recipe.name.clone();
        if let Err(err) = save_result {
            self.fail(tr!(
                "error-template-save",
                cause = errors::io::error_message(&err)
            ));
            return;
        }
        // The "modified" witness compares with the last successful save, not
        // only with the template loaded at opening. On failure, keep the old
        // reference so as not to announce a save.
        state.baseline = state.recipe.clone();
        self.notice(tr!("status-template-saved", name = saved_name));
    }

    /// Deletes the template `name` from the `RecipeStore` and persists the
    /// store to disk. Same failure policy as [`AppState::save_import_recipe`].
    pub fn delete_import_recipe(&mut self, name: &str) {
        let Some(state) = &mut self.import_state else {
            return;
        };
        state.store.remove(name);
        if let Err(err) = state.store.save() {
            self.fail(tr!(
                "error-template-delete",
                cause = errors::io::error_message(&err)
            ));
            return;
        }
        self.notice(tr!("status-template-deleted", name = name));
    }

    /// Preview of the **first** chosen file, reparsed with the current
    /// settings. Recomputed at each call rather than cached (preview files are
    /// small, see the head doc of [`ImportState`]). `None` if the window is
    /// not open or no file has been chosen yet; `Some(Err(_))` if reading or
    /// parsing the file failed (an unreadable file is treated as a parse
    /// error so as not to multiply the failure types exposed to the window).
    pub fn import_preview(
        &self,
    ) -> Option<Result<stokesforge_import::ParsedTable, ImportReadError>> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        let result = self.import_bytes(&path).and_then(|bytes| {
            stokesforge_import::parse(&bytes, state.preview_recipe())
                .map_err(ImportReadError::Parse)
        });
        Some(result)
    }

    /// Preview of the split into lines (header/subheader/data/footer) of the
    /// **first** chosen file, with the current settings. See
    /// [`stokesforge_import::preview::preview`] for the cost budget.
    /// Recomputed at each call, like [`AppState::import_preview`]: this
    /// budget is exactly what makes recomputing at every frame acceptable,
    /// even on a big file (the cell-by-cell work stays bounded by
    /// `recipe.preview_lines`, never by the file size). `None` under the same
    /// conditions as `import_preview`.
    pub fn import_line_preview(
        &self,
    ) -> Option<Result<stokesforge_import::FilePreview, ImportReadError>> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        let result = self.import_bytes(&path).and_then(|bytes| {
            stokesforge_import::preview(&bytes, state.preview_recipe())
                .map_err(ImportReadError::Parse)
        });
        Some(result)
    }

    /// Re-evaluates `header_layout` (number of main header and subheader
    /// lines) from the content of the **first** chosen file. Equivalent of
    /// OriginLab's automatic re-evaluation button
    /// ([`stokesforge_import::preview::detect_header_layout`]): an explicit
    /// action, not a recomputation at every frame, so as never to overwrite a
    /// value the user has just typed by hand. No effect if the window is not
    /// open or no file has been chosen yet.
    pub fn auto_detect_import_header(&mut self) {
        let Some(import) = &self.import_state else {
            return;
        };
        let Some(path) = self.import_preview_path() else {
            self.fail(tr!("error-import-choose-file-first"));
            return;
        };
        let (separator, decimal) = (import.recipe.separator.clone(), import.recipe.decimal);
        match self.import_bytes(&path).map(|bytes| bytes.as_ref().clone()) {
            Ok(bytes) => {
                let layout = stokesforge_import::detect_header_layout(&bytes, separator, decimal);
                if let Some(import) = &mut self.import_state {
                    import.recipe.header_layout = layout;
                }
            }
            Err(err) => self.fail(errors::import::read_message(&err)),
        }
    }

    // -- internal detail ---------------------------------------------------

    /// Purges the sheet selections and the input buffers that have become
    /// stale: a workbook, sheet or column that no longer exists in the new
    /// tree (reload, deletion, including by another actor client).
    ///
    /// Unlike the old single global selection, `selected_sheets` does not
    /// need to be **reassigned** here: [`AppState::selected_sheet_in`] falls
    /// back by itself to the first sheet of a workbook whose entry has gone
    /// (or never existed). This method therefore only discards what no longer
    /// points to anything valid, so as not to let the map grow with dead
    /// entries over the reloads.
    fn revalidate_selection(&mut self) {
        let tree = &self.snapshot.tree;
        self.selected_sheets.retain(|&book, &mut sheet| {
            tree.node(book)
                .is_some_and(|node| matches!(node.kind(), NodeKind::Workbook { .. }))
                && tree
                    .node(sheet)
                    .is_some_and(|node| node.kind().is_worksheet() && node.parent() == Some(book))
        });
        // The input buffer points to a column of the previous tree: if it has
        // no column node any more, it makes no sense here (typically a reload:
        // the identifiers have nothing to do with the earlier ones). If it
        // still points to a valid column, it is left alone: that is the case
        // of a `refresh` triggered by an action that does not touch the column
        // being typed in.
        if let Some(edit) = &self.editing {
            if tree.node(edit.column).and_then(Node::as_column).is_none() {
                self.editing = None;
            }
        }
        if let Some(edit) = &self.note_editing {
            if !matches!(
                tree.node(edit.note).map(Node::kind),
                Some(NodeKind::Note { .. })
            ) {
                self.note_editing = None;
            }
        }
        // A selection whose columns no longer exist (sheet or columns
        // deleted) makes no sense any more; see `Selection::columns`, which
        // returns an empty list in that case. A selected header field, on the
        // other hand, *never* has a column via `columns()` (see its doc): its
        // column is checked directly.
        self.selection.retain(
            |&sheet, selection| match selection.selected_header_field() {
                Some((column, _)) => tree.node(column).and_then(Node::as_column).is_some(),
                None => !selection.columns(tree, sheet).is_empty(),
            },
        );
        if let Some(edit) = &self.header_editing {
            if tree.node(edit.column).and_then(Node::as_column).is_none() {
                self.header_editing = None;
            }
        }
    }

    /// Records an informational message. `pub(crate)` so that the window
    /// layer can report what only it knows: an unreadable system clipboard
    /// at the time of a "Paste" taken from a context menu (see
    /// `app::grid::system_clipboard_text`). This failure arises on the
    /// platform side, never in the model.
    pub fn notice(&mut self, text: String) {
        self.dialog_error = None;
        self.record(text, StatusLevel::Info);
    }

    /// Records a warning (S-M4): visible until the next message.
    pub fn warn(&mut self, text: String) {
        self.dialog_error = None;
        self.record(text, StatusLevel::Warning);
    }

    /// Records a failure message: in the footer of the open modal dialog
    /// (S-M7, #228), in the status bar otherwise.
    fn fail(&mut self, text: String) {
        if self.modal_dialog_open() {
            self.dialog_error = Some(text);
            return;
        }
        self.record(text, StatusLevel::Error);
    }

    /// Sets the current message and adds it to the history (S-M6, last 50).
    fn record(&mut self, text: String, level: StatusLevel) {
        let message = StatusMessage {
            text,
            level,
            at: std::time::Instant::now(),
            wall: std::time::SystemTime::now(),
        };
        if self.status_log.len() == STATUS_HISTORY_LEN {
            self.status_log.pop_front();
        }
        self.status_log.push_back(message.clone());
        self.status = Some(message);
    }

    /// Clears the information that is too old (S-M5) and returns the delay
    /// left before the next deadline, to schedule **that one** repaint: no
    /// periodic wake-up at rest. Warnings and errors stay.
    pub fn expire_status(&mut self, now: std::time::Instant) -> Option<std::time::Duration> {
        let message = self.status.as_ref()?;
        if message.level != StatusLevel::Info {
            return None;
        }
        let age = now.saturating_duration_since(message.at);
        if age >= INFO_LIFETIME {
            self.status = None;
            None
        } else {
            Some(INFO_LIFETIME - age)
        }
    }

    /// The latest messages, from oldest to newest (S-M6).
    pub fn status_history(&self) -> impl Iterator<Item = &StatusMessage> {
        self.status_log.iter()
    }

    /// Is the message history open?
    pub fn status_history_open(&self) -> bool {
        self.status_history_open
    }

    /// Is the Preferences window open?
    pub fn preferences_open(&self) -> bool {
        self.preferences_open
    }

    /// Is the keyboard shortcuts window open?
    pub fn keyboard_shortcuts_open(&self) -> bool {
        self.keyboard_shortcuts_open
    }

    /// Window title: project name, "•" while it is not saved (S-R15):
    /// "essai.stf • — StokesForge".
    pub fn window_title(&self) -> String {
        let name = if self.path.is_empty() {
            tr!("window-title-untitled")
        } else {
            std::path::Path::new(&self.path)
                .file_name()
                .map_or_else(|| self.path.clone(), |n| n.to_string_lossy().into_owned())
        };
        if self.has_unsaved_work(false) {
            tr!("window-title-project-unsaved", name = name)
        } else {
            tr!("window-title-project", name = name)
        }
    }

    /// Did the last gesture fail? Error in the dialog footer or in the status
    /// bar.
    pub(crate) fn last_action_failed(&self) -> bool {
        // The low R² warning (#288) is not a failure: the gesture that opened
        // it succeeded.
        self.dialog_error.is_some()
            || self
                .alert
                .as_ref()
                .is_some_and(|alert| alert.kind != AlertKind::LowRSquared)
            || self.status.as_ref().is_some_and(StatusMessage::is_error)
    }

    /// Sends a command to the actor and reports the result.
    fn execute(&mut self, command: Command) {
        // The label is computed on the tree from **before**: afterwards, a
        // deleted column has no name.
        let label = command.label(&self.snapshot.tree);
        // Interruptible (#239): the command is applied without waiting for the
        // formula recalculation, which Stop can halt.
        match self.handle.execute_interruptible(command) {
            Ok(revision) => {
                self.applied_revision = self.applied_revision.max(revision);
                self.notice(history_text(&label));
                self.settle_recalc();
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    /// Waits [`RECALC_WAIT`] for the end of the recalculation that the command
    /// has just started: a short recalculation stays invisible and the
    /// snapshot is fresh on return, as before #239. Beyond that, hand back
    /// control (the interface repaints with its indicator) and
    /// [`AppState::poll_recalc`] will resume.
    fn settle_recalc(&mut self) {
        if self.handle.wait_recalc(RECALC_WAIT) {
            self.finish_recalc();
        } else {
            self.recalc_since = Some(std::time::Instant::now() - RECALC_WAIT);
        }
    }

    /// The recalculation is done: reads how, refreshes, and says whether Stop
    /// undid the command.
    fn finish_recalc(&mut self) {
        self.recalc_since = None;
        let end = self.handle.take_recalc_end();
        self.refresh();
        if end == Some(RecalcEnd::Stopped) {
            self.notice(tr!("status-recalc-stopped"));
        }
    }

    /// To call at every frame: resumes when a recalculation that the
    /// interface did not wait for has just finished. Never blocks.
    pub fn poll_recalc(&mut self) {
        self.check_session();
        if self.recalc_since.is_some() && self.handle.recalc_running().is_none() {
            self.finish_recalc();
        }
    }

    /// Has the session actor disappeared (panic, #310)? In that case, **once
    /// only**: the "running" recalculation is abandoned, a backup copy of the
    /// last snapshot is written (if a folder is known) and a blocking dialog
    /// announces it. Nothing goes through the actor afterwards: closing is the
    /// only way out.
    fn check_session(&mut self) {
        if self.session_lost || self.handle.actor_alive() {
            return;
        }
        self.session_lost = true;
        self.recalc_since = None;
        self.unsaved_pending = None;
        self.after_save = None;
        let detail = match self.recovery_dir.clone() {
            None => String::new(),
            Some(dir) => match crate::recovery::write_copy(&dir, &self.path, &self.snapshot) {
                Ok(path) => tr!(
                    "alert-session-lost-recovered",
                    path = path.display().to_string()
                ),
                Err(err) => tr!(
                    "alert-session-lost-no-recovery",
                    detail = match &err {
                        crate::recovery::RecoveryError::CreateDir(io) => {
                            errors::io::error_message(io)
                        }
                        crate::recovery::RecoveryError::Save(save) => {
                            errors::file::error_message(save)
                        }
                    }
                ),
            },
        };
        self.alert = Some(Alert {
            kind: AlertKind::SessionLost,
            message: detail,
        });
    }

    /// Sets the backup copies folder (`None`: none). By default the
    /// application data folder; a test or an integrator replaces it so as to
    /// write nothing outside its temporary folder.
    pub fn set_recovery_dir(&mut self, dir: Option<PathBuf>) {
        self.recovery_dir = dir;
    }

    /// Quit without asking to save (demo forced by `STOKESFORGE_DEMO=1`, see
    /// [`crate::startup`]): the binary turns it on, nothing else. The backup
    /// copy goes away as for a Discard.
    pub fn set_quit_without_prompt(&mut self, on: bool) {
        self.quit_without_prompt = on;
    }

    /// Is the session actor dead (#310)? The window then has only to close.
    pub fn session_lost(&self) -> bool {
        self.session_lost
    }

    /// The running recalculation seen as work to show (#239): how long it has
    /// lasted.
    pub fn recalc_work(&self) -> Option<std::time::Duration> {
        let since = self.recalc_since?;
        self.handle.recalc_running()?;
        Some(since.elapsed())
    }

    /// Stop of the recalculation (#239, S-P2): it halts between two columns,
    /// the command that triggered it is undone, what had changed is
    /// recalculated. The message comes at the end ([`AppState::finish_recalc`]).
    pub fn stop_recalc(&mut self) {
        if let Some(id) = self.handle.recalc_running() {
            self.handle.stop_recalc(id);
        }
    }

    pub(super) fn undo(&mut self) {
        // The label disappears from the stack as soon as the undo is done: it
        // must be read before.
        let label = self.snapshot.history.undo_label.clone();
        match self.handle.undo() {
            Ok(Some(_)) => {
                self.notice(match label {
                    Some(label) => tr!("history-undone", label = history_text(&label)),
                    None => tr!("history-undone-last"),
                });
                self.refresh();
            }
            Ok(None) => self.warn(tr!("status-nothing-to-undo")),
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    fn redo(&mut self) {
        let label = self.snapshot.history.redo_label.clone();
        match self.handle.redo() {
            Ok(Some(_)) => {
                self.notice(match label {
                    Some(label) => tr!("history-redone", label = history_text(&label)),
                    None => tr!("history-redone-last"),
                });
                self.refresh();
            }
            Ok(None) => self.warn(tr!("status-nothing-to-redo")),
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    /// Saves; then carries on with the gesture that was waiting for the save
    /// (Save answer, S-F4). A failure is a serious failure (S-M8): blocking
    /// dialog, and the pending gesture is abandoned.
    fn save(&mut self) -> bool {
        // A script writes into the model from its thread: saving now would
        // freeze an intermediate state (#315). The button is disabled; this
        // refusal is the guard of the state.
        if self.script_running() {
            self.after_save = None;
            self.fail(tr!("error-save-script-running"));
            return false;
        }
        // Overwriting the source file of a read that discarded data would
        // delete it from disk: ask first (#309, F3). Waiting is not a
        // failure; `after_save` stays pending the answer.
        if self
            .discarded_source
            .as_ref()
            .is_some_and(|source| source.path == Path::new(&self.path))
        {
            self.pending_discarded_save = Some(PendingDiscardedSave { restore_path: None });
            return false;
        }
        self.write_save()
    }

    /// The write of [`AppState::save`], with no further question.
    fn write_save(&mut self) -> bool {
        let path = PathBuf::from(&self.path);
        match self.handle.save(&path) {
            Ok(()) => {
                self.refresh();
                self.saved_revision =
                    Some((self.snapshot.revision, self.snapshot.geometry_revision));
                // Saved: the backup copy no longer has a reason to exist.
                self.autosave_remove_slot();
                self.autosave_mark_clean();
                if self
                    .discarded_source
                    .as_ref()
                    .is_some_and(|source| source.path == path)
                {
                    self.discarded_source = None;
                }
                // A2 (#316): a file bigger than the opening cap can be saved,
                // but will not reopen as is: say so, and how to raise the
                // limit.
                let limit = self.object_manager_preferences.max_file_bytes();
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if size > limit {
                    let size_mb = i64::try_from(size.div_ceil(1 << 20)).unwrap_or(i64::MAX);
                    let limit_mb = i64::from(self.object_manager_preferences.max_file_mb());
                    let text = crate::tr!(
                        "status-saved-over-limit",
                        path = self.path.as_str(),
                        size = size_mb,
                        limit = limit_mb
                    );
                    self.warn(text);
                } else {
                    let text = crate::tr!("status-saved", path = self.path.as_str());
                    self.notice(text);
                }
                if let Some(gesture) = self.after_save.take() {
                    self.perform_unsaved(gesture);
                }
                true
            }
            Err(err) => {
                self.after_save = None;
                self.alert = Some(Alert {
                    kind: AlertKind::SaveFailed,
                    message: errors::session::error_message(&err),
                });
                false
            }
        }
    }

    /// Saves as `path`. The path is replaced only if the save succeeds (model
    /// of [`AppState::open_project`]); it is restored **after** `save`, whose
    /// pending gesture may change the project.
    fn save_as(&mut self, path: String) {
        let previous = std::mem::replace(&mut self.path, path);
        if !self.save() {
            if let Some(pending) = self.pending_discarded_save.as_mut() {
                // To the source: the path is restored only if the answer is
                // Cancel or if the write fails.
                pending.restore_path = Some(previous);
            } else {
                self.path = previous;
            }
        }
    }

    /// Voir [`Action::ConfirmDiscardedSave`].
    fn confirm_discarded_save(&mut self, accept: bool) {
        let Some(pending) = self.pending_discarded_save.take() else {
            return;
        };
        let saved = accept && self.write_save();
        if !saved {
            if let Some(previous) = pending.restore_path {
                self.path = previous;
            }
            if !accept {
                self.after_save = None;
            }
        }
    }

    /// The file that saving would overwrite without what its read discarded,
    /// with what was discarded ("2 calculations and 1 correction record"),
    /// waiting for confirmation (#309, F3).
    pub fn pending_discarded_save(&self) -> Option<(String, String)> {
        self.pending_discarded_save.as_ref()?;
        let source = self.discarded_source.as_ref()?;
        let what = discarded_what(source.calculations, source.corrections)?;
        let name = source.path.file_name().map_or_else(
            || source.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Some((name, what))
    }

    /// Applies `change` to a clone of the spec of `graph` and sends it to the
    /// model ([`Command::SetPlotSpec`]) if it really changed something.
    fn edit_annotations(&mut self, graph: NodeId, change: impl FnOnce(&mut PlotSpec) -> bool) {
        let Some(mut spec) = self.plot_spec(graph) else {
            return;
        };
        let before = spec.clone();
        if change(&mut spec) && spec != before {
            self.execute(Command::SetPlotSpec { graph, spec });
        }
    }

    /// Same delta as the properties; the reason specifies the gesture in the history.
    fn edit_plot_object(
        &mut self,
        graph: NodeId,
        kind: PlotEditKind,
        change: impl FnOnce(&mut PlotSpec) -> bool,
    ) {
        let Some(mut spec) = self.plot_spec(graph) else {
            return;
        };
        let before = spec.clone();
        if change(&mut spec) && spec != before {
            self.execute(Command::EditPlotSpec { graph, spec, kind });
        }
    }

    /// Cap on the size of a `.stf` at opening, in MB (A2, #316).
    pub fn max_file_mb(&self) -> u32 {
        self.object_manager_preferences.max_file_mb()
    }

    /// Global budget of cells at opening, in millions (B1, #316).
    pub fn max_cells_m(&self) -> u32 {
        self.object_manager_preferences.max_cells_m()
    }

    /// Is the "Low R²" box allowed by the application (#288)?
    pub fn analysis_low_r2_dialog(&self) -> bool {
        self.object_manager_preferences.analysis_low_r2_dialog()
    }

    /// The header rows `(Metadata, x')` of new workbooks.
    pub fn workbook_header_defaults(&self) -> (bool, bool) {
        self.object_manager_preferences.workbook_header_rows()
    }

    /// How the text of SVG exports is written (#162).
    pub fn copy_formats(&self) -> &[crate::preferences::CopyFormatEntry] {
        self.object_manager_preferences.copy_formats()
    }

    pub fn svg_text_mode(&self) -> stokesforge_render_svg::SvgTextMode {
        self.object_manager_preferences.svg_text_mode()
    }

    /// The scale-up of PNG exports.
    pub fn png_export_scale(&self) -> f32 {
        self.png_export_scale
    }

    /// See [`Action::ExportGraph`]. Measures and composes like the screen:
    /// same logical size (`DEFAULT_SIZE`, the one where
    /// `GraphView::paint_scaled` lays out the figure), same fonts.
    ///
    /// The native picker asks before overwriting the typed name; the extension
    /// added here gives another file, which it has not seen: if it exists, the
    /// export waits for confirmation (S-F3, [`Action::ConfirmOverwrite`]).
    fn export_graph(&mut self, mut path: PathBuf, spec: &PlotSpec) {
        if path.extension().is_none() {
            path.set_extension("svg");
            if path.exists() {
                self.pending_overwrite = Some((path, Box::new(spec.clone())));
                return;
            }
        }
        self.write_graph_export(&path, spec);
    }

    /// The write of the export, with no further question.
    fn write_graph_export(&mut self, path: &std::path::Path, spec: &PlotSpec) {
        let Some(format) = stokesforge_render_svg::ExportFormat::from_path(
            path,
            self.png_export_scale,
            self.svg_text_mode(),
        ) else {
            self.fail(tr!(
                "error-export-unknown-format",
                path = path.display().to_string()
            ));
            return;
        };
        let plot = stokesforge_core::resolve::resolve_spec(&self.snapshot.tree, spec);
        let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
        match stokesforge_render_svg::export_to_file_with_texts(
            path,
            &plot,
            stokesforge_render_svg::DEFAULT_SIZE,
            &measurer,
            &stokesforge_render_svg::FontChoice::default(),
            format,
            &crate::figure_texts::figure_texts(),
        ) {
            Ok(()) => self.notice(tr!(
                "status-graph-exported",
                path = path.display().to_string()
            )),
            Err(err) => self.fail(errors::export::error_message(&err)),
        }
    }

    /// See [`Action::CopyGraphFigure`]. Same size, measurement and fonts as
    /// the export to a file; the figure is composed only once for the three
    /// formats. A format that fails alone is omitted and named in the
    /// message; if none is ready, nothing is offered.
    fn copy_graph_figure(&mut self, spec: &PlotSpec) {
        use crate::preferences::CopyFormat;
        use stokesforge_render_svg::ExportFormat;

        let plot = stokesforge_core::resolve::resolve_spec(&self.snapshot.tree, spec);
        let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
        // The order and the choice are those of Preferences › Export: the
        // order of the representations is the one in which applications see
        // them.
        let wanted: Vec<(&str, ExportFormat)> = self
            .copy_formats()
            .iter()
            .filter(|entry| entry.on)
            .map(|entry| match entry.format {
                // The clipboard always takes the default, SVG text (#162).
                CopyFormat::Svg => (
                    "SVG",
                    ExportFormat::Svg {
                        text: stokesforge_render_svg::SvgTextMode::Text,
                    },
                ),
                CopyFormat::Png => (
                    "PNG",
                    ExportFormat::Png {
                        scale: self.png_export_scale,
                    },
                ),
                CopyFormat::Pdf => ("PDF", ExportFormat::Pdf),
            })
            .collect();
        let formats: Vec<ExportFormat> = wanted.iter().map(|(_, format)| *format).collect();
        let results = match stokesforge_render_svg::export_bytes_many_with_texts(
            &plot,
            stokesforge_render_svg::DEFAULT_SIZE,
            &measurer,
            &stokesforge_render_svg::FontChoice::default(),
            &formats,
            &crate::figure_texts::figure_texts(),
        ) {
            Ok(results) => results,
            Err(err) => {
                self.fail(errors::export::error_message(&err));
                return;
            }
        };
        let mut figures = Vec::new();
        let mut offered = Vec::new();
        let mut missing = Vec::new();
        let mut first_error = None;
        for ((name, format), result) in wanted.iter().zip(results) {
            match result {
                Ok(bytes) => {
                    figures.push(FigureBytes {
                        format: *format,
                        bytes,
                    });
                    offered.push(*name);
                }
                Err(err) => {
                    missing.push(*name);
                    first_error.get_or_insert(err);
                }
            }
        }
        if let Some(err) = first_error.filter(|_| figures.is_empty()) {
            self.fail(errors::export::error_message(&err));
            return;
        }
        self.figure_clipboard = Some(figures);
        let text = if missing.is_empty() {
            tr!("status-graph-copied", formats = offered.join(", "))
        } else {
            tr!(
                "status-graph-copied-partly",
                formats = offered.join(", "),
                missing = missing.join(", ")
            )
        };
        self.notice(text);
    }

    /// The shapes of the figure that [`Action::CopyGraphFigure`] has just
    /// prepared, to offer to the system clipboard by the window that received
    /// the gesture; read only once. Bytes labeled with their
    /// [`ExportFormat`](stokesforge_render_svg::ExportFormat): the MIME type
    /// and the offer to the system are the window's business
    /// (`figure_copy.rs`), which alone knows `stokesforge-clipboard`.
    pub fn take_figure_clipboard(&mut self) -> Option<Vec<FigureBytes>> {
        self.figure_clipboard.take()
    }

    /// The system refused the offer of the figure: the "copied" message gives
    /// way to the cause, which the window has already put into a sentence.
    pub fn figure_clipboard_failed(&mut self, message: String) {
        self.fail(message);
    }

    /// Reloads the current file. A read failure is a serious failure (S-M8):
    /// blocking dialog; returns `false`.
    fn load(&mut self) -> bool {
        let path = PathBuf::from(&self.path);
        let limits = self.object_manager_preferences.load_limits();
        match self.handle.load_with_limits(&path, limits) {
            Ok((_, report)) => {
                // The identifiers of the file have nothing to do with those of
                // the previous tree: the selection must be revalidated, which
                // `refresh` does.
                self.editing = None;
                let text = crate::tr!("status-loaded", path = self.path.as_str());
                self.notice(text);
                // An unreadable calculation is discarded (invariant 10) but
                // reported: the message replaces "Loaded" in the status bar (#278).
                if let Some(text) = discarded_calculations_text(&report) {
                    self.warn(text);
                }
                self.applied_revision = 0;
                self.refresh();
                self.saved_revision =
                    Some((self.snapshot.revision, self.snapshot.geometry_revision));
                // The old project is closed cleanly: its copy goes away.
                self.autosave_remove_slot();
                self.autosave_mark_clean();
                self.discarded_source = DiscardedSource::of(PathBuf::from(&self.path), &report);
                true
            }
            Err(err) => {
                self.alert = Some(Alert {
                    kind: AlertKind::OpenFailed,
                    message: errors::session::error_message(&err),
                });
                false
            }
        }
    }

    /// Opens `path`; an unreadable file leaves the current project **and**
    /// path (a following Ctrl+S does not overwrite the chosen file).
    fn open_project(&mut self, path: String) {
        let previous = std::mem::replace(&mut self.path, path);
        if !self.load() {
            self.path = previous;
        }
    }

    /// Changing project must not pick up an import or script result launched
    /// on the old project, nor interrupt a macro.
    pub fn can_new_project(&self) -> bool {
        !self.script_running() && self.import_job.is_none() && !self.snapshot.history.in_macro
    }

    /// The gesture waiting for the "Discard Changes · Cancel · Save" answer
    /// (S-F4, #229).
    pub fn unsaved_pending(&self) -> Option<&UnsavedGesture> {
        self.unsaved_pending.as_ref()
    }

    /// The serious failure to report (S-M8, #229).
    pub fn alert(&self) -> Option<&Alert> {
        self.alert.as_ref()
    }

    /// The file an export would overwrite, waiting for confirmation (S-F3).
    pub fn pending_overwrite(&self) -> Option<&std::path::Path> {
        self.pending_overwrite
            .as_ref()
            .map(|(path, _)| path.as_path())
    }

    /// Is a blocking dialog of the application waiting for an answer
    /// (confirmation, axis choice, serious failure)? Nothing else acts then.
    pub fn blocking_dialog(&self) -> bool {
        self.unsaved_pending.is_some()
            || self.pending_row_drop.is_some()
            || self.alert.is_some()
            || self.pending_overwrite.is_some()
            || self.pending_discarded_save.is_some()
            || self.recovery_delete_pending()
    }

    /// Nothing holds the application back any more (Quit accepted); once only.
    pub fn take_quit_ready(&mut self) -> bool {
        std::mem::take(&mut self.quit_ready)
    }

    /// A gesture that would replace or close the project (S-F3): with no work
    /// to lose it applies, otherwise it waits for the "Discard Changes ·
    /// Cancel · Save" answer.
    fn request_unsaved(&mut self, gesture: UnsavedGesture, pending_edits: bool) {
        self.refresh();
        if gesture == UnsavedGesture::NewProject && !self.can_new_project() {
            return;
        }
        if self.has_unsaved_work(pending_edits) {
            self.unsaved_pending = Some(gesture);
        } else {
            self.perform_unsaved(gesture);
        }
    }

    /// See [`Action::ConfirmUnsaved`]. Save with no file: the gesture waits for
    /// the chosen file (the window opens the picker, then `Action::Save`);
    /// Cancel also forgets a gesture that was waiting for a save.
    fn confirm_unsaved(&mut self, choice: UnsavedChoice) {
        let gesture = self.unsaved_pending.take();
        if choice == UnsavedChoice::Cancel {
            self.after_save = None;
            return;
        }
        let Some(gesture) = gesture else {
            return;
        };
        match choice {
            UnsavedChoice::Discard => self.perform_unsaved(gesture),
            UnsavedChoice::Save => {
                self.after_save = Some(gesture);
                if !self.path.is_empty() {
                    self.save();
                }
            }
            UnsavedChoice::Cancel => {}
        }
    }

    fn perform_unsaved(&mut self, gesture: UnsavedGesture) {
        match gesture {
            UnsavedGesture::NewProject => self.new_project(),
            UnsavedGesture::Open(path) => self.open_project(path),
            UnsavedGesture::Quit => {
                // Clean close: the backup copy goes away.
                self.autosave_remove_slot();
                self.quit_ready = true;
            }
            UnsavedGesture::OpenRecovery(path) => self.open_recovery(path),
        }
    }

    /// Would work be lost: changes since the save, or an input in progress
    /// (here or in a view, `pending_edits`)?
    ///
    /// The compared revision is the highest of the snapshot and
    /// `applied_revision`: while a long recalculation runs, the snapshot keeps
    /// the old revision although the command is already in the actor (#239).
    fn has_unsaved_work(&self, pending_edits: bool) -> bool {
        let revision = self.snapshot.revision.max(self.applied_revision);
        let has_content = self
            .snapshot
            .tree
            .node(self.snapshot.tree.root())
            .is_some_and(|node| !node.children().is_empty());
        let changed = self
            .saved_revision
            .map_or(has_content || revision != 0, |saved| {
                saved != (revision, self.snapshot.geometry_revision)
            });
        changed
            || pending_edits
            || self.editing.is_some()
            || self.note_editing.is_some()
            || self.header_editing.is_some()
    }

    fn new_project(&mut self) {
        if !self.can_new_project() {
            return;
        }
        match self.handle.new_project() {
            Ok(_) => {
                self.path.clear();
                self.saved_revision = None;
                self.discarded_source = None;
                self.selected_sheets.clear();
                self.selection.clear();
                self.editing = None;
                self.header_editing = None;
                self.note_editing = None;
                self.plot_edit = None;
                self.plot_settings = None;
                self.metadata_view = None;
                self.analysis = None;
                self.import_state = None;
                self.last_import = None;
                self.delete_repeat = None;
                self.applied_revision = 0;
                self.refresh();
                self.saved_revision =
                    Some((self.snapshot.revision, self.snapshot.geometry_revision));
                self.autosave_remove_slot();
                self.autosave_mark_clean();
                self.notice(tr!("action-new-project"));
            }
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
    }

    fn select_sheet(&mut self, id: NodeId) {
        let node = self.snapshot.tree.node(id);
        if !node.is_some_and(|node| node.kind().is_worksheet()) {
            self.fail(tr!("error-sheet-gone"));
            return;
        }
        // A sheet is always the direct child of a workbook (the only parent
        // `accepts_child` allows): it is that workbook, not the sheet itself,
        // that is the key of `selected_sheets`.
        let Some(book) = node.and_then(Node::parent) else {
            self.fail(tr!("error-sheet-no-workbook"));
            return;
        };
        self.selected_sheets.insert(book, id);
        self.editing = None;
        self.status = None;
    }

    /// Transport to publish by the viewport that receives the copy gesture.
    pub fn take_object_clipboard_text(&mut self) -> Option<String> {
        self.object_clipboard_output.take()
    }

    /// Copies created: source, destination and whether the view state is kept.
    pub fn take_copied_objects(&mut self) -> Vec<(NodeId, NodeId, bool)> {
        std::mem::take(&mut self.copied_objects)
    }

    /// **Remaining** graphs that use columns of the given subtrees (a graph
    /// deleted with them is not named).
    pub fn object_users(&self, ids: &[NodeId]) -> Vec<String> {
        let removed: std::collections::HashSet<_> = ids
            .iter()
            .flat_map(|id| self.tree().walk(*id).unwrap_or_default())
            .collect();
        self.tree()
            .walk_all()
            .into_iter()
            .filter(|id| !removed.contains(id))
            .filter_map(|id| self.tree().node(id))
            .filter(|node| match node.kind() {
                NodeKind::Graph { spec, .. } => removed.iter().any(|id| spec.references(*id)),
                _ => false,
            })
            .map(|node| node.name().to_string())
            .collect()
    }

    fn clipboard_origin(&self) -> String {
        format!("{}:{}", self.object_clipboard_origin, self.snapshot.epoch)
    }

    // Dropping duplicates and descendants of another target prevents deleting
    // or moving the same content twice. The order stays that of the tree.
    fn object_targets(&self, ids: &[NodeId]) -> Vec<NodeId> {
        self.tree()
            .walk_all()
            .into_iter()
            .filter(|id| ids.contains(id))
            .filter(|id| *id != self.tree().root())
            .filter(|id| {
                self.tree()
                    .node(*id)
                    .is_some_and(|n| n.kind().is_window() || n.kind().is_folder())
            })
            .filter(|id| {
                !self
                    .tree()
                    .ancestors(*id)
                    .unwrap_or_default()
                    .iter()
                    .any(|a| ids.contains(a))
            })
            .collect()
    }

    fn rename_object(&mut self, id: NodeId, name: &str) {
        if self.object_targets(&[id]).is_empty() {
            return;
        }
        let Some(node) = self.tree().node(id) else {
            return;
        };
        let name = name.trim();
        if node.name() == name {
            return;
        }
        if name.is_empty() {
            self.fail(tr!("object-rename-empty"));
            return;
        }
        self.execute(Command::Rename {
            id,
            name: name.into(),
        });
    }

    fn rename_objects(&mut self, changes: Vec<(NodeId, String)>) {
        let mut commands = Vec::new();
        for (id, name) in changes {
            let Some(node) = self.tree().node(id) else {
                continue;
            };
            if self.object_targets(&[id]).is_empty() {
                continue;
            }
            let name = name.trim();
            if name.is_empty() {
                self.fail(tr!("object-rename-empty"));
                return;
            }
            if name != node.name() {
                commands.push(Command::Rename {
                    id,
                    name: name.into(),
                });
            }
        }
        self.execute_object_group(MacroKind::RenameObjects, commands);
    }

    fn capture_objects(&self, ids: &[NodeId]) -> Vec<NodeCopy> {
        self.object_targets(ids)
            .into_iter()
            .filter_map(|id| NodeCopy::capture(self.tree(), id).ok())
            .collect()
    }

    /// Makes the given data workbooks visible and opens them.
    fn reveal_books(&mut self, books: Vec<NodeId>) {
        for book in books {
            let Some(NodeKind::Workbook { meta, .. }) = self.tree().node(book).map(Node::kind)
            else {
                continue;
            };
            let mut meta = meta.clone();
            if meta.hidden {
                meta.hidden = false;
                self.execute(Command::SetWindowMeta { id: book, meta });
            }
            self.copied_objects.push((book, book, false));
        }
    }

    /// Copying or duplicating a folder is refused for now (#170, point 4): the
    /// local copy would keep its references to the original workbooks, which
    /// are not in the folder. The gesture is explained in the status bar (the
    /// menu disables the entry with the same sentence). Returns true when the
    /// gesture is refused. To take up in a separate issue.
    fn refuse_folder_copy(&mut self, ids: &[NodeId]) -> bool {
        let has_folder = self.object_targets(ids).into_iter().any(|id| {
            self.tree()
                .node(id)
                .is_some_and(|node| node.kind().is_folder())
        });
        if has_folder {
            self.warn(tr!("disabled-copy-folder"));
        }
        has_folder
    }

    fn copy_objects(&mut self, ids: &[NodeId]) {
        if self.refuse_folder_copy(ids) {
            return;
        }
        let copies = self.capture_objects(ids);
        if copies.is_empty() {
            return;
        }
        match stokesforge_persistence::clipboard::encode_objects(&self.clipboard_origin(), &copies)
        {
            Ok(text) => self.object_clipboard_output = Some(text),
            Err(err) => self.fail(errors::file::error_message(&err)),
        }
    }

    fn next_object_copy_name(&self, parent: NodeId, origin: &str) -> String {
        (1u64..)
            .map(|number| {
                tr!(
                    "sheet-copy-name",
                    name = origin,
                    number = number.to_string()
                )
            })
            .find(|name| {
                self.tree()
                    .children(parent)
                    .unwrap_or_default()
                    .iter()
                    .all(|id| self.tree().node(*id).is_none_or(|node| node.name() != name))
            })
            .unwrap_or_else(|| origin.to_string())
    }

    fn duplicate_objects(&mut self, ids: &[NodeId]) {
        if self.refuse_folder_copy(ids) {
            return;
        }
        let copies = self.capture_objects(ids);
        if copies.is_empty() {
            return;
        }
        self.insert_object_copies(copies, None, true);
    }

    fn paste_objects(&mut self, parent: NodeId, text: &str) {
        let (origin, copies) = match stokesforge_persistence::clipboard::decode_objects(text) {
            Ok(value) => value,
            Err(_) => {
                self.fail(tr!("object-clipboard-invalid"));
                return;
            }
        };
        let copies = if origin != self.clipboard_origin() {
            copies.into_iter().map(NodeCopy::into_external).collect()
        } else {
            copies
        };
        self.insert_object_copies(copies, Some(parent), false);
    }

    fn insert_object_copies(
        &mut self,
        copies: Vec<NodeCopy>,
        parent: Option<NodeId>,
        duplicate: bool,
    ) {
        let kind = if duplicate {
            MacroKind::DuplicateObjects
        } else {
            MacroKind::PasteObjects
        };
        if self.handle.begin_macro(kind).is_err() {
            return;
        }
        let mut created = Vec::new();
        for copy in copies {
            let source = copy.root().id();
            let destination = parent
                .or(copy.root().parent())
                .unwrap_or(self.tree().root());
            let name = self.next_object_copy_name(destination, copy.root().name());
            let before = self
                .tree()
                .children(destination)
                .unwrap_or_default()
                .to_vec();
            let copy_kind = copy.root().kind().type_name();
            if let Err(err) = self.handle.execute(Command::InsertCopy {
                parent: destination,
                copy,
                name,
            }) {
                let _ = self.handle.abort_macro();
                self.refresh();
                self.fail(errors::session::error_message(&err));
                return;
            }
            self.refresh();
            if let Some(id) = self
                .tree()
                .children(destination)
                .unwrap_or_default()
                .iter()
                .find(|id| {
                    !before.contains(id)
                        && self
                            .tree()
                            .node(**id)
                            .is_some_and(|node| node.kind().type_name() == copy_kind)
                })
            {
                created.push((source, *id, duplicate));
            }
        }
        if let Err(err) = self.handle.end_macro() {
            self.fail(errors::session::error_message(&err));
        } else {
            self.copied_objects.extend(created);
            self.notice(history_text(&HistoryLabel::Macro(kind)));
        }
        self.refresh();
    }

    /// Deletes without confirmation (S-F3, #229): a single Ctrl+Z gives it all
    /// back. The message names the remaining graphs that used the data.
    fn delete_objects(&mut self, ids: &[NodeId]) {
        let targets = self.object_targets(ids);
        if targets.is_empty() {
            return;
        }
        let users = self.object_users(&targets);
        let readers = self.column_users_before(&targets);
        let commands = targets
            .into_iter()
            .map(|id| Command::RemoveNode { id })
            .collect();
        self.execute_object_group(MacroKind::DeleteObjects, commands);
        if !users.is_empty() && !self.last_action_failed() {
            self.notice(tr!("object-delete-used", graphs = users.join(", ")));
        }
        self.open_column_use(readers);
    }

    fn move_objects(&mut self, ids: &[NodeId], parent: NodeId) {
        let index = self.tree().children(parent).map_or(0, <[NodeId]>::len);
        let commands = self
            .object_targets(ids)
            .into_iter()
            .filter(|id| {
                self.tree()
                    .node(*id)
                    .is_some_and(|node| node.parent() != Some(parent))
            })
            .enumerate()
            .map(|(offset, id)| Command::Move {
                id,
                new_parent: parent,
                index: index + offset,
            })
            .collect();
        self.execute_object_group(MacroKind::MoveObjects, commands);
    }

    fn execute_object_group(&mut self, kind: MacroKind, commands: Vec<Command>) {
        if commands.is_empty() || self.handle.begin_macro(kind).is_err() {
            return;
        }
        for command in commands {
            if let Err(err) = self.handle.execute(command) {
                let _ = self.handle.abort_macro();
                self.refresh();
                self.fail(errors::session::error_message(&err));
                return;
            }
        }
        if let Err(err) = self.handle.end_macro() {
            self.fail(errors::session::error_message(&err));
        } else {
            self.notice(history_text(&HistoryLabel::Macro(kind)));
        }
        self.refresh();
    }

    /// Sheet whose tab is being renamed.
    pub fn renaming_sheet(&self) -> Option<NodeId> {
        self.renaming_sheet
    }

    /// The last sheet of a workbook cannot be deleted.
    pub fn can_delete_sheet(&self, sheet: NodeId) -> bool {
        self.sheet_book(sheet)
            .is_some_and(|book| self.worksheets_of(book).len() > 1)
    }

    /// Names of the graphs that plot at least one column of `sheet`.
    pub fn sheet_users(&self, sheet: NodeId) -> Vec<String> {
        let tree = &self.snapshot.tree;
        let Some(node) = tree.node(sheet) else {
            return Vec::new();
        };
        let columns = node.children();
        tree.walk_all()
            .into_iter()
            .filter_map(|id| tree.node(id))
            .filter(|node| match node.kind() {
                NodeKind::Graph { spec, .. } => columns.iter().any(|c| spec.references(*c)),
                _ => false,
            })
            .map(|node| node.name().to_string())
            .collect()
    }

    /// Parent workbook of `sheet`, if it is indeed one.
    fn sheet_book(&self, sheet: NodeId) -> Option<NodeId> {
        let node = self.snapshot.tree.node(sheet)?;
        node.kind().is_worksheet().then(|| node.parent()).flatten()
    }

    fn begin_rename_sheet(&mut self, sheet: NodeId) {
        if self.sheet_book(sheet).is_none() {
            self.fail(tr!("sheet-invalid", id = sheet.to_string()));
            return;
        }
        self.select_sheet(sheet);
        self.renaming_sheet = Some(sheet);
    }

    fn commit_rename_sheet(&mut self, sheet: NodeId, name: &str) {
        self.renaming_sheet = None;
        let Some(book) = self.sheet_book(sheet) else {
            return;
        };
        let name = name.trim();
        let current = self
            .snapshot
            .tree
            .node(sheet)
            .map(|node| node.name().to_string())
            .unwrap_or_default();
        if name == current {
            return;
        }
        let taken = self
            .worksheets_of(book)
            .iter()
            .any(|(id, other)| *id != sheet && other == name);
        if name.is_empty() {
            self.fail(tr!("sheet-rename-empty", name = current));
        } else if taken {
            self.fail(tr!("sheet-rename-taken", name = name, old = current));
        } else {
            self.execute(Command::Rename {
                id: sheet,
                name: name.to_string(),
            });
        }
    }

    fn duplicate_sheet(&mut self, sheet: NodeId) {
        let Some(book) = self.sheet_book(sheet) else {
            self.fail(tr!("sheet-invalid", id = sheet.to_string()));
            return;
        };
        let origin = self
            .snapshot
            .tree
            .node(sheet)
            .map(|node| node.name().to_string())
            .unwrap_or_default();
        let existing = self.worksheets_of(book);
        let name = (1u64..)
            .map(|n| {
                tr!(
                    "sheet-copy-name",
                    name = origin.as_str(),
                    number = n.to_string()
                )
            })
            .find(|candidate| existing.iter().all(|(_, other)| other != candidate))
            .unwrap_or(origin.clone());
        let before: Vec<NodeId> = existing.iter().map(|(id, _)| *id).collect();
        self.execute(Command::DuplicateWorksheet { sheet, name });
        let copy = self
            .worksheets_of(book)
            .into_iter()
            .map(|(id, _)| id)
            .find(|id| !before.contains(id));
        if let Some(copy) = copy {
            self.select_sheet(copy);
        }
    }

    /// Voir [`Action::DeleteSheet`].
    fn delete_sheet(&mut self, sheet: NodeId) {
        if self.sheet_book(sheet).is_none() {
            self.fail(tr!("sheet-invalid", id = sheet.to_string()));
        } else if !self.can_delete_sheet(sheet) {
            self.fail(tr!("sheet-delete-last"));
        } else {
            let users = self.sheet_users(sheet);
            self.editing = None;
            self.execute(Command::RemoveNode { id: sheet });
            if !users.is_empty() && !self.last_action_failed() {
                self.notice(tr!("sheet-delete-used", graphs = users.join(", ")));
            }
        }
    }

    /// Returns `true` if the input was accepted (including a no-op because the
    /// text did not change), `false` only on a validation failure (unknown
    /// column, unparsable numeric text); this is what
    /// [`AppState::commit_cell_and_advance`] uses to decide whether Enter
    /// should add a row.
    fn commit_cell(&mut self, column: NodeId, row: usize, text: String) -> bool {
        // B6 + A9 (option 1, `guide-de-correction-robustesse.md` §F5): a
        // single fix for two anomalies: if the validated text is exactly the
        // one already displayed, nothing was typed, and no command is
        // dispatched. Without this early exit, passing through a cell without
        // typing a keystroke still wrote (B6: revision, column version and
        // history entry for nothing), and an empty string that is present
        // (`Text(Some(""))`, displayed "" like `Text(None)`) was rewritten as
        // an absent value at the slightest pass-through (A9): an input cell
        // never tells the two apart on input. Comparing with the displayed
        // text avoids both without introducing a new convention: the case
        // where the user retypes the same thing never needs to write.
        if text == self.cell_text(column, row) {
            return true;
        }
        let Some(kind) = self
            .snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(Column::column_type)
        else {
            self.fail(tr!("error-column-gone"));
            return false;
        };
        // An input that does not fit the column type no longer makes it fail:
        // it **widens** the column to `ColumnType::Mixed`, through
        // `Command::SetCellsPromoting`, which groups the conversion and the
        // write in a single history entry. Typing "n/a" under a column of
        // measurements is the usual gesture of a lab notebook; refusing it
        // forced retyping the whole column as text, hence making everything
        // else untraceable.
        // The single rule for typed text (`ValueBlock::from_texts`, #258): the
        // same as paste. A text that does not fit a numeric column returns a
        // text block, and `SetCellsPromoting` then widens the column to
        // `Mixed` to hold it; an empty box is a missing value, not zero (the
        // way to clear a cell).
        let (values, promoting) = ValueBlock::from_texts(kind, &[text.as_str()]);
        self.execute(if promoting {
            Command::SetCellsPromoting {
                column,
                start: row,
                values,
                expected_version: None,
            }
        } else {
            Command::SetCells {
                column,
                start: row,
                values,
                expected_version: None,
            }
        });
        // A mixed column whose present values all turned out numeric converts
        // itself to numeric, the counterpart of the opposite direction already
        // handled above (`SetCellsPromoting`: a numeric column that receives
        // text widens to `Mixed`). There is no homogeneous `Text` type left to
        // promote: it is now a mixed column, filled cell by cell, that can end
        // up entirely numeric without any of its cells ever having been
        // explicitly "number", typically after fixing an "n/a" left as the
        // only non-numeric value.
        if kind == ColumnType::Mixed {
            self.auto_promote_mixed_column_if_all_numeric(column);
        }
        // Discreet notice (#311, S-R6): "1,234" is read as 1.234, not one
        // thousand two hundred thirty-four. Set after `execute` (which rewrites
        // the status), and only for a Numeric or Mixed cell input that
        // succeeded.
        if matches!(kind, ColumnType::Numeric | ColumnType::Mixed)
            && !self.last_action_failed()
            && stokesforge_core::number_format::looks_like_thousands_separator(&text)
        {
            if let Some(value) = stokesforge_core::number_format::parse_input(&text) {
                self.notice(tr!(
                    "status-cell-decimal-comma",
                    text = text.trim().to_string(),
                    value = stokesforge_core::number_format::format_exact(value)
                ));
            }
        }
        true
    }

    /// Converts `column` (assumed `Mixed`) to `Numeric` if all its present
    /// cells are numbers; see the call in [`AppState::commit_cell`] and, for
    /// paste (#258), [`AppState::execute_pasted_cells`].
    fn auto_promote_mixed_column_if_all_numeric(&mut self, column: NodeId) {
        let Some(col) = self.snapshot.tree.node(column).and_then(Node::as_column) else {
            return;
        };
        if column_all_numeric_after(col, None) {
            self.execute(Command::SetColumnType {
                column,
                column_type: ColumnType::Numeric,
            });
        }
    }

    /// Validates a cell input then, if `row` was the last row *displayed* of
    /// `sheet` and the validation succeeded, adds a row to `sheet` (`at ==
    /// len` is a valid target for `InsertRows`): Enter on the last cell of a
    /// column.
    ///
    /// Comparing with `row_count(sheet)` (the max of the lengths of **all**
    /// the columns of the sheet, `Tree::row_count_in`) rather than with the
    /// length of the internal buffer of `column` alone is essential: a text
    /// column never filled down to the bottom of the sheet has a buffer
    /// shorter than the other columns (sparse buffers, see
    /// `Column::len`/`ColumnValues::get`), so `row + 1 >= column.len()` could
    /// become true in the *middle* of the table: a phantom row was then
    /// inserted at the bottom of the whole sheet as soon as a cell of a row
    /// never written lower in that column was validated.
    fn commit_cell_and_advance(&mut self, column: NodeId, row: usize, text: String, sheet: NodeId) {
        let was_last_row = row + 1 >= self.row_count(sheet);
        let accepted = self.commit_cell(column, row, text);
        if accepted && was_last_row {
            self.rows(sheet, row + 1, 1, true);
        }
    }

    /// Clears the content of the selected cells or header fields of `sheet`
    /// (never both at once, a selection no longer mixes the two; see the head
    /// doc of [`crate::selection`]), without shifting what follows
    /// (Backspace/Ctrl+X): a single `Command::SetCells` per touched column for
    /// a data selection, one `Command::SetColumnMeta` (via
    /// [`AppState::set_column_meta_field`]) per touched field for a header
    /// selection (see
    /// [`crate::selection::Selection::cleared_header_fields`]). Everything is
    /// grouped in a single history entry as soon as more than one operation
    /// is involved (`History::begin_macro`/`end_macro`), so that undo restores
    /// everything at once.
    fn clear_selected_cells(&mut self, sheet: NodeId) {
        let Some(selection) = self.selection.get(&sheet) else {
            return;
        };
        let tree = &self.snapshot.tree;
        let ranges = selection.clear_ranges(tree, sheet);
        let header_fields = selection.cleared_header_fields(tree, sheet);
        if ranges.is_empty() && header_fields.is_empty() {
            return;
        }
        let grouped = ranges.len() + header_fields.len() > 1;
        if grouped && self.handle.begin_macro(MacroKind::ClearSelection).is_err() {
            self.fail(tr!("error-clear-group"));
            return;
        }
        for (column, start, count) in ranges {
            let Some(kind) = self
                .snapshot
                .tree
                .node(column)
                .and_then(Node::as_column)
                .map(Column::column_type)
            else {
                continue;
            };
            self.execute(Command::SetCells {
                column,
                start,
                values: ValueBlock::missing_for(kind, count),
                expected_version: None,
            });
        }
        for (column, field) in header_fields {
            let tree = Arc::clone(&self.snapshot.tree);
            self.set_column_meta_field(column, |meta| {
                // An empty text is always accepted.
                let _ = field.write_text(&tree, column, meta, String::new());
            });
        }
        if grouped {
            self.end_grouped_macro();
        }
    }

    /// Reacts to a press on Delete in two steps; see [`AppState::
    /// delete_repeat`] for the state that carries the memory of a previous
    /// press. The first press on a selection clears the boxes
    /// ([`Action::ClearSelectedCells`], like Backspace); the second,
    /// *only* if it targets exactly the same selection of the same sheet with
    /// no other grid mutation slipped in between (see [`AppState::
    /// forget_delete_repeat`]), deletes for good
    /// ([`Action::DeleteSelectedCells`]) and shifts what follows; never both
    /// on the first press, so as never to shift data out of a spreadsheet
    /// habit where Delete clears without shifting. `None` if `sheet` has no
    /// selection: nothing to do.
    pub fn consume_delete_key(&mut self, sheet: NodeId) -> Option<Action> {
        let selection = self.selection.get(&sheet)?.clone();
        if self.delete_repeat.as_ref() == Some(&(sheet, selection.clone())) {
            self.delete_repeat = None;
            Some(Action::DeleteSelectedCells { sheet })
        } else {
            self.delete_repeat = Some((sheet, selection));
            Some(Action::ClearSelectedCells { sheet })
        }
    }

    /// Forgets a possible double press on Delete in progress for `sheet`; to
    /// call before any other grid mutation that could slip in between the two
    /// steps of [`AppState::consume_delete_key`] (Cut, Backspace, paste...), so
    /// that a following press on Delete starts again at the first step (clear)
    /// rather than deleting by surprise on a selection that has not changed
    /// but whose content has moved in the meantime.
    pub fn forget_delete_repeat(&mut self, sheet: NodeId) {
        if matches!(&self.delete_repeat, Some((s, _)) if *s == sheet) {
            self.delete_repeat = None;
        }
    }

    /// Clears the content of the selected cells of `sheet` **and** moves up
    /// the following cells of each touched column, independently (Delete
    /// key): a single `Command::DeleteCells`, already multi-column.
    ///
    /// On a selection entirely in the header, there is nothing to move up (a
    /// field is not repeated per row, see
    /// [`crate::selection::Selection::delete_at_count`]): Delete clears the
    /// fields there, like Backspace. Without that, the key would stay without
    /// effect in the header, whereas it cleared the field there before the
    /// unification of the keyboard block.
    fn delete_selected_cells(&mut self, sheet: NodeId) {
        let Some(selection) = self.selection.get(&sheet) else {
            return;
        };
        if selection.is_columns() {
            let columns = selection.columns(&self.snapshot.tree, sheet);
            self.delete_columns(&columns);
            return;
        }
        let (columns, at, count) = selection.delete_at_count(&self.snapshot.tree, sheet);
        if columns.is_empty() || count == 0 {
            self.clear_selected_cells(sheet);
            return;
        }
        self.execute(Command::DeleteCells { columns, at, count });
    }

    /// See [`Action::MoveColumns`]: the sequence of `Command::Move` computed
    /// by [`crate::column_move::plan`], a single `execute` for one column, a
    /// `MoveColumns` macro for several. Neither `column_users_before` nor
    /// `open_column_use`: a move does not change what a calculation reads
    /// (formulas and analyses designate their columns by identifier).
    fn move_columns(
        &mut self,
        sheet: NodeId,
        columns: &[NodeId],
        to: crate::column_move::ColumnMove,
    ) {
        let order: Vec<NodeId> = self
            .visible_columns(sheet)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let children: Vec<NodeId> = self
            .snapshot
            .tree
            .children(sheet)
            .map(<[NodeId]>::to_vec)
            .unwrap_or_default();
        let Some(steps) = crate::column_move::plan(&children, &order, columns, to) else {
            return;
        };
        let moved = columns.iter().filter(|id| order.contains(id)).count();
        let step_command = |step: crate::column_move::MoveStep| Command::Move {
            id: step.id,
            new_parent: sheet,
            index: step.index,
        };
        if moved <= 1 {
            for step in steps {
                self.execute(step_command(step));
            }
            return;
        }
        if self.handle.begin_macro(MacroKind::MoveColumns).is_err() {
            self.fail(tr!("error-columns-group"));
            return;
        }
        for step in steps {
            self.execute(step_command(step));
        }
        self.end_grouped_macro();
    }

    /// Removes whole columns from their sheet ("Delete" menu of the column
    /// letters, Delete as a second press on a range of columns): one
    /// `Command::DeleteColumn` per column, grouped so that a single Undo gives
    /// them all back. The series that cited them stay plotted in grey
    /// (invariant 10), nothing else to clean.
    fn delete_columns(&mut self, columns: &[NodeId]) {
        // The list of readers is computed on the tree from before (#306).
        let users = self.column_users_before(columns);
        match columns {
            [] => {}
            [column] => self.execute(Command::DeleteColumn { column: *column }),
            _ => {
                if self.handle.begin_macro(MacroKind::DeleteColumns).is_err() {
                    self.fail(tr!("error-columns-group"));
                    return;
                }
                for &column in columns {
                    self.execute(Command::DeleteColumn { column });
                }
                self.end_grouped_macro();
            }
        }
        self.open_column_use(users);
    }

    /// Pastes `text` from the anchor of the current selection of `sheet`:
    /// Ctrl+V ([`Action::Paste`]). Replays [`AppState::grid_clipboard`] if
    /// `text` is exactly what [`AppState::copy_selection`]/
    /// [`AppState::copy_full_columns`] last pushed to the system clipboard
    /// ([`AppState::clipboard_text`]); otherwise `text` is an external text,
    /// rebuilt into a [`GridClipboard`] by [`external_clipboard_from_text`].
    /// See [`AppState::apply_paste`] for the full behavior matrix.
    fn paste(&mut self, sheet: NodeId, text: String) {
        if !self.paste_target_is_unambiguous(sheet) {
            return;
        }
        if stokesforge_persistence::clipboard::is_object_clipboard(&text) {
            self.fail(tr!("object-clipboard-use-manager"));
            return;
        }
        let Some(selection) = self.selection.get(&sheet) else {
            self.fail(tr!("error-paste-no-cell"));
            return;
        };
        let Some(anchor) = selection.anchor(&self.snapshot.tree, sheet) else {
            self.fail(tr!("error-paste-no-cell"));
            return;
        };
        let clipboard = if self.clipboard_text.as_deref() == Some(text.as_str()) {
            self.grid_clipboard.clone()
        } else {
            None
        }
        .unwrap_or_else(|| external_clipboard_from_text(&text));
        self.apply_paste(sheet, anchor, &clipboard, false);
    }

    /// False, with a status message, if the selection of `sheet` is
    /// non-contiguous (#159): a paste then has no unambiguous target.
    fn paste_target_is_unambiguous(&mut self, sheet: NodeId) -> bool {
        let contiguous = self
            .selection
            .get(&sheet)
            .is_none_or(|selection| selection.is_contiguous(&self.snapshot.tree, sheet));
        if !contiguous {
            self.fail(tr!("status-paste-non-contiguous"));
        }
        contiguous
    }

    /// Pastes in absolute position from [`AppState::grid_clipboard`]:
    /// Ctrl+Shift+V ([`Action::PasteAbsolute`]), which never has a text of its
    /// own (see the doc of the action). No effect if nothing has been copied
    /// yet in this session: an external text has no original rank to replay.
    fn paste_absolute(&mut self, sheet: NodeId) {
        if !self.paste_target_is_unambiguous(sheet) {
            return;
        }
        let Some(selection) = self.selection.get(&sheet) else {
            self.fail(tr!("error-paste-no-cell"));
            return;
        };
        let Some(anchor) = selection.anchor(&self.snapshot.tree, sheet) else {
            self.fail(tr!("error-paste-no-cell"));
            return;
        };
        let Some(clipboard) = self.grid_clipboard.clone() else {
            self.notice(tr!("status-paste-absolute-empty"));
            return;
        };
        self.apply_paste(sheet, anchor, &clipboard, true);
    }

    /// Core of paste, shared by [`AppState::paste`] (`absolute: false`) and
    /// [`AppState::paste_absolute`] (`absolute: true`); the full behavior
    /// matrix (copy/paste section at the head of this module):
    ///
    /// - [`GridClipboard::Text`] is written into the anchored box, whatever it
    ///   is (header field or data cell); `absolute` makes no sense for a
    ///   single box, ignored;
    /// - [`GridClipboard::Relative`] shifts onto the anchor if it is of the
    ///   same kind (fields on a field, data on a cell); nothing is pasted if
    ///   the anchor is of the other kind; in absolute, each row falls back on
    ///   its original rank rather than on the anchor;
    /// - [`GridClipboard::FullColumns`] pastes the header from the field
    ///   anchored field (nothing above) and the body without shifting if the
    ///   anchor is a field; drops the header and shifts the body onto the
    ///   anchor if it is a cell; in absolute, everything falls back on its
    ///   original rank whatever the anchor.
    ///
    /// Paste of data rows (#311): an **absent** box (shorter row) keeps the
    /// existing value, a present and empty box is a missing value; columns
    /// copied beyond the sheet are dropped, the status gives their number.
    ///
    /// A formula column naturally refuses the write of data
    /// (`CoreError::FormulaColumnReadOnly`): reported by the status, without
    /// blocking the other columns already pasted. A paste that touches several
    /// columns or several fields stays grouped in a single history entry.
    fn apply_paste(
        &mut self,
        sheet: NodeId,
        anchor: GridCursor,
        clipboard: &GridClipboard,
        absolute: bool,
    ) {
        let order = self.column_order(sheet);
        let anchor_column = grid_cursor_column(anchor);
        let Some(anchor_index) = order.iter().position(|&id| id == anchor_column) else {
            self.fail(tr!("error-column-gone"));
            return;
        };
        let order_from_anchor = &order[anchor_index..];
        match clipboard {
            GridClipboard::Text(text) => self.paste_single_value(anchor, text),
            GridClipboard::Relative(relative) => {
                self.paste_relative(anchor, order_from_anchor, relative, absolute)
            }
            GridClipboard::FullColumns(columns) => {
                self.paste_full_columns(anchor, order_from_anchor, columns, absolute)
            }
        }
    }

    /// [`GridClipboard::Text`]: writes `text` into the anchored box, header
    /// field or data cell depending on `anchor`; a single command, never
    /// grouped (a single field/cell touched).
    fn paste_single_value(&mut self, anchor: GridCursor, text: &str) {
        match anchor {
            GridCursor::Header(column, field) => {
                if let Some(refusal) = self.paste_header_text(column, field, text.to_string()) {
                    self.fail(refusal);
                }
            }
            GridCursor::Data(column, row) => {
                let Some(kind) = self
                    .snapshot
                    .tree
                    .node(column)
                    .and_then(Node::as_column)
                    .map(Column::column_type)
                else {
                    return;
                };
                let (values, promoting) =
                    value_block_from_tsv_column(kind, &[vec![text.to_string()]], 0);
                self.execute_pasted_cells(column, row, values, promoting, kind, false);
            }
        }
    }

    /// Writes a block of pasted cells from [`value_block_from_tsv_column`]:
    /// `Command::SetCellsPromoting` rather than `Command::SetCells` when
    /// `promoting` asks to widen the column to `Mixed` (see the doc of
    /// [`value_block_from_tsv_column`]), otherwise the same strict command as
    /// before. Shared by the three paste points into data cells
    /// ([`AppState::paste_single_value`], [`AppState::
    /// paste_relative`], [`AppState::paste_full_columns`]).
    ///
    /// `kind` is the type of the column **before** the paste. If it was
    /// `Mixed` and all its present boxes are numbers after the write, it goes
    /// back to `Numeric` (#258), as after an input
    /// (`Command::SetColumnType`, invariant 1). A paste stays **one** history
    /// entry (invariant 8): `grouped` says that a `PasteCells` macro is
    /// already open by the caller, the re-promotion slips into it before it
    /// closes; otherwise, a macro of its own is opened only if a re-promotion
    /// will happen (a simple paste stays a simple entry).
    fn execute_pasted_cells(
        &mut self,
        column: NodeId,
        start: usize,
        values: ValueBlock,
        promoting: bool,
        kind: ColumnType,
        grouped: bool,
    ) {
        let repromote = kind == ColumnType::Mixed
            && self
                .snapshot
                .tree
                .node(column)
                .and_then(Node::as_column)
                .is_some_and(|col| column_all_numeric_after(col, Some((start, &values))));
        let own_macro = repromote && !grouped;
        if own_macro && self.handle.begin_macro(MacroKind::PasteCells).is_err() {
            self.fail(tr!("error-paste-group"));
            return;
        }
        self.execute(if promoting {
            Command::SetCellsPromoting {
                column,
                start,
                values,
                expected_version: None,
            }
        } else {
            Command::SetCells {
                column,
                start,
                values,
                expected_version: None,
            }
        });
        // A refused write (formula column) is followed by no conversion.
        if repromote && !self.last_action_failed() {
            self.auto_promote_mixed_column_if_all_numeric(column);
        }
        if own_macro {
            self.end_grouped_macro();
        }
    }

    /// Status set when the copy overflows the sheet (#311): `columns` columns
    /// beyond the right edge, `fields` header rows beyond Formula (field
    /// paste anchored low). What overflows is dropped, never added. Set
    /// **after** the writes (each command rewrites the status), and only if
    /// none failed: an error (formula column) stays displayed. A single
    /// message if both overflow.
    fn notice_dropped(&mut self, columns: usize, fields: usize) {
        if self.last_action_failed() {
            return;
        }
        match (columns, fields) {
            (0, 0) => {}
            (columns, 0) => self.notice(tr!("status-paste-columns-dropped", count = columns)),
            (0, fields) => self.notice(tr!("status-paste-fields-dropped", count = fields)),
            (columns, fields) => self.notice(tr!(
                "status-paste-columns-fields-dropped",
                columns = columns,
                fields = fields
            )),
        }
    }

    /// Number of columns to drop: `wanted` wanted, `available` places.
    fn dropped_columns(wanted: usize, available: usize) -> usize {
        wanted.saturating_sub(available)
    }

    /// [`GridClipboard::Relative`]: see [`AppState::apply_paste`] for the
    /// rule. Nothing is pasted if `anchor` is not of the same kind as
    /// `relative` (fields against a data cell, or the reverse).
    fn paste_relative(
        &mut self,
        anchor: GridCursor,
        order_from_anchor: &[NodeId],
        relative: &RelativeCopy,
        absolute: bool,
    ) {
        match (anchor, relative) {
            (GridCursor::Header(_, anchor_field), RelativeCopy::HeaderFields { start, rows }) => {
                let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
                let targets: Vec<NodeId> = order_from_anchor
                    .iter()
                    .copied()
                    .take(column_count)
                    .collect();
                if targets.is_empty() {
                    return;
                }
                let shift = if absolute {
                    0
                } else {
                    anchor_field.index() as isize - start.index() as isize
                };
                let writes: Vec<(HeaderField, &Vec<String>)> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, cells)| {
                        let origin = start.index() as isize + i as isize;
                        let target = origin + shift;
                        (0..HeaderField::COUNT as isize)
                            .contains(&target)
                            .then(|| (HeaderField::from_index(target as usize), cells))
                    })
                    .collect();
                let grouped = writes.len() * targets.len() > 1;
                if grouped && self.handle.begin_macro(MacroKind::PasteCells).is_err() {
                    self.fail(tr!("error-paste-group"));
                    return;
                }
                let mut refusal = None;
                for &(field, cells) in &writes {
                    for (index, &column) in targets.iter().enumerate() {
                        let text = cells.get(index).cloned().unwrap_or_default();
                        let refused = self.paste_header_text(column, field, text);
                        refusal = refusal.or(refused);
                    }
                }
                if grouped {
                    self.end_grouped_macro();
                }
                // Header rows dropped: the low anchor pushes them beyond
                // Formula (`writes` keeps only those that fit).
                self.notice_dropped(
                    Self::dropped_columns(column_count, targets.len()),
                    rows.len() - writes.len(),
                );
                // The refusal of a formula goes last: it wins over the
                // overflow notice, the other fields are pasted.
                if let Some(refusal) = refusal {
                    self.fail(refusal);
                }
            }
            (GridCursor::Data(_, anchor_row), RelativeCopy::DataRows { start, rows }) => {
                let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
                let targets: Vec<NodeId> = order_from_anchor
                    .iter()
                    .copied()
                    .take(column_count)
                    .collect();
                if targets.is_empty() || rows.is_empty() {
                    return;
                }
                let paste_row = if absolute { *start } else { anchor_row };
                let grouped = targets.len() > 1;
                if grouped && self.handle.begin_macro(MacroKind::PasteCells).is_err() {
                    self.fail(tr!("error-paste-group"));
                    return;
                }
                for (index, &column) in targets.iter().enumerate() {
                    let Some(col) = self.snapshot.tree.node(column).and_then(Node::as_column)
                    else {
                        continue;
                    };
                    let kind = col.column_type();
                    let Some((values, promoting)) =
                        value_block_keeping_absent_cells(col, paste_row, rows, index)
                    else {
                        continue;
                    };
                    self.execute_pasted_cells(column, paste_row, values, promoting, kind, grouped);
                }
                if grouped {
                    self.end_grouped_macro();
                }
                self.notice_dropped(Self::dropped_columns(column_count, targets.len()), 0);
            }
            // Copied fields + anchor on a data cell, or copied data rows +
            // anchor on a header field: different kinds, nothing is pasted
            // (see the behavior matrix at the head of this module).
            _ => {}
        }
    }

    /// [`GridClipboard::FullColumns`]: see [`AppState::apply_paste`] for the
    /// rule; depends on `anchor` (field or cell) and on `absolute`.
    fn paste_full_columns(
        &mut self,
        anchor: GridCursor,
        order_from_anchor: &[NodeId],
        columns: &[FullColumnCopy],
        absolute: bool,
    ) {
        let targets: Vec<NodeId> = order_from_anchor
            .iter()
            .copied()
            .take(columns.len())
            .collect();
        if targets.is_empty() {
            return;
        }
        // Starting field of the header to write: all of them in absolute, from
        // the anchored field (nothing above) in relative if the anchor is a
        // field, none (header dropped) in relative if the anchor is a cell.
        let header_from: Option<usize> = if absolute {
            Some(0)
        } else {
            match anchor {
                GridCursor::Header(_, field) => Some(field.index()),
                GridCursor::Data(..) => None,
            }
        };
        // Starting row of the body: never shifted, except in relative anchored
        // on a data cell (shift onto the anchor).
        let body_start = match (absolute, anchor) {
            (false, GridCursor::Data(_, row)) => row,
            _ => 0,
        };
        let header_ops = header_from
            .map(|from| HeaderField::COUNT - from)
            .unwrap_or(0)
            * targets.len();
        let body_ops = targets
            .iter()
            .zip(columns)
            .filter(|(_, c)| !c.data.is_empty())
            .count();
        let grouped = header_ops + body_ops > 1;
        if grouped && self.handle.begin_macro(MacroKind::PasteCells).is_err() {
            self.fail(tr!("error-paste-group"));
            return;
        }
        let mut refusal = None;
        if let Some(from) = header_from {
            for field_index in from..HeaderField::COUNT {
                let field = HeaderField::from_index(field_index);
                for (&column, source) in targets.iter().zip(columns) {
                    let value = match field {
                        HeaderField::XPrime => source.x_prime.clone(),
                        HeaderField::LongName => source.long_name.clone(),
                        HeaderField::Unit => source.unit.clone(),
                        HeaderField::Comment => source.comment.clone(),
                        HeaderField::Formula => source.formula.clone().unwrap_or_default(),
                    };
                    let refused = self.paste_header_text(column, field, value);
                    refusal = refusal.or(refused);
                }
            }
        }
        for (&column, source) in targets.iter().zip(columns) {
            if source.data.is_empty() {
                continue;
            }
            let Some(kind) = self
                .snapshot
                .tree
                .node(column)
                .and_then(Node::as_column)
                .map(Column::column_type)
            else {
                continue;
            };
            let rows: Vec<Vec<String>> = source.data.iter().map(|v| vec![v.clone()]).collect();
            let (values, promoting) = value_block_from_tsv_column(kind, &rows, 0);
            self.execute_pasted_cells(column, body_start, values, promoting, kind, grouped);
        }
        if grouped {
            self.end_grouped_macro();
        }
        self.notice_dropped(Self::dropped_columns(columns.len(), targets.len()), 0);
        // The refusal of a formula goes last: the other fields and the values
        // are pasted, the status bar says what is not.
        if let Some(refusal) = refusal {
            self.fail(refusal);
        }
    }

    /// Re-reads the current metadata of `column`, lets `update` modify a
    /// single field, then replaces them as a block
    /// (`Command::SetColumnMeta`: the "block replacement, never by subfield"
    /// doctrine of the `stokesforge-commands` crate, see its head doc).
    /// Writes `text` into the header field `field` of `column`; a text that
    /// the field refuses is reported without writing anything. Non-numeric x':
    /// the status bar says so right away. Formula refused (unknown letter,
    /// self-read, `#REF`): the **sentence** of the refusal is returned, and the
    /// caller sets it, **after** the writes of the whole paste (the other
    /// fields are pasted, the status bar keeps the refusal).
    fn paste_header_text(
        &mut self,
        column: NodeId,
        field: HeaderField,
        text: String,
    ) -> Option<String> {
        let Some(mut meta) = self
            .snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(|c| c.meta().clone())
        else {
            self.fail(tr!("error-column-gone"));
            return None;
        };
        match field.write_text(&self.snapshot.tree, column, &mut meta, text) {
            Ok(()) => {}
            Err(HeaderTextRefused::NotANumber) => {
                self.fail(crate::tr!("status-x-prime-invalid"));
                return None;
            }
            Err(HeaderTextRefused::Formula(error)) => {
                // Pasted formula with a letter that does not exist in the
                // destination sheet, or that carries `#REF`: nothing is written
                // in this field.
                return Some(errors::formula::error_message(&error));
            }
        }
        self.set_meta_dropping_formula_data(column, meta, false);
        None
    }

    /// [`Action::SetColumnXPrime`].
    fn set_column_x_prime(&mut self, column: NodeId, text: String) {
        let _ = self.paste_header_text(column, HeaderField::XPrime, text);
    }

    fn set_column_meta_field(&mut self, column: NodeId, update: impl FnOnce(&mut ColumnMeta)) {
        let Some(mut meta) = self
            .snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map(|c| c.meta().clone())
        else {
            self.fail(tr!("error-column-gone"));
            return;
        };
        update(&mut meta);
        self.set_meta_dropping_formula_data(column, meta, false);
    }

    /// At least one of the `columns` carries a formula.
    pub fn any_formula(&self, columns: &[NodeId]) -> bool {
        columns
            .iter()
            .any(|&column| self.column_meta(column).formula.is_some())
    }

    /// Columns whose Formula field is selected and carries a formula.
    fn formula_columns_selected(&self, sheet: NodeId) -> Vec<NodeId> {
        let Some(selection) = self.selection.get(&sheet) else {
            return Vec::new();
        };
        let tree = &self.snapshot.tree;
        selection
            .cleared_header_fields(tree, sheet)
            .into_iter()
            .filter(|&(column, field)| {
                field == HeaderField::Formula && self.column_meta(column).formula.is_some()
            })
            .map(|(column, _)| column)
            .collect()
    }

    /// [`Action::RemoveFormulaKeepData`].
    fn remove_formulas_keep_data(&mut self, columns: &[NodeId]) {
        let grouped = columns
            .iter()
            .filter(|&&column| self.column_meta(column).formula.is_some())
            .count()
            > 1;
        if grouped && self.handle.begin_macro(MacroKind::RemoveFormula).is_err() {
            self.fail(tr!("error-columns-group"));
            return;
        }
        for &column in columns {
            if self.column_meta(column).formula.is_none() {
                continue;
            }
            let mut meta = self.column_meta(column);
            meta.formula = None;
            self.set_meta_dropping_formula_data(column, meta, true);
        }
        if grouped {
            self.end_grouped_macro();
        }
    }

    /// **The** write path for a column's metadata (#236): when they remove a
    /// formula, the computed values leave with it (`DeleteCells` of the whole
    /// column) except `keep_data`, all in a single history entry
    /// (`MacroKind::RemoveFormula`) so that Ctrl+Z gives back formula and data
    /// together. Every way of clearing the formula (header, Delete, paste,
    /// clearing the field, several columns) goes through here.
    fn set_meta_dropping_formula_data(
        &mut self,
        column: NodeId,
        meta: ColumnMeta,
        keep_data: bool,
    ) {
        let removes_formula = meta.formula.is_none() && self.column_meta(column).formula.is_some();
        let len = self
            .snapshot
            .tree
            .node(column)
            .and_then(Node::as_column)
            .map_or(0, Column::len);
        if !removes_formula || keep_data || len == 0 {
            if removes_formula && keep_data {
                // A single command, but the entry still carries the
                // "Remove Formula" label.
                if self.handle.begin_macro(MacroKind::RemoveFormula).is_ok() {
                    self.execute(Command::SetColumnMeta { column, meta });
                    self.end_grouped_macro();
                    return;
                }
            }
            self.execute(Command::SetColumnMeta { column, meta });
            return;
        }
        if self.handle.begin_macro(MacroKind::RemoveFormula).is_err() {
            self.fail(tr!("error-clear-group"));
            return;
        }
        self.execute(Command::SetColumnMeta { column, meta });
        self.execute(Command::DeleteCells {
            columns: vec![column],
            at: 0,
            count: len,
        });
        self.end_grouped_macro();
    }

    fn add_column(&mut self, sheet: NodeId) {
        self.execute(Command::AddColumn {
            sheet,
            index: None,
            name: None,
            column: Column::new(ColumnType::Numeric),
        });
    }

    /// New workbook of the user (File menu): its first sheet already gets an
    /// X column and a Y column, in a single history entry. Import, for its
    /// part, creates its workbooks empty (`create_import_book`): the file
    /// provides the columns.
    fn add_workbook(&mut self) {
        let root = self.snapshot.tree.root();
        if !self.begin_creation_macro(MacroKind::AddWorkbook) {
            return;
        }
        let previous = self
            .snapshot
            .tree
            .node(root)
            .and_then(|node| node.children().last().copied());
        self.execute(Command::AddWorkbook {
            parent: root,
            name: None,
        });
        let book = self
            .snapshot
            .tree
            .node(root)
            .and_then(|node| node.children().last().copied())
            .filter(|book| Some(*book) != previous);
        if let Some(sheet) =
            book.and_then(|book| self.worksheets_of(book).first().map(|(id, _)| *id))
        {
            self.add_xy_columns(sheet);
        }
        self.end_creation_macro();
    }

    /// New folder of the user (#326): "FolderN" is the first free name among
    /// the children of `parent` (uniqueness is among siblings). A vanished
    /// parent fails in the status bar, with no history entry.
    fn add_folder(&mut self, parent: NodeId) {
        let tree = &self.snapshot.tree;
        let taken = |name: &str| {
            tree.node(parent).is_some_and(|node| {
                node.children()
                    .iter()
                    .filter_map(|child| tree.node(*child))
                    .any(|child| child.name() == name)
            })
        };
        let mut n = 1u64;
        let mut name = format!("Folder{n}");
        while taken(&name) {
            n += 1;
            name = format!("Folder{n}");
        }
        self.execute(Command::AddFolder { parent, name });
    }

    /// New sheet of the user: like [`AppState::add_workbook`], it is born with
    /// an X column and a Y column.
    fn add_worksheet(&mut self, book: NodeId) {
        if !self.begin_creation_macro(MacroKind::AddWorksheet) {
            return;
        }
        let previous = self
            .snapshot
            .tree
            .node(book)
            .and_then(|node| node.children().last().copied());
        self.execute(Command::AddWorksheet { book, name: None });
        let sheet = self
            .snapshot
            .tree
            .node(book)
            .and_then(|node| node.children().last().copied())
            .filter(|sheet| Some(*sheet) != previous);
        if let Some(sheet) = sheet {
            self.add_xy_columns(sheet);
        }
        self.end_creation_macro();
    }

    fn begin_creation_macro(&mut self, kind: MacroKind) -> bool {
        if self.handle.begin_macro(kind).is_err() {
            self.fail(tr!("error-create-group"));
            return false;
        }
        true
    }

    fn end_creation_macro(&mut self) {
        self.end_grouped_macro();
    }

    /// **Common** closing of every grouped operation (paste, clearing,
    /// creation, graph properties): closes the macro, raises the error into a
    /// [`StatusMessage`], then refreshes the snapshot. `end_macro` refreshes
    /// nothing, and `execute` did so only **during** the macro: without this
    /// last `refresh`, `can_undo`/`undo_label` stay those from before the
    /// closing until an incidental refresh (issue #22).
    fn set_columns_grouped(
        &mut self,
        kind: MacroKind,
        columns: &[NodeId],
        command: impl Fn(NodeId) -> Command,
    ) {
        let grouped = columns.len() > 1;
        if grouped && self.handle.begin_macro(kind).is_err() {
            self.fail(tr!("error-columns-group"));
            return;
        }
        for &column in columns {
            self.execute(command(column));
        }
        if grouped {
            self.end_grouped_macro();
        }
    }

    /// Role of several columns, in a single undoable group.
    fn set_columns_role_grouped(&mut self, columns: &[NodeId], role: ColumnRole) {
        let grouped = columns.len() > 1;
        if grouped && self.handle.begin_macro(MacroKind::SetColumnsRole).is_err() {
            self.fail(tr!("error-columns-group"));
            return;
        }
        for &column in columns {
            self.set_column_meta_field(column, |meta| meta.role = role);
        }
        if grouped {
            self.end_grouped_macro();
        }
    }

    /// Closes a group opened by `begin_macro` (see the common closing above).
    /// The status bar then says the label **of the group**, put into a
    /// sentence by `history_text` as undo does ("Move Columns"), and not that
    /// of the last step that `execute` had set there ("Move “B”"). A failure
    /// (of the closing or of a step) keeps its place. The label is that of
    /// the history entry that has just been pushed: an empty group pushes
    /// none, and then replaces nothing (only a group entry is taken over).
    fn end_grouped_macro(&mut self) {
        let closed = self.handle.end_macro();
        if let Err(err) = &closed {
            self.fail(tr!(
                "error-group-close",
                cause = errors::session::error_message(err)
            ));
        }
        self.refresh();
        if closed.is_ok() && !self.last_action_failed() {
            if let Some(label @ HistoryLabel::Macro(_)) = self.snapshot.history.undo_label.clone() {
                self.notice(history_text(&label));
            }
        }
    }

    /// Adds an X column (abscissa role) then a Y column to `sheet`.
    fn add_xy_columns(&mut self, sheet: NodeId) {
        self.add_column(sheet);
        self.add_column(sheet);
        let columns = self.visible_columns(sheet);
        if let Some((x, ..)) = columns.first() {
            let mut meta = self.column_meta(*x);
            meta.role = ColumnRole::X;
            self.execute(Command::SetColumnMeta { column: *x, meta });
        }
    }

    /// Adds a column at the end of `sheet` then selects `row` in it; see
    /// [`Action::AddColumnAndSelect`]. `self.execute` has already refreshed
    /// `self.snapshot` on return: the new column, last of the visible
    /// columns, therefore already exists for `select_cell`.
    fn add_column_and_select(&mut self, sheet: NodeId, row: usize) {
        self.add_column(sheet);
        if let Some((column, ..)) = self.visible_columns(sheet).into_iter().next_back() {
            self.select_cell(sheet, column, row);
        }
    }

    /// Adds a column at the end of `sheet` then selects `field` in it; see
    /// [`Action::AddColumnAndSelectHeaderField`], the header-side counterpart
    /// of [`AppState::add_column_and_select`].
    fn add_column_and_select_header_field(&mut self, sheet: NodeId, field: HeaderField) {
        self.add_column(sheet);
        if let Some((column, ..)) = self.visible_columns(sheet).into_iter().next_back() {
            self.select_header_field(sheet, column, field);
        }
    }

    /// Insertion (`insert`) or deletion of rows on `sheet`.
    fn rows(&mut self, sheet: NodeId, at: usize, count: usize, insert: bool) {
        let command = if insert {
            Command::InsertRows { sheet, at, count }
        } else {
            Command::DeleteRows { sheet, at, count }
        };
        self.execute(command);
    }

    /// Imports each file of `paths` into its own new sheet, according to
    /// `destination`, with `recipe`: [`Action::ImportFiles`]. An unreadable
    /// or unparsable file becomes a [`StatusMessage`] and moves on to the
    /// next, never a complete abandon of the batch. The whole batch fits in a
    /// single history entry: `self.handle.begin_macro`/`end_macro` frame the
    /// whole loop, even if a single file succeeds; see
    /// `History::begin_macro`/`end_macro` (`stokesforge-commands/src/
    /// history.rs`) for the absorption of a macro that would end up empty.
    fn import_files(
        &mut self,
        destination: ImportDestination,
        recipe: stokesforge_import::ImportRecipe,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }
        let items = paths
            .iter()
            .flat_map(|path| import_items(path, &recipe))
            .collect();
        self.apply_import(destination, items);
    }

    /// Creates the sheets and columns of a batch **already parsed** (by
    /// [`AppState::import_files`], or in the background by
    /// [`AppState::start_import`]), in a single history entry "Import".
    ///
    /// Three decisions, all read from the recipe of the first readable file:
    /// the **layout** of the files into sheets ([`stokesforge_import::FilesLayout`]),
    /// the **mode** in the active workbook ([`stokesforge_import::ImportMode`]), and the
    /// destination. The plan (which columns of which files, on which
    /// sheet) is computed first ([`plan_import`]), then executed.
    fn apply_import(&mut self, destination: ImportDestination, items: Vec<ImportItem>) {
        // Distinct files (a file with several blocks counts as one).
        let mut paths: Vec<PathBuf> = Vec::new();
        for item in &items {
            if !paths.contains(&item.path) {
                paths.push(item.path.clone());
            }
        }
        if self.handle.begin_macro(MacroKind::Import).is_err() {
            self.fail(tr!("error-import-group"));
            return;
        }
        // Accumulated rather than passed to `self.fail` along the loop:
        // `self.fail`/`self.notice` replace `self.status` without combining it
        // with what it already carried, so one `self.fail` per failing file
        // would have been overwritten by the next. Everything ends up in a
        // single message, a failure as soon as at least one file failed, so
        // that it shows.
        let mut failures: Vec<String> = Vec::new();
        for item in &items {
            if let Err(failure) = &item.table {
                failures.push(errors::import::failure_message(failure));
            }
        }
        let mut done_paths: Vec<PathBuf> = Vec::new();
        let first_ok = items.iter().find(|item| item.table.is_ok());
        if let Some(ImportItem {
            recipe: first_recipe,
            ..
        }) = first_ok
        {
            let plans = plan_import(&items, first_recipe.files_layout);
            let merge_sheet = match (destination, first_recipe.import_mode) {
                (_, stokesforge_import::ImportMode::NewSheet) => None,
                (ImportDestination::ActiveBook(book), _) => self
                    .selected_sheet_in(book)
                    .or_else(|| self.worksheets_of(book).first().map(|(id, _)| *id)),
                _ => None,
            };
            if let Some(sheet) = merge_sheet {
                // All the plans merge into the active sheet.
                let columns: Vec<PlannedColumn> = plans
                    .iter()
                    .flat_map(|plan| plan.columns.iter().copied())
                    .collect();
                self.fill_existing_sheet(sheet, first_recipe.import_mode, &items, &columns);
                for plan in &plans {
                    for &item in &plan.items {
                        if !done_paths.contains(&items[item].path) {
                            done_paths.push(items[item].path.clone());
                        }
                    }
                }
            } else {
                let mut shared_book = None;
                for plan in &plans {
                    let Some(book) =
                        self.book_for_plan(destination, plan, &mut shared_book, &mut failures)
                    else {
                        continue;
                    };
                    let Some(sheet) = self.add_named_sheet(book, &plan.name, &mut failures) else {
                        continue;
                    };
                    for &planned in &plan.columns {
                        if let Some(column) =
                            stokesforge_importer::make_import_column(&items, planned, 0)
                        {
                            self.execute(Command::AddColumn {
                                sheet,
                                index: None,
                                name: None,
                                column,
                            });
                        }
                    }
                    for &item in &plan.items {
                        if !done_paths.contains(&items[item].path) {
                            done_paths.push(items[item].path.clone());
                        }
                    }
                }
            }
        }
        let imported = done_paths.len();
        if self.handle.end_macro().is_err() {
            failures.push(tr!("error-import-close-group"));
        }
        if imported > 0 {
            // Material for "Reopen Last Import…" and the recent files.
            self.recent_to_record.extend(done_paths.iter().cloned());
            if let Some(ImportItem {
                recipe: first_recipe,
                ..
            }) = first_ok
            {
                self.last_import = Some(LastImport {
                    destination,
                    recipe: first_recipe.clone(),
                    paths: done_paths,
                });
            }
        }
        // `self.execute` refreshes the snapshot at each command, but always
        // **while** the macro is open: without this last refresh, `can_undo`
        // would stay on the state from before the closing.
        self.refresh();
        if failures.is_empty() {
            self.notice(tr!(
                "status-import-done",
                imported = imported,
                total = paths.len()
            ));
        } else {
            self.fail(tr!(
                "error-import-partial",
                imported = imported,
                total = paths.len(),
                failures = failures.join(" ; ")
            ));
        }
    }

    /// The workbook that receives `plan`: the active workbook, or a new
    /// workbook (one per plan, or a single shared one).
    fn book_for_plan(
        &mut self,
        destination: ImportDestination,
        plan: &ImportPlan,
        shared_book: &mut Option<NodeId>,
        failures: &mut Vec<String>,
    ) -> Option<NodeId> {
        match destination {
            ImportDestination::ActiveBook(book) => Some(book),
            ImportDestination::NewBookForAll => {
                if let Some(book) = *shared_book {
                    return Some(book);
                }
                let book = self.create_import_book(None);
                if book.is_none() {
                    failures.push(tr!("error-import-create-book", name = plan.name.clone()));
                }
                *shared_book = book;
                book
            }
            ImportDestination::NewBookPerFile => {
                let book = self.create_import_book(Some(&plan.name));
                if book.is_none() {
                    failures.push(tr!("error-import-create-book", name = plan.name.clone()));
                }
                book
            }
        }
    }

    /// Adds a sheet to `book` and names it `name` (best effort: if the name is
    /// taken, it keeps its automatic name; this is not an import failure).
    ///
    /// `Command::AddWorksheet` returns only the `Revision`, never the
    /// identifier of the created sheet. `Project::add_worksheet` always
    /// inserts the sheet in last position among the children of `book`, and
    /// `self.execute` has already refreshed the snapshot: the last child just
    /// after is the new sheet, the only way to find it again from this bench,
    /// which only has a frozen `Arc<Tree>`.
    fn add_named_sheet(
        &mut self,
        book: NodeId,
        name: &str,
        failures: &mut Vec<String>,
    ) -> Option<NodeId> {
        let previous = self
            .snapshot
            .tree
            .node(book)
            .and_then(|node| node.children().last().copied());
        self.execute(Command::AddWorksheet { book, name: None });
        let sheet = self
            .snapshot
            .tree
            .node(book)
            .and_then(|node| node.children().last().copied())
            .filter(|sheet| Some(*sheet) != previous);
        let Some(sheet) = sheet else {
            failures.push(tr!("error-import-create-sheet", name = name));
            return None;
        };
        self.execute(Command::Rename {
            id: sheet,
            name: name.to_string(),
        });
        Some(sheet)
    }

    /// "Active workbook" mode outside a new sheet: replaces the content of
    /// `sheet`, adds columns after it, or extends its columns.
    fn fill_existing_sheet(
        &mut self,
        sheet: NodeId,
        mode: stokesforge_import::ImportMode,
        items: &[ImportItem],
        columns: &[PlannedColumn],
    ) {
        use stokesforge_import::ImportMode;
        match mode {
            ImportMode::NewSheet => {}
            ImportMode::ReplaceSheet | ImportMode::AddColumns => {
                if mode == ImportMode::ReplaceSheet {
                    for (column, ..) in self.visible_columns(sheet) {
                        self.execute(Command::DeleteColumn { column });
                    }
                }
                for &planned in columns {
                    if let Some(column) =
                        stokesforge_importer::make_import_column(items, planned, 0)
                    {
                        self.execute(Command::AddColumn {
                            sheet,
                            index: None,
                            name: None,
                            column,
                        });
                    }
                }
            }
            ImportMode::AddRows => {
                let start = self.row_count(sheet);
                let existing: Vec<NodeId> = self
                    .visible_columns(sheet)
                    .into_iter()
                    .map(|(id, ..)| id)
                    .collect();
                let added = columns
                    .iter()
                    .filter_map(|&planned| {
                        items[planned.item].table.as_ref().ok().map(|table| {
                            table
                                .columns
                                .get(planned.column)
                                .map_or(0, |c| c.values.len())
                        })
                    })
                    .max()
                    .unwrap_or(0);
                if added > 0 {
                    self.execute(Command::InsertRows {
                        sheet,
                        at: start,
                        count: added,
                    });
                }
                for (position, &planned) in columns.iter().enumerate() {
                    match existing.get(position) {
                        Some(&target) => {
                            if let Some(block) = stokesforge_importer::import_block(items, planned)
                            {
                                self.execute(Command::SetCellsPromoting {
                                    column: target,
                                    start,
                                    values: block,
                                    expected_version: None,
                                });
                            }
                        }
                        // More columns in the file than in the sheet: the new
                        // column starts with as many empty boxes.
                        None => {
                            if let Some(column) =
                                stokesforge_importer::make_import_column(items, planned, start)
                            {
                                self.execute(Command::AddColumn {
                                    sheet,
                                    index: None,
                                    name: None,
                                    column,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// `AddEmptyWorkbook` avoids the "Sheet1" sheet that `AddWorkbook` would
    /// add on top of the imported sheets.
    fn create_import_book(&mut self, file_stem: Option<&str>) -> Option<NodeId> {
        let root = self.snapshot.tree.root();
        let previous = self.snapshot.tree.node(root)?.children().last().copied();
        self.execute(Command::AddEmptyWorkbook {
            parent: root,
            name: None,
        });
        let book = self.snapshot.tree.node(root)?.children().last().copied()?;
        if Some(book) == previous {
            return None;
        }
        if let Some(stem) = file_stem {
            self.execute(Command::Rename {
                id: book,
                name: stem.to_string(),
            });
        }
        Some(book)
    }

    /// Creates a graph from the selected columns of `sheet`; see
    /// [`Action::PlotColumns`]. The graph is added in the folder that
    /// contains the workbook of `sheet` (the parent of the workbook, not of
    /// the sheet). No command sent if no Y column is present in `columns`:
    /// this plot action creates nothing without a Y. An empty graph is
    /// created explicitly by [`Action::AddGraph`].
    fn plot_columns(&mut self, sheet: NodeId, columns: Vec<NodeId>, plot_type: PlotType) {
        let planned = series_from_columns(
            &self.snapshot.tree,
            sheet,
            &columns,
            &plot_type,
            0,
            [0; OVERLAY_ROLES],
        );
        if planned.is_empty() {
            self.fail(tr!("error-plot-no-y"));
            return;
        }
        let Some(book) = self.snapshot.tree.node(sheet).and_then(Node::parent) else {
            self.fail(tr!("error-sheet-gone"));
            return;
        };
        let Some(parent) = self.snapshot.tree.node(book).and_then(Node::parent) else {
            self.fail(tr!("error-workbook-no-folder"));
            return;
        };
        // Curves plotted together form a group (#177).
        let mut panel = PanelSpec::default();
        panel.push_series_batch(planned.primary);
        // The Y2, Y3 and Y4 columns each go on their own overlaid layer:
        // X axis on top, Y axis on the right, own scales (#167, #168).
        let mut panels = vec![panel.clone()];
        for (slot, series) in planned.overlays.into_iter().enumerate() {
            if series.is_empty() {
                continue;
            }
            let mut layer = PanelSpec::overlay_on(
                &panel,
                OverlaySpec::role_layer(OverlaySpec::FIRST_ROLE_LAYER + slot as u8),
            );
            layer.push_series_batch(series);
            panels.push(layer);
        }
        let spec = PlotSpec {
            panels: Panels::new(panels),
            ..PlotSpec::default()
        };
        self.execute(Command::AddGraph {
            parent,
            name: None,
            spec,
        });
    }

    /// Adds series built from `columns` to the layer `panel` of the current
    /// `PlotSpec` of `graph`; see [`Action::AddSeriesToGraph`].
    /// Same X/Y pairing rule as [`AppState::plot_columns`], but the
    /// sheet the columns come from is deduced from their own parent
    /// (the open workbook is not carried by this action): all the
    /// columns of `columns` must therefore belong to the same sheet. Without
    /// `plot_type`, the series take that of the layer ([`panel_plot_type`]).
    /// The new series extend the palette after those already present,
    /// without touching the existing series, in a single undoable command.
    /// No command sent if `columns` is empty, if none has the Y role, if
    /// `panel` does not exist, or if `graph` is not a graph of this
    /// project.
    fn add_series_to_graph(
        &mut self,
        graph: NodeId,
        panel: usize,
        columns: Vec<NodeId>,
        plot_type: Option<PlotType>,
    ) {
        let Some(&first) = columns.first() else {
            return;
        };
        let Some(sheet) = self.snapshot.tree.node(first).and_then(Node::parent) else {
            self.fail(tr!("error-column-gone"));
            return;
        };
        let Some(mut spec) = current_plot_spec(&self.snapshot.tree, graph) else {
            self.fail(tr!("error-graph-gone"));
            return;
        };
        let Some(target) = spec.panels.get_mut(panel) else {
            self.fail(tr!("error-layer-gone"));
            return;
        };
        let plot_type = plot_type.unwrap_or_else(|| panel_plot_type(target));
        // The layer of each role Y2, Y3, Y4: the one of that number that
        // already covers the target layer, to be created otherwise (#167, #168).
        let base = overlay_base(&spec, panel);
        let existing: [Option<usize>; OVERLAY_ROLES] = std::array::from_fn(|slot| {
            role_layer_of(&spec, base, OverlaySpec::FIRST_ROLE_LAYER + slot as u8)
        });
        let overlay_starts: [usize; OVERLAY_ROLES] = std::array::from_fn(|slot| {
            existing[slot]
                .and_then(|index| spec.panels.get(index))
                .map_or(0, |layer| layer.series.len())
        });
        let Some(target) = spec.panels.get(panel) else {
            return;
        };
        let planned = series_from_columns(
            &self.snapshot.tree,
            sheet,
            &columns,
            &plot_type,
            target.series.len(),
            overlay_starts,
        );
        if planned.is_empty() {
            self.fail(tr!("error-add-no-y"));
            return;
        }
        if let Some(target) = spec.panels.get_mut(panel) {
            target.push_series_batch(planned.primary);
        }
        for (slot, series) in planned.overlays.into_iter().enumerate() {
            if series.is_empty() {
                continue;
            }
            let index = match existing[slot] {
                Some(index) => index,
                None => {
                    let Some(base_panel) = spec.panels.get(base).cloned() else {
                        return;
                    };
                    spec.panels.push(PanelSpec::overlay_on(
                        &base_panel,
                        OverlaySpec::role_layer(OverlaySpec::FIRST_ROLE_LAYER + slot as u8),
                    ));
                    spec.panels.len() - 1
                }
            };
            if let Some(layer) = spec.panels.get_mut(index) {
                layer.push_series_batch(series);
            }
        }
        self.execute(Command::SetPlotSpec { graph, spec });
    }

    /// Adds an overlaid layer to the base layer of `panel`; see
    /// [`Action::AddLayer`]. No command if `graph` is not a graph of this
    /// project or if `panel` does not exist.
    fn add_layer(&mut self, graph: NodeId, panel: usize, x_top: bool, y_right: bool) {
        let Some(mut spec) = current_plot_spec(&self.snapshot.tree, graph) else {
            self.fail(tr!("error-graph-gone"));
            return;
        };
        let base = overlay_base(&spec, panel);
        let Some(base_panel) = spec.panels.get(base).cloned() else {
            self.fail(tr!("error-layer-gone"));
            return;
        };
        if overlays_full(&spec, base) {
            self.fail(tr!("error-layers-full"));
            return;
        }
        // A two-axis layer takes the first free role number: the Xn/Yn columns
        // of that number are then stored in it (#168).
        let axes = match free_role_layer(&spec, base) {
            Some(layer) if x_top && y_right => OverlaySpec::role_layer(layer),
            _ => OverlaySpec::new(x_top, y_right),
        };
        spec.panels.push(PanelSpec::overlay_on(&base_panel, axes));
        self.execute(Command::SetPlotSpec { graph, spec });
    }

    /// [`Action::AddRowsToGraph`]: one "row" series per row of `rows`,
    /// anchored on the first x' column of `sheet` (the sheet of a row series
    /// is read from its `y` column, see [`SeriesSpec::row`]).
    ///
    /// [`RowAxis::Ask`]: a graph that already has an X (a series with abscissa
    /// from an X column, or a row series) asks the question instead of
    /// acting ([`AppState::pending_row_drop`]); with no X, the main axis.
    /// [`RowAxis::New`] adds an overlaid layer whose X axis is on top
    /// (the Y axis follows that of the covered layer); [`RowAxis::Present`] adds
    /// the series to the target layer. A single command, hence a single undo
    /// step.
    fn add_rows_to_graph(
        &mut self,
        graph: NodeId,
        panel: usize,
        sheet: NodeId,
        rows: Vec<usize>,
        axis: RowAxis,
    ) {
        let Some(mut spec) = current_plot_spec(&self.snapshot.tree, graph) else {
            self.fail(tr!("error-graph-gone"));
            return;
        };
        if spec.panels.get(panel).is_none() {
            self.fail(tr!("error-layer-gone"));
            return;
        }
        let Some(&anchor) = x_prime_columns(&self.snapshot.tree, sheet).first() else {
            self.fail(crate::tr!("status-row-no-x-prime"));
            return;
        };
        let row_count = self.row_count(sheet);
        let mut rows: Vec<usize> = rows.into_iter().filter(|row| *row < row_count).collect();
        rows.sort_unstable();
        rows.dedup();
        if rows.is_empty() {
            return;
        }
        let axis = match axis {
            RowAxis::Ask if spec_has_x(&spec) => {
                self.pending_row_drop = Some(PendingRowDrop {
                    graph,
                    panel,
                    sheet,
                    rows,
                });
                return;
            }
            RowAxis::Ask => RowAxis::Present,
            chosen => chosen,
        };
        let target = match axis {
            RowAxis::New => {
                let base = overlay_base(&spec, panel);
                let Some(base_panel) = spec.panels.get(base).cloned() else {
                    return;
                };
                spec.panels
                    .push(PanelSpec::overlay_on(&base_panel, OverlaySpec::TOP_X));
                spec.panels.len() - 1
            }
            _ => panel,
        };
        let plot_type = spec
            .panels
            .get(panel)
            .map_or(PlotType::Line, panel_plot_type);
        let Some(layer) = spec.panels.get_mut(target) else {
            return;
        };
        let start = layer.series.len();
        layer.push_series_batch(row_series(
            anchor,
            &rows,
            start,
            &plot_type,
            row_series_name,
        ));
        self.execute(Command::SetPlotSpec { graph, spec });
    }

    /// [`Action::PlotRows`]: a new graph, in the folder of the workbook of
    /// `sheet`, whose series are the requested rows.
    fn plot_rows(&mut self, sheet: NodeId, rows: Vec<usize>, plot_type: PlotType) {
        let Some(&anchor) = x_prime_columns(&self.snapshot.tree, sheet).first() else {
            self.fail(crate::tr!("status-row-no-x-prime"));
            return;
        };
        let row_count = self.row_count(sheet);
        let mut rows: Vec<usize> = rows.into_iter().filter(|row| *row < row_count).collect();
        rows.sort_unstable();
        rows.dedup();
        if rows.is_empty() {
            return;
        }
        let Some(parent) = self
            .snapshot
            .tree
            .node(sheet)
            .and_then(Node::parent)
            .and_then(|book| self.snapshot.tree.node(book).and_then(Node::parent))
        else {
            self.fail(tr!("error-sheet-gone"));
            return;
        };
        let mut panel = PanelSpec::default();
        panel.push_series_batch(row_series(anchor, &rows, 0, &plot_type, row_series_name));
        let spec = PlotSpec {
            panels: Panels::new(vec![panel]),
            ..PlotSpec::default()
        };
        self.execute(Command::AddGraph {
            parent,
            name: None,
            spec,
        });
    }

    /// [`Action::ConfirmRowDrop`]: replays the pending drop with the chosen
    /// axis, or abandons it.
    fn confirm_row_drop(&mut self, choice: Option<RowAxis>) {
        let Some(pending) = self.pending_row_drop.take() else {
            return;
        };
        if let Some(axis) = choice.filter(|axis| *axis != RowAxis::Ask) {
            self.add_rows_to_graph(
                pending.graph,
                pending.panel,
                pending.sheet,
                pending.rows,
                axis,
            );
        }
    }

    /// Replaces the text of a note, unless it is already exactly that: the
    /// same guard as [`AppState::commit_cell`] (B6/A9 option 1): passing
    /// through a note editor without typing a keystroke must not write a
    /// history entry for nothing.
    fn set_note_text(&mut self, note: NodeId, text: String) {
        if self.note_text(note).as_ref() == Some(&text) {
            return;
        }
        self.execute(Command::SetNoteText { id: note, text });
    }

    /// Refuses to run a script: this binary does not embed the Python bridge
    /// (criterion 1 of `tests/shell.rs`). Sends no command, the revision does
    /// not move.
    #[cfg(not(feature = "python"))]
    fn run_script(&mut self) {
        self.fail(tr!("error-script-unavailable"));
    }

    /// Starts a script run on a detached thread, reusing the engine if it
    /// already exists or creating it lazily otherwise: both the creation and
    /// the run happen **off the calling thread**, so that even the very first
    /// script (interpreter startup cost included) never freezes the window.
    /// The result is only collected by [`AppState::poll_script`].
    #[cfg(feature = "python")]
    fn run_script(&mut self) {
        if self.pending_script.is_some() {
            self.fail(tr!("error-script-running"));
            return;
        }
        let Some(python) = self.python.clone() else {
            self.fail(tr!("error-script-no-python"));
            return;
        };
        let existing_engine = self.engine.clone();
        // Identity of the script's own: a macro opened by the interface
        // (graph properties) does not absorb it and does not undo it (#21).
        let handle = self.handle.with_new_identity();
        let source = self.script_source.clone();
        // The path that `pyst.save()` will write: the project's, if it has one.
        let project_path = (!self.path.is_empty()).then(|| PathBuf::from(&self.path));
        // Guard #309: `pyst.save()` does not overwrite the source of a read that
        // discarded data (`save_as` stays allowed).
        let protected_path = self
            .discarded_source
            .as_ref()
            .map(|source| source.path.clone());
        let (reply_tx, reply_rx) = mpsc::channel();

        let spawned = thread::Builder::new()
            .name("stokesforge-app-script".to_owned())
            .spawn(move || {
                let engine = match existing_engine {
                    Some(engine) => Ok(engine),
                    None => ScriptEngine::spawn(handle, python).map(Arc::new),
                };
                let reply = match engine {
                    Ok(engine) => {
                        engine.set_project_path(project_path);
                        engine.set_protected_path(protected_path);
                        let outcome = engine.run(&source);
                        (Some(engine), outcome)
                    }
                    Err(message) => (None, Err(message)),
                };
                // Nobody to warn if `AppState` has disappeared in the meantime
                // (window closed during the run): the channel is then simply
                // dropped.
                let _ = reply_tx.send(reply);
            });

        match spawned {
            Ok(_detached) => {
                self.pending_script = Some(reply_rx);
                self.notice(tr!("status-script-running"));
            }
            Err(err) => self.fail(tr!(
                "error-script-start",
                cause = errors::io::error_message(&err)
            )),
        }
    }

    /// Applies the result of a finished script: remembers its output, sets a
    /// status message (short, an error for an exception (S-M10, #230); its
    /// trace is read in the output, never in the status bar) then refreshes
    /// the snapshot (this is the trap of this task: without this gesture, the
    /// grid would stay frozen although the model changed).
    ///
    /// The message is set **before** `refresh`, as in [`AppState::
    /// execute`]: if the refresh fails (actor unreachable), it overwrites
    /// this message with the error; more serious, it must win, not the
    /// reverse.
    #[cfg(feature = "python")]
    fn finish_script_run(&mut self, outcome: ScriptOutcome) {
        let interrupted = outcome.interrupted;
        let failed = outcome.error.is_some();
        self.script_output = Some(ScriptOutput::from(outcome));
        if failed && !interrupted {
            // S-M10: a script exception is an error, the trace is in the
            // console (the only exception written to "a single channel", S-R13).
            self.fail(tr!("status-script-exception"));
        } else {
            self.notice(if interrupted {
                tr!("status-script-interrupted")
            } else {
                tr!("status-script-done")
            });
        }
        self.refresh();
    }

    /// Refuses to interrupt a script: this binary does not embed the Python
    /// bridge.
    #[cfg(not(feature = "python"))]
    fn interrupt_script(&mut self) {
        self.fail(tr!("error-script-unavailable"));
    }

    /// Requests the stop of the running script. Does not block: sets a flag
    /// that the detached thread consults at its next checkpoint (see
    /// `stokesforge_script::ScriptEngine::interrupt`). No effect if there is
    /// no running script.
    #[cfg(feature = "python")]
    fn interrupt_script(&mut self) {
        // The `Arc` is cloned then the borrow released before `notice`: the two
        // fields of `self` cannot be borrowed at the same time as
        // `self.notice` needs an exclusive borrow.
        let engine = self.engine.clone();
        match engine {
            Some(engine) if self.pending_script.is_some() => {
                engine.interrupt();
                self.notice(tr!("status-script-interrupt-requested"));
            }
            _ => self.notice(tr!("status-script-none-running")),
        }
    }
}

/// Workbooks referenced by a graph; usable from all viewports.
pub fn graph_data_books(tree: &Tree, graph: NodeId) -> Vec<NodeId> {
    let Some(NodeKind::Graph { spec, .. }) = tree.node(graph).map(Node::kind) else {
        return Vec::new();
    };
    workbooks_of_columns(
        tree,
        spec.panels
            .iter()
            .flat_map(|p| &p.series)
            .flat_map(|s| s.referenced_columns()),
    )
}

/// Workbooks of the columns of a single curve; empty if the curve no longer exists.
pub fn series_data_books(
    tree: &Tree,
    graph: NodeId,
    series: stokesforge_plot::SeriesRef,
) -> Vec<NodeId> {
    let Some(NodeKind::Graph { spec, .. }) = tree.node(graph).map(Node::kind) else {
        return Vec::new();
    };
    spec.series_at(series)
        .map(|s| workbooks_of_columns(tree, s.referenced_columns()))
        .unwrap_or_default()
}

fn workbooks_of_columns(tree: &Tree, columns: impl IntoIterator<Item = NodeId>) -> Vec<NodeId> {
    let mut books = Vec::new();
    for column in columns {
        let Some(book) = tree
            .node(column)
            .filter(|n| n.as_column().is_some())
            .and_then(Node::parent)
            .and_then(|id| tree.node(id))
            .and_then(Node::parent)
        else {
            continue;
        };
        if tree
            .node(book)
            .is_some_and(|n| matches!(n.kind(), NodeKind::Workbook { .. }))
            && !books.contains(&book)
        {
            books.push(book);
        }
    }
    books
}

#[cfg(test)]
mod tests {
    //! Unit tests targeted at what depends on no session nor project: the
    //! value ↔ text boundary itself (A9 option 2 and C4,
    //! `guide-de-correction-robustesse.md` §F1/§F8). The rest of the file
    //! (dispatch, history, save...) is covered in integration by
    //! `tests/shell.rs` and by the `stokesforge-robustness` bench (battery 15),
    //! which need a real session.

    use super::{
        external_clipboard_from_text, grid_clipboard_to_text, GridClipboard, HeaderField,
        RelativeCopy, MISSING_TEXT_DISPLAY,
    };
    use stokesforge_core::number_format::format_exact as format_f64_for_display;

    /// A9 (option 2): the display convention on its own: an absent value
    /// must never display as the empty string that represents a **present**
    /// empty string, whatever the caller.
    #[test]
    fn a9_option2_the_absence_marker_is_not_the_empty_string() {
        assert_ne!(
            MISSING_TEXT_DISPLAY, "",
            "otherwise an absent value and a present empty string \
             display identically (anomaly A9)"
        );
        assert_eq!(
            MISSING_TEXT_DISPLAY, "\u{00A0}",
            "chosen convention: a single non-breaking space"
        );
    }

    /// C4: the bench reproduction (`b15_a_scientific_measurement_fits_in_a_cell`)
    /// fixes the specification; this test pins the same three values at the
    /// level of the formatting function itself, without going through a session.
    #[test]
    fn c4_the_three_reproduction_measures_fit_in_24_characters() {
        const READABLE: usize = 24;
        for value in [1.602_176_634e-19_f64, 6.022_140_76e23, 0.1 + 0.2] {
            let shown = format_f64_for_display(value);
            assert!(
                shown.len() <= READABLE,
                "{value} → \"{shown}\" ({} characters)",
                shown.len()
            );
        }
    }

    /// C4: the round trip stays bit-exact; this is what §F8 of the guide
    /// already checks for the default `Display`, and which must stay true
    /// for scientific notation.
    #[test]
    fn c4_the_text_round_trip_is_bit_exact() {
        for value in [
            1.602_176_634e-19_f64,
            6.022_140_76e23,
            0.1 + 0.2,
            0.0,
            -0.0,
            f64::MIN_POSITIVE,
            f64::MAX,
        ] {
            let shown = format_f64_for_display(value);
            let reread: f64 = shown
                .parse()
                .unwrap_or_else(|e| panic!("\"{shown}\" (from {value:e}) does not reparse: {e}"));
            assert_eq!(
                reread.to_bits(),
                value.to_bits(),
                "{value:e} → \"{shown}\" → {reread:e}: different bits"
            );
        }
    }

    /// C4: zero never switches to scientific notation (`"0e0"` would be
    /// correct but unreadable for such a common case).
    #[test]
    fn c4_zero_stays_decimal() {
        assert_eq!(format_f64_for_display(0.0), "0");
        assert_eq!(format_f64_for_display(-0.0), "-0");
    }

    /// C4: an ordinary value (the one the grid displays most often) is not
    /// touched by the threshold.
    #[test]
    fn c4_an_ordinary_value_stays_decimal() {
        assert_eq!(format_f64_for_display(42.5), "42.5");
        assert_eq!(format_f64_for_display(0.0), "0");
        assert_eq!(format_f64_for_display(1.0), "1");
    }

    /// [`external_clipboard_from_text`]: rule 1 of the behavior matrix: an
    /// external text with no tab or line break becomes a
    /// [`GridClipboard::Text`], whatever its content (including empty).
    #[test]
    fn external_clipboard_from_text_plain_text_becomes_text() {
        assert_eq!(
            external_clipboard_from_text("Sensor 1"),
            GridClipboard::Text("Sensor 1".to_string())
        );
        assert_eq!(
            external_clipboard_from_text(""),
            GridClipboard::Text(String::new())
        );
    }

    /// A tabbed external text (even with no line break) or a multi-line one
    /// becomes a relative rectangle of data, starting from row 0; never a
    /// header field from an external text.
    #[test]
    fn external_clipboard_from_text_tabbed_or_multiline_text_becomes_data() {
        assert_eq!(
            external_clipboard_from_text("0\t10\n0.5\t\n"),
            GridClipboard::Relative(RelativeCopy::DataRows {
                start: 0,
                rows: vec![
                    vec!["0".to_string(), "10".to_string()],
                    vec!["0.5".to_string(), String::new()],
                ],
            })
        );
        assert_eq!(
            external_clipboard_from_text("a\tb"),
            GridClipboard::Relative(RelativeCopy::DataRows {
                start: 0,
                rows: vec![vec!["a".to_string(), "b".to_string()]],
            })
        );
    }

    /// #259: an external spreadsheet ends its lines with `\r\n` and quotes a
    /// box that carries a line break; this box stays a box, and a lone box
    /// (even multiline) stays a text.
    #[test]
    fn external_clipboard_from_text_reads_the_spreadsheet_format() {
        assert_eq!(
            external_clipboard_from_text("a\t\"b\nc\"\r\nd\te\r\n"),
            GridClipboard::Relative(RelativeCopy::DataRows {
                start: 0,
                rows: vec![
                    vec!["a".to_string(), "b\nc".to_string()],
                    vec!["d".to_string(), "e".to_string()],
                ],
            })
        );
        assert_eq!(
            external_clipboard_from_text("\"a\nb\""),
            GridClipboard::Text("a\nb".to_string())
        );
    }

    /// #259: the text pushed to the system quotes the boxes that carry a
    /// tab, line break or quote.
    #[test]
    fn grid_clipboard_to_text_quotes_the_special_boxes() {
        let clipboard = GridClipboard::Relative(RelativeCopy::DataRows {
            start: 0,
            rows: vec![vec!["a\nb".to_string(), "c".to_string()]],
        });
        assert_eq!(grid_clipboard_to_text(&clipboard), "\"a\nb\"\tc\n");
        assert_eq!(
            grid_clipboard_to_text(&GridClipboard::Text("x\ty".to_string())),
            "\"x\ty\""
        );
        assert_eq!(
            grid_clipboard_to_text(&GridClipboard::Text("simple".to_string())),
            "simple"
        );
    }

    /// [`grid_clipboard_to_text`]: the text pushed to the system clipboard
    /// never carries a control character any more (unlike the former
    /// `HEADER_LINE_MARKER`): a copy of header fields reads like an ordinary
    /// TSV.
    #[test]
    fn grid_clipboard_to_text_contains_no_control_character() {
        let clipboard = GridClipboard::Relative(RelativeCopy::HeaderFields {
            start: HeaderField::Unit,
            rows: vec![vec!["nm".to_string()], vec!["a comment".to_string()]],
        });
        let text = grid_clipboard_to_text(&clipboard);
        assert_eq!(text, "nm\na comment\n");
        assert!(text.chars().all(|c| !c.is_control() || c == '\n'));
    }
}

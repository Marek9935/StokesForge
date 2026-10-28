//! The primitives of the foundation.
//!
//! A command is a **value**, not a stateful object: it holds no `Project`, no
//! `Tree`, not even an `Arc`. This is what lets it cross a channel between
//! threads as it is, from the UI to the actor that holds the single
//! `&mut Project` of the process.
//!
//! Everything needed to undo is computed **at apply time**, by `History`, not
//! carried by the command.
//!
//! # Node creation: one command per kind, not a generic `AddNode`
//!
//! Each node kind (folder, workbook, empty workbook, worksheet, graph, matrix,
//! note) has its own variant rather than a single
//! `AddNode { kind: NodeKindSpec, .. }`. This is the same logic as `Rename`
//! or `SetColumnType`, already fine-grained: the style of this module is "one
//! command per intent", and `stokesforge_core::Project` itself exposes one
//! `add_*` method per kind rather than a generic constructor parameterized by
//! a `NodeKind`. Following this shape avoids inventing, on the commands side,
//! a level of indirection that the core does not have. Removal, however, stays
//! generic (`RemoveNode { id }`): `Project::remove` already is, on any
//! `NodeId`, with no distinction of kind to make at the call.
//!
//! # Content editing: whole replacement, never by subfield
//!
//! `SetWindowMeta`, `SetNoteText`, `SetColumnMeta` and `SetPlotSpec` replace
//! their whole target, like `SetCells` which replaces a whole range rather than
//! cell by cell. None of these four commands is **structural** in the sense of
//! [`Command::is_structural`]: they change neither the shape of the tree nor
//! the order of the nodes, only the content of an existing node — same nature
//! as `SetColumnType`, which also replaces a content as a whole without
//! touching the structure.

use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, ColumnType, NodeCopy, NodeId, Tree, ValueBlock,
    WindowMeta,
};
use stokesforge_plot::{PlotSpec, SavedColors};

use crate::label::{ColumnRef, HistoryLabel};

/// One of the model mutations a user can express.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Writes `values` into `column` starting at row `start`, extending the
    /// column if needed.
    SetCells {
        /// Target column.
        column: NodeId,
        /// First row written.
        start: usize,
        /// Block written; its variant must match the column type.
        values: ValueBlock,
        /// A2 (`guide-de-correction-robustesse.md` §F6) — optimistic
        /// condition, checked by `History::apply` **at the moment of applying**
        /// the command, never before: `None` checks nothing (historical
        /// behavior, that of the UI which has no such discipline);
        /// `Some(v)` requires `Column::version` of `column` to still equal
        /// exactly `v`, otherwise the command is refused
        /// (`CoreError::ConcurrentModification`) without touching the model or
        /// the history. This is the field that `stokesforge-script/bridge.rs`
        /// fills to close the TOCTOU between the snapshot read by a script
        /// and the command it sends.
        expected_version: Option<u64>,
    },
    /// Writes `values` into `column` like [`Command::SetCells`], but
    /// **widens the column type** instead of refusing a block that does not
    /// match it.
    ///
    /// This is the command of *tolerant* input: typing "n/a" in a numeric
    /// column is no longer an error or a silent `NaN`, it is a column that
    /// becomes [`ColumnType::Mixed`] and a cell that carries a text, the
    /// others keeping their numbers. `SetCells` stays the command of *strict*
    /// input, that of a caller who knows what it writes and wants to be
    /// refused if it is wrong (the Python bridge, a script).
    ///
    /// Three paths, from the cheapest to the most expensive:
    ///
    /// 1. the block already matches the column type → strictly `SetCells`,
    ///    same delta, same cost;
    /// 2. the column is **already** mixed and the block is not → the block is
    ///    converted ([`ValueBlock::into_mixed`]) and written; still a cell
    ///    delta, so still proportional to what is written, not to the size of
    ///    the column;
    /// 3. the column is not mixed and the block does not match it → the column
    ///    is promoted to `Mixed` **then** written. This is the only path that
    ///    costs a column snapshot in the history, exactly like
    ///    `SetColumnType` which it contains; it happens only once per column,
    ///    the second tolerant input taking path 2.
    SetCellsPromoting {
        /// Target column.
        column: NodeId,
        /// First row written.
        start: usize,
        /// Block written; its variant does **not** have to match the column
        /// type.
        values: ValueBlock,
        /// Same optimistic condition as [`Command::SetCells`], checked at the
        /// same moment and with the same effect on disagreement.
        expected_version: Option<u64>,
    },
    /// Adds a column to `sheet`. `index: None` → at the end;
    /// `name: None` → first free name, spreadsheet style.
    AddColumn {
        /// Host sheet.
        sheet: NodeId,
        /// Insertion position, or end of sheet.
        index: Option<usize>,
        /// Requested name, or automatic name.
        name: Option<String>,
        /// Initial content of the column.
        column: Column,
    },
    /// Deletes a column and its values.
    DeleteColumn {
        /// Column to delete.
        column: NodeId,
    },
    /// Renames any node (column, sheet, workbook, folder...).
    Rename {
        /// Node to rename.
        id: NodeId,
        /// New name.
        name: String,
    },
    /// Moves a node under a new parent, at a given position.
    Move {
        /// Node to move.
        id: NodeId,
        /// Host parent.
        new_parent: NodeId,
        /// Position among the children of the host parent.
        index: usize,
    },
    /// Changes the type of a column (destructive conversion of the values).
    SetColumnType {
        /// Target column.
        column: NodeId,
        /// Requested type.
        column_type: ColumnType,
    },
    /// Inserts rows at sheet level (columns such that `at < len`).
    InsertRows {
        /// Target sheet.
        sheet: NodeId,
        /// Insertion row.
        at: usize,
        /// Number of rows inserted.
        count: usize,
    },
    /// Deletes rows at sheet level (columns such that `at < len`).
    DeleteRows {
        /// Target sheet.
        sheet: NodeId,
        /// First row deleted.
        at: usize,
        /// Number of rows deleted.
        count: usize,
    },
    /// Deletes `count` rows from `at`, column by column and
    /// **independently**, in the columns listed by `columns` — the
    /// counterpart of `DeleteRows` for a selection that does not necessarily
    /// cover all the columns of a sheet (Delete key on a rectangle of cells).
    /// A column such that `at >= len` is silently ignored, exactly as
    /// `DeleteRows` already does via `Project::row_targets`; duplicates in
    /// `columns` are ignored.
    DeleteCells {
        /// Target columns.
        columns: Vec<NodeId>,
        /// First row deleted in each affected column.
        at: usize,
        /// Number of rows requested (undo uses the quantity actually removed
        /// per column, as for `DeleteRows`).
        count: usize,
    },

    /// Adds a folder at the end of `parent`. Unlike windows, a folder has no
    /// automatic name in `stokesforge-core` (`Project::add_folder` takes a
    /// required `name`): so this command does not make it optional either.
    AddFolder {
        /// Host folder.
        parent: NodeId,
        /// Folder name.
        name: String,
    },
    /// Adds a workbook with one "Sheet1" worksheet. `name: None` → first free
    /// "BookN".
    AddWorkbook {
        /// Host folder.
        parent: NodeId,
        /// Requested name, or automatic name.
        name: Option<String>,
    },
    /// Adds a workbook **without** a worksheet. `name: None` → first free
    /// "BookN". Distinct from `AddWorkbook`: an empty workbook has not the same
    /// history entry to undo (no "Sheet1" worksheet to recreate), and a caller
    /// who wants to build its own worksheets must not have to undo the one
    /// that `AddWorkbook` would have imposed.
    AddEmptyWorkbook {
        /// Host folder.
        parent: NodeId,
        /// Requested name, or automatic name.
        name: Option<String>,
    },
    /// Adds a worksheet at the end of the workbook. `name: None` → first free
    /// "SheetN" in this workbook.
    AddWorksheet {
        /// Host workbook.
        book: NodeId,
        /// Requested name, or automatic name.
        name: Option<String>,
    },
    /// Duplicates a worksheet (columns included, new identifiers) right after
    /// it, under `name`. The name is **required**: the « copie » suffix is a
    /// phrase of the interface, which `stokesforge-core` does not know; the
    /// caller composes it (`Project::duplicate_worksheet`).
    DuplicateWorksheet {
        /// Worksheet to duplicate.
        sheet: NodeId,
        /// Name of the copy.
        name: String,
    },
    /// Pastes a self-contained copy, whatever its kind.
    InsertCopy {
        /// Destination container.
        parent: NodeId,
        /// Values captured at copy time, with no source project or allocator.
        copy: NodeCopy,
        /// Name chosen in the destination container.
        name: String,
    },
    /// Adds a graph carrying `spec`. `name: None` → first free "GraphN".
    /// `spec` is required, as in `Project::add_graph`: a graph without a spec
    /// describes nothing more than an empty window, so make it explicit here
    /// too (`PlotSpec::default()` for an empty graph).
    AddGraph {
        /// Host folder.
        parent: NodeId,
        /// Requested name, or automatic name.
        name: Option<String>,
        /// Description of the graph.
        spec: PlotSpec,
    },
    /// Adds a matrix. `name: None` → first free "MBookN".
    AddMatrix {
        /// Host folder.
        parent: NodeId,
        /// Requested name, or automatic name.
        name: Option<String>,
    },
    /// Adds a note carrying `text`. `name: None` → first free "NoteN".
    AddNote {
        /// Host folder.
        parent: NodeId,
        /// Requested name, or automatic name.
        name: Option<String>,
        /// Initial text.
        text: String,
    },
    /// Removes any node and all its descendants — folder, workbook,
    /// worksheet, column, graph, matrix or note. `Project::remove` is already
    /// generic on `NodeId`: this command therefore does not need one variant
    /// per kind, unlike the creation commands (see the head doc of this
    /// module). What becomes of a window open on the removed node is a UI
    /// decision, outside the scope of this command.
    RemoveNode {
        /// Node to remove, with its descendants.
        id: NodeId,
    },

    /// Replaces the metadata of a window (workbook, graph, matrix, note) as a
    /// whole.
    SetWindowMeta {
        /// Target window.
        id: NodeId,
        /// New metadata.
        meta: WindowMeta,
    },
    /// Replaces the text of a note.
    SetNoteText {
        /// Target note.
        id: NodeId,
        /// New text.
        text: String,
    },
    /// Replaces the metadata of a column (plot role, unit, long name,
    /// comment, display format) as a whole.
    SetColumnMeta {
        /// Target column.
        column: NodeId,
        /// New metadata.
        meta: ColumnMeta,
    },
    /// Replaces the `PlotSpec` of an existing graph, as a whole — no editing
    /// by subfield (series, axis, legend): see the head doc of this
    /// module.
    SetPlotSpec {
        /// Target graph.
        graph: NodeId,
        /// New description of the graph.
        spec: PlotSpec,
    },
    /// Same replacement as SetPlotSpec, with its history reason as data.
    EditPlotSpec {
        graph: NodeId,
        spec: PlotSpec,
        kind: crate::label::PlotEditKind,
    },
    /// Records an analysis calculation (#181): creates its output columns
    /// right after the Y column of each series, with their values, and its
    /// record. **One command, one history entry**, structural (columns
    /// appear): undoing it removes the columns and the record at once. The
    /// values are computed *before*, by `stokesforge-analysis`, and carried by
    /// `draft` — the command calls no calculation, it stays a value.
    /// Replaces the sources by the correction and records its immutable trace.
    ReplaceAnalysisData {
        draft: CalculationDraft,
        expected_versions: Vec<(NodeId, u64)>,
    },
    AddCalculation {
        /// The operation, its inputs and the columns to write.
        draft: CalculationDraft,
    },
    /// "Restore original data" (#248): gives back to the columns that a
    /// correction replaced their earlier values (kept in the hidden workbook of
    /// analyses), overwrites what was typed in them since and removes the
    /// trace. Structural: undoing gives back the correction and its copies.
    RestoreOriginalData {
        /// The trace of the correction (Results row).
        trace: NodeId,
    },
    /// Replaces the parameters, the columns and the results of a calculation
    /// ("Edit Calculation…", dragging a baseline handle on the graph).
    /// Outputs that already exist are rewritten in place.
    UpdateCalculation {
        /// Target calculation.
        id: NodeId,
        /// New description.
        draft: CalculationDraft,
    },
    /// Removes a calculation. `with_columns` also deletes its output columns;
    /// otherwise they become ordinary columns ("Detach from
    /// Calculation").
    RemoveCalculation {
        /// Target calculation.
        id: NodeId,
        /// Also delete the output columns.
        with_columns: bool,
    },
    /// Creates or completes the linked transposed sheet of `source` (#253) with
    /// the rows `rows` (0 = the first): sheet "Sourceᵀ", x' column then one
    /// column per row, all read from the source. A row already present reuses
    /// its column. Structural (sheet and columns appear); the refusal names the
    /// column without x' or with text.
    TransposeRows {
        /// Source sheet.
        source: NodeId,
        /// Rows requested.
        rows: Vec<usize>,
    },
    /// Refreshes a transposed sheet from its source (#253): rewrites the
    /// linked columns (edited values are lost, undoable).
    RecalculateTranspose {
        /// Transposed sheet.
        sheet: NodeId,
    },
    /// Replaces the colors saved in the project, as a whole (saving or
    /// removing a color of the color picker). Data, not structure: no node
    /// moves.
    SetSavedColors {
        /// New saved colors.
        colors: SavedColors,
    },
}

impl Command {
    /// **Structured** history label (the action and its parameters, not a
    /// sentence: the application turns it into a sentence in the current
    /// language), to be computed on the tree **before** applying: that is where
    /// the targeted names still exist (a deleted column has no name
    /// afterwards).
    ///
    /// This function cannot fail: if the node is unknown, the field concerned
    /// is `None` and the sentence falls back to a generic form. It is applying
    /// the command that will fail, with the right error; a label is not the
    /// place to report a problem.
    pub fn label(&self, tree: &Tree) -> HistoryLabel {
        let name = |id: NodeId| name_of(tree, id).map(str::to_string);
        match self {
            Command::SetCells { column, .. } | Command::SetCellsPromoting { column, .. } => {
                HistoryLabel::SetCells {
                    column: column_ref(tree, *column),
                }
            }
            Command::AddColumn { .. } => HistoryLabel::AddColumn,
            Command::DeleteColumn { column } => HistoryLabel::DeleteColumn {
                column: column_ref(tree, *column),
            },
            Command::Rename { id, name: to } => HistoryLabel::Rename {
                from: name(*id),
                to: to.clone(),
            },
            Command::Move { id, .. } => HistoryLabel::Move { name: name(*id) },
            Command::SetColumnType { column, .. } => HistoryLabel::SetColumnType {
                column: column_ref(tree, *column),
            },
            Command::InsertRows { count, .. } => HistoryLabel::InsertRows { count: *count },
            Command::DeleteRows { count, .. } => HistoryLabel::DeleteRows { count: *count },
            Command::DeleteCells { count, .. } => HistoryLabel::DeleteCells { count: *count },
            Command::AddFolder { name, .. } => HistoryLabel::AddFolder { name: name.clone() },
            Command::AddWorkbook { .. } => HistoryLabel::AddWorkbook,
            Command::AddEmptyWorkbook { .. } => HistoryLabel::AddEmptyWorkbook,
            Command::AddWorksheet { .. } => HistoryLabel::AddWorksheet,
            Command::DuplicateWorksheet { sheet, .. } => {
                HistoryLabel::DuplicateWorksheet { name: name(*sheet) }
            }
            Command::InsertCopy { .. } => HistoryLabel::Macro(crate::MacroKind::PasteObjects),
            Command::AddGraph { .. } => HistoryLabel::AddGraph,
            Command::AddMatrix { .. } => HistoryLabel::AddMatrix,
            Command::AddNote { .. } => HistoryLabel::AddNote,
            Command::RemoveNode { id } => HistoryLabel::RemoveNode { name: name(*id) },
            Command::SetWindowMeta { id, .. } => HistoryLabel::SetWindowMeta { name: name(*id) },
            Command::SetNoteText { id, .. } => HistoryLabel::SetNoteText { name: name(*id) },
            Command::SetColumnMeta { column, .. } => HistoryLabel::SetColumnMeta {
                column: column_ref(tree, *column),
            },
            Command::EditPlotSpec { kind, .. } => HistoryLabel::EditPlotSpec(*kind),
            Command::SetPlotSpec { graph, .. } => HistoryLabel::SetPlotSpec { name: name(*graph) },
            Command::SetSavedColors { .. } => HistoryLabel::SetSavedColors,
            Command::TransposeRows { .. } => HistoryLabel::TransposeRows,
            Command::RecalculateTranspose { .. } => HistoryLabel::RecalculateTranspose,
            Command::ReplaceAnalysisData { draft, .. } => {
                HistoryLabel::ReplaceAnalysisData(draft.operation.kind())
            }
            Command::AddCalculation { draft } => {
                HistoryLabel::AddCalculation(draft.operation.kind())
            }
            Command::RestoreOriginalData { trace } => HistoryLabel::RestoreOriginalData(
                tree.correction_traces()
                    .get(trace)
                    .map(|record| record.operation.kind()),
            ),
            Command::UpdateCalculation { id, draft } => HistoryLabel::UpdateCalculation(
                tree.calculation(*id)
                    .map_or_else(|| draft.operation.kind(), |c| c.operation.kind()),
            ),
            Command::RemoveCalculation { id, with_columns } => HistoryLabel::RemoveCalculation {
                kind: tree.calculation(*id).map(|c| c.operation.kind()),
                with_columns: *with_columns,
            },
        }
    }
}

impl Command {
    /// True for the commands that touch the **structure** (tree, names,
    /// order, nodes appearing or disappearing) rather than the **content**
    /// of an existing node — see the doctrine of the two kinds at the top of
    /// `history.rs`. `History::execute` uses it to decide whether
    /// `Entry::Structural` needs a full snapshot of the tree
    /// (structure) or a simple delta is enough (data): this is exactly the
    /// distinction that made the cost of a data command diverge with the size
    /// of the project (D3, `guide-de-correction-robustesse.md`).
    ///
    /// The eight node creation/removal commands (`AddFolder`,
    /// `AddWorkbook`, `AddEmptyWorkbook`, `AddWorksheet`, `AddGraph`,
    /// `AddMatrix`, `AddNote`, `RemoveNode`) join `AddColumn` and
    /// `DeleteColumn` here, for the same reason as they do: they change the
    /// shape of the tree. `SetWindowMeta`, `SetNoteText`, `SetColumnMeta` and
    /// `SetPlotSpec` are **not** here: they replace the content of a node
    /// already in place, without touching its position or that of any other —
    /// same nature as `SetColumnType`, which stays on the data side although
    /// it too replaces a whole content as a block.
    ///
    /// Neither is `SetCellsPromoting`, although it can change the **type** of
    /// a column: changing the type of a node is not changing the shape of the
    /// tree, and its delta is exactly that of `SetColumnType` — a column
    /// snapshot, never a tree snapshot.
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            Command::AddColumn { .. }
                | Command::DeleteColumn { .. }
                | Command::Rename { .. }
                | Command::Move { .. }
                | Command::AddFolder { .. }
                | Command::AddWorkbook { .. }
                | Command::AddEmptyWorkbook { .. }
                | Command::AddWorksheet { .. }
                | Command::DuplicateWorksheet { .. }
                | Command::InsertCopy { .. }
                | Command::AddGraph { .. }
                | Command::AddMatrix { .. }
                | Command::AddNote { .. }
                | Command::RemoveNode { .. }
                | Command::ReplaceAnalysisData { .. }
                | Command::AddCalculation { .. }
                | Command::RestoreOriginalData { .. }
                | Command::UpdateCalculation { .. }
                | Command::RemoveCalculation { .. }
                | Command::TransposeRows { .. }
                | Command::RecalculateTranspose { .. }
        )
    }
}

/// Name of a node if it exists in this tree.
fn name_of(tree: &Tree, id: NodeId) -> Option<&str> {
    tree.node(id).map(stokesforge_core::Node::name)
}

/// Designation of a **column** in a history label: its position in its
/// sheet ([`ColumnRef`]), never its node name — see the doc of
/// [`ColumnRef`] for the trap of the two identities.
///
/// `None` if the column is no longer in the tree or has no parent sheet: the
/// sentence then falls back to a generic form.
fn column_ref(tree: &Tree, column: NodeId) -> Option<ColumnRef> {
    let sheet = tree.parent(column).ok().flatten()?;
    let index = tree
        .children(sheet)
        .ok()?
        .iter()
        .position(|&child| child == column)?;
    Some(ColumnRef(index))
}

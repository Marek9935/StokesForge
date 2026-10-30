//! **Structured** history labels (issue #46, work package #39).
//!
//! A history label is not a sentence: it is an action and its parameters
//! ("insert *3* rows", "rename *A* to *Z*"). This crate returns the data; the
//! sentence, in the user's language, belongs to `stokesforge-app` (`tr!`,
//! Fluent plurals). No dependency on Fluent here: `stokesforge-commands`
//! stays below the translation boundary (invariant 2).
//!
//! # Why not a `String`
//!
//! A ready-made sentence freezes a language at the moment the command is
//! applied, and forces each crate to redo its grammar (the plural of "row"
//! written by hand in `Command::label`). A structured label is turned into a
//! sentence **at display time**, so it follows the current language; it is
//! compared (`==`) without parsing a text (the application used to ask
//! `label == "Importer"`), and it is carried as is if the history is one day
//! written to a file — the history is **not** in the `.stf` today
//! (`docs/persistence.md`).
//!
//! # The English `Display`
//!
//! [`HistoryLabel`] is displayed in **English**: it is the text of the logs
//! and of the CLI, which do not translate (work package #39). It mirrors the
//! `en` catalog of the application (`history-*` keys);
//! `stokesforge-app-state/tests/history_text.rs` checks that the two say the
//! same thing, variant by variant.

use std::fmt;

use stokesforge_analysis::ops::OperationKind;
use stokesforge_core::family_english;

/// Designation of a **column** in a label: its position in its
/// sheet, never its node name.
///
/// A column carries two identities and only one is visible: its node name is
/// a key of the model, the grid shows only the position letter (A, B, C…). A
/// label that named the node would announce "Time" for a column that the user
/// knows only under the name "A". The letter is computed by
/// [`stokesforge_core::spreadsheet_name`], the function of the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnRef(pub usize);

impl ColumnRef {
    /// The grid header letter: `A`, `B`, … `Z`, `AA`…
    pub fn letter(self) -> String {
        stokesforge_core::spreadsheet_name(self.0)
    }
}

/// The groups of actions that the application opens with `begin_macro`: a
/// **closed** enumeration, so that a group is data and not a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroKind {
    /// Paste cells.
    PasteCells,
    /// Paste project objects.
    PasteObjects,
    /// Duplicate the selected objects.
    DuplicateObjects,
    /// Delete the selected objects.
    DeleteObjects,
    /// Move the selected objects.
    MoveObjects,
    /// Rename the Project Manager selection.
    RenameObjects,
    /// Clear the grid selection.
    ClearSelection,
    /// Apply the properties window of a graph.
    GraphProperties,
    /// Refresh columns from their source file.
    RefreshFromFile,
    /// Import instrument files.
    Import,
    /// Create a workbook and fill it (plot, import into a new workbook).
    AddWorkbook,
    /// Create a worksheet and fill it.
    AddWorksheet,
    /// Run the writes of a Python script.
    Script,
    /// Change the type of several columns at once.
    SetColumnsType,
    /// Change the role of several columns at once.
    SetColumnsRole,
    /// An analysis calculation and the plotting of its columns on a graph (#200).
    AnalysisToGraph,
    /// Remove the formula of a column, with or without its values (#236).
    RemoveFormula,
    /// Delete several whole columns of a worksheet.
    DeleteColumns,
    /// Move several columns of a worksheet in one gesture (#319); moving a
    /// single column stays a `Move` entry. Distinct from `MoveObjects`, which
    /// is the Project Manager one.
    MoveColumns,
    /// Analyze rows: the linked transposed sheet (created or completed)
    /// and the calculation on its column, undone as one block (#253).
    AnalyzeRows,
}

impl MacroKind {
    /// All the group kinds, for exhaustive tests.
    pub const ALL: [MacroKind; 20] = [
        MacroKind::PasteCells,
        MacroKind::PasteObjects,
        MacroKind::DuplicateObjects,
        MacroKind::DeleteObjects,
        MacroKind::MoveObjects,
        MacroKind::RenameObjects,
        MacroKind::ClearSelection,
        MacroKind::GraphProperties,
        MacroKind::RefreshFromFile,
        MacroKind::Import,
        MacroKind::AddWorkbook,
        MacroKind::AddWorksheet,
        MacroKind::Script,
        MacroKind::SetColumnsType,
        MacroKind::SetColumnsRole,
        MacroKind::AnalysisToGraph,
        MacroKind::RemoveFormula,
        MacroKind::DeleteColumns,
        MacroKind::MoveColumns,
        MacroKind::AnalyzeRows,
    ];
}

/// The label of a history entry: the action and its parameters.
///
/// An `Option` field is `None` when the targeted node is no longer in the tree
/// at the time of computing the label (a command that will fail anyway): the
/// sentence then falls back to a generic form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryLabel {
    /// Writing cells (`SetCells`, `SetCellsPromoting`).
    SetCells {
        /// Column written.
        column: Option<ColumnRef>,
    },
    /// Adding a column.
    AddColumn,
    /// Deleting a column.
    DeleteColumn {
        /// Column deleted.
        column: Option<ColumnRef>,
    },
    /// Renaming a node.
    Rename {
        /// Name before.
        from: Option<String>,
        /// Name after.
        to: String,
    },
    /// Moving a node.
    Move {
        /// Name of the moved node.
        name: Option<String>,
    },
    /// Changing the type of a column.
    SetColumnType {
        /// Column converted.
        column: Option<ColumnRef>,
    },
    /// Inserting rows.
    InsertRows {
        /// Number of rows.
        count: usize,
    },
    /// Deleting rows.
    DeleteRows {
        /// Number of rows.
        count: usize,
    },
    /// Deleting cells, with those below moving up.
    DeleteCells {
        /// Number of rows removed in each column.
        count: usize,
    },
    /// Adding a folder.
    AddFolder {
        /// Name of the folder.
        name: String,
    },
    /// Adding a workbook.
    AddWorkbook,
    /// Adding a workbook without a worksheet.
    AddEmptyWorkbook,
    /// Adding a worksheet.
    AddWorksheet,
    /// Duplicating a worksheet.
    DuplicateWorksheet {
        /// Name of the duplicated worksheet.
        name: Option<String>,
    },
    /// Adding a graph.
    AddGraph,
    /// Adding a matrix.
    AddMatrix,
    /// Adding a note.
    AddNote,
    /// Deleting a node and its descendants.
    RemoveNode {
        /// Name of the deleted node.
        name: Option<String>,
    },
    /// Changing the properties of a window.
    SetWindowMeta {
        /// Name of the window.
        name: Option<String>,
    },
    /// Changing the text of a note.
    SetNoteText {
        /// Name of the note.
        name: Option<String>,
    },
    /// Changing the properties of a column.
    SetColumnMeta {
        /// Column modified.
        column: Option<ColumnRef>,
    },
    /// Changing the plot of a graph.
    SetPlotSpec {
        /// Name of the graph.
        name: Option<String>,
    },
    /// Changing the colors saved in the project.
    SetSavedColors,
    /// Recording an analysis calculation (#181).
    AddCalculation(OperationKind),
    /// Final correction of the original values.
    ReplaceAnalysisData(OperationKind),
    /// Restoring the original values that a correction had replaced
    /// (#248), with the family of the correction if it is still known.
    RestoreOriginalData(Option<OperationKind>),
    /// Changing the parameters of an analysis calculation.
    UpdateCalculation(OperationKind),
    /// Removing an analysis calculation.
    RemoveCalculation {
        /// Family of the removed calculation, if still known.
        kind: Option<OperationKind>,
        /// Its output columns are deleted with it.
        with_columns: bool,
    },
    /// Creating or completing a linked transposed sheet (#253).
    TransposeRows,
    /// Refreshing a transposed sheet from its source (#253).
    RecalculateTranspose,
    /// Targeted edit of the graph content, always by replacing the spec.
    EditPlotSpec(PlotEditKind),
    /// A group of actions undone as one block.
    Macro(MacroKind),
}

/// Reason for an edit of the graph content (#84–85), with no translated text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotEditKind {
    Visibility,
    RenameSeries,
    ReorderSeries,
    RemoveSeries,
    RemoveText,
    RemoveObject,
    ReorderObject,
    /// Name of a series group (#197).
    RenameGroup,
    /// A series group dissolved: its series stay (#197).
    DissolveGroup,
    /// A series group deleted with its series (#197).
    RemoveGroup,
    /// A series group moved as a block (#197).
    ReorderGroup,
    /// Closed or reopened frame of a layer: top and right mirrored, dashes
    /// copied (#344).
    ClosedFrame,
    /// A cell added beside another one (#357).
    AddCell,
    /// A cell, or an overlay layer, deleted (#357).
    RemoveCell,
    /// The plot area (frame) width of a grid column or height of a grid row
    /// set, by a field or a handle (#394).
    ResizeCell,
    /// The first plot area placed from the left or top edge of the image
    /// (Left, Top) by dragging an edge of the figure (#405).
    PlaceFigure,
}
impl PlotEditKind {
    pub const ALL: [Self; 16] = [
        Self::Visibility,
        Self::RenameSeries,
        Self::ReorderSeries,
        Self::RemoveSeries,
        Self::RemoveText,
        Self::RemoveObject,
        Self::ReorderObject,
        Self::RenameGroup,
        Self::DissolveGroup,
        Self::RemoveGroup,
        Self::ReorderGroup,
        Self::ClosedFrame,
        Self::AddCell,
        Self::RemoveCell,
        Self::ResizeCell,
        Self::PlaceFigure,
    ];
}
impl std::fmt::Display for PlotEditKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Visibility => "Change Visibility",
            Self::RenameSeries => "Rename Series",
            Self::ReorderSeries => "Reorder Series",
            Self::RemoveSeries => "Delete Series",
            Self::RemoveText => "Delete Text",
            Self::RemoveObject => "Delete Object",
            Self::ReorderObject => "Reorder Object",
            Self::RenameGroup => "Rename Group",
            Self::DissolveGroup => "Ungroup Series",
            Self::RemoveGroup => "Delete Group",
            Self::ReorderGroup => "Reorder Group",
            Self::ClosedFrame => "Change Closed Frame",
            Self::AddCell => "Add Layer Beside",
            Self::RemoveCell => "Delete Layer",
            Self::ResizeCell => "Resize Plot Area",
            Self::PlaceFigure => "Position Plot Area",
        })
    }
}

/// "Row" or "Rows": in English only 1 is singular.
fn rows(count: usize) -> &'static str {
    if count == 1 {
        "Row"
    } else {
        "Rows"
    }
}

impl fmt::Display for MacroKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MacroKind::PasteCells => "Paste",
            MacroKind::PasteObjects => "Paste Objects",
            MacroKind::DuplicateObjects => "Duplicate Objects",
            MacroKind::DeleteObjects => "Delete Objects",
            MacroKind::MoveObjects => "Move Objects",
            MacroKind::RenameObjects => "Rename Objects",
            MacroKind::ClearSelection => "Clear Selection",
            MacroKind::GraphProperties => "Edit Graph Properties",
            MacroKind::RefreshFromFile => "Refresh from File",
            MacroKind::Import => "Import",
            MacroKind::AddWorkbook => "Add Workbook",
            MacroKind::AddWorksheet => "Add Sheet",
            MacroKind::Script => "Run Script",
            MacroKind::SetColumnsType => "Change Column Types",
            MacroKind::SetColumnsRole => "Change Column Roles",
            MacroKind::AnalysisToGraph => "Analyze and Plot",
            MacroKind::RemoveFormula => "Remove Formula",
            MacroKind::DeleteColumns => "Delete Columns",
            MacroKind::MoveColumns => "Move Columns",
            MacroKind::AnalyzeRows => "Analyze Rows",
        })
    }
}

impl fmt::Display for HistoryLabel {
    /// The label **in English**, for the logs and the CLI. The interface does
    /// not use it: it turns the variant into a sentence through its catalog.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryLabel::SetCells { column: Some(c) } => {
                write!(f, "Edit Cells in Column {}", c.letter())
            }
            HistoryLabel::SetCells { column: None } => f.write_str("Edit Cells"),
            HistoryLabel::AddColumn => f.write_str("Add Column"),
            HistoryLabel::DeleteColumn { column: Some(c) } => {
                write!(f, "Delete Column {}", c.letter())
            }
            HistoryLabel::DeleteColumn { column: None } => f.write_str("Delete Column"),
            HistoryLabel::Rename {
                from: Some(from),
                to,
            } => write!(f, "Rename “{from}” to “{to}”"),
            HistoryLabel::Rename { from: None, to } => write!(f, "Rename to “{to}”"),
            HistoryLabel::Move { name: Some(name) } => write!(f, "Move “{name}”"),
            HistoryLabel::Move { name: None } => f.write_str("Move"),
            HistoryLabel::SetColumnType { column: Some(c) } => {
                write!(f, "Change Type of Column {}", c.letter())
            }
            HistoryLabel::SetColumnType { column: None } => f.write_str("Change Column Type"),
            HistoryLabel::InsertRows { count } => write!(f, "Insert {count} {}", rows(*count)),
            HistoryLabel::DeleteRows { count } => write!(f, "Delete {count} {}", rows(*count)),
            HistoryLabel::DeleteCells { count } => {
                write!(f, "Delete Cells and Shift Up ({count} {})", rows(*count))
            }
            HistoryLabel::AddFolder { name } => write!(f, "Add Folder “{name}”"),
            HistoryLabel::AddWorkbook => f.write_str("Add Workbook"),
            HistoryLabel::AddEmptyWorkbook => f.write_str("Add Empty Workbook"),
            HistoryLabel::AddWorksheet => f.write_str("Add Sheet"),
            HistoryLabel::DuplicateWorksheet { name } => match name {
                Some(name) => write!(f, "Duplicate Sheet “{name}”"),
                None => f.write_str("Duplicate Sheet"),
            },
            HistoryLabel::AddGraph => f.write_str("Add Graph"),
            HistoryLabel::AddMatrix => f.write_str("Add Matrix"),
            HistoryLabel::AddNote => f.write_str("Add Note"),
            HistoryLabel::RemoveNode { name: Some(name) } => write!(f, "Delete “{name}”"),
            HistoryLabel::RemoveNode { name: None } => f.write_str("Delete"),
            HistoryLabel::SetWindowMeta { name: Some(name) } => {
                write!(f, "Edit Properties of “{name}”")
            }
            HistoryLabel::SetWindowMeta { name: None } => f.write_str("Edit Window Properties"),
            HistoryLabel::SetNoteText { name: Some(name) } => write!(f, "Edit Note “{name}”"),
            HistoryLabel::SetNoteText { name: None } => f.write_str("Edit Note"),
            HistoryLabel::SetColumnMeta { column: Some(c) } => {
                write!(f, "Edit Properties of Column {}", c.letter())
            }
            HistoryLabel::SetColumnMeta { column: None } => f.write_str("Edit Column Properties"),
            HistoryLabel::SetPlotSpec { name: Some(name) } => write!(f, "Edit Graph “{name}”"),
            HistoryLabel::SetPlotSpec { name: None } => f.write_str("Edit Graph"),
            HistoryLabel::EditPlotSpec(kind) => kind.fmt(f),
            HistoryLabel::SetSavedColors => f.write_str("Edit Saved Colors"),
            HistoryLabel::ReplaceAnalysisData(kind) => {
                write!(f, "Replace Original Data ({})", family_english(*kind))
            }
            HistoryLabel::RestoreOriginalData(Some(kind)) => {
                write!(f, "Restore Original Data ({})", family_english(*kind))
            }
            HistoryLabel::RestoreOriginalData(None) => f.write_str("Restore Original Data"),
            HistoryLabel::AddCalculation(kind) => {
                write!(f, "Add {} Calculation", family_english(*kind))
            }
            HistoryLabel::UpdateCalculation(kind) => {
                write!(f, "Edit {} Calculation", family_english(*kind))
            }
            HistoryLabel::RemoveCalculation { kind, with_columns } => {
                let verb = if *with_columns { "Delete" } else { "Detach" };
                match kind {
                    Some(kind) => write!(f, "{verb} {} Calculation", family_english(*kind)),
                    None => write!(f, "{verb} Calculation"),
                }
            }
            HistoryLabel::TransposeRows => f.write_str("Transpose Rows"),
            HistoryLabel::RecalculateTranspose => f.write_str("Recalculate Transposed Sheet"),
            HistoryLabel::Macro(kind) => kind.fmt(f),
        }
    }
}

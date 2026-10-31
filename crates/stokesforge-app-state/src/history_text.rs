//! Turning history labels into sentences (issue #46, work package #39).
//!
//! `stokesforge-commands` returns a [`HistoryLabel`] — an action and its
//! parameters — and never a sentence; here, and only here, it becomes a text
//! in the current language, through the `history-*` keys of the `en` catalog.
//! The plurals ("Insert 1 Row" / "Insert 3 Rows") are Fluent's, not Rust's.
//!
//! The `match` has **no** default branch: a variant added to `HistoryLabel`
//! does not compile until it has its sentence, hence its key. The `Display` of
//! `HistoryLabel` (English, for logs and the CLI) duplicates the `en`
//! catalog; `tests/shell.rs` checks that they say the same thing.

use stokesforge_analysis::ops::OperationKind;
use stokesforge_commands::{HistoryLabel, MacroKind, PlotEditKind};

use i18n_embed::fluent::FluentLanguageLoader;

use crate::i18n::loader;

/// A number for Fluent, which does not convert `usize`: saturated at
/// `i64::MAX` (unreachable: a sheet does not hold 9×10¹⁸ rows).
fn number(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// The name of an analysis operation family, in the loader's language.
pub fn kind_title(loader: &FluentLanguageLoader, kind: OperationKind) -> String {
    macro_rules! tr {
        ($($arguments:tt)*) => {
            crate::tr!(in loader; $($arguments)*)
        };
    }
    match kind {
        OperationKind::Baseline => tr!("analysis-kind-baseline"),
        OperationKind::Correct => tr!("analysis-kind-correct"),
        OperationKind::Smooth => tr!("analysis-kind-smooth"),
        OperationKind::Integrate => tr!("analysis-kind-integrate"),
        OperationKind::Fit => tr!("analysis-kind-fit"),
        OperationKind::Peaks => tr!("analysis-kind-peaks"),
        OperationKind::Reconvolution => tr!("analysis-kind-reconvolution"),
        OperationKind::DirectDeconvolution => tr!("analysis-kind-direct-deconvolution"),
    }
}

/// The sentence of a history label, in the current language.
pub fn history_text(label: &HistoryLabel) -> String {
    history_text_in(loader(), label)
}

/// The sentence of a history label in the language of `loader`: a test thus
/// compares English, or the pseudo-language, without depending on the
/// environment's language.
pub fn history_text_in(loader: &FluentLanguageLoader, label: &HistoryLabel) -> String {
    macro_rules! tr {
        ($($arguments:tt)*) => {
            crate::tr!(in loader; $($arguments)*)
        };
    }
    match label {
        HistoryLabel::SetCells { column: None } => tr!("history-set-cells"),
        HistoryLabel::SetCells { column: Some(c) } => {
            tr!("history-set-cells-in-column", column = c.letter())
        }
        HistoryLabel::AddColumn => tr!("history-add-column"),
        HistoryLabel::DeleteColumn { column: None } => tr!("history-delete-column"),
        HistoryLabel::DeleteColumn { column: Some(c) } => {
            tr!("history-delete-column-named", column = c.letter())
        }
        HistoryLabel::Rename {
            from: Some(from),
            to,
        } => tr!("history-rename", from = from.as_str(), to = to.as_str()),
        HistoryLabel::Rename { from: None, to } => {
            tr!("history-rename-unnamed", to = to.as_str())
        }
        HistoryLabel::Move { name: None } => tr!("history-move"),
        HistoryLabel::Move { name: Some(name) } => {
            tr!("history-move-named", name = name.as_str())
        }
        HistoryLabel::SetColumnType { column: None } => tr!("history-set-column-type"),
        HistoryLabel::SetColumnType { column: Some(c) } => {
            tr!("history-set-column-type-of", column = c.letter())
        }
        HistoryLabel::InsertRows { count } => tr!("history-insert-rows", count = number(*count)),
        HistoryLabel::DeleteRows { count } => tr!("history-delete-rows", count = number(*count)),
        HistoryLabel::DeleteCells { count } => tr!("history-delete-cells", count = number(*count)),
        HistoryLabel::AddFolder { name } => tr!("history-add-folder", name = name.as_str()),
        HistoryLabel::AddWorkbook => tr!("history-add-workbook"),
        HistoryLabel::AddEmptyWorkbook => tr!("history-add-empty-workbook"),
        HistoryLabel::AddWorksheet => tr!("history-add-worksheet"),
        HistoryLabel::DuplicateWorksheet { name: None } => tr!("history-duplicate-worksheet"),
        HistoryLabel::DuplicateWorksheet { name: Some(name) } => {
            tr!("history-duplicate-worksheet-named", name = name.as_str())
        }
        HistoryLabel::AddGraph => tr!("history-add-graph"),
        HistoryLabel::AddMatrix => tr!("history-add-matrix"),
        HistoryLabel::AddNote => tr!("history-add-note"),
        HistoryLabel::RemoveNode { name: None } => tr!("history-remove-node"),
        HistoryLabel::RemoveNode { name: Some(name) } => {
            tr!("history-remove-node-named", name = name.as_str())
        }
        HistoryLabel::SetWindowMeta { name: None } => tr!("history-set-window-meta"),
        HistoryLabel::SetWindowMeta { name: Some(name) } => {
            tr!("history-set-window-meta-named", name = name.as_str())
        }
        HistoryLabel::SetNoteText { name: None } => tr!("history-set-note-text"),
        HistoryLabel::SetNoteText { name: Some(name) } => {
            tr!("history-set-note-text-named", name = name.as_str())
        }
        HistoryLabel::SetColumnMeta { column: None } => tr!("history-set-column-meta"),
        HistoryLabel::SetColumnMeta { column: Some(c) } => {
            tr!("history-set-column-meta-of", column = c.letter())
        }
        HistoryLabel::SetPlotSpec { name: None } => tr!("history-set-plot-spec"),
        HistoryLabel::SetPlotSpec { name: Some(name) } => {
            tr!("history-set-plot-spec-named", name = name.as_str())
        }
        HistoryLabel::TransposeRows => tr!("history-transpose-rows"),
        HistoryLabel::RecalculateTranspose => tr!("history-recalculate-transpose"),
        HistoryLabel::EditPlotSpec(kind) => match kind {
            PlotEditKind::Visibility => tr!("history-plot-visibility"),
            PlotEditKind::RenameSeries => tr!("history-plot-rename-series"),
            PlotEditKind::ReorderSeries => tr!("history-plot-reorder-series"),
            PlotEditKind::RemoveSeries => tr!("history-plot-remove-series"),
            PlotEditKind::RemoveText => tr!("history-plot-remove-text"),
            PlotEditKind::RemoveObject => tr!("history-plot-remove-object"),
            PlotEditKind::ReorderObject => tr!("history-plot-reorder-object"),
            PlotEditKind::RenameGroup => tr!("history-plot-rename-group"),
            PlotEditKind::DissolveGroup => tr!("history-plot-dissolve-group"),
            PlotEditKind::RemoveGroup => tr!("history-plot-remove-group"),
            PlotEditKind::ReorderGroup => tr!("history-plot-reorder-group"),
            PlotEditKind::ClosedFrame => tr!("history-plot-closed-frame"),
            PlotEditKind::AddCell => tr!("history-plot-add-cell"),
            PlotEditKind::RemoveCell => tr!("history-plot-remove-cell"),
            PlotEditKind::ResizeCell => tr!("history-plot-resize-cell"),
            PlotEditKind::PlaceFigure => tr!("history-plot-place-figure"),
            PlotEditKind::BringLayerToFront => tr!("history-plot-bring-layer-to-front"),
            PlotEditKind::SendLayerToBack => tr!("history-plot-send-layer-to-back"),
            PlotEditKind::LayerBackground => tr!("history-plot-layer-background"),
            PlotEditKind::MoveLayer => tr!("history-plot-move-layer"),
            PlotEditKind::ResizeLayer => tr!("history-plot-resize-layer"),
        },
        HistoryLabel::SetSavedColors => tr!("history-set-saved-colors"),
        HistoryLabel::ReplaceAnalysisData(kind) => tr!(
            "history-replace-analysis-data",
            kind = kind_title(loader, *kind)
        ),
        HistoryLabel::RestoreOriginalData(Some(kind)) => tr!(
            "history-restore-original-data",
            kind = kind_title(loader, *kind)
        ),
        HistoryLabel::RestoreOriginalData(None) => tr!("history-restore-original-data-any"),
        HistoryLabel::AddCalculation(kind) => {
            tr!("history-add-calculation", kind = kind_title(loader, *kind))
        }
        HistoryLabel::UpdateCalculation(kind) => {
            tr!("history-edit-calculation", kind = kind_title(loader, *kind))
        }
        HistoryLabel::RemoveCalculation {
            kind: Some(kind),
            with_columns: true,
        } => tr!(
            "history-delete-calculation",
            kind = kind_title(loader, *kind)
        ),
        HistoryLabel::RemoveCalculation {
            kind: Some(kind),
            with_columns: false,
        } => tr!(
            "history-detach-calculation",
            kind = kind_title(loader, *kind)
        ),
        HistoryLabel::RemoveCalculation {
            kind: None,
            with_columns: true,
        } => tr!("history-delete-calculation-any"),
        HistoryLabel::RemoveCalculation {
            kind: None,
            with_columns: false,
        } => tr!("history-detach-calculation-any"),
        HistoryLabel::Macro(kind) => match kind {
            MacroKind::PasteCells => tr!("history-macro-paste-cells"),
            MacroKind::PasteObjects => tr!("history-macro-paste-objects"),
            MacroKind::DuplicateObjects => tr!("history-macro-duplicate-objects"),
            MacroKind::DeleteObjects => tr!("history-macro-delete-objects"),
            MacroKind::MoveObjects => tr!("history-macro-move-objects"),
            MacroKind::RenameObjects => tr!("history-macro-rename-objects"),
            MacroKind::ClearSelection => tr!("history-macro-clear-selection"),
            MacroKind::GraphProperties => tr!("history-macro-graph-properties"),
            MacroKind::RefreshFromFile => tr!("history-macro-refresh-from-file"),
            MacroKind::Import => tr!("history-macro-import"),
            MacroKind::AddWorkbook => tr!("history-macro-add-workbook"),
            MacroKind::AddWorksheet => tr!("history-macro-add-worksheet"),
            MacroKind::Script => tr!("history-macro-script"),
            MacroKind::SetColumnsType => tr!("history-macro-set-columns-type"),
            MacroKind::SetColumnsRole => tr!("history-macro-set-columns-role"),
            MacroKind::AnalysisToGraph => tr!("history-macro-analysis-to-graph"),
            MacroKind::RemoveFormula => tr!("history-macro-remove-formula"),
            MacroKind::DeleteColumns => tr!("history-macro-delete-columns"),
            MacroKind::MoveColumns => tr!("history-macro-move-columns"),
            MacroKind::AnalyzeRows => tr!("history-macro-analyze-rows"),
        },
    }
}

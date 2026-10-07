//! Mise en phrase des libellés d'historique (issue #46, chantier #39).
//!
//! `stokesforge-commands` rend un [`HistoryLabel`] — une action et ses paramètres —
//! et jamais une phrase ; c'est ici, et seulement ici, qu'il devient un texte
//! dans la langue courante, par les clés `history-*` du catalogue `en`. Les
//! pluriels (« Insert 1 Row » / « Insert 3 Rows ») sont ceux de Fluent, pas du
//! Rust.
//!
//! Le `match` n'a **pas** de branche par défaut : une variante ajoutée à
//! `HistoryLabel` ne compile pas tant qu'elle n'a pas sa phrase, donc sa clé.
//! `Display` de `HistoryLabel` (anglais, pour les journaux et le CLI) double le
//! catalogue `en` ; `tests/shell.rs` vérifie qu'ils disent la même chose.

use stokesforge_commands::{HistoryLabel, MacroKind, PlotEditKind};

use i18n_embed::fluent::FluentLanguageLoader;

use crate::i18n::loader;

/// Un nombre pour Fluent, qui ne convertit pas `usize` : saturé à `i64::MAX`
/// (inatteignable : une feuille ne porte pas 9×10¹⁸ lignes).
fn number(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// La phrase d'un libellé d'historique, dans la langue courante.
pub fn history_text(label: &HistoryLabel) -> String {
    history_text_in(loader(), label)
}

/// La phrase d'un libellé d'historique dans la langue du chargeur `loader` : un
/// test compare ainsi l'anglais, ou la pseudo-langue, sans dépendre de la
/// langue de l'environnement.
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
        HistoryLabel::EditPlotSpec(kind) => match kind {
            PlotEditKind::Visibility => tr!("history-plot-visibility"),
            PlotEditKind::RenameSeries => tr!("history-plot-rename-series"),
            PlotEditKind::ReorderSeries => tr!("history-plot-reorder-series"),
            PlotEditKind::RemoveSeries => tr!("history-plot-remove-series"),
            PlotEditKind::RemoveText => tr!("history-plot-remove-text"),
            PlotEditKind::RemoveObject => tr!("history-plot-remove-object"),
            PlotEditKind::ReorderObject => tr!("history-plot-reorder-object"),
        },
        HistoryLabel::SetSavedColors => tr!("history-set-saved-colors"),
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
        },
    }
}

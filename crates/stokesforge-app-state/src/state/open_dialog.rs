//! The one signal "a dialog is open in the main window" (#471).
//!
//! Dialogs always belong to the main window (work package #470). A gesture
//! made elsewhere (a detached window, a detached panel) must therefore be
//! refused while one is open, and the window of the gesture says why. This
//! module is the single place that knows which states count as a dialog:
//! [`AppState::open_dialog`] reads the state, not the painters, because the
//! actions of a pass are drained before the dialogs are painted (a paint-based
//! mark would miss a dialog opened at the end of the previous pass).
//!
//! The answer is data ([`OpenDialog`], no egui type, no window identity): the
//! shell publishes it for its viewports, and [`refusal_text`] turns it into
//! the sentence of the band and of the status history.

use super::AppState;
use crate::tr;

/// What the person has to do to get rid of an open dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    /// Edits something and ends with Apply or Cancel (Graph Properties).
    Editing,
    /// Only shows something and ends with Close (About, Preferences...).
    CloseOnly,
    /// A question that waits for an answer (confirmations, alerts).
    Question,
}

/// A dialog open in the main window. Closed set: a new dialog does not compile
/// until it is named here and in [`OpenDialog::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenDialog {
    /// The import window.
    ImportWizard,
    /// The graph properties window (and the series properties).
    GraphProperties,
    /// The analysis window.
    Analysis,
    /// The file metadata of a column.
    ColumnMetadata,
    /// The message history.
    StatusHistory,
    /// The list of the fits with a low R².
    LowR2List,
    /// The keyboard shortcuts.
    KeyboardShortcuts,
    /// The formula functions.
    FormulaFunctions,
    /// The About window.
    About,
    /// The preferences.
    Preferences,
    /// The list of the "column in use" users.
    ColumnUse,
    /// The recovery list.
    Recovery,
    /// A question waiting for an answer: confirmations, alerts, choices.
    Question,
}

impl OpenDialog {
    /// How the dialog ends, which decides the sentence.
    #[must_use]
    pub fn kind(self) -> DialogKind {
        match self {
            Self::ImportWizard | Self::GraphProperties | Self::Analysis => DialogKind::Editing,
            Self::ColumnMetadata
            | Self::StatusHistory
            | Self::LowR2List
            | Self::KeyboardShortcuts
            | Self::FormulaFunctions
            | Self::About
            | Self::Preferences
            | Self::ColumnUse
            | Self::Recovery => DialogKind::CloseOnly,
            Self::Question => DialogKind::Question,
        }
    }

    /// The name of the dialog in the sentence of the band. A question has no
    /// name: it is asked generically.
    fn name(self) -> String {
        match self {
            Self::ImportWizard => tr!("import-window-title"),
            Self::GraphProperties => tr!("dialog-name-graph-properties"),
            Self::Analysis => tr!("dialog-name-analysis"),
            Self::ColumnMetadata => tr!("workbook-metadata-title"),
            Self::StatusHistory => tr!("status-history-title"),
            Self::LowR2List => tr!("alert-low-r2-title"),
            Self::KeyboardShortcuts => tr!("keyboard-shortcuts-title"),
            Self::FormulaFunctions => tr!("formula-help-title"),
            Self::About => tr!("about-title"),
            Self::Preferences => tr!("preferences-title"),
            Self::ColumnUse => tr!("column-use-title"),
            Self::Recovery => tr!("recovery-title"),
            Self::Question => String::new(),
        }
    }
}

/// The sentence that says why a gesture made outside the main window was
/// refused, in the current language.
#[must_use]
pub fn refusal_text(dialog: OpenDialog) -> String {
    let name = dialog.name();
    match dialog.kind() {
        DialogKind::Editing => tr!("refused-dialog-editing", name = name),
        DialogKind::CloseOnly => tr!("refused-dialog-close", name = name),
        DialogKind::Question => tr!("refused-dialog-question"),
    }
}

/// The sentence a detached window shows when one of its gestures (or a message
/// that arrived while the person worked in it) opened `dialog` in the main
/// window (#472).
#[must_use]
pub fn opened_text(dialog: OpenDialog) -> String {
    match dialog.kind() {
        DialogKind::Question => tr!("opened-question-in-main"),
        DialogKind::Editing | DialogKind::CloseOnly => {
            tr!("opened-dialog-in-main", name = dialog.name())
        }
    }
}

impl AppState {
    /// The dialog open in the main window, if any. A question wins over a
    /// dialog, because it is the one that waits for the person.
    #[must_use]
    pub fn open_dialog(&self) -> Option<OpenDialog> {
        if self.blocking_dialog()
            || self.pending_calculation_delete().is_some()
            || self.pending_analysis_replacement()
            || self.pending_transpose_relink().is_some()
            || self.pending_stale_export.is_some()
            || self.pending_full_copy().is_some()
        {
            return Some(OpenDialog::Question);
        }
        let open = [
            (self.import_state().is_some(), OpenDialog::ImportWizard),
            (self.plot_settings().is_some(), OpenDialog::GraphProperties),
            (self.analysis_form().is_some(), OpenDialog::Analysis),
            (
                self.column_metadata_view().is_some(),
                OpenDialog::ColumnMetadata,
            ),
            (self.status_history_open(), OpenDialog::StatusHistory),
            (self.low_r2_list_open(), OpenDialog::LowR2List),
            (
                self.keyboard_shortcuts_open(),
                OpenDialog::KeyboardShortcuts,
            ),
            (self.formula_functions_open(), OpenDialog::FormulaFunctions),
            (self.about_open(), OpenDialog::About),
            (self.preferences_open(), OpenDialog::Preferences),
            (self.column_use.is_some(), OpenDialog::ColumnUse),
            (self.recovery_list_open(), OpenDialog::Recovery),
        ];
        open.into_iter()
            .find_map(|(is_open, dialog)| is_open.then_some(dialog))
    }
}

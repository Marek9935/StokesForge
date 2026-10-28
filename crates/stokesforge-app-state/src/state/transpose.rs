//! Analyze rows (#253): a row is transposed into a linked sheet, then its
//! column is analyzed with the column tools.
//!
//! The gesture (right-click of a "row" curve, Object Manager, Graph ›
//! Selection, workbook Analysis menu on whole rows) opens an undoable
//! `AnalyzeRows` group, creates or completes the transposed sheet
//! (`Command::TransposeRows`), then opens the analysis window on its
//! columns. The group stays open as long as the window is (it is modal):
//! Apply closes it with the calculation, Cancel aborts it and the sheet
//! never existed. The analysis itself has no rule of its own: the result
//! follows exactly the behavior of a column.
//!
//! The same module carries the update of a transposed sheet
//! (`Command::RecalculateTranspose`): without confirmation when it is only
//! out of date, with confirmation when a value of a column created by the
//! link has been edited (the edited values would be overwritten).

use stokesforge_commands::{Command, HistoryLabel, MacroKind};
use stokesforge_core::{NodeId, SeriesRef};

use super::{history_text, AppState};
use crate::analysis::{AnalysisForm, Preset};
use crate::errors;
use crate::tr;

/// What the interface knows about a linked transposed sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransposeInfo {
    /// The source sheet.
    pub source: NodeId,
    /// Its name, as the transposed sheet cites it.
    pub source_name: String,
    /// The source is no longer worth what the link read.
    pub stale: bool,
    /// A value of a column created by the link has been edited.
    pub modified: bool,
}

impl AppState {
    /// The state of the link of `sheet`, if it is a linked transposed sheet.
    pub fn transpose_info(&self, sheet: NodeId) -> Option<TransposeInfo> {
        let tree = &self.snapshot.tree;
        let link = tree.transpose_link(sheet)?;
        let state = link.state(tree);
        Some(TransposeInfo {
            source: link.source,
            source_name: tree.node(link.source)?.name().to_string(),
            stale: state.stale,
            modified: state.modified,
        })
    }

    /// The sheet whose update is waiting for confirmation to overwrite edited
    /// values.
    pub fn pending_transpose_relink(&self) -> Option<NodeId> {
        self.pending_transpose_relink
    }

    /// Opens the analysis on rows `(sheet, row)`: creates or completes the
    /// transposed sheet of each sheet (a single undoable group), then opens
    /// the window on the columns of the transposed one. A refusal (column
    /// without x', text column) is said in the status bar and leaves nothing
    /// behind.
    pub(super) fn open_row_analysis(&mut self, preset: &Preset, rows: &[(NodeId, usize)]) {
        self.abort_row_macro();
        if rows.is_empty() {
            self.fail(tr!("status-analysis-no-series"));
            return;
        }
        if self.handle.begin_macro(MacroKind::AnalyzeRows).is_err() {
            return;
        }
        let mut sheets: Vec<NodeId> = Vec::new();
        for (sheet, _) in rows {
            if !sheets.contains(sheet) {
                sheets.push(*sheet);
            }
        }
        for sheet in &sheets {
            let wanted: Vec<usize> = rows
                .iter()
                .filter(|(s, _)| s == sheet)
                .map(|(_, row)| *row)
                .collect();
            self.execute(Command::TransposeRows {
                source: *sheet,
                rows: wanted,
            });
            if self.last_action_failed() {
                let _ = self.handle.abort_macro();
                self.refresh();
                return;
            }
        }
        self.refresh();
        let tree = &self.snapshot.tree;
        let series: Option<Vec<SeriesRef>> = rows
            .iter()
            .map(|(sheet, row)| {
                let (_, link) = tree.transposed_from(*sheet)?;
                let column = link.rows.iter().find(|r| r.row == *row)?.column;
                Some(SeriesRef {
                    x: Some(link.x_column),
                    y: column,
                    sigma: None,
                })
            })
            .collect();
        let Some(series) = series else {
            let _ = self.handle.abort_macro();
            self.refresh();
            self.fail(tr!("status-analysis-no-series"));
            return;
        };
        self.analysis = Some(AnalysisForm::new(
            preset,
            series,
            self.analysis_keep_results,
        ));
        self.analysis_rows_macro = true;
    }

    /// Closes the `AnalyzeRows` group: the analysis has just been validated.
    pub(super) fn end_row_macro(&mut self) {
        if !std::mem::take(&mut self.analysis_rows_macro) {
            return;
        }
        match self.handle.end_macro() {
            Ok(_) => self.notice(history_text(&HistoryLabel::Macro(MacroKind::AnalyzeRows))),
            Err(err) => self.fail(errors::session::error_message(&err)),
        }
        self.refresh();
    }

    /// Aborts the `AnalyzeRows` group: the window is closed without analysis,
    /// the transposed sheet created for it disappears.
    pub(super) fn abort_row_macro(&mut self) {
        if !std::mem::take(&mut self.analysis_rows_macro) {
            return;
        }
        let _ = self.handle.abort_macro();
        self.refresh();
    }

    /// "Recalculate" of a transposed sheet: right away if it is only out of
    /// date, after confirmation if edited values would be overwritten.
    pub(super) fn recalculate_transpose(&mut self, sheet: NodeId) {
        let Some(info) = self.transpose_info(sheet) else {
            self.fail(tr!("status-transpose-not-linked"));
            return;
        };
        if info.modified {
            self.pending_transpose_relink = Some(sheet);
            return;
        }
        self.execute(Command::RecalculateTranspose { sheet });
    }

    /// Answer to the confirmation of the update.
    pub(super) fn confirm_transpose_relink(&mut self, accept: bool) {
        let Some(sheet) = self.pending_transpose_relink.take() else {
            return;
        };
        if accept {
            self.execute(Command::RecalculateTranspose { sheet });
        }
    }
}

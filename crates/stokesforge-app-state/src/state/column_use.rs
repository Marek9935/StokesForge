//! "Column used by a calculation" (#306): after deleting columns (or a
//! sheet, a workbook) that a calculation reads, an information window says
//! which ones, with two answers: `Close` does nothing, `Undo` undoes the
//! gesture.
//!
//! The gesture is never blocked nor confirmed beforehand: it stays undoable
//! (invariant 1), the list is computed on the tree from **before** (a deleted
//! column has no name any more) and the window opens afterwards, only if the
//! gesture succeeded. The texts are `tr!` here, never in the crates below.
//!
//! Four kinds of readers, and a single exclusion:
//! - a **formula** column of the same sheet that reads a deleted column
//!   (#321: a formula keeps columns by identifier, the following letters no
//!   longer slide; the formula displays `#REF`);
//! - an **analysis calculation** one of whose series (X, Y, σ) is a deleted
//!   column. The hidden reference of a correction (`aux`) does not count: it
//!   is a copy. A calculation whose output column disappears with the
//!   gesture does not count either: it disappears with it.
//!
//! The stored text of a formula is never rewritten
//! (`docs/data-model.md`): the displayed letters follow the columns.

use std::collections::BTreeSet;

use stokesforge_analysis::ops::OperationKind;
use stokesforge_commands::HistoryLabel;
use stokesforge_core::{Node, NodeId, NodeKind, Tree};

use super::AppState;
use crate::analysis::kind_title;
use crate::tr;

/// The answer to the Ctrl+Shift+C question about formula columns (#306).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullColumnsCopy {
    /// Copies the values; the Formula row stays empty.
    Values,
    /// Also copies the formulas.
    Formulas,
    /// Copies nothing.
    Cancel,
}

/// A reader of a deleted column, as data: the text is made at display time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ColumnUser {
    /// A formula column, named by its short name (same sheet).
    Formula { name: String },
    /// An analysis calculation: its family and its first output column,
    /// named `[Workbook]Sheet!Column` (or by its short name in the sheet of a
    /// deleted column).
    Calculation {
        kind: OperationKind,
        output: Option<String>,
    },
}

/// The open window: what it lists, and the undo label of **this** gesture,
/// so that `Undo` undoes nothing else.
#[derive(Debug, Clone)]
pub(super) struct ColumnUseNotice {
    users: Vec<ColumnUser>,
    undo_label: Option<HistoryLabel>,
}

/// The columns (data sheets) under `ids`, `ids` included.
pub(super) fn columns_under(tree: &Tree, ids: &[NodeId]) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = ids.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let Some(node) = tree.node(id) else {
            continue;
        };
        if node.as_column().is_some() {
            out.push(id);
        }
        if let Ok(children) = tree.children(id) {
            stack.extend(children.iter().rev().copied());
        }
    }
    out
}

/// Who reads the `deleted` columns (to compute **before** the deletion).
pub(super) fn column_users(tree: &Tree, deleted: &[NodeId]) -> Vec<ColumnUser> {
    let gone: BTreeSet<NodeId> = deleted.iter().copied().collect();
    let mut users = Vec::new();
    // Formula columns: per sheet, from the smallest deleted rank.
    let sheets: BTreeSet<NodeId> = deleted
        .iter()
        .filter_map(|&c| tree.node(c).and_then(Node::parent))
        .collect();
    for sheet in sheets {
        let Ok(columns) = tree.children(sheet) else {
            continue;
        };
        if !columns.iter().any(|c| gone.contains(c)) {
            continue;
        }
        for &column in columns {
            if gone.contains(&column) {
                continue;
            }
            let Some(node) = tree.node(column) else {
                continue;
            };
            let Some(formula) = node.as_column().and_then(|c| c.meta().formula.as_ref()) else {
                continue;
            };
            if formula.columns().iter().any(|read| gone.contains(read)) {
                users.push(ColumnUser::Formula {
                    name: node.name().to_string(),
                });
            }
        }
    }
    // Calculs d'analyse.
    for calculation in tree.calculations().values() {
        let reads = calculation.series.iter().any(|s| {
            s.x.iter()
                .chain(std::iter::once(&s.y))
                .chain(s.sigma.iter())
                .any(|c| gone.contains(c))
        });
        let output_gone = calculation
            .outputs
            .iter()
            .any(|o| o.column.is_some_and(|c| gone.contains(&c)));
        if !reads || output_gone {
            continue;
        }
        let output = calculation
            .outputs
            .iter()
            .find_map(|o| o.column)
            .and_then(|c| column_path(tree, c));
        users.push(ColumnUser::Calculation {
            kind: calculation.operation.kind(),
            output,
        });
    }
    users
}

/// `[Workbook]Sheet!Column`, `None` if the column has no sheet.
fn column_path(tree: &Tree, column: NodeId) -> Option<String> {
    let node = tree.node(column)?;
    let sheet = tree.node(node.parent()?)?;
    let book = tree.node(sheet.parent()?)?;
    (matches!(sheet.kind(), NodeKind::Worksheet { .. }))
        .then(|| format!("[{}]{}!{}", book.name(), sheet.name(), node.name()))
}

impl AppState {
    /// The columns read, computed before the gesture: `ids` are the deleted
    /// nodes (columns, sheets, workbooks).
    pub(super) fn column_users_before(&self, ids: &[NodeId]) -> Vec<ColumnUser> {
        let tree = &self.snapshot.tree;
        column_users(tree, &columns_under(tree, ids))
    }

    /// Opens the window after a successful gesture that orphaned `users`.
    /// Nothing if the list is empty or if the gesture failed.
    pub(super) fn open_column_use(&mut self, users: Vec<ColumnUser>) {
        if users.is_empty() || self.last_action_failed() {
            return;
        }
        self.column_use = Some(ColumnUseNotice {
            users,
            undo_label: self.snapshot.history.undo_label.clone(),
        });
    }

    /// The lines of the "column used" window, open or not.
    pub fn column_use_lines(&self) -> Option<Vec<String>> {
        let notice = self.column_use.as_ref()?;
        Some(
            notice
                .users
                .iter()
                .map(|user| match user {
                    ColumnUser::Formula { name } => {
                        tr!("column-use-formula", name = name.as_str())
                    }
                    ColumnUser::Calculation {
                        kind,
                        output: Some(output),
                    } => tr!(
                        "column-use-calculation",
                        kind = kind_title(*kind),
                        output = output.as_str()
                    ),
                    ColumnUser::Calculation { kind, output: None } => {
                        tr!("column-use-calculation-bare", kind = kind_title(*kind))
                    }
                })
                .collect(),
        )
    }

    /// `Close`: does nothing.
    pub(super) fn close_column_use(&mut self) {
        self.column_use = None;
    }

    /// `Undo`: undoes the gesture, and only it; if the stack no longer has its
    /// label on top, close without undoing anything.
    pub(super) fn undo_column_use(&mut self) {
        let Some(notice) = self.column_use.take() else {
            return;
        };
        if self.snapshot.history.undo_label == notice.undo_label {
            self.undo();
        }
    }

    /// Answer to the Ctrl+Shift+C question: the copy is done now, and its text
    /// goes to the system clipboard through the objects path.
    pub(super) fn confirm_full_columns_copy(&mut self, choice: FullColumnsCopy) {
        let Some(sheet) = self.pending_full_copy.take() else {
            return;
        };
        let keep_formula = match choice {
            FullColumnsCopy::Values => false,
            FullColumnsCopy::Formulas => true,
            FullColumnsCopy::Cancel => return,
        };
        if let Some(text) = self.copy_full_columns_with(sheet, keep_formula) {
            self.object_clipboard_output = Some(text);
        }
    }
}

//! Workbook editing from pyst (#331, batch 2, S5): rows, column type,
//! move, duplication, text of a note.
//!
//! No rule here: each call sends the undoable command that the interface
//! also uses (`InsertRows`, `DeleteRows`, `SetColumnType`, `Move`,
//! `DuplicateWorksheet`/`InsertCopy`, `SetNoteText`), in the script's
//! undoable group. Handles are revalidated under the reservation.
//! A column write by the script updates the version it had read of that
//! column: its following writes do not refuse themselves.

use pyo3::prelude::*;
use stokesforge_commands::Command;
use stokesforge_core::{same_name_ignoring_case, NodeCopy, NodeId};

use crate::bridge::{self, HostError};
use crate::engine::{self, ScriptContext};
use crate::graph::invalid;
use crate::surface::{to_py_error, Handle};

/// The columns of worksheet `sheet` (the ones that rows touch).
fn sheet_columns(
    context: &ScriptContext,
    sheet: NodeId,
    epoch: u64,
) -> Result<Vec<NodeId>, HostError> {
    let snapshot = context
        .session()
        .snapshot()
        .map_err(HostError::from_session)?;
    Ok(bridge::checked_node(&snapshot, sheet, epoch)?
        .children()
        .iter()
        .copied()
        .filter(|id| {
            snapshot
                .tree
                .node(*id)
                .is_some_and(|node| node.as_column().is_some())
        })
        .collect())
}

/// Inserts (`insert`) or deletes rows at the worksheet level.
fn edit_rows(
    context: &ScriptContext,
    sheet: NodeId,
    epoch: u64,
    at: usize,
    count: usize,
    insert: bool,
) -> Result<(), HostError> {
    let snapshot = bridge::write_snapshot(context, sheet, epoch)?;
    if !bridge::checked_node(&snapshot, sheet, epoch)?
        .kind()
        .is_worksheet()
    {
        return Err(HostError::InvalidValue(format!(
            "{sheet} is not a worksheet"
        )));
    }
    // The model silently ignores a range outside all the columns; for a
    // script, that is a typo that is better reported.
    let rows = sheet_columns(context, sheet, epoch)?
        .into_iter()
        .filter_map(|id| snapshot.tree.node(id)?.as_column().map(|c| c.len()))
        .max()
        .unwrap_or(0);
    if (insert && at > rows) || (!insert && at >= rows) {
        return Err(HostError::InvalidValue(format!(
            "row {at} is outside the worksheet, which has {rows} rows"
        )));
    }
    let command = if insert {
        Command::InsertRows { sheet, at, count }
    } else {
        Command::DeleteRows { sheet, at, count }
    };
    context
        .session()
        .execute(command)
        .map_err(HostError::from_session)?;
    for column in sheet_columns(context, sheet, epoch)? {
        bridge::retain_written_version(context, column, epoch)?;
    }
    Ok(())
}

fn check_row_count(count: usize) -> PyResult<()> {
    let limit = stokesforge_core::column::MAX_COLUMN_ROWS;
    if count > limit {
        return Err(invalid(format!(
            "row count {count} exceeds the limit of {limit} rows"
        )));
    }
    Ok(())
}

/// Inserts `count` empty rows (one by default) at row `at` (0 = at the top)
/// of the worksheet: all its columns of at least `at` rows grow, the following
/// cells move down. An `at` equal to the worksheet's row count appends at the
/// end; a column shorter than `at` is not touched. Undoable with
/// `pyst.undo()`.
///
/// Raises `InvalidValueError` for a handle that is not a worksheet, an
/// `at` past the last row or a total above the model's cap;
/// `BrokenReferenceError` for a stale handle.
#[pyfunction]
#[pyo3(signature = (worksheet, at, count=1))]
pub(crate) fn insert_rows(
    py: Python<'_>,
    worksheet: &Handle,
    at: usize,
    count: usize,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    check_row_count(count)?;
    let (id, epoch) = (worksheet.id, worksheet.epoch);
    py.detach(|| edit_rows(&context, id, epoch, at, count, true))
        .map_err(to_py_error)
}

/// Deletes `count` rows (one by default) starting at row `at` (0 = the
/// first) in the worksheet's columns that have a row `at`, and moves up
/// what follows. Undoable with `pyst.undo()`, which returns the removed cells.
///
/// Raises `InvalidValueError` for a handle that is not a worksheet or an
/// `at` past the last row; `BrokenReferenceError` for a stale handle.
#[pyfunction]
#[pyo3(signature = (worksheet, at, count=1))]
pub(crate) fn delete_rows(
    py: Python<'_>,
    worksheet: &Handle,
    at: usize,
    count: usize,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (worksheet.id, worksheet.epoch);
    py.detach(|| edit_rows(&context, id, epoch, at, count, false))
        .map_err(to_py_error)
}

/// Changes a column's type: `'numeric'`, `'datetime'` or `'mixed'` (text
/// columns are `'mixed'`). **The conversion can lose values** (a text turned
/// into `'numeric'` does not read back); `pyst.undo()` restores type and
/// values. Role, unit and name stay. A formula column refuses the conversion.
///
/// Raises `InvalidValueError` for an unknown type or a handle that is not
/// a column; `BrokenReferenceError` for a stale handle.
#[pyfunction]
#[pyo3(signature = (column, column_type))]
pub(crate) fn set_column_type(py: Python<'_>, column: &Handle, column_type: &str) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let column_type = crate::surface::parse_column_type(column_type)?;
    let (id, epoch) = (column.id, column.epoch);
    py.detach(|| -> Result<(), HostError> {
        let snapshot = bridge::write_snapshot(&context, id, epoch)?;
        if bridge::checked_node(&snapshot, id, epoch)?
            .as_column()
            .is_none()
        {
            return Err(HostError::InvalidValue(format!("{id} is not a column")));
        }
        context
            .session()
            .execute(Command::SetColumnType {
                column: id,
                column_type,
            })
            .map_err(HostError::from_session)?;
        bridge::retain_written_version(&context, id, epoch)
    })
    .map_err(to_py_error)
}

/// Moves an object under `parent` (folder, workbook or worksheet depending on
/// its kind) to position `index` among `parent`'s other children; without
/// `index`, last. With the same parent, it is a plain reordering.
/// The handle stays valid. Undoable with `pyst.undo()`.
///
/// Raises `InvalidValueError` if `parent` does not accept this kind of object,
/// if the move is circular, if the name is already taken in `parent`, if
/// `index` is out of bounds or if the object is the root; `BrokenReferenceError`
/// for a stale handle.
#[pyfunction]
#[pyo3(name = "move", signature = (handle, parent, index=None))]
pub(crate) fn move_node(
    py: Python<'_>,
    handle: &Handle,
    parent: &Handle,
    index: Option<usize>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    let (parent_id, parent_epoch) = (parent.id, parent.epoch);
    py.detach(|| -> Result<(), HostError> {
        let snapshot = bridge::write_snapshot(&context, id, epoch)?;
        let target = bridge::checked_node(&snapshot, parent_id, parent_epoch)?;
        let node = bridge::checked_node(&snapshot, id, epoch)?;
        let others = target.children().len() - usize::from(node.parent() == Some(parent_id));
        context
            .session()
            .execute(Command::Move {
                id,
                new_parent: parent_id,
                index: index.unwrap_or(others),
            })
            .map(|_| ())
            .map_err(HostError::from_session)
    })
    .map_err(to_py_error)
}

/// The first free "<name> copy N" among `parent`'s children.
fn free_copy_name(
    snapshot: &stokesforge_session::Snapshot,
    parent: NodeId,
    origin: &str,
) -> String {
    let taken = |name: &str| {
        snapshot
            .tree
            .children(parent)
            .unwrap_or_default()
            .iter()
            .any(|id| {
                snapshot
                    .tree
                    .node(*id)
                    .is_some_and(|node| same_name_ignoring_case(node.name(), name))
            })
    };
    (1u64..)
        .map(|number| format!("{origin} copy {number}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| origin.to_owned())
}

/// Duplicates an object (and its descendants, with new identifiers) in the same
/// parent and returns the copy's handle. A worksheet is copied right after
/// the original, other objects at the end of the parent. `name` names the copy;
/// without it, the first free "<name> copy N". Undoable with `pyst.undo()`.
///
/// Raises `InvalidValueError` for the root, a refused or already taken name,
/// or an object the model does not copy; `BrokenReferenceError` for a stale
/// handle.
#[pyfunction]
#[pyo3(signature = (handle, name=None))]
pub(crate) fn duplicate(py: Python<'_>, handle: &Handle, name: Option<String>) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    let created = py
        .detach(|| {
            let snapshot = bridge::write_snapshot(&context, id, epoch)?;
            let node = bridge::checked_node(&snapshot, id, epoch)?;
            let parent = node.parent().ok_or_else(|| {
                HostError::InvalidValue("the project root cannot be duplicated".into())
            })?;
            let name = name.unwrap_or_else(|| free_copy_name(&snapshot, parent, node.name()));
            let command = if node.kind().is_worksheet() {
                Command::DuplicateWorksheet { sheet: id, name }
            } else {
                Command::InsertCopy {
                    parent,
                    copy: NodeCopy::capture(&snapshot.tree, id).map_err(HostError::from_core)?,
                    name,
                }
            };
            let before = snapshot.tree.children(parent).unwrap_or_default().to_vec();
            context
                .session()
                .execute(command)
                .map_err(HostError::from_session)?;
            // Under the script's macro, nobody else writes: the child that
            // appeared is the copy.
            let after = context
                .session()
                .snapshot()
                .map_err(HostError::from_session)?;
            after
                .tree
                .children(parent)
                .unwrap_or_default()
                .iter()
                .copied()
                .find(|child| !before.contains(child))
                .ok_or_else(|| HostError::InvalidValue("the copy was not created".into()))
        })
        .map_err(to_py_error)?;
    Ok(Handle { id: created, epoch })
}

/// Replaces a note's text (`text` is the whole text, line breaks
/// included). Undoable with `pyst.undo()`.
///
/// Raises `InvalidValueError` if the handle is not a note;
/// `BrokenReferenceError` for a stale handle.
#[pyfunction]
#[pyo3(signature = (note, text))]
pub(crate) fn set_note(py: Python<'_>, note: &Handle, text: String) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (note.id, note.epoch);
    py.detach(|| bridge::mutate_node(&context, id, epoch, Command::SetNoteText { id, text }))
        .map_err(to_py_error)
}

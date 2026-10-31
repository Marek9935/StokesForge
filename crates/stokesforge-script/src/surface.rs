//! The `pyst` surface: project navigation and reading, then the foundation's writes.
//!
//! The Python work package (#65, batches #66 and #67) extends what exists; the old archived
//! v1 API is no longer a target to copy. Handles and reads
//! work without view state, equally for the console and for a
//! session with no interface.
//!
//! Two distinct reads avoid an implicit conversion: `column`
//! keeps its real numeric numpy array without copying; `values` copies the
//! cells into a Python tuple, without losing the texts or confusing `None`,
//! an empty string and a NaN number. Metadata are dictionaries
//! detached from the model: changing them is never a write.
//!
//! # Error vocabulary
//!
//! Four exceptions, and only four: [`UnknownPathError`],
//! [`BrokenReferenceError`], [`InvalidValueError`], [`ConcurrentWriteError`].
//! The first three are taken as is from the v1 API specification, so as not to
//! rename them later; the fourth anticipates it (A2, `guide-de-correction-robustesse.md` §F6, then #312):
//! `set_column` and `set_values` refuse at once a write whose column
//! changed since it was read, and must tell this conflict apart from the other refusals. Every
//! host error (`SessionError`, `CommandError`, `CoreError`) is
//! **translated** into one of them (see [`crate::bridge`]), never carried
//! as is across the actor's channel.
//!
//! # Where the GIL is released
//!
//! Each call prepares its
//! arguments in pure Rust, then goes through `Python::detach` to wait for
//! the actor. The `Ungil` marker forbids at compile time that a GIL-bound value
//! enters this block or comes out of it.

use numpy::PyArray1;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple};
use pyo3::IntoPyObjectExt;
use pyo3::{create_exception, wrap_pyfunction};
use stokesforge_commands::Command;
use stokesforge_core::{
    ColumnRole, ColumnType, ColumnValues, MixedCell, NodeId, NodeKind, ValueBlock,
};
use stokesforge_python::array::{borrowed_readonly_array, copy_array_to_vec};

use crate::bridge::{self, HostError};
use crate::engine;

create_exception!(
    pyst,
    UnknownPathError,
    PyException,
    "The given path is malformed, or does not designate any object of the project."
);
create_exception!(
    pyst,
    BrokenReferenceError,
    PyException,
    "The handle designates an object absent from the current project (deleted, or from a reloaded project)."
);
create_exception!(
    pyst,
    InvalidValueError,
    PyException,
    "Value refused: wrong type, wrong length, or object of an unexpected kind."
);
create_exception!(
    pyst,
    ConcurrentWriteError,
    PyException,
    "pyst.set_column or pyst.set_values was refused because another client modified the \
     column after the script read it. This is neither a script bug nor a refused value: \
     read the column again, then write."
);

/// Translates a host error, already classified, into a Python exception.
pub(crate) fn to_py_error(error: HostError) -> PyErr {
    match error {
        HostError::UnknownPath(message) => UnknownPathError::new_err(message),
        HostError::BrokenReference(message) => BrokenReferenceError::new_err(message),
        HostError::InvalidValue(message) => InvalidValueError::new_err(message),
        HostError::ConcurrentWrite(message) => ConcurrentWriteError::new_err(message),
    }
}

/// **Opaque** handle to a project object.
///
/// A script can neither inspect nor forge it: it has no
/// Python constructor and exposes no integer. [`resolve`], [`root`],
/// [`children`], [`info`] and [`import_file`] produce them.
///
/// A handle stays valid after a rename, after a move, and after
/// a deletion **followed by an undo**: this is the identity contract,
/// ensured by the model itself (monotonic identifiers, never reused).
/// Between the deletion and the undo, using it raises
/// `BrokenReferenceError`.
///
/// **It does not, however, survive a project reload** (A3,
/// `guide-de-correction-robustesse.md` §F2/F6): `epoch` remembers the project's
/// identity at the time of [`resolve`], and every function that receives a
/// `Handle` compares it with the current epoch **before** looking for `id` in
/// the tree; otherwise a `NodeId` that exists by coincidence in the
/// new project (same file shape, same identifiers reassigned)
/// would silently designate a completely different object.
#[pyclass(frozen, module = "pyst", name = "Handle")]
#[derive(Debug)]
pub struct Handle {
    /// Designated node. Private field: nothing, from Python, gives access to it.
    pub(crate) id: NodeId,
    /// Project epoch at the time [`resolve`] produced this handle.
    pub(crate) epoch: u64,
}

#[pymethods]
impl Handle {
    /// Readable representation, for example `Handle(node 412)`.
    fn __repr__(&self) -> String {
        format!("Handle({})", self.id)
    }
}

/// Resolves an object path and returns an opaque handle.
///
/// The grammar is StokesForge's: `[Book1]Sheet1!ColA`, possibly
/// prefixed with folders (`/Runs/[Book1]Sheet1!ColA`).
///
/// Raises `UnknownPathError` if the path is malformed **or** designates
/// nothing: for a script, both cases are the same dead end.
#[pyfunction]
#[pyo3(text_signature = "(path)")]
fn resolve(py: Python<'_>, path: &str) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let session = context.session();
    let (id, epoch) = py
        .detach(|| bridge::resolve(session, path))
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Returns the handle of the current project's root folder.
#[pyfunction]
fn root(py: Python<'_>) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = py
        .detach(|| bridge::root(context.session()))
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Returns the direct children in their order: folders and windows of a
/// folder, sheets of a workbook, columns of a sheet. Without a handle,
/// starts from the root folder. A column or another sheet of the tree returns
/// an empty list. A stale handle raises `BrokenReferenceError`.
#[pyfunction]
#[pyo3(signature = (handle=None))]
fn children(py: Python<'_>, handle: Option<&Handle>) -> PyResult<Vec<Handle>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let handle = handle.map(|handle| (handle.id, handle.epoch));
    let children = py
        .detach(|| bridge::children(context.session(), handle))
        .map_err(to_py_error)?;
    Ok(children
        .into_iter()
        .map(|(id, epoch)| Handle { id, epoch })
        .collect())
}

/// Describes a node through a dictionary independent of the model: `name`, `kind`,
/// `parent` (handle or None), `row_count` (sheet/column, otherwise None),
/// `column` (a column's metadata, otherwise None). A sheet's row count
/// is the maximum of its columns' lengths, zero if it is empty.
/// The metadata include `type`, `role`, `long_name`, `unit`, `comment`, `x_prime`,
/// `display_format`, `formula` (in letters) and `version`. A stale handle raises
/// `BrokenReferenceError`; no read changes the history.
#[pyfunction]
fn info<'py>(py: Python<'py>, handle: &Handle) -> PyResult<Bound<'py, PyDict>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let id = handle.id;
    let epoch = handle.epoch;
    let (node, row_count, formula) = py
        .detach(|| bridge::info(context.session(), id, epoch))
        .map_err(to_py_error)?;
    let result = PyDict::new(py);
    result.set_item("name", node.name())?;
    result.set_item(
        "kind",
        match node.kind() {
            NodeKind::Folder { .. } => "folder",
            NodeKind::Workbook { .. } => "workbook",
            NodeKind::Worksheet { .. } => "worksheet",
            NodeKind::Column(_) => "column",
            NodeKind::Graph { .. } => "graph",
            NodeKind::Matrix { .. } => "matrix",
            NodeKind::Note { .. } => "note",
            NodeKind::Unknown(_) => "unknown",
        },
    )?;
    result.set_item("parent", node.parent().map(|id| Handle { id, epoch }))?;
    result.set_item("row_count", row_count)?;
    if let Some(column) = node.as_column() {
        let meta = column.meta();
        let description = PyDict::new(py);
        description.set_item(
            "type",
            match column.column_type() {
                ColumnType::Numeric => "numeric",
                ColumnType::DateTime => "datetime",
                ColumnType::Mixed => "mixed",
            },
        )?;
        description.set_item(
            "role",
            match meta.role {
                ColumnRole::X => "x",
                ColumnRole::Y => "y",
                ColumnRole::X2 => "x2",
                ColumnRole::Y2 => "y2",
                ColumnRole::X3 => "x3",
                ColumnRole::Y3 => "y3",
                ColumnRole::X4 => "x4",
                ColumnRole::Y4 => "y4",
                ColumnRole::Z => "z",
                ColumnRole::XError => "x_error",
                ColumnRole::YError => "y_error",
                ColumnRole::Label => "label",
                ColumnRole::None => "none",
            },
        )?;
        description.set_item("long_name", meta.long_name.as_str())?;
        description.set_item("unit", meta.unit.as_str())?;
        description.set_item("comment", meta.comment.as_str())?;
        description.set_item("x_prime", meta.x_prime)?;
        description.set_item("display_format", meta.display_format.as_str())?;
        // In letters: today's, `#REF` for a read column that
        // is no longer in the sheet (a `#REF` formula is not rewritten).
        description.set_item("formula", formula.as_deref())?;
        description.set_item("version", column.version())?;
        result.set_item("column", description)?;
    } else {
        result.set_item("column", py.None())?;
    }
    Ok(result)
}

/// Copies a column's stored cells into a Python tuple: `float`,
/// `str` or `None`. Never reparses a numeric text and keeps the
/// distinctions between empty cell, empty string and NaN number. Dates
/// stay POSIX timestamps in seconds. The tuple is a snapshot;
/// for a numeric read without copy, use `column`.
/// Raises `InvalidValueError` on another node, `BrokenReferenceError` on a
/// stale handle.
#[pyfunction]
fn values<'py>(py: Python<'py>, handle: &Handle) -> PyResult<Bound<'py, PyTuple>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let id = handle.id;
    let epoch = handle.epoch;
    let (values, version) = py
        .detach(|| bridge::column_values(context.session(), id, epoch))
        .map_err(to_py_error)?;
    context.remember_version(id, epoch, version);
    let cells: Vec<Bound<'py, PyAny>> = match values {
        ColumnValues::Float(values) => values
            .iter()
            .map(|value| value.into_bound_py_any(py))
            .collect::<PyResult<_>>()?,
        ColumnValues::Mixed(values) => values
            .iter()
            .map(|cell| match cell {
                MixedCell::Empty => Ok(py.None().into_bound(py)),
                MixedCell::Number(value) => value.into_bound_py_any(py),
                MixedCell::Text(text) => text.into_bound_py_any(py),
            })
            .collect::<PyResult<_>>()?,
    };
    PyTuple::new(py, cells)
}

/// Returns a numpy view, **read-only and without copy**, on the column's
/// values.
///
/// The array points at the model's real memory: not a byte is
/// duplicated, whatever the column's size.
///
/// **A view is a snapshot, not a subscription.** A view taken before a
/// write keeps showing the old values, silently. Simple rule
/// that avoids the whole problem: do not keep a view longer
/// than a calculation, and read the column again after any write.
///
/// Raises `BrokenReferenceError` if the handle designates nothing anymore, and
/// `InvalidValueError` if it does not designate a numeric column (a
/// sheet, a workbook, a text column).
#[pyfunction]
#[pyo3(text_signature = "(handle)")]
fn column<'py>(py: Python<'py>, handle: &Handle) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let session = context.session();
    let id = handle.id;
    let epoch = handle.epoch;
    let (buffer, version) = py
        .detach(|| bridge::float_buffer(session, id, epoch))
        .map_err(to_py_error)?;
    context.remember_version(id, epoch, version);
    borrowed_readonly_array(py, &buffer)
}

/// Writes `values` into the column, **through the command bus**: the write
/// enters the history and is undone by `pyst.undo()` as by the window's
/// `Ctrl+Z`.
///
/// `values` must be a one-dimensional `float64` numpy array; non-contiguous
/// arrays (`v[::2]`) are accepted. The values are
/// **copied** to Rust: the model owns its buffers and never depends
/// on the lifetime of a Python object.
///
/// The length must stay exactly the column's. Any other
/// length raises `InvalidValueError` before any command. To change the
/// length and write text, use `set_values`.
///
/// Protected write: the column must be unchanged since the script's last
/// read (`column`/`values`), otherwise `ConcurrentWriteError`
/// at once, with no retry and no overwrite. `expected_version` (optional,
/// the `version` returned by `info`) replaces this read version. A
/// column the script never read is not protected. `undo()` forgets the
/// versions read.
#[pyfunction]
#[pyo3(signature = (handle, values, *, expected_version=None))]
fn set_column(
    py: Python<'_>,
    handle: &Handle,
    values: &Bound<'_, PyAny>,
    expected_version: Option<u64>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let id = handle.id;
    let epoch = handle.epoch;
    // The copy happens with the GIL in hand; the resulting `Vec<f64>` is pure Rust, it
    // can cross the wait for the actor and be dropped on any
    // thread.
    if let Ok(array) = values.cast::<numpy::PyUntypedArray>() {
        use numpy::PyUntypedArrayMethods;
        check_value_count(array.len())?;
    }
    let values =
        copy_array_to_vec(values).map_err(|error| InvalidValueError::new_err(error.to_string()))?;
    py.detach(move || bridge::set_column(&context, id, epoch, values, expected_version))
        .map_err(to_py_error)
}

/// Undoes one step of the session's history stack.
///
/// Returns `True` if something was undone, **`False` if there was nothing**,
/// never an exception: an empty stack is not an error, it is the case the
/// interface translates as a greyed-out button. It is also what is written
/// naturally in an "undo while there is something" loop.
///
/// There is **only one stack** in the session: `pyst.undo()` undoes a
/// command that came from the interface just as well as one from the script.
#[pyfunction]
#[pyo3(text_signature = "()")]
fn undo(py: Python<'_>) -> PyResult<bool> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    py.detach(|| bridge::undo(&context)).map_err(to_py_error)
}

/// Redoes the most recently undone entry, symmetric to `undo`.
///
/// Returns `True` if something was redone, `False` if there was nothing
/// (never an exception). A write group opened by the script is
/// first committed, which empties the redo stack: a `redo()` placed
/// right after writes therefore returns `False`. It redoes a command from the
/// interface just as well as a command from the script.
#[pyfunction]
#[pyo3(text_signature = "()")]
fn redo(py: Python<'_>) -> PyResult<bool> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    py.detach(|| bridge::redo(&context)).map_err(to_py_error)
}

/// Saves the project at the current path (that of the file opened in
/// the application, or of the last `save_as`).
///
/// The script's write group is first **committed** (like `undo`): what
/// precedes the save is no longer undone if the script fails afterward.
/// Saving is not an undoable command and does not touch the
/// history. Handles stay valid.
///
/// Raises `InvalidValueError` if the project has no path yet (use
/// `save_as`), or if the write fails.
#[pyfunction]
#[pyo3(text_signature = "()")]
fn save(py: Python<'_>) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    py.detach(|| bridge::save(&context, None))
        .map_err(to_py_error)
}

/// Saves the project under `path` (text or `pathlib.Path`), which becomes the
/// current path of `save()`. Same write-group commit and same
/// errors as `save`. Handles stay valid.
#[pyfunction]
#[pyo3(text_signature = "(path)")]
fn save_as(py: Python<'_>, path: std::path::PathBuf) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    py.detach(|| bridge::save(&context, Some(&path)))
        .map_err(to_py_error)
}

/// Exports a graph to a file, with the same rendering as the
/// application's export (Typst included). The format comes from the extension of `path`
/// (text or `pathlib.Path`): `.svg` (text kept as text), `.pdf` or
/// `.png` (`png_scale`: factor applied to the figure's size, 4 by
/// default like the application). Does not change the project.
///
/// Raises `InvalidValueError` for an unknown extension, a non-positive
/// `png_scale`, a handle that is not a graph, or an impossible write;
/// `BrokenReferenceError` for a stale handle.
#[pyfunction]
#[pyo3(signature = (graph, path, *, png_scale=bridge::DEFAULT_PNG_SCALE))]
fn export_graph(
    py: Python<'_>,
    graph: &Handle,
    path: std::path::PathBuf,
    png_scale: f32,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| bridge::export_graph(context.session(), id, epoch, &path, png_scale))
        .map_err(to_py_error)
}

/// Imports a file into a new workbook and returns the handles of its
/// sheets. `recipe` names a saved template; without this name,
/// the template is chosen automatically, otherwise the permissive recipe.
/// Columns, roles, metadata and source are those of the wizard.
/// A single Ctrl+Z (or `pyst.undo()`) undoes the whole import. An unreadable file
/// or an unknown template raises `InvalidValueError` without touching the project.
#[pyfunction]
#[pyo3(signature = (path, recipe=None))]
fn import_file(
    py: Python<'_>,
    path: std::path::PathBuf,
    recipe: Option<String>,
) -> PyResult<Vec<Handle>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let sheets = py
        .detach(|| bridge::import_file(&context, &path, recipe.as_deref()))
        .map_err(to_py_error)?;
    Ok(sheets
        .into_iter()
        .map(|(id, epoch)| Handle { id, epoch })
        .collect())
}

/// Creates an empty workbook in `parent` (folder), or in the root.
/// Without a name, uses the first free BookN. Returns a stable handle.
#[pyfunction]
#[pyo3(signature = (name=None, parent=None))]
fn create_workbook(
    py: Python<'_>,
    name: Option<String>,
    parent: Option<&Handle>,
) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let parent = parent.map(|h| (h.id, h.epoch));
    let (id, epoch) = py
        .detach(|| {
            bridge::create_node(&context, parent, |parent| Command::AddEmptyWorkbook {
                parent,
                name,
            })
        })
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Creates an empty sheet at the end of the workbook. Without a name, uses SheetN.
#[pyfunction]
#[pyo3(signature = (workbook, name=None))]
fn create_worksheet(py: Python<'_>, workbook: &Handle, name: Option<String>) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let parent = (workbook.id, workbook.epoch);
    let (id, epoch) = py
        .detach(|| {
            bridge::create_node(&context, Some(parent), |book| Command::AddWorksheet {
                book,
                name,
            })
        })
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// The column type named on the script side: `numeric`, `datetime` or `mixed`.
pub(crate) fn parse_column_type(name: &str) -> PyResult<ColumnType> {
    match name {
        "numeric" => Ok(ColumnType::Numeric),
        "datetime" => Ok(ColumnType::DateTime),
        "mixed" => Ok(ColumnType::Mixed),
        _ => Err(InvalidValueError::new_err(
            "unknown column type: use numeric, datetime or mixed",
        )),
    }
}

/// Creates an empty column at the end of the sheet, of type numeric, datetime or
/// mixed. Text columns use mixed. Without a name, uses A, B, C...
#[pyfunction]
#[pyo3(signature = (worksheet, name=None, column_type="numeric"))]
fn create_column(
    py: Python<'_>,
    worksheet: &Handle,
    name: Option<String>,
    column_type: &str,
) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let column_type = parse_column_type(column_type)?;
    let parent = (worksheet.id, worksheet.epoch);
    let (id, epoch) = py
        .detach(|| {
            bridge::create_node(&context, Some(parent), |sheet| Command::AddColumn {
                sheet,
                index: None,
                name,
                column: stokesforge_core::Column::new(column_type),
            })
        })
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Renames an object without changing its handle. The model's name rules
/// apply: non-empty name, at most 255 characters, unique within the parent.
#[pyfunction]
fn rename(py: Python<'_>, handle: &Handle, name: String) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::mutate_node(&context, id, epoch, Command::Rename { id, name }))
        .map_err(to_py_error)
}

/// Deletes the object and its descendants. The handle becomes valid again after
/// an undo. The project root cannot be deleted.
#[pyfunction]
fn remove(py: Python<'_>, handle: &Handle) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::mutate_node(&context, id, epoch, Command::RemoveNode { id }))
        .map_err(to_py_error)
}

/// Explicit copy of the inputs before any wait for the actor. The numpy path
/// keeps the existing checks: float64, one dimension, mask refused.
/// The iterable path keeps texts as they are, even "12" and "".
fn copy_cells(values: &Bound<'_, PyAny>) -> PyResult<ValueBlock> {
    let invalid = |message: String| InvalidValueError::new_err(message);
    if let Ok(array) = values.cast::<numpy::PyUntypedArray>() {
        use numpy::PyUntypedArrayMethods;
        check_value_count(array.len())?;
        return copy_array_to_vec(values)
            .map(ValueBlock::Float)
            .map_err(|err| invalid(err.to_string()));
    }
    if values.is_instance_of::<pyo3::types::PyString>() {
        return Err(invalid(
            "expected an iterable of cells, not a single string".into(),
        ));
    }
    // Do not trust __len__ to bound the iteration: a generator has none,
    // and a user object can lie. Counting protects against both.
    if let Ok(length) = values.len() {
        check_value_count(length)?;
    }
    let mut cells = Vec::new();
    for value in values
        .try_iter()
        .map_err(|_| invalid("expected an iterable of float, int, str or None cells".into()))?
    {
        let value = value?;
        check_value_count(cells.len() + 1)?;
        let cell = if value.is_none() {
            MixedCell::Empty
        } else if value.is_instance_of::<pyo3::types::PyString>() {
            MixedCell::Text(value.extract::<String>()?)
        } else if value.is_instance_of::<pyo3::types::PyBool>() {
            // bool is a subclass of int: test for it first, otherwise
            // True/False would be silently written as 1.0/0.0.
            return Err(invalid(
                "bool is not a valid cell: convert it explicitly to int or float".into(),
            ));
        } else if value.is_instance_of::<pyo3::types::PyInt>()
            || value.is_instance_of::<pyo3::types::PyFloat>()
        {
            let number = value
                .extract::<f64>()
                .map_err(|err| invalid(err.to_string()))?;
            if value.is_instance_of::<pyo3::types::PyInt>()
                && value
                    .extract::<i64>()
                    .map_or(true, |n| n.unsigned_abs() > 9_007_199_254_740_992)
            {
                return Err(invalid(
                    "integer outside the exact float64 range: convert it explicitly before writing"
                        .into(),
                ));
            }
            MixedCell::Number(number)
        } else {
            return Err(invalid(
                "expected a float, int, str or None cell; convert other values explicitly".into(),
            ));
        };
        cells.push(cell);
    }
    Ok(ValueBlock::Mixed(cells))
}

/// Refuses before copying large arrays or crossing the actor. The
/// cap is the model's, not a second value to keep in agreement.
fn check_value_count(count: usize) -> PyResult<()> {
    let limit = stokesforge_core::column::MAX_COLUMN_ROWS;
    if count > limit {
        return Err(InvalidValueError::new_err(format!(
            "column length {count} exceeds the limit of {limit} rows"
        )));
    }
    Ok(())
}

/// Replaces all the cells, lengthening or truncating this one
/// column. Accepts a 1D numpy float64 or an iterable of float/int/str/None.
/// Numeric/datetime refuse texts and turn None into NaN; mixed
/// keeps None, empty text, numeric text, NaN and signed zero distinct.
/// Copies the inputs; the type and metadata stay unchanged. `True`
/// and `False` are refused (`InvalidValueError`, column unchanged). The write
/// and the truncation of the tail form a single atomic operation. The
/// column must be unchanged since the script's last read
/// (`column`/`values`), or since `expected_version` if given (optional):
/// otherwise `ConcurrentWriteError` at once, with no overwrite. A column never
/// read is not protected.
#[pyfunction]
#[pyo3(signature = (handle, values, *, expected_version=None))]
fn set_values(
    py: Python<'_>,
    handle: &Handle,
    values: &Bound<'_, PyAny>,
    expected_version: Option<u64>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let cells = copy_cells(values)?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::set_values(&context, id, epoch, cells, expected_version))
        .map_err(to_py_error)
}

/// The fields that `set_column_meta` accepts, in one place: the error
/// message for an unknown field and the surface test (`tests/frozen_api.rs`,
/// #331) start from this list.
pub(crate) const COLUMN_META_FIELDS: &[&str] = &[
    "role",
    "long_name",
    "unit",
    "comment",
    "display_format",
    "formula",
];

/// Changes only the named fields: role (x, y, x2, y2, x3, y3, x4, y4, z, x_error, y_error,
/// label, none), long_name, unit, comment, display_format, formula.
/// Values are str; formula=None removes the formula. The formula is
/// entered in letters: a letter that designates no column of the sheet,
/// the column itself or `#REF` raise `InvalidValueError`. Type, version
/// and provenance are preserved. A formula column refuses writes.
#[pyfunction]
#[pyo3(signature = (handle, **metadata))]
fn set_column_meta(
    py: Python<'_>,
    handle: &Handle,
    metadata: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let mut changes = Vec::new();
    if let Some(metadata) = metadata {
        for (key, value) in metadata.iter() {
            let key = key.extract::<String>()?;
            if !COLUMN_META_FIELDS.contains(&key.as_str()) {
                return Err(InvalidValueError::new_err(format!(
                    "unknown column metadata field: {key} (accepted: {})",
                    COLUMN_META_FIELDS.join(", ")
                )));
            }
            let value =
                if key == "formula" && value.is_none() {
                    None
                } else {
                    Some(value.extract::<String>().map_err(|_| {
                        InvalidValueError::new_err(format!("{key} must be a string"))
                    })?)
                };
            changes.push((key, value));
        }
    }
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::set_column_meta(&context, id, epoch, changes))
        .map_err(to_py_error)
}

/// Places a graphic object on a graph and returns its rank in the panel.
///
/// `kind`: `"rectangle"`, `"ellipse"`, `"line"` or `"arrow"` (head at
/// `(x2, y2)`). Points are in the panel's **data coordinates**
/// `panel = (row, column)` and of the `axis` ordinate (`"left"` or
/// `"right"`): the object follows the zoom. Colors `"#RRGGBB"` or `"#RRGGBBAA"`;
/// `line_color=""` = automatic, `fill_color=""` = no fill.
/// The object is placed above the others. A run's writes are
/// grouped: a `pyst.undo()` or Ctrl+Z undoes them together.
#[pyfunction]
#[pyo3(signature = (graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis="left", line_color="", line_width=1.5, fill_color=""))]
#[allow(clippy::too_many_arguments)]
fn add_shape(
    py: Python<'_>,
    graph: &Handle,
    kind: &str,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    panel: (usize, usize),
    axis: &str,
    line_color: &str,
    line_width: f64,
    fill_color: &str,
) -> PyResult<usize> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let args = bridge::ShapeArgs {
        kind: kind.to_owned(),
        points: (x1, y1, x2, y2),
        panel,
        axis: axis.to_owned(),
        line_color: line_color.to_owned(),
        line_width,
        fill_color: fill_color.to_owned(),
    };
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| bridge::add_shape(&context, id, epoch, &args))
        .map_err(to_py_error)
}

/// Removes the graphic object of rank `index` from panel `panel`. The objects
/// above it move down one rank. Undoable by `pyst.undo()`.
#[pyfunction]
#[pyo3(signature = (graph, index, *, panel=(0, 0)))]
fn remove_shape(
    py: Python<'_>,
    graph: &Handle,
    index: usize,
    panel: (usize, usize),
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| bridge::remove_shape(&context, id, epoch, panel, index))
        .map_err(to_py_error)
}

/// Module documentation as a script sees it (`help(pyst)`).
const MODULE_DOC: &str = "\
StokesForge script surface: navigation, reading, writing, undo, import,
saving and figure export.

    pyst.root()                     -> Handle (dossier racine)
    pyst.children(handle=None)      -> list[Handle] (ordered direct children)
    pyst.info(handle)               -> dict (name, kind, parent, rows, metadata)
    pyst.values(handle)             -> tuple (copy of the float/str/None cells)
    pyst.resolve(path)              -> Handle
    pyst.column(handle)             -> numpy.ndarray (read-only, no copy)
    pyst.set_column(handle, values) -> None (goes through the history)
    pyst.create_workbook(name=None, parent=None) -> Handle (empty workbook)
    pyst.create_worksheet(workbook, name=None) -> Handle
    pyst.create_column(worksheet, name=None, column_type=\"numeric\") -> Handle
    pyst.rename(handle, name)       -> None
    pyst.remove(handle)             -> None
    pyst.set_values(handle, values) -> None (replacement, length included)
    pyst.set_column_meta(handle, **metadata) -> None
    pyst.plot(y, x=None, *, graph=None, name=None, parent=None, **options) -> Handle
                  (y: a column, or a list of columns = a series group;
                   label=[...] one name per series, group_name='...')
    pyst.set_graph(graph, *, panel=(0, 0), axis='left', unit='cm', **options) -> None (titles,
                  scales, bounds, legend, plot areas, image size...; one history entry; keys in
                  help(pyst.set_graph))
    pyst.graph_size(graph, *, unit='cm') -> dict (width, height, plot_widths, plot_heights,
                  plot_left, plot_top, fixed_size, layer_placement, layers; lengths in `unit`)
    pyst.insert_rows(worksheet, at, count=1) -> None
    pyst.delete_rows(worksheet, at, count=1) -> None
    pyst.set_column_type(column, column_type) -> None ('numeric', 'datetime', 'mixed')
    pyst.move(handle, parent, index=None) -> None (index among the other children; default: last)
    pyst.duplicate(handle, name=None) -> Handle (the copy, in the same parent)
    pyst.set_note(note, text) -> None
    pyst.series(graph, *, panel=(0, 0)) -> list[dict] (index, name, group, color, visible, y)
    pyst.series_groups(graph, *, panel=(0, 0)) -> list[dict] (id, name, members)
    pyst.group_series(graph, indices, *, name=None, panel=(0, 0)) -> int (group number)
    pyst.set_series_group(graph, index, group, *, panel=(0, 0)) -> int (new rank;
                  group=None takes the series out of its group)
    pyst.rename_group(graph, group, name, *, panel=(0, 0)) -> None ('' = automatic name)
    pyst.ungroup(graph, group, *, panel=(0, 0)) -> None (the series stay)
    pyst.remove_group(graph, group, *, panel=(0, 0)) -> None (also removes the series)
    pyst.set_group_style(graph, group, *, panel=(0, 0), line_style=None, line_width=None,
                  symbol=None, symbol_size=None) -> None
    pyst.apply_gradient(graph, group, gradient, *, panel=(0, 0)) -> None (name of a built-in
                  or project-saved color gradient, or list of '#RRGGBB' stops)
    pyst.add_shape(graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis='left',
                  line_color='', line_width=1.5, fill_color='') -> int
    pyst.remove_shape(graph, index, *, panel=(0, 0)) -> None
    pyst.undo()                     -> bool (False if nothing to undo)
    pyst.redo()                     -> bool (False if nothing to redo)
    pyst.save()                     -> None (at the project's current path)
    pyst.save_as(path)              -> None (and path becomes the current path)
    pyst.export_graph(graph, path, *, png_scale=4.0) -> None (.svg, .pdf or .png)
    pyst.import_file(path, recipe=None) -> list[Handle] (sheets of a new workbook)
    pyst.analysis.baseline / correct / integrate / fit / peaks / reconvolve /
        deconvolve(y, x=None, ...) -> dict (see help(pyst.analysis))
    pyst.analysis.readiness(kind, **options) -> dict (what is missing before a calculation)

Errors: UnknownPathError, BrokenReferenceError, InvalidValueError,
          ConcurrentWriteError.

Known limits of this surface:
  - the output (print) is returned only at the end of the script, not as it is produced;
  - a single Ctrl+Z undoes the script's writes;
    an uncaught error or a requested stop undoes the current group;
  - undo() ends then undoes the current group;
  - set_column requires the exact length; set_values also replaces the length;
  - other clients cannot write during the script's group;
  - a Handle does not survive a project reload (BrokenReferenceError
    on next use);
  - atexit never runs (the interpreter is not finalized): anything that
    must be written to disk must be written explicitly;
  - an interruption does not cut a long native call (a numpy operation on
    ten million points will run to completion).
";

/// Version of the `pyst` API, `"MAJOR.MINOR"` (`pyst.__api_version__`, #356).
///
/// Alpha while it is `0.x`: any change of surface (addition or break) bumps the
/// minor. `scripts/versions-aligned.sh` checks it against the frozen snapshot
/// `tests/api-witnesses/v<version>.txt` (see docs/python-and-scripting.md).
pub const PYST_API_VERSION: &str = "0.4";

/// Builds the `pyst` module and registers it in `sys.modules`.
///
/// The module is **stateless**: the functions find the session through
/// the current thread's context (see [`crate::engine`]). Two script engines
/// living in the same process can therefore share the same module without getting
/// in each other's way, and the four exception types are process singletons:
/// a script catches the same `pyst.UnknownPathError` whatever the engine.
pub(crate) fn install(py: Python<'_>) -> PyResult<()> {
    let module = PyModule::new(py, "pyst")?;
    module.add("__doc__", MODULE_DOC)?;
    module.add("__api_version__", PYST_API_VERSION)?;
    module.add_class::<Handle>()?;
    module.add_function(wrap_pyfunction!(resolve, &module)?)?;
    module.add_function(wrap_pyfunction!(root, &module)?)?;
    module.add_function(wrap_pyfunction!(children, &module)?)?;
    module.add_function(wrap_pyfunction!(info, &module)?)?;
    module.add_function(wrap_pyfunction!(values, &module)?)?;
    module.add_function(wrap_pyfunction!(column, &module)?)?;
    module.add_function(wrap_pyfunction!(set_column, &module)?)?;
    module.add_function(wrap_pyfunction!(create_workbook, &module)?)?;
    module.add_function(wrap_pyfunction!(create_worksheet, &module)?)?;
    module.add_function(wrap_pyfunction!(create_column, &module)?)?;
    module.add_function(wrap_pyfunction!(rename, &module)?)?;
    module.add_function(wrap_pyfunction!(remove, &module)?)?;
    module.add_function(wrap_pyfunction!(set_values, &module)?)?;
    module.add_function(wrap_pyfunction!(set_column_meta, &module)?)?;
    module.add_function(wrap_pyfunction!(crate::graph::plot, &module)?)?;
    module.add_function(wrap_pyfunction!(add_shape, &module)?)?;
    module.add_function(wrap_pyfunction!(remove_shape, &module)?)?;
    for function in [
        wrap_pyfunction!(crate::groups::series, &module)?,
        wrap_pyfunction!(crate::groups::series_groups, &module)?,
        wrap_pyfunction!(crate::groups::group_series, &module)?,
        wrap_pyfunction!(crate::groups::set_series_group, &module)?,
        wrap_pyfunction!(crate::groups::rename_group, &module)?,
        wrap_pyfunction!(crate::groups::ungroup, &module)?,
        wrap_pyfunction!(crate::groups::remove_group, &module)?,
        wrap_pyfunction!(crate::groups::set_group_style, &module)?,
        wrap_pyfunction!(crate::groups::apply_gradient, &module)?,
    ] {
        module.add_function(function)?;
    }
    module.add_function(wrap_pyfunction!(undo, &module)?)?;
    module.add_function(wrap_pyfunction!(redo, &module)?)?;
    module.add_function(wrap_pyfunction!(save, &module)?)?;
    module.add_function(wrap_pyfunction!(save_as, &module)?)?;
    module.add_function(wrap_pyfunction!(export_graph, &module)?)?;
    module.add_function(wrap_pyfunction!(import_file, &module)?)?;
    module.add_function(wrap_pyfunction!(crate::set_graph::set_graph, &module)?)?;
    module.add_function(wrap_pyfunction!(crate::graph_size::graph_size, &module)?)?;
    for function in [
        wrap_pyfunction!(crate::sheet_edit::insert_rows, &module)?,
        wrap_pyfunction!(crate::sheet_edit::delete_rows, &module)?,
        wrap_pyfunction!(crate::sheet_edit::set_column_type, &module)?,
        wrap_pyfunction!(crate::sheet_edit::move_node, &module)?,
        wrap_pyfunction!(crate::sheet_edit::duplicate, &module)?,
        wrap_pyfunction!(crate::sheet_edit::set_note, &module)?,
    ] {
        module.add_function(function)?;
    }
    module.add("UnknownPathError", py.get_type::<UnknownPathError>())?;
    module.add(
        "BrokenReferenceError",
        py.get_type::<BrokenReferenceError>(),
    )?;
    module.add("InvalidValueError", py.get_type::<InvalidValueError>())?;
    module.add(
        "ConcurrentWriteError",
        py.get_type::<ConcurrentWriteError>(),
    )?;
    crate::analysis::install(py, &module)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyst", &module)?;
    Ok(())
}

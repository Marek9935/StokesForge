//! Script Settings from a script (#548, lot L4): `pyst.settings` reads the
//! TOML of a Script Settings object as a dict, `pyst.set_settings` writes
//! values into it.
//!
//! The model keeps the raw text; the parsing, the type rules and the editing
//! that keeps comments and order live in `stokesforge_core::script_settings`.
//! Here only the conversion between Python values and the neutral
//! [`SettingsValue`] tree happens, on the script thread (it needs the GIL,
//! invariant 9); the core work runs after `detach`. A write is one
//! `SetScriptSettingsText` in the script's single group. Hints (choices,
//! bounds) never refuse a script; a missing key is created and a wrong type
//! is refused, naming the key, with nothing written (decisions U2, U4, U5).

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use stokesforge_commands::Command;
use stokesforge_core::script_settings::{self, Scalar, SettingsValue};

use crate::bridge::{self, HostError};
use crate::engine;
use crate::graph::invalid;
use crate::surface::{to_py_error, Handle};

/// The stored text and name of a Script Settings object.
fn stored_text(
    snapshot: &stokesforge_session::Snapshot,
    id: stokesforge_core::NodeId,
    epoch: u64,
) -> Result<(String, String), HostError> {
    let node = bridge::checked_node(snapshot, id, epoch)?;
    match node.kind() {
        stokesforge_core::NodeKind::ScriptSettings { text, .. } => {
            Ok((node.name().to_owned(), text.clone()))
        }
        _ => Err(HostError::InvalidValue(format!(
            "{id} is not a Script Settings object"
        ))),
    }
}

fn date_from_text<'py>(py: Python<'py>, class: &str, text: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("datetime")?
        .getattr(class)?
        .call_method1("fromisoformat", (text,))
}

fn scalar_to_py<'py>(py: Python<'py>, scalar: &Scalar) -> PyResult<Bound<'py, PyAny>> {
    Ok(match scalar {
        Scalar::String(text) => PyString::new(py, text).into_any(),
        Scalar::Integer(number) => number.into_pyobject(py)?.into_any(),
        Scalar::Float(number) => PyFloat::new(py, *number).into_any(),
        Scalar::Bool(flag) => PyBool::new(py, *flag).to_owned().into_any(),
        Scalar::OffsetDateTime(text) | Scalar::LocalDateTime(text) => {
            date_from_text(py, "datetime", text)?
        }
        Scalar::LocalDate(text) => date_from_text(py, "date", text)?,
        Scalar::LocalTime(text) => date_from_text(py, "time", text)?,
    })
}

fn value_to_py<'py>(py: Python<'py>, value: &SettingsValue) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        SettingsValue::Scalar(scalar) => scalar_to_py(py, scalar)?,
        SettingsValue::List(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(value_to_py(py, item)?)?;
            }
            list.into_any()
        }
        SettingsValue::Map(entries) => {
            let dict = PyDict::new(py);
            for (key, item) in entries {
                dict.set_item(key, value_to_py(py, item)?)?;
            }
            dict.into_any()
        }
    })
}

fn py_to_value(value: &Bound<'_, PyAny>, path: &str) -> PyResult<SettingsValue> {
    let py = value.py();
    let datetime = py.import("datetime")?;
    let is = |class: &str| -> PyResult<bool> { value.is_instance(&datetime.getattr(class)?) };
    // `bool` is an `int` and `datetime` is a `date`: the narrower first.
    if value.is_instance_of::<PyBool>() {
        return Ok(SettingsValue::Scalar(Scalar::Bool(value.extract()?)));
    }
    if is("datetime")? {
        let text: String = value.call_method0("isoformat")?.extract()?;
        let aware = !value.getattr("tzinfo")?.is_none();
        return Ok(SettingsValue::Scalar(if aware {
            Scalar::OffsetDateTime(text)
        } else {
            Scalar::LocalDateTime(text)
        }));
    }
    if is("date")? {
        return Ok(SettingsValue::Scalar(Scalar::LocalDate(
            value.call_method0("isoformat")?.extract()?,
        )));
    }
    if is("time")? {
        if !value.getattr("tzinfo")?.is_none() {
            return Err(invalid(format!(
                "{path}: a time with a time zone cannot be stored"
            )));
        }
        return Ok(SettingsValue::Scalar(Scalar::LocalTime(
            value.call_method0("isoformat")?.extract()?,
        )));
    }
    if value.is_instance_of::<PyInt>() {
        let number: i64 = value
            .extract()
            .map_err(|_| invalid(format!("{path}: the integer does not fit in 64 bits")))?;
        return Ok(SettingsValue::Scalar(Scalar::Integer(number)));
    }
    if value.is_instance_of::<PyFloat>() {
        return Ok(SettingsValue::Scalar(Scalar::Float(value.extract()?)));
    }
    if value.is_instance_of::<PyString>() {
        return Ok(SettingsValue::Scalar(Scalar::String(value.extract()?)));
    }
    if let Ok(dict) = value.cast::<PyDict>() {
        let mut entries = Vec::new();
        for (key, item) in dict.iter() {
            let key: String = key
                .extract()
                .map_err(|_| invalid(format!("{path}: keys must be text")))?;
            let child = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            entries.push((key, py_to_value(&item, &child)?));
        }
        return Ok(SettingsValue::Map(entries));
    }
    if value.is_instance_of::<PyList>() || value.is_instance_of::<PyTuple>() {
        let mut items = Vec::new();
        for item in value.try_iter()? {
            items.push(py_to_value(&item?, path)?);
        }
        return Ok(SettingsValue::List(items));
    }
    Err(invalid(format!(
        "{}: a value of type {} cannot be stored in settings",
        if path.is_empty() { "(root)" } else { path },
        value.get_type().name()?
    )))
}

/// Reads a Script Settings object as a dict, in file order: text, `int`
/// (exact), `float`, `bool`, `datetime.date` / `datetime.datetime` /
/// `datetime.time` by the kind written in the file, lists, nested dicts for
/// tables, and a list of dicts for `[[x]]`.
///
/// Raises `InvalidValueError` for a handle that is not a Script Settings
/// object, or whose stored text is not exact TOML (the message names the
/// object, the line and the parser's reason); `BrokenReferenceError` for a
/// stale handle.
#[pyfunction]
#[pyo3(signature = (target))]
pub(crate) fn settings<'py>(py: Python<'py>, target: &Handle) -> PyResult<Bound<'py, PyAny>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (target.id, target.epoch);
    let tree = py
        .detach(|| -> Result<SettingsValue, HostError> {
            let snapshot = context
                .session()
                .snapshot()
                .map_err(HostError::from_session)?;
            let (name, text) = stored_text(&snapshot, id, epoch)?;
            let doc = script_settings::parse(&text)
                .map_err(|error| HostError::InvalidValue(format!("{name}: {error}")))?;
            Ok(doc.to_values())
        })
        .map_err(to_py_error)?;
    value_to_py(py, &tree)
}

/// Writes values into a Script Settings object: `values` is a dict (a nested
/// dict for a table). A key that exists takes the new value, which must have
/// the type of the stored one (an `int` is accepted for a float key); a
/// missing key is created; the keys you do not name stay. Comments, order and
/// spacing stay. Hints in comments (choices, bounds) do not refuse a script.
/// One undo entry, inside the script's group.
///
/// Raises `InvalidValueError` for a handle that is not a Script Settings
/// object, a value that cannot be stored, a wrong type (the message names the
/// key), or a stored text that is not exact TOML; nothing is written then.
/// `BrokenReferenceError` for a stale handle.
#[pyfunction]
#[pyo3(signature = (target, values))]
pub(crate) fn set_settings(
    py: Python<'_>,
    target: &Handle,
    values: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (target.id, target.epoch);
    // Python to the neutral tree here, with the GIL (invariant 9).
    let tree = py_to_value(values.as_any(), "")?;
    py.detach(|| -> Result<(), HostError> {
        let snapshot = bridge::write_snapshot(&context, id, epoch)?;
        let (name, text) = stored_text(&snapshot, id, epoch)?;
        let updated = script_settings::apply_values(&text, &tree)
            .map_err(|error| HostError::InvalidValue(format!("{name}: {error}")))?;
        if !script_settings::same_text(&updated, &text) {
            context
                .session()
                .execute(Command::SetScriptSettingsText { id, text: updated })
                .map_err(HostError::from_session)?;
        }
        Ok(())
    })
    .map_err(to_py_error)
}

//! Round trip of arrays between Rust and numpy: **zero-copy reading**,
//! **writing by copy**, and the safety contract that makes the pair workable.
//!
//! # The asymmetry is a decision
//!
//! | Direction | Mechanism | Copy? |
//! |---|---|---|
//! | Rust → Python (read) | numpy array **borrowing** the `Arc<Vec<f64>>`, `writeable = False` | **no** |
//! | Python → Rust (write) | copy of the numpy array into a fresh `Vec<f64>` | **yes** |
//!
//! Writing copies **by choice**: the model must own its buffers.
//! Adopting memory allocated by the Python allocator would make the
//! lifetime of a column depend on that of a Python object. A second,
//! independent reason: a copied `Vec<f64>` is pure Rust, it is destroyed on
//! any thread — whereas destroying a Python object requires the GIL, which
//! would forbid passing the value to the thread that owns the model
//! (task 5.4).
//!
//! # This module does not know the model
//!
//! It handles `Arc<Vec<f64>>` — a standard library type — and
//! not columns. `stokesforge-python` does not depend on `stokesforge-core`: this is a
//! compiler-checked boundary, and it is what will let a Matplotlib
//! rendering engine sit here without going through the script
//! layer.
//!
//! # Precondition
//!
//! Like the rest of the crate, these functions assume the interpreter is started
//! (see [`crate::interpreter::EmbeddedPython::initialize`]).

use std::sync::Arc;
use std::time::Instant;

use numpy::ndarray::aview1;
use numpy::{
    dtype, PyArray1, PyArrayDescrMethods, PyArrayMethods, PyUntypedArray, PyUntypedArrayMethods,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;

use crate::interpreter::{attached, PythonError};

create_exception!(
    stokesforge_python,
    MaskedArrayError,
    PyException,
    "A `numpy.ma.masked_array` carries at least one masked value: copying it \
     would write this hidden value as real data. Make the mask \
     explicit before the call — `.filled(float('nan'))` to translate it into \
     the model's missing value, `.compressed()` to drop it — rather \
     than letting `copy_array_to_vec` guess."
);

/// Python container that keeps the Rust buffer alive under a numpy view.
///
/// It is the `base` attribute of the array returned by [`borrowed_readonly_array`]:
/// as long as it lives, the `Arc` reference count stays **≥ 2**, so
/// any write on the model side goes through `Arc::make_mut` and **allocates a fresh
/// buffer**. The buffer under the view is then neither moved, nor reallocated, nor
/// freed: the safety clause of `borrow_from_array` is honored **by
/// construction, not by vigilance**.
///
/// The `pyclass` is `frozen`: nothing, from Python, can replace or mutate
/// the `Arc` it holds.
#[pyclass(frozen, module = "stokesforge")]
pub struct FloatBufferOwner {
    /// Clone of the borrowed `Arc`. It is **not** a pointer nor a borrowed
    /// reference: it is this clone, and only this one, that keeps the count above
    /// 1.
    data: Arc<Vec<f64>>,
}

impl FloatBufferOwner {
    /// Length of the buffer kept alive — useful for tests and diagnostics.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True if the buffer kept alive is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Returns a **zero-copy** numpy view of `data`, read-only.
///
/// The returned array points **exactly** to `data.as_ptr()`: not a single byte
/// is duplicated, whatever the size of the column. Its `base` attribute
/// is a [`FloatBufferOwner`] that holds a clone of the `Arc`.
///
/// # `writeable = False` is a guard, not a boundary
///
/// It catches the slip of attention (`col[0] = 3` in a script) and documents
/// the intent. It does not resist a determined script, and does not have to:
/// the trust model already gives the interpreter full powers.
///
/// # A view is a snapshot, not a subscription
///
/// A view taken **before** a write stays on the **old** buffer. The script
/// keeps reading stale values from it, silently:
///
/// ```python
/// v = pyst.column(col)       # v looks straight at the model's memory
/// print(v[0])               # 1.0
/// pyst.set_column(col, other_values)
/// print(v[0])               # 1.0  ← still the old value
/// print(pyst.column(col)[0]) # 2.0  ← the new one
/// ```
///
/// **This is the chosen behavior**, and it is the only safe choice: mutating the
/// buffer under numpy's feet would be a crash, not a
/// surprising value. The two rejected options, for the record: *invalidating the view*
/// (numpy cannot cleanly invalidate an array; we would have to return
/// a wrapper object, thus losing the fact that it is a real `ndarray`, which
/// the whole scientific library expects); *live view that updates itself*
/// (impossible without a copy or a lock — precisely what zero-copy
/// forbids).
///
/// **Simple rule that avoids the whole problem: do not keep a view longer
/// than a computation; re-read the column after any write.**
///
/// # Errors
///
/// [`Err`] if allocating the Python container fails, or if the write
/// flag could not be removed.
pub fn borrowed_readonly_array<'py>(
    py: Python<'py>,
    data: &Arc<Vec<f64>>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    // The container is created BEFORE the borrow: if it cannot be allocated, we
    // leave without having built an array that would point to nothing.
    let owner = Bound::new(
        py,
        FloatBufferOwner {
            data: Arc::clone(data),
        },
    )?;

    // `ndarray` view borrowed over the slice — no byte copied: `aview1` only
    // retains pointer, length and stride.
    let view = aview1(&data[..]);

    // SAFETY: the clause of `PyArray::borrow_from_array` demands two things.
    //
    // (1) "`container` is set as the base object of the returned array, which
    //     must not be destroyed before `container`". The opposite is what is
    //     built here: `owner` is *consumed* by the call, its reference is
    //     transferred to the numpy array, which releases it at its own destruction.
    //     The container therefore mechanically outlives the array.
    //
    // (2) "`array` must not be reallocated while `container` lives".
    //     `owner` holds a CLONE of the `Arc`, so the buffer's reference count
    //     is ≥ 2 as long as the view exists. On the model side, any write
    //     goes through `Arc::make_mut`, which, with a count ≥ 2, **allocates a
    //     fresh buffer** and leaves the old one intact: neither in-place mutation,
    //     nor reallocation (growth of the `Vec`), nor freeing can reach
    //     the buffer under the view. The `Vec` itself cannot be freed: the
    //     `Arc` clone keeps it alive even if the caller releases its own.
    //
    // Accepted consequence, documented above: the view becomes a
    // snapshot of the old buffer. It is a stale value, never an invalid
    // access.
    let array = unsafe { PyArray1::borrow_from_array(&view, owner.into_any()) };

    // `writeable = False`. The path goes through rust-numpy's exclusive borrow,
    // the only *safe* API that removes the flag; the borrow guard
    // is released at once, it does not outlive this function. The array cannot
    // already be borrowed: it was just created and nobody else
    // has a reference to it.
    let readonly = array
        .try_readwrite()
        .map_err(|err| PyRuntimeError::new_err(format!("exclusive borrow refused: {err}")))?
        .make_nonwriteable();
    drop(readonly);

    Ok(array)
}

/// Copies a numpy `float64` array into a fresh `Vec<f64>`.
///
/// Accepts **non-contiguous** arrays — a slice `v[::2]` is one: the
/// compaction path is explicit, it never assumes contiguity.
///
/// The copy is the *write* direction of the asymmetry described at the top of the module:
/// the returned `Vec` belongs to Rust, it no longer depends on any Python object and
/// is destroyed on any thread, GIL or not.
///
/// # A `numpy.ma.masked_array` is refused, never guessed
///
/// `numpy.ma.MaskedArray` is a subclass of `ndarray`: without a dedicated
/// check, it would pass the three checks below (it is an
/// array, `float64`, one dimension) and its masked cells would be copied
/// **as if they were real data** — the mask, which says
/// "value absent", would simply be ignored.
///
/// The model has exactly one missing value (`NaN`, documented at the top of
/// `stokesforge-core::column`), but choosing **here** to translate a mask into `NaN`
/// would take the decision away from the script: `.filled()`, `.compressed()`, or another
/// fallback value belong to it. The repository already ruled on this kind
/// of case for `int64`, refused rather than silently converted: "an
/// implicit conversion would hide a loss of precision beyond 2⁵³" —
/// the same reasoning applies word for word to the mask: an implicit
/// conversion would hide the loss of the mask itself. Hence [`MaskedArrayError`]
/// as soon as at least one cell is masked; a `MaskedArray` with no masked cell
/// (`nomask` mask, or entirely `False`) hides nothing and passes like
/// an ordinary `ndarray`.
///
/// # Errors
///
/// [`Err`] with an explicit message if the object is not a numpy array, or
/// not of `dtype` `float64`, or not one-dimensional, or [`MaskedArrayError`]
/// if the object is a `numpy.ma.masked_array` carrying at least one masked
/// value.
pub fn copy_array_to_vec(array: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let py = array.py();

    // 0. numpy mask, before everything else: a `MaskedArray` is a valid
    //    `ndarray` in the eyes of the next three checks, so this is
    //    the only place to tell it apart.
    if has_hidden_masked_values(array)? {
        return Err(MaskedArrayError::new_err(format!(
            "masked numpy array (`{}`) refused: at least one value is \
             masked and would be copied as real data if accepted \
             — call `.filled(float('nan'))` (masked cell -> missing \
             value of the model) or `.compressed()` (masked cell -> dropped) \
             before `pyst.set_column`",
            type_name(array)
        )));
    }

    // 1. Is it even a numpy array? A Python list fails here.
    let untyped = array.cast::<PyUntypedArray>().map_err(|_| {
        PyTypeError::new_err(format!(
            "expected a one-dimensional numpy float64 array, got an object of type `{}`",
            type_name(array)
        ))
    })?;

    // 2. Dimension. A 2-D array fails here, with its shape in the message.
    if untyped.ndim() != 1 {
        return Err(PyTypeError::new_err(format!(
            "expected a one-dimensional numpy array, got {} dimensions (shape {:?})",
            untyped.ndim(),
            untyped.shape()
        )));
    }

    // 3. `dtype`. An `int64` array fails here: we refuse explicitly
    //    rather than silently converting — an implicit conversion would hide
    //    a loss of precision beyond 2^53.
    let expected = dtype::<f64>(py);
    let actual = untyped.dtype();
    if !actual.is_equiv_to(&expected) {
        return Err(PyTypeError::new_err(format!(
            "expected a numpy float64 array, got an array of `{actual}`"
        )));
    }

    let typed = untyped.cast::<PyArray1<f64>>().map_err(|_| {
        PyTypeError::new_err("one-dimensional numpy float64 array expected".to_owned())
    })?;
    let readonly = typed
        .try_readonly()
        .map_err(|err| PyRuntimeError::new_err(format!("shared borrow refused: {err}")))?;
    let view = readonly.as_array();

    // Explicit compaction: `as_slice` returns `Some` only for an array
    // contiguous in memory. Otherwise we iterate following the strides, which covers
    // `v[::2]`, `v[::-1]` and any exotic view.
    match view.as_slice() {
        Some(contiguous) => Ok(contiguous.to_vec()),
        None => Ok(view.iter().copied().collect()),
    }
}

/// True if `object` is a `numpy.ma.masked_array` with **at least one**
/// masked value.
///
/// Delegates to `numpy.ma.is_masked`, rather than re-reading the `mask` attribute and
/// deciding here: numpy knows the `nomask` case (the sentinel
/// carried by a `MaskedArray` with no masked cell, for which a manual check
/// would also have to cover an explicit but entirely `False` mask).
/// `is_masked` returns `False` for any object that is not a `MaskedArray` —
/// a list, an integer, an ordinary `ndarray` — so this call is safe even
/// before knowing whether `object` is a numpy array.
///
/// `numpy.ma` is already loaded as soon as `numpy` is (its `__init__.py`
/// imports its `ma` submodule): `py.import` therefore finds a module already in
/// memory, never a disk access.
fn has_hidden_masked_values(object: &Bound<'_, PyAny>) -> PyResult<bool> {
    let py = object.py();
    let ma = py.import("numpy.ma")?;
    ma.call_method1("is_masked", (object,))?.extract()
}

/// Name of the Python type of an object, for error messages.
fn type_name(object: &Bound<'_, PyAny>) -> String {
    object
        .get_type()
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "?".to_owned())
}

/// Lock measurements: `(duration of a borrow, duration of a copy)` for `len`
/// values, in **nanoseconds**.
///
/// Both gestures are measured within a single GIL acquisition, after a
/// warm-up **at the same size**: it absorbs the lazy initialization
/// of the numpy C API and of the [`FloatBufferOwner`] type, but above all it makes
/// the page faults of the first 8 MB array be paid in advance. Without it, the
/// measured copy is that of a *cold* allocation (≈ 2.7 ms for 1 M `f64`)
/// instead of the steady state (≈ 0.8 ms) — the gap comes from the system, not from numpy,
/// and it would skew the comparison with task 5.1a.
///
/// The borrow takes **constant** time (it allocates only two Python objects) whereas
/// the copy is **linear**: the gap grows with the size of the column.
///
/// # Errors
///
/// [`Err`] if a Python call fails.
pub fn measure_borrow_vs_copy(len: usize) -> Result<(u128, u128), PythonError> {
    let data: Arc<Vec<f64>> = Arc::new((0..len).map(|index| index as f64).collect());

    attached(|py| {
        // Warm-up at the real size: `FloatBufferOwner` type,
        // `PY_ARRAY_API`, borrow flags, and page faults of the 8 MB
        // allocation. The warm-up array is destroyed before the measurement.
        std::hint::black_box(borrowed_readonly_array(py, &data)?);
        std::hint::black_box(PyArray1::from_slice(py, &data));

        let before_borrow = Instant::now();
        let borrowed = borrowed_readonly_array(py, &data)?;
        let borrow_ns = before_borrow.elapsed().as_nanos();
        std::hint::black_box(&borrowed);

        let before_copy = Instant::now();
        let copied = PyArray1::from_slice(py, &data);
        let copy_ns = before_copy.elapsed().as_nanos();
        std::hint::black_box(&copied);

        Ok((borrow_ns, copy_ns))
    })
}

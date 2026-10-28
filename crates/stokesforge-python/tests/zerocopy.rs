//! Integration tests of the Rust ↔ numpy array round trip (task 5.2).
//!
//! They run only with the `python` feature and require the vendored
//! prefix: run `scripts/python-vendor.sh` then `scripts/python-test.sh`.
//!
//! Each test matches an acceptance criterion of the lock. Two of
//! them deserve to be read before the others:
//!
//! * [`real_borrow_not_copy`] is the decisive test: it compares the address of the
//!   buffer seen by numpy to `Vec::as_ptr` — on the Python **and** the Rust side;
//! * [`stale_view_stays_on_the_old_buffer`] engraves the chosen policy:
//!   a view is a **snapshot**, not a subscription.
//!
//! As in `tests/embedded.rs`, CPython starts only once per
//! process: [`prepare`] is the single entry point, guarded by a `Once`.
//! Since the two test binaries are separate processes, this bootstrap is
//! deliberately duplicated rather than shared through a `common` module.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::{Arc, Once};

use numpy::{PyArray1, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use stokesforge_python::array::{
    borrowed_readonly_array, copy_array_to_vec, measure_borrow_vs_copy, FloatBufferOwner,
};
use stokesforge_python::interpreter::{self, EmbeddedPython};

static PREPARE: Once = Once::new();

/// Resolves the embedded prefix: `STOKESFORGE_PYTHON_HOME` if set by the test
/// script, otherwise `vendor/python/extracted/python` relative to the manifest.
fn home() -> PathBuf {
    match std::env::var_os(interpreter::HOME_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        }
    }
}

/// Starts the embedded interpreter, only once for the whole binary.
fn prepare() {
    PREPARE.call_once(|| {
        EmbeddedPython::with_home(home())
            .initialize()
            .expect("the embedded interpreter must start");
    });
}

/// Reads the content of a numpy `f64` array through rust-numpy's shared
/// borrow (never through `unsafe`).
fn values(array: &Bound<'_, PyArray1<f64>>) -> Vec<f64> {
    let readonly = array.try_readonly().expect("shared borrow");
    readonly.as_array().iter().copied().collect()
}

/// Buffer address as **numpy** publishes it, read from Python.
fn python_data_pointer(array: &Bound<'_, PyArray1<f64>>) -> usize {
    array
        .getattr("__array_interface__")
        .expect("__array_interface__")
        .get_item("data")
        .expect("data key")
        .get_item(0)
        .expect("address")
        .extract::<usize>()
        .expect("integer address")
}

/// A dictionary of globals containing `numpy`, for the tests' `eval`s.
fn numpy_globals<'py>(py: Python<'py>) -> Bound<'py, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("numpy", py.import("numpy").expect("import numpy"))
        .expect("set numpy");
    globals
}

// --- 1. Correct reading ----------------------------------------------------

#[test]
fn view_returns_the_values_and_the_length() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");
        assert_eq!(view.len(), 3);
        assert_eq!(view.ndim(), 1);
        assert_eq!(values(&view), vec![1.0, 2.0, 3.0]);
    });
}

#[test]
fn view_over_an_empty_buffer() {
    prepare();
    let data: Arc<Vec<f64>> = Arc::new(Vec::new());
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("empty borrow");
        assert_eq!(view.len(), 0);
        assert!(values(&view).is_empty());
    });
}

// --- 2. Real borrow, not a copy (decisive test) ------------------------------

#[test]
fn real_borrow_not_copy() {
    prepare();
    let data = Arc::new((0..1024).map(|index| index as f64).collect::<Vec<f64>>());
    let rust_pointer = data.as_ptr() as usize;

    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");

        // Rust side: rust-numpy exposes the array's data pointer.
        assert_eq!(
            view.data() as usize,
            rust_pointer,
            "the numpy array must point EXACTLY to the Vec buffer"
        );
        // Python side: the same address, published by numpy itself.
        assert_eq!(
            python_data_pointer(&view),
            rust_pointer,
            "__array_interface__ must publish the address of the Rust buffer"
        );
    });
}

// --- 3. `base` set ---------------------------------------------------------

#[test]
fn base_is_a_float_buffer_owner() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");
        let base = view.getattr("base").expect("base attribute");
        assert!(!base.is_none(), "base must not be None");

        let owner = base
            .cast::<FloatBufferOwner>()
            .expect("base must be a FloatBufferOwner");
        assert_eq!(owner.get().len(), 3);
        assert!(!owner.get().is_empty());
    });
}

// --- 4. Read-only -------------------------------------------------------

#[test]
fn view_is_read_only() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");

        let writeable = view
            .getattr("flags")
            .expect("flags")
            .getattr("writeable")
            .expect("writeable")
            .extract::<bool>()
            .expect("bool");
        assert!(!writeable, "arr.flags.writeable must be false");

        // A write from Python raises an exception…
        let globals = PyDict::new(py);
        globals.set_item("arr", &view).expect("set arr");
        let error = py.run(c"arr[0] = 9.0", Some(&globals), None);
        assert!(error.is_err(), "arr[0] = 9.0 must raise an exception");

        // …and the Rust buffer is unchanged.
        assert_eq!(data[0], 1.0);
        assert_eq!(values(&view), vec![1.0, 2.0, 3.0]);
    });
}

#[test]
fn the_writeable_flag_cannot_be_set_back() {
    prepare();
    // `borrow_from_array` leaves OWNDATA at zero: numpy then refuses to make
    // the array writeable again. This is NOT what guarantees safety (the
    // Arc clone does), but it is one more guard, and checking it
    // documents what a script can or cannot do.
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");
        let globals = PyDict::new(py);
        globals.set_item("arr", &view).expect("set arr");
        let reset = py.run(c"arr.flags.writeable = True", Some(&globals), None);
        assert!(
            reset.is_err(),
            "a script must not be able to make the view writeable"
        );
    });
}

// --- 5. Reference count ---------------------------------------------

#[test]
fn reference_count_around_the_view() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    assert_eq!(Arc::strong_count(&data), 1, "before the borrow");

    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");
        assert!(
            Arc::strong_count(&data) >= 2,
            "during the view, the count must be at least 2 (saw {})",
            Arc::strong_count(&data)
        );

        drop(view);
        // CPython counts references immediately: destroying the
        // array releases `base`, hence the `FloatBufferOwner`, hence the Arc
        // clone. `gc.collect()` is there only for possible cycles; if it
        // became necessary, it would be the sign of a cycle to document.
        py.import("gc")
            .expect("import gc")
            .call_method0("collect")
            .expect("gc.collect");

        assert_eq!(
            Arc::strong_count(&data),
            1,
            "after the view is destroyed, the count must fall back to 1"
        );
    });
}

#[test]
fn two_views_coexist_on_the_same_buffer() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let first = borrowed_readonly_array(py, &data).expect("first view");
        let second = borrowed_readonly_array(py, &data).expect("second view");

        // Same buffer, two distinct containers, so the count is 3.
        assert_eq!(first.data() as usize, second.data() as usize);
        assert_eq!(Arc::strong_count(&data), 3);
        assert_eq!(values(&first), values(&second));

        drop(first);
        assert_eq!(Arc::strong_count(&data), 2);
        drop(second);
        assert_eq!(Arc::strong_count(&data), 1);
    });
}

// --- 6. Stale view: the chosen policy ---------------------------------

#[test]
fn stale_view_stays_on_the_old_buffer() {
    prepare();
    let mut data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let old_view = borrowed_readonly_array(py, &data).expect("borrow");
        let old_pointer = old_view.data() as usize;

        // Write on the "model" side. The count is ≥ 2: `make_mut` allocates a
        // fresh buffer and leaves the old one intact.
        Arc::make_mut(&mut data)[0] = 42.0;

        // The view re-reads the OLD value, without crashing. This is the chosen
        // behavior: a view is a snapshot, not a subscription.
        assert_eq!(values(&old_view), vec![1.0, 2.0, 3.0]);

        let new_view = borrowed_readonly_array(py, &data).expect("new borrow");
        assert_eq!(values(&new_view), vec![42.0, 2.0, 3.0]);

        assert_ne!(
            old_pointer,
            new_view.data() as usize,
            "make_mut with a count ≥ 2 must have allocated a fresh buffer"
        );
    });
}

// --- 7. Growth under the view ---------------------------------------------

#[test]
fn growth_under_the_view() {
    prepare();
    let mut data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");
        let old_pointer = view.data() as usize;

        // Adding rows: without the Arc clone, the Vec would grow in place
        // and the reallocation would leave the view on freed memory.
        Arc::make_mut(&mut data).extend([4.0, 5.0, 6.0, 7.0, 8.0]);

        assert_eq!(view.len(), 3, "the view keeps its original length");
        assert_eq!(values(&view), vec![1.0, 2.0, 3.0]);
        assert_eq!(data.len(), 8);
        assert_ne!(old_pointer, data.as_ptr() as usize);
    });
}

// --- 8. Survival after release on the Rust side ------------------------------------

#[test]
fn the_view_survives_the_rust_side_drop() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");
        let pointer = view.data() as usize;

        // The container now holds the last reference: this is the
        // direct proof that `base` does its job.
        drop(data);

        assert_eq!(values(&view), vec![1.0, 2.0, 3.0]);
        assert_eq!(view.data() as usize, pointer, "the buffer did not move");
    });
}

// --- 9. Return: contiguous, non-contiguous, and the refusals -------------------------

#[test]
fn return_non_contiguous() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let slice = py
            .eval(
                c"numpy.arange(10, dtype=numpy.float64)[::2]",
                Some(&globals),
                None,
            )
            .expect("eval");
        assert_eq!(
            copy_array_to_vec(&slice).expect("non-contiguous copy"),
            vec![0.0, 2.0, 4.0, 6.0, 8.0]
        );
    });
}

#[test]
fn return_contiguous() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let array = py
            .eval(
                c"numpy.arange(5, dtype=numpy.float64)",
                Some(&globals),
                None,
            )
            .expect("eval");
        assert_eq!(
            copy_array_to_vec(&array).expect("contiguous copy"),
            vec![0.0, 1.0, 2.0, 3.0, 4.0]
        );
    });
}

#[test]
fn return_reversed() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let array = py
            .eval(
                c"numpy.arange(4, dtype=numpy.float64)[::-1]",
                Some(&globals),
                None,
            )
            .expect("eval");
        assert_eq!(
            copy_array_to_vec(&array).expect("negative-stride copy"),
            vec![3.0, 2.0, 1.0, 0.0]
        );
    });
}

#[test]
fn return_refuses_an_int64() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let array = py
            .eval(c"numpy.arange(4, dtype=numpy.int64)", Some(&globals), None)
            .expect("eval");
        let message = copy_array_to_vec(&array)
            .expect_err("an int64 must be refused")
            .to_string();
        assert!(
            message.contains("float64") && message.contains("int64"),
            "unclear message: {message}"
        );
    });
}

#[test]
fn return_refuses_two_dimensions() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let array = py
            .eval(
                c"numpy.zeros((2, 3), dtype=numpy.float64)",
                Some(&globals),
                None,
            )
            .expect("eval");
        let message = copy_array_to_vec(&array)
            .expect_err("a 2-D array must be refused")
            .to_string();
        assert!(
            message.contains("2 dimensions"),
            "unclear message: {message}"
        );
    });
}

#[test]
fn return_refuses_a_python_list() {
    prepare();
    Python::attach(|py| {
        let list = py.eval(c"[1.0, 2.0, 3.0]", None, None).expect("eval");
        let message = copy_array_to_vec(&list)
            .expect_err("a list must be refused")
            .to_string();
        assert!(
            message.contains("list") && message.contains("numpy"),
            "unclear message: {message}"
        );
    });
}

// --- 10. Full round trip, bit for bit ------------------------------------

#[test]
fn bit_for_bit_round_trip() {
    prepare();
    let mut values_list: Vec<f64> = (0..1000).map(|index| index as f64 * 0.5).collect();
    values_list[7] = f64::NAN;
    values_list[9] = -0.0;
    values_list[11] = f64::INFINITY;
    values_list[13] = f64::NEG_INFINITY;
    values_list[17] = f64::MIN_POSITIVE / 3.0; // subnormal
    let data = Arc::new(values_list);

    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("borrow");
        let globals = numpy_globals(py);
        globals.set_item("view", &view).expect("set view");
        let copy = py
            .eval(c"numpy.copy(view)", Some(&globals), None)
            .expect("numpy.copy");

        let returned = copy_array_to_vec(&copy).expect("return");
        assert_eq!(returned.len(), data.len());
        // Bit-for-bit comparison: `NaN != NaN` in IEEE, and `-0.0 == 0.0`.
        for (index, (before, after)) in data.iter().zip(returned.iter()).enumerate() {
            assert_eq!(
                before.to_bits(),
                after.to_bits(),
                "value {index} altered by the round trip"
            );
        }
    });
}

// --- 11. Measurement -------------------------------------------------------------

#[test]
fn borrow_at_least_twenty_times_faster_than_copy() {
    prepare();
    const LEN: usize = 1_000_000;
    let (borrow_ns, copy_ns) = measure_borrow_vs_copy(LEN).expect("measurement");

    println!("borrow of {LEN} f64: {borrow_ns} ns");
    println!("copy   of {LEN} f64: {copy_ns} ns");
    println!("ratio: ×{:.1}", copy_ns as f64 / borrow_ns.max(1) as f64);

    assert!(borrow_ns > 0, "zero borrow measurement: clock too coarse");

    // `scripts/python-memcheck.sh` sets STOKESFORGE_UNDER_VALGRIND=1 before
    // running this binary under valgrind. Under instrumentation, everything is
    // slowed down, but NOT uniformly: the borrow is dominated by a few
    // calls (allocation of the FloatBufferOwner, construction of the ndarray), which
    // valgrind penalizes proportionally much more than a linear run;
    // the copy is dominated by a memory transfer, which valgrind
    // slows down by a much more modest factor. The RATIO measured under
    // instrumentation therefore means nothing and would make this test fail on a
    // false negative (observed: ×9 under valgrind versus ×240 to ×370 in
    // ordinary execution). We then just print the two measurements,
    // without asserting the ratio. Outside instrumentation, the assertion stays
    // active: it is what protects zero-copy against a silent
    // regression into a copy.
    if std::env::var_os("STOKESFORGE_UNDER_VALGRIND").is_some() {
        println!(
            "STOKESFORGE_UNDER_VALGRIND set: ratio not asserted (valgrind slows down \
             the borrow and the copy non-uniformly, the ratio measured here \
             is meaningless)"
        );
        return;
    }

    assert!(
        copy_ns >= borrow_ns * 20,
        "the borrow must be at least 20× faster: borrow {borrow_ns} ns, copy {copy_ns} ns"
    );
}

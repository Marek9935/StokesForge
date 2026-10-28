//! Battery 7: the Rust <-> Python boundary: type conversions, error
//! propagation, and what crosses silently.
//!
//! Requires the `python` feature and the vendored interpreter (see
//! `scripts/python-test.sh` for the three variables to set).
#![cfg(feature = "python")]

use std::sync::OnceLock;
use stokesforge_commands::Command;
use stokesforge_core::{Column, ColumnType, ColumnValues, NodeId, Project, ValueBlock};
use stokesforge_python::interpreter::EmbeddedPython;
use stokesforge_script::ScriptEngine;
use stokesforge_session::{Session, SessionHandle};

/// A single engine for the whole file: CPython starts only once per process,
/// and a script's namespace persists from one run to the next (documented).
struct Bench {
    _session: Session,
    handle: SessionHandle,
    engine: ScriptEngine,
    columns: Vec<NodeId>,
    #[allow(dead_code)]
    sheet: NodeId,
}

static BENCH: OnceLock<std::sync::Mutex<Bench>> = OnceLock::new();

fn bench() -> &'static std::sync::Mutex<Bench> {
    BENCH.get_or_init(|| {
        let mut p = Project::new("PY").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let mut columns = Vec::new();
        for label in ["A", "B"] {
            columns.push(
                p.add_column(sheet, None, Some(label), Column::from_f64(&[1.0, 2.0, 3.0, 4.0, 5.0]))
                    .unwrap(),
            );
        }
        // A mixed (text) column, for the type refusals.
        columns.push(p.add_column(sheet, None, Some("T"), Column::new(ColumnType::Mixed)).unwrap());
        let session = Session::spawn(p, None).unwrap();
        let handle = session.handle();
        let python = EmbeddedPython::locate().expect(
            "vendored interpreter not found: set STOKESFORGE_PYTHON_HOME (see scripts/python-test.sh)",
        );
        let engine = ScriptEngine::spawn(handle.clone(), python).expect("engine start");
        std::sync::Mutex::new(Bench { _session: session, handle, engine, columns, sheet })
    })
}

/// Runs a script and returns (stdout, stderr, formatted error).
fn run(source: &str) -> (String, String, Option<String>) {
    let b = bench().lock().unwrap();
    let out = b
        .engine
        .run(source)
        .expect("the engine must accept the work");
    (out.stdout, out.stderr, out.error)
}

/// Identifiers of the bench columns, copied once. Rereading them through the
/// lock from an expression that already holds it would deadlock:
/// `std::sync::Mutex` is not reentrant.
fn column_a() -> NodeId {
    bench().lock().unwrap().columns[0]
}

fn values_of(col: NodeId) -> Vec<f64> {
    let b = bench().lock().unwrap();
    match b
        .handle
        .snapshot()
        .unwrap()
        .tree
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!("text column"),
    }
}

fn reset() {
    let b = bench().lock().unwrap();
    let col = b.columns[0];
    b.handle
        .execute(Command::SetCells {
            column: col,
            start: 0,
            values: ValueBlock::Float(vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            expected_version: None,
        })
        .unwrap();
    // #312: the engine is shared between tests and remembers the version it
    // read. The reset is an edit by "another client": the script rereads the
    // column, as it must before writing.
    b.engine
        .run("pyst.column(pyst.resolve('[Book1]Sheet1!A'))")
        .expect("the engine must accept the work");
}

// ============================================================== conversions

#[test]
fn b7_types_refused_by_set_column() {
    reset();
    let cases: &[(&str, &str)] = &[
        ("a Python list", "[1.0, 2.0, 3.0, 4.0, 5.0]"),
        ("an integer", "42"),
        ("a string", "'abcde'"),
        ("None", "None"),
        ("an int64 array", "np.arange(5)"),
        ("a float32 array", "np.zeros(5, dtype=np.float32)"),
        ("a 2-D array", "np.zeros((1, 5))"),
        ("a numpy scalar", "np.float64(1.0)"),
        ("a 0-D array", "np.array(1.0)"),
        ("a complex array", "np.zeros(5, dtype=np.complex128)"),
        ("an object array", "np.array([1.0]*5, dtype=object)"),
        ("an array of length 4", "np.zeros(4)"),
        ("an array of length 6", "np.zeros(6)"),
    ];
    let mut accepted = Vec::new();
    for (label, expr) in cases {
        let src = format!(
            "import numpy as np\n\
             h = pyst.resolve('[Book1]Sheet1!A')\n\
             try:\n    pyst.set_column(h, {expr})\n    print('ACCEPTED')\n\
             except Exception as e:\n    print(type(e).__name__)\n"
        );
        let (out, _err, error) = run(&src);
        assert!(
            error.is_none(),
            "{label}: the script itself failed: {error:?}"
        );
        let verdict = out.trim().to_string();
        println!("{label:30} -> {verdict}");
        if verdict == "ACCEPTED" {
            accepted.push((*label, values_of(column_a())));
        }
        reset();
    }
    assert!(
        accepted.is_empty(),
        "types that are not a 1-D float64 numpy array were accepted: {accepted:?}"
    );
}

#[test]
fn b7_byte_order_of_a_big_endian_array() {
    // A '>f8' array has the right dtype "float64" but the opposite byte
    // order. If it passes the check, its bytes are reread as they are: silent
    // corruption.
    reset();
    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        src = np.array([1.0, 2.0, 3.0, 4.0, 5.0])\n\
        be = src.astype('>f8')\n\
        print('dtype', be.dtype, 'Python values', list(be))\n\
        try:\n    pyst.set_column(h, be)\n    print('ACCEPTED')\n\
        except Exception as e:\n    print(type(e).__name__, e)\n";
    let (out, _err, error) = run(src);
    println!("{out}");
    assert!(error.is_none(), "{error:?}");
    if out.contains("ACCEPTED") {
        let v = values_of(column_a());
        assert_eq!(
            v,
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            "a big-endian array was accepted and reread backwards: the model \
             holds {v:?} instead of the values Python displayed"
        );
    }
    reset();
}

#[test]
fn b7_masked_array_loses_its_mask() {
    reset();
    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        m = np.ma.masked_array([1.0, 2.0, 3.0, 4.0, 5.0], mask=[0,1,0,1,0], fill_value=-999.0)\n\
        print('seen from Python:', m.filled())\n\
        try:\n    pyst.set_column(h, m)\n    print('ACCEPTED')\n\
        except Exception as e:\n    print(type(e).__name__)\n";
    let (out, _err, error) = run(src);
    println!("{out}");
    assert!(error.is_none(), "{error:?}");
    if out.contains("ACCEPTED") {
        let v = values_of(column_a());
        assert!(
            v[1].is_nan() && v[3].is_nan(),
            "a masked array was accepted and its mask ignored: the model \
             holds {v:?}, the masked cells are not missing"
        );
    }
    reset();
}

#[test]
fn b7_non_contiguous_arrays_are_read_correctly() {
    reset();
    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        base = np.arange(10, dtype=np.float64)\n\
        pyst.set_column(h, base[::2])          # step of 2\n\
        print('step2 ok')\n";
    let (out, _err, error) = run(src);
    assert!(error.is_none(), "{out} / {error:?}");
    assert_eq!(values_of(column_a()), vec![0.0, 2.0, 4.0, 6.0, 8.0]);

    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        pyst.set_column(h, np.arange(5, dtype=np.float64)[::-1])   # negative step\n";
    let (_out, _err, error) = run(src);
    assert!(error.is_none(), "{error:?}");
    assert_eq!(values_of(column_a()), vec![4.0, 3.0, 2.0, 1.0, 0.0]);
    reset();
}

#[test]
fn b7_special_values_cross_faithfully() {
    reset();
    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        v = np.array([np.nan, np.inf, -np.inf, -0.0, 5e-324])\n\
        pyst.set_column(h, v)\n";
    let (_out, _err, error) = run(src);
    assert!(error.is_none(), "{error:?}");
    let v = values_of(column_a());
    assert!(v[0].is_nan(), "NaN lost: {v:?}");
    assert_eq!(v[1], f64::INFINITY);
    assert_eq!(v[2], f64::NEG_INFINITY);
    assert_eq!(
        v[3].to_bits(),
        (-0.0f64).to_bits(),
        "negative zero became {} (bits {:x})",
        v[3],
        v[3].to_bits()
    );
    assert_eq!(v[4], 5e-324, "the minimal denormal became {}", v[4]);
    reset();
}

#[test]
fn b7_zero_copy_view_points_at_the_model() {
    reset();
    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        v = pyst.column(h)\n\
        print('writeable', v.flags.writeable, 'owndata', v.flags.owndata)\n\
        print('base', type(v.base).__name__)\n\
        print('values', list(v))\n\
        try:\n    v[0] = 99.0\n    print('WRITE ACCEPTED')\n\
        except ValueError as e:\n    print('write refused')\n";
    let (out, _err, error) = run(src);
    println!("{out}");
    assert!(error.is_none(), "{error:?}");
    assert!(
        out.contains("writeable False"),
        "the view is writable: {out}"
    );
    assert!(out.contains("write refused"), "{out}");
    assert!(
        !values_of(column_a()).contains(&99.0),
        "the model was modified through the read-only view"
    );
    reset();
}

#[test]
fn b7_view_kept_during_a_write_does_not_corrupt() {
    // Documented contract: the view stays on the old buffer (stale snapshot),
    // NEVER an invalid access. It is checked on a big buffer, so that the
    // reallocation is certain if it happens.
    reset();
    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!B')\n\
        pyst.set_column(h, np.arange(5, dtype=np.float64))\n\
        view = pyst.column(h)\n\
        before = view.copy()\n\
        for _ in range(200):\n\
        \x20   pyst.set_column(h, np.random.default_rng(0).random(5))\n\
        after = np.asarray(view)\n\
        print('view stable:', bool(np.array_equal(before, after)))\n\
        print('view reread:', list(pyst.column(h))[:2])\n\
        print('view sum :', float(np.sum(view)))\n";
    let (out, _err, error) = run(src);
    println!("{out}");
    assert!(error.is_none(), "{error:?}");
    assert!(
        out.contains("view stable: True"),
        "the numpy view changed under the script's feet: {out}"
    );
}

#[test]
fn b7_host_errors_become_the_three_announced_exceptions() {
    let src = "import numpy as np\n\
        def attempt(f):\n\
        \x20   try:\n\
        \x20       f()\n\
        \x20       return 'NONE'\n\
        \x20   except Exception as e:\n\
        \x20       return type(e).__name__\n\
        print('absurd path       :', attempt(lambda: pyst.resolve('[[[')))\n\
        print('empty path        :', attempt(lambda: pyst.resolve('')))\n\
        print('missing object    :', attempt(lambda: pyst.resolve('[Nope]S!C')))\n\
        print('sheet as column   :', attempt(lambda: pyst.column(pyst.resolve('[Book1]Sheet1'))))\n\
        print('text column       :', attempt(lambda: pyst.column(pyst.resolve('[Book1]Sheet1!T'))))\n\
        print('workbook as column:', attempt(lambda: pyst.column(pyst.resolve('[Book1]'))))\n";
    let (out, _err, error) = run(src);
    println!("{out}");
    assert!(error.is_none(), "{error:?}");
    for line in out.lines() {
        assert!(
            line.contains("UnknownPathError")
                || line.contains("BrokenReferenceError")
                || line.contains("InvalidValueError"),
            "a host error is not one of the three announced exceptions: {line}"
        );
    }
}

#[test]
fn b7_handle_is_opaque_and_cannot_be_forged() {
    let src = "\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        print('repr', repr(h))\n\
        print('attributes', [a for a in dir(h) if not a.startswith('__')])\n\
        try:\n    pyst.Handle()\n    print('CONSTRUCTIBLE')\n\
        except TypeError:\n    print('not constructible')\n\
        import pickle\n\
        try:\n    pickle.dumps(h)\n    print('PICKLABLE')\n\
        except Exception as e:\n    print('not picklable', type(e).__name__)\n";
    let (out, _err, error) = run(src);
    println!("{out}");
    assert!(error.is_none(), "{error:?}");
    assert!(
        !out.contains("CONSTRUCTIBLE"),
        "a handle can be forged: {out}"
    );
    assert!(
        out.contains("attributes []"),
        "the handle exposes attributes: {out}"
    );
}

#[test]
fn b7_surface_unusable_from_a_python_thread() {
    let src = "import threading\n\
        result = []\n\
        def work():\n\
        \x20   try:\n\
        \x20       result.append(('ok', pyst.resolve('[Book1]Sheet1!A')))\n\
        \x20   except Exception as e:\n\
        \x20       result.append((type(e).__name__, str(e)[:60]))\n\
        t = threading.Thread(target=work)\n\
        t.start(); t.join()\n\
        print(result[0][0])\n";
    let (out, _err, error) = run(src);
    println!("from a Python thread: {out}");
    assert!(error.is_none(), "{error:?}");
    assert!(
        !out.trim().starts_with("ok"),
        "the pyst surface is usable from a thread created by the script: {out}"
    );
    assert!(out.contains("RuntimeError"), "unexpected message: {out}");
}

#[test]
fn b7_undo_from_python_is_the_hosts_stack() {
    reset();
    let src = "import numpy as np\n\
        h = pyst.resolve('[Book1]Sheet1!A')\n\
        pyst.set_column(h, np.array([10.0, 20.0, 30.0, 40.0, 50.0]))\n\
        print('after write', list(pyst.column(h)))\n\
        print('undo', pyst.undo())\n\
        print('after undo  ', list(pyst.column(h)))\n";
    let (out, _err, error) = run(src);
    println!("{out}");
    assert!(error.is_none(), "{error:?}");
    let v = values_of(column_a());
    assert_eq!(
        v,
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        "pyst.undo() did not undo the write"
    );
    reset();
}

#[test]
fn b7_source_with_nul_byte_and_unicode() {
    let (out, _err, error) = run("print('\u{e9}\u{1f980}')");
    assert!(error.is_none(), "{error:?}");
    assert_eq!(out.trim(), "é🦀");
    // A nul byte in the source: documented refusal, no panic.
    let out = {
        let b = bench().lock().unwrap();
        b.engine
            .run("print('a\0b')")
            .expect("the engine accepts the work")
    };
    assert!(
        out.error.is_some(),
        "a source containing a nul byte was executed"
    );
    println!(
        "nul byte -> {}",
        out.error.unwrap().lines().last().unwrap_or("")
    );
}

#[test]
fn b7_python_exception_comes_back_with_its_trace() {
    let (_out, _err, error) = run("def f():\n    raise ValueError('boum')\nf()\n");
    let e = error.expect("an exception must come back");
    assert!(e.contains("ValueError"), "{e}");
    assert!(e.contains("boum"), "{e}");
    assert!(e.contains("Traceback"), "trace missing: {e}");
}

#[test]
fn b7_deep_recursion_on_the_python_side() {
    let (_out, _err, error) = run("def f(n):\n    return f(n+1)\ntry:\n    f(0)\nexcept RecursionError:\n    print('RecursionError')\n");
    assert!(
        error.is_none(),
        "the infinite recursion was not caught: {error:?}"
    );
}

#[test]
fn b7_massive_output_does_not_blow_up_the_capture() {
    let (out, _err, error) = run("for i in range(20000):\n    print('x' * 40)\n");
    assert!(error.is_none(), "{error:?}");
    println!("captured output: {} bytes", out.len());
    assert!(out.len() > 100, "the output disappeared");
}

#[test]
fn b7_mixed_write_and_numpy_cap_preserve_the_distinctions() {
    let (_, _, error) = run(r#"
import math, numpy as np
sheet = pyst.resolve('[Book1]Sheet1')
t = pyst.create_column(sheet, 'Frontière67', column_type='mixed')
pyst.set_values(t, [None, '', '12', float('nan'), -0.0, 2**53, -(2**53)])
v = pyst.values(t)
assert v[:3] == (None, '', '12') and math.isnan(v[3])
assert math.copysign(1., v[4]) < 0 and v[5:] == (float(2**53), -float(2**53))
for bad in ([2**53+1], [-(2**53)-1], np.broadcast_to(np.array([1.]), (100_000_001,))):
    try: pyst.set_values(t, bad)
    except pyst.InvalidValueError: pass
    else: raise AssertionError('unrepresentable write accepted')
assert pyst.values(t)[:3] == v[:3]
assert pyst.undo()
"#);
    assert!(error.is_none(), "{error:?}");
}

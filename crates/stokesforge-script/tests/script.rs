//! Integration tests of the script thread (tasks 5.3 to 5.5).
//!
//! They run only with the `python` feature and require the vendored prefix:
//! run `scripts/python-vendor.sh` then `scripts/python-test.sh`.
//!
//! Each test matches an acceptance criterion. Only one deserves to be read
//! before the others: [`end_criterion_zero_copy_read_write_through_the_bus_undo_with_ctrl_z`],
//! which crosses the actor, the GIL, the lifetime of the shared buffers and
//! the history in one go. Until it passes, the bridge is not proven.
//!
//! As in `stokesforge-python/tests/zerocopy.rs`, CPython starts only once per
//! process; here the script thread of the **first** engine created takes care
//! of it, and the following engines reuse the interpreter.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;
use std::time::{Duration, Instant};

use pyo3::prelude::*;
use stokesforge_commands::{Command, HistoryLabel, MacroKind};
use stokesforge_core::{Column, NodeId, Project, Tree};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptError, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

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

/// Interpreter access lock, **shared by the whole test binary**.
///
/// `sys.setswitchinterval` is a setting **global to the interpreter**.
/// Criterion 11 must push it very high to be decisive, and meanwhile every
/// other Python thread of the binary is starved: measured, a single test left
/// running in parallel with that one raised the duration of the suite from
/// 0.1 s to over 60 s. This reader/writer lock states exactly the constraint:
/// any test that runs a script takes **shared** access; the only one that
/// moves the switch interval takes **exclusive** access.
fn interpreter_lock() -> &'static RwLock<()> {
    static LOCK: OnceLock<RwLock<()>> = OnceLock::new();
    LOCK.get_or_init(|| RwLock::new(()))
}

/// The interpreter access held by a bench, kept alive as long as the bench is.
///
/// The guards are never *read*: their only purpose is their lifetime, hence
/// the `allow`; a guard that was read would be a guard misunderstood.
#[allow(dead_code)]
enum Access {
    /// Coexists with the other tests.
    Shared(RwLockReadGuard<'static, ()>),
    /// Excludes any other test that would run Python.
    Exclusive(RwLockWriteGuard<'static, ()>),
}

/// The bench: a project "Demo", its actor, and a script engine plugged into it.
///
/// `Book1` contains `Sheet1` with `A = [1, 2, 3]`, `B = [10, 20, 30]` and a
/// text column `T`. The latter is not in the specification's fixture; it
/// serves criterion 6 only ("`column` refuses what it must").
struct Bench {
    /// The engine, in an `Arc` so it can be shared with a test thread that
    /// must be abandonable if a bounded criterion expires.
    engine: Arc<ScriptEngine>,
    /// Client handle to the actor, on the Rust side ("what the window would do").
    session: SessionHandle,
    /// The actor itself; keeping it alive is its whole role here.
    _actor: Session,
    /// Colonne `A`.
    a: NodeId,
    /// Colonne `B`.
    b: NodeId,
    /// Interpreter access, released when the bench is dropped.
    _access: Access,
}

/// Builds a fresh bench, coexisting with the other tests.
fn bench() -> Bench {
    build_bench(Access::Shared(
        interpreter_lock()
            .read()
            .unwrap_or_else(PoisonError::into_inner),
    ))
}

/// Builds a fresh bench that **excludes** any other test running Python.
fn exclusive_bench() -> Bench {
    build_bench(Access::Exclusive(
        interpreter_lock()
            .write()
            .unwrap_or_else(PoisonError::into_inner),
    ))
}

/// Isolates the import template store from the machine running the tests.
///
/// `pyst.import_file` reads `import_recipes.json` from the user's
/// configuration folder; a saved template named "Jasco" came ahead of the
/// built-in template "Jasco (point-virgule)" and made the automatic choice
/// fail (#195). The config folder is a *process* state
/// (`STOKESFORGE_CONFIG_DIR`, the only variable that `directories` does not
/// ignore outside Linux, #332; `XDG_CONFIG_HOME` is also still set): it is set
/// once, to an empty folder kept until the end of the binary, before any
/// bench, so there is no race between parallel tests and no restoring.
fn hermetic_configuration() {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().expect("temporary configuration folder");
        std::env::set_var(
            stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
            dir.path(),
        );
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        dir
    });
}

/// Builds the project "Demo", its actor and a script engine plugged into it.
fn build_bench(access: Access) -> Bench {
    hermetic_configuration();
    let mut project = Project::new("Demo").expect("project Demo");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet Sheet1");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("column B");
    project
        .add_column(
            sheet,
            None,
            Some("T"),
            Column::from_text(&[Some("x".to_owned()), None, Some("z".to_owned())]),
        )
        .expect("column T");

    let actor = Session::spawn(project, None).expect("actor started");
    let session = actor.handle();
    let engine = ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(home()))
        .expect("script engine started");

    Bench {
        engine: Arc::new(engine),
        session,
        _actor: actor,
        a,
        b,
        _access: access,
    }
}

/// Runs a script and requires it to finish without error; returns the output.
fn execute(bench: &Bench, source: &str) -> ScriptOutcome {
    let outcome = bench
        .engine
        .run(source)
        .expect("the engine accepts the script");
    assert!(
        outcome.error.is_none(),
        "script failed:\n{}",
        outcome.error.clone().unwrap_or_default()
    );
    outcome
}

/// The Python import uses the supplied template and the shared conversion; a
/// single Ctrl+Z removes workbook, sheets, columns and metadata.
#[test]
fn python_import_by_template_reads_and_undoes_in_one_gesture() {
    let bench = bench();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import/jasco_uvvis_semicolon.csv");
    let before = bench.session.snapshot().unwrap();
    execute(
        &bench,
        &format!(
            "sheets_handle = pyst.import_file({:?}, recipe='Jasco (point-virgule)')\n\
         assert len(sheets_handle) == 1\n",
            path.to_str().unwrap()
        ),
    );
    let imported = bench.session.snapshot().unwrap();
    let root = imported.tree.root();
    let book = *imported.tree.node(root).unwrap().children().last().unwrap();
    let sheets = imported.tree.node(book).unwrap().children();
    assert_eq!(sheets.len(), 1, "no extra empty sheet");
    let columns = imported.tree.node(sheets[0]).unwrap().children();
    let x = imported.tree.node(columns[0]).unwrap().as_column().unwrap();
    let y = imported.tree.node(columns[1]).unwrap().as_column().unwrap();
    assert_eq!(x.meta().role, stokesforge_core::ColumnRole::X);
    assert_eq!(
        x.float_values().unwrap().as_slice(),
        &[800., 798., 796., 794., 792., 790.]
    );
    assert_eq!(y.meta().long_name, "Fixture_Jasco_UVVis");
    assert!(y.meta().file_metadata.iter().any(|m| m.key == "XUNITS"));
    assert!(y.meta().import_source.is_some());
    assert_eq!(
        imported.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    execute(
        &bench,
        &format!(
            "x = pyst.resolve('[{}]{}!{}')\n\
         assert list(pyst.column(x)) == [800., 798., 796., 794., 792., 790.]\n\
         assert pyst.undo()\n",
            imported.tree.node(book).unwrap().name(),
            imported.tree.node(sheets[0]).unwrap().name(),
            imported.tree.node(columns[0]).unwrap().name()
        ),
    );
    let undone = bench.session.snapshot().unwrap();
    assert_eq!(*undone.tree, *before.tree);
    assert!(!undone.history.can_undo);
    bench.session.redo().unwrap();
    assert_eq!(*bench.session.snapshot().unwrap().tree, *imported.tree);
}

/// The automatic choice uses the same Jasco signature as the application.
#[test]
fn python_import_chooses_the_template_automatically() {
    let bench = bench();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import/jasco_uvvis_semicolon.csv");
    execute(
        &bench,
        &format!(
            "from pathlib import Path\nassert len(pyst.import_file(Path({:?}))) == 1",
            path.to_str().unwrap()
        ),
    );
    let snapshot = bench.session.snapshot().unwrap();
    let source = snapshot
        .tree
        .walk_all()
        .into_iter()
        .filter_map(|id| {
            snapshot
                .tree
                .node(id)?
                .as_column()?
                .meta()
                .import_source
                .as_ref()
        })
        .next()
        .unwrap();
    assert!(source.recipe_json.contains("Jasco (point-virgule)"));
}

/// Unknown template, missing or binary file: no mutation and no macro left
/// open. An interface macro is never absorbed.
#[test]
fn python_import_refuses_without_touching_the_project_or_others_macros() {
    let bench = bench();
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("binary.dat");
    std::fs::write(&binary, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let absent = dir.path().join("absent.csv");
    let before = bench.session.snapshot().unwrap();
    for source in [
        format!(
            "pyst.import_file({:?}, recipe='Nonexistent template')",
            binary.to_str().unwrap()
        ),
        format!("pyst.import_file({:?})", absent.to_str().unwrap()),
        format!("pyst.import_file({:?})", binary.to_str().unwrap()),
    ] {
        let outcome = bench.engine.run(&source).unwrap();
        assert!(
            outcome
                .error
                .as_deref()
                .is_some_and(|e| e.contains("InvalidValueError")),
            "{outcome:?}"
        );
        let snapshot = bench.session.snapshot().unwrap();
        assert_eq!(*snapshot.tree, *before.tree);
        assert_eq!(snapshot.revision, before.revision);
        assert!(!snapshot.history.in_macro);
    }
    let csv = dir.path().join("measurement.csv");
    std::fs::write(&csv, "1,10\n2,20\n").unwrap();
    let interface = bench._actor.handle();
    interface.begin_macro(MacroKind::Import).unwrap();
    let outcome = bench
        .engine
        .run(&format!("pyst.import_file({:?})", csv.to_str().unwrap()))
        .unwrap();
    assert!(outcome
        .error
        .as_deref()
        .is_some_and(|e| e.contains("InvalidValueError")));
    let snapshot = bench.session.snapshot().unwrap();
    assert_eq!(*snapshot.tree, *before.tree);
    assert!(snapshot.history.in_macro, "the interface macro stays open");
    interface.abort_macro().unwrap();
}

/// Current values of a numeric column, read on the Rust side.
fn values(session: &SessionHandle, id: NodeId) -> Vec<f64> {
    session
        .snapshot()
        .expect("snapshot")
        .tree
        .node(id)
        .expect("node present")
        .as_column()
        .expect("column")
        .float_values()
        .expect("numeric column")
        .as_slice()
        .to_vec()
}

/// Address of a column's `Vec<f64>`, as Rust sees it.
fn rust_address(session: &SessionHandle, id: NodeId) -> usize {
    session
        .snapshot()
        .expect("snapshot")
        .tree
        .node(id)
        .expect("node present")
        .as_column()
        .expect("column")
        .float_values()
        .expect("numeric column")
        .as_ptr() as usize
}

/// Current tree of the project.
fn tree(session: &SessionHandle) -> Arc<Tree> {
    session.snapshot().expect("snapshot").tree
}

/// Current revision of the project.
fn revision(session: &SessionHandle) -> u64 {
    session.snapshot().expect("snapshot").revision
}

/// The lines of a script's standard output, without the border whitespace.
fn stdout_lines(outcome: &ScriptOutcome) -> Vec<String> {
    outcome
        .stdout
        .lines()
        .map(|line| line.trim().to_owned())
        .collect()
}

/// Waits for `condition` to become true, for at most `delay`. Returns `false`
/// if the delay expires: no test may wait indefinitely.
fn wait_until(delay: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    condition()
}

// --- 1. The engine starts ---------------------------------------------------

#[test]
fn the_engine_starts_and_runs_an_empty_script() {
    let bench = bench();
    assert!(!bench.engine.is_running(), "no script at startup");

    let outcome = execute(&bench, "");
    assert_eq!(outcome, ScriptOutcome::default());
    assert!(outcome.succeeded());
    assert!(!bench.engine.is_running(), "nothing runs afterwards");

    Arc::try_unwrap(bench.engine)
        .expect("a single holder of the engine")
        .shutdown()
        .expect("clean shutdown");
}

// --- 2. `print` captured -----------------------------------------------------

/// Marker unusual enough that no other output of the test binary can produce
/// it by chance.
const MARKER: &str = "hello-4f1c9a2e-witness";

/// Redirects descriptor 1 of the **process** to a pipe for the duration of
/// `action`, then returns what flowed into it.
///
/// This is the only honest way to check that "nothing appears on the process
/// output": `sys.stdout` as seen from Python says nothing about what actually
/// reaches the descriptor. The assertion is on the **absence of the marker**,
/// not on an empty pipe: other tests may write to the process output during
/// this window (the `libtest` harness, notably).
fn process_output_during<T>(action: impl FnOnce() -> T) -> (T, String) {
    let (saved, read_end) = Python::attach(|py| {
        let os = py.import("os").expect("import os");
        let saved: i32 = os
            .call_method1("dup", (1,))
            .expect("dup")
            .extract()
            .expect("descriptor");
        let pipe = os.call_method0("pipe").expect("pipe");
        let read_end: i32 = pipe
            .get_item(0)
            .expect("read end")
            .extract()
            .expect("descriptor");
        let write_end: i32 = pipe
            .get_item(1)
            .expect("write end")
            .extract()
            .expect("descriptor");
        os.call_method1("dup2", (write_end, 1)).expect("dup2");
        os.call_method1("close", (write_end,)).expect("close");
        (saved, read_end)
    });

    let value = action();

    let leak = Python::attach(|py| {
        let os = py.import("os").expect("import os");
        // Putting the real output back closes the last write end of the pipe:
        // the read that follows therefore sees end of file and does not
        // block.
        os.call_method1("dup2", (saved, 1)).expect("dup2");
        os.call_method1("close", (saved,)).expect("close");
        let bytes: Vec<u8> = os
            .call_method1("read", (read_end, 65_536))
            .expect("read")
            .extract()
            .expect("bytes");
        os.call_method1("close", (read_end,)).expect("close");
        String::from_utf8_lossy(&bytes).into_owned()
    });

    (value, leak)
}

#[test]
fn print_is_captured_and_does_not_leak_onto_the_process_output() {
    let bench = bench();
    let source =
        format!("print(\"{MARKER}\")\nimport sys\nprint(\"to stderr\", file=sys.stderr)\n");

    let (outcome, leak) = process_output_during(|| execute(&bench, &source));

    assert!(
        outcome.stdout.contains(MARKER),
        "stdout captured: {:?}",
        outcome.stdout
    );
    assert!(
        outcome.stderr.contains("to stderr"),
        "stderr captured: {:?}",
        outcome.stderr
    );
    assert!(
        !leak.contains(MARKER),
        "the marker leaked onto the process output: {leak:?}"
    );
}

// --- 3. Call trace ------------------------------------------------------

#[test]
fn an_exception_returns_the_full_trace_and_keeps_the_output_already_written() {
    let bench = bench();
    let outcome = bench
        .engine
        .run("print(\"before\")\n1 / 0\n")
        .expect("the engine accepts the script");

    let error = outcome.error.clone().expect("the script failed");
    assert!(error.contains("ZeroDivisionError"), "{error}");
    assert!(error.contains("Traceback"), "{error}");
    assert!(
        outcome.stdout.contains("before"),
        "what was printed before the error is kept: {:?}",
        outcome.stdout
    );
    assert!(!outcome.interrupted, "this is not an interruption");
}

// --- 4. `resolve` -----------------------------------------------------------

#[test]
fn resolve_returns_an_opaque_handle() {
    let bench = bench();
    let outcome = execute(
        &bench,
        r#"
h = pyst.resolve("[Book1]Sheet1!A")
print(repr(h))
print(sorted(name for name in dir(h) if not name.startswith("_")))
try:
    type(h)(1)
    print("CONSTRUCTIBLE")
except TypeError:
    print("NON-CONSTRUCTIBLE")
"#,
    );

    let stdout_lines = stdout_lines(&outcome);
    assert!(
        stdout_lines[0].starts_with("Handle(") && stdout_lines[0].ends_with(')'),
        "readable repr expected, got {:?}",
        stdout_lines[0]
    );
    assert_eq!(
        stdout_lines[1], "[]",
        "no public attribute may expose the integer: {:?}",
        stdout_lines[1]
    );
    assert_eq!(
        stdout_lines[2], "NON-CONSTRUCTIBLE",
        "a script must not be able to forge a handle"
    );
}

#[test]
fn resolve_raises_unknown_path_error_on_an_unknown_or_invalid_path() {
    let bench = bench();
    let outcome = execute(
        &bench,
        r#"
for path in ("[Absent]Sheet1!A", "[Book1]Sheet1!NonexistentColumn", "[Book1", "", "!!"):
    try:
        pyst.resolve(path)
        print("RESOLVED", path)
    except pyst.UnknownPathError:
        print("UNKNOWN", path)
    except Exception as error:
        print("OTHER", type(error).__name__, path)
"#,
    );

    for line in stdout_lines(&outcome) {
        assert!(
            line.starts_with("UNKNOWN"),
            "every faulty path must raise UnknownPathError: {line}"
        );
    }
}

// --- 5. `column` without copy -------------------------------------------------

#[test]
fn column_is_a_zero_copy_view_on_the_model_buffer() {
    let bench = bench();
    let expected = rust_address(&bench.session, bench.a);

    let outcome = execute(
        &bench,
        r#"
v = pyst.column(pyst.resolve("[Book1]Sheet1!A"))
print([float(x) for x in v])
print(v.flags.writeable)
print(v.__array_interface__["data"][0])
"#,
    );

    let stdout_lines = stdout_lines(&outcome);
    assert_eq!(stdout_lines[0], "[1.0, 2.0, 3.0]");
    assert_eq!(stdout_lines[1], "False", "the view must be read-only");
    let view: usize = stdout_lines[2].parse().expect("integer address");
    assert_eq!(
        view, expected,
        "numpy must look at EXACTLY the model's Vec: a silent copy \
         would return the same values at another address"
    );
}

// --- 6. `column` refuses what it must ---------------------------------------

#[test]
fn column_refuses_a_sheet_and_a_text_column() {
    let bench = bench();
    let outcome = execute(
        &bench,
        r#"
for path in ("[Book1]Sheet1", "[Book1]Sheet1!T"):
    try:
        pyst.column(pyst.resolve(path))
        print("ACCEPTED", path)
    except pyst.InvalidValueError:
        print("REFUSED", path)
    except Exception as error:
        print("OTHER", type(error).__name__, path)
"#,
    );

    assert_eq!(
        stdout_lines(&outcome),
        vec![
            "REFUSED [Book1]Sheet1".to_owned(),
            "REFUSED [Book1]Sheet1!T".to_owned()
        ]
    );
}

// --- 7. `set_column` goes through the bus ---------------------------------------

#[test]
fn set_column_writes_through_the_bus() {
    let bench = bench();
    execute(
        &bench,
        r#"
a = pyst.resolve("[Book1]Sheet1!A")
b = pyst.resolve("[Book1]Sheet1!B")
pyst.set_column(b, pyst.column(a))
"#,
    );

    assert_eq!(values(&bench.session, bench.b), vec![1.0, 2.0, 3.0]);
    assert!(
        bench.session.snapshot().expect("snapshot").history.can_undo,
        "the write must have left a history entry"
    );
}

#[test]
fn set_column_refuses_a_different_length_without_sending_a_command() {
    let bench = bench();
    let before = revision(&bench.session);

    let outcome = execute(
        &bench,
        r#"
import numpy
b = pyst.resolve("[Book1]Sheet1!B")
for array in (numpy.array([1.0, 2.0]), numpy.array([1.0, 2.0, 3.0, 4.0])):
    try:
        pyst.set_column(b, array)
        print("ACCEPTED", len(array))
    except pyst.InvalidValueError:
        print("REFUSED", len(array))
"#,
    );

    assert_eq!(
        stdout_lines(&outcome),
        vec!["REFUSED 2".to_owned(), "REFUSED 4".to_owned()]
    );
    assert_eq!(
        revision(&bench.session),
        before,
        "a length refusal must send no command"
    );
    assert_eq!(values(&bench.session, bench.b), vec![10.0, 20.0, 30.0]);
}

#[test]
fn set_column_accepts_a_non_contiguous_array() {
    let bench = bench();
    let outcome = execute(
        &bench,
        r#"
import numpy
source = numpy.arange(6.0)
slice = source[::2]
print(slice.flags["C_CONTIGUOUS"])
pyst.set_column(pyst.resolve("[Book1]Sheet1!B"), slice)
"#,
    );

    assert_eq!(
        stdout_lines(&outcome)[0],
        "False",
        "the slice must indeed be non-contiguous, otherwise the test proves nothing"
    );
    assert_eq!(values(&bench.session, bench.b), vec![0.0, 2.0, 4.0]);
}

// --- 8. `undo` from Python ------------------------------------------------

#[test]
fn undo_from_python_undoes_the_writes_then_returns_false() {
    let bench = bench();
    let outcome = execute(
        &bench,
        r#"
import numpy
a = pyst.resolve("[Book1]Sheet1!A")
b = pyst.resolve("[Book1]Sheet1!B")
pyst.set_column(b, pyst.column(a))
pyst.set_column(b, numpy.array([7.0, 8.0, 9.0]))
print([float(x) for x in pyst.column(b)])
print(pyst.undo())
print([float(x) for x in pyst.column(b)])
print(pyst.undo())
print([float(x) for x in pyst.column(b)])
print(pyst.undo())
"#,
    );

    assert_eq!(
        stdout_lines(&outcome),
        vec![
            "[7.0, 8.0, 9.0]".to_owned(),
            "True".to_owned(),
            "[10.0, 20.0, 30.0]".to_owned(),
            "False".to_owned(),
            "[10.0, 20.0, 30.0]".to_owned(),
            // Empty stack: `False`, never an exception.
            "False".to_owned(),
        ]
    );
    assert_eq!(values(&bench.session, bench.b), vec![10.0, 20.0, 30.0]);
}

// --- 9. A single stack ------------------------------------------------------

#[test]
fn a_single_history_stack_for_the_actor_and_the_script() {
    let bench = bench();
    // A command coming from the Rust side, as a cell entry would.
    bench
        .session
        .execute(Command::SetCells {
            column: bench.a,
            start: 0,
            values: stokesforge_core::ValueBlock::Float(vec![42.0]),
            expected_version: None,
        })
        .expect("actor command");
    assert_eq!(values(&bench.session, bench.a), vec![42.0, 2.0, 3.0]);

    let outcome = execute(&bench, "print(pyst.undo())\n");

    assert_eq!(stdout_lines(&outcome), vec!["True".to_owned()]);
    assert_eq!(
        values(&bench.session, bench.a),
        vec![1.0, 2.0, 3.0],
        "it is indeed the actor's command that the script undid"
    );
}

// --- 10. THE END CRITERION --------------------------------------------------

#[test]
fn end_criterion_zero_copy_read_write_through_the_bus_undo_with_ctrl_z() {
    let bench = bench();
    let tree_before = tree(&bench.session);
    let address_of_a = rust_address(&bench.session, bench.a);

    // A script reads A without copying and writes B through the bus.
    let outcome = execute(
        &bench,
        r#"
a = pyst.resolve("[Book1]Sheet1!A")
b = pyst.resolve("[Book1]Sheet1!B")
view_read = pyst.column(a)
print(view_read.__array_interface__["data"][0])
pyst.set_column(b, view_read)
print([float(x) for x in pyst.column(b)])
"#,
    );

    let stdout_lines = stdout_lines(&outcome);
    let address_seen_by_numpy: usize = stdout_lines[0].parse().expect("integer address");
    assert_eq!(
        address_seen_by_numpy, address_of_a,
        "the read must be WITHOUT COPY: numpy looks at the model's Vec"
    );
    assert_eq!(stdout_lines[1], "[1.0, 2.0, 3.0]");
    assert_eq!(values(&bench.session, bench.b), vec![1.0, 2.0, 3.0]);

    // Ctrl+Z in the window: an undo coming from the Rust side.
    let undone = bench.session.undo().expect("host undo");
    assert!(undone.is_some(), "there was indeed something to undo");

    assert_eq!(values(&bench.session, bench.b), vec![10.0, 20.0, 30.0]);
    assert_eq!(
        tree(&bench.session),
        tree_before,
        "after undo, the tree must be bit-for-bit equal to the one before the script"
    );
}

// --- 11. The GIL is released while waiting for the actor -------------------

#[test]
fn the_gil_is_released_while_waiting_for_the_actor() {
    // Exclusive access: this test moves a global setting of the interpreter.
    let bench = exclusive_bench();

    // A huge switch interval prevents the interpreter from releasing the GIL
    // on its own between two instructions. If another thread nonetheless
    // manages to run Python during the loop, the script thread released it
    // EXPLICITLY, meaning `Python::detach` does its job. Without this
    // precaution, the test would pass by itself: CPython yields every 5 ms,
    // whatever the script does.
    let source = r#"
import sys
previous = sys.getswitchinterval()
sys.setswitchinterval(5.0)
try:
    b = pyst.resolve("[Book1]Sheet1!B")
    values = pyst.column(pyst.resolve("[Book1]Sheet1!A"))
    for _ in range(500):
        pyst.set_column(b, values)
finally:
    sys.setswitchinterval(previous)
"#;

    let counter = Arc::new(AtomicUsize::new(0));
    let witness_counter = Arc::clone(&counter);
    let witness_engine = Arc::clone(&bench.engine);
    let witness = thread::spawn(move || {
        if !wait_until(Duration::from_secs(30), || witness_engine.is_running()) {
            return;
        }
        while witness_engine.is_running() {
            Python::attach(|py| {
                let _ = py.eval(c"1 + 1", None, None);
            });
            witness_counter.fetch_add(1, Ordering::Relaxed);
            // Without this pause, the witness thread retakes the GIL as soon as
            // it is released and puts the script thread in a convoy: the
            // 500-round loop went from a few tens of milliseconds to about
            // twenty seconds. The test measures the POSSIBILITY of taking the
            // GIL, not the ability to monopolize it.
            thread::sleep(Duration::from_micros(200));
        }
    });

    let outcome = bench
        .engine
        .run(source)
        .expect("the engine accepts the script");
    witness.join().expect("the witness thread finishes");

    assert!(
        outcome.error.is_none(),
        "script failed:\n{}",
        outcome.error.clone().unwrap_or_default()
    );
    let rounds = counter.load(Ordering::Relaxed);
    assert!(
        rounds >= 20,
        "another thread must be able to run Python during the loop: \
         {rounds} round(s) obtained. With a switch interval of 5 s, \
         a script thread that kept the GIL during its round trips would let \
         none through."
    );
}

// --- 12. Interruption -------------------------------------------------------

/// Runs `source` on a detached thread and returns the channel that will carry
/// its result.
///
/// Detached, and not `thread::scope`: if a bounded criterion expires, the
/// thread must be abandonable rather than blocking the test on its join.
fn spawn_in_background(
    engine: &Arc<ScriptEngine>,
    source: &'static str,
) -> mpsc::Receiver<Result<ScriptOutcome, ScriptError>> {
    let (tx, rx) = mpsc::channel();
    let engine = Arc::clone(engine);
    thread::spawn(move || {
        let _ = tx.send(engine.run(source));
    });
    rx
}

#[test]
fn interruption_of_a_loop_of_pyst_calls() {
    let bench = bench();
    let result = spawn_in_background(&bench.engine, "while True:\n    pyst.undo()\n");

    assert!(
        wait_until(Duration::from_secs(30), || bench.engine.is_running()),
        "the script must have started"
    );
    bench.engine.interrupt();

    let outcome = result
        .recv_timeout(Duration::from_secs(30))
        .expect("the script must stop")
        .expect("the engine had accepted the script");
    assert!(outcome.interrupted, "the stop must be reported as intended");
    assert!(
        outcome
            .error
            .as_deref()
            .is_some_and(|error| error.contains("KeyboardInterrupt")),
        "the trace must say where the script stopped: {:?}",
        outcome.error
    );
}

#[test]
fn interruption_of_a_pure_python_loop() {
    let bench = bench();
    let result = spawn_in_background(&bench.engine, "while True:\n    pass\n");

    assert!(
        wait_until(Duration::from_secs(30), || bench.engine.is_running()),
        "the script must have started"
    );
    // Let the loop enter the `while`: the asynchronous exception only takes
    // effect at an interpreter check point.
    thread::sleep(Duration::from_millis(50));
    bench.engine.interrupt();

    let outcome = result
        .recv_timeout(Duration::from_secs(30))
        .expect("a pure Python loop must stop too")
        .expect("the engine had accepted the script");
    assert!(outcome.interrupted, "the stop must be reported as intended");
    assert!(
        outcome
            .error
            .as_deref()
            .is_some_and(|error| error.contains("KeyboardInterrupt")),
        "the loop must have been broken by a KeyboardInterrupt: {:?}",
        outcome.error
    );
}

// --- 13. One script at a time ------------------------------------------------

#[test]
fn one_script_at_a_time() {
    let bench = bench();
    let result = spawn_in_background(&bench.engine, "while True:\n    pyst.undo()\n");

    assert!(
        wait_until(Duration::from_secs(30), || bench.engine.is_running()),
        "the first script must have started"
    );
    let refusal = bench.engine.run("pass");
    assert!(
        matches!(refusal, Err(ScriptError::Busy)),
        "a second script must be refused: {refusal:?}"
    );

    bench.engine.interrupt();
    let _ = result.recv_timeout(Duration::from_secs(30));
}

// --- 14. The thread survives a script in error --------------------------------

#[test]
fn the_thread_survives_a_script_in_error() {
    let bench = bench();
    let failed = bench
        .engine
        .run("print(\"before\")\n1 / 0\n")
        .expect("the engine accepts the script");
    assert!(failed.error.is_some());

    let afterwards = execute(
        &bench,
        "print([float(x) for x in pyst.column(pyst.resolve(\"[Book1]Sheet1!A\"))])\n",
    );
    assert_eq!(
        stdout_lines(&afterwards),
        vec!["[1.0, 2.0, 3.0]".to_owned()]
    );
    assert!(!bench.engine.is_running());
}

// --- 15. Handle and undo ----------------------------------------------

#[test]
fn a_handle_survives_an_undone_deletion() {
    let bench = bench();
    // The namespace is kept from one run to the next: the handle set here
    // will still be there at the third script.
    execute(&bench, "h = pyst.resolve(\"[Book1]Sheet1!B\")\n");

    bench
        .session
        .execute(Command::DeleteColumn { column: bench.b })
        .expect("deletion on the Rust side");

    let broken = execute(
        &bench,
        r#"
try:
    pyst.column(h)
    print("STILL THERE")
except pyst.BrokenReferenceError:
    print("BROKEN")
"#,
    );
    assert_eq!(stdout_lines(&broken), vec!["BROKEN".to_owned()]);

    bench.session.undo().expect("undo of the deletion");

    let restored = execute(&bench, "print([float(x) for x in pyst.column(h)])\n");
    assert_eq!(
        stdout_lines(&restored),
        vec!["[10.0, 20.0, 30.0]".to_owned()],
        "a handle stays valid after an undone deletion"
    );
}

// --- Additions -----------------------------------------------------------------
//
// The exact list of `pyst` calls, signatures, keys and values is no longer
// checked here: see `tests/frozen_api.rs` (#331), which reads "Frozen API V1".

#[test]
fn the_pyst_surface_is_unusable_from_a_thread_created_by_the_script() {
    let bench = bench();
    let outcome = execute(
        &bench,
        r#"
import threading
result = []
def work():
    try:
        pyst.undo()
        result.append("PASSED")
    except RuntimeError:
        result.append("REFUSED")
worker = threading.Thread(target=work)
worker.start()
worker.join()
print(result[0])
"#,
    );
    assert_eq!(stdout_lines(&outcome), vec!["REFUSED".to_owned()]);
}

/// `pyst.add_shape` / `pyst.remove_shape` (issue #76) go through `SetPlotSpec`:
/// the calls join the script's group, the object is placed above the others,
/// and a refusal (unknown kind, invalid color, non-graph) touches nothing.
#[test]
fn graphic_objects_can_be_placed_removed_and_undone_from_python() {
    let bench = bench();
    let root = bench.session.snapshot().unwrap().tree.root();
    bench
        .session
        .execute(stokesforge_commands::Command::AddGraph {
            parent: root,
            name: Some("Graph1".to_owned()),
            spec: stokesforge_plot::PlotSpec::empty("G").with_panels(
                stokesforge_plot::Panels::new(vec![stokesforge_plot::PanelSpec::at(0, 0)]),
            ),
        })
        .expect("graph");
    let shapes = |bench: &Bench| -> Vec<stokesforge_plot::ShapeSpec> {
        let snapshot = bench.session.snapshot().unwrap();
        let id = snapshot.tree.resolve("[Graph1]").unwrap();
        match snapshot.tree.node(id).unwrap().kind() {
            stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.panels[0].shapes.clone(),
            _ => panic!("not a graph"),
        }
    };

    execute(
        &bench,
        r##"
g = pyst.resolve("[Graph1]")
assert pyst.add_shape(g, "rectangle", 1, 2, 3, 4, fill_color="#33669980", line_width=2) == 0
assert pyst.add_shape(g, "arrow", 0, 0, 5, 5, line_color="#aa3311") == 1
"##,
    );
    let placed = shapes(&bench);
    assert_eq!(placed.len(), 2);
    assert_eq!(placed[0].kind, stokesforge_plot::ShapeKind::Rectangle);
    assert_eq!(placed[0].fill_color, "#33669980");
    assert_eq!(placed[0].line_width, 2.0);
    assert_eq!(placed[1].kind, stokesforge_plot::ShapeKind::Arrow);
    assert_eq!((placed[1].x2, placed[1].y2), (5.0, 5.0));

    // A refusal changes nothing and raises InvalidValueError.
    let before = bench.session.snapshot().unwrap().revision;
    execute(
        &bench,
        r##"
g = pyst.resolve("[Graph1]")
for call in (
    lambda: pyst.add_shape(g, "triangle", 0, 0, 1, 1),
    lambda: pyst.add_shape(g, "line", 0, 0, 1, 1, line_color="red"),
    lambda: pyst.add_shape(g, "line", 0, 0, float("nan"), 1),
    lambda: pyst.add_shape(g, "line", 0, 0, 1, 1, panel=(4, 4)),
    lambda: pyst.add_shape(g, "line", 0, 0, 1, 1, axis="top"),
    lambda: pyst.remove_shape(g, 9),
    lambda: pyst.add_shape(pyst.resolve("[Book1]Sheet1!A"), "line", 0, 0, 1, 1),
):
    try:
        call()
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError("refusal expected")
"##,
    );
    assert_eq!(bench.session.snapshot().unwrap().revision, before);

    execute(&bench, "pyst.remove_shape(pyst.resolve('[Graph1]'), 0)\n");
    let remaining = shapes(&bench);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].kind, stokesforge_plot::ShapeKind::Arrow);
    execute(&bench, "assert pyst.undo()\n");
    assert_eq!(
        shapes(&bench).len(),
        2,
        "Ctrl+Z restores the removed object"
    );
}

//! Acceptance criteria of the script console, driven **without a window**
//! but **with** the interpreter.
//!
//! Symmetric to `tests/shell.rs`: that one proves the bench works without the
//! Python bridge, this one proves that the bridge, once plugged in, keeps the
//! promises of task 5.6 — output retrieved, grid refreshed without any extra
//! action, `Action::Undo` undoing what a script wrote, script exception that
//! breaks nothing — seen from `AppState`, exactly as the window would see
//! them.
//!
//! Requires the vendored prefix and the Python environment described in
//! `.codex/README.md`, then `cargo test -p stokesforge-app-state --features python
//! --test script_panel`. `scripts/python-test.sh` also runs the application
//! state tests, including this file and the numpy → columns → graph path.
//!
//! # `Action::RunScript` does not block
//!
//! [`AppState::dispatch`] never blocks, `RunScript` included (see the top-level
//! doc of `stokesforge_app_state::state`): the result arrives only through
//! [`AppState::poll_script`]. Each test therefore waits for a run by polling
//! this method in a short loop ([`run_and_wait`]) — the same gesture that
//! `stokesforge_app::app` makes on every frame, never a blocking `sleep` that
//! just hopes.
//!
//! # The lazy start of the interpreter is NOT proven here
//!
//! `EmbeddedPython::is_initialized()` is a **process-global** state, and Cargo
//! runs the tests of a single binary in parallel by default: this file
//! contains several tests that all start a script, so the interpreter is
//! initialized anyway from the first of them to run, in an unguaranteed order.
//! The "false before, true after" proof (criterion 8) requires being the
//! **only one** to touch Python in its process: it lives in its own file,
//! `tests/script_bridge_lazy.rs`, which is its own test binary.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use stokesforge_app_state::state::{Action, AppState, UnsavedChoice};
use stokesforge_core::{Column, NodeId, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_session::Session;

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

/// Maximum time a test waits for a script to finish before giving up: no test
/// must wait indefinitely.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Demo project of this file: a `Book1` workbook with `Sheet1` and a numeric
/// column `Time`. Independent of the binary's demo project (`main.rs`): this
/// file must not break if that one changes, and vice versa — same principle
/// as `tests/shell.rs`.
fn demo_project() -> Project {
    let mut project = Project::new("Demo").expect("demo project");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet1 = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 readable")
        .expect("Sheet1 created with the workbook");
    project
        .add_column(
            sheet1,
            None,
            Some("Time"),
            Column::from_f64(&[0.0, 0.5, 1.0, 1.5, 2.0]),
        )
        .expect("Time column");
    project
}

/// Starts a session on the demo project and the associated application state,
/// with an interpreter descriptor set (but **not started** — see the doc of
/// `stokesforge_app_state::state`). Returns the session, to be kept alive, and
/// the explicit sheet that the tests drive, like `tests/shell.rs`.
fn start() -> (Session, AppState, NodeId) {
    let project = demo_project();
    let sheet = project.tree().resolve("[Book1]Sheet1").expect("demo sheet");
    let session = Session::spawn(project, None).expect("session thread");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        "script-test.stf".to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .expect("initial application state");
    state.set_python(EmbeddedPython::with_home(home()));
    (session, state, sheet)
}

/// Identifier of the column `name` in `sheet`.
fn column_id(state: &AppState, sheet: NodeId, name: &str) -> NodeId {
    state
        .visible_columns(sheet)
        .into_iter()
        .find(|(_, column_name, _)| column_name == name)
        .map(|(id, _, _)| id)
        .unwrap_or_else(|| panic!("column {name} visible in {sheet}"))
}

/// Sets `source` as the current script, runs it and waits — by polling
/// [`AppState::poll_script`], as `stokesforge_app::app` does on every frame —
/// until the result is ready. Panics if `TIMEOUT` is exceeded.
fn run_and_wait(state: &mut AppState, source: &str) {
    state.set_script_source(source.to_string());
    state.dispatch(Action::RunScript);

    let deadline = Instant::now() + TIMEOUT;
    while state.script_running() {
        assert!(
            Instant::now() <= deadline,
            "the script did not finish within {TIMEOUT:?}"
        );
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }
}

// --- 4. A script runs and its output comes back -----------------------------

#[test]
fn a_script_runs_and_its_output_comes_back() {
    let (_session, mut state, _sheet) = start();

    run_and_wait(&mut state, "print(\"hello\")\n");

    let output = state.script_output().expect("a recorded output");
    assert!(output.error.is_none(), "script failed: {:?}", output.error);
    assert!(
        output.stdout.contains("hello"),
        "output: {:?}",
        output.stdout
    );
}

// --- 5. The grid reflects the model after the script -------------------------

#[test]
fn the_grid_reflects_the_model_after_a_script_without_any_extra_action() {
    let (_session, mut state, sheet) = start();
    let time_column = column_id(&state, sheet, "Time");
    assert_eq!(state.cell_text(time_column, 0), "0");
    assert_eq!(state.cell_text(time_column, 4), "2");

    run_and_wait(
        &mut state,
        "a = pyst.resolve(\"[Book1]Sheet1!Time\")\n\
         pyst.set_column(a, pyst.column(a) * 0.0 + 42.0)\n",
    );

    // No extra action was dispatched: it is `poll_script`, polled by
    // `run_and_wait` (as `stokesforge_app::app` does on every frame), that
    // refreshed the snapshot. This is the trap of this task.
    assert_eq!(state.cell_text(time_column, 0), "42");
    assert_eq!(state.cell_text(time_column, 4), "42");
}

// --- 6. Undoing what a script wrote ------------------------------------------

#[test]
fn action_undo_reverts_what_a_script_wrote() {
    let (_session, mut state, sheet) = start();
    let time_column = column_id(&state, sheet, "Time");
    assert_eq!(state.cell_text(time_column, 1), "0.5");

    run_and_wait(
        &mut state,
        "a = pyst.resolve(\"[Book1]Sheet1!Time\")\n\
         pyst.set_column(a, pyst.column(a) * 0.0 + 99.0)\n",
    );
    assert_eq!(state.cell_text(time_column, 1), "99");
    assert!(
        state.can_undo(),
        "the script's write must be in the history"
    );

    // Ctrl+Z on the window side: this is the final criterion, seen from the interface.
    state.dispatch(Action::Undo);

    assert_eq!(
        state.cell_text(time_column, 1),
        "0.5",
        "Action::Undo must undo the write made by the script"
    );
}

// --- 7. A script exception is reported, its trace stays in the console -------

#[test]
fn a_script_exception_is_a_short_error_with_its_trace_in_the_console() {
    let (_session, mut state, sheet) = start();
    let before = state.snapshot().revision;

    run_and_wait(&mut state, "1 / 0\n");

    let output = state.script_output().expect("a recorded output");
    assert!(
        output
            .error
            .as_deref()
            .is_some_and(|trace| trace.contains("ZeroDivisionError")),
        "expected a trace with ZeroDivisionError: {:?}",
        output.error
    );
    assert!(
        output
            .error
            .as_deref()
            .is_some_and(|trace| trace.contains("Traceback")),
        "expected a full call trace: {:?}",
        output.error
    );

    // S-M10 (#230): the exception is reported in the status bar, the trace
    // staying in the console — a single short message, never the trace.
    let status = state.status();
    assert!(
        status.is_some_and(
            |status| status.is_error() && status.text == "Script failed. See the console"
        ),
        "a script exception must be reported as a short error: {status:?}"
    );
    // Nothing was written to the model: the revision did not move.
    assert_eq!(state.snapshot().revision, before);

    // The state really remains usable: a normal action still works after a
    // script exception.
    let columns_before = state.visible_columns(sheet).len();
    state.dispatch(Action::AddColumn { sheet });
    assert!(state.status().is_some_and(|status| !status.is_error()));
    assert_eq!(state.visible_columns(sheet).len(), columns_before + 1);
}

#[test]
fn reading_calculation_results_and_graph_refresh_and_undo_together() {
    let (_session, mut state, _sheet) = start();
    let before = state.snapshot().tree.clone();
    run_and_wait(
        &mut state,
        r#"
import numpy as np
source = pyst.column(pyst.resolve('[Book1]Sheet1!Time'))
book = pyst.create_workbook('Résultats')
sheet = pyst.create_worksheet(book, 'Calculation')
x = pyst.create_column(sheet, 'X')
y = pyst.create_column(sheet, 'Y')
pyst.set_values(x, np.asarray(source, dtype=np.float64))
pyst.set_values(y, source**2)
pyst.set_column_meta(y, unit='mV', role='y')
pyst.plot(y, x, name='Graph1', title='$t^2$')
"#,
    );
    assert!(
        state.script_output().unwrap().error.is_none(),
        "{:?}",
        state.script_output()
    );
    assert!(state.snapshot().tree.resolve("[Graph1]").is_ok());
    let y = state
        .snapshot()
        .tree
        .resolve("[Résultats]Calculation!Y")
        .unwrap();
    assert_eq!(
        state
            .snapshot()
            .tree
            .node(y)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_slice(),
        &[0., 0.25, 1., 2.25, 4.]
    );
    let after = state.snapshot().tree.clone();
    state.dispatch(Action::Undo);
    assert_eq!(*state.snapshot().tree, *before);
    state.dispatch(Action::Redo);
    assert_eq!(*state.snapshot().tree, *after);
}

/// #315: Save is refused during a script (nothing is written, a message says
/// so) and abandons the pending gesture; once the script is done, a Save does
/// not replay that gesture.
#[test]
fn save_is_refused_during_a_script() {
    let (_session, mut state, _sheet) = start();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("during.stf");
    state.set_path(file.display().to_string());
    state.dispatch(Action::AddWorkbook);
    state.set_script_source("import time\ntime.sleep(0.5)\n".to_string());
    state.dispatch(Action::RunScript);
    assert!(state.script_running());

    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(!file.exists(), "nothing is written during the script");
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert!(!state.take_quit_ready());

    let deadline = Instant::now() + TIMEOUT;
    while state.script_running() {
        assert!(Instant::now() <= deadline);
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }
    state.dispatch(Action::Save);
    assert!(
        file.is_file(),
        "witness: Save writes once the script is done"
    );
    assert!(
        !state.take_quit_ready(),
        "the pending gesture was abandoned"
    );
}

// --- Saving from a script (#331) ---------------------------------------------

/// `pyst.save_as` saves the project: the window title does not stay
/// "modified" (`saved_revision` follows), and the application adopts the path.
#[test]
fn a_script_save_clears_the_modified_mark_and_adopts_the_path() {
    let (_session, mut state, _sheet) = start();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("by_script.stf");
    run_and_wait(
        &mut state,
        &format!(
            "a = pyst.resolve(\"[Book1]Sheet1!Time\")\n\
             pyst.set_values(a, [1.0, 2.0])\n\
             pyst.save_as({file:?})\n"
        ),
    );
    assert!(file.exists());
    assert_eq!(state.path(), file.to_str().unwrap());
    assert!(
        !state.window_title().contains('•'),
        "title: {}",
        state.window_title()
    );
    // Witness: a write without saving does mark the project modified.
    run_and_wait(
        &mut state,
        "a = pyst.resolve(\"[Book1]Sheet1!Time\")\npyst.set_values(a, [5.0])\n",
    );
    assert!(state.window_title().contains('•'));
}

/// Guard #309: `pyst.save()` refuses to overwrite the source of a read that
/// discarded data; `save_as` stays allowed.
#[test]
fn a_script_save_refuses_to_overwrite_a_source_that_lost_data() {
    let (_session, mut state, _sheet) = start();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.stf");
    std::fs::write(&source, b"original").unwrap();
    state.set_path(source.to_str().unwrap().to_string());
    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_calculations: vec![stokesforge_session::DiscardedCalculation {
            id: 1,
            kind: None,
        }],
        discarded_corrections: Vec::new(),
    });
    run_and_wait(
        &mut state,
        "try:\n    pyst.save()\nexcept pyst.InvalidValueError as e:\n    print('REFUSED', e)\n",
    );
    let output = state.script_output().unwrap();
    assert!(output.stdout.contains("REFUSED"), "{:?}", output);
    assert!(output.stdout.contains("save_as"));
    assert_eq!(std::fs::read(&source).unwrap(), b"original");

    let elsewhere = dir.path().join("elsewhere.stf");
    run_and_wait(
        &mut state,
        &format!("pyst.save_as({elsewhere:?})\npyst.save()\n"),
    );
    assert!(state.script_output().unwrap().error.is_none());
    assert!(elsewhere.exists());
    assert_eq!(std::fs::read(&source).unwrap(), b"original");
}

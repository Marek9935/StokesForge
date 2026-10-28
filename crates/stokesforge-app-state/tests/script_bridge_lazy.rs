//! Criterion 3/8: the interpreter starts **only at the first script**, never
//! before — not at the construction of `AppState`, not when the Python
//! descriptor is set, not when a window is opened.
//!
//! # Why this test lives alone in its own file
//!
//! `EmbeddedPython::is_initialized()` reads a **process-global** state
//! (`static INIT: OnceLock<...>` in `stokesforge-python::interpreter`), and
//! Cargo runs the tests of a single binary in parallel by default, on threads
//! of the same process. Any other test of the same binary that runs a script
//! would already have started Python before this one, in an unguaranteed
//! order — the "false before" assertion could then never be honest. A file
//! under `tests/` is its **own binary** (hence its own process): this one
//! contains a single test, which is therefore the only one able to touch
//! Python in its process. That is what makes "false before, true after"
//! verifiable without depending on execution order.
//!
//! The other script criteria (execution, output, refresh, undo, exception)
//! are in `tests/script_panel.rs`, where the order of interpreter
//! initialization does not matter.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{Column, Project};
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

#[test]
fn the_bridge_starts_only_at_the_first_script() {
    assert!(
        !EmbeddedPython::is_initialized(),
        "this test must be alone in its binary for this to be honest \
         (see the top-level doc of this file)"
    );

    let mut project = Project::new("Demo").expect("demo project");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 readable")
        .expect("Sheet1 created with the workbook");
    project
        .add_column(sheet, None, Some("Time"), Column::from_f64(&[1.0]))
        .expect("Time column");

    let session = Session::spawn(project, None).expect("session thread");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        "script-test.stf".to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .expect("initial application state");

    // Building the state must not touch Python: this is the heart of the
    // criterion "window and sheet open without an interpreter".
    assert!(
        !EmbeddedPython::is_initialized(),
        "AppState::new does not start the interpreter"
    );

    state.set_python(EmbeddedPython::with_home(home()));
    assert!(
        !EmbeddedPython::is_initialized(),
        "setting the descriptor starts nothing either: locate/with_home \
         do not touch CPython"
    );

    state.set_script_source("print('hello')".to_string());
    state.dispatch(Action::RunScript);

    let deadline = Instant::now() + Duration::from_secs(30);
    while state.script_running() {
        assert!(
            Instant::now() <= deadline,
            "the script did not finish within 30 s"
        );
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }

    assert!(
        EmbeddedPython::is_initialized(),
        "the first script must have started the interpreter"
    );
    let output = state.script_output().expect("a recorded output");
    assert!(output.error.is_none(), "script failed: {:?}", output.error);
    assert!(
        output.stdout.contains("hello"),
        "output: {:?}",
        output.stdout
    );
}

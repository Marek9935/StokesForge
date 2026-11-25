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

// --- The end of a script wakes the server thread (#475) ---------------------

/// The script's thread calls the waker after sending its reply: the server
/// thread learns of the end at once, with no 50 ms look in between. When the
/// waker fires the result is already there.
#[test]
fn the_end_of_a_script_calls_the_waker_after_the_reply_is_sent() {
    let (_session, mut state, _sheet) = start();
    let (woken_tx, woken) = std::sync::mpsc::channel();
    state.set_waker(std::sync::Arc::new(move || {
        let _ = woken_tx.send(());
    }));
    state.set_script_source("print(1)\n".to_string());
    state.dispatch(Action::RunScript);
    woken
        .recv_timeout(TIMEOUT)
        .expect("the end of the script woke the server thread");
    state.poll_script();
    assert!(
        !state.script_running(),
        "the reply was sent before the wake"
    );
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

    // #465: the refusal leaves an entry being typed open (nothing committed).
    let sheet = state.tree().resolve("[Book1]Sheet1").expect("demo sheet");
    let column = state.visible_columns(sheet)[0].0;
    state.begin_edit(column, 0);
    state.dispatch(Action::Save);
    assert!(state.editing().is_some(), "the entry stays open");
    state.cancel_edit();

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

// --- Graph themes from a script (#546) -----------------------------------------

/// The graph named `name` as the script made it.
fn graph_named(state: &AppState, name: &str) -> stokesforge_plot::PlotSpec {
    let id = state
        .graphs()
        .into_iter()
        .find(|(_, graph_name)| graph_name == name)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("graph {name}"));
    state.plot_spec(id).expect("spec")
}

/// A user theme "Poster" with a dark background, from a new graph.
fn save_poster(state: &mut AppState) {
    state.dispatch(Action::AddGraph);
    let graph = state.graphs().last().expect("a graph").0;
    let mut spec = state.plot_spec(graph).expect("spec");
    spec.background = "#102030".into();
    state.dispatch(Action::SetPlotSpec { graph, spec });
    state.add_user_theme("Poster", graph).expect("saved");
}

/// The default theme of new graphs never reaches a graph made by `pyst.plot`:
/// the script keeps control of its style.
#[test]
fn a_default_theme_does_not_touch_a_graph_made_by_a_script() {
    let dir = tempfile::tempdir().unwrap();
    let session = Session::spawn(demo_project(), None).expect("session thread");
    // Preferences with a file: the default theme is saved there.
    let mut state = AppState::new_with_preferences(
        session.handle(),
        "script-test.stf".to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.path().join("preferences.json"),
        )),
    )
    .expect("initial application state");
    state.set_python(EmbeddedPython::with_home(home()));
    state.use_user_themes_file(dir.path().join("themes.json"));
    let plot = |name: &str| {
        format!("t = pyst.resolve('[Book1]Sheet1!Time')\npyst.plot(t, name='{name}')\n")
    };
    run_and_wait(&mut state, &plot("Before"));
    state
        .set_new_graph_theme(Some(stokesforge_app_state::user_themes::ThemeRef::Builtin(
            2,
        )))
        .unwrap();
    run_and_wait(&mut state, &plot("After"));
    assert_eq!(
        graph_named(&state, "After"),
        graph_named(&state, "Before"),
        "a script graph takes no default theme"
    );
}

/// The engine is reused: a user theme created after a first run is seen by
/// the next one, a built-in answers to its French name whatever the interface
/// language, and a modified built-in applies modified.
#[test]
fn the_themes_of_the_application_are_injected_before_every_run() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, _sheet) = start();
    state.use_user_themes_file(dir.path().join("themes.json"));
    run_and_wait(
        &mut state,
        "t = pyst.resolve('[Book1]Sheet1!Time')\ng = pyst.plot(t, name='Scripted')\n\
         print([x['name'] for x in pyst.themes() if x['source'] == 'user'])\n",
    );
    assert_eq!(state.script_output().unwrap().stdout.trim(), "[]");
    save_poster(&mut state);
    // Second run, same engine: the new theme is there.
    run_and_wait(
        &mut state,
        "pyst.apply_theme(g, 'poster')\n\
         print([x['name'] for x in pyst.themes() if x['source'] == 'user'])\n",
    );
    let output = state.script_output().unwrap();
    assert!(output.error.is_none(), "{:?}", output.error);
    assert_eq!(output.stdout.trim(), "['Poster']");
    assert_eq!(graph_named(&state, "Scripted").background, "#102030");
    // A French name works whatever the language of the interface.
    run_and_wait(
        &mut state,
        "pyst.apply_theme(g, 'Publication large')\npyst.apply_theme(g, 'Clair')\n",
    );
    let output = state.script_output().unwrap();
    assert!(output.error.is_none(), "{:?}", output.error);
    // A built-in the user modified applies modified (and `themes()` says so).
    let modified = graph_named(&state, "Scripted");
    let mut spec = modified.clone();
    spec.background = "#abcdef".into();
    let graph = state
        .graphs()
        .into_iter()
        .find(|(_, name)| name == "Scripted")
        .unwrap()
        .0;
    state.dispatch(Action::SetPlotSpec { graph, spec });
    state.edit_builtin_theme(0, graph).unwrap();
    run_and_wait(
        &mut state,
        "pyst.apply_theme(g, 'light')\n\
         print([x['modified'] for x in pyst.themes() if x['key'] == 'builtin:light'])\n",
    );
    let output = state.script_output().unwrap();
    assert!(output.error.is_none(), "{:?}", output.error);
    assert_eq!(output.stdout.trim(), "[True]");
    assert_eq!(graph_named(&state, "Scripted").background, "#abcdef");
}

/// A family answers to the name the Apply Theme window shows, in French as
/// well as in English, whatever the interface language (#551).
#[test]
fn a_family_is_named_in_every_catalogue_language_in_the_application() {
    let (_session, mut state, _sheet) = start();
    run_and_wait(
        &mut state,
        "t = pyst.resolve('[Book1]Sheet1!Time')\n\
         a = pyst.plot(t, name='ByName')\nb = pyst.plot(t, name='ByKey')\n\
         c = pyst.plot(t, name='Untouched')\n\
         pyst.apply_theme(a, 'publication', families=['Courbes', 'Taille'])\n\
         pyst.apply_theme(b, 'publication', families=['curves', 'size-and-page'])\n",
    );
    let output = state.script_output().unwrap();
    assert!(output.error.is_none(), "{:?}", output.error);
    let (by_name, by_key, untouched) = (
        graph_named(&state, "ByName"),
        graph_named(&state, "ByKey"),
        graph_named(&state, "Untouched"),
    );
    assert_ne!(by_name.panels[0].series, untouched.panels[0].series);
    assert_eq!(by_name.panels[0].series, by_key.panels[0].series);
    assert_eq!(by_name.frame_widths, by_key.frame_widths);
}

// --- Running a script file (#552) -------------------------------------------

/// Runs `path` as a file and waits for the end, like [`run_and_wait`].
fn run_file_and_wait(state: &mut AppState, action: Action) {
    state.dispatch(action);
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

fn run_file(path: &std::path::Path) -> Action {
    Action::RunScriptFile {
        path: path.to_path_buf(),
    }
}

/// A script and its sibling module in a temporary folder.
/// Starts a console script that runs until Stop, and returns once it is really
/// running.
fn start_spinning(state: &mut AppState, dir: &std::path::Path) {
    let marker = dir.join("spinning.marker");
    state.set_script_source(format!(
        "open({:?}, 'w').close()\nwhile True:\n    pass\n",
        marker.display().to_string()
    ));
    state.dispatch(Action::RunScript);
    let deadline = Instant::now() + TIMEOUT;
    while !marker.exists() {
        assert!(Instant::now() <= deadline, "the script did not start");
        thread::sleep(Duration::from_millis(5));
    }
}

fn script_with_sibling(dir: &std::path::Path) -> PathBuf {
    std::fs::write(dir.join("helpers.py"), "VALUE = 42.0\n").unwrap();
    let main = dir.join("main.py");
    std::fs::write(
        &main,
        "import helpers\n\
         a = pyst.resolve(\"[Book1]Sheet1!Time\")\n\
         pyst.set_column(a, pyst.column(a) * 0.0 + helpers.VALUE)\n\
         print(\"from file\")\n",
    )
    .unwrap();
    main
}

/// A file run shows its output, refreshes the snapshot, imports a sibling
/// module, is one undo step and leaves the console text alone. Witness: run
/// the file as text (the sibling import then fails).
#[test]
fn a_script_file_runs_with_its_sibling_module_in_one_undo_step() {
    let dir = tempfile::tempdir().unwrap();
    let main = script_with_sibling(dir.path());
    let (_session, mut state, sheet) = start();
    let time = column_id(&state, sheet, "Time");
    state.set_script_source("keep = 1\n".to_string());

    run_file_and_wait(&mut state, run_file(&main));

    let output = state.script_output().expect("an output");
    assert!(output.error.is_none(), "failed: {:?}", output.error);
    assert!(output.stdout.contains("from file"), "{:?}", output.stdout);
    assert_eq!(state.cell_text(time, 0), "42", "the snapshot was refreshed");
    assert_eq!(
        state.script_source(),
        "keep = 1\n",
        "console text untouched"
    );
    assert_eq!(state.last_script_file(), Some(main.as_path()));
    assert!(state.take_script_reveal(), "the console is asked to show");
    assert!(!state.take_script_reveal(), "once");

    state.dispatch(Action::Undo);
    assert_eq!(state.cell_text(time, 0), "0", "one Ctrl+Z undoes the run");
}

/// A file that has gone: the red message of the read error, no new revision,
/// the file stays the one of Run Again.
#[test]
fn a_missing_script_file_is_a_red_message_and_no_revision() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, _sheet) = start();
    let before = state.snapshot().revision;

    run_file_and_wait(&mut state, run_file(&dir.path().join("gone.py")));

    let status = state.status().expect("a message");
    assert!(status.is_error());
    assert!(status.text.contains("gone.py"), "{}", status.text);
    assert_eq!(state.snapshot().revision, before);
    assert!(!state.can_undo());
    assert!(state.script_output().is_none());
}

/// A run in progress refuses a second one, the file is not remembered, and
/// Stop ends the first. Witness: drop the running test and both would start.
#[test]
fn a_script_file_is_refused_while_a_script_runs() {
    let dir = tempfile::tempdir().unwrap();
    let main = script_with_sibling(dir.path());
    let (_session, mut state, _sheet) = start();
    start_spinning(&mut state, dir.path());
    assert!(state.script_running());

    state.dispatch(run_file(&main));
    assert_eq!(
        state.status().expect("message").text,
        stokesforge_app_state::tr!("error-script-running")
    );
    assert!(
        state.last_script_file().is_none(),
        "the refused file is not kept"
    );
    assert!(!state.take_script_reveal());
    assert!(state.script_blocked_reason().is_some());

    state.dispatch(Action::InterruptScript);
    let deadline = Instant::now() + TIMEOUT;
    while state.script_running() {
        assert!(Instant::now() <= deadline, "Stop did not end the run");
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }
}

/// Run Again reruns the edited file; with no file yet it runs nothing.
#[test]
fn run_again_runs_the_edited_file_and_does_nothing_before_a_first_run() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, _sheet) = start();
    state.dispatch(Action::RunScriptAgain);
    assert!(!state.script_running());
    assert!(state.script_output().is_none());

    let file = dir.path().join("again.py");
    std::fs::write(&file, "print('one')\n").unwrap();
    run_file_and_wait(&mut state, run_file(&file));
    assert!(state.script_output().unwrap().stdout.contains("one"));

    std::fs::write(&file, "print('two')\n").unwrap();
    run_file_and_wait(&mut state, Action::RunScriptAgain);
    let stdout = &state.script_output().unwrap().stdout;
    assert!(
        stdout.contains("two") && !stdout.contains("one"),
        "{stdout:?}"
    );
}

/// Opening a project never runs a script nor remembers one. Witness: make the
/// open path call `RunScriptFile` and the output appears.
#[test]
fn opening_a_project_runs_and_remembers_no_script() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.py"), "print('boom')\n").unwrap();
    let (_session, mut state, _sheet) = start();
    let project = dir.path().join("p.stf");
    state.dispatch(Action::SaveAs {
        path: project.display().to_string(),
    });
    state.dispatch(Action::OpenProject {
        path: project.display().to_string(),
        pending_edits: false,
    });
    assert!(!state.script_running());
    assert!(state.script_output().is_none());
    assert!(state.last_script_file().is_none());
    assert!(!state.take_script_reveal());
}

/// The picker starts in the folder of the last file run (not the Import one).
#[test]
fn the_script_picker_starts_in_the_folder_of_the_last_file_run() {
    use stokesforge_app_state::file_picker::PickRequest;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.py");
    std::fs::write(&file, "pass\n").unwrap();
    let (_session, mut state, _sheet) = start();
    run_file_and_wait(&mut state, run_file(&file));
    assert_eq!(
        state
            .folder_candidates(&PickRequest::RunScriptFile)
            .session_last,
        Some(dir.path().to_path_buf())
    );
}

fn drop_script(state: &mut AppState, paths: Vec<PathBuf>) {
    state.dispatch(Action::DropFiles {
        paths,
        target_book: None,
    });
}

/// A dropped script asks first; Cancel runs nothing, Run runs it once.
/// Witness: run in the drop handler and the Cancel case runs it.
#[test]
fn a_dropped_script_asks_then_cancel_runs_nothing_and_run_runs_once() {
    let dir = tempfile::tempdir().unwrap();
    let main = script_with_sibling(dir.path());
    let (_session, mut state, sheet) = start();
    let time = column_id(&state, sheet, "Time");

    drop_script(&mut state, vec![main.clone()]);
    assert_eq!(state.pending_script_drop(), Some(main.as_path()));
    assert!(state.blocking_dialog());
    assert!(!state.script_running(), "nothing runs before the answer");

    state.dispatch(Action::ConfirmScriptDrop(false));
    assert!(state.pending_script_drop().is_none());
    assert!(!state.script_running());
    assert!(state.script_output().is_none());
    assert!(state.last_script_file().is_none());
    assert_eq!(state.cell_text(time, 0), "0");

    drop_script(
        &mut state,
        vec![PathBuf::from(main.to_string_lossy().to_uppercase())],
    );
    state.dispatch(Action::ConfirmScriptDrop(false));
    drop_script(&mut state, vec![main.clone()]);
    run_file_and_wait(&mut state, Action::ConfirmScriptDrop(true));
    assert_eq!(state.cell_text(time, 0), "42");
    assert_eq!(state.last_script_file(), Some(main.as_path()));
    assert!(state.pending_script_drop().is_none());
    state.dispatch(Action::Undo);
    assert_eq!(state.cell_text(time, 0), "0", "ran once: one undo step");
}

/// Several scripts in one drop: one message, no dialog, nothing runs; the other
/// files follow their usual path (a `.toml` still becomes Script Settings).
#[test]
fn several_dropped_scripts_are_refused_with_one_message() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, _sheet) = start();
    drop_script(
        &mut state,
        vec![dir.path().join("a.py"), dir.path().join("b.pyst")],
    );
    assert!(state.pending_script_drop().is_none());
    let status = state.status().expect("message");
    assert!(status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-script-drop-several")
    );
}

/// No dialog while a script runs, while the import window is open or when
/// Graph Properties is open (the drop is refused for every file).
#[test]
fn a_dropped_script_asks_nothing_while_something_else_holds_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let main = script_with_sibling(dir.path());
    let (_session, mut state, _sheet) = start();

    state.open_import(None::<NodeId>);
    drop_script(&mut state, vec![main.clone()]);
    assert!(state.pending_script_drop().is_none());
    // An open import window takes the failure into its own footer.
    assert_eq!(
        state.dialog_error(),
        Some(stokesforge_app_state::tr!("error-script-drop-import-open").as_str())
    );
    state.cancel_import();

    start_spinning(&mut state, dir.path());
    drop_script(&mut state, vec![main]);
    assert!(state.pending_script_drop().is_none());
    assert_eq!(
        state.status().expect("message").text,
        stokesforge_app_state::tr!("error-script-running")
    );
    state.dispatch(Action::InterruptScript);
    let deadline = Instant::now() + TIMEOUT;
    while state.script_running() {
        assert!(Instant::now() <= deadline);
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }
}

fn wait_for_end(state: &mut AppState) {
    let deadline = Instant::now() + TIMEOUT;
    while state.script_running() {
        assert!(Instant::now() <= deadline, "Stop did not end the run");
        thread::sleep(Duration::from_millis(1));
        state.poll_script();
    }
}

// --- Stop during the very first run of a session (A2 of #552) --------------

/// A fresh state, an endless console script as the very first run: Stop ends it
/// as interrupted. Witness: store the engine only when the run ends (the old
/// assignment): Stop finds no engine and the run never ends.
#[test]
fn stop_ends_the_very_first_console_run_of_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, _sheet) = start();
    start_spinning(&mut state, dir.path());

    state.dispatch(Action::InterruptScript);
    wait_for_end(&mut state);
    assert!(state.script_output().unwrap().interrupted);
}

/// Same, with a file as the very first run.
#[test]
fn stop_ends_the_very_first_file_run_of_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("spinning.marker");
    let file = dir.path().join("spin.py");
    std::fs::write(
        &file,
        format!(
            "open({:?}, 'w').close()\nwhile True:\n    pass\n",
            marker.display().to_string()
        ),
    )
    .unwrap();
    let (_session, mut state, _sheet) = start();
    state.dispatch(run_file(&file));
    let deadline = Instant::now() + TIMEOUT;
    while !marker.exists() {
        assert!(Instant::now() <= deadline, "the script did not start");
        thread::sleep(Duration::from_millis(5));
    }

    state.dispatch(Action::InterruptScript);
    wait_for_end(&mut state);
    assert!(state.script_output().unwrap().interrupted);
}

/// A Stop clicked right after Run, while Python is still starting and the
/// script has not begun, is kept and takes effect as soon as it runs; the next
/// run is not affected by it.
#[test]
fn a_stop_before_the_engine_is_ready_takes_effect_and_does_not_leak() {
    let (_session, mut state, _sheet) = start();
    state.set_script_source("while True:\n    pass\n".to_string());
    state.dispatch(Action::RunScript);
    state.dispatch(Action::InterruptScript);
    wait_for_end(&mut state);
    assert!(state.script_output().unwrap().interrupted);

    run_and_wait(&mut state, "print('next')\n");
    let output = state.script_output().unwrap();
    assert!(!output.interrupted, "the earlier Stop leaked: {output:?}");
    assert!(output.stdout.contains("next"), "{:?}", output.stdout);
}

/// A Stop with nothing running is not remembered for the next run.
#[test]
fn a_stop_with_nothing_running_does_not_leak_into_the_next_run() {
    let (_session, mut state, _sheet) = start();
    state.dispatch(Action::InterruptScript);
    run_and_wait(&mut state, "print('ok')\n");
    let output = state.script_output().unwrap();
    assert!(!output.interrupted && output.error.is_none(), "{output:?}");
}

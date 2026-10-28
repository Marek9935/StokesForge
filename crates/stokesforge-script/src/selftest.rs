//! Replays, as a library, the **completion criterion** of the whole foundation:
//!
//! > from a packaged binary on Linux: an edited cell is undone by
//! > `Ctrl+Z` and redone by `Ctrl+Y`; `Ctrl+S` writes a file that
//! > `Ctrl+O` reopens identically; and a Python script reads a column
//! > without copying, writes another through the bus, and `Ctrl+Z` undoes what the
//! > script just wrote.
//!
//! All the logic lives here, not in `src/bin/stokesforge-selftest.rs`: this
//! is what lets [`tests/selftest.rs`](../../tests/selftest.rs) exercise
//! exactly the same path as a user who runs the binary from the
//! packaged distribution, under the usual `cargo test` harness.
//!
//! [`run_selftest`] builds the demo project itself (`Book1` /
//! `Sheet1` / columns `A` and `B`), runs the actor and the script thread
//! for the time of the check, then stops both cleanly. It never
//! panics: every failure, including the startup of the
//! interpreter itself, becomes a failed [`SelfTestStep`], never an
//! interruption of the program.

use std::path::PathBuf;

use stokesforge_commands::Command;
use stokesforge_core::{Column, CoreResult, NodeId, Project, Tree, ValueBlock};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_session::{Session, SessionHandle};

use crate::engine::ScriptEngine;

/// Names of the seven steps, in the order [`run_selftest`] replays them. Serves
/// both to name a passed step and to fill the steps left
/// **unverified** when a failure upstream prevents reaching them.
const STEP_NAMES: [&str; 7] = [
    "Embedded, isolated interpreter",
    "Edit, undo, redo",
    "Disk round trip",
    "Zero-copy read from a script",
    "Write through the bus, undone from the host",
    "Undo from Python",
    "Output and errors of a script",
];

/// A step of the check, and its verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfTestStep {
    /// Name of the step, one of [`STEP_NAMES`].
    pub name: String,
    /// True if the step passed.
    pub passed: bool,
    /// Readable explanation of the verdict: what was checked if the step
    /// passes, what was observed instead of what was expected otherwise.
    pub detail: String,
}

impl SelfTestStep {
    /// Passed step.
    fn ok(name: &str, detail: String) -> Self {
        Self {
            name: name.to_owned(),
            passed: true,
            detail,
        }
    }

    /// Failed step.
    fn fail(name: &str, detail: String) -> Self {
        Self {
            name: name.to_owned(),
            passed: false,
            detail,
        }
    }
}

/// The full report of a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfTestReport {
    /// The steps replayed, in the order of [`STEP_NAMES`].
    pub steps: Vec<SelfTestStep>,
    /// What was checked: embedded prefix, versions, isolation,
    /// number of `sys.path` entries, current executable. One `(label, value)`
    /// pair per line, in the order in which it was established.
    pub environment: Vec<(String, String)>,
}

impl SelfTestReport {
    /// True if all the steps passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.steps.iter().all(|step| step.passed)
    }

    /// Text rendering, in English, one line per step, prefixed with a
    /// success marker (`[OK]`) or failure marker (`[FAILED]`).
    #[must_use]
    pub fn render(&self) -> String {
        self.steps
            .iter()
            .map(|step| {
                let marker = if step.passed { "[OK]   " } else { "[FAILED]" };
                format!("{marker} {} — {}", step.name, step.detail)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Replays the foundation's completion criterion end to end. Never panics:
/// a step that fails is a failed [`SelfTestStep`], not an
/// interruption of the program.
///
/// `python` says where to find the embedded interpreter, typically
/// [`EmbeddedPython::locate`] from the binary, or
/// [`EmbeddedPython::with_home`] pointed at the vendored prefix from a
/// test.
#[must_use]
pub fn run_selftest(python: EmbeddedPython) -> SelfTestReport {
    // The two pieces of information that depend on no step: always
    // present, even if everything else fails before it could run.
    let mut environment = vec![
        (
            "embedded Python prefix".to_owned(),
            python.home().display().to_string(),
        ),
        (
            "current executable".to_owned(),
            match std::env::current_exe() {
                Ok(path) => path.display().to_string(),
                Err(error) => format!("unavailable: {error}"),
            },
        ),
    ];
    let mut steps: Vec<SelfTestStep> = Vec::new();

    let (project, column_a, column_b) = match build_demo_project() {
        Ok(value) => value,
        Err(error) => {
            return abort(
                steps,
                environment,
                format!("preparing the demo project failed: {error}"),
            );
        }
    };

    let actor = match Session::spawn(project, None) {
        Ok(actor) => actor,
        Err(error) => {
            return abort(
                steps,
                environment,
                format!("the session actor could not start: {error}"),
            );
        }
    };
    let session = actor.handle();

    let engine = match ScriptEngine::spawn(session.clone(), python.clone()) {
        Ok(engine) => engine,
        Err(error) => {
            let _ = actor.shutdown();
            return abort(
                steps,
                environment,
                format!("the script engine could not start: {error}"),
            );
        }
    };

    steps.push(step_interpreter(&python, &mut environment));
    steps.push(step_edit_undo_redo(&session, column_a));
    steps.push(step_disk_roundtrip(&session, column_b));
    steps.push(step_zero_copy_read(&engine, &session, column_a));
    steps.push(step_bus_write_host_undo(&engine, &session, column_b));
    steps.push(step_undo_from_python(&engine, &session, column_b));
    steps.push(step_output_and_errors(&engine));

    // Best-effort stop: a stop that goes wrong does not call into question what
    // the steps above have already, positively, verified.
    let _ = engine.shutdown();
    let _ = actor.shutdown();

    SelfTestReport { steps, environment }
}

/// Builds the demo project: `Book1`/`Sheet1`, columns
/// `A = [1.0, 2.0, 3.0]` and `B = [10.0, 20.0, 30.0]`.
fn build_demo_project() -> CoreResult<(Project, NodeId, NodeId)> {
    let mut project = Project::new("StokesForgeSelfTest")?;
    let root = project.root();
    project.add_workbook(root, Some("Book1"))?;
    let sheet = project.resolve_path("[Book1]Sheet1")?;
    let column_a =
        project.add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))?;
    let column_b = project.add_column(
        sheet,
        None,
        Some("B"),
        Column::from_f64(&[10.0, 20.0, 30.0]),
    )?;
    Ok((project, column_a, column_b))
}

/// Completes the report when a failure occurring **before** the
/// first step prevents any check: pushes a failed step
/// for the interpreter (nothing could be checked without it) and marks the other six
/// as not reached, never leaving the report
/// incomplete.
fn abort(
    mut steps: Vec<SelfTestStep>,
    environment: Vec<(String, String)>,
    detail: String,
) -> SelfTestReport {
    steps.push(SelfTestStep::fail(STEP_NAMES[0], detail));
    for name in &STEP_NAMES[1..] {
        steps.push(SelfTestStep::fail(
            name,
            "not reached: a failure upstream prevented any check".to_owned(),
        ));
    }
    SelfTestReport { steps, environment }
}

/// Step 1: interpreter and numpy version, isolation, and
/// **no** `sys.path` entry outside the embedded prefix. Also feeds
/// `environment` with these four pieces of information.
fn step_interpreter(
    python: &EmbeddedPython,
    environment: &mut Vec<(String, String)>,
) -> SelfTestStep {
    let name = STEP_NAMES[0];
    let mut problems: Vec<String> = Vec::new();

    match interpreter::python_version() {
        Ok(version) => environment.push(("version de Python".to_owned(), version)),
        Err(error) => {
            environment.push((
                "version de Python".to_owned(),
                format!("unavailable: {error}"),
            ));
            problems.push(format!("version de Python indisponible : {error}"));
        }
    }

    match interpreter::numpy_version() {
        Ok(version) => environment.push(("version de numpy".to_owned(), version)),
        Err(error) => {
            environment.push((
                "version de numpy".to_owned(),
                format!("unavailable: {error}"),
            ));
            problems.push(format!("version de numpy indisponible : {error}"));
        }
    }

    match interpreter::sys_flags_isolated() {
        Ok(true) => environment.push(("sys.flags.isolated".to_owned(), "true".to_owned())),
        Ok(false) => {
            environment.push(("sys.flags.isolated".to_owned(), "false".to_owned()));
            problems
                .push("sys.flags.isolated is false: the interpreter is not isolated".to_owned());
        }
        Err(error) => {
            environment.push((
                "sys.flags.isolated".to_owned(),
                format!("unavailable: {error}"),
            ));
            problems.push(format!("sys.flags.isolated unavailable: {error}"));
        }
    }

    match interpreter::sys_path() {
        Ok(paths) => {
            environment.push(("sys.path entries".to_owned(), paths.len().to_string()));
            let strays: Vec<&str> = paths
                .iter()
                .filter(|entry| !python.contains_path(entry))
                .map(String::as_str)
                .collect();
            if !strays.is_empty() {
                problems.push(format!(
                    "sys.path holds {} entry(ies) outside the embedded prefix: {}",
                    strays.len(),
                    strays.join(", ")
                ));
            }
        }
        Err(error) => {
            environment.push((
                "sys.path entries".to_owned(),
                format!("unavailable: {error}"),
            ));
            problems.push(format!("sys.path unavailable: {error}"));
        }
    }

    if problems.is_empty() {
        SelfTestStep::ok(
            name,
            "version, isolation and exhaustive sys.path (no entry outside the embedded prefix), \
             all checked"
                .to_owned(),
        )
    } else {
        SelfTestStep::fail(name, problems.join(" ; "))
    }
}

/// Value of the first row of a numeric column, or `None` if the
/// column is absent or not numeric.
fn first_float(tree: &Tree, column: NodeId) -> Option<f64> {
    tree.node(column)?
        .as_column()?
        .float_values()?
        .first()
        .copied()
}

/// Current values of a numeric column, read on the Rust side through a
/// fresh snapshot.
fn column_values(session: &SessionHandle, column: NodeId) -> Result<Vec<f64>, String> {
    let snapshot = session
        .snapshot()
        .map_err(|error| format!("snapshot failed: {error}"))?;
    let node = snapshot
        .tree
        .node(column)
        .ok_or_else(|| format!("{column} no longer exists in the project"))?;
    node.as_column()
        .and_then(|col| col.float_values())
        .map(|values| values.as_slice().to_vec())
        .ok_or_else(|| format!("{column} is not a numeric column"))
}

/// Step 2: a write through the bus is undone and redone, bit for bit.
///
/// Ends with a second undo that brings the tree back to the pristine state
/// from before this step: the following steps assume `A = [1.0, 2.0, 3.0]`.
fn step_edit_undo_redo(session: &SessionHandle, column_a: NodeId) -> SelfTestStep {
    let name = STEP_NAMES[1];

    let before = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => return SelfTestStep::fail(name, format!("initial snapshot failed: {error}")),
    };

    if let Err(error) = session.execute(Command::SetCells {
        column: column_a,
        start: 0,
        values: ValueBlock::Float(vec![99.0]),
        expected_version: None,
    }) {
        return SelfTestStep::fail(name, format!("write through the bus refused: {error}"));
    }

    let after_edit = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("snapshot after edit failed: {error}"))
        }
    };
    if first_float(&after_edit, column_a) != Some(99.0) {
        return SelfTestStep::fail(
            name,
            format!(
                "written value missing: expected 99.0, got {:?}",
                first_float(&after_edit, column_a)
            ),
        );
    }

    match session.undo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(name, "undo had no effect: nothing to undo".to_owned())
        }
        Err(error) => return SelfTestStep::fail(name, format!("undo refused: {error}")),
    }
    let after_undo = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("snapshot after undo failed: {error}"))
        }
    };
    if after_undo != before {
        return SelfTestStep::fail(
            name,
            "after undo, the tree differs bit for bit from the state before the edit".to_owned(),
        );
    }

    match session.redo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(name, "redo had no effect: nothing to redo".to_owned())
        }
        Err(error) => return SelfTestStep::fail(name, format!("redo refused: {error}")),
    }
    let after_redo = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("snapshot after redo failed: {error}"))
        }
    };
    if after_redo != after_edit {
        return SelfTestStep::fail(
            name,
            "after redo, the tree differs bit for bit from the state after the edit".to_owned(),
        );
    }

    // Cleanup: the following steps assume A pristine.
    match session.undo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(
                name,
                "cleanup undo had no effect after the check".to_owned(),
            )
        }
        Err(error) => return SelfTestStep::fail(name, format!("cleanup undo refused: {error}")),
    }
    let restored = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => return SelfTestStep::fail(name, format!("final snapshot failed: {error}")),
    };
    if restored != before {
        return SelfTestStep::fail(
            name,
            "the pristine state could not be restored after the check".to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        "write through the bus, undo and redo conform bit for bit".to_owned(),
    )
}

/// Cleanup guard: removes the temporary file at the end of step 3,
/// whether it succeeds or fails.
struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Step 3: disk round trip, bit-for-bit equal to what was saved,
/// including after an intermediate change, which the reload must
/// overwrite entirely.
fn step_disk_roundtrip(session: &SessionHandle, column_b: NodeId) -> SelfTestStep {
    let name = STEP_NAMES[2];

    let before = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => return SelfTestStep::fail(name, format!("initial snapshot failed: {error}")),
    };

    let path =
        std::env::temp_dir().join(format!("stokesforge-selftest-{}.stf", std::process::id()));
    let _guard = TempFileGuard(path.clone());

    if let Err(error) = session.save(&path) {
        return SelfTestStep::fail(name, format!("save refused: {error}"));
    }

    // Changes something other than what was just saved: the reread
    // must overwrite everything, including this intermediate change.
    if let Err(error) = session.execute(Command::SetCells {
        column: column_b,
        start: 0,
        values: ValueBlock::Float(vec![-1.0]),
        expected_version: None,
    }) {
        return SelfTestStep::fail(name, format!("intermediate change refused: {error}"));
    }

    if let Err(error) = session.load(&path) {
        return SelfTestStep::fail(name, format!("reload refused: {error}"));
    }

    let after = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("snapshot after reload failed: {error}"))
        }
    };

    if after != before {
        return SelfTestStep::fail(
            name,
            "the reloaded tree differs bit for bit from the saved one".to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        format!(
            "disk round trip conforms bit for bit, including after a change overwritten by \
             the reload ({})",
            path.display()
        ),
    )
}

/// Step 4: the numpy view taken by a script points **exactly** at the
/// column's Rust buffer, not a copy with the right values.
fn step_zero_copy_read(
    engine: &ScriptEngine,
    session: &SessionHandle,
    column_a: NodeId,
) -> SelfTestStep {
    let name = STEP_NAMES[3];

    let rust_address = match session.snapshot() {
        Ok(snapshot) => match snapshot
            .tree
            .node(column_a)
            .and_then(|node| node.as_column())
            .and_then(|column| column.float_values())
        {
            Some(values) => values.as_ptr() as usize,
            None => {
                return SelfTestStep::fail(
                    name,
                    "column A not found or not numeric on the Rust side".to_owned(),
                )
            }
        },
        Err(error) => return SelfTestStep::fail(name, format!("snapshot failed: {error}")),
    };

    let outcome = match engine.run(
        "v = pyst.column(pyst.resolve(\"[Book1]Sheet1!A\"))\n\
         print(v.__array_interface__[\"data\"][0])\n",
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("the engine refused the script: {error}"))
        }
    };
    if let Some(error) = &outcome.error {
        return SelfTestStep::fail(name, format!("the script failed: {error}"));
    }

    let printed = outcome.stdout.trim();
    let numpy_address: usize = match printed.parse() {
        Ok(value) => value,
        Err(_) => {
            return SelfTestStep::fail(
                name,
                format!("unreadable address in the script output: {printed:?}"),
            )
        }
    };

    if numpy_address != rust_address {
        return SelfTestStep::fail(
            name,
            format!(
                "numpy looks at a different address than the model's Vec (rust=0x{rust_address:x}, \
                 numpy=0x{numpy_address:x}): a silent copy with the right values would give \
                 exactly this symptom"
            ),
        );
    }

    SelfTestStep::ok(
        name,
        format!("zero-copy numpy view confirmed: same address (0x{rust_address:x}) on the Rust and numpy sides"),
    )
}

/// Step 5: the script writes `B` from the view of `A` through
/// `pyst.set_column`, and an undo **on the host side**, as `Ctrl+Z` in the
/// window would do, restores `B` and the whole tree, bit for bit.
fn step_bus_write_host_undo(
    engine: &ScriptEngine,
    session: &SessionHandle,
    column_b: NodeId,
) -> SelfTestStep {
    let name = STEP_NAMES[4];

    let before_script = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => return SelfTestStep::fail(name, format!("initial snapshot failed: {error}")),
    };

    let outcome = match engine.run(
        "a = pyst.resolve(\"[Book1]Sheet1!A\")\n\
         b = pyst.resolve(\"[Book1]Sheet1!B\")\n\
         pyst.set_column(b, pyst.column(a))\n",
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("the engine refused the script: {error}"))
        }
    };
    if let Some(error) = &outcome.error {
        return SelfTestStep::fail(name, format!("the script failed: {error}"));
    }

    match column_values(session, column_b) {
        Ok(values) if values == [1.0, 2.0, 3.0] => {}
        Ok(values) => {
            return SelfTestStep::fail(
                name,
                format!("B after the script: expected [1.0, 2.0, 3.0], got {values:?}"),
            )
        }
        Err(detail) => return SelfTestStep::fail(name, detail),
    }

    match session.undo() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return SelfTestStep::fail(name, "the host-side undo found nothing to undo".to_owned())
        }
        Err(error) => {
            return SelfTestStep::fail(name, format!("the host-side undo failed: {error}"))
        }
    }

    let after_undo = match session.snapshot() {
        Ok(snapshot) => snapshot.tree,
        Err(error) => {
            return SelfTestStep::fail(name, format!("snapshot after undo failed: {error}"))
        }
    };
    if after_undo != before_script {
        return SelfTestStep::fail(
            name,
            "after the host-side undo, the tree differs bit for bit from the one before the script"
                .to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        "write through the bus undone from the host: B restored and tree equal bit for bit to the one \
         before the script"
            .to_owned(),
    )
}

/// Step 6: `pyst.undo()` undoes what the script just wrote, and a
/// second call on an empty stack returns `False` without raising an exception.
fn step_undo_from_python(
    engine: &ScriptEngine,
    session: &SessionHandle,
    column_b: NodeId,
) -> SelfTestStep {
    let name = STEP_NAMES[5];

    let outcome = match engine.run(
        "a = pyst.resolve(\"[Book1]Sheet1!A\")\n\
         b = pyst.resolve(\"[Book1]Sheet1!B\")\n\
         pyst.set_column(b, pyst.column(a))\n\
         print(pyst.undo())\n\
         print(pyst.undo())\n",
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("the engine refused the script: {error}"))
        }
    };
    if let Some(error) = &outcome.error {
        return SelfTestStep::fail(name, format!("the script failed: {error}"));
    }

    let lines: Vec<&str> = outcome.stdout.lines().map(str::trim).collect();
    if lines != ["True", "False"] {
        return SelfTestStep::fail(
            name,
            format!("sortie attendue [\"True\", \"False\"], obtenue {lines:?}"),
        );
    }

    match column_values(session, column_b) {
        Ok(values) if values == [10.0, 20.0, 30.0] => {}
        Ok(values) => {
            return SelfTestStep::fail(
                name,
                format!("B after the double undo: expected [10.0, 20.0, 30.0], got {values:?}"),
            )
        }
        Err(detail) => return SelfTestStep::fail(name, detail),
    }

    SelfTestStep::ok(
        name,
        "pyst.undo() undoes the script's write, then returns False on an empty stack without raising \
         an exception"
            .to_owned(),
    )
}

/// Step 7: a script's standard output is captured, an exception returns
/// a full traceback, and the engine stays usable afterward.
fn step_output_and_errors(engine: &ScriptEngine) -> SelfTestStep {
    let name = STEP_NAMES[6];
    const MARKER: &str = "STOKESFORGE-SELFTEST-2f8e19a4";

    let printed = match engine.run(&format!("print(\"{MARKER}\")\n")) {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(name, format!("the engine refused the script: {error}"))
        }
    };
    if let Some(error) = &printed.error {
        return SelfTestStep::fail(name, format!("a plain print must not fail: {error}"));
    }
    if !printed.stdout.contains(MARKER) {
        return SelfTestStep::fail(name, "the script's print was not captured".to_owned());
    }

    let failing = match engine.run("1 / 0\n") {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("the engine refused the failing script: {error}"),
            )
        }
    };
    let trace = match &failing.error {
        Some(trace) => trace,
        None => {
            return SelfTestStep::fail(
                name,
                "an exception should have been returned by the script".to_owned(),
            )
        }
    };
    if !trace.contains("Traceback") || !trace.contains("ZeroDivisionError") {
        return SelfTestStep::fail(
            name,
            format!(
                "incomplete traceback: expected \"Traceback\" and \"ZeroDivisionError\", got {trace:?}"
            ),
        );
    }

    let after = match engine.run("print(\"moteur encore utilisable\")\n") {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfTestStep::fail(
                name,
                format!("the engine does not survive a failing script: {error}"),
            )
        }
    };
    if after.error.is_some() || !after.stdout.contains("moteur encore utilisable") {
        return SelfTestStep::fail(
            name,
            "the script after the error does not run normally".to_owned(),
        );
    }

    SelfTestStep::ok(
        name,
        "print captured, full exception traceback (type and \"Traceback\"), engine usable after \
         the error"
            .to_owned(),
    )
}

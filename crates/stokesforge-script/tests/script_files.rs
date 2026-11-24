//! Contract of running a Python **file** (#552): its folder importable, its own
//! namespace, the process-global state put back, no trace left next to it.
//!
//! Every test mutates process-global interpreter state (`sys.path`,
//! `sys.modules`), so they take one exclusive lock and run one after the other.
#![cfg(feature = "python")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptError, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

static EXCLUSIVE: Mutex<()> = Mutex::new(());

struct Bench {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
    a: NodeId,
    dir: tempfile::TempDir,
    _lock: MutexGuard<'static, ()>,
}

/// An exclusive bench: the file runs move process-global Python state.
fn exclusive_bench() -> Bench {
    let lock = EXCLUSIVE.lock().unwrap_or_else(PoisonError::into_inner);
    let mut project = Project::new("Files").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let prefix = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine =
        Arc::new(ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(prefix)).unwrap());
    Bench {
        engine,
        session,
        _actor: actor,
        a,
        dir: tempfile::tempdir().unwrap(),
        _lock: lock,
    }
}

impl Bench {
    /// Writes a file under the bench folder and returns its path.
    fn write(&self, relative: &str, content: &[u8]) -> PathBuf {
        let path = self.dir.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Runs a file; the engine must accept it.
    fn file(&self, path: &Path) -> ScriptOutcome {
        self.engine.run_file(path).expect("the engine accepts it")
    }

    /// Runs a file that must succeed.
    fn file_ok(&self, path: &Path) -> ScriptOutcome {
        let outcome = self.file(path);
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert!(!outcome.interrupted);
        outcome
    }

    /// Runs console text that must succeed.
    fn text_ok(&self, source: &str) -> ScriptOutcome {
        let outcome = self.engine.run(source).unwrap();
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        outcome
    }

    fn a_values(&self) -> Vec<f64> {
        self.session
            .snapshot()
            .unwrap()
            .tree
            .node(self.a)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_ref()
            .clone()
    }
}

fn out(outcome: &ScriptOutcome) -> Vec<String> {
    outcome.stdout.lines().map(str::to_owned).collect()
}

/// The environment the runs must leave untouched, as one printable line.
const ENV_PROBE: &str = "import sys\nprint(repr(sys.path), repr(getattr(sys, 'argv', None)), \
    id(sys.modules['__main__']), sys.dont_write_bytecode, id(sys.path))\n";

fn probe(bench: &Bench) -> String {
    bench.text_ok(ENV_PROBE).stdout
}

#[test]
fn a_sibling_module_and_a_package_are_importable() {
    let bench = exclusive_bench();
    bench.write("helper.py", b"VALUE = 41\n");
    bench.write("pkg/__init__.py", b"NAME = 'pkg'\n");
    bench.write("pkg/inner.py", b"from . import NAME\nTWO = NAME * 2\n");
    let script = bench.write(
        "main.py",
        b"import helper\nimport pkg.inner\nprint(helper.VALUE + 1, pkg.inner.TWO)\n",
    );
    assert_eq!(out(&bench.file_ok(&script)), vec!["42 pkgpkg"]);
}

/// The Stop case proves the state is restored after an interruption, but not
/// the ORDER (restore after `running` dropped): that depends on the timing of the
/// asynchronous exception, so a restore done while still armed is not caught.
#[test]
fn the_environment_is_restored_after_success_error_and_stop() {
    let bench = exclusive_bench();
    bench.write("helper.py", b"VALUE = 1\n");
    let before = probe(&bench);

    let ok = bench.write("ok.py", b"import helper\n");
    bench.file_ok(&ok);
    assert_eq!(probe(&bench), before, "after a success");

    let bad = bench.write("bad.py", b"import helper\nraise RuntimeError('boom')\n");
    assert!(bench.file(&bad).error.is_some());
    assert_eq!(probe(&bench), before, "after an error");

    let spin = bench.write("spin.py", b"import helper\nwhile True:\n    pass\n");
    let engine = Arc::clone(&bench.engine);
    let worker = {
        let spin = spin.clone();
        thread::spawn(move || engine.run_file(&spin))
    };
    let start = Instant::now();
    while !bench.engine.is_running() {
        assert!(start.elapsed() < Duration::from_secs(30));
        thread::yield_now();
    }
    thread::sleep(Duration::from_millis(100));
    bench.engine.interrupt();
    let outcome = worker.join().unwrap().unwrap();
    assert!(outcome.interrupted);
    assert_eq!(probe(&bench), before, "after a stop");

    // The helper is gone from the console's view.
    let console = bench.engine.run("import helper\n").unwrap();
    assert!(
        console
            .error
            .as_deref()
            .is_some_and(|e| e.contains("ModuleNotFoundError")),
        "{:?}",
        console.error
    );
}

#[test]
fn an_edited_helper_a_new_sibling_and_a_namespace_package_are_read_again() {
    let bench = exclusive_bench();
    let helper = bench.write("helper.py", b"VALUE = 1\n");
    bench.write("ns/part.py", b"PART = 'a'\n");
    let script = bench.write(
        "main.py",
        b"import helper, ns.part\nprint(helper.VALUE, ns.part.PART)\ntry:\n    import late\n    print(late.X)\nexcept ImportError:\n    print('no late')\n",
    );
    assert_eq!(out(&bench.file_ok(&script)), vec!["1 a", "no late"]);

    std::fs::write(&helper, b"VALUE = 2\n").unwrap();
    bench.write("ns/part.py", b"PART = 'b'\n");
    bench.write("late.py", b"X = 'new'\n");
    assert_eq!(out(&bench.file_ok(&script)), vec!["2 b", "new"]);
    // The namespace package itself (no `__file__`) is forgotten too.
    let left = bench.text_ok("import sys\nprint('ns' in sys.modules, 'helper' in sys.modules)\n");
    assert_eq!(left.stdout.trim(), "False False");
}

#[test]
fn a_folder_above_the_embedded_prefix_keeps_numpy_and_json_identities() {
    let bench = exclusive_bench();
    let root = std::fs::canonicalize(
        std::env::var_os(interpreter::HOME_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../vendor/python/extracted/python")
            }),
    )
    .unwrap();
    // A script directly in the parent of the embedded prefix: the folder is an
    // ancestor of numpy and json.
    let parent = root.parent().unwrap().to_path_buf();
    let script = parent.join(format!("stokesforge_552_probe_{}.py", std::process::id()));
    std::fs::write(
        &script,
        b"import numpy, json\nprint(id(numpy), id(json), id(numpy.core if hasattr(numpy, 'core') else numpy))\n",
    )
    .unwrap();
    let first = bench.text_ok("import numpy, json\nprint(id(numpy), id(json))\n");
    let ids: Vec<String> = first.stdout.split_whitespace().map(str::to_owned).collect();
    let outcome = bench.engine.run_file(&script);
    std::fs::remove_file(&script).unwrap();
    let outcome = outcome.unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let got: Vec<&str> = outcome.stdout.split_whitespace().collect();
    assert_eq!(&got[..2], &[ids[0].as_str(), ids[1].as_str()]);
    let after = bench.text_ok("import numpy, json\nprint(id(numpy), id(json))\n");
    assert_eq!(after.stdout.split_whitespace().collect::<Vec<_>>(), ids);
}

#[test]
fn no_pycache_and_any_source_encoding_is_accepted() {
    let bench = exclusive_bench();
    bench.write("helper.py", b"V = 'h'\n");
    let dir = bench.dir.path().join("with spaces");
    std::fs::create_dir_all(&dir).unwrap();
    bench.write("with spaces/dep.py", b"D = 'd'\n");
    let spaced = bench.write("with spaces/main.py", b"import dep\nprint(dep.D)\n");
    assert_eq!(out(&bench.file_ok(&spaced)), vec!["d"]);
    assert!(!dir.join("__pycache__").exists());

    let main = bench.write("main.py", b"import helper\nprint(helper.V)\n");
    bench.file_ok(&main);
    assert!(!bench.dir.path().join("__pycache__").exists());

    let bom = bench.write(
        "bom.py",
        b"\xef\xbb\xbffrom __future__ import annotations\r\nprint('bom ok')\r\n",
    );
    assert_eq!(out(&bench.file_ok(&bom)), vec!["bom ok"]);

    let latin = bench.write(
        "latin.py",
        b"# -*- coding: latin-1 -*-\nprint(len('\xe9'))\n",
    );
    assert_eq!(out(&bench.file_ok(&latin)), vec!["1"]);

    let null = bench.write("null.py", b"x = 1\0\n");
    assert!(bench.file(&null).error.is_some());
}

#[test]
fn a_traceback_names_both_files_and_the_file_environment_is_exact() {
    let bench = exclusive_bench();
    let helper = bench.write("broken.py", b"x = 1\nraise ValueError('in helper')\n");
    let script = bench.write("main.py", b"import broken\n");
    let outcome = bench.file(&script);
    let error = outcome.error.expect("the helper fails");
    assert!(error.contains(helper.to_str().unwrap()), "{error}");
    assert!(error.contains("line 2"), "{error}");
    assert!(error.contains(script.to_str().unwrap()), "{error}");
    assert!(error.contains("line 1"), "{error}");

    let probe = bench.write(
        "env.py",
        b"import sys, dataclasses\nprint(__name__, __file__)\nprint(sys.argv)\n\
          print(sys.modules['__main__'].__dict__ is globals())\n\
          @dataclasses.dataclass\nclass P:\n    x: int = 1\nprint(P().x)\n\
          print(sys.path[0])\n",
    );
    let lines = out(&bench.file_ok(&probe));
    assert_eq!(lines[0], format!("__main__ {}", probe.display()));
    assert_eq!(lines[1], format!("['{}']", probe.display()));
    assert_eq!(lines[2], "True");
    assert_eq!(lines[3], "1");
    assert_eq!(lines[4], bench.dir.path().to_str().unwrap());
}

#[test]
fn a_normal_exit_commits_and_any_other_exit_rolls_back() {
    let bench = exclusive_bench();
    let write = "h = pyst.resolve('[Book1]Sheet1!A')\npyst.set_values(h, [7., 8., 9.])\n";
    for (name, tail) in [("e0.py", "exit()\n"), ("e1.py", "exit(0)\n")] {
        let path = bench.write(name, format!("{write}{tail}").as_bytes());
        bench.file_ok(&path);
        assert_eq!(bench.a_values(), vec![7., 8., 9.], "{name}");
        bench.session.undo().unwrap();
        assert_eq!(bench.a_values(), vec![1., 2., 3.], "undone {name}");
    }
    for tail in ["exit(2)\n", "exit('message')\n"] {
        let path = bench.write("bad.py", format!("{write}{tail}").as_bytes());
        let outcome = bench.file(&path);
        assert!(outcome.error.is_some(), "{tail}");
        assert_eq!(bench.a_values(), vec![1., 2., 3.], "{tail}");
    }
    // The same rule for console text.
    let text = format!("{write}exit()\n");
    bench.text_ok(&text);
    assert_eq!(bench.a_values(), vec![7., 8., 9.]);
    bench.session.undo().unwrap();
    for tail in ["exit(2)\n", "exit('message')\n"] {
        let outcome = bench.engine.run(&format!("{write}{tail}")).unwrap();
        assert!(outcome.error.is_some(), "{tail}");
        assert_eq!(bench.a_values(), vec![1., 2., 3.], "{tail}");
    }
}

#[test]
fn a_stop_in_a_helper_leaves_it_importable_by_the_next_run() {
    let bench = exclusive_bench();
    let helper = bench.write("slow.py", b"while True:\n    pass\n");
    let script = bench.write("main.py", b"import slow\nprint(slow.OK)\n");
    let engine = Arc::clone(&bench.engine);
    let worker = {
        let script = script.clone();
        thread::spawn(move || engine.run_file(&script))
    };
    let start = Instant::now();
    while !bench.engine.is_running() {
        assert!(start.elapsed() < Duration::from_secs(30));
        thread::yield_now();
    }
    thread::sleep(Duration::from_millis(100));
    bench.engine.interrupt();
    let outcome = worker.join().unwrap().unwrap();
    assert!(outcome.interrupted);
    std::fs::write(&helper, b"OK = 'fixed'\n").unwrap();
    assert_eq!(out(&bench.file_ok(&script)), vec!["fixed"]);
}

#[test]
fn a_file_run_is_its_own_run_for_the_read_versions_and_has_its_own_namespace() {
    let bench = exclusive_bench();
    // The console reads A and keeps a variable.
    bench.text_ok("h = pyst.resolve('[Book1]Sheet1!A')\nv = pyst.column(h)\nconsole_var = 5\n");
    // File run 1 reads A.
    let read = bench.write("read.py", b"h = pyst.resolve('[Book1]Sheet1!A')\nv = pyst.column(h)\nfile_var = 1\nprint('console_var' in globals())\n");
    assert_eq!(out(&bench.file_ok(&read)), vec!["False"]);
    // A is edited by hand.
    bench
        .session
        .execute(Command::SetCells {
            column: bench.a,
            start: 1,
            values: ValueBlock::Float(vec![99.]),
            expected_version: None,
        })
        .unwrap();
    // File run 2 writes A without reading it: accepted.
    let write = bench.write(
        "write.py",
        b"h = pyst.resolve('[Book1]Sheet1!A')\npyst.set_values(h, [4., 5., 6.])\n",
    );
    bench.file_ok(&write);
    assert_eq!(bench.a_values(), vec![4., 5., 6.]);
    // The console handle read before is still protected.
    let console = bench
        .engine
        .run("pyst.set_values(h, [0., 0., 0.])\n")
        .unwrap();
    assert!(
        console
            .error
            .as_deref()
            .is_some_and(|e| e.contains("ConcurrentWriteError")),
        "{:?}",
        console.error
    );
    // Fresh namespaces on both sides.
    let out = bench.text_ok("print('file_var' in globals(), console_var)\n");
    assert_eq!(out.stdout.trim(), "False 5");
    // One undo step per file run.
    bench.session.undo().unwrap();
    assert_eq!(bench.a_values(), vec![1., 99., 3.]);
}

#[test]
fn an_unreadable_file_is_a_host_error() {
    let bench = exclusive_bench();
    let missing = bench.dir.path().join("absent.py");
    match bench.engine.run_file(&missing) {
        Err(ScriptError::ReadFile { path, .. }) => assert_eq!(path, missing),
        other => panic!("{other:?}"),
    }
    // The engine is not left busy.
    bench.text_ok("pass\n");
}

/// A folder with accents. Needs `utf8_mode = 1` in the interpreter start
/// (`stokesforge-python/src/interpreter.rs`): the isolated config does not read
/// the locale, so the file system encoding is ASCII and the import of a
/// sibling fails with `UnicodeEncodeError`. Pending a decision on that file.
#[test]
#[ignore = "needs utf8_mode in interpreter.rs (#552), outside the L1 scope"]
fn a_folder_with_accents_is_importable() {
    let bench = exclusive_bench();
    bench.write("with accents \u{e9}/dep.py", "D = '\u{e9}'\n".as_bytes());
    let accent = bench.write("with accents \u{e9}/main.py", b"import dep\nprint(dep.D)\n");
    assert_eq!(out(&bench.file_ok(&accent)), vec!["\u{e9}"]);
}

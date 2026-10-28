//! Contract of Python writes (#67): existing commands, one group per run, and
//! rollback after an error or an interruption.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use stokesforge_commands::{Command, CommandError, HistoryLabel, MacroKind};
use stokesforge_core::{Column, ColumnMeta, NodeId, Project, ValueBlock};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionError, SessionHandle};

struct Bench {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
    a: NodeId,
    b: NodeId,
}

fn bench() -> Bench {
    let mut project = Project::new("Writing").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[9.0; 5]))
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
        b,
    }
}

fn run(bench: &Bench, source: &str) -> ScriptOutcome {
    let outcome = bench.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

fn floats(bench: &Bench, id: NodeId) -> Vec<f64> {
    bench
        .session
        .snapshot()
        .unwrap()
        .tree
        .node(id)
        .unwrap()
        .as_column()
        .unwrap()
        .float_values()
        .unwrap()
        .as_ref()
        .clone()
}

#[test]
fn builds_renames_fills_and_removes_in_a_single_group() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    run(
        &bench,
        r#"
book = pyst.create_workbook('Results')
assert pyst.children(book) == []
sheet = pyst.create_worksheet(book, 'Computations')
x = pyst.create_column(sheet, 'Time')
y = pyst.create_column(sheet, 'Signal')
t = pyst.create_column(sheet, 'Text', column_type='mixed')
d = pyst.create_column(sheet, 'Date', column_type='datetime')
pyst.set_values(x, [0, 1, 2])
pyst.set_values(y, [10.0, 20.0])
pyst.set_values(t, ['12', '', None, 'µ'])
pyst.set_values(d, [0.0, None])
pyst.set_column_meta(x, role='x', long_name='Elapsed time', unit='s', comment='Trial', display_format='.2f')
assert pyst.info(x)['column']['unit'] == 's'
pyst.rename(y, 'Intensité')
assert pyst.info(y)['name'] == 'Intensité'
pyst.remove(d)
assert len(pyst.children(sheet)) == 3
assert pyst.info(sheet)['row_count'] == 4
assert pyst.info(x)['column']['role'] == 'x'
assert pyst.values(t) == ('12', '', None, 'µ')
"#,
    );
    let after = bench.session.snapshot().unwrap();
    assert_eq!(
        after.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    assert!(!after.history.in_macro);
    assert_ne!(*after.tree, *before.tree);
    bench.session.undo().unwrap();
    let undone = bench.session.snapshot().unwrap();
    assert_eq!(*undone.tree, *before.tree);
    assert!(
        !undone.history.can_undo,
        "a single Ctrl+Z undoes the whole run"
    );
    bench.session.redo().unwrap();
    assert_eq!(*bench.session.snapshot().unwrap().tree, *after.tree);
}

#[test]
fn replaces_lengths_without_touching_other_columns_and_keeps_the_views() {
    let bench = bench();
    run(
        &bench,
        r#"
import numpy as np
a = pyst.resolve('[Book1]Sheet1!A')
old = pyst.column(a)
pyst.set_column_meta(a, unit='s')
pyst.set_values(a, np.arange(12., dtype=np.float64)[::2])
assert pyst.values(a) == (0., 2., 4., 6., 8., 10.)
pyst.set_values(a, [float('nan'), -0.0])
assert np.isnan(pyst.values(a)[0])
assert np.signbit(pyst.values(a)[1])
assert list(old) == [1., 2., 3.]
assert pyst.info(a)['column']['unit'] == 's'
assert pyst.info(pyst.resolve('[Book1]Sheet1!B'))['row_count'] == 5
"#,
    );
    assert_eq!(floats(&bench, bench.b), vec![9.0; 5]);
    assert_eq!(floats(&bench, bench.a).len(), 2);
    bench.session.undo().unwrap();
    assert_eq!(floats(&bench, bench.a), vec![1., 2., 3.]);
    bench.session.redo().unwrap();
    assert_eq!(floats(&bench, bench.a)[1].to_bits(), (-0.0_f64).to_bits());
    // #312: the host's undo/redo changed the column since the script read it;
    // the script rereads it before writing.
    run(&bench, "pyst.values(a)\npyst.set_values(a, [])");
    assert!(floats(&bench, bench.a).is_empty());
    bench.session.undo().unwrap();
    assert_eq!(floats(&bench, bench.a).len(), 2);
}

#[test]
fn mixed_cells_are_neither_reparsed_nor_normalized() {
    let bench = bench();
    run(
        &bench,
        r#"
import math
import numpy as np
sheet = pyst.info(pyst.resolve('[Book1]Sheet1!A'))['parent']
t = pyst.create_column(sheet, column_type='mixed')
source = [None, '', '12', float('nan'), -0.0, 2]
pyst.set_values(t, iter(source))
source[2] = 'modified after the copy'
v = pyst.values(t)
assert v[:3] == (None, '', '12')
assert math.isnan(v[3])
assert math.copysign(1., v[4]) == -1.
assert v[5] == 2.
pyst.set_values(t, np.array([float('nan'), -0.0]))
v = pyst.values(t)
assert math.isnan(v[0]) and v[0] is not None
assert math.copysign(1., v[1]) == -1.
"#,
    );
}

#[test]
fn set_values_refuses_booleans_and_leaves_the_column_unchanged() {
    let bench = bench();
    run(
        &bench,
        r#"
a = pyst.resolve('[Book1]Sheet1!A')
for bad in ([True, 2.], [1., False]):
    try:
        pyst.set_values(a, bad)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('bool accepted')
assert pyst.values(a) == (1., 2., 3.)
"#,
    );
    assert_eq!(floats(&bench, bench.a), vec![1., 2., 3.]);
}

#[test]
fn uncaught_error_also_undoes_creations_and_preserves_redo() {
    let bench = bench();
    run(
        &bench,
        "pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [4.])",
    );
    bench.session.undo().unwrap();
    let before = bench.session.snapshot().unwrap();
    let outcome = bench
        .engine
        .run(
            r#"
pyst.create_workbook('To undo')
pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [7., 8., 9., 10.])
raise RuntimeError('deliberate failure')
"#,
        )
        .unwrap();
    assert!(outcome.error.unwrap().contains("deliberate failure"));
    let after = bench.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    bench.session.redo().unwrap();
    assert_eq!(floats(&bench, bench.a), vec![4.]);
    run(&bench, "assert pyst.children()");
}

#[test]
fn refuses_types_names_and_caps_without_a_partial_write() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    run(
        &bench,
        r#"
import numpy as np
import numpy.ma as ma
a = pyst.resolve('[Book1]Sheet1!A')
for values in (['text'], [object()], [2**53 + 1], 'text', np.array([1], dtype=np.int64), np.ones((2, 2)), ma.array([1.], mask=[True]), np.broadcast_to(np.array([1.]), (100_000_001,))):
    try:
        pyst.set_values(a, values)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('invalid value accepted')
class TooLong:
    def __len__(self): return 100_000_001
    def __iter__(self): raise AssertionError('the cap must come before the copy')
try:
    pyst.set_values(a, TooLong())
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('cap ignored')
for name in ('', 'x' * 256, 'B', 'x!y'):
    try:
        pyst.rename(a, name)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('invalid name accepted')
for changes in ({'role': 'unknown'}, {'type': 'mixed'}, {'version': 1}, {'unit': None}):
    try:
        pyst.set_column_meta(a, **changes)
    except pyst.InvalidValueError:
        pass
    else:
        raise AssertionError('invalid metadata accepted')
try:
    pyst.create_worksheet(a)
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('wrong parent accepted')
try:
    pyst.remove(pyst.root())
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('root removed')
"#,
    );
    let after = bench.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert!(!after.history.can_undo);
    assert!(!after.history.in_macro);
}

#[test]
fn removal_and_load_do_not_make_a_stale_handle_valid() {
    let bench = bench();
    run(
        &bench,
        "a = pyst.resolve('[Book1]Sheet1!A')\npyst.remove(a)",
    );
    run(
        &bench,
        r#"
try:
    pyst.rename(a, 'Stale')
except pyst.BrokenReferenceError:
    pass
else:
    raise AssertionError('removed handle accepted')
"#,
    );
    bench.session.undo().unwrap();
    run(&bench, "pyst.rename(a, 'Renamed')");
    bench.session.undo().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.stf");
    bench.session.save(&path).unwrap();
    bench.session.load(&path).unwrap();
    let before = bench.session.snapshot().unwrap();
    run(
        &bench,
        r#"
for action in (lambda: pyst.rename(a, 'Stale'), lambda: pyst.remove(a), lambda: pyst.set_values(a, [8.]), lambda: pyst.set_column_meta(a, unit='s'), lambda: pyst.create_workbook(parent=a), lambda: pyst.create_worksheet(a), lambda: pyst.create_column(a)):
    try:
        action()
    except pyst.BrokenReferenceError:
        pass
    else:
        raise AssertionError('old epoch accepted')
"#,
    );
    let after = bench.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn new_project_invalidates_a_handle_even_if_the_identifier_is_reused() {
    let bench = bench();
    run(&bench, "a = pyst.resolve('[Book1]Sheet1!A')");
    bench.session.new_project().unwrap();
    run(&bench, "book = pyst.create_workbook('Book1')\ns = pyst.create_worksheet(book, 'Sheet1')\nc = pyst.create_column(s, 'A')\npyst.set_values(c, [1.])");
    let before = bench.session.snapshot().unwrap();
    assert!(before.tree.node(bench.a).is_some()); // identifier really reused
    assert_eq!(floats(&bench, bench.a), vec![1.]);
    run(
        &bench,
        r#"
try:
    pyst.set_values(a, [99.])
except pyst.BrokenReferenceError:
    pass
else:
    raise AssertionError('old epoch accepted after new project')
"#,
    );
    let after = bench.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn partial_metadata_preserves_the_fields_and_the_formula_is_protected() {
    let bench = bench();
    bench
        .session
        .execute(Command::SetColumnMeta {
            column: bench.b,
            meta: ColumnMeta {
                comment: "To preserve".into(),
                ..Default::default()
            },
        })
        .unwrap();
    run(
        &bench,
        r#"
b = pyst.resolve('[Book1]Sheet1!B')
pyst.set_column_meta(b, unit='s', role='y')
assert pyst.info(b)['column']['comment'] == 'To preserve'
pyst.set_column_meta(b, formula='A * 2')
assert pyst.info(b)['column']['formula'] == 'A * 2'
# A letter that designates no column of the sheet is refused (#321), the
# formula already set stays; writing another field does not touch it.
try:
    pyst.set_column_meta(b, formula='Z * 2')
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('unknown letter accepted')
assert pyst.info(b)['column']['formula'] == 'A * 2'
pyst.set_column_meta(b, unit='m')
assert pyst.info(b)['column']['formula'] == 'A * 2'
try:
    pyst.set_values(b, [123.])
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('formula column written')
pyst.set_column_meta(b, formula=None)
pyst.set_values(b, [123.])
"#,
    );
    run(
        &bench,
        "assert pyst.values(b) == (123.,)\nassert pyst.info(b)['column']['type'] == 'mixed'",
    );
    bench.session.undo().unwrap();
    assert_eq!(floats(&bench, bench.b), vec![9.; 5]);
}

#[test]
fn read_without_macro_and_explicit_undo_by_groups() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    run(&bench, "pyst.values(pyst.resolve('[Book1]Sheet1!A'))");
    assert_eq!(bench.session.snapshot().unwrap().revision, before.revision);
    run(
        &bench,
        r#"
a = pyst.resolve('[Book1]Sheet1!A')
pyst.set_values(a, [4.])
pyst.set_values(a, [5.])
assert pyst.undo()
assert pyst.values(a) == (1., 2., 3.)
assert not pyst.undo()
pyst.set_values(a, [6.])
"#,
    );
    assert_eq!(floats(&bench, bench.a), vec![6.]);
    bench.session.undo().unwrap();
    assert_eq!(*bench.session.snapshot().unwrap().tree, *before.tree);
}

#[test]
fn interruption_undoes_the_group_and_frees_the_session() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    let engine = Arc::clone(&bench.engine);
    let job = std::thread::spawn(move || {
        engine
            .run(
                r#"
a = pyst.resolve('[Book1]Sheet1!A')
pyst.create_workbook('Interrupted')
pyst.set_values(a, [4., 5., 6., 7.])
while True:
    pyst.set_values(a, [8., 9.])
"#,
            )
            .unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = bench.session.snapshot().unwrap();
        if snapshot
            .tree
            .node(bench.a)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_slice()
            != [1., 2., 3.]
        {
            assert!(snapshot.history.in_macro);
            assert!(matches!(
                bench.session.execute(Command::SetCells {
                    column: bench.b,
                    start: 0,
                    values: ValueBlock::Float(vec![20.]),
                    expected_version: None
                }),
                Err(SessionError::Command(CommandError::ForeignMacro("execute")))
            ));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the script did not start writing"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    bench.engine.interrupt();
    let outcome = job.join().unwrap();
    assert!(outcome.interrupted);
    let after = bench.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    run(
        &bench,
        "pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [42.])",
    );
    assert_eq!(floats(&bench, bench.a), vec![42.]);
}

#[test]
fn imports_and_writes_share_the_script_group() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("measurements.csv");
    std::fs::write(&path, "x;y\n1;2\n3;4\n").unwrap();
    run(
        &bench,
        &format!(
            r#"
sheets = pyst.import_file({path:?})
column = pyst.children(sheets[0])[0]
source = pyst.info(column)['column']['comment']
pyst.set_column_meta(column, unit='s')
assert pyst.info(column)['column']['comment'] == source
pyst.rename(sheets[0], 'Imported')
pyst.set_values(pyst.resolve('[Book1]Sheet1!A'), [99.])
"#,
            path = path.to_str().unwrap()
        ),
    );
    let after = bench.session.snapshot().unwrap();
    assert_eq!(
        after.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    bench.session.undo().unwrap();
    assert_eq!(*bench.session.snapshot().unwrap().tree, *before.tree);
    assert!(!bench.session.snapshot().unwrap().history.can_undo);
}

// --- #312: retained read version, expected_version, immediate refusal ----

/// A run expected to fail; returns the error text.
fn run_fails(bench: &Bench, source: &str) -> String {
    bench
        .engine
        .run(source)
        .unwrap()
        .error
        .expect("the script was supposed to fail")
}

/// Edit by another client (the interface) on A, outside any script.
fn edit_a_from_another_client(bench: &Bench) {
    bench
        .session
        .execute(Command::SetCells {
            column: bench.a,
            start: 1,
            values: ValueBlock::Float(vec![99.]),
            expected_version: None,
        })
        .unwrap();
}

const READ_A: &str = "h = pyst.resolve('[Book1]Sheet1!A')\n";

#[test]
fn set_column_refuses_a_column_modified_since_the_read() {
    let bench = bench();
    run(&bench, &format!("{READ_A}v = pyst.column(h)\n"));
    edit_a_from_another_client(&bench);
    let before = bench.session.snapshot().unwrap();
    let error = run_fails(&bench, "pyst.set_column(h, v * 2)\n");
    assert!(error.contains("ConcurrentWriteError"), "{error}");
    assert!(
        !error.contains("5 times") && !error.contains("five"),
        "{error}"
    );
    assert_eq!(floats(&bench, bench.a), vec![1., 99., 3.]);
    let after = bench.session.snapshot().unwrap();
    assert_eq!(after.history, before.history);
}

#[test]
fn set_values_refuses_a_column_modified_since_the_read() {
    let bench = bench();
    run(&bench, &format!("{READ_A}v = pyst.values(h)\n"));
    edit_a_from_another_client(&bench);
    let before = bench.session.snapshot().unwrap();
    let error = run_fails(&bench, "pyst.set_values(h, [7., 8., 9.])\n");
    assert!(error.contains("ConcurrentWriteError"), "{error}");
    assert_eq!(floats(&bench, bench.a), vec![1., 99., 3.]);
    assert_eq!(bench.session.snapshot().unwrap().history, before.history);
}

#[test]
fn two_writes_in_a_row_after_a_read_succeed() {
    let bench = bench();
    run(
        &bench,
        &format!(
            "{READ_A}v = pyst.column(h)\npyst.set_column(h, v * 2)\npyst.set_column(h, v * 3)\n\
             pyst.set_values(h, [4., 5., 6.])\npyst.set_values(h, [7., 8., 9.])\n"
        ),
    );
    assert_eq!(floats(&bench, bench.a), vec![7., 8., 9.]);
}

#[test]
fn explicit_expected_version_wins_over_the_version_read() {
    let bench = bench();
    run(&bench, &format!("{READ_A}v = pyst.column(h)\n"));
    edit_a_from_another_client(&bench);
    // Current version supplied: succeeds despite the edit made after the
    // read (v is the old read: the other's edit is overwritten knowingly).
    run(
        &bench,
        "pyst.set_column(h, v * 2, expected_version=pyst.info(h)['column']['version'])\n",
    );
    assert_eq!(floats(&bench, bench.a), vec![2., 4., 6.]);
    // Old version: refusal, column unchanged.
    let error = run_fails(
        &bench,
        "pyst.set_values(h, [0., 0., 0.], expected_version=0)\n",
    );
    assert!(error.contains("ConcurrentWriteError"), "{error}");
    let error = run_fails(&bench, "pyst.set_column(h, v, expected_version=0)\n");
    assert!(error.contains("ConcurrentWriteError"), "{error}");
    assert_eq!(floats(&bench, bench.a), vec![2., 4., 6.]);
}

#[test]
fn a_column_never_read_stays_writable() {
    let bench = bench();
    run(
        &bench,
        &format!("{READ_A}pyst.set_values(h, [5., 5., 5.])\n"),
    );
    edit_a_from_another_client(&bench);
    run(&bench, "pyst.set_values(h, [6., 6., 6.])\n");
    assert_eq!(floats(&bench, bench.a), vec![6., 6., 6.]);
}

#[test]
fn undo_forgets_the_versions_read() {
    let bench = bench();
    run(&bench, &format!("{READ_A}v = pyst.column(h)\n"));
    edit_a_from_another_client(&bench);
    // Without undo() the version read would be stale (the edit is undone below).
    // After undo(), the script has no read version left: writing is allowed.
    run(&bench, "pyst.undo()\npyst.set_column(h, v * 2)\n");
    assert_eq!(floats(&bench, bench.a), vec![2., 4., 6.]);
}

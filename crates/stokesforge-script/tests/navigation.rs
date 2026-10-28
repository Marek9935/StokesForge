//! Contract for navigating and reading the project from `pyst` (issue #66).
//! The same snapshots and handles serve a console and a session without an
//! interface; no test depends on an active sheet or a window.

#![cfg(feature = "python")]

use std::path::PathBuf;

use stokesforge_commands::Command;
use stokesforge_core::{
    Column, ColumnFormula, ColumnMeta, ColumnRole, ColumnType, FormulaPart, MixedCell, NodeId,
    Project, ValueBlock,
};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::ScriptEngine;
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: ScriptEngine,
    session: SessionHandle,
    _actor: Session,
    book: NodeId,
    empty_folder: NodeId,
    number: NodeId,
    mixed: NodeId,
}

fn bench() -> Bench {
    let mut project = Project::new("Reading").unwrap();
    let root = project.root();
    let folder = project.add_folder(root, "Experiments").unwrap();
    let empty_folder = project.add_folder(root, "Empty").unwrap();
    let book = project.add_workbook(folder, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project.add_worksheet(book, Some("Empty")).unwrap();
    let number = project
        .add_column(
            sheet,
            None,
            Some("A"),
            Column::from_block(
                ColumnType::Numeric,
                ValueBlock::Float(vec![1.0, -0.0, f64::NAN]),
                ColumnMeta {
                    role: ColumnRole::X,
                    long_name: "Température".into(),
                    unit: "°C".into(),
                    comment: "Trial μ".into(),
                    display_format: ".2f".into(),
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
    project
        .add_column(
            sheet,
            None,
            Some("T"),
            Column::from_text(&[
                Some("étiquette".into()),
                None,
                Some("".into()),
                Some("12.5".into()),
            ]),
        )
        .unwrap();
    let mixed = project
        .add_column(
            sheet,
            None,
            Some("M"),
            Column::from_mixed(&[
                MixedCell::Empty,
                MixedCell::Text("".into()),
                MixedCell::Number(f64::NAN),
                MixedCell::Number(-0.0),
                MixedCell::Text("12.5".into()),
                MixedCell::Number(f64::INFINITY),
                MixedCell::Text("μ\nλ".into()),
            ]),
        )
        .unwrap();
    project
        .add_column(
            sheet,
            None,
            Some("Date"),
            Column::from_block(
                ColumnType::DateTime,
                ValueBlock::Float(vec![0.0, 1_700_000_000.0]),
                ColumnMeta::default(),
            )
            .unwrap(),
        )
        .unwrap();
    project
        .add_column(sheet, None, Some("Empty"), Column::from_mixed(&[]))
        .unwrap();
    project.add_note(root, Some("Note1"), "Note").unwrap();
    project
        .add_graph(root, Some("Graph1"), Default::default())
        .unwrap();
    project.add_matrix(root, Some("Matrix1")).unwrap();

    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let home = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine = ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(home)).unwrap();
    Bench {
        engine,
        session,
        _actor: actor,
        book,
        empty_folder,
        number,
        mixed,
    }
}

fn execute(bench: &Bench, source: &str) {
    let outcome = bench.engine.run(source).unwrap();
    assert!(
        outcome.error.is_none(),
        "{}",
        outcome.error.unwrap_or_default()
    );
}

#[test]
fn navigation_ordering_metadata_and_lengths_without_mutation() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    execute(
        &bench,
        r#"
def names(handles):
    return [pyst.info(h)['name'] for h in handles]
root_handle = pyst.root()
assert pyst.info(root_handle) == dict(name='Reading', kind='folder', parent=None, row_count=None, column=None)
assert names(pyst.children()) == ['Experiments', 'Empty', 'Note1', 'Graph1', 'Matrix1']
assert names(pyst.children(root_handle)) == names(pyst.children())
folder = pyst.children()[0]
book = pyst.children(folder)[0]
assert pyst.info(book)['kind'] == 'workbook'
assert pyst.info(pyst.info(book)['parent'])['name'] == 'Experiments'
assert names(pyst.children(book)) == ['Sheet1', 'Empty']
sheet, empty_sheet = pyst.children(book)
assert pyst.info(sheet)['kind'] == 'worksheet'
assert pyst.info(sheet)['row_count'] == 7
assert pyst.info(empty_sheet)['row_count'] == 0
assert pyst.children(empty_sheet) == []
assert pyst.children(pyst.children()[1]) == []
assert names(pyst.children(sheet)) == ['A', 'T', 'M', 'Date', 'Empty']
a = pyst.children(sheet)[0]
description = pyst.info(a)
assert description['name'] == 'A' and description['kind'] == 'column'
assert description['row_count'] == 3
assert pyst.info(description['parent'])['name'] == 'Sheet1'
meta = description['column']
assert set(meta) == {'type', 'role', 'long_name', 'unit', 'comment', 'x_prime', 'display_format', 'formula', 'version'}
assert meta['type'] == 'numeric' and meta['role'] == 'x'
assert meta['long_name'] == 'Température' and meta['unit'] == '°C'
assert meta['comment'] == 'Trial μ' and meta['display_format'] == '.2f'
assert meta['formula'] is None and meta['x_prime'] is None and isinstance(meta['version'], int)
assert pyst.info(pyst.resolve('[Book1]Sheet1!Date'))['column']['type'] == 'datetime'
assert pyst.info(pyst.resolve('[Book1]Sheet1!T'))['column']['type'] == 'mixed'
assert pyst.children(a) == []
for name, kind in [('Note1', 'note'), ('Graph1', 'graph'), ('Matrix1', 'matrix')]:
    h = pyst.resolve(f'[{name}]')
    assert pyst.info(h)['kind'] == kind
    assert pyst.info(h)['column'] is None and pyst.info(h)['row_count'] is None
    assert pyst.children(h) == []
# The dictionary and the list are values detached from the model.
meta['unit'] = 'K'
description['name'] = 'Other'
columns = pyst.children(sheet)
columns.clear()
assert pyst.info(a)['column']['unit'] == '°C'
assert pyst.info(a)['name'] == 'A' and len(pyst.children(sheet)) == 5
assert not hasattr(a, 'id') and not hasattr(a, 'epoch')
try:
    pyst.Handle()
except TypeError:
    pass
else:
    raise AssertionError('handle constructible from Python')
"#,
    );
    let after = bench.session.snapshot().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

#[test]
fn text_mixed_and_numeric_cells_kept_exactly() {
    let bench = bench();
    execute(
        &bench,
        r#"
import math
import numpy
text = pyst.values(pyst.resolve('[Book1]Sheet1!T'))
assert text == ('étiquette', None, '', '12.5')
m = pyst.resolve('[Book1]Sheet1!M')
cells = pyst.values(m)
assert isinstance(cells, tuple) and len(cells) == 7
assert cells[0] is None and cells[1] == '' and math.isnan(cells[2])
assert cells[3] == 0 and math.copysign(1, cells[3]) == -1
assert cells[4] == '12.5' and type(cells[4]) is str
assert math.isinf(cells[5]) and cells[6] == 'μ\nλ'
try:
    cells[0] = 42
except TypeError:
    pass
else:
    raise AssertionError('tuple modifiable')
assert pyst.values(pyst.resolve('[Book1]Sheet1!Empty')) == ()
a = pyst.resolve('[Book1]Sheet1!A')
numbers = pyst.values(a)
assert numbers[0] == 1 and math.copysign(1, numbers[1]) == -1 and math.isnan(numbers[2])
view = pyst.column(a)
assert isinstance(view, numpy.ndarray) and view.dtype == numpy.float64 and not view.flags.writeable
assert numpy.shares_memory(view, pyst.column(a))
date = pyst.resolve('[Book1]Sheet1!Date')
assert pyst.values(date) == (0., 1700000000.)
assert list(pyst.column(date)) == [0., 1700000000.]
"#,
    );
    bench
        .session
        .execute(Command::SetCells {
            column: bench.mixed,
            start: 0,
            values: ValueBlock::Mixed(vec![MixedCell::Text("new".into())]),
            expected_version: None,
        })
        .unwrap();
    execute(&bench, "assert cells[0] is None\nassert pyst.values(m)[0] == 'new'\nassert text == ('étiquette', None, '', '12.5')");
}

#[test]
fn reads_refuse_wrong_targets_with_the_bridge_exceptions() {
    let bench = bench();
    let before = bench.session.snapshot().unwrap();
    execute(
        &bench,
        r#"
for h in [pyst.root(), pyst.resolve('[Book1]'), pyst.resolve('[Book1]Sheet1'), pyst.resolve('[Note1]')]:
    try:
        pyst.values(h)
    except pyst.InvalidValueError as e:
        assert 'not a column' in str(e)
    else:
        raise AssertionError('a cell read accepts another object')
for path in ['[Absent]', '[Book1]Absent', '[Book1]Sheet1!Absent', '[malformed']:
    try:
        pyst.resolve(path)
    except pyst.UnknownPathError:
        pass
    else:
        raise AssertionError('unknown path accepted')
"#,
    );
    let after = bench.session.snapshot().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.history, before.history);
}

#[test]
fn navigation_handles_survive_renaming_moving_and_undo() {
    let bench = bench();
    execute(
        &bench,
        "book = pyst.children(pyst.children()[0])[0]\na = pyst.children(pyst.children(book)[0])[0]",
    );
    bench
        .session
        .execute(Command::Rename {
            id: bench.number,
            name: "Renamed".into(),
        })
        .unwrap();
    bench
        .session
        .execute(Command::Move {
            id: bench.book,
            new_parent: bench.empty_folder,
            index: 0,
        })
        .unwrap();
    execute(
        &bench,
        r#"
assert pyst.info(a)['name'] == 'Renamed'
assert pyst.values(a)[0] == 1
assert pyst.info(pyst.info(book)['parent'])['name'] == 'Empty'
assert pyst.info(pyst.children(pyst.children(book)[0])[0])['name'] == 'Renamed'
"#,
    );
    bench
        .session
        .execute(Command::DeleteColumn {
            column: bench.number,
        })
        .unwrap();
    execute(
        &bench,
        r#"
for reader in [pyst.info, pyst.children, pyst.values, pyst.column]:
    try:
        reader(a)
    except pyst.BrokenReferenceError:
        pass
    else:
        raise AssertionError('removed handle accepted')
"#,
    );
    bench.session.undo().unwrap();
    execute(
        &bench,
        "assert pyst.info(a)['name'] == 'Renamed'\nassert pyst.values(a)[0] == 1",
    );
}

#[test]
fn loading_invalidates_all_reads_even_if_the_identifiers_reappear() {
    let bench = bench();
    execute(
        &bench,
        r#"
root_handle = pyst.root()
book = pyst.children(pyst.children()[0])[0]
sheet = pyst.children(book)[0]
a, text, mixed_col, date, empty_col = pyst.children(sheet)
parent = pyst.info(a)['parent']
"#,
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identical.stf");
    bench.session.save(&path).unwrap();
    bench.session.load(&path).unwrap();
    execute(
        &bench,
        r#"
for h in [root_handle, book, sheet, parent, a, text, mixed_col, date, empty_col]:
    for reader in [pyst.info, pyst.children, pyst.values, pyst.column]:
        try:
            reader(h)
        except pyst.BrokenReferenceError as e:
            assert 'loaded' in str(e)
        else:
            raise AssertionError('a stale handle designates the reloaded project')
assert pyst.info(pyst.root())['name'] == 'Reading'
assert len(pyst.children()) == 5
assert pyst.values(pyst.resolve('[Book1]Sheet1!T')) == ('étiquette', None, '', '12.5')
"#,
    );
}

#[test]
fn metadata_read_back_and_stable_column_roles() {
    let bench = bench();
    for (role, expected) in [
        (ColumnRole::X, "x"),
        (ColumnRole::Y, "y"),
        (ColumnRole::X2, "x2"),
        (ColumnRole::Y2, "y2"),
        (ColumnRole::X3, "x3"),
        (ColumnRole::Y3, "y3"),
        (ColumnRole::X4, "x4"),
        (ColumnRole::Y4, "y4"),
        (ColumnRole::Z, "z"),
        (ColumnRole::XError, "x_error"),
        (ColumnRole::YError, "y_error"),
        (ColumnRole::Label, "label"),
        (ColumnRole::None, "none"),
    ] {
        bench
            .session
            .execute(Command::SetColumnMeta {
                column: bench.number,
                meta: ColumnMeta {
                    role,
                    unit: "K".into(),
                    formula: Some(ColumnFormula::from_parts(vec![FormulaPart::Text(
                        "2+3".into(),
                    )])),
                    ..Default::default()
                },
            })
            .unwrap();
        execute(&bench, &format!("meta = pyst.info(pyst.resolve('[Book1]Sheet1!A'))['column']\nassert meta['role'] == {expected:?}\nassert meta['unit'] == 'K'\nassert meta['formula'] == '2+3'"));
    }
}

/// #168: `set_column_meta(role='y4')` reads back as `'y4'`; a role outside the
/// list (`'x5'`) is refused.
#[test]
fn roles_x3_to_y4_can_be_written_and_x5_is_refused() {
    let bench = bench();
    execute(
        &bench,
        r#"
a = pyst.resolve('[Book1]Sheet1!A')
pyst.set_column_meta(a, role='y4')
assert pyst.info(a)['column']['role'] == 'y4'
try:
    pyst.set_column_meta(a, role='x5')
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('role x5 accepted')
"#,
    );
}

/// Formulas in letters on the `pyst` side (#321): `info` reads letters, an
/// unknown letter is refused, a read column that disappears leaves `#REF`
/// (which cannot be rewritten), and changing another field of that column
/// succeeds without touching the formula.
#[test]
fn formulas_are_read_and_written_in_letters_and_ref_is_refused() {
    let bench = bench();
    execute(
        &bench,
        r#"
e = pyst.resolve('[Book1]Sheet1!Empty')
pyst.set_column_meta(e, formula='a * 2')
assert pyst.info(e)['column']['formula'] == 'A * 2'

try:
    pyst.set_column_meta(e, formula='F*2')
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('letter without a column accepted')
assert pyst.info(e)['column']['formula'] == 'A * 2'

pyst.remove(pyst.resolve('[Book1]Sheet1!A'))
assert pyst.info(e)['column']['formula'] == '#REF * 2'

try:
    pyst.set_column_meta(e, formula='#REF * 2')
except pyst.InvalidValueError:
    pass
else:
    raise AssertionError('#REF rewritten')
pyst.set_column_meta(e, unit='m')
meta = pyst.info(e)['column']
assert meta['unit'] == 'm' and meta['formula'] == '#REF * 2'
"#,
    );
}

#[test]
fn closed_session_also_refuses_the_root_and_reads_without_a_handle() {
    let Bench {
        engine,
        _actor: actor,
        ..
    } = bench();
    let outcome = engine
        .run("root_handle = pyst.root()\na = pyst.resolve('[Book1]Sheet1!A')")
        .unwrap();
    assert!(outcome.error.is_none());
    actor.shutdown().unwrap();
    let outcome = engine
        .run(
            r#"
for reader in [pyst.root, pyst.children, lambda: pyst.children(root_handle),
                lambda: pyst.info(a), lambda: pyst.values(a), lambda: pyst.column(a)]:
    try:
        reader()
    except pyst.BrokenReferenceError as e:
        assert 'session is closed' in str(e)
    else:
        raise AssertionError('the read ignores the closing of the session')
"#,
        )
        .unwrap();
    assert!(
        outcome.error.is_none(),
        "{}",
        outcome.error.unwrap_or_default()
    );
}

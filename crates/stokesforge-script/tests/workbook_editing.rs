//! Workbook editing from `pyst` (#331, batch 2, S5): `insert_rows`,
//! `delete_rows`, `set_column_type`, `move`, `duplicate`, `set_note`. Each
//! call goes through an undoable command; `pyst.undo()` restores the earlier
//! state.
#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Arc;

use stokesforge_core::{Column, MixedCell, Project};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ScriptOutcome};
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: Arc<ScriptEngine>,
    session: SessionHandle,
    _actor: Session,
}

/// `[Book1]Sheet1`: `A` = 1, 2, 3; `B` = 4, 5, 6; `C` mixed = "x", empty, 3.
/// A folder `Folder` and a note `Note1` at the root.
fn bench() -> Bench {
    let mut project = Project::new("Editing").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[4.0, 5.0, 6.0]))
        .unwrap();
    project
        .add_column(
            sheet,
            None,
            Some("C"),
            Column::from_mixed(&[
                MixedCell::Text("x".into()),
                MixedCell::Empty,
                MixedCell::Number(3.0),
            ]),
        )
        .unwrap();
    project.add_folder(project.root(), "Folder").unwrap();
    project
        .add_note(project.root(), Some("Note1"), "before")
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
    }
}

fn run(bench: &Bench, source: &str) -> ScriptOutcome {
    let outcome = bench.engine.run(source).unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    outcome
}

const HANDLES: &str = "import math\n\
    sheet = pyst.resolve('[Book1]Sheet1')\n\
    a = pyst.resolve('[Book1]Sheet1!A')\n\
    b = pyst.resolve('[Book1]Sheet1!B')\n\
    c = pyst.resolve('[Book1]Sheet1!C')\n";

/// Runs `body` after the usual handles.
fn script(bench: &Bench, body: &str) -> ScriptOutcome {
    run(bench, &format!("{HANDLES}{body}"))
}

/// The script must raise `kind`; returns the message.
fn refused(bench: &Bench, kind: &str, body: &str) -> String {
    let source = format!(
        "{HANDLES}try:\n    {body}\nexcept pyst.{kind} as error:\n    caught = str(error)\nelse:\n    raise AssertionError('no {kind}')\nprint(caught)"
    );
    run(bench, &source).stdout.trim().to_owned()
}

#[test]
fn insert_rows_inserts_empty_rows_and_undoes() {
    let b = bench();
    script(
        &b,
        "pyst.insert_rows(sheet, 1, 2)\n\
         va = pyst.values(a)\n\
         assert len(va) == 5 and va[0] == 1.0 and va[3] == 2.0 and va[4] == 3.0, va\n\
         assert math.isnan(va[1]) and math.isnan(va[2])\n\
         vc = pyst.values(c)\n\
         assert vc == ('x', None, None, None, 3.0), vc\n\
         pyst.insert_rows(sheet, 5)\n\
         assert len(pyst.values(b)) == 6\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\nassert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         assert pyst.values(c) == ('x', None, 3.0)\n",
    );
}

#[test]
fn row_writes_do_not_stale_the_version_read_by_the_script() {
    let b = bench();
    // Read `a`, change its rows, then write again: no ConcurrentWriteError.
    script(
        &b,
        "pyst.values(a)\npyst.insert_rows(sheet, 0)\npyst.set_values(a, [9.0, 9.0, 9.0, 9.0])\n\
         pyst.values(b)\npyst.delete_rows(sheet, 0)\npyst.set_values(b, [7.0])\n\
         pyst.values(c)\npyst.set_column_type(c, 'mixed')\npyst.set_values(c, ['z'])\n",
    );
}

#[test]
fn insert_rows_refuses_a_non_worksheet_or_an_overflow() {
    let b = bench();
    let message = refused(&b, "InvalidValueError", "pyst.insert_rows(a, 0)");
    assert!(message.contains("not a worksheet"), "{message}");
    refused(&b, "InvalidValueError", "pyst.insert_rows(sheet, 10, 1)");
    let message = refused(&b, "InvalidValueError", "pyst.insert_rows(sheet, 0, 10**9)");
    assert!(message.contains("exceeds the limit"), "{message}");
    refused(
        &b,
        "InvalidValueError",
        "pyst.delete_rows(pyst.resolve('[Note1]'), 0, 1)",
    );
    script(&b, "assert pyst.values(a) == (1.0, 2.0, 3.0)");
}

#[test]
fn delete_rows_removes_rows_and_undoes() {
    let b = bench();
    script(
        &b,
        "pyst.delete_rows(sheet, 0)\n\
         assert pyst.values(a) == (2.0, 3.0) and pyst.values(b) == (5.0, 6.0)\n\
         assert pyst.values(c) == (None, 3.0)\n\
         pyst.delete_rows(sheet, 0, 2)\n\
         assert pyst.values(a) == ()\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\nassert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         assert pyst.values(b) == (4.0, 5.0, 6.0)\nassert pyst.values(c) == ('x', None, 3.0)\n",
    );
}

#[test]
fn set_column_type_converts_and_undoes() {
    let b = bench();
    script(
        &b,
        "pyst.set_column_type(a, 'mixed')\n\
         assert pyst.info(a)['column']['type'] == 'mixed'\n\
         assert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         pyst.set_column_type(c, 'numeric')\n\
         assert pyst.info(c)['column']['type'] == 'numeric'\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\n\
         assert pyst.info(a)['column']['type'] == 'numeric'\n\
         assert pyst.info(c)['column']['type'] == 'mixed'\n\
         assert pyst.values(c) == ('x', None, 3.0)\n",
    );
}

#[test]
fn set_column_type_refuses_an_unknown_type_or_a_non_column() {
    let b = bench();
    let message = refused(&b, "InvalidValueError", "pyst.set_column_type(a, 'text')");
    assert!(message.contains("unknown column type"), "{message}");
    refused(
        &b,
        "InvalidValueError",
        "pyst.set_column_type(sheet, 'mixed')",
    );
    script(&b, "assert pyst.info(a)['column']['type'] == 'numeric'");
}

#[test]
fn move_reorders_and_relocates_without_changing_the_handle() {
    let b = bench();
    script(
        &b,
        "pyst.move(a, sheet)\n\
         assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['B', 'C', 'A']\n\
         pyst.move(c, sheet, 0)\n\
         assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['C', 'B', 'A']\n\
         note = pyst.resolve('[Note1]')\n\
         folder = pyst.resolve('Folder')\n\
         pyst.move(note, folder)\n\
         assert pyst.info(pyst.info(note)['parent'])['name'] == 'Folder'\n\
         assert pyst.resolve('/Folder/[Note1]') is not None\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\n\
         assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['A', 'B', 'C']\n\
         assert pyst.info(pyst.info(pyst.resolve('[Note1]'))['parent'])['name'] == pyst.info(pyst.root())['name']\n",
    );
}

#[test]
fn move_refuses_an_unfit_parent_a_cycle_or_an_out_of_range_index() {
    let b = bench();
    for body in [
        "pyst.move(a, pyst.resolve('Folder'))",
        "pyst.move(pyst.resolve('Folder'), pyst.resolve('Folder'))",
        "pyst.move(a, sheet, 7)",
        "pyst.move(pyst.root(), pyst.resolve('Folder'))",
    ] {
        refused(&b, "InvalidValueError", body);
    }
    script(
        &b,
        "assert [pyst.info(h)['name'] for h in pyst.children(sheet)] == ['A', 'B', 'C']",
    );
}

#[test]
fn duplicate_copies_a_sheet_right_after_the_original() {
    let b = bench();
    script(
        &b,
        "copy = pyst.duplicate(sheet)\n\
         book = pyst.info(sheet)['parent']\n\
         assert [pyst.info(h)['name'] for h in pyst.children(book)] == ['Sheet1', 'Sheet1 copy 1']\n\
         ca = pyst.children(copy)[0]\n\
         assert pyst.values(ca) == (1.0, 2.0, 3.0)\n\
         pyst.set_values(ca, [0.0])\n\
         assert pyst.values(a) == (1.0, 2.0, 3.0)\n\
         named = pyst.duplicate(sheet, name='Other')\n\
         assert pyst.info(named)['name'] == 'Other'\n\
         again = pyst.duplicate(sheet)\n\
         assert pyst.info(again)['name'] == 'Sheet1 copy 2'\n",
    );
    script(
        &b,
        "assert pyst.undo() is True\n\
         book = pyst.info(sheet)['parent']\n\
         assert [pyst.info(h)['name'] for h in pyst.children(book)] == ['Sheet1']\n",
    );
}

#[test]
fn duplicate_also_copies_a_note_and_refuses_the_root_or_a_taken_name() {
    let b = bench();
    script(
        &b,
        "note = pyst.resolve('[Note1]')\n\
         copy = pyst.duplicate(note)\n\
         assert pyst.info(copy)['name'] == 'Note1 copy 1' and pyst.info(copy)['kind'] == 'note'\n\
         pyst.set_note(copy, 'changé')\n",
    );
    refused(&b, "InvalidValueError", "pyst.duplicate(pyst.root())");
    refused(
        &b,
        "InvalidValueError",
        "pyst.duplicate(pyst.resolve('[Note1]'), name='Note1')",
    );
    refused(
        &b,
        "InvalidValueError",
        "pyst.duplicate(pyst.resolve('[Note1]'), name='')",
    );
}

#[test]
fn set_note_replaces_the_text_and_undoes() {
    let b = bench();
    let text = |b: &Bench| {
        let snapshot = b.session.snapshot().unwrap();
        let id = snapshot.tree.resolve("[Note1]").unwrap();
        match snapshot.tree.node(id).unwrap().kind() {
            stokesforge_core::NodeKind::Note { text, .. } => text.clone(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(text(&b), "before");
    script(
        &b,
        "pyst.set_note(pyst.resolve('[Note1]'), 'line 1\\nline 2')",
    );
    assert_eq!(text(&b), "line 1\nline 2");
    script(&b, "assert pyst.undo() is True");
    assert_eq!(text(&b), "before");
    let message = refused(&b, "InvalidValueError", "pyst.set_note(a, 'x')");
    assert!(message.contains("not a note"), "{message}");
}

//! The sheet of an untouched import ("Raw Data") renames itself to "Sheet1" at
//! the first change to the value of an imported column, inside the history
//! entry of that change (#488). Public API of `stokesforge-session` only.

use stokesforge_commands::{Command, HistoryLabel, MacroKind};
use stokesforge_core::{
    Column, ColumnMeta, ColumnType, ImportSource, NodeId, Project, RawSheetMark, ValueBlock,
};
use stokesforge_session::{Session, SessionHandle};

struct Fixture {
    project: Project,
    sheet: NodeId,
    /// Imported columns A and B, and a hand-made column C.
    a: NodeId,
    b: NodeId,
    c: NodeId,
}

fn source(column: usize) -> ImportSource {
    ImportSource {
        path: "/tmp/measure.csv".into(),
        recipe_json: "{}".into(),
        column,
        block: 0,
    }
}

fn fixture() -> Fixture {
    let mut project = Project::new("Demo").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("measure")).expect("book");
    let sheet = project.tree().children(book).expect("sheets")[0];
    project.rename(sheet, "Raw Data").expect("rename");
    let mut add = |name: &str, values: &[f64], imported: Option<usize>| {
        let id = project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .expect("column");
        if let Some(index) = imported {
            let mut meta = project
                .tree()
                .node(id)
                .and_then(|n| n.as_column())
                .expect("column")
                .meta()
                .clone();
            meta.import_source = Some(source(index));
            project.set_column_meta(id, meta).expect("meta");
        }
        id
    };
    let a = add("A", &[1.0, 2.0, 3.0], Some(0));
    let b = add("B", &[10.0, 20.0, 30.0], Some(1));
    let c = add("C", &[7.0, 8.0, 9.0], None);
    project
        .set_raw_mark(
            sheet,
            Some(RawSheetMark {
                modified_base: "Sheet".into(),
            }),
        )
        .expect("mark");
    Fixture {
        project,
        sheet,
        a,
        b,
        c,
    }
}

fn name_of(handle: &SessionHandle, node: NodeId) -> String {
    handle
        .snapshot()
        .expect("snapshot")
        .tree
        .node(node)
        .expect("node")
        .name()
        .to_string()
}

fn marked(handle: &SessionHandle, sheet: NodeId) -> bool {
    handle
        .snapshot()
        .expect("snapshot")
        .tree
        .raw_sheets()
        .contains(&sheet)
}

fn meta_of(handle: &SessionHandle, column: NodeId) -> ColumnMeta {
    handle
        .snapshot()
        .expect("snapshot")
        .tree
        .node(column)
        .and_then(|n| n.as_column())
        .expect("column")
        .meta()
        .clone()
}

fn set(column: NodeId, value: f64) -> Command {
    Command::SetCells {
        column,
        start: 0,
        values: ValueBlock::Float(vec![value]),
        expected_version: None,
    }
}

fn undo_count(handle: &SessionHandle) -> usize {
    let mut project_steps = 0;
    while handle.undo().expect("undo").is_some() {
        project_steps += 1;
    }
    project_steps
}

/// Runs `command` on a fresh fixture and checks the sheet renamed itself in one
/// history step that undo and redo carry.
fn assert_renames(command: impl FnOnce(&Fixture) -> Command) {
    let f = fixture();
    let command = command(&f);
    let (sheet, session) = (f.sheet, Session::spawn(f.project, None).expect("spawn"));
    let handle = session.handle();
    handle.execute(command).expect("execute");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
    assert!(!marked(&handle, sheet));
    handle.undo().expect("undo");
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert!(marked(&handle, sheet));
    handle.redo().expect("redo");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
    assert!(!marked(&handle, sheet));
    // One entry: a single undo took everything back.
    assert_eq!(undo_count(&handle), 1);
}

fn assert_untouched(command: impl FnOnce(&Fixture) -> Command) {
    let f = fixture();
    let command = command(&f);
    let (sheet, session) = (f.sheet, Session::spawn(f.project, None).expect("spawn"));
    let handle = session.handle();
    handle.execute(command).expect("execute");
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert!(marked(&handle, sheet));
}

#[test]
fn typing_in_an_imported_column_renames_the_sheet_in_the_same_step() {
    let f = fixture();
    let (sheet, a) = (f.sheet, f.a);
    let session = Session::spawn(f.project, None).expect("spawn");
    let handle = session.handle();
    handle
        .execute(set(a, 1.0))
        .expect("a cell typed, even the same value");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
    assert!(!marked(&handle, sheet));
    let state = handle.snapshot().expect("snapshot").history;
    assert_eq!(
        state.undo_label,
        Some(HistoryLabel::SetCells {
            column: Some(stokesforge_commands::ColumnRef(0))
        })
    );
    handle.undo().expect("undo");
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert!(marked(&handle, sheet));
    assert!(!handle.snapshot().expect("snapshot").history.can_undo);
    handle.redo().expect("redo");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
}

#[test]
fn each_change_of_a_value_renames() {
    assert_renames(|f| Command::InsertRows {
        sheet: f.sheet,
        at: 1,
        count: 1,
    });
    assert_renames(|f| Command::DeleteRows {
        sheet: f.sheet,
        at: 0,
        count: 1,
    });
    assert_renames(|f| Command::SetColumnType {
        column: f.a,
        column_type: ColumnType::Mixed,
    });
    assert_renames(|f| Command::DeleteColumn { column: f.b });
    assert_renames(|f| {
        let mut meta = ColumnMeta {
            x_prime: Some(5.0),
            ..Default::default()
        };
        meta.import_source = Some(source(0));
        Command::SetColumnMeta { column: f.a, meta }
    });
}

#[test]
fn a_formula_put_on_an_imported_column_renames() {
    let f = fixture();
    let (sheet, a) = (f.sheet, f.a);
    let session = Session::spawn(f.project, None).expect("spawn");
    let handle = session.handle();
    let formula = handle
        .snapshot()
        .expect("snapshot")
        .tree
        .formula_from_letters(a, "B*2")
        .expect("formula");
    let mut meta = meta_of(&handle, a);
    meta.formula = Some(formula);
    handle
        .execute(Command::SetColumnMeta { column: a, meta })
        .expect("execute");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
}

#[test]
fn headers_are_not_a_change() {
    assert_untouched(|f| {
        let mut meta = ColumnMeta {
            long_name: "Absorbance".into(),
            unit: "a.u.".into(),
            comment: "note".into(),
            display_format: "0.00".into(),
            ..Default::default()
        };
        meta.import_source = Some(source(0));
        Command::SetColumnMeta { column: f.a, meta }
    });
}

#[test]
fn a_calculated_column_added_beside_is_not_a_change() {
    assert_untouched(|f| Command::AddColumn {
        sheet: f.sheet,
        index: None,
        name: Some("D".into()),
        column: Column::from_f64(&[0.0]),
    });
}

#[test]
fn writing_a_hand_made_column_is_not_a_change() {
    assert_untouched(|f| set(f.c, 0.5));
}

#[test]
fn a_column_added_without_a_source_then_written_is_not_a_change() {
    let f = fixture();
    let sheet = f.sheet;
    let session = Session::spawn(f.project, None).expect("spawn");
    let handle = session.handle();
    handle
        .execute(Command::AddColumn {
            sheet,
            index: None,
            name: Some("D".into()),
            column: Column::from_f64(&[0.0, 0.0]),
        })
        .expect("add");
    let d = handle
        .snapshot()
        .expect("snapshot")
        .tree
        .child_by_name(sheet, "D")
        .expect("lookup")
        .expect("D");
    handle.execute(set(d, 4.0)).expect("write");
    assert_eq!(name_of(&handle, sheet), "Raw Data");
}

#[test]
fn writes_inside_import_and_refresh_macros_never_rename() {
    for kind in [MacroKind::Import, MacroKind::RefreshFromSource] {
        let f = fixture();
        let (sheet, a) = (f.sheet, f.a);
        let session = Session::spawn(f.project, None).expect("spawn");
        let handle = session.handle();
        handle.begin_macro(kind).expect("begin");
        handle.execute(set(a, 99.0)).expect("write");
        handle.end_macro().expect("end");
        assert_eq!(name_of(&handle, sheet), "Raw Data", "{kind:?}");
        assert!(marked(&handle, sheet));
    }
}

#[test]
fn a_paste_macro_renames_once_and_is_one_entry() {
    let f = fixture();
    let (sheet, a, b) = (f.sheet, f.a, f.b);
    let session = Session::spawn(f.project, None).expect("spawn");
    let handle = session.handle();
    handle.begin_macro(MacroKind::PasteCells).expect("begin");
    handle.execute(set(a, 5.0)).expect("a");
    handle.execute(set(b, 6.0)).expect("b");
    handle.end_macro().expect("end");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
    assert_eq!(undo_count(&handle), 1);
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert!(marked(&handle, sheet));
}

#[test]
fn an_aborted_macro_gives_the_name_back() {
    let f = fixture();
    let (sheet, a) = (f.sheet, f.a);
    let session = Session::spawn(f.project, None).expect("spawn");
    let handle = session.handle();
    handle.begin_macro(MacroKind::Script).expect("begin");
    handle.execute(set(a, 5.0)).expect("a");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
    handle.abort_macro().expect("abort");
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert!(marked(&handle, sheet));
}

#[test]
fn a_sheet_renamed_by_the_user_is_never_renamed_again() {
    let f = fixture();
    let (sheet, a) = (f.sheet, f.a);
    let session = Session::spawn(f.project, None).expect("spawn");
    let handle = session.handle();
    handle
        .execute(Command::Rename {
            id: sheet,
            name: "Mine".into(),
        })
        .expect("rename");
    assert!(!marked(&handle, sheet));
    handle.execute(set(a, 5.0)).expect("write");
    assert_eq!(name_of(&handle, sheet), "Mine");
    // Undo of the user's rename gives the mark back with the name.
    handle.undo().expect("undo write");
    handle.undo().expect("undo rename");
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert!(marked(&handle, sheet));
}

#[test]
fn the_new_name_is_the_first_free_one_from_one() {
    let f = fixture();
    let (sheet, a) = (f.sheet, f.a);
    let mut project = f.project;
    let book = project.tree().parent(sheet).unwrap().unwrap();
    project.add_worksheet(book, Some("Sheet1")).unwrap();
    let session = Session::spawn(project, None).expect("spawn");
    let handle = session.handle();
    handle.execute(set(a, 5.0)).expect("write");
    assert_eq!(name_of(&handle, sheet), "Sheet2");
}

#[test]
fn a_kept_formula_waiting_for_the_new_name_binds_in_the_same_gesture() {
    let f = fixture();
    let (sheet, a) = (f.sheet, f.a);
    let mut project = f.project;
    let root = project.root();
    let other = project.add_workbook(root, Some("other")).unwrap();
    let other_sheet = project.tree().children(other).unwrap()[0];
    let z = project
        .add_column(other_sheet, None, Some("Z"), Column::from_f64(&[0.0]))
        .unwrap();
    let (formula, error) = project.tree().formula_keeping(z, "max([measure]Sheet1!A)");
    assert!(
        error.is_some(),
        "Sheet1 does not exist yet: the formula is kept"
    );
    let mut meta = project
        .tree()
        .node(z)
        .and_then(|n| n.as_column())
        .unwrap()
        .meta()
        .clone();
    meta.formula = Some(formula);
    let kept = meta.formula.clone();
    project.set_column_meta(z, meta).unwrap();
    assert!(project.tree().pending_bindings().is_empty());

    let session = Session::spawn(project, None).expect("spawn");
    let handle = session.handle();
    handle.execute(set(a, 5.0)).expect("write");
    assert_eq!(name_of(&handle, sheet), "Sheet1");
    assert_ne!(meta_of(&handle, z).formula, kept);
    let tree = handle.snapshot().expect("snapshot").tree;
    assert!(
        tree.pending_bindings().is_empty(),
        "bound in the same gesture"
    );
    assert!(tree.formula_letters(z).expect("letters").contains("Sheet1"));
    handle.undo().expect("one undo");
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert_eq!(
        meta_of(&handle, z).formula,
        kept,
        "the undo unbinds with the rename"
    );
    assert!(!handle.snapshot().expect("snapshot").history.can_undo);
}

/// Stop on the recalculation that follows the first change undoes the command
/// like Ctrl+Z: the whole entry goes, the rename with it.
#[test]
fn stopping_the_recalculation_gives_raw_data_back() {
    let f = fixture();
    let (sheet, a) = (f.sheet, f.a);
    let mut project = f.project;
    // A long chain of formula columns downstream of A, heavy enough for Stop
    // to catch the pass in flight.
    let rows = vec![1.0; 300_000];
    project
        .set_values(a, 0, ValueBlock::Float(rows))
        .expect("long column");
    let mut previous = "A".to_string();
    for k in 0..14 {
        let name = format!("F{k}");
        let id = project
            .add_column(sheet, None, Some(&name), Column::new(ColumnType::Numeric))
            .expect("formula column");
        let formula = project
            .tree()
            .formula_from_letters(id, &format!("{previous}+1"))
            .expect("formula");
        let mut meta = ColumnMeta::clone(
            project
                .tree()
                .node(id)
                .and_then(|n| n.as_column())
                .unwrap()
                .meta(),
        );
        meta.formula = Some(formula);
        project.set_column_meta(id, meta).expect("meta");
        previous = project.tree().column_letters(id).expect("letters");
    }
    let session = Session::spawn(project, None).expect("spawn");
    let handle = session.handle();
    handle
        .execute_interruptible(set(a, 5.0))
        .expect("applied before the recalculation");
    let id = handle.recalc_running().expect("the pass is running");
    assert!(handle.stop_recalc(id), "stopped in flight");
    assert!(handle.wait_recalc(std::time::Duration::from_secs(60)));
    assert_eq!(name_of(&handle, sheet), "Raw Data");
    assert!(marked(&handle, sheet));
    assert!(!handle.snapshot().expect("snapshot").history.can_undo);
}

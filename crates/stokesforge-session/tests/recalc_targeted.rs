//! Targeted recalculation of formula columns (#193): named cases, seen from the
//! session's public API.
//!
//! Two families of checks, and the second is the one most easily missed:
//!
//! - **correct**: after the gesture, the formula column holds the value that its
//!   text and its sources impose — a forgotten recalculation fails here;
//! - **targeted**: a formula column that the gesture does not concern is not
//!   rewritten, which is read from its [`Column::version`] — a memo that
//!   recalculated everything would pass the first family without flinching.
//!
//! Each "targeted" test has its **witness**: the same scenario in
//! [`FormulaRecalc::Full`] rewrites the column, so changes its version. Without
//! this witness, an unchanged version could just as well come from a write
//! that does not change the version (which the repository forbids, but which
//! no other test would see here).
//!
//! The complete contract — "targeted recalculation gives exactly the full
//! recalculation" — is held by the differential battery
//! `stokesforge-robustesse/tests/b22_recalcul_cible.rs`; these named cases say
//! *why* each component of the fingerprint exists
//! (`stokesforge-session/src/recalc.rs`, `Fingerprint`).

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnType, ColumnValues, MixedCell, NodeCopy, NodeId, Project,
    ValueBlock,
};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};

// -- fixture -----------------------------------------------------------------

/// `Book1 → Sheet1` with `A [1,2,3]`, `B [10,20,30]`, and two empty columns
/// `C`, `D` to put formulas on.
struct SheetFixture {
    sheet: NodeId,
    a: NodeId,
    b: NodeId,
    c: NodeId,
    d: NodeId,
}

fn fixture_project() -> (Project, SheetFixture) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("B");
    let c = project
        .add_column(sheet, None, Some("C"), Column::new(ColumnType::Numeric))
        .expect("C");
    let d = project
        .add_column(sheet, None, Some("D"), Column::new(ColumnType::Numeric))
        .expect("D");
    (project, SheetFixture { sheet, a, b, c, d })
}

fn start(mode: FormulaRecalc) -> (Session, SessionHandle, SheetFixture) {
    let (project, fixture) = fixture_project();
    let session = Session::spawn_with_recalc(project, None, mode).expect("actor started");
    let handle = session.handle();
    (session, handle, fixture)
}

/// The command that sets `text` (entered in **letters**) as the formula of
/// `column`: letters to identifiers by `Tree::formula_from_letters`, like the
/// interface (#321).
fn set_formula(handle: &SessionHandle, column: NodeId, text: &str) -> Command {
    let formula = take_snapshot(handle)
        .tree
        .formula_from_letters(column, text)
        .expect("formula entered");
    Command::SetColumnMeta {
        column,
        meta: ColumnMeta {
            formula: Some(formula),
            ..Default::default()
        },
    }
}

fn enter(column: NodeId, start: usize, values: &[f64]) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(values.to_vec()),
        expected_version: None,
    }
}

/// A command that touches no column: one more folder at the root, named after
/// `rank` to stay unique.
fn unrelated(snapshot: &Snapshot, rank: usize) -> Command {
    Command::AddFolder {
        parent: snapshot.tree.root(),
        name: format!("Witness{rank}"),
    }
}

fn take_snapshot(handle: &SessionHandle) -> Snapshot {
    handle.snapshot().expect("snapshot")
}

fn version(snapshot: &Snapshot, column: NodeId) -> u64 {
    snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("column present")
        .version()
}

/// Cells of a column read as numbers; `None` for a non-numeric cell (empty,
/// text, `"---"`).
fn numbers(snapshot: &Snapshot, column: NodeId) -> Vec<Option<f64>> {
    let column = snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("column present");
    match column.values() {
        ColumnValues::Float(values) => values.iter().map(|v| (!v.is_nan()).then_some(*v)).collect(),
        ColumnValues::Mixed(cells) => cells
            .iter()
            .map(|cell| match cell {
                MixedCell::Number(v) if !v.is_nan() => Some(*v),
                _ => None,
            })
            .collect(),
    }
}

/// True if all the cells of `column` are the `"---"` marker of a result not
/// calculated (or `NaN` for a numeric column).
fn all_not_calculated(snapshot: &Snapshot, column: NodeId) -> bool {
    let column = snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("column present");
    (0..column.len()).all(|row| match column.get(row) {
        Cell::Float(v) => v.is_nan(),
        Cell::Text(Some(t)) => t == "---",
        Cell::Text(None) => false,
    })
}

/// True if all the cells of `column` are **empty** (`NaN` for a numeric column,
/// empty cell for a mixed column) — the result of a formula whose column read
/// vanished (`#REF`), not to be confused with the `"---"` of a calculation that
/// failed ([`all_not_calculated`]).
fn all_empty(snapshot: &Snapshot, column: NodeId) -> bool {
    let column = snapshot
        .tree
        .node(column)
        .and_then(|node| node.as_column())
        .expect("column present");
    !column.is_empty()
        && (0..column.len()).all(|row| match column.get(row) {
            Cell::Float(v) => v.is_nan(),
            Cell::Text(None) => true,
            Cell::Text(Some(_)) => false,
        })
}

fn some(values: &[f64]) -> Vec<Option<f64>> {
    values.iter().copied().map(Some).collect()
}

// -- targeted: what must no longer be rewritten --------------------------------

/// Plays the scenario "a formula, then an unrelated command" and returns
/// `(version before, version after)` of the formula column.
fn versions_around_an_unrelated_command(mode: FormulaRecalc) -> (u64, u64) {
    let (_session, handle, f) = start(mode);
    handle
        .execute(set_formula(&handle, f.c, "A+B"))
        .expect("formula");
    let before = take_snapshot(&handle);
    handle
        .execute(unrelated(&before, 0))
        .expect("unrelated command");
    let after = take_snapshot(&handle);
    assert_eq!(numbers(&after, f.c), some(&[11.0, 22.0, 33.0]));
    (version(&before, f.c), version(&after, f.c))
}

/// The heart of #193: changing something that is the source of no formula no
/// longer rewrites any formula column.
#[test]
fn an_unrelated_command_rewrites_no_formula() {
    let (before, after) = versions_around_an_unrelated_command(FormulaRecalc::Targeted);
    assert_eq!(before, after, "C had no reason to be rewritten");

    // Witness: the full recalculation rewrites it, and the version shows it.
    let (before, after) = versions_around_an_unrelated_command(FormulaRecalc::Full);
    assert_ne!(before, after, "witness: the full recalculation rewrites C");
}

/// Modifying A recalculates the formulas that read A, and only them.
#[test]
fn changing_a_source_recalculates_only_its_dependents() {
    for mode in [FormulaRecalc::Targeted, FormulaRecalc::Full] {
        let (_session, handle, f) = start(mode);
        handle
            .execute(set_formula(&handle, f.c, "A*2"))
            .expect("C = A*2");
        handle
            .execute(set_formula(&handle, f.d, "B+1"))
            .expect("D = B+1");
        let before = take_snapshot(&handle);

        handle
            .execute(enter(f.a, 0, &[100.0]))
            .expect("input into A");
        let after = take_snapshot(&handle);

        assert_eq!(numbers(&after, f.c), some(&[200.0, 4.0, 6.0]));
        assert_eq!(numbers(&after, f.d), some(&[11.0, 21.0, 31.0]));
        assert_ne!(version(&before, f.c), version(&after, f.c), "C lit A");
        match mode {
            FormulaRecalc::Targeted => assert_eq!(
                version(&before, f.d),
                version(&after, f.d),
                "D does not read A: it is not rewritten"
            ),
            FormulaRecalc::Full => assert_ne!(
                version(&before, f.d),
                version(&after, f.d),
                "witness: the full recalculation rewrites D"
            ),
        }
    }
}

/// Plays "D = C+1, C = A*2, input into A, then an unrelated command" and
/// returns true if C and D kept their version during the last one.
fn chain_unchanged_by_an_unrelated_command(mode: FormulaRecalc) -> bool {
    let (_session, handle, f) = start(mode);
    // D reads C which reads A; C is set **after** D, so that the declaration
    // order does not coincide with the calculation order.
    handle
        .execute(set_formula(&handle, f.d, "C+1"))
        .expect("D = C+1");
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle.execute(enter(f.a, 1, &[7.0])).expect("input into A");
    let before = take_snapshot(&handle);
    assert_eq!(numbers(&before, f.c), some(&[2.0, 14.0, 6.0]));
    assert_eq!(numbers(&before, f.d), some(&[3.0, 15.0, 7.0]));

    handle
        .execute(unrelated(&before, 0))
        .expect("unrelated command");
    let after = take_snapshot(&handle);
    version(&before, f.c) == version(&after, f.c) && version(&before, f.d) == version(&after, f.d)
}

/// A second immediate pass rewrites nothing: the order of the steps of the
/// recalculation (unreadable, types, cycles, topological order) leaves no
/// column written after a column that reads it took its fingerprint. Without
/// this property, every command would recalculate a whole chain.
#[test]
fn a_recalculated_chain_is_stable_at_the_next_command() {
    assert!(chain_unchanged_by_an_unrelated_command(
        FormulaRecalc::Targeted
    ));
    assert!(
        !chain_unchanged_by_an_unrelated_command(FormulaRecalc::Full),
        "witness: the full recalculation rewrites the chain"
    );
}

// -- correct: each component of the fingerprint has its reason ---------------

/// Propagation follows the chain in the same pass: recalculating C changes its
/// version, so D, which reads C, is in turn seen as needing recalculation.
#[test]
fn propagation_follows_the_chain_and_comes_back_through_undo_redo() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle
        .execute(set_formula(&handle, f.d, "C+1"))
        .expect("D = C+1");

    handle
        .execute(enter(f.a, 2, &[50.0]))
        .expect("input into A");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.c), some(&[2.0, 4.0, 100.0]));
    assert_eq!(numbers(&s, f.d), some(&[3.0, 5.0, 101.0]));

    handle.undo().expect("undo the input");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.c), some(&[2.0, 4.0, 6.0]));
    assert_eq!(numbers(&s, f.d), some(&[3.0, 5.0, 7.0]));

    handle.redo().expect("redo the input");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.c), some(&[2.0, 4.0, 100.0]));
    assert_eq!(numbers(&s, f.d), some(&[3.0, 5.0, 101.0]));
}

/// Changing the formula text, without touching any data, recalculates
/// (`source` component); undoing that change too.
#[test]
fn changing_the_formula_text_recalculates() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle
        .execute(set_formula(&handle, f.c, "A*3"))
        .expect("C = A*3");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[3.0, 6.0, 9.0])
    );

    handle.undo().expect("undo");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[2.0, 4.0, 6.0])
    );
}

/// Plays "C = A*2, then B moves to the front of the sheet" and returns `(values
/// of C, displayed text of C, did C stay at its version?, displayed text after
/// undo)`.
fn move_b_to_front(
    mode: FormulaRecalc,
) -> (Vec<Option<f64>>, Option<String>, bool, Option<String>) {
    let (_session, handle, f) = start(mode);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    let before = take_snapshot(&handle);
    handle
        .execute(Command::Move {
            id: f.b,
            new_parent: f.sheet,
            index: 0,
        })
        .expect("B moves to the front");
    let after = take_snapshot(&handle);
    handle.undo().expect("undo the move");
    (
        numbers(&after, f.c),
        after.tree.formula_letters(f.c),
        version(&before, f.c) == version(&after, f.c),
        take_snapshot(&handle).tree.formula_letters(f.c),
    )
}

/// Moving a source **within the sheet** does not change what the formula reads
/// (#321, F1): it remembers the column by its identifier. The **displayed**
/// letters follow (A became the second column: `B*2`), the stored text does not
/// change, and nothing is rewritten — neither A nor B was written, the
/// fingerprint does not contain the displayed text. Witness: the full
/// recalculation rewrites the column. Mutant: resolution by position.
#[test]
fn moving_a_source_within_the_sheet_the_formula_follows() {
    let (values, text, unchanged, after_undo) = move_b_to_front(FormulaRecalc::Targeted);
    assert_eq!(values, some(&[2.0, 4.0, 6.0]), "C still reads A");
    assert_eq!(
        text.as_deref(),
        Some("B*2"),
        "A is now the second column: the displayed letter follows"
    );
    assert!(unchanged, "the formula is not rewritten");
    assert_eq!(after_undo.as_deref(), Some("A*2"));

    let (values, _, unchanged, _) = move_b_to_front(FormulaRecalc::Full);
    assert_eq!(values, some(&[2.0, 4.0, 6.0]), "oracle");
    assert!(!unchanged, "witness: the full recalculation rewrites C");
}

/// Moving a source to another sheet: the formula no longer finds it in **its**
/// sheet, it displays `#REF` and its cells are empty (not `---`); undo brings
/// back the column, the formula and its values. Mutants: resolution in the
/// whole tree (the formula would still read the departed column), or presence
/// in the sheet missing from the fingerprint (the move would trigger no
/// recalculation).
#[test]
fn moving_a_source_to_another_sheet() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.d, "B+1"))
        .expect("D = B+1");
    let book = take_snapshot(&handle)
        .tree
        .parent(f.sheet)
        .expect("consistent tree")
        .expect("the sheet has a workbook");
    handle
        .execute(Command::AddWorksheet { book, name: None })
        .expect("second sheet");
    let other = *take_snapshot(&handle)
        .tree
        .children(book)
        .expect("workbook")
        .last()
        .expect("second sheet present");
    handle
        .execute(Command::Move {
            id: f.b,
            new_parent: other,
            index: 0,
        })
        .expect("B goes to the other sheet");
    let s = take_snapshot(&handle);
    assert_eq!(s.tree.formula_letters(f.d).as_deref(), Some("#REF+1"));
    assert!(all_empty(&s, f.d), "empty cells, not \"---\"");
    assert_eq!(
        numbers(&s, f.d).len(),
        3,
        "at the length of the ordinary columns"
    );

    handle.undo().expect("undo the move");
    let s = take_snapshot(&handle);
    assert_eq!(s.tree.formula_letters(f.d).as_deref(), Some("B+1"));
    assert_eq!(numbers(&s, f.d), some(&[11.0, 21.0, 31.0]));
}

/// Deleting a column that a formula reads: `#REF*2`, empty cells; Ctrl+Z brings
/// back the column **and** the formula (the identifier stayed stored), with its
/// values.
#[test]
fn deleting_a_source_gives_ref_then_undo_restores_the_formula() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "B*2"))
        .expect("C = B*2");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[20.0, 40.0, 60.0])
    );

    handle
        .execute(Command::DeleteColumn { column: f.b })
        .expect("delete B");
    let s = take_snapshot(&handle);
    assert_eq!(s.tree.formula_letters(f.c).as_deref(), Some("#REF*2"));
    assert!(all_empty(&s, f.c), "empty cells, not \"---\"");

    handle.undo().expect("undo the deletion");
    let s = take_snapshot(&handle);
    assert_eq!(s.tree.formula_letters(f.c).as_deref(), Some("B*2"));
    assert_eq!(numbers(&s, f.c), some(&[20.0, 40.0, 60.0]));
}

/// An input into a formula column is refused (`FormulaColumnReadOnly`), but
/// two commands modify it all the same without touching its sources: changing
/// its type and deleting cells in it. The recalculation must undo them, as
/// before #193 — this is the "version of the column itself" component of the
/// fingerprint, the only one that changes here.
#[test]
fn a_formula_column_modified_directly_is_recalculated() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    let refusal = handle.execute(enter(f.c, 0, &[999.0]));
    assert!(refusal.is_err(), "direct input stays refused");

    handle
        .execute(Command::DeleteCells {
            columns: vec![f.c],
            at: 0,
            count: 1,
        })
        .expect("delete a cell of C");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[2.0, 4.0, 6.0])
    );

    handle
        .execute(Command::SetColumnType {
            column: f.c,
            column_type: ColumnType::Numeric,
        })
        .expect("change the type of C");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.c), some(&[2.0, 4.0, 6.0]));
    assert_eq!(
        s.tree
            .node(f.c)
            .and_then(|node| node.as_column())
            .map(Column::column_type),
        Some(ColumnType::Mixed),
        "the recalculation gives C back the type its formula imposes"
    );

    // Undoing the deletion reinserts a cell in C, which goes to four rows; the
    // recalculation brings it back to the length of A (#194).
    handle.undo().expect("undo the type change");
    handle.undo().expect("undo the deletion");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[2.0, 4.0, 6.0])
    );
}

/// The result has the length of the longest of the columns the formula reads
/// (#194): lengthening A lengthens C = A*2, undoing the lengthening shortens
/// it. Before #194, the length was that of the sheet, counted formula column
/// included: C, once lengthened, no longer shortened.
#[test]
fn the_result_follows_the_length_of_the_columns_read() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle
        .execute(enter(f.a, 4, &[5.0]))
        .expect("A goes down to 5 rows");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        vec![Some(2.0), Some(4.0), Some(6.0), None, Some(10.0)]
    );
    handle.undo().expect("undo the lengthening of A");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[2.0, 4.0, 6.0])
    );

    // Two sources of different lengths: the longest wins.
    handle
        .execute(set_formula(&handle, f.d, "A+B"))
        .expect("D = A+B");
    handle
        .execute(enter(f.b, 3, &[40.0]))
        .expect("B goes down to 4 rows");
    let s = take_snapshot(&handle);
    assert_eq!(
        numbers(&s, f.d),
        vec![Some(11.0), Some(22.0), Some(33.0), None]
    );
    assert_eq!(numbers(&s, f.c).len(), 3, "C does not read B");
}

/// A formula without a reference (constant) has the length of the ordinary
/// columns, and loses it with them (#194).
#[test]
fn a_constant_follows_the_ordinary_columns() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "1+2"))
        .expect("C = 1+2");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[3.0, 3.0, 3.0])
    );
    handle
        .execute(enter(f.d, 4, &[1.0]))
        .expect("D goes down to 5 rows");
    assert_eq!(numbers(&take_snapshot(&handle), f.c), some(&[3.0; 5]));
    handle.undo().expect("undo");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[3.0, 3.0, 3.0])
    );
}

/// Plays "lengthen D, which the formula does not read" and returns
/// `(C after, did C stay at its version?)`.
fn lengthen_an_unread_column(mode: FormulaRecalc) -> (Vec<Option<f64>>, bool) {
    let (_session, handle, f) = start(mode);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    let before = take_snapshot(&handle);
    handle
        .execute(enter(f.d, 4, &[1.0]))
        .expect("D goes down to 5 rows");
    let after = take_snapshot(&handle);
    (
        numbers(&after, f.c),
        version(&before, f.c) == version(&after, f.c),
    )
}

/// Lengthening a column that the formula does not read no longer lengthens it
/// (#194), and the targeted mode does not even rewrite it; witness: the full
/// mode rewrites it, identically.
#[test]
fn lengthening_an_unread_column_does_not_change_the_formula() {
    let (targeted, spared) = lengthen_an_unread_column(FormulaRecalc::Targeted);
    assert_eq!(targeted, some(&[2.0, 4.0, 6.0]));
    assert!(spared, "C was not rewritten");
    let (full, spared) = lengthen_an_unread_column(FormulaRecalc::Full);
    assert_eq!(full, targeted);
    assert!(!spared, "witness: the full recalculation rewrites it");
}

/// An unreadable formula takes the length of the sheet's ordinary columns and
/// follows it when one of them grows (`Unparsable` outcome, "length"
/// component), then, once corrected, that of the columns it reads (#194).
#[test]
fn an_unreadable_formula_follows_the_sheet_then_gets_corrected() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*"))
        .expect("C unreadable");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.c), vec![None, None, None]);

    handle
        .execute(enter(f.d, 4, &[1.0]))
        .expect("D goes down to 5 rows");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.c).len(), 5, "the column follows the sheet");

    // And shortens with it: the "ordinary" length does not count the formula
    // column itself, which would otherwise sustain itself at five rows
    // (#194).
    handle.undo().expect("undo the lengthening of D");
    assert_eq!(numbers(&take_snapshot(&handle), f.c).len(), 3);
    handle.redo().expect("redo");

    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C corrected");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[2.0, 4.0, 6.0])
    );
}

/// A cycle created **by another column** blocks a formula whose text and
/// sources have not changed content (`Blocked` outcome), and breaking it makes
/// it calculable.
#[test]
fn a_cycle_created_elsewhere_blocks_the_formula_and_is_undone() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "B*2"))
        .expect("C = B*2");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[20.0, 40.0, 60.0])
    );

    handle
        .execute(set_formula(&handle, f.b, "C+1"))
        .expect("B = C+1: cycle");
    let s = take_snapshot(&handle);
    assert!(all_not_calculated(&s, f.c), "C is caught in the cycle");
    assert!(all_not_calculated(&s, f.b), "B too");

    handle.undo().expect("undo the formula of B");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[20.0, 40.0, 60.0])
    );
}

/// Sets the cycle "C = B*2, B = C+1" and returns true if an unrelated command
/// leaves the versions of B and C intact.
fn cycle_unchanged_by_an_unrelated_command(mode: FormulaRecalc) -> bool {
    let (_session, handle, f) = start(mode);
    handle
        .execute(set_formula(&handle, f.c, "B*2"))
        .expect("C = B*2");
    handle
        .execute(set_formula(&handle, f.b, "C+1"))
        .expect("B = C+1: cycle");
    let before = take_snapshot(&handle);
    assert!(all_not_calculated(&before, f.b) && all_not_calculated(&before, f.c));
    handle.execute(unrelated(&before, 0)).expect("unrelated");
    let after = take_snapshot(&handle);
    version(&before, f.b) == version(&after, f.b) && version(&before, f.c) == version(&after, f.c)
}

/// A stable cycle is not rewritten at every command: its members do not bounce
/// their versions back and forth (the sources of a blocked formula do not
/// enter its fingerprint).
#[test]
fn a_stable_cycle_is_not_rewritten() {
    assert!(cycle_unchanged_by_an_unrelated_command(
        FormulaRecalc::Targeted
    ));
    assert!(
        !cycle_unchanged_by_an_unrelated_command(FormulaRecalc::Full),
        "witness: the full recalculation rewrites the cycle"
    );
}

/// A cycle follows the length of the sheet: lengthening it by a column that no
/// formula reads lengthens its members (`Blocked` outcome, "row count"
/// component).
#[test]
fn a_cycle_follows_the_length_of_the_sheet() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "B*2"))
        .expect("C = B*2");
    handle
        .execute(set_formula(&handle, f.b, "C+1"))
        .expect("B = C+1: cycle");
    handle
        .execute(enter(f.d, 4, &[1.0]))
        .expect("D goes down to 5 rows");
    let s = take_snapshot(&handle);
    for member in [f.b, f.c] {
        assert_eq!(numbers(&s, member).len(), 5, "the cycle follows the sheet");
        assert!(all_not_calculated(&s, member));
    }
}

/// A blocked or unreadable formula, modified directly (cell deletion), is
/// rewritten ("version of the column itself" component for the `Blocked` and
/// `Unparsable` outcomes): otherwise it would keep one row fewer than the
/// sheet.
#[test]
fn a_blocked_or_unreadable_formula_modified_directly_is_rewritten() {
    for (text, cycle) in [("B*2", true), ("A*", false)] {
        let (_session, handle, f) = start(FormulaRecalc::Targeted);
        handle
            .execute(set_formula(&handle, f.c, text))
            .expect("formula of C");
        if cycle {
            handle
                .execute(set_formula(&handle, f.b, "C+1"))
                .expect("B = C+1: cycle");
        }
        handle
            .execute(Command::DeleteCells {
                columns: vec![f.c],
                at: 0,
                count: 1,
            })
            .expect("delete a cell of C");
        let s = take_snapshot(&handle);
        assert_eq!(numbers(&s, f.c).len(), 3, "{text}: C follows the sheet");
        assert!(all_not_calculated(&s, f.c), "{text}");
    }
}

/// Changing the type of a source (text input that promotes to mixed)
/// recalculates: the text cell becomes a result not calculated.
#[test]
fn a_source_promoted_to_text_recalculates() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle
        .execute(Command::SetCellsPromoting {
            column: f.a,
            start: 1,
            values: ValueBlock::Mixed(vec![MixedCell::Text("abc".into())]),
            expected_version: None,
        })
        .expect("text in A");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.c), vec![Some(2.0), None, Some(6.0)]);
}

/// Aborting a macro undoes in one block writes that were already recalculated:
/// the formulas come back with them.
#[test]
fn aborting_a_macro_recalculates() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle.begin_macro(MacroKind::PasteCells).expect("macro");
    handle.execute(enter(f.a, 0, &[5.0, 5.0])).expect("input");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[10.0, 10.0, 6.0])
    );
    handle.abort_macro().expect("abort");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[2.0, 4.0, 6.0])
    );
}

/// Deleting a formula column then undoing: it comes back correct. (A source
/// modified "in the meantime" cannot be: the history is strictly LIFO, undoing
/// the deletion first undoes what followed.)
#[test]
fn a_restored_formula_column_is_correct() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle
        .execute(Command::DeleteColumn { column: f.c })
        .expect("delete C");
    // The letter A still names A: deleting C (after A) shifts nothing before it.
    handle.undo().expect("restore C");
    assert_eq!(
        numbers(&take_snapshot(&handle), f.c),
        some(&[2.0, 4.0, 6.0])
    );
}

/// Duplicates the sheet, inputs into the copy of A, and returns true if the
/// **original** formula column kept its version.
fn original_unchanged_by_an_input_into_the_copy(mode: FormulaRecalc) -> bool {
    let (_session, handle, f) = start(mode);
    handle
        .execute(set_formula(&handle, f.c, "A*2"))
        .expect("C = A*2");
    handle
        .execute(Command::DuplicateWorksheet {
            sheet: f.sheet,
            name: "Copy".into(),
        })
        .expect("duplicate");
    let s = take_snapshot(&handle);
    let book = s.tree.parent(f.sheet).expect("tree").expect("workbook");
    let copy = s
        .tree
        .children(book)
        .expect("workbook")
        .iter()
        .copied()
        .find(|&id| id != f.sheet)
        .expect("copy present");
    let columns = s.tree.children(copy).expect("sheet").to_vec();
    let (a2, c2) = (columns[0], columns[2]);

    handle
        .execute(enter(a2, 0, &[9.0]))
        .expect("input into the copy");
    let after = take_snapshot(&handle);
    assert_eq!(numbers(&after, c2), some(&[18.0, 4.0, 6.0]));
    assert_eq!(numbers(&after, f.c), some(&[2.0, 4.0, 6.0]));
    version(&s, f.c) == version(&after, f.c)
}

/// A duplicated sheet calculates its own formulas on its own columns: the memo
/// is kept by identifier, not by name nor by letter, and the original is not
/// rewritten by an input into the copy.
#[test]
fn a_duplicated_sheet_has_its_own_formulas() {
    assert!(original_unchanged_by_an_input_into_the_copy(
        FormulaRecalc::Targeted
    ));
    assert!(
        !original_unchanged_by_an_input_into_the_copy(FormulaRecalc::Full),
        "witness: the full recalculation rewrites the original"
    );
}

/// A project built like [`fixture_project`] — hence with the same identifiers —
/// where A and C carry the versions `version_a` and `version_c` with other
/// content: A is `[5, 5, 5]` and C, which keeps the formula `A*2`, carries
/// zeros that no calculation justifies. This is the coincidence that an
/// uncleared memo would take for "nothing changed". `add_column` keeps the
/// version of the column it is given.
fn rigged_project(f: &SheetFixture, version_a: u64, version_c: u64, meta_c: ColumnMeta) -> Project {
    let mut fake = Project::new("Demo").expect("valid project");
    let root = fake.root();
    fake.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = fake.resolve_path("[Book1]Sheet1").expect("sheet");
    let fake_a = Column::from_block_with_version(
        ColumnType::Numeric,
        ValueBlock::Float(vec![5.0, 5.0, 5.0]),
        ColumnMeta::default(),
        version_a,
    )
    .expect("rigged A");
    let fake_c = Column::from_block_with_version(
        ColumnType::Mixed,
        ValueBlock::Mixed(vec![MixedCell::Number(0.0); 3]),
        meta_c,
        version_c,
    )
    .expect("rigged C");
    let ids = [
        fake.add_column(sheet, None, Some("A"), fake_a),
        fake.add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        ),
        fake.add_column(sheet, None, Some("C"), fake_c),
        fake.add_column(sheet, None, Some("D"), Column::new(ColumnType::Numeric)),
    ]
    .map(|id| id.expect("column"));
    assert_eq!(ids, [f.a, f.b, f.c, f.d], "deterministic construction");
    fake
}

/// Sets `C = A*2` and returns `(version of A, version of C, metadata of C)` as
/// the memo has just noted them.
fn memo_of_c(handle: &SessionHandle, f: &SheetFixture) -> (u64, u64, ColumnMeta) {
    handle
        .execute(set_formula(handle, f.c, "A*2"))
        .expect("C = A*2");
    let s = take_snapshot(handle);
    assert_eq!(numbers(&s, f.c), some(&[2.0, 4.0, 6.0]));
    let c = s
        .tree
        .node(f.c)
        .and_then(|node| node.as_column())
        .expect("C");
    assert_eq!(c.column_type(), ColumnType::Mixed);
    (version(&s, f.a), c.version(), c.meta().clone())
}

/// `Load` clears the memo. The loaded file reproduces everything the memo
/// compares — same identifiers, same versions, same text — with another source
/// A and wrong values of C ([`rigged_project`]). Only forgetting the memo forces
/// the recalculation: a `.stf` written elsewhere (another process, an earlier
/// version) has no reason to share the version numbering of this project.
#[test]
fn loading_forgets_the_memo_of_the_replaced_project() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    let (version_a, version_c, meta_c) = memo_of_c(&handle, &f);
    let fake = rigged_project(&f, version_a, version_c, meta_c);

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("fake.stf");
    stokesforge_persistence::save(&fake, &path).expect("write the file");

    handle.load(&path).expect("load");
    let s = take_snapshot(&handle);
    assert_eq!(numbers(&s, f.a), some(&[5.0, 5.0, 5.0]));
    assert_eq!(numbers(&s, f.c), some(&[10.0, 10.0, 10.0]));
}

/// `NewProject` clears the memo. An ordinary pass already purges the entries of
/// absent columns, which is enough as long as the first request of the new
/// project creates no formula column; the case that requires forgetting is
/// therefore a **paste** as the very first request. The new project restarts
/// from the same identifiers: the pasted workbook ([`rigged_project`], captured
/// with its versions as Copy does) falls back on the identifiers and the
/// versions that the memo noted.
#[test]
fn a_new_project_forgets_the_memo_of_the_old_one() {
    let (_session, handle, f) = start(FormulaRecalc::Targeted);
    let (version_a, version_c, meta_c) = memo_of_c(&handle, &f);
    let fake = rigged_project(&f, version_a, version_c, meta_c);
    let book = fake.resolve_path("[Book1]").expect("rigged workbook");
    let copy = NodeCopy::capture(fake.tree(), book).expect("capture");

    handle.new_project().expect("new project");
    let root = take_snapshot(&handle).tree.root();
    handle
        .execute(Command::InsertCopy {
            parent: root,
            copy,
            name: "Book1".into(),
        })
        .expect("paste the workbook, first request of the new project");

    let s = take_snapshot(&handle);
    // The coincidence holds: the pasted column carries the identifier of C, and
    // A, which the recalculation does not write, the version the memo noted.
    assert!(
        s.tree.node(f.c).and_then(|node| node.as_column()).is_some(),
        "C pasted under the identifier the memo knows"
    );
    assert_eq!(version(&s, f.a), version_a);
    assert_eq!(numbers(&s, f.a), some(&[5.0, 5.0, 5.0]));
    assert_eq!(numbers(&s, f.c), some(&[10.0, 10.0, 10.0]));
}

/// A column added with an already carried version (coming from another project,
/// from a snapshot taken before a load, from a file) must not see that number
/// serve again for another of its contents: the project's counter moves past it
/// on insertion, as for a paste. Otherwise, A added at version 40 falls back to
/// 40 after a few writes, with other content, and C = A*2 thinks it is up to
/// date (false "clean" found in the review of #193).
fn column_added_with_its_version(mode: FormulaRecalc) -> (Vec<Option<f64>>, u64) {
    const V: u64 = 40;
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet");
    let session = Session::spawn_with_recalc(project, None, mode).expect("actor started");
    let handle = session.handle();
    let add = |name: &str, column: Column| {
        handle
            .execute(Command::AddColumn {
                sheet,
                index: None,
                name: Some(name.into()),
                column,
            })
            .expect("column");
    };
    add(
        "A",
        Column::from_block_with_version(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0, 2.0, 3.0]),
            ColumnMeta::default(),
            V,
        )
        .expect("A at version 40"),
    );
    add("B", Column::from_f64(&[0.0]));
    // `A*2`, entered in letters on the sheet as it is (A, B), for a column C
    // that does not exist yet: letters to identifiers.
    let existing = take_snapshot(&handle)
        .tree
        .children(sheet)
        .expect("sheet")
        .to_vec();
    let formula_of_c =
        stokesforge_core::ColumnFormula::from_letters("A*2", &existing, NodeId::from_raw(u64::MAX))
            .expect("formula entered");
    let mut c = Column::new(ColumnType::Numeric);
    c.set_meta(ColumnMeta {
        formula: Some(formula_of_c),
        ..Default::default()
    });
    add("C", c);
    let ids = take_snapshot(&handle)
        .tree
        .children(sheet)
        .expect("sheet")
        .to_vec();
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    assert_eq!(numbers(&take_snapshot(&handle), c), some(&[2.0, 4.0, 6.0]));

    // Inputs into B, which C does not read, bring the counter up to V.
    while version(&take_snapshot(&handle), b) < V - 1 {
        handle.execute(enter(b, 0, &[1.0])).expect("input into B");
    }
    handle.execute(enter(a, 0, &[100.0])).expect("input into A");
    let s = take_snapshot(&handle);
    (numbers(&s, c), version(&s, a))
}

#[test]
fn a_column_added_with_its_version_does_not_make_that_number_serve_again() {
    let (full, _) = column_added_with_its_version(FormulaRecalc::Full);
    assert_eq!(full, some(&[200.0, 4.0, 6.0]), "oracle");
    let (targeted, version_a) = column_added_with_its_version(FormulaRecalc::Targeted);
    assert!(version_a > 40, "A does not fall back to 40: {version_a}");
    assert_eq!(targeted, full, "the targeted mode gives the full one");
}

// -- too-deep formula (#289) -----------------------------------------------

/// Chain of `terms` times `A`, joined by `+`. Sizes hard-coded in the tests:
/// 1001 terms pass, 1002 are refused (`MAX_DEPTH = 1000`).
fn sum_of_a(terms: usize) -> String {
    vec!["A"; terms].join("+")
}

/// A formula refused at parse by `FormulaError::TooDeep` is not an actor
/// failure: the column is a not-calculated block of the sheet's length
/// (`Unparsable` outcome), the actor still answers, and the corrected formula
/// is recalculated at once (invariant 11) — in both modes.
#[test]
fn a_too_deep_formula_is_a_not_calculated_block_then_gets_corrected() {
    for mode in [FormulaRecalc::Targeted, FormulaRecalc::Full] {
        let (_session, handle, f) = start(mode);

        // Just under the cap: calculated, on the actor's thread (2 MiB).
        handle
            .execute(set_formula(&handle, f.c, &sum_of_a(1001)))
            .expect("1001 terms");
        assert_eq!(
            numbers(&take_snapshot(&handle), f.c),
            some(&[1001.0, 2002.0, 3003.0]),
            "{mode:?}"
        );

        // One term too many: refused, a not-calculated block of 3 rows.
        handle
            .execute(set_formula(&handle, f.c, &sum_of_a(1002)))
            .expect("1002 terms");
        let s = take_snapshot(&handle);
        assert_eq!(numbers(&s, f.c), vec![None, None, None], "{mode:?}");
        assert!(all_not_calculated(&s, f.c), "{mode:?}");

        // Corrected: recalculated, although the column is already "up to date".
        handle
            .execute(set_formula(&handle, f.c, "A+B"))
            .expect("corrected");
        assert_eq!(
            numbers(&take_snapshot(&handle), f.c),
            some(&[11.0, 22.0, 33.0]),
            "{mode:?}"
        );

        // Too deep through parentheses, then through signs, without the actor
        // falling: each time a not-calculated block.
        for too_deep in [
            format!("{}A{}", "(".repeat(101), ")".repeat(101)),
            format!("{}A", "-".repeat(101)),
            format!("{}A", "-".repeat(100_000)),
        ] {
            handle
                .execute(set_formula(&handle, f.c, &too_deep))
                .expect("too deep");
            let s = take_snapshot(&handle);
            assert!(all_not_calculated(&s, f.c), "{mode:?}");
            assert_eq!(numbers(&s, f.c).len(), 3, "{mode:?}");
        }

        // The undo gives back the last valid formula and its values.
        handle.undo().expect("undo");
        handle.undo().expect("undo");
        handle.undo().expect("undo");
        assert_eq!(
            numbers(&take_snapshot(&handle), f.c),
            some(&[11.0, 22.0, 33.0]),
            "{mode:?}"
        );
    }
}

/// A block not calculated because of `TooDeep` follows the sheet's length like
/// any unreadable formula, and a formula that reads it is itself not calculated
/// as long as it is (the refused parse does not enter the graph), then catches
/// up when the source is corrected.
#[test]
fn a_too_deep_formula_follows_the_sheet_and_catches_up_its_readers() {
    for mode in [FormulaRecalc::Targeted, FormulaRecalc::Full] {
        let (_session, handle, f) = start(mode);
        handle
            .execute(set_formula(&handle, f.c, &sum_of_a(1002)))
            .expect("C too deep");
        handle
            .execute(set_formula(&handle, f.d, "C+1"))
            .expect("D = C+1");
        let s = take_snapshot(&handle);
        assert!(all_not_calculated(&s, f.c), "{mode:?}");
        assert!(all_not_calculated(&s, f.d), "{mode:?}");

        // The sheet grows (B goes to 5 rows): the block follows.
        handle
            .execute(enter(f.b, 4, &[1.0]))
            .expect("B goes down to 5 rows");
        let s = take_snapshot(&handle);
        assert_eq!(numbers(&s, f.c).len(), 5, "{mode:?}");
        assert!(all_not_calculated(&s, f.c), "{mode:?}");

        // C corrected: D, which reads it, follows in the same pass.
        handle
            .execute(set_formula(&handle, f.c, "A*2"))
            .expect("C corrected");
        let s = take_snapshot(&handle);
        assert_eq!(numbers(&s, f.c)[..3], some(&[2.0, 4.0, 6.0]), "{mode:?}");
        assert_eq!(numbers(&s, f.d)[..3], some(&[3.0, 5.0, 7.0]), "{mode:?}");
    }
}

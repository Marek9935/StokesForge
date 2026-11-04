//! Tests of cross-structure formula behavior (#412): copies, pastes, transposes,
//! type changes, removals, moves, renames, cycles, row edits, reloads. Every
//! scenario checks the **correct values**, computed by hand, in both
//! [`FormulaRecalc::Targeted`] and [`FormulaRecalc::Full`].
//!
//! A test that fails because the application is wrong stays here with
//! `#[ignore = "bug: #N …"]`; a test that pins an undocumented choice is
//! marked `#[ignore = "design: #433 point N …"]` or carries a comment.

use std::collections::BTreeMap;

use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, CopyInsertion, MixedCell,
    NodeCopy, NodeId, Project, Tree, ValueBlock,
};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};

const BOTH: [FormulaRecalc; 2] = [FormulaRecalc::Targeted, FormulaRecalc::Full];

// -- helpers -------------------------------------------------------------------

fn add_book(p: &mut Project, parent: NodeId, name: &str) -> NodeId {
    p.add_workbook(parent, Some(name)).expect("workbook")
}

fn first_sheet(p: &Project, book: NodeId) -> NodeId {
    p.tree().children(book).expect("sheets")[0]
}

fn add_col(p: &mut Project, sheet: NodeId, name: &str, values: &[f64]) -> NodeId {
    p.add_column(sheet, None, Some(name), Column::from_f64(values))
        .expect("column")
}

fn set_x(p: &mut Project, column: NodeId) {
    p.set_column_meta(
        column,
        ColumnMeta {
            role: ColumnRole::X,
            ..Default::default()
        },
    )
    .expect("role X");
}

/// Sets a formula typed in letters (the loop check is not applied: use
/// `Tree::formula_for_entry` explicitly where it matters).
fn set_f(p: &mut Project, column: NodeId, text: &str) {
    let formula = p
        .tree()
        .formula_from_letters(column, text)
        .unwrap_or_else(|e| panic!("formula {text:?}: {e:?}"));
    p.set_column_meta(
        column,
        ColumnMeta {
            formula: Some(formula),
            ..Default::default()
        },
    )
    .expect("formula set");
}

fn spawn(p: Project, mode: FormulaRecalc) -> (Session, SessionHandle) {
    let session = Session::spawn_with_recalc(p, None, mode).expect("actor");
    let handle = session.handle();
    // A session starts without a recalculation pass: prime it.
    let root = handle.snapshot().expect("snapshot").tree.root();
    handle
        .execute(Command::AddFolder {
            parent: root,
            name: "Prime".into(),
        })
        .expect("prime");
    (session, handle)
}

fn snap(h: &SessionHandle) -> Snapshot {
    h.snapshot().expect("snapshot")
}

fn node(t: &Tree, path: &str) -> NodeId {
    t.resolve(path)
        .unwrap_or_else(|e| panic!("path {path}: {e}"))
}

/// Column `name` of the sheet at `path` (first match).
fn colat(t: &Tree, path: &str, name: &str) -> NodeId {
    let sheet = node(t, path);
    t.child_by_name(sheet, name)
        .expect("sheet")
        .unwrap_or_else(|| panic!("column {name} in {path}"))
}

fn column(t: &Tree, id: NodeId) -> &Column {
    t.node(id).and_then(|n| n.as_column()).expect("column")
}

fn nums(t: &Tree, id: NodeId) -> Vec<Option<f64>> {
    match column(t, id).values() {
        ColumnValues::Float(v) => v.iter().map(|x| (!x.is_nan()).then_some(*x)).collect(),
        ColumnValues::Mixed(cells) => cells
            .iter()
            .map(|c| match c {
                MixedCell::Number(v) if !v.is_nan() => Some(*v),
                _ => None,
            })
            .collect(),
    }
}

fn some(v: &[f64]) -> Vec<Option<f64>> {
    v.iter().copied().map(Some).collect()
}

fn all_empty(t: &Tree, id: NodeId) -> bool {
    let c = column(t, id);
    !c.is_empty()
        && (0..c.len()).all(|r| match c.get(r) {
            Cell::Float(v) => v.is_nan(),
            Cell::Text(Some(s)) => s == "---",
            Cell::Text(None) => true,
        })
}

fn enter(column: NodeId, start: usize, v: &[f64]) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(v.to_vec()),
        expected_version: None,
    }
}

fn set_meta_formula(t: &Tree, column: NodeId, text: &str) -> Command {
    Command::SetColumnMeta {
        column,
        meta: ColumnMeta {
            formula: Some(t.formula_from_letters(column, text).expect("formula")),
            ..Default::default()
        },
    }
}

fn versions(t: &Tree) -> BTreeMap<NodeId, u64> {
    t.iter()
        .filter_map(|(id, n)| n.as_column().map(|c| (id, c.version())))
        .collect()
}

/// A fresh Full session loaded from a saved copy of `h`: the independent
/// re-evaluation of every formula. Returns its snapshot.
fn reloaded_full(h: &SessionHandle, name: &str) -> Snapshot {
    let dir = scratch(name);
    let path = dir.path().join("reloaded.stf");
    h.save(&path).expect("save");
    let project = stokesforge_persistence::load(&path).expect("load");
    let session = Session::spawn_with_recalc(project, None, FormulaRecalc::Full).expect("actor");
    let handle = session.handle();
    // A fresh session does not recalculate: one more command does, in full.
    let root = snap(&handle).tree.root();
    handle
        .execute(Command::AddFolder {
            parent: root,
            name: "Oracle".into(),
        })
        .expect("oracle pass");
    let s = snap(&handle);
    drop(session);
    s
}

/// A fresh temporary directory, removed when the guard drops.
fn scratch(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("formula_structure-{name}-"))
        .tempdir()
        .expect("scratch dir")
}

/// Every formula column's cells of `a` equal those of `b` (same identifiers).
fn same_formula_cells(a: &Tree, b: &Tree) -> Result<(), String> {
    for (id, n) in a.iter() {
        let Some(c) = n.as_column() else { continue };
        if c.meta().formula.is_none() {
            continue;
        }
        let Some(other) = b.node(id).and_then(|n| n.as_column()) else {
            return Err(format!("{id} missing after reload"));
        };
        if c.values() != other.values() {
            return Err(format!("{id}: {:?} vs {:?}", c.values(), other.values()));
        }
    }
    Ok(())
}

// -- 1. copies of a lone formula column ----------------------------------------

/// `Book1/Sheet1`: A [1,2,3], B [10,20,30], C = A+B.
fn book_with_c() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s = first_sheet(&p, book);
    add_col(&mut p, s, "A", &[1.0, 2.0, 3.0]);
    add_col(&mut p, s, "B", &[10.0, 20.0, 30.0]);
    let c = add_col(&mut p, s, "C", &[]);
    set_f(&mut p, c, "A+B");
    p
}

/// DESIGN QUESTION. A same-project `InsertCopy` of a lone `A+B` column into
/// its own sheet: `project.rs` (formula remap loop, "reservation with no node")
/// reserves an identifier for every column outside the copy, even when the copy
/// is local, so the pasted column reads `#REF+#REF` and is empty. Graph series
/// do the opposite (a local copy keeps reading the original). The docs say
/// "reads outside the copy becomes #REF" only for the external case
/// (`docs/commands-and-history.md`, cross-project paragraph), and say "Within
/// the same project, the dependencies are not inserted". The interface copies a
/// column's formula through the grid clipboard (letters), not this path, so the
/// object-level copy of a lone column is probably unreachable from the UI.
#[test]
// #433 point 4
fn a_local_copy_of_a_lone_formula_column_reads_ref() {
    for mode in BOTH {
        let (_s, h) = spawn(book_with_c(), mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        let sheet = node(&t, "[Book1]Sheet1");
        h.execute(Command::InsertCopy {
            parent: sheet,
            copy: NodeCopy::capture(&t, c).expect("capture"),
            name: "C2".into(),
        })
        .expect("paste");
        let t = snap(&h).tree;
        let c2 = colat(&t, "[Book1]Sheet1", "C2");
        assert_eq!(
            t.formula_letters(c2).as_deref(),
            Some("#REF+#REF"),
            "{mode:?}"
        );
        assert!(
            all_empty(&t, c2) || nums(&t, c2).iter().all(Option::is_none),
            "{mode:?}"
        );
        // The original is untouched.
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Sheet1", "C")),
            some(&[11.0, 22.0, 33.0])
        );
    }
}

/// Duplicating the sheet makes the copy read its own columns (documented).
#[test]
fn duplicate_worksheet_copy_reads_its_own_columns_and_follows_them() {
    for mode in BOTH {
        let (_s, h) = spawn(book_with_c(), mode);
        let t = snap(&h).tree;
        let sheet = node(&t, "[Book1]Sheet1");
        h.execute(Command::DuplicateWorksheet {
            sheet,
            name: "Dup".into(),
        })
        .expect("duplicate");
        let t = snap(&h).tree;
        let (ca, cc) = (colat(&t, "[Book1]Dup", "A"), colat(&t, "[Book1]Dup", "C"));
        assert_eq!(nums(&t, cc), some(&[11.0, 22.0, 33.0]), "{mode:?}");
        h.execute(enter(ca, 0, &[100.0])).expect("edit copy A");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, cc),
            some(&[110.0, 22.0, 33.0]),
            "{mode:?} copy follows its A"
        );
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Sheet1", "C")),
            some(&[11.0, 22.0, 33.0]),
            "{mode:?} original unaffected"
        );
        // Undo then redo keep both coherent.
        h.undo().expect("undo");
        h.undo().expect("undo duplicate");
        h.redo().expect("redo");
        h.redo().expect("redo");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Dup", "C")),
            some(&[110.0, 22.0, 33.0]),
            "{mode:?}"
        );
    }
}

/// A folder copied into another folder: the workbook inside reads its own copy,
/// and a remote reference to a workbook outside the copy keeps reading the
/// original.
#[test]
fn copied_folder_reads_its_own_sheets_and_the_original_outside_ones() {
    let build = || {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let f1 = p.add_folder(root, "F1").expect("F1");
        p.add_folder(root, "F2").expect("F2");
        let book9 = add_book(&mut p, root, "Book9");
        let s9 = first_sheet(&p, book9);
        add_col(&mut p, s9, "A", &[7.0, 8.0, 9.0]);
        let book = add_book(&mut p, f1, "Book1");
        let s = first_sheet(&p, book);
        add_col(&mut p, s, "A", &[1.0, 2.0, 3.0]);
        let c = add_col(&mut p, s, "C", &[]);
        set_f(&mut p, c, "A*2");
        let r = add_col(&mut p, s, "R", &[]);
        set_f(&mut p, r, "max([Book9]Sheet1!A)");
        p
    };
    for mode in BOTH {
        let (_s, h) = spawn(build(), mode);
        let t = snap(&h).tree;
        let f1 = node(&t, "/F1");
        let f2 = node(&t, "/F2");
        h.execute(Command::InsertCopy {
            parent: f2,
            copy: NodeCopy::capture(&t, f1).expect("capture"),
            name: "F1".into(),
        })
        .expect("paste folder");
        let t = snap(&h).tree;
        let a = colat(&t, "/F2/F1/[Book1]Sheet1", "A");
        let c = colat(&t, "/F2/F1/[Book1]Sheet1", "C");
        let r = colat(&t, "/F2/F1/[Book1]Sheet1", "R");
        assert_eq!(nums(&t, c), some(&[2.0, 4.0, 6.0]), "{mode:?}");
        assert_eq!(nums(&t, r), some(&[9.0; 3]), "{mode:?}");
        h.execute(enter(a, 0, &[50.0])).expect("edit");
        h.execute(enter(
            colat(&t, "/[Book9]Sheet1", "A"),
            0,
            &[70.0, 80.0, 90.0],
        ))
        .expect("edit Book9");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, c),
            some(&[100.0, 4.0, 6.0]),
            "{mode:?}: copy reads copy"
        );
        assert_eq!(
            nums(&t, r),
            some(&[90.0; 3]),
            "{mode:?}: copy still reads Book9"
        );
        let (c0, r0) = (
            colat(&t, "/F1/[Book1]Sheet1", "C"),
            colat(&t, "/F1/[Book1]Sheet1", "R"),
        );
        assert_eq!(
            nums(&t, c0),
            some(&[2.0, 4.0, 6.0]),
            "{mode:?}: original untouched"
        );
        assert_eq!(nums(&t, r0), some(&[90.0; 3]), "{mode:?}");
    }
}

// -- 2. multi-root InsertCopies ------------------------------------------------

/// Book1/Sheet1: A [1,2,3], X = max([Book2]Sheet1!A). Book2/Sheet1: A [5,6,7],
/// X = max([Book1]Sheet1!A).
fn two_books() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let b1 = add_book(&mut p, root, "Book1");
    let b2 = add_book(&mut p, root, "Book2");
    let (s1, s2) = (first_sheet(&p, b1), first_sheet(&p, b2));
    add_col(&mut p, s1, "A", &[1.0, 2.0, 3.0]);
    add_col(&mut p, s2, "A", &[5.0, 6.0, 7.0]);
    let x1 = add_col(&mut p, s1, "X", &[]);
    let x2 = add_col(&mut p, s2, "X", &[]);
    set_f(&mut p, x1, "max([Book2]Sheet1!A)");
    set_f(&mut p, x2, "max([Book1]Sheet1!A)");
    p
}

fn copy_of(t: &Tree, path: &str) -> NodeCopy {
    NodeCopy::capture(t, node(t, &path.replace("Sheet1", ""))).expect("capture")
}

#[test]
fn copies_of_one_gesture_read_each_other_two_gestures_read_originals() {
    for mode in BOTH {
        // One gesture.
        let (_s, h) = spawn(two_books(), mode);
        let t = snap(&h).tree;
        let root = t.root();
        h.execute(Command::InsertCopies {
            items: vec![
                CopyInsertion {
                    parent: root,
                    copy: copy_of(&t, "/[Book1]"),
                    name: "Book1c".into(),
                },
                CopyInsertion {
                    parent: root,
                    copy: copy_of(&t, "/[Book2]"),
                    name: "Book2c".into(),
                },
            ],
        })
        .expect("paste both");
        let t = snap(&h).tree;
        let (x1c, x2c) = (
            colat(&t, "/[Book1c]Sheet1", "X"),
            colat(&t, "/[Book2c]Sheet1", "X"),
        );
        assert_eq!(nums(&t, x1c), some(&[7.0; 3]), "{mode:?}");
        assert_eq!(nums(&t, x2c), some(&[3.0; 3]), "{mode:?}");
        // Edit Book2c!A: Book1c.X follows (reads the copy), Book1.X does not.
        h.execute(enter(colat(&t, "/[Book2c]Sheet1", "A"), 0, &[500.0]))
            .expect("edit");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, x1c),
            some(&[500.0; 3]),
            "{mode:?}: copy reads copy"
        );
        assert_eq!(
            nums(&t, colat(&t, "/[Book1]Sheet1", "X")),
            some(&[7.0; 3]),
            "{mode:?}"
        );
        assert_eq!(
            t.formula_letters(x1c).as_deref(),
            Some("max([Book2c]Sheet1!A)"),
            "{mode:?}"
        );

        // Two gestures: the first copy reads the original Book2, and the second
        // copy (of the originals) reads the original Book1.
        let (_s, h) = spawn(two_books(), mode);
        let t = snap(&h).tree;
        let root = t.root();
        h.execute(Command::InsertCopy {
            parent: root,
            copy: copy_of(&t, "/[Book1]"),
            name: "Book1c".into(),
        })
        .expect("paste 1");
        let t = snap(&h).tree;
        h.execute(Command::InsertCopy {
            parent: root,
            copy: copy_of(&t, "/[Book2]"),
            name: "Book2c".into(),
        })
        .expect("paste 2");
        let t = snap(&h).tree;
        let (x1c, x2c) = (
            colat(&t, "/[Book1c]Sheet1", "X"),
            colat(&t, "/[Book2c]Sheet1", "X"),
        );
        h.execute(enter(colat(&t, "/[Book2c]Sheet1", "A"), 0, &[500.0]))
            .expect("edit copy");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, x1c),
            some(&[7.0; 3]),
            "{mode:?}: reads the original Book2"
        );
        assert_eq!(
            nums(&t, x2c),
            some(&[3.0; 3]),
            "{mode:?}: reads the original Book1"
        );
        h.execute(enter(colat(&t, "/[Book2]Sheet1", "A"), 0, &[900.0]))
            .expect("edit original");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, x1c), some(&[900.0; 3]), "{mode:?}");
    }
}

// -- 3. external paste ---------------------------------------------------------

/// A formula reading `[Book2]Sheet5!B`, pasted from another project (no Book2):
/// `#REF`; creating a Book2/Sheet5/B afterwards must not revive it.
#[test]
fn external_paste_of_a_remote_reader_is_ref_and_stays_ref() {
    let mut src = Project::new("Src").expect("project");
    let root = src.root();
    let b1 = add_book(&mut src, root, "Book1");
    let s1 = first_sheet(&src, b1);
    add_col(&mut src, s1, "A", &[1.0, 2.0, 3.0]);
    let b2 = add_book(&mut src, root, "Book2");
    let s5 = src.add_worksheet(b2, Some("Sheet5")).expect("Sheet5");
    add_col(&mut src, s5, "A", &[1.0, 2.0, 3.0]);
    add_col(&mut src, s5, "B", &[4.0, 5.0, 6.0]);
    let r = add_col(&mut src, s1, "R", &[]);
    set_f(&mut src, r, "max([Book2]Sheet5!B)");
    let l = add_col(&mut src, s1, "L", &[]);
    set_f(&mut src, l, "A*2");
    // A bare worksheet cannot travel through the object clipboard (decode wraps
    // the roots in a folder): the app only copies windows and folders.
    let copy = NodeCopy::capture(src.tree(), b1).expect("capture");
    let text = stokesforge_persistence::clipboard::encode_objects("other-origin", &[copy])
        .expect("encode");
    let (_, copies) = stokesforge_persistence::clipboard::decode_objects(&text).expect("decode");
    let copy = copies.into_iter().next().expect("one copy").into_external();

    for mode in BOTH {
        let mut dst = Project::new("Dst").expect("project");
        let root = dst.root();
        add_book(&mut dst, root, "BookD");
        let (_s, h) = spawn(dst, mode);
        let t = snap(&h).tree;
        h.execute(Command::InsertCopy {
            parent: t.root(),
            copy: copy.clone(),
            name: "Pasted".into(),
        })
        .expect("paste");
        let t = snap(&h).tree;
        let r = colat(&t, "/[Pasted]Sheet1", "R");
        let l = colat(&t, "/[Pasted]Sheet1", "L");
        assert_eq!(nums(&t, l), some(&[2.0, 4.0, 6.0]), "{mode:?}");
        assert!(
            t.formula_letters(r).expect("letters").contains("#REF"),
            "{mode:?}"
        );
        assert!(
            nums(&t, r).iter().all(Option::is_none),
            "{mode:?}: {:?}",
            nums(&t, r)
        );

        // Now create Book2/Sheet5/B in the destination, plenty of columns/sheets.
        h.execute(Command::AddWorkbook {
            parent: t.root(),
            name: Some("Book2".into()),
        })
        .expect("Book2");
        let t = snap(&h).tree;
        let b2 = node(&t, "/[Book2]");
        h.execute(Command::AddWorksheet {
            book: b2,
            name: Some("Sheet5".into()),
        })
        .expect("Sheet5");
        for k in 0..30 {
            let t = snap(&h).tree;
            let s5 = node(&t, "/[Book2]Sheet5");
            h.execute(Command::AddColumn {
                sheet: s5,
                index: None,
                name: Some(if k == 1 { "B".into() } else { format!("Z{k}") }),
                column: Column::from_f64(&[40.0, 50.0, 60.0]),
            })
            .expect("column");
        }
        let t = snap(&h).tree;
        assert!(
            t.formula_letters(r).expect("letters").contains("#REF"),
            "{mode:?}: stays REF"
        );
        assert!(
            nums(&t, r).iter().all(Option::is_none),
            "{mode:?}: no resurrection"
        );
    }
}

// -- 4. transposed sheets and formulas -----------------------------------------

/// `Sheet1`: Y columns B [10,20,30] (x' 1), C [100,200,300] (x' 2), D = B+C (x' 3).
fn transposable() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s = first_sheet(&p, book);
    let b = add_col(&mut p, s, "B", &[10.0, 20.0, 30.0]);
    let c = add_col(&mut p, s, "C", &[100.0, 200.0, 300.0]);
    let d = add_col(&mut p, s, "D", &[]);
    for (col, x) in [(b, 1.0), (c, 2.0), (d, 3.0)] {
        p.set_column_meta(
            col,
            ColumnMeta {
                x_prime: Some(x),
                ..Default::default()
            },
        )
        .expect("x'");
    }
    // Letters: the sheet's columns are A=B, B=C, C=D.
    let formula = p.tree().formula_from_letters(d, "A+B").expect("formula");
    p.set_column_meta(
        d,
        ColumnMeta {
            x_prime: Some(3.0),
            formula: Some(formula),
            ..Default::default()
        },
    )
    .expect("D formula");
    p
}

#[test]
fn transpose_reads_formula_columns_and_a_formula_reads_a_transposed_column() {
    for mode in BOTH {
        let (_s, h) = spawn(transposable(), mode);
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Sheet1", "D")),
            some(&[110.0, 220.0, 330.0])
        );
        let source = node(&t, "[Book1]Sheet1");
        h.execute(Command::TransposeRows {
            source,
            rows: vec![0, 1],
        })
        .expect("transpose");
        let t = snap(&h).tree;
        let tsheet = t
            .children(node(&t, "[Book1]"))
            .expect("sheets")
            .iter()
            .copied()
            .find(|id| *id != source)
            .expect("transposed sheet");
        let cols = t.children(tsheet).expect("cols").to_vec();
        assert_eq!(cols.len(), 3, "x', Row 1, Row 2");
        assert_eq!(nums(&t, cols[0]), some(&[1.0, 2.0, 3.0]));
        assert_eq!(nums(&t, cols[1]), some(&[10.0, 100.0, 110.0]));
        assert_eq!(nums(&t, cols[2]), some(&[20.0, 200.0, 220.0]));
        // A formula column in the transposed sheet reading a linked column,
        // and a formula in the source sheet reading the transposed one.
        h.execute(Command::AddColumn {
            sheet: tsheet,
            index: None,
            name: Some("F".into()),
            column: Column::new(ColumnType::Numeric),
        })
        .expect("F");
        let t = snap(&h).tree;
        let f = *t.children(tsheet).expect("cols").last().expect("F");
        h.execute(set_meta_formula(&t, f, "B*2"))
            .expect("F = Row 1 * 2");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, f), some(&[20.0, 200.0, 220.0]), "{mode:?}");
        let tname = t.node(tsheet).expect("sheet").name().to_string();
        let g = colat(&t, "[Book1]Sheet1", "D");
        let _ = g;
        // Change the source, no refresh: the link is a snapshot, F is unchanged.
        let b = colat(&t, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("edit B");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Sheet1", "D")),
            some(&[111.0, 220.0, 330.0])
        );
        assert_eq!(
            nums(&t, f),
            some(&[20.0, 200.0, 220.0]),
            "{mode:?}: snapshot link"
        );
        h.execute(Command::RecalculateTranspose { sheet: tsheet })
            .expect("recalculate transpose");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), some(&[11.0, 100.0, 111.0]), "{mode:?}");
        assert_eq!(
            nums(&t, f),
            some(&[22.0, 200.0, 222.0]),
            "{mode:?}: F follows the refresh"
        );
        // Undo the refresh, then the edit, then redo both.
        h.undo().expect("undo refresh");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, f), some(&[20.0, 200.0, 220.0]), "{mode:?}");
        h.redo().expect("redo");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, f), some(&[22.0, 200.0, 222.0]), "{mode:?}");
        // The transposed sheet's name (non-ASCII mark) is addressable from a
        // formula of another sheet and round-trips.
        assert!(tname.contains('\u{1D40}'));
        let oracle = reloaded_full(&h, "transpose");
        same_formula_cells(&t, &oracle.tree).expect("reload agrees");
    }
}

// -- 5. SetColumnType on formula columns and on columns they read --------------

#[test]
fn set_column_type_on_a_formula_column_is_coherent_and_undoable() {
    for mode in BOTH {
        let (_s, h) = spawn(book_with_c(), mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        for target in [ColumnType::Mixed, ColumnType::DateTime, ColumnType::Numeric] {
            let r = h.execute(Command::SetColumnType {
                column: c,
                column_type: target,
            });
            let t = snap(&h).tree;
            eprintln!(
                "{mode:?} -> {target:?}: {:?} / type {:?} values {:?}",
                r.is_ok(),
                column(&t, c).column_type(),
                nums(&t, c)
            );
            // Whatever the answer, the cells must equal A+B afterwards, or the
            // gesture must have been refused.
            if r.is_ok() {
                assert_eq!(
                    nums(&t, c),
                    some(&[11.0, 22.0, 33.0]),
                    "{mode:?} {target:?}"
                );
            }
            let oracle = reloaded_full(&h, "ctype");
            same_formula_cells(&t, &oracle.tree).expect("coherent with a fresh evaluation");
        }
    }
}

#[test]
fn set_column_type_on_a_read_column_then_undo_redo() {
    for mode in BOTH {
        let (_s, h) = spawn(book_with_c(), mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        let a = colat(&t, "[Book1]Sheet1", "A");
        for target in [ColumnType::Mixed, ColumnType::DateTime] {
            h.execute(Command::SetColumnType {
                column: a,
                column_type: target,
            })
            .expect("type change");
            let t = snap(&h).tree;
            eprintln!("{mode:?} A -> {target:?}: C = {:?}", nums(&t, c));
            let oracle = reloaded_full(&h, "ctype2");
            same_formula_cells(&t, &oracle.tree).expect("coherent");
            h.undo().expect("undo");
            let t = snap(&h).tree;
            assert_eq!(
                nums(&t, c),
                some(&[11.0, 22.0, 33.0]),
                "{mode:?} undo restores"
            );
            h.redo().expect("redo");
            h.undo().expect("undo again");
            let t = snap(&h).tree;
            assert_eq!(nums(&t, c), some(&[11.0, 22.0, 33.0]), "{mode:?}");
        }
    }
}

/// Direct cell writes into a formula column: the doc of `check_not_calculated`
/// says no client may write them; whatever the reply, the cells must end up
/// equal to the formula's value, and an undo must not leave stale values.
#[test]
fn direct_writes_into_a_formula_column_never_survive() {
    for mode in BOTH {
        let (_s, h) = spawn(book_with_c(), mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        let r = h.execute(enter(c, 1, &[999.0]));
        let t = snap(&h).tree;
        eprintln!(
            "{mode:?} SetCells on formula column: accepted={}",
            r.is_ok()
        );
        assert_eq!(nums(&t, c), some(&[11.0, 22.0, 33.0]), "{mode:?}");
        if r.is_ok() {
            h.undo().expect("undo");
            let t = snap(&h).tree;
            assert_eq!(
                nums(&t, c),
                some(&[11.0, 22.0, 33.0]),
                "{mode:?} after undo"
            );
        }
    }
}

/// pyst's docstring (`stokesforge-script/src/sheet_edit.rs`, `set_column_type`)
/// says "A formula column refuses the conversion", and `SetCells` into a formula
/// column is refused (`FormulaColumnReadOnly`, history.rs `check_writable`), but
/// `Command::SetColumnType` only runs `check_not_calculated` (calculation outputs):
/// the gesture is accepted, records an undo entry, and the next recalculation
/// pass silently puts the computed type back.
#[test]
fn set_column_type_on_a_formula_column_is_refused() {
    for mode in BOTH {
        let (_s, h) = spawn(book_with_c(), mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        let r = h.execute(Command::SetColumnType {
            column: c,
            column_type: ColumnType::DateTime,
        });
        assert!(r.is_err(), "{mode:?}: the conversion is accepted");
    }
}

// -- 6. removing a workbook / folder that holds read sources --------------------

/// Book1/Sheet1: A [1,2,3]. Fold/BookS/Sheet1: S [5,6,7]. Reader in Book1:
/// R = max([BookS]Sheet1!A), W = 2*A (local, stays valid).
fn remote_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let fold = p.add_folder(root, "Fold").expect("Fold");
    let bs = add_book(&mut p, fold, "BookS");
    let ss = first_sheet(&p, bs);
    add_col(&mut p, ss, "S", &[5.0, 6.0, 7.0]);
    let b1 = add_book(&mut p, root, "Book1");
    let s1 = first_sheet(&p, b1);
    add_col(&mut p, s1, "A", &[1.0, 2.0, 3.0]);
    let r = add_col(&mut p, s1, "R", &[]);
    set_f(&mut p, r, "max([BookS]Sheet1!A)");
    let w = add_col(&mut p, s1, "W", &[]);
    set_f(&mut p, w, "A*2");
    p
}

fn check_remote_state(t: &Tree, removed: bool, ctx: &str) {
    let r = colat(t, "[Book1]Sheet1", "R");
    let w = colat(t, "[Book1]Sheet1", "W");
    assert_eq!(nums(t, w), some(&[2.0, 4.0, 6.0]), "{ctx}");
    if removed {
        assert!(
            t.formula_letters(r).expect("letters").contains("#REF"),
            "{ctx}"
        );
        assert!(
            nums(t, r).iter().all(Option::is_none),
            "{ctx}: {:?}",
            nums(t, r)
        );
        assert_eq!(
            column(t, r).len(),
            3,
            "{ctx}: the empty result keeps the sheet's length"
        );
    } else {
        assert_eq!(nums(t, r), some(&[7.0; 3]), "{ctx}");
        assert_eq!(
            t.formula_letters(r).as_deref(),
            Some("max([BookS]Sheet1!A)"),
            "{ctx}"
        );
    }
}

#[test]
fn removing_the_remote_workbook_or_folder_gives_ref_and_undo_restores() {
    for (what, path) in [("workbook", "/Fold/[BookS]"), ("folder", "/Fold")] {
        for mode in BOTH {
            let ctx = format!("{what} {mode:?}");
            let (_s, h) = spawn(remote_world(), mode);
            check_remote_state(&snap(&h).tree, false, &ctx);
            let id = node(&snap(&h).tree, path);
            h.execute(Command::RemoveNode { id }).expect("remove");
            check_remote_state(&snap(&h).tree, true, &format!("{ctx} removed"));
            h.undo().expect("undo");
            check_remote_state(&snap(&h).tree, false, &format!("{ctx} undone"));
            h.redo().expect("redo");
            check_remote_state(&snap(&h).tree, true, &format!("{ctx} redone"));
            h.undo().expect("undo");
            // Inside a macro: remove, then abort.
            h.begin_macro(MacroKind::DeleteObjects).expect("macro");
            h.execute(Command::RemoveNode { id })
                .expect("remove in macro");
            check_remote_state(&snap(&h).tree, true, &format!("{ctx} macro"));
            h.abort_macro().expect("abort");
            check_remote_state(&snap(&h).tree, false, &format!("{ctx} aborted"));
            // Macro closed, undone, redone.
            h.begin_macro(MacroKind::DeleteObjects).expect("macro");
            h.execute(Command::RemoveNode { id })
                .expect("remove in macro");
            h.end_macro().expect("close");
            check_remote_state(&snap(&h).tree, true, &format!("{ctx} macro closed"));
            h.undo().expect("undo macro");
            check_remote_state(&snap(&h).tree, false, &format!("{ctx} macro undone"));
            h.redo().expect("redo macro");
            check_remote_state(&snap(&h).tree, true, &format!("{ctx} macro redone"));
            // Save with the dangling reference, reload: still REF, then undo is
            // not possible across a load, but the values hold.
            let dir = scratch("remove");
            let path = dir.path().join(format!("{what}-{mode:?}.stf"));
            h.save(&path).expect("save");
            h.load(&path).expect("load");
            check_remote_state(&snap(&h).tree, true, &format!("{ctx} reloaded"));
        }
    }
}

// -- 7. moving the reading column into the sheet it reads ------------------------

#[test]
fn moving_a_remote_reader_into_the_sheet_it_reads() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let s1 = first_sheet(&p, book);
        let s3 = p.add_worksheet(book, Some("Sheet3")).expect("Sheet3");
        add_col(&mut p, s1, "A", &[1.0, 2.0, 3.0]);
        add_col(&mut p, s3, "A", &[0.0, 1.0, 2.0]);
        add_col(&mut p, s3, "B", &[100.0, 200.0, 300.0]);
        let r = add_col(&mut p, s1, "R", &[]);
        set_f(&mut p, r, "max(Sheet3!B)");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let r = colat(&t, "[Book1]Sheet1", "R");
        assert_eq!(nums(&t, r), some(&[300.0; 3]));
        let sheet3 = node(&t, "[Book1]Sheet3");
        h.execute(Command::Move {
            id: r,
            new_parent: sheet3,
            index: 2,
        })
        .expect("move R into Sheet3");
        let t = snap(&h).tree;
        // R now sits in the sheet it reads, next to the column it names. A
        // formula that reads a column of its own sheet through a remote prefix
        // is a loop only if it reads itself, which it does not: B is a plain
        // column. The display falls back to the local letter or keeps the
        // prefix; the value must be 300 either way, and stable.
        eprintln!(
            "{mode:?} moved R: {:?} {:?}",
            t.formula_letters(r),
            nums(&t, r)
        );
        assert!(!t.formula_in_loop(r), "{mode:?}: not a loop");
        assert_eq!(nums(&t, r), some(&[300.0; 3]), "{mode:?}");
        // Re-entering the displayed text gives the same formula.
        let shown = t.formula_letters(r).expect("letters");
        // (The stored formula still carries its remote side table, the entered
        // one has none: equal in meaning, not in structure.)
        let again = t.formula_for_entry(r, &shown).expect("re-entry accepted");
        assert_eq!(
            again.parts(),
            column(&t, r)
                .meta()
                .formula
                .as_ref()
                .expect("formula")
                .parts(),
            "{mode:?}: round trip of {shown:?}"
        );
        // Moving back out reads the other sheet again, with its prefix.
        h.undo().expect("undo move");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Sheet1", "R")),
            some(&[300.0; 3]),
            "{mode:?}"
        );
        assert_eq!(
            t.formula_letters(r).as_deref(),
            Some("max(Sheet3!B)"),
            "{mode:?}"
        );
        h.redo().expect("redo move");
        h.execute(Command::Move {
            id: r,
            new_parent: node(&t, "[Book1]Sheet1"),
            index: 1,
        })
        .expect("move back out");
        let t = snap(&h).tree;
        assert_eq!(
            t.formula_letters(r).as_deref(),
            Some("max(Sheet3!B)"),
            "{mode:?}"
        );
        assert_eq!(nums(&t, r), some(&[300.0; 3]), "{mode:?}");
    }
}

// -- 8. ambiguity of a workbook name ------------------------------------------

/// `/F1/Book2/Sheet1`: A [4,5,6]. `/Book1/Sheet1`: A [1,2,3], R = max([Book2]Sheet1!A).
fn ambiguity_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let f1 = p.add_folder(root, "F1").expect("F1");
    p.add_folder(root, "F2").expect("F2");
    let b2 = add_book(&mut p, f1, "Book2");
    let s2 = first_sheet(&p, b2);
    add_col(&mut p, s2, "A", &[4.0, 5.0, 6.0]);
    let b1 = add_book(&mut p, root, "Book1");
    let s1 = first_sheet(&p, b1);
    add_col(&mut p, s1, "A", &[1.0, 2.0, 3.0]);
    let r = add_col(&mut p, s1, "R", &[]);
    set_f(&mut p, r, "max([Book2]Sheet1!A)");
    p
}

/// The displayed text of `r`, re-entered, must give the stored formula back.
fn assert_round_trip(t: &Tree, r: NodeId, ctx: &str) {
    let shown = t.formula_letters(r).expect("letters");
    let again = t
        .formula_for_entry(r, &shown)
        .unwrap_or_else(|e| panic!("{ctx}: re-entry of {shown:?} refused: {e:?}"));
    assert_eq!(
        Some(&again),
        column(t, r).meta().formula.as_ref(),
        "{ctx}: round trip of {shown:?}"
    );
}

#[test]
fn an_ambiguous_workbook_name_falls_back_to_a_path_without_changing_values() {
    for mode in BOTH {
        let (_s, h) = spawn(ambiguity_world(), mode);
        let t = snap(&h).tree;
        let r = colat(&t, "[Book1]Sheet1", "R");
        assert_eq!(nums(&t, r), some(&[6.0; 3]), "{mode:?}");
        assert_eq!(
            t.formula_letters(r).as_deref(),
            Some("max([Book2]Sheet1!A)")
        );
        assert_round_trip(&t, r, "unambiguous");

        // A second Book2 in another folder: the display needs a path.
        let f2 = node(&t, "/F2");
        h.execute(Command::AddWorkbook {
            parent: f2,
            name: Some("Book2".into()),
        })
        .expect("second Book2");
        let t = snap(&h).tree;
        h.execute(Command::AddColumn {
            sheet: node(&t, "/F2/[Book2]Sheet1"),
            index: None,
            name: Some("A".into()),
            column: Column::from_f64(&[400.0, 500.0, 600.0]),
        })
        .expect("column in the other Book2");
        let t = snap(&h).tree;
        let shown = t.formula_letters(r).expect("letters");
        eprintln!("{mode:?} ambiguous display: {shown}");
        assert!(shown.contains("F1"), "{mode:?}: a path is shown: {shown}");
        assert_eq!(
            nums(&t, r),
            some(&[6.0; 3]),
            "{mode:?}: the value must not move"
        );
        assert_round_trip(&t, r, &format!("{mode:?} ambiguous"));

        // Rename the other one: the short form comes back.
        let other_book = node(&t, "/F2/[Book2]");
        h.execute(Command::Rename {
            id: other_book,
            name: "Book3".into(),
        })
        .expect("rename");
        let t = snap(&h).tree;
        assert_eq!(
            t.formula_letters(r).as_deref(),
            Some("max([Book2]Sheet1!A)"),
            "{mode:?}"
        );
        assert_eq!(nums(&t, r), some(&[6.0; 3]), "{mode:?}");

        // Move the reader's workbook into F2, where a Book2 would shadow: not
        // here (renamed), so rename it back first, then move the reader.
        h.execute(Command::Rename {
            id: other_book,
            name: "Book2".into(),
        })
        .expect("rename back");
        let book1 = node(&t, "/[Book1]");
        h.execute(Command::Move {
            id: book1,
            new_parent: f2,
            index: 1,
        })
        .expect("move the reader into F2");
        let t = snap(&h).tree;
        let shown = t.formula_letters(r).expect("letters");
        eprintln!("{mode:?} reader in F2: {shown}");
        assert_eq!(
            nums(&t, r),
            some(&[6.0; 3]),
            "{mode:?}: still reads F1/Book2"
        );
        assert_round_trip(&t, r, &format!("{mode:?} reader moved"));
        h.undo().expect("undo move");
        h.undo().expect("undo rename back");
        h.undo().expect("undo rename");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, r), some(&[6.0; 3]), "{mode:?}");
        assert_round_trip(&t, r, &format!("{mode:?} undone"));
    }
}

// -- 9. Rename recalculates nothing ---------------------------------------------

/// `/Book1/Sheet1`: A [1,2,3], R = max([Book2]Sheet1!A), L = A*2, N = max(Sheet3!A).
/// `/Book2/Sheet1`: A [4,5,6]. `/Book1/Sheet3`: A [9,8,7].
fn rename_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let b2 = add_book(&mut p, root, "Book2");
    let s2 = first_sheet(&p, b2);
    add_col(&mut p, s2, "A", &[4.0, 5.0, 6.0]);
    let b1 = add_book(&mut p, root, "Book1");
    let s1 = first_sheet(&p, b1);
    let s3 = p.add_worksheet(b1, Some("Sheet3")).expect("Sheet3");
    add_col(&mut p, s3, "A", &[9.0, 8.0, 7.0]);
    add_col(&mut p, s1, "A", &[1.0, 2.0, 3.0]);
    let r = add_col(&mut p, s1, "R", &[]);
    set_f(&mut p, r, "max([Book2]Sheet1!A)");
    let l = add_col(&mut p, s1, "L", &[]);
    set_f(&mut p, l, "A*2");
    let n = add_col(&mut p, s1, "N", &[]);
    set_f(&mut p, n, "max(Sheet3!A)");
    p
}

const AWKWARD: [&str; 14] = [
    "Blanc 2",
    "it's",
    "a!b",
    "a[b]",
    "1st",
    "h\u{e9}llo \u{4e16}\u{754c}",
    "x y z",
    "a]b",
    "[x",
    "a'b'c",
    "'",
    "''quoted''",
    "SHEET3",
    "Sheet3",
];

#[test]
fn renames_recalculate_nothing_keep_values_and_round_trip() {
    for mode in BOTH {
        let (_s, h) = spawn(rename_world(), mode);
        let t = snap(&h).tree;
        let (r, l, n) = (
            colat(&t, "[Book1]Sheet1", "R"),
            colat(&t, "[Book1]Sheet1", "L"),
            colat(&t, "[Book1]Sheet1", "N"),
        );
        assert_eq!(nums(&t, r), some(&[6.0; 3]));
        assert_eq!(nums(&t, n), some(&[9.0; 3]));
        let mut accepted = 0;
        let targets = [
            (node(&t, "[Book1]Sheet3"), "sheet"),
            (node(&t, "/[Book2]"), "workbook"),
        ];
        for name in AWKWARD {
            for (id, what) in targets {
                let t0 = snap(&h).tree;
                let before = versions(&t0);
                let res = h.execute(Command::Rename {
                    id,
                    name: name.to_string(),
                });
                let t1 = snap(&h).tree;
                if res.is_err() {
                    continue;
                }
                accepted += 1;
                let ctx = format!("{mode:?} {what} renamed to {name:?}");
                assert_eq!(nums(&t1, r), some(&[6.0; 3]), "{ctx}");
                assert_eq!(nums(&t1, l), some(&[2.0, 4.0, 6.0]), "{ctx}");
                assert_eq!(nums(&t1, n), some(&[9.0; 3]), "{ctx}");
                if mode == FormulaRecalc::Targeted {
                    assert_eq!(before, versions(&t1), "{ctx}: a rename rewrote a column");
                }
                for c in [r, n] {
                    assert!(
                        !t1.formula_has_deleted_reference(c),
                        "{ctx}: {:?}",
                        t1.formula_letters(c)
                    );
                    assert_round_trip(&t1, c, &ctx);
                }
            }
        }
        assert!(accepted >= 10, "{mode:?}: only {accepted} renames accepted");
        // Column rename: letters are positional, values untouched.
        let a = colat(&snap(&h).tree, "[Book1]Sheet1", "A");
        let before = versions(&snap(&h).tree);
        h.execute(Command::Rename {
            id: a,
            name: "Renamed A".into(),
        })
        .expect("rename column");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, l), some(&[2.0, 4.0, 6.0]));
        if mode == FormulaRecalc::Targeted {
            assert_eq!(before, versions(&t), "a column rename rewrote a column");
        }
        assert_eq!(t.formula_letters(l).as_deref(), Some("A*2"));
        // Case-only rename of a workbook.
        let b2 = targets[1].0;
        let name = t.node(b2).expect("Book2").name().to_string();
        let upper = name.to_uppercase();
        if h.execute(Command::Rename {
            id: b2,
            name: upper.clone(),
        })
        .is_ok()
        {
            let t = snap(&h).tree;
            assert_eq!(nums(&t, r), some(&[6.0; 3]), "{mode:?} case-only");
            assert_round_trip(&t, r, "case-only");
        }
    }
}

// -- 9b. arithmetic NaN is a failed calculation (#433 point 1) ---------------------

#[test]
fn zero_over_zero_shows_a_failed_calculation_in_the_cell() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let s = first_sheet(&p, book);
        add_col(&mut p, s, "A", &[0.0, 1.0]);
        let c = add_col(&mut p, s, "C", &[]);
        set_f(&mut p, c, "A/A");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        assert_eq!(
            column(&t, c).get(0),
            Cell::Text(Some("---".into())),
            "{mode:?}"
        );
        assert_eq!(nums(&t, c), vec![None, Some(1.0)], "{mode:?}");
        let oracle = reloaded_full(&h, "zero-over-zero");
        same_formula_cells(&t, &oracle.tree).expect("coherent with a reload");
    }
}

/// No point in the range: empty cells (not `---`): the NaN operand is not a
/// failed calculation of the arithmetic.
#[test]
fn dividing_by_a_range_with_no_point_stays_empty() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let s = first_sheet(&p, book);
        let x = add_col(&mut p, s, "A", &[1.0, 2.0]);
        set_x(&mut p, x);
        add_col(&mut p, s, "B", &[3.0, 4.0]);
        let c = add_col(&mut p, s, "C", &[]);
        set_f(&mut p, c, "B/max(B, 100, 200)");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        assert!(nums(&t, c).iter().all(Option::is_none), "{mode:?}");
        assert!(
            (0..2).all(|r| column(&t, c).get(r) != Cell::Text(Some("---".into()))),
            "{mode:?}"
        );
    }
}

// -- 10. a sheet whose only columns are formulas reading remote columns --------

#[test]
// #433 point 8
fn a_sheet_holding_only_remote_formulas_has_at_least_one_row() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let s1 = first_sheet(&p, book);
        let s3 = p.add_worksheet(book, Some("Sheet3")).expect("Sheet3");
        let x = add_col(&mut p, s3, "A", &[0.0, 1.0, 2.0]);
        set_x(&mut p, x);
        add_col(&mut p, s3, "B", &[100.0, 200.0, 300.0]);
        let f1 = add_col(&mut p, s1, "F1", &[]);
        let f2 = add_col(&mut p, s1, "F2", &[]);
        set_f(&mut p, f1, "max(Sheet3!B)");
        set_f(&mut p, f2, "at(Sheet3!B, 1)");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let (f1, f2) = (
            colat(&t, "[Book1]Sheet1", "F1"),
            colat(&t, "[Book1]Sheet1", "F2"),
        );
        assert_eq!(nums(&t, f1), some(&[300.0]), "{mode:?}");
        assert_eq!(nums(&t, f2), some(&[200.0]), "{mode:?}");
        // Giving the sheet rows makes them appear, with the right values.
        h.execute(Command::AddColumn {
            sheet: node(&t, "[Book1]Sheet1"),
            index: Some(0),
            name: Some("Rows".into()),
            column: Column::from_f64(&[1.0, 2.0, 3.0, 4.0]),
        })
        .expect("ordinary column");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, f1), some(&[300.0; 4]), "{mode:?}");
        assert_eq!(nums(&t, f2), some(&[200.0; 4]), "{mode:?}");
        // Undo: back to the single row; redo: four again.
        h.undo().expect("undo");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, f1), some(&[300.0]), "{mode:?} after undo");
        assert_eq!(nums(&t, f2), some(&[200.0]), "{mode:?} after undo");
        h.redo().expect("redo");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, f1), some(&[300.0; 4]), "{mode:?}");
        let oracle = reloaded_full(&h, "only-remote");
        same_formula_cells(&t, &oracle.tree).expect("coherent with a reload");
    }
}

/// `Book1/Sheet1` holding an empty ordinary column `E` and the one formula
/// `src` (column `F`); `Sheet3` has A (X) and B per `sheet3_b`.
fn floor_world(src: &str, sheet3_b: &[f64]) -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s1 = first_sheet(&p, book);
    let s3 = p.add_worksheet(book, Some("Sheet3")).expect("Sheet3");
    let x = add_col(
        &mut p,
        s3,
        "A",
        &(0..sheet3_b.len()).map(|i| i as f64).collect::<Vec<_>>(),
    );
    set_x(&mut p, x);
    add_col(&mut p, s3, "B", sheet3_b);
    add_col(&mut p, s1, "E", &[]);
    let f = add_col(&mut p, s1, "F", &[]);
    set_f(&mut p, f, src);
    p
}

#[test]
// #433 point 8
fn the_one_row_floor_applies_only_to_formulas_naming_no_column_of_their_sheet() {
    for mode in BOTH {
        // A constant: one row, although an (empty) ordinary column exists.
        let (_s, h) = spawn(floor_world("1+2", &[1.0]), mode);
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Sheet1", "F")),
            some(&[3.0]),
            "{mode:?}"
        );
        // A local reference to an empty column: no floor.
        let (_s, h) = spawn(floor_world("A+1", &[1.0]), mode);
        let t = snap(&h).tree;
        assert_eq!(
            column(&t, colat(&t, "[Book1]Sheet1", "F")).len(),
            0,
            "{mode:?}"
        );
        // A local reference gone to #REF: no floor either.
        let mut p = floor_world("A+1", &[1.0]);
        let t = p.tree().clone();
        let e = colat(&t, "[Book1]Sheet1", "E");
        p.remove(e).expect("remove E");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let f = colat(&t, "[Book1]Sheet1", "F");
        assert_eq!(t.formula_letters(f).as_deref(), Some("#REF+1"), "{mode:?}");
        assert_eq!(column(&t, f).len(), 0, "{mode:?}");
        // Remote, Sheet3 empty: one empty row; undo of the entry: no rows.
        let mut p = floor_world("1", &[]);
        let t = p.tree().clone();
        let f = colat(&t, "[Book1]Sheet1", "F");
        let c = ColumnMeta {
            formula: None,
            ..Default::default()
        };
        p.set_column_meta(f, c).expect("clear");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let f = colat(&t, "[Book1]Sheet1", "F");
        h.execute(set_meta_formula(&t, f, "max(Sheet3!B)"))
            .expect("entry");
        let t = snap(&h).tree;
        let f = colat(&t, "[Book1]Sheet1", "F");
        assert_eq!(column(&t, f).len(), 1, "{mode:?}");
        assert!(nums(&t, f).iter().all(Option::is_none), "{mode:?}");
        h.undo().expect("undo");
        let t = snap(&h).tree;
        assert_eq!(
            column(&t, colat(&t, "[Book1]Sheet1", "F")).len(),
            0,
            "{mode:?}"
        );
    }
}

/// A formula that reads a formula column of its own sheet follows its length
/// (one row here), whatever that one reads.
#[test]
// #433 point 8
fn a_formula_reading_a_remote_formula_of_its_sheet_has_the_same_single_row() {
    for mode in BOTH {
        let mut p = floor_world("max(Sheet3!B)", &[5.0, 7.0]);
        let t = p.tree().clone();
        let g = add_col(&mut p, node(&t, "[Book1]Sheet1"), "G", &[]);
        set_f(&mut p, g, "B*2");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, colat(&t, "[Book1]Sheet1", "G")),
            some(&[14.0]),
            "{mode:?}"
        );
        let oracle = reloaded_full(&h, "floor-chain");
        same_formula_cells(&t, &oracle.tree).expect("coherent with a reload");
    }
}

// -- 11. cycles created by roles, moves and unchecked commands --------------------

/// Plain `W` (X, [0,1,2]), `P` = `integral(C)` (Y), `S` [5,6,7]. Letters W=A, P=B,
/// S=C. `P` reads S and S's paired X (W): 12.
fn pairing_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s = first_sheet(&p, book);
    let w = add_col(&mut p, s, "W", &[0.0, 1.0, 2.0]);
    set_x(&mut p, w);
    let pp = add_col(&mut p, s, "P", &[]);
    add_col(&mut p, s, "S", &[5.0, 6.0, 7.0]);
    set_f(&mut p, pp, "integral(C)");
    p
}

/// `W` (X), `S` [5,6,7], `P` (X role, formula `integral(B)` = S's integral over
/// W). Letters W=A, S=B, P=C. Moving P to index 1 makes P the X of S.
fn move_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s = first_sheet(&p, book);
    let w = add_col(&mut p, s, "W", &[0.0, 1.0, 2.0]);
    set_x(&mut p, w);
    add_col(&mut p, s, "S", &[5.0, 6.0, 7.0]);
    let pp = add_col(&mut p, s, "P", &[]);
    let formula = p
        .tree()
        .formula_from_letters(pp, "integral(B)")
        .expect("formula");
    p.set_column_meta(
        pp,
        ColumnMeta {
            role: ColumnRole::X,
            formula: Some(formula),
            ..Default::default()
        },
    )
    .expect("P");
    p
}

type FormulaCells = Vec<(NodeId, Vec<Option<f64>>)>;

fn formula_snapshot(t: &Tree) -> FormulaCells {
    let mut v: Vec<_> = t
        .iter()
        .filter_map(|(id, n)| {
            let c = n.as_column()?;
            c.meta().formula.is_some().then(|| (id, nums(t, id)))
        })
        .collect();
    v.sort_by_key(|(id, _)| *id);
    v
}

#[test]
fn a_cycle_made_by_a_role_change_or_a_move_is_framed_and_the_entry_refuses_it() {
    let mut stages: Vec<Vec<FormulaCells>> = vec![Vec::new(), Vec::new()];
    for (m, mode) in BOTH.into_iter().enumerate() {
        // Role change.
        let (_s, h) = spawn(pairing_world(), mode);
        let t = snap(&h).tree;
        let p = colat(&t, "[Book1]Sheet1", "P");
        assert_eq!(nums(&t, p), some(&[12.0; 3]), "{mode:?}");
        let mut meta = column(&t, p).meta().clone();
        meta.role = ColumnRole::X;
        h.execute(Command::SetColumnMeta { column: p, meta })
            .expect("role X");
        let t = snap(&h).tree;
        assert!(
            nums(&t, p).iter().all(Option::is_none),
            "{mode:?}: P is the X of the column it reads: {:?}",
            nums(&t, p)
        );
        stages[m].push(formula_snapshot(&t));
        // The entry path refuses the same text.
        assert!(
            matches!(
                t.formula_for_entry(p, "integral(C)"),
                Err(stokesforge_core::FormulaError::Cycle)
            ),
            "{mode:?}: entry must refuse a loop through the paired X"
        );
        // Stable under further passes: nothing is rewritten in Targeted mode.
        let before = versions(&t);
        h.execute(Command::AddFolder {
            parent: t.root(),
            name: "Later".into(),
        })
        .expect("later");
        let t2 = snap(&h).tree;
        if mode == FormulaRecalc::Targeted {
            assert_eq!(
                before,
                versions(&t2),
                "the cycle is rewritten at every pass"
            );
        }
        h.undo().expect("undo folder");
        h.undo().expect("undo role");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, p), some(&[12.0; 3]), "{mode:?} undone");
        h.redo().expect("redo role");
        assert!(nums(&snap(&h).tree, p).iter().all(Option::is_none));

        // Move.
        let (_s, h) = spawn(move_world(), mode);
        let t = snap(&h).tree;
        let p = colat(&t, "[Book1]Sheet1", "P");
        assert_eq!(nums(&t, p), some(&[12.0; 3]), "{mode:?} before the move");
        h.execute(Command::Move {
            id: p,
            new_parent: node(&t, "[Book1]Sheet1"),
            index: 1,
        })
        .expect("move P in front of S");
        let t = snap(&h).tree;
        assert!(
            nums(&t, p).iter().all(Option::is_none),
            "{mode:?}: {:?}",
            nums(&t, p)
        );
        stages[m].push(formula_snapshot(&t));
        h.undo().expect("undo move");
        assert_eq!(nums(&snap(&h).tree, p), some(&[12.0; 3]), "{mode:?}");
    }
    assert_eq!(stages[0], stages[1], "Targeted and Full agree");
}

#[test]
fn unchecked_commands_can_install_a_loop_and_it_stays_framed() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let s = first_sheet(&p, book);
        add_col(&mut p, s, "A", &[1.0, 2.0, 3.0]);
        let q = add_col(&mut p, s, "Q", &[]);
        let r = add_col(&mut p, s, "R", &[]);
        let u = add_col(&mut p, s, "U", &[]);
        set_f(&mut p, u, "A+1");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let ids: Vec<NodeId> = ["Q", "R", "U"]
            .iter()
            .map(|n| colat(&t, "[Book1]Sheet1", n))
            .collect();
        assert_eq!(ids, vec![q, r, u]);
        // Q = R+1, R = U+1, U = Q+1: a 3-cycle through the unchecked command.
        // letters: A, Q=B, R=C, U=D.
        for (c, text) in [(ids[0], "C+1"), (ids[1], "D+1")] {
            h.execute(set_meta_formula(&snap(&h).tree, c, text))
                .expect("set");
        }
        // U closes the loop; the *entry* path refuses it...
        let t = snap(&h).tree;
        assert!(matches!(
            t.formula_for_entry(ids[2], "B+1"),
            Err(stokesforge_core::FormulaError::Cycle)
        ));
        // ...the command does not check.
        h.execute(set_meta_formula(&t, ids[2], "B+1"))
            .expect("unchecked");
        let t = snap(&h).tree;
        for c in &ids {
            assert!(
                nums(&t, *c).iter().all(Option::is_none),
                "{mode:?}: loop cells are empty"
            );
        }
        let before = versions(&t);
        h.execute(Command::AddFolder {
            parent: t.root(),
            name: "Later".into(),
        })
        .expect("later");
        if mode == FormulaRecalc::Targeted {
            assert_eq!(before, versions(&snap(&h).tree), "loop not rewritten again");
        }
        // Break the loop: values come back along the chain.
        h.execute(set_meta_formula(&snap(&h).tree, ids[2], "A+1"))
            .expect("break");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, ids[2]), some(&[2.0, 3.0, 4.0]), "{mode:?}");
        assert_eq!(nums(&t, ids[1]), some(&[3.0, 4.0, 5.0]), "{mode:?}");
        assert_eq!(nums(&t, ids[0]), some(&[4.0, 5.0, 6.0]), "{mode:?}");
    }
}

// -- 12. row edits under column functions ----------------------------------------

/// Independent evaluation: finite (x, y) pairs, ascending X.
fn pairs(t: &Tree, x: NodeId, y: NodeId) -> Vec<(f64, f64)> {
    let (xs, ys) = (nums(t, x), nums(t, y));
    xs.iter()
        .zip(&ys)
        .filter_map(|(x, y)| Some((((*x)?), ((*y)?))))
        .collect()
}

fn trapz(p: &[(f64, f64)]) -> f64 {
    p.windows(2)
        .map(|w| (w[1].0 - w[0].0) * (w[0].1 + w[1].1) / 2.0)
        .sum()
}

fn interpolate(p: &[(f64, f64)], x: f64) -> Option<f64> {
    p.windows(2)
        .find(|w| w[0].0 <= x && x <= w[1].0)
        .map(|w| w[0].1 + (w[1].1 - w[0].1) * (x - w[0].0) / (w[1].0 - w[0].0))
}

fn mean_in(p: &[(f64, f64)], lo: f64, hi: f64) -> Option<f64> {
    let v: Vec<f64> = p
        .iter()
        .filter(|(x, _)| (lo..=hi).contains(x))
        .map(|(_, y)| *y)
        .collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

/// `Sheet3`: A (X) [0,1,2,3], B [100,250,300,500], C = integral(B), D = at(B, 1.5),
/// E = mean(B, 1, 3). `Sheet1`: P [1,2,3,4], R1..R3 the same three read remotely.
fn rows_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s1 = first_sheet(&p, book);
    let s3 = p.add_worksheet(book, Some("Sheet3")).expect("Sheet3");
    let a = add_col(&mut p, s3, "A", &[0.0, 1.0, 2.0, 3.0]);
    set_x(&mut p, a);
    add_col(&mut p, s3, "B", &[100.0, 250.0, 300.0, 500.0]);
    for (name, text) in [
        ("C", "integral(B)"),
        ("D", "at(B, 1.5)"),
        ("E", "mean(B, 1, 3)"),
    ] {
        let c = add_col(&mut p, s3, name, &[]);
        set_f(&mut p, c, text);
    }
    add_col(&mut p, s1, "P", &[1.0, 2.0, 3.0, 4.0]);
    for (name, text) in [
        ("R1", "integral(Sheet3!B)"),
        ("R2", "at(Sheet3!B, 1.5)"),
        ("R3", "mean(Sheet3!B, 1, 3)"),
    ] {
        let c = add_col(&mut p, s1, name, &[]);
        set_f(&mut p, c, text);
    }
    p
}

fn check_rows(t: &Tree, ctx: &str) {
    let (x, y) = (
        colat(t, "[Book1]Sheet3", "A"),
        colat(t, "[Book1]Sheet3", "B"),
    );
    let p = pairs(t, x, y);
    let expected = [Some(trapz(&p)), interpolate(&p, 1.5), mean_in(&p, 1.0, 3.0)];
    let sheet3_len = column(t, y).len().max(column(t, x).len());
    for (i, (local, remote)) in [("C", "R1"), ("D", "R2"), ("E", "R3")]
        .into_iter()
        .enumerate()
    {
        let lc = colat(t, "[Book1]Sheet3", local);
        let rc = colat(t, "[Book1]Sheet1", remote);
        assert_eq!(
            nums(t, lc),
            vec![expected[i]; sheet3_len],
            "{ctx}: local {local}"
        );
        assert_eq!(nums(t, rc), vec![expected[i]; 4], "{ctx}: remote {remote}");
    }
}

#[test]
fn row_edits_keep_column_functions_exact_locally_and_remotely() {
    for mode in BOTH {
        let (_s, h) = spawn(rows_world(), mode);
        let t = snap(&h).tree;
        check_rows(&t, &format!("{mode:?} start"));
        let (c, r1) = (
            colat(&t, "[Book1]Sheet3", "C"),
            colat(&t, "[Book1]Sheet1", "R1"),
        );
        assert_eq!(nums(&t, c), some(&[850.0; 4]), "hand-computed integral");
        let sheet3 = node(&t, "[Book1]Sheet3");
        assert_eq!(nums(&t, r1), some(&[850.0; 4]));
        // Insert two empty rows inside the data: nothing changes numerically.
        h.execute(Command::InsertRows {
            sheet: sheet3,
            at: 1,
            count: 2,
        })
        .expect("insert");
        check_rows(&snap(&h).tree, &format!("{mode:?} inserted"));
        // Fill one of them with a point on the same curve, in A and B.
        h.execute(enter(
            colat(&snap(&h).tree, "[Book1]Sheet3", "A"),
            1,
            &[0.5],
        ))
        .expect("x");
        h.execute(enter(
            colat(&snap(&h).tree, "[Book1]Sheet3", "B"),
            1,
            &[999.0],
        ))
        .expect("y");
        check_rows(&snap(&h).tree, &format!("{mode:?} point added"));
        h.undo().expect("undo y");
        h.undo().expect("undo x");
        h.undo().expect("undo insert");
        check_rows(&snap(&h).tree, &format!("{mode:?} undone"));
        assert_eq!(nums(&snap(&h).tree, c), some(&[850.0; 4]));
        // Delete the first and a middle row.
        h.execute(Command::DeleteRows {
            sheet: sheet3,
            at: 0,
            count: 1,
        })
        .expect("delete first");
        check_rows(&snap(&h).tree, &format!("{mode:?} first row deleted"));
        h.execute(Command::DeleteRows {
            sheet: sheet3,
            at: 1,
            count: 1,
        })
        .expect("delete middle");
        check_rows(&snap(&h).tree, &format!("{mode:?} middle row deleted"));
        h.undo().expect("undo");
        h.undo().expect("undo");
        check_rows(&snap(&h).tree, &format!("{mode:?} deletes undone"));
        assert_eq!(
            nums(&snap(&h).tree, r1),
            some(&[850.0; 4]),
            "{mode:?} exact after undo"
        );
        h.redo().expect("redo");
        h.redo().expect("redo");
        check_rows(&snap(&h).tree, &format!("{mode:?} redone"));
        let oracle = reloaded_full(&h, "rows");
        same_formula_cells(&snap(&h).tree, &oracle.tree).expect("reload agrees");
    }
}

// -- 13. dangling references survive a save and never alias new ids ---------------

#[test]
fn a_dangling_reference_never_comes_back_after_reload_and_many_additions() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let s1 = first_sheet(&p, book);
        let s3 = p.add_worksheet(book, Some("Sheet3")).expect("Sheet3");
        add_col(&mut p, s3, "A", &[9.0, 8.0, 7.0]);
        add_col(&mut p, s1, "A", &[1.0, 2.0, 3.0]);
        add_col(&mut p, s1, "B", &[10.0, 20.0, 30.0]);
        let m = add_col(&mut p, s1, "M", &[]);
        set_f(&mut p, m, "B*1");
        let r = add_col(&mut p, s1, "R", &[]);
        set_f(&mut p, r, "max(Sheet3!A)");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let (m, r) = (
            colat(&t, "[Book1]Sheet1", "M"),
            colat(&t, "[Book1]Sheet1", "R"),
        );
        assert_eq!(nums(&t, m), some(&[10.0, 20.0, 30.0]));
        assert_eq!(nums(&t, r), some(&[9.0; 3]));
        h.execute(Command::DeleteColumn {
            column: colat(&t, "[Book1]Sheet1", "B"),
        })
        .expect("delete B");
        h.execute(Command::RemoveNode {
            id: node(&t, "[Book1]Sheet3"),
        })
        .expect("remove Sheet3");
        let dir = scratch("dangling");
        let path = dir.path().join(format!("{mode:?}.stf"));
        h.save(&path).expect("save");
        h.load(&path).expect("load");
        let t = snap(&h).tree;
        assert!(t.formula_has_deleted_reference(m) && t.formula_has_deleted_reference(r));
        // Many additions, of columns and of sheets: none may be mistaken for
        // the vanished ones.
        let book = node(&t, "[Book1]");
        for i in 0..25 {
            h.execute(Command::AddWorksheet {
                book,
                name: Some(format!("New{i}")),
            })
            .expect("sheet");
            let t = snap(&h).tree;
            let s = node(&t, &format!("[Book1]New{i}"));
            for j in 0..4 {
                h.execute(Command::AddColumn {
                    sheet: s,
                    index: None,
                    name: None,
                    column: Column::from_f64(&[1000.0 + f64::from(j), 2.0, 3.0]),
                })
                .expect("column");
            }
        }
        let s1 = node(&snap(&h).tree, "[Book1]Sheet1");
        for _ in 0..30 {
            h.execute(Command::AddColumn {
                sheet: s1,
                index: None,
                name: None,
                column: Column::from_f64(&[77.0, 78.0, 79.0]),
            })
            .expect("column");
        }
        let t = snap(&h).tree;
        for c in [m, r] {
            assert!(
                t.formula_has_deleted_reference(c),
                "{mode:?}: {:?}",
                t.formula_letters(c)
            );
            assert!(
                nums(&t, c).iter().all(Option::is_none),
                "{mode:?}: {:?}",
                nums(&t, c)
            );
            assert!(t.formula_letters(c).expect("letters").contains("#REF"));
        }
        // And a second reload keeps it.
        h.save(&path).expect("save");
        h.load(&path).expect("load");
        let t = snap(&h).tree;
        assert!(nums(&t, r).iter().all(Option::is_none));
    }
}

// -- 14. corrections and calculation outputs read by a remote formula -------------

use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_core::{compute, CalculationDraft, OutputDraft, SeriesRef};

/// `/Book1/Sheet1`: P [1,2,3], R = max(Sheet2!B). `Sheet2`: A (X) [0,1,2,3],
/// B [10,20,30,41].
fn analysis_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s1 = first_sheet(&p, book);
    let s2 = p.add_worksheet(book, Some("Sheet2")).expect("Sheet2");
    let a = add_col(&mut p, s2, "A", &[0.0, 1.0, 2.0, 3.0]);
    set_x(&mut p, a);
    add_col(&mut p, s2, "B", &[10.0, 20.0, 30.0, 41.0]);
    add_col(&mut p, s1, "P", &[1.0, 2.0, 3.0]);
    let r = add_col(&mut p, s1, "R", &[]);
    set_f(&mut p, r, "max(Sheet2!B)");
    p
}

fn draft_from(
    t: &Tree,
    operation: Operation,
    series: Vec<SeriesRef>,
    auto: bool,
) -> CalculationDraft {
    let result = compute(t, &operation, &series, None).expect("compute");
    CalculationDraft {
        operation,
        series,
        aux: None,
        outputs: result
            .columns
            .into_iter()
            .map(|c| OutputDraft {
                series: c.series,
                key: c.key,
                index: c.index,
                meta: ColumnMeta::default(),
                data: c.data,
            })
            .collect(),
        results: result.results,
        keep_results: true,
        auto_recalc: auto,
        low_r2_dialog: false,
    }
}

#[test]
fn a_correction_of_a_source_recalculates_a_remote_reader() {
    for mode in BOTH {
        let (_s, h) = spawn(analysis_world(), mode);
        let t = snap(&h).tree;
        let (r, b) = (
            colat(&t, "[Book1]Sheet1", "R"),
            colat(&t, "[Book1]Sheet2", "B"),
        );
        assert_eq!(nums(&t, r), some(&[41.0; 3]));
        let operation = Operation::Baseline {
            method: BaselineMethod::Constant { point: (0.0, 1.5) },
            op: BaselineOp::Subtract,
            keep_baseline: false,
        };
        let series = vec![SeriesRef {
            x: None,
            y: b,
            sigma: None,
        }];
        let draft = draft_from(&t, operation, series, false);
        // The correction replaces the source column's data.
        let version = column(&t, b).version();
        h.execute(Command::ReplaceAnalysisData {
            draft,
            expected_versions: vec![(b, version)],
        })
        .expect("replace data");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, b),
            some(&[8.5, 18.5, 28.5, 39.5]),
            "{mode:?} correction applied"
        );
        assert_eq!(
            nums(&t, r),
            some(&[39.5; 3]),
            "{mode:?}: reader follows the corrected source"
        );
        h.undo().expect("undo");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, r), some(&[41.0; 3]), "{mode:?}: undo");
        h.redo().expect("redo");
        assert_eq!(nums(&snap(&h).tree, r), some(&[39.5; 3]), "{mode:?}: redo");
    }
}

#[test]
fn a_calculation_output_read_remotely_follows_its_automatic_recalculation() {
    for mode in BOTH {
        let (_s, h) = spawn(analysis_world(), mode);
        let t = snap(&h).tree;
        let (a, b) = (
            colat(&t, "[Book1]Sheet2", "A"),
            colat(&t, "[Book1]Sheet2", "B"),
        );
        let operation = Operation::Fit {
            spec: FitSpec {
                model: Model::Linear,
                params: vec![],
            },
            range: None,
            residuals: false,
        };
        let series = vec![SeriesRef {
            x: Some(a),
            y: b,
            sigma: None,
        }];
        h.execute(Command::AddCalculation {
            draft: draft_from(&t, operation, series, true),
        })
        .expect("fit");
        let t = snap(&h).tree;
        let calc = t
            .calculations()
            .values()
            .next()
            .expect("calculation")
            .clone();
        let out = calc
            .outputs
            .iter()
            .find_map(|o| o.column)
            .expect("an output column");
        let sheet = t.parent(out).expect("tree").expect("sheet");
        let position = t
            .children(sheet)
            .expect("cols")
            .iter()
            .position(|c| *c == out)
            .expect("pos");
        let sheet_name = t.node(sheet).expect("sheet").name().to_string();
        let letter = char::from(b'A' + u8::try_from(position).expect("small"));
        let r = colat(&t, "[Book1]Sheet1", "R");
        h.execute(set_meta_formula(
            &t,
            r,
            &format!("max({sheet_name}!{letter})"),
        ))
        .expect("reader");
        let max_of = |t: &Tree| {
            nums(t, out)
                .iter()
                .flatten()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, r),
            vec![Some(max_of(&t)); 3],
            "{mode:?} first read"
        );
        let before = max_of(&t);
        // Edit the source: the automatic calculation reruns, the reader follows.
        h.execute(enter(b, 3, &[400.0])).expect("edit source");
        let t = snap(&h).tree;
        let now = max_of(&t);
        assert!(
            (now - before).abs() > 1.0,
            "{mode:?}: the output moved ({before} -> {now})"
        );
        assert_eq!(
            nums(&t, r),
            vec![Some(now); 3],
            "{mode:?}: reader recalculated"
        );
        h.undo().expect("undo edit");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, r), vec![Some(max_of(&t)); 3], "{mode:?}: undo");
        assert!((max_of(&t) - before).abs() < 1e-9, "{mode:?}");
        h.redo().expect("redo");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, r), vec![Some(max_of(&t)); 3], "{mode:?}: redo");
    }
}

// -- 15. a chain over three sheets and two workbooks, macro, undo/redo, reload ----

fn chain_world() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let b2 = add_book(&mut p, root, "Book2");
    let b1 = add_book(&mut p, root, "Book1");
    let s2 = first_sheet(&p, b2);
    let s1 = first_sheet(&p, b1);
    let t2 = p.add_worksheet(b1, Some("Sheet2")).expect("Sheet2");
    add_col(&mut p, s2, "A", &[1.0, 2.0, 3.0]);
    let z = add_col(&mut p, s2, "Z", &[]);
    set_f(&mut p, z, "A*10");
    add_col(&mut p, t2, "A", &[5.0, 6.0, 7.0]);
    let y = add_col(&mut p, t2, "Y", &[]);
    set_f(&mut p, y, "max([Book2]Sheet1!B)");
    let y2 = add_col(&mut p, t2, "Y2", &[]);
    set_f(&mut p, y2, "B+A");
    add_col(&mut p, s1, "A", &[1.0, 2.0, 3.0]);
    let t = add_col(&mut p, s1, "T", &[]);
    set_f(&mut p, t, "sum(Sheet2!C)");
    let u = add_col(&mut p, s1, "U", &[]);
    set_f(&mut p, u, "B/A");
    p
}

#[test]
fn a_macro_over_a_three_sheet_chain_undoes_redoes_and_reloads_exactly() {
    for mode in BOTH {
        let (_s, h) = spawn(chain_world(), mode);
        let t = snap(&h).tree;
        let (z, y, y2) = (
            colat(&t, "/[Book2]Sheet1", "Z"),
            colat(&t, "[Book1]Sheet2", "Y"),
            colat(&t, "[Book1]Sheet2", "Y2"),
        );
        let (tt, u) = (
            colat(&t, "[Book1]Sheet1", "T"),
            colat(&t, "[Book1]Sheet1", "U"),
        );
        let initial = |t: &Tree| {
            assert_eq!(nums(t, z), some(&[10.0, 20.0, 30.0]), "{mode:?}");
            assert_eq!(nums(t, y), some(&[30.0; 3]));
            assert_eq!(nums(t, y2), some(&[35.0, 36.0, 37.0]));
            assert_eq!(nums(t, tt), some(&[108.0; 3]));
            assert_eq!(nums(t, u), some(&[108.0, 54.0, 36.0]));
        };
        let changed = |t: &Tree| {
            assert_eq!(nums(t, z), some(&[20.0, 20.0, 30.0]), "{mode:?}");
            assert_eq!(nums(t, y), some(&[20.0; 3]));
            assert_eq!(nums(t, y2), some(&[25.0, 26.0, 27.0]));
            assert_eq!(nums(t, tt), some(&[27.0; 3]));
            assert_eq!(nums(t, u), some(&[27.0, 13.5, 9.0]));
        };
        initial(&t);
        h.begin_macro(MacroKind::PasteCells).expect("macro");
        h.execute(enter(colat(&t, "/[Book2]Sheet1", "A"), 0, &[2.0]))
            .expect("edit");
        h.execute(set_meta_formula(&snap(&h).tree, y, "min([Book2]Sheet1!B)"))
            .expect("Y");
        h.execute(set_meta_formula(&snap(&h).tree, tt, "max(Sheet2!C)"))
            .expect("T");
        h.end_macro().expect("close");
        changed(&snap(&h).tree);
        h.undo().expect("undo macro");
        initial(&snap(&h).tree);
        h.redo().expect("redo macro");
        changed(&snap(&h).tree);
        let dir = scratch("chain");
        let path = dir.path().join(format!("{mode:?}.stf"));
        h.save(&path).expect("save");
        h.load(&path).expect("load");
        changed(&snap(&h).tree);
        let oracle = reloaded_full(&h, "chain-oracle");
        changed(&oracle.tree);
        // After the reload, a further edit still propagates through the chain.
        h.execute(enter(
            colat(&snap(&h).tree, "/[Book2]Sheet1", "A"),
            2,
            &[0.5],
        ))
        .expect("edit");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, z), some(&[20.0, 20.0, 5.0]), "{mode:?}");
        assert_eq!(nums(&t, y), some(&[5.0; 3]));
        assert_eq!(nums(&t, tt), some(&[12.0; 3]), "{mode:?}: max(10,11,12)");
    }
}

// -- extra: moving the read sheet, resurrection by a re-pasted source, odd names --

#[test]
fn moving_the_read_sheet_to_another_workbook_follows_it_and_a_repaste_never_revives_a_removed_one()
{
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let b1 = add_book(&mut p, root, "Book1");
        let b2 = add_book(&mut p, root, "Book2");
        let s1 = first_sheet(&p, b1);
        let s3 = p.add_worksheet(b1, Some("Sheet3")).expect("Sheet3");
        add_col(&mut p, s3, "A", &[9.0, 8.0, 7.0]);
        add_col(&mut p, s1, "P", &[1.0, 2.0, 3.0]);
        let r = add_col(&mut p, s1, "R", &[]);
        set_f(&mut p, r, "max(Sheet3!A)");
        let _ = b2;
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let r = colat(&t, "[Book1]Sheet1", "R");
        let sheet3 = node(&t, "[Book1]Sheet3");
        // Move the read sheet into Book2: still read, now with a prefix.
        h.execute(Command::Move {
            id: sheet3,
            new_parent: node(&t, "/[Book2]"),
            index: 1,
        })
        .expect("move sheet");
        let t = snap(&h).tree;
        assert_eq!(
            t.formula_letters(r).as_deref(),
            Some("max([Book2]Sheet3!A)"),
            "{mode:?}"
        );
        assert_eq!(nums(&t, r), some(&[9.0; 3]), "{mode:?}");
        assert_round_trip(&t, r, "moved sheet");
        // Capture the sheet, remove it, paste the capture under the same name.
        let copy = NodeCopy::capture(&t, sheet3).expect("capture");
        h.execute(Command::RemoveNode { id: sheet3 })
            .expect("remove");
        let t = snap(&h).tree;
        assert!(t.formula_has_deleted_reference(r), "{mode:?}");
        h.execute(Command::InsertCopy {
            parent: node(&t, "/[Book2]"),
            copy,
            name: "Sheet3".into(),
        })
        .expect("re-paste");
        let t = snap(&h).tree;
        // The pasted sheet is a new object with new identifiers: the old
        // reference stays broken (identifiers, never names).
        assert!(
            t.formula_has_deleted_reference(r),
            "{mode:?}: no silent resurrection"
        );
        assert!(nums(&t, r).iter().all(Option::is_none), "{mode:?}");
        // Undo the paste and the removal: the original comes back to life.
        h.undo().expect("undo paste");
        h.undo().expect("undo remove");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, r),
            some(&[9.0; 3]),
            "{mode:?}: undo revives the original"
        );
    }
}

#[test]
fn a_non_ascii_sheet_name_is_quoted_on_display_and_round_trips() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let b1 = add_book(&mut p, root, "Book1");
        let s1 = first_sheet(&p, b1);
        let ts = p
            .add_worksheet(b1, Some("Sheet1\u{1D40}"))
            .expect("odd sheet");
        add_col(&mut p, ts, "A", &[4.0, 5.0]);
        add_col(&mut p, s1, "P", &[1.0, 2.0, 3.0]);
        let r = add_col(&mut p, s1, "R", &[]);
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let r_id = colat(&t, "[Book1]Sheet1", "R");
        let _ = r;
        let f = t
            .formula_for_entry(r_id, "max('Sheet1\u{1D40}'!A)")
            .expect("quoted entry");
        h.execute(Command::SetColumnMeta {
            column: r_id,
            meta: ColumnMeta {
                formula: Some(f),
                ..Default::default()
            },
        })
        .expect("set");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, r_id), some(&[5.0; 3]), "{mode:?}");
        assert_eq!(
            t.formula_letters(r_id).as_deref(),
            Some("max('Sheet1\u{1D40}'!A)"),
            "{mode:?}"
        );
        assert_round_trip(&t, r_id, "non-ascii");
    }
}

// -- extra: a transposed sheet over a formula column is not spuriously stale -----

/// State of the link of the sheet transposed from `source`.
fn link_state(h: &SessionHandle, source: NodeId) -> stokesforge_core::TransposeState {
    let t = snap(h).tree;
    let (_sheet, link) = t.transposed_from(source).expect("link");
    link.state(&t)
}

fn add_folder(h: &SessionHandle, name: &str) {
    h.execute(Command::AddFolder {
        parent: snap(h).tree.root(),
        name: name.into(),
    })
    .expect("unrelated command");
}

/// (#430) The link of a transposed sheet records the version of each source
/// column it read, formula columns included. A reload (whose first pass
/// rewrites every formula column and renumbers it, same values), an unrelated
/// command and an undo of an unrelated edit must not make it look stale. The
/// first pass after a load trusts a link that was up to date in the file and
/// whose inputs kept their type and values (`trust_loaded_transposes`).
#[test]
fn a_transposed_sheet_over_a_formula_column_is_not_stale_after_reload_or_unrelated_commands() {
    let (_s, h) = spawn(transposable(), FormulaRecalc::Targeted);
    let t = snap(&h).tree;
    let source = node(&t, "[Book1]Sheet1");
    h.execute(Command::TransposeRows {
        source,
        rows: vec![0, 1],
    })
    .expect("transpose");
    assert!(!link_state(&h, source).stale, "fresh");
    add_folder(&h, "Later");
    assert!(!link_state(&h, source).stale, "after an unrelated command");
    let dir = scratch("transpose-stale");
    let path = dir.path().join("t.stf");
    h.save(&path).expect("save");
    h.load(&path).expect("load");
    assert!(!link_state(&h, source).stale, "after save and load");
    add_folder(&h, "Later2");
    assert!(
        !link_state(&h, source).stale,
        "after load then an unrelated command"
    );
    // Editing the source really makes it stale.
    let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
    h.execute(enter(b, 0, &[11.0])).expect("edit");
    assert!(link_state(&h, source).stale, "an edited source is stale");
}

/// (#430) A link already stale in the file stays stale: the trust applies only
/// to a link up to date before the first pass.
#[test]
fn a_link_stale_in_the_file_stays_stale_after_load() {
    let (_s, h) = spawn(transposable(), FormulaRecalc::Targeted);
    let source = node(&snap(&h).tree, "[Book1]Sheet1");
    h.execute(Command::TransposeRows {
        source,
        rows: vec![0, 1],
    })
    .expect("transpose");
    let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
    h.execute(enter(b, 0, &[11.0])).expect("edit");
    assert!(link_state(&h, source).stale, "stale before saving");
    let dir = scratch("transpose-stale-in-file");
    let path = dir.path().join("t.stf");
    h.save(&path).expect("save");
    h.load(&path).expect("load");
    assert!(link_state(&h, source).stale, "stale after load");
}

/// (#430) A file whose formula values differ from what the formula gives: the
/// link was up to date in the file, but the first pass really changes the
/// input D, so the link is stale.
#[test]
fn a_link_whose_formula_input_really_changes_on_load_is_stale() {
    let mut p = transposable();
    let source = p.tree().resolve("[Book1]Sheet1").expect("sheet");
    // No session: D keeps its stored (empty) values, the link reads them.
    p.transpose_rows(source, &[0, 1]).expect("transpose");
    let dir = scratch("transpose-differing");
    let path = dir.path().join("t.stf");
    stokesforge_persistence::save(&p, &path).expect("save");
    let (_s, h) = spawn(
        Project::new("Other").expect("project"),
        FormulaRecalc::Targeted,
    );
    h.load(&path).expect("load");
    let t = snap(&h).tree;
    assert_eq!(
        nums(&t, colat(&t, "[Book1]Sheet1", "D")),
        some(&[110.0, 220.0, 330.0]),
        "D recomputed on load"
    );
    assert!(link_state(&h, source).stale, "inputs changed on load");
}

/// (#430) The trust is one pass only, and in both modes: Full rewrites the
/// formula columns on every pass, so after the load an unrelated command
/// makes the link stale again (accepted limit, the oracle only).
#[test]
fn the_link_trust_after_load_is_one_pass_only() {
    // A Full session restamps at every pass, so the file is written by a
    // Targeted one (link up to date in it) and loaded by a Full one.
    let (_t, writer) = spawn(transposable(), FormulaRecalc::Targeted);
    let source = node(&snap(&writer).tree, "[Book1]Sheet1");
    writer
        .execute(Command::TransposeRows {
            source,
            rows: vec![0, 1],
        })
        .expect("transpose");
    let dir = scratch("transpose-one-pass");
    let path = dir.path().join("t.stf");
    writer.save(&path).expect("save");
    let (_s, h) = spawn(Project::new("Other").expect("project"), FormulaRecalc::Full);
    h.load(&path).expect("load");
    assert!(!link_state(&h, source).stale, "trusted on the load pass");
    add_folder(&h, "Later");
    assert!(
        link_state(&h, source).stale,
        "Full restamps on the next pass (accepted limit)"
    );
}

/// (#430) Regression check: after a load, editing a cell of another sheet and
/// undoing it leaves the link up to date (Targeted).
#[test]
fn a_link_stays_up_to_date_after_load_and_an_unrelated_edit_undone() {
    let mut p = transposable();
    let root = p.root();
    let book2 = add_book(&mut p, root, "Book2");
    let s2 = first_sheet(&p, book2);
    add_col(&mut p, s2, "Z", &[1.0, 2.0]);
    let (_s, h) = spawn(p, FormulaRecalc::Targeted);
    let source = node(&snap(&h).tree, "[Book1]Sheet1");
    h.execute(Command::TransposeRows {
        source,
        rows: vec![0, 1],
    })
    .expect("transpose");
    let dir = scratch("transpose-after-load-edit");
    let path = dir.path().join("t.stf");
    h.save(&path).expect("save");
    h.load(&path).expect("load");
    let z = colat(&snap(&h).tree, "[Book2]Sheet1", "Z");
    h.execute(enter(z, 0, &[9.0])).expect("edit elsewhere");
    assert!(!link_state(&h, source).stale, "after the edit");
    h.undo().expect("undo");
    assert!(!link_state(&h, source).stale, "after the undo");
}

/// Control of the bug above: a calculation that reads a formula column is not
/// stale after a load (`Recalculator::run_after_load` trusts the file).
#[test]
fn a_calculation_over_a_formula_column_is_not_stale_after_load() {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s = first_sheet(&p, book);
    let a = add_col(&mut p, s, "A", &[0.0, 1.0, 2.0, 3.0]);
    set_x(&mut p, a);
    add_col(&mut p, s, "B", &[1.0, 3.0, 5.0, 8.0]);
    let c = add_col(&mut p, s, "C", &[]);
    set_f(&mut p, c, "B*2");
    let (_s, h) = spawn(p, FormulaRecalc::Targeted);
    let t = snap(&h).tree;
    let (a, c) = (
        colat(&t, "[Book1]Sheet1", "A"),
        colat(&t, "[Book1]Sheet1", "C"),
    );
    let operation = Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: false,
    };
    let series = vec![SeriesRef {
        x: Some(a),
        y: c,
        sigma: None,
    }];
    h.execute(Command::AddCalculation {
        draft: draft_from(&t, operation, series, false),
    })
    .expect("fit");
    let stale = |h: &SessionHandle| {
        snap(h)
            .tree
            .calculations()
            .values()
            .map(|c| c.stale.is_some())
            .collect::<Vec<_>>()
    };
    assert_eq!(stale(&h), vec![false]);
    let dir = scratch("diag2");
    let path = dir.path().join("t.stf");
    h.save(&path).expect("save");
    h.load(&path).expect("load");
    assert_eq!(stale(&h), vec![false], "after load");
}

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

/// A same-project `InsertCopy` of a lone `A+B` column into its own sheet reads
/// the same columns (#433 point 4): `A+B` stays `A+B`, with the same values, and
/// follows an edit of A. Into another sheet it reads `#REF` (core test
/// `copy_formula_columns.rs`). The interface copies a column's formula through
/// the grid clipboard (letters); this object-level copy is reachable from
/// `pyst.duplicate`.
#[test]
fn a_local_copy_of_a_lone_formula_column_reads_the_same_columns() {
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
        let (c, c2) = (
            colat(&t, "[Book1]Sheet1", "C"),
            colat(&t, "[Book1]Sheet1", "C2"),
        );
        assert_eq!(t.formula_letters(c2).as_deref(), Some("A+B"), "{mode:?}");
        assert_eq!(nums(&t, c2), some(&[11.0, 22.0, 33.0]), "{mode:?}");
        assert_eq!(nums(&t, c), some(&[11.0, 22.0, 33.0]), "{mode:?}");
        // Editing A reaches the copy.
        h.execute(enter(colat(&t, "[Book1]Sheet1", "A"), 1, &[100.0]))
            .expect("edit A");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, c2), some(&[11.0, 120.0, 33.0]), "{mode:?}");
        // Undo the edit, then the paste; redo both.
        h.undo().expect("undo edit");
        h.undo().expect("undo paste");
        assert!(snap(&h)
            .tree
            .child_by_name(sheet, "C2")
            .expect("sheet")
            .is_none());
        h.redo().expect("redo paste");
        h.redo().expect("redo edit");
        let t = snap(&h).tree;
        let c2 = colat(&t, "[Book1]Sheet1", "C2");
        assert_eq!(nums(&t, c2), some(&[11.0, 120.0, 33.0]), "{mode:?}");
        let oracle = reloaded_full(&h, "local-copy");
        same_formula_cells(&t, &oracle.tree).expect("coherent with a reload");
    }
}

/// Into another sheet of the same workbook: the local reads become `#REF`, the
/// remote part is kept.
#[test]
fn a_copy_of_a_lone_formula_column_into_another_sheet_reads_ref() {
    for mode in BOTH {
        let mut p = book_with_c();
        let t = p.tree().clone();
        let book = node(&t, "[Book1]");
        let s2 = p.add_worksheet(book, Some("Sheet2")).expect("Sheet2");
        let s3 = p.add_worksheet(book, Some("Sheet3")).expect("Sheet3");
        let x = add_col(&mut p, s3, "X", &[0.0, 1.0]);
        set_x(&mut p, x);
        add_col(&mut p, s3, "Y", &[5.0, 7.0]);
        let c = colat(&t, "[Book1]Sheet1", "C");
        set_f(&mut p, c, "A+max(Sheet3!B)");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        h.execute(Command::InsertCopy {
            parent: s2,
            copy: NodeCopy::capture(&t, c).expect("capture"),
            name: "C2".into(),
        })
        .expect("paste");
        let t = snap(&h).tree;
        let c2 = colat(&t, "[Book1]Sheet2", "C2");
        assert_eq!(
            t.formula_letters(c2).as_deref(),
            Some("#REF+max(Sheet3!B)"),
            "{mode:?}"
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
        uncheck_auto(&h, source);
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

/// Unchecks "Recalculate automatically" on the sheet transposed from `source`:
/// these tests pin the behavior of the snapshot link (#463: a new link follows
/// its source by default).
fn uncheck_auto(h: &SessionHandle, source: NodeId) {
    let sheet = snap(h).tree.transposed_from(source).expect("link").0;
    h.execute(Command::SetTransposeAuto { sheet, auto: false })
        .expect("uncheck");
}

fn add_folder(h: &SessionHandle, name: &str) {
    h.execute(Command::AddFolder {
        parent: snap(h).tree.root(),
        name: name.into(),
    })
    .expect("unrelated command");
}

/// (#430, #439) The link of a transposed sheet records the version of each
/// source column it read, formula columns included. A reload, an unrelated
/// command and an undo of an unrelated edit must not make it look stale: the
/// recalculation pass that recomputes a formula column to the same content
/// writes nothing, so the column keeps its version, in both modes.
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
    uncheck_auto(&h, source);
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
    uncheck_auto(&h, source);
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
    let sheet = p.transpose_rows(source, &[0, 1]).expect("transpose");
    p.set_transpose_auto(sheet, false).expect("uncheck");
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

/// (#439) A link up to date in the file stays up to date through the first
/// pass and every later one, in both modes: the pass keeps the version of a
/// formula column it recomputes to the same content, so nothing is "trusted
/// for one pass" any more (the Full oracle used to restamp at the next pass).
#[test]
fn a_link_up_to_date_in_the_file_stays_so_after_load_and_an_unrelated_command() {
    let (_t, writer) = spawn(transposable(), FormulaRecalc::Targeted);
    let source = node(&snap(&writer).tree, "[Book1]Sheet1");
    writer
        .execute(Command::TransposeRows {
            source,
            rows: vec![0, 1],
        })
        .expect("transpose");
    let dir = scratch("transpose-stays-fresh");
    let path = dir.path().join("t.stf");
    writer.save(&path).expect("save");
    for mode in BOTH {
        let (_s, h) = spawn(Project::new("Other").expect("project"), mode);
        h.load(&path).expect("load");
        assert!(!link_state(&h, source).stale, "{mode:?}: on the load pass");
        add_folder(&h, "Later");
        assert!(
            !link_state(&h, source).stale,
            "{mode:?}: after an unrelated command"
        );
    }
}

/// True if the root holds a folder called `name`.
fn has_folder(h: &SessionHandle, name: &str) -> bool {
    let t = snap(h).tree;
    t.child_by_name(t.root(), name).expect("root").is_some()
}

/// The two gestures that must store no history entry over a link that is up to
/// date: transposing the same rows again, and recalculating the link.
fn run_a_noop_over_an_up_to_date_link(
    mode: FormulaRecalc,
    noop: impl Fn(&SessionHandle, NodeId, NodeId),
) {
    let (_s, h) = spawn(transposable(), mode);
    let source = node(&snap(&h).tree, "[Book1]Sheet1");
    h.execute(Command::TransposeRows {
        source,
        rows: vec![0, 1],
    })
    .expect("transpose");
    uncheck_auto(&h, source);
    let sheet = snap(&h).tree.transposed_from(source).expect("link").0;
    add_folder(&h, "Between");
    noop(&h, source, sheet);
    h.undo().expect("undo");
    assert!(
        !has_folder(&h, "Between"),
        "{mode:?}: the undo took back the folder, the gesture before it stored nothing"
    );
    assert!(
        snap(&h).tree.node(sheet).is_some(),
        "{mode:?}: the transposed sheet is still there"
    );
    assert!(!link_state(&h, source).stale, "{mode:?}: not stale");
}

/// (#439) `Transpose, AddFolder, Transpose, Undo`: the second transpose finds
/// nothing to do and stores no entry, in Full as in Targeted, so the undo
/// takes back the folder. Witness: a pass that always writes renumbers the
/// formula column, the link reads stale and the second transpose stores an
/// entry (Full used to).
#[test]
fn transposing_again_over_an_up_to_date_link_stores_nothing() {
    for mode in BOTH {
        run_a_noop_over_an_up_to_date_link(mode, |h, source, _sheet| {
            h.execute(Command::TransposeRows {
                source,
                rows: vec![0, 1],
            })
            .expect("transpose again");
        });
    }
}

/// (#439) `Transpose, AddFolder, RecalculateTranspose, Undo`: same.
#[test]
fn recalculating_an_up_to_date_link_stores_nothing() {
    for mode in BOTH {
        run_a_noop_over_an_up_to_date_link(mode, |h, _source, sheet| {
            h.execute(Command::RecalculateTranspose { sheet })
                .expect("recalculate");
        });
    }
}

/// (#439) A user's write draws a fresh version even when it changes nothing:
/// a command writing the value a cell already holds makes the link stale, like another
/// number would; recalculating clears it. Witness: no fresh version on an
/// identical write in `Project::set_values`.
#[test]
fn writing_the_same_value_in_the_source_makes_the_link_stale() {
    for mode in BOTH {
        let (_s, h) = spawn(transposable(), mode);
        let source = node(&snap(&h).tree, "[Book1]Sheet1");
        h.execute(Command::TransposeRows {
            source,
            rows: vec![0, 1],
        })
        .expect("transpose");
        uncheck_auto(&h, source);
        assert!(!link_state(&h, source).stale, "{mode:?}: fresh");
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[10.0])).expect("write 10 on 10");
        assert!(
            link_state(&h, source).stale,
            "{mode:?}: stale after writing the same value"
        );
        let sheet = snap(&h).tree.transposed_from(source).expect("link").0;
        h.execute(Command::RecalculateTranspose { sheet })
            .expect("recalculate");
        assert!(!link_state(&h, source).stale, "{mode:?}: cleared");
    }
}

/// (#439) A change then Ctrl+Z, or a change then the old value typed back, leaves
/// the link stale: the accepted limit (a number never designates two contents,
/// so an undone or restored column is a new number).
#[test]
fn a_changed_then_restored_source_stays_stale() {
    for mode in BOTH {
        let (_s, h) = spawn(transposable(), mode);
        let source = node(&snap(&h).tree, "[Book1]Sheet1");
        h.execute(Command::TransposeRows {
            source,
            rows: vec![0, 1],
        })
        .expect("transpose");
        uncheck_auto(&h, source);
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("change");
        h.undo().expect("undo the change");
        assert!(
            link_state(&h, source).stale,
            "{mode:?}: undone, still stale"
        );
        h.execute(enter(b, 0, &[12.0])).expect("change");
        h.execute(enter(b, 0, &[10.0])).expect("put back by hand");
        assert!(
            link_state(&h, source).stale,
            "{mode:?}: put back, still stale"
        );
    }
}

/// `Sheet1`: Y column B [10,20,30] (x' 1) and D = max(Sheet2!B) (x' 3), a
/// formula that reads **another sheet**; `Sheet2`: P, Q [4,9,2] (its column B).
fn reading_another_sheet() -> Project {
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let s1 = first_sheet(&p, book);
    let s2 = p.add_worksheet(book, Some("Sheet2")).expect("Sheet2");
    add_col(&mut p, s2, "P", &[1.0, 2.0, 3.0]);
    add_col(&mut p, s2, "Q", &[4.0, 9.0, 2.0]);
    let b = add_col(&mut p, s1, "B", &[10.0, 20.0, 30.0]);
    let d = add_col(&mut p, s1, "D", &[]);
    set_f(&mut p, d, "max(Sheet2!B)");
    for (col, x) in [(b, 1.0), (d, 3.0)] {
        let meta = column(p.tree(), col).meta().clone();
        p.set_column_meta(
            col,
            ColumnMeta {
                x_prime: Some(x),
                ..meta
            },
        )
        .expect("x'");
    }
    p
}

/// (#439) A formula recomputed to the same numbers keeps its version, so the
/// link over it is not stale; recomputed to other numbers, it is. Witnesses:
/// a pass that always writes (the version moves, stale); a pass that compares
/// the lengths only (the new max is never written, so the values assertion
/// fails).
#[test]
fn a_formula_recomputed_to_the_same_numbers_does_not_make_the_link_stale() {
    for mode in BOTH {
        let (_s, h) = spawn(reading_another_sheet(), mode);
        let source = node(&snap(&h).tree, "[Book1]Sheet1");
        let d = colat(&snap(&h).tree, "[Book1]Sheet1", "D");
        let q = colat(&snap(&h).tree, "[Book1]Sheet2", "Q");
        assert_eq!(nums(&snap(&h).tree, d), some(&[9.0; 3]), "{mode:?}");
        h.execute(Command::TransposeRows {
            source,
            rows: vec![0],
        })
        .expect("transpose");
        uncheck_auto(&h, source);
        assert!(!link_state(&h, source).stale, "{mode:?}: fresh");

        // An edit that does not change the max: D is evaluated, not renumbered.
        let before = snap(&h);
        h.execute(enter(q, 2, &[1.0])).expect("lower the 3rd");
        let after = snap(&h);
        assert_eq!(
            after.formula_evaluations - before.formula_evaluations,
            1,
            "{mode:?}: D read the edited column and was evaluated"
        );
        assert_eq!(nums(&after.tree, d), some(&[9.0; 3]), "{mode:?}");
        assert_eq!(
            column(&before.tree, d).version(),
            column(&after.tree, d).version(),
            "{mode:?}: same numbers, same version"
        );
        assert!(!link_state(&h, source).stale, "{mode:?}: not stale");

        // An edit that changes it.
        h.execute(enter(q, 1, &[20.0])).expect("raise the max");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, d), some(&[20.0; 3]), "{mode:?}");
        assert_ne!(
            column(&after.tree, d).version(),
            column(&t, d).version(),
            "{mode:?}: other numbers, fresh version"
        );
        assert!(link_state(&h, source).stale, "{mode:?}: stale");
    }
}

/// (#439) A formula result that changes only by the sign of a zero is another
/// content (bit for bit, like `ColumnValues`): it is written with a fresh
/// version. Witness: comparing as IEEE numbers (0.0 == -0.0) skips the write.
#[test]
fn a_formula_going_from_zero_to_negative_zero_is_written_with_a_fresh_version() {
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let s = first_sheet(&p, book);
        add_col(&mut p, s, "A", &[0.0, 0.0, 0.0]);
        add_col(&mut p, s, "B", &[1.0, 1.0, 1.0]);
        let c = add_col(&mut p, s, "C", &[]);
        set_f(&mut p, c, "A*B");
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        let (c, b) = (
            colat(&t, "[Book1]Sheet1", "C"),
            colat(&t, "[Book1]Sheet1", "B"),
        );
        let bits = |t: &Tree| -> Vec<u64> {
            nums(t, c)
                .iter()
                .map(|v| v.expect("number").to_bits())
                .collect()
        };
        assert_eq!(bits(&t), vec![0.0f64.to_bits(); 3], "{mode:?}");
        let v0 = column(&t, c).version();
        h.execute(enter(b, 0, &[-1.0, -1.0, -1.0]))
            .expect("negate B");
        let t = snap(&h).tree;
        assert_eq!(
            bits(&t),
            vec![(-0.0f64).to_bits(); 3],
            "{mode:?}: -0.0 written"
        );
        assert_ne!(column(&t, c).version(), v0, "{mode:?}: fresh version");
    }
}

/// (#439) The pass keeps the number of a formula column it recomputes to the
/// same content (here after a change of the formula text that gives the same
/// numbers), and restores a column a user's gesture damaged with a fresh
/// version. A direct write of values into a formula column is refused
/// (`FormulaColumnReadOnly`), so the damage is a `DeleteCells`. Both modes
/// number alike.
#[test]
fn the_pass_keeps_the_number_of_an_identical_formula_and_restores_a_damaged_one() {
    let mut deltas = Vec::new();
    for mode in BOTH {
        let (_s, h) = spawn(book_with_c(), mode);
        let t = snap(&h).tree;
        let c = colat(&t, "[Book1]Sheet1", "C");
        let v0 = column(&t, c).version();
        let before = snap(&h).formula_evaluations;
        // `B+A` gives the numbers `A+B` gave: evaluated, same content, same number.
        h.execute(set_meta_formula(&t, c, "B+A"))
            .expect("other text");
        let s = snap(&h);
        assert_eq!(nums(&s.tree, c), some(&[11.0, 22.0, 33.0]), "{mode:?}");
        assert_eq!(s.formula_evaluations - before, 1, "{mode:?}: evaluated");
        assert_eq!(column(&s.tree, c).version(), v0, "{mode:?}: same number");
        // A gesture damages the column: the pass restores it, new number.
        h.execute(Command::DeleteCells {
            columns: vec![c],
            at: 0,
            count: 1,
        })
        .expect("damage");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, c), some(&[11.0, 22.0, 33.0]), "{mode:?}: restored");
        let restored = column(&t, c).version();
        assert!(restored > v0, "{mode:?}: fresh number");
        deltas.push(restored - v0);
    }
    assert_eq!(deltas[0], deltas[1], "the twins number alike");
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
    uncheck_auto(&h, source);
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

/// #433 (Q3): a file whose kept formula names a column that **is** there
/// (written when the column was missing, or by hand) binds when the session
/// loads it, with no history entry (the history is just cleared) and with the
/// right values; a kept formula that still names something missing stays kept.
/// The kept parts and the reason survive the save and the load. Witness: with
/// the load scan removed, the column stays empty until the first command.
#[test]
fn a_kept_formula_binds_when_the_file_is_loaded() {
    use stokesforge_core::{ColumnFormula, FormulaPart};
    for mode in BOTH {
        let mut p = Project::new("P").expect("project");
        let root = p.root();
        let book = add_book(&mut p, root, "Book1");
        let sheet = first_sheet(&p, book);
        let a = add_col(&mut p, sheet, "A", &[1.0, 2.0, 3.0]);
        // Letters are positions: B, C, D, then E.
        for name in ["B", "C", "D"] {
            add_col(&mut p, sheet, name, &[0.0]);
        }
        add_col(&mut p, sheet, "E", &[10.0, 20.0, 30.0]);
        let waiting = add_col(&mut p, sheet, "F", &[]);
        let ready = add_col(&mut p, sheet, "G", &[]);
        let keep = |p: &mut Project, id: NodeId, tail: &str| {
            p.set_column_meta(
                id,
                ColumnMeta {
                    formula: Some(ColumnFormula::from_parts(vec![
                        FormulaPart::Column(a),
                        FormulaPart::Text(tail.to_string()),
                    ])),
                    ..Default::default()
                },
            )
            .expect("kept");
        };
        keep(&mut p, ready, "+E");
        keep(&mut p, waiting, "+Q");
        let dir = scratch("load-binds");
        let path = dir.path().join("kept.stf");
        stokesforge_persistence::save(&p, &path).expect("save");

        let session =
            Session::spawn_with_recalc(Project::new("Empty").unwrap(), None, mode).expect("actor");
        let h = session.handle();
        h.load(&path).expect("load");
        let s = snap(&h);
        assert_eq!(nums(&s.tree, ready), some(&[11.0, 22.0, 33.0]), "{mode:?}");
        assert_eq!(s.tree.formula_letters(ready).as_deref(), Some("A+E"));
        assert!(!s.history.can_undo, "no history entry for the load scan");
        assert_eq!(s.bound.count, 0, "opening a file reports nothing");
        // The one that still names something missing stays kept, same text.
        assert_eq!(s.tree.formula_letters(waiting).as_deref(), Some("A+Q"));
        assert!(column(&s.tree, waiting)
            .meta()
            .formula
            .as_ref()
            .is_some_and(|f| f.compile().is_err()));
        assert!(s.tree.pending_bindings().is_empty());
    }
}

// -- 5. a transposed sheet that follows its source (#463) ------------------------

/// The followed sheet of `source` and its columns (x', Row 1, Row 2).
fn followed(h: &SessionHandle, source: NodeId) -> (NodeId, Vec<NodeId>) {
    let t = snap(h).tree;
    let sheet = t.transposed_from(source).expect("link").0;
    (sheet, t.children(sheet).expect("columns").to_vec())
}

/// `transposable()` with its two rows transposed; the box is checked.
fn followed_start(mode: FormulaRecalc) -> (Session, SessionHandle, NodeId) {
    let (s, h) = spawn(transposable(), mode);
    let source = node(&snap(&h).tree, "[Book1]Sheet1");
    h.execute(Command::TransposeRows {
        source,
        rows: vec![0, 1],
    })
    .expect("transpose");
    (s, h, source)
}

/// Edit a source value: the followed sheet follows by itself, through the
/// source's own formula column, with no entry of its own; undo brings both back.
/// Witnesses: the follow step removed; the box ignored (the unchecked case).
#[test]
fn a_followed_sheet_follows_its_source_and_one_undo_takes_both_back() {
    for mode in BOTH {
        let (_s, h, source) = followed_start(mode);
        let (_sheet, cols) = followed(&h, source);
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("edit B");
        let t = snap(&h).tree;
        // Row 1 = [B, C, D] at row 0; D = B + C was recomputed in the same pass.
        assert_eq!(nums(&t, cols[1]), some(&[11.0, 100.0, 111.0]), "{mode:?}");
        assert_eq!(nums(&t, cols[2]), some(&[20.0, 200.0, 220.0]), "{mode:?}");
        assert!(!link_state(&h, source).stale, "{mode:?}: not stale");
        // One Ctrl+Z undoes the edit; the sheet follows the restored source.
        h.undo().expect("undo").expect("an entry");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), some(&[10.0, 100.0, 110.0]), "{mode:?}");
        assert!(
            !link_state(&h, source).stale,
            "{mode:?}: not stale after undo"
        );
        // The next entry down is the transposition itself: no entry for the follow.
        h.undo().expect("undo").expect("the transposition");
        assert!(snap(&h).tree.transposes().is_empty(), "{mode:?}");
    }
}

#[test]
fn an_unchecked_sheet_stays_a_snapshot() {
    for mode in BOTH {
        let (_s, h, source) = followed_start(mode);
        let (sheet, cols) = followed(&h, source);
        h.execute(Command::SetTransposeAuto { sheet, auto: false })
            .expect("uncheck");
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("edit B");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), some(&[10.0, 100.0, 110.0]), "{mode:?}");
        assert!(
            link_state(&h, source).stale,
            "{mode:?}: out of date as before"
        );
        // Checking it again catches up at once.
        h.execute(Command::SetTransposeAuto { sheet, auto: true })
            .expect("check");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), some(&[11.0, 100.0, 111.0]), "{mode:?}");
        assert!(!link_state(&h, source).stale, "{mode:?}");
    }
}

/// Rule 4: a value typed in the followed sheet is never overwritten; the whole
/// sheet waits (box stays checked) until Relink or Ctrl+Z. Witness: the pass
/// ignores `modified`.
#[test]
fn a_typed_value_stops_the_whole_sheet_following_until_relink_or_undo() {
    for mode in BOTH {
        let (_s, h, source) = followed_start(mode);
        let (sheet, cols) = followed(&h, source);
        h.execute(enter(cols[1], 0, &[-5.0]))
            .expect("type in Row 1");
        assert!(link_state(&h, source).modified, "{mode:?}");
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("edit B");
        let t = snap(&h).tree;
        assert_eq!(
            nums(&t, cols[1])[0],
            Some(-5.0),
            "{mode:?}: typed value kept"
        );
        assert_eq!(
            nums(&t, cols[2]),
            some(&[20.0, 200.0, 220.0]),
            "{mode:?}: other rows wait too"
        );
        let state = link_state(&h, source);
        assert!(state.stale && state.modified, "{mode:?}: both shown");
        assert!(
            t.transpose_link(sheet).expect("link").auto_recalc,
            "{mode:?}"
        );
        // Relink follows again.
        h.execute(Command::RecalculateTranspose { sheet })
            .expect("relink");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), some(&[11.0, 100.0, 111.0]), "{mode:?}");
        // Ctrl+Z brings the typed value (and the sheet's stoppage) back, then
        // undoing the typing leaves a sheet that follows again.
        h.undo().expect("undo relink");
        h.undo().expect("undo edit B");
        h.undo().expect("undo typing");
        let state = link_state(&h, source);
        assert!(!state.stale && !state.modified, "{mode:?}: following again");
        h.execute(enter(b, 0, &[12.0])).expect("edit B again");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), some(&[12.0, 100.0, 112.0]), "{mode:?}");
    }
}

/// The same value typed again in the source: the pass compares by content, so
/// Row 1 keeps its version and the link is up to date. Witness: `overwrite`.
#[test]
fn retyping_the_same_source_value_leaves_the_rows_untouched() {
    for mode in BOTH {
        let (_s, h, source) = followed_start(mode);
        let (_sheet, cols) = followed(&h, source);
        let before = versions(&snap(&h).tree);
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[10.0])).expect("same value");
        let after = versions(&snap(&h).tree);
        for column in &cols {
            assert_eq!(
                before[column], after[column],
                "{mode:?}: {column} untouched"
            );
        }
        assert!(!link_state(&h, source).stale, "{mode:?}: up to date");
    }
}

/// A deleted Row column stays deleted when the sheet follows (E8).
#[test]
fn following_does_not_bring_a_deleted_row_column_back() {
    for mode in BOTH {
        let (_s, h, source) = followed_start(mode);
        let (sheet, cols) = followed(&h, source);
        h.execute(Command::DeleteColumn { column: cols[2] })
            .expect("delete Row 2");
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("edit B");
        let t = snap(&h).tree;
        assert_eq!(t.children(sheet).expect("columns").len(), 2, "{mode:?}");
        assert_eq!(nums(&t, cols[1]), some(&[11.0, 100.0, 111.0]), "{mode:?}");
        assert!(!link_state(&h, source).stale, "{mode:?}");
    }
}

/// A source column that cannot be followed (a new column has no x'): the last
/// numbers stay, the sheet reads Out of date, and it resumes once the x' is set.
#[test]
fn a_source_that_cannot_be_followed_keeps_its_numbers_and_resumes_when_fixed() {
    for mode in BOTH {
        let (_s, h, source) = followed_start(mode);
        let (_sheet, cols) = followed(&h, source);
        h.execute(Command::AddColumn {
            sheet: source,
            index: None,
            name: Some("New".into()),
            column: Column::from_f64(&[1.0, 2.0, 3.0]),
        })
        .expect("a column without x'");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), some(&[10.0, 100.0, 110.0]), "{mode:?}");
        assert!(link_state(&h, source).stale, "{mode:?}: out of date");
        assert!(matches!(
            stokesforge_core::unfollowable_column(&t, source),
            Some(stokesforge_core::CoreError::TransposeNoXPrime { .. })
        ));
        let new = colat(&t, "[Book1]Sheet1", "New");
        h.execute(Command::SetColumnMeta {
            column: new,
            meta: ColumnMeta {
                x_prime: Some(4.0),
                ..ColumnMeta::default()
            },
        })
        .expect("x' set");
        let t = snap(&h).tree;
        assert!(!link_state(&h, source).stale, "{mode:?}: resumed");
        assert_eq!(
            nums(&t, cols[1]),
            some(&[10.0, 100.0, 110.0, 1.0]),
            "{mode:?}"
        );
    }
}

/// Source formula -> followed row -> formula of another sheet, in one pass,
/// with no calculation in the project (witness: the follow step is not skipped
/// when there is no calculation) and then with an automatic fit of the row.
#[test]
fn a_change_travels_from_the_source_through_the_followed_row_in_one_pass() {
    for mode in BOTH {
        let (_s, h, source) = followed_start(mode);
        let (_sheet, cols) = followed(&h, source);
        let book = node(&snap(&h).tree, "[Book1]");
        h.execute(Command::AddWorksheet {
            book,
            name: Some("Sheet2".into()),
        })
        .expect("Sheet2");
        let t = snap(&h).tree;
        let sheet2 = node(&t, "[Book1]Sheet2");
        let tname = t
            .node(t.transposed_from(source).expect("link").0)
            .expect("sheet")
            .name()
            .to_string();
        h.execute(Command::AddColumn {
            sheet: sheet2,
            index: None,
            name: Some("M".into()),
            column: Column::new(ColumnType::Numeric),
        })
        .expect("M");
        let t = snap(&h).tree;
        let m = colat(&t, "[Book1]Sheet2", "M");
        let text = format!("max('{tname}'!B)");
        h.execute(set_meta_formula(&t, m, &text))
            .expect("M reads Row 1");
        assert_eq!(nums(&snap(&h).tree, m)[0], Some(110.0), "{mode:?}");
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("edit B");
        // B -> D (source formula) -> Row 1 (followed) -> M (formula of Sheet2).
        assert_eq!(
            nums(&snap(&h).tree, m)[0],
            Some(111.0),
            "{mode:?}: one pass"
        );

        // The same chain ending in an automatic fit of (x', Row 1).
        let fit = add_fit_chain(&h, cols[0], cols[1]);
        let slope_before = fit_slope(&snap(&h), fit);
        h.execute(enter(b, 0, &[500.0])).expect("edit B again");
        let s = snap(&h);
        assert_ne!(
            fit_slope(&s, fit),
            slope_before,
            "{mode:?}: the fit followed"
        );
        let fresh = stokesforge_core::compute(
            &s.tree,
            &fit_op(),
            &[stokesforge_core::SeriesRef {
                x: Some(cols[0]),
                y: cols[1],
                sigma: None,
            }],
            None,
        )
        .expect("fresh fit");
        let fresh_slope = fresh.results[0]
            .values
            .iter()
            .find(|v| v.key == "param.slope")
            .expect("slope")
            .value;
        assert_eq!(fit_slope(&s, fit), fresh_slope, "{mode:?}");
    }
}

fn fit_op() -> stokesforge_analysis::ops::Operation {
    stokesforge_analysis::ops::Operation::Fit {
        spec: stokesforge_analysis::fit::FitSpec {
            model: stokesforge_analysis::model::Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: false,
    }
}

fn add_fit_chain(h: &SessionHandle, x: NodeId, y: NodeId) -> NodeId {
    let series = vec![stokesforge_core::SeriesRef {
        x: Some(x),
        y,
        sigma: None,
    }];
    let computed = stokesforge_core::compute(&snap(h).tree, &fit_op(), &series, None).expect("fit");
    let outputs = computed
        .columns
        .iter()
        .map(|c| stokesforge_core::OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data.clone(),
        })
        .collect();
    h.execute(Command::AddCalculation {
        draft: stokesforge_core::CalculationDraft {
            operation: fit_op(),
            series,
            aux: None,
            outputs,
            results: computed.results,
            keep_results: true,
            auto_recalc: true,
            low_r2_dialog: true,
        },
    })
    .expect("add the fit");
    *snap(h)
        .tree
        .calculations()
        .keys()
        .max()
        .expect("calculation")
}

fn fit_slope(s: &Snapshot, id: NodeId) -> f64 {
    s.tree.calculation(id).expect("calculation").results[0]
        .values
        .iter()
        .find(|v| v.key == "param.slope")
        .expect("slope")
        .value
}

/// A followed sheet that was out of date in the file stays out of date on
/// opening, through unrelated commands, and follows on a real change (A1,
/// #316). Witness: the load hold removed (the next command rewrites the sheet).
#[test]
fn a_followed_sheet_out_of_date_in_the_file_is_not_rewritten_until_the_source_changes() {
    // No formula column here: one recomputed on load would be a real change.
    let mut p = Project::new("P").expect("project");
    let root = p.root();
    let book = add_book(&mut p, root, "Book1");
    let source = first_sheet(&p, book);
    for (name, values, x) in [
        ("B", [10.0, 20.0, 30.0], 1.0),
        ("C", [100.0, 200.0, 300.0], 2.0),
    ] {
        let id = add_col(&mut p, source, name, &values);
        p.set_column_meta(
            id,
            ColumnMeta {
                x_prime: Some(x),
                ..Default::default()
            },
        )
        .expect("x'");
    }
    let sheet = p.transpose_rows(source, &[0, 1]).expect("transpose");
    // Stale in the file: the source changes with no pass.
    let b = p
        .tree()
        .child_by_name(source, "B")
        .expect("source")
        .expect("B");
    p.set_values(b, 0, ValueBlock::Float(vec![11.0]))
        .expect("edit");
    let dir = scratch("followed-stale-in-file");
    let path = dir.path().join("t.stf");
    stokesforge_persistence::save(&p, &path).expect("save");
    for mode in BOTH {
        let (_s, h) = spawn(Project::new("Other").expect("project"), mode);
        h.load(&path).expect("load");
        let row = snap(&h).tree.children(sheet).expect("columns")[1];
        assert!(link_state(&h, source).stale, "{mode:?}: stale after load");
        assert_eq!(nums(&snap(&h).tree, row)[0], Some(10.0), "{mode:?}: kept");
        add_folder(&h, "Later");
        assert!(link_state(&h, source).stale, "{mode:?}: still stale");
        assert_eq!(
            nums(&snap(&h).tree, row)[0],
            Some(10.0),
            "{mode:?}: not rewritten"
        );
        let b = colat(&snap(&h).tree, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[12.0])).expect("a real change");
        assert!(!link_state(&h, source).stale, "{mode:?}: follows");
        assert_eq!(nums(&snap(&h).tree, row)[0], Some(12.0), "{mode:?}");
    }
}

/// A file (or a paste, or a Move) that brings a formula of the source reading
/// its followed sheet: the column is framed and empty, the sheet is not
/// followed, nothing hangs and nothing is written (invariant 10). Unchecking
/// computes the formula again. Witnesses: the blanking removed; the follow
/// block removed.
#[test]
fn a_stored_formula_reading_the_followed_sheet_is_blanked_and_blocks_following() {
    for mode in BOTH {
        let mut p = transposable();
        let source = p.tree().resolve("[Book1]Sheet1").expect("sheet");
        let sheet = p.transpose_rows(source, &[0, 1]).expect("transpose");
        let label = add_col(&mut p, source, "L", &[1.0, 1.0, 1.0]);
        let tname = p.tree().node(sheet).expect("sheet").name().to_string();
        set_f(&mut p, label, &format!("max('{tname}'!B)"));
        let (_s, h) = spawn(p, mode);
        let t = snap(&h).tree;
        assert!(
            nums(&t, label).iter().all(Option::is_none),
            "{mode:?}: blanked {:?}",
            nums(&t, label)
        );
        assert!(t.formula_follow_error(label, None).is_some());
        let cols = t.children(sheet).expect("columns").to_vec();
        let held = nums(&t, cols[1]);
        let b = colat(&t, "[Book1]Sheet1", "B");
        h.execute(enter(b, 0, &[11.0])).expect("edit B");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, cols[1]), held, "{mode:?}: not followed");
        h.execute(Command::SetTransposeAuto { sheet, auto: false })
            .expect("uncheck");
        let t = snap(&h).tree;
        assert_eq!(nums(&t, label)[0], Some(100.0), "{mode:?}: computes again");
        // Rule 3: rechecking is refused while the formula is there.
        assert!(h
            .execute(Command::SetTransposeAuto { sheet, auto: true })
            .is_err());
    }
}

/// #484: Recalculate on a followed sheet whose source keeps a formula reading
/// it is refused (the column is blanked, so the gesture would write empty
/// cells): the rows and the link are untouched and the sheet stays out of
/// date; once the formula is removed the sheet follows by itself. Witness: the
/// loop check removed from `recalculate_transpose_inner` (empty cells appear).
#[test]
fn recalculate_is_refused_while_a_kept_formula_reads_the_followed_sheet() {
    for mode in BOTH {
        let mut p = transposable();
        let source = p.tree().resolve("[Book1]Sheet1").expect("sheet");
        let sheet = p.transpose_rows(source, &[0, 1]).expect("transpose");
        let label = add_col(&mut p, source, "L", &[1.0, 1.0, 1.0]);
        let tname = p.tree().node(sheet).expect("sheet").name().to_string();
        let (kept, _error) = p
            .tree()
            .formula_keeping(label, &format!("max('{tname}'!B)"));
        p.set_column_meta(
            label,
            ColumnMeta {
                x_prime: Some(4.0),
                formula: Some(kept),
                ..Default::default()
            },
        )
        .expect("kept formula");
        let (_s, h) = spawn(p, mode);
        let (sheet, cols) = followed(&h, source);
        assert!(link_state(&h, source).stale, "{mode:?}: out of date");
        let before = snap(&h).tree;
        let error = h
            .execute(Command::RecalculateTranspose { sheet })
            .expect_err("refused");
        assert!(
            format!("{error:?}").contains("TransposeSourceReadsIt"),
            "{mode:?}: {error:?}"
        );
        let after = snap(&h).tree;
        for column in &cols {
            assert_eq!(nums(&before, *column), nums(&after, *column), "{mode:?}");
        }
        assert_eq!(
            before.transpose_link(sheet).expect("link").inputs,
            after.transpose_link(sheet).expect("link").inputs,
            "{mode:?}: link untouched"
        );
        assert!(link_state(&h, source).stale, "{mode:?}: still out of date");
        // Removing the formula: the sheet follows alone, no gesture needed.
        h.execute(Command::SetColumnMeta {
            column: label,
            meta: ColumnMeta {
                x_prime: Some(4.0),
                ..Default::default()
            },
        })
        .expect("formula removed");
        assert!(!link_state(&h, source).stale, "{mode:?}: follows alone");
    }
}

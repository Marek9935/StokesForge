//! Where a lone formula column reads after a copy (#433 point 4): the same
//! columns when pasted into the sheet it was copied from, in the same project;
//! `#REF` into another sheet or from another project.

use stokesforge_core::{Column, ColumnMeta, CopyInsertion, NodeCopy, NodeId, Project};

struct World {
    project: Project,
    sheet1: NodeId,
    sheet2: NodeId,
    c: NodeId,
}

/// `Sheet1`: A, B, C = `A+B`, D = `A`; `Sheet2` and `Sheet3` (with a column B).
fn world(source: &str) -> World {
    let mut project = Project::new("P").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("book");
    let sheet1 = project.tree().children(book).expect("sheets")[0];
    let sheet2 = project.add_worksheet(book, Some("Sheet2")).expect("Sheet2");
    let sheet3 = project.add_worksheet(book, Some("Sheet3")).expect("Sheet3");
    project
        .add_column(sheet3, None, Some("B"), Column::from_f64(&[1.0]))
        .expect("Sheet3 B");
    for name in ["A", "B"] {
        project
            .add_column(sheet1, None, Some(name), Column::from_f64(&[1.0, 2.0]))
            .expect("column");
    }
    let c = add_formula(&mut project, sheet1, "C", source);
    World {
        project,
        sheet1,
        sheet2,
        c,
    }
}

fn add_formula(project: &mut Project, sheet: NodeId, name: &str, text: &str) -> NodeId {
    let column = project
        .add_column(sheet, None, Some(name), Column::from_f64(&[]))
        .expect("column");
    let formula = project
        .tree()
        .formula_from_letters(column, text)
        .expect("formula");
    project
        .set_column_meta(
            column,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .expect("meta");
    column
}

fn letters(project: &Project, column: NodeId) -> String {
    project.tree().formula_letters(column).expect("formula")
}

#[test]
fn a_lone_column_pasted_into_its_own_sheet_reads_the_same_columns() {
    let mut w = world("A+B");
    let copy = NodeCopy::capture(w.project.tree(), w.c).expect("capture");
    let original = w
        .project
        .tree()
        .node(w.c)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .formula
        .clone();
    let c2 = w.project.insert_copy(w.sheet1, &copy, "C2").expect("paste");
    assert_eq!(letters(&w.project, c2), "A+B");
    let pasted = w
        .project
        .tree()
        .node(c2)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .formula
        .clone();
    assert_eq!(pasted, original, "the very same identifiers");
}

#[test]
fn a_lone_column_pasted_into_another_sheet_reads_ref_and_keeps_remote_reads() {
    let mut w = world("A+max(Sheet3!A)");
    let copy = NodeCopy::capture(w.project.tree(), w.c).expect("capture");
    let c2 = w.project.insert_copy(w.sheet2, &copy, "C2").expect("paste");
    assert_eq!(letters(&w.project, c2), "#REF+max(Sheet3!A)");
    // Not only displayed as `#REF`: the local read is a reservation, never the
    // original column (which could otherwise come back when moved there).
    let read = |column: NodeId| {
        let node = w.project.tree().node(column).unwrap();
        node.as_column()
            .unwrap()
            .meta()
            .formula
            .clone()
            .unwrap()
            .columns()
    };
    let local = w
        .project
        .tree()
        .child_by_name(w.sheet1, "A")
        .unwrap()
        .unwrap();
    assert!(
        !read(c2).contains(&local),
        "reads a reservation, not the original"
    );
}

#[test]
fn an_external_paste_is_never_the_same_sheet_case() {
    let mut w = world("A+B");
    let copy = NodeCopy::capture(w.project.tree(), w.c)
        .expect("capture")
        .into_external();
    // Same project, same sheet, same identifiers: still `#REF` when external.
    let c2 = w.project.insert_copy(w.sheet1, &copy, "C2").expect("paste");
    assert_eq!(letters(&w.project, c2), "#REF+#REF");
}

/// Two roots read A, one pasted into its sheet and one into another, in both
/// orders: a reservation made for one must not leak into the other.
#[test]
fn two_roots_reading_a_column_go_each_their_way_in_either_order() {
    for flip in [false, true] {
        let mut w = world("A+B");
        let d = add_formula(&mut w.project, w.sheet1, "D", "A");
        let e = add_formula(&mut w.project, w.sheet1, "E", "A");
        let copies = NodeCopy::capture_all(w.project.tree(), &[d, e]).expect("capture");
        let mut items = vec![
            CopyInsertion {
                parent: w.sheet1,
                copy: copies[0].clone(),
                name: "D2".into(),
            },
            CopyInsertion {
                parent: w.sheet2,
                copy: copies[1].clone(),
                name: "E2".into(),
            },
        ];
        if flip {
            items.reverse();
        }
        let roots = w.project.insert_copies(&items).expect("paste");
        let by_name = |name: &str| {
            let at = items.iter().position(|i| i.name == name).unwrap();
            letters(&w.project, roots[at])
        };
        assert_eq!(by_name("D2"), "A", "flip {flip}");
        assert_eq!(by_name("E2"), "#REF", "flip {flip}");
    }
}

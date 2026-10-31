//! Formulas that read another sheet (#403), batch 1: the `.stf` does not carry
//! the sheet of a remote column yet, so a saved remote formula comes back
//! reading a column that is not in its own sheet. It must not crash, and it
//! shows `#REF`.
//!
//! Batch 2 adds the sheet identifier to the column record; this test then flips:
//! the reloaded formula reads `Sheet3!B` again.

use stokesforge_core::{Column, ColumnMeta, Project};
use stokesforge_persistence::{load, save};

#[test]
fn a_remote_formula_reloads_as_ref_without_crashing() {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet1 = project.add_worksheet(book, Some("Sheet1")).expect("sheet");
    let sheet3 = project.add_worksheet(book, Some("Sheet3")).expect("sheet");
    project
        .add_column(sheet3, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("column");
    let b = project
        .add_column(sheet3, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
        .expect("column");
    let c = project
        .add_column(sheet1, None, Some("A"), Column::from_f64(&[0.0, 0.0]))
        .expect("column");
    let formula = project
        .tree()
        .formula_from_letters(c, "max(Sheet3!B)")
        .expect("remote formula entered");
    assert_eq!(formula.remote_sheets().get(&b), Some(&sheet3));
    project
        .set_column_meta(
            c,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .expect("formula set");
    assert_eq!(
        project.tree().formula_letters(c).as_deref(),
        Some("max(Sheet3!B)")
    );

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("remote.stf");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");

    // The column read is kept by identifier (invariant 7)...
    let reloaded = loaded
        .tree()
        .node(c)
        .and_then(|node| node.as_column())
        .and_then(|column| column.meta().formula.clone())
        .expect("the formula is kept");
    assert!(reloaded.columns().contains(&b));
    // ...but its sheet is not written yet: the reference reads as deleted.
    assert!(reloaded.remote_refs().is_empty());
    assert_eq!(
        loaded.tree().formula_letters(c).as_deref(),
        Some("max(#REF)")
    );
    assert!(loaded.tree().formula_has_deleted_reference(c));
}

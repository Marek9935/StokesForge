//! Formulas that read another sheet (#403), persistence: the `.stf` carries
//! the sheet of a remote column in the additive `formula_remote` side field, so
//! a saved remote formula reloads reading the same sheet. A project without
//! remote reference writes no such field (its bytes are those of before).

use stokesforge_core::{Column, ColumnMeta, Project};
use stokesforge_persistence::{load, save};

#[test]
fn a_remote_formula_keeps_its_reference_through_save_and_reload() {
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

    let reloaded = loaded
        .tree()
        .node(c)
        .and_then(|node| node.as_column())
        .and_then(|column| column.meta().formula.clone())
        .expect("the formula is kept");
    let original = project
        .tree()
        .node(c)
        .and_then(|node| node.as_column())
        .and_then(|column| column.meta().formula.clone())
        .expect("the formula is set");
    assert_eq!(reloaded, original);
    assert_eq!(reloaded.remote_sheets().get(&b), Some(&sheet3));
    assert_eq!(
        loaded.tree().formula_letters(c).as_deref(),
        Some("max(Sheet3!B)")
    );
    assert!(!loaded.tree().formula_has_deleted_reference(c));
}

#[test]
fn a_project_without_remote_reference_writes_no_remote_field() {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet = project.add_worksheet(book, Some("Sheet1")).expect("sheet");
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column");
    let c = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[0.0]))
        .expect("column");
    let formula = project
        .tree()
        .formula_from_letters(c, "A*2")
        .expect("formula entered");
    project
        .set_column_meta(
            c,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .expect("formula set");
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("local.stf");
    save(&project, &path).expect("save succeeded");
    let bytes = std::fs::read(&path).expect("read");
    let needle = b"formula_remote";
    assert!(!bytes.windows(needle.len()).any(|window| window == needle));
}

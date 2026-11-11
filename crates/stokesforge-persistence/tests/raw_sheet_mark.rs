//! The mark of an untouched import sheet ("Raw Data", #488) survives save and
//! reopen; an unmarked sheet writes no field, so older files are unchanged.

use stokesforge_core::{Column, Project, RawSheetMark};
use stokesforge_persistence::{load, save};

const FIELD: &[u8] = b"raw_modified_base";

fn project(mark: Option<&str>) -> (Project, stokesforge_core::NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("book");
    let sheet = project.tree().children(book).expect("sheets")[0];
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column");
    project
        .set_raw_mark(
            sheet,
            mark.map(|base| RawSheetMark {
                modified_base: base.into(),
            }),
        )
        .expect("mark");
    (project, sheet)
}

fn contains(bytes: &[u8]) -> bool {
    bytes.windows(FIELD.len()).any(|window| window == FIELD)
}

#[test]
fn the_mark_and_its_base_survive_save_and_reopen() {
    let (project, sheet) = project(Some("Feuille"));
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("marked.stf");
    save(&project, &path).expect("save");
    assert!(contains(&std::fs::read(&path).expect("read")));
    let loaded = load(&path).expect("load");
    assert_eq!(
        loaded.raw_mark(sheet).map(|m| m.modified_base.as_str()),
        Some("Feuille")
    );
    assert!(*loaded.tree() == *project.tree());
}

#[test]
fn an_unmarked_sheet_writes_no_field_and_reads_back_without_a_mark() {
    let (project, sheet) = project(None);
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("plain.stf");
    save(&project, &path).expect("save");
    assert!(!contains(&std::fs::read(&path).expect("read")));
    assert_eq!(load(&path).expect("load").raw_mark(sheet), None);
}

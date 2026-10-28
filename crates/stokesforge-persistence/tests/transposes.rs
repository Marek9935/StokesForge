//! Linked transposed sheets (#253) go through the `.stf` file: the link,
//! its fingerprints, and the derived states (stale, modified) are the same
//! after a round trip.

use stokesforge_core::{Column, ColumnMeta, NodeId, Project, ValueBlock};
use stokesforge_persistence::{load, save};

fn project() -> (Project, NodeId, NodeId) {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let source = project.tree().children(book).unwrap()[0];
    for i in 0..3 {
        let mut column = Column::from_f64(&[1.0 + f64::from(i), 5.0]);
        column.set_meta(ColumnMeta {
            x_prime: Some(f64::from(i) * 10.0),
            ..ColumnMeta::default()
        });
        project
            .add_column(source, None, Some(&format!("C{i}")), column)
            .unwrap();
    }
    let sheet = project.transpose_rows(source, &[0, 1]).unwrap();
    (project, source, sheet)
}

fn state(project: &Project, sheet: NodeId) -> (bool, bool) {
    let s = project
        .tree()
        .transpose_link(sheet)
        .unwrap()
        .state(project.tree());
    (s.stale, s.modified)
}

#[test]
fn the_link_and_its_derived_states_survive_a_round_trip() {
    let (mut project, source, sheet) = project();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.stf");
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    assert_eq!(back.tree().transposes(), project.tree().transposes());
    assert_eq!(state(&back, sheet), (false, false));

    // Stale and modified: the state is found again, nothing is stored as is.
    let first = project.tree().children(source).unwrap()[0];
    project
        .set_values(first, 0, ValueBlock::Float(vec![99.0]))
        .unwrap();
    let row = project.tree().children(sheet).unwrap()[1];
    project
        .set_values(row, 0, ValueBlock::Float(vec![-3.0]))
        .unwrap();
    assert_eq!(state(&project, sheet), (true, true));
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    assert_eq!(state(&back, sheet), (true, true));
    assert_eq!(
        back.tree().transposed_from(source).map(|(s, _)| s),
        Some(sheet)
    );
}

#[test]
fn two_saves_of_the_same_project_have_the_same_links() {
    let (project, _, _) = project();
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.stf"), dir.path().join("b.stf"));
    save(&project, &a).unwrap();
    save(&project, &b).unwrap();
    assert_eq!(
        load(&a).unwrap().tree().transposes(),
        load(&b).unwrap().tree().transposes()
    );
}

//! Smoke test of the model: build, clone, snapshot and destroy realistic
//! projects without panicking or leaking. This is the "0.5" of phase 0: it
//! is written to run one day under `miri`, so it uses only the public
//! API and stays small in computing time.

use std::sync::Arc;

use stokesforge_core::{Column, ColumnType, NodeId, Project, Tree, ValueBlock, WindowMeta};

#[test]
fn an_empty_project_tree_clones_and_drops() {
    // B9 (`guide-de-correction-robustesse.md` §F2) — `Project` is no longer
    // `Clone`: `#[derive(Clone)]` duplicated `IdAllocator` with the tree, so
    // that two copies of the same project then assigned the same
    // identifiers to different nodes. What this smoke test really meant
    // to cover — a shared structure that survives the destruction
    // of what produced it — still holds for `Arc<Tree>`, which is the only
    // type meant to be cloned here (free, a reference counter).
    let project = Project::new("Empty").expect("valid name");
    let tree = Arc::clone(project.tree());
    assert_eq!(tree.len(), project.tree().len());
    drop(project);
    // The shared tree stays usable after the project is destroyed.
    assert_eq!(tree.get(tree.root()).expect("root").name(), "Empty");
}

#[test]
fn a_realistic_project_supports_snapshots_and_mutations() {
    let mut project = Project::new("Campaign").expect("valid name");
    let root = project.root();
    let folder = project.add_folder(root, "Experiments").expect("folder");

    let mut columns: Vec<NodeId> = Vec::new();
    let mut sheets: Vec<NodeId> = Vec::new();
    for _ in 0..3 {
        let book = project.add_empty_workbook(folder, None).expect("workbook");
        for _ in 0..2 {
            let sheet = project.add_worksheet(book, None).expect("sheet");
            sheets.push(sheet);
            for _ in 0..5 {
                let values: Vec<f64> = (0..1000).map(|row| row as f64).collect();
                let column = project
                    .add_column(sheet, None, None, Column::from_f64(&values))
                    .expect("column");
                columns.push(column);
            }
        }
    }
    assert_eq!(columns.len(), 30);
    assert_eq!(project.tree().len(), 1 + 1 + 3 + 6 + 30);

    // Ten mutations, a snapshot between each.
    let mut snapshots: Vec<Arc<Tree>> = Vec::new();
    for step in 0..10 {
        snapshots.push(project.tree().clone());
        match step % 5 {
            0 => {
                project
                    .set_values(columns[step], 0, ValueBlock::Float(vec![step as f64; 16]))
                    .expect("write");
            }
            1 => {
                project
                    .rename(columns[step], &format!("Measurement{step}"))
                    .expect("rename");
            }
            2 => {
                project
                    .insert_rows(sheets[step % sheets.len()], 10, 5)
                    .expect("row insertion");
            }
            3 => {
                project
                    .add_column(
                        sheets[step % sheets.len()],
                        None,
                        None,
                        Column::new(ColumnType::Numeric),
                    )
                    .expect("column");
            }
            _ => {
                let note = project.add_note(root, None, "day's reading").expect("note");
                project
                    .set_window_meta(
                        note,
                        WindowMeta {
                            label: format!("Note {step}"),
                            comment: String::new(),
                            ..Default::default()
                        },
                    )
                    .expect("metadata");
            }
        }
    }
    assert_eq!(snapshots.len(), 10);

    // Each snapshot stays readable and consistent.
    for snapshot in &snapshots {
        assert!(snapshot.contains(root));
        assert_eq!(snapshot.walk_all().len(), snapshot.len());
    }

    // The oldest snapshot is restorable; the counter does not go back.
    let hint = project.next_id_hint();
    let oldest = snapshots[0].clone();
    project.replace_tree(oldest);
    assert_eq!(project.next_id_hint(), hint);
    assert!(project.add_folder(root, "After").expect("folder").raw() >= hint);

    drop(snapshots);
    drop(project);
}

//! The order of the view tables does not depend on the hash seed: two
//! saves of the same project give the same bytes (2d, #323).

use std::collections::HashMap;

use stokesforge_core::{NodeId, Project};
use stokesforge_persistence::{
    save_with_geometries, ViewTables, WindowGeometry, WindowMode, WindowView,
};

const WINDOWS: usize = 20;

/// A project of twenty workbooks, and view tables that cite them all.
/// Each call rebuilds fresh `HashMap`s (hence other seeds).
fn project_and_views() -> (Project, ViewTables) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let ids: Vec<NodeId> = (0..WINDOWS)
        .map(|_| project.add_empty_workbook(root, None).expect("workbook"))
        .collect();
    let mut window_geometries = HashMap::new();
    let mut column_widths = HashMap::new();
    for (rank, id) in ids.iter().enumerate() {
        window_geometries.insert(
            *id,
            WindowGeometry {
                x: rank as f64,
                y: 2.0 * rank as f64,
                width: 300.0,
                height: 200.0,
                view: Some(WindowView {
                    mode: WindowMode::Open,
                    collapsed: false,
                    stack: rank as u32,
                }),
            },
        );
        column_widths.insert(*id, 40.0 + rank as f32);
    }
    (
        project,
        ViewTables {
            window_geometries,
            column_widths,
        },
    )
}

#[test]
fn two_saves_give_the_same_bytes() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let mut written = Vec::new();
    for n in 0..4 {
        let (project, views) = project_and_views();
        let path = dir.path().join(format!("{n}.stf"));
        save_with_geometries(&project, &views, &path).expect("save");
        written.push(std::fs::read(&path).expect("read"));
    }
    assert!(
        written.windows(2).all(|pair| pair[0] == pair[1]),
        "the view tables change the byte order from one save to the next"
    );
}

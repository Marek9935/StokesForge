//! L'ordre des tables de vue ne dépend pas de la graine de hachage : deux
//! enregistrements d'un même projet donnent les mêmes octets (2d, #323).

use std::collections::HashMap;

use stokesforge_core::{NodeId, Project};
use stokesforge_persistence::{
    save_with_geometries, ViewTables, WindowGeometry, WindowMode, WindowView,
};

const WINDOWS: usize = 20;

/// Un projet de vingt classeurs, et des tables de vue qui les citent tous.
/// Chaque appel reconstruit des `HashMap` neuves (donc d'autres graines).
fn project_and_views() -> (Project, ViewTables) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    let ids: Vec<NodeId> = (0..WINDOWS)
        .map(|_| project.add_empty_workbook(root, None).expect("classeur"))
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
fn deux_enregistrements_donnent_les_memes_octets() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let mut written = Vec::new();
    for n in 0..4 {
        let (project, views) = project_and_views();
        let path = dir.path().join(format!("{n}.stf"));
        save_with_geometries(&project, &views, &path).expect("enregistrement");
        written.push(std::fs::read(&path).expect("lecture"));
    }
    assert!(
        written.windows(2).all(|pair| pair[0] == pair[1]),
        "les tables de vue changent l'ordre des octets d'un enregistrement à l'autre"
    );
}

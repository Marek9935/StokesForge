//! Global cell budget of a `.stf` at open (B1, #316).

use stokesforge_core::{Column, Project};
use stokesforge_persistence::{load_with_limits, save, LoadLimits, PersistenceError};

/// Two columns of 100 and 50 values: 150 declared cells.
fn write_file(dir: &std::path::Path) -> std::path::PathBuf {
    let mut project = Project::new("P").unwrap();
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0; 100]))
        .unwrap();
    project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[2.0; 50]))
        .unwrap();
    let path = dir.join("p.stf");
    save(&project, &path).unwrap();
    path
}

fn limits(max_cells: u64) -> LoadLimits {
    LoadLimits {
        max_cells,
        ..LoadLimits::default()
    }
}

#[test]
fn the_total_of_the_columns_is_counted_and_the_exact_limit_passes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(dir.path());

    // Exactly the total: it opens (witness: `>` changed to `>=`).
    assert!(load_with_limits(&path, limits(150)).is_ok());

    // One cell fewer: refused, by the sum of the two columns (neither
    // reaches 149 alone).
    match load_with_limits(&path, limits(149)) {
        Err(PersistenceError::TooManyCells { cells, limit, .. }) => {
            assert_eq!(limit, 149);
            assert_eq!(cells, 150);
        }
        other => panic!("TooManyCells expected, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn the_default_is_one_billion() {
    assert_eq!(LoadLimits::default().max_cells, 1_000_000_000);
}

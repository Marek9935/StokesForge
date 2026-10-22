//! Budget global de cellules d'un `.stf` à l'ouverture (B1, #316).

use stokesforge_core::{Column, Project};
use stokesforge_persistence::{load_with_limits, save, LoadLimits, PersistenceError};

/// Deux colonnes de 100 et 50 valeurs : 150 cellules déclarées.
fn fichier(dir: &std::path::Path) -> std::path::PathBuf {
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

fn limites(max_cells: u64) -> LoadLimits {
    LoadLimits {
        max_cells,
        ..LoadLimits::default()
    }
}

#[test]
fn le_total_des_colonnes_est_compte_et_la_limite_exacte_passe() {
    let dir = tempfile::tempdir().unwrap();
    let path = fichier(dir.path());

    // Exactement le total : s'ouvre (témoin : `>` changé en `>=`).
    assert!(load_with_limits(&path, limites(150)).is_ok());

    // Une cellule de moins : refusé, par la somme des deux colonnes (aucune
    // des deux n'atteint seule 149).
    match load_with_limits(&path, limites(149)) {
        Err(PersistenceError::TooManyCells { cells, limit, .. }) => {
            assert_eq!(limit, 149);
            assert_eq!(cells, 150);
        }
        other => panic!("TooManyCells attendu, obtenu {:?}", other.map(|_| ())),
    }
}

#[test]
fn le_defaut_est_un_milliard() {
    assert_eq!(LoadLimits::default().max_cells, 1_000_000_000);
}

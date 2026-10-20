//! Plafond de taille d'un `.stf` à l'ouverture (A2, #316).

use stokesforge_core::Project;
use stokesforge_persistence::{load_with_limit, save, PersistenceError};

fn fichier(dir: &std::path::Path) -> (std::path::PathBuf, u64) {
    let project = Project::new("P").unwrap();
    let path = dir.join("p.stf");
    save(&project, &path).unwrap();
    let size = std::fs::metadata(&path).unwrap().len();
    (path, size)
}

#[test]
fn un_fichier_d_un_octet_de_trop_est_refuse_et_a_la_limite_exacte_il_s_ouvre() {
    let dir = tempfile::tempdir().unwrap();
    let (path, size) = fichier(dir.path());

    // Exactement la limite : s'ouvre (témoin : `>` changé en `>=`).
    assert!(load_with_limit(&path, size).is_ok());

    // Un octet de moins que le fichier : refusé, avec les deux tailles.
    match load_with_limit(&path, size - 1) {
        Err(PersistenceError::TooLarge {
            size: found, limit, ..
        }) => {
            assert_eq!(found, size);
            assert_eq!(limit, size - 1);
        }
        other => panic!("TooLarge attendu, obtenu {:?}", other.map(|_| ())),
    }
}

#[test]
fn le_refus_ne_lit_rien_d_un_fichier_qui_n_est_meme_pas_stokesforge() {
    // Trop grand prime sur « pas un fichier StokesForge » : le contenu n'est
    // jamais regardé.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gros.stf");
    std::fs::write(&path, vec![0u8; 100]).unwrap();
    assert!(matches!(
        load_with_limit(&path, 99),
        Err(PersistenceError::TooLarge { size: 100, .. })
    ));
    assert!(matches!(
        load_with_limit(&path, 100),
        Err(PersistenceError::NotAStokesForgeFile { .. })
    ));
}

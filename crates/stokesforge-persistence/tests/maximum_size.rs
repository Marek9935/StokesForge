//! Size cap of a `.stf` at open (A2, #316).

use stokesforge_core::Project;
use stokesforge_persistence::{load_with_limit, save, PersistenceError};

fn write_file(dir: &std::path::Path) -> (std::path::PathBuf, u64) {
    let project = Project::new("P").unwrap();
    let path = dir.join("p.stf");
    save(&project, &path).unwrap();
    let size = std::fs::metadata(&path).unwrap().len();
    (path, size)
}

#[test]
fn a_file_one_byte_too_large_is_refused_and_at_the_exact_limit_it_opens() {
    let dir = tempfile::tempdir().unwrap();
    let (path, size) = write_file(dir.path());

    // Exactly the limit: it opens (witness: `>` changed to `>=`).
    assert!(load_with_limit(&path, size).is_ok());

    // One byte less than the file: refused, with both sizes.
    match load_with_limit(&path, size - 1) {
        Err(PersistenceError::TooLarge {
            size: found, limit, ..
        }) => {
            assert_eq!(found, size);
            assert_eq!(limit, size - 1);
        }
        other => panic!("TooLarge expected, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn the_refusal_reads_nothing_of_a_file_that_is_not_even_stokesforge() {
    // Too large takes precedence over "not a StokesForge file": the content
    // is never looked at.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.stf");
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

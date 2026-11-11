// SPDX-License-Identifier: GPL-3.0-or-later

//! Local check on real Origin files that are kept private (never committed).
//! Ignored by default; run it with
//! `cargo test -p stokesforge-import --test origin_private -- --ignored`.
//! The folder comes from `STOKESFORGE_PRIVATE_OPJ_DIR` (default
//! `test/private_file/` of the main checkout). The test passes with a message
//! when the folder is absent. Only counts are asserted here.

use std::path::PathBuf;

use stokesforge_import::origin::{read_project, OriginProject};

fn counts(project: &OriginProject) -> (usize, usize, usize) {
    let sheets = project.books.iter().map(|b| b.sheets.len()).sum();
    let columns = project
        .books
        .iter()
        .flat_map(|b| &b.sheets)
        .map(|s| s.columns.len())
        .sum();
    (project.books.len(), sheets, columns)
}

#[test]
#[ignore = "needs the private Origin files"]
fn private_horiba_projects_read_completely() {
    let dir = std::env::var_os("STOKESFORGE_PRIVATE_OPJ_DIR").map_or_else(
        || PathBuf::from("/home/aliocha/dev_project/StokesForge/test/private_file/"),
        PathBuf::from,
    );
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!(
            "no private Origin folder at {}: nothing checked",
            dir.display()
        );
        return;
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("opj") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("readable file");
        let project = read_project(&bytes).expect("the private file reads");
        for book in &project.books {
            let sheets: Vec<_> = book.sheets.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(sheets, ["Graph", "Data", "Note"], "sheets of a book");
            let data = &book.sheets[1];
            let x = data
                .columns
                .iter()
                .find(|c| c.designation == stokesforge_import::origin::OriginDesignation::X)
                .expect("an X column in the Data sheet");
            assert_eq!(x.unit, "nm");
            assert!(!x.values.is_empty());
            assert!(!book.info.is_empty(), "the Horiba tree of a book");
        }
        found.push(counts(&project));
    }
    if found.is_empty() {
        eprintln!("no .opj file in {}: nothing checked", dir.display());
        return;
    }
    found.sort_unstable();
    assert_eq!(found, [(12, 36, 634), (19, 57, 1005)]);
}

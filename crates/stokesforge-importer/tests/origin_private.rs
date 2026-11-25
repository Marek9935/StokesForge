// SPDX-License-Identifier: GPL-3.0-or-later

//! Local check of the Origin column formulas (#556) on real Origin files that
//! are kept private (never committed). Ignored by default; run it with
//! `cargo test -p stokesforge-importer --test origin_private -- --ignored --nocapture`.
//! The folder comes from `STOKESFORGE_PRIVATE_OPJ_DIR` (default
//! `test/private_file/` of the main checkout); it is read with every folder
//! under it (`these/`). The test passes with a message when the folder is
//! absent. Only counts are printed and asserted: no name, no formula text.

use std::path::{Path, PathBuf};

use stokesforge_import::origin::read_project;
use stokesforge_import::FilesLayout;
use stokesforge_importer::{
    judge_origin_formulas, plan_origin_formulas, plan_origin_layout, FormulaVerdict, Judgement,
    OriginSource, OriginWhere,
};

fn dir() -> PathBuf {
    std::env::var_os("STOKESFORGE_PRIVATE_OPJ_DIR").map_or_else(
        || PathBuf::from("/home/aliocha/dev_project/StokesForge/test/private_file/"),
        PathBuf::from,
    )
}

fn origin_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            found.extend(origin_files(&path));
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("opj" | "opju")
        ) {
            found.push(path);
        }
    }
    found
}

#[derive(Default, Debug)]
struct Counts {
    files: usize,
    total: usize,
    set: usize,
    stale: usize,
    refused: usize,
    untranslatable: usize,
    not_imported: usize,
}

#[test]
#[ignore = "needs the private Origin files"]
fn private_files_count_the_formulas_set_stale_untranslatable_and_not_imported() {
    let root = dir();
    // The top level and every folder under it (`these/` and its sub-folders).
    let files = origin_files(&root);
    if files.is_empty() {
        eprintln!(
            "no private Origin file in {}: nothing checked",
            root.display()
        );
        return;
    }
    let mut counts = Counts::default();
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(project) = read_project(&bytes) else {
            continue;
        };
        if project.partial {
            continue;
        }
        let selection: Vec<Vec<Vec<bool>>> = project
            .books
            .iter()
            .map(|book| {
                book.sheets
                    .iter()
                    .map(|sheet| vec![true; sheet.columns.len()])
                    .collect()
            })
            .collect();
        let sources = [OriginSource {
            project: &project,
            selection: &selection,
            file_stem: "",
        }];
        let plan = plan_origin_layout(
            &sources,
            OriginWhere::PerOriginWorkbook,
            FilesLayout::SheetPerFile,
        );
        let formulas = plan_origin_formulas(&sources, &plan);
        let judgements = judge_origin_formulas(&sources, &plan, &formulas);
        counts.files += 1;
        for (key, formula) in &formulas {
            counts.total += 1;
            match &formula.verdict {
                FormulaVerdict::Untranslatable(_) => counts.untranslatable += 1,
                FormulaVerdict::NotImportedInput(_) => counts.not_imported += 1,
                FormulaVerdict::Translated(_) => match judgements.get(key) {
                    Some(Judgement::Equal) => counts.set += 1,
                    Some(Judgement::Stale(_)) => counts.stale += 1,
                    Some(Judgement::Refused) | None => counts.refused += 1,
                },
            }
        }
    }
    println!("{counts:?}");
    assert_eq!(
        counts.set + counts.stale + counts.refused + counts.untranslatable + counts.not_imported,
        counts.total
    );
}

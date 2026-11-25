// SPDX-License-Identifier: GPL-3.0-or-later

//! Local check on real Origin files that are kept private (never committed).
//! Ignored by default; run it with
//! `cargo test -p stokesforge-import --test origin_private -- --ignored`.
//! The folder comes from `STOKESFORGE_PRIVATE_OPJ_DIR` (default
//! `test/private_file/` of the main checkout). The test passes with a message
//! when the folder is absent. Only counts and equalities are asserted here.

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
    let dir = dir();
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

fn dir() -> PathBuf {
    std::env::var_os("STOKESFORGE_PRIVATE_OPJ_DIR").map_or_else(
        || PathBuf::from("/home/aliocha/dev_project/StokesForge/test/private_file/"),
        PathBuf::from,
    )
}

fn same_bits(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            // NaN by bits: every missing value is the canonical NaN here.
            (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits()
        })
}

/// Every `.opju` reads completely; when an `.opj` with the same stem exists,
/// both give the same books, sheets, columns and values. Prints counts only.
#[test]
#[ignore = "needs the private Origin files"]
fn private_opju_projects_read_completely_and_match_their_opj() {
    let dir = dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("no private Origin folder: nothing checked");
        return;
    };
    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("opju") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("readable file");
        let started = std::time::Instant::now();
        let project = read_project(&bytes).expect("the private .opju reads");
        let elapsed = started.elapsed();
        assert!(!project.partial, "the whole window section is understood");
        let (books, sheets, columns) = counts(&project);
        let cells: usize = project
            .books
            .iter()
            .flat_map(|b| &b.sheets)
            .flat_map(|s| &s.columns)
            .map(|c| c.values.len())
            .sum();
        let mut line = format!(
            "opju: {books} books, {sheets} sheets, {columns} columns, {cells} cells, \
             {} graphs skipped, {} matrices, {} functions, read in {elapsed:.2?}",
            project.skipped.graphs, project.skipped.matrices, project.skipped.functions
        );
        let twin = path.with_extension("opj");
        if let Ok(twin_bytes) = std::fs::read(&twin) {
            let old = read_project(&twin_bytes).expect("the twin .opj reads");
            assert_eq!(counts(&old), counts(&project), "books, sheets, columns");
            assert_eq!(old.skipped, project.skipped, "skipped windows");
            assert_eq!(old.version, project.version);
            let mut extra_texts = 0;
            for (a, b) in old.books.iter().zip(&project.books) {
                assert_eq!(a.short_name, b.short_name);
                assert_eq!(a.long_name, b.long_name);
                // The `.opj` stores a backslash of a value doubled, the `.opju`
                // once: compare the lines with the `.opj` ones unescaped.
                let unescaped: Vec<_> = a
                    .info
                    .iter()
                    .map(|l| (l.key.as_str(), l.value.replace("\\\\", "\\")))
                    .collect();
                let read: Vec<_> = b
                    .info
                    .iter()
                    .map(|l| (l.key.as_str(), l.value.clone()))
                    .collect();
                assert!(unescaped == read, "the Horiba lines of a book differ");
                for (sa, sb) in a.sheets.iter().zip(&b.sheets) {
                    assert_eq!(sa.name, sb.name);
                    for (ca, cb) in sa.columns.iter().zip(&sb.columns) {
                        assert_eq!(ca.short_name, cb.short_name);
                        assert_eq!(ca.designation, cb.designation);
                        assert_eq!(ca.long_name, cb.long_name);
                        assert_eq!(ca.unit, cb.unit);
                        assert_eq!(ca.comment, cb.comment);
                        assert!(same_bits(&ca.values, &cb.values), "values bit for bit");
                        for row in 0..cb.values.len() {
                            if cb.text(row).is_some() && ca.text(row).is_none() {
                                extra_texts += 1;
                            } else {
                                assert_eq!(ca.text(row), cb.text(row), "a text both read");
                            }
                        }
                    }
                }
            }
            line.push_str(&format!(
                ", equal to the .opj; {extra_texts} text cells only the .opju reads"
            ));
        }
        eprintln!("{line}");
        checked += 1;
    }
    if checked == 0 {
        eprintln!("no .opju file: nothing checked");
    }
}

/// The `.opj` and `.opju` files of the folder (not its subfolders) and of
/// `these/`, at any depth. The flag is `true` for a file under `these/`, where
/// an unreadable or partial file is tolerated.
fn origin_files(dir: &std::path::Path) -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    let mut pending = vec![
        (dir.to_path_buf(), false, false),
        (dir.join("these"), true, true),
    ];
    while let Some((folder, tolerant, recurse)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && recurse {
                pending.push((path, tolerant, true));
            } else if kind.is_file()
                && matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("opj" | "opju")
                )
            {
                out.push((path, tolerant));
            }
        }
    }
    out.sort();
    out
}

fn formulas_of(project: &OriginProject) -> Vec<Option<&str>> {
    project
        .books
        .iter()
        .flat_map(|b| &b.sheets)
        .flat_map(|s| &s.columns)
        .map(|c| c.formula.as_deref())
        .collect()
}

/// Formulas and graphs (#556, #557) on the private files; prints counts
/// only. A file that reads completely resolves its curves to columns (all
/// but those of a dataset whose book has no window), and
/// an `.opj` and `.opju` twin agree on formulas and graphs.
#[test]
#[ignore = "needs the private Origin files"]
fn private_formulas_and_graphs_resolve_and_twins_agree() {
    let dir = dir();
    let files = origin_files(&dir);
    if files.is_empty() {
        eprintln!("no Origin file in {}: nothing checked", dir.display());
        return;
    }
    let (mut unreadable, mut partial, mut complete) = (0, 0, 0);
    let (mut formulas, mut formula_files) = (0, 0);
    let (mut graphs, mut curves, mut resolved, mut twins) = (0, 0, 0, 0);
    let mut windows_without_data = 0;
    let mut by_path = std::collections::BTreeMap::new();
    for (path, tolerant) in &files {
        let bytes = std::fs::read(path).expect("readable file");
        let project = match read_project(&bytes) {
            Ok(project) => project,
            Err(_) => {
                assert!(tolerant, "a top-level file does not read");
                unreadable += 1;
                continue;
            }
        };
        if project.partial {
            assert!(tolerant, "a top-level file reads partially");
            partial += 1;
            continue;
        }
        complete += 1;
        let n = formulas_of(&project).iter().flatten().count();
        formulas += n;
        formula_files += usize::from(n > 0);
        graphs += project.graphs.len();
        windows_without_data += project.skipped.graphs;
        assert!(project.graphs.len() <= project.skipped.graphs);
        for curve in project
            .graphs
            .iter()
            .flat_map(|g| &g.layers)
            .flat_map(|l| &l.curves)
        {
            curves += 1;
            resolved += usize::from(curve.y.is_some() && curve.x.is_some());
            for at in [curve.y, curve.x].into_iter().flatten() {
                let sheet = &project.books[at.book].sheets[at.sheet];
                assert!(at.column < sheet.columns.len());
            }
        }
        by_path.insert(path.clone(), project);
    }
    // A dataset whose book has no window (a deleted book) is not resolvable.
    assert!(
        resolved * 1000 >= curves * 995,
        "nearly every curve of a complete file resolves: {resolved} of {curves}"
    );
    for (path, opju) in &by_path {
        if path.extension().and_then(|e| e.to_str()) != Some("opju") {
            continue;
        }
        if let Some(opj) = by_path.get(&path.with_extension("opj")) {
            assert_eq!(formulas_of(opj), formulas_of(opju), "formulas of a twin");
            assert_eq!(opj.graphs, opju.graphs, "graphs of a twin");
            twins += 1;
        }
    }
    eprintln!(
        "formulas and graphs: {} files, {complete} complete, {partial} partial, \
         {unreadable} unreadable; {formulas} formulas in {formula_files} files; \
         {graphs} graphs of {windows_without_data} windows without data, {curves} curves, {resolved} resolved; {twins} twin pairs equal",
        files.len()
    );
}

/// Timing (#536): `fs::read` then `read_project` of every private `.opj` and
/// `.opju`. Prints only sizes, counts and durations, never file contents.
/// Run with `--release` as well as in debug.
#[test]
#[ignore = "needs the private Origin files; measurement only"]
fn private_projects_read_timing() {
    let dir = dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("no private Origin folder: nothing measured");
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("opj" | "opju")))
        .collect();
    paths.sort();
    if paths.is_empty() {
        eprintln!("no Origin file: nothing measured");
        return;
    }
    for (index, path) in paths.iter().enumerate() {
        let started = std::time::Instant::now();
        let bytes = std::fs::read(path).expect("readable file");
        let read = started.elapsed();
        let started = std::time::Instant::now();
        let project = read_project(&bytes).expect("the private file reads");
        let parsed = started.elapsed();
        let (books, sheets, columns) = counts(&project);
        eprintln!(
            "timing file #{index} (.{}): {} bytes, {books} books, {sheets} sheets, \
             {columns} columns; fs::read {read:.2?}, read_project {parsed:.2?}",
            path.extension().and_then(|e| e.to_str()).unwrap_or(""),
            bytes.len(),
        );
    }
}

//! Battery 8: property-based testing (proptest) on the path grammar and the
//! column primitives.

use proptest::prelude::*;
use stokesforge_core::{
    format_path, parse_path, Column, ColumnType, ObjectPath, Project, ValueBlock,
};

/// A valid name segment in the sense of `check_name`: non-empty, without
/// edge whitespace, without a reserved character.
fn segment() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9éàü _.()+-]{1,12}"
        .prop_map(|s| s.trim().to_string())
        .prop_filter("non-empty after trimming", |s| !s.is_empty())
}

fn path_strategy() -> impl Strategy<Value = ObjectPath> {
    (
        proptest::collection::vec(segment(), 0..4),
        proptest::option::of(segment()),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(folders, window, with_sheet, with_column, rooted)| {
            // A path that has a column necessarily has a sheet, and a sheet
            // implies a window: the grammar is respected.
            let window_present = window.is_some();
            ObjectPath {
                folders,
                sheet: if window_present && (with_sheet || with_column) {
                    Some("Sheet1".to_string())
                } else {
                    None
                },
                column: if window_present && with_column {
                    Some("ColA".to_string())
                } else {
                    None
                },
                window,
                rooted,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    /// `parse_path(format_path(p)) == p` for every representable path.
    ///
    /// The fully empty path (`ObjectPath::default()`) is excluded: no parse
    /// produces it (`parse_path("")` returns `Err`), so its canonical form
    /// "/" is legitimately reread as rooted.
    #[test]
    fn b8_path_round_trip(p in path_strategy()) {
        prop_assume!(p.window.is_some() || !p.folders.is_empty() || p.rooted);
        let text = format_path(&p);
        let reread = parse_path(&text);
        prop_assert!(
            reread.is_ok(),
            "canonical form {text:?} of path {p:?} refused on reread: {:?}",
            reread.unwrap_err()
        );
        let reread = reread.unwrap();
        prop_assert_eq!(
            &reread, &p,
            "unfaithful round trip: {:?} -> {} -> {:?}", p, text, reread
        );
    }

    /// Parsing the canonical text twice gives the same result.
    #[test]
    fn b8_canonical_form_is_stable(p in path_strategy()) {
        let t1 = format_path(&p);
        if let Ok(q) = parse_path(&t1) {
            let t2 = format_path(&q);
            prop_assert_eq!(&t1, &t2, "the canonical form is not idempotent");
        }
    }

    /// `parse_path` never panics, whatever the input.
    #[test]
    fn b8_parse_never_panics(text in ".{0,60}") {
        let _ = parse_path(&text);
    }

    /// Writing then undoing a write brings the column back identical.
    #[test]
    fn b8_write_then_undo_is_faithful(
        initial_values in proptest::collection::vec(-1e6f64..1e6, 0..40),
        start in 0usize..60,
        written in proptest::collection::vec(-1e6f64..1e6, 0..40),
    ) {
        use stokesforge_commands::{Command, History};
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let col = p.add_column(sheet, None, Some("A"), Column::from_f64(&initial_values)).unwrap();
        let before = std::sync::Arc::clone(p.tree());

        let mut h = History::new();
        h.execute(&mut p, Command::SetCells {
            column: col, start, values: ValueBlock::Float(written.clone()),
            expected_version: None,
        }).unwrap();
        prop_assert!(h.undo(&mut p).unwrap());
        prop_assert!(
            before.as_ref() == p.tree().as_ref(),
            "undoing SetCells(start={start}, {} values) on {} initial values \
             does not give back the previous tree",
            written.len(), initial_values.len()
        );
    }

    /// Inserting then deleting the same rows brings the sheet back identical.
    #[test]
    fn b8_insert_then_delete_rows_is_neutral(
        columns in proptest::collection::vec(
            proptest::collection::vec(-100f64..100.0, 0..15), 1..4),
        at in 0usize..20,
        count in 0usize..10,
    ) {
        use stokesforge_commands::{Command, History};
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        for (i, vals) in columns.iter().enumerate() {
            p.add_column(sheet, None, Some(&format!("C{i}")), Column::from_f64(vals)).unwrap();
        }
        let before = std::sync::Arc::clone(p.tree());
        let mut h = History::new();
        h.execute(&mut p, Command::InsertRows { sheet, at, count }).unwrap();
        // B5 (`guide-de-correction-robustesse.md` §F5): an `InsertRows` that
        // touches no column (or `count == 0`) pushes nothing anymore:
        // `undo` then returns `Ok(false)` (nothing to undo), which is just as
        // neutral as undoing a real effect. Both cases leave the tree equal
        // to `before`; only this equality is the property tested, not the
        // result of `undo`.
        h.undo(&mut p).unwrap();
        prop_assert!(
            before.as_ref() == p.tree().as_ref(),
            "InsertRows(at={at}, count={count}) then undo is not neutral"
        );
    }

    /// A type change then its undo restores type AND values.
    #[test]
    fn b8_type_change_can_be_undone(
        values in proptest::collection::vec(-1e10f64..1e10, 0..30),
        target_index in 0usize..3,
    ) {
        use stokesforge_commands::{Command, History};
        let types = [ColumnType::Numeric, ColumnType::DateTime, ColumnType::Mixed];
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        p.add_column(sheet, None, Some("A"), Column::from_f64(&values)).unwrap();
        let col = p.resolve_path("[Book1]Sheet1!A").unwrap();
        let before = std::sync::Arc::clone(p.tree());
        let mut h = History::new();
        h.execute(&mut p, Command::SetColumnType { column: col, column_type: types[target_index] }).unwrap();
        prop_assert!(h.undo(&mut p).unwrap());
        prop_assert!(
            before.as_ref() == p.tree().as_ref(),
            "changing the type to {:?} then undoing does not restore the column",
            types[target_index]
        );
    }

    /// Faithful disk round trip for any content.
    #[test]
    fn b8_disk_round_trip(
        columns in proptest::collection::vec(
            proptest::collection::vec(
                prop_oneof![
                    Just(f64::NAN), Just(-0.0), Just(0.0), Just(f64::INFINITY),
                    Just(f64::NEG_INFINITY), -1e300f64..1e300,
                ], 0..20), 0..5),
    ) {
        let dir = tempfile::TempDir::new().unwrap();
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        for (i, vals) in columns.iter().enumerate() {
            p.add_column(sheet, None, Some(&format!("C{i}")), Column::from_f64(vals)).unwrap();
        }
        let path = dir.path().join("p.stf");
        stokesforge_persistence::save(&p, &path).unwrap();
        let reread = stokesforge_persistence::load(&path).unwrap();
        prop_assert_eq!(p.tree().len(), reread.tree().len());
        for (id, node) in p.tree().iter() {
            let other = reread.tree().node(id).unwrap();
            if let (Some(c1), Some(c2)) = (node.as_column(), other.as_column()) {
                prop_assert_eq!(c1.len(), c2.len());
                for r in 0..c1.len() {
                    prop_assert_eq!(
                        c1.get(r), c2.get(r),
                        "column {} row {} altered by the disk round trip", node.name(), r
                    );
                }
            }
        }
    }

    /// The automatic column name never collides.
    #[test]
    fn b8_automatic_names_are_free(n in 1usize..200) {
        let mut p = Project::new("P").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let mut names = std::collections::HashSet::new();
        for _ in 0..n {
            let id = p.add_column(sheet, None, None, Column::new(ColumnType::Numeric)).unwrap();
            let name = p.tree().node(id).unwrap().name().to_string();
            prop_assert!(names.insert(name.clone()), "automatic name repeated: {name}");
        }
    }

    /// `path_of` then `resolve_path` is the identity for every node of the tree.
    #[test]
    fn b8_canonical_path_resolves_again(
        folders_list in proptest::collection::vec(segment(), 0..3),
        sheet_count in 1usize..4,
        columns in 0usize..4,
    ) {
        let mut p = Project::new("Root").unwrap();
        let mut parent = p.root();
        let mut seen = std::collections::HashSet::new();
        for d in &folders_list {
            if !seen.insert(d.clone()) { continue; }
            parent = match p.add_folder(parent, d) { Ok(id) => id, Err(_) => continue };
            seen.clear();
        }
        let book = p.add_empty_workbook(parent, Some("Book1")).unwrap();
        for f in 0..sheet_count {
            let sheet = p.add_worksheet(book, Some(&format!("S{f}"))).unwrap();
            for c in 0..columns {
                p.add_column(sheet, None, Some(&format!("C{c}")), Column::new(ColumnType::Numeric)).unwrap();
            }
        }
        for id in p.tree().walk_all() {
            let path = p.path_of(id).unwrap();
            let found = p.resolve_path(&path);
            prop_assert!(
                found.is_ok(),
                "the canonical path {path:?} of node {id} cannot be resolved again: {:?}",
                found.unwrap_err()
            );
            prop_assert_eq!(
                found.unwrap(), id,
                "the canonical path {} names another node", path
            );
        }
    }
}

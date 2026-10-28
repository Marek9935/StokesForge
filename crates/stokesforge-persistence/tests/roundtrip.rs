//! Integration tests of `stokesforge-persistence`, through the public API
//! only (`save`, `load`).

use std::path::Path;

mod common;
use common::{fixture_project, non_trivial_spec, recipe_non_default};

use stokesforge_core::{Column, ColumnMeta, ColumnType, MixedCell, NodeKind, Project, ValueBlock};
use stokesforge_persistence::{load, save, PersistenceError};
use stokesforge_plot::{PanelSpec, PlotSpec};

#[test]
fn full_round_trip_preserves_the_tree_bit_for_bit() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("demo.plyx");

    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");

    assert!(
        *loaded.tree() == *project.tree(),
        "the reloaded tree must be bit-for-bit identical to the original"
    );
    // A disk round trip preserves the counter exactly: the persisted `next_id_hint`
    // is already the next identifier to assign, and nothing in the tree read
    // back contradicts it.
    assert_eq!(loaded.next_id_hint(), project.next_id_hint());
}

#[test]
fn ids_are_preserved_and_the_counter_resumes_above_them() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("demo.plyx");
    save(&project, &path).expect("save succeeded");

    let highest_before = project
        .tree()
        .iter()
        .map(|(id, _)| id.raw())
        .max()
        .unwrap_or(0);

    let mut loaded = load(&path).expect("load succeeded");
    let sheet = loaded
        .resolve_path("/Manips/[BookManips]Sheet1")
        .expect("sheet resolved");
    let new_id = loaded
        .add_column(sheet, None, None, Column::new(ColumnType::Numeric))
        .expect("column added after load");

    // The new id stays strictly above all the ids of the file:
    // no id is ever reused, reloads included.
    assert!(new_id.raw() > highest_before);
    // After a load, adding a column gives the identifier
    // `project.next_id_hint()` itself — not one more.
    assert_eq!(new_id.raw(), project.next_id_hint());
}

#[test]
fn repeated_save_load_cycles_do_not_drift() {
    // No accumulated drift: three chained `save`/`load` cycles leave
    // `next_id_hint()` identical to the original, and the tree equal bit for bit
    // at each round.
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("demo.plyx");

    let mut current = project;
    for round in 0..3 {
        save(&current, &path).expect("save succeeded");
        let loaded = load(&path).expect("load succeeded");
        assert!(
            *loaded.tree() == *current.tree(),
            "tree not preserved bit for bit at round {round}"
        );
        assert_eq!(
            loaded.next_id_hint(),
            current.next_id_hint(),
            "next_id_hint drifted at round {round}"
        );
        current = loaded;
    }
}

#[test]
fn the_modification_counter_survives_the_round_trip() {
    // It must go through the disk: a counter reset to zero at load
    // would end up going back through a value that a reader from before the
    // save would think it recognizes, on data that has become entirely different.
    // `Column`'s `PartialEq` ignores the counter, so the bit-for-bit equality test
    // of the tree does not cover this case — hence this dedicated test.
    let mut project = Project::new("Counter").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("sheet created");
    let column_id = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("column added");

    for value in [10.0, 20.0, 30.0] {
        project
            .set_values(column_id, 0, ValueBlock::Float(vec![value]))
            .expect("valid write");
    }
    let before = project
        .tree()
        .node(column_id)
        .and_then(|node| node.as_column())
        .expect("column present")
        .version();
    assert_eq!(before, 3, "three writes, three increments");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("counter.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");

    let after = loaded
        .tree()
        .node(column_id)
        .and_then(|node| node.as_column())
        .expect("column present")
        .version();
    assert_eq!(after, before);
}

#[test]
fn empty_string_and_missing_text_survive_the_round_trip() {
    let mut project = Project::new("Text").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("sheet created");
    let column = Column::from_block(
        ColumnType::Mixed,
        ValueBlock::Mixed(vec![
            MixedCell::Text(String::new()),
            MixedCell::Empty,
            MixedCell::Text("x".to_string()),
        ]),
        ColumnMeta::default(),
    )
    .expect("valid text column");
    let column_id = project
        .add_column(sheet, None, Some("A"), column)
        .expect("column added");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("text.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");

    let node = loaded.tree().node(column_id).expect("column present");
    let loaded_column = node.as_column().expect("column node");
    assert_eq!(
        loaded_column.get(0),
        stokesforge_core::Cell::Text(Some(String::new()))
    );
    assert_eq!(loaded_column.get(1), stokesforge_core::Cell::Text(None));
    assert_eq!(
        loaded_column.get(2),
        stokesforge_core::Cell::Text(Some("x".to_string()))
    );
}

#[test]
fn two_saves_of_the_same_project_are_byte_identical() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path1 = dir.path().join("a.plyx");
    let path2 = dir.path().join("b.plyx");

    save(&project, &path1).expect("first save");
    save(&project, &path2).expect("second save");

    let bytes1 = std::fs::read(&path1).expect("read the first file");
    let bytes2 = std::fs::read(&path2).expect("read the second file");
    assert_eq!(bytes1, bytes2);
}

#[test]
fn a_file_from_a_newer_version_is_rejected_without_reading_its_body() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("future.plyx");
    save(&project, &path).expect("save succeeded");

    let mut bytes = std::fs::read(&path).expect("read the saved file");
    let mut version = [0u8; 4];
    version.copy_from_slice(&bytes[4..8]);
    let bumped = u32::from_le_bytes(version) + 1;
    bytes[4..8].copy_from_slice(&bumped.to_le_bytes());

    // Body made totally unreadable: the proof that it is not read is that
    // the error stays `UnsupportedVersion`, not `Corrupt`.
    bytes.truncate(8);
    bytes.extend_from_slice(b"this is not valid msgpack");

    std::fs::write(&path, &bytes).expect("write the modified file");

    let err = load(&path).expect_err("future version must be refused");
    match err {
        PersistenceError::UnsupportedVersion {
            found, supported, ..
        } => {
            assert_eq!(found, bumped);
            assert_eq!(supported, bumped - 1);
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn a_file_without_the_stokesforge_signature_is_rejected() {
    let dir = tempfile::tempdir().expect("temporary directory");

    let zip_like = dir.path().join("zip_like.plyx");
    std::fs::write(&zip_like, b"PK\x03\x04rest").expect("write the test file");
    assert!(matches!(
        load(&zip_like),
        Err(PersistenceError::NotAStokesForgeFile { .. })
    ));

    let empty = dir.path().join("empty.plyx");
    std::fs::write(&empty, b"").expect("write the test file");
    assert!(matches!(
        load(&empty),
        Err(PersistenceError::NotAStokesForgeFile { .. })
    ));
}

/// The raw text of the read errors is readable English: it is what
/// the CLI, the scripts and the logs show (the application puts it
/// into a sentence through its catalog, #47).
#[test]
fn reading_errors_display_in_plain_english() {
    let dir = tempfile::tempdir().expect("temporary directory");

    let not_stokesforge = dir.path().join("not_stokesforge.plyx");
    std::fs::write(&not_stokesforge, b"PK\x03\x04rest").expect("write the test file");
    let message = load(&not_stokesforge)
        .expect_err("not a StokesForge file")
        .to_string();
    assert_eq!(
        message,
        format!("{} is not a StokesForge file", not_stokesforge.display())
    );

    let truncated = dir.path().join("truncated.plyx");
    std::fs::write(&truncated, b"STKF\x01").expect("write the test file");
    let message = load(&truncated).expect_err("truncated header").to_string();
    assert_eq!(
        message,
        format!(
            "corrupt file {}: header truncated before the version number",
            truncated.display()
        )
    );

    let missing = dir.path().join("absent.plyx");
    let message = load(&missing).expect_err("missing file").to_string();
    assert!(
        message.starts_with(&format!("I/O error on {}: ", missing.display())),
        "{message}"
    );
}

#[test]
fn a_truncated_or_garbage_body_is_reported_as_corrupt() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("corrupt.plyx");
    save(&project, &path).expect("save succeeded");

    let mut bytes = std::fs::read(&path).expect("read the saved file");
    // Version unchanged (still supported), but the body is truncated in
    // the middle of a MessagePack record.
    bytes.truncate(bytes.len() - 5);
    std::fs::write(&path, &bytes).expect("write the truncated file");

    assert!(matches!(load(&path), Err(PersistenceError::Corrupt { .. })));
}

#[test]
fn saving_into_a_missing_directory_fails_cleanly_and_creates_nothing() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("missing_dir").join("demo.plyx");

    let err = save(&project, &path).expect_err("missing parent directory must fail");
    assert!(matches!(err, PersistenceError::Io { .. }));
    assert!(!path.exists());
    assert!(!Path::new(&dir.path().join("missing_dir")).exists());
}

#[test]
fn saving_leaves_no_temporary_file_behind() {
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("demo.plyx");

    save(&project, &path).expect("save succeeded");

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read the directory")
        .collect::<Result<_, _>>()
        .expect("valid entries");
    assert_eq!(
        entries.len(),
        1,
        "a single file expected in the directory after save: {entries:?}"
    );
}

#[test]
fn saving_over_an_existing_valid_file_replaces_it_atomically() {
    // B8 (`guide-de-correction-robustesse.md` §F2) — two successive
    // saves of the same `project`, to the same path, no longer produce
    // byte-identical files: each one advances
    // `FileBody::save_counter`, which must change so that `save` can
    // detect a concurrent writer. This test used to check the bit-for-bit
    // stability; it now checks what remains true —
    // the atomic write, without residue — and what B8 adds: the counter
    // advances, and two successive saves FROM THIS PROCESS stay
    // accepted (`tracked_counter` follows the last value seen).
    let project = fixture_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("demo.plyx");

    save(&project, &path).expect("first save");
    let first_bytes = std::fs::read(&path).expect("read the first file");

    save(&project, &path).expect("second save over the first file, accepted");
    let second_bytes = std::fs::read(&path).expect("read the second file");

    assert_ne!(
        first_bytes, second_bytes,
        "the save counter must have advanced between the two writes"
    );
    // Only the end of the file (the MessagePack counter) must differ; all
    // the rest — nodes, next_id_hint — is unchanged.
    assert_eq!(first_bytes.len(), second_bytes.len());
    let first_prefix_len = first_bytes.len() - 1;
    assert_eq!(
        &first_bytes[..first_prefix_len],
        &second_bytes[..first_prefix_len],
        "only the save counter was supposed to differ"
    );

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read the directory")
        .collect::<Result<_, _>>()
        .expect("valid entries");
    assert_eq!(entries.len(), 1, "no temporary residue: {entries:?}");
}

#[test]
fn loading_a_missing_path_fails_with_io() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("absent.plyx");
    assert!(matches!(load(&path), Err(PersistenceError::Io { .. })));
}

#[test]
fn a_large_float_column_is_stored_as_raw_bytes_not_as_a_msgpack_array() {
    let mut project = Project::new("Volume").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("sheet created");
    let values: Vec<f64> = (0..100_000).map(|i| i as f64 * 0.5).collect();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&values))
        .expect("column added");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("volume.plyx");
    save(&project, &path).expect("save succeeded");

    let size = std::fs::metadata(&path).expect("file metadata").len();
    assert!(
        size < 100_000 * 8 + 4_096,
        "file of {size} bytes: the encoding must not exceed the raw data by more than 4 KB"
    );
}

/// A mostly numeric mixed column costs, on disk, roughly what the same
/// numeric column would cost — one tag byte per row more, and nothing
/// else.
///
/// This is what the three parallel arrays of `ValuesRecord::Mixed`
/// buy: `to_vec_named` serializes enums by the **name** of their
/// variant, so a `Vec<MixedCellRecord>` would have written the string
/// `"Number"` in front of each of the 100,000 values — close to a megabyte of
/// variant names. The test is the same as for the raw numeric columns,
/// with a margin of one byte per row.
#[test]
fn a_large_mixed_column_costs_one_tag_byte_per_row_not_a_variant_name() {
    let mut project = Project::new("Volume").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .add_worksheet(book, Some("Sheet1"))
        .expect("sheet created");
    const ROWS: usize = 100_000;
    // A column of measurements of which one row in a thousand is an "n/a" — the
    // real case that justifies the type.
    let cells: Vec<MixedCell> = (0..ROWS)
        .map(|i| {
            if i % 1000 == 999 {
                MixedCell::Text("n/a".to_string())
            } else {
                MixedCell::Number(i as f64 * 0.5)
            }
        })
        .collect();
    project
        .add_column(sheet, None, Some("A"), Column::from_mixed(&cells))
        .expect("column added");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("volume.plyx");
    save(&project, &path).expect("save succeeded");

    let size = std::fs::metadata(&path).expect("file metadata").len();
    // 8 bytes per number + 1 tag byte per row + the texts.
    let cap = (ROWS * 8 + ROWS + 4_096) as u64;
    assert!(
        size < cap,
        "file of {size} bytes, above the cap of {cap}: the mixed \
         encoding has stopped being compact"
    );
}

/// Reading refuses a mixed column whose three arrays do not
/// agree, rather than silently shifting all the following rows.
/// Same policy as `BadFloatLength` for a truncated numeric
/// buffer: a file written by this crate never produces this case, and there
/// is no sensible repair to attempt.
#[test]
fn a_mixed_column_with_inconsistent_tags_is_reported_as_corrupt() {
    use serde::Serialize;

    // Mirror schema, like `b3`/`b19`: the format is self-describing, two
    // declarations that agree on the names produce the same bytes.
    #[derive(Serialize)]
    struct FileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
    }
    #[derive(Serialize)]
    struct NodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: KindRecord,
    }
    // `Box` on the big variant: only the bytes produced matter
    // here (MessagePack does not see the indirection), and clippy refuses an enum
    // whose variants have very different sizes.
    #[derive(Serialize)]
    enum KindRecord {
        Folder { children: Vec<u64> },
        Column(Box<ColumnRecord>),
    }
    #[derive(Serialize)]
    struct ColumnRecord {
        column_type: &'static str,
        role: &'static str,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        version: u64,
        formula: Option<String>,
    }
    #[derive(Serialize)]
    enum ValuesRecord {
        Mixed {
            #[serde(with = "serde_bytes")]
            tags: Vec<u8>,
            #[serde(with = "serde_bytes")]
            numbers: Vec<u8>,
            texts: Vec<String>,
        },
    }

    let make_column = |values: ValuesRecord| {
        KindRecord::Column(Box::new(ColumnRecord {
            column_type: "Mixed",
            role: "Y",
            unit: String::new(),
            long_name: String::new(),
            comment: String::new(),
            display_format: String::new(),
            values,
            version: 0,
            formula: None,
        }))
    };

    // Two "number" tags but a single `f64` in the array.
    let disagreement = make_column(ValuesRecord::Mixed {
        tags: vec![1, 1],
        numbers: 1.0f64.to_le_bytes().to_vec(),
        texts: Vec::new(),
    });
    // A tag that no version assigns.
    let unknown_tag = make_column(ValuesRecord::Mixed {
        tags: vec![7],
        numbers: Vec::new(),
        texts: Vec::new(),
    });

    for (case, kind) in [
        ("arrays in disagreement", disagreement),
        ("unknown tag", unknown_tag),
    ] {
        let body = FileBody {
            root: 0,
            next_id_hint: 2,
            nodes: vec![
                NodeRecord {
                    id: 0,
                    name: "Project".to_string(),
                    parent: None,
                    kind: KindRecord::Folder { children: vec![1] },
                },
                NodeRecord {
                    id: 1,
                    name: "A".to_string(),
                    parent: Some(0),
                    kind,
                },
            ],
        };
        let mut bytes = b"STKF".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&rmp_serde::to_vec_named(&body).expect("encoding"));

        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("hostile.plyx");
        std::fs::write(&path, &bytes).expect("write");

        match load(&path) {
            Err(PersistenceError::Corrupt { .. }) => {}
            other => panic!("{case}: expected Corrupt, got {other:?}"),
        }
    }
}

/// Task 3.5: a graph with a non-trivial `PlotSpec`, saved then reloaded,
/// gives an **equal** spec — the disk round trip loses and invents nothing,
/// including on column references whose order goes across two
/// panels.
#[test]
fn a_non_trivial_plot_spec_round_trips_intact() {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet = project.add_worksheet(book, Some("Sheet1")).expect("sheet");
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[0.0, 1.0, 2.0]))
        .expect("column X");
    let y1 = project
        .add_column(sheet, None, Some("Y1"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column Y1");
    let y2 = project
        .add_column(sheet, None, Some("Y2"), Column::from_f64(&[4.0, 5.0, 6.0]))
        .expect("column Y2");

    let spec = non_trivial_spec(x, y1, y2);
    let graph = project
        .add_graph(root, Some("Fig1"), spec.clone())
        .expect("graph created");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("spec.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");

    let node = loaded.tree().node(graph).expect("graph present");
    match node.kind() {
        NodeKind::Graph { spec: reloaded, .. } => assert_eq!(*reloaded, spec),
        other => panic!("expected Graph, got {other:?}"),
    }
}

/// #168: the roles X3…Y4 and the layer number of an overlaid layer
/// come back identical after `save`/`load`.
#[test]
fn roles_x3_to_y4_and_the_layer_number_survive_save_and_load() {
    use stokesforge_core::ColumnRole;
    use stokesforge_plot::{OverlaySpec, Panels};
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let roles = [
        ColumnRole::X3,
        ColumnRole::Y3,
        ColumnRole::X4,
        ColumnRole::Y4,
    ];
    let columns: Vec<_> = roles
        .iter()
        .map(|role| {
            let id = project
                .add_column(sheet, None, None, Column::from_f64(&[1.0, 2.0]))
                .expect("column");
            let mut meta = project
                .tree()
                .node(id)
                .unwrap()
                .as_column()
                .unwrap()
                .meta()
                .clone();
            meta.role = *role;
            project.set_column_meta(id, meta).expect("role");
            id
        })
        .collect();
    let base = PanelSpec::at(0, 0);
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![
        base.clone(),
        PanelSpec::overlay_on(&base, OverlaySpec::role_layer(3)),
        PanelSpec::overlay_on(&base, OverlaySpec::role_layer(4)),
    ]));
    let graph = project.add_graph(root, None, spec.clone()).expect("graph");
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("roles.stf");
    save(&project, &path).expect("save");
    let loaded = load(&path).expect("load");
    for (id, role) in columns.iter().zip(roles) {
        let column = loaded.tree().node(*id).unwrap().as_column().unwrap();
        assert_eq!(column.meta().role, role);
    }
    match loaded.tree().node(graph).unwrap().kind() {
        NodeKind::Graph { spec: reloaded, .. } => assert_eq!(*reloaded, spec),
        other => panic!("expected Graph, got {other:?}"),
    }
}

/// `ColumnMeta::formula` (calculated columns) survives `save`/`load`, `None`
/// as well as `Some(_)`: same fragments, same identifiers (#321), and the
/// displayed letters are those of before.
#[test]
fn column_formula_survives_save_and_load() {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet = project.add_worksheet(book, Some("Sheet1")).expect("sheet");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("column A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
        .expect("column B");
    let formula = project
        .tree()
        .formula_from_letters(b, "A*2")
        .expect("formula entered");
    assert_eq!(
        formula.parts(),
        [
            stokesforge_core::FormulaPart::Column(a),
            stokesforge_core::FormulaPart::Text("*2".to_string())
        ]
    );
    project
        .set_column_meta(
            b,
            ColumnMeta {
                formula: Some(formula.clone()),
                ..Default::default()
            },
        )
        .expect("formula set");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("formula.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");

    let node_a = loaded.tree().node(a).expect("column A present");
    assert_eq!(node_a.as_column().expect("column").meta().formula, None);

    let node_b = loaded.tree().node(b).expect("column B present");
    assert_eq!(
        node_b.as_column().expect("column").meta().formula,
        Some(formula)
    );
    assert_eq!(loaded.tree().formula_letters(b).as_deref(), Some("A*2"));
}

/// A formula that reads a **deleted** column (`#REF`) comes back as
/// it was: the identifier stays stored, the file does not lose it, and the
/// next identifier does not reassign it (invariant 7).
#[test]
fn a_formula_with_a_deleted_column_survives_save_and_load() {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet = project.add_worksheet(book, Some("Sheet1")).expect("sheet");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("column A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
        .expect("column B");
    let c = project
        .add_column(sheet, None, Some("C"), Column::from_f64(&[0.0, 0.0]))
        .expect("column C");
    let formula = project
        .tree()
        .formula_from_letters(c, "B*2")
        .expect("formula entered");
    project
        .set_column_meta(
            c,
            ColumnMeta {
                formula: Some(formula.clone()),
                ..Default::default()
            },
        )
        .expect("formula set");
    project.remove(b).expect("removal of B");
    assert_eq!(project.tree().formula_letters(c).as_deref(), Some("#REF*2"));

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("ref.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");

    let reloaded = loaded
        .tree()
        .node(c)
        .and_then(|node| node.as_column())
        .and_then(|column| column.meta().formula.clone())
        .expect("formula read back");
    assert_eq!(reloaded, formula, "same fragments, same identifiers");
    assert_eq!(loaded.tree().formula_letters(c).as_deref(), Some("#REF*2"));
    assert!(loaded.tree().node(a).is_some());
    assert!(
        loaded.next_id_hint() > b.raw(),
        "the identifier of the deleted column is not reassigned"
    );
}

/// A file from before #321 carries the formula as letters under the key `formula`:
/// it is no longer read (the format does not have to stay compatible during
/// development) — the column loads **with its values and without a formula**,
/// and stays an ordinary column (never filed as `Unknown`).
#[test]
fn a_legacy_letters_formula_is_dropped_and_the_column_keeps_its_values() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct FileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
    }
    #[derive(Serialize)]
    struct NodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: KindRecord,
    }
    #[derive(Serialize)]
    enum KindRecord {
        Folder {
            children: Vec<u64>,
        },
        Workbook {
            sheets: Vec<u64>,
            hidden: bool,
            label: String,
            comment: String,
        },
        Worksheet {
            columns: Vec<u64>,
        },
        Column(Box<ColumnRecord>),
    }
    #[derive(Serialize)]
    struct ColumnRecord {
        column_type: &'static str,
        role: &'static str,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        version: u64,
        formula: Option<String>,
    }
    #[derive(Serialize)]
    enum ValuesRecord {
        Float(#[serde(with = "serde_bytes")] Vec<u8>),
    }

    let value_bytes: Vec<u8> = [1.0f64, 2.0, 3.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let node = |id: u64, name: &str, parent: u64, kind: KindRecord| NodeRecord {
        id,
        name: name.to_string(),
        parent: Some(parent),
        kind,
    };
    let body = FileBody {
        root: 1,
        next_id_hint: 6,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "Projet".to_string(),
                parent: None,
                kind: KindRecord::Folder { children: vec![2] },
            },
            node(
                2,
                "Book1",
                1,
                KindRecord::Workbook {
                    sheets: vec![3],
                    hidden: false,
                    label: String::new(),
                    comment: String::new(),
                },
            ),
            node(
                3,
                "Sheet1",
                2,
                KindRecord::Worksheet {
                    columns: vec![4, 5],
                },
            ),
            node(
                4,
                "A",
                3,
                KindRecord::Column(Box::new(ColumnRecord {
                    column_type: "Numeric",
                    role: "Y",
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(value_bytes.clone()),
                    version: 1,
                    formula: None,
                })),
            ),
            node(
                5,
                "B",
                3,
                KindRecord::Column(Box::new(ColumnRecord {
                    column_type: "Numeric",
                    role: "Y",
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(value_bytes),
                    version: 2,
                    formula: Some("A*2".to_string()),
                })),
            ),
        ],
    };
    let mut bytes = b"STKF".to_vec();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&rmp_serde::to_vec_named(&body).expect("encoding"));
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("old.plyx");
    std::fs::write(&path, &bytes).expect("write");

    let loaded = load(&path).expect("the old file loads");
    let column = loaded
        .tree()
        .node(stokesforge_core::NodeId::from_raw(5))
        .and_then(|node| node.as_column())
        .expect("ordinary column B, not an unknown node");
    assert_eq!(column.meta().formula, None, "the old formula is lost");
    assert_eq!(column.len(), 3, "the values are kept");
    assert_eq!(column.values().number_at(2), 3.0);
}

/// Writes in `dir` a crafted file: `Sheet1` carries `A` (id 4) and `B`
/// (id 5), whose formula is `<column read> * 2`, the column read being
/// designated by its raw identifier `read`.
fn crafted_file_with_formula(dir: &std::path::Path, read: u64) -> std::path::PathBuf {
    use serde::Serialize;

    #[derive(Serialize)]
    struct FileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
    }
    #[derive(Serialize)]
    struct NodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: KindRecord,
    }
    #[derive(Serialize)]
    enum KindRecord {
        Folder {
            children: Vec<u64>,
        },
        Workbook {
            sheets: Vec<u64>,
            hidden: bool,
            label: String,
            comment: String,
        },
        Worksheet {
            columns: Vec<u64>,
        },
        Column(Box<ColumnRecord>),
    }
    #[derive(Serialize)]
    struct ColumnRecord {
        column_type: &'static str,
        role: &'static str,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        version: u64,
        formula_parts: Option<Vec<PartRecord>>,
    }
    #[derive(Serialize)]
    enum PartRecord {
        Text(String),
        Column(u64),
    }
    #[derive(Serialize)]
    enum ValuesRecord {
        Float(#[serde(with = "serde_bytes")] Vec<u8>),
    }

    let value_bytes: Vec<u8> = [1.0f64, 2.0, 3.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let node = |id: u64, name: &str, parent: u64, kind: KindRecord| NodeRecord {
        id,
        name: name.to_string(),
        parent: Some(parent),
        kind,
    };
    let body = FileBody {
        root: 1,
        next_id_hint: 6,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "Project".to_string(),
                parent: None,
                kind: KindRecord::Folder { children: vec![2] },
            },
            node(
                2,
                "Book1",
                1,
                KindRecord::Workbook {
                    sheets: vec![3],
                    hidden: false,
                    label: String::new(),
                    comment: String::new(),
                },
            ),
            node(
                3,
                "Sheet1",
                2,
                KindRecord::Worksheet {
                    columns: vec![4, 5],
                },
            ),
            node(
                4,
                "A",
                3,
                KindRecord::Column(Box::new(ColumnRecord {
                    column_type: "Numeric",
                    role: "Y",
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(value_bytes.clone()),
                    version: 1,
                    formula_parts: None,
                })),
            ),
            node(
                5,
                "B",
                3,
                KindRecord::Column(Box::new(ColumnRecord {
                    column_type: "Numeric",
                    role: "Y",
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(value_bytes),
                    version: 2,
                    formula_parts: Some(vec![
                        PartRecord::Column(read),
                        PartRecord::Text("*2".to_string()),
                    ]),
                })),
            ),
        ],
    };
    let mut bytes = b"STKF".to_vec();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&rmp_serde::to_vec_named(&body).expect("encoding"));
    let path = dir.join("crafted.stf");
    std::fs::write(&path, &bytes).expect("write");

    path
}

/// Invariant 10: an identifier read by a formula that designates no
/// column of the sheet (crafted file, vanished column) loads without
/// panic — the formula displays `#REF`, it computes as a deleted-column
/// error.
#[test]
fn a_formula_reading_an_id_outside_the_sheet_loads_as_ref() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = crafted_file_with_formula(dir.path(), 99);
    let loaded = load(&path).expect("the file loads");
    assert_eq!(
        loaded
            .tree()
            .formula_letters(stokesforge_core::NodeId::from_raw(5))
            .as_deref(),
        Some("#REF*2")
    );
}

/// Invariant 7: the identifier of a column read is bounded at read like
/// all the others (`MAX_NODE_ID`) — otherwise a crafted file would make
/// the allocator start at the edge of `u64`.
#[test]
fn a_formula_reading_an_id_beyond_the_limit_is_refused() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = crafted_file_with_formula(dir.path(), 2_000_000_000_000);
    assert!(load(&path).is_err(), "out-of-bounds identifier refused");
}

/// The colors saved in the project, filed by usage, survive
/// `save`/`load` — and the tree read back stays identical to the original.
#[test]
fn saved_colors_survive_save_and_load() {
    use stokesforge_plot::{ColorUsage, SavedColors};
    let mut project = fixture_project();
    project.set_saved_colors(
        SavedColors::default()
            .with_added(ColorUsage::Curves, "#ff0000")
            .with_added(ColorUsage::Text, "#20202080")
            .with_added(ColorUsage::Background, "#ffffff00")
            // Saved color gradient (#178).
            .with_gradient_added("Warm", &["#000000".to_string(), "#ff0000".to_string()]),
    );
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("colors.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");
    assert_eq!(loaded.tree().saved_colors(), project.tree().saved_colors());
    assert!(*loaded.tree() == *project.tree());
}

/// `ColumnMeta::file_metadata` (metadata of an imported file, issue #5)
/// survives `save`/`load`, in order, sections and lines without a key included.
#[test]
fn column_file_metadata_survives_save_and_load() {
    use stokesforge_core::MetaLine;
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet = project.add_worksheet(book, Some("Sheet1")).expect("sheet");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column A");
    let lines = vec![
        MetaLine {
            key: "[Measurement Information]".into(),
            value: String::new(),
        },
        MetaLine {
            key: "XUNITS".into(),
            value: "NANOMETERS".into(),
        },
        MetaLine {
            key: String::new(),
            value: "##### Extended Information".into(),
        },
    ];
    project
        .set_column_meta(
            a,
            ColumnMeta {
                file_metadata: lines.clone(),
                ..Default::default()
            },
        )
        .expect("metadata set");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("meta.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");
    let column = loaded.tree().node(a).unwrap().as_column().unwrap();
    assert_eq!(column.meta().file_metadata, lines);
}

/// `ColumnMeta::import_source` (path, recipe, column and block index)
/// survives `save`/`load`: this is what makes it possible to refresh a column after
/// reopening the project (issue #7). Each field of the recipe is
/// non-default.
#[test]
fn column_import_source_survives_save_and_load() {
    use stokesforge_core::ImportSource;
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Book1"))
        .expect("workbook");
    let sheet = project.add_worksheet(book, Some("Sheet1")).expect("sheet");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[1.0]))
        .expect("column B");
    let source = ImportSource {
        path: "/data/measure.csv".into(),
        recipe_json: recipe_non_default().to_json(),
        column: 3,
        block: 2,
    };
    project
        .set_column_meta(
            a,
            ColumnMeta {
                import_source: Some(source.clone()),
                ..Default::default()
            },
        )
        .expect("source set");
    // An unreadable recipe (column crafted by hand) is written without a source.
    project
        .set_column_meta(
            b,
            ColumnMeta {
                import_source: Some(ImportSource {
                    recipe_json: "{\"name\":\"x\"}".into(),
                    ..source.clone()
                }),
                ..Default::default()
            },
        )
        .expect("unreadable source set");
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("source.plyx");
    save(&project, &path).expect("save succeeded");
    let loaded = load(&path).expect("load succeeded");
    let column = loaded.tree().node(a).unwrap().as_column().unwrap();
    assert_eq!(column.meta().import_source, Some(source));
    let column = loaded.tree().node(b).unwrap().as_column().unwrap();
    assert_eq!(column.meta().import_source, None);
}

/// A file that carries the `STKF` signature but cannot be read (interrupted
/// copy, version 0, undecodable body) is never overwritten by `save`:
/// replacing it would make it unrecoverable. The file stays intact byte for
/// byte and the error says so.
#[test]
fn save_refuses_to_overwrite_an_unreadable_stokesforge_file() {
    let project = Project::new("New").expect("project");
    let dir = tempfile::tempdir().expect("temporary directory");
    let cases: [(&str, Vec<u8>); 3] = [
        ("truncated header", b"STKF\x01".to_vec()),
        ("version 0", b"STKF\x00\x00\x00\x00body".to_vec()),
        (
            "undecodable body",
            b"STKF\x01\x00\x00\x00\xc1\xc1\xc1".to_vec(),
        ),
    ];
    for (name, bytes) in cases {
        let path = dir.path().join("target.stf");
        std::fs::write(&path, &bytes).expect("write the test file");
        let err = save(&project, &path).expect_err(name);
        assert!(
            matches!(err, PersistenceError::UnreadableTarget { .. }),
            "{name}: {err:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "{name}: intact");
    }
}

/// A target that does not carry the signature (other format, empty file) stays
/// replaceable, as before: only an unreadable `.stf` is protected.
#[test]
fn save_still_overwrites_a_foreign_or_empty_target() {
    let project = Project::new("New").expect("project");
    let dir = tempfile::tempdir().expect("temporary directory");
    for bytes in [&b"PK\x03\x04rest"[..], &b""[..]] {
        let path = dir.path().join("target.stf");
        std::fs::write(&path, bytes).expect("write the test file");
        save(&project, &path).expect("replacement of a foreign target");
        load(&path).expect("the written file reads back");
    }
}

#[test]
fn unreadable_target_error_displays_in_plain_english() {
    let error = PersistenceError::UnreadableTarget {
        path: "/data/run.stf".into(),
        detail: "header truncated".into(),
    };
    assert_eq!(
        error.to_string(),
        "/data/run.stf exists but cannot be read as a StokesForge file \
         (header truncated): refusing to overwrite it"
    );
}

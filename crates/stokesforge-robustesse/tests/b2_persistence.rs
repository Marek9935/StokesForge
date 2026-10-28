//! Battery 2: deserialization of malformed structures and disk round trip.

use std::io::Write;
use std::path::PathBuf;
use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, Project, ValueBlock,
};
use stokesforge_persistence::{load, save};
use tempfile::TempDir;

const MAGIC: &[u8; 4] = b"STKF";

fn write_file(dir: &TempDir, name: &str, body: &[u8], version: u32) -> PathBuf {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(MAGIC).unwrap();
    f.write_all(&version.to_le_bytes()).unwrap();
    f.write_all(body).unwrap();
    f.sync_all().unwrap();
    path
}

#[test]
fn b2_random_bodies_never_panic() {
    let dir = TempDir::new().unwrap();
    // Deterministic seeds: home-made xorshift, no dependency.
    let mut state: u64 = 0x2026_0911_2026_0911;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for case in 0..2000u32 {
        let size = (next() % 512) as usize;
        let body: Vec<u8> = (0..size).map(|_| (next() & 0xff) as u8).collect();
        let path = write_file(&dir, &format!("f{case}.stf"), &body, 1);
        let r = std::panic::catch_unwind(|| load(&path));
        assert!(
            r.is_ok(),
            "panic on a random body of {size} bytes: {body:02x?}"
        );
        if let Ok(Ok(p)) = r {
            // A random body that gets through: we want to know.
            println!(
                "case {case} ACCEPTED ({} nodes) body={body:02x?}",
                p.tree().len()
            );
        }
    }
}

#[test]
fn b2_progressive_truncation_of_a_valid_file() {
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Truncated").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let full = dir.path().join("full.stf");
    save(&project, &full).unwrap();
    let bytes = std::fs::read(&full).unwrap();

    for n in 0..bytes.len() {
        let path = dir.path().join(format!("t{n}.stf"));
        std::fs::write(&path, &bytes[..n]).unwrap();
        let r = std::panic::catch_unwind(|| load(&path));
        assert!(r.is_ok(), "panic on a truncation at {n} bytes");
        if let Ok(Ok(_)) = r {
            panic!("a truncation at {n}/{} bytes was ACCEPTED", bytes.len());
        }
    }
}

#[test]
fn b2_single_byte_mutation_in_a_valid_file() {
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Bitflip").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_graph(root, Some("G1"), stokesforge_plot::PlotSpec::empty("t"))
        .unwrap();
    let full = dir.path().join("full.stf");
    save(&project, &full).unwrap();
    let bytes = std::fs::read(&full).unwrap();

    let mut accepted = 0usize;
    for i in 8..bytes.len() {
        for bit in [0x01u8, 0x80u8] {
            let mut copy = bytes.clone();
            copy[i] ^= bit;
            let path = dir.path().join("mut.stf");
            std::fs::write(&path, &copy).unwrap();
            let r = std::panic::catch_unwind(|| load(&path));
            assert!(
                r.is_ok(),
                "panic on a bit flipped at position {i} (mask {bit:#04x})"
            );
            if let Ok(Ok(_)) = r {
                accepted += 1;
            }
        }
    }
    println!(
        "{accepted} one-bit variants accepted out of {} tries (expected: those that \
         only touch useful data)",
        (bytes.len() - 8) * 2
    );
}

#[test]
fn b2_round_trip_preserves_everything() {
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("AR").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();

    // Numeric column with pathological values.
    let pathological = [
        0.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE,
        1e-320,
        f64::MAX,
        f64::MIN,
    ];
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&pathological))
        .unwrap();
    // A NaN with a non-standard payload: the format promises "bit for bit".
    let signaling_nan = f64::from_bits(0x7ff4_0000_dead_beef);
    project
        .set_values(a, 0, ValueBlock::Float(vec![signaling_nan]))
        .unwrap();

    // Text column (mixed, entirely textual) with empty / None / unicode /
    // null character.
    let mut texts = Column::new(ColumnType::Mixed);
    texts
        .set_values(
            0,
            ValueBlock::Mixed(vec![
                MixedCell::Text(String::new()),
                MixedCell::Empty,
                MixedCell::Text("é\u{0}à".into()),
                MixedCell::Text("🦀".into()),
                MixedCell::Text("  spaces  ".into()),
            ]),
        )
        .unwrap();
    let b = project.add_column(sheet, None, Some("B"), texts).unwrap();
    project
        .set_column_meta(
            b,
            ColumnMeta {
                role: ColumnRole::XError,
                unit: "µm".into(),
                long_name: "Long name".into(),
                comment: "".into(),
                x_prime: Some(-0.5),
                display_format: "%.3f".into(),
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
            },
        )
        .unwrap();

    // Column with mixed types: number, text, empty cell, NaN written as a
    // deliberate number (distinct from the empty cell), unicode text.
    let mixed = Column::from_mixed(&[
        MixedCell::Number(1.5),
        MixedCell::Text("n/a".to_string()),
        MixedCell::Empty,
        MixedCell::Number(f64::NAN),
        MixedCell::Text(String::new()),
        MixedCell::Text("é ü 日本".to_string()),
    ]);
    project.add_column(sheet, None, Some("C"), mixed).unwrap();

    let path = dir.path().join("ar.stf");
    save(&project, &path).unwrap();
    let reloaded = load(&path).unwrap();

    assert_eq!(project.tree().len(), reloaded.tree().len());
    assert_eq!(project.next_id_hint(), reloaded.next_id_hint());

    for (id, node) in project.tree().iter() {
        let other = reloaded.tree().node(id).expect("node lost on reload");
        assert_eq!(node.name(), other.name());
        assert_eq!(node.parent(), other.parent());
        if let (Some(c1), Some(c2)) = (node.as_column(), other.as_column()) {
            assert_eq!(c1.column_type(), c2.column_type());
            assert_eq!(c1.meta(), c2.meta());
            assert_eq!(
                c1.version(),
                c2.version(),
                "version counter lost for {}",
                node.name()
            );
            assert_eq!(c1.len(), c2.len());
            for row in 0..c1.len() {
                let (x, y) = (c1.get(row), c2.get(row));
                assert_eq!(x, y, "column {} row {row}: {x:?} != {y:?}", node.name());
            }
        }
    }
    // Byte-for-byte determinism promised by the `save` doc.
    let path2 = dir.path().join("ar2.stf");
    save(&reloaded, &path2).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&path2).unwrap(),
        "two saves of the same project differ"
    );
}

#[test]
fn b2_very_wide_project_reloads() {
    // 2000 columns in a sheet: validation on load is O(n²) over the children
    // of a single parent (occurrence counting).
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Wide").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    for i in 0..2000 {
        project
            .add_column(
                sheet,
                None,
                Some(&format!("C{i}")),
                Column::from_f64(&[1.0]),
            )
            .unwrap();
    }
    let path = dir.path().join("large.stf");
    save(&project, &path).unwrap();
    let start = std::time::Instant::now();
    let reloaded = load(&path).unwrap();
    let duration = start.elapsed();
    println!("reload of 2000 columns: {duration:?}");
    assert_eq!(reloaded.tree().len(), project.tree().len());
    assert!(
        duration < std::time::Duration::from_secs(10),
        "abnormally long reload: {duration:?}"
    );
}

#[test]
fn b2_extreme_folder_depth() {
    // Very deep tree: loading validates by a walk; if it were recursive, the
    // stack would blow.
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Deep").unwrap();
    let mut parent = project.root();
    for i in 0..5000 {
        parent = project.add_folder(parent, &format!("d{i}")).unwrap();
    }
    let path = dir.path().join("deep.stf");
    save(&project, &path).unwrap();
    let reloaded = load(&path).unwrap();
    assert_eq!(reloaded.tree().len(), 5001);
    // And the canonical path of the deepest one.
    let canonical_path = reloaded.path_of(parent).unwrap();
    assert!(canonical_path.ends_with("/d4999"));
}

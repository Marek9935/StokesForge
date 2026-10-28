//! Battery 3: `.stf` files that are syntactically valid (well-formed
//! MessagePack, schema respected) but semantically hostile.
//!
//! The schema is private to `stokesforge-persistence`; it is redeclared here
//! identically. That is legitimate: the format is self-describing (named
//! maps), so two declarations that agree on the names produce the same bytes.

use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use stokesforge_persistence::load;
use tempfile::TempDir;

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
        label: String,
        comment: String,
    },
    Worksheet {
        columns: Vec<u64>,
    },
    Column {
        column_type: TypeRecord,
        role: RoleRecord,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        version: u64,
    },
    Graph {
        label: String,
        comment: String,
        spec: PlotSpecRecord,
    },
    #[allow(dead_code)]
    Matrix {
        label: String,
        comment: String,
    },
    #[allow(dead_code)]
    Note {
        label: String,
        comment: String,
        text: String,
    },
}
#[derive(Serialize)]
enum TypeRecord {
    Numeric,
    #[allow(dead_code)]
    DateTime,
}
#[derive(Serialize)]
enum RoleRecord {
    #[allow(dead_code)]
    X,
    Y,
}
#[derive(Serialize)]
enum ValuesRecord {
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
    Mixed {
        #[serde(with = "serde_bytes")]
        tags: Vec<u8>,
        #[serde(with = "serde_bytes")]
        numbers: Vec<u8>,
        texts: Vec<String>,
    },
}
#[derive(Serialize, Default)]
struct PlotSpecRecord {
    title: String,
    panels: Vec<PanelSpecRecord>,
    background: String,
}
#[derive(Serialize, Default)]
struct PanelSpecRecord {
    title: String,
    series: Vec<SeriesSpecRecord>,
    bottom: AxisSpecRecord,
    left: AxisSpecRecord,
    right: AxisSpecRecord,
    top: AxisSpecRecord,
    legend: LegendSpecRecord,
    row: u64,
    column: u64,
}
// `Default` is derived: these records are used to build hostile files, not
// to replicate the defaults of `stokesforge-plot` (the hidden top axis, in
// particular, need not be hidden here).
#[derive(Serialize)]
struct AxisSpecRecord {
    label: String,
    unit: String,
    scale: AxisScaleRecord,
    auto_range: bool,
    range_min: Option<f64>,
    range_max: Option<f64>,
    grid: bool,
    visible: bool,
    inverted: bool,
}
impl Default for AxisSpecRecord {
    fn default() -> Self {
        Self {
            label: String::new(),
            unit: String::new(),
            scale: AxisScaleRecord::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
        }
    }
}
#[derive(Serialize)]
enum AxisScaleRecord {
    Linear,
    #[allow(dead_code)]
    Log,
}
#[derive(Serialize)]
struct SeriesSpecRecord {
    y: u64,
    x: Option<u64>,
    x_error: Option<u64>,
    y_error: Option<u64>,
    plot_type: PlotTypeRecord,
    name: String,
    line: LineSpecRecord,
    symbol: SymbolSpecRecord,
    axis: SeriesAxisRecord,
    visible: bool,
    show_in_legend: bool,
    bar_width: Option<f64>,
    histogram_bins: u32,
}
impl Default for SeriesSpecRecord {
    fn default() -> Self {
        Self {
            y: 1,
            x: None,
            x_error: None,
            y_error: None,
            plot_type: PlotTypeRecord::Line,
            name: String::new(),
            line: LineSpecRecord::default(),
            symbol: SymbolSpecRecord::default(),
            axis: SeriesAxisRecord::Left,
            visible: true,
            show_in_legend: true,
            bar_width: None,
            histogram_bins: 10,
        }
    }
}
#[derive(Serialize)]
enum PlotTypeRecord {
    #[allow(dead_code)]
    Scatter,
    Line,
    #[allow(dead_code)]
    Bar,
}
#[derive(Serialize)]
enum SeriesAxisRecord {
    Left,
    #[allow(dead_code)]
    Right,
}
#[derive(Serialize)]
struct LineSpecRecord {
    color: String,
    style: LineStyleRecord,
    width: f64,
}
impl Default for LineSpecRecord {
    fn default() -> Self {
        Self {
            color: String::new(),
            style: LineStyleRecord::Solid,
            width: 1.0,
        }
    }
}
#[derive(Serialize)]
enum LineStyleRecord {
    #[allow(dead_code)]
    None,
    Solid,
}
#[derive(Serialize)]
struct SymbolSpecRecord {
    shape: SymbolShapeRecord,
    size: f64,
    color: String,
    edge_color: String,
}
impl Default for SymbolSpecRecord {
    fn default() -> Self {
        Self {
            shape: SymbolShapeRecord::Circle,
            size: 5.0,
            color: String::new(),
            edge_color: String::new(),
        }
    }
}
#[derive(Serialize)]
enum SymbolShapeRecord {
    #[allow(dead_code)]
    None,
    Circle,
}
#[derive(Serialize)]
struct LegendSpecRecord {
    visible: bool,
    position: LegendPositionRecord,
}
impl Default for LegendSpecRecord {
    fn default() -> Self {
        Self {
            visible: true,
            position: LegendPositionRecord::TopRight,
        }
    }
}
#[derive(Serialize)]
enum LegendPositionRecord {
    #[allow(dead_code)]
    TopLeft,
    TopRight,
}

fn write_file(dir: &TempDir, name: &str, body: &FileBody) -> PathBuf {
    let payload = rmp_serde::to_vec_named(body).unwrap();
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"STKF").unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap();
    f.write_all(&payload).unwrap();
    f.sync_all().unwrap();
    path
}

fn folder(id: u64, name: &str, parent: Option<u64>, children: Vec<u64>) -> NodeRecord {
    NodeRecord {
        id,
        name: name.into(),
        parent,
        kind: KindRecord::Folder { children },
    }
}
fn column(id: u64, name: &str, parent: u64, values: Vec<f64>) -> NodeRecord {
    let mut bytes = Vec::new();
    for v in &values {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    NodeRecord {
        id,
        name: name.into(),
        parent: Some(parent),
        kind: KindRecord::Column {
            column_type: TypeRecord::Numeric,
            role: RoleRecord::Y,
            unit: String::new(),
            long_name: String::new(),
            comment: String::new(),
            display_format: String::new(),
            values: ValuesRecord::Float(bytes),
            version: 0,
        },
    }
}

#[test]
fn b3_parent_cycle_is_refused() {
    let dir = TempDir::new().unwrap();
    // root 1 -> 2, and 2 -> 3 -> 2 (cycle)
    let body = FileBody {
        root: 1,
        next_id_hint: 4,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            folder(2, "A", Some(3), vec![3]),
            folder(3, "B", Some(2), vec![2]),
        ],
    };
    let path = write_file(&dir, "cycle.stf", &body);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(r.is_ok(), "panic (or loop) on a parent cycle");
    let r = r.unwrap();
    assert!(r.is_err(), "a parent cycle was ACCEPTED");
    println!("cycle -> {}", r.unwrap_err());
}

#[test]
fn b3_self_parent_is_refused() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            folder(2, "A", Some(2), vec![2]),
        ],
    };
    let path = write_file(&dir, "self.stf", &body);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(
        r.is_ok(),
        "panic (or loop) on a node that is its own parent"
    );
    assert!(r.unwrap().is_err(), "self-parenting ACCEPTED");
}

#[test]
fn b3_id_zero_and_max() {
    let dir = TempDir::new().unwrap();
    // NodeId 0 is never assigned by the allocator; u64::MAX forces
    // `resume(hint, highest)` to saturate.
    let body = FileBody {
        root: 0,
        next_id_hint: 1,
        nodes: vec![
            folder(0, "R", None, vec![u64::MAX]),
            folder(u64::MAX, "A", Some(0), vec![]),
        ],
    };
    let path = write_file(&dir, "ids.stf", &body);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(r.is_ok(), "panic on extreme identifiers");
    match r.unwrap() {
        Ok(p) => {
            println!("ACCEPTED: next_id_hint = {}", p.next_id_hint());
            // The next identifier to be assigned must be free.
            assert!(
                !p.tree()
                    .contains(stokesforge_core::NodeId::from_raw(p.next_id_hint())),
                "the next identifier to be assigned ({}) is ALREADY taken: \
                 the next creation will overwrite an existing node",
                p.next_id_hint()
            );
        }
        Err(e) => println!("refused: {e}"),
    }
}

#[test]
fn b3_panel_at_extreme_position() {
    let dir = TempDir::new().unwrap();
    let panel = PanelSpecRecord {
        row: u64::MAX,
        column: u64::MAX,
        ..PanelSpecRecord::default()
    };
    let spec = PlotSpecRecord {
        title: "T".into(),
        panels: vec![panel],
        background: String::new(),
    };
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "G1".into(),
                parent: Some(1),
                kind: KindRecord::Graph {
                    label: String::new(),
                    comment: String::new(),
                    spec,
                },
            },
        ],
    };
    let path = write_file(&dir, "grid.stf", &body);
    // B2 (`guide-de-correction-robustesse.md` §F3/F4), fixed: a panel at
    // (row, column) beyond `MAX_PANEL_POSITION` (10,000) is now refused ON
    // LOAD, before `PlotSpec::rows()`/`columns()` have any chance of
    // overflowing on the position it carried.
    let result = load(&path);
    println!("loading a panel at (u64::MAX, u64::MAX) → {result:?}");
    assert!(
        result.is_err(),
        "a panel at an extreme position must be refused on load, not left to \
         PlotSpec::rows()/columns(): {result:?}"
    );
}

#[test]
fn b3_series_referencing_a_missing_or_non_column_node() {
    let dir = TempDir::new().unwrap();
    // 999 names no node; 1 is the root, a folder and not a column.
    let dangling_series = SeriesSpecRecord {
        y: 999,
        ..SeriesSpecRecord::default()
    };
    let folder_series = SeriesSpecRecord {
        y: 1,
        ..SeriesSpecRecord::default()
    };
    let panel = PanelSpecRecord {
        series: vec![dangling_series, folder_series],
        ..PanelSpecRecord::default()
    };
    let spec = PlotSpecRecord {
        title: String::new(),
        panels: vec![panel],
        background: String::new(),
    };
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "G1".into(),
                parent: Some(1),
                kind: KindRecord::Graph {
                    label: String::new(),
                    comment: String::new(),
                    spec,
                },
            },
        ],
    };
    let path = write_file(&dir, "broken.stf", &body);
    let project = load(&path).expect("dangling reference: accepted by contract");
    let tree = project.tree();
    let node = tree.node(stokesforge_core::NodeId::from_raw(2)).unwrap();
    let spec = match node.kind() {
        stokesforge_core::NodeKind::Graph { spec, .. } => spec,
        _ => panic!(),
    };
    let resolved = stokesforge_core::resolve_spec(tree, spec);
    assert_eq!(
        resolved.broken_series().count(),
        2,
        "both series must be marked broken"
    );
    assert_eq!(resolved.point_count(), 0);
}

#[test]
fn b3_float_values_length_not_a_multiple_of_eight() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 5,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "S1".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![4] },
            },
            NodeRecord {
                id: 4,
                name: "A".into(),
                parent: Some(3),
                kind: KindRecord::Column {
                    column_type: TypeRecord::Numeric,
                    role: RoleRecord::Y,
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(vec![0u8; 13]), // 13 = not a multiple of 8
                    version: 0,
                },
            },
        ],
    };
    let path = write_file(&dir, "wrong_size.stf", &body);
    let r = load(&path);
    assert!(r.is_err(), "a 13-byte buffer was accepted as f64 values");
    println!("13 bytes -> {}", r.unwrap_err());
}

#[test]
fn b3_column_type_inconsistent_with_its_values() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 5,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "S1".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![4] },
            },
            NodeRecord {
                id: 4,
                name: "A".into(),
                parent: Some(3),
                kind: KindRecord::Column {
                    column_type: TypeRecord::Numeric, // declared numeric
                    role: RoleRecord::Y,
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Mixed {
                        tags: vec![2],
                        numbers: Vec::new(),
                        texts: vec!["x".into()],
                    }, // but text
                    version: 0,
                },
            },
        ],
    };
    let path = write_file(&dir, "inconsistent.stf", &body);
    let r = load(&path);
    assert!(r.is_err(), "a numeric column containing text was accepted");
    println!("inconsistent type -> {}", r.unwrap_err());
}

#[test]
fn b3_duplicate_window_names_are_refused() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 4,
        nodes: vec![
            folder(1, "R", None, vec![2, 3]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![],
                    label: String::new(),
                    comment: String::new(),
                },
            },
        ],
    };
    let path = write_file(&dir, "duplicate.stf", &body);
    let r = load(&path);
    assert!(
        r.is_err(),
        "two windows with the same name accepted: [Book1] becomes ambiguous"
    );
    println!("same-name windows -> {}", r.unwrap_err());
}

#[test]
fn b3_child_declared_twice_is_refused() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            folder(1, "R", None, vec![2, 2]),
            folder(2, "A", Some(1), vec![]),
        ],
    };
    let path = write_file(&dir, "twice.stf", &body);
    let r = load(&path);
    assert!(r.is_err(), "a child listed twice was accepted");
    println!("doubled child -> {}", r.unwrap_err());
}

#[test]
fn b3_column_outside_a_sheet_is_refused() {
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            column(2, "A", 1, vec![1.0]), // a column directly under a folder
        ],
    };
    let path = write_file(&dir, "misplaced.stf", &body);
    let r = load(&path);
    assert!(r.is_err(), "a column stored under a folder was accepted");
    println!("misplaced column -> {}", r.unwrap_err());
}

#[test]
fn b3_invalid_name_is_refused() {
    let dir = TempDir::new().unwrap();
    for bad in ["", " A", "A ", "A!B", "[A]", "a/b"] {
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![
                folder(1, "R", None, vec![2]),
                folder(2, bad, Some(1), vec![]),
            ],
        };
        let path = write_file(&dir, "name.stf", &body);
        let r = load(&path);
        assert!(r.is_err(), "the name {bad:?} was accepted on load");
    }
}

/// A2 (#316): a file bigger than the cap is refused before being read (the
/// file size is its decompressed size: the `.stf` is not compressed), without
/// even checking whether it is well formed.
#[test]
fn b3_file_bigger_than_the_cap_is_refused_before_reading() {
    use stokesforge_persistence::{load_with_limit, PersistenceError};
    let dir = TempDir::new().unwrap();
    let body = FileBody {
        root: 1,
        next_id_hint: 2,
        nodes: vec![folder(1, "R", None, vec![])],
    };
    let path = write_file(&dir, "limit.stf", &body);
    let size = std::fs::metadata(&path).unwrap().len();
    assert!(load_with_limit(&path, size).is_ok());
    assert!(
        matches!(
            load_with_limit(&path, size - 1),
            Err(PersistenceError::TooLarge { .. })
        ),
        "a file above the cap must be refused"
    );
    // Noise bigger than the cap: refused for its size, not decoded.
    let noise = dir.path().join("noise.stf");
    std::fs::write(&noise, vec![0xC1u8; 4096]).unwrap();
    assert!(matches!(
        load_with_limit(&noise, 1024),
        Err(PersistenceError::TooLarge { size: 4096, .. })
    ));
}

/// #325: the column-width block of a hostile file: NaN, infinity, zero,
/// negative, orphan key, out-of-range identifier. An unreadable entry is
/// dropped alone (invariant 10), never the file; an out-of-range identifier
/// is refused as for geometries.
#[test]
fn b3_hostile_column_widths() {
    use std::collections::BTreeMap;
    use stokesforge_core::NodeId;
    use stokesforge_persistence::load_with_report;

    #[derive(Serialize)]
    struct FileBodyWidths {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
        column_widths: BTreeMap<u64, f32>,
    }
    let write_widths = |dir: &TempDir, name: &str, widths: &[(u64, f32)]| {
        let body = FileBodyWidths {
            root: 1,
            next_id_hint: 3,
            nodes: vec![
                folder(1, "R", None, vec![2]),
                folder(2, "A", Some(1), vec![]),
            ],
            column_widths: widths.iter().copied().collect(),
        };
        let payload = rmp_serde::to_vec_named(&body).unwrap();
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"STKF").unwrap();
        f.write_all(&1u32.to_le_bytes()).unwrap();
        f.write_all(&payload).unwrap();
        path
    };
    let dir = TempDir::new().unwrap();
    let path = write_widths(
        &dir,
        "mixed.stf",
        &[
            (1, f32::NAN),
            (2, 48.5),
            (3, 60.0), // no node
            (4, f32::INFINITY),
            (5, 0.0),
            (6, -12.0),
        ],
    );
    let r = std::panic::catch_unwind(|| load_with_report(&path));
    assert!(r.is_ok(), "panic on hostile widths");
    let (_, view, _) = r
        .unwrap()
        .expect("hostile widths do not make the file fail");
    assert_eq!(
        view.column_widths,
        [(NodeId::from_raw(2), 48.5)].into_iter().collect(),
        "only the sound entry next to hostile entries must remain"
    );
    let out_of_range = write_widths(&dir, "out_of_range.stf", &[(u64::MAX, 50.0)]);
    assert!(load_with_report(&out_of_range).is_err());
}

/// B1 (#316): a file whose columns declare more cells than the global budget
/// is refused before any column allocation. The forged columns are each under
/// `MAX_COLUMN_ROWS`: only the total stops them.
#[test]
fn b3_global_cell_budget_refuses_the_total() {
    use stokesforge_core::{Column, Project};
    use stokesforge_persistence::{load_with_limits, save, LoadLimits, PersistenceError};
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("P").unwrap();
    let root = project.root();
    let book = project.add_workbook(root, Some("B")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    for name in ["A", "B", "C"] {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[0.0; 1000]))
            .unwrap();
    }
    let path = dir.path().join("total.stf");
    save(&project, &path).unwrap();
    let with_max_cells = |max_cells| LoadLimits {
        max_cells,
        ..LoadLimits::default()
    };
    assert!(load_with_limits(&path, with_max_cells(3000)).is_ok());
    assert!(matches!(
        load_with_limits(&path, with_max_cells(2999)),
        Err(PersistenceError::TooManyCells { .. })
    ));
}

/// #253: the linked transposed sheets block of a hostile file: link to a
/// missing sheet, to a workbook, to itself, out-of-range identifier, missing
/// column. An unreadable link is dropped alone (invariant 10): never the file,
/// and the sheet stays, ordinary; the sound neighboring link lives.
#[test]
fn b3_hostile_transposed_sheets() {
    use stokesforge_core::NodeId;

    #[derive(Serialize)]
    struct LinkRow {
        row: u64,
        column: u64,
        stamp: u64,
    }
    #[derive(Serialize)]
    struct Link {
        sheet: u64,
        source: u64,
        x_column: u64,
        x_stamp: u64,
        rows: Vec<LinkRow>,
        inputs: Vec<u8>,
    }
    #[derive(Serialize)]
    struct FileBodyLinks {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
        transposes: Vec<Link>,
    }
    let link = |sheet: u64, source: u64, column: u64| Link {
        sheet,
        source,
        x_column: column,
        x_stamp: 7,
        rows: vec![LinkRow {
            row: 0,
            column,
            stamp: 9,
        }],
        inputs: Vec::new(),
    };
    let dir = TempDir::new().unwrap();
    let body = FileBodyLinks {
        root: 1,
        next_id_hint: 5,
        nodes: vec![
            folder(1, "R", None, vec![2]),
            NodeRecord {
                id: 2,
                name: "Book1".into(),
                parent: Some(1),
                kind: KindRecord::Workbook {
                    sheets: vec![3, 4],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "S1".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![] },
            },
            NodeRecord {
                id: 4,
                name: "S1T".into(),
                parent: Some(2),
                kind: KindRecord::Worksheet { columns: vec![] },
            },
        ],
        transposes: vec![
            link(999, 3, 50),         // missing sheet
            link(2, 3, 50),           // "sheet" that is a workbook
            link(3, 3, 50),           // a sheet linked to itself
            link(4, 3, u64::MAX / 2), // out-of-range column identifier
            link(4, 3, 50),           // sound, missing column tolerated
        ],
    };
    let payload = rmp_serde::to_vec_named(&body).unwrap();
    let path = dir.path().join("links.stf");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"STKF").unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap();
    f.write_all(&payload).unwrap();
    drop(f);
    let r = std::panic::catch_unwind(|| load(&path));
    assert!(r.is_ok(), "panic on hostile links");
    let project = r.unwrap().expect("hostile links do not make the file fail");
    let links = project.tree().transposes();
    assert_eq!(
        links.keys().copied().collect::<Vec<_>>(),
        vec![NodeId::from_raw(4)],
        "only the sound link survives"
    );
    assert!(
        project.tree().node(NodeId::from_raw(4)).is_some(),
        "the sheet of a dropped link stays"
    );
}

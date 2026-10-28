//! Battery 19 — what a `.stf` written by a **newer** version does to this
//! reader.
//!
//! The format carries two compatibility mechanisms, and they must be told
//! apart:
//!
//! - the **version number** of the header, read before the body: a newer file
//!   is cleanly rejected (`UnsupportedVersion`), without a single byte of its
//!   body being decoded;
//! - the **named maps** of MessagePack: an unknown field is ignored, a missing
//!   field takes its `#[serde(default)]`. This is what must allow "adding data
//!   without breaking files already written", **without touching the version
//!   number** — this is how it is written in the top-level doc of
//!   `stokesforge-persistence` and of `schema.rs`.
//!
//! The second mechanism covers **fields**. It does not cover **variants** — and
//! nothing says so.
//!
//! The schema is private to the crate; we redeclare it here, like `b3`. This is
//! legitimate: the format is self-describing, so two declarations that agree on
//! the names produce the same bytes.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use stokesforge_core::NodeKind;
use stokesforge_persistence::{load, save, PersistenceError};
use tempfile::TempDir;

// -- mirror schema, in its "future" version ------------------------------

#[derive(Serialize)]
struct FileBody<K> {
    root: u64,
    next_id_hint: u64,
    nodes: Vec<NodeRecord<K>>,
}

#[derive(Serialize)]
struct NodeRecord<K> {
    id: u64,
    name: String,
    parent: Option<u64>,
    kind: K,
}

/// Today's schema, **plus a field** that a future version would have added
/// to folders.
#[derive(Serialize)]
enum KindWithNewField {
    Folder {
        children: Vec<u64>,
        /// What v2 would have added.
        label_color: String,
    },
}

/// Today's schema, **plus a variant**: a future version that would know how
/// to store images in a folder.
#[derive(Serialize)]
enum KindWithNewVariant {
    Folder {
        children: Vec<u64>,
    },
    #[allow(dead_code)]
    Image {
        source: String,
    },
}

/// Mirror schema of a `PlotSpec`, **plus a plot type** that this reader does
/// not know. Only the fields matter: the values are the default ones, the file
/// must be valid everywhere else.
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

#[derive(Serialize, Default)]
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

#[derive(Serialize, Default)]
enum AxisScaleRecord {
    #[default]
    Linear,
}

#[derive(Serialize, Default)]
struct LegendSpecRecord {
    visible: bool,
    position: LegendPositionRecord,
}

#[derive(Serialize, Default)]
enum LegendPositionRecord {
    #[default]
    TopRight,
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

/// `Violin` does not exist in `stokesforge-plot`: it is the plot type that a
/// future version would have added.
#[derive(Serialize)]
enum PlotTypeRecord {
    #[allow(dead_code)]
    Line,
    Violin,
}

#[derive(Serialize, Default)]
struct LineSpecRecord {
    color: String,
    style: LineStyleRecord,
    width: f64,
}

#[derive(Serialize, Default)]
enum LineStyleRecord {
    #[default]
    Solid,
}

#[derive(Serialize, Default)]
struct SymbolSpecRecord {
    shape: SymbolShapeRecord,
    size: f64,
    color: String,
    edge_color: String,
}

#[derive(Serialize, Default)]
enum SymbolShapeRecord {
    #[default]
    Circle,
}

#[derive(Serialize, Default)]
enum SeriesAxisRecord {
    #[default]
    Left,
}

/// A tree that is valid from end to end, in which one graph carries the
/// faulty spec.
#[derive(Serialize)]
enum KindWithNewPlot {
    Folder {
        children: Vec<u64>,
    },
    Worksheet {
        columns: Vec<u64>,
    },
    Workbook {
        sheets: Vec<u64>,
        label: String,
        comment: String,
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
}

#[derive(Serialize)]
enum TypeRecord {
    Numeric,
}

#[derive(Serialize)]
enum RoleRecord {
    Y,
}

#[derive(Serialize)]
enum ValuesRecord {
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
}

/// Writes a body behind the `STKF` + `version` header.
fn write_file<K: Serialize>(
    dir: &TempDir,
    name: &str,
    version: u32,
    body: &FileBody<K>,
) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).expect("file created");
    file.write_all(b"STKF").expect("signature");
    file.write_all(&version.to_le_bytes())
        .expect("version number");
    file.write_all(&rmp_serde::to_vec_named(body).expect("encoding"))
        .expect("body");
    path
}

// -- guards ---------------------------------------------------------------

#[test]
fn b19_an_unknown_field_is_indeed_ignored() {
    // The promise kept: this is the mechanism on which all the evolution of
    // the format rests, and it works.
    let dir = TempDir::new().expect("directory");
    let body = FileBody {
        root: 1,
        next_id_hint: 2,
        nodes: vec![NodeRecord {
            id: 1,
            name: "R".to_string(),
            parent: None,
            kind: KindWithNewField::Folder {
                children: Vec::new(),
                label_color: "#ff0000".to_string(),
            },
        }],
    };
    let project = load(&write_file(&dir, "field.stf", 1, &body)).expect("unknown field ignored");
    assert_eq!(project.tree().len(), 1);
}

#[test]
fn b19_a_higher_version_number_is_cleanly_rejected() {
    let dir = TempDir::new().expect("directory");
    let body = FileBody {
        root: 1,
        next_id_hint: 2,
        nodes: vec![NodeRecord {
            id: 1,
            name: "R".to_string(),
            parent: None,
            kind: KindWithNewVariant::Folder {
                children: Vec::new(),
            },
        }],
    };
    let error = load(&write_file(&dir, "v2.stf", 2, &body)).expect_err("future version rejected");
    assert!(
        matches!(error, PersistenceError::UnsupportedVersion { found: 2, .. }),
        "{error:?}"
    );
}

// -- guards (C5, level 2) -----------------------------------------------

#[test]
// C5 (level 2) — fixed: `KindRecord`/`PlotTypeRecord` (`schema.rs`) no
// longer have `Serialize`/`Deserialize` derived, but written by hand
// (`unknown.rs`). Decoding first captures the MessagePack value that follows
// the variant name in a generic tree (`rmpv::Value`), re-encodes it, then
// tries to read it back as one of the known variants with the same `rmp_serde`
// decoder as for the rest of the file. An unknown variant name — or a known
// variant whose content fails deeper down (an unknown plot type buried in the
// spec of a graph, `b19_an_unknown_plot_type_does_not_lose_the_data` below) —
// no longer propagates the error: the raw name and content are kept in
// `KindRecord::Unknown`/`PlotTypeRecord::Unknown`, which carry
// `stokesforge_plot::RawVariant` all the way into the in-memory model
// (`NodeKind::Unknown`, `PlotType::Unknown`) so that a following save
// rewrites the node identically (see `b20_shared_file.rs`, unchanged and
// still green). The loss is bounded to that single node, never to the whole
// file.
fn b19_an_unknown_variant_does_not_lose_the_whole_file() {
    let dir = TempDir::new().expect("directory");
    // A perfectly readable project: a root, and **a single** node of a kind
    // that this reader does not know.
    let body = FileBody {
        root: 1,
        next_id_hint: 3,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".to_string(),
                parent: None,
                kind: KindWithNewVariant::Folder { children: vec![2] },
            },
            NodeRecord {
                id: 2,
                name: "Photo".to_string(),
                parent: Some(1),
                kind: KindWithNewVariant::Image {
                    source: "manip.png".to_string(),
                },
            },
        ],
    };
    // Version 1 in the header, on purpose: this is the scenario the
    // documentation encourages — adding data without touching the number.
    let path = write_file(&dir, "image.stf", 1, &body);
    let project = load(&path).expect(
        "a node of an unknown kind must no longer take the whole file with it — the \
         others, perfectly readable, must not be lost along with it",
    );
    println!("loaded: {} nodes", project.tree().len());
    assert_eq!(
        project.tree().len(),
        2,
        "the root AND the unknown node must survive"
    );

    let photo = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(2))
        .expect("node \"Photo\" present despite its unknown kind");
    match photo.kind() {
        NodeKind::Unknown(raw) => assert_eq!(
            raw.name, "Image",
            "the name of the original variant must survive"
        ),
        other => panic!("expected NodeKind::Unknown, got {other:?}"),
    }

    // Two successive saves of the same project must produce the same file
    // (B20, `b20_two_successive_saves_give_the_same_file`, unchanged and
    // still green): this is the guarantee that bounds the loss to the unknown
    // node, never to its rewrite fidelity.
    let a = dir.path().join("image_a.stf");
    let b = dir.path().join("image_b.stf");
    save(&project, &a).expect("first save of a project carrying an unknown node");
    save(&project, &b).expect("second save of the same project");
    assert_eq!(
        std::fs::read(&a).expect("read a"),
        std::fs::read(&b).expect("read b"),
        "the node of an unknown kind must survive two identical saves, \
         not be lost or renormalized"
    );
}

#[test]
// C5 (level 2) — fixed, same mechanism as above applied to
// `PlotTypeRecord`/`PlotType`: an unknown plot type buried in the spec of a
// graph no longer makes the `Graph` that carries it fail — still less the
// workbook and the column that have nothing to do with it.
fn b19_an_unknown_plot_type_does_not_lose_the_data() {
    // The realistic case: a colleague plots a violin with a newer version and
    // sends the file. Everything else — the workbook, the worksheet, the
    // thousand measurements — is perfectly readable by this reader.
    let dir = TempDir::new().expect("directory");
    let measurements: Vec<u8> = (0..1000u32)
        .flat_map(|i| f64::from(i).to_le_bytes())
        .collect();
    let data_bytes = measurements.len();

    let series = SeriesSpecRecord {
        y: 4,
        x: None,
        x_error: None,
        y_error: None,
        plot_type: PlotTypeRecord::Violin, // the only field that this reader does not know
        name: String::new(),
        line: LineSpecRecord::default(),
        symbol: SymbolSpecRecord::default(),
        axis: SeriesAxisRecord::default(),
        visible: true,
        show_in_legend: true,
        bar_width: None,
        histogram_bins: 40,
    };
    let spec = PlotSpecRecord {
        panels: vec![PanelSpecRecord {
            series: vec![series],
            ..PanelSpecRecord::default()
        }],
        ..PlotSpecRecord::default()
    };

    let body = FileBody {
        root: 1,
        next_id_hint: 6,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".to_string(),
                parent: None,
                kind: KindWithNewPlot::Folder {
                    children: vec![2, 5],
                },
            },
            NodeRecord {
                id: 2,
                name: "Book1".to_string(),
                parent: Some(1),
                kind: KindWithNewPlot::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "Sheet1".to_string(),
                parent: Some(2),
                kind: KindWithNewPlot::Worksheet { columns: vec![4] },
            },
            NodeRecord {
                id: 4,
                name: "Signal".to_string(),
                parent: Some(3),
                kind: KindWithNewPlot::Column {
                    column_type: TypeRecord::Numeric,
                    role: RoleRecord::Y,
                    unit: "V".to_string(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: ValuesRecord::Float(measurements),
                    version: 7,
                },
            },
            NodeRecord {
                id: 5,
                name: "Fig1".to_string(),
                parent: Some(1),
                kind: KindWithNewPlot::Graph {
                    label: String::new(),
                    comment: String::new(),
                    spec,
                },
            },
        ],
    };

    let path = write_file(&dir, "violin.stf", 1, &body);
    let project = load(&path).expect(
        "a single unknown `plot_type` field, in a single series of a single graph, \
         must no longer take the measurements of the same file with it",
    );
    println!("loaded: {} nodes", project.tree().len());
    assert_eq!(
        project.tree().len(),
        5,
        "the workbook, the worksheet, the {data_bytes}-byte column and the graph \
         must all survive"
    );

    // The unknown plot type is captured at the scale of the series, not of the
    // whole graph: `PlotTypeRecord`/`PlotType` have the same `Unknown`
    // mechanism as `KindRecord`/`NodeKind` (see the doc above the test). The
    // graph itself stays a proper `NodeKind::Graph`, with its panel and its
    // series — only `plot_type` is opaque.
    let graph = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(5))
        .expect("node \"Fig1\" present");
    match graph.kind() {
        NodeKind::Graph { spec, .. } => {
            let series = &spec.panels[0].series[0];
            match &series.plot_type {
                stokesforge_plot::PlotType::Unknown(raw) => {
                    assert_eq!(raw.name, "Violin", "the plot type name must survive");
                }
                other => panic!("expected PlotType::Unknown, got {other:?}"),
            }
        }
        other => panic!(
            "expected NodeKind::Graph (degradation at series scale, not node scale), got {other:?}"
        ),
    }

    // The measurements themselves, carried by the "Signal" column, must be
    // intact: this node has nothing to do with the unknown plot type.
    let column = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(4))
        .expect("node \"Signal\" present")
        .as_column()
        .expect("\"Signal\" is a column");
    assert_eq!(
        column
            .float_values()
            .expect("numeric column")
            .as_slice()
            .len(),
        1000,
        "the 1000 measurements must survive intact"
    );

    // Same fidelity check as in the previous test (B20): two successive saves
    // of the same project are identical.
    let a = dir.path().join("violin_a.stf");
    let b = dir.path().join("violin_b.stf");
    save(&project, &a).expect("first save of a project carrying an unknown plot type");
    save(&project, &b).expect("second save of the same project");
    assert_eq!(
        std::fs::read(&a).expect("read a"),
        std::fs::read(&b).expect("read b"),
        "the unknown plot type must survive two identical saves"
    );
}

// -- the *backward* compatibility of mixed-type columns ----------------------

/// Today's schema, **plus a column type**: what this reader would see in a
/// file written by a newer version.
///
/// The real case that motivates this test is the reverse in time — a reader
/// from **before** mixed-type columns opening a file that carries one, and so
/// meeting the variant name `"Mixed"` that it does not know. It is exactly the
/// same mechanism; this test exercises it with a name that *no* version
/// assigns, so that it stays valid now that `Mixed` is known to this reader.
#[derive(Serialize)]
enum KindWithNewColumnType {
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
        column_type: FutureTypeRecord,
        role: RoleRecord,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: FutureValuesRecord,
        version: u64,
    },
}

#[derive(Serialize)]
enum FutureTypeRecord {
    #[allow(dead_code)]
    Numeric,
    /// A column type that no version ever wrote — here it plays the role that
    /// `Mixed` would play for an earlier reader.
    Complex,
}

#[derive(Serialize)]
enum FutureValuesRecord {
    #[allow(dead_code)]
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
    /// The matching form of values: parallel arrays, like
    /// `ValuesRecord::Mixed` in today's schema.
    Complex {
        #[serde(with = "serde_bytes")]
        real_parts: Vec<u8>,
        #[serde(with = "serde_bytes")]
        imaginary_parts: Vec<u8>,
    },
}

#[test]
// What makes the migration strategy for mixed-type columns tenable **without
// touching the format's version number**: an older reader that meets a column
// type it does not know loses only that column, never the file. This is the C5
// (level 2) mechanism of `unknown.rs`, already in place for node kinds — a
// column is a `KindRecord::Column`, so it benefits from it without a single
// line specific to it. This test checks it explicitly, because the decision
// not to increment `FORMAT_VERSION` rests on it.
fn b19_an_unknown_column_type_loses_only_that_column() {
    let dir = TempDir::new().expect("directory");
    let measurements: Vec<u8> = (0..500u32)
        .flat_map(|i| f64::from(i).to_le_bytes())
        .collect();

    let body = FileBody {
        root: 1,
        next_id_hint: 6,
        nodes: vec![
            NodeRecord {
                id: 1,
                name: "R".to_string(),
                parent: None,
                kind: KindWithNewColumnType::Folder { children: vec![2] },
            },
            NodeRecord {
                id: 2,
                name: "Book1".to_string(),
                parent: Some(1),
                kind: KindWithNewColumnType::Workbook {
                    sheets: vec![3],
                    label: String::new(),
                    comment: String::new(),
                },
            },
            NodeRecord {
                id: 3,
                name: "Sheet1".to_string(),
                parent: Some(2),
                kind: KindWithNewColumnType::Worksheet {
                    columns: vec![4, 5],
                },
            },
            NodeRecord {
                id: 4,
                name: "Signal".to_string(),
                parent: Some(3),
                kind: KindWithNewColumnType::Column {
                    column_type: FutureTypeRecord::Numeric,
                    role: RoleRecord::Y,
                    unit: "V".to_string(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: FutureValuesRecord::Float(measurements),
                    version: 3,
                },
            },
            NodeRecord {
                id: 5,
                name: "Exotic".to_string(),
                parent: Some(3),
                kind: KindWithNewColumnType::Column {
                    column_type: FutureTypeRecord::Complex,
                    role: RoleRecord::Y,
                    unit: String::new(),
                    long_name: String::new(),
                    comment: String::new(),
                    display_format: String::new(),
                    values: FutureValuesRecord::Complex {
                        real_parts: 1.0f64.to_le_bytes().to_vec(),
                        imaginary_parts: 2.0f64.to_le_bytes().to_vec(),
                    },
                    version: 9,
                },
            },
        ],
    };

    let path = write_file(&dir, "complex.stf", 1, &body);
    let project = load(&path).expect(
        "an unknown column type must not take the file with it: this is the \
         condition for a new column type to be added without incrementing \
         FORMAT_VERSION",
    );
    assert_eq!(
        project.tree().len(),
        5,
        "the four readable nodes AND the unknown column must survive"
    );

    // The neighboring column, perfectly readable, keeps its 500 measurements.
    let signal = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(4))
        .expect("node \"Signal\" present")
        .as_column()
        .expect("\"Signal\" is a column");
    assert_eq!(
        signal
            .float_values()
            .expect("numeric column")
            .as_slice()
            .len(),
        500,
        "the 500 measurements of the neighboring column must survive intact"
    );

    // The exotic column, for its part, becomes an opaque node.
    let exotic = project
        .tree()
        .node(stokesforge_core::NodeId::from_raw(5))
        .expect("node \"Exotic\" present despite its unknown column type");
    match exotic.kind() {
        NodeKind::Unknown(raw) => assert_eq!(raw.name, "Column"),
        other => panic!("expected NodeKind::Unknown, got {other:?}"),
    }

    // And the rewrite fidelity (B20): what this reader does not understand, it
    // returns as is — so a round trip through an older version does not
    // destroy the mixed column of a newer version.
    let a = dir.path().join("complex_a.stf");
    let b = dir.path().join("complex_b.stf");
    save(&project, &a).expect("first save");
    save(&project, &b).expect("second save");
    assert_eq!(
        std::fs::read(&a).expect("read a"),
        std::fs::read(&b).expect("read b"),
        "the column of an unknown type must survive two identical saves"
    );
}

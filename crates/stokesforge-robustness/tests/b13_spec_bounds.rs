//! Battery 13 — what a `PlotSpec` coming from disk can ask of the renderer.
//!
//! `stokesforge-plot` made "zero bins" inexpressible (`histogram_bins:
//! NonZeroU32`), and `stokesforge-persistence` rejects a zero from a file
//! (`ConvertError::ZeroHistogramBins`). The other end of the interval was for
//! a long time the only unbounded one — neither the type nor the conversion
//! bounded it. B4 (`guide-de-correction-robustesse.md` §F4) closed both:
//! `stokesforge_plot::BinCount` makes an excess inexpressible at the type
//! level (`b13_the_type_itself_bounds_the_number_of_bins`), and
//! `stokesforge_persistence::convert::record_to_series_spec` rejects on read a
//! raw value that exceeds the same bound
//! (`b13_histogram_bins_from_disk_is_bounded`) — needed separately, since a
//! hostile file written by another tool is not required to go through
//! `BinCount` to exist on disk.

use std::num::NonZeroU32;

use stokesforge_core::{Column, Project};
use stokesforge_plot::{BinCount, PanelSpec, Panels, PlotSpec, PlotType, SeriesSpec};
use tempfile::TempDir;

/// Order of magnitude beyond which a number of bins no longer describes a
/// histogram but a corrupted file. A screen is a few thousand pixels wide;
/// 65,536 bins are already ten times too many — this is also
/// [`stokesforge_plot::MAX_HISTOGRAM_BINS`], checked below.
const REASONABLE_CAP: u32 = 1 << 16;

// -- guards ---------------------------------------------------------------

#[test]
fn b13_an_ordinary_number_of_bins_crosses_the_disk_unchanged() {
    let bins = BinCount::new(40).expect("40 is within bounds");
    let dir = TempDir::new().expect("temporary directory");
    let path = dir.path().join("histogram.stf");

    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column Y");
    let series = SeriesSpec {
        plot_type: PlotType::Histogram,
        histogram_bins: bins,
        ..SeriesSpec::new(y)
    };
    let panel = PanelSpec {
        series: vec![series],
        ..PanelSpec::default()
    };
    project
        .add_graph(
            root,
            Some("Fig1"),
            PlotSpec::default().with_panels(Panels::one(panel)),
        )
        .expect("graph");

    stokesforge_persistence::save(&project, &path).expect("save");
    let reloaded = stokesforge_persistence::load(&path).expect("reload");
    let fig = reloaded.resolve_path("[Fig1]").expect("graph present");
    let node = reloaded.tree().node(fig).expect("node present");
    let reloaded_bins = match node.kind() {
        stokesforge_core::NodeKind::Graph { spec, .. } => spec.panels[0].series[0].histogram_bins,
        other => panic!("expected a graph, got {}", other.type_name()),
    };
    assert_eq!(reloaded_bins, bins);
}

#[test]
fn b13_an_ordinary_histogram_computes_without_incident() {
    let (edges, counts) = stokesforge_core::resolve::histogram(
        &[1.0, 2.0, 3.0, 4.0],
        NonZeroU32::new(4).expect("non-zero"),
    );
    assert_eq!(edges.len(), counts.len() + 1);
    assert_eq!(counts.iter().sum::<f64>(), 4.0);
}

#[test]
fn b13_the_type_cap_is_the_documented_one() {
    assert_eq!(stokesforge_plot::MAX_HISTOGRAM_BINS, REASONABLE_CAP);
}

// -- reproductions --------------------------------------------------------

#[test]
fn b13_the_type_itself_bounds_the_number_of_bins() {
    // B4 — `BinCount` (unlike the bare `NonZeroU32` from before this fix)
    // makes an excess inexpressible: the construction itself fails, there is no
    // absurd value left to inspect afterwards.
    let too_large = BinCount::new(u32::MAX);
    println!("BinCount::new(u32::MAX) = {too_large:?}");
    assert!(
        too_large.is_none(),
        "the type must refuse to represent {} bins, beyond the cap {}",
        u32::MAX,
        REASONABLE_CAP
    );
    let limit = BinCount::new(REASONABLE_CAP);
    assert!(limit.is_some(), "the cap itself must remain constructible");
    assert!(
        BinCount::new(REASONABLE_CAP + 1).is_none(),
        "one bin more than the cap must be rejected"
    );
}

/// On-disk schema redeclared identically to forge a hostile file — see
/// `b3_hostile_files.rs`, which documents why this is legitimate
/// (self-describing format: two declarations that agree on the names produce
/// the same bytes). Reduced to the only fields that a one-histogram graph
/// needs.
mod hostile {
    use serde::Serialize;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[derive(Serialize)]
    pub(super) struct FileBody {
        pub root: u64,
        pub next_id_hint: u64,
        pub nodes: Vec<NodeRecord>,
    }
    #[derive(Serialize)]
    pub(super) struct NodeRecord {
        pub id: u64,
        pub name: String,
        pub parent: Option<u64>,
        pub kind: KindRecord,
    }
    #[derive(Serialize)]
    pub(super) enum KindRecord {
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
        Graph {
            label: String,
            comment: String,
            spec: PlotSpecRecord,
        },
    }
    #[derive(Serialize, Default)]
    pub(super) struct PlotSpecRecord {
        pub title: String,
        pub panels: Vec<PanelSpecRecord>,
        pub background: String,
    }
    #[derive(Serialize, Default)]
    pub(super) struct PanelSpecRecord {
        pub title: String,
        pub series: Vec<SeriesSpecRecord>,
        pub bottom: AxisSpecRecord,
        pub left: AxisSpecRecord,
        pub right: AxisSpecRecord,
        pub top: AxisSpecRecord,
        pub legend: LegendSpecRecord,
        pub row: u64,
        pub column: u64,
    }
    #[derive(Serialize)]
    pub(super) struct AxisSpecRecord {
        pub label: String,
        pub unit: String,
        pub scale: AxisScaleRecord,
        pub auto_range: bool,
        pub range_min: Option<f64>,
        pub range_max: Option<f64>,
        pub grid: bool,
        pub visible: bool,
        pub inverted: bool,
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
    pub(super) enum AxisScaleRecord {
        Linear,
    }
    #[derive(Serialize)]
    pub(super) struct SeriesSpecRecord {
        pub y: u64,
        pub x: Option<u64>,
        pub x_error: Option<u64>,
        pub y_error: Option<u64>,
        pub plot_type: PlotTypeRecord,
        pub name: String,
        pub line: LineSpecRecord,
        pub symbol: SymbolSpecRecord,
        pub axis: SeriesAxisRecord,
        pub visible: bool,
        pub show_in_legend: bool,
        pub bar_width: Option<f64>,
        pub histogram_bins: u32,
    }
    impl Default for SeriesSpecRecord {
        fn default() -> Self {
            Self {
                y: 1,
                x: None,
                x_error: None,
                y_error: None,
                plot_type: PlotTypeRecord::Histogram,
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
    pub(super) enum PlotTypeRecord {
        Histogram,
    }
    #[derive(Serialize)]
    pub(super) enum SeriesAxisRecord {
        Left,
    }
    #[derive(Serialize)]
    pub(super) struct LineSpecRecord {
        pub color: String,
        pub style: LineStyleRecord,
        pub width: f64,
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
    pub(super) enum LineStyleRecord {
        Solid,
    }
    #[derive(Serialize)]
    pub(super) struct SymbolSpecRecord {
        pub shape: SymbolShapeRecord,
        pub size: f64,
        pub color: String,
        pub edge_color: String,
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
    pub(super) enum SymbolShapeRecord {
        Circle,
    }
    #[derive(Serialize)]
    pub(super) struct LegendSpecRecord {
        pub visible: bool,
        pub position: LegendPositionRecord,
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
    pub(super) enum LegendPositionRecord {
        TopRight,
    }

    pub(super) fn write_file(dir: &TempDir, name: &str, body: &FileBody) -> PathBuf {
        let payload = rmp_serde::to_vec_named(body).unwrap();
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"STKF").unwrap();
        f.write_all(&1u32.to_le_bytes()).unwrap();
        f.write_all(&payload).unwrap();
        f.sync_all().unwrap();
        path
    }

    /// A valid file holding a single graph, whose only series is a histogram
    /// with `bins` raw bins — no `BinCount` here, this module simulates a file
    /// written by a tool that never heard of it.
    pub(super) fn project_with_histogram_bins(dir: &TempDir, bins: u32) -> PathBuf {
        let series = SeriesSpecRecord {
            histogram_bins: bins,
            ..SeriesSpecRecord::default()
        };
        let panel = PanelSpecRecord {
            series: vec![series],
            ..PanelSpecRecord::default()
        };
        let spec = PlotSpecRecord {
            panels: vec![panel],
            ..PlotSpecRecord::default()
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 5,
            nodes: vec![
                NodeRecord {
                    id: 1,
                    name: "R".to_string(),
                    parent: None,
                    kind: KindRecord::Folder {
                        children: vec![2, 4],
                    },
                },
                NodeRecord {
                    id: 2,
                    name: "Book1".to_string(),
                    parent: Some(1),
                    kind: KindRecord::Workbook {
                        sheets: vec![3],
                        label: String::new(),
                        comment: String::new(),
                    },
                },
                NodeRecord {
                    id: 3,
                    name: "Sheet1".to_string(),
                    parent: Some(2),
                    kind: KindRecord::Worksheet { columns: vec![] },
                },
                NodeRecord {
                    id: 4,
                    name: "Fig1".to_string(),
                    parent: Some(1),
                    kind: KindRecord::Graph {
                        label: String::new(),
                        comment: String::new(),
                        spec,
                    },
                },
            ],
        };
        write_file(dir, "hostile_histogram.stf", &body)
    }
}

#[test]
fn b13_histogram_bins_from_disk_is_bounded() {
    let dir = TempDir::new().expect("temporary directory");
    // `BinCount` now prevents writing this scenario through the public API
    // (see `b13_the_type_itself_bounds_the_number_of_bins`): a hostile file
    // written by a third-party tool is not required to go through `BinCount`
    // to exist — hence the hostile file forged here.
    let path = hostile::project_with_histogram_bins(&dir, u32::MAX);
    let result = stokesforge_persistence::load(&path);
    println!(
        "loading a file with histogram_bins = {} → {result:?}",
        u32::MAX
    );
    assert!(
        result.is_err(),
        "a histogram_bins of {} (far beyond the cap {REASONABLE_CAP}) must be \
         rejected at load time rather than letting the renderer ask for the \
         allocation that would abort() the process (`cargo run --bin \
         stokesforge-alloc-probe -- histogram_bins 4294967295`)",
        u32::MAX
    );
}

// -- frame sizes (#394) ------------------------------------------------------

/// A hostile file can store anything as the width of a column or the height of
/// a row: NaN, negative, absurdly large, far more entries than the grid has
/// cells. The record crosses the disk as written, and the spec reads it back
/// within bounds (the drawing of such a figure is `render/tests/figure_layout.rs`):
/// no frame below the minimum or above the maximum, and a figure that cannot ask
/// for the moon (a PNG ×4 of it would not fit anywhere).
#[test]
fn b13_hostile_frame_sizes_cross_the_disk_and_read_back_bounded() {
    let dir = TempDir::new().expect("temporary directory");
    let path = dir.path().join("hostile.stf");
    let mut project = Project::new("Hostile").expect("valid project");
    let panels: Vec<PanelSpec> = (0..16)
        .flat_map(|r| (0..16).map(move |c| PanelSpec::at(r, c)))
        .collect();
    let hostile: Vec<f64> = [f64::NAN, 1e300, -3.0, f64::INFINITY]
        .into_iter()
        .cycle()
        .take(1000)
        .collect();
    let spec = PlotSpec {
        frame_widths: hostile.clone(),
        frame_heights: vec![2000.0; 16],
        ..PlotSpec::empty("T").with_panels(Panels::new(panels))
    };
    let root = project.root();
    let graph = project.add_graph(root, Some("Fig1"), spec).expect("graph");

    stokesforge_persistence::save(&project, &path).expect("save");
    let reloaded = stokesforge_persistence::load(&path).expect("reload");
    let stokesforge_core::NodeKind::Graph { spec, .. } =
        reloaded.tree().node(graph).expect("graph").kind()
    else {
        panic!("a graph expected");
    };
    assert_eq!(spec.frame_widths.len(), 1000, "kept as written");
    let widths = spec.sanitized_frame_widths().expect("stored");
    let heights = spec.sanitized_frame_heights().expect("stored");
    assert_eq!((widths.len(), heights.len()), (16, 16));
    let extent = stokesforge_plot::MAX_FIGURE_EXTENT;
    assert!(widths.iter().sum::<f64>() <= extent + 1e-6);
    assert!(heights.iter().sum::<f64>() <= extent + 1e-6);
    assert!(widths.iter().all(|w| w.is_finite() && *w > 0.0));
}

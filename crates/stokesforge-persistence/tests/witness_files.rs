//! Witness files of the `.stf` format (S4, #323).
//!
//! `witnesses/v1/project.stf` is what V1 writes for the project of
//! [`witness_project`], which exercises every node type, column type and
//! series type, with non-default values everywhere (a renamed field that falls
//! back to its default value makes the comparison fail). Two guards:
//!
//! - **read back**: the file reads back and gives the expected project, with
//!   its view tables and a clean report;
//! - **drift**: saving the project gives exactly the bytes of the file. A
//!   change to the written format therefore goes through a decision: the
//!   version number goes up (the old witness freezes in its `vN/` folder), or
//!   the change is additive and the witness is regenerated.
//!
//! Regenerate (explicit gesture):
//! `STOKESFORGE_REGENERATE_WITNESSES=1 cargo test -p stokesforge-persistence --test witness_files regenerate -- --ignored`

mod common;

use std::collections::HashMap;
use std::path::PathBuf;

use common::{fixture_project, non_trivial_spec, recipe_non_default};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::{Operation, Warning};
use stokesforge_core::{
    AuxRef, CalculationDraft, Column, ColumnMeta, ColumnRole, ImportSource, MetaLine, NodeId,
    OutputDraft, Project, ResultValue, SeriesRef, SeriesResult,
};
use stokesforge_persistence::{
    load_with_report, save_with_geometries, ViewTables, WindowGeometry, WindowMode, WindowView,
};
use stokesforge_plot::{ColorUsage, PanelSpec, PlotSpec, PlotType, SavedColors, SeriesSpec};

fn witnesses_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/witnesses")
}

fn v1_file() -> PathBuf {
    witnesses_dir().join("v1/project.stf")
}

fn add_column(project: &mut Project, sheet: NodeId, name: &str, values: &[f64]) -> NodeId {
    project
        .add_column(sheet, None, Some(name), Column::from_f64(values))
        .expect("column added")
}

fn versions(project: &Project, ids: &[NodeId]) -> Vec<(NodeId, u64)> {
    ids.iter()
        .map(|id| {
            let column = project.tree().get(*id).ok().and_then(|n| n.as_column());
            (*id, column.expect("column present").version())
        })
        .collect()
}

/// The witness project and its view tables. Any change to this function
/// changes the expected bytes: regenerate.
fn witness_project() -> (Project, ViewTables) {
    let mut project = fixture_project();
    let root = project.root();

    // A workbook of measurements: one role per column, a formula, an import
    // source, file metadata.
    let book = project
        .add_workbook(root, Some("Mesures"))
        .expect("workbook");
    let sheet = project.tree().children(book).expect("sheet")[0];
    let x = add_column(&mut project, sheet, "X", &[0.0, 1.0, 2.0]);
    let y1 = add_column(&mut project, sheet, "Y1", &[1.0, 2.0, 3.0]);
    let y2 = add_column(&mut project, sheet, "Y2", &[4.0, 5.0, 6.0]);
    let roles = [
        ColumnRole::X2,
        ColumnRole::Y2,
        ColumnRole::X3,
        ColumnRole::Y3,
        ColumnRole::X4,
        ColumnRole::Y4,
        ColumnRole::Z,
        ColumnRole::XError,
        ColumnRole::YError,
        ColumnRole::Label,
        ColumnRole::None,
    ];
    for (i, role) in roles.into_iter().enumerate() {
        let id = add_column(&mut project, sheet, &format!("R{i}"), &[i as f64]);
        project
            .set_column_meta(
                id,
                ColumnMeta {
                    role,
                    ..Default::default()
                },
            )
            .expect("role set");
    }
    let formula = project
        .tree()
        .formula_from_letters(y2, "A*2+B")
        .expect("formula entered");
    let calculated = add_column(&mut project, sheet, "Calculee", &[0.0, 0.0, 0.0]);
    project
        .set_column_meta(
            calculated,
            ColumnMeta {
                formula: Some(formula),
                unit: "u".into(),
                long_name: "Calculée".into(),
                comment: "par formule".into(),
                display_format: "0.00".into(),
                x_prime: Some(-1.5),
                file_metadata: vec![
                    MetaLine {
                        key: "[Section]".into(),
                        value: String::new(),
                    },
                    MetaLine {
                        key: "XUNITS".into(),
                        value: "NANOMETERS".into(),
                    },
                    MetaLine {
                        key: String::new(),
                        value: "ligne libre".into(),
                    },
                ],
                import_source: Some(ImportSource {
                    path: "/data/mesure.asc".into(),
                    recipe_json: recipe_non_default().to_json(),
                    column: 3,
                    block: 2,
                }),
                ..Default::default()
            },
        )
        .expect("metadata set");

    // Graphs: the non-trivial spec, and a graph with each series type.
    project
        .add_graph(root, Some("Fig1"), non_trivial_spec(x, y1, y2))
        .expect("graph");
    let series = [
        PlotType::Scatter,
        PlotType::Line,
        PlotType::LineScatter,
        PlotType::Bar,
        PlotType::Histogram,
        PlotType::ErrorBar,
    ]
    .into_iter()
    .map(|plot_type| SeriesSpec {
        plot_type,
        ..SeriesSpec::new_xy(x, y1)
    })
    .collect::<Vec<_>>();
    let all_types = PlotSpec::empty("Tous les types").with_panels(vec![PanelSpec {
        series,
        ..PanelSpec::at(0, 0)
    }]);
    project
        .add_graph(root, Some("Fig2"), all_types)
        .expect("graph");

    // An active calculation, then a correction that replaces its data with a
    // frozen reference (hidden workbook, trace, original).
    let calculation_book = project
        .add_workbook(root, Some("Calculs"))
        .expect("workbook");
    let calculation_sheet = project.tree().children(calculation_book).expect("sheet")[0];
    let cx = add_column(&mut project, calculation_sheet, "X", &[1.0, 2.0, 3.0]);
    let cy = add_column(&mut project, calculation_sheet, "Y", &[10.0, 20.0, 30.0]);
    let rx = add_column(&mut project, calculation_sheet, "RX", &[1.0, 2.0, 3.0]);
    let ry = add_column(&mut project, calculation_sheet, "RY", &[2.0, 2.0, 2.0]);
    project
        .add_calculation(CalculationDraft {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: vec![SeriesRef {
                x: Some(cx),
                y: cy,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![9.0, 18.0, f64::NAN],
            }],
            results: vec![SeriesResult {
                values: vec![
                    ResultValue {
                        key: "count.outside".into(),
                        index: None,
                        value: 1.0,
                        error: None,
                    },
                    ResultValue {
                        key: "param.a".into(),
                        index: Some(2),
                        value: 0.25,
                        error: Some(0.5),
                    },
                ],
                skipped: 3,
                warnings: vec![Warning::NegativeClipped(4), Warning::IrfStepMismatch],
            }],
            keep_results: true,
            auto_recalc: true,
            low_r2_dialog: false,
        })
        .expect("calculation");
    let replaced = versions(&project, &[rx, ry, cx, cy]);
    project
        .replace_analysis_data(
            CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Divide,
                },
                series: vec![SeriesRef {
                    x: Some(rx),
                    y: ry,
                    sigma: None,
                }],
                aux: Some(AuxRef { x: Some(cx), y: cy }),
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![5.0, 10.0, 15.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: false,
                auto_recalc: false,
                low_r2_dialog: true,
            },
            &replaced,
        )
        .expect("correction");

    // A linked transposed sheet.
    let source_book = project
        .add_workbook(root, Some("Spectres"))
        .expect("workbook");
    let source = project.tree().children(source_book).expect("sheet")[0];
    for i in 0..3 {
        let mut column = Column::from_f64(&[1.0 + f64::from(i), 5.0]);
        column.set_meta(ColumnMeta {
            x_prime: Some(f64::from(i) * 10.0),
            ..ColumnMeta::default()
        });
        project
            .add_column(source, None, Some(&format!("C{i}")), column)
            .expect("column");
    }
    project.transpose_rows(source, &[0, 1]).expect("transposed");

    project.set_saved_colors(
        SavedColors::default()
            .with_added(ColorUsage::Curves, "#ff0000")
            .with_added(ColorUsage::Text, "#20202080")
            .with_added(ColorUsage::Background, "#ffffff00")
            .with_gradient_added("Chaud", &["#000000".to_string(), "#ff0000".to_string()]),
    );

    // View tables: more than two windows, so that the order matters.
    let mut window_geometries = HashMap::new();
    let mut column_widths = HashMap::new();
    for (rank, id) in project.tree().walk_all().into_iter().take(20).enumerate() {
        window_geometries.insert(
            id,
            WindowGeometry {
                x: rank as f64 * 3.5,
                y: rank as f64 + 0.25,
                width: 420.0,
                height: 310.5,
                view: Some(WindowView {
                    mode: [WindowMode::Open, WindowMode::Maximized, WindowMode::Closed][rank % 3],
                    collapsed: rank % 2 == 0,
                    stack: rank as u32,
                }),
            },
        );
        column_widths.insert(id, 40.0 + rank as f32 * 1.5);
    }
    (
        project,
        ViewTables {
            window_geometries,
            column_widths,
        },
    )
}

fn witness_project_bytes() -> Vec<u8> {
    let (project, views) = witness_project();
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("project.stf");
    save_with_geometries(&project, &views, &path).expect("save");
    std::fs::read(&path).expect("read")
}

#[test]
fn the_v1_witness_reads_back_and_gives_the_expected_project() {
    let (expected, expected_views) = witness_project();
    let (read_back, views, report) =
        load_with_report(&v1_file()).expect("the V1 witness reads back");
    assert!(report.is_clean(), "{report:?}");
    assert!(
        *read_back.tree() == *expected.tree(),
        "the tree read back differs from the expected project"
    );
    assert_eq!(read_back.next_id_hint(), expected.next_id_hint());
    assert_eq!(views.window_geometries, expected_views.window_geometries);
    assert_eq!(views.column_widths, expected_views.column_widths);
}

#[test]
fn the_written_format_does_not_drift_without_a_version_change() {
    let file = std::fs::read(v1_file()).expect("witness present");
    assert!(
        witness_project_bytes() == file,
        "the written format changed: if intended, bump the version number \
         (removal, rename, change of meaning) or regenerate the witness \
         (addition), see docs/persistance.md, « Version rule »"
    );
}

#[test]
#[ignore = "explicit gesture: regenerates the witness of the current format"]
fn regenerate() {
    assert!(
        std::env::var_os("STOKESFORGE_REGENERATE_WITNESSES").is_some(),
        "set STOKESFORGE_REGENERATE_WITNESSES=1"
    );
    let path = v1_file();
    std::fs::create_dir_all(path.parent().expect("folder")).expect("creation");
    std::fs::write(&path, witness_project_bytes()).expect("write");
}

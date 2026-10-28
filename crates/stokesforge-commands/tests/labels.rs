//! Structured history labels (issue #46): each variant of
//! `Command` produces its `HistoryLabel`, and the English `Display` says the
//! same thing as the application catalog.

use std::collections::BTreeSet;

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::{Operation, OperationKind};
use stokesforge_commands::{ColumnRef, Command, HistoryLabel, MacroKind, PlotEditKind};
use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, ColumnType, NodeId, Project, SeriesRef, ValueBlock,
    WindowMeta,
};
use stokesforge_plot::{PlotSpec, SavedColors};

struct World {
    project: Project,
    book: NodeId,
    sheet: NodeId,
    first: NodeId,
    third: NodeId,
}

/// `Book1` / `Sheet1` with three columns named `Time`, `Signal`, `Noise`:
/// names that are **not** the grid letters, so that the test
/// tells the position from the node name.
fn world() -> World {
    let mut project = Project::new("Demo").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet");
    let mut columns = Vec::new();
    for name in ["Time", "Signal", "Noise"] {
        columns.push(
            project
                .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0]))
                .expect("column"),
        );
    }
    World {
        project,
        book,
        sheet,
        first: columns[0],
        third: columns[2],
    }
}

/// The rank of each variant: the `match` is **exhaustive** (no default
/// branch), so a variant added to `Command` does not compile here until
/// it has its rank — and the coverage test then demands an example.
fn rank(command: &Command) -> usize {
    match command {
        Command::SetCells { .. } => 0,
        Command::SetCellsPromoting { .. } => 1,
        Command::AddColumn { .. } => 2,
        Command::DeleteColumn { .. } => 3,
        Command::Rename { .. } => 4,
        Command::Move { .. } => 5,
        Command::SetColumnType { .. } => 6,
        Command::InsertRows { .. } => 7,
        Command::DeleteRows { .. } => 8,
        Command::DeleteCells { .. } => 9,
        Command::AddFolder { .. } => 10,
        Command::AddWorkbook { .. } => 11,
        Command::AddEmptyWorkbook { .. } => 12,
        Command::AddWorksheet { .. } => 13,
        Command::AddGraph { .. } => 14,
        Command::AddMatrix { .. } => 15,
        Command::AddNote { .. } => 16,
        Command::RemoveNode { .. } => 17,
        Command::SetWindowMeta { .. } => 18,
        Command::SetNoteText { .. } => 19,
        Command::SetColumnMeta { .. } => 20,
        Command::SetPlotSpec { .. } => 21,
        Command::SetSavedColors { .. } => 22,
        Command::EditPlotSpec { .. } => 23,
        Command::DuplicateWorksheet { .. } => 24,
        Command::InsertCopy { .. } => 25,
        Command::AddCalculation { .. } => 26,
        Command::UpdateCalculation { .. } => 27,
        Command::RemoveCalculation { .. } => 28,
        Command::ReplaceAnalysisData { .. } => 29,
        Command::RestoreOriginalData { .. } => 30,
        Command::TransposeRows { .. } => 31,
        Command::RecalculateTranspose { .. } => 32,
    }
}

const VARIANTS: usize = 33;

fn named(name: &str) -> Option<String> {
    Some(name.to_string())
}

/// One command of each variant, with the expected label.
fn samples(w: &World) -> Vec<(Command, HistoryLabel)> {
    let root = w.project.root();
    vec![
        (
            Command::ReplaceAnalysisData {
                draft: calc_draft(w),
                expected_versions: Vec::new(),
            },
            HistoryLabel::ReplaceAnalysisData(OperationKind::Correct),
        ),
        (
            Command::InsertCopy {
                parent: root,
                copy: stokesforge_core::NodeCopy::capture(w.project.tree(), w.book).unwrap(),
                name: "Copy".into(),
            },
            HistoryLabel::Macro(MacroKind::PasteObjects),
        ),
        (
            Command::SetCells {
                column: w.third,
                start: 0,
                values: ValueBlock::Float(vec![1.0]),
                expected_version: None,
            },
            HistoryLabel::SetCells {
                column: Some(ColumnRef(2)),
            },
        ),
        (
            Command::SetCellsPromoting {
                column: w.first,
                start: 0,
                values: ValueBlock::Float(vec![1.0]),
                expected_version: None,
            },
            HistoryLabel::SetCells {
                column: Some(ColumnRef(0)),
            },
        ),
        (
            Command::AddColumn {
                sheet: w.sheet,
                index: None,
                name: None,
                column: Column::new(ColumnType::Numeric),
            },
            HistoryLabel::AddColumn,
        ),
        (
            Command::DeleteColumn { column: w.third },
            HistoryLabel::DeleteColumn {
                column: Some(ColumnRef(2)),
            },
        ),
        (
            Command::Rename {
                id: w.book,
                name: "Measures".to_string(),
            },
            HistoryLabel::Rename {
                from: named("Book1"),
                to: "Measures".to_string(),
            },
        ),
        (
            Command::Move {
                id: w.first,
                new_parent: w.sheet,
                index: 1,
            },
            HistoryLabel::Move {
                name: named("Time"),
            },
        ),
        (
            Command::SetColumnType {
                column: w.third,
                column_type: ColumnType::Mixed,
            },
            HistoryLabel::SetColumnType {
                column: Some(ColumnRef(2)),
            },
        ),
        (
            Command::InsertRows {
                sheet: w.sheet,
                at: 0,
                count: 3,
            },
            HistoryLabel::InsertRows { count: 3 },
        ),
        (
            Command::DeleteRows {
                sheet: w.sheet,
                at: 0,
                count: 1,
            },
            HistoryLabel::DeleteRows { count: 1 },
        ),
        (
            Command::DeleteCells {
                columns: vec![w.first],
                at: 0,
                count: 2,
            },
            HistoryLabel::DeleteCells { count: 2 },
        ),
        (
            Command::AddFolder {
                parent: root,
                name: "Experiments".to_string(),
            },
            HistoryLabel::AddFolder {
                name: "Experiments".to_string(),
            },
        ),
        (
            Command::AddWorkbook {
                parent: root,
                name: None,
            },
            HistoryLabel::AddWorkbook,
        ),
        (
            Command::AddEmptyWorkbook {
                parent: root,
                name: None,
            },
            HistoryLabel::AddEmptyWorkbook,
        ),
        (
            Command::AddWorksheet {
                book: w.book,
                name: None,
            },
            HistoryLabel::AddWorksheet,
        ),
        (
            Command::DuplicateWorksheet {
                sheet: w.sheet,
                name: "Sheet1 copy 1".to_string(),
            },
            HistoryLabel::DuplicateWorksheet {
                name: named("Sheet1"),
            },
        ),
        (
            Command::AddGraph {
                parent: root,
                name: None,
                spec: PlotSpec::default(),
            },
            HistoryLabel::AddGraph,
        ),
        (
            Command::AddMatrix {
                parent: root,
                name: None,
            },
            HistoryLabel::AddMatrix,
        ),
        (
            Command::AddNote {
                parent: root,
                name: None,
                text: String::new(),
            },
            HistoryLabel::AddNote,
        ),
        (
            Command::RemoveNode { id: w.sheet },
            HistoryLabel::RemoveNode {
                name: named("Sheet1"),
            },
        ),
        (
            Command::SetWindowMeta {
                id: w.book,
                meta: WindowMeta::default(),
            },
            HistoryLabel::SetWindowMeta {
                name: named("Book1"),
            },
        ),
        (
            Command::SetNoteText {
                id: w.book,
                text: String::new(),
            },
            HistoryLabel::SetNoteText {
                name: named("Book1"),
            },
        ),
        (
            Command::SetColumnMeta {
                column: w.first,
                meta: ColumnMeta::default(),
            },
            HistoryLabel::SetColumnMeta {
                column: Some(ColumnRef(0)),
            },
        ),
        (
            Command::SetPlotSpec {
                graph: w.book,
                spec: PlotSpec::default(),
            },
            HistoryLabel::SetPlotSpec {
                name: named("Book1"),
            },
        ),
        (
            Command::SetSavedColors {
                colors: SavedColors::default(),
            },
            HistoryLabel::SetSavedColors,
        ),
        (
            Command::TransposeRows {
                source: w.sheet,
                rows: vec![0],
            },
            HistoryLabel::TransposeRows,
        ),
        (
            Command::RecalculateTranspose { sheet: w.sheet },
            HistoryLabel::RecalculateTranspose,
        ),
        (
            Command::AddCalculation {
                draft: calc_draft(w),
            },
            HistoryLabel::AddCalculation(OperationKind::Correct),
        ),
        (
            // Trace unknown to the tree: no family to name.
            Command::RestoreOriginalData {
                trace: NodeId::from_raw(9_999),
            },
            HistoryLabel::RestoreOriginalData(None),
        ),
        (
            // Calculation unknown to the tree: the family is read from the draft.
            Command::UpdateCalculation {
                id: NodeId::from_raw(9_999),
                draft: calc_draft(w),
            },
            HistoryLabel::UpdateCalculation(OperationKind::Correct),
        ),
        (
            Command::RemoveCalculation {
                id: NodeId::from_raw(9_999),
                with_columns: true,
            },
            HistoryLabel::RemoveCalculation {
                kind: None,
                with_columns: true,
            },
        ),
    ]
}

fn calc_draft(w: &World) -> CalculationDraft {
    CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![SeriesRef {
            x: Some(w.first),
            y: w.third,
            sigma: None,
        }],
        aux: None,
        outputs: vec![],
        results: vec![],
        keep_results: false,
        auto_recalc: true,
        low_r2_dialog: true,
    }
}

/// The English `Display` of the calculation labels (logs, CLI).
#[test]
fn calculation_labels_display_in_english() {
    assert_eq!(
        HistoryLabel::AddCalculation(OperationKind::Baseline).to_string(),
        "Add Correction Calculation"
    );
    assert_eq!(
        HistoryLabel::UpdateCalculation(OperationKind::Peaks).to_string(),
        "Edit Peak Decomposition Calculation"
    );
    let remove = |kind, with_columns| HistoryLabel::RemoveCalculation { kind, with_columns };
    assert_eq!(
        remove(Some(OperationKind::Fit), true).to_string(),
        "Delete Fit Calculation"
    );
    assert_eq!(
        remove(Some(OperationKind::Fit), false).to_string(),
        "Detach Fit Calculation"
    );
    assert_eq!(remove(None, false).to_string(), "Detach Calculation");
    assert_eq!(
        HistoryLabel::RestoreOriginalData(Some(OperationKind::Baseline)).to_string(),
        "Restore Original Data (Correction)"
    );
    assert_eq!(
        HistoryLabel::RestoreOriginalData(None).to_string(),
        "Restore Original Data"
    );
    for kind in OperationKind::ALL {
        assert!(!HistoryLabel::AddCalculation(kind).to_string().is_empty());
    }
}

/// Each `Command` variant produces its label — and the examples cover
/// **all** the variants (the witness: without it, a forgotten example would
/// pass for an exhaustive test).
#[test]
fn every_command_variant_produces_its_label() {
    let w = world();
    let mut samples = samples(&w);
    samples.extend(PlotEditKind::ALL.map(|kind| {
        (
            Command::EditPlotSpec {
                graph: w.book,
                spec: PlotSpec::default(),
                kind,
            },
            HistoryLabel::EditPlotSpec(kind),
        )
    }));
    let covered: BTreeSet<usize> = samples.iter().map(|(command, _)| rank(command)).collect();
    assert_eq!(
        covered,
        (0..VARIANTS).collect::<BTreeSet<_>>(),
        "a Command variant has no example"
    );
    for (command, expected) in samples {
        assert_eq!(
            command.label(w.project.tree()),
            expected,
            "label of {command:?}"
        );
    }
}

/// A column is designated by its position, never by its node name: the
/// label of `Time` (first column) carries `ColumnRef(0)`, whose letter
/// is that of the grid header.
#[test]
fn a_column_is_designated_by_its_position_not_its_node_name() {
    let w = world();
    let label = Command::DeleteColumn { column: w.third }.label(w.project.tree());
    assert_eq!(label.to_string(), "Delete Column C");
    assert!(!label.to_string().contains("Noise"));
}

/// A node absent from the tree gives the generic form, without failing: it is
/// applying the command that will fail, with the right error.
#[test]
fn a_missing_node_gives_the_generic_label() {
    let mut w = world();
    let ghost = w.first;
    w.project.remove(ghost).expect("deletion");
    let tree = w.project.tree();
    let cases = [
        (
            Command::SetCells {
                column: ghost,
                start: 0,
                values: ValueBlock::Float(vec![1.0]),
                expected_version: None,
            },
            "Edit Cells",
        ),
        (Command::DeleteColumn { column: ghost }, "Delete Column"),
        (
            Command::Rename {
                id: ghost,
                name: "Z".to_string(),
            },
            "Rename to “Z”",
        ),
        (Command::RemoveNode { id: ghost }, "Delete"),
        (
            Command::SetColumnMeta {
                column: ghost,
                meta: ColumnMeta::default(),
            },
            "Edit Column Properties",
        ),
    ];
    for (command, expected) in cases {
        assert_eq!(command.label(tree).to_string(), expected, "{command:?}");
    }
}

/// The English plural of the `Display`: only 1 is singular. The same case is
/// held on the interface side by the catalog (`stokesforge-app-state/tests/history_text.rs`).
#[test]
fn the_english_display_pluralizes_rows() {
    let text = |count| HistoryLabel::InsertRows { count }.to_string();
    assert_eq!(text(1), "Insert 1 Row");
    assert_eq!(text(3), "Insert 3 Rows");
    assert_eq!(text(0), "Insert 0 Rows");
    assert_eq!(
        HistoryLabel::DeleteCells { count: 1 }.to_string(),
        "Delete Cells and Shift Up (1 Row)"
    );
}

/// A label is comparable: the application no longer has to read a text to know
/// whether the entry on top is an import.
#[test]
fn macro_labels_compare_by_kind() {
    assert_eq!(
        HistoryLabel::Macro(MacroKind::Import),
        HistoryLabel::Macro(MacroKind::Import)
    );
    assert_ne!(
        HistoryLabel::Macro(MacroKind::Import),
        HistoryLabel::Macro(MacroKind::PasteCells)
    );
    for kind in MacroKind::ALL {
        assert!(!HistoryLabel::Macro(kind).to_string().is_empty());
    }
}

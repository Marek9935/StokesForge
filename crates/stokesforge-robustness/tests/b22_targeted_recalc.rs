//! Battery 22 — targeted recalculation of formula columns (#193): **the
//! targeted pass returns exactly what the full pass returns**, after every
//! step of a randomly drawn sequence.
//!
//! Since #193, the actor recalculates a formula column only if a piece of data
//! of its computation has changed (`stokesforge-session/src/recalc.rs`). The
//! risk of this kind of optimization is unique and silent: a forgotten
//! invalidation leaves a wrong value **without an error**. The safety net is an
//! oracle, the full recalculation from before #193, kept as is in the same code
//! ([`FormulaRecalc::Full`]): two twin sessions, one targeted, the other full,
//! receive the same requests, and their trees must stay equal at every step
//! (column equality ignores the version counters, which necessarily diverge:
//! the targeted one writes less).
//!
//! Three properties, checked after every step:
//!
//! 1. **oracle** — same replies (success or refusal) and same trees in both
//!    sessions;
//! 2. **version ↔ content**, in each session — the assumption on which the
//!    whole targeted recalculation depends: a version number never designates
//!    two contents of the same column. `b14` checks it on `History` alone;
//!    this one checks it in the actor, recalculation writes and macros
//!    included. Its scope is one in-memory instance: a `Load` restarts from the
//!    versions of the file (`Project::from_tree`), so the table restarts from
//!    zero on every load — this is also why the actor empties its memo on
//!    load;
//! 3. **targeted** — an unrelated command (one more folder) does not change
//!    the version of any column of the targeted session. Without this
//!    property, a "targeted" pass that recalculated everything would pass the
//!    first one without flinching: this is the witness of the battery.
//!
//! The draw covers what can change a piece of data of a formula: entries
//! (numbers, texts that promote to mixed), formulas set, changed, unreadable,
//! self-referencing, in a cycle, removed; columns added (including with an
//! already carried version, or cloned from a snapshot), deleted, moved within
//! the worksheet or to another one; types changed; rows inserted, deleted;
//! cells deleted (including in a formula column); worksheets duplicated,
//! pasted, deleted; workbooks pasted; undo, redo, and series of undos followed
//! by as many redos (the redo of a structural entry is the only path that does
//! not restamp the versions); macros closed or aborted; save and reload,
//! including of an older state. A final check requires every kind of step to
//! have launched at least twenty passes: a gesture that is always refused
//! would prove nothing.
//!
//! **A feature that adds a piece of data to the computation of a formula must
//! add here the step that makes it change** — see `docs/tests.md`, "Targeted
//! recalculation". Without that step, the oracle cannot see the omission.
//!
//! The draw is **deterministic** (`TestRng::deterministic_rng`), like that of
//! `b14`: a failure reproduces identically and no regression file is written.
//! To stay so, nothing here picks a target in the order of `Tree::iter` (a
//! hash table with a random seed): worksheets are taken in the order of the
//! walk from the root.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_commands::{Command, MacroKind};
use stokesforge_core::{
    compute, AuxRef, Calculation, CalculationDraft, Cell, Column, ColumnMeta, ColumnType,
    ColumnValues, MixedCell, NodeCopy, NodeId, OutputDraft, Project, SeriesRef, Tree, ValueBlock,
};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};
use tempfile::TempDir;

// -- draw -----------------------------------------------------------------

/// Formulas drawn, **in letters** (#321): references inside and outside the
/// worksheet (up to `G`), self-references and cycles depending on the column
/// that receives them, division (zeros are entered), pure constant, unreadable
/// text. The step converts them to identifiers (`Tree::formula_from_letters`)
/// as the interface does; a refusal (letter outside the worksheet,
/// self-reference) gives a step without a command, counted as "rejected".
const FORMULAS: [&str; 15] = [
    "A*2",
    "B+1",
    "A+B",
    "C-A",
    "D/2",
    "A*",
    "F+1",
    "1+2",
    "A+B*C",
    "E",
    "(A+B)/D",
    "G-1",
    "B*B",
    // Row-wise functions and conditions (#401): they read their columns.
    "sqrt(A)+B^2",
    "if(A>B,A,B)",
];

#[derive(Debug, Clone)]
enum Step {
    /// Final correction of an ordinary column (#212).
    ReplaceCorrection {
        sheet: usize,
        column: usize,
    },
    Enter {
        sheet: usize,
        column: usize,
        start_row: usize,
        count: usize,
        seed: usize,
    },
    EnterText {
        sheet: usize,
        column: usize,
        start_row: usize,
    },
    SetFormula {
        sheet: usize,
        column: usize,
        formula_index: usize,
    },
    RemoveFormula {
        sheet: usize,
        column: usize,
    },
    /// Non-zero `version`: a column coming from elsewhere (file, another
    /// project) that already carries a number, possibly beyond the counter.
    AddColumn {
        sheet: usize,
        index: usize,
        length: usize,
        version: u64,
    },
    /// Adds, through `AddColumn`, the clone of a column of the snapshot —
    /// version included, as a client that keeps a column read before a load
    /// would do.
    AddClone {
        sheet: usize,
        column: usize,
        target_sheet: usize,
    },
    DeleteColumn {
        sheet: usize,
        column: usize,
    },
    MoveColumn {
        sheet: usize,
        column: usize,
        target_sheet: usize,
        index: usize,
    },
    ChangeType {
        sheet: usize,
        column: usize,
        type_index: usize,
    },
    InsertRows {
        sheet: usize,
        at: usize,
        count: usize,
    },
    DeleteRows {
        sheet: usize,
        at: usize,
        count: usize,
    },
    DeleteCells {
        sheet: usize,
        column: usize,
        at: usize,
        count: usize,
    },
    DuplicateWorksheet {
        sheet: usize,
    },
    /// Copy-paste (`InsertCopy`) of a worksheet into its workbook.
    PasteWorksheet {
        sheet: usize,
    },
    /// Copy-paste (`InsertCopy`) of a worksheet's workbook at the root.
    PasteWorkbook {
        sheet: usize,
    },
    DeleteWorksheet {
        sheet: usize,
    },
    /// An analysis calculation (#181) on two columns of the worksheet: a
    /// linear fit, or a correction by two other columns.
    AddCalculation {
        sheet: usize,
        x: usize,
        y: usize,
        correction: bool,
        automatic: bool,
    },
    /// Removes the `rank`-th calculation (by identifier), columns included or not.
    RemoveCalculation {
        rank: usize,
        with_columns: bool,
    },
    /// "Recalculate Out of Date": reruns the stale calculations.
    RecalculateOutOfDate,
    Undo,
    Redo,
    /// `k` undos then `k` redos: without it, the redo of a structural entry
    /// (the only path that does not restamp the versions) almost never
    /// happens.
    UndoThenRedo {
        k: usize,
    },
    OpenMacro,
    CloseMacro,
    AbortMacro,
    /// Saves then reloads the current state.
    Reload,
    /// Reloads a state saved earlier in the sequence.
    RevertTo {
        save_index: usize,
    },
    Unrelated,
}

fn step() -> impl Strategy<Value = Step> {
    let f = 0usize..4;
    let c = 0usize..8;
    prop_oneof![
        6 => (f.clone(), c.clone(), 0usize..6, 1usize..4, 0usize..50).prop_map(
            |(sheet, column, start_row, count, seed)| Step::Enter {
                sheet,
                column,
                start_row,
                count,
                seed
            }
        ),
        1 => (f.clone(), c.clone(), 0usize..5).prop_map(|(sheet, column, start_row)| {
            Step::EnterText {
                sheet,
                column,
                start_row,
            }
        }),
        6 => (f.clone(), c.clone(), 0..FORMULAS.len()).prop_map(|(sheet, column, formula_index)| {
            Step::SetFormula {
                sheet,
                column,
                formula_index,
            }
        }),
        1 => (f.clone(), c.clone())
            .prop_map(|(sheet, column)| Step::RemoveFormula { sheet, column }),
        2 => (
            f.clone(),
            0usize..8,
            0usize..7,
            prop_oneof![Just(0u64), 1u64..120]
        )
            .prop_map(|(sheet, index, length, version)| Step::AddColumn {
                sheet,
                index,
                length,
                version,
            }),
        1 => (f.clone(), c.clone(), f.clone()).prop_map(|(sheet, column, target_sheet)| {
            Step::AddClone {
                sheet,
                column,
                target_sheet,
            }
        }),
        1 => (f.clone(), c.clone())
            .prop_map(|(sheet, column)| Step::DeleteColumn { sheet, column }),
        3 => (f.clone(), c.clone(), f.clone(), 0usize..8).prop_map(
            |(sheet, column, target_sheet, index)| Step::MoveColumn {
                sheet,
                column,
                target_sheet,
                index
            }
        ),
        1 => (f.clone(), c.clone(), 0usize..3).prop_map(|(sheet, column, type_index)| {
            Step::ChangeType {
                sheet,
                column,
                type_index,
            }
        }),
        2 => (f.clone(), 0usize..6, 1usize..3).prop_map(|(sheet, at, count)| {
            Step::InsertRows {
                sheet,
                at,
                count,
            }
        }),
        2 => (f.clone(), 0usize..6, 1usize..3).prop_map(|(sheet, at, count)| {
            Step::DeleteRows {
                sheet,
                at,
                count,
            }
        }),
        1 => (f.clone(), c, 0usize..5, 1usize..3).prop_map(|(sheet, column, at, count)| {
            Step::DeleteCells {
                sheet,
                column,
                at,
                count,
            }
        }),
        1 => f.clone().prop_map(|sheet| Step::DuplicateWorksheet { sheet }),
        1 => f.clone().prop_map(|sheet| Step::PasteWorksheet { sheet }),
        1 => f.clone().prop_map(|sheet| Step::PasteWorkbook { sheet }),
        1 => f.clone().prop_map(|sheet| Step::DeleteWorksheet { sheet }),
        3 => (f.clone(), 0usize..8).prop_map(|(sheet, column)| Step::ReplaceCorrection { sheet, column }),
        4 => (f, 0usize..8, 0usize..8, any::<bool>(), any::<bool>()).prop_map(
            |(sheet, x, y, correction, automatic)| Step::AddCalculation {
                sheet,
                x,
                y,
                correction,
                automatic,
            }
        ),
        1 => (0usize..4, any::<bool>())
            .prop_map(|(rank, with_columns)| Step::RemoveCalculation { rank, with_columns }),
        2 => Just(Step::RecalculateOutOfDate),
        4 => Just(Step::Undo),
        2 => Just(Step::Redo),
        2 => (1usize..5).prop_map(|k| Step::UndoThenRedo { k }),
        1 => Just(Step::OpenMacro),
        1 => Just(Step::CloseMacro),
        1 => Just(Step::AbortMacro),
        1 => Just(Step::Reload),
        1 => (0usize..8).prop_map(|save_index| Step::RevertTo { save_index }),
        3 => Just(Step::Unrelated),
    ]
}

/// All the kinds of step, for the coverage check.
const STEP_NAMES: &[&str] = &[
    "ReplaceCorrection",
    "Enter",
    "EnterText",
    "SetFormula",
    "RemoveFormula",
    "AddColumn",
    "AddClone",
    "DeleteColumn",
    "MoveColumn",
    "ChangeType",
    "InsertRows",
    "DeleteRows",
    "DeleteCells",
    "DuplicateWorksheet",
    "PasteWorksheet",
    "PasteWorkbook",
    "DeleteWorksheet",
    "AddCalculation",
    "RemoveCalculation",
    "RecalculateOutOfDate",
    "Undo",
    "Redo",
    "UndoThenRedo",
    "OpenMacro",
    "CloseMacro",
    "AbortMacro",
    "Reload",
    "RevertTo",
    "Unrelated",
];

/// Name of the kind of a step (an entry of [`STEP_NAMES`]).
fn step_name(step: &Step) -> &'static str {
    match step {
        Step::ReplaceCorrection { .. } => "ReplaceCorrection",
        Step::Enter { .. } => "Enter",
        Step::EnterText { .. } => "EnterText",
        Step::SetFormula { .. } => "SetFormula",
        Step::RemoveFormula { .. } => "RemoveFormula",
        Step::AddColumn { .. } => "AddColumn",
        Step::AddClone { .. } => "AddClone",
        Step::DeleteColumn { .. } => "DeleteColumn",
        Step::MoveColumn { .. } => "MoveColumn",
        Step::ChangeType { .. } => "ChangeType",
        Step::InsertRows { .. } => "InsertRows",
        Step::DeleteRows { .. } => "DeleteRows",
        Step::DeleteCells { .. } => "DeleteCells",
        Step::DuplicateWorksheet { .. } => "DuplicateWorksheet",
        Step::PasteWorksheet { .. } => "PasteWorksheet",
        Step::PasteWorkbook { .. } => "PasteWorkbook",
        Step::DeleteWorksheet { .. } => "DeleteWorksheet",
        Step::AddCalculation { .. } => "AddCalculation",
        Step::RemoveCalculation { .. } => "RemoveCalculation",
        Step::RecalculateOutOfDate => "RecalculateOutOfDate",
        Step::Undo => "Undo",
        Step::Redo => "Redo",
        Step::UndoThenRedo { .. } => "UndoThenRedo",
        Step::OpenMacro => "OpenMacro",
        Step::CloseMacro => "CloseMacro",
        Step::AbortMacro => "AbortMacro",
        Step::Reload => "Reload",
        Step::RevertTo { .. } => "RevertTo",
        Step::Unrelated => "Unrelated",
    }
}

/// Runner with a fixed seed, without a regression file on the repository disk.
fn runner(cases: u32) -> TestRunner {
    TestRunner::new_with_rng(
        Config {
            cases,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    )
}

// -- the world played -----------------------------------------------------

/// `Book1` with two worksheets of different lengths, already equipped with
/// formulas (including a chain and a constant), so that the first steps have
/// something to recalculate.
fn initial_project() -> Project {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let s1 = project.resolve_path("[Book1]Sheet1").expect("worksheet 1");
    let s2 = project.add_worksheet(book, None).expect("worksheet 2");
    let columns_spec: [(NodeId, &[f64], Option<&str>); 7] = [
        (s1, &[1.0, 2.0, 3.0], None),
        (s1, &[10.0, 0.0, 30.0, 40.0], None),
        (s1, &[], Some("A*2")),
        (s1, &[], Some("C+B")),
        (s2, &[1.0, 2.0, 3.0, 4.0, 5.0], None),
        (s2, &[], Some("A/2")),
        (s2, &[], Some("1+2")),
    ];
    let mut with_formula = Vec::new();
    for (sheet, values, formula_text) in columns_spec {
        let id = project
            .add_column(sheet, None, None, Column::from_f64(values))
            .expect("column");
        if let Some(text) = formula_text {
            with_formula.push((id, text));
        }
    }
    // Formulas are entered in letters once the columns of their worksheet are
    // in place (#321): letters to identifiers.
    for (id, text) in with_formula {
        let formula = project
            .tree()
            .formula_from_letters(id, text)
            .expect("starting formula");
        project
            .set_column_meta(
                id,
                ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            )
            .expect("formula set");
    }
    project
}

struct Twin {
    /// Kept alive: dropping the `Session` would stop the actor.
    _session: Session,
    handle: SessionHandle,
    /// `(NodeId, version) → (type, values)`, for property 2. The metadata (the
    /// formula itself, the unit…) are not part of it: the version has never
    /// covered them, and the formula text has its own component in the
    /// recalculation fingerprint.
    seen: HashMap<(NodeId, u64), (ColumnType, ColumnValues)>,
}

impl Twin {
    fn new(mode: FormulaRecalc) -> Self {
        let session =
            Session::spawn_with_recalc(initial_project(), None, mode).expect("actor started");
        let handle = session.handle();
        Self {
            _session: session,
            handle,
            seen: HashMap::new(),
        }
    }

    fn snapshot(&self) -> Snapshot {
        self.handle.snapshot().expect("snapshot")
    }
}

/// The worksheets of the tree, in the order of the walk from the root —
/// never that of `Tree::iter`, backed by a hash table whose seed changes with
/// every process: the draw would no longer be replayable.
fn worksheets_of(tree: &Tree) -> Vec<NodeId> {
    tree.walk_all()
        .into_iter()
        .filter(|&id| tree.node(id).is_some_and(|node| node.kind().is_worksheet()))
        .collect()
}

fn pick<T: Copy>(liste: &[T], i: usize) -> Option<T> {
    (!liste.is_empty()).then(|| liste[i % liste.len()])
}

fn columns_of(tree: &Tree, sheet: Option<NodeId>) -> Vec<NodeId> {
    sheet
        .and_then(|f| tree.children(f).ok())
        .map(<[NodeId]>::to_vec)
        .unwrap_or_default()
}

/// Choice in worksheet order, without depending on the hash table. The
/// computation and the command stay the same as in the real clients.
fn correction_command(tree: &Tree, columns: &[NodeId], rank: usize) -> Option<Command> {
    let candidates: Vec<_> = columns
        .iter()
        .copied()
        .filter(|id| {
            tree.node(*id).and_then(|n| n.as_column()).is_some_and(|c| {
                c.column_type() == ColumnType::Numeric
                    && c.meta().formula.is_none()
                    && tree.calculation_of_column(*id).is_none()
            })
        })
        .collect();
    let y = *candidates.get(rank % candidates.len().max(1))?;
    let version = tree.node(y)?.as_column()?.version();
    let operation = Operation::Baseline {
        method: BaselineMethod::Constant { point: (0.0, 1.5) },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let series = vec![SeriesRef {
        x: None,
        y,
        sigma: None,
    }];
    let result = compute(tree, &operation, &series, None).ok()?;
    Some(Command::ReplaceAnalysisData {
        draft: CalculationDraft {
            operation,
            series,
            aux: None,
            outputs: result
                .columns
                .into_iter()
                .map(|c| OutputDraft {
                    series: c.series,
                    key: c.key,
                    index: c.index,
                    meta: ColumnMeta::default(),
                    data: c.data,
                })
                .collect(),
            results: result.results,
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: true,
        },
        expected_versions: vec![(y, version)],
    })
}

/// The versions of all the columns of the tree.
fn versions(tree: &Tree) -> HashMap<NodeId, u64> {
    tree.iter()
        .filter_map(|(id, node)| node.as_column().map(|c| (id, c.version())))
        .collect()
}

/// Translates a step into a command, on the (shared) tree of the two twins.
/// `None`: the step is not a command, or has no target.
fn command(step: &Step, tree: &Tree, rank: usize) -> Option<Command> {
    let sheets = worksheets_of(tree);
    let sheet_at = |i: usize| pick(&sheets, i);
    let column_at = |f: usize, c: usize| pick(&columns_of(tree, sheet_at(f)), c);
    match *step {
        Step::ReplaceCorrection {
            sheet: f,
            column: c,
        } => correction_command(tree, &columns_of(tree, sheet_at(f)), c),
        Step::Enter {
            sheet: f,
            column: c,
            start_row,
            count,
            seed,
        } => column_at(f, c).map(|column| Command::SetCells {
            column,
            start: start_row,
            // Zeros and negatives: division and sign matter.
            values: ValueBlock::Float(
                (0..count)
                    .map(|k| ((seed * 7 + k * 3) % 11) as f64 - 3.0)
                    .collect(),
            ),
            expected_version: None,
        }),
        Step::EnterText {
            sheet: f,
            column: c,
            start_row,
        } => column_at(f, c).map(|column| Command::SetCellsPromoting {
            column,
            start: start_row,
            values: ValueBlock::Mixed(vec![MixedCell::Text("x".into())]),
            expected_version: None,
        }),
        Step::SetFormula {
            sheet: f,
            column: c,
            formula_index,
        } => column_at(f, c).and_then(|column| {
            // Letters to identifiers on the snapshot (the two twins carry the
            // same identifiers). A refusal — letter outside the worksheet,
            // column that reads itself — gives `None`: the step is not played,
            // like any step without a target (#321).
            let formula = tree
                .formula_from_letters(column, FORMULAS[formula_index])
                .ok()?;
            Some(Command::SetColumnMeta {
                column,
                meta: ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            })
        }),
        Step::RemoveFormula {
            sheet: f,
            column: c,
        } => column_at(f, c).map(|column| Command::SetColumnMeta {
            column,
            meta: ColumnMeta::default(),
        }),
        Step::AddColumn {
            sheet: f,
            index,
            length,
            version,
        } => sheet_at(f).and_then(|sheet| {
            Some(Command::AddColumn {
                sheet,
                index: Some(index % (columns_of(tree, Some(sheet)).len() + 1)),
                name: None,
                column: Column::from_block_with_version(
                    ColumnType::Numeric,
                    ValueBlock::Float((0..length).map(|k| k as f64 + 0.5).collect()),
                    ColumnMeta::default(),
                    version,
                )
                .ok()?,
            })
        }),
        Step::AddClone {
            sheet: f,
            column: c,
            target_sheet,
        } => column_at(f, c)
            .zip(sheet_at(target_sheet))
            .and_then(|(id, sheet)| {
                let clone = tree.node(id)?.as_column()?.clone();
                Some(Command::AddColumn {
                    sheet,
                    index: None,
                    name: None,
                    column: clone,
                })
            }),
        Step::DeleteColumn {
            sheet: f,
            column: c,
        } => column_at(f, c).map(|column| Command::DeleteColumn { column }),
        Step::MoveColumn {
            sheet: f,
            column: c,
            target_sheet,
            index,
        } => column_at(f, c)
            .zip(sheet_at(target_sheet))
            .map(|(id, new_parent)| Command::Move {
                id,
                new_parent,
                // An index beyond the worksheet is refused: bring it back within
                // its bounds, otherwise three moves out of four were.
                index: index % (columns_of(tree, Some(new_parent)).len() + 1),
            }),
        Step::ChangeType {
            sheet: f,
            column: c,
            type_index,
        } => column_at(f, c).map(|column| Command::SetColumnType {
            column,
            column_type: [ColumnType::Numeric, ColumnType::Mixed, ColumnType::DateTime][type_index],
        }),
        Step::InsertRows {
            sheet: f,
            at,
            count,
        } => sheet_at(f).map(|sheet| Command::InsertRows { sheet, at, count }),
        Step::DeleteRows {
            sheet: f,
            at,
            count,
        } => sheet_at(f).map(|sheet| Command::DeleteRows { sheet, at, count }),
        Step::DeleteCells {
            sheet: f,
            column: c,
            at,
            count,
        } => column_at(f, c).map(|column| Command::DeleteCells {
            columns: vec![column],
            at,
            count,
        }),
        Step::DuplicateWorksheet { sheet: f } => {
            sheet_at(f).map(|sheet| Command::DuplicateWorksheet {
                sheet,
                name: format!("Copy{rank}"),
            })
        }
        Step::PasteWorksheet { sheet: f } => sheet_at(f).and_then(|sheet| {
            Some(Command::InsertCopy {
                parent: tree.parent(sheet).ok()??,
                copy: NodeCopy::capture(tree, sheet).ok()?,
                name: format!("Pasted{rank}"),
            })
        }),
        Step::PasteWorkbook { sheet: f } => sheet_at(f).and_then(|sheet| {
            let book = tree.parent(sheet).ok()??;
            Some(Command::InsertCopy {
                parent: tree.root(),
                copy: NodeCopy::capture(tree, book).ok()?,
                name: format!("PastedBook{rank}"),
            })
        }),
        Step::DeleteWorksheet { sheet: f } => {
            sheet_at(f).map(|sheet| Command::RemoveNode { id: sheet })
        }
        Step::AddCalculation {
            sheet: f,
            x,
            y,
            correction,
            automatic,
        } => {
            let cols = columns_of(tree, sheet_at(f));
            let (cx, cy) = (pick(&cols, x)?, pick(&cols, y)?);
            if cx == cy {
                return None;
            }
            let (operation, aux) = if correction {
                (
                    Operation::Correct {
                        op: CorrectOp::Divide,
                    },
                    Some(AuxRef {
                        x: pick(&cols, x + 1),
                        y: pick(&cols, y + 2)?,
                    }),
                )
            } else {
                (
                    Operation::Fit {
                        spec: FitSpec {
                            model: Model::Linear,
                            params: vec![],
                        },
                        range: None,
                        residuals: false,
                    },
                    None,
                )
            };
            let series = vec![SeriesRef {
                x: Some(cx),
                y: cy,
                sigma: None,
            }];
            let computation = compute(tree, &operation, &series, aux.as_ref()).ok()?;
            Some(Command::AddCalculation {
                draft: CalculationDraft {
                    operation,
                    series,
                    aux,
                    outputs: computation
                        .columns
                        .iter()
                        .map(|c| OutputDraft {
                            series: c.series,
                            key: c.key.clone(),
                            index: c.index,
                            meta: ColumnMeta::default(),
                            data: c.data.clone(),
                        })
                        .collect(),
                    results: computation.results,
                    keep_results: true,
                    auto_recalc: automatic,
                    low_r2_dialog: true,
                },
            })
        }
        Step::RemoveCalculation { rank, with_columns } => {
            let ids: Vec<NodeId> = tree.calculations().keys().copied().collect();
            pick(&ids, rank).map(|id| Command::RemoveCalculation { id, with_columns })
        }
        Step::Unrelated => Some(Command::AddFolder {
            parent: tree.root(),
            name: format!("Witness{rank}"),
        }),
        Step::RecalculateOutOfDate
        | Step::Undo
        | Step::Redo
        | Step::UndoThenRedo { .. }
        | Step::OpenMacro
        | Step::CloseMacro
        | Step::AbortMacro
        | Step::Reload
        | Step::RevertTo { .. } => None,
    }
}

/// A session's reply to a step: this is what the two twins must share.
/// Ordered from least to most: the reply to a composite step is the strongest
/// of its requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reply {
    Rejected,
    /// Accepted without a recalculation pass: opening or closing a macro,
    /// undoing or redoing with nothing to undo.
    AcceptedWithoutPass,
    /// Accepted, and the actor launched a recalculation pass.
    AcceptedWithPass,
}

impl Reply {
    fn from_result<T, E>(result: Result<T, E>, ran_pass: impl FnOnce(&T) -> bool) -> Self {
        match result {
            Err(_) => Reply::Rejected,
            Ok(value) if ran_pass(&value) => Reply::AcceptedWithPass,
            Ok(_) => Reply::AcceptedWithoutPass,
        }
    }
}

/// Plays a step on a twin. `saves`: the files of this twin, in the
/// order they were written.
fn play(
    handle: &SessionHandle,
    step: &Step,
    command: Option<&Command>,
    saves: &mut Vec<PathBuf>,
    dir: &TempDir,
    name: &str,
) -> Reply {
    match step {
        Step::ReplaceCorrection { .. } => {
            let Some(Command::ReplaceAnalysisData {
                draft,
                expected_versions,
            }) = command
            else {
                return Reply::Rejected;
            };
            let Ok(snapshot) = handle.snapshot() else {
                return Reply::Rejected;
            };
            // The two counters diverge when the full mode rewrites unchanged
            // formulas. The common result does not change; only its
            // concurrency guard takes the versions of its twin.
            let expected_versions: Option<Vec<_>> = expected_versions
                .iter()
                .map(|(id, _)| Some((*id, snapshot.tree.node(*id)?.as_column()?.version())))
                .collect();
            let Some(expected_versions) = expected_versions else {
                return Reply::Rejected;
            };
            Reply::from_result(
                handle.execute(Command::ReplaceAnalysisData {
                    draft: draft.clone(),
                    expected_versions,
                }),
                |_| true,
            )
        }
        Step::RecalculateOutOfDate => Reply::from_result(handle.recalculate(&[]), |_| true),
        Step::Undo => Reply::from_result(handle.undo(), Option::is_some),
        Step::Redo => Reply::from_result(handle.redo(), Option::is_some),
        Step::UndoThenRedo { k } => {
            let mut replies = Vec::new();
            for _ in 0..*k {
                replies.push(Reply::from_result(handle.undo(), Option::is_some));
            }
            for _ in 0..*k {
                replies.push(Reply::from_result(handle.redo(), Option::is_some));
            }
            replies.into_iter().max().unwrap_or(Reply::Rejected)
        }
        Step::OpenMacro => Reply::from_result(handle.begin_macro(MacroKind::PasteCells), |_| false),
        Step::CloseMacro => Reply::from_result(handle.end_macro(), |_| false),
        Step::AbortMacro => Reply::from_result(handle.abort_macro(), |_| true),
        Step::Reload => {
            let path = dir.path().join(format!("{name}-{}.stf", saves.len()));
            if handle.save(&path).is_err() {
                return Reply::Rejected;
            }
            saves.push(path.clone());
            Reply::from_result(handle.load(&path), |_| true)
        }
        Step::RevertTo { save_index } => match saves.get(*save_index % saves.len().max(1)) {
            Some(path) => Reply::from_result(handle.load(path), |_| true),
            None => Reply::Rejected,
        },
        _ => match command {
            Some(command) => Reply::from_result(handle.execute(command.clone()), |_| true),
            None => Reply::Rejected,
        },
    }
}

/// Property 2 on a twin: a `(column, version)` already seen carries the same
/// content. Returns the first faulty column.
fn check_versions(twin: &mut Twin, tree: &Tree) -> Option<(NodeId, u64)> {
    for (id, node) in tree.iter() {
        let Some(column) = node.as_column() else {
            continue;
        };
        let key = (id, column.version());
        let content = (column.column_type(), column.values().clone());
        match twin.seen.get(&key) {
            Some(already) if *already != content => return Some(key),
            Some(_) => {}
            None => {
                twin.seen.insert(key, content);
            }
        }
    }
    None
}

/// What the draw actually reached, counted on the targeted session after
/// every step that launched a recalculation pass, non-empty formula column by
/// non-empty formula column.
#[derive(Debug, Default, Clone)]
struct Coverage {
    /// At least one computed cell.
    computed: usize,
    /// No computed cell: unreadable, cycle, missing or empty source.
    not_computed: usize,
    /// Present before and after a pass, with the same version: spared by the
    /// targeted pass.
    spared: usize,
    /// Steps accepted by both sessions, recalculation pass or not.
    accepted: usize,
    /// Analysis calculations present after a pass, including the stale ones.
    calculations: usize,
    stale_calculations: usize,
    /// Per kind of step: how many were accepted **with** a recalculation
    /// pass. A gesture that never passes proves nothing to the oracle.
    per_step: BTreeMap<&'static str, usize>,
}

impl Coverage {
    fn add(&mut self, other: Coverage) {
        self.computed += other.computed;
        self.not_computed += other.not_computed;
        self.spared += other.spared;
        self.accepted += other.accepted;
        self.calculations += other.calculations;
        self.stale_calculations += other.stale_calculations;
        for (name, n) in other.per_step {
            *self.per_step.entry(name).or_default() += n;
        }
    }

    /// `loading`: a `Load` restarts from the versions of the file, and a
    /// rewritten column may land by chance on the number it carried before —
    /// comparing the versions on both sides then says nothing, and the count
    /// would depend on the numbering.
    fn tally(&mut self, before: &Tree, after: &Tree, loading: bool) {
        let versions_before = if loading {
            HashMap::new()
        } else {
            versions(before)
        };
        for calculation in after.calculations().values() {
            self.calculations += 1;
            self.stale_calculations += usize::from(Calculation::clone(calculation).stale.is_some());
        }
        for (id, node) in after.iter() {
            let Some(column) = node.as_column() else {
                continue;
            };
            if column.meta().formula.is_none() || column.is_empty() {
                continue;
            }
            let is_computed = (0..column.len()).any(|row| match column.get(row) {
                Cell::Float(v) => !v.is_nan(),
                Cell::Text(Some(t)) => t != "---",
                Cell::Text(None) => false,
            });
            if is_computed {
                self.computed += 1;
            } else {
                self.not_computed += 1;
            }
            if versions_before.get(&id) == Some(&column.version()) {
                self.spared += 1;
            }
        }
    }
}

/// Plays `draw` in both twins while checking the three properties.
fn run_sequence(draw: &[Step]) -> Result<Coverage, TestCaseError> {
    let mut coverage = Coverage::default();
    let mut targeted = Twin::new(FormulaRecalc::Targeted);
    let mut full = Twin::new(FormulaRecalc::Full);
    let dir = TempDir::new().expect("temporary directory");
    let (mut targeted_files, mut full_files) = (Vec::new(), Vec::new());

    // The starting project carries formulas never computed: the first
    // recalculation pass computes them all, and a load empties the memo.
    // Property 3 holds only once a pass has been made since either one.
    let mut memo_primed = false;
    for (rank, step) in draw.iter().enumerate() {
        let before = targeted.snapshot();
        let command = command(step, &before.tree, rank);

        let targeted_reply = play(
            &targeted.handle,
            step,
            command.as_ref(),
            &mut targeted_files,
            &dir,
            "targeted",
        );
        let full_reply = play(
            &full.handle,
            step,
            command.as_ref(),
            &mut full_files,
            &dir,
            "full",
        );
        prop_assert_eq!(
            targeted_reply,
            full_reply,
            "step {} ({:?}): the two sessions do not reply the same way",
            rank,
            step
        );
        coverage.accepted += usize::from(targeted_reply != Reply::Rejected);

        let (s_targeted, s_full) = (targeted.snapshot(), full.snapshot());
        // 1. oracle
        prop_assert!(
            *s_targeted.tree == *s_full.tree,
            "step {rank} ({step:?}): the targeted recalculation diverges from the full recalculation\ntargeted calculations: {:?}\nfull calculations: {:?}",
            s_targeted.tree.calculations(),
            s_full.tree.calculations()
        );

        // 2. version ↔ content, in each session.
        if matches!(step, Step::Reload | Step::RevertTo { .. })
            && targeted_reply == Reply::AcceptedWithPass
        {
            targeted.seen.clear();
            full.seen.clear();
        }
        for (twin, tree, which) in [
            (&mut targeted, &s_targeted.tree, "targeted"),
            (&mut full, &s_full.tree, "full"),
        ] {
            if let Some((id, version)) = check_versions(twin, tree) {
                prop_assert!(
                    false,
                    "step {rank} ({step:?}), {which} session: {id} carries version {version} \
                     with content different from the one already seen under that number"
                );
            }
        }

        // 3. targeted: an unrelated command rewrites no column.
        if matches!(step, Step::Unrelated)
            && targeted_reply == Reply::AcceptedWithPass
            && memo_primed
        {
            prop_assert_eq!(
                versions(&before.tree),
                versions(&s_targeted.tree),
                "step {}: an unrelated command rewrote a column",
                rank
            );
        }
        let ran_pass = targeted_reply == Reply::AcceptedWithPass;
        if ran_pass {
            coverage.tally(
                &before.tree,
                &s_targeted.tree,
                matches!(step, Step::Reload | Step::RevertTo { .. }),
            );
            *coverage.per_step.entry(step_name(step)).or_default() += 1;
        }
        memo_primed |= ran_pass;
    }
    Ok(coverage)
}

// -- guard ------------------------------------------------------------------

/// The oracle, then the check of what the draw reached: without the latter,
/// a draw that never set a formula, or whose formulas were all in error, would
/// pass the oracle trivially.
#[test]
fn b22_the_targeted_recalculation_returns_exactly_the_full_recalculation() {
    let total = std::cell::RefCell::new(Coverage::default());
    runner(128)
        .run(&prop::collection::vec(step(), 40..90), |draw| {
            let coverage = run_sequence(&draw)?;
            total.borrow_mut().add(coverage);
            Ok(())
        })
        .expect("the targeted recalculation follows the oracle at every step");
    let total = total.into_inner();
    eprintln!("b22: {total:?}");
    assert!(total.computed > 1000, "{total:?}");
    assert!(total.not_computed > 100, "{total:?}");
    assert!(total.spared > 1000, "{total:?}");
    assert!(total.calculations > 200, "{total:?}");
    assert!(total.stale_calculations > 30, "{total:?}");
    // Every gesture that launches a pass did so often enough for the oracle to
    // have had the chance to see what it would break. Opening and closing a
    // macro never launch a pass.
    for gesture in STEP_NAMES {
        if matches!(*gesture, "OpenMacro" | "CloseMacro") {
            continue;
        }
        let n = total.per_step.get(gesture).copied().unwrap_or(0);
        assert!(n >= 20, "{gesture}: only {n} passes — {total:?}");
    }
}

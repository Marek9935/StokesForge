//! Central property of the bus: **whatever it is put through, undoing
//! everything brings back the initial tree bit for bit, and redoing everything
//! brings back the final tree**.
//!
//! This is the most valuable test here, because the history is hybrid:
//! structure snapshots and data deltas are intermixed in the
//! same stack, and only a somewhat long random sequence brings out the
//! undo orders that do not work.
//!
//! The draw is **deterministic** (`TestRng::deterministic_rng`): a failure is
//! reproduced identically, and no regression file is written.
//!
//! A command refused by the model (name already taken, column gone in the
//! meantime...) is a **draw rejection**, not a failure: we move on to the next step.
//! This is the only way to draw long sequences without making the generator
//! a second implementation of the rules of the core.

use std::sync::Arc;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestRng, TestRunner};

use stokesforge_commands::{Command, History, HistoryLabel, MacroKind};
use stokesforge_core::{Column, ColumnType, MixedCell, Node, NodeId, Project, ValueBlock};
use stokesforge_plot::{PlotSpec, SeriesSpec};

// -- draw -----------------------------------------------------------------

/// Values drawn: plenty of `NaN` (the model equality is binary,
/// `NaN == NaN`), a negative zero (binary-distinct from `0.0`), and enough to
/// exercise the numeric ↔ text conversion.
const SAMPLE_VALUES: &[f64] = &[
    f64::NAN,
    f64::NAN,
    -f64::NAN,
    0.0,
    -0.0,
    1.0,
    -1.5,
    42.0,
    1e300,
    f64::INFINITY,
];

/// A draw step: the indices are **relative** (they will be brought back to
/// the current state at the time of playing), so that a draw stays playable
/// even after columns have been added and removed.
#[derive(Debug, Clone)]
enum Step {
    Cells {
        column: usize,
        start: usize,
        values: Vec<f64>,
    },
    /// **Tolerant** input (`Command::SetCellsPromoting`): the block is
    /// always text, so it fits the column only if the column
    /// is already text — the other cases go through promotion to
    /// `ColumnType::Mixed`, with or without a type change depending on whether
    /// the column already was. This is what puts the three paths of the
    /// command in the same draw as everything else.
    CellsPromoting {
        column: usize,
        start: usize,
        values: Vec<f64>,
    },
    AddColumn {
        sheet: usize,
        index: usize,
    },
    DeleteColumn {
        column: usize,
    },
    Rename {
        column: usize,
    },
    Move {
        column: usize,
        sheet: usize,
        index: usize,
    },
    SetType {
        column: usize,
        kind: usize,
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
}

/// A single step, or a group of 2-3 steps played in a macro.
#[derive(Debug, Clone)]
enum Script {
    Single(Step),
    Group(Vec<Step>),
}

fn value() -> impl Strategy<Value = f64> {
    prop::sample::select(SAMPLE_VALUES.to_vec())
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0usize..8, 0usize..6, prop::collection::vec(value(), 0..4)).prop_map(
            |(column, start, values)| Step::Cells {
                column,
                start,
                values
            }
        ),
        (0usize..8, 0usize..6, prop::collection::vec(value(), 0..4)).prop_map(
            |(column, start, values)| Step::CellsPromoting {
                column,
                start,
                values
            }
        ),
        (0usize..2, 0usize..6).prop_map(|(sheet, index)| Step::AddColumn { sheet, index }),
        (0usize..8).prop_map(|column| Step::DeleteColumn { column }),
        (0usize..8).prop_map(|column| Step::Rename { column }),
        (0usize..8, 0usize..2, 0usize..6).prop_map(|(column, sheet, index)| Step::Move {
            column,
            sheet,
            index
        }),
        (0usize..8, 0usize..4).prop_map(|(column, kind)| Step::SetType { column, kind }),
        (0usize..2, 0usize..6, 1usize..4).prop_map(|(sheet, at, count)| Step::InsertRows {
            sheet,
            at,
            count
        }),
        (0usize..2, 0usize..6, 1usize..4).prop_map(|(sheet, at, count)| Step::DeleteRows {
            sheet,
            at,
            count
        }),
    ]
}

/// ~15% of macros of 2 to 3 commands.
fn script() -> impl Strategy<Value = Script> {
    prop_oneof![
        17 => step().prop_map(Script::Single),
        3 => prop::collection::vec(step(), 2..=3).prop_map(Script::Group),
    ]
}

/// Runner with a fixed seed: reproducible, and no regression file on the
/// disk of the repository.
fn deterministic_runner() -> TestRunner {
    let config = Config {
        cases: 32,
        failure_persistence: None,
        ..Config::default()
    };
    TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha))
}

// -- interpretation -------------------------------------------------------

/// The world on which a draw is played: the acceptance criteria
/// fixture, plus a counter that makes rename names never
/// taken (a rename must fail for good reasons, not because the
/// generator always pulls the same name out of the hat).
struct World {
    project: Project,
    sheets: Vec<NodeId>,
    next_name: u32,
}

impl World {
    fn new() -> Self {
        let mut project = Project::new("Demo").expect("project Demo");
        let root = project.root();
        let book = project
            .add_workbook(root, Some("Book1"))
            .expect("workbook Book1");
        let sheet1 = project.resolve_path("[Book1]Sheet1").expect("sheet Sheet1");
        let col_a = project
            .add_column(sheet1, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("column A");
        project
            .add_column(sheet1, None, Some("B"), Column::from_f64(&[10.0, 20.0]))
            .expect("column B");
        project
            .add_column(
                sheet1,
                None,
                Some("T"),
                Column::from_text(&[Some("a".to_string()), None, Some("c".to_string())]),
            )
            .expect("column T");
        let sheet2 = project
            .add_worksheet(book, Some("Sheet2"))
            .expect("sheet Sheet2");
        let col_x = project
            .add_column(sheet2, None, Some("X"), Column::from_f64(&[0.5, 1.5]))
            .expect("column X");
        // A graph with "row" series (#307): the drawn row insertions and
        // deletions rewrite its `PlotSpec` (`plots` delta), so
        // the tree equality also covers their undo and redo.
        let mut spec = PlotSpec::empty("Rows");
        let panel = spec.panels.get_mut(0).expect("panel");
        for (anchor, row) in [(col_a, 4), (col_a, 1), (col_x, 2), (col_x, 0)] {
            let mut series = SeriesSpec::new(anchor);
            series.row = Some(row);
            panel.series.push(series);
        }
        let root = project.root();
        project
            .add_graph(root, Some("Rows"), spec)
            .expect("rows graph");
        Self {
            project,
            sheets: vec![sheet1, sheet2],
            next_name: 0,
        }
    }

    fn children_of(&self, sheet: NodeId) -> Vec<NodeId> {
        self.project
            .tree()
            .children(sheet)
            .map(<[NodeId]>::to_vec)
            .unwrap_or_default()
    }

    /// All the live columns, the two sheets one after the other.
    fn columns(&self) -> Vec<NodeId> {
        self.sheets
            .iter()
            .flat_map(|sheet| self.children_of(*sheet))
            .collect()
    }

    fn column_type(&self, column: NodeId) -> Option<ColumnType> {
        self.project
            .tree()
            .node(column)
            .and_then(Node::as_column)
            .map(Column::column_type)
    }

    /// Translates a step into a command playable on the current state, or `None` if the
    /// step has no target (no column left, for example).
    fn command_for(&mut self, step: &Step) -> Option<Command> {
        let columns = self.columns();
        match step {
            Step::Cells {
                column,
                start,
                values,
            } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let block = match self.column_type(target)? {
                    ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(values.clone()),
                    ColumnType::Mixed => {
                        ValueBlock::Mixed(values.iter().copied().map(MixedCell::from_f64).collect())
                    }
                };
                Some(Command::SetCells {
                    column: target,
                    start: start % 8,
                    values: block,
                    expected_version: None,
                })
            }
            Step::CellsPromoting {
                column,
                start,
                values,
            } => {
                let target = *columns.get(column % columns.len().max(1))?;
                // A mixed block with text cells, whatever the type of
                // the column: this is precisely what `SetCells`
                // would refuse and what this command accepts by widening the
                // column.
                let block = ValueBlock::Mixed(
                    values
                        .iter()
                        .map(|v| {
                            if v.is_nan() {
                                MixedCell::Empty
                            } else {
                                MixedCell::Text(format!("v{v}"))
                            }
                        })
                        .collect(),
                );
                Some(Command::SetCellsPromoting {
                    column: target,
                    start: start % 8,
                    values: block,
                    expected_version: None,
                })
            }
            Step::AddColumn { sheet, index } => {
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                let count = self.children_of(sheet).len();
                Some(Command::AddColumn {
                    sheet,
                    index: Some(index % (count + 1)),
                    name: None,
                    column: Column::from_f64(&[7.0, f64::NAN]),
                })
            }
            Step::DeleteColumn { column } => {
                // Always keep enough to play the following steps.
                if columns.len() <= 1 {
                    return None;
                }
                Some(Command::DeleteColumn {
                    column: *columns.get(column % columns.len())?,
                })
            }
            Step::Rename { column } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let name = format!("K{}", self.next_name);
                self.next_name += 1;
                Some(Command::Rename { id: target, name })
            }
            Step::Move {
                column,
                sheet,
                index,
            } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                let count = self.children_of(sheet).len();
                Some(Command::Move {
                    id: target,
                    new_parent: sheet,
                    index: index % (count + 1),
                })
            }
            Step::SetType { column, kind } => {
                let target = *columns.get(column % columns.len().max(1))?;
                let column_type = match kind % 3 {
                    0 => ColumnType::Numeric,
                    1 => ColumnType::DateTime,
                    _ => ColumnType::Mixed,
                };
                Some(Command::SetColumnType {
                    column: target,
                    column_type,
                })
            }
            Step::InsertRows { sheet, at, count } => {
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                Some(Command::InsertRows {
                    sheet,
                    at: *at,
                    count: *count,
                })
            }
            Step::DeleteRows { sheet, at, count } => {
                let sheet = *self.sheets.get(sheet % self.sheets.len())?;
                Some(Command::DeleteRows {
                    sheet,
                    at: *at,
                    count: *count,
                })
            }
        }
    }

    /// Plays a step. Returns `true` if it was accepted; a refusal is a draw
    /// rejection, silent.
    fn play_step(&mut self, history: &mut History, step: &Step) -> bool {
        match self.command_for(step) {
            Some(command) => history.execute(&mut self.project, command).is_ok(),
            None => false,
        }
    }

    /// Plays a complete scenario and returns the number of accepted commands.
    fn play(&mut self, history: &mut History, scripts: &[Script]) -> usize {
        let mut accepted = 0;
        for script in scripts {
            match script {
                Script::Single(step) => {
                    if self.play_step(history, step) {
                        accepted += 1;
                    }
                }
                Script::Group(steps) => {
                    // B7: a refusal (nesting depth at the cap) is
                    // a draw rejection, silent like the others — see
                    // the doc of `play_step`. Not nested here (a group
                    // never opens before the previous one has
                    // closed), so never reached in practice.
                    if history.begin_macro(MacroKind::Import).is_err() {
                        return accepted;
                    }
                    for step in steps {
                        if self.play_step(history, step) {
                            accepted += 1;
                        }
                    }
                    // An empty macro leaves no entry: nothing to handle
                    // unless all the commands of the group were refused.
                    if history.end_macro(&mut self.project).is_err() {
                        return accepted;
                    }
                }
            }
        }
        accepted
    }

    /// Plays single steps until `target` commands are accepted.
    fn play_until(&mut self, history: &mut History, steps: &[Step], target: usize) -> usize {
        let mut accepted = 0;
        for step in steps {
            if accepted >= target {
                break;
            }
            if self.play_step(history, step) {
                accepted += 1;
            }
        }
        accepted
    }
}

fn undo_all(history: &mut History, project: &mut Project) -> Result<(), TestCaseError> {
    loop {
        match history.undo(project) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(err) => return Err(TestCaseError::fail(format!("undo refused: {err}"))),
        }
    }
}

fn redo_all(history: &mut History, project: &mut Project) -> Result<(), TestCaseError> {
    loop {
        match history.redo(project) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(err) => return Err(TestCaseError::fail(format!("redo refused: {err}"))),
        }
    }
}

// -- 22. full round trip --------------------------------------------------

#[test]
fn undoing_everything_restores_the_initial_tree_bit_for_bit() {
    let mut runner = deterministic_runner();
    runner
        .run(&prop::collection::vec(script(), 70..=90), |scripts| {
            let mut world = World::new();
            let mut history = History::with_limit(0);
            let initial = Arc::clone(world.project.tree());

            let accepted = world.play(&mut history, &scripts);
            prop_assert!(
                accepted >= 50,
                "draw too degenerate: only {accepted} commands accepted"
            );

            let last = Arc::clone(world.project.tree());
            prop_assert!(
                *last != *initial,
                "the draw changed nothing: the test would prove nothing"
            );

            undo_all(&mut history, &mut world.project)?;
            // Binary equality, `NaN` included — and the `NodeId`s live in
            // the nodes, so this comparison also covers the return of a
            // deleted node under its original identifier.
            prop_assert!(
                **world.project.tree() == *initial,
                "after a full undo, the tree differs from the initial one"
            );

            redo_all(&mut history, &mut world.project)?;
            prop_assert!(
                **world.project.tree() == *last,
                "after a full redo, the tree differs from the final one"
            );
            Ok(())
        })
        .expect("round-trip property verified");
}

// -- 23. determinism -------------------------------------------------------

#[test]
fn replaying_the_same_draw_yields_the_same_history_labels() {
    /// Plays a draw and returns the labels of the stack, from the top to the bottom.
    fn labels(scripts: &[Script]) -> Vec<HistoryLabel> {
        let mut world = World::new();
        let mut history = History::with_limit(0);
        world.play(&mut history, scripts);
        let mut collected = Vec::new();
        while let Some(label) = history.undo_label().cloned() {
            collected.push(label);
            if history.undo(&mut world.project) != Ok(true) {
                break;
            }
        }
        collected
    }

    let mut runner = deterministic_runner();
    runner
        .run(&prop::collection::vec(script(), 70..=90), |scripts| {
            prop_assert_eq!(labels(&scripts), labels(&scripts));
            Ok(())
        })
        .expect("deterministic labels");
}

// -- 24. under a limit -----------------------------------------------------

#[test]
fn a_bounded_history_still_round_trips_within_its_window() {
    let mut runner = deterministic_runner();
    runner
        .run(&prop::collection::vec(step(), 60..=80), |steps| {
            let mut world = World::new();
            let mut history = History::with_limit(5);

            let accepted = world.play_until(&mut history, &steps, 30);
            prop_assert!(
                accepted == 30,
                "draw too degenerate: {accepted} commands accepted out of 30 expected"
            );
            prop_assert_eq!(history.len(), 5);

            let before = Arc::clone(world.project.tree());
            for _ in 0..3 {
                prop_assert_eq!(history.undo(&mut world.project), Ok(true));
            }
            for _ in 0..3 {
                prop_assert_eq!(history.redo(&mut world.project), Ok(true));
            }
            prop_assert!(
                **world.project.tree() == *before,
                "three undos then three redos must be neutral"
            );
            Ok(())
        })
        .expect("under-limit property verified");
}

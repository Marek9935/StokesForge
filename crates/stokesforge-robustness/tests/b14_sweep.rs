//! Battery 14 — sweep: three invariants checked **after every step** of a
//! randomly drawn sequence.
//!
//! Batteries 1 to 13 test chosen places. This one chooses nothing: it plays
//! sequences of commands (writes, structure, conversions, rows, undos, redos)
//! and rechecks after every step three properties that the repository states
//! elsewhere:
//!
//! 1. **version ↔ content** — two different states of the same column never
//!    share a [`stokesforge_core::Column::version`] number;
//! 2. **`path_of` ∘ `resolve_path` = identity** — the property stated by
//!    `b8_properties`, here on trees that this draw builds;
//! 3. **disk round trip** — `save` then `load` returns an equal tree, and
//!    equal version counters.
//!
//! The draw is **deterministic** (`TestRng::deterministic_rng`), like that of
//! `stokesforge-commands/tests/undo_random.rs`: a failure reproduces
//! identically and no regression file is written.
//!
//! This sweep found A7 and A8 without being told where to look; it stays here
//! as a safety net, not only as proof.

use std::collections::HashMap;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
use stokesforge_analysis::ops::Operation;
use stokesforge_commands::{Command, History};
use stokesforge_core::{compute, CalculationDraft, OutputDraft, SeriesRef, Tree};
use stokesforge_core::{Column, ColumnMeta, ColumnType, ColumnValues, NodeId, Project, ValueBlock};
use tempfile::TempDir;

// -- draw -----------------------------------------------------------------

/// Names deliberately shared between folders, notes and columns: this is what
/// brings the model's three uniqueness scopes face to face.
const NAMES: [&str; 4] = ["P", "Q", "R", "S"];

#[derive(Debug, Clone)]
enum Step {
    /// Final correction of an ordinary column (#212).
    ReplaceCorrection {
        column: usize,
    },
    Cells {
        column: usize,
        start_row: usize,
        count: usize,
    },
    /// Non-zero `version`: a column that already carries a number (coming
    /// from a file, from another project), possibly beyond the project's
    /// counter — this is the case that showed, while rereading #193, that an
    /// insertion must move the counter past it.
    AddColumn {
        index: usize,
        version: u64,
    },
    DeleteColumn {
        column: usize,
    },
    RenameColumn {
        column: usize,
        name: usize,
    },
    AddFolder {
        name: usize,
    },
    AddNote {
        name: usize,
    },
    MoveToRoot {
        child: usize,
        index: usize,
    },
    ChangeType {
        column: usize,
        type_index: usize,
    },
    InsertRows {
        at: usize,
        count: usize,
    },
    DeleteRows {
        at: usize,
        count: usize,
    },
    Undo,
    Redo,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0usize..6).prop_map(|column| Step::ReplaceCorrection { column }),
        (0usize..6, 0usize..6, 0usize..4).prop_map(|(column, start_row, count)| Step::Cells {
            column,
            start_row,
            count
        }),
        (0usize..6, prop_oneof![Just(0u64), 1u64..80])
            .prop_map(|(index, version)| Step::AddColumn { index, version }),
        (0usize..6).prop_map(|column| Step::DeleteColumn { column }),
        (0usize..6, 0usize..4).prop_map(|(column, name)| Step::RenameColumn { column, name }),
        (0usize..4).prop_map(|name| Step::AddFolder { name }),
        (0usize..4).prop_map(|name| Step::AddNote { name }),
        (0usize..5, 0usize..5).prop_map(|(child, index)| Step::MoveToRoot { child, index }),
        (0usize..6, 0usize..3)
            .prop_map(|(column, type_index)| Step::ChangeType { column, type_index }),
        (0usize..5, 1usize..3).prop_map(|(at, count)| Step::InsertRows { at, count }),
        (0usize..5, 1usize..3).prop_map(|(at, count)| Step::DeleteRows { at, count }),
        Just(Step::Undo),
        Just(Step::Redo),
    ]
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

// -- the world played ------------------------------------------------------

struct World {
    project: Project,
    history: History,
    sheet: NodeId,
}

impl World {
    fn new() -> Self {
        let mut project = Project::new("Demo").expect("valid project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("workbook");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
        project
            .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
            .expect("column A");
        project
            .add_column(sheet, None, Some("B"), Column::from_f64(&[3.0]))
            .expect("column B");
        Self {
            project,
            history: History::with_limit(0),
            sheet,
        }
    }

    fn columns(&self) -> Vec<NodeId> {
        self.project
            .tree()
            .children(self.sheet)
            .map(<[NodeId]>::to_vec)
            .unwrap_or_default()
    }

    /// Plays one step. A refusal by the model is a draw rejection, not a
    /// failure — same convention as `undo_random.rs`.
    fn play(&mut self, step: &Step) {
        let columns = self.columns();
        let pick = |i: usize| columns.get(i % columns.len().max(1)).copied();
        let command = match step {
            Step::ReplaceCorrection { column } => {
                correction_command(self.project.tree(), &columns, *column)
            }
            Step::Cells {
                column,
                start_row,
                count,
            } => pick(*column).map(|c| Command::SetCells {
                column: c,
                start: *start_row,
                values: ValueBlock::Float((0..*count).map(|k| k as f64 + 0.5).collect()),
                expected_version: None,
            }),
            Step::AddColumn { index, version } => Column::from_block_with_version(
                ColumnType::Numeric,
                ValueBlock::Float(vec![7.0, 8.0]),
                ColumnMeta::default(),
                *version,
            )
            .ok()
            .map(|column| Command::AddColumn {
                sheet: self.sheet,
                index: Some(*index),
                name: None,
                column,
            }),
            Step::DeleteColumn { column } => {
                pick(*column).map(|c| Command::DeleteColumn { column: c })
            }
            Step::RenameColumn { column, name } => pick(*column).map(|c| Command::Rename {
                id: c,
                name: NAMES[*name].to_string(),
            }),
            Step::AddFolder { name } => {
                let root = self.project.root();
                let _ = self.project.add_folder(root, NAMES[*name]);
                None
            }
            Step::AddNote { name } => {
                let root = self.project.root();
                let _ = self.project.add_note(root, Some(NAMES[*name]), "");
                None
            }
            Step::MoveToRoot { child, index } => {
                let root = self.project.root();
                let children = self
                    .project
                    .tree()
                    .children(root)
                    .map(<[NodeId]>::to_vec)
                    .unwrap_or_default();
                children
                    .get(*child % children.len().max(1))
                    .map(|id| Command::Move {
                        id: *id,
                        new_parent: root,
                        index: *index,
                    })
            }
            Step::ChangeType { column, type_index } => {
                pick(*column).map(|c| Command::SetColumnType {
                    column: c,
                    column_type: [ColumnType::Numeric, ColumnType::Mixed, ColumnType::DateTime]
                        [*type_index],
                })
            }
            Step::InsertRows { at, count } => Some(Command::InsertRows {
                sheet: self.sheet,
                at: *at,
                count: *count,
            }),
            Step::DeleteRows { at, count } => Some(Command::DeleteRows {
                sheet: self.sheet,
                at: *at,
                count: *count,
            }),
            Step::Undo => {
                let _ = self.history.undo(&mut self.project);
                None
            }
            Step::Redo => {
                let _ = self.history.redo(&mut self.project);
                None
            }
        };
        if let Some(command) = command {
            let _ = self.history.execute(&mut self.project, command);
        }
    }
}

/// `NodeId → (version, binary content)` for all the columns of the tree.
/// The content is reduced to bits (`to_bits` for floats) so that
/// `NaN == NaN`, as everywhere else in the model.
fn fingerprints(project: &Project) -> HashMap<NodeId, (u64, Vec<u64>)> {
    project
        .tree()
        .iter()
        .filter_map(|(id, node)| {
            let column = node.as_column()?;
            let bits: Vec<u64> = match column.values() {
                ColumnValues::Float(v) => v.iter().map(|x| x.to_bits()).collect(),
                // A fingerprint, not a serialization: the cell's tag is enough
                // to tell the three variants apart, and the number's bit
                // pattern is what matters for `NaN`.
                ColumnValues::Mixed(v) => v
                    .iter()
                    .map(|c| match c {
                        stokesforge_core::MixedCell::Empty => 0,
                        stokesforge_core::MixedCell::Number(x) => x.to_bits(),
                        stokesforge_core::MixedCell::Text(t) => t.len() as u64 + 1,
                    })
                    .collect(),
            };
            Some((id, (column.version(), bits)))
        })
        .collect()
}

// -- guard ----------------------------------------------------------------

#[test]
fn b14_the_disk_round_trip_holds_at_every_step() {
    let mut runner = runner(8);
    runner
        .run(&prop::collection::vec(step(), 40..60), |draw| {
            let mut world = World::new();
            let dir = TempDir::new().expect("temporary directory");
            let path = dir.path().join("sweep.stf");
            for (rank, step) in draw.iter().enumerate() {
                world.play(step);
                stokesforge_persistence::save(&world.project, &path).expect("save");
                let reloaded = stokesforge_persistence::load(&path).expect("reload");
                prop_assert!(
                    **reloaded.tree() == **world.project.tree(),
                    "step {rank} ({step:?}): the reloaded tree differs from the in-memory tree"
                );
                prop_assert!(
                    fingerprints(&reloaded) == fingerprints(&world.project),
                    "step {rank} ({step:?}): the version counters do not survive the disk"
                );
            }
            Ok(())
        })
        .expect("disk round trip checked");
}

// -- reproductions --------------------------------------------------------

#[test]
fn b14_a_version_never_designates_two_contents() {
    let mut runner = runner(16);
    runner
        .run(&prop::collection::vec(step(), 60..80), |draw| {
            let mut world = World::new();
            let mut seen: HashMap<(NodeId, u64), Vec<u64>> = HashMap::new();
            for (rank, step) in draw.iter().enumerate() {
                world.play(step);
                for (id, (version, bits)) in fingerprints(&world.project) {
                    if let Some(earlier) = seen.get(&(id, version)) {
                        prop_assert!(
                            *earlier == bits,
                            "step {rank} ({step:?}): {id} is at version {version} with content \
                             different from the one already seen under that number — a reader \
                             that compares versions will think it is up to date"
                        );
                    }
                    seen.insert((id, version), bits);
                }
            }
            Ok(())
        })
        .expect("version ↔ content checked");
}

#[test]
fn b14_the_canonical_path_always_designates_its_node_again() {
    let mut runner = runner(16);
    runner
        .run(&prop::collection::vec(step(), 60..80), |draw| {
            let mut world = World::new();
            for (rank, step) in draw.iter().enumerate() {
                world.play(step);
                for id in world.project.tree().walk_all() {
                    let Ok(path) = world.project.path_of(id) else {
                        continue;
                    };
                    prop_assert!(
                        world.project.resolve_path(&path) == Ok(id),
                        "step {rank} ({step:?}): path_of({id}) = {path:?}, which resolves to \
                         {:?}",
                        world.project.resolve_path(&path)
                    );
                }
            }
            Ok(())
        })
        .expect("path_of ∘ resolve_path checked");
}

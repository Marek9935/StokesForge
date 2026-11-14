//! Battery 23 — formulas and **structural** gestures (#412): the steps that
//! `b22` does not draw — renames (sheet, workbook, folder, column, case-only,
//! awkward names), moves of a sheet, a workbook or a folder, removals of a
//! workbook, a folder or a sheet, transposed sheets and their refresh, pastes
//! through the object clipboard (same project or external), several roots in
//! one gesture, column type changes — mixed with entries, formulas, undo,
//! redo and reloads.
//!
//! Two properties after every step:
//!
//! 1. **oracle** — a targeted session and a full-recalculation session, fed
//!    the same steps, give the same replies and equal trees (as in `b22`);
//! 2. **independent re-evaluation** — the current project is saved and loaded
//!    into a *fresh* [`FormulaRecalc::Full`] session, one command makes it
//!    calculate everything from the file, and every formula cell must equal
//!    the live one. This catches an error common to both live modes that the
//!    twin oracle cannot see (a stale value kept by both).
//!
//! The draw is deterministic like `b22`. `SF_PROPTEST_SEED` replaces the seed and
//! `SF_PROPTEST_SCALE` multiplies the number of sequences (96 at scale 1).

use std::collections::BTreeMap;
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::TestRunner;
use stokesforge_commands::Command;
use stokesforge_core::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, CopyInsertion, NodeCopy, NodeId, NodeKind,
    Project, Tree, ValueBlock,
};
use stokesforge_persistence::clipboard::{decode_objects, encode_objects};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};
use tempfile::TempDir;

mod common;
use common::link_states;

// -- draw -----------------------------------------------------------------

/// Letters, in the sheet the column is in. References to sheets and workbooks
/// that a rename, a move or a removal may have made unreachable are refused or
/// read `#REF`; both are fine.
const FORMULAS: [&str; 16] = [
    "A*2",
    "B+1",
    "A+B",
    "max(B)",
    "integral(B)",
    "at(B, 2)",
    "max(Sheet1!B)",
    "max(Sheet2!B)",
    "max([Book2]Sheet1!B)",
    "mean([Book3]Sheet1!B, 1, 3)",
    "at([Book1]Sheet2!B, 1.5)",
    "max('Blanc 2'!A)",
    "sum('it''s'!A)",
    "max([F1/Book3]Sheet1!A)",
    "max([Book1]Sheet1!A)",
    "B/max(Sheet2!B)",
];

/// Names given by a rename, besides the case-only variants of the current one.
const NAMES: [&str; 14] = [
    "Blanc 2",
    "it's",
    "a!b",
    "a[b]",
    "1st",
    "h\u{e9}llo",
    "Book1",
    "Book2",
    "Book3",
    "Sheet1",
    "Sheet2",
    "F1",
    "x y",
    "\u{4e16}\u{754c}",
];

#[derive(Debug, Clone)]
enum Step {
    Enter {
        sheet: usize,
        column: usize,
        row: usize,
        count: usize,
        seed: usize,
    },
    SetFormula {
        sheet: usize,
        column: usize,
        formula: usize,
    },
    /// Keeps `A+<next letter>` in a column: the letter of the column the sheet
    /// does not have yet (#433). A column added, moved or pasted into the
    /// sheet, or an undo of a deletion, binds it.
    SetPending {
        sheet: usize,
        column: usize,
    },
    /// A column added at the end of a sheet.
    AddColumn {
        sheet: usize,
    },
    /// A column moved into another sheet (or within its own): it can fill the
    /// letter a kept formula waits for, or leave one (#433).
    MoveColumn {
        sheet: usize,
        column: usize,
        target: usize,
        index: usize,
    },
    /// `kind`: 0 sheet, 1 workbook, 2 folder, 3 column.
    Rename {
        kind: usize,
        idx: usize,
        name: usize,
    },
    /// `kind`: 0 sheet (into a workbook), 1 workbook (into a folder), 2 folder.
    MoveNode {
        kind: usize,
        idx: usize,
        target: usize,
        index: usize,
    },
    /// `kind`: 0 sheet, 1 workbook, 2 folder.
    RemoveNode {
        kind: usize,
        idx: usize,
    },
    Transpose {
        idx: usize,
    },
    RecalculateTranspose {
        idx: usize,
    },
    /// Checks or unchecks "Recalculate automatically" on a linked sheet (#463).
    ToggleAuto {
        idx: usize,
    },
    /// A value typed in a Y column of the source of a linked sheet (#463): a
    /// followed sheet catches up with it in the same pass.
    EditSource {
        idx: usize,
        column: usize,
        seed: usize,
    },
    /// A value typed by hand in a linked column of a transposed sheet (#463):
    /// a followed sheet must stop following, never overwrite it.
    TypeInRow {
        idx: usize,
        column: usize,
        seed: usize,
    },
    /// Copy a workbook (0) or a folder (1) through encode/decode, back into the
    /// same project (`external` false) or as if it came from another one.
    Paste {
        kind: usize,
        idx: usize,
        external: bool,
        target: usize,
    },
    /// Two roots in one `InsertCopies`.
    PasteMany {
        a: usize,
        b: usize,
    },
    SetType {
        sheet: usize,
        column: usize,
        kind: usize,
    },
    DuplicateSheet {
        sheet: usize,
    },
    DeleteColumn {
        sheet: usize,
        column: usize,
    },
    Undo,
    Redo,
    UndoThenRedo {
        k: usize,
    },
    Reload,
    Unrelated,
}

fn step() -> impl Strategy<Value = Step> {
    let i = 0usize..8;
    prop_oneof![
        6 => (i.clone(), i.clone(), 0usize..5, 1usize..4, 0usize..50)
            .prop_map(|(sheet, column, row, count, seed)| Step::Enter { sheet, column, row, count, seed }),
        5 => (i.clone(), i.clone(), 0..FORMULAS.len())
            .prop_map(|(sheet, column, formula)| Step::SetFormula { sheet, column, formula }),
        3 => (i.clone(), i.clone()).prop_map(|(sheet, column)| Step::SetPending { sheet, column }),
        3 => i.clone().prop_map(|sheet| Step::AddColumn { sheet }),
        3 => (i.clone(), i.clone(), i.clone(), 0usize..6)
            .prop_map(|(sheet, column, target, index)| Step::MoveColumn { sheet, column, target, index }),
        6 => (0usize..4, i.clone(), 0usize..(NAMES.len() + 2))
            .prop_map(|(kind, idx, name)| Step::Rename { kind, idx, name }),
        5 => (0usize..3, i.clone(), i.clone(), 0usize..5)
            .prop_map(|(kind, idx, target, index)| Step::MoveNode { kind, idx, target, index }),
        3 => (0usize..3, i.clone()).prop_map(|(kind, idx)| Step::RemoveNode { kind, idx }),
        3 => i.clone().prop_map(|idx| Step::Transpose { idx }),
        2 => i.clone().prop_map(|idx| Step::RecalculateTranspose { idx }),
        3 => i.clone().prop_map(|idx| Step::ToggleAuto { idx }),
        1 => (i.clone(), 0usize..4, 0usize..50)
            .prop_map(|(idx, column, seed)| Step::TypeInRow { idx, column, seed }),
        6 => (i.clone(), 0usize..4, 0usize..50)
            .prop_map(|(idx, column, seed)| Step::EditSource { idx, column, seed }),
        3 => (0usize..2, i.clone(), any::<bool>(), i.clone())
            .prop_map(|(kind, idx, external, target)| Step::Paste { kind, idx, external, target }),
        2 => (i.clone(), i.clone()).prop_map(|(a, b)| Step::PasteMany { a, b }),
        1 => (i.clone(), i.clone(), 0usize..3).prop_map(|(sheet, column, kind)| Step::SetType { sheet, column, kind }),
        1 => i.clone().prop_map(|sheet| Step::DuplicateSheet { sheet }),
        1 => (i.clone(), i.clone()).prop_map(|(sheet, column)| Step::DeleteColumn { sheet, column }),
        3 => Just(Step::Undo),
        2 => Just(Step::Redo),
        1 => (1usize..4).prop_map(|k| Step::UndoThenRedo { k }),
        1 => Just(Step::Reload),
        1 => Just(Step::Unrelated),
    ]
}

fn step_name(step: &Step) -> &'static str {
    match step {
        Step::Enter { .. } => "Enter",
        Step::SetFormula { .. } => "SetFormula",
        Step::SetPending { .. } => "SetPending",
        Step::AddColumn { .. } => "AddColumn",
        Step::MoveColumn { .. } => "MoveColumn",
        Step::Rename { kind: 0, .. } => "RenameSheet",
        Step::Rename { kind: 1, .. } => "RenameWorkbook",
        Step::Rename { kind: 2, .. } => "RenameFolder",
        Step::Rename { .. } => "RenameColumn",
        Step::MoveNode { kind: 0, .. } => "MoveSheet",
        Step::MoveNode { kind: 1, .. } => "MoveWorkbook",
        Step::MoveNode { .. } => "MoveFolder",
        Step::RemoveNode { kind: 0, .. } => "RemoveSheet",
        Step::RemoveNode { kind: 1, .. } => "RemoveWorkbook",
        Step::RemoveNode { .. } => "RemoveFolder",
        Step::Transpose { .. } => "Transpose",
        Step::RecalculateTranspose { .. } => "RecalculateTranspose",
        Step::ToggleAuto { .. } => "ToggleAuto",
        Step::TypeInRow { .. } => "TypeInRow",
        Step::EditSource { .. } => "EditSource",
        Step::Paste {
            external: false, ..
        } => "PasteSame",
        Step::Paste { .. } => "PasteExternal",
        Step::PasteMany { .. } => "PasteMany",
        Step::SetType { .. } => "SetType",
        Step::DuplicateSheet { .. } => "DuplicateSheet",
        Step::DeleteColumn { .. } => "DeleteColumn",
        Step::Undo => "Undo",
        Step::Redo => "Redo",
        Step::UndoThenRedo { .. } => "UndoThenRedo",
        Step::Reload => "Reload",
        Step::Unrelated => "Unrelated",
    }
}

fn runner(cases: u32) -> TestRunner {
    common::env_runner(cases)
}

// -- the world --------------------------------------------------------------

fn col(
    p: &mut Project,
    sheet: NodeId,
    values: &[f64],
    role: ColumnRole,
    x_prime: Option<f64>,
) -> NodeId {
    p.add_column(
        sheet,
        None,
        None,
        Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values.to_vec()),
            ColumnMeta {
                role,
                x_prime,
                ..Default::default()
            },
        )
        .expect("column"),
    )
    .expect("column")
}

fn formula(p: &mut Project, column: NodeId, text: &str) {
    let formula = p
        .tree()
        .formula_from_letters(column, text)
        .expect("starting formula");
    let mut meta = p
        .tree()
        .node(column)
        .and_then(|n| n.as_column())
        .expect("column")
        .meta()
        .clone();
    meta.formula = Some(formula);
    p.set_column_meta(column, meta).expect("formula set");
}

fn initial_project() -> Project {
    let mut p = Project::new("Demo").expect("project");
    let root = p.root();
    let f1 = p.add_folder(root, "F1").expect("F1");
    p.add_folder(root, "F2").expect("F2");
    let b3 = p.add_workbook(f1, Some("Book3")).expect("Book3");
    let b1 = p.add_workbook(root, Some("Book1")).expect("Book1");
    let b2 = p.add_workbook(root, Some("Book2")).expect("Book2");
    let s3 = p.tree().children(b3).expect("sheets")[0];
    let s1 = p.tree().children(b1).expect("sheets")[0];
    let s1b = p.add_worksheet(b1, None).expect("Sheet2");
    let s2 = p.tree().children(b2).expect("sheets")[0];
    let s2b = p.add_worksheet(b2, None).expect("Sheet2 of Book2");
    let mut pending: Vec<(NodeId, &str)> = Vec::new();
    col(&mut p, s3, &[1.0, 2.0, 3.0, 4.0], ColumnRole::X, None);
    col(&mut p, s3, &[10.0, 20.0, 30.0], ColumnRole::Y, None);
    let c = col(&mut p, s3, &[], ColumnRole::Y, None);
    pending.push((c, "A+B"));
    col(&mut p, s1, &[1.0, 2.0, 3.0], ColumnRole::X, None);
    col(&mut p, s1, &[10.0, 0.0, 30.0, 40.0], ColumnRole::Y, None);
    for text in ["A*2", "max([Book2]Sheet1!B)", "mean([Book3]Sheet1!B, 1, 3)"] {
        let c = col(&mut p, s1, &[], ColumnRole::Y, None);
        pending.push((c, text));
    }
    col(&mut p, s1b, &[0.0, 1.0, 2.0, 3.0], ColumnRole::X, None);
    col(
        &mut p,
        s1b,
        &[100.0, 250.0, 300.0, 500.0],
        ColumnRole::Y,
        None,
    );
    for text in ["integral(B)", "B/max(Sheet1!B)", "at([Book2]Sheet1!B, 5)"] {
        let c = col(&mut p, s1b, &[], ColumnRole::Y, None);
        pending.push((c, text));
    }
    col(&mut p, s2, &[4.0, 5.0, 6.0], ColumnRole::X, None);
    col(&mut p, s2, &[40.0, 50.0, 60.0], ColumnRole::Y, None);
    let c = col(&mut p, s2, &[], ColumnRole::Y, None);
    pending.push((c, "max([Book1]Sheet2!B)"));
    // A transposable sheet: every Y column numeric with an x'.
    col(&mut p, s2b, &[1.0, 2.0, 3.0], ColumnRole::Y, Some(1.0));
    col(&mut p, s2b, &[4.0, 5.0, 6.0], ColumnRole::Y, Some(2.0));
    // A formula Y column with an x' (#439): a link over it must stay up to date
    // while the formula gives the same numbers.
    let c = col(&mut p, s2b, &[], ColumnRole::Y, Some(3.0));
    pending.push((c, "A+B"));
    // Formulas last: they may read columns of sheets created after them.
    for (c, text) in pending {
        formula(&mut p, c, text);
    }
    p
}

// -- helpers ------------------------------------------------------------------

struct Twin {
    _session: Session,
    handle: SessionHandle,
}

impl Twin {
    fn new(mode: FormulaRecalc) -> Self {
        let session = Session::spawn_with_recalc(initial_project(), None, mode).expect("actor");
        let handle = session.handle();
        let root = handle.snapshot().expect("snapshot").tree.root();
        // A first pass, so that the starting formulas are computed.
        handle
            .execute(Command::AddFolder {
                parent: root,
                name: "Prime".into(),
            })
            .expect("prime");
        Self {
            _session: session,
            handle,
        }
    }

    fn snapshot(&self) -> Snapshot {
        self.handle.snapshot().expect("snapshot")
    }
}

fn nodes_where(tree: &Tree, keep: impl Fn(&Tree, NodeId) -> bool) -> Vec<NodeId> {
    tree.walk_all()
        .into_iter()
        .filter(|&id| keep(tree, id))
        .collect()
}

fn sheets(tree: &Tree) -> Vec<NodeId> {
    nodes_where(tree, |t, id| {
        t.node(id).is_some_and(|n| n.kind().is_worksheet())
    })
}

fn books(tree: &Tree) -> Vec<NodeId> {
    nodes_where(tree, |t, id| {
        t.node(id)
            .is_some_and(|n| matches!(n.kind(), NodeKind::Workbook { .. }))
    })
}

fn folders(tree: &Tree) -> Vec<NodeId> {
    nodes_where(tree, |t, id| {
        t.node(id).is_some_and(|n| n.kind().is_folder())
    })
}

fn pick<T: Copy>(list: &[T], i: usize) -> Option<T> {
    (!list.is_empty()).then(|| list[i % list.len()])
}

fn columns_of(tree: &Tree, sheet: Option<NodeId>) -> Vec<NodeId> {
    sheet
        .and_then(|s| tree.children(s).ok())
        .map(<[NodeId]>::to_vec)
        .unwrap_or_default()
}

fn is_ancestor(tree: &Tree, ancestor: NodeId, node: NodeId) -> bool {
    tree.ancestors(node).is_ok_and(|a| a.contains(&ancestor))
}

/// Sheets whose Y columns can all be transposed (an x' on every one, or on none:
/// index mode, #493), formula columns included (#439: they were left out while
/// the targeted and full twins differed after an undo over such a link).
fn transposable(tree: &Tree) -> Vec<NodeId> {
    let candidates: Vec<(NodeId, bool)> = sheets(tree)
        .into_iter()
        .filter_map(|s| {
            let ys: Vec<_> = columns_of(tree, Some(s))
                .into_iter()
                .filter_map(|c| tree.node(c).and_then(|n| n.as_column()))
                .filter(|c| c.meta().role == ColumnRole::Y)
                .collect();
            let has = |c: &&stokesforge_core::Column| c.meta().x_prime.is_some_and(f64::is_finite);
            let free = tree.transpose_link(s).is_none() && !ys.is_empty();
            if free && ys.iter().all(has) {
                Some((s, true))
            } else if free && !ys.iter().any(has) {
                Some((s, false))
            } else {
                None
            }
        })
        .collect();
    // Index-mode sheets come last, so that the draws that picked the sheets with
    // an x' before #493 still pick them.
    let (with_x, without_x): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|(_, x)| *x);
    with_x
        .into_iter()
        .chain(without_x)
        .map(|(s, _)| s)
        .collect()
}

fn capture_roots(tree: &Tree, ids: &[NodeId]) -> Option<Vec<NodeCopy>> {
    NodeCopy::capture_all(tree, ids).ok()
}

/// Round trip through the object clipboard, as a paste would do.
fn through_clipboard(copies: &[NodeCopy], external: bool) -> Option<Vec<NodeCopy>> {
    let text = encode_objects("origin", copies).ok()?;
    let (_, decoded) = decode_objects(&text).ok()?;
    Some(if external {
        decoded.into_iter().map(NodeCopy::into_external).collect()
    } else {
        decoded
    })
}

fn command(step: &Step, tree: &Tree, rank: usize) -> Option<Command> {
    let all_sheets = sheets(tree);
    let column_at = |s: usize, c: usize| pick(&columns_of(tree, pick(&all_sheets, s)), c);
    let root = tree.root();
    match *step {
        Step::Enter {
            sheet,
            column,
            row,
            count,
            seed,
        } => column_at(sheet, column).map(|c| Command::SetCells {
            column: c,
            start: row,
            values: ValueBlock::Float(
                (0..count)
                    .map(|k| ((seed * 7 + k * 3) % 11) as f64 - 3.0)
                    .collect(),
            ),
            expected_version: None,
        }),
        Step::SetFormula {
            sheet,
            column,
            formula,
        } => {
            let c = column_at(sheet, column)?;
            // The keeping reading of the field (#433): what cannot be read is
            // stored as typed, a loop included; the binder turns it into a
            // formula when the sheet, the workbook or the column appears.
            let (f, _) = tree.formula_keeping(c, FORMULAS[formula]);
            Some(Command::SetColumnMeta {
                column: c,
                meta: ColumnMeta {
                    formula: Some(f),
                    ..Default::default()
                },
            })
        }
        Step::SetPending { sheet, column } => {
            let c = column_at(sheet, column)?;
            let next = columns_of(tree, pick(&all_sheets, sheet)).len();
            let letter = char::from(b'A' + u8::try_from(next).ok().filter(|n| *n < 26)?);
            let (f, _) = tree.formula_keeping(c, &format!("A+{letter}"));
            Some(Command::SetColumnMeta {
                column: c,
                meta: ColumnMeta {
                    formula: Some(f),
                    ..Default::default()
                },
            })
        }
        Step::AddColumn { sheet } => pick(&all_sheets, sheet).map(|sheet| Command::AddColumn {
            sheet,
            index: None,
            name: Some(format!("Added{rank}")),
            column: Column::from_f64(&[1.0, 2.0, 3.0]),
        }),
        Step::MoveColumn {
            sheet,
            column,
            target,
            index,
        } => {
            let id = column_at(sheet, column)?;
            let new_parent = pick(&all_sheets, target)?;
            Some(Command::Move {
                id,
                new_parent,
                index: index % (tree.children(new_parent).ok()?.len() + 1),
            })
        }
        Step::Rename { kind, idx, name } => {
            let id = match kind {
                0 => pick(&all_sheets, idx)?,
                1 => pick(&books(tree), idx)?,
                2 => pick(&folders(tree)[1..], idx)?,
                _ => column_at(idx, idx / 2)?,
            };
            let current = tree.node(id)?.name().to_string();
            let name = match name {
                0 => current.to_uppercase(),
                1 => current.to_lowercase(),
                n => NAMES[(n - 2) % NAMES.len()].to_string(),
            };
            Some(Command::Rename { id, name })
        }
        Step::MoveNode {
            kind,
            idx,
            target,
            index,
        } => match kind {
            0 => {
                let id = pick(&all_sheets, idx)?;
                let new_parent = pick(&books(tree), target)?;
                Some(Command::Move {
                    id,
                    new_parent,
                    index: index % (tree.children(new_parent).ok()?.len() + 1),
                })
            }
            1 => {
                let id = pick(&books(tree), idx)?;
                let new_parent = pick(&folders(tree), target)?;
                Some(Command::Move {
                    id,
                    new_parent,
                    index: index % (tree.children(new_parent).ok()?.len() + 1),
                })
            }
            _ => {
                let id = pick(&folders(tree)[1..], idx)?;
                let new_parent = pick(&folders(tree), target)?;
                Some(Command::Move {
                    id,
                    new_parent,
                    index: index % (tree.children(new_parent).ok()?.len() + 1),
                })
            }
        },
        Step::RemoveNode { kind, idx } => {
            let id = match kind {
                0 => pick(&all_sheets, idx)?,
                1 => pick(&books(tree), idx)?,
                _ => pick(&folders(tree)[1..], idx)?,
            };
            Some(Command::RemoveNode { id })
        }
        Step::Transpose { idx } => {
            pick(&transposable(tree), idx).map(|source| Command::TransposeRows {
                source,
                rows: vec![0, 1],
            })
        }
        Step::RecalculateTranspose { idx } => {
            let linked: Vec<NodeId> = tree.transposes().keys().copied().collect();
            pick(&linked, idx).map(|sheet| Command::RecalculateTranspose { sheet })
        }
        Step::ToggleAuto { idx } => {
            let linked: Vec<NodeId> = tree.transposes().keys().copied().collect();
            let sheet = pick(&linked, idx)?;
            let auto = !tree.transpose_link(sheet)?.auto_recalc;
            Some(Command::SetTransposeAuto { sheet, auto })
        }
        Step::EditSource { idx, column, seed } => {
            let linked: Vec<NodeId> = tree.transposes().keys().copied().collect();
            let source = tree.transpose_link(pick(&linked, idx)?)?.source;
            let ys: Vec<NodeId> = tree
                .children(source)
                .ok()?
                .iter()
                .copied()
                .filter(|id| {
                    tree.node(*id).and_then(|n| n.as_column()).is_some_and(|c| {
                        c.meta().formula.is_none() && c.meta().role == ColumnRole::Y
                    })
                })
                .collect();
            Some(Command::SetCells {
                column: pick(&ys, column)?,
                start: seed % 2,
                values: ValueBlock::Float(vec![seed as f64 + 0.25]),
                expected_version: None,
            })
        }
        Step::TypeInRow { idx, column, seed } => {
            let linked: Vec<NodeId> = tree.transposes().keys().copied().collect();
            let link = tree.transpose_link(pick(&linked, idx)?)?;
            let mut columns = vec![link.x_column];
            columns.extend(link.rows.iter().map(|r| r.column));
            let column = pick(&columns, column)?;
            Some(Command::SetCells {
                column,
                start: 0,
                values: ValueBlock::Float(vec![seed as f64 + 0.5]),
                expected_version: None,
            })
        }
        Step::Paste {
            kind,
            idx,
            external,
            target,
        } => {
            let id = if kind == 0 {
                pick(&books(tree), idx)?
            } else {
                pick(&folders(tree)[1..], idx)?
            };
            let copy = through_clipboard(&capture_roots(tree, &[id])?, external)?
                .into_iter()
                .next()?;
            let parent = pick(&folders(tree), target)?;
            Some(Command::InsertCopies {
                items: vec![CopyInsertion {
                    parent,
                    copy,
                    name: format!("Pasted{rank}"),
                }],
            })
        }
        Step::PasteMany { a, b } => {
            let mut roots: Vec<NodeId> = books(tree);
            roots.extend(&folders(tree)[1..]);
            let (x, y) = (pick(&roots, a)?, pick(&roots, b)?);
            if x == y || is_ancestor(tree, x, y) || is_ancestor(tree, y, x) {
                return None;
            }
            let copies = capture_roots(tree, &[x, y])?;
            let items = copies
                .into_iter()
                .enumerate()
                .map(|(k, copy)| CopyInsertion {
                    parent: root,
                    copy,
                    name: format!("Many{rank}x{k}"),
                })
                .collect();
            Some(Command::InsertCopies { items })
        }
        Step::SetType {
            sheet,
            column,
            kind,
        } => column_at(sheet, column).map(|c| Command::SetColumnType {
            column: c,
            column_type: [ColumnType::Numeric, ColumnType::Mixed, ColumnType::DateTime][kind],
        }),
        Step::DuplicateSheet { sheet } => {
            pick(&all_sheets, sheet).map(|s| Command::DuplicateWorksheet {
                sheet: s,
                name: format!("Dup{rank}"),
            })
        }
        Step::DeleteColumn { sheet, column } => {
            column_at(sheet, column).map(|c| Command::DeleteColumn { column: c })
        }
        Step::Unrelated => Some(Command::AddFolder {
            parent: root,
            name: format!("Witness{rank}"),
        }),
        Step::Undo | Step::Redo | Step::UndoThenRedo { .. } | Step::Reload => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reply {
    Rejected,
    AcceptedWithoutPass,
    AcceptedWithPass,
}

fn reply<T, E>(result: Result<T, E>, ran_pass: impl FnOnce(&T) -> bool) -> Reply {
    match result {
        Err(_) => Reply::Rejected,
        Ok(v) if ran_pass(&v) => Reply::AcceptedWithPass,
        Ok(_) => Reply::AcceptedWithoutPass,
    }
}

fn play(
    handle: &SessionHandle,
    step: &Step,
    command: Option<&Command>,
    dir: &TempDir,
    name: &str,
    saves: &mut usize,
) -> Reply {
    match step {
        Step::Undo => reply(handle.undo(), Option::is_some),
        Step::Redo => reply(handle.redo(), Option::is_some),
        Step::UndoThenRedo { k } => {
            let mut replies = Vec::new();
            for _ in 0..*k {
                replies.push(reply(handle.undo(), Option::is_some));
            }
            for _ in 0..*k {
                replies.push(reply(handle.redo(), Option::is_some));
            }
            replies.into_iter().max().unwrap_or(Reply::Rejected)
        }
        Step::Reload => {
            *saves += 1;
            let path = dir.path().join(format!("{name}-{saves}.stf"));
            if handle.save(&path).is_err() {
                return Reply::Rejected;
            }
            reply(handle.load(&path), |_| true)
        }
        _ => match command {
            Some(c) => reply(handle.execute(c.clone()), |_| true),
            None => Reply::Rejected,
        },
    }
}

/// The independent evaluation: the live project saved, loaded in a fresh full
/// session, calculated from the file by one command. `None` if the project
/// cannot be saved or loaded (a refusal both live sessions share).
fn fresh_evaluation(handle: &SessionHandle, dir: &TempDir, n: usize) -> Option<Snapshot> {
    let path: PathBuf = dir.path().join(format!("oracle-{n}.stf"));
    handle.save(&path).ok()?;
    let project = stokesforge_persistence::load(&path).ok()?;
    let session = Session::spawn_with_recalc(project, None, FormulaRecalc::Full).ok()?;
    let fresh = session.handle();
    let root = fresh.snapshot().ok()?.tree.root();
    fresh
        .execute(Command::AddFolder {
            parent: root,
            name: "~oracle".into(),
        })
        .ok()?;
    fresh.snapshot().ok()
}

fn compare_formulas(live: &Tree, fresh: &Tree) -> Result<(), String> {
    for (id, node) in live.iter() {
        let Some(c) = node.as_column() else { continue };
        if c.meta().formula.is_none() {
            continue;
        }
        let Some(o) = fresh.node(id).and_then(|n| n.as_column()) else {
            return Err(format!("{id} is missing from the fresh evaluation"));
        };
        if c.values() != o.values() {
            return Err(format!(
                "{id} ({:?}): live {:?} vs fresh {:?}",
                live.formula_letters(id),
                c.values(),
                o.values()
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Default, Clone)]
struct Coverage {
    accepted: usize,
    computed: usize,
    ref_cells: usize,
    checked: usize,
    /// Kept formulas the binder turned into formulas (#433), on the targeted
    /// session at the end of each sequence.
    bindings: usize,
    /// Accepted reloads over a tree holding a link that is up to date over a
    /// formula column (#439): the first pass must not make it stale.
    reload_fresh_formula_link: usize,
    /// Accepted `Transpose` / `RecalculateTranspose` steps that changed nothing
    /// and stored no history entry, with such a link in the tree (#439).
    noop_over_formula_link: usize,
    /// Steps after which a followed transposed sheet caught up with its source
    /// by itself (#463): its link re-read the source with no gesture on it.
    follows: usize,
    /// Steps after which a followed sheet held a typed value and stood still.
    typed_stops: usize,
    per_step: BTreeMap<&'static str, usize>,
}

impl Coverage {
    fn add(&mut self, other: Coverage) {
        self.accepted += other.accepted;
        self.computed += other.computed;
        self.ref_cells += other.ref_cells;
        self.checked += other.checked;
        self.bindings += other.bindings;
        self.reload_fresh_formula_link += other.reload_fresh_formula_link;
        self.noop_over_formula_link += other.noop_over_formula_link;
        self.follows += other.follows;
        self.typed_stops += other.typed_stops;
        for (k, v) in other.per_step {
            *self.per_step.entry(k).or_default() += v;
        }
    }

    fn tally(&mut self, tree: &Tree) {
        for (id, node) in tree.iter() {
            let Some(c) = node.as_column() else { continue };
            if c.meta().formula.is_none() || c.is_empty() {
                continue;
            }
            let computed = (0..c.len()).any(|r| match c.get(r) {
                Cell::Float(v) => !v.is_nan(),
                Cell::Text(Some(t)) => t != "---",
                Cell::Text(None) => false,
            });
            self.computed += usize::from(computed);
            self.ref_cells += usize::from(tree.formula_has_deleted_reference(id));
        }
    }
}

/// True if `tree` holds a link that is up to date and reads a formula column.
fn has_fresh_formula_link(tree: &Tree) -> bool {
    tree.transposes().values().any(|link| {
        !link.state(tree).stale
            && link.inputs.iter().any(|input| {
                tree.node(input.column)
                    .and_then(|n| n.as_column())
                    .is_some_and(|c| c.meta().formula.is_some())
            })
    })
}

fn run_sequence(draw: &[Step]) -> Result<Coverage, TestCaseError> {
    let mut coverage = Coverage::default();
    let (targeted, full) = (
        Twin::new(FormulaRecalc::Targeted),
        Twin::new(FormulaRecalc::Full),
    );
    let dir = TempDir::new().expect("temporary directory");
    let (mut saves_t, mut saves_f) = (0usize, 0usize);
    for (rank, step) in draw.iter().enumerate() {
        let before = targeted.snapshot();
        let cmd = command(step, &before.tree, rank);
        let rt = play(
            &targeted.handle,
            step,
            cmd.as_ref(),
            &dir,
            "t",
            &mut saves_t,
        );
        let rf = play(&full.handle, step, cmd.as_ref(), &dir, "f", &mut saves_f);
        prop_assert_eq!(rt, rf, "step {} ({:?}): replies differ", rank, step);
        prop_assert!(
            targeted.handle.actor_alive() && full.handle.actor_alive(),
            "step {rank} ({step:?}): the actor died"
        );
        coverage.accepted += usize::from(rt != Reply::Rejected);
        let (st, sf) = (targeted.snapshot(), full.snapshot());
        // 0. a reload keeps every link's state on the targeted side: a formula
        // column recomputed to the same content keeps its version, so a link
        // that was up to date stays so (#430, #439); a stale or modified one
        // stays so.
        if matches!(step, Step::Reload) && rt != Reply::Rejected {
            coverage.reload_fresh_formula_link += usize::from(has_fresh_formula_link(&before.tree));
            prop_assert_eq!(
                link_states(&before.tree),
                link_states(&st.tree),
                "step {} ({:?}): a reload changed the state of a link",
                rank,
                step
            );
        }
        // 0b. fixed point of the binder (#433): after every step, undo, redo
        // and reload included, no kept formula can be read any more.
        for (tree, which) in [(&st.tree, "targeted"), (&sf.tree, "full")] {
            prop_assert!(
                tree.pending_bindings().is_empty(),
                "step {rank} ({step:?}), {which} session: a kept formula could be bound: {:?}",
                tree.pending_bindings()
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>()
            );
        }
        // The gestures that must find nothing to do over an up-to-date link.
        if matches!(
            step,
            Step::Transpose { .. } | Step::RecalculateTranspose { .. }
        ) && rt != Reply::Rejected
            && has_fresh_formula_link(&before.tree)
            && before.tree == st.tree
            && before.history == st.history
        {
            coverage.noop_over_formula_link += 1;
        }
        // 0c. what following did (#463), counted on the targeted side.
        for (sheet, link) in st.tree.transposes() {
            let Some(old) = before.tree.transpose_link(*sheet) else {
                continue;
            };
            let new_state = link.state(&st.tree);
            if link.auto_recalc
                && old.auto_recalc
                && link.inputs != old.inputs
                && !new_state.stale
                && !matches!(
                    step,
                    Step::Transpose { .. }
                        | Step::RecalculateTranspose { .. }
                        | Step::Undo
                        | Step::Redo
                        | Step::UndoThenRedo { .. }
                        | Step::Reload
                )
            {
                coverage.follows += 1;
            }
            if link.auto_recalc && new_state.modified && new_state.stale {
                coverage.typed_stops += 1;
            }
        }
        // 1. twin oracle: the trees are equal, transposed links and the version
        // of every column included — the twins number alike (#439).
        prop_assert!(
            st.tree == sf.tree,
            "step {rank} ({step:?}): targeted and full trees differ"
        );
        // A command that stores an undo entry in one twin and not in the other only
        // shows as a tree difference at a later Undo; compare histories to catch it
        // at its step (#439).
        prop_assert_eq!(
            &st.history,
            &sf.history,
            "step {} ({:?}): targeted and full histories differ",
            rank,
            step
        );
        // 2. independent re-evaluation, after each accepted step with a pass.
        if rt == Reply::AcceptedWithPass {
            coverage.tally(&st.tree);
            *coverage.per_step.entry(step_name(step)).or_default() += 1;
            if let Some(fresh) = fresh_evaluation(&targeted.handle, &dir, rank) {
                coverage.checked += 1;
                if let Err(why) = compare_formulas(&st.tree, &fresh.tree) {
                    prop_assert!(
                        false,
                        "step {rank} ({step:?}): live value differs from a fresh evaluation: {why}"
                    );
                }
            }
        }
    }
    coverage.bindings = targeted.snapshot().bound.count as usize;
    Ok(coverage)
}

#[test]
fn b23_formulas_stay_exact_across_structural_gestures() {
    let cases = 96 * common::env_scale();
    let total = std::cell::RefCell::new(Coverage::default());
    let started = std::time::Instant::now();
    runner(cases)
        .run(&prop::collection::vec(step(), 30..60), |draw| {
            let c = run_sequence(&draw)?;
            total.borrow_mut().add(c);
            Ok(())
        })
        .unwrap_or_else(|e| {
            panic!(
                "targeted == full == fresh evaluation at every step: {e}; {}",
                common::seed_note()
            )
        });
    let total = total.into_inner();
    eprintln!("b23: {cases} cases in {:?}: {total:?}", started.elapsed());
    assert!(total.computed > 500, "{total:?}");
    assert!(total.ref_cells > 20, "{total:?}");
    assert!(total.checked > 500, "{total:?}");
    if cases >= 64 {
        // The binder (#433) bound kept formulas.
        assert!(total.bindings > 10, "{total:?}");
        // The draw reaches what #439 is about: a reload over an up-to-date link
        // over a formula column, and a gesture over it that stores no entry.
        assert!(total.reload_fresh_formula_link >= 1, "{total:?}");
        assert!(total.noop_over_formula_link >= 1, "{total:?}");
        // The draw reaches #463: sheets that follow their source by themselves,
        // and a typed value that stops one.
        assert!(total.follows >= 20, "{total:?}");
        assert!(total.typed_stops >= 1, "{total:?}");
    }
    if cases >= 64 {
        // Every structural gesture was accepted, with a pass, at least a few times.
        for gesture in [
            "RenameSheet",
            "RenameWorkbook",
            "RenameFolder",
            "RenameColumn",
            "MoveSheet",
            "MoveWorkbook",
            "MoveFolder",
            "RemoveSheet",
            "RemoveWorkbook",
            "RemoveFolder",
            "Transpose",
            "PasteSame",
            "PasteExternal",
            "SetType",
            "Undo",
            "Redo",
            "Reload",
        ] {
            let n = total.per_step.get(gesture).copied().unwrap_or(0);
            assert!(n >= 3, "{gesture}: only {n} accepted passes — {total:?}");
        }
    }
}

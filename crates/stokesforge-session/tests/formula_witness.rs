//! Golden test of the formula families on the V1 `.stf` witness (#412).
//!
//! The witness file (`stokesforge-persistence/tests/witnesses/v1/project.stf`)
//! holds the sheet `Formules`: one formula column per family of the grammar.
//! Loading it, recalculating it (in `Targeted` and in `Full` mode) must give
//! the **literal** formula texts and the **literal** values below, computed by
//! hand. The data of the sheet are `A` (X) = 0 1 2 3, `B` = 1 2 4 8, `C` = 3 5 7 9.
//!
//! Left out on purpose, because it is an open bug: `round` on a tie (#428)
//! (`at` on a grid point, #427, `logx` of powers, `erf` and `mean` of 0.1s,
//! #429, are pinned elsewhere). Regenerating the witness changes the formula texts, not the values
//! here: this test then fails until the table is edited by hand.

use std::path::PathBuf;

use stokesforge_commands::Command;
use stokesforge_core::{Cell, NodeId, Tree};
use stokesforge_session::{FormulaRecalc, Session, Snapshot};

/// Name, formula text in letters, expected values (one per row).
const EXPECTED: [(&str, &str, [f64; 4]); 14] = [
    ("Operators", "B*2+C-A/2", [5.0, 8.5, 14.0, 23.5]),
    ("Power", "2^3^2+A^2", [512.0, 513.0, 516.0, 521.0]),
    (
        "Comparisons",
        "(B>2)+(C<=7)*10+(A==1)*100+(A!=1)*1000+(A>=2)*10000+(A<1)*100000",
        [101010.0, 110.0, 11011.0, 11001.0],
    ),
    (
        "If",
        "if(A<B, A, B)+if(C>6, 100, 0)",
        [0.0, 1.0, 102.0, 103.0],
    ),
    (
        "RowFunctions1",
        "sqrt(B*B)+abs(A-2)+sign(A-1)",
        [2.0, 3.0, 5.0, 10.0],
    ),
    (
        "RowFunctions2",
        "floor(C/4)+ceil(C/4)+round(C/4, 0)+mod(C, 4)+pow(2, A)+min(A, 1)+max(B, 3)+exp(A*0)+ln(B/B)+cos(A*0)",
        [11.0, 13.0, 19.0, 27.0],
    ),
    ("At", "at(B, 1.5)", [3.0; 4]),
    ("MaxRange", "max(B, 0.5, 2.5)", [4.0; 4]),
    ("MinAll", "min(B)", [1.0; 4]),
    ("MeanRange", "mean(B, 0.5, 2.5)", [3.0; 4]),
    ("SumAll", "sum(C)", [24.0; 4]),
    ("StdRange", "std(C, -1, 2.5)", [2.0; 4]),
    ("IntegralAll", "integral(B)", [10.5; 4]),
    ("IntegralRange", "integral(B, 0.5, 2.5)", [6.375; 4]),
];

fn witness_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../stokesforge-persistence/tests/witnesses/v1/project.stf")
}

fn column_named(tree: &Tree, sheet: NodeId, name: &str) -> NodeId {
    *tree
        .children(sheet)
        .expect("columns")
        .iter()
        .find(|id| tree.node(**id).expect("node").name() == name)
        .unwrap_or_else(|| panic!("column {name} in the witness"))
}

fn formulas_sheet(tree: &Tree) -> NodeId {
    tree.iter()
        .find(|(_, n)| n.name() == "Formules" && n.as_column().is_none())
        .map(|(id, _)| id)
        .expect("sheet Formules in the witness")
}

fn evaluated(mode: FormulaRecalc) -> Snapshot {
    let project = stokesforge_persistence::load(&witness_path()).expect("the V1 witness loads");
    let session = Session::spawn_with_recalc(project, None, mode).expect("actor");
    let handle = session.handle();
    // A fresh session does not recalculate: one command does.
    let root = handle.snapshot().expect("snapshot").tree.root();
    handle
        .execute(Command::AddFolder {
            parent: root,
            name: "Pass".into(),
        })
        .expect("pass");
    let snapshot = handle.snapshot().expect("snapshot");
    drop(session);
    snapshot
}

fn values(tree: &Tree, column: NodeId) -> Vec<f64> {
    let column = tree
        .node(column)
        .and_then(|n| n.as_column())
        .expect("column");
    (0..column.len())
        .map(|row| match column.get(row) {
            Cell::Float(v) => v,
            Cell::Text(t) => panic!("text cell {t:?}"),
        })
        .collect()
}

#[test]
fn the_witness_formulas_keep_their_text_and_compute_the_hand_values() {
    for mode in [FormulaRecalc::Targeted, FormulaRecalc::Full] {
        let snapshot = evaluated(mode);
        let tree = &snapshot.tree;
        let sheet = formulas_sheet(tree);
        for (name, text, expected) in EXPECTED {
            let id = column_named(tree, sheet, name);
            assert_eq!(
                tree.formula_letters(id).as_deref(),
                Some(text),
                "{mode:?} {name}: formula text"
            );
            assert_eq!(values(tree, id), expected, "{mode:?} {name}: values");
        }
    }
}

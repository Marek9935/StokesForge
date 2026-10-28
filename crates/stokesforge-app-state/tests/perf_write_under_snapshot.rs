//! Measures the cost of a cell input when the interface **holds** a snapshot
//! between each keystroke, compared with the case where it does not.
//!
//! # This test only makes sense in the release profile
//!
//! It times a memory copy over a million `f64`; in the debug profile, the
//! times are dominated by the lack of optimization of the generated code and
//! say nothing about the real cost of the architecture. Run it with:
//!
//! ```text
//! cargo test -p stokesforge-app-state --release --test perf_write_under_snapshot -- --ignored --nocapture
//! ```
//!
//! The test is marked `#[ignore]`: it allocates an 8 MB column and takes
//! several hundred milliseconds, which has no place in an ordinary
//! `cargo test`.
//!
//! # What is measured, and why
//!
//! A column lives in the model as an `Arc<Vec<f64>>`, mutated by
//! `Arc::make_mut` (`stokesforge_core::column`, `stokesforge_core::tree`). As
//! long as the `Arc` has a single owner, `Arc::make_mut` writes in place; as
//! soon as it has two, it clones the `Vec` before writing. Now `SessionHandle::
//! snapshot` (`stokesforge_session::actor::build_snapshot`) returns the current
//! tree through a plain `Arc::clone` — and the interface **permanently keeps**
//! the last snapshot received, to paint without ever locking the actor.
//! So, in real `AppState` use, at the moment a `SetCells` command reaches the
//! actor, the previous snapshot (the one the UI has just painted) is still
//! alive on the client side: the column is shared, and the write must copy
//! the whole buffer before modifying a single cell.
//!
//! This cost is bounded (one column, not the whole model) and does not
//! accumulate in memory (the old buffer is freed as soon as the last snapshot
//! that references it is dropped). But it must be measured, not assumed: that
//! is what this test does, by comparing two scenarios that run only the
//! *strictly* necessary to compile the same number of commands and
//! refreshes — only the lifetime of the intermediate snapshot changes.
//!
//! # What the measurement showed, and which shifts the expected conclusion
//!
//! The two scenarios give **almost identical** averages. This is not a bug in
//! the test: `History::execute` (`stokesforge-commands/src/
//! history.rs`) clones `project.tree()` (`let before = Arc::clone(project.
//! tree());`) **before applying each command**, to compute its label and be
//! able to restore the tree on failure. This local `before` aliases the tree
//! for the duration of the application, which triggers the same cascade of
//! copy-on-write down to the column's `Vec` — **whether or not the caller
//! holds a snapshot**. Verified by address comparison (`Arc::as_ptr`): even a
//! series of writes where no client ever asks for a snapshot reallocates the
//! buffer on every command.
//!
//! Consequence for the architecture documentation: copy-on-write is thus not
//! a cost the UI could avoid by releasing its snapshot earlier — it is already
//! paid by `History::execute` for *every* client, including a future Python
//! bridge that would display nothing. Reducing this cost would require
//! changing `History::execute` itself (for example cloning `project.tree()`
//! only on demand, or only for "structure" commands, which really need it for
//! their `Arc<Tree>` restoration — see the doc of `history.rs`).

use std::time::{Duration, Instant};

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_session::{Session, SessionHandle, Snapshot};

/// Length of the measured column: a million `f64`, i.e. 8 MB — big enough
/// for a full copy to stand out clearly from the measurement noise.
const COLUMN_LEN: usize = 1_000_000;
/// Number of writes averaged per scenario.
const ITERATIONS: usize = 100;

/// Builds a single-sheet project carrying a `Large` column of [`COLUMN_LEN`]
/// values, then starts its session. Returns the [`Session`] (to be kept
/// alive: dropping it ends the actor) and the identifier of the measured
/// column.
fn spawn_session_with_large_column() -> (Session, NodeId) {
    let mut project = Project::new("Perf").expect("test project");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 readable")
        .expect("Sheet1 created with the workbook");
    let values = vec![0.0_f64; COLUMN_LEN];
    let column = project
        .add_column(sheet, None, Some("Large"), Column::from_f64(&values))
        .expect("Large column");
    let session = Session::spawn(project, None).expect("session thread");
    (session, column)
}

/// Command that writes a single cell at row `row` of `column`.
fn set_one_cell(column: NodeId, row: usize) -> Command {
    Command::SetCells {
        column,
        start: row,
        values: ValueBlock::Float(vec![row as f64]),
        expected_version: None,
    }
}

/// Real `AppState` scenario: the last snapshot received is **kept alive**
/// until the next refresh. On each turn, the snapshot held since the previous
/// turn still aliases the actor's tree when the command reaches it, then it is
/// replaced by the new snapshot, which will play the same role on the next
/// turn.
fn measure_writes_holding_snapshot(handle: &SessionHandle, column: NodeId) -> Duration {
    let mut held: Snapshot = handle.snapshot().expect("initial snapshot");
    let mut total = Duration::ZERO;
    for row in 0..ITERATIONS {
        let start = Instant::now();
        handle
            .execute(set_one_cell(column, row))
            .expect("cell write");
        // Replaces `held`: the old snapshot is dropped only here, after the
        // command has already had to deal with it.
        held = handle.snapshot().expect("refreshed snapshot");
        total += start.elapsed();
    }
    drop(held);
    total
}

/// Same series of writes, but the snapshot is released **as soon as it is
/// received**: no client aliases the column any more when the next command is
/// sent (see however the note at the top of the file: this no longer suffices
/// to avoid the copy, since `History::execute` aliases one itself for the
/// duration of each application).
fn measure_writes_releasing_snapshot(handle: &SessionHandle, column: NodeId) -> Duration {
    let mut total = Duration::ZERO;
    for row in 0..ITERATIONS {
        let start = Instant::now();
        handle
            .execute(set_one_cell(column, row))
            .expect("cell write");
        let snapshot = handle.snapshot().expect("snapshot");
        drop(snapshot);
        total += start.elapsed();
    }
    total
}

#[test]
#[ignore = "costly (a column of a million f64) and only meaningful in the release profile — see this file's doc"]
fn measures_cell_write_cost_with_and_without_a_held_snapshot() {
    let (session_held, column_held) = spawn_session_with_large_column();
    let handle_held = session_held.handle();
    let held_total = measure_writes_holding_snapshot(&handle_held, column_held);

    let (session_released, column_released) = spawn_session_with_large_column();
    let handle_released = session_released.handle();
    let released_total = measure_writes_releasing_snapshot(&handle_released, column_released);

    let held_avg_ms = held_total.as_secs_f64() * 1_000.0 / ITERATIONS as f64;
    let released_avg_ms = released_total.as_secs_f64() * 1_000.0 / ITERATIONS as f64;

    println!();
    println!("Average cost of a cell write under {COLUMN_LEN} rows ({ITERATIONS} writes):");
    println!("  snapshot held between writes     : {held_avg_ms:>9.4} ms");
    println!("  snapshot released as soon as received : {released_avg_ms:>9.4} ms");

    // Both scenarios must have produced the same final state: otherwise the
    // measurement does not compare the same thing twice.
    let final_held = handle_held.snapshot().expect("final snapshot (held)");
    let final_released = handle_released
        .snapshot()
        .expect("final snapshot (released)");
    assert_eq!(
        *final_held.tree, *final_released.tree,
        "both scenarios must produce the same final tree"
    );

    assert!(
        session_held.shutdown().is_ok(),
        "clean session shutdown (held)"
    );
    assert!(
        session_released.shutdown().is_ok(),
        "clean session shutdown (released)"
    );
}

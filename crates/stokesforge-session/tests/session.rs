//! Integration tests of `stokesforge-session`, through the public API only
//! (`Session`, `SessionHandle`, `Snapshot`).

use std::sync::Arc;
use std::thread;

use stokesforge_commands::{
    ColumnRef, Command, CommandError, HistoryLabel, MacroKind, DEFAULT_LIMIT,
};
use stokesforge_core::{Cell, Column, CoreError, NodeId, Project, ValueBlock};
use stokesforge_persistence::PersistenceError;
use stokesforge_session::{Session, SessionError, Snapshot};

/// Builds the fixture project: `Book1` → `Sheet1` → column `A [1,2,3]`.
/// Returns `(project, sheet, a)`.
fn fixture() -> (Project, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("sheet resolved");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column created");
    (project, sheet, a)
}

/// Reads column `a` of a snapshot as a `Vec<f64>`.
fn read_a(snapshot: &Snapshot, a: NodeId) -> Vec<f64> {
    let node = snapshot.tree.node(a).expect("column present");
    let column = node.as_column().expect("the node is a column");
    (0..column.len())
        .map(|row| match column.get(row) {
            Cell::Float(value) => value,
            Cell::Text(_) => unreachable!("fixture column is always numeric"),
        })
        .collect()
}

/// Command "write a single float value at `start` in `column`".
fn set_one(column: NodeId, start: usize, value: f64) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(vec![value]),
        expected_version: None,
    }
}

#[test]
fn new_project_clears_history_and_geometries_and_changes_identity() {
    let (project, _, a) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.execute(set_one(a, 0, 42.0)).unwrap();
    handle
        .set_window_geometry(
            a,
            stokesforge_session::WindowGeometry {
                x: 1.0,
                y: 2.0,
                width: 300.0,
                height: 200.0,
                view: None,
            },
        )
        .unwrap();
    let before = handle.snapshot().unwrap();
    handle.new_project().unwrap();
    let after = handle.snapshot().unwrap();
    assert_eq!(after.tree.len(), 1);
    assert!(!after.history.can_undo);
    assert!(!after.history.can_redo);
    assert!(after.window_geometries.is_empty());
    assert_eq!(after.epoch, before.epoch + 1);
    assert!(after.revision > before.revision);
    assert!(after.geometry_revision > before.geometry_revision);
    assert!(handle.undo().unwrap().is_none());
    session.shutdown().unwrap();
}

#[test]
fn new_project_refuses_a_macro_even_a_foreign_one() {
    let (project, _, a) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.begin_macro(MacroKind::Script).unwrap();
    handle.execute(set_one(a, 0, 42.0)).unwrap();
    let before = handle.snapshot().unwrap();
    assert!(matches!(
        handle.with_new_identity().new_project(),
        Err(SessionError::Command(CommandError::MacroOpen(_)))
    ));
    let after = handle.snapshot().unwrap();
    assert_eq!(after.epoch, before.epoch);
    assert_eq!(after.revision, before.revision);
    assert_eq!(read_a(&after, a)[0], 42.0);
    handle.abort_macro().unwrap();
    session.shutdown().unwrap();
}

#[test]
fn round_trip_execute_undo_redo() {
    let (project, _sheet, a) = fixture();
    let initial_len = project.tree().len();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let snap0 = handle.snapshot().expect("snapshot");
    assert_eq!(snap0.revision, 0);
    assert_eq!(snap0.tree.len(), initial_len);
    assert!(!snap0.history.can_undo);

    let rev = handle
        .execute(set_one(a, 0, 42.0))
        .expect("execution succeeded");
    assert_eq!(rev, 1);

    let snap1 = handle.snapshot().expect("snapshot");
    assert_eq!(snap1.revision, 1);
    assert_eq!(read_a(&snap1, a)[0], 42.0);
    assert!(snap1.history.can_undo);
    assert_eq!(
        snap1.history.undo_label,
        Some(HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );

    let after_undo = handle.undo().expect("undo succeeded");
    assert_eq!(after_undo, Some(2));
    let snap2 = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&snap2, a)[0], 1.0);
    assert!(snap2.history.can_redo);

    let after_redo = handle.redo().expect("redo succeeded");
    assert_eq!(after_redo, Some(3));

    let first_undo = handle.undo().expect("undo succeeded");
    assert_eq!(first_undo, Some(4));
    let second_undo = handle
        .undo()
        .expect("undo without effect, but not an error");
    assert_eq!(second_undo, None);

    // The revision did not move on the second undo (nothing to undo).
    let snap_final = handle.snapshot().expect("snapshot");
    assert_eq!(snap_final.revision, 4);

    session.shutdown().expect("clean shutdown");
}

#[test]
fn error_leaves_state_unchanged() {
    let (project, sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let before = handle.snapshot().expect("snapshot").revision;

    let unknown = NodeId::from_raw(9999);
    let err = handle
        .execute(set_one(unknown, 0, 1.0))
        .expect_err("unknown node must fail");
    assert!(matches!(
        err,
        SessionError::Command(CommandError::Core(CoreError::UnknownNode(_)))
    ));
    assert_eq!(handle.snapshot().expect("snapshot").revision, before);

    let dup = handle
        .execute(Command::AddColumn {
            sheet,
            index: None,
            name: Some("A".to_string()),
            column: Column::from_f64(&[9.0]),
        })
        .expect_err("name already taken must fail");
    assert!(matches!(
        dup,
        SessionError::Command(CommandError::Core(CoreError::DuplicateName(_)))
    ));
    assert_eq!(handle.snapshot().expect("snapshot").revision, before);

    session.shutdown().expect("clean shutdown");
}

#[test]
fn snapshots_are_immutable_and_independent() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let s0 = handle.snapshot().expect("snapshot");
    handle
        .execute(set_one(a, 0, 7.0))
        .expect("execution succeeded");
    let s1 = handle.snapshot().expect("snapshot");

    assert_eq!(read_a(&s0, a)[0], 1.0);
    assert_eq!(read_a(&s1, a)[0], 7.0);
    assert!(!Arc::ptr_eq(&s0.tree, &s1.tree));

    session.shutdown().expect("clean shutdown");
}

#[test]
fn macros_group_and_undo_atomically() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let begin_rev = handle
        .begin_macro(MacroKind::Import)
        .expect("macro opening");
    assert_eq!(begin_rev, 1);
    assert!(handle.snapshot().expect("snapshot").history.in_macro);

    handle
        .execute(set_one(a, 0, 10.0))
        .expect("execution succeeded");
    handle
        .execute(set_one(a, 1, 20.0))
        .expect("execution succeeded");

    handle.end_macro().expect("macro closing");

    let snap = handle.snapshot().expect("snapshot");
    assert!(!snap.history.in_macro);
    assert_eq!(
        snap.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Import))
    );

    handle.undo().expect("undo of the macro");
    let after = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&after, a), vec![1.0, 2.0, 3.0]);

    let err = handle.end_macro().expect_err("outside a macro must fail");
    assert!(matches!(
        err,
        SessionError::Command(CommandError::NotInMacro)
    ));

    handle
        .begin_macro(MacroKind::PasteCells)
        .expect("opening of a new macro");
    let err = handle.undo().expect_err("undo during a macro must fail");
    assert!(matches!(
        err,
        SessionError::Command(CommandError::MacroOpen(_))
    ));

    let before_abort = handle.snapshot().expect("snapshot");
    handle
        .execute(set_one(a, 0, 99.0))
        .expect("execution succeeded in the macro");
    handle.abort_macro().expect("abort of the macro");

    let after_abort = handle.snapshot().expect("snapshot");
    assert!(!after_abort.history.in_macro);
    assert_eq!(read_a(&after_abort, a), read_a(&before_abort, a));

    session.shutdown().expect("clean shutdown");
}

#[test]
fn two_clients_share_one_history_stack() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle1 = session.handle();
    let handle2 = session.handle();

    let t1 = thread::spawn(move || {
        handle1
            .execute(set_one(a, 0, 10.0))
            .expect("execution succeeded (client 1)")
    });
    let t2 = thread::spawn(move || {
        handle2
            .execute(set_one(a, 1, 20.0))
            .expect("execution succeeded (client 2)")
    });
    t1.join().expect("thread 1 ended without panic");
    t2.join().expect("thread 2 ended without panic");

    let handle = session.handle();
    let both_done = handle.snapshot().expect("snapshot");
    assert_eq!(
        both_done.history.undo_label,
        Some(HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );

    // The first undo undoes the last write to arrive, whichever it is: only
    // one of the two modifications must remain.
    handle.undo().expect("first undo");
    let mid = handle.snapshot().expect("snapshot");
    let mid_values = read_a(&mid, a);
    assert!(
        mid_values == vec![10.0, 2.0, 3.0] || mid_values == vec![1.0, 20.0, 3.0],
        "unexpected intermediate state: {mid_values:?}"
    );

    handle.undo().expect("second undo");
    let restored = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&restored, a), vec![1.0, 2.0, 3.0]);

    session.shutdown().expect("clean shutdown");
}

#[test]
fn concurrent_executes_yield_distinct_contiguous_revisions() {
    let (project, _sheet, a) = fixture();
    // Unlimited limit (0): the 200 concurrent writes must all remain undoable
    // afterwards.
    let session = Session::spawn(project, Some(0)).expect("actor thread started");

    let workers: Vec<_> = (0..4)
        .map(|_| {
            let handle = session.handle();
            thread::spawn(move || {
                (0..50)
                    .map(|k| {
                        handle
                            .execute(set_one(a, 0, k as f64))
                            .expect("execution succeeded")
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    let mut revisions: Vec<u64> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("thread ended without panic"))
        .collect();
    revisions.sort_unstable();
    let expected: Vec<u64> = (1..=200).collect();
    assert_eq!(revisions, expected, "revisions not distinct or with a gap");

    let handle = session.handle();
    let snap = handle.snapshot().expect("snapshot");
    assert_eq!(snap.revision, 200);
    assert!(snap.history.can_undo);

    let mut undone = 0usize;
    while handle.undo().expect("undo without error").is_some() {
        undone += 1;
    }
    assert_eq!(undone, 200, "the 200 writes must all be undone");
    let after = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&after, a), vec![1.0, 2.0, 3.0]);

    session.shutdown().expect("clean shutdown");
}

#[test]
fn history_limit_evicts_oldest_entries() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, Some(3)).expect("actor thread started");
    let handle = session.handle();

    for k in 0..5 {
        handle
            .execute(set_one(a, 0, k as f64))
            .expect("execution succeeded");
    }

    assert!(handle.undo().expect("undo 1/3").is_some());
    assert!(handle.undo().expect("undo 2/3").is_some());
    assert!(handle.undo().expect("undo 3/3").is_some());
    assert!(
        handle.undo().expect("undo beyond the limit").is_none(),
        "the fourth entry should have been evicted"
    );

    session.shutdown().expect("clean shutdown");
}

#[test]
fn default_history_limit_matches_the_commands_crate_default() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    for k in 0..(DEFAULT_LIMIT + 50) {
        handle
            .execute(set_one(a, 0, k as f64))
            .expect("execution succeeded");
    }

    for i in 0..DEFAULT_LIMIT {
        assert!(
            handle
                .undo()
                .expect("undo within the default limit")
                .is_some(),
            "undo {i} unexpectedly empty"
        );
    }
    assert!(
        handle
            .undo()
            .expect("undo beyond the default limit")
            .is_none(),
        "the entry beyond DEFAULT_LIMIT should have been evicted"
    );

    session.shutdown().expect("clean shutdown");
}

#[test]
fn shutdown_returns_the_project_and_disconnects_handles() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_one(a, 0, 42.0))
        .expect("execution succeeded");

    // Dropping an extra external handle before the shutdown must change
    // nothing about the proper course of `shutdown`.
    let extra = session.handle();
    drop(extra);

    let project = session.shutdown().expect("clean shutdown");
    let node = project.tree().node(a).expect("column present");
    let column = node.as_column().expect("the node is a column");
    assert_eq!(column.get(0), Cell::Float(42.0));

    assert!(matches!(handle.snapshot(), Err(SessionError::Disconnected)));
    assert!(matches!(
        handle.execute(set_one(a, 0, 1.0)),
        Err(SessionError::Disconnected)
    ));
}

#[test]
fn snapshot_does_not_clone_the_tree_when_state_is_unchanged() {
    let (project, _sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let s1 = handle.snapshot().expect("snapshot");
    let s2 = handle.snapshot().expect("snapshot");
    assert!(
        Arc::ptr_eq(&s1.tree, &s2.tree),
        "a snapshot without mutation between two calls must share the tree, not clone it"
    );

    session.shutdown().expect("clean shutdown");
}

#[test]
fn save_does_not_change_revision_and_load_resets_history() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_one(a, 0, 42.0))
        .expect("execution succeeded");
    let rev_after_execute = handle.snapshot().expect("snapshot").revision;

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("session.plyx");
    handle.save(&path).expect("save succeeded");

    // Saving does not change the revision: it is the same state as before the
    // call to `save`.
    assert_eq!(
        handle.snapshot().expect("snapshot").revision,
        rev_after_execute
    );

    handle
        .execute(set_one(a, 0, 7.0))
        .expect("execution succeeded");
    let rev_before_load = handle.snapshot().expect("snapshot").revision;

    let rev_after_load = handle.load(&path).expect("load succeeded");
    let snap = handle.snapshot().expect("snapshot");

    assert_eq!(read_a(&snap, a)[0], 42.0, "saved value restored");
    assert!(!snap.history.can_undo, "the history must be cleared");
    assert!(!snap.history.can_redo, "the history must be cleared");
    assert!(snap.revision > rev_before_load);
    assert_eq!(snap.revision, rev_after_load);

    session.shutdown().expect("clean shutdown");
}

#[test]
fn load_of_a_missing_path_leaves_project_and_history_untouched() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .execute(set_one(a, 0, 42.0))
        .expect("execution succeeded");
    let before = handle.snapshot().expect("snapshot");

    let dir = tempfile::tempdir().expect("temporary directory");
    let missing = dir.path().join("missing.plyx");
    let err = handle.load(&missing).expect_err("missing path must fail");
    assert!(
        matches!(err, SessionError::Persistence(PersistenceError::Io { .. })),
        "{err:?}"
    );

    let after = handle.snapshot().expect("snapshot");
    assert_eq!(after.revision, before.revision, "revision unchanged");
    assert!(
        Arc::ptr_eq(&after.tree, &before.tree),
        "the tree must stay the same Arc, not merely equal"
    );
    assert!(after.history.can_undo, "the history must stay intact");

    session.shutdown().expect("clean shutdown");
}

#[test]
fn load_is_refused_while_a_macro_is_open() {
    let (project, _sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .begin_macro(MacroKind::Import)
        .expect("macro opening");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("session.plyx");
    let err = handle
        .load(&path)
        .expect_err("load during a macro must fail");
    assert!(
        matches!(err, SessionError::Command(CommandError::MacroOpen(_))),
        "{err:?}"
    );

    session.shutdown().expect("clean shutdown");
}

#[test]
fn after_load_new_ids_stay_above_every_id_in_the_file() {
    let (project, sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("session.plyx");
    handle.save(&path).expect("save succeeded");

    let highest_in_file = {
        let snap = handle.snapshot().expect("snapshot");
        snap.tree.iter().map(|(id, _)| id.raw()).max().unwrap_or(0)
    };

    handle.load(&path).expect("load succeeded");

    handle
        .execute(Command::AddColumn {
            sheet,
            index: None,
            name: Some("Z".to_string()),
            column: Column::from_f64(&[9.0]),
        })
        .expect("column added after loading (id counter restored)");

    let snap = handle.snapshot().expect("snapshot");
    let new_id: NodeId = snap
        .tree
        .child_by_name(sheet, "Z")
        .expect("sheet present in the reloaded tree")
        .expect("column Z added");
    assert!(
        new_id.raw() > highest_in_file,
        "id {new_id} expected strictly above {highest_in_file}"
    );

    session.shutdown().expect("clean shutdown");
}

/// `with_new_identity` (issue #21): a handle obtained from `.clone()` stays the
/// same sender and enters its own macro; a handle with a new identity is
/// refused as long as another's macro is open.
#[test]
fn a_handle_with_a_new_identity_cannot_write_into_anothers_macro() {
    use stokesforge_commands::{Command, CommandError};
    use stokesforge_core::{Column, Project, ValueBlock};
    use stokesforge_session::{Session, SessionError};

    let mut project = Project::new("Demo").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let owner = session.handle();
    let same = owner.clone();
    let other = owner.with_new_identity();
    let write = |value: f64| Command::SetCells {
        column,
        start: 0,
        values: ValueBlock::Float(vec![value]),
        expected_version: None,
    };

    owner.begin_macro(MacroKind::GraphProperties).unwrap();
    assert!(
        same.execute(write(5.0)).is_ok(),
        "the clone is the same sender"
    );
    let refused = other.execute(write(99.0));
    assert!(
        matches!(
            refused,
            Err(SessionError::Command(CommandError::ForeignMacro(_)))
        ),
        "{refused:?}"
    );
    owner.abort_macro().unwrap();
    let snapshot = session.handle().snapshot().unwrap();
    assert!(snapshot.history.undo_label.is_none(), "nothing undoable");
}

/// #344: a file that carries `inverted` next to increasing bounds opens with
/// its bounds swapped, without a history entry; the file itself is not touched
/// by the read.
#[test]
fn load_swaps_the_bounds_of_an_inverted_axis_without_history() {
    use stokesforge_core::NodeKind;
    use stokesforge_plot::{AxisSpec, PanelSpec, PlotSpec};

    let (mut project, _sheet, _a) = fixture();
    let root = project.root();
    let axis = AxisSpec {
        auto_range: false,
        inverted: true,
        range_min: Some(-1.0),
        range_max: Some(10.0),
        ..AxisSpec::default()
    };
    let panel = PanelSpec {
        bottom: axis.clone(),
        left: AxisSpec {
            auto_range: false,
            range_min: Some(4.0),
            range_max: Some(2.0),
            ..AxisSpec::default()
        },
        ..PanelSpec::default()
    };
    let graph = project
        .add_graph(root, Some("Fig1"), PlotSpec::default().with_panels(panel))
        .expect("graph");
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("direction.stf");
    stokesforge_persistence::save(&project, &path).expect("save");
    let bytes = std::fs::read(&path).expect("read");

    let (empty, _, _) = fixture();
    let session = Session::spawn(empty, None).expect("actor thread started");
    let handle = session.handle();
    handle.load(&path).expect("load succeeded");
    let snap = handle.snapshot().expect("snapshot");
    let NodeKind::Graph { spec, .. } = snap.tree.node(graph).expect("graph").kind() else {
        panic!("a graph");
    };
    let bottom = &spec.panels[0].bottom;
    assert!(bottom.inverted);
    assert_eq!(
        (bottom.range_min, bottom.range_max),
        (Some(10.0), Some(-1.0))
    );
    let left = &spec.panels[0].left;
    assert!(left.inverted, "From > To read as an inverted axis");
    assert_eq!((left.range_min, left.range_max), (Some(4.0), Some(2.0)));
    assert!(!snap.history.can_undo && !snap.history.can_redo);
    assert_eq!(std::fs::read(&path).expect("reread"), bytes);
    session.shutdown().expect("clean shutdown");
}

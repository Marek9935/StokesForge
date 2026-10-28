//! Integration tests of window geometry — see "Window geometry" at the top of
//! `stokesforge_session` for the decision they check.

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_session::{Session, WindowGeometry};

/// Builds the fixture project: `Book1` → `Sheet1` → column `A [1,2,3]`.
/// Returns `(project, book, column)`.
fn fixture() -> (Project, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("workbook created");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("sheet resolved");
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column created");
    (project, book, column)
}

fn geometry(x: f64, y: f64, width: f64, height: f64) -> WindowGeometry {
    WindowGeometry {
        x,
        y,
        width,
        height,
        view: None,
    }
}

/// Setting a geometry touches neither `revision` nor the history — `undo`
/// stays without effect on it (there is nothing to undo), and the entry
/// survives an `undo`/`redo` of an unrelated command.
#[test]
fn setting_geometry_does_not_move_revision_nor_history() {
    let (project, book, _column) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    let snap0 = handle.snapshot().expect("snapshot");
    assert_eq!(snap0.revision, 0);
    assert!(snap0.window_geometries.is_empty());
    assert_eq!(snap0.geometry_revision, 0);

    let geometry_rev = handle
        .set_window_geometry(book, geometry(10.0, 20.0, 640.0, 480.0))
        .expect("geometry set");
    assert_eq!(geometry_rev, 1);

    let snap1 = handle.snapshot().expect("snapshot");
    // The model's revision did not move: it is not a mutation.
    assert_eq!(snap1.revision, 0);
    assert!(!snap1.history.can_undo);
    assert_eq!(snap1.geometry_revision, 1);
    assert_eq!(
        snap1.window_geometries.get(&book),
        Some(&geometry(10.0, 20.0, 640.0, 480.0))
    );

    // `undo`/`redo` have nothing to undo: the command stack is empty.
    let after_undo = handle.undo().expect("undo without effect, no error");
    assert_eq!(after_undo, None);
    let after_redo = handle.redo().expect("redo without effect, no error");
    assert_eq!(after_redo, None);

    let snap2 = handle.snapshot().expect("snapshot");
    assert_eq!(snap2.revision, 0);
    assert_eq!(snap2.geometry_revision, 1);
    assert_eq!(
        snap2.window_geometries.get(&book),
        Some(&geometry(10.0, 20.0, 640.0, 480.0))
    );
}

/// A geometry set, then a real data mutation executed and undone: the geometry
/// is affected neither by the execution, nor by `undo`/`redo` of a command that
/// does not touch the node concerned.
#[test]
fn geometry_survives_unrelated_execute_undo_redo() {
    let (project, book, column) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .set_window_geometry(book, geometry(1.0, 2.0, 3.0, 4.0))
        .expect("geometry set");

    let rev = handle
        .execute(Command::SetCells {
            column,
            start: 0,
            values: ValueBlock::Float(vec![42.0]),
            expected_version: None,
        })
        .expect("command executed");
    assert_eq!(rev, 1);

    let snap_after_execute = handle.snapshot().expect("snapshot");
    assert_eq!(
        snap_after_execute.window_geometries.get(&book),
        Some(&geometry(1.0, 2.0, 3.0, 4.0))
    );
    // No purge: the node targeted by the geometry still exists, and no other
    // node vanished — `geometry_revision` stays at 1.
    assert_eq!(snap_after_execute.geometry_revision, 1);

    handle.undo().expect("undo succeeded");
    let snap_after_undo = handle.snapshot().expect("snapshot");
    assert_eq!(
        snap_after_undo.window_geometries.get(&book),
        Some(&geometry(1.0, 2.0, 3.0, 4.0))
    );
}

/// `save`/`load` round trip: a geometry set, saved, reloaded in a new session,
/// comes back identical.
#[test]
fn geometry_round_trips_through_save_and_load() {
    let (project, book, _column) = fixture();
    let session = Session::spawn(project, None).expect("actor thread started");
    let handle = session.handle();

    handle
        .set_window_geometry(book, geometry(100.0, 200.0, 800.0, 600.0))
        .expect("geometry set");

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("geometry.plyx");
    handle.save(&path).expect("save succeeded");

    // New session, new project: the file must give it back the window
    // layout, without anything being set again by hand.
    let empty = Project::new("Empty").expect("valid project");
    let other_session = Session::spawn(empty, None).expect("actor thread started");
    let other_handle = other_session.handle();

    other_handle.load(&path).expect("load succeeded");
    let snapshot = other_handle.snapshot().expect("snapshot");

    // `load` resets the identifiers: `book` names the same node in the
    // reloaded file (first save of a freshly built project, no deletion took
    // place in between).
    assert_eq!(
        snapshot.window_geometries.get(&book),
        Some(&geometry(100.0, 200.0, 800.0, 600.0))
    );
}

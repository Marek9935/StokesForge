//! Column width (#325): the actor's `NodeId -> f32` table, beside the window
//! geometries — outside the model, outside the history, sharing
//! `geometry_revision`, saved in the `.stf`, never purged on mutation.

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId, Project};
use stokesforge_session::Session;

/// `Book1` → `Sheet1` → columns `A`, `B`. Returns `(project, a, b)`.
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
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("column A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[3.0, 4.0]))
        .expect("column B");
    (project, a, b)
}

/// Setting a width touches neither `revision` nor the history:
/// `geometry_revision` alone advances. Witness: make the setting go through
/// `revision`.
#[test]
fn setting_a_width_only_advances_geometry_revision() {
    let (project, _a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    let before = handle.snapshot().unwrap();
    assert!(before.column_widths.is_empty());

    let rev = handle.set_column_width(b, 80.0).unwrap();
    assert_eq!(rev, before.geometry_revision + 1);

    let after = handle.snapshot().unwrap();
    assert_eq!(after.revision, before.revision, "revision intact");
    assert!(!after.history.can_undo, "no history entry");
    assert_eq!(after.geometry_revision, before.geometry_revision + 1);
    assert_eq!(after.column_widths.get(&b), Some(&80.0));
    // The same width set again, or an invalid width, wakes nobody.
    handle.set_column_width(b, 80.0).unwrap();
    handle.set_column_width(b, f32::NAN).unwrap();
    handle.set_column_width(b, -4.0).unwrap();
    handle.set_column_width(b, 0.0).unwrap();
    handle.set_column_width(b, f32::INFINITY).unwrap();
    let still = handle.snapshot().unwrap();
    assert_eq!(still.geometry_revision, after.geometry_revision);
    assert_eq!(still.column_widths.get(&b), Some(&80.0));
    session.shutdown().unwrap();
}

/// Delete the column then Ctrl+Z: its width comes back (invariant 7: the
/// `NodeId` is the same, and the table is not purged on mutation).
/// Witness: add the widths to `purge_orphaned_geometries`.
#[test]
fn the_width_comes_back_with_the_column_after_undo() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();

    handle.execute(Command::DeleteColumn { column: b }).unwrap();
    let deleted = handle.snapshot().unwrap();
    assert!(deleted.tree.node(b).is_none());
    assert_eq!(deleted.column_widths.get(&a), Some(&61.0));

    handle.undo().unwrap();
    let restored = handle.snapshot().unwrap();
    assert!(restored.tree.node(b).is_some());
    assert_eq!(restored.column_widths.get(&b), Some(&99.0));
    assert_eq!(restored.column_widths.get(&a), Some(&61.0));
    session.shutdown().unwrap();
}

/// `save`/`load` round trip: the table comes back identical.
/// Witness: do not write the field.
#[test]
fn the_widths_make_the_file_round_trip() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.5).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("widths.stf");
    handle.save(&path).unwrap();

    let other = Session::spawn(Project::new("Empty").unwrap(), None).unwrap();
    let other_handle = other.handle();
    other_handle.load(&path).unwrap();
    let loaded = other_handle.snapshot().unwrap();
    assert_eq!(
        *loaded.column_widths,
        *handle.snapshot().unwrap().column_widths
    );
    assert_eq!(loaded.column_widths.len(), 2);
    // A new project empties the table.
    other_handle.new_project().unwrap();
    assert!(other_handle.snapshot().unwrap().column_widths.is_empty());
    session.shutdown().unwrap();
    other.shutdown().unwrap();
}

/// Save after deletion: the key of the vanished column is not written, while
/// the actor's table keeps it (for Ctrl+Z).
/// Witness: remove the filter of `save_with_geometries`.
#[test]
fn saving_does_not_write_the_width_of_a_deleted_column() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();
    handle.execute(Command::DeleteColumn { column: b }).unwrap();
    // The actor keeps the dead key...
    assert!(handle.snapshot().unwrap().column_widths.contains_key(&b));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deleted.stf");
    handle.save(&path).unwrap();
    // ... but the file does not carry it.
    let (_, view, _) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert_eq!(view.column_widths.get(&a), Some(&61.0));
    assert!(!view.column_widths.contains_key(&b));
    // Nor does the raw file (the read filter does not hide the write).
    let raw = std::fs::read(&path).unwrap();
    let needle = b"column_widths";
    let at = raw
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("the field is written");
    // A one-entry map: fixmap 0x81 right after the key (13-byte str: 0xad + name).
    assert_eq!(raw[at + needle.len()], 0x81, "only one width written");
    session.shutdown().unwrap();
}

/// Resetting a set width to the default (#330): the key disappears,
/// `geometry_revision` advances by 1, neither `revision` nor the history
/// moves. A column without a width: nothing changes. Witness tried: an
/// unconditional `+= 1` (red).
#[test]
fn resetting_to_default_removes_the_key_and_advances_only_if_it_existed() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();
    let before = handle.snapshot().unwrap();

    handle.reset_column_widths(&[b]).unwrap();
    let after = handle.snapshot().unwrap();
    assert!(!after.column_widths.contains_key(&b));
    assert_eq!(after.column_widths.get(&a), Some(&61.0), "A keeps its own");
    assert_eq!(after.geometry_revision, before.geometry_revision + 1);
    assert_eq!(after.revision, before.revision, "revision intact");
    assert!(!after.history.can_undo, "no history entry");

    // Already at the default: unchanged.
    handle.reset_column_widths(&[b]).unwrap();
    handle.reset_column_widths(&[]).unwrap();
    let still = handle.snapshot().unwrap();
    assert_eq!(still.geometry_revision, after.geometry_revision);

    // Several columns at once: a single advance.
    handle.reset_column_widths(&[a, b]).unwrap();
    let all = handle.snapshot().unwrap();
    assert!(all.column_widths.is_empty());
    assert_eq!(all.geometry_revision, after.geometry_revision + 1);
    session.shutdown().unwrap();
}

/// Reset then save: the file no longer carries the key (the session does not
/// write 54). Witness tried: set 54 instead of removing, or remove nothing
/// (red).
#[test]
fn a_column_reset_to_default_is_no_longer_in_the_file() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();
    handle.reset_column_widths(&[b]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reset.stf");
    handle.save(&path).unwrap();
    let (_, view, _) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert_eq!(view.column_widths.get(&a), Some(&61.0));
    assert!(!view.column_widths.contains_key(&b));
    session.shutdown().unwrap();
}

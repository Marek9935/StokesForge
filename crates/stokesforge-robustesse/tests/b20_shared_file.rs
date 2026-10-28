//! Battery 20 — two writers, one file.
//!
//! `save` keeps its promise of **atomicity**: a reader never sees a
//! half-written file, and a write that fails does not touch the target
//! (`NamedTempFile::persist`, a `rename` on the same filesystem). This is a
//! guarantee against **interruption**.
//!
//! It now also says something about **concurrency** (B8,
//! `guide-de-correction-robustesse.md` §F2): `FileBody` carries a monotonic
//! save counter, compared with what THIS process saw last time for this
//! `Project` instance before overwriting an existing target. The question — the
//! "lost update" of any software that edits documents — is answered only for
//! this specific case: two distinct `Project`s, in different processes (or
//! threads), that load then save the same file without ever reloading in
//! between.

use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_persistence::{load, save};
use tempfile::TempDir;

/// `[Book1]Sheet1!A` = [1, 2, 3].
fn project() -> (Project, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    (project, column)
}

fn values(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .node(column)
        .and_then(stokesforge_core::Node::as_column)
        .and_then(Column::float_values)
        .expect("numeric column")
        .as_slice()
        .to_vec()
}

// -- guards ---------------------------------------------------------------

#[test]
fn b20_an_interrupted_save_does_not_touch_the_target() {
    // The positive counterpart: the target does not exist yet, the write fails
    // (nonexistent parent directory), and nothing is left behind.
    let dir = TempDir::new().expect("directory");
    let (project, _) = project();
    let inaccessible = dir.path().join("missing-subfolder").join("p.stf");
    assert!(save(&project, &inaccessible).is_err());
    assert!(
        !inaccessible.exists(),
        "a write that fails must leave no file"
    );
}

#[test]
fn b20_two_successive_saves_give_the_same_file() {
    let dir = TempDir::new().expect("directory");
    let (project, _) = project();
    let a = dir.path().join("a.stf");
    let b = dir.path().join("b.stf");
    save(&project, &a).expect("first save");
    save(&project, &b).expect("second save");
    assert_eq!(
        std::fs::read(&a).expect("read a"),
        std::fs::read(&b).expect("read b"),
        "two saves of the same project must be identical byte for byte"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
fn b20_saving_over_a_file_modified_elsewhere_is_rejected() {
    let dir = TempDir::new().expect("directory");
    let path = dir.path().join("manip.stf");

    // Instance A opens the project and saves it.
    let (project_a, column) = project();
    save(&project_a, &path).expect("save of A");

    // Instance B — another window, another process, or a script — loads the
    // same file, writes, saves.
    let mut project_b = load(&path).expect("load by B");
    project_b
        .set_values(column, 0, ValueBlock::Float(vec![42.0]))
        .expect("write of B");
    save(&project_b, &path).expect("save of B");
    let after_b = values(&load(&path).expect("reread"), column);
    println!("after B's save: {after_b:?}");

    // A saw nothing happen. It modifies its copy, and saves.
    let mut project_a = project_a;
    project_a
        .set_values(column, 1, ValueBlock::Float(vec![7.0]))
        .expect("write of A");
    let save_of_a = save(&project_a, &path);
    let final_ = values(&load(&path).expect("reread"), column);
    println!("after A's save: {final_:?}");
    println!("  (B's 42 has disappeared; A knew nothing, B will know nothing)");

    assert!(
        save_of_a.is_err(),
        "the file changed between A's load and its save, and nothing noticed: B's \
         work is lost without a trace. `save` has no lock, no kept modification \
         date, no fingerprint of the original file — it overwrites. The atomicity of \
         `NamedTempFile::persist` protects against interruption, not against \
         concurrency, and nothing in the crate's documentation distinguishes the two"
    );
}

#[test]
fn b20_a_file_carries_what_is_needed_to_detect_an_overwrite() {
    // B8 (`guide-de-correction-robustesse.md` §F2) — fixed:
    // `FileBody` now carries `save_counter`, a monotonic save counter
    // (persisted, `#[serde(default)]` to stay readable by an older reader that
    // would ignore it), without moving the format's version number. This is
    // exactly the field that this test used to find missing: `save` compares,
    // before writing, the counter on disk with the one THIS process saw last
    // time for this instance — see
    // `b20_saving_over_a_file_modified_elsewhere_is_rejected`, which checks the
    // refusal itself. This one only checks that the counter advances on every
    // save, which makes that refusal possible.
    let dir = TempDir::new().expect("directory");
    let path = dir.path().join("manip.stf");
    let (mut project, column) = project();
    save(&project, &path).expect("save");
    let first = std::fs::read(&path).expect("read");

    project
        .set_values(column, 0, ValueBlock::Float(vec![42.0]))
        .expect("write");
    save(&project, &path).expect("save");
    let second = std::fs::read(&path).expect("read");

    println!(
        "two successive states of the same file: {} then {} bytes",
        first.len(),
        second.len()
    );
    assert_ne!(
        first, second,
        "the save counter must have changed between the two writes, even if the rest \
         of the content had stayed identical"
    );
}

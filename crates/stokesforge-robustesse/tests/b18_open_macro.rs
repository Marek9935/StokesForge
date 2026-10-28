//! Battery 18 — what remains possible while a macro is open.
//!
//! A macro promises atomicity: its commands become **a single** history entry,
//! and `abort_macro` must "leave no trace". The question this battery asks is:
//! *no trace where?* The actor keeps a macro open between two requests, and
//! meanwhile six other requests are still served. Two of them should not be.
//!
//! `Request::Load` already knows it — it returns `Err(MacroOpen)` before even
//! touching the disk. `Request::Save`, three lines above in the same `match`,
//! does not.

use stokesforge_commands::{Command, History, MacroKind};
use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_session::{Session, SessionError, SessionHandle};
use tempfile::TempDir;

/// `[Book1]Sheet1!A` = [1, 2, 3], served by an actor.
fn session() -> (Session, SessionHandle, NodeId) {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column A");
    let session = Session::spawn(project, None).expect("actor started");
    let handle = session.handle();
    (session, handle, column)
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

fn write(handle: &SessionHandle, column: NodeId, value: f64) {
    handle
        .execute(Command::SetCells {
            column,
            start: 0,
            values: ValueBlock::Float(vec![value]),
            expected_version: None,
        })
        .expect("write accepted");
}

// -- guards ---------------------------------------------------------------

#[test]
fn b18_loading_is_rejected_during_a_macro() {
    let dir = TempDir::new().expect("directory");
    let path = dir.path().join("other.stf");
    let (_session, handle, column) = session();
    handle.save(&path).expect("initial save");
    handle.begin_macro(MacroKind::Import).expect("macro opened");
    write(&handle, column, 99.0);
    assert!(
        matches!(
            handle.load(&path),
            Err(SessionError::Command(
                stokesforge_commands::CommandError::MacroOpen(_)
            ))
        ),
        "`load` must refuse as long as a macro is open — and it does"
    );
}

#[test]
fn b18_an_aborted_macro_leaves_nothing_in_the_model() {
    let (session, handle, column) = session();
    handle.begin_macro(MacroKind::Import).expect("macro opened");
    write(&handle, column, 99.0);
    handle.abort_macro().expect("macro aborted");
    let project = session.shutdown().expect("shutdown");
    assert_eq!(values(&project, column), vec![1.0, 2.0, 3.0]);
}

#[test]
fn b18_a_macro_of_reasonable_depth_undoes_as_one_block() {
    let mut project = Project::new("Demo").expect("valid project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("worksheet");
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column");
    let mut history = History::with_limit(0);
    for _ in 0..1000 {
        history
            .begin_macro(MacroKind::Import)
            .expect("within the limit of MAX_MACRO_DEPTH");
    }
    history
        .execute(
            &mut project,
            Command::SetCells {
                column,
                start: 0,
                values: ValueBlock::Float(vec![9.0]),
                expected_version: None,
            },
        )
        .expect("write");
    for _ in 0..1000 {
        history.end_macro(&mut project).expect("closing");
    }
    assert_eq!(history.len(), 1, "a thousand levels, a single entry");
    assert_eq!(history.undo(&mut project), Ok(true));
    assert_eq!(values(&project, column), vec![1.0]);
}

// -- reproductions --------------------------------------------------------

#[test]
fn b18_saving_is_rejected_during_a_macro() {
    let dir = TempDir::new().expect("directory");
    let path = dir.path().join("project.stf");
    let (session, handle, column) = session();

    handle.begin_macro(MacroKind::Import).expect("macro");
    write(&handle, column, 99.0);
    let save = handle.save(&path);
    println!("save during the macro → {save:?}");

    if save.is_ok() {
        let on_disk = stokesforge_persistence::load(&path).expect("reread");
        println!("the file contains: {:?}", values(&on_disk, column));
        handle.abort_macro().expect("macro aborted");
        let after = session.shutdown().expect("shutdown");
        println!("the model, after the abort: {:?}", values(&after, column));
    }

    assert!(
        matches!(
            save,
            Err(SessionError::Command(
                stokesforge_commands::CommandError::MacroOpen(_)
            ))
        ),
        "the written file contains the intermediate state of a macro that the user \
         then abandons: the model goes back to [1, 2, 3], the disk keeps [99, 2, 3], \
         and no undo can reach it any more. `Request::Load` already refuses this case \
         (`Err(MacroOpen)`, guard b18_loading_is_rejected_during_a_macro); \
         `Request::Save`, in the same `match` of `actor.rs`, does not consult \
         `history.in_macro()`"
    );
}

#[test]
fn b18_macro_nesting_is_bounded() {
    // B7 (`guide-de-correction-robustesse.md` §F4) — fixed:
    // `begin_macro` refuses (`Err(MacroTooDeep)`) beyond
    // `stokesforge_commands::MAX_MACRO_DEPTH`, before even approaching the
    // boundary where undo (`History::revert`, recursive on `Entry::Macro`)
    // would overflow the actor thread's stack — measured at 10,891 levels
    // (`cargo run --bin stokesforge-stack-probe -- <n> undo`).
    // Safe: **building** the nesting never overflows by itself
    // (`begin_macro`/`end_macro` are iterative); it is indeed the refusal
    // itself, not a cap on the construction, that closes the anomaly.
    let mut history = History::with_limit(0);
    let mut refused_at = None;
    for i in 0..50_000 {
        if let Err(err) = history.begin_macro(MacroKind::Import) {
            println!("begin_macro refused at level {i}: {err}");
            refused_at = Some(i);
            break;
        }
    }
    println!("depth reached: {}", history.macro_depth());
    assert_eq!(
        refused_at,
        Some(stokesforge_commands::MAX_MACRO_DEPTH),
        "opening a macro at level {} (the bound) must be refused, not accepted",
        stokesforge_commands::MAX_MACRO_DEPTH
    );
    assert_eq!(
        history.macro_depth(),
        stokesforge_commands::MAX_MACRO_DEPTH,
        "a refusal must not have changed the depth reached"
    );
}

#[test]
fn b18_the_interface_does_not_announce_an_impossible_undo() {
    let (_session, handle, column) = session();
    write(&handle, column, 5.0);
    handle.begin_macro(MacroKind::Import).expect("macro");

    let snapshot = handle.snapshot().expect("snapshot");
    println!(
        "during the macro: can_undo = {}, undo_label = {:?}, in_macro = {}",
        snapshot.history.can_undo, snapshot.history.undo_label, snapshot.history.in_macro
    );
    println!("undo → {:?}", handle.undo());

    assert!(
        !snapshot.history.can_undo || !snapshot.history.in_macro,
        "the snapshot announces \"Undo {:?}\" while any undo will return \
         `Err(MacroOpen)`. `HistoryState` already carries `in_macro`: it is `can_undo` \
         (and `AppState::can_undo`, which copies it as is) that does not consult it — \
         the menu offers an action that is bound to fail",
        snapshot.history.undo_label
    );
}

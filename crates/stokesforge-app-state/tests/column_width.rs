//! Column width (#325) on the application state side: a view setting that goes
//! through the actor, with no command and no history, including from a detached
//! window (its own `AppState`, the same session).

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{Column, NodeId, Project};
use stokesforge_session::Session;

fn setup(dir: &std::path::Path) -> (Session, AppState, NodeId) {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    (session, state, column)
}

/// The action sets the width on the actor and reads it back without waiting for
/// a refresh; neither the revision nor the history moves.
#[test]
fn the_action_sets_the_width_without_history() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, column) = setup(dir.path());
    let revision = state.revision();
    assert_eq!(state.column_width(column), None);

    state.dispatch(Action::SetColumnWidth {
        column,
        width: 88.0,
    });

    assert_eq!(state.column_width(column), Some(88.0));
    assert_eq!(state.revision(), revision);
    assert!(!state.can_undo(), "no history entry");
    let actor = session.handle().snapshot().unwrap();
    assert_eq!(actor.column_widths.get(&column), Some(&88.0));
    assert_eq!(actor.revision, revision);
    // An invalid width enters neither the actor nor the local copy.
    state.dispatch(Action::SetColumnWidth {
        column,
        width: f32::NAN,
    });
    assert_eq!(state.column_width(column), Some(88.0));
}

/// The detached window has its own `AppState`: its width also goes through
/// the actor, and the main state reads it back at its next refresh.
#[test]
fn the_detached_window_sends_its_width_to_the_actor() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut main, column) = setup(dir.path());
    let mut detached = main.fork_for_detached_view().unwrap();

    detached.dispatch(Action::SetColumnWidth {
        column,
        width: 120.0,
    });

    assert_eq!(detached.column_width(column), Some(120.0));
    assert_eq!(
        session
            .handle()
            .snapshot()
            .unwrap()
            .column_widths
            .get(&column),
        Some(&120.0)
    );
    assert_eq!(main.column_width(column), None, "not refreshed yet");
    main.refresh();
    assert_eq!(main.column_width(column), Some(120.0));
}

/// The reset action (#330) removes the width from the actor and from the local
/// copy without waiting for a refresh; neither the revision nor the history
/// moves. Tested witness: do not remove the key from the local copy.
#[test]
fn the_reset_action_removes_the_width_without_history() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, column) = setup(dir.path());
    let revision = state.revision();
    state.dispatch(Action::SetColumnWidth {
        column,
        width: 88.0,
    });
    assert_eq!(state.column_width(column), Some(88.0));

    state.dispatch(Action::ResetColumnWidths {
        columns: vec![column],
    });

    assert_eq!(state.column_width(column), None);
    assert_eq!(state.revision(), revision);
    let actor = session.handle().snapshot().unwrap();
    assert!(!actor.column_widths.contains_key(&column));
}

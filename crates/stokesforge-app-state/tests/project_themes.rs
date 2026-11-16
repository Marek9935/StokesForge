//! #540: the actions that keep graph themes in the project. No interface yet:
//! these are the entry points the picker and the Save as theme form call.

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState, StatusLevel};
use stokesforge_core::{NodeId, Project};
use stokesforge_session::Session;

fn start(dir: &Path) -> (Session, AppState) {
    let mut project = Project::new("Themes").expect("project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let session = Session::spawn(project, None).expect("session");
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("projet.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.join("preferences.json"),
        )),
    )
    .expect("state");
    (session, state)
}

fn graph(state: &mut AppState, background: &str) -> NodeId {
    state.dispatch(Action::AddGraph);
    let graph = state.graphs().last().expect("a graph").0;
    let mut spec = state.plot_spec(graph).expect("spec");
    spec.background = background.into();
    state.dispatch(Action::SetPlotSpec { graph, spec });
    graph
}

fn names(state: &AppState) -> Vec<String> {
    state
        .tree()
        .project_themes()
        .iter()
        .map(|theme| theme.name.clone())
        .collect()
}

#[test]
fn add_rename_overwrite_and_delete_are_one_undo_step_each() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let red = graph(&mut state, "#aa0000");
    let blue = graph(&mut state, "#0000aa");

    state.dispatch(Action::AddProjectTheme {
        graph: red,
        name: "Thesis".into(),
    });
    assert_eq!(names(&state), ["Thesis"]);
    assert_eq!(state.tree().project_themes()[0].style.background, "#aa0000");

    state.dispatch(Action::RenameProjectTheme {
        name: "thesis".into(),
        new_name: "Paper".into(),
    });
    assert_eq!(names(&state), ["Paper"]);

    state.dispatch(Action::OverwriteProjectTheme {
        name: "PAPER".into(),
        graph: blue,
    });
    assert_eq!(names(&state), ["Paper"]);
    assert_eq!(state.tree().project_themes()[0].style.background, "#0000aa");

    state.dispatch(Action::DeleteProjectTheme {
        name: "Paper".into(),
    });
    assert!(names(&state).is_empty());

    // One undo per change, back to the start.
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().project_themes()[0].style.background, "#0000aa");
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().project_themes()[0].style.background, "#aa0000");
    state.dispatch(Action::Undo);
    assert_eq!(names(&state), ["Thesis"]);
    state.dispatch(Action::Undo);
    assert!(names(&state).is_empty());
}

#[test]
fn a_duplicate_name_is_refused_with_a_message_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let red = graph(&mut state, "#aa0000");
    state.dispatch(Action::AddProjectTheme {
        graph: red,
        name: "Thesis".into(),
    });
    let undo = state.undo_label();
    state.dispatch(Action::AddProjectTheme {
        graph: red,
        name: " THESIS ".into(),
    });
    assert_eq!(names(&state), ["Thesis"]);
    assert_eq!(state.undo_label(), undo);
    assert!(state
        .status()
        .is_some_and(|status| status.level == StatusLevel::Error));

    state.dispatch(Action::AddProjectTheme {
        graph: red,
        name: "Other".into(),
    });
    state.dispatch(Action::RenameProjectTheme {
        name: "Other".into(),
        new_name: "thesis".into(),
    });
    assert_eq!(names(&state), ["Thesis", "Other"]);
}

#[test]
fn a_captured_theme_holds_no_data_reference() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let red = graph(&mut state, "#aa0000");
    state.dispatch(Action::AddProjectTheme {
        graph: red,
        name: "T".into(),
    });
    let theme = &state.tree().project_themes()[0];
    assert!(theme
        .style
        .panels
        .iter()
        .all(|panel| panel.series.is_empty()));
}

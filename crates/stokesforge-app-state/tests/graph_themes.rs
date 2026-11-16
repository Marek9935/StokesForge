//! #539: the user's graph themes on this machine and the default theme of new
//! graphs. A new graph made by Add Graph takes the theme's style (every family
//! but the texts and the axis ranges), the legend text still comes from the
//! Preferences, and a graph made through the session (scripts) is not touched.

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::user_themes::{ThemeRef, UserThemeError};
use stokesforge_commands::Command;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{LegendText, LegendTextFrom, PlotSpec};
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

fn last_graph(state: &AppState) -> NodeId {
    state.graphs().last().expect("a graph").0
}

/// A graph with a dark blue background, written axis ranges and a custom
/// legend text, saved as the user theme "Poster".
fn poster(state: &mut AppState) -> u64 {
    state.dispatch(Action::AddGraph);
    let graph = last_graph(state);
    let mut spec = state.plot_spec(graph).expect("spec");
    spec.background = "#102030".into();
    let panel = spec.panels.get_mut(0).expect("layer");
    panel.left.range_min = Some(5.0);
    panel.left.range_max = Some(9.0);
    panel.legend.text.template = "from the theme".into();
    state.dispatch(Action::SetPlotSpec { graph, spec });
    state.add_user_theme("Poster", graph).expect("saved")
}

#[test]
fn a_new_graph_takes_the_default_theme_but_keeps_legend_text_and_ranges() {
    let dir = tempfile::tempdir().expect("folder");
    let (_session, mut state) = start(dir.path());
    let id = poster(&mut state);
    state
        .set_new_graph_theme(Some(ThemeRef::User(id)))
        .expect("saved");
    state.dispatch(Action::SetNewGraphLegendText(LegendText {
        from: LegendTextFrom::Custom,
        template: "*#ln*".into(),
        row_column: None,
    }));
    state.dispatch(Action::AddGraph);
    let spec = state.plot_spec(last_graph(&state)).expect("spec");
    assert_eq!(spec.background, "#102030", "the theme's background");
    assert_eq!(
        spec.panels[0].legend.text.template, "*#ln*",
        "the Preferences' legend text wins"
    );
    assert_eq!(spec.panels[0].left.range_min, None, "its own ranges");
    assert_eq!(spec.panels[0].left.range_max, None);
}

#[test]
fn no_default_theme_no_change_and_a_deleted_one_falls_back_to_none() {
    let dir = tempfile::tempdir().expect("folder");
    let (_session, mut state) = start(dir.path());
    let id = poster(&mut state);
    state.dispatch(Action::AddGraph);
    let plain = state.plot_spec(last_graph(&state)).expect("spec");
    assert_ne!(plain.background, "#102030");
    state
        .set_new_graph_theme(Some(ThemeRef::User(id)))
        .expect("saved");
    state.delete_user_theme(id).expect("deleted");
    assert!(state.new_graph_theme().is_none());
    state.dispatch(Action::AddGraph);
    let after = state.plot_spec(last_graph(&state)).expect("spec");
    assert_eq!(after.background, plain.background);
}

#[test]
fn a_built_in_default_gives_its_colours_and_a_project_theme_is_refused() {
    let dir = tempfile::tempdir().expect("folder");
    let (_session, mut state) = start(dir.path());
    state
        .set_new_graph_theme(Some(ThemeRef::Builtin(1)))
        .expect("saved");
    state.dispatch(Action::AddGraph);
    let spec = state.plot_spec(last_graph(&state)).expect("spec");
    assert_eq!(spec.background, "#1e1e1e");
    state
        .set_new_graph_theme(Some(ThemeRef::project("Mine")))
        .expect("saved as none");
    assert!(state.new_graph_theme().is_none());
}

#[test]
fn a_graph_made_through_the_session_is_not_themed() {
    let dir = tempfile::tempdir().expect("folder");
    let (session, mut state) = start(dir.path());
    let id = poster(&mut state);
    state
        .set_new_graph_theme(Some(ThemeRef::User(id)))
        .expect("saved");
    let root = state.tree().root();
    session
        .handle()
        .execute(Command::AddGraph {
            parent: root,
            name: None,
            spec: PlotSpec::empty(""),
        })
        .expect("script-like creation");
    state.refresh();
    let spec = state.plot_spec(last_graph(&state)).expect("spec");
    assert_ne!(spec.background, "#102030");
}

#[test]
fn the_library_and_the_default_survive_a_restart() {
    let dir = tempfile::tempdir().expect("folder");
    let id = {
        let (_session, mut state) = start(dir.path());
        let id = poster(&mut state);
        state
            .set_new_graph_theme(Some(ThemeRef::User(id)))
            .expect("saved");
        state
            .record_theme_use(&ThemeRef::User(id))
            .expect("counted");
        id
    };
    let (_session, mut state) = start(dir.path());
    assert!(state.user_themes().get(&ThemeRef::User(id)).is_some());
    assert_eq!(state.user_themes().uses_of(&ThemeRef::User(id)), 1);
    state.dispatch(Action::AddGraph);
    let spec = state.plot_spec(last_graph(&state)).expect("spec");
    assert_eq!(spec.background, "#102030");
}

#[test]
fn a_library_from_a_newer_version_is_read_only_through_the_state() {
    let dir = tempfile::tempdir().expect("folder");
    let file = dir.path().join("user_themes.json");
    std::fs::write(&file, r#"{"version":9}"#).expect("write");
    let (_session, mut state) = start(dir.path());
    assert!(state.user_themes_read_only().is_some());
    state.dispatch(Action::AddGraph);
    let graph = last_graph(&state);
    assert!(matches!(
        state.add_user_theme("A", graph),
        Err(UserThemeError::ReadOnly)
    ));
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        r#"{"version":9}"#
    );
}

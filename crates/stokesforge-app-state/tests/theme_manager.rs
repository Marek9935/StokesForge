//! #542: the theme manager of the Theme tab of Graph Properties, seen from the
//! state: each control greyed with its reason, the confirmation before a user
//! theme goes (a project theme is undoable and asks nothing), Apply Theme…
//! into the draft of the open window (Cancel gives the graph back, OK keeps
//! it), a renamed theme keeping its use count, and the default theme of new
//! graphs.

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState, PlotSettingsTab};
use stokesforge_app_state::theme_window::{ThemeSource, ThemeTarget};
use stokesforge_app_state::user_themes::ThemeRef;
use stokesforge_core::{NodeId, Project};
use stokesforge_session::Session;

fn start(dir: &Path) -> (Session, AppState) {
    let mut project = Project::new("Themes").expect("project");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("workbook");
    let session = Session::spawn(project, None).expect("session");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.join("projet.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.join("preferences.json"),
        )),
    )
    .expect("state");
    state.use_user_themes_file(dir.join("themes.json"));
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

fn user(state: &mut AppState, graph: NodeId, name: &str) -> ThemeRef {
    state.dispatch(Action::SaveThemeAs {
        graph,
        name: name.into(),
        target: ThemeTarget::Mine,
    });
    state
        .user_themes()
        .entries()
        .into_iter()
        .map(|entry| entry.reference)
        .find(|reference| matches!(reference, ThemeRef::User(_)))
        .expect("a user theme")
}

fn project(state: &mut AppState, graph: NodeId, name: &str) -> ThemeRef {
    state.dispatch(Action::SaveThemeAs {
        graph,
        name: name.into(),
        target: ThemeTarget::Project,
    });
    ThemeRef::project(name)
}

fn source(reference: &ThemeRef) -> ThemeSource {
    ThemeSource::Theme(reference.clone())
}

#[test]
fn each_greyed_control_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    let mine = user(&mut state, g, "Mine one");
    let shared = project(&mut state, g, "Shared");

    // An unmodified built-in: Reset, Rename and Delete are greyed, Use for new
    // graphs and Update are open.
    let light = state.theme_buttons(&source(&ThemeRef::Builtin(0)));
    assert!(light.reset.as_deref().is_some_and(|why| !why.is_empty()));
    assert!(light.rename.is_some() && light.delete.is_some());
    assert!(light.use_for_new.is_none() && light.update.is_none());

    // Once edited, its Reset opens.
    state.dispatch(Action::UpdateTheme {
        theme: ThemeRef::Builtin(0),
        graph: g,
    });
    assert!(state
        .theme_buttons(&source(&ThemeRef::Builtin(0)))
        .reset
        .is_none());

    // A user theme: everything open but Reset.
    let buttons = state.theme_buttons(&source(&mine));
    assert!(buttons.rename.is_none() && buttons.delete.is_none() && buttons.update.is_none());
    assert!(buttons.reset.is_some() && buttons.use_for_new.is_none());

    // A project theme cannot be the default of new graphs.
    let buttons = state.theme_buttons(&source(&shared));
    assert!(buttons.use_for_new.is_some());
    assert!(buttons.rename.is_none() && buttons.delete.is_none());

    // The copied theme is temporary: nothing but Apply.
    let copied = state.theme_buttons(&ThemeSource::Copied);
    assert!(copied.rename.is_some() && copied.update.is_some() && copied.delete.is_some());
    assert!(copied.reset.is_some() && copied.use_for_new.is_some());
}

#[test]
fn deleting_a_user_theme_asks_and_a_project_theme_is_undoable() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    let mine = user(&mut state, g, "Mine one");
    let shared = project(&mut state, g, "Shared");

    state.dispatch(Action::DeleteTheme {
        theme: mine.clone(),
    });
    assert_eq!(state.pending_theme_delete(), Some(&mine));
    assert!(state.user_themes().get(&mine).is_some(), "not deleted yet");
    state.dispatch(Action::CancelDeleteTheme);
    assert!(state.pending_theme_delete().is_none());
    assert!(state.user_themes().get(&mine).is_some(), "Cancel keeps it");
    state.dispatch(Action::DeleteTheme {
        theme: mine.clone(),
    });
    state.dispatch(Action::ConfirmDeleteTheme);
    assert!(state.user_themes().get(&mine).is_none(), "confirmed");

    state.dispatch(Action::DeleteTheme {
        theme: shared.clone(),
    });
    assert!(state.pending_theme_delete().is_none(), "no question asked");
    assert!(state.tree().project_themes().is_empty());
    state.dispatch(Action::Undo);
    assert_eq!(
        state.tree().project_themes().len(),
        1,
        "Undo brings it back"
    );

    // A built-in is never deleted.
    state.dispatch(Action::DeleteTheme {
        theme: ThemeRef::Builtin(1),
    });
    assert!(state.pending_theme_delete().is_none());
}

/// Opens Graph Properties on the Theme tab of `graph`, as the window does.
fn open_properties(state: &mut AppState, graph: NodeId) {
    state.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Theme,
        axis: None,
    });
    state.begin_plot_edit(graph);
}

#[test]
fn apply_theme_from_the_tab_goes_into_the_draft_and_properties_decides() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    let dark = ThemeRef::Builtin(1);

    for keep in [false, true] {
        open_properties(&mut state, g);
        state.select_theme(source(&dark));
        state.dispatch(Action::OpenApplyThemeIntoDraft { graph: g });
        let window = state.apply_theme_window().expect("the window opens");
        assert!(window.into_draft());
        // The preview starts from the draft, not from the model.
        state.plot_edit_mut().unwrap().spec.title = "typed, not committed".into();
        let preview = state.apply_preview().expect("a preview");
        assert_eq!(preview.before.title, "typed, not committed");
        state.dispatch(Action::ConfirmApplyTheme);
        assert!(state.apply_theme_window().is_none(), "the window closes");
        let draft = state.plot_edit().unwrap().spec.clone();
        assert_eq!(draft.background, "#1e1e1e", "the draft carries the theme");
        assert_eq!(draft.title, "typed, not committed", "texts are ticked, so");
        if keep {
            state.close_plot_settings();
            assert_eq!(state.plot_spec(g).unwrap().background, "#1e1e1e");
            // One history step for the whole window.
            state.dispatch(Action::Undo);
            assert_eq!(state.plot_spec(g).unwrap().background, "#aa0000");
        } else {
            state.cancel_plot_settings();
            assert_eq!(
                state.plot_spec(g).unwrap().background,
                "#aa0000",
                "Cancel of Graph Properties gives the graph back"
            );
        }
    }
}

#[test]
fn a_renamed_theme_keeps_its_use_count() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    let mine = user(&mut state, g, "Mine one");
    let shared = project(&mut state, g, "Shared");
    for reference in [&mine, &shared, &shared] {
        state.record_theme_use(reference).unwrap();
    }

    state.dispatch(Action::RenameTheme {
        theme: mine.clone(),
        name: "Renamed".into(),
    });
    assert_eq!(state.user_themes().uses_of(&mine), 1);
    assert_eq!(state.theme_display_name(&mine), "Renamed");

    state.select_theme(source(&shared));
    state.dispatch(Action::RenameTheme {
        theme: shared.clone(),
        name: "Group".into(),
    });
    let renamed = ThemeRef::project("Group");
    assert_eq!(state.user_themes().uses_of(&renamed), 2);
    assert_eq!(state.user_themes().uses_of(&shared), 0);
    assert_eq!(
        state.theme_selection(),
        &source(&renamed),
        "selection follows"
    );
}

#[test]
fn a_refused_name_says_why_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    let one = user(&mut state, g, "One");
    let _two = project(&mut state, g, "Two");
    state.dispatch(Action::OpenRenameTheme { theme: one.clone() });
    assert_eq!(state.rename_theme_window().unwrap().name, "One");
    state.dispatch(Action::RenameTheme {
        theme: one.clone(),
        name: "  ".into(),
    });
    assert!(
        state.rename_theme_window().is_some(),
        "the window stays open"
    );
    assert_eq!(state.theme_display_name(&one), "One");
    // The name of a built-in is taken.
    state.dispatch(Action::RenameTheme {
        theme: one.clone(),
        name: "Light".into(),
    });
    assert_eq!(state.theme_display_name(&one), "One");
    // A project theme cannot take another project theme's name.
    state.dispatch(Action::OpenRenameTheme {
        theme: ThemeRef::project("Two"),
    });
    state.dispatch(Action::RenameTheme {
        theme: ThemeRef::project("Two"),
        name: "Two".into(),
    });
    assert!(state.rename_theme_window().is_none(), "same name is fine");
    // A built-in has no rename window at all.
    state.dispatch(Action::OpenRenameTheme {
        theme: ThemeRef::Builtin(0),
    });
    assert!(state.rename_theme_window().is_none());
}

#[test]
fn use_for_new_graphs_and_the_preferences_row_are_one_setting() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    let mine = user(&mut state, g, "Mine one");
    assert!(state.new_graph_theme().is_none());

    state.dispatch(Action::SetNewGraphTheme(Some(mine.clone())));
    assert_eq!(state.new_graph_theme().map(|(r, _)| r), Some(mine.clone()));
    // The row lists none, the nine built-ins, then the user's themes.
    let choices = state.default_theme_choices();
    assert_eq!(choices.len(), 1 + 9 + 1);
    assert_eq!(choices[0].0, None);
    assert!(choices
        .iter()
        .any(|(r, name)| *r == Some(mine.clone()) && name == "Mine one"));
    assert!(choices
        .iter()
        .all(|(r, _)| !matches!(r, Some(ThemeRef::Project(_)))));

    state.dispatch(Action::SetNewGraphTheme(None));
    assert!(state.new_graph_theme().is_none());
}

#[test]
fn reset_gives_an_edited_built_in_its_original_style_back() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    state.dispatch(Action::UpdateTheme {
        theme: ThemeRef::Builtin(0),
        graph: g,
    });
    assert!(state.user_themes().is_modified(0));
    state.dispatch(Action::ResetTheme {
        theme: ThemeRef::Builtin(0),
    });
    assert!(!state.user_themes().is_modified(0));
}

#[test]
fn a_project_theme_deleted_from_properties_joins_its_step() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let g = graph(&mut state, "#aa0000");
    let shared = project(&mut state, g, "Shared");

    open_properties(&mut state, g);
    state.dispatch(Action::DeleteTheme {
        theme: shared.clone(),
    });
    assert!(state.tree().project_themes().is_empty());
    state.cancel_plot_settings();
    assert_eq!(
        state.tree().project_themes().len(),
        1,
        "Cancel of Graph Properties takes the delete back"
    );

    open_properties(&mut state, g);
    state.dispatch(Action::DeleteTheme { theme: shared });
    state.close_plot_settings();
    assert!(state.tree().project_themes().is_empty(), "OK keeps it");
}

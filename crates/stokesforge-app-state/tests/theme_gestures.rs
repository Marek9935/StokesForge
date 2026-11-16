//! #541: the graph-theme gestures of the state: Copy Theme and Paste Theme, the
//! Apply Theme window (targets, hover-first preview, greyed Apply, one undo
//! step, the use counted), Save Theme As to Mine and to the project, and the
//! picker's ranking.

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::theme_window::{
    paste_families, theme_menu, ThemeSource, ThemeTarget, STYLE_PARTS,
};
use stokesforge_app_state::user_themes::ThemeRef;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PlotSpec, ThemeFamily};
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

/// A new graph with `background` and the lower bound `min` on its bottom axis.
fn graph(state: &mut AppState, background: &str, min: f64) -> NodeId {
    state.dispatch(Action::AddGraph);
    let graph = state.graphs().last().expect("a graph").0;
    let mut spec = state.plot_spec(graph).expect("spec");
    spec.background = background.into();
    for panel in spec.panels.iter_mut() {
        panel.bottom.range_min = Some(min);
    }
    state.dispatch(Action::SetPlotSpec { graph, spec });
    graph
}

fn spec_of(state: &AppState, graph: NodeId) -> PlotSpec {
    state.plot_spec(graph).expect("a graph")
}

fn bottom_min(spec: &PlotSpec) -> Option<f64> {
    spec.panels[0].bottom.range_min
}

#[test]
fn paste_theme_leaves_the_axis_ranges_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let source = graph(&mut state, "#aa0000", 5.0);
    let target = graph(&mut state, "#00aa00", 9.0);
    state.dispatch(Action::CopyTheme { graph: source });
    state.dispatch(Action::PasteTheme {
        graphs: vec![target],
    });
    let pasted = spec_of(&state, target);
    assert_eq!(pasted.background, "#aa0000", "the style came over");
    assert_eq!(bottom_min(&pasted), Some(9.0), "the range stayed");
    assert!(!paste_families().contains(ThemeFamily::AxisRanges));
    assert!(!paste_families().contains(ThemeFamily::Texts));
    assert!(STYLE_PARTS.iter().all(|p| paste_families().contains(*p)));
}

#[test]
fn paste_without_a_copied_theme_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let target = graph(&mut state, "#00aa00", 9.0);
    let undo = state.undo_label();
    state.dispatch(Action::PasteTheme {
        graphs: vec![target],
    });
    assert_eq!(state.undo_label(), undo);
    assert!(theme_menu(false, 1).paste.is_some());
    assert!(theme_menu(true, 1).paste.is_none());
}

#[test]
fn the_menu_greys_each_entry_with_its_reason() {
    assert!(theme_menu(true, 0).copy.is_some(), "no graph");
    assert!(theme_menu(true, 0).apply.is_some());
    assert!(theme_menu(true, 2).copy.is_some(), "Copy takes one graph");
    assert!(theme_menu(true, 2).save.is_some());
    assert!(theme_menu(true, 2).apply.is_none(), "Apply takes several");
    assert!(theme_menu(true, 2).paste.is_none(), "Paste takes several");
    assert!(theme_menu(false, 1).copy.is_none());
    assert!(theme_menu(false, 1).apply.is_none());
}

#[test]
fn the_window_targets_graphs_only() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let one = graph(&mut state, "#aa0000", 1.0);
    let two = graph(&mut state, "#00aa00", 2.0);
    // The root folder is not a graph.
    let book = state.tree().root();
    state.dispatch(Action::OpenApplyTheme {
        graphs: vec![book, one, one, two],
    });
    let window = state.apply_theme_window().expect("opened");
    assert_eq!(window.graphs(), [one, two], "once each, graphs only");
    state.dispatch(Action::CloseApplyTheme);
    state.dispatch(Action::OpenApplyTheme { graphs: vec![book] });
    assert!(state.apply_theme_window().is_none(), "no graph, no window");
}

#[test]
fn apply_is_greyed_with_its_reason_when_nothing_is_checked() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let target = graph(&mut state, "#00aa00", 9.0);
    state.dispatch(Action::OpenApplyTheme {
        graphs: vec![target],
    });
    let window = state.apply_theme_window_mut().unwrap();
    window.set_style(false);
    for family in [
        ThemeFamily::SizeAndPage,
        ThemeFamily::Texts,
        ThemeFamily::AxisRanges,
    ] {
        window.set_family(family, false);
    }
    let reason = window.apply_blocked().expect("greyed");
    assert!(
        !reason.is_empty() && !reason.contains("theme-apply"),
        "{reason}"
    );
    let undo = state.undo_label();
    state.dispatch(Action::ConfirmApplyTheme);
    assert_eq!(state.undo_label(), undo, "nothing applied");
    assert!(
        state.apply_theme_window().is_some(),
        "the window stays open"
    );
}

#[test]
fn one_undo_takes_an_apply_on_two_graphs_back_and_the_use_is_counted() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let one = graph(&mut state, "#aa0000", 1.0);
    let two = graph(&mut state, "#00aa00", 2.0);
    let (before_one, before_two) = (spec_of(&state, one), spec_of(&state, two));
    state.dispatch(Action::OpenApplyTheme {
        graphs: vec![one, two],
    });
    let dark = ThemeSource::Theme(ThemeRef::Builtin(1));
    state.apply_theme_window_mut().unwrap().choose(dark);
    state.dispatch(Action::ConfirmApplyTheme);
    assert!(state.apply_theme_window().is_none(), "closed on Apply");
    assert_ne!(spec_of(&state, one), before_one);
    assert_ne!(spec_of(&state, two), before_two);
    assert_eq!(state.user_themes().uses_of(&ThemeRef::Builtin(1)), 1);
    state.dispatch(Action::Undo);
    assert_eq!(spec_of(&state, one), before_one, "one Ctrl+Z, graph one");
    assert_eq!(spec_of(&state, two), before_two, "one Ctrl+Z, graph two");
}

#[test]
fn hovering_previews_without_changing_the_choice() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let target = graph(&mut state, "#aa0000", 1.0);
    state.dispatch(Action::OpenApplyTheme {
        graphs: vec![target],
    });
    let light = ThemeSource::Theme(ThemeRef::Builtin(0));
    let dark = ThemeSource::Theme(ThemeRef::Builtin(1));
    let resting = state.apply_preview().expect("preview");
    let window = state.apply_theme_window_mut().unwrap();
    let ticked = window.families();
    let flipped = window.families_if_family_flipped(ThemeFamily::Axes);
    window.set_hover(Some(dark.clone()), Some(flipped), Some(true));
    assert_eq!(window.chosen(), &light, "the choice is the click's");
    assert_eq!(window.families(), ticked);
    assert!(!window.add_missing());
    assert_eq!(window.shown_theme(), &dark);
    assert_eq!(window.shown_families(), flipped);
    let hovered = state.apply_preview().expect("preview");
    assert_ne!(
        hovered.after, resting.after,
        "the preview follows the hover"
    );
    assert_eq!(hovered.before, resting.before);
    state
        .apply_theme_window_mut()
        .unwrap()
        .set_hover(None, None, None);
    assert_eq!(state.apply_preview().expect("preview"), resting);
}

#[test]
fn light_and_dark_stay_first_whatever_the_use() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let target = graph(&mut state, "#aa0000", 1.0);
    for _ in 0..3 {
        state.record_theme_use(&ThemeRef::Builtin(4)).unwrap();
    }
    let mine = state.picker_entries().mine;
    assert_eq!(mine[0].source, ThemeSource::Theme(ThemeRef::Builtin(0)));
    assert_eq!(mine[1].source, ThemeSource::Theme(ThemeRef::Builtin(1)));
    assert_eq!(
        mine[2].source,
        ThemeSource::Theme(ThemeRef::Builtin(4)),
        "then the most used"
    );
    state.dispatch(Action::CopyTheme { graph: target });
    let entries = state.picker_entries();
    assert_eq!(entries.copied.expect("copied").source, ThemeSource::Copied);
}

#[test]
fn save_theme_as_to_the_project_is_one_undo_step() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let target = graph(&mut state, "#aa0000", 1.0);
    state.dispatch(Action::OpenSaveTheme { graph: target });
    assert!(state.save_theme_window().is_some());
    state.dispatch(Action::SaveThemeAs {
        graph: target,
        name: "Thesis".into(),
        target: ThemeTarget::Project,
    });
    assert!(state.save_theme_window().is_none(), "closed on success");
    assert_eq!(state.tree().project_themes().len(), 1);
    assert_eq!(state.picker_entries().project.len(), 1);
    state.dispatch(Action::Undo);
    assert!(state.tree().project_themes().is_empty());
}

#[test]
fn save_theme_as_to_mine_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let target = graph(&mut state, "#aa0000", 1.0);
    state.dispatch(Action::SaveThemeAs {
        graph: target,
        name: "Mine one".into(),
        target: ThemeTarget::Mine,
    });
    assert!(
        state.tree().project_themes().is_empty(),
        "not in the project"
    );
    drop(state);
    // A new session of the application reads the same file.
    let (_session, again) = start(dir.path());
    let names: Vec<String> = again
        .picker_entries()
        .mine
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert!(names.contains(&"Mine one".to_string()), "{names:?}");
}

#[test]
fn a_refused_name_says_why_keeps_the_window_open_and_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let target = graph(&mut state, "#aa0000", 1.0);
    state.dispatch(Action::OpenSaveTheme { graph: target });
    state.dispatch(Action::SaveThemeAs {
        graph: target,
        name: "  ".into(),
        target: ThemeTarget::Project,
    });
    assert!(state.tree().project_themes().is_empty());
    assert!(state.save_theme_window().is_some());
    assert!(state
        .dialog_error()
        .is_some_and(|e| !e.contains("error-theme")));
    state.dispatch(Action::SaveThemeAs {
        graph: target,
        name: "Thesis".into(),
        target: ThemeTarget::Project,
    });
    state.dispatch(Action::OpenSaveTheme { graph: target });
    state.dispatch(Action::SaveThemeAs {
        graph: target,
        name: " thesis ".into(),
        target: ThemeTarget::Project,
    });
    assert_eq!(state.tree().project_themes().len(), 1);
    assert!(state.save_theme_window().is_some());
    assert!(state.dialog_error().is_some());
    // Built-in names are taken in Mine.
    state.dispatch(Action::SaveThemeAs {
        graph: target,
        name: "Dark".into(),
        target: ThemeTarget::Mine,
    });
    assert!(state.dialog_error().is_some());
    assert!(!state
        .picker_entries()
        .mine
        .iter()
        .any(|e| matches!(e.source, ThemeSource::Theme(ThemeRef::User(_)))));
}

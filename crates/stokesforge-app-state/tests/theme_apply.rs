//! #538: applying a graph theme to graphs through `Action::ApplyTheme`: one
//! undo step for N graphs, no step when nothing changes, the missing layers
//! option, and the placement switch of the Size and page family (a free theme
//! never leaves two layers on one frame).

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{
    FreeFrame, GraphTheme, LayerPlacement, PanelSpec, PlotSpec, ThemeFamilies, ThemeFamily, THEMES,
    TRANSPARENT,
};
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

/// True if the next Undo would undo a theme application.
fn last_step_is_a_theme(state: &AppState) -> bool {
    state
        .undo_label()
        .is_some_and(|label| label.contains("Apply Theme"))
}

fn graphs_of(state: &AppState, count: usize) -> Vec<NodeId> {
    let graphs: Vec<NodeId> = state.graphs().into_iter().map(|(id, _)| id).collect();
    assert_eq!(graphs.len(), count);
    graphs
}

fn spec_of(state: &AppState, graph: NodeId) -> PlotSpec {
    state.plot_spec(graph).expect("a graph")
}

fn apply(
    state: &mut AppState,
    theme: &GraphTheme,
    families: ThemeFamilies,
    graphs: &[NodeId],
    add_missing_layers: bool,
) {
    state.dispatch(Action::ApplyTheme {
        theme: theme.clone(),
        families,
        graphs: graphs.to_vec(),
        add_missing_layers,
    });
}

/// A theme of `layers` layers laid out in a row, each with its own layer
/// background, free-placed with one distinct frame per layer when `free`.
fn theme_with_layers(layers: usize, free: bool) -> GraphTheme {
    let mut spec = PlotSpec::empty("theme");
    spec.background = "#102030".into();
    for i in 0..layers {
        let mut panel = PanelSpec::at(0, i);
        panel.layer_background = format!("#00000{i}");
        if free {
            panel.free_frame = Some(FreeFrame {
                left: 100.0 * i as f64,
                top: 20.0,
                width: 80.0,
                height: 60.0,
                stack: i as i32,
            });
        }
        if i == 0 {
            spec.panels = panel.into();
        } else {
            spec.panels.push(panel);
        }
    }
    if free {
        spec.layer_placement = LayerPlacement::Free;
    } else {
        spec.frame_widths = vec![150.0; layers];
        spec.frame_heights = vec![110.0];
    }
    GraphTheme::capture("T", &spec)
}

fn distinct_frames(spec: &PlotSpec) -> bool {
    let frames: Vec<(u64, u64)> = spec
        .panels
        .iter()
        .filter(|p| p.overlay.is_none())
        .map(|p| {
            let f = p.free_frame.expect("a free frame");
            (f.left.to_bits(), f.top.to_bits())
        })
        .collect();
    frames
        .iter()
        .enumerate()
        .all(|(i, a)| frames[i + 1..].iter().all(|b| a != b))
}

#[test]
fn a_theme_on_two_graphs_is_one_undo_step() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::AddGraph);
    let graphs = graphs_of(&state, 2);
    let before: Vec<PlotSpec> = graphs.iter().map(|g| spec_of(&state, *g)).collect();
    let dark = GraphTheme::from_colors("Dark", &THEMES[1]);
    apply(&mut state, &dark, ThemeFamilies::all(), &graphs, false);
    assert!(graphs
        .iter()
        .all(|g| spec_of(&state, *g).background == "#1e1e1e"));
    assert!(last_step_is_a_theme(&state));
    state.dispatch(Action::Undo);
    let after: Vec<PlotSpec> = graphs.iter().map(|g| spec_of(&state, *g)).collect();
    assert_eq!(after, before, "one Ctrl+Z restores both graphs");
    assert!(!last_step_is_a_theme(&state));
    state.dispatch(Action::Redo);
    assert!(graphs
        .iter()
        .all(|g| spec_of(&state, *g).background == "#1e1e1e"));
}

#[test]
fn a_theme_that_changes_nothing_adds_no_history_step() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::AddGraph);
    let graphs = graphs_of(&state, 2);
    let dark = GraphTheme::from_colors("Dark", &THEMES[1]);
    apply(&mut state, &dark, ThemeFamilies::all(), &graphs, false);
    let original = spec_of(&state, graphs[0]);
    // Same theme again: nothing differs, nothing is pushed, nothing is said.
    let messages = state.recorded_count();
    apply(&mut state, &dark, ThemeFamilies::all(), &graphs, false);
    assert_eq!(state.recorded_count(), messages);
    // No family at all: nothing to copy either.
    apply(&mut state, &dark, ThemeFamilies::none(), &graphs, false);
    assert_eq!(spec_of(&state, graphs[0]), original);
    state.dispatch(Action::Undo);
    assert!(!last_step_is_a_theme(&state));
    assert_eq!(spec_of(&state, graphs[0]).background, TRANSPARENT);
}

#[test]
fn a_node_that_is_not_a_graph_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    state.dispatch(Action::AddGraph);
    let graph = graphs_of(&state, 1)[0];
    let root = state.tree().root();
    let dark = GraphTheme::from_colors("Dark", &THEMES[1]);
    apply(&mut state, &dark, ThemeFamilies::all(), &[root], false);
    assert!(!last_step_is_a_theme(&state));
    apply(
        &mut state,
        &dark,
        ThemeFamilies::all(),
        &[root, graph, graph],
        false,
    );
    assert_eq!(spec_of(&state, graph).background, "#1e1e1e");
    state.dispatch(Action::Undo);
    assert!(
        !last_step_is_a_theme(&state),
        "a graph listed twice is still one entry"
    );
}

#[test]
fn missing_layers_are_created_only_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    state.dispatch(Action::AddGraph);
    let graph = graphs_of(&state, 1)[0];
    assert_eq!(spec_of(&state, graph).panels.len(), 1);
    let theme = theme_with_layers(2, false);
    let layer_background = ThemeFamilies::only(ThemeFamily::Background);
    // Off (the default): the existing layer is styled, none is created.
    apply(&mut state, &theme, layer_background, &[graph], false);
    let spec = spec_of(&state, graph);
    assert_eq!(spec.panels.len(), 1);
    assert_eq!(spec.panels.as_slice()[0].layer_background, "#000000");
    state.dispatch(Action::Undo);
    // On: the second layer is created empty, then styled like the theme's.
    apply(&mut state, &theme, layer_background, &[graph], true);
    let spec = spec_of(&state, graph);
    assert_eq!(spec.panels.len(), 2);
    let added = &spec.panels.as_slice()[1];
    assert!(added.series.is_empty());
    assert_eq!((added.row, added.column), (0, 1), "beside the first layer");
    assert_eq!(added.layer_background, "#000001");
    state.dispatch(Action::Undo);
    assert_eq!(spec_of(&state, graph).panels.len(), 1, "one undo, no layer");
}

#[test]
fn a_stacked_theme_adds_its_layer_below() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    state.dispatch(Action::AddGraph);
    let graph = graphs_of(&state, 1)[0];
    let mut stacked = PlotSpec::empty("fit");
    stacked.panels = PanelSpec::at(0, 0).into();
    stacked.panels.push(PanelSpec::at(1, 0));
    let theme = GraphTheme::capture("Fit", &stacked);
    apply(
        &mut state,
        &theme,
        ThemeFamilies::none().with(ThemeFamily::Texts),
        &[graph],
        true,
    );
    let spec = spec_of(&state, graph);
    let added = &spec.panels.as_slice()[1];
    assert_eq!((added.row, added.column), (1, 0));
}

/// A graph of three layers in a row on the regular grid.
fn three_layer_grid(state: &mut AppState) -> NodeId {
    state.dispatch(Action::AddGraph);
    let graph = graphs_of(state, 1)[0];
    let mut spec = spec_of(state, graph);
    spec.panels.push(PanelSpec::at(0, 1));
    spec.panels.push(PanelSpec::at(0, 2));
    spec.frame_widths = vec![200.0; 3];
    state.dispatch(Action::SetPlotSpec { graph, spec });
    graph
}

#[test]
fn a_free_theme_on_a_grid_graph_never_stacks_two_layers() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let graph = three_layer_grid(&mut state);
    // The theme has fewer layers (2) than the target (3): the surplus is offset.
    let theme = theme_with_layers(2, true);
    let size = ThemeFamilies::only(ThemeFamily::SizeAndPage);
    apply(&mut state, &theme, size, &[graph], false);
    let spec = spec_of(&state, graph);
    assert_eq!(spec.layer_placement, LayerPlacement::Free);
    assert_eq!(spec.panels.len(), 3);
    assert!(distinct_frames(&spec), "no two layers share a frame");
    let frame = |i: usize| spec.panels.as_slice()[i].free_frame.unwrap();
    assert_eq!(frame(0).left, 0.0);
    assert_eq!(frame(1).left, 100.0);
    assert!(frame(2).left > frame(1).left + frame(1).width);
    // One undo returns to the regular grid.
    state.dispatch(Action::Undo);
    assert_eq!(spec_of(&state, graph).layer_placement, LayerPlacement::Grid);
}

#[test]
fn the_placement_follows_only_with_the_size_family() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let graph = three_layer_grid(&mut state);
    let theme = theme_with_layers(2, true);
    apply(
        &mut state,
        &theme,
        ThemeFamilies::all().without(ThemeFamily::SizeAndPage),
        &[graph],
        false,
    );
    let spec = spec_of(&state, graph);
    assert_eq!(spec.layer_placement, LayerPlacement::Grid);
    assert_eq!(spec.background, "#102030");
}

#[test]
fn a_grid_theme_on_a_free_graph_goes_back_to_the_grid() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(dir.path());
    let graph = three_layer_grid(&mut state);
    let free = theme_with_layers(3, true);
    let size = ThemeFamilies::only(ThemeFamily::SizeAndPage);
    apply(&mut state, &free, size, &[graph], false);
    assert_eq!(spec_of(&state, graph).layer_placement, LayerPlacement::Free);
    let grid = theme_with_layers(3, false);
    apply(&mut state, &grid, size, &[graph], false);
    let spec = spec_of(&state, graph);
    assert_eq!(spec.layer_placement, LayerPlacement::Grid);
    assert_eq!(spec.frame_widths, [150.0; 3]);
}

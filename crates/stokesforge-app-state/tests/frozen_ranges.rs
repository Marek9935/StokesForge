//! #529: a new graph computes its automatic axis ranges once, with the user's
//! margins, and writes them (From / To, `auto_range = false`); it never moves
//! afterwards. The ranges ride in the very command of the gesture: one history
//! entry.

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{Column, ColumnRole, NodeId, Project};
use stokesforge_plot::{AxisSpec, PlotSpec, PlotType};
use stokesforge_render::AutoMargins;
use stokesforge_session::Session;

const TOP_10: AutoMargins = AutoMargins {
    left: 0.0,
    right: 0.0,
    bottom: 1.0,
    top: 10.0,
};

/// A sheet with X = 1..3 and Y = 4..6, a state on a throwaway preferences
/// file whose margins are `margins`.
fn start(dir: &Path, margins: AutoMargins) -> (Session, AppState, NodeId, [NodeId; 4]) {
    let mut project = Project::new("Ranges").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("book");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let mut add = |name: &str, values: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .expect("column")
    };
    let x = add("X", &[1.0, 2.0, 3.0]);
    let y = add("Y", &[4.0, 5.0, 6.0]);
    let x2 = add("X2", &[100.0, 200.0, 300.0]);
    let y2 = add("Y2", &[0.0, 0.5, 1.0]);
    let session = Session::spawn(project, None).expect("session");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.join("projet.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.join("preferences.json"),
        )),
    )
    .expect("state");
    state.dispatch(Action::SetAutoMargins(margins));
    state.dispatch(Action::SetColumnRole {
        column: x,
        role: ColumnRole::X,
    });
    (session, state, sheet, [x, y, x2, y2])
}

fn plot(state: &mut AppState, sheet: NodeId, columns: &[NodeId]) -> NodeId {
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: columns.to_vec(),
        plot_type: PlotType::Line,
    });
    state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created")
}

fn bounds(axis: &AxisSpec) -> (f64, f64) {
    (axis.range_min.expect("From"), axis.range_max.expect("To"))
}

fn close(a: (f64, f64), b: (f64, f64)) {
    assert!(
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9,
        "{a:?} != {b:?}"
    );
}

/// Plotting columns writes the ranges drawn with the user's margins, in the
/// creation step itself.
#[test]
fn plotting_columns_writes_the_ranges_with_the_user_margins() {
    let dir = tempfile::tempdir().expect("dir");
    let (_session, mut state, sheet, [x, y, ..]) = start(dir.path(), TOP_10);
    let graph = plot(&mut state, sheet, &[x, y]);
    let spec: PlotSpec = state.plot_spec(graph).expect("spec");
    let panel = &spec.panels[0];
    assert!(!panel.bottom.auto_range && !panel.left.auto_range);
    close(bounds(&panel.bottom), (1.0, 3.0));
    // 4 - 1 % of 2, 6 + 10 % of 2.
    close(bounds(&panel.left), (3.98, 6.2));
    assert!(
        panel.right.auto_range,
        "no data on the right axis: untouched"
    );
    // One history entry: a single Undo removes the graph.
    state.dispatch(Action::Undo);
    assert!(state.plot_spec(graph).is_none());
}

/// The preference changes later do not move a written range.
#[test]
fn a_written_range_does_not_move_with_the_preferences() {
    let dir = tempfile::tempdir().expect("dir");
    let (_session, mut state, sheet, [x, y, ..]) = start(dir.path(), TOP_10);
    let graph = plot(&mut state, sheet, &[x, y]);
    state.dispatch(Action::SetAutoMargins(AutoMargins {
        left: 20.0,
        right: 20.0,
        bottom: 20.0,
        top: 20.0,
    }));
    let spec = state.plot_spec(graph).expect("spec");
    close(bounds(&spec.panels[0].left), (3.98, 6.2));
}

/// A layer added empty stays on Auto (no data to frame: not `0..1`) and is
/// frozen by the first command that gives it data.
#[test]
fn an_empty_layer_stays_auto_and_freezes_when_data_arrive() {
    let dir = tempfile::tempdir().expect("dir");
    let (_session, mut state, sheet, [x, y, ..]) = start(dir.path(), TOP_10);
    let graph = plot(&mut state, sheet, &[x, y]);
    state.dispatch(Action::AddLayer {
        graph,
        panel: 0,
        x_top: true,
        y_right: true,
    });
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 2);
    let layer = &spec.panels[1];
    assert!(
        layer.bottom.auto_range && layer.left.auto_range,
        "an empty layer is not frozen on 0..1"
    );
    // The first layer is still the one that was written.
    close(bounds(&spec.panels[0].left), (3.98, 6.2));

    // Data arrive in the layer: the next write freezes it, aligned on the
    // covered layer (#174) like the drawing.
    let mut edited = spec.clone();
    edited
        .panels
        .get_mut(1)
        .expect("layer")
        .series
        .push(spec.panels[0].series[0].clone());
    state.begin_plot_edit(graph);
    state.plot_edit_mut().expect("draft").spec = edited;
    state.commit_plot_edit();
    let spec = state.plot_spec(graph).expect("spec");
    let layer = &spec.panels[1];
    assert!(!layer.bottom.auto_range && !layer.left.auto_range);
    close(bounds(&layer.bottom), bounds(&spec.panels[0].bottom));
}

/// A Y2 layer created by the same gesture is frozen with the graph.
#[test]
fn a_y2_layer_created_by_the_gesture_is_frozen_too() {
    let dir = tempfile::tempdir().expect("dir");
    let (_session, mut state, sheet, [x, y, x2, y2]) = start(dir.path(), TOP_10);
    for (column, role) in [(x2, ColumnRole::X2), (y2, ColumnRole::Y2)] {
        state.dispatch(Action::SetColumnRole { column, role });
    }
    let graph = plot(&mut state, sheet, &[x, y, x2, y2]);
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 2, "one more layer for Y2");
    for panel in spec.panels.iter() {
        assert!(
            !panel.bottom.auto_range && !panel.left.auto_range,
            "{panel:?}"
        );
    }
    // The Y2 data (0..1) fit in the frozen range, aligned on the first layer.
    let (low, high) = bounds(&spec.panels[1].left);
    assert!(low <= -0.01 && high >= 1.1, "{low} {high}");
}

/// A file opened at startup gets the same note as `File > Open`: the project
/// is modified and the orange line counts the graphs written; zero graphs
/// leaves a clean project and no message.
#[test]
fn ranges_written_at_opening_mark_the_project_modified() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, _sheet, _columns) = start(dir.path(), TOP_10);
    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_calculations: Vec::new(),
        discarded_corrections: Vec::new(),
    });
    state.mark_ranges_written(0);
    assert!(
        !state.window_title().contains('•'),
        "clean when nothing was written"
    );
    state.mark_ranges_written(2);
    assert!(state.window_title().contains('•'));
    let status = state.status().expect("the orange message");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Warning
    );
    assert!(status.text.contains("2 graphs"), "{}", status.text);
}

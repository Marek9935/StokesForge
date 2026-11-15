//! #529, L5: a series added to a graph that goes beyond the written ranges
//! widens only the exceeded side(s), at once, with the margins, as a history
//! step of its own: the first Ctrl+Z restores the ranges and keeps the series,
//! the second removes it. An orange message says so. Inside the ranges, one
//! entry and no message.

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState, StatusLevel};
use stokesforge_app_state::widen::widen_to_data;
use stokesforge_commands::{HistoryLabel, PlotEditKind};
use stokesforge_core::{Column, ColumnRole, NodeId, Project};
use stokesforge_plot::{AxisScale, PlotSpec, PlotType};
use stokesforge_render::AutoMargins;
use stokesforge_session::Session;

const TOP_10: AutoMargins = AutoMargins {
    left: 0.0,
    right: 0.0,
    bottom: 1.0,
    top: 10.0,
};

struct Bench {
    _session: Session,
    state: AppState,
    graph: NodeId,
    sheet: NodeId,
    /// Y columns of the sheet: `tall` (4, 5, 12), `inside` (4.5, 5, 5.5),
    /// `wide_x` against X = 1..3, plus X2 and Y2 for the overlay.
    tall: NodeId,
    inside: NodeId,
    y2_tall: NodeId,
}

fn start(dir: &Path) -> Bench {
    let mut project = Project::new("Widen").expect("project");
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
    let tall = add("Tall", &[4.0, 5.0, 12.0]);
    let inside = add("Inside", &[4.5, 5.0, 5.5]);
    let x2 = add("X2", &[100.0, 200.0, 300.0]);
    let y2 = add("Y2", &[0.0, 0.5, 1.0]);
    let y2_tall = add("Y2tall", &[0.0, 0.5, 5.0]);
    let session = Session::spawn(project, None).expect("session");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.join("projet.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.join("preferences.json"),
        )),
    )
    .expect("state");
    state.dispatch(Action::SetAutoMargins(TOP_10));
    state.dispatch(Action::SetColumnRole {
        column: x,
        role: ColumnRole::X,
    });
    for (column, role) in [(x2, ColumnRole::X2), (y2, ColumnRole::Y2)] {
        state.dispatch(Action::SetColumnRole { column, role });
    }
    state.dispatch(Action::SetColumnRole {
        column: y2_tall,
        role: ColumnRole::Y2,
    });
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![x, y],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph");
    Bench {
        _session: session,
        state,
        graph,
        sheet,
        tall,
        inside,
        y2_tall,
    }
}

fn spec(bench: &Bench) -> PlotSpec {
    bench.state.plot_spec(bench.graph).expect("spec")
}

fn left(spec: &PlotSpec) -> (f64, f64) {
    (
        spec.panels[0].left.range_min.expect("From"),
        spec.panels[0].left.range_max.expect("To"),
    )
}

fn first(spec: &mut PlotSpec) -> &mut stokesforge_plot::PanelSpec {
    spec.panels.get_mut(0).expect("layer")
}

/// The graph as it was before a first series: nothing known of its data.
fn no_data(spec: &PlotSpec) -> PlotSpec {
    let mut before = spec.clone();
    before.panels.get_mut(0).expect("layer").series.clear();
    before
}

fn close(a: (f64, f64), b: (f64, f64)) {
    assert!(
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9,
        "{a:?} != {b:?}"
    );
}

fn add_series(bench: &mut Bench, column: NodeId) {
    bench.state.dispatch(Action::AddSeriesToGraph {
        graph: bench.graph,
        panel: 0,
        columns: vec![column],
        plot_type: None,
    });
}

/// A series beyond the top moves the top only (the bottom and X keep their
/// written values); the widening is a second step; the first Ctrl+Z restores
/// the ranges and keeps the series, the second removes it; an orange message
/// names the axis.
#[test]
fn a_series_beyond_the_top_widens_only_the_top_in_a_second_step() {
    let dir = tempfile::tempdir().expect("dir");
    let mut bench = start(dir.path());
    let before = spec(&bench);
    close(left(&before), (3.98, 6.2));
    let tall = bench.tall;
    add_series(&mut bench, tall);
    let after = spec(&bench);
    assert_eq!(after.panels[0].series.len(), 2);
    // Data 4..12: 12 + 10 % of 8; the bottom (data min 4 >= 3.98) stays.
    close(left(&after), (3.98, 12.8));
    assert_eq!(
        after.panels[0].bottom, before.panels[0].bottom,
        "X did not move"
    );
    let status = bench.state.status().expect("message");
    assert_eq!(status.level, StatusLevel::Warning);
    assert!(
        status.text.contains("Widened the Y axis") && status.text.contains("Ctrl+Z"),
        "{}",
        status.text
    );
    assert_eq!(
        bench.state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(PlotEditKind::WidenRanges))
    );
    bench.state.dispatch(Action::Undo);
    let restored = spec(&bench);
    assert_eq!(restored.panels[0].series.len(), 2, "the series stays");
    close(left(&restored), (3.98, 6.2));
    bench.state.dispatch(Action::Undo);
    assert_eq!(spec(&bench).panels[0].series.len(), 1, "then it goes");
}

/// Inside the ranges: one entry, no message, nothing moves.
#[test]
fn a_series_inside_the_ranges_adds_one_entry_and_no_message() {
    let dir = tempfile::tempdir().expect("dir");
    let mut bench = start(dir.path());
    let before = spec(&bench);
    let inside = bench.inside;
    add_series(&mut bench, inside);
    let after = spec(&bench);
    assert_eq!(after.panels[0].series.len(), 2);
    close(left(&after), left(&before));
    assert_ne!(
        bench.state.status().map(|m| m.level),
        Some(StatusLevel::Warning)
    );
    assert_ne!(
        bench.state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(PlotEditKind::WidenRanges))
    );
    bench.state.dispatch(Action::Undo);
    assert_eq!(spec(&bench).panels[0].series.len(), 1, "one entry");
}

/// A Y2 layer created by the add gets its own fresh ranges: nothing exceeds,
/// no second step. A series added to it later that goes beyond widens its own
/// Y only; the covered layer does not move.
#[test]
fn a_series_beyond_an_overlay_axis_widens_that_axis() {
    let dir = tempfile::tempdir().expect("dir");
    let mut bench = start(dir.path());
    let y2 = bench
        .state
        .tree()
        .child_by_name(bench.sheet, "Y2")
        .unwrap()
        .unwrap();
    add_series(&mut bench, y2);
    let with_layer = spec(&bench);
    assert_eq!(with_layer.panels.len(), 2);
    assert_ne!(
        bench.state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(PlotEditKind::WidenRanges)),
        "a fresh layer is framed, not widened"
    );
    bench.state.dispatch(Action::AddSeriesToGraph {
        graph: bench.graph,
        panel: 1,
        columns: vec![bench.y2_tall],
        plot_type: None,
    });
    let after = spec(&bench);
    assert_eq!(
        after.panels[0].left, with_layer.panels[0].left,
        "the covered layer keeps its ranges"
    );
    assert!(
        after.panels[1].left.range_max.unwrap() > with_layer.panels[1].left.range_max.unwrap(),
        "the overlay's Y widened"
    );
    assert_eq!(
        bench.state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(PlotEditKind::WidenRanges))
    );
}

/// An axis that is still on Auto (no data to frame) follows its data: no
/// widening is needed nor written.
#[test]
fn nothing_is_written_on_an_axis_that_is_still_auto() {
    let dir = tempfile::tempdir().expect("dir");
    let bench = start(dir.path());
    let mut spec = spec(&bench);
    first(&mut spec).left.auto_range = true;
    assert!(widen_to_data(bench.state.tree(), &no_data(&spec), &spec, TOP_10).is_none());
}

/// An inverted axis widens on the side the data exceeds, and keeps its order.
#[test]
fn an_inverted_axis_widens_on_the_exceeded_side() {
    let dir = tempfile::tempdir().expect("dir");
    let bench = start(dir.path());
    let mut spec = spec(&bench);
    // Y = 4..6, stored high first; 5.0 cuts the top. The Bottom margin (1 %)
    // is the edge of the frame, so on this axis it sits on the high bound.
    first(&mut spec).left.inverted = true;
    first(&mut spec).left.range_min = Some(5.0);
    first(&mut spec).left.range_max = Some(3.98);
    let widened =
        widen_to_data(bench.state.tree(), &no_data(&spec), &spec, TOP_10).expect("exceeded");
    let axis = &widened.spec.panels[0].left;
    assert!(axis.inverted);
    close(
        (axis.range_min.unwrap(), axis.range_max.unwrap()),
        (6.02, 3.98),
    );
}

/// A logarithmic axis widens to the next decade, margins ignored.
#[test]
fn a_log_axis_widens_to_the_decade() {
    let dir = tempfile::tempdir().expect("dir");
    let mut bench = start(dir.path());
    let tall = bench.tall;
    add_series(&mut bench, tall);
    let mut spec = spec(&bench);
    first(&mut spec).left.scale = AxisScale::Log;
    first(&mut spec).left.range_min = Some(1.0);
    first(&mut spec).left.range_max = Some(10.0);
    let widened =
        widen_to_data(bench.state.tree(), &no_data(&spec), &spec, TOP_10).expect("exceeded");
    let axis = &widened.spec.panels[0].left;
    close(
        (axis.range_min.unwrap(), axis.range_max.unwrap()),
        (1.0, 100.0),
    );
}

/// A graph zoomed by hand keeps its zoom when the series added stays inside the
/// data it had: only a series that takes the data further widens.
#[test]
fn a_hand_zoom_is_kept_when_the_new_series_adds_nothing_beyond_the_data() {
    let dir = tempfile::tempdir().expect("dir");
    let mut bench = start(dir.path());
    let mut zoomed = spec(&bench);
    first(&mut zoomed).left.range_min = Some(4.4);
    first(&mut zoomed).left.range_max = Some(5.2);
    bench.state.dispatch(Action::SetPlotSpec {
        graph: bench.graph,
        spec: zoomed,
    });
    let inside = bench.inside;
    add_series(&mut bench, inside);
    close(left(&spec(&bench)), (4.4, 5.2));
    assert_ne!(
        bench.state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(PlotEditKind::WidenRanges))
    );
}

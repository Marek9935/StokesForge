//! #529, L4: opening a project writes the automatic axis ranges once (an old
//! file's Auto axes, computed with the new rule and the user's margins). No
//! history entry, no format change; the project counts as modified only when
//! an axis was written, and an orange message says how many graphs.

use std::path::Path;

use stokesforge_app_state::state::{Action, AppState, StatusLevel, UnsavedChoice, UnsavedGesture};
use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
use stokesforge_plot::{AxisSpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::AutoMargins;
use stokesforge_session::Session;

const TOP_10: AutoMargins = AutoMargins {
    left: 0.0,
    right: 0.0,
    bottom: 1.0,
    top: 10.0,
};

/// A project file with one graph (X = 1..3, Y = 4..6) whose axes are
/// `auto` (an old file) or already written.
struct Fixture {
    path: std::path::PathBuf,
    graph: NodeId,
}

fn write_file(dir: &Path, auto: bool, stale_formula: bool) -> Fixture {
    let mut project = Project::new("Old").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("book");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("x");
    project
        .set_column_meta(
            x,
            ColumnMeta {
                role: ColumnRole::X,
                ..Default::default()
            },
        )
        .expect("role");
    // With `stale_formula`, Y holds a cache (1, 1, 1) that its formula
    // (X * 10 -> 10, 20, 30) does not give back: the load recalculates it.
    let cached = if stale_formula {
        [1.0, 1.0, 1.0]
    } else {
        [4.0, 5.0, 6.0]
    };
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&cached))
        .expect("y");
    if stale_formula {
        let formula = project
            .tree()
            .formula_from_letters(y, "A*10")
            .expect("formula");
        project
            .set_column_meta(
                y,
                ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            )
            .expect("meta");
    }
    let mut panel = PanelSpec::default();
    panel.series.push(SeriesSpec::new_xy(x, y));
    if !auto {
        panel.bottom = written(1.0, 3.0);
        panel.left = written(3.0, 7.0);
    }
    let spec = PlotSpec {
        panels: Panels::new(vec![panel]),
        ..PlotSpec::default()
    };
    let graph = project
        .add_graph(root, Some("Graph1"), spec)
        .expect("graph");
    let path = dir.join("old.stf");
    stokesforge_persistence::save(&project, &path).expect("save");
    Fixture { path, graph }
}

fn written(low: f64, high: f64) -> AxisSpec {
    AxisSpec {
        auto_range: false,
        range_min: Some(low),
        range_max: Some(high),
        ..AxisSpec::default()
    }
}

fn open(dir: &Path, fixture: &Fixture) -> (Session, AppState) {
    let session = Session::spawn(Project::new("Empty").expect("project"), None).expect("session");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.join("other.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(
            dir.join("preferences.json"),
        )),
    )
    .expect("state");
    state.dispatch(Action::SetAutoMargins(TOP_10));
    state.dispatch(Action::OpenProject {
        path: fixture.path.display().to_string(),
        pending_edits: false,
    });
    (session, state)
}

/// Does leaving now ask what to do with changes?
fn modified(state: &mut AppState) -> bool {
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    let asked = state.unsaved_pending() == Some(&UnsavedGesture::Quit);
    if asked {
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    }
    asked
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

/// An old file's Auto axes are written at opening, with the user's margins,
/// with no history entry; the project is modified and an orange message counts
/// the graphs.
#[test]
fn opening_an_old_file_writes_its_auto_ranges() {
    let dir = tempfile::tempdir().expect("dir");
    let fixture = write_file(dir.path(), true, false);
    let (_session, mut state) = open(dir.path(), &fixture);
    let spec = state.plot_spec(fixture.graph).expect("spec");
    let panel = &spec.panels[0];
    assert!(!panel.bottom.auto_range && !panel.left.auto_range);
    close(panel.bottom.range_min.unwrap(), 1.0);
    close(panel.bottom.range_max.unwrap(), 3.0);
    // 4 - 1 % of 2, 6 + 10 % of 2.
    close(panel.left.range_min.unwrap(), 3.98);
    close(panel.left.range_max.unwrap(), 6.2);
    assert!(!state.snapshot().history.can_undo, "no history entry");
    assert!(modified(&mut state), "written ranges are unsaved work");
    let message = state.status().expect("message");
    assert_eq!(message.level, StatusLevel::Warning);
    assert!(
        message.text.contains("1 graph") && message.text.contains("Save to keep"),
        "{}",
        message.text
    );
}

/// A file with nothing on Auto opens clean, and says nothing.
#[test]
fn a_file_with_no_auto_axis_opens_unmodified_and_silent() {
    let dir = tempfile::tempdir().expect("dir");
    let fixture = write_file(dir.path(), false, false);
    let (_session, mut state) = open(dir.path(), &fixture);
    let spec = state.plot_spec(fixture.graph).expect("spec");
    close(spec.panels[0].left.range_min.unwrap(), 3.0);
    close(spec.panels[0].left.range_max.unwrap(), 7.0);
    assert!(!modified(&mut state));
    assert_ne!(
        state.status().map(|m| m.level),
        Some(StatusLevel::Warning),
        "no message"
    );
}

/// The ranges read the values the opening recalculation has just written, not
/// the cache of the file (invariant 11).
#[test]
fn the_ranges_read_the_recalculated_formula_values() {
    let dir = tempfile::tempdir().expect("dir");
    let fixture = write_file(dir.path(), true, true);
    let (_session, state) = open(dir.path(), &fixture);
    let spec = state.plot_spec(fixture.graph).expect("spec");
    // Y = X * 10 = 10..30 (the cache said 1): 10 - 1 % of 20, 30 + 10 % of 20.
    close(spec.panels[0].left.range_min.unwrap(), 9.8);
    close(spec.panels[0].left.range_max.unwrap(), 32.0);
}

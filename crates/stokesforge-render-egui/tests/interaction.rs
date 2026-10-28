//! Navigation — tasks R7.1 to R7.4, checked with real egui events replayed
//! over several frames rather than by a direct call to the private methods
//! of `GraphView` (they are precisely not public: what is under test here
//! is the wiring from egui input to `ViewState`, not the geometry — already
//! covered, independently of egui, by `stokesforge-render/src/interact.rs`).
//!
//! # Why two frames for one gesture
//!
//! `egui` computes the interactions of a frame from the widget rectangles
//! **of the previous frame** (`egui::interaction`: *"Calculated at the start
//! of each frame based on: Widget rects from previous frame"*). A first
//! blank frame (no event) is therefore used only to make the view's
//! interactive rectangle exist; the pointer events themselves are sent from
//! the second frame on.

use egui::{Event, MouseWheelUnit, PointerButton, Pos2, Rect, Vec2};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render_egui::{font_definitions, GraphView};
use stokesforge_session::{Session, Snapshot};

const RECT: Rect = Rect {
    min: Pos2::ZERO,
    max: Pos2::new(800.0, 600.0),
};

/// A graph with one series, framed on `0..100` on both sides once the 5 %
/// margin of `pipeline::padded` is mentally removed — the tests depend only
/// on the sign of the movements, never on the exact margin.
fn fixture() -> (Session, NodeId) {
    let mut project = Project::new("Navigation").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let xs: Vec<f64> = (0..=100).map(|i| i as f64).collect();
    let ys = xs.clone();
    let x = project
        .add_column(sheet, None, Some("X"), numeric(&xs))
        .expect("X");
    let y = project
        .add_column(sheet, None, Some("Y"), numeric(&ys))
        .expect("Y");
    let panel = {
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        panel
    };
    let spec = PlotSpec::empty("Navigation").with_panels(panel);
    let graph = project.add_graph(root, None, spec).expect("graph");
    (Session::spawn(project, None).expect("session"), graph)
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("column")
}

fn new_context() -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(font_definitions());
    context
}

/// Paints a frame with the given events, and returns whether it came from
/// the cache. Always consumes the `TexturesDelta`: without a window nobody
/// else does, and `epaint` panics on a delta left aside (see
/// `tests/cache_session.rs`).
fn paint_frame(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
    events: Vec<Event>,
) -> bool {
    let mut from_cache = false;
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let mut output = context.run_ui(input, |ui| {
        from_cache = view.paint(ui, RECT, snapshot, graph).expect("painting");
    });
    output.textures_delta.clear();
    from_cache
}

fn pointer_moved(pos: Pos2) -> Event {
    Event::PointerMoved(pos)
}

fn pointer_button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

/// A scroll whose unit is `Point` and whose amplitude is **under 8.0**:
/// this is the exact condition (`egui::input_state::wheel_state`,
/// `is_smooth`) under which `smooth_scroll_delta` receives the whole delta
/// in the frame where the event arrives, without smoothing over several
/// frames — which makes the test deterministic in a single frame. Above 8.0,
/// `egui` spreads the delta over several frames (meant for a physical wheel,
/// which moves in large notches), and the test would have to wait several
/// frames to see the whole effect.
fn wheel(delta_y: f32) -> Event {
    wheel_with(delta_y, egui::Modifiers::CTRL)
}

fn wheel_with(delta_y: f32, modifiers: egui::Modifiers) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: Vec2::new(0.0, delta_y),
        modifiers,
        phase: egui::TouchPhase::Move,
    }
}

/// A point of the panel, in pixels — the center of the test rectangle,
/// arbitrary but fixed.
const UNDER_CURSOR: Pos2 = Pos2::new(400.0, 300.0);

#[test]
fn wheel_zoom_shrinks_the_framing_and_advances_the_generation() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();

    // Frame 0: blank, so that the interactive rectangle exists.
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let before = view.last_drawn().expect("a figure was drawn").x_range();
    assert_eq!(view.view().generation(), 0);

    // Frame 1: the wheel, hovering over the figure. The sign is the one
    // that, with `handle_navigation` (`factor = exp(-scroll × k)`), gives a
    // factor `< 1` — hence a shrink; the opposite would enlarge, which
    // `wheel_state.rs` does not physically tell apart from one direction or
    // the other (R7 imposes no convention).
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(UNDER_CURSOR), wheel(7.9)],
    );
    // A second frame is needed for the pipeline to replay with the new
    // framing (the generation changed the cache key **during** the frame
    // where the gesture happened, so `outcome.cached` of that frame may
    // still carry the old figure depending on the exact order of the steps;
    // this test fixes only the stable result, one frame later).
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);

    assert!(
        view.view().generation() > 0,
        "the wheel did not advance the view generation"
    );
    let after = view.last_drawn().expect("a figure was drawn").x_range();
    assert!(
        (after.1 - after.0) < (before.1 - before.0),
        "the framing did not shrink: before {before:?}, after {after:?}"
    );
}

/// Ctrl as a real keyboard delivers it: `ctrl` and `command` together.
fn ctrl(on: bool) -> Event {
    Event::ModifiersChanged(if on {
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    } else {
        egui::Modifiers::NONE
    })
}

/// A drag replayed over five frames (#318): press at `start`, two moves,
/// release at `end`. `ctrl_on_press`: Ctrl held down before the press;
/// `ctrl_midway`: its state from the frame of the first move. Returns,
/// frame by frame, whether the figure came from the cache.
#[allow(clippy::too_many_arguments)]
fn drag(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
    start: Pos2,
    end: Pos2,
    button: PointerButton,
    ctrl_on_press: bool,
    ctrl_midway: bool,
) -> Vec<bool> {
    let mid = start + (end - start) * 0.5;
    let mut press = Vec::new();
    if ctrl_on_press {
        press.push(ctrl(true));
    }
    press.push(pointer_moved(start));
    press.push(pointer_button(start, button, true));
    let mut midway = Vec::new();
    if ctrl_midway != ctrl_on_press {
        midway.push(ctrl(ctrl_midway));
    }
    midway.push(pointer_moved(mid));
    let mut finish = vec![pointer_button(end, button, false)];
    if ctrl_midway {
        finish.push(ctrl(false));
    }
    [press, midway, vec![pointer_moved(end)], finish, vec![]]
        .into_iter()
        .map(|events| paint_frame(context, view, snapshot, graph, events))
        .collect()
}

fn width_of(range: (f64, f64)) -> f64 {
    range.1 - range.0
}

/// The basic framing drag, on the background, for the #318 tests.
fn pan_on_background(
    tool: stokesforge_render_egui::GraphTool,
    button: PointerButton,
    ctrl_on_press: bool,
    ctrl_midway: bool,
) -> ((f64, f64), (f64, f64), u64, Vec<bool>) {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    view.set_tool(tool);
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let before = view.last_drawn().expect("figure").x_range();
    let cache = drag(
        &context,
        &mut view,
        &snapshot,
        graph,
        Pos2::new(300.0, 300.0),
        Pos2::new(500.0, 300.0),
        button,
        ctrl_on_press,
        ctrl_midway,
    );
    let after = view.last_drawn().expect("figure").x_range();
    (before, after, view.view().generation(), cache)
}

/// #318: Ctrl + drag pans the framing, anywhere in the plot and whatever the
/// Pointer tool; dragging to the right reveals what was on the left (both
/// bounds go down), without changing the width.
#[test]
fn ctrl_drag_pans_the_framing() {
    use stokesforge_render_egui::GraphTool;
    let (before, after, generation, _) =
        pan_on_background(GraphTool::Pointer, PointerButton::Primary, true, true);
    assert!(generation > 0, "the drag advanced no generation");
    assert!(
        after.0 < before.0 && after.1 < before.1,
        "wrong direction: before {before:?}, after {after:?}"
    );
    assert!(
        (width_of(after) - width_of(before)).abs() < 1e-6,
        "a pan is not a zoom"
    );
}

/// #318: middle click + drag does the same pan, without Ctrl.
#[test]
fn middle_click_drag_pans_the_framing() {
    use stokesforge_render_egui::GraphTool;
    let (before, after, generation, _) =
        pan_on_background(GraphTool::Pointer, PointerButton::Middle, false, false);
    assert!(generation > 0, "the middle drag framed nothing");
    assert!(
        after.0 < before.0 && after.1 < before.1,
        "{before:?} -> {after:?}"
    );
    assert!((width_of(after) - width_of(before)).abs() < 1e-6);
}

/// #318: a plain drag (primary button, no Ctrl) no longer pans with the
/// Pointer: framing and generation unchanged, and the figure stays served
/// from the cache during the whole gesture (nothing to rebuild).
#[test]
fn plain_drag_on_the_background_does_nothing() {
    use stokesforge_render_egui::GraphTool;
    let (before, after, generation, cache) =
        pan_on_background(GraphTool::Pointer, PointerButton::Primary, false, false);
    assert_eq!(generation, 0, "a plain drag framed");
    assert_eq!(before, after);
    assert!(
        cache.iter().all(|c| *c),
        "the figure was rebuilt: {cache:?}"
    );
}

/// #318: with Ctrl released midway, the pan continues; with Ctrl pressed
/// midway, it does not start (the gesture is decided at the press).
#[test]
fn ctrl_is_read_at_press_not_midway() {
    use stokesforge_render_egui::GraphTool;
    let (before, after, generation, _) =
        pan_on_background(GraphTool::Pointer, PointerButton::Primary, true, false);
    assert!(
        generation > 0 && after.0 < before.0,
        "the pan stopped with Ctrl"
    );
    let (_, _, generation, _) =
        pan_on_background(GraphTool::Pointer, PointerButton::Primary, false, true);
    assert_eq!(generation, 0, "Ctrl pressed midway triggered a pan");
}

/// #318: the Zoom tool keeps its rectangle on a plain drag; Ctrl or middle
/// click pan there, without a rectangle (the framing width does not change).
#[test]
fn zoom_tool_ctrl_drag_pans_without_rectangle() {
    use stokesforge_render_egui::GraphTool;
    let (before, after, _, _) =
        pan_on_background(GraphTool::ZoomBox, PointerButton::Primary, false, false);
    assert!(
        width_of(after) < width_of(before),
        "the zoom rectangle is gone"
    );
    for (button, ctrl_on_press) in [
        (PointerButton::Primary, true),
        (PointerButton::Middle, false),
    ] {
        let (before, after, _, _) =
            pan_on_background(GraphTool::ZoomBox, button, ctrl_on_press, ctrl_on_press);
        assert!(
            after.0 < before.0 && after.1 < before.1,
            "no pan: {before:?} -> {after:?}"
        );
        assert!(
            (width_of(after) - width_of(before)).abs() < 1e-6,
            "a rectangle zoomed during the pan: {before:?} -> {after:?}"
        );
    }
}

/// #318: with a drawing tool, Ctrl + drag pans without drawing anything and
/// the tool stays active; Ctrl + click places neither a shape nor a text.
#[test]
fn drawing_tools_do_not_draw_under_ctrl() {
    use stokesforge_render_egui::{GraphEvent, GraphTool};
    for tool in [GraphTool::Rectangle, GraphTool::Line] {
        let (session, graph) = fixture();
        let snapshot = session.handle().snapshot().expect("snapshot");
        let context = new_context();
        let mut view = GraphView::new();
        view.set_tool(tool);
        paint_frame(&context, &mut view, &snapshot, graph, vec![]);
        let before = view.last_drawn().expect("figure").x_range();
        drag(
            &context,
            &mut view,
            &snapshot,
            graph,
            Pos2::new(300.0, 300.0),
            Pos2::new(500.0, 300.0),
            PointerButton::Primary,
            true,
            true,
        );
        assert!(
            view.take_events().is_empty(),
            "{tool:?}: an event under Ctrl"
        );
        assert_eq!(view.tool(), tool, "the tool handed control back");
        assert!(view.shape_preview().is_none());
        let after = view.last_drawn().expect("figure").x_range();
        assert!(after.0 < before.0, "{tool:?}: no pan");

        // Plain Ctrl + click: no shape.
        for events in [
            vec![
                ctrl(true),
                pointer_moved(UNDER_CURSOR),
                pointer_button(UNDER_CURSOR, PointerButton::Primary, true),
            ],
            vec![pointer_button(UNDER_CURSOR, PointerButton::Primary, false)],
            vec![ctrl(false)],
            vec![],
        ] {
            paint_frame(&context, &mut view, &snapshot, graph, events);
        }
        let events = view.take_events();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GraphEvent::ShapeChanged { .. })),
            "{tool:?}: a shape under Ctrl + click: {events:?}"
        );
        assert_eq!(view.tool(), tool);
    }
}

/// #318: Ctrl + click with the Text tool places no text.
#[test]
fn ctrl_click_with_the_text_tool_places_nothing() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    view.set_tool(stokesforge_render_egui::GraphTool::Text);
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    for events in [
        vec![
            ctrl(true),
            pointer_moved(UNDER_CURSOR),
            pointer_button(UNDER_CURSOR, PointerButton::Primary, true),
        ],
        vec![pointer_button(UNDER_CURSOR, PointerButton::Primary, false)],
        vec![ctrl(false)],
        vec![],
    ] {
        paint_frame(&context, &mut view, &snapshot, graph, events);
    }
    assert!(
        view.take_events().is_empty(),
        "a text placed under Ctrl + click"
    );
    // A plain drag with the Text tool does nothing either.
    drag(
        &context,
        &mut view,
        &snapshot,
        graph,
        Pos2::new(300.0, 300.0),
        Pos2::new(500.0, 300.0),
        PointerButton::Primary,
        false,
        false,
    );
    assert_eq!(view.view().generation(), 0);
}

#[test]
fn rubber_band_rectangle_zooms_on_the_chosen_area() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    view.set_tool(stokesforge_render_egui::GraphTool::ZoomBox);

    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    // The expected framing is read on the transform from *before* the
    // gesture: it is what translates the two chosen corners into data.
    let panel_before = view
        .last_drawn()
        .expect("a figure was drawn")
        .panel(0)
        .expect("a panel")
        .clone();
    let transform_before =
        stokesforge_render_egui::PanelTransform::of(&panel_before).expect("panel of non-zero size");

    let start = Pos2::new(300.0, 200.0);
    let end = Pos2::new(500.0, 400.0);
    let expected = transform_before
        .range_between(
            (start.x as f64, start.y as f64),
            (end.x as f64, end.y as f64),
        )
        .expect("both corners are inside the framing");

    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Primary, true),
        ],
    );
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(end)],
    );
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(end, PointerButton::Primary, false)],
    );
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);

    assert!(
        view.view().generation() > 0,
        "the rubber-band rectangle advanced no view generation"
    );
    let framing = view
        .view()
        .framing(panel_before.index)
        .expect("the panel should carry an explicit framing");
    assert!(
        (framing.x.unwrap().0 - expected.0 .0).abs() < 1e-6
            && (framing.x.unwrap().1 - expected.0 .1).abs() < 1e-6,
        "x_range = {:?}, expected {:?}",
        framing.x,
        expected.0
    );
    assert!(
        (framing.y.unwrap().0 - expected.1 .0).abs() < 1e-6
            && (framing.y.unwrap().1 - expected.1 .1).abs() < 1e-6,
        "y_range = {:?}, expected {:?}",
        framing.y,
        expected.1
    );
}

#[test]
fn hovering_does_not_increase_the_computation_counter() {
    // The central property of task 7.3: the crosshair is painted over the
    // cache, without invalidating it (`crate::cache`, module head). Moving
    // only the cursor must not cause any new computation of the Plotters
    // pipeline.
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();

    let first = paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    assert!(!first, "the first frame is necessarily a computation");
    let computations = view.cache().misses();

    for pos in [
        Pos2::new(200.0, 200.0),
        Pos2::new(210.0, 205.0),
        Pos2::new(600.0, 450.0),
        Pos2::new(400.0, 300.0),
    ] {
        let from_cache = paint_frame(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![pointer_moved(pos)],
        );
        assert!(from_cache, "a hover triggered a computation at {pos:?}");
    }

    assert_eq!(
        view.cache().misses(),
        computations,
        "moving the crosshair raised the number of pipeline computations"
    );
}

#[test]
fn hovering_over_a_known_point_hits_it() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();

    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let panel = view
        .last_drawn()
        .expect("a figure was drawn")
        .panel(0)
        .expect("a panel");
    let transform =
        stokesforge_render_egui::PanelTransform::of(panel).expect("panel of non-zero size");
    // The datum (50, 50) exists in the series (`fixture`, X = Y = 0..=100):
    // its pixel is a guaranteed hit point, unlike a randomly chosen pixel
    // that could fall between two points on a decimated figure.
    let (px, py) = transform
        .data_to_pixel(50.0, 50.0)
        .expect("(50, 50) is inside the framing");
    let pixel = Pos2::new(px as f32, py as f32);

    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(pixel)],
    );

    let hit = view.hovered_point().expect("a point should have been hit");
    assert_eq!(hit.data, (50.0, 50.0));
    assert!(hit.distance < 1.0, "distance = {}", hit.distance);
}

#[test]
fn double_click_resets_the_zoom() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();

    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(UNDER_CURSOR), wheel(7.9)],
    );
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    assert!(!view.view().is_empty(), "the zoom set no framing");

    let double_click = vec![
        pointer_moved(UNDER_CURSOR),
        pointer_button(UNDER_CURSOR, PointerButton::Primary, true),
        pointer_button(UNDER_CURSOR, PointerButton::Primary, false),
        pointer_button(UNDER_CURSOR, PointerButton::Primary, true),
        pointer_button(UNDER_CURSOR, PointerButton::Primary, false),
    ];
    paint_frame(&context, &mut view, &snapshot, graph, double_click);
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);

    assert!(
        view.view().is_empty(),
        "the double click did not reset the framing"
    );
}

#[test]
fn wheel_alone_no_longer_zooms() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(UNDER_CURSOR),
            wheel_with(7.9, egui::Modifiers::default()),
        ],
    );
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    assert_eq!(view.view().generation(), 0, "the wheel alone zoomed");
}

#[test]
fn secondary_drag_no_longer_draws_a_rectangle() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let (start, end) = (Pos2::new(300.0, 200.0), Pos2::new(500.0, 400.0));
    for events in [
        vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Secondary, true),
        ],
        vec![pointer_moved(end)],
        vec![pointer_button(end, PointerButton::Secondary, false)],
        vec![],
    ] {
        paint_frame(&context, &mut view, &snapshot, graph, events);
    }
    assert_eq!(view.view().generation(), 0, "the right click zoomed");
}

#[test]
fn text_tool_asks_for_text_on_click_and_does_not_scroll() {
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    view.set_tool(stokesforge_render_egui::GraphTool::Text);
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    for events in [
        vec![
            pointer_moved(UNDER_CURSOR),
            pointer_button(UNDER_CURSOR, PointerButton::Primary, true),
        ],
        vec![pointer_button(UNDER_CURSOR, PointerButton::Primary, false)],
        vec![],
    ] {
        paint_frame(&context, &mut view, &snapshot, graph, events);
    }
    let events = view.take_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, stokesforge_render_egui::GraphEvent::PlaceText { .. })),
        "no PlaceText: {events:?}"
    );
    assert_eq!(view.view().generation(), 0);
}

#[test]
fn series_selection_by_click_and_tree_without_mutation_or_rebuild() {
    use stokesforge_plot::SeriesRef;
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graph")
    };
    let spec = spec.clone();
    let context = new_context();
    let mut view = GraphView::new();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let panel = &view.last_drawn().unwrap().panels[0];
    let point = panel.series_geometry[0].paths[0][50];
    let pos = Pos2::new(point.0 as f32, point.1 as f32);
    assert_eq!(view.series_at(pos, &spec), Some(SeriesRef::new(0, 0)));
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(pos),
            pointer_button(pos, PointerButton::Primary, true),
        ],
    );
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(pos, PointerButton::Primary, false)],
    );
    assert_eq!(view.selected_series(), Some(SeriesRef::new(0, 0)));
    assert!(view.selected_annotation().is_none());
    assert!(view.selected_text().is_none());
    view.select_series(None, &spec);
    assert!(view.selected_series().is_none());
    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    assert!(
        paint_frame(&context, &mut view, &snapshot, graph, vec![]),
        "selection outside the figure cache"
    );
    view.select_series(Some(SeriesRef::new(8, 8)), &spec);
    assert!(view.selected_series().is_none());
    assert_eq!(
        session.handle().snapshot().unwrap().revision,
        snapshot.revision
    );
}

#[test]
fn selection_layer_right_axis_bars_and_hidden_series() {
    use stokesforge_commands::Command;
    use stokesforge_plot::{PlotType, SeriesAxis, SeriesRef};
    let (session, graph) = fixture();
    let handle = session.handle();
    let snapshot = handle.snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graph")
    };
    let first = spec.panels[0].clone();
    let mut second = first.clone();
    second.row = 1;
    second.series[0].visible = false;
    let mut right = second.series[0].clone();
    right.visible = true;
    right.axis = SeriesAxis::Right;
    second.series.push(right);
    second.right.visible = true;
    second.right.auto_range = false;
    second.right.range_min = Some(0.0);
    second.right.range_max = Some(1000.0);
    let mut spec = spec.clone().with_panels(vec![first, second]);
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    let context = new_context();
    let mut view = GraphView::new();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let panel = &view.last_drawn().unwrap().panels[1];
    assert_eq!(panel.series_geometry.len(), 1);
    assert_eq!(panel.series_geometry[0].index, 1);
    let point = panel.series_geometry[0].paths[0][50];
    let pos = Pos2::new(point.0 as f32, point.1 as f32);
    assert_eq!(view.series_at(pos, &spec), Some(SeriesRef::new(1, 1)));
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(pos),
            pointer_button(pos, PointerButton::Primary, true),
        ],
    );
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(pos, PointerButton::Primary, false)],
    );
    assert_eq!(view.selected_series(), Some(SeriesRef::new(1, 1)));
    // Bars are also picked inside, not only at the top.
    spec.series_at_mut(SeriesRef::new(1, 1)).unwrap().plot_type = PlotType::Bar;
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let panel = &view.last_drawn().unwrap().panels[1];
    let (a, b) = panel.series_geometry[0].bars[50];
    let pos = Pos2::new((a.0 + b.0) as f32 / 2.0, (a.1 + b.1) as f32 / 2.0);
    assert_eq!(view.series_at(pos, &spec), Some(SeriesRef::new(1, 1)));
    // Hiding keeps the tree selection and removes all geometry.
    spec.series_at_mut(SeriesRef::new(1, 1)).unwrap().visible = false;
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    assert_eq!(view.selected_series(), Some(SeriesRef::new(1, 1)));
    assert!(view.last_drawn().unwrap().panels[1]
        .series_geometry
        .is_empty());
    assert!(view.series_at(pos, &spec).is_none());
}

#[test]
fn object_manager_selects_hidden_contents_without_two_selections() {
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, SeriesRef, SeriesSpec, ShapeKind, ShapeRef, ShapeSpec,
        TextTarget,
    };
    let mut spec = stokesforge_plot::PlotSpec::empty("Title");
    let panel = spec.panels.get_mut(0).unwrap();
    panel
        .series
        .push(SeriesSpec::new(stokesforge_core::NodeId::from_raw(900)));
    panel
        .annotations
        .push(AnnotationSpec::new("Text", 0.0, 0.0));
    panel.annotations[0].visible = false;
    panel
        .shapes
        .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 1.0));
    panel.shapes[0].visible = false;
    spec.title_visible = false;
    let annotation = AnnotationRef { panel: 0, index: 0 };
    let shape = ShapeRef { panel: 0, index: 0 };
    let mut view = stokesforge_render_egui::GraphView::new();
    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    view.select_annotation(Some(annotation), &spec);
    assert_eq!(view.selected_annotation(), Some(annotation));
    assert_eq!(view.selected_series(), None);
    view.select_shape(Some(shape));
    assert_eq!(view.selected_shape(), Some(shape));
    assert_eq!(view.selected_annotation(), None);
    view.select_text(Some(TextTarget::Title), &spec);
    assert_eq!(view.selected_text(), Some(TextTarget::Title));
    assert_eq!(view.selected_shape(), None);
    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    assert_eq!(view.selected_text(), None);
    view.select_annotation(
        Some(AnnotationRef {
            index: 99,
            ..annotation
        }),
        &spec,
    );
    assert_eq!(view.selected_annotation(), None);
}

/// Paints a frame without events and returns the shapes produced, flattened,
/// and whether the figure came from the cache.
fn painted_shapes(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
) -> (Vec<egui::Shape>, bool) {
    let mut from_cache = false;
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        from_cache = view.paint(ui, RECT, snapshot, graph).expect("painting");
    });
    output.textures_delta.clear();
    fn flatten(shape: egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.into_iter().for_each(|s| flatten(s, out)),
            shape => out.push(shape),
        }
    }
    let mut out = Vec::new();
    for clipped in output.shapes {
        flatten(clipped.shape, &mut out);
    }
    (out, from_cache)
}

/// The handles: filled squares of side `HANDLE_SIDE`, color `ink`.
fn handles_of(shapes: &[egui::Shape], ink: (u8, u8, u8)) -> Vec<Rect> {
    let side = stokesforge_render::series_selection::HANDLE_SIDE as f32;
    let ink = egui::Color32::from_rgb(ink.0, ink.1, ink.2);
    shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Rect(rect)
                if rect.fill == ink
                    && (rect.rect.width() - side).abs() < 1e-3
                    && (rect.rect.height() - side).abs() < 1e-3 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .collect()
}

/// #98: a selected series carries small squares laid along its plot, in the
/// background's automatic ink, without the figure (the curve's plot
/// included) changing or being rebuilt.
#[test]
fn selected_series_carries_handles_without_changing_its_plot() {
    use stokesforge_commands::Command;
    use stokesforge_plot::SeriesRef;
    use stokesforge_render::color::{DARK_INK, LIGHT_INK};
    use stokesforge_render::series_selection::HANDLE_SPACING;
    let (session, graph) = fixture();
    let handle = session.handle();
    let snapshot = handle.snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graph")
    };
    let mut spec = spec.clone();
    let context = new_context();
    let mut view = GraphView::new();
    let (without, _) = painted_shapes(&context, &mut view, &snapshot, graph);
    assert!(handles_of(&without, DARK_INK).is_empty());

    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    let (with_selection, from_cache) = painted_shapes(&context, &mut view, &snapshot, graph);
    assert!(from_cache, "selecting rebuilt the figure");
    let squares = handles_of(&with_selection, DARK_INK);
    // The diagonal is several hundred pixels long: several handles, each
    // laid on the drawn curve.
    assert!(squares.len() >= 5, "{squares:?}");
    let scale = view.display_scale().unwrap();
    let origin = view.display_rect().unwrap().min;
    let path = &view.last_drawn().unwrap().panels[0].series_geometry[0].paths[0];
    let (first, last) = (path[0], path[path.len() - 1]);
    let a = origin + Vec2::new(first.0 as f32, first.1 as f32) * scale;
    let b = origin + Vec2::new(last.0 as f32, last.1 as f32) * scale;
    let direction = (b - a).normalized();
    for square in &squares {
        let c = square.center() - a;
        let offset = (c.x * direction.y - c.y * direction.x).abs();
        assert!(offset < 2.0, "handle off the curve: {square:?}");
    }
    // Spaced HANDLE_SPACING apart on screen (up to pixel rounding).
    let step = squares[0].center().distance(squares[1].center());
    assert!((step - HANDLE_SPACING as f32).abs() <= 1.5, "step : {step}");
    // The rest of the frame is exactly what it was: the curve is neither
    // repainted nor highlighted.
    let rest: Vec<_> = with_selection
        .iter()
        .filter(|shape| !matches!(shape, egui::Shape::Rect(r) if squares.contains(&r.rect)))
        .cloned()
        .collect();
    assert_eq!(rest, without);

    // Dark background: the handles turn white.
    spec.background = "#101828".to_owned();
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    let (dark, _) = painted_shapes(&context, &mut view, &snapshot, graph);
    assert!(handles_of(&dark, DARK_INK).is_empty());
    assert_eq!(handles_of(&dark, LIGHT_INK).len(), squares.len());

    // Deselecting removes them.
    view.select_series(None, &spec);
    let (after, _) = painted_shapes(&context, &mut view, &snapshot, graph);
    assert!(handles_of(&after, LIGHT_INK).is_empty());
}

/// #98: the handles are view state, not model state — the spec the
/// application exports (`framed_spec`) is the same, selected or not.
#[test]
fn selection_does_not_enter_the_exported_spec() {
    use stokesforge_plot::SeriesRef;
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graph")
    };
    let mut view = GraphView::new();
    let before = view.framed_spec(spec);
    view.select_series(Some(SeriesRef::new(0, 0)), spec);
    assert!(view.selected_series().is_some());
    assert_eq!(view.framed_spec(spec), before);
}

/// #153: a click inserts a visible object, a drag keeps its free drawing.
/// The events are published only once, on commit.
#[test]
fn shapes_insert_on_click_with_a_visible_size() {
    use stokesforge_commands::Command;
    use stokesforge_render_egui::{GraphEvent, GraphTool};
    for (tool, width, height) in [
        (GraphTool::Rectangle, 48.0, 48.0),
        (GraphTool::Ellipse, 48.0, 48.0),
        (GraphTool::Line, 80.0, 0.0),
        (GraphTool::Arrow, 80.0, 0.0),
    ] {
        let (session, graph) = fixture();
        let handle = session.handle();
        let snapshot = handle.snapshot().unwrap();
        let context = new_context();
        let mut view = GraphView::new();
        paint_frame(&context, &mut view, &snapshot, graph, vec![]);
        view.set_tool(tool);
        let at = Pos2::new(350.0, 300.0);
        paint_frame(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![
                pointer_moved(at),
                pointer_button(at, PointerButton::Primary, true),
            ],
        );
        assert!(view.take_events().is_empty());
        paint_frame(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![pointer_button(at, PointerButton::Primary, false)],
        );
        let events = view.take_events();
        assert_eq!(events.len(), 1);
        let GraphEvent::ShapeChanged { target, shape } = &events[0] else {
            panic!("insertion")
        };
        assert_eq!(view.tool(), GraphTool::Pointer);
        let stokesforge_core::node::NodeKind::Graph { spec, .. } =
            snapshot.tree.node(graph).unwrap().kind()
        else {
            panic!("graph")
        };
        let mut updated = spec.clone();
        assert!(target.set(&mut updated, shape.clone()));
        handle
            .execute(Command::SetPlotSpec {
                graph,
                spec: updated,
            })
            .unwrap();
        paint_frame(
            &context,
            &mut view,
            &handle.snapshot().unwrap(),
            graph,
            vec![],
        );
        let drawn = view.drawn_shape(*target).unwrap();
        assert!(((drawn.p2.0 - drawn.p1.0).abs() - width).abs() <= 1.0);
        assert!(((drawn.p2.1 - drawn.p1.1).abs() - height).abs() <= 1.0);
        handle.undo().unwrap();
        assert_eq!(
            handle.snapshot().unwrap().tree.node(graph).unwrap().kind(),
            snapshot.tree.node(graph).unwrap().kind()
        );
        handle.redo().unwrap();
        paint_frame(
            &context,
            &mut view,
            &handle.snapshot().unwrap(),
            graph,
            vec![],
        );
        assert!(view.drawn_shape(*target).is_some());
    }
}

/// Tests the egui wiring of Shift and its change during the gesture; the
/// ratio is that of a shape that is not square at the start, never the
/// default's.
#[test]
fn shift_constrains_resizing_and_releases_during_the_gesture() {
    use stokesforge_commands::Command;
    use stokesforge_plot::{ShapeKind, ShapeRef, ShapeSpec};
    use stokesforge_render_egui::GraphEvent;
    let (session, graph) = fixture();
    let handle = session.handle();
    let snapshot = handle.snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graph")
    };
    let mut spec = spec.clone();
    spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
        ShapeKind::Rectangle,
        30.0,
        40.0,
        55.0,
        65.0,
    ));
    handle
        .execute(Command::SetPlotSpec { graph, spec })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    let context = new_context();
    let mut view = GraphView::new();
    let target = ShapeRef { panel: 0, index: 0 };
    view.select_shape(Some(target));
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let drawn = *view.drawn_shape(target).unwrap();
    let ratio = (drawn.p2.0 - drawn.p1.0).abs() / (drawn.p2.1 - drawn.p1.1).abs();
    assert!(
        (ratio - 1.0).abs() > 0.1,
        "initial shape not square on screen"
    );
    let scale = view.display_scale().unwrap();
    let origin = view.display_rect().unwrap().min;
    let at = origin
        + Vec2::new(
            drawn.p1.0.max(drawn.p2.0) as f32,
            drawn.p1.1.max(drawn.p2.1) as f32,
        ) * scale;
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(at),
            pointer_button(at, PointerButton::Primary, true),
        ],
    );
    let end = at + Vec2::new(90.0, 15.0);
    let paint = |view: &mut GraphView, shift| {
        let input = egui::RawInput {
            events: vec![
                Event::ModifiersChanged(egui::Modifiers {
                    shift,
                    ..Default::default()
                }),
                pointer_moved(end),
            ],
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            view.paint(ui, RECT, &snapshot, graph).unwrap();
        });
        output.textures_delta.clear();
    };
    paint(&mut view, true);
    assert!(view.take_events().is_empty());
    // The preview is drawn on the next frame.
    paint(&mut view, true);
    let resized = view.drawn_shape(target).unwrap();
    let constrained = (resized.p2.0 - resized.p1.0).abs() / (resized.p2.1 - resized.p1.1).abs();
    assert!(
        (ratio - constrained).abs() < 0.03,
        "{ratio} != {constrained}"
    );
    paint(&mut view, false);
    paint(&mut view, false);
    let resized = view.drawn_shape(target).unwrap();
    let free = (resized.p2.0 - resized.p1.0).abs() / (resized.p2.1 - resized.p1.1).abs();
    assert!((ratio - free).abs() > 0.1);
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(end, PointerButton::Primary, false)],
    );
    assert!(
        matches!(view.take_events().as_slice(), [GraphEvent::ShapeChanged { target: changed, .. }] if *changed == target)
    );
}

#[test]
fn dragging_a_shape_keeps_the_free_drawing() {
    use stokesforge_render_egui::{GraphEvent, GraphTool};
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().unwrap();
    let context = new_context();
    let mut view = GraphView::new();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    view.set_tool(GraphTool::Rectangle);
    let start = Pos2::new(300.0, 250.0);
    let end = Pos2::new(420.0, 280.0);
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Primary, true),
        ],
    );
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(end)],
    );
    assert!(view.take_events().is_empty());
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let (target, _) = view.shape_preview().unwrap();
    let drawn = view.drawn_shape(*target).unwrap();
    let scale = view.display_scale().unwrap() as f64;
    assert!(((drawn.p2.0 - drawn.p1.0).abs() * scale - 120.0).abs() < 2.0);
    assert!(((drawn.p2.1 - drawn.p1.1).abs() * scale - 30.0).abs() < 2.0);
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(end, PointerButton::Primary, false)],
    );
    assert!(matches!(
        view.take_events().as_slice(),
        [GraphEvent::ShapeChanged { .. }]
    ));
}

/// Three layers in the same cell: the base layer, a layer with its own axes
/// (X on top, Y on the right) and a layer whose X axis is linked.
fn three_layers() -> (Session, NodeId) {
    use stokesforge_plot::{OverlaySpec, Panels};
    let mut project = Project::new("Layers").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let mut column = |name: &str, values: Vec<f64>| {
        project
            .add_column(sheet, None, Some(name), numeric(&values))
            .expect("column")
    };
    let xs: Vec<f64> = (0..=100).map(f64::from).collect();
    let x = column("X", xs.clone());
    let y = column("Y", xs.clone());
    let x2 = column("X2", xs.iter().map(|v| 400.0 + 4.0 * v).collect());
    let y2 = column("Y2", xs.iter().map(|v| v / 100.0).collect());
    let mut base = PanelSpec::at(0, 0);
    base.series.push(SeriesSpec::new_xy(x, y));
    let mut own = PanelSpec::overlay_on(&base, OverlaySpec::new(true, true));
    own.series.push(SeriesSpec::new_xy(x2, y2));
    let mut linked = PanelSpec::overlay_on(&base, OverlaySpec::new(false, true));
    linked.series.push(SeriesSpec::new_xy(x, y2));
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![base, own, linked]));
    let graph = project.add_graph(root, None, spec).expect("graph");
    (Session::spawn(project, None).expect("session"), graph)
}

/// #167: a framing gesture applies to the **scales** of all the layers of
/// the cell, not only to the touched layer — an overlay layer with its own
/// axes keeps its ticks aligned with those of the base layer, and a linked
/// scale (axis not shown) has no framing of its own.
#[test]
fn zoom_frames_the_scales_of_all_layers_in_the_cell() {
    let (session, graph) = three_layers();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    view.set_tool(stokesforge_render_egui::GraphTool::ZoomBox);
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);

    let before = view.last_drawn().expect("figure").clone();
    let (start, end) = (Pos2::new(300.0, 200.0), Pos2::new(500.0, 400.0));
    let expected = |i: usize| {
        stokesforge_render_egui::PanelTransform::of(before.panel(i).expect("layer"))
            .expect("transform")
            .range_between(
                (start.x as f64, start.y as f64),
                (end.x as f64, end.y as f64),
            )
            .expect("corners inside the framing")
    };
    for events in [
        vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Primary, true),
        ],
        vec![pointer_moved(end)],
        vec![pointer_button(end, PointerButton::Primary, false)],
        vec![],
    ] {
        paint_frame(&context, &mut view, &snapshot, graph, events);
    }

    let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
    let base = view.view().framing(0).expect("framing of the base layer");
    assert!(close(base.x.unwrap(), expected(0).0) && close(base.y.unwrap(), expected(0).1));
    let own = view
        .view()
        .framing(1)
        .expect("framing of the layer with its own axes");
    assert!(
        close(own.x.unwrap(), expected(1).0) && close(own.y.unwrap(), expected(1).1),
        "the own scales follow the same gesture: {own:?}"
    );
    let linked = view.view().framing(2).expect("framing of the own Y axis");
    assert!(
        linked.x.is_none(),
        "the linked X scale has no framing of its own"
    );
    assert!(close(linked.y.unwrap(), expected(2).1));

    // The ticks stay aligned: the same fraction of each axis is visible in
    // the base layer and in the overlay layer.
    let after = view.last_drawn().expect("figure").clone();
    let fraction = |panel: usize, full: (f64, f64), visible: (f64, f64)| {
        let _ = panel;
        (
            (visible.0 - full.0) / (full.1 - full.0),
            (visible.1 - full.0) / (full.1 - full.0),
        )
    };
    let (b0, b1) = (
        before.panel(0).unwrap().x_range,
        after.panel(0).unwrap().x_range,
    );
    let (p0, p1) = (
        before.panel(1).unwrap().x_range,
        after.panel(1).unwrap().x_range,
    );
    let (fb, fp) = (fraction(0, b0, b1), fraction(1, p0, p1));
    assert!(
        close(fb, fp),
        "visible fractions: base {fb:?}, overlay {fp:?}"
    );
    assert_eq!(
        after.panel(2).unwrap().x_range,
        after.panel(0).unwrap().x_range
    );
}

/// #167: over an axis, the wheel and the drag frame only that scale; the
/// other scales of the cell, linked or not, do not move.
#[test]
fn wheel_and_drag_on_an_axis_frame_only_that_scale() {
    let (session, graph) = three_layers();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let panel = view.last_drawn().expect("figure").panel(0).unwrap().clone();
    let center_x = panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0;
    let below = Pos2::new(
        center_x,
        (panel.pixel_origin.1 + panel.pixel_size.1 as i32) as f32 + 8.0,
    );
    let above = Pos2::new(center_x, panel.pixel_origin.1 as f32 - 8.0);

    // Wheel on the bottom X axis (base layer).
    paint_frame(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(below), wheel(7.9)],
    );
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let base = view.view().framing(0).expect("the bottom X axis is framed");
    assert!(base.x.is_some(), "{base:?}");
    assert!(
        base.y.is_none(),
        "the Y axis of the base layer did not move"
    );
    assert!(
        view.view().framing(1).is_none() && view.view().framing(2).is_none(),
        "the other layers do not move"
    );

    // Plain drag started from an axis: nothing (#318).
    let arrival = Pos2::new(above.x + 80.0, above.y);
    drag(
        &context,
        &mut view,
        &snapshot,
        graph,
        above,
        arrival,
        PointerButton::Primary,
        false,
        false,
    );
    assert!(
        view.view().framing(1).is_none(),
        "a plain drag on an axis framed"
    );
    // Ctrl + drag, then middle click + drag, started from the top X axis
    // (layer with its own axes): that scale only.
    drag(
        &context,
        &mut view,
        &snapshot,
        graph,
        above,
        arrival,
        PointerButton::Primary,
        true,
        true,
    );
    let after_ctrl = view.view().framing(1).expect("the top X axis is framed").x;
    drag(
        &context,
        &mut view,
        &snapshot,
        graph,
        above,
        arrival,
        PointerButton::Middle,
        false,
        false,
    );
    assert_ne!(
        view.view().framing(1).unwrap().x,
        after_ctrl,
        "the middle click did not frame the axis"
    );
    let own = view.view().framing(1).expect("the top X axis is framed");
    assert!(own.x.is_some() && own.y.is_none(), "{own:?}");
    assert_eq!(
        view.view().framing(0).unwrap().x,
        base.x,
        "the base layer did not follow"
    );
}

/// #197: a series group is selected through its first series; selecting any
/// other curve — even of the same group — or an absent group lifts it.
#[test]
fn a_group_is_selected_by_its_first_series_and_another_curve_lifts_it() {
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesGroupRef, SeriesRef, SeriesSpec};
    let mut panel = PanelSpec::default();
    panel.push_series_batch(vec![SeriesSpec::new(stokesforge_core::NodeId::from_raw(1))]);
    panel.push_series_batch(
        (2..=3)
            .map(|n| SeriesSpec::new(stokesforge_core::NodeId::from_raw(n)))
            .collect(),
    );
    let spec = PlotSpec::default().with_panels(panel);
    let mut view = GraphView::new();
    let group = SeriesGroupRef::new(0, 1);
    view.select_series_group(Some(group), &spec);
    assert_eq!(view.selected_group(), Some(group));
    assert_eq!(view.selected_series(), Some(SeriesRef::new(0, 1)));

    view.select_series(Some(SeriesRef::new(0, 2)), &spec);
    assert_eq!(view.selected_group(), None, "another curve lifts the group");
    assert_eq!(view.selected_series(), Some(SeriesRef::new(0, 2)));

    view.select_series_group(Some(group), &spec);
    view.select_series(Some(SeriesRef::new(0, 1)), &spec);
    assert_eq!(
        view.selected_group(),
        None,
        "re-selecting the same curve alone does too"
    );

    view.select_series_group(Some(SeriesGroupRef::new(0, 9)), &spec);
    assert_eq!(view.selected_group(), None);
    assert_eq!(view.selected_series(), None, "absent group: nothing");
}

/// #318, invariant 6: feathering is cut only by a real gesture. A plain drag
/// with no effect (Pointer on the background) leaves it in place; a pan
/// (Ctrl) cuts it for the duration of the gesture and restores it on
/// release.
#[test]
fn feathering_is_cut_only_during_a_real_gesture() {
    use stokesforge_render_egui::feathering;
    let (session, graph) = fixture();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = new_context();
    let mut view = GraphView::new();
    paint_frame(&context, &mut view, &snapshot, graph, vec![]);
    let (start, mid, end) = (
        Pos2::new(300.0, 300.0),
        Pos2::new(400.0, 300.0),
        Pos2::new(500.0, 300.0),
    );
    for (ctrl_on, cut) in [(false, false), (true, true)] {
        let mut press = vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Primary, true),
        ];
        if ctrl_on {
            press.insert(0, ctrl(true));
        }
        paint_frame(&context, &mut view, &snapshot, graph, press);
        paint_frame(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![pointer_moved(mid)],
        );
        paint_frame(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![pointer_moved(end)],
        );
        assert_eq!(
            feathering::current(&context).is_on(),
            !cut,
            "Ctrl={ctrl_on}: feathering during the drag"
        );
        paint_frame(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![
                pointer_button(end, PointerButton::Primary, false),
                ctrl(false),
            ],
        );
        paint_frame(&context, &mut view, &snapshot, graph, vec![]);
        assert!(
            feathering::current(&context).is_on(),
            "not restored at rest"
        );
    }
}

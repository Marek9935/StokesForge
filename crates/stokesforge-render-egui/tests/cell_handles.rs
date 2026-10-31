//! The resize handles on the outer boundary of a cell (#394), end to end: the
//! session, an `egui::Context` and the pointer events of a drag.
//!
//! No graphics server: `egui::Context::run_ui` produces shapes without painting
//! anything.

use egui::{Pos2, Rect};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{
    FreeFrame, LayerPlacement, LineSpec, PanelSpec, Panels, PlotSpec, SeriesSpec,
};
use stokesforge_render_egui::{font_definitions, GraphEvent, GraphView};
use stokesforge_session::{Session, Snapshot};

struct Fixture {
    session: Session,
    graph: NodeId,
}

impl Fixture {
    /// A graph with stored frames of 300 x 200 pt (explicit) or, with `legacy`,
    /// none (an old file).
    fn new(legacy: bool) -> Self {
        Self::placed(legacy, None, None)
    }

    /// Same, with the position of the first frame stored (#405).
    fn placed(legacy: bool, left: Option<f64>, top: Option<f64>) -> Self {
        let mut project = Project::new("Handles").expect("project");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("workbook");
        let sheet = project.add_worksheet(book, None).expect("sheet");
        let xs: Vec<f64> = (0..32).map(f64::from).collect();
        let ys: Vec<f64> = xs.iter().map(|x| x * 1.5).collect();
        let x = project
            .add_column(sheet, None, Some("X"), numeric(&xs))
            .expect("X");
        let y = project
            .add_column(sheet, None, Some("Y"), numeric(&ys))
            .expect("Y");
        let mut panel = PanelSpec::at(0, 0);
        let mut series = SeriesSpec::new_xy(x, y);
        series.line = LineSpec::from_palette(0);
        panel.series.push(series);
        let mut spec = PlotSpec::empty("").with_panels(panel);
        if !legacy {
            spec.frame_widths = vec![300.0];
            spec.frame_heights = vec![200.0];
            spec.figure_left = left;
            spec.figure_top = top;
        }
        let graph = project.add_graph(root, None, spec).expect("graph");
        Self {
            session: Session::spawn(project, None).expect("session"),
            graph,
        }
    }

    /// A graph in free placement (#398) with two layers: layer 0 at the right
    /// (grid cell `(0, 0)`), layer 1 at the left (grid cell `(0, 1)`).
    fn free_two_layers() -> Self {
        Self::free_two_layers_at((400.0, 40.0), (0, 0))
    }

    /// [`Self::free_two_layers`] with the `left` of each layer and their
    /// stacking keys given (overlapping when the lefts are closer than 300).
    fn free_two_layers_at(lefts: (f64, f64), stacks: (i32, i32)) -> Self {
        let fixture = Self::new(false);
        let snapshot = fixture.snapshot();
        let node = snapshot.tree.node(fixture.graph).expect("graph");
        let stokesforge_core::node::NodeKind::Graph { spec, .. } = node.kind() else {
            panic!("a graph");
        };
        let mut first = spec.panels[0].clone();
        first.free_frame = Some(FreeFrame {
            left: lefts.0,
            top: 20.0,
            width: 300.0,
            height: 200.0,
            stack: stacks.0,
        });
        let mut second = first.clone();
        second.row = 0;
        second.column = 1;
        second.free_frame = Some(FreeFrame {
            left: lefts.1,
            top: 20.0,
            width: 300.0,
            height: 200.0,
            stack: stacks.1,
        });
        let mut free = spec.clone().with_panels(Panels::new([first, second]));
        free.layer_placement = LayerPlacement::Free;
        fixture
            .session
            .handle()
            .execute(stokesforge_commands::Command::SetPlotSpec {
                graph: fixture.graph,
                spec: free,
            })
            .expect("free spec");
        fixture
    }

    fn snapshot(&self) -> Snapshot {
        self.session.handle().snapshot().expect("snapshot")
    }
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("column")
}

fn button_event(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// A frame at the figure's own size (display scale 1) with the given events;
/// returns the signals and the cursor the frame asked for.
fn frame(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    events: Vec<egui::Event>,
    time: f64,
) -> (Vec<GraphEvent>, egui::CursorIcon) {
    frame_with(
        context,
        view,
        fixture,
        events,
        time,
        1.0,
        egui::Modifiers::NONE,
    )
}

/// [`frame`] with the figure displayed `scale` times larger and the given
/// modifiers held.
fn frame_with(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    events: Vec<egui::Event>,
    time: f64,
    scale: f32,
    modifiers: egui::Modifiers,
) -> (Vec<GraphEvent>, egui::CursorIcon) {
    let layout = view
        .figure_layout(context, &fixture.snapshot(), fixture.graph)
        .expect("layout");
    let rect = Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(layout.size.0 as f32 * scale, layout.size.1 as f32 * scale),
    );
    let mut events = events;
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let input = egui::RawInput {
        events,
        time: Some(time),
        ..Default::default()
    };
    let mut output = context.run_ui(input, |ui| {
        view.paint_scaled(ui, rect, &fixture.snapshot(), fixture.graph)
            .expect("painting");
    });
    let cursor = output.platform_output.cursor_icon;
    output.textures_delta.clear();
    (view.take_events(), cursor)
}

fn context() -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(font_definitions());
    context
}

/// Drags from `from` to `to` in five steps; every signal is collected, and none
/// may precede the release.
fn drag(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    from: Pos2,
    to: Pos2,
) -> Vec<GraphEvent> {
    drag_with(context, view, fixture, from, to, 1.0, egui::Modifiers::NONE)
}

/// [`drag`] at a display scale, with modifiers held during the whole gesture.
fn drag_with(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    from: Pos2,
    to: Pos2,
    scale: f32,
    modifiers: egui::Modifiers,
) -> Vec<GraphEvent> {
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    let mut time = 0.1;
    let (mut signals, _) = frame_with(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(from), button(from, true)],
        time,
        scale,
        modifiers,
    );
    for step in 1..=5 {
        time += 0.05;
        let pos = from + (to - from) * (step as f32 / 5.0);
        let (events, _) = frame_with(
            context,
            view,
            fixture,
            vec![egui::Event::PointerMoved(pos)],
            time,
            scale,
            modifiers,
        );
        signals.extend(events);
        assert!(signals.is_empty(), "a signal before the release");
    }
    let (events, _) = frame_with(
        context,
        view,
        fixture,
        vec![button(to, false)],
        time + 0.05,
        scale,
        modifiers,
    );
    signals.extend(events);
    signals
}

/// The first cell as drawn: (cell right, cell bottom, frame bottom, frame
/// centre x), in figure pixels = screen points at scale 1.
fn geometry(view: &GraphView) -> (f32, f32, f32, f32) {
    let panel = &view.last_drawn().expect("drawn").panels[0];
    (
        (panel.cell_origin.0 + panel.cell_size.0 as i32) as f32,
        (panel.cell_origin.1 + panel.cell_size.1 as i32) as f32,
        (panel.pixel_origin.1 + panel.pixel_size.1 as i32) as f32,
        panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0,
    )
}

fn ready(fixture: &Fixture) -> (egui::Context, GraphView) {
    let context = context();
    let mut view = GraphView::new();
    assert!(frame(&context, &mut view, fixture, vec![], 0.0)
        .0
        .is_empty());
    (context, view)
}

/// Pulling the right boundary: the preview changes the layout (so the window
/// that follows the figure sees it) without touching the model, and a single
/// signal carries the new frame width on release.
#[test]
fn pulling_the_right_boundary_signals_the_new_frame_width_on_release_only() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let (right, bottom, _, _) = geometry(&view);
    let revision = fixture.snapshot().revision;
    let size = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;

    let from = Pos2::new(right - 2.0, bottom / 2.0);
    let (_, hover) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from)],
        0.05,
    );
    assert_eq!(hover, egui::CursorIcon::ResizeHorizontal);

    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(60.0, 0.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FrameResized {
            column: Some((0, 360.0)),
            row: None
        }]
    );
    assert_eq!(fixture.snapshot().revision, revision, "the model moved");
    let after = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout");
    assert_eq!(
        after.size,
        (size.0 + 60, size.1),
        "the preview is held until the model changes"
    );
    assert_eq!(after.frame_widths, [360.0]);
}

/// The preview lays the figure out while the pointer is still down.
#[test]
fn the_preview_resizes_the_figure_during_the_drag() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let (right, bottom, _, _) = geometry(&view);
    let size = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;
    let from = Pos2::new(right - 2.0, bottom / 2.0);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from), button_event(from, true)],
        0.1,
    );
    let mut time = 0.1;
    for step in 1..=4 {
        time += 0.05;
        frame(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(
                from + egui::vec2(10.0 * step as f32, 0.0),
            )],
            time,
        );
    }
    let during = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;
    assert_eq!(during, (size.0 + 40, size.1));
}

/// The bottom boundary changes the row's frame height; the corner both, in a
/// single signal.
#[test]
fn the_bottom_boundary_and_the_corner_signal_once() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let (right, bottom, _, _) = geometry(&view);

    let from = Pos2::new(60.0, bottom - 2.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(0.0, -50.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FrameResized {
            column: None,
            row: Some((0, 150.0))
        }]
    );

    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let corner = Pos2::new(right - 2.0, bottom - 2.0);
    let (_, cursor) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(corner)],
        0.05,
    );
    assert_eq!(cursor, egui::CursorIcon::ResizeNwSe);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner + egui::vec2(20.0, 30.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FrameResized {
            column: Some((0, 320.0)),
            row: Some((0, 230.0))
        }]
    );
}

/// A drag that starts on the axis line, or inside the frame, is no handle.
#[test]
fn a_drag_on_the_axis_line_or_in_the_frame_resizes_nothing() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let (_, _, frame_bottom, centre) = geometry(&view);
    let on_axis = Pos2::new(centre, frame_bottom - 1.0);
    let (_, cursor) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(on_axis)],
        0.05,
    );
    assert_eq!(cursor, egui::CursorIcon::Default, "no resize cursor");
    for from in [on_axis, Pos2::new(centre, frame_bottom - 60.0)] {
        let signals = drag(
            &context,
            &mut view,
            &fixture,
            from,
            from + egui::vec2(30.0, 40.0),
        );
        assert!(
            !signals
                .iter()
                .any(|signal| matches!(signal, GraphEvent::FrameResized { .. })),
            "{signals:?}"
        );
    }
}

/// A graph from an old file previews and signals the frames it is drawn with,
/// moved by the drag.
#[test]
fn a_legacy_graph_is_pulled_from_the_frames_it_is_drawn_with() {
    let fixture = Fixture::new(true);
    let (context, mut view) = ready(&fixture);
    let drawn = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .frame_widths[0];
    let (right, bottom, _, _) = geometry(&view);
    let from = Pos2::new(right - 2.0, bottom / 2.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(25.0, 0.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FrameResized {
            column: Some((0, drawn + 25.0)),
            row: None
        }]
    );
}

/// A press and a release that moved nothing (or under half a point) commit
/// nothing and leave no preview behind.
#[test]
fn a_drag_that_comes_back_signals_nothing() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let (right, bottom, _, _) = geometry(&view);
    let from = Pos2::new(right - 2.0, bottom / 2.0);
    let size = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;
    let signals = drag(&context, &mut view, &fixture, from, from);
    assert!(signals.is_empty(), "{signals:?}");
    let after = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;
    assert_eq!(after, size);
}

/// Pulling the left edge of the figure outwards gives the frame more room on
/// the left: one signal on release, Left = start + distance, nothing before.
#[test]
fn pulling_the_left_edge_signals_the_new_left_on_release_only() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let revision = fixture.snapshot().revision;
    let size = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;
    let from = Pos2::new(2.0, size.1 as f32 / 2.0);
    let (_, hover) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from)],
        0.05,
    );
    assert_eq!(hover, egui::CursorIcon::ResizeHorizontal);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(-40.0, 0.0),
    );
    // 76 pt of left axis and margin + 40.
    assert_eq!(
        signals,
        [GraphEvent::FramePlaced {
            left: Some(116.0),
            top: None
        }]
    );
    assert_eq!(fixture.snapshot().revision, revision, "the model moved");
    let after = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout");
    assert_eq!(after.size, (size.0 + 40, size.1), "the preview is held");
    assert_eq!(after.frame_rect(0, 0).map(|r| r.0), Some(116));
}

/// The top edge, and the corner (both at once, one signal); pulling inwards
/// below what the axes need stops at the minimum.
#[test]
fn pulling_the_top_edge_and_the_corner_signal_once() {
    let fixture = Fixture::placed(false, Some(150.0), Some(120.0));
    let (context, mut view) = ready(&fixture);
    let size = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;
    let from = Pos2::new(size.0 as f32 / 2.0, 2.0);
    let (_, hover) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from)],
        0.05,
    );
    assert_eq!(hover, egui::CursorIcon::ResizeVertical);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(0.0, 30.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FramePlaced {
            left: None,
            top: Some(90.0)
        }]
    );

    let fixture = Fixture::placed(false, Some(150.0), Some(120.0));
    let (context, mut view) = ready(&fixture);
    let corner = Pos2::new(2.0, 2.0);
    let (_, cursor) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(corner)],
        0.05,
    );
    assert_eq!(cursor, egui::CursorIcon::ResizeNwSe);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner + egui::vec2(500.0, 500.0),
    );
    // The axes need 76 on the left and 12 on top: the values stop there.
    assert_eq!(
        signals,
        [GraphEvent::FramePlaced {
            left: Some(76.0),
            top: Some(12.0)
        }]
    );
    assert_eq!(view.placing_edges(), (false, false), "the gesture is over");
}

/// A click on the left axis line is no handle.
#[test]
fn the_left_axis_line_is_not_the_left_edge() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let panel = view.last_drawn().expect("drawn").panels[0].clone();
    let on_axis = Pos2::new(
        panel.pixel_origin.0 as f32 + 1.0,
        panel.pixel_origin.1 as f32 + 80.0,
    );
    let (_, cursor) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(on_axis)],
        0.05,
    );
    assert_eq!(cursor, egui::CursorIcon::Default);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        on_axis,
        on_axis + egui::vec2(-30.0, 0.0),
    );
    assert!(signals.is_empty(), "{signals:?}");
}

/// #405: a graph from an old file stores no frame, yet its left edge is pulled
/// from the plot area it is drawn with (76 pt), not from zero: pulling out by
/// 40 gives 116, pushing in below what the axes need changes nothing and sends
/// no signal (no stray history step).
#[test]
fn a_legacy_graph_edge_starts_from_its_drawn_plot_area() {
    let fixture = Fixture::new(true);
    let (context, mut view) = ready(&fixture);
    let from = Pos2::new(2.0, 200.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(-40.0, 0.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FramePlaced {
            left: Some(116.0),
            top: None
        }]
    );

    let fixture = Fixture::new(true);
    let (context, mut view) = ready(&fixture);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(60.0, 0.0),
    );
    assert!(signals.is_empty(), "{signals:?}");
}

/// #405: the handle can be taken from the margin just outside the figure (the
/// window leaves room around it), where the window's own border never reaches.
#[test]
fn the_outer_edges_are_grabbed_from_the_margin_outside_the_figure() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let size = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .size;
    // 3 points left of the figure: inside the strip, outside the figure.
    let from = Pos2::new(-3.0, size.1 as f32 / 2.0);
    let (_, cursor) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from)],
        0.05,
    );
    assert_eq!(cursor, egui::CursorIcon::ResizeHorizontal);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(-40.0, 0.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FramePlaced {
            left: Some(116.0),
            top: None
        }]
    );
    // Same on the right outer edge, 3 points beyond the figure.
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let from = Pos2::new(size.0 as f32 + 3.0, 100.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(30.0, 0.0),
    );
    assert_eq!(
        signals,
        [GraphEvent::FrameResized {
            column: Some((0, 330.0)),
            row: None
        }]
    );
}

/// Layer `base` of the free fixture as laid out: its outer rectangle and its
/// frame in figure pixels.
fn free_cell(
    view: &mut GraphView,
    context: &egui::Context,
    fixture: &Fixture,
    base: usize,
) -> stokesforge_render::pipeline::FreeCellRect {
    *view
        .figure_layout(context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
        .free_cells
        .iter()
        .find(|cell| cell.base == base)
        .expect("cell")
}

/// The stored frame of layer `base`.
fn stored(fixture: &Fixture, base: usize) -> FreeFrame {
    let snapshot = fixture.snapshot();
    let node = snapshot.tree.node(fixture.graph).expect("graph");
    let stokesforge_core::node::NodeKind::Graph { spec, .. } = node.kind() else {
        panic!("a graph");
    };
    spec.panels[base].sanitized_free_frame()
}

/// One quiet frame at `scale`, so that the view knows the display scale it is
/// pressed at (it reads the geometry of the last drawn frame).
fn warm_up(context: &egui::Context, view: &mut GraphView, fixture: &Fixture, scale: f32) {
    frame_with(
        context,
        view,
        fixture,
        vec![],
        0.02,
        scale,
        egui::Modifiers::NONE,
    );
}

fn framed(signals: &[GraphEvent]) -> (usize, FreeFrame, bool) {
    match signals {
        [GraphEvent::LayerFramed {
            panel,
            frame,
            resized,
        }] => (*panel, *frame, *resized),
        other => panic!("one LayerFramed expected, got {other:?}"),
    }
}

fn center(cell: &stokesforge_render::pipeline::FreeCellRect, scale: f32) -> Pos2 {
    let (x, y, w, h) = cell.frame;
    Pos2::new(
        (x as f32 + w as f32 / 2.0) * scale,
        (y as f32 + h as f32 / 2.0) * scale,
    )
}

/// #398 L2: a plain drag in the plot area moves the layer by the distance
/// dragged divided by the display scale, with a single signal at the release.
/// Witness: dropping the division by the scale, or signalling during the drag.
#[test]
fn dragging_the_plot_area_moves_the_layer_in_free_placement() {
    for scale in [1.0_f32, 2.0] {
        let fixture = Fixture::free_two_layers();
        let (context, mut view) = ready(&fixture);
        warm_up(&context, &mut view, &fixture, scale);
        let cell = free_cell(&mut view, &context, &fixture, 0);
        let from = center(&cell, scale);
        let signals = drag_with(
            &context,
            &mut view,
            &fixture,
            from,
            from + egui::vec2(40.0, 20.0),
            scale,
            egui::Modifiers::NONE,
        );
        let (panel, frame, resized) = framed(&signals);
        let before = stored(&fixture, 0);
        assert_eq!(panel, 0);
        assert!(!resized);
        assert!(
            (frame.left - (before.left + 40.0 / f64::from(scale))).abs() < 0.011,
            "scale {scale}: {frame:?}"
        );
        assert!((frame.top - (before.top + 20.0 / f64::from(scale))).abs() < 0.011);
        assert_eq!((frame.width, frame.height), (300.0, 200.0));
    }
}

/// #398 L2: the preview follows the pointer without touching the model.
#[test]
fn the_move_previews_the_layer_before_the_release() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let cell = free_cell(&mut view, &context, &fixture, 0);
    let from = center(&cell, 1.0);
    let revision = fixture.snapshot().revision;
    let (signals, _) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from), button_event(from, true)],
        0.1,
    );
    assert!(signals.is_empty());
    for (step, time) in [(10.0, 0.15), (30.0, 0.2), (40.0, 0.25)] {
        let (signals, cursor) = frame(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(from + egui::vec2(step, 0.0))],
            time,
        );
        assert!(signals.is_empty());
        // The gesture starts during the first of these frames.
        if step > 10.0 {
            assert_eq!(cursor, egui::CursorIcon::Move);
        }
    }
    let moved = free_cell(&mut view, &context, &fixture, 0);
    assert_eq!(moved.frame.0, cell.frame.0 + 40, "the layer is drawn moved");
    assert_eq!(fixture.snapshot().revision, revision, "model untouched");
}

/// #398 L2: a click without movement does nothing but activate.
#[test]
fn a_click_without_movement_moves_nothing() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let cell = free_cell(&mut view, &context, &fixture, 0);
    let at = center(&cell, 1.0);
    let (mut signals, _) = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(at), button_event(at, true)],
        0.1,
    );
    signals.extend(
        frame(
            &context,
            &mut view,
            &fixture,
            vec![button_event(at, false)],
            0.15,
        )
        .0,
    );
    assert!(signals.is_empty(), "{signals:?}");
    // Nor does the model change: the layer is still where it was.
    assert_eq!(stored(&fixture, 0).left, 400.0);
}

/// #398: the grid is unchanged, a plain drag in a plot area does nothing.
#[test]
fn a_plain_drag_in_the_plot_area_does_nothing_on_the_grid() {
    let fixture = Fixture::new(false);
    let (context, mut view) = ready(&fixture);
    let panel = view.last_drawn().expect("drawn").panels[0].clone();
    let from = Pos2::new(
        panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0,
        panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0,
    );
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(40.0, 20.0),
    );
    assert!(signals.is_empty(), "{signals:?}");
}

/// #398: Ctrl + drag pans, before everything: no layer moves.
#[test]
fn ctrl_drag_pans_instead_of_moving_the_layer() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let cell = free_cell(&mut view, &context, &fixture, 0);
    let from = center(&cell, 1.0);
    let signals = drag_with(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(40.0, 20.0),
        1.0,
        egui::Modifiers::COMMAND,
    );
    assert!(signals.is_empty(), "{signals:?}");
    assert!(view.view().generation() > 0, "the view was panned");
}

/// #398: a drag started on the legend moves the legend, not the layer.
#[test]
fn a_drag_started_on_the_legend_moves_the_legend_not_the_layer() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let legend = stokesforge_render_egui::TextTarget::Legend { panel: 0 };
    let (center, size, _) = view.text_screen_box(legend).expect("legend drawn");
    let entry = center + egui::vec2(0.0, -size.y / 2.0 + 14.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        entry,
        entry + egui::vec2(-30.0, 20.0),
    );
    assert!(
        matches!(&signals[..], [GraphEvent::TextMoved { target, .. }] if *target == legend),
        "{signals:?}"
    );
}

/// #398: the four edges resize the frame; left and top move the position and
/// keep the opposite edge still; corners do both. Resize cursors on hover.
#[test]
fn the_edges_resize_the_frame_and_the_opposite_edge_stays() {
    // (name, grab point of the outer rectangle, delta, expected frame, cursor)
    type EdgeCase = (
        &'static str,
        (f32, f32),
        egui::Vec2,
        (f64, f64, f64, f64),
        egui::CursorIcon,
    );
    let start = {
        let fixture = Fixture::free_two_layers_at((520.0, 40.0), (0, 0));
        stored(&fixture, 0)
    };
    // (name, grab point of the outer rectangle, delta, expected frame, cursor)
    let cases: [EdgeCase; 6] = [
        (
            "right",
            (1.0, 0.5),
            egui::vec2(30.0, 0.0),
            (start.left, start.top, 330.0, 200.0),
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            "left",
            (0.0, 0.5),
            egui::vec2(-25.0, 0.0),
            (start.left - 25.0, start.top, 325.0, 200.0),
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            "bottom",
            (0.5, 1.0),
            egui::vec2(0.0, 18.0),
            (start.left, start.top, 300.0, 218.0),
            egui::CursorIcon::ResizeVertical,
        ),
        (
            "top",
            (0.5, 0.0),
            egui::vec2(0.0, -12.0),
            (start.left, start.top - 12.0, 300.0, 212.0),
            egui::CursorIcon::ResizeVertical,
        ),
        (
            "bottom-right corner",
            (1.0, 1.0),
            egui::vec2(10.0, 15.0),
            (start.left, start.top, 310.0, 215.0),
            egui::CursorIcon::ResizeNwSe,
        ),
        (
            "top-left corner",
            (0.0, 0.0),
            egui::vec2(-10.0, -15.0),
            (start.left - 10.0, start.top - 15.0, 310.0, 215.0),
            egui::CursorIcon::ResizeNwSe,
        ),
    ];
    for (name, (fx, fy), delta, (left, top, width, height), cursor) in cases {
        let fixture = Fixture::free_two_layers_at((520.0, 40.0), (0, 0));
        let (context, mut view) = ready(&fixture);
        let cell = free_cell(&mut view, &context, &fixture, 0);
        let (x, y, w, h) = cell.outer;
        let from = Pos2::new(x as f32 + w as f32 * fx, y as f32 + h as f32 * fy);
        let (_, hovered) = frame(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(from)],
            0.05,
        );
        assert_eq!(hovered, cursor, "{name}");
        let signals = drag(&context, &mut view, &fixture, from, from + delta);
        let (panel, frame, resized) = framed(&signals);
        assert_eq!(panel, 0, "{name}");
        assert!(resized, "{name}");
        assert_eq!(
            (frame.left, frame.top, frame.width, frame.height),
            (left, top, width, height),
            "{name}"
        );
    }
}

/// #398: sizes stay within 36 and 2000 pt; a left edge dragged beyond the
/// minimum keeps width 36 and the right edge does not move. Witness: no clamp
/// on the left edge shifts the right edge.
#[test]
fn the_edges_keep_the_size_within_bounds() {
    let fixture = Fixture::free_two_layers_at((520.0, 40.0), (0, 0));
    let before = stored(&fixture, 0);
    let (context, mut view) = ready(&fixture);
    let cell = free_cell(&mut view, &context, &fixture, 0);
    let (x, y, w, h) = cell.outer;
    let left = Pos2::new(x as f32, y as f32 + h as f32 / 2.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        left,
        left + egui::vec2(800.0, 0.0),
    );
    let (_, frame, _) = framed(&signals);
    assert_eq!(frame.width, 36.0);
    assert_eq!(frame.left + frame.width, before.left + before.width);

    let fixture = Fixture::free_two_layers_at((520.0, 40.0), (0, 0));
    let (context, mut view) = ready(&fixture);
    let right = Pos2::new((x + w) as f32, y as f32 + h as f32 / 2.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        right,
        right + egui::vec2(-900.0, 0.0),
    );
    assert_eq!(framed(&signals).1.width, 36.0);
    let fixture = Fixture::free_two_layers_at((520.0, 40.0), (0, 0));
    let (context, mut view) = ready(&fixture);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        right,
        right + egui::vec2(5000.0, 0.0),
    );
    assert_eq!(framed(&signals).1.width, 2000.0);
}

/// #398: on an overlap the front-most layer takes the gesture. Witness: a
/// forward scan moves layer 0 in both cases.
#[test]
fn the_front_most_layer_takes_the_move() {
    for (stacks, front) in [((0, 0), 1), ((1, 0), 0)] {
        let fixture = Fixture::free_two_layers_at((100.0, 200.0), stacks);
        let (context, mut view) = ready(&fixture);
        let b = free_cell(&mut view, &context, &fixture, 1);
        // Inside both frames.
        let from = Pos2::new(b.frame.0 as f32 + 40.0, b.frame.1 as f32 + 40.0);
        let signals = drag(
            &context,
            &mut view,
            &fixture,
            from,
            from + egui::vec2(20.0, 0.0),
        );
        assert_eq!(framed(&signals).0, front, "stacks {stacks:?}");
    }
}

/// #398 L3: the edges of the moved frame snap to the edges of the other layers
/// (6 screen px), Shift held disables it, and the dashed guide shows while the
/// gesture lasts only. Layer 1 spans 40..340; layer 0 starts at 400.
#[test]
fn the_edges_snap_to_the_other_layers_unless_shift_is_held() {
    let run = |shift: bool, dx: f32, scale: f32| {
        let fixture = Fixture::free_two_layers();
        let (context, mut view) = ready(&fixture);
        warm_up(&context, &mut view, &fixture, scale);
        let cell = free_cell(&mut view, &context, &fixture, 0);
        let from = center(&cell, scale);
        let modifiers = egui::Modifiers {
            shift,
            ..egui::Modifiers::NONE
        };
        // Press and move (not released) to read the guides.
        frame_with(
            &context,
            &mut view,
            &fixture,
            vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ],
            0.1,
            scale,
            modifiers,
        );
        for (step, time) in [(0.5, 0.15), (1.0, 0.2)] {
            frame_with(
                &context,
                &mut view,
                &fixture,
                vec![egui::Event::PointerMoved(from + egui::vec2(dx * step, 0.0))],
                time,
                scale,
                modifiers,
            );
        }
        let guides = view
            .magnet_guides()
            .iter()
            .filter(|guide| guide.vertical)
            .count();
        let signals = frame_with(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerButton {
                pos: from + egui::vec2(dx, 0.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
            0.3,
            scale,
            modifiers,
        )
        .0;
        assert!(
            view.magnet_guides().is_empty(),
            "guides end with the gesture"
        );
        (framed(&signals).1.left, guides)
    };
    // Left edge 400 - 57 = 343: 3 pt from the right edge of layer 1 (340).
    let (left, guides) = run(false, -57.0, 1.0);
    assert_eq!(left, 340.0);
    assert_eq!(guides, 1);
    // 8 pt away: free.
    let (left, guides) = run(false, -52.0, 1.0);
    assert_eq!(left, 348.0);
    assert_eq!(guides, 0);
    // Shift: free at 3 pt.
    let (left, guides) = run(true, -57.0, 1.0);
    assert_eq!(left, 343.0);
    assert_eq!(guides, 0);
    // The reach is 6 screen px, so 3 pt at scale 2: 2.5 pt (5 px) snaps, 8 pt
    // (16 px) does not.
    let (left, _) = run(false, -115.0, 2.0);
    assert_eq!(left, 340.0);
    let (left, guides) = run(false, -2.0 * 52.0, 2.0);
    assert_eq!(left, 348.0);
    assert_eq!(guides, 0);
}

/// #398: a click picks the layer under the cursor by where it is drawn, not by
/// its grid cell: layer 1 sits left of layer 0, and clicking there activates
/// layer 1.
#[test]
fn a_click_activates_the_layer_drawn_under_the_cursor_in_free_placement() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let spec = {
        let snapshot = fixture.snapshot();
        let node = snapshot.tree.node(fixture.graph).expect("graph");
        let stokesforge_core::node::NodeKind::Graph { spec, .. } = node.kind() else {
            panic!("a graph");
        };
        spec.clone()
    };
    let drawn = view.last_drawn().expect("drawn").panels.clone();
    let centre = |index: usize| {
        let panel = drawn
            .iter()
            .find(|p| p.index == index)
            .expect("drawn layer");
        Pos2::new(
            panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0,
            panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0,
        )
    };
    assert!(centre(1).x < centre(0).x, "layer 1 is drawn on the left");
    let click = |view: &mut GraphView, at: Pos2, time: f64| {
        frame(
            &context,
            view,
            &fixture,
            vec![egui::Event::PointerMoved(at), button_event(at, true)],
            time,
        );
        frame(
            &context,
            view,
            &fixture,
            vec![button_event(at, false)],
            time + 0.05,
        );
    };
    click(&mut view, centre(1), 1.0);
    assert_eq!(view.active_panel(&spec), 1);
    click(&mut view, centre(0), 2.0);
    assert_eq!(view.active_panel(&spec), 0);
}

/// #398: on the area two overlapping layers share, a click activates the cell
/// in front, whichever has the lower index. Witness: a forward scan activates
/// layer 0 in both cases.
#[test]
fn a_click_on_the_shared_area_activates_the_front_cell() {
    for (stacks, front) in [((0, 0), 1), ((1, 0), 0)] {
        let fixture = Fixture::free_two_layers_at((100.0, 200.0), stacks);
        let (context, mut view) = ready(&fixture);
        let spec = {
            let snapshot = fixture.snapshot();
            let node = snapshot.tree.node(fixture.graph).expect("graph");
            let stokesforge_core::node::NodeKind::Graph { spec, .. } = node.kind() else {
                panic!("a graph");
            };
            spec.clone()
        };
        let drawn = view.last_drawn().expect("drawn").panels.clone();
        let (a, b) = (&drawn[0], &drawn[1]);
        // A point inside both plot areas.
        let at = Pos2::new(
            b.pixel_origin.0 as f32 + 20.0,
            b.pixel_origin.1 as f32 + 20.0,
        );
        assert!(at.x < (a.pixel_origin.0 + a.pixel_size.0 as i32) as f32);
        frame(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(at), button_event(at, true)],
            1.0,
        );
        frame(
            &context,
            &mut view,
            &fixture,
            vec![button_event(at, false)],
            1.05,
        );
        assert_eq!(view.active_panel(&spec), front, "stacks {stacks:?}");
    }
}

/// The spec of the fixture's graph.
fn spec_of(fixture: &Fixture) -> PlotSpec {
    let snapshot = fixture.snapshot();
    let node = snapshot.tree.node(fixture.graph).expect("graph");
    let stokesforge_core::node::NodeKind::Graph { spec, .. } = node.kind() else {
        panic!("a graph");
    };
    spec.clone()
}

/// A click at `at` with `modifiers` held (press, then release).
fn click_with(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    at: Pos2,
    modifiers: egui::Modifiers,
    time: f64,
) {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    frame_with(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(at), button(true)],
        time,
        1.0,
        modifiers,
    );
    frame_with(
        context,
        view,
        fixture,
        vec![button(false)],
        time + 0.05,
        1.0,
        modifiers,
    );
}

/// #398: Shift + click on another layer of a free figure selects it without
/// changing the active layer, a second one removes it, a plain click on a layer
/// clears the selection and activates that layer, and the object selection is
/// left alone. Witness: Shift + click changes the active layer, or does not
/// toggle.
#[test]
fn shift_click_selects_other_layers_and_a_plain_click_clears_them() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let spec = spec_of(&fixture);
    let drawn = view.last_drawn().expect("drawn").panels.clone();
    let centre = |index: usize| {
        let panel = drawn.iter().find(|p| p.index == index).expect("layer");
        Pos2::new(
            panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0,
            panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0,
        )
    };
    let shift = egui::Modifiers::SHIFT;
    click_with(
        &context,
        &mut view,
        &fixture,
        centre(0),
        egui::Modifiers::NONE,
        1.0,
    );
    assert_eq!(view.active_panel(&spec), 0);
    assert!(view.extra_layers(&spec).is_empty());

    let object = view.selected_series();
    click_with(&context, &mut view, &fixture, centre(1), shift, 2.0);
    assert_eq!(view.extra_layers(&spec), vec![1], "selected");
    assert_eq!(view.active_panel(&spec), 0, "the reference does not change");
    assert_eq!(
        view.selected_series(),
        object,
        "the object selection is left alone"
    );
    assert!(view.take_events().is_empty(), "no move started");

    click_with(&context, &mut view, &fixture, centre(1), shift, 3.0);
    assert!(
        view.extra_layers(&spec).is_empty(),
        "a second Shift + click removes it"
    );
    assert_eq!(view.active_panel(&spec), 0);

    click_with(&context, &mut view, &fixture, centre(1), shift, 4.0);
    assert_eq!(view.extra_layers(&spec), vec![1]);
    click_with(
        &context,
        &mut view,
        &fixture,
        centre(1),
        egui::Modifiers::NONE,
        5.0,
    );
    assert_eq!(
        view.active_panel(&spec),
        1,
        "a plain click activates the layer"
    );
    assert!(
        view.extra_layers(&spec).is_empty(),
        "and clears the selection"
    );
}

/// #398: a curve hidden by a layer in front shows no handle where the front
/// layer covers it; the front layer's own curve keeps all of its handles.
/// Witness: dropping the filter, or hiding with `>=` on the rank.
#[test]
fn a_curve_loses_its_handles_where_a_front_layer_covers_it() {
    let count = |lefts: (f64, f64), panel: usize| {
        let fixture = Fixture::free_two_layers_at(lefts, (0, 0));
        let (_, view) = ready(&fixture);
        view.series_handle_centers(stokesforge_plot::SeriesRef::new(panel, 0))
            .len()
    };
    let (apart_back, apart_front) = (count((100.0, 500.0), 0), count((100.0, 500.0), 1));
    assert!(apart_back > 4, "the curve has handles: {apart_back}");
    assert!(
        count((100.0, 200.0), 0) < apart_back,
        "the back curve is covered"
    );
    assert_eq!(
        count((100.0, 200.0), 1),
        apart_front,
        "the front curve is whole"
    );
}

/// #398: a right-click on a layer inside the selection of layers keeps the
/// selection and its reference; outside it, the layer pointed at becomes the
/// active one and the selection is cleared. Witness: a right-click that always
/// resets the selection.
#[test]
fn a_right_click_inside_the_layer_selection_keeps_it() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let spec = spec_of(&fixture);
    let drawn = view.last_drawn().expect("drawn").panels.clone();
    let centre = |index: usize| {
        let panel = drawn.iter().find(|p| p.index == index).expect("layer");
        Pos2::new(
            panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0,
            panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0,
        )
    };
    let right_click = |view: &mut GraphView, at: Pos2, time: f64| {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &context,
            view,
            &fixture,
            vec![egui::Event::PointerMoved(at), button(true)],
            time,
        );
        frame(&context, view, &fixture, vec![button(false)], time + 0.05);
    };
    click_with(
        &context,
        &mut view,
        &fixture,
        centre(0),
        egui::Modifiers::NONE,
        1.0,
    );
    click_with(
        &context,
        &mut view,
        &fixture,
        centre(1),
        egui::Modifiers::SHIFT,
        2.0,
    );
    assert_eq!(view.extra_layers(&spec), vec![1]);
    right_click(&mut view, centre(1), 3.0);
    assert_eq!(view.active_panel(&spec), 0, "the reference stays");
    assert_eq!(view.extra_layers(&spec), vec![1], "the selection stays");
    // Outside the selection: a lone layer replaces it.
    click_with(
        &context,
        &mut view,
        &fixture,
        centre(1),
        egui::Modifiers::NONE,
        4.0,
    );
    assert!(view.extra_layers(&spec).is_empty());
    right_click(&mut view, centre(0), 5.0);
    assert_eq!(view.active_panel(&spec), 0);
    assert!(view.extra_layers(&spec).is_empty());
}

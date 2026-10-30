//! The resize handles on the outer boundary of a cell (#394), end to end: the
//! session, an `egui::Context` and the pointer events of a drag.
//!
//! No graphics server: `egui::Context::run_ui` produces shapes without painting
//! anything.

use egui::{Pos2, Rect};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
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
        }
        let graph = project.add_graph(root, None, spec).expect("graph");
        Self {
            session: Session::spawn(project, None).expect("session"),
            graph,
        }
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
    let layout = view
        .figure_layout(context, &fixture.snapshot(), fixture.graph)
        .expect("layout");
    let rect = Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(layout.size.0 as f32, layout.size.1 as f32),
    );
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
    let mut time = 0.1;
    let (mut signals, _) = frame(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(from), button_event(from, true)],
        time,
    );
    for step in 1..=5 {
        time += 0.05;
        let pos = from + (to - from) * (step as f32 / 5.0);
        let (events, _) = frame(
            context,
            view,
            fixture,
            vec![egui::Event::PointerMoved(pos)],
            time,
        );
        signals.extend(events);
        assert!(signals.is_empty(), "a signal before the release");
    }
    let (events, _) = frame(
        context,
        view,
        fixture,
        vec![button_event(to, false)],
        time + 0.05,
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

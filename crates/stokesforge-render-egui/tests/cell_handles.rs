//! The mouse gestures of a graph outside the Page Layout Mode, end to end (#426):
//! the session, an `egui::Context` and the pointer events of a drag. Resizing and
//! moving the layers happens only in the mode (`layout_mode.rs`); here the old
//! edges and the plot areas do nothing, and the object drags still work.
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

fn center(cell: &stokesforge_render::pipeline::FreeCellRect, scale: f32) -> Pos2 {
    let (x, y, w, h) = cell.frame;
    Pos2::new(
        (x as f32 + w as f32 / 2.0) * scale,
        (y as f32 + h as f32 / 2.0) * scale,
    )
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

/// Hover, press, drag and release at `from`: no signal, the default cursor on
/// the way, and the figure laid out as before.
fn assert_nothing_happens(fixture: &Fixture, from: Pos2, what: &str) {
    let (context, mut view) = ready(fixture);
    let (context, view) = (&context, &mut view);
    let layout = |view: &mut GraphView| {
        view.figure_layout(context, &fixture.snapshot(), fixture.graph)
            .expect("layout")
    };
    let before = layout(view);
    let revision = fixture.snapshot().revision;
    let (signals, cursor) = frame(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(from)],
        0.05,
    );
    assert!(signals.is_empty(), "{what}: hover {signals:?}");
    assert_eq!(cursor, egui::CursorIcon::Default, "{what}: cursor");
    let signals = drag(context, view, fixture, from, from + egui::vec2(30.0, 25.0));
    assert!(signals.is_empty(), "{what}: drag {signals:?}");
    let (_, cursor) = frame(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(from + egui::vec2(30.0, 25.0))],
        2.0,
    );
    assert_eq!(cursor, egui::CursorIcon::Default, "{what}: cursor after");
    assert_eq!(layout(view), before, "{what}: preview left behind");
    assert_eq!(fixture.snapshot().revision, revision, "{what}: model");
}

/// #426: outside the Page Layout Mode no drag resizes or moves a graph, grid or
/// Free: not the outer boundary of a cell, not the left and top edges of the
/// figure, not the edges or the plot area of a Free layer; and the pointer
/// keeps its default cursor over all of them. Witness: restoring one of the
/// removed handlers (a cell edge, the figure edge, or the layer move).
#[test]
fn no_layout_drag_outside_the_mode() {
    // Regular grid: the old cell and figure handles.
    let fixture = Fixture::new(false);
    let (_context, view) = ready(&fixture);
    let (right, bottom, frame_bottom, centre) = geometry(&view);
    let panel = view.last_drawn().expect("drawn").panels[0].clone();
    let middle = bottom / 2.0;
    for (what, at) in [
        ("right boundary", Pos2::new(right - 2.0, middle)),
        ("bottom boundary", Pos2::new(centre, bottom - 2.0)),
        ("corner", Pos2::new(right - 2.0, bottom - 2.0)),
        ("left figure edge", Pos2::new(2.0, middle)),
        ("top figure edge", Pos2::new(centre, 2.0)),
        ("axis line", Pos2::new(centre, frame_bottom - 1.0)),
        (
            "left axis line",
            Pos2::new(
                panel.pixel_origin.0 as f32 + 1.0,
                panel.pixel_origin.1 as f32 + 80.0,
            ),
        ),
        (
            "plot area",
            Pos2::new(
                panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0,
                panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0,
            ),
        ),
    ] {
        assert_nothing_happens(&fixture, at, what);
    }
    // A legacy graph (no stored frames) too.
    let legacy = Fixture::new(true);
    let (_context, view) = ready(&legacy);
    let (right, bottom, _, _) = geometry(&view);
    assert_nothing_happens(
        &legacy,
        Pos2::new(right - 2.0, bottom / 2.0),
        "legacy right boundary",
    );
    assert_nothing_happens(&legacy, Pos2::new(2.0, 200.0), "legacy left edge");

    // Free: the edges and corners of a layer, its plot area, the page margin.
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = ready(&fixture);
    let cell = free_cell(&mut view, &context, &fixture, 0);
    let (ox, oy, ow, oh) = cell.outer;
    let (ox, oy, ow, oh) = (ox as f32, oy as f32, ow as f32, oh as f32);
    for (what, at) in [
        ("layer right edge", Pos2::new(ox + ow - 2.0, oy + oh / 2.0)),
        ("layer left edge", Pos2::new(ox + 2.0, oy + oh / 2.0)),
        ("layer top edge", Pos2::new(ox + ow / 2.0, oy + 2.0)),
        ("layer bottom edge", Pos2::new(ox + ow / 2.0, oy + oh - 2.0)),
        ("layer corner", Pos2::new(ox + ow - 2.0, oy + oh - 2.0)),
        ("layer plot area", center(&cell, 1.0)),
    ] {
        assert_nothing_happens(&fixture, at, what);
    }
}

/// A Free graph with a title, a free text and a filled object, on layer 0.
fn free_with_objects() -> Fixture {
    let fixture = Fixture::free_two_layers();
    let snapshot = fixture.snapshot();
    let node = snapshot.tree.node(fixture.graph).expect("graph");
    let stokesforge_core::node::NodeKind::Graph { spec, .. } = node.kind() else {
        panic!("a graph");
    };
    let mut spec = spec.clone();
    spec.title = "Title".to_owned();
    let mut panel = spec.panels[0].clone();
    panel
        .annotations
        .push(stokesforge_plot::AnnotationSpec::new("Peak", 16.0, 24.0));
    panel.shapes.push(stokesforge_plot::ShapeSpec {
        fill_color: "#3366cc66".to_string(),
        ..stokesforge_plot::ShapeSpec::new(
            stokesforge_plot::ShapeKind::Rectangle,
            4.0,
            4.0,
            8.0,
            12.0,
        )
    });
    let mut panels: Vec<PanelSpec> = spec.panels.iter().cloned().collect();
    panels[0] = panel;
    spec.panels = Panels::new(panels);
    fixture
        .session
        .handle()
        .execute(stokesforge_commands::Command::SetPlotSpec {
            graph: fixture.graph,
            spec,
        })
        .expect("spec set");
    fixture
}

/// #426: the drags of the objects of a Free graph are untouched by the removal
/// of the layer gestures: a free text, a filled object and the title move, the
/// title's width handle resizes it, each with one signal on release. Witness:
/// gating the object handlers on the Page Layout Mode.
#[test]
fn text_shape_and_title_drags_still_work_on_a_free_graph() {
    use stokesforge_render_egui::TextTarget;
    let fixture = free_with_objects();
    let (context, mut view) = ready(&fixture);

    let annotation = stokesforge_plot::AnnotationRef { panel: 0, index: 0 };
    let (at, _, _) = view.annotation_screen_box(annotation).expect("text drawn");
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        at,
        at + egui::vec2(25.0, 10.0),
    );
    assert!(
        matches!(&signals[..], [GraphEvent::AnnotationMoved { target, .. }] if *target == annotation),
        "{signals:?}"
    );

    let drawn = view
        .drawn_shape(stokesforge_plot::ShapeRef { panel: 0, index: 0 })
        .expect("object drawn");
    let origin = view.display_rect().expect("rect").min.to_vec2();
    let middle = Pos2::new(
        (drawn.p1.0 + drawn.p2.0) as f32 / 2.0,
        (drawn.p1.1 + drawn.p2.1) as f32 / 2.0,
    ) + origin;
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        middle,
        middle + egui::vec2(20.0, 0.0),
    );
    assert!(
        matches!(&signals[..], [GraphEvent::ShapeChanged { .. }]),
        "{signals:?}"
    );

    let (title, size, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("title drawn");
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        title,
        title + egui::vec2(0.0, 12.0),
    );
    assert!(
        matches!(&signals[..], [GraphEvent::TextMoved { target, .. }] if *target == TextTarget::Title),
        "{signals:?}"
    );

    // Select the title (a click), then pull its width handle.
    let (title, size_now, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("title drawn");
    let _ = size;
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(title), button_event(title, true)],
        3.0,
    );
    frame(
        &context,
        &mut view,
        &fixture,
        vec![button_event(title, false)],
        3.1,
    );
    assert_eq!(view.selected_text(), Some(TextTarget::Title));
    let handle = title + egui::vec2(size_now.x / 2.0 + 3.0, 0.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        handle,
        handle + egui::vec2(60.0, 0.0),
    );
    assert!(
        matches!(&signals[..], [GraphEvent::TextResized { target, .. }] if *target == TextTarget::Title),
        "{signals:?}"
    );
}

//! The Page Layout Mode (#426d), end to end: the session, an `egui::Context` and
//! the pointer events of a drag.
//!
//! No graphics server: `egui::Context::run_ui` produces shapes without painting
//! anything.

#![allow(dead_code)]

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

    /// [`Self::free_two_layers`] moved so that the outer rectangles of the two
    /// layers touch exactly: layer 1 then layer 0 side by side, or, `stacked`,
    /// layer 1 above layer 0. The gutters are read from a first paint.
    fn touching(stacked: bool) -> Self {
        let fixture = Self::free_two_layers();
        let (_context, view) = in_mode(&fixture);
        let (g0, g1) = (box_of(&view, 0).gutters, box_of(&view, 1).gutters);
        let mut spec = spec_of(&fixture);
        let mut panels = vec![spec.panels[0].clone(), spec.panels[1].clone()];
        let place = |panel: &mut PanelSpec, left: f64, top: f64| {
            let mut frame = panel.sanitized_free_frame();
            frame.left = left;
            frame.top = top;
            panel.free_frame = Some(frame);
        };
        if stacked {
            place(&mut panels[1], 100.0, 40.0);
            place(&mut panels[0], 100.0, 40.0 + 200.0 + g1.3 + g0.1);
        } else {
            let left = 60.0 + g1.0;
            place(&mut panels[1], left, 40.0);
            place(&mut panels[0], left + 300.0 + g1.2 + g0.0, 40.0);
        }
        spec.panels = Panels::new(panels);
        fixture
            .session
            .handle()
            .execute(stokesforge_commands::Command::SetPlotSpec {
                graph: fixture.graph,
                spec,
            })
            .expect("touching spec");
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

/// What one frame produced.
struct Out {
    events: Vec<GraphEvent>,
    cursor: egui::CursorIcon,
    shapes: Vec<egui::epaint::ClippedShape>,
    cached: bool,
}

/// One frame with the figure shown at `scale` times its own size.
fn frame_with(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    events: Vec<egui::Event>,
    time: f64,
    modifiers: egui::Modifiers,
    scale: f32,
) -> Out {
    let layout = view
        .figure_layout(context, &fixture.snapshot(), fixture.graph)
        .expect("layout");
    let rect = Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(layout.size.0 as f32, layout.size.1 as f32) * scale,
    );
    let mut events = events;
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let input = egui::RawInput {
        events,
        time: Some(time),
        ..Default::default()
    };
    let mut cached = false;
    let mut output = context.run_ui(input, |ui| {
        cached = view
            .paint_scaled(ui, rect, &fixture.snapshot(), fixture.graph)
            .expect("painting");
    });
    output.textures_delta.clear();
    Out {
        events: view.take_events(),
        cursor: output.platform_output.cursor_icon,
        shapes: output.shapes,
        cached,
    }
}

fn frame(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    events: Vec<egui::Event>,
    time: f64,
    modifiers: egui::Modifiers,
) -> Vec<GraphEvent> {
    frame_with(context, view, fixture, events, time, modifiers, 1.0).events
}

fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// Drags in five steps; no signal may precede the release.
fn drag(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    from: Pos2,
    to: Pos2,
    modifiers: egui::Modifiers,
) -> Vec<GraphEvent> {
    let mut time = 0.1;
    let mut signals = frame(
        context,
        view,
        fixture,
        vec![
            egui::Event::PointerMoved(from),
            button(from, true, modifiers),
        ],
        time,
        modifiers,
    );
    for step in 1..=5 {
        time += 0.05;
        let pos = from + (to - from) * (step as f32 / 5.0);
        signals.extend(frame(
            context,
            view,
            fixture,
            vec![egui::Event::PointerMoved(pos)],
            time,
            modifiers,
        ));
        assert!(signals.is_empty(), "a signal before the release");
    }
    signals.extend(frame(
        context,
        view,
        fixture,
        vec![button(to, false, modifiers)],
        time + 0.05,
        modifiers,
    ));
    signals
}

fn click(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    at: Pos2,
    modifiers: egui::Modifiers,
    time: f64,
) {
    frame(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(at), button(at, true, modifiers)],
        time,
        modifiers,
    );
    frame(
        context,
        view,
        fixture,
        vec![button(at, false, modifiers)],
        time + 0.05,
        modifiers,
    );
}

fn spec_of(fixture: &Fixture) -> PlotSpec {
    let snapshot = fixture.snapshot();
    let node = snapshot.tree.node(fixture.graph).expect("graph");
    let stokesforge_core::node::NodeKind::Graph { spec, .. } = node.kind() else {
        panic!("a graph");
    };
    spec.clone()
}

/// A view in the Page Layout Mode, painted once so that it knows its boxes.
fn in_mode(fixture: &Fixture) -> (egui::Context, GraphView) {
    let context = egui::Context::default();
    context.set_fonts(font_definitions());
    let mut view = GraphView::new();
    assert!(frame(
        &context,
        &mut view,
        fixture,
        vec![],
        0.0,
        egui::Modifiers::NONE
    )
    .is_empty());
    view.set_layout_mode(true);
    frame(
        &context,
        &mut view,
        fixture,
        vec![],
        0.02,
        egui::Modifiers::NONE,
    );
    assert!(view.layout_mode());
    (context, view)
}

fn box_of(view: &GraphView, base: usize) -> stokesforge_render_egui::GraphBox {
    view.layout_boxes()
        .into_iter()
        .find(|graph| graph.base == base)
        .expect("box")
}

fn screen(view: &GraphView, x: f64, y: f64) -> Pos2 {
    view.stored_to_screen(x, y).expect("on screen")
}

/// The centre of the plot area of box `base`, on screen.
fn middle(view: &GraphView, base: usize) -> Pos2 {
    let frame = box_of(view, base).frame;
    screen(
        view,
        frame.left + frame.width / 2.0,
        frame.top + frame.height / 2.0,
    )
}

fn edited(
    signals: &[GraphEvent],
) -> (
    &[(usize, FreeFrame)],
    Option<stokesforge_render_egui::FreePage>,
) {
    match signals {
        [GraphEvent::LayoutEdited { frames, page }] => (frames, *page),
        other => panic!("one LayoutEdited expected, got {other:?}"),
    }
}

const SHIFT: egui::Modifiers = egui::Modifiers {
    shift: true,
    ..egui::Modifiers::NONE
};

/// Dragging the inside of a box moves it: one event at the release with the
/// moved frame, the same size, and the page left exactly as it was.
/// Witness: a page that follows the box, or an event during the drag.
#[test]
fn dragging_a_box_moves_it_and_keeps_the_page() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let page = view.layout_page().expect("page");
    let before = box_of(&view, 1);
    let from = middle(&view, 1);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(20.0, 0.0),
        SHIFT,
    );
    let (frames, new_page) = edited(&signals);
    assert_eq!(new_page, Some(page), "the figure keeps its size");
    let [(base, frame)] = frames else {
        panic!("one frame expected");
    };
    assert_eq!(*base, 1);
    assert!(
        (frame.left - before.frame.left - 20.0).abs() < 0.02,
        "{frame:?}"
    );
    assert_eq!(frame.top, before.frame.top);
    assert_eq!(frame.width, before.frame.width);
    assert_eq!(frame.height, before.frame.height);
}

/// The mode only previews: the model is untouched until the shell commits.
#[test]
fn the_move_is_previewed_and_the_model_is_untouched() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let stored_before = spec_of(&fixture);
    let from = middle(&view, 1);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from), button(from, true, SHIFT)],
        0.1,
        SHIFT,
    );
    for step in 1..=4 {
        let to = from + egui::vec2(8.0 * step as f32, 0.0);
        frame(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(to)],
            0.1 + 0.05 * step as f64,
            SHIFT,
        );
    }
    let moving = box_of(&view, 1);
    let start = stored_before.panels[1].sanitized_free_frame();
    assert!(moving.frame.left > start.left + 20.0, "{moving:?}");
    assert_eq!(spec_of(&fixture), stored_before);
}

/// Pulling a corner handle of a box resizes its outer rectangle: the frame
/// shrinks, the opposite corner stays, the page is unchanged.
#[test]
fn a_corner_handle_resizes_the_box_inside_the_page() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    // The handles of the selected box win over the box in front of it, whose
    // edge passes near this corner: select the box whose corner is pulled.
    let spec = spec_of(&fixture);
    view.set_active_panel(1, &spec);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        0.5,
        egui::Modifiers::NONE,
    );
    let page = view.layout_page().expect("page");
    let before = box_of(&view, 1);
    let corner = screen(
        &view,
        before.outer.left + before.outer.width,
        before.outer.top + before.outer.height,
    );
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner - egui::vec2(30.0, 20.0),
        SHIFT,
    );
    let (frames, new_page) = edited(&signals);
    assert_eq!(new_page, Some(page));
    let [(base, frame)] = frames else {
        panic!("one frame expected");
    };
    assert_eq!(*base, 1);
    assert!((frame.left - before.frame.left).abs() < 0.02, "{frame:?}");
    assert!((frame.top - before.frame.top).abs() < 0.02, "{frame:?}");
    assert!(
        (before.frame.width - frame.width - 30.0).abs() < 0.02,
        "{frame:?}"
    );
    assert!(
        (before.frame.height - frame.height - 20.0).abs() < 0.02,
        "{frame:?}"
    );
}

/// Pulling a page corner changes the page and scales every box with it.
#[test]
fn a_page_corner_drag_scales_the_page_and_the_boxes() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let page = view.layout_page().expect("page");
    let before = view.layout_boxes();
    let corner = screen(
        &view,
        page.left + page.width + 4.0,
        page.top + page.height + 4.0,
    );
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner + egui::vec2(100.0, 50.0),
        SHIFT,
    );
    let (frames, new_page) = edited(&signals);
    let new_page = new_page.expect("a page");
    assert!(
        (new_page.width - page.width - 100.0).abs() < 0.02,
        "{new_page:?}"
    );
    assert!(
        (new_page.height - page.height - 50.0).abs() < 0.02,
        "{new_page:?}"
    );
    assert_eq!((new_page.left, new_page.top), (page.left, page.top));
    assert_eq!(frames.len(), 2, "every box follows");
    let sx = new_page.width / page.width;
    for (base, frame) in frames {
        let old = before.iter().find(|g| g.base == *base).expect("box");
        let wanted = (old.outer.left - page.left) * sx + page.left + old.gutters.0;
        assert!((frame.left - wanted).abs() < 0.05, "{frame:?} vs {wanted}");
    }
}

/// Shift switches the grid off: the box lands where the pointer put it, while
/// without Shift its outer left edge lands on a grid line.
#[test]
fn shift_disables_the_grid() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    view.set_layout_grid(Some(20.0));
    let before = box_of(&view, 1);
    let page = view.layout_page().expect("page");
    let from = middle(&view, 1);
    // Just past a grid line: the grid pulls the box back onto it.
    let offset = before.outer.left - page.left;
    let line = ((offset / 20.0).floor() + 1.0) * 20.0;
    let delta = (line + 5.0 - offset) as f32;
    let free = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(delta, 0.0),
        SHIFT,
    );
    let (frames, _) = edited(&free);
    assert!((frames[0].1.left - before.frame.left - f64::from(delta)).abs() < 0.02);
    // The held preview of the first gesture waits for the model: a new view.
    let (context, mut view) = in_mode(&fixture);
    view.set_layout_grid(Some(20.0));
    let from = middle(&view, 1);
    let snapped = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(delta, 0.0),
        egui::Modifiers::NONE,
    );
    let (frames, _) = edited(&snapped);
    let outer_left = frames[0].1.left - before.gutters.0 - page.left;
    assert!(
        (outer_left - line).abs() < 0.02,
        "on the grid line: {outer_left} vs {line}"
    );
}

/// A burst of arrow keys is one event, committed after 0.7 s of quiet.
#[test]
fn a_nudge_burst_is_one_event_after_the_quiet_time() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    view.set_active_panel(1, &spec);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        0.5,
        egui::Modifiers::NONE,
    );
    let before = box_of(&view, 1);
    view.nudge((1, 0), 1.0);
    view.nudge((1, 0), 1.1);
    view.nudge((1, 0), 1.2);
    let mut signals = Vec::new();
    for time in [1.3, 1.6, 1.85] {
        signals.extend(frame(
            &context,
            &mut view,
            &fixture,
            vec![],
            time,
            egui::Modifiers::NONE,
        ));
    }
    assert!(signals.is_empty(), "quiet time not elapsed: {signals:?}");
    assert!(box_of(&view, 1).frame.left > before.frame.left, "previewed");
    signals.extend(frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        2.0,
        egui::Modifiers::NONE,
    ));
    signals.extend(frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        2.1,
        egui::Modifiers::NONE,
    ));
    let (frames, page) = edited(&signals);
    assert!(page.is_some());
    let step = 5.0 * 72.0 / 25.4;
    let [(1, frame)] = frames else {
        panic!("one frame expected: {frames:?}");
    };
    assert!(
        (frame.left - before.frame.left - 3.0 * step).abs() < 0.02,
        "{frame:?}"
    );
    assert_eq!(frame.top, before.frame.top);
}

/// Leaving the mode commits the pending burst.
#[test]
fn leaving_the_mode_commits_a_pending_nudge() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    view.set_active_panel(1, &spec);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        0.5,
        egui::Modifiers::NONE,
    );
    view.nudge((1, 0), 1.0);
    view.set_layout_mode(false);
    let signals = view.take_events();
    assert_eq!(signals.len(), 1, "{signals:?}");
    assert!(matches!(signals[0], GraphEvent::LayoutEdited { .. }));
}

/// A click selects the box under the pointer; Shift + click adds one.
#[test]
fn a_click_selects_the_box_and_shift_click_adds_one() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    let at = middle(&view, 1);
    click(
        &context,
        &mut view,
        &fixture,
        at,
        egui::Modifiers::NONE,
        1.0,
    );
    assert_eq!(view.active_panel(&spec), 1);
    assert_eq!(view.selected_box().map(|b| b.base), Some(1));
    let at = middle(&view, 0);
    click(&context, &mut view, &fixture, at, SHIFT, 2.0);
    assert_eq!(view.active_panel(&spec), 1);
    assert_eq!(view.extra_layers(&spec), vec![0]);
    let at = middle(&view, 0);
    click(
        &context,
        &mut view,
        &fixture,
        at,
        egui::Modifiers::NONE,
        3.0,
    );
    assert_eq!(view.active_panel(&spec), 0);
    assert!(view.extra_layers(&spec).is_empty());
}

/// In the mode a Ctrl + drag does not pan, and nothing but the layout moves.
#[test]
fn ctrl_drag_does_not_pan_in_the_mode() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let generation = view.view().generation();
    let from = middle(&view, 1);
    let ctrl = egui::Modifiers {
        command: true,
        ctrl: true,
        ..egui::Modifiers::NONE
    };
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(25.0, 0.0),
        ctrl,
    );
    assert_eq!(view.view().generation(), generation, "no pan");
    assert!(
        signals
            .iter()
            .all(|s| matches!(s, GraphEvent::LayoutEdited { .. })),
        "{signals:?}"
    );
}

/// Outside the mode, a drag on a layer edge or in a plot area is no layout
/// gesture any more.
#[test]
fn no_layout_drag_outside_the_mode() {
    let fixture = Fixture::free_two_layers();
    let context = egui::Context::default();
    context.set_fonts(font_definitions());
    let mut view = GraphView::new();
    frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        0.0,
        egui::Modifiers::NONE,
    );
    let layout = view
        .figure_layout(&context, &fixture.snapshot(), fixture.graph)
        .expect("layout");
    let cell = layout
        .free_cells
        .iter()
        .find(|c| c.base == 1)
        .expect("cell");
    let (fx, fy, fw, fh) = cell.frame;
    let (ox, oy, ow, oh) = cell.outer;
    let inside = Pos2::new(fx as f32 + fw as f32 / 2.0, fy as f32 + fh as f32 / 2.0);
    let edge = Pos2::new((ox + ow) as f32, (oy + oh / 2) as f32);
    for from in [inside, edge] {
        let signals = drag(
            &context,
            &mut view,
            &fixture,
            from,
            from + egui::vec2(30.0, 10.0),
            egui::Modifiers::NONE,
        );
        assert!(
            !signals.iter().any(|s| matches!(
                s,
                GraphEvent::LayoutEdited { .. }
                    | GraphEvent::LayerFramed { .. }
                    | GraphEvent::FrameResized { .. }
                    | GraphEvent::FramePlaced { .. }
            )),
            "{signals:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Boundaries between touching graphs
// ---------------------------------------------------------------------------

/// The pointer moves to `at`; the second frame has seen the hover.
fn hover(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    at: Pos2,
    time: f64,
    scale: f32,
) -> Out {
    frame_with(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(at)],
        time,
        egui::Modifiers::NONE,
        scale,
    );
    frame_with(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(at)],
        time + 0.02,
        egui::Modifiers::NONE,
        scale,
    )
}

/// Stored position of the line between layer 1 and layer 0, and the middle of
/// the shared edge along it.
fn shared_line(view: &GraphView, stacked: bool) -> (f64, f64) {
    let low = box_of(view, 1);
    if stacked {
        (
            low.outer.top + low.outer.height,
            low.outer.left + low.outer.width / 2.0,
        )
    } else {
        (
            low.outer.left + low.outer.width,
            low.outer.top + low.outer.height / 2.0,
        )
    }
}

/// A point `d` stored points across the line (towards layer 0), on the middle of
/// the shared edge.
fn across(view: &GraphView, stacked: bool, d: f64) -> Pos2 {
    let (line, along) = shared_line(view, stacked);
    if stacked {
        screen(view, along, line + d)
    } else {
        screen(view, line + d, along)
    }
}

/// A drag across the line moves both layers and ends in one event; dragging
/// back to the start ends in none. Witness: the page moved; an event during the
/// drag (the helper checks); a frame per layer in separate events.
#[test]
fn dragging_the_line_between_two_graphs_is_one_edit_of_both() {
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    let page = view.layout_page().expect("page");
    let before = (box_of(&view, 0), box_of(&view, 1));
    let from = across(&view, false, 0.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(20.0, 0.0),
        SHIFT,
    );
    let (frames, new_page) = edited(&signals);
    assert_eq!(new_page, Some(page), "the page keeps its size");
    assert_eq!(frames.len(), 2);
    let frame_of = |base: usize| frames.iter().find(|(b, _)| *b == base).expect("frame").1;
    // Layer 1 (low) grows by 20, layer 0 (high) loses 20 on its near side.
    let (low, high) = (frame_of(1), frame_of(0));
    assert!(
        (low.width - before.1.frame.width - 20.0).abs() < 0.02,
        "{low:?}"
    );
    assert!(
        (high.left - before.0.frame.left - 20.0).abs() < 0.02,
        "{high:?}"
    );
    assert!(
        (high.width - before.0.frame.width + 20.0).abs() < 0.02,
        "{high:?}"
    );
    assert_eq!(
        (low.left, low.top, low.height),
        (
            before.1.frame.left,
            before.1.frame.top,
            before.1.frame.height
        )
    );
    assert_eq!(
        (high.top, high.height),
        (before.0.frame.top, before.0.frame.height)
    );

    // Out and back to the start: nothing to commit.
    let (context, mut view) = in_mode(&fixture);
    let from = across(&view, false, 0.0);
    let mut signals = frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from), button(from, true, SHIFT)],
        0.1,
        SHIFT,
    );
    for (step, dx) in [15.0, 30.0, 10.0, 0.0].into_iter().enumerate() {
        signals.extend(frame(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(from + egui::vec2(dx, 0.0))],
            0.2 + 0.05 * step as f64,
            SHIFT,
        ));
    }
    signals.extend(frame(
        &context,
        &mut view,
        &fixture,
        vec![button(from, false, SHIFT)],
        0.6,
        SHIFT,
    ));
    assert!(signals.is_empty(), "{signals:?}");
}

/// The cursors: single arrows pointing out of the graph on a side band, double
/// arrows on the centre band and on a lone edge, diagonals on corners, and the
/// arrow of the press kept when the pointer crosses the line. Scale 2 counts the
/// bands in screen points. Witness: the hover re-resolved during the drag; the
/// raw 2.0 as a stored distance.
#[test]
fn the_cursors_follow_the_bands() {
    use egui::CursorIcon::*;
    for (stacked, double, low_side, high_side) in [
        (false, ResizeHorizontal, ResizeEast, ResizeWest),
        (true, ResizeVertical, ResizeSouth, ResizeNorth),
    ] {
        let fixture = Fixture::touching(stacked);
        let (context, mut view) = in_mode(&fixture);
        let mut time = 1.0;
        let mut cursor = |view: &mut GraphView, d: f64| {
            time += 0.1;
            let at = across(view, stacked, d);
            hover(&context, view, &fixture, at, time, 1.0).cursor
        };
        assert_eq!(cursor(&mut view, 0.0), double);
        assert_eq!(cursor(&mut view, -1.5), double);
        assert_eq!(cursor(&mut view, 1.5), double);
        assert_eq!(cursor(&mut view, -6.0), low_side);
        assert_eq!(cursor(&mut view, 6.0), high_side);
        assert_eq!(cursor(&mut view, -9.0), low_side);
        assert_eq!(cursor(&mut view, 9.0), high_side);
        assert_eq!(cursor(&mut view, -12.0), Move);
        assert_eq!(cursor(&mut view, 12.0), Move);
    }
    // A corner keeps its diagonal, even right on the shared line.
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    view.set_active_panel(1, &spec);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        0.5,
        egui::Modifiers::NONE,
    );
    let low = box_of(&view, 1);
    let corner = screen(
        &view,
        low.outer.left + low.outer.width - 1.0,
        low.outer.top + 2.0,
    );
    assert_eq!(
        hover(&context, &mut view, &fixture, corner, 1.0, 1.0).cursor,
        ResizeNeSw
    );
    // The arrow of the press stays while the pointer crosses the line.
    let from = across(&view, false, 6.0);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from), button(from, true, SHIFT)],
        2.0,
        SHIFT,
    );
    for (step, d) in [-10.0, -30.0].into_iter().enumerate() {
        let out = frame_with(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(from + egui::vec2(d as f32, 0.0))],
            2.1 + 0.1 * step as f64,
            SHIFT,
            1.0,
        );
        assert_eq!(out.cursor, ResizeWest, "step {step}");
    }
    // At scale 2 the bands are 1 / 5 stored points wide.
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    // One frame at the new scale so that the view knows where things are.
    frame_with(
        &context,
        &mut view,
        &fixture,
        vec![],
        0.9,
        egui::Modifiers::NONE,
        2.0,
    );
    let mut time = 1.0;
    for (d, wanted) in [(0.75, ResizeHorizontal), (1.5, ResizeWest), (5.5, Move)] {
        time += 0.1;
        let at = across(&view, false, d);
        assert_eq!(
            hover(&context, &mut view, &fixture, at, time, 2.0).cursor,
            wanted,
            "d {d}"
        );
    }
}

/// A click in a band, side or centre, selects the graph on the pointer's side.
/// Witness: `Side` left out of the click; always the low side.
#[test]
fn a_click_on_a_line_selects_the_pointer_side() {
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    let mut time = 1.0;
    for (d, wanted) in [(-6.0, 1), (1.0, 0), (-1.0, 1), (6.0, 0)] {
        time += 0.5;
        let at = across(&view, false, d);
        click(
            &context,
            &mut view,
            &fixture,
            at,
            egui::Modifiers::NONE,
            time,
        );
        assert_eq!(view.active_panel(&spec), wanted, "d {d}");
    }
}

/// White 7 x 7 squares of the corner handles, solid (white) and faint, and the
/// text shapes. Witness: the drag condition dropped; the early return before
/// the label.
fn handles(shapes: &[egui::epaint::ClippedShape]) -> (usize, usize, usize) {
    fn walk(shape: &egui::Shape, counts: &mut (usize, usize, usize)) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|shape| walk(shape, counts)),
            egui::Shape::Rect(rect) => {
                let size = rect.rect.size();
                if (size.x - 7.0).abs() < 0.01 && (size.y - 7.0).abs() < 0.01 {
                    if rect.fill == egui::Color32::WHITE {
                        counts.0 += 1;
                    } else if rect.fill.a() > 0 {
                        counts.1 += 1;
                    }
                }
            }
            egui::Shape::Text(_) => counts.2 += 1,
            _ => {}
        }
    }
    let mut counts = (0, 0, 0);
    for clipped in shapes {
        walk(&clipped.shape, &mut counts);
    }
    counts
}

#[test]
fn only_the_selected_and_the_hovered_graph_show_handles() {
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    view.set_active_panel(0, &spec);
    let elsewhere = Pos2::new(-100.0, -100.0);
    let quiet = hover(&context, &mut view, &fixture, elsewhere, 1.0, 1.0);
    let (solid, faint, texts) = handles(&quiet.shapes);
    assert_eq!((solid, faint), (4, 0), "the selected graph only");
    // Over the other graph: its handles show faintly, the selected ones stay.
    let over = middle(&view, 1);
    let out = hover(&context, &mut view, &fixture, over, 1.2, 1.0);
    let (solid, faint, with_hover) = handles(&out.shapes);
    assert_eq!((solid, faint), (4, 4));
    assert_eq!(with_hover, texts, "the size labels stay");
    // Over the selected one: nothing faint.
    let own = middle(&view, 0);
    let out = hover(&context, &mut view, &fixture, own, 1.4, 1.0);
    assert_eq!((handles(&out.shapes).0, handles(&out.shapes).1), (4, 0));
    // In the centre band both show faintly, one of them solid.
    let band = across(&view, false, 0.5);
    let out = hover(&context, &mut view, &fixture, band, 1.6, 1.0);
    assert_eq!((handles(&out.shapes).0, handles(&out.shapes).1), (4, 4));
    // During a drag of the line no handle is faint.
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(band), button(band, true, SHIFT)],
        2.0,
        SHIFT,
    );
    for step in 1..=3 {
        let out = frame_with(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(
                band + egui::vec2(4.0 * step as f32, 0.0),
            )],
            2.0 + 0.1 * step as f64,
            SHIFT,
            1.0,
        );
        let (solid, faint, labels) = handles(&out.shapes);
        assert_eq!((solid, faint), (4, 0), "step {step}");
        assert_eq!(labels, texts, "step {step}");
    }
}

/// Moving the pointer and switching the setting never touch the cache, the
/// model or the events. Witness: the hover through the frame preview.
#[test]
fn hovering_and_the_setting_leave_the_cache_and_the_model_alone() {
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    let stored = spec_of(&fixture);
    let generation = view.view().generation();
    let mut time = 1.0;
    for follow in [
        stokesforge_render_egui::BoundaryFollow::Pair,
        stokesforge_render_egui::BoundaryFollow::Line,
    ] {
        view.set_layout_boundary(follow);
        assert_eq!(view.layout_boundary(), follow);
        for d in [-30.0, -6.0, 0.0, 6.0, 30.0] {
            time += 0.1;
            let at = across(&view, false, d);
            let out = hover(&context, &mut view, &fixture, at, time, 1.0);
            assert!(out.cached, "d {d}");
            assert!(out.events.is_empty());
        }
    }
    assert_eq!(view.view().generation(), generation);
    assert_eq!(spec_of(&fixture), stored);
}

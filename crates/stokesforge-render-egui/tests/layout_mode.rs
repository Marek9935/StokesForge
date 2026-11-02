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

fn frame(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    events: Vec<egui::Event>,
    time: f64,
    modifiers: egui::Modifiers,
) -> Vec<GraphEvent> {
    let layout = view
        .figure_layout(context, &fixture.snapshot(), fixture.graph)
        .expect("layout");
    let rect = Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(layout.size.0 as f32, layout.size.1 as f32),
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
    output.textures_delta.clear();
    view.take_events()
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

//! The Page Layout Mode (#426), end to end: the session, an `egui::Context` and
//! the pointer events of a drag.
//!
//! No graphics server: `egui::Context::run_ui` produces shapes without painting
//! anything.

use egui::{Pos2, Rect};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::compose::{CellSide, MoveTarget};
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

/// The pointer moves to `at`; the second frame has seen the hover. The result
/// is the second frame's, with the events of both and `cached` true only if
/// both frames reused the cache.
fn hover(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    at: Pos2,
    time: f64,
    scale: f32,
) -> Out {
    let first = frame_with(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(at)],
        time,
        egui::Modifiers::NONE,
        scale,
    );
    let mut second = frame_with(
        context,
        view,
        fixture,
        vec![egui::Event::PointerMoved(at)],
        time + 0.02,
        egui::Modifiers::NONE,
        scale,
    );
    second.cached &= first.cached;
    second.events.splice(0..0, first.events);
    second
}

/// A point `d` stored points across the line between layer 1 and layer 0
/// (towards layer 0), on the middle of the shared edge.
fn across(view: &GraphView, stacked: bool, d: f64) -> Pos2 {
    let low = box_of(view, 1);
    if stacked {
        screen(
            view,
            low.outer.left + low.outer.width / 2.0,
            low.outer.top + low.outer.height + d,
        )
    } else {
        screen(
            view,
            low.outer.left + low.outer.width + d,
            low.outer.top + low.outer.height / 2.0,
        )
    }
}

impl Fixture {
    /// A regular grid of `columns` x `rows` graphs with stored frames of
    /// 200 x 150 pt; `legacy` stores none (an old file); `titled` gives the
    /// graph a title and `fixed` a Fixed figure size.
    fn grid(columns: usize, rows: usize, legacy: bool, titled: bool, fixed: bool) -> Self {
        let fixture = Self::new(true);
        let mut spec = spec_of(&fixture);
        let template = spec.panels[0].clone();
        let mut panels = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                let mut panel = template.clone();
                panel.row = row;
                panel.column = column;
                if titled {
                    panel.title = format!("Panel {row}{column}");
                    panel.title_visible = true;
                }
                panels.push(panel);
            }
        }
        spec.panels = Panels::new(panels);
        if !legacy {
            spec.frame_widths = vec![200.0; columns];
            spec.frame_heights = vec![150.0; rows];
        }
        if titled {
            spec.title = "A figure title".into();
            spec.title_visible = true;
        }
        spec.fixed_figure = fixed;
        fixture.replace(spec);
        fixture
    }

    /// Two panels far apart on a 17 x 17 grid: more cells than a layout stores
    /// frames for, so the figure falls back to a compact grid.
    fn compact() -> Self {
        let fixture = Self::new(true);
        let mut spec = spec_of(&fixture);
        let mut far = spec.panels[0].clone();
        far.row = 16;
        far.column = 16;
        let first = spec.panels[0].clone();
        spec.panels = Panels::new([first, far]);
        fixture.replace(spec);
        fixture
    }

    /// A free 2 x 2 whose four outer rectangles touch: bases 0 1 on the top
    /// row, 2 3 below. The gutters are read from a first paint.
    fn free_two_by_two() -> Self {
        let fixture = Self::new(false);
        let mut spec = spec_of(&fixture);
        let template = spec.panels[0].clone();
        let mut panels = Vec::new();
        for (row, column) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            let mut panel = template.clone();
            panel.row = row;
            panel.column = column;
            panel.free_frame = Some(FreeFrame {
                left: 60.0 + 400.0 * column as f64,
                top: 40.0 + 300.0 * row as f64,
                width: 300.0,
                height: 200.0,
                stack: 0,
            });
            panels.push(panel);
        }
        spec.panels = Panels::new(panels);
        spec.layer_placement = LayerPlacement::Free;
        fixture.replace(spec.clone());
        let (_context, view) = in_mode(&fixture);
        let g = box_of(&view, 0).gutters;
        let (ow, oh) = (300.0 + g.0 + g.2, 200.0 + g.1 + g.3);
        for (index, panel) in spec.panels.iter_mut().enumerate() {
            let (row, column) = ((index / 2) as f64, (index % 2) as f64);
            panel.free_frame = Some(FreeFrame {
                left: 60.0 + g.0 + column * ow,
                top: 40.0 + g.1 + row * oh,
                width: 300.0,
                height: 200.0,
                stack: 0,
            });
        }
        fixture.replace(spec);
        fixture
    }

    fn replace(&self, spec: PlotSpec) {
        self.session
            .handle()
            .execute(stokesforge_commands::Command::SetPlotSpec {
                graph: self.graph,
                spec,
            })
            .expect("spec");
    }
}

/// The layout as drawn, for the positions of the grid lines.
fn layout_of(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
) -> stokesforge_render::pipeline::FigureLayout {
    view.figure_layout(context, &fixture.snapshot(), fixture.graph)
        .expect("layout")
}

/// The screen position (scale 1) of the meeting of vertical line `i` and
/// horizontal line `j` of a regular grid.
fn meeting(layout: &stokesforge_render::pipeline::FigureLayout, i: usize, j: usize) -> Pos2 {
    Pos2::new(
        layout.column_breaks[i] as f32,
        (layout.row_breaks[j] + layout.title_band) as f32,
    )
}

fn grid_edited(signals: &[GraphEvent]) -> (&[f64], &[f64]) {
    match signals {
        [GraphEvent::GridFramesEdited {
            widths,
            heights,
            figure: false,
        }] => (widths, heights),
        other => panic!("one GridFramesEdited expected, got {other:?}"),
    }
}

fn hover_at(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    at: Pos2,
    time: f64,
) -> Out {
    hover(context, view, fixture, at, time, 1.0)
}

// ---------------------------------------------------------------------------
// Both placements: the mode is a view
// ---------------------------------------------------------------------------

/// Entering the mode and hovering over every handle sends no event and changes
/// neither the model nor the cache, on a Regular and on a Free graph, and a
/// Regular graph is not converted. Witness: a preview or a SetLayerPlacement
/// on hover.
#[test]
fn entering_and_hovering_change_nothing_in_either_placement() {
    for (fixture, placement) in [
        (
            Fixture::grid(2, 2, false, true, false),
            LayerPlacement::Grid,
        ),
        (Fixture::free_two_by_two(), LayerPlacement::Free),
    ] {
        let stored = spec_of(&fixture);
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
        view.set_layout_mode(true);
        let entry = frame_with(
            &context,
            &mut view,
            &fixture,
            vec![],
            0.1,
            egui::Modifiers::NONE,
            1.0,
        );
        assert!(entry.events.is_empty() && entry.cached, "entry");
        let layout = layout_of(&context, &mut view, &fixture);
        let mut spots = Vec::new();
        if placement == LayerPlacement::Grid {
            for (i, j) in [(1, 1), (1, 0), (0, 1), (2, 1), (1, 2)] {
                spots.push(meeting(&layout, i, j));
            }
        } else {
            for base in 0..4 {
                spots.push(middle(&view, base));
            }
            let line = box_of(&view, 0);
            spots.push(screen(
                &view,
                line.outer.left + line.outer.width,
                line.outer.top + line.outer.height / 2.0,
            ));
        }
        let mut time = 0.2;
        for at in spots {
            time += 0.1;
            let out = hover_at(&context, &mut view, &fixture, at, time);
            assert!(out.events.is_empty() && out.cached, "hover at {at:?}");
        }
        assert_eq!(spec_of(&fixture), stored, "model untouched");
        assert_eq!(view.layout_placement(), Some(placement));
        assert_eq!(spec_of(&fixture).layer_placement, placement);
        view.set_layout_mode(false);
        assert!(view.take_events().is_empty(), "leaving sends nothing");
    }
}

// ---------------------------------------------------------------------------
// Regular Grid
// ---------------------------------------------------------------------------

/// Dragging an internal boundary trades the two columns and sends one event;
/// the figure keeps its size, with a graph title, panel titles and Fixed.
/// Witness: only the left column changed.
#[test]
fn a_boundary_drag_trades_the_two_columns_and_keeps_the_figure_size() {
    let fixture = Fixture::grid(2, 1, false, true, true);
    let (context, mut view) = in_mode(&fixture);
    let before = layout_of(&context, &mut view, &fixture);
    let from = meeting(&before, 1, 0) + egui::vec2(0.0, 60.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(30.0, 0.0),
        egui::Modifiers::NONE,
    );
    let (widths, heights) = grid_edited(&signals);
    assert_eq!(widths, [230.0, 170.0]);
    assert_eq!(heights, [150.0]);
    // The preview is held until the model changes: the figure is the same size.
    let after = layout_of(&context, &mut view, &fixture);
    assert_eq!(after.frame_widths, vec![230.0, 170.0]);
    assert_eq!(after.size, before.size, "the figure keeps its size");
    assert_eq!(spec_of(&fixture).frame_widths, vec![200.0, 200.0], "model");
}

/// With four graphs the middle corner moves the column and the row in one
/// event; a corner on one boundary and the figure edge moves that one; the
/// figure's own corners and outer edges have no handle. A legacy graph starts
/// from the frames it is drawn with. Witness: figure corners included.
#[test]
fn corners_move_every_boundary_they_lie_on() {
    let fixture = Fixture::grid(2, 2, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    assert_eq!(view.layout_grid_corners().len(), 5, "the five meetings");
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        meeting(&layout, 1, 1),
        meeting(&layout, 1, 1) + egui::vec2(20.0, -10.0),
        egui::Modifiers::NONE,
    );
    let (widths, heights) = grid_edited(&signals);
    assert_eq!(
        (widths, heights),
        (&[220.0, 180.0][..], &[140.0, 160.0][..])
    );

    let (context, mut view) = in_mode(&fixture);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        meeting(&layout, 2, 1),
        meeting(&layout, 2, 1) + egui::vec2(-15.0, 10.0),
        egui::Modifiers::NONE,
    );
    let (widths, heights) = grid_edited(&signals);
    assert_eq!(
        (widths, heights),
        (&[200.0, 200.0][..], &[160.0, 140.0][..]),
        "the corner on the figure edge moves the row only"
    );
    let (context, mut view) = in_mode(&fixture);
    let cursor = hover_at(&context, &mut view, &fixture, meeting(&layout, 2, 1), 5.0).cursor;
    assert_eq!(cursor, egui::CursorIcon::ResizeVertical);
    let cursor = hover_at(&context, &mut view, &fixture, meeting(&layout, 1, 1), 5.2).cursor;
    assert_eq!(cursor, egui::CursorIcon::ResizeNwSe);

    // The figure's own corners and outer edges grab nothing.
    let (context, mut view) = in_mode(&fixture);
    let edge_middle = meeting(&layout, 0, 0) + egui::vec2(0.0, 75.0);
    let spots = [
        meeting(&layout, 0, 0),
        meeting(&layout, 2, 0),
        meeting(&layout, 0, 2),
        meeting(&layout, 2, 2),
        edge_middle,
    ];
    for (n, at) in spots.into_iter().enumerate() {
        let (i, j) = (n, 0);
        let signals = drag(
            &context,
            &mut view,
            &fixture,
            at,
            at + egui::vec2(10.0, 10.0),
            egui::Modifiers::NONE,
        );
        assert!(signals.is_empty(), "({i}, {j}): {signals:?}");
    }

    // A legacy graph: the first gesture starts from the frames it is drawn with.
    let legacy = Fixture::grid(2, 2, true, false, false);
    let (context, mut view) = in_mode(&legacy);
    let drawn = layout_of(&context, &mut view, &legacy);
    assert_eq!(drawn.frame_widths.len(), 2, "drawn frames are known");
    let at = meeting(&drawn, 1, 1);
    let signals = drag(
        &context,
        &mut view,
        &legacy,
        at,
        at + egui::vec2(12.0, 0.0),
        egui::Modifiers::NONE,
    );
    let (widths, heights) = grid_edited(&signals);
    assert_eq!(widths[0], drawn.frame_widths[0] + 12.0);
    assert_eq!(heights, drawn.frame_heights.as_slice());
}

/// A compact fallback grid has no frame to edit: no handle, no corner.
/// Witness: the `editable` check dropped.
#[test]
fn a_compact_fallback_grid_has_no_handle() {
    let fixture = Fixture::compact();
    let (context, mut view) = in_mode(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    assert!(layout.frame_widths.is_empty(), "compact fallback expected");
    assert!(view.layout_grid_corners().is_empty());
    let line = layout.column_breaks.get(1).copied().unwrap_or(1) as f32;
    let at = Pos2::new(line, (layout.title_band + layout.row_breaks[1]) as f32);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        at,
        at + egui::vec2(20.0, 20.0),
        egui::Modifiers::NONE,
    );
    assert!(signals.is_empty(), "{signals:?}");
    assert_eq!(
        hover_at(&context, &mut view, &fixture, at, 3.0).cursor,
        egui::CursorIcon::Default
    );
}

/// The L page corners scale frames the layout does not have: on a compact
/// fallback grid they are neither drawn nor grabbable. Witness: the `editable`
/// check dropped from either the painting or the grab.
#[test]
fn a_compact_fallback_grid_has_no_page_corner() {
    let fixture = Fixture::compact();
    let (context, mut view) = in_mode(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    let corner = Pos2::new(layout.size.0 as f32 + 4.0, layout.size.1 as f32 + 4.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner + egui::vec2(30.0, 30.0),
        egui::Modifiers::NONE,
    );
    assert!(signals.is_empty(), "{signals:?}");
    let out = hover_at(&context, &mut view, &fixture, corner, 3.0);
    assert_eq!(out.cursor, egui::CursorIcon::Default);
    assert_eq!(l_corners(&out.shapes), 0, "no corner is drawn");
}

/// The number of rectangles outlined with the 2.5 point accent stroke (the
/// target cell of a graph being moved).
fn target_outlines(shapes: &[egui::epaint::ClippedShape]) -> usize {
    fn walk(shape: &egui::Shape, count: &mut usize) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|shape| walk(shape, count)),
            egui::Shape::Rect(rect) if (rect.stroke.width - 2.5).abs() < 0.01 => *count += 1,
            _ => {}
        }
    }
    let mut count = 0;
    for clipped in shapes {
        walk(&clipped.shape, &mut count);
    }
    count
}

fn moved(signals: &[GraphEvent]) -> (usize, MoveTarget) {
    match signals {
        [GraphEvent::GridCellMoved { panel, target }] => (*panel, *target),
        other => panic!("one GridCellMoved expected, got {other:?}"),
    }
}

/// On a Regular grid a drag that starts inside a graph moves it (#491): nothing
/// is sent or changed before the release, then one event names the cell under
/// the pointer. Witness: the `MoveCell` start removed from `begin_layout_drag`.
#[test]
fn dragging_inside_a_graph_of_a_regular_grid_sends_one_move_on_release() {
    let fixture = Fixture::grid(2, 2, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let stored = spec_of(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    let (from, to) = (middle(&view, 0), middle(&view, 3));
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        to,
        egui::Modifiers::NONE,
    );
    assert_eq!(moved(&signals), (0, MoveTarget::Cell { row: 1, column: 1 }));
    assert_eq!(spec_of(&fixture), stored, "the model is for the shell");
    let after = layout_of(&context, &mut view, &fixture);
    assert_eq!(after.frame_widths, layout.frame_widths, "nothing previewed");
}

/// A drag that starts on a boundary keeps its meaning and priority (#491).
/// Witness: the cell check moved before the handle check.
#[test]
fn a_drag_from_a_boundary_still_resizes_and_never_moves() {
    let fixture = Fixture::grid(2, 2, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    let from = meeting(&layout, 1, 0) + egui::vec2(0.0, 60.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from + egui::vec2(30.0, 0.0),
        egui::Modifiers::NONE,
    );
    let _ = grid_edited(&signals);
}

/// Released on its own cell, outside the figure, or without a drag: nothing
/// is sent, and a plain click only selects (#491). Witness: `move_target`
/// returns the source cell.
#[test]
fn releasing_on_the_same_cell_or_outside_sends_nothing() {
    let fixture = Fixture::grid(2, 2, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let start = middle(&view, 0);
    let same = drag(
        &context,
        &mut view,
        &fixture,
        start,
        start + egui::vec2(12.0, 9.0),
        egui::Modifiers::NONE,
    );
    assert!(same.is_empty(), "{same:?}");
    let outside = drag(
        &context,
        &mut view,
        &fixture,
        start,
        Pos2::new(-60.0, -60.0),
        egui::Modifiers::NONE,
    );
    assert!(outside.is_empty(), "{outside:?}");
    let target = middle(&view, 3);
    click(
        &context,
        &mut view,
        &fixture,
        target,
        egui::Modifiers::NONE,
        5.0,
    );
    assert!(view.take_events().is_empty());
}

/// While the graph is dragged over another cell, that cell is outlined; over
/// its own cell, nothing is (#491). Witness: the outline never painted.
#[test]
fn the_target_cell_is_outlined_while_a_graph_is_dragged() {
    let fixture = Fixture::grid(2, 2, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let start = middle(&view, 0);
    let none = egui::Modifiers::NONE;
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(start), button(start, true, none)],
        0.1,
        none,
    );
    let mid_drag = |view: &mut GraphView, to: Pos2, time: f64| {
        let step = start + (to - start) * 0.5;
        frame_with(
            &context,
            view,
            &fixture,
            vec![egui::Event::PointerMoved(step)],
            time,
            none,
            1.0,
        );
        frame_with(
            &context,
            view,
            &fixture,
            vec![egui::Event::PointerMoved(to)],
            time + 0.05,
            none,
            1.0,
        )
    };
    let own = mid_drag(&mut view, start + egui::vec2(14.0, 10.0), 0.2);
    let over_three = middle(&view, 3);
    let other = mid_drag(&mut view, over_three, 0.4);
    assert!(other.events.is_empty() && own.events.is_empty());
    assert_eq!(
        target_outlines(&other.shapes),
        target_outlines(&own.shapes) + 1,
        "one more outline over another cell"
    );
}

/// A drop in the bottom edge band makes a new row beside the cell nearest the
/// pointer (#491): [A B] with B dragged under A. Witness: the edge band test
/// removed from `move_target`.
#[test]
fn dropping_in_the_bottom_edge_band_asks_for_a_new_row_beside_the_nearest_cell() {
    let fixture = Fixture::grid(2, 1, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    let under_a = Pos2::new(60.0, layout.size.1 as f32 - 10.0);
    let (b, a) = (middle(&view, 1), middle(&view, 0));
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        b,
        under_a,
        egui::Modifiers::NONE,
    );
    assert_eq!(
        moved(&signals),
        (
            1,
            MoveTarget::Beside {
                panel: 0,
                side: CellSide::Below
            }
        )
    );
    // The right band gives a new column beside the nearest cell.
    let beside_b = Pos2::new(layout.size.0 as f32 - 10.0, 60.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        a,
        beside_b,
        egui::Modifiers::NONE,
    );
    assert_eq!(
        moved(&signals),
        (
            0,
            MoveTarget::Beside {
                panel: 1,
                side: CellSide::Right
            }
        )
    );
}

/// The left and top bands give a new column on the left and a new row above,
/// beside the nearest cell (#503). Witness: the `Left` and `Above` entries
/// removed from `edge_drop_side`.
#[test]
fn dropping_in_the_left_and_top_edge_bands_asks_for_a_cell_on_that_side() {
    let fixture = Fixture::grid(2, 1, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let (b, a) = (middle(&view, 1), middle(&view, 0));
    let left_of_a = Pos2::new(10.0, 80.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        b,
        left_of_a,
        egui::Modifiers::NONE,
    );
    assert_eq!(
        moved(&signals),
        (
            1,
            MoveTarget::Beside {
                panel: 0,
                side: CellSide::Left
            }
        )
    );
    let above_b = Pos2::new(a.x + 150.0, 8.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        a,
        above_b,
        egui::Modifiers::NONE,
    );
    assert_eq!(
        moved(&signals),
        (
            0,
            MoveTarget::Beside {
                panel: 1,
                side: CellSide::Above
            }
        )
    );
}

/// An arrow key moves the selected graph one cell, a hole included; at the
/// edge of the grid it does nothing; Shift still counts one cell (#491).
/// Witness: the guard of `nudge` restored.
#[test]
fn an_arrow_key_moves_the_selected_graph_one_cell_on_a_regular_grid() {
    let fixture = Fixture::grid(2, 2, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let mut time = 1.0;
    let mut press = |view: &mut GraphView, steps: (i32, i32)| {
        view.nudge(steps, time);
        time += 1.0;
        frame(
            &context,
            view,
            &fixture,
            vec![],
            time,
            egui::Modifiers::NONE,
        )
    };
    assert_eq!(
        moved(&press(&mut view, (1, 0))),
        (0, MoveTarget::Cell { row: 0, column: 1 })
    );
    assert_eq!(
        moved(&press(&mut view, (0, 1))),
        (0, MoveTarget::Cell { row: 1, column: 0 })
    );
    assert_eq!(
        moved(&press(&mut view, (0, 10))),
        (0, MoveTarget::Cell { row: 1, column: 0 }),
        "Shift counts one cell"
    );
    assert!(press(&mut view, (-1, 0)).is_empty(), "left edge");
    assert!(press(&mut view, (0, -1)).is_empty(), "top edge");
    // A pending event is never kept as a preview.
    assert!(!view.layout_dragging());
}

/// A grid with no frames to edit (compact fallback) cannot move a graph, by
/// drag or by key (#491). Witness: the `editable` test removed.
#[test]
fn a_compact_fallback_grid_cannot_move_a_graph() {
    let fixture = Fixture::compact();
    let (context, mut view) = in_mode(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    let centre = |row: usize, column: usize| {
        let (x, y, w, h) = layout.cell_rect(row, column).expect("cell");
        Pos2::new(x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0)
    };
    let last = layout.column_breaks.len() - 2;
    let (inside, other) = (centre(0, 0), centre(0, last));
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        inside,
        other,
        egui::Modifiers::NONE,
    );
    assert!(signals.is_empty(), "{signals:?}");
    view.nudge((1, 0), 9.0);
    let after = frame(
        &context,
        &mut view,
        &fixture,
        vec![],
        10.0,
        egui::Modifiers::NONE,
    );
    assert!(after.is_empty(), "{after:?}");
}

/// The number of L page corner polylines (stroke width 2.5) painted.
fn l_corners(shapes: &[egui::epaint::ClippedShape]) -> usize {
    fn walk(shape: &egui::Shape, count: &mut usize) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|shape| walk(shape, count)),
            egui::Shape::LineSegment { .. } => {}
            egui::Shape::Path(path) if (path.stroke.width - 2.5).abs() < 0.01 => *count += 1,
            _ => {}
        }
    }
    let mut count = 0;
    for clipped in shapes {
        walk(&clipped.shape, &mut count);
    }
    count
}

/// The pills sit on internal boundaries only and the squares on the corners
/// that lie on one: a selected graph of a 2 x 2 shows 2 pills and 3 squares,
/// a one-cell figure none. Witness: outer edges or figure corners included.
#[test]
fn regular_handles_are_drawn_on_internal_boundaries_only() {
    fn marks(shapes: &[egui::epaint::ClippedShape]) -> (usize, usize) {
        fn walk(shape: &egui::Shape, counts: &mut (usize, usize)) {
            match shape {
                egui::Shape::Vec(list) => list.iter().for_each(|shape| walk(shape, counts)),
                egui::Shape::Rect(rect) => {
                    let size = rect.rect.size();
                    if (size.x - 7.0).abs() < 0.01 && (size.y - 7.0).abs() < 0.01 {
                        counts.0 += 1;
                    } else if ((size.x - 4.0).abs() < 0.01 && (size.y - 18.0).abs() < 0.01)
                        || ((size.x - 18.0).abs() < 0.01 && (size.y - 4.0).abs() < 0.01)
                    {
                        counts.1 += 1;
                    }
                }
                _ => {}
            }
        }
        let mut counts = (0, 0);
        for clipped in shapes {
            walk(&clipped.shape, &mut counts);
        }
        counts
    }
    let fixture = Fixture::grid(2, 2, false, false, false);
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    view.set_active_panel(0, &spec);
    let out = hover_at(
        &context,
        &mut view,
        &fixture,
        Pos2::new(-100.0, -100.0),
        1.0,
    );
    assert_eq!(
        marks(&out.shapes),
        (3, 2),
        "squares, pills of the top-left graph"
    );
    assert_eq!(l_corners(&out.shapes), 4, "the L page corners");
    let single = Fixture::grid(1, 1, false, false, false);
    let (context, mut view) = in_mode(&single);
    let out = hover_at(&context, &mut view, &single, Pos2::new(-100.0, -100.0), 1.0);
    assert_eq!(marks(&out.shapes), (0, 0));
    assert_eq!(l_corners(&out.shapes), 4, "even on a one-cell figure");
}

/// An L corner of a Regular figure scales every column and row by one factor
/// per axis, whichever corner; one event flagged as a figure resize, the
/// figure bigger; nothing without a drag. Witness: only the last column
/// scaled.
#[test]
fn an_l_corner_scales_the_whole_regular_figure() {
    let fixture = Fixture::grid(2, 2, false, true, false);
    let (context, mut view) = in_mode(&fixture);
    let before = layout_of(&context, &mut view, &fixture);
    let (w, h) = (before.size.0 as f32, before.size.1 as f32);
    let corner = Pos2::new(w + 4.0, h + 4.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner + egui::vec2(40.0, 30.0),
        egui::Modifiers::NONE,
    );
    let [GraphEvent::GridFramesEdited {
        widths,
        heights,
        figure: true,
    }] = signals.as_slice()
    else {
        panic!("one figure event expected, got {signals:?}");
    };
    assert_eq!(widths, &vec![220.0, 220.0]);
    assert_eq!(heights, &vec![165.0, 165.0]);
    let after = layout_of(&context, &mut view, &fixture);
    assert_eq!(
        after.size,
        (before.size.0 + 40, before.size.1 + 30),
        "bands keep their size"
    );
    let (context, mut view) = in_mode(&fixture);
    assert_eq!(
        hover_at(
            &context,
            &mut view,
            &fixture,
            Pos2::new(w + 4.0, h + 4.0),
            9.0
        )
        .cursor,
        egui::CursorIcon::ResizeNwSe
    );

    // A corner on the left or top scales too.
    let (context, mut view) = in_mode(&fixture);
    // The figure's top is under the graph title, as the page of a free figure.
    let top_left = Pos2::new(-4.0, before.title_band as f32 - 4.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        top_left,
        top_left - egui::vec2(40.0, 30.0),
        egui::Modifiers::NONE,
    );
    let [GraphEvent::GridFramesEdited {
        widths,
        heights,
        figure: true,
    }] = signals.as_slice()
    else {
        panic!("one figure event expected, got {signals:?}");
    };
    assert_eq!(
        (widths, heights),
        (&vec![220.0, 220.0], &vec![165.0, 165.0])
    );

    // No movement, no event.
    let (context, mut view) = in_mode(&fixture);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner,
        egui::Modifiers::NONE,
    );
    assert!(signals.is_empty(), "{signals:?}");
}

/// With the ratio lock closed, an L corner of a Regular figure keeps the image
/// ratio whichever axis the pointer moved most. Witness: the lock not fed to
/// the view (the other axis stays as dragged).
#[test]
fn a_locked_l_corner_keeps_the_ratio_of_the_regular_image() {
    let fixture = Fixture::grid(2, 2, false, true, false);
    let (context, mut view) = in_mode(&fixture);
    let before = layout_of(&context, &mut view, &fixture);
    let (w, h) = (before.size.0 as f32, before.size.1 as f32);
    let ratio = f64::from(h) / f64::from(w);
    view.set_figure_ratio_lock(Some(ratio));
    let corner = Pos2::new(w + 4.0, h + 4.0);
    // The pointer mostly goes right: the height follows.
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner + egui::vec2(60.0, 2.0),
        egui::Modifiers::NONE,
    );
    assert!(
        matches!(
            signals.as_slice(),
            [GraphEvent::GridFramesEdited { figure: true, .. }]
        ),
        "{signals:?}"
    );
    let after = layout_of(&context, &mut view, &fixture);
    let kept = f64::from(after.size.1) / f64::from(after.size.0);
    assert!((kept - ratio).abs() < 0.004, "{kept} vs {ratio}");
    assert!(after.size.0 >= before.size.0 + 59, "{:?}", after.size);
}

/// Same for the page corner of a Free figure.
#[test]
fn a_locked_page_corner_keeps_the_ratio_of_the_free_page() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let page = view.layout_page().expect("page");
    view.set_figure_ratio_lock(Some(page.height / page.width));
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
        corner + egui::vec2(100.0, 1.0),
        SHIFT,
    );
    let (_, new_page) = edited(&signals);
    let new_page = new_page.expect("a page");
    assert!(
        (new_page.height / new_page.width - page.height / page.width).abs() < 1e-3,
        "{new_page:?} from {page:?}"
    );
    assert!(new_page.width > page.width + 90.0);
}

/// Pulled far inward, the columns and rows stop together at the minimum with
/// their proportions. Witness: a per-frame clamp.
#[test]
fn an_l_corner_pulled_far_inward_stops_every_frame_together() {
    let fixture = Fixture::grid(2, 1, false, false, false);
    let mut spec = spec_of(&fixture);
    spec.frame_widths = vec![100.0, 300.0];
    fixture.replace(spec);
    let (context, mut view) = in_mode(&fixture);
    let layout = layout_of(&context, &mut view, &fixture);
    let corner = Pos2::new(layout.size.0 as f32 + 4.0, layout.size.1 as f32 + 4.0);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        corner,
        corner - egui::vec2(380.0, 100.0),
        egui::Modifiers::NONE,
    );
    let [GraphEvent::GridFramesEdited {
        widths,
        figure: true,
        ..
    }] = signals.as_slice()
    else {
        panic!("one figure event expected, got {signals:?}");
    };
    assert_eq!(widths, &vec![36.0, 108.0], "3:1 kept at the stop");
}

// ---------------------------------------------------------------------------
// Free placement
// ---------------------------------------------------------------------------

/// Dragging the inside of a box moves it: one event at the release with the
/// moved frame, the same size, and the page left exactly as it was.
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
    assert_eq!(
        (frame.top, frame.width, frame.height),
        (before.frame.top, before.frame.width, before.frame.height)
    );
}

/// The selected box's handles win over a box in front of it, and pulling a
/// corner keeps the opposite one and the page.
#[test]
fn a_corner_handle_of_the_selected_box_resizes_it() {
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
    let [(1, frame)] = frames else {
        panic!("one frame expected");
    };
    assert!(
        (before.frame.width - frame.width - 30.0).abs() < 0.02,
        "{frame:?}"
    );
    assert!(
        (before.frame.height - frame.height - 20.0).abs() < 0.02,
        "{frame:?}"
    );
    assert_eq!(
        (frame.left, frame.top),
        (before.frame.left, before.frame.top)
    );
}

/// Pulling a page corner scales the page and every box with it.
#[test]
fn a_page_corner_drag_scales_the_page_and_the_boxes() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    let page = view.layout_page().expect("page");
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
    assert_eq!(frames.len(), 2, "every box follows");
}

/// Centre band: only the two graphs on the line move, in a 2 x 2. Side band:
/// only the graph on the pointer's side. Beyond the bands, the box moves.
/// Witness: the whole-line group restored.
#[test]
fn in_a_two_by_two_the_centre_band_moves_two_graphs_and_a_side_band_one() {
    let fixture = Fixture::free_two_by_two();
    // Vertical line between bases 0 and 1: pointer d points across it.
    let at = |view: &GraphView, d: f64| {
        let low = box_of(view, 0);
        screen(
            view,
            low.outer.left + low.outer.width + d,
            low.outer.top + low.outer.height / 2.0,
        )
    };
    for (d, bases) in [
        (0.0, vec![0, 1]),
        (2.9, vec![0, 1]),
        (-2.9, vec![0, 1]),
        (-3.1, vec![0]),
        (-22.9, vec![0]),
        (3.1, vec![1]),
        (22.9, vec![1]),
    ] {
        let (context, mut view) = in_mode(&fixture);
        let from = at(&view, d);
        let signals = drag(
            &context,
            &mut view,
            &fixture,
            from,
            from + egui::vec2(10.0, 0.0),
            SHIFT,
        );
        assert!(!signals.is_empty(), "no event for d {d}");
        let (frames, _) = edited(&signals);
        let mut got: Vec<usize> = frames.iter().map(|(b, _)| *b).collect();
        got.sort_unstable();
        assert_eq!(got, bases, "d {d}");
    }
    let (context, mut view) = in_mode(&fixture);
    let from = at(&view, 23.5);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        from,
        from - egui::vec2(10.0, 0.0),
        SHIFT,
    );
    let (frames, _) = edited(&signals);
    assert_eq!(
        frames.len(),
        1,
        "beyond the band: the box under the pointer moves"
    );
}

/// The cursors: single arrows pointing out of the graph on a side band,
/// double arrows on the centre band, the box cursor beyond. Witness: the old
/// 2 and 10 point constants.
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
        assert_eq!(cursor(&mut view, -2.9), double);
        assert_eq!(cursor(&mut view, 2.9), double);
        assert_eq!(cursor(&mut view, -3.2), low_side);
        assert_eq!(cursor(&mut view, 3.2), high_side);
        assert_eq!(cursor(&mut view, -22.5), low_side);
        assert_eq!(cursor(&mut view, 22.5), high_side);
        assert_eq!(cursor(&mut view, -23.5), Move);
        assert_eq!(cursor(&mut view, 23.5), Move);
    }
}

/// A click in a band, side or centre, selects the graph on the pointer's side.
#[test]
fn a_click_on_a_line_selects_the_pointer_side() {
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    let spec = spec_of(&fixture);
    let mut time = 1.0;
    for (d, wanted) in [(-6.0, 1), (6.0, 0), (-2.0, 1), (2.0, 0)] {
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
    let mut signals = Vec::new();
    for time in [1.3, 1.6] {
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
    for time in [2.0, 2.1] {
        signals.extend(frame(
            &context,
            &mut view,
            &fixture,
            vec![],
            time,
            egui::Modifiers::NONE,
        ));
    }
    let (frames, _) = edited(&signals);
    let step = 5.0 * 72.0 / 25.4;
    let [(1, frame)] = frames else {
        panic!("one frame expected: {frames:?}");
    };
    assert!(
        (frame.left - before.frame.left - 2.0 * step).abs() < 0.02,
        "{frame:?}"
    );
}

/// A pending nudge is committed by `commit_layout_edit` (the shell calls it
/// before a placement change) and by leaving the mode. Witness: the commit
/// removed: the burst is lost when the placement changes.
#[test]
fn a_pending_nudge_is_committed_before_a_placement_change_and_on_leaving() {
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
    view.commit_layout_edit();
    let signals = view.take_events();
    assert!(
        matches!(signals.as_slice(), [GraphEvent::LayoutEdited { .. }]),
        "{signals:?}"
    );

    let (context, mut view) = in_mode(&fixture);
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
    assert!(
        matches!(signals.as_slice(), [GraphEvent::LayoutEdited { .. }]),
        "{signals:?}"
    );
}

/// A placement change during a drag (an Undo, a toggle) drops the gesture:
/// nothing is sent at the release and the view reads the new placement from
/// the figure. Witness: the reconcile removed.
#[test]
fn a_placement_change_drops_the_gesture_in_progress() {
    let fixture = Fixture::free_two_layers();
    let (context, mut view) = in_mode(&fixture);
    assert_eq!(view.layout_placement(), Some(LayerPlacement::Free));
    let from = middle(&view, 1);
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from), button(from, true, SHIFT)],
        0.1,
        SHIFT,
    );
    frame(
        &context,
        &mut view,
        &fixture,
        vec![egui::Event::PointerMoved(from + egui::vec2(10.0, 0.0))],
        0.2,
        SHIFT,
    );
    let mut grid = spec_of(&fixture);
    grid.layer_placement = LayerPlacement::Grid;
    fixture.replace(grid);
    let mut signals = Vec::new();
    for (time, dx) in [(0.3, 20.0), (0.4, 30.0)] {
        signals.extend(frame(
            &context,
            &mut view,
            &fixture,
            vec![egui::Event::PointerMoved(from + egui::vec2(dx, 0.0))],
            time,
            SHIFT,
        ));
    }
    signals.extend(frame(
        &context,
        &mut view,
        &fixture,
        vec![button(from + egui::vec2(30.0, 0.0), false, SHIFT)],
        0.5,
        SHIFT,
    ));
    assert!(signals.is_empty(), "{signals:?}");
    assert_eq!(view.layout_placement(), Some(LayerPlacement::Grid));
}

/// Handles: solid on the selected graph, faint under the pointer, none on the
/// others; none faint during a drag.
fn handles(shapes: &[egui::epaint::ClippedShape]) -> (usize, usize) {
    fn walk(shape: &egui::Shape, counts: &mut (usize, usize)) {
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
            _ => {}
        }
    }
    let mut counts = (0, 0);
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
    let quiet = hover_at(
        &context,
        &mut view,
        &fixture,
        Pos2::new(-100.0, -100.0),
        1.0,
    );
    assert_eq!(handles(&quiet.shapes), (4, 0), "the selected graph only");
    let over = middle(&view, 1);
    assert_eq!(
        handles(&hover_at(&context, &mut view, &fixture, over, 1.2).shapes),
        (4, 4)
    );
    let own = middle(&view, 0);
    assert_eq!(
        handles(&hover_at(&context, &mut view, &fixture, own, 1.4).shapes),
        (4, 0)
    );
    let band = across(&view, false, 0.5);
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
        assert_eq!(handles(&out.shapes), (4, 0), "step {step}");
    }
}

/// Hovering over the lines never touches the cache, the model or the events.
#[test]
fn hovering_leaves_the_cache_and_the_model_alone_in_free() {
    let fixture = Fixture::touching(false);
    let (context, mut view) = in_mode(&fixture);
    let stored = spec_of(&fixture);
    let generation = view.view().generation();
    let mut time = 1.0;
    for d in [-30.0, -6.0, 0.0, 6.0, 30.0] {
        time += 0.1;
        let at = across(&view, false, d);
        let out = hover(&context, &mut view, &fixture, at, time, 1.0);
        assert!(out.cached && out.events.is_empty(), "d {d}");
    }
    assert_eq!(view.view().generation(), generation);
    assert_eq!(spec_of(&fixture), stored);
}

/// The magnets' guides of a box drag are drawn while the pointer is down and
/// gone at the release; Shift (no magnets) shows none. Witness: reading the
/// guides from anything but the mode's own drag.
#[test]
fn the_guides_of_a_box_drag_show_while_the_pointer_is_down() {
    let guides_during = |modifiers: egui::Modifiers| {
        let fixture = Fixture::free_two_layers();
        let (context, mut view) = in_mode(&fixture);
        // Box 1 sits left of box 0: pulling box 0 left until its frame is 3 pt
        // from the right edge of box 1's frame meets the magnet.
        let from = middle(&view, 0);
        let gap = box_of(&view, 0).frame.left
            - (box_of(&view, 1).frame.left + box_of(&view, 1).frame.width);
        frame(
            &context,
            &mut view,
            &fixture,
            vec![
                egui::Event::PointerMoved(from),
                button(from, true, modifiers),
            ],
            0.1,
            modifiers,
        );
        let to = from + egui::vec2(-(gap as f32) + 3.0, 0.0);
        let mut shown = 0;
        for (step, time) in [(0.5, 0.15), (1.0, 0.2)] {
            let pos = from + (to - from) * step;
            frame(
                &context,
                &mut view,
                &fixture,
                vec![egui::Event::PointerMoved(pos)],
                time,
                modifiers,
            );
            shown = view.magnet_guides().len();
        }
        let signals = frame(
            &context,
            &mut view,
            &fixture,
            vec![button(to, false, modifiers)],
            0.3,
            modifiers,
        );
        assert_eq!(signals.len(), 1, "{signals:?}");
        assert!(
            view.magnet_guides().is_empty(),
            "guides end with the gesture"
        );
        shown
    };
    assert!(guides_during(egui::Modifiers::NONE) > 0, "a guide shows");
    assert_eq!(guides_during(SHIFT), 0, "Shift: no magnets, no guide");
}

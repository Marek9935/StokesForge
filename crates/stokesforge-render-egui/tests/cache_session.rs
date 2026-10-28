//! Task R4.3 verified **end to end**: from the session to the painting.
//!
//! `src/cache.rs` tests the cache alone, on a fabricated key. This file
//! tests it plugged into what really drives it: a
//! `stokesforge_session::Snapshot`, an `egui::Context`, and a bus command
//! that advances the `revision`. It is the only way to verify the claim that
//! matters — *"a frame without change does not replay the Plotters
//! pipeline"* — rather than a neighboring claim.
//!
//! No graphics server: `egui::Context::run_ui` produces shapes without
//! painting anything, and that is all we need.

use egui::{Pos2, Rect};
use stokesforge_commands::Command;
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render_egui::{font_definitions, GraphView};
use stokesforge_session::{Session, SessionHandle, Snapshot};

struct Fixture {
    session: Session,
    graph: NodeId,
    column: NodeId,
}

impl Fixture {
    fn new() -> Self {
        Self::with_title("Cache")
    }

    fn with_title(title: &str) -> Self {
        let mut project = Project::new("Cache").expect("project");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("workbook");
        let sheet = project.add_worksheet(book, None).expect("sheet");
        let xs: Vec<f64> = (0..32).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| x * 1.5).collect();
        let x = project
            .add_column(sheet, None, Some("X"), numeric(&xs))
            .expect("X");
        let column = project
            .add_column(sheet, None, Some("Y"), numeric(&ys))
            .expect("Y");

        let mut panel = PanelSpec::at(0, 0);
        let mut series = SeriesSpec::new_xy(x, column);
        series.line = LineSpec::from_palette(0);
        panel.series.push(series);
        let spec = PlotSpec {
            title: title.into(),
            ..PlotSpec::empty("")
        }
        .with_panels(panel);
        let graph = project.add_graph(root, None, spec).expect("graph");

        let session = Session::spawn(project, None).expect("session");
        Self {
            session,
            graph,
            column,
        }
    }

    fn handle(&self) -> SessionHandle {
        self.session.handle()
    }

    fn snapshot(&self) -> Snapshot {
        self.handle().snapshot().expect("snapshot")
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

/// Paints a frame and returns `true` if it came from the cache.
///
/// The `Context` is the one reused from frame to frame: this is what makes
/// the experiment honest, since a fresh `Context` on each call would have
/// neither the same atlas nor necessarily the same `pixels_per_point`.
fn paint_frame(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
    rect: Rect,
) -> bool {
    let mut from_cache = false;
    let output = context.run_ui(egui::RawInput::default(), |ui| {
        from_cache = view.paint(ui, rect, snapshot, graph).expect("painting");
    });
    consume(output);
    from_cache
}

/// Without a window nobody uploads the glyph atlas — and `epaint`
/// **panics** if an unapplied `TexturesDelta` is dropped (`textures.rs`:
/// *"Deltas need to be handled"*). It is a useful precaution in real use,
/// and a pitfall to know about when testing rendering without a graphics
/// server: the texture must be declared consumed.
fn consume(mut output: egui::FullOutput) {
    output.textures_delta.clear();
}

fn new_context() -> egui::Context {
    let context = egui::Context::default();
    // Like `stokesforge-app`: the single font source is set explicitly, not
    // inherited from egui's default.
    context.set_fonts(font_definitions());
    context
}

#[test]
fn a_frame_without_change_does_not_replay_the_pipeline() {
    let fixture = Fixture::new();
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(600.0, 400.0));
    let snapshot = fixture.snapshot();

    assert!(
        !paint_frame(&context, &mut view, &snapshot, fixture.graph, rect),
        "the first frame must build"
    );
    for _ in 0..5 {
        assert!(
            paint_frame(&context, &mut view, &snapshot, fixture.graph, rect),
            "a following frame, with identical snapshot and rectangle, must \
             come from the cache"
        );
    }
    assert_eq!(
        view.cache().misses(),
        1,
        "a single computation for six frames"
    );
    assert_eq!(view.cache().hits(), 5);
    assert!(!view.cache().shapes().is_empty());
    assert_eq!(
        view.last_drawn().map(|drawn| drawn.drawn_series),
        Some(1),
        "the series should have been drawn"
    );
}

#[test]
fn a_command_on_a_referenced_column_invalidates_the_cache() {
    // This is the heart of the protocol: the renderer is one more client of
    // the session's `revision`, just like the UI and the script thread
    // (`architecture-choisie.md` §4.2).
    let fixture = Fixture::new();
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));

    let before = fixture.snapshot();
    paint_frame(&context, &mut view, &before, fixture.graph, rect);
    assert!(paint_frame(
        &context,
        &mut view,
        &before,
        fixture.graph,
        rect
    ));

    fixture
        .handle()
        .execute(Command::SetCells {
            column: fixture.column,
            start: 3,
            values: ValueBlock::Float(vec![999.0]),
            expected_version: None,
        })
        .expect("command accepted");

    let after = fixture.snapshot();
    assert_ne!(
        after.revision, before.revision,
        "the revision must have advanced"
    );
    assert!(
        !paint_frame(&context, &mut view, &after, fixture.graph, rect),
        "a new snapshot must make the pipeline replay"
    );
    assert_eq!(view.cache().misses(), 2);
}

#[test]
fn resizing_invalidates_the_cache() {
    let fixture = Fixture::new();
    let context = new_context();
    let mut view = GraphView::new();
    let snapshot = fixture.snapshot();

    let petit = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
    let grand = Rect::from_min_size(Pos2::ZERO, egui::vec2(401.0, 300.0));
    paint_frame(&context, &mut view, &snapshot, fixture.graph, petit);
    assert!(paint_frame(
        &context,
        &mut view,
        &snapshot,
        fixture.graph,
        petit
    ));
    assert!(
        !paint_frame(&context, &mut view, &snapshot, fixture.graph, grand),
        "one more pixel of width changes the geometry of the figure"
    );
}

#[test]
fn a_node_that_is_not_a_graph_is_an_error_not_a_crash() {
    let fixture = Fixture::new();
    let context = new_context();
    let mut view = GraphView::new();
    let snapshot = fixture.snapshot();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));

    let mut result = None;
    consume(context.run_ui(egui::RawInput::default(), |ui| {
        result = Some(view.paint(ui, rect, &snapshot, fixture.column));
    }));
    assert!(matches!(
        result,
        Some(Err(stokesforge_render_egui::RenderError::NotAGraph(_)))
    ));

    let mut unknown = None;
    consume(context.run_ui(egui::RawInput::default(), |ui| {
        unknown = Some(view.paint(ui, rect, &snapshot, NodeId::from_raw(u64::MAX)));
    }));
    assert!(matches!(
        unknown,
        Some(Err(stokesforge_render_egui::RenderError::UnknownNode(_)))
    ));
}

/// Number of textured meshes among the cached shapes — the form a Typst
/// formula takes on screen (`typst_screen::mesh`), where an ordinary text is
/// a `Shape::Text`.
fn mesh_count(view: &GraphView) -> usize {
    view.cache()
        .shapes()
        .iter()
        .filter(|shape| matches!(shape, egui::Shape::Mesh(_)))
        .count()
}

/// The texts left as `Shape::Text` (laid out by `epaint`).
fn epaint_texts(view: &GraphView) -> Vec<String> {
    view.cache()
        .shapes()
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

/// Issues #29 and #32: `GraphView` — hence the application — sends titles
/// and legend through Typst, **with or without `$`**, and ticks never.
///
/// The two halves hold each other up: a mesh coming from elsewhere (a
/// series, a marker) would make "there are meshes" pass without Typst having
/// been consulted, but not "the title is no longer `epaint` text"; and a
/// backend that sent everything through Typst would not leave the ticks as
/// `Shape::Text`.
#[test]
fn graph_view_sends_titles_and_legend_through_typst() {
    let context = new_context();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));

    for title in ["Absorbance", "$lambda_\"max\"$ (nm)"] {
        let fixture = Fixture::with_title(title);
        let mut vue = GraphView::new();
        paint_frame(&context, &mut vue, &fixture.snapshot(), fixture.graph, rect);
        let texts = epaint_texts(&vue);
        assert!(
            !texts.iter().any(|text| text == title),
            "the title {title:?} stayed epaint text"
        );
        assert!(mesh_count(&vue) > 0, "no Typst mesh for {title:?}");
        assert!(vue.formula_count() > 0);
        assert!(!texts.is_empty(), "no epaint tick left");
        for text in &texts {
            assert!(
                text.parse::<f64>().is_ok(),
                "{text:?} is not a tick and did not go through Typst"
            );
        }
    }
}

/// Issue #29: a rebuild of the figure (new revision) does not recompile
/// unchanged texts, and the measurement of a text shares the entry of its
/// drawing — one entry per text, not one per call to `estimate_text_size`.
#[test]
fn rebuilding_the_figure_does_not_recompile_an_unchanged_title() {
    let fixture = Fixture::with_title("$Delta H$");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));

    paint_frame(
        &context,
        &mut view,
        &fixture.snapshot(),
        fixture.graph,
        rect,
    );
    let (formulas, meshes_before) = (view.formula_count(), mesh_count(&view));
    assert!(formulas > 0);

    fixture
        .handle()
        .execute(Command::SetCells {
            column: fixture.column,
            start: 3,
            values: ValueBlock::Float(vec![999.0]),
            expected_version: None,
        })
        .expect("command accepted");
    assert!(!paint_frame(
        &context,
        &mut view,
        &fixture.snapshot(),
        fixture.graph,
        rect
    ));
    assert_eq!(mesh_count(&view), meshes_before);
    assert_eq!(
        view.formula_count(),
        formulas,
        "unchanged texts come from the cache"
    );
}

/// The size in pixels of the texture of the first mesh of `shapes`.
fn mesh_texture<'a>(
    context: &egui::Context,
    shapes: impl IntoIterator<Item = &'a egui::Shape>,
) -> (egui::TextureId, [usize; 2]) {
    let id = shapes
        .into_iter()
        .find_map(|shape| match shape {
            egui::Shape::Mesh(mesh) => Some(mesh.texture_id),
            _ => None,
        })
        .expect("no mesh");
    let size = context
        .tex_manager()
        .read()
        .meta(id)
        .expect("unknown texture")
        .size;
    (id, size)
}

/// Issue #31, end to end: a graph displayed twice as large as its logical
/// layout (`paint_scaled`, like the application) paints its Typst title with
/// a texture twice as fine as the cache's, instead of stretching it. The
/// witness is the same view at its logical size: it paints the cache texture
/// as is.
#[test]
fn an_enlarged_graph_paints_its_formulas_at_the_displayed_size() {
    let fixture = Fixture::with_title("$Delta H$");
    let context = new_context();
    let mut view = GraphView::new();
    let (width, height) = stokesforge_render_egui::DEFAULT_FIGURE_SIZE;
    let logical = egui::vec2(width as f32, height as f32);

    let paint = |view: &mut GraphView, size: egui::Vec2| {
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            view.paint_scaled(
                ui,
                Rect::from_min_size(Pos2::ZERO, size),
                &fixture.snapshot(),
                fixture.graph,
            )
            .expect("painting");
        });
        let painted: Vec<egui::Shape> = output.shapes.drain(..).map(|c| c.shape).collect();
        output.textures_delta.clear();
        mesh_texture(&context, &painted)
    };

    let (witness, _) = paint(&mut view, logical);
    let formulas = view.formula_count();
    let (cached, [w, h]) = mesh_texture(&context, view.cache().shapes());
    assert_eq!(witness, cached, "at logical size, the cache texture");

    let (enlarged, [w2, h2]) = paint(&mut view, logical * 2.0);
    assert_ne!(enlarged, cached, "the cache texture is stretched");
    assert!(
        w2.abs_diff(2 * w) <= 1 && h2.abs_diff(2 * h) <= 1,
        "{:?} is not twice {:?}",
        [w2, h2],
        [w, h]
    );
    assert_eq!(view.formula_count(), formulas, "a text was recompiled");
}

// -- Issue #35: dragging and double-clicking a free text --

/// A fixture whose panel carries a free text in the middle of the data.
fn fixture_with_text() -> Fixture {
    let fixture = Fixture::with_title("Text");
    let snapshot = fixture.snapshot();
    let mut spec = match snapshot.tree.node(fixture.graph).expect("graph").kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("not a graph"),
    };
    if let Some(panel) = spec.panels.get_mut(0) {
        panel
            .annotations
            .push(stokesforge_plot::AnnotationSpec::new("Peak", 16.0, 24.0));
    }
    fixture
        .handle()
        .execute(Command::SetPlotSpec {
            graph: fixture.graph,
            spec,
        })
        .expect("spec set");
    fixture
}

/// A frame with the given events, at instant `time` (seconds).
fn frame_with(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    rect: Rect,
    events: Vec<egui::Event>,
    time: f64,
) -> Vec<stokesforge_render_egui::GraphEvent> {
    let input = egui::RawInput {
        events,
        time: Some(time),
        ..Default::default()
    };
    let output = context.run_ui(input, |ui| {
        view.paint(ui, rect, &fixture.snapshot(), fixture.graph)
            .expect("painting");
    });
    consume(output);
    view.take_events()
}

fn button_event(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Dragging a text over several frames: the preview follows the pointer
/// without touching the model, and **a single** signal comes out, on
/// release — hence a single model change, a single history step. The view
/// framing, for its part, did not move: the drag did not scroll.
#[test]
fn dragging_a_text_signals_only_on_release() {
    let fixture = fixture_with_text();
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    let target = stokesforge_plot::AnnotationRef { panel: 0, index: 0 };

    assert!(frame_with(&context, &mut view, &fixture, rect, vec![], 0.0).is_empty());
    let (center, _, _) = view.annotation_screen_box(target).expect("text drawn");
    let revision = fixture.snapshot().revision;
    let generation = view.view().generation();

    let mut signals = frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![
            egui::Event::PointerMoved(center),
            button_event(center, true),
        ],
        0.1,
    );
    for step in 1..=5 {
        let pos = center + egui::vec2(12.0 * step as f32, 0.0);
        signals.extend(frame_with(
            &context,
            &mut view,
            &fixture,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.1 + 0.05 * step as f64,
        ));
        assert!(signals.is_empty(), "a signal before the release");
    }
    let preview = view.annotation_preview().expect("preview during the drag");
    assert!(preview.1.x > 16.0, "the preview did not follow the pointer");
    assert_eq!(
        fixture.snapshot().revision,
        revision,
        "the model moved during the drag"
    );

    let finish = center + egui::vec2(60.0, 0.0);
    signals.extend(frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![button_event(finish, false)],
        0.5,
    ));
    let [stokesforge_render_egui::GraphEvent::AnnotationMoved {
        target: moved,
        annotation,
    }] = &signals[..]
    else {
        panic!("a single signal expected: {signals:?}")
    };
    assert_eq!(*moved, target);
    assert!(annotation.x > 16.0 && (annotation.y - 24.0).abs() < 1.0);
    assert_eq!(view.view().generation(), generation, "the drag scrolled");
}

/// Pulling the right handle of a selected text (#244) fixes its width, as a
/// fraction of the figure, symmetrically around the center; a single signal,
/// on release, and the text does not move.
#[test]
fn pulling_the_handle_fixes_the_width() {
    let fixture = fixture_with_text();
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    let target = stokesforge_plot::AnnotationRef { panel: 0, index: 0 };

    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let (center, size, _) = view.annotation_screen_box(target).expect("text drawn");
    // A click selects the text.
    frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![
            egui::Event::PointerMoved(center),
            button_event(center, true),
        ],
        0.1,
    );
    frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![button_event(center, false)],
        0.2,
    );
    assert_eq!(view.selected_annotation(), Some(target));

    let handle_pos = center + egui::vec2(size.x / 2.0 + 3.0, 0.0);
    let goal = center + egui::vec2(150.0, 0.0);
    let mut signals = frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![
            egui::Event::PointerMoved(handle_pos),
            button_event(handle_pos, true),
        ],
        0.3,
    );
    for step in 1..=5 {
        let pos = handle_pos + (goal - handle_pos) * (step as f32 / 5.0);
        signals.extend(frame_with(
            &context,
            &mut view,
            &fixture,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.3 + 0.05 * step as f64,
        ));
    }
    assert!(signals.is_empty(), "a signal before the release");
    signals.extend(frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![button_event(goal, false)],
        1.0,
    ));
    let [stokesforge_render_egui::GraphEvent::AnnotationMoved { annotation, .. }] = &signals[..]
    else {
        panic!("a single signal expected: {signals:?}")
    };
    let width = annotation.width.expect("width fixed");
    assert!((width - 0.5).abs() < 0.03, "width {width}");
    assert_eq!((annotation.x, annotation.y), (16.0, 24.0));
}

/// A double click on a text asks to edit it and does not reset the zoom;
/// elsewhere, it keeps its role (reset).
#[test]
fn double_clicking_a_text_asks_to_edit_it() {
    let fixture = fixture_with_text();
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    let target = stokesforge_plot::AnnotationRef { panel: 0, index: 0 };
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let (center, _, _) = view.annotation_screen_box(target).expect("text drawn");
    let mut signals = Vec::new();
    for (index, (pressed, time)) in [(true, 0.1), (false, 0.12), (true, 0.2), (false, 0.22)]
        .into_iter()
        .enumerate()
    {
        let mut events = vec![button_event(center, pressed)];
        if index == 0 {
            events.insert(0, egui::Event::PointerMoved(center));
        }
        signals.extend(frame_with(
            &context, &mut view, &fixture, rect, events, time,
        ));
    }
    assert_eq!(
        signals,
        vec![stokesforge_render_egui::GraphEvent::EditAnnotation(target)]
    );
    assert_eq!(
        view.selected_annotation(),
        Some(target),
        "the text is selected"
    );
}

// -- Issue #53: dragging and double-clicking a title, a legend --

/// Pulling the handle of a selected title (#244) fixes its width, as a
/// fraction of the figure; a single signal, on release.
#[test]
fn pulling_a_title_handle_fixes_its_width() {
    use stokesforge_render_egui::{GraphEvent, TextTarget};
    let fixture = Fixture::with_title("Title");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let (center, size, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("title drawn");
    frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![
            egui::Event::PointerMoved(center),
            button_event(center, true),
        ],
        0.1,
    );
    frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![button_event(center, false)],
        0.2,
    );
    assert_eq!(view.selected_text(), Some(TextTarget::Title));

    let handle_pos = center + egui::vec2(size.x / 2.0 + 3.0, 0.0);
    let goal = center + egui::vec2(150.0, 0.0);
    let mut signals = frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![
            egui::Event::PointerMoved(handle_pos),
            button_event(handle_pos, true),
        ],
        0.3,
    );
    for step in 1..=5 {
        let pos = handle_pos + (goal - handle_pos) * (step as f32 / 5.0);
        signals.extend(frame_with(
            &context,
            &mut view,
            &fixture,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.3 + 0.05 * step as f64,
        ));
    }
    assert!(signals.is_empty(), "a signal before the release");
    signals.extend(frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![button_event(goal, false)],
        1.0,
    ));
    let [GraphEvent::TextResized { target, width }] = &signals[..] else {
        panic!("a single signal expected: {signals:?}")
    };
    assert_eq!(*target, TextTarget::Title);
    assert!((width - 0.5).abs() < 0.03, "width {width}");
}

/// Dragging the graph title: the preview follows the pointer without
/// touching the model, **a single** signal comes out on release, and the
/// view does not scroll.
#[test]
fn dragging_a_title_signals_only_on_release() {
    use stokesforge_render_egui::{GraphEvent, TextTarget};
    let fixture = Fixture::with_title("Title");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    assert!(frame_with(&context, &mut view, &fixture, rect, vec![], 0.0).is_empty());
    let (center, _, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("title drawn");
    let revision = fixture.snapshot().revision;
    let generation = view.view().generation();

    let mut signals = frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![
            egui::Event::PointerMoved(center),
            button_event(center, true),
        ],
        0.1,
    );
    for step in 1..=5 {
        let pos = center + egui::vec2(0.0, 20.0 * step as f32);
        signals.extend(frame_with(
            &context,
            &mut view,
            &fixture,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.1 + 0.05 * step as f64,
        ));
        assert!(signals.is_empty(), "a signal before the release");
    }
    let (goal, preview) = *view.text_preview().expect("preview during the drag");
    assert_eq!(goal, TextTarget::Title);
    assert_eq!(fixture.snapshot().revision, revision, "the model moved");

    signals.extend(frame_with(
        &context,
        &mut view,
        &fixture,
        rect,
        vec![button_event(center + egui::vec2(0.0, 100.0), false)],
        0.5,
    ));
    let [GraphEvent::TextMoved { target, anchor }] = &signals[..] else {
        panic!("a single signal expected: {signals:?}")
    };
    assert_eq!(*target, TextTarget::Title);
    assert_eq!(*anchor, preview, "the committed anchor is the preview's");
    assert_eq!(view.view().generation(), generation, "the drag scrolled");
    // Set the anchor in the model: the title is then drawn on it.
    let mut spec = match fixture
        .snapshot()
        .tree
        .node(fixture.graph)
        .expect("graph")
        .kind()
    {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("not a graph"),
    };
    assert!(TextTarget::Title.set_anchor(&mut spec, Some(*anchor)));
    fixture
        .handle()
        .execute(Command::SetPlotSpec {
            graph: fixture.graph,
            spec,
        })
        .expect("spec set");
    frame_with(&context, &mut view, &fixture, rect, vec![], 1.0);
    frame_with(&context, &mut view, &fixture, rect, vec![], 1.1);
    let (after, _, _) = view.text_screen_box(TextTarget::Title).expect("drawn");
    assert!(
        (after.y - (center.y + 100.0)).abs() < 3.0,
        "the title is where it was dropped: {after:?}"
    );
}

/// A double click on the title asks to edit it, and does not reset the
/// zoom.
#[test]
fn double_clicking_a_title_asks_to_edit_it() {
    use stokesforge_render_egui::{GraphEvent, TextTarget};
    let fixture = Fixture::with_title("Title");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let (center, _, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("title drawn");
    let mut signals = Vec::new();
    for (index, (pressed, time)) in [(true, 0.1), (false, 0.12), (true, 0.2), (false, 0.22)]
        .into_iter()
        .enumerate()
    {
        let mut events = vec![button_event(center, pressed)];
        if index == 0 {
            events.insert(0, egui::Event::PointerMoved(center));
        }
        signals.extend(frame_with(
            &context, &mut view, &fixture, rect, events, time,
        ));
    }
    assert_eq!(signals, vec![GraphEvent::EditText(TextTarget::Title)]);
}

/// Issue #77: a click on the title selects it (and the frame is painted), a
/// click in the void selects nothing anymore; the legend is selected by a
/// click on one of its entries, and moved by a drag that starts from an
/// entry.
#[test]
fn a_click_selects_a_title_or_the_legend() {
    use stokesforge_render_egui::{GraphEvent, TextTarget};
    let fixture = Fixture::with_title("Title");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    assert_eq!(view.selected_text(), None);

    let (title, _, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("title drawn");
    let mut elapsed = 0.1;
    let mut click = |view: &mut GraphView, pos: Pos2| {
        for (pressed, dt) in [(true, 0.0), (false, 0.02)] {
            let mut events = vec![button_event(pos, pressed)];
            if pressed {
                events.insert(0, egui::Event::PointerMoved(pos));
            }
            elapsed += dt + 0.4;
            frame_with(&context, view, &fixture, rect, events, elapsed);
        }
    };
    click(&mut view, title);
    assert_eq!(view.selected_text(), Some(TextTarget::Title));

    let legend = TextTarget::Legend { panel: 0 };
    let (center, size, _) = view.text_screen_box(legend).expect("legend drawn");
    let entry = center + egui::vec2(0.0, -size.y / 2.0 + 14.0);
    click(&mut view, entry);
    assert_eq!(view.selected_text(), Some(legend));

    // The void: in the center of the plot area, far from the legend and the title.
    click(&mut view, Pos2::new(300.0, 250.0));
    assert_eq!(view.selected_text(), None, "a click in the void deselects");

    // A drag started from a legend entry moves the legend.
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        rect,
        entry,
        entry + egui::vec2(-60.0, 40.0),
        10.0,
    );
    assert!(
        signals
            .iter()
            .any(|s| matches!(s, GraphEvent::TextMoved { target, .. } if *target == legend)),
        "{signals:?}"
    );
    assert_eq!(view.selected_text(), Some(legend));
}

// -- Issue #76: graphic objects (rectangle, ellipse, line, arrow) --

/// A fixture whose panel carries a **filled** rectangle in the middle of the
/// data (the fill makes it hittable in its middle).
fn fixture_with_shape() -> Fixture {
    let fixture = Fixture::with_title("Shape");
    let snapshot = fixture.snapshot();
    let mut spec = match snapshot.tree.node(fixture.graph).expect("graph").kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("not a graph"),
    };
    if let Some(panel) = spec.panels.get_mut(0) {
        panel.shapes.push(stokesforge_plot::ShapeSpec {
            fill_color: "#3366cc66".to_string(),
            ..stokesforge_plot::ShapeSpec::new(
                stokesforge_plot::ShapeKind::Rectangle,
                10.0,
                15.0,
                20.0,
                35.0,
            )
        });
    }
    fixture
        .handle()
        .execute(Command::SetPlotSpec {
            graph: fixture.graph,
            spec,
        })
        .expect("spec set");
    fixture
}

/// Drags from `from` to `to` in five steps; returns the signals emitted,
/// none of which may precede the release.
fn drag(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    rect: Rect,
    from: Pos2,
    to: Pos2,
    mut time: f64,
) -> Vec<stokesforge_render_egui::GraphEvent> {
    let mut signals = frame_with(
        context,
        view,
        fixture,
        rect,
        vec![egui::Event::PointerMoved(from), button_event(from, true)],
        time,
    );
    for step in 1..=5 {
        time += 0.05;
        let pos = from + (to - from) * (step as f32 / 5.0);
        signals.extend(frame_with(
            context,
            view,
            fixture,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            time,
        ));
        assert!(signals.is_empty(), "a signal before the release");
    }
    signals.extend(frame_with(
        context,
        view,
        fixture,
        rect,
        vec![button_event(to, false)],
        time + 0.05,
    ));
    signals
}

/// The on-screen corners of object `index` of the first panel.
fn shape_on_screen(view: &GraphView, index: usize) -> (Pos2, Pos2) {
    let drawn = view
        .drawn_shape(stokesforge_plot::ShapeRef { panel: 0, index })
        .expect("object drawn");
    let origin = view.display_rect().expect("rect").min.to_vec2();
    (
        Pos2::new(drawn.p1.0 as f32, drawn.p1.1 as f32) + origin,
        Pos2::new(drawn.p2.0 as f32, drawn.p2.1 as f32) + origin,
    )
}

/// Drawing with the Rectangle tool: the preview follows the pointer without
/// touching the model, **a single** signal comes out on release (an
/// addition, at the index after the last one), and the tool hands control
/// back to the Pointer.
#[test]
fn drawing_a_rectangle_signals_only_on_release() {
    let fixture = Fixture::with_title("Drawing");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    assert!(frame_with(&context, &mut view, &fixture, rect, vec![], 0.0).is_empty());
    view.set_tool(stokesforge_render_egui::GraphTool::Rectangle);
    let revision = fixture.snapshot().revision;

    let signals = drag(
        &context,
        &mut view,
        &fixture,
        rect,
        Pos2::new(250.0, 150.0),
        Pos2::new(380.0, 260.0),
        0.1,
    );
    let [stokesforge_render_egui::GraphEvent::ShapeChanged { target, shape }] = &signals[..] else {
        panic!("a single signal expected: {signals:?}")
    };
    assert_eq!(target.index, 0, "addition at the index after the last one");
    assert_eq!(shape.kind, stokesforge_plot::ShapeKind::Rectangle);
    assert!((shape.x2 - shape.x1).abs() > 0.0 && (shape.y2 - shape.y1).abs() > 0.0);
    assert_eq!(fixture.snapshot().revision, revision, "the model moved");
    assert_eq!(view.tool(), stokesforge_render_egui::GraphTool::Pointer);
    assert_eq!(view.selected_shape(), Some(*target));
}

/// A hand tremor with a drawing tool creates nothing.
#[test]
fn a_hand_tremor_with_a_drawing_tool_creates_nothing() {
    let fixture = Fixture::with_title("Drawing");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    view.set_tool(stokesforge_render_egui::GraphTool::Arrow);
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        rect,
        Pos2::new(300.0, 200.0),
        Pos2::new(302.0, 201.0),
        0.1,
    );
    assert!(signals.is_empty(), "{signals:?}");
    assert!(view.shape_preview().is_none());
}

/// Dragging a filled object moves it as a block (both points, same offset);
/// a single signal, on release; the framing did not move.
#[test]
fn dragging_a_shape_moves_it_in_a_single_signal() {
    let fixture = fixture_with_shape();
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let (a, b) = shape_on_screen(&view, 0);
    let center = a.lerp(b, 0.5);
    let generation = view.view().generation();

    let signals = drag(
        &context,
        &mut view,
        &fixture,
        rect,
        center,
        center + egui::vec2(60.0, 0.0),
        0.1,
    );
    let [stokesforge_render_egui::GraphEvent::ShapeChanged { target, shape }] = &signals[..] else {
        panic!("a single signal expected: {signals:?}")
    };
    assert_eq!(target.index, 0, "replacement, not addition");
    assert!(shape.x1 > 10.0 && shape.x2 > 20.0, "{shape:?}");
    assert!(
        ((shape.x2 - shape.x1) - 10.0).abs() < 1e-6 && ((shape.y2 - shape.y1) - 20.0).abs() < 1e-6,
        "moved as a block: the size does not change ({shape:?})"
    );
    assert_eq!(view.view().generation(), generation, "the drag scrolled");
}

/// A corner handle resizes: the opposite corner stays in place.
#[test]
fn dragging_a_handle_resizes_the_shape() {
    let fixture = fixture_with_shape();
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let (a, b) = shape_on_screen(&view, 0);
    // A click in the center selects the object, which makes its handles appear.
    let center = a.lerp(b, 0.5);
    let mut time = 0.1;
    for pressed in [true, false] {
        frame_with(
            &context,
            &mut view,
            &fixture,
            rect,
            vec![
                egui::Event::PointerMoved(center),
                button_event(center, pressed),
            ],
            time,
        );
        time += 0.05;
    }
    assert_eq!(view.selected_shape().map(|t| t.index), Some(0));

    let bottom_right_corner = Pos2::new(a.x.max(b.x), a.y.max(b.y));
    let signals = drag(
        &context,
        &mut view,
        &fixture,
        rect,
        bottom_right_corner,
        bottom_right_corner + egui::vec2(40.0, 30.0),
        time + 0.2,
    );
    let [stokesforge_render_egui::GraphEvent::ShapeChanged { shape, .. }] = &signals[..] else {
        panic!("a single signal expected: {signals:?}")
    };
    let (left, top) = (shape.x1.min(shape.x2), shape.y1.max(shape.y2));
    assert!(
        (left - 10.0).abs() < 0.2,
        "the opposite corner moved: {shape:?}"
    );
    assert!(
        (top - 35.0).abs() < 0.5,
        "the opposite corner moved: {shape:?}"
    );
    assert!(
        (shape.x2 - shape.x1).abs() > 10.5,
        "not enlarged: {shape:?}"
    );
}

// -- Issue #113: double click opens the properties of the targeted object --

/// A double click at `pos` over four frames; returns the signals emitted.
fn double_click(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    rect: Rect,
    pos: Pos2,
) -> Vec<stokesforge_render_egui::GraphEvent> {
    let mut signals = Vec::new();
    for (index, (pressed, time)) in [(true, 0.1), (false, 0.12), (true, 0.2), (false, 0.22)]
        .into_iter()
        .enumerate()
    {
        let mut events = vec![button_event(pos, pressed)];
        if index == 0 {
            events.insert(0, egui::Event::PointerMoved(pos));
        }
        signals.extend(frame_with(context, view, fixture, rect, events, time));
    }
    signals
}

/// On an axis: the axis properties. On a curve: the series properties. In
/// the void of the plot area: the zoom is reset, as before.
#[test]
fn double_clicking_an_axis_or_a_curve_opens_its_properties() {
    use stokesforge_render::{AxisSide, PlotComponent};
    use stokesforge_render_egui::GraphEvent;
    let fixture = Fixture::with_title("Title");
    let context = new_context();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let spec = match fixture
        .snapshot()
        .tree
        .node(fixture.graph)
        .expect("graph")
        .kind()
    {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("not a graph"),
    };
    let sweep = || {
        (0..600).step_by(2).flat_map(|x| {
            (0..400)
                .step_by(2)
                .map(move |y| Pos2::new(x as f32, y as f32))
        })
    };

    let axis = sweep()
        .find(|pos| {
            matches!(
                view.component_at(*pos),
                Some(PlotComponent::Axis {
                    side: AxisSide::Bottom,
                    ..
                })
            ) && view.series_at(*pos, &spec).is_none()
        })
        .expect("a point of the abscissa axis");
    let signals = double_click(&context, &mut view, &fixture, rect, axis);
    assert!(
        matches!(
            signals.as_slice(),
            [GraphEvent::OpenAxisProperties {
                side: AxisSide::Bottom,
                ..
            }]
        ),
        "{signals:?}"
    );

    // Fresh context: egui time does not go back, and the previous double
    // click must not mix with the next one.
    let context = new_context();
    let mut view = GraphView::new();
    frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
    let curve = sweep()
        .find(|pos| view.series_at(*pos, &spec).is_some())
        .expect("a point of the curve");
    let expected = view.series_at(curve, &spec).expect("targeted series");
    let signals = double_click(&context, &mut view, &fixture, rect, curve);
    assert_eq!(signals, vec![GraphEvent::OpenSeriesProperties(expected)]);
}

// -- #318: Ctrl + drag pans everywhere, including over an object --

fn ctrl_key(on: bool) -> egui::Event {
    egui::Event::ModifiersChanged(if on {
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    } else {
        egui::Modifiers::NONE
    })
}

/// Drags from `from` to `to` in five steps, with Ctrl held down from before
/// the press (or the middle button if `middle`); returns the signals
/// emitted.
fn drag_under_ctrl(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    rect: Rect,
    from: Pos2,
    to: Pos2,
    middle: bool,
) -> Vec<stokesforge_render_egui::GraphEvent> {
    let button = if middle {
        egui::PointerButton::Middle
    } else {
        egui::PointerButton::Primary
    };
    let press = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut start = vec![egui::Event::PointerMoved(from), press(from, true)];
    if !middle {
        start.insert(0, ctrl_key(true));
    }
    let mut signals = frame_with(context, view, fixture, rect, start, 0.1);
    for step in 1..=5 {
        let pos = from + (to - from) * (step as f32 / 5.0);
        signals.extend(frame_with(
            context,
            view,
            fixture,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.1 + 0.05 * step as f64,
        ));
    }
    signals.extend(frame_with(
        context,
        view,
        fixture,
        rect,
        vec![press(to, false), ctrl_key(false)],
        0.5,
    ));
    signals.extend(frame_with(context, view, fixture, rect, vec![], 0.6));
    signals
}

/// A free text, a title and a shape do not move under Ctrl or middle click:
/// the gesture is a pan, without `AnnotationMoved`, `TextMoved` or
/// `ShapeChanged`. The plain drag, for its part, always moves them (adjacent
/// tests).
#[test]
fn ctrl_drag_started_on_an_object_pans_instead_of_moving_it() {
    use stokesforge_render_egui::TextTarget;
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    for middle in [false, true] {
        // Free text.
        let fixture = fixture_with_text();
        let context = new_context();
        let mut view = GraphView::new();
        frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
        let (center, _, _) = view
            .annotation_screen_box(stokesforge_plot::AnnotationRef { panel: 0, index: 0 })
            .expect("text drawn");
        let signals = drag_under_ctrl(
            &context,
            &mut view,
            &fixture,
            rect,
            center,
            center + egui::vec2(60.0, 0.0),
            middle,
        );
        assert!(signals.is_empty(), "text (middle={middle}): {signals:?}");
        assert!(
            view.view().generation() > 0,
            "text (middle={middle}): no pan"
        );
        assert!(view.annotation_preview().is_none());

        // Title.
        let fixture = Fixture::with_title("Title");
        let context = new_context();
        let mut view = GraphView::new();
        frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
        let (center, _, _) = view.text_screen_box(TextTarget::Title).expect("title");
        let signals = drag_under_ctrl(
            &context,
            &mut view,
            &fixture,
            rect,
            center,
            center + egui::vec2(0.0, 60.0),
            middle,
        );
        assert!(signals.is_empty(), "title (middle={middle}): {signals:?}");
        assert!(
            view.view().generation() > 0,
            "title (middle={middle}): no pan"
        );

        // Shape.
        let fixture = fixture_with_shape();
        let context = new_context();
        let mut view = GraphView::new();
        frame_with(&context, &mut view, &fixture, rect, vec![], 0.0);
        let (a, b) = shape_on_screen(&view, 0);
        let center = a.lerp(b, 0.5);
        let signals = drag_under_ctrl(
            &context,
            &mut view,
            &fixture,
            rect,
            center,
            center + egui::vec2(60.0, 0.0),
            middle,
        );
        assert!(signals.is_empty(), "shape (middle={middle}): {signals:?}");
        assert!(
            view.view().generation() > 0,
            "shape (middle={middle}): no pan"
        );
    }
}

//! The anti-aliasing lever — task R5.5 bis, verified rather than asserted.
//!
//! Two things to prove, and they are not proved in the same place:
//!
//! 1. **the lever has an effect**, and is not just a boolean that flips.
//!    This is measurement R0 0.3 replayed in small: the *same* figure is
//!    tessellated twice and the vertices are counted. If `epaint` ever
//!    changed its fringe strategy, this test would fail and the decision of
//!    2026-09-10 would have to be reexamined — which is exactly what we want
//!    to know;
//! 2. **the cache is not invalidated by it.** This is the half that is
//!    easiest to lose: adding the anti-aliasing mode to the key
//!    `(revision, rect, pixels_per_point)` "to be safe" would rebuild the
//!    whole figure on every button press, thus paying dearly at precisely
//!    the moment the lever exists to lighten
//!    (`architecture-choisie.md` §4.2).

use egui::epaint::Primitive;
use egui::{Pos2, Rect};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render_egui::feathering::{self, Feathering};
use stokesforge_render_egui::{font_definitions, GraphView};
use stokesforge_session::{Session, Snapshot};

const RECT: Rect = Rect {
    min: Pos2::ZERO,
    max: Pos2::new(800.0, 600.0),
};

/// A project holding a deliberately heavy graph in **strokes**: it is the
/// fringe of strokes that the lever removes, not that of text.
fn bench() -> (Session, NodeId) {
    let mut project = Project::new("Fringe").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");

    let mut panel = PanelSpec::at(0, 0);
    for series_index in 0..8 {
        let xs: Vec<f64> = (0..2000).map(|i| i as f64 * 0.01).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| (x + series_index as f64).sin() * (1.0 + series_index as f64))
            .collect();
        let x = project
            .add_column(sheet, None, Some(&format!("X{series_index}")), numeric(&xs))
            .expect("X");
        let y = project
            .add_column(sheet, None, Some(&format!("Y{series_index}")), numeric(&ys))
            .expect("Y");
        let mut series = SeriesSpec::new_xy(x, y);
        series.line = LineSpec::from_palette(series_index);
        series.show_in_legend = false;
        panel.series.push(series);
    }
    let spec = PlotSpec::empty("Fringe").with_panels(panel);
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

fn context() -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(font_definitions());
    context
}

/// Paints a frame and returns its shapes, ready to tessellate.
fn shapes_of(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
) -> (Vec<egui::epaint::ClippedShape>, bool) {
    let mut from_cache = false;
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        from_cache = view.paint(ui, RECT, snapshot, graph).expect("painting");
    });
    let shapes = std::mem::take(&mut output.shapes);
    // Without a window, nobody uploads the atlas: `epaint` panics on an
    // unapplied `TexturesDelta` (see `tests/cache_session.rs`).
    output.textures_delta.clear();
    (shapes, from_cache)
}

fn vertex_count(context: &egui::Context, shapes: Vec<egui::epaint::ClippedShape>) -> usize {
    context
        .tessellate(shapes, 1.0)
        .iter()
        .map(|clipped| match &clipped.primitive {
            Primitive::Mesh(mesh) => mesh.vertices.len(),
            Primitive::Callback(_) => 0,
        })
        .sum()
}

#[test]
fn turning_off_the_fringe_really_reduces_the_tessellation() {
    let (session, graph) = bench();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = context();
    let mut view = GraphView::new();
    let (shapes, _) = shapes_of(&context, &mut view, &snapshot, graph);
    assert!(
        !shapes.is_empty(),
        "the figure is empty, nothing to measure"
    );

    feathering::apply(&context, Feathering::Full);
    let with = vertex_count(&context, shapes.clone());
    feathering::apply(&context, Feathering::Off);
    let without = vertex_count(&context, shapes);

    assert!(
        without < with,
        "turning off the fringe changed nothing ({without} vertices against {with}): \
         the lever of task R5.5 bis is no longer one, and the decision of \
         2026-09-10 must be reexamined"
    );
    // Measurement R0 0.3 gave 78% of the tessellation cost; the vertex ratio
    // is not the time ratio, and the figure here also carries text, which the
    // fringe does not concern. So we check an order of magnitude, not the
    // figure: a fringe that cost only a few percent would mean the lever has
    // stopped being the one we believe in.
    let saving = (with - without) as f64 / with as f64;
    assert!(
        saving > 0.25,
        "the fringe now weighs only {:.0}% of the vertices (with {with}, without {without})",
        saving * 100.0
    );
}

#[test]
fn the_mode_can_be_read_and_restored() {
    let context = context();
    let initial = feathering::current(&context);
    assert_eq!(initial, Feathering::Full, "egui starts with the fringe on");

    let previous = feathering::apply(&context, Feathering::Off);
    assert_eq!(previous, Feathering::Full);
    assert_eq!(feathering::current(&context), Feathering::Off);

    feathering::apply(&context, previous);
    assert_eq!(feathering::current(&context), Feathering::Full);
}

#[test]
fn toggling_the_fringe_does_not_invalidate_the_shape_cache() {
    // The property that `architecture-choisie.md` §4.2 states and that
    // `src/cache.rs` comments on: turning off the fringe changes the
    // tessellation, not the `Shape`s. Three written reminders are not worth
    // a check.
    let (session, graph) = bench();
    let snapshot = session.handle().snapshot().expect("snapshot");
    let context = context();
    let mut view = GraphView::new();

    let (_, first) = shapes_of(&context, &mut view, &snapshot, graph);
    assert!(!first, "the first frame is necessarily a computation");
    let key = view.cache().key();
    let builds = view.cache().misses();

    feathering::apply(&context, Feathering::Off);
    let (_, second) = shapes_of(&context, &mut view, &snapshot, graph);
    assert!(
        second,
        "turning off the fringe invalidated the cache: the key grew a term \
         that it must not carry (architecture-choisie.md §4.2)"
    );

    feathering::apply(&context, Feathering::Full);
    let (_, third) = shapes_of(&context, &mut view, &snapshot, graph);
    assert!(third, "restoring must not invalidate anything either");

    assert_eq!(view.cache().key(), key, "the cache key changed");
    assert_eq!(
        view.cache().misses(),
        builds,
        "the Plotters pipeline was replayed although only the fringe moved"
    );
}

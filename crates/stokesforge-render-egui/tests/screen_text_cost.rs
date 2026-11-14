//! Cost bench of the screen formula textures (#508): memory and time.
//!
//! Prints figures only, asserts nothing about them. Costly and noisy, so
//! `#[ignore]`: run it with
//! `cargo test -p stokesforge-render-egui --test screen_text_cost -- --ignored --nocapture`.
//!
//! It lives here and not in `stokesforge-robustness` (next to the cost battery
//! b17): that crate does not depend on egui, and the textures belong to an
//! `egui::Context`.

use std::time::Instant;

use egui::{Pos2, Rect};
use stokesforge_commands::Command;
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{AnnotationSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::formula::{self, FormulaCache, Ink};
use stokesforge_render::{FontChoice, TextMeasurer};
use stokesforge_render_egui::scaled::{ScaledKey, ScaledShapes};
use stokesforge_render_egui::{
    collect_shapes_with_typst, font_definitions, GraphView, ScreenText, TypstScreen,
};
use stokesforge_session::{Session, SessionHandle, Snapshot};

struct Fixture {
    session: Session,
    graph: NodeId,
}

impl Fixture {
    /// A title, three axis titles, 24 legend entries and two annotations.
    fn heavy() -> Self {
        let mut project = Project::new("Cost").expect("project");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("workbook");
        let sheet = project.add_worksheet(book, None).expect("sheet");
        let column = |values: Vec<f64>| {
            Column::from_block(
                ColumnType::Numeric,
                ValueBlock::Float(values),
                ColumnMeta::default(),
            )
            .expect("column")
        };
        let xs: Vec<f64> = (0..32).map(f64::from).collect();
        let x = project
            .add_column(sheet, None, Some("X"), column(xs.clone()))
            .expect("X");
        let y = project
            .add_column(
                sheet,
                None,
                Some("Y"),
                column(xs.iter().map(|v| v * 1.5).collect()),
            )
            .expect("Y");
        let mut panel = PanelSpec::at(0, 0);
        panel.bottom.label = "$lambda$ (nm)".to_owned();
        panel.left.label = "Absorbance".to_owned();
        panel.right.label = "Intensity (a.u.)".to_owned();
        for index in 0..24 {
            let mut series = SeriesSpec::new_xy(x, y);
            series.line = LineSpec::from_palette(index);
            series.name = format!("Sample {index} $Delta H$");
            panel.series.push(series);
        }
        panel
            .annotations
            .push(AnnotationSpec::new("Peak $alpha$", 5.0, 5.0));
        panel
            .annotations
            .push(AnnotationSpec::new("Shoulder", 12.0, 9.0));
        let spec = PlotSpec {
            title: "Spectrum of the samples".into(),
            ..PlotSpec::empty("")
        }
        .with_panels(panel);
        let graph = project.add_graph(root, None, spec).expect("graph");
        let session = Session::spawn(project, None).expect("session");
        Self { session, graph }
    }

    fn handle(&self) -> SessionHandle {
        self.session.handle()
    }

    fn snapshot(&self) -> Snapshot {
        self.handle().snapshot().expect("snapshot")
    }

    fn spec(&self) -> PlotSpec {
        match self.snapshot().tree.node(self.graph).expect("graph").kind() {
            stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
            _ => panic!("not a graph"),
        }
    }

    fn set_title(&self, title: &str) {
        let mut spec = self.spec();
        spec.title = title.to_owned();
        self.handle()
            .execute(Command::SetPlotSpec {
                graph: self.graph,
                spec,
            })
            .expect("spec set");
    }
}

fn new_context(pixels_per_point: f32) -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(font_definitions());
    context.set_pixels_per_point(pixels_per_point);
    context
}

/// (allocated textures, bytes at 4 bytes per pixel) over the whole manager,
/// glyph atlas included.
fn textures(context: &egui::Context) -> (usize, usize) {
    let manager = context.tex_manager();
    let manager = manager.read();
    let bytes = manager
        .allocated()
        .map(|(_, meta)| meta.size[0] * meta.size[1] * 4)
        .sum();
    (manager.num_allocated(), bytes)
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn frame(
    context: &egui::Context,
    view: &mut GraphView,
    fixture: &Fixture,
    scale: f32,
) -> (f64, bool) {
    // The logical size is the figure's own, found by a first layout.
    let mut size = egui::Vec2::ZERO;
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        let layout = view
            .figure_layout(ui.ctx(), &fixture.snapshot(), fixture.graph)
            .expect("layout");
        size = egui::vec2(layout.size.0 as f32, layout.size.1 as f32);
    });
    output.textures_delta.clear();
    let started = Instant::now();
    let mut cached = false;
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        cached = view
            .paint_scaled(
                ui,
                Rect::from_min_size(Pos2::ZERO, size * scale),
                &fixture.snapshot(),
                fixture.graph,
            )
            .expect("painting");
    });
    output.textures_delta.clear();
    (started.elapsed().as_secs_f64() * 1000.0, cached)
}

#[test]
#[ignore = "cost bench: prints figures, run by hand with --nocapture"]
fn screen_text_cost() {
    let fixture = Fixture::heavy();

    println!("== (a) memory of the heavy figure (title, 3 axis titles, 24 entries, 2 notes)");
    println!("ppp  scale  textures  MB     formulas  first_ms  repaint_ms");
    for ppp in [1.0_f32, 2.0] {
        for scale in [1.0_f32, 2.0] {
            let context = new_context(ppp);
            let mut view = GraphView::new();
            let (base_n, base_bytes) = {
                let mut output = context.run_ui(egui::RawInput::default(), |_| {});
                output.textures_delta.clear();
                textures(&context)
            };
            let (first, _) = frame(&context, &mut view, &fixture, scale);
            let (n, bytes) = textures(&context);
            let (again, _) = frame(&context, &mut view, &fixture, scale);
            println!(
                "{ppp:<4} {scale:<6} {n:<9} {:<6.2} {:<9} {first:<9.1} {again:.2}   (baseline {base_n} textures, {:.2} MB)",
                mb(bytes),
                view.formula_count(),
                mb(base_bytes)
            );
        }
    }

    println!("== (b) 300 title revisions at scale 1.37, ppp 1");
    {
        let context = new_context(1.0);
        let mut view = GraphView::new();
        frame(&context, &mut view, &fixture, 1.37);
        let (n0, b0) = textures(&context);
        let started = Instant::now();
        let mut peak = (n0, b0, view.formula_count());
        for revision in 0..300 {
            fixture.set_title(&format!("Title revision {revision} $alpha_{revision}$"));
            frame(&context, &mut view, &fixture, 1.37);
            let (n, bytes) = textures(&context);
            if bytes > peak.1 {
                peak = (n, bytes, view.formula_count());
            }
        }
        let (n, bytes) = textures(&context);
        println!(
            "start: {n0} textures {:.2} MB | end: {n} textures {:.2} MB, {} formulas | peak: {} textures {:.2} MB, {} formulas | {:.0} ms for 300 revisions",
            mb(b0),
            mb(bytes),
            view.formula_count(),
            peak.0,
            mb(peak.1),
            peak.2,
            started.elapsed().as_secs_f64() * 1000.0
        );
        fixture.set_title("Spectrum of the samples");
    }

    println!("== (c) per additional open GraphView (same figure, one context, scale 1.37, ppp 1)");
    {
        let context = new_context(1.0);
        let mut views: Vec<GraphView> = Vec::new();
        let mut previous = textures(&context);
        println!("views  textures  MB     delta_MB  delta_textures");
        for count in 1..=3 {
            let mut view = GraphView::new();
            frame(&context, &mut view, &fixture, 1.37);
            views.push(view);
            let now = textures(&context);
            println!(
                "{count:<6} {:<9} {:<6.2} {:<9.2} {}",
                now.0,
                mb(now.1),
                mb(now.1.saturating_sub(previous.1)),
                now.0 as isize - previous.0 as isize
            );
            previous = now;
        }
        drop(views);
    }

    println!("== time: compile vs rasterize vs full build");
    {
        let choice = FontChoice::default();
        let (sans, math) = formula::resolve_fonts(&choice).expect("fonts");
        let texts: Vec<String> = (0..24)
            .map(|i| format!("Sample {i} $Delta H$"))
            .chain([
                "Spectrum of the samples".to_owned(),
                "$lambda$ (nm)".to_owned(),
            ])
            .collect();
        let ink: Ink = [0, 0, 0, 255];
        let started = Instant::now();
        let compiled: Vec<_> = texts
            .iter()
            .map(|t| formula::compile(t, 12.0, ink, sans.clone(), math.clone()).expect("compile"))
            .collect();
        let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
        let _ = compiled;
        // Rasterize through the public `rerender` of a texture built by the
        // view at ppp 1: it reuses the compiled page.
        let context = new_context(1.0);
        let mut view = GraphView::new();
        let (first, _) = frame(&context, &mut view, &fixture, 1.0);
        println!(
            "{} texts: compile only {compile_ms:.1} ms ({:.2} ms/text); full first build of the figure {first:.1} ms",
            texts.len(),
            compile_ms / texts.len() as f64
        );
        let (cached_hit, hit) = frame(&context, &mut view, &fixture, 1.0);
        println!("repaint without change: {cached_hit:.2} ms (cache hit: {hit})");
        // Rebuild with warm formula cache: a new title only.
        fixture.set_title("A new title");
        let (rebuild, _) = frame(&context, &mut view, &fixture, 1.0);
        println!("rebuild with a warm formula cache (one new title): {rebuild:.1} ms");
        fixture.set_title("Spectrum of the samples");
    }

    println!("== ScaledShapes rebuild on a resize drag, 30 frames (scale changes every frame)");
    {
        let context = new_context(1.0);
        let mut view = GraphView::new();
        frame(&context, &mut view, &fixture, 1.0);
        let before = textures(&context);
        let mut times = Vec::new();
        let mut peak = before;
        for step in 0..30 {
            let scale = 1.0 + 0.02 * (step + 1) as f32;
            let (ms, _) = frame(&context, &mut view, &fixture, scale);
            times.push(ms);
            let now = textures(&context);
            if now.1 > peak.1 {
                peak = now;
            }
        }
        let after = textures(&context);
        times.sort_by(|a, b| a.total_cmp(b));
        println!(
            "paint_scaled per frame: min {:.1} ms, median {:.1} ms, max {:.1} ms, total {:.0} ms",
            times[0],
            times[15],
            times[29],
            times.iter().sum::<f64>()
        );
        println!(
            "textures: before {} / {:.2} MB, peak {} / {:.2} MB, after {} / {:.2} MB",
            before.0,
            mb(before.1),
            peak.0,
            mb(peak.1),
            after.0,
            mb(after.1)
        );
    }

    // `ScaledShapes` used directly, to isolate its rebuild from the figure's.
    println!("== ScaledShapes::get alone, 30 distinct scales over the cached shapes");
    {
        let context = new_context(1.0);
        let snapshot = fixture.snapshot();
        let spec = fixture.spec();
        let plot = resolve_spec(&snapshot.tree, &spec);
        let measurer = TextMeasurer::default();
        let choice = FontChoice::default();
        let formulas: FormulaCache<_> = FormulaCache::new();
        let held = stokesforge_render_egui::TextureSet::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(688.0, 400.0));
        let mut source = Vec::new();
        let mut output = context.run_ui(egui::RawInput::default(), |_| {
            let text = ScreenText::new(&context, &measurer);
            let typst = TypstScreen::new(&context, &formulas, &choice)
                .expect("fonts")
                .holding(&held);
            collect_shapes_with_typst(&mut source, rect, &text, typst, &plot).expect("drawing");
        });
        output.textures_delta.clear();
        let key0 = stokesforge_render_egui::cache::CacheKey {
            revision: Default::default(),
            rect,
            pixels_per_point: 1.0,
            view_generation: 0,
        };
        let mut scaled = ScaledShapes::default();
        let mut times = Vec::new();
        for step in 0..30 {
            let scale = 1.0 + 0.02 * (step + 1) as f32;
            let key = ScaledKey {
                source: key0,
                display: Rect::from_min_size(Pos2::ZERO, rect.size() * scale),
                build: 0,
            };
            let transform = egui::emath::TSTransform::new(egui::Vec2::ZERO, scale);
            let started = Instant::now();
            let _ = scaled.get(&context, key, &source, transform, &held);
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let (n, bytes) = textures(&context);
        times.sort_by(|a, b| a.total_cmp(b));
        println!(
            "get per frame: min {:.1} ms, median {:.1} ms, max {:.1} ms; textures {n}, {:.2} MB",
            times[0],
            times[15],
            times[29],
            mb(bytes)
        );
    }
}

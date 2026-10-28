//! Bench #141: offscreen CPU, without claiming to measure a GPU submission.
//!
//! The volumes are the TOTAL of points (spread over 1 or 10 series).
//! The cache is the one of the application; zoom/pan change its generation,
//! moving the window keeps its key. The pipeline and LTTB stay those
//! of the shared rendering. The isolated preparation and LTTB are side
//! measurements, not durations to subtract from the pipeline (different
//! allocations). No time threshold is an assertion: this bench stays manual.
//! It exercises the normal LTTB; the witness without LTTB of the initial
//! experiment was not kept as a renderer option.

use std::hint::black_box;
use std::time::Instant;

use egui::epaint::Primitive;
use egui::{Pos2, Rect};
use plotters::drawing::IntoDrawingArea;
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{Column, Project};
use stokesforge_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::view::{Framing, ViewState};
use stokesforge_render::{decimate, pipeline, series};
use stokesforge_render_egui::{
    feathering, font_definitions, CacheKey, EguiBackend, Feathering, ScreenText, ShapeCache,
    TextMeasurer,
};

const RECT: Rect = Rect {
    min: Pos2::ZERO,
    max: Pos2::new(800.0, 600.0),
};

fn bench_project(total: usize, count: usize) -> (Project, PlotSpec) {
    let mut project = Project::new("Curve measurement").unwrap();
    let book = project.add_empty_workbook(project.root(), None).unwrap();
    let sheet = project.add_worksheet(book, None).unwrap();
    let n = total / count;
    let xs: Vec<_> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&xs))
        .unwrap();
    let mut panel = PanelSpec::at(0, 0);
    for rank in 0..count {
        // Deterministic signal, fast and slow ripples, all the
        // data visible at the initial framing. The series share X.
        let ys: Vec<_> = xs
            .iter()
            .map(|x| (x * 100.0 + rank as f64).sin() + 0.2 * (x * 7919.0).sin())
            .collect();
        let y = project
            .add_column(
                sheet,
                None,
                Some(&format!("Y{rank}")),
                Column::from_f64(&ys),
            )
            .unwrap();
        let mut spec = SeriesSpec::new_xy(x, y);
        spec.line = LineSpec::from_palette(rank);
        spec.show_in_legend = false;
        panel.series.push(spec);
    }
    // Without rich texts: no Typst cost hidden in the curve measurements.
    (project, PlotSpec::empty("").with_panels(panel))
}

#[derive(Clone, Copy, Default)]
struct Measurement {
    pipeline: f64,
    image: f64,
    tessellation: f64,
    total: f64,
    bytes: usize,
    removed: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

struct Bench {
    context: egui::Context,
    text: TextMeasurer,
    cache: ShapeCache,
    view: ViewState,
}

impl Bench {
    fn new() -> Self {
        let context = egui::Context::default();
        context.set_fonts(font_definitions());
        Self {
            context,
            text: TextMeasurer::default(),
            cache: ShapeCache::new(),
            view: ViewState::new(),
        }
    }

    fn image(
        &mut self,
        project: &Project,
        spec: &PlotSpec,
        feathering: Feathering,
        warm: bool,
    ) -> Measurement {
        feathering::apply(&self.context, feathering);
        let key = CacheKey {
            revision: 1,
            rect: RECT,
            pixels_per_point: 1.0,
            view_generation: self.view.generation(),
        };
        let start = Instant::now();
        let mut measurement = Measurement::default();
        let mut output = self.context.run_ui(
            egui::RawInput {
                screen_rect: Some(RECT),
                ..Default::default()
            },
            |ui| {
                let mut framed = spec.clone();
                self.view.apply(&mut framed);
                let outcome = self
                    .cache
                    .get_or_build(key, |shapes| {
                        let t = Instant::now();
                        let plot = resolve_spec(project.tree(), &framed);
                        let text = ScreenText::new(&self.context, &self.text);
                        let backend = EguiBackend::in_rect(shapes, RECT, &text).into_drawing_area();
                        let drawn = pipeline::draw_on(&backend, &plot).unwrap();
                        measurement.pipeline = ms(t);
                        measurement.removed = drawn.decimated_points;
                        Ok::<_, ()>(drawn)
                    })
                    .unwrap();
                assert_eq!(outcome.cached, warm, "wrong cache regime");
                ui.painter()
                    .with_clip_rect(RECT)
                    .extend(self.cache.shapes().iter().cloned());
            },
        );
        measurement.image = ms(start);
        let t = Instant::now();
        let primitives = self
            .context
            .tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        measurement.tessellation = ms(t);
        measurement.total = ms(start);
        for p in &primitives {
            if let Primitive::Mesh(mesh) = &p.primitive {
                measurement.bytes += mesh.vertices.len()
                    * std::mem::size_of::<egui::epaint::Vertex>()
                    + mesh.indices.len() * std::mem::size_of::<u32>();
            }
        }
        black_box(&primitives);
        // Without a native renderer, the atlas is not uploaded (#103).
        output.textures_delta.clear();
        measurement
    }
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs[xs.len() / 2]
}

fn report(total: usize, count: usize, scenario: &str, samples: &[Measurement]) {
    let m = |f: fn(&Measurement) -> f64| median(samples.iter().map(f).collect());
    println!(
        "IMAGE,{total},{count},true,{scenario},{:.3},{:.3},{:.3},{:.3},{:.3},{},{}",
        m(|s| s.pipeline),
        m(|s| s.image),
        m(|s| s.tessellation),
        m(|s| s.total),
        samples.iter().map(|s| s.total).fold(0.0, f64::max),
        samples[0].bytes,
        samples[0].removed
    );
}

fn phases(
    project: &Project,
    spec: &PlotSpec,
    total: usize,
    count: usize,
    iterations: usize,
    threshold: usize,
) {
    let plot = resolve_spec(project.tree(), spec);
    let mut prepares = Vec::new();
    let mut decimates = Vec::new();
    for _ in 0..iterations {
        let t = Instant::now();
        let prepared: Vec<_> = plot.panels[0]
            .series
            .iter()
            .map(|s| series::prepare(s, spec.panels[0].bottom.scale, spec.panels[0].left.scale))
            .collect();
        prepares.push(ms(t));
        let t = Instant::now();
        let outputs: Vec<_> = prepared
            .iter()
            .filter_map(|p| match &p.geometry {
                series::Geometry::Curve { points, .. } => Some(decimate::lttb(points, threshold)),
                _ => None,
            })
            .collect();
        decimates.push(ms(t));
        black_box(outputs);
    }
    println!(
        "PHASES,{total},{count},{threshold},{:.3},{:.3}",
        median(prepares),
        median(decimates)
    );
}

#[test]
#[ignore = "costly bench #141, to run in release with --ignored --nocapture"]
fn curve_bench_large_volumes() {
    if cfg!(debug_assertions) {
        panic!("the measurements require --release");
    }
    let iterations = std::env::var("STOKESFORGE_BENCH_FRAMES")
        .map(|s| s.parse().unwrap())
        .unwrap_or(5usize);
    assert!(iterations >= 3, "at least three frames per regime");
    let volumes: Vec<usize> = std::env::var("STOKESFORGE_BENCH_POINTS")
        .map(|s| s.split(',').map(|n| n.parse().unwrap()).collect())
        .unwrap_or_else(|_| vec![100_000, 1_000_000, 10_000_000]);
    println!("IMAGE,points_total,series,lttb,scenario,pipeline_ms,image_ms,tessellation_ms,total_median_ms,total_max_ms,mesh_bytes,points_removed");
    println!("PHASES,points_total,series,threshold,preparation_median_ms,isolated_lttb_median_ms");
    for total in volumes {
        for count in [1, 10] {
            assert!(total / count >= 3 && total % count == 0);
            let (project, spec) = bench_project(total, count);
            let mut bench = Bench::new();
            // The font warm-up and the first frame are outside the
            // statistics; only the SHAPE cache is cold afterwards.
            bench.image(&project, &spec, Feathering::Full, false);
            {
                let plot = resolve_spec(project.tree(), &spec);
                let mut shapes = Vec::new();
                let drawn =
                    stokesforge_render_egui::collect_shapes(&mut shapes, RECT, &bench.text, &plot)
                        .unwrap();
                let width = drawn.panels[0].pixel_size.0;
                phases(
                    &project,
                    &spec,
                    total,
                    count,
                    iterations,
                    decimate::threshold_for_width(width),
                );
            }
            for scenario in [
                "window_warm",
                "zoom_cold",
                "zoom_warm",
                "pan_cold",
                "pan_warm",
            ] {
                let mut samples = Vec::new();
                for frame in 0..iterations {
                    let cold = scenario.ends_with("cold");
                    if cold {
                        let step = frame as f64 * 0.002;
                        let x = if scenario.starts_with("zoom") {
                            (0.1 + step, 0.9 - step)
                        } else {
                            (0.1 + step, 0.9 + step)
                        };
                        bench.view.set_framing(0, Framing::xy(x, (-1.3, 1.3)));
                    }
                    let feathering = if scenario.starts_with("pan") {
                        Feathering::Off
                    } else {
                        Feathering::Full
                    };
                    samples.push(bench.image(&project, &spec, feathering, !cold));
                }
                report(total, count, scenario, &samples);
            }
        }
    }
}

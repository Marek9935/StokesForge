//! Banc #141 : CPU hors écran, sans prétendre mesurer une soumission GPU.
//!
//! Les volumes sont le TOTAL des points (répartis en 1 ou 10 séries).
//! Le cache est celui de l'application ; zoom/pan changent sa génération,
//! déplacer la fenêtre conserve sa clé. Le pipeline et LTTB restent ceux
//! du rendu partagé. La préparation et LTTB isolés sont des mesures annexes,
//! pas des durées à soustraire du pipeline (allocations différentes).
//! Aucun seuil temporel n'est une assertion : ce banc reste manuel.
//! Il exerce le LTTB normal ; le témoin sans LTTB de l'expérience initiale
//! n'a pas été conservé comme option du renderer.

use std::hint::black_box;
use std::time::Instant;

use egui::epaint::Primitive;
use egui::{Pos2, Rect};
use plotters::drawing::IntoDrawingArea;
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{Column, Project};
use stokesforge_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::{decimate, pipeline, series};
use stokesforge_render_egui::{
    feathering, font_definitions, CacheKey, EguiBackend, Feathering, Framing, ScreenText,
    ShapeCache, TextMeasurer, ViewState,
};

const RECT: Rect = Rect {
    min: Pos2::ZERO,
    max: Pos2::new(800.0, 600.0),
};

fn banc(total: usize, count: usize) -> (Project, PlotSpec) {
    let mut project = Project::new("Mesure des courbes").unwrap();
    let book = project.add_empty_workbook(project.root(), None).unwrap();
    let sheet = project.add_worksheet(book, None).unwrap();
    let n = total / count;
    let xs: Vec<_> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&xs))
        .unwrap();
    let mut panel = PanelSpec::at(0, 0);
    for rank in 0..count {
        // Signal déterministe, ondulations rapides et lentes, toutes les
        // données visibles au cadrage initial. Les séries partagent X.
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
    // Sans textes riches : aucun coût Typst caché dans les mesures de courbes.
    (project, PlotSpec::empty("").with_panels(panel))
}

#[derive(Clone, Copy, Default)]
struct Mesure {
    pipeline: f64,
    image: f64,
    tessellation: f64,
    total: f64,
    octets: usize,
    retires: usize,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

struct Banc {
    context: egui::Context,
    text: TextMeasurer,
    cache: ShapeCache,
    view: ViewState,
}

impl Banc {
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
        frange: Feathering,
        chaud: bool,
    ) -> Mesure {
        feathering::apply(&self.context, frange);
        let key = CacheKey {
            revision: 1,
            rect: RECT,
            pixels_per_point: 1.0,
            view_generation: self.view.generation(),
        };
        let debut = Instant::now();
        let mut mesure = Mesure::default();
        let mut sortie = self.context.run_ui(
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
                        mesure.pipeline = ms(t);
                        mesure.retires = drawn.decimated_points;
                        Ok::<_, ()>(drawn)
                    })
                    .unwrap();
                assert_eq!(outcome.cached, chaud, "régime du cache incorrect");
                ui.painter()
                    .with_clip_rect(RECT)
                    .extend(self.cache.shapes().iter().cloned());
            },
        );
        mesure.image = ms(debut);
        let t = Instant::now();
        let primitives = self
            .context
            .tessellate(std::mem::take(&mut sortie.shapes), sortie.pixels_per_point);
        mesure.tessellation = ms(t);
        mesure.total = ms(debut);
        for p in &primitives {
            if let Primitive::Mesh(mesh) = &p.primitive {
                mesure.octets += mesh.vertices.len() * std::mem::size_of::<egui::epaint::Vertex>()
                    + mesh.indices.len() * std::mem::size_of::<u32>();
            }
        }
        black_box(&primitives);
        // Sans renderer natif, l'atlas n'est pas téléversé (#103).
        sortie.textures_delta.clear();
        mesure
    }
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs[xs.len() / 2]
}

fn rapport(total: usize, count: usize, scenario: &str, samples: &[Mesure]) {
    let m = |f: fn(&Mesure) -> f64| median(samples.iter().map(f).collect());
    println!(
        "IMAGE,{total},{count},true,{scenario},{:.3},{:.3},{:.3},{:.3},{:.3},{},{}",
        m(|s| s.pipeline),
        m(|s| s.image),
        m(|s| s.tessellation),
        m(|s| s.total),
        samples.iter().map(|s| s.total).fold(0.0, f64::max),
        samples[0].octets,
        samples[0].retires
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
#[ignore = "banc coûteux #141, à lancer en release avec --ignored --nocapture"]
fn mesure_courbes_gros_volumes() {
    if cfg!(debug_assertions) {
        panic!("les mesures exigent --release");
    }
    let iterations = std::env::var("STOKESFORGE_BENCH_FRAMES")
        .map(|s| s.parse().unwrap())
        .unwrap_or(5usize);
    assert!(iterations >= 3, "au moins trois images par régime");
    let volumes: Vec<usize> = std::env::var("STOKESFORGE_BENCH_POINTS")
        .map(|s| s.split(',').map(|n| n.parse().unwrap()).collect())
        .unwrap_or_else(|_| vec![100_000, 1_000_000, 10_000_000]);
    println!("IMAGE,points_total,series,lttb,scenario,pipeline_ms,image_ms,tessellation_ms,total_median_ms,total_max_ms,mesh_octets,points_retires");
    println!("PHASES,points_total,series,seuil,preparation_mediane_ms,lttb_isole_mediane_ms");
    for total in volumes {
        for count in [1, 10] {
            assert!(total / count >= 3 && total % count == 0);
            let (project, spec) = banc(total, count);
            let mut banc = Banc::new();
            // L'échauffement des polices et le premier cadre sont hors
            // statistiques ; seul le cache de FORMES est ensuite froid.
            banc.image(&project, &spec, Feathering::Full, false);
            {
                let plot = resolve_spec(project.tree(), &spec);
                let mut shapes = Vec::new();
                let drawn =
                    stokesforge_render_egui::collect_shapes(&mut shapes, RECT, &banc.text, &plot)
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
                "fenetre_chaud",
                "zoom_froid",
                "zoom_chaud",
                "pan_froid",
                "pan_chaud",
            ] {
                let mut samples = Vec::new();
                for frame in 0..iterations {
                    let froid = scenario.ends_with("froid");
                    if froid {
                        let step = frame as f64 * 0.002;
                        let x = if scenario.starts_with("zoom") {
                            (0.1 + step, 0.9 - step)
                        } else {
                            (0.1 + step, 0.9 + step)
                        };
                        banc.view.set_framing(0, 0, Framing { x, y: (-1.3, 1.3) });
                    }
                    let frange = if scenario.starts_with("pan") {
                        Feathering::Off
                    } else {
                        Feathering::Full
                    };
                    samples.push(banc.image(&project, &spec, frange, !froid));
                }
                rapport(total, count, scenario, &samples);
            }
        }
    }
}

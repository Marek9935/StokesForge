//! Bench of the server thread's lock (#475, step 1, lot L4): the real
//! application (`StokesForgeApp` behind `ServedApp`, the thread, the actor, the
//! autosave) on a project built here, driven without a person.
//!
//! Run by `tests/server_bench.rs` under a headless compositor; read its
//! doc for the criteria (M1 to M6). Environment:
//!
//! - `BENCH_SCENARIO`: `lock` (root visible, repainting at once, a recalculation
//!   and an import now and then: M1, M2), `rest` (root visible, nothing to do:
//!   M5), `hidden` (a second maximised process, or a minimised root, covers
//!   everything; edits, a recalculation and an import come from a bench thread
//!   that takes the lock like a hidden-window event would: M2, M3, M4, M5),
//!   `quit` (M6), `reattach` (#475, L4: a graph detached and maximised over the
//!   root, Reattach clicked from a bench thread; the window must close itself).
//! - `BENCH_SIZE`: `small` (about 10k cells) or `big` (1M rows x 10 columns).
//! - `BENCH_DIR`: scratch folder (preferences, recovery copies, import files).
//! - `BENCH_SECS`: duration. `BENCH_EDIT_AT`: seconds of the edits of `hidden`.
//! - `BENCH_MINIMISE=1`: the root minimises itself too (`hidden`).
//! - `STOKESFORGE_SERVER_STATS`: where the samples go (see `server_stats.rs`).
//!
//! Every line the bench itself prints starts with `bench: `; times are wall
//! clock microseconds since the Unix epoch, comparable with the samples file.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;
use stokesforge_app::app::{BenchState, PassHook, ServedApp};
use stokesforge_app_state::preferences::Preferences;
use stokesforge_app_state::state::{Action, AppState, ImportDestination};
use stokesforge_core::{
    Column, ColumnFormula, ColumnMeta, ColumnType, FormulaPart, NodeId, Project,
};
use stokesforge_session::Session;

fn wall_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn env_f64(name: &str, default: f64) -> f64 {
    env(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Column A (numbers), some data columns, and formula columns that read A, so
/// that an edit of A starts a full recalculation of all of them.
fn build_project(
    rows: usize,
    data: usize,
    formulas: usize,
    terms: usize,
) -> (Project, NodeId, NodeId) {
    let mut project = Project::new("Bench").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("book");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("Sheet1");
    let a: Vec<f64> = (0..rows).map(|r| r as f64).collect();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&a))
        .expect("A");
    let column_a = project.tree().children(sheet).expect("sheet")[0];
    let mut first_data = column_a;
    for c in 0..data {
        let v: Vec<f64> = (0..rows)
            .map(|r| ((r as f64) * 0.001 * (c + 1) as f64).sin() * 100.0)
            .collect();
        let id = project
            .add_column(sheet, None, Some(&format!("D{c}")), Column::from_f64(&v))
            .expect("data");
        if c == 0 {
            first_data = id;
        }
    }
    for index in 0..formulas {
        let column = project
            .add_column(
                sheet,
                None,
                Some(&format!("F{index}")),
                Column::new(ColumnType::Numeric),
            )
            .expect("formula column");
        let mut parts = vec![FormulaPart::Column(column_a)];
        for k in 0..terms {
            parts.push(FormulaPart::Text(format!("*{}+", k + 2)));
            parts.push(FormulaPart::Column(column_a));
        }
        parts.push(FormulaPart::Text("*3".into()));
        project
            .set_column_meta(
                column,
                ColumnMeta {
                    formula: Some(ColumnFormula::from_parts(parts)),
                    ..Default::default()
                },
            )
            .expect("formula");
    }
    (project, column_a, first_data)
}

fn write_csv(dir: &Path, files: usize, rows: usize) -> Vec<PathBuf> {
    (0..files)
        .map(|index| {
            let path = dir.join(format!("import{index}.csv"));
            let mut text = String::with_capacity(rows * 60);
            for r in 0..rows {
                for c in 0..10 {
                    if c > 0 {
                        text.push(',');
                    }
                    text.push_str(&format!("{:.4}", (r * (c + 1)) as f64 * 0.001));
                }
                text.push('\n');
            }
            std::fs::write(&path, text).expect("csv");
            path
        })
        .collect()
}

/// The root, around the application: counts its passes.
struct Bench {
    inner: ServedApp,
    passes: Arc<AtomicU64>,
    continuous: bool,
}

impl eframe::App for Bench {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.passes.fetch_add(1, Relaxed);
        if self.continuous {
            ui.ctx().request_repaint();
        }
        self.inner.ui(ui, frame);
    }

    fn on_exit(&mut self) {
        self.inner.on_exit();
    }
}

struct Plan {
    scenario: String,
    big: bool,
    secs: f64,
    dir: PathBuf,
    minimise: bool,
    edit_at: Vec<f64>,
}

fn main() -> eframe::Result {
    if env("BENCH_ROLE").is_some_and(|v| v == "cover") {
        return run_cover();
    }
    let plan = Plan {
        scenario: env("BENCH_SCENARIO").unwrap_or_else(|| "lock".into()),
        big: env("BENCH_SIZE").is_some_and(|v| v == "big"),
        secs: env_f64("BENCH_SECS", 30.0),
        dir: PathBuf::from(env("BENCH_DIR").expect("BENCH_DIR")),
        minimise: env("BENCH_MINIMISE").is_some_and(|v| v == "1"),
        edit_at: env("BENCH_EDIT_AT")
            .map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect())
            .unwrap_or_default(),
    };
    let (rows, data, formulas, terms) = if plan.big {
        (1_000_000, 4, 5, 6)
    } else {
        (1_000, 6, 3, 1)
    };
    let t = Instant::now();
    let (mut project, column_a, column_d) = build_project(rows, data, formulas, terms);
    let graph = (plan.scenario == "reattach").then(|| {
        let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
        panel
            .series
            .push(stokesforge_plot::SeriesSpec::new_xy(column_a, column_d));
        let root = project.root();
        project
            .add_graph(
                root,
                None,
                stokesforge_plot::PlotSpec::empty("Figure").with_panels(panel),
            )
            .expect("graph")
    });
    let session = Session::spawn(project, None).expect("session");
    println!(
        "bench: project rows={rows} columns={} built_ms={}",
        1 + data + formulas,
        t.elapsed().as_millis()
    );
    std::fs::create_dir_all(plan.dir.join("recovery")).expect("recovery dir");
    let prefs_file = plan.dir.join("preferences.json");
    std::fs::write(&prefs_file, r#"{"autosave_minutes":1}"#).expect("preferences");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        Preferences::load_from(Some(prefs_file)),
    )
    .expect("state");
    state.set_recovery_dir(Some(plan.dir.join("recovery")));
    state.set_quit_without_prompt(true);
    let imports = if plan.big {
        write_csv(&plan.dir, 3, 60_000)
    } else {
        write_csv(&plan.dir, 3, 200)
    };
    let handle = session.handle();
    let passes = Arc::new(AtomicU64::new(0));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("bench root")
            .with_inner_size([1100.0, 700.0]),
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration::default()
            .with_surface_config(eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY),
        ..Default::default()
    };
    let started = Instant::now();
    let passes_for_app = Arc::clone(&passes);
    let outcome = eframe::run_native(
        "stokesforge-server-bench",
        options,
        Box::new(move |cc| {
            cc.egui_ctx
                .set_fonts(stokesforge_render_egui::font_definitions());
            stokesforge_app::theme::install(&cc.egui_ctx);
            let mut app = stokesforge_app::app::StokesForgeApp::new(state);
            if let Some(graph) = graph {
                app.bench_detach_graph(graph);
            }
            let ctx = cc.egui_ctx.clone();
            let mut served = ServedApp::new(app, ctx.clone());
            let shared = served.shared_state();
            let waker = served.bench_waker();
            let end = Arc::new(AtomicBool::new(false));
            let continuous = matches!(plan.scenario.as_str(), "lock" | "quit" | "quit_idle");
            if let Some(graph) = graph {
                reattach_thread(
                    graph,
                    Arc::clone(&shared),
                    ctx.clone(),
                    Arc::clone(&passes_for_app),
                    started,
                );
            }
            served.set_pass_hook(pass_hook(
                &plan,
                column_a,
                column_d,
                imports.clone(),
                Arc::clone(&end),
                started,
            ));
            start_threads(
                &plan,
                column_a,
                column_d,
                imports,
                shared,
                waker,
                handle,
                ctx,
                end,
                Arc::clone(&passes_for_app),
                started,
            );
            Ok(Box::new(Bench {
                inner: served,
                passes: passes_for_app,
                continuous,
            }))
        }),
    );
    println!("bench: window_closed_us {}", wall_us());
    let shutdown = session.shutdown();
    println!("bench: exit_us {}", wall_us());
    println!("bench: root_passes_total {}", passes.load(Relaxed));
    outcome?;
    let _ = shutdown;
    Ok(())
}

/// What a gesture would do, in the root's own pass, with the lock held.
fn pass_hook(
    plan: &Plan,
    column_a: NodeId,
    column_d: NodeId,
    imports: Vec<PathBuf>,
    end: Arc<AtomicBool>,
    started: Instant,
) -> PassHook {
    let scenario = plan.scenario.clone();
    let minimise = plan.minimise;
    let mut recalcs = 0usize;
    let mut next_recalc = 3.0;
    let mut imported = false;
    let mut edited = false;
    let mut minimised = false;
    let mut quit_sent = false;
    let mut n = 0usize;
    Box::new(move |state: &mut AppState, ctx: &egui::Context| {
        let t = started.elapsed().as_secs_f64();
        if end.load(Relaxed) && !quit_sent {
            quit_sent = true;
            println!("bench: quit_request_us {}", wall_us());
            state.dispatch(Action::Quit {
                pending_edits: false,
            });
            return;
        }
        match scenario.as_str() {
            "lock" => {
                if t >= next_recalc && !end.load(Relaxed) {
                    next_recalc += 5.0;
                    recalcs += 1;
                    n += 1;
                    let t0 = Instant::now();
                    state.dispatch(Action::CommitCell {
                        column: column_a,
                        row: 5,
                        text: format!("{}.5", n),
                    });
                    println!(
                        "bench: gesture recalc #{recalcs} dispatch_ms={:.1}",
                        t0.elapsed().as_secs_f64() * 1000.0
                    );
                }
                if t >= 12.0 && !imported {
                    imported = true;
                    state.dispatch(Action::StartImport {
                        destination: ImportDestination::NewBookPerFile,
                        recipe: stokesforge_import::ImportRecipe::new("Serie"),
                        paths: imports.clone(),
                    });
                    println!("bench: gesture import started");
                }
            }
            "quit" => {
                if t >= 2.0 && !edited {
                    edited = true;
                    state.dispatch(Action::CommitCell {
                        column: column_d,
                        row: 7,
                        text: "1.5".into(),
                    });
                }
                if !quit_sent && stokesforge_app::app::bench_write_in_flight() {
                    quit_sent = true;
                    println!("bench: quit_request_us {}", wall_us());
                    state.dispatch(Action::Quit {
                        pending_edits: false,
                    });
                }
            }
            "quit_idle" if t >= 5.0 && !quit_sent => {
                quit_sent = true;
                println!("bench: quit_request_us {}", wall_us());
                state.dispatch(Action::Quit {
                    pending_edits: false,
                });
            }
            "hidden" if minimise && t >= 3.0 && !minimised => {
                minimised = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            _ => {}
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn start_threads(
    plan: &Plan,
    column_a: NodeId,
    column_d: NodeId,
    imports: Vec<PathBuf>,
    shared: Arc<BenchState>,
    waker: Arc<dyn Fn() + Send + Sync>,
    handle: stokesforge_session::SessionHandle,
    ctx: egui::Context,
    end: Arc<AtomicBool>,
    passes: Arc<AtomicU64>,
    started: Instant,
) {
    let secs = plan.secs;
    let scenario = plan.scenario.clone();
    let recovery = plan.dir.join("recovery");
    // The end of the run.
    {
        let (end, ctx, scenario) = (Arc::clone(&end), ctx.clone(), scenario.clone());
        let passes = Arc::clone(&passes);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(secs));
            println!("bench: root_passes_at_end {}", passes.load(Relaxed));
            if scenario == "hidden" {
                // Nothing paints: end it from here, samples first.
                stokesforge_app::app::bench_flush_stats();
                println!("bench: end_us {}", wall_us());
                if let Some(mut cover) = COVER.lock().take() {
                    let _ = cover.kill();
                    let _ = cover.wait();
                }
                std::process::exit(0);
            }
            end.store(true, Relaxed);
            ctx.request_repaint();
        });
    }
    if matches!(
        scenario.as_str(),
        "quit" | "quit_idle" | "lock" | "reattach"
    ) {
        return;
    }
    if scenario == "rest" {
        // Passes counted over 5 s once the start-up has settled.
        let passes = Arc::clone(&passes);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(6));
            let a = passes.load(Relaxed);
            let t0 = wall_us();
            std::thread::sleep(Duration::from_secs(5));
            let b = passes.load(Relaxed);
            println!(
                "bench: rest_window_start_us {t0} rest_window_end_us {} root_passes_in_window {}",
                wall_us(),
                b - a
            );
        });
        return;
    }
    // `hidden`: the cover process, the root's passes, the slot file, the edits.
    {
        let passes = Arc::clone(&passes);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            if env("BENCH_MINIMISE").is_none_or(|v| v != "1") {
                if let Ok(exe) = std::env::current_exe() {
                    let child = std::process::Command::new(exe)
                        .env("BENCH_ROLE", "cover")
                        .stdout(std::process::Stdio::null())
                        .spawn();
                    if let Ok(child) = child {
                        *COVER.lock() = Some(child);
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(6));
            println!(
                "bench: hidden_from_us {} root_passes_at_hidden {}",
                wall_us(),
                passes.load(Relaxed)
            );
        });
    }
    {
        let recovery = recovery.clone();
        std::thread::spawn(move || {
            let mut last: Option<SystemTime> = None;
            loop {
                std::thread::sleep(Duration::from_millis(50));
                let newest = std::fs::read_dir(&recovery)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "stf"))
                    .filter_map(|e| e.metadata().ok()?.modified().ok())
                    .max();
                if newest.is_some() && newest != last {
                    last = newest;
                    let m = newest
                        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_micros());
                    println!("bench: slot_mtime observed_us {} mtime_us {m}", wall_us());
                }
            }
        });
    }
    // The edits (a script's or an import's commands would do the same).
    {
        let shared = Arc::clone(&shared);
        let waker = Arc::clone(&waker);
        let edit_at = plan.edit_at.clone();
        std::thread::spawn(move || {
            for (i, at) in edit_at.iter().enumerate() {
                let wait = *at - started.elapsed().as_secs_f64();
                if wait > 0.0 {
                    std::thread::sleep(Duration::from_secs_f64(wait));
                }
                shared.lock().dispatch(Action::CommitCell {
                    column: column_d,
                    row: 10 + i,
                    text: format!("{i}.25"),
                });
                waker();
                println!("bench: edit #{i} us {}", wall_us());
            }
        });
    }
    // A recalculation that outlasts the command wait, its true end, an import.
    {
        let shared = Arc::clone(&shared);
        let waker = Arc::clone(&waker);
        let at = env_f64("BENCH_RECALC_AT", 20.0);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(
                (at - started.elapsed().as_secs_f64()).max(0.0),
            ));
            let t0 = wall_us();
            shared.lock().dispatch(Action::CommitCell {
                column: column_a,
                row: 3,
                text: "3.75".into(),
            });
            waker();
            println!("bench: recalc_dispatched_us {t0} returned_us {}", wall_us());
            let mut seen = false;
            loop {
                if handle.recalc_running().is_some() {
                    seen = true;
                } else if seen {
                    println!("bench: recalc_true_end_us {}", wall_us());
                    break;
                } else if wall_us() - t0 > 2_000_000 {
                    println!("bench: recalc_not_seen (finished inside the wait)");
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
    }
    {
        let at = env_f64("BENCH_IMPORT_AT", 30.0);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(
                (at - started.elapsed().as_secs_f64()).max(0.0),
            ));
            println!("bench: import_started_us {}", wall_us());
            shared.lock().dispatch(Action::StartImport {
                destination: ImportDestination::NewBookPerFile,
                recipe: stokesforge_import::ImportRecipe::new("Serie"),
                paths: imports,
            });
            waker();
        });
    }
}

/// `reattach` (#475, L4): the detached graph is maximised over the root, then
/// "Reattach" is clicked from here. The window must close itself in its own
/// pass: the tab floats in the main window, and the root, covered, paints
/// nothing meanwhile (its passes are counted).
fn reattach_thread(
    graph: NodeId,
    shared: Arc<BenchState>,
    ctx: egui::Context,
    passes: Arc<AtomicU64>,
    started: Instant,
) {
    std::thread::spawn(move || {
        let id = stokesforge_app::app::StokesForgeApp::bench_graph_viewport(graph);
        std::thread::sleep(Duration::from_secs_f64(
            (4.0 - started.elapsed().as_secs_f64()).max(0.0),
        ));
        println!("bench: reattach before {:?}", shared.graph_places(graph));
        ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Maximized(true));
        ctx.request_repaint_of(id);
        std::thread::sleep(Duration::from_secs(2));
        let maximised = ctx.viewport_for(id, |viewport| viewport.input.viewport().maximized);
        // Let a covered root's own throttled passes settle, then count.
        let root_before = passes.load(Relaxed);
        let click_us = wall_us();
        shared.click_reattach(graph);
        ctx.request_repaint_of(id);
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut gone_us = None;
        while Instant::now() < deadline {
            if !shared.graph_places(graph).0 {
                gone_us = Some(wall_us());
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let root_at_gone = passes.load(Relaxed);
        println!(
            "bench: reattach maximised={maximised:?} click_to_gone_us {} root_ui_delta_until_gone {}",
            gone_us.map_or(-1, |gone| (gone - click_us) as i64),
            root_at_gone - root_before
        );
        std::thread::sleep(Duration::from_millis(1500));
        println!(
            "bench: reattach after {:?} root_ui_delta_after_uncovered {}",
            shared.graph_places(graph),
            passes.load(Relaxed) - root_at_gone
        );
    });
}

/// The cover process, killed when the bench ends by itself.
static COVER: parking_lot::Mutex<Option<std::process::Child>> = parking_lot::Mutex::new(None);

struct CoverApp;

fn started() -> Instant {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *START.get_or_init(Instant::now)
}

impl eframe::App for CoverApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        });
        ui.label("cover");
        // Never outlive the bench by long, whatever happens to its parent.
        if started().elapsed() > Duration::from_secs(900) {
            std::process::exit(0);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

/// The second maximised process that hides every window of the application.
fn run_cover() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("bench cover")
            .with_inner_size([1300.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "stokesforge-server-bench-cover",
        options,
        Box::new(|_cc| Ok(Box::new(CoverApp))),
    )
}

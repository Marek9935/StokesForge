//! Probe for #457: does the root window keep running when a detached window
//! covers it on native Wayland?
//!
//! A root window and one deferred child viewport (no decorations, opaque, like
//! `show_detached_tabs`). The child maximises itself, so it fully covers the
//! root, then queues a request every 200 ms into a shared counter and wakes the
//! root with `request_repaint_of(ROOT)`, as a detached window does. The root
//! drains the counter in `ui`. The child keeps repainting itself, so the
//! compositor keeps sending it frame callbacks.
//!
//! The child prints the verdict (the root may be stalled, so it cannot) and
//! exits: code 0 when the root drained most requests, 1 when it did not. With
//! `PROBE_IDLE=1` nothing is queued after the settle and the exit code checks
//! that the root stays asleep (rule #104: no periodic wake at rest).
//!
//! Run it through the headless compositor with
//! `cargo test -p stokesforge-app --test wayland_covered_root -- --ignored`,
//! or by hand inside a native Wayland session (not `STOKESFORGE_X11`). Under
//! the headless mutter it needs `WGPU_BACKEND=gl`: on the Vulkan backend that
//! compositor stops answering frame callbacks to any window a few seconds in,
//! covered or not, which hides what this probe measures.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use eframe::egui;

static ROOT_UI: AtomicU64 = AtomicU64::new(0);
static QUEUED: AtomicU64 = AtomicU64::new(0);
static PENDING: AtomicU64 = AtomicU64::new(0);
static DRAINED: AtomicU64 = AtomicU64::new(0);
static CHILD_PASSES: AtomicU64 = AtomicU64::new(0);
// Two-children mode (`PROBE_TWO=1`): child A is small and repaints itself; child B
// maximises over everything, so A is fully covered. What the root read of A.
static A_SEEN: AtomicU64 = AtomicU64::new(0);
static A_OCCLUDED: AtomicU64 = AtomicU64::new(0);
static A_PASSES: AtomicU64 = AtomicU64::new(0);

const SETTLE: Duration = Duration::from_secs(4);
const WINDOW: Duration = Duration::from_secs(5);
const PERIOD: Duration = Duration::from_millis(200);

struct Root;

impl eframe::App for Root {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // As the application does on its first root pass (#457).
        // Wayland only: the example still has to compile on every platform,
        // since `cargo test --features python` builds examples (#469).
        #[cfg(target_os = "linux")]
        if let Some(window) = frame.winit_window() {
            use winit::platform::wayland::WindowExtWayland as _;
            window.set_reports_occlusion(true);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = frame;
        start();
        ROOT_UI.fetch_add(1, Relaxed);
        DRAINED.fetch_add(PENDING.swap(0, Relaxed), Relaxed);
        ui.label("root");
        let ctx = ui.ctx().clone();
        if two_children() {
            let a = egui::ViewportId::from_hash_of("child-a");
            if let Some(info) = ctx.input(|i| i.raw.viewports.get(&a).cloned()) {
                A_SEEN.fetch_add(1, Relaxed);
                if info.occluded == Some(true) {
                    A_OCCLUDED.fetch_add(1, Relaxed);
                }
            }
            ctx.show_viewport_deferred(
                a,
                egui::ViewportBuilder::default()
                    .with_title("probe child A")
                    .with_decorations(false)
                    .with_inner_size([200.0, 150.0]),
                child_a,
            );
            // B appears once A is up, so that B is the one on top.
            if start().elapsed() < Duration::from_secs(2) {
                ctx.request_repaint_after(Duration::from_millis(500));
                return;
            }
        }
        // The detached window, republished every root pass as the app does.
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("child"),
            egui::ViewportBuilder::default()
                .with_title("probe child")
                .with_decorations(false)
                .with_inner_size([300.0, 200.0]),
            child_ui,
        );
    }
}

fn two_children() -> bool {
    std::env::var_os("PROBE_TWO").is_some_and(|v| v == "1")
}

fn start() -> Instant {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn child_a(ui: &mut egui::Ui, _class: egui::ViewportClass) {
    let passes = A_PASSES.fetch_add(1, Relaxed) + 1;
    ui.label(format!("child A pass {passes}"));
    ui.ctx().request_repaint_after(Duration::from_millis(50));
}

fn cpu_ticks() -> u64 {
    // utime + stime of /proc/self/stat (fields 14 and 15), in 1/100 s.
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after = stat.rsplit(')').next().unwrap_or("");
    let f: Vec<&str> = after.split_whitespace().collect();
    let get = |i: usize| f.get(i).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    get(11) + get(12)
}

struct ChildClock {
    start: Instant,
    last_queue: Instant,
    at_window_start: Option<(u64, u64, u64)>,
}

fn child_ui(ui: &mut egui::Ui, _class: egui::ViewportClass) {
    use std::sync::Mutex;
    static CLOCK: Mutex<Option<ChildClock>> = Mutex::new(None);
    let idle = std::env::var_os("PROBE_IDLE").is_some_and(|v| v == "1");
    let ctx = ui.ctx().clone();
    let passes = CHILD_PASSES.fetch_add(1, Relaxed) + 1;
    let mut guard = CLOCK.lock().unwrap();
    let clock = guard.get_or_insert_with(|| {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        ChildClock {
            start: Instant::now(),
            last_queue: Instant::now(),
            at_window_start: None,
        }
    });
    let elapsed = clock.start.elapsed();
    ui.label(format!("child pass {passes}"));
    if elapsed < SETTLE {
        ctx.request_repaint_after(Duration::from_millis(50));
        return;
    }
    let baseline = *clock.at_window_start.get_or_insert((
        ROOT_UI.load(Relaxed),
        DRAINED.load(Relaxed),
        cpu_ticks(),
    ));
    if elapsed < SETTLE + WINDOW {
        if idle {
            ctx.request_repaint_after(WINDOW);
        } else {
            if clock.last_queue.elapsed() >= PERIOD {
                clock.last_queue = Instant::now();
                QUEUED.fetch_add(1, Relaxed);
                PENDING.fetch_add(1, Relaxed);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        return;
    }
    let root_ui = ROOT_UI.load(Relaxed) - baseline.0;
    let drained = DRAINED.load(Relaxed) - baseline.1;
    let cpu_ms = (cpu_ticks() - baseline.2) * 10;
    let queued = QUEUED.load(Relaxed);
    let maximized = ctx.input(|i| i.viewport().maximized);
    println!(
        "probe: mode={} root_ui={root_ui} queued={queued} drained={drained} \
         child_passes={passes} maximized={maximized:?} cpu_ms={cpu_ms}",
        if idle { "idle" } else { "active" }
    );
    let a_seen = A_SEEN.load(Relaxed);
    let a_occluded = A_OCCLUDED.load(Relaxed);
    if two_children() {
        println!(
            "probe: child A passes={} seen_by_root={a_seen} occluded_true={a_occluded}",
            A_PASSES.load(Relaxed)
        );
    }
    let ok = if two_children() {
        // A covered child must not be reported occluded (it would lose its input),
        // and the root must keep draining what B asks for.
        a_seen >= 5 && a_occluded == 0 && queued >= 15 && drained * 10 >= queued * 8
    } else if idle {
        root_ui <= 20
    } else {
        // A child that passed so rarely it queued next to nothing proves nothing.
        queued >= 15 && drained * 10 >= queued * 8
    };
    println!("probe: {}", if ok { "PASS" } else { "FAIL: root stalled" });
    std::process::exit(if ok { 0 } else { 1 });
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("probe root")
            .with_inner_size([400.0, 300.0]),
        // Same surface configuration as the application (main.rs).
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration::default()
            .with_surface_config(eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY),
        ..Default::default()
    };
    eframe::run_native(
        "wayland-covered-root",
        options,
        Box::new(|_cc| Ok(Box::new(Root))),
    )
}

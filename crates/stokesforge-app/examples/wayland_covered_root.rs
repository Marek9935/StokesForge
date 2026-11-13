//! Probe for #457 and #475: does a window that covers the root keep working
//! in its own passes on native Wayland, with the root left alone?
//!
//! A root window and one deferred child viewport (no decorations, opaque, like
//! the detached windows). The child maximises itself, so it fully covers the
//! root. One state sits behind one `Arc<Mutex>`: the child locks it in its own
//! closure, applies the gestures there and paints the result in the same pass,
//! so the root has nothing to do. A helper thread plays the input events: it
//! queues a gesture every 200 ms and wakes only the child. Verdict: the root `ui`
//! count stays flat (<= 2) while drained == queued, and the gesture is applied
//! within 2 child passes. With `PROBE_IDLE=1` nothing is queued and the exit code
//! checks that the root stays asleep (rule #104: no periodic wake at rest).
//!
//! `PROBE_REATTACH=1` then makes the child close itself (it registered itself as
//! its own parent in its own pass; the root registers it only until the first
//! child pass) and checks that the window is gone with no root pass, and that the
//! root is painted within 500 ms of the wake. `PROBE_COVER=1` spawns a second
//! maximised process (`PROBE_ROLE=cover`) on top of everything and reports what
//! still runs when all windows are hidden (informative, always exit 0).
//!
//! Knobs: `PROBE_ROOT_CLOSE=1` (control: the root, not the child, drops the
//! child), `PROBE_NO_ROOT_WAKE=1`, `PROBE_NO_MAX=1` (child not maximised). Under
//! X11 run it with `WAYLAND_DISPLAY` unset and a `DISPLAY` (`PROBE_X11=1` does
//! this in the test file, through mutter's Xwayland).
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
static DRAINED: AtomicU64 = AtomicU64::new(0);
static CHILD_PASSES: AtomicU64 = AtomicU64::new(0);
static APPLIED: AtomicU64 = AtomicU64::new(0);
static GONE_AT_MS: AtomicU64 = AtomicU64::new(0);
static ROOT_AFTER_MS: AtomicU64 = AtomicU64::new(0);
static CLOSE_AT_MS: AtomicU64 = AtomicU64::new(0);
static ROOT_UI_AT_CLOSE: AtomicU64 = AtomicU64::new(0);
static CHILD_IN_VIEWPORTS_AFTER_CLOSE: AtomicU64 = AtomicU64::new(0);

const SETTLE: Duration = Duration::from_secs(4);
const WINDOW: Duration = Duration::from_secs(5);
const PERIOD: Duration = Duration::from_millis(200);

struct Root;

impl eframe::App for Root {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        start();
        ROOT_UI.fetch_add(1, Relaxed);
        let ctx = ui.ctx().clone();
        {
            ui.label("root");
            if GONE_AT_MS.load(Relaxed) != 0 {
                // After the close: record when the root is painted again and
                // whether egui still lists the child.
                let _ = ROOT_AFTER_MS.compare_exchange(0, now_ms(), Relaxed, Relaxed);
                let id = egui::ViewportId::from_hash_of("child");
                // The viewport map of the input is updated when the window is dropped:
                // read it only a while after the child is gone.
                if now_ms() >= GONE_AT_MS.load(Relaxed) + 300 {
                    LATE_ROOT_PASSES.fetch_add(1, Relaxed);
                    // A listed entry with a size is a live native window; one without
                    // (never initialised, default title) is an empty egui entry that a
                    // delayed repaint request addressed to the removed id re-creates.
                    match ctx.input(|i| i.raw.viewports.get(&id).cloned()) {
                        Some(info) if info.inner_rect.is_some() => {
                            CHILD_IN_VIEWPORTS_AFTER_CLOSE.fetch_add(1, Relaxed);
                        }
                        Some(_) => {
                            EMPTY_ENTRY_AFTER_CLOSE.fetch_add(1, Relaxed);
                        }
                        None => {}
                    }
                }
            }
            // The root registers the child only until the child has had a pass;
            // after that the child registers itself (Reattach candidate), or, without
            // PROBE_REATTACH, the root keeps publishing it as the app does.
            let registers = CHILD_PASSES.load(Relaxed) == 0 || !reattach() || root_close();
            if registers && CLOSE_AT_MS.load(Relaxed) == 0 {
                let keep = root_close().then(flag);
                ctx.show_viewport_deferred(child_id(), child_builder(), move |ui, class| {
                    let _keep = &keep;
                    child_single(ui, class);
                });
            }
            if PROBE_COVER_ROLE_WAKE.load(Relaxed) == 0 {
                PROBE_COVER_ROLE_WAKE.store(1, Relaxed);
                start_cover_if_asked();
            }
        }
    }
}

static PROBE_COVER_ROLE_WAKE: AtomicU64 = AtomicU64::new(0);

fn reattach() -> bool {
    std::env::var_os("PROBE_REATTACH").is_some_and(|v| v == "1")
}

/// Control: the classic path, the root stops publishing the child (PROBE_ROOT_CLOSE=1).
fn root_close() -> bool {
    std::env::var_os("PROBE_ROOT_CLOSE").is_some_and(|v| v == "1")
}

fn cover() -> bool {
    std::env::var_os("PROBE_COVER").is_some_and(|v| v == "1")
}

fn now_ms() -> u64 {
    start().elapsed().as_millis() as u64 + 1
}

fn child_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("child")
}

fn child_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("probe child")
        .with_decorations(false)
        .with_inner_size([300.0, 200.0])
}

/// With `PROBE_COVER=1`, a second process of this example, role `cover`, a
/// maximised window that repaints itself, opened after the child is up.
fn start_cover_if_asked() {
    if !cover() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let child = std::process::Command::new(exe)
            .env("PROBE_ROLE", "cover")
            .env("PROBE_COVER_DELAY_MS", "1500")
            .stdout(std::process::Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            // Reap it when this process exits; a detached waiter thread.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            COVER_PID.store(0, Relaxed);
        }
    }
}
static COVER_PID: AtomicU64 = AtomicU64::new(0);

struct CoverApp;

impl eframe::App for CoverApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        });
        static PASSES: AtomicU64 = AtomicU64::new(0);
        let n = PASSES.fetch_add(1, Relaxed) + 1;
        if n == 1 || n.is_multiple_of(120) {
            let maxi = ui.ctx().input(|i| i.viewport().maximized);
            eprintln!("probe-cover: pass {n} maximized={maxi:?}");
        }
        ui.label("cover");
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        // Safety net: never outlive the probe by more than a minute.
        if start().elapsed() > Duration::from_secs(60) {
            std::process::exit(0);
        }
    }
}

/// Guard whose drop says egui released the child's closure (viewport removed).
struct DropFlag;
impl Drop for DropFlag {
    fn drop(&mut self) {
        ROOT_UI_AT_GONE.store(ROOT_UI.load(Relaxed), Relaxed);
        let _ = GONE_AT_MS.compare_exchange(0, now_ms(), Relaxed, Relaxed);
    }
}

struct Shared {
    pending: Vec<(Instant, u64)>,
    applied: u64,
    latencies_ms: Vec<f64>,
    max_frames: u64,
}

fn shared() -> std::sync::Arc<std::sync::Mutex<Shared>> {
    static S: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Shared>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| {
        std::sync::Arc::new(std::sync::Mutex::new(Shared {
            pending: Vec::new(),
            applied: 0,
            latencies_ms: Vec::new(),
            max_frames: 0,
        }))
    })
    .clone()
}

fn flag() -> std::sync::Arc<DropFlag> {
    // Only a Weak is kept here, so the guard drops with the last closure egui holds.
    static F: std::sync::Mutex<std::sync::Weak<DropFlag>> =
        std::sync::Mutex::new(std::sync::Weak::new());
    let mut weak = F.lock().unwrap();
    weak.upgrade().unwrap_or_else(|| {
        let arc = std::sync::Arc::new(DropFlag);
        *weak = std::sync::Arc::downgrade(&arc);
        arc
    })
}

/// The child of the single-state mode: lock, apply, paint, all in this pass.
fn child_single(ui: &mut egui::Ui, _class: egui::ViewportClass) {
    use std::sync::atomic::AtomicBool;
    static GENERATING: AtomicBool = AtomicBool::new(false);
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let ctx = ui.ctx().clone();
    let passes = CHILD_PASSES.fetch_add(1, Relaxed) + 1;
    let t0 = *T0.get_or_init(|| {
        if std::env::var_os("PROBE_NO_MAX").is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
        Instant::now()
    });
    let elapsed = t0.elapsed();
    let closing = CLOSE_AT_MS.load(Relaxed) != 0;
    if reattach() && !closing && !root_close() {
        // Reattach candidate: re-register as my own parent, in my own pass.
        let keep = flag();
        ctx.show_viewport_deferred(child_id(), child_builder(), move |ui, class| {
            let _keep = &keep;
            child_single(ui, class);
        });
    } else if !reattach() {
        // The root republishes it; hold the guard in this closure's lifetime only.
    }
    // Apply what is queued, under the one lock, then paint the result.
    let (applied, shown) = {
        let state = shared();
        let mut s = state.lock().unwrap();
        let pending = std::mem::take(&mut s.pending);
        for (at, passes_at_gen) in pending {
            s.applied += 1;
            DRAINED.fetch_add(1, Relaxed);
            s.latencies_ms.push(at.elapsed().as_secs_f64() * 1000.0);
            s.max_frames = s.max_frames.max(passes - passes_at_gen);
        }
        (s.applied, s.applied)
    };
    ui.label(format!(
        "child pass {passes} applied {applied} shown {shown}"
    ));
    APPLIED.store(applied, Relaxed);
    if closing {
        return;
    }
    if elapsed < SETTLE {
        ctx.request_repaint_after(Duration::from_millis(50));
        return;
    }
    let baseline = (BASE.get_or_init(|| {
        (
            ROOT_UI.load(Relaxed),
            cpu_ticks(),
            passes,
            std::time::Instant::now(),
        )
    }))
    .to_owned();
    if !GENERATING.swap(true, Relaxed) {
        let idle = std::env::var_os("PROBE_IDLE").is_some_and(|v| v == "1");
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            // Plays the input events: queue a gesture, wake the child window only.
            let end = Instant::now() + WINDOW;
            while Instant::now() < end && !idle {
                std::thread::sleep(PERIOD);
                let state = shared();
                state
                    .lock()
                    .unwrap()
                    .pending
                    .push((Instant::now(), CHILD_PASSES.load(Relaxed)));
                QUEUED.fetch_add(1, Relaxed);
                ctx2.request_repaint_of(child_id());
                if cover() {
                    // Hidden case: also ask the root for a pass (autosave, jobs).
                    ctx2.request_repaint_of(egui::ViewportId::ROOT);
                }
            }
        });
    }
    if elapsed < SETTLE + WINDOW + Duration::from_millis(600) {
        ctx.request_repaint_after(Duration::from_millis(100));
        return;
    }
    let root_ui = ROOT_UI.load(Relaxed) - baseline.0;
    let cpu_ms = (cpu_ticks() - baseline.1) * 10;
    let queued = QUEUED.load(Relaxed);
    let drained = DRAINED.load(Relaxed);
    let (max_frames, p50, max_ms) = {
        let state = shared();
        let s = state.lock().unwrap();
        let mut l = s.latencies_ms.clone();
        l.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (
            s.max_frames,
            l.get(l.len() / 2).copied().unwrap_or(0.0),
            l.last().copied().unwrap_or(0.0),
        )
    };
    println!(
        "probe: root_ui={root_ui} queued={queued} drained={drained} \
         child_passes={} max_frames_to_apply={max_frames} latency_ms_p50={p50:.1} \
         latency_ms_max={max_ms:.1} cpu_ms={cpu_ms}",
        passes - baseline.2
    );
    if cover() {
        // Informative: what still runs when every window is hidden.
        println!(
            "probe: hidden-case: root woken {queued} times, root_ui={root_ui} (root served \
             when hidden: {}); not a pass/fail",
            root_ui + 2 >= queued
        );
        std::process::exit(0);
    }
    if !reattach() {
        let idle = std::env::var_os("PROBE_IDLE").is_some_and(|v| v == "1");
        let ok = if idle {
            // At rest nothing wakes the root (#104).
            root_ui <= 2
        } else {
            queued >= 15 && drained == queued && root_ui <= 2 && max_frames <= 2
        };
        println!("probe: {}", if ok { "PASS" } else { "FAIL: see numbers" });
        std::process::exit(if ok { 0 } else { 1 });
    }
    // Reattach: close myself. Stop re-registering from the next pass on, with the
    // root left alone: the root is woken only once the window is gone.
    ROOT_UI_AT_CLOSE.store(ROOT_UI.load(Relaxed), Relaxed);
    CLOSE_AT_MS.store(now_ms(), Relaxed);
    ctx.request_repaint_of(child_id());
    if root_close() {
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_millis(2000);
        while GONE_AT_MS.load(Relaxed) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let gone = GONE_AT_MS.load(Relaxed);
        let close = CLOSE_AT_MS.load(Relaxed);
        let root_ui_delta = ROOT_UI_AT_GONE
            .load(Relaxed)
            .saturating_sub(ROOT_UI_AT_CLOSE.load(Relaxed));
        // Ask for the root pass as the app does after a Reattach, and again 400 ms
        // later so that a root pass is certain to read the viewport map late.
        let wake = std::env::var_os("PROBE_NO_ROOT_WAKE").is_none();
        if wake {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        std::thread::sleep(Duration::from_millis(400));
        if wake {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        std::thread::sleep(Duration::from_millis(600));
        let root_after = ROOT_AFTER_MS.load(Relaxed);
        let still = CHILD_IN_VIEWPORTS_AFTER_CLOSE.load(Relaxed);
        let late = LATE_ROOT_PASSES.load(Relaxed);
        let gone_dt = (gone != 0).then(|| gone as i64 - close as i64);
        let root_dt = (root_after != 0 && gone != 0).then(|| root_after as i64 - gone as i64);
        println!(
            "probe: reattach gone_after_ms={gone_dt:?} root_ui_delta_until_gone={root_ui_delta} \
             root_painted_ms_after_gone={root_dt:?} late_root_passes={late} live_window_listed_by_root={still} empty_entry_listed={}",
            EMPTY_ENTRY_AFTER_CLOSE.load(Relaxed)
        );
        let ok = gone != 0
            && root_ui_delta == 0
            && root_dt.is_some_and(|d| d <= 500)
            && late >= 1
            && still == 0;
        println!(
            "probe: {}",
            if ok {
                "PASS"
            } else {
                "FAIL: not closed by itself"
            }
        );
        std::process::exit(if ok { 0 } else { 1 });
    });
}

static EMPTY_ENTRY_AFTER_CLOSE: AtomicU64 = AtomicU64::new(0);
static LATE_ROOT_PASSES: AtomicU64 = AtomicU64::new(0);
static ROOT_UI_AT_GONE: AtomicU64 = AtomicU64::new(0);

static BASE: std::sync::OnceLock<(u64, u64, u64, Instant)> = std::sync::OnceLock::new();

fn start() -> Instant {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn cpu_ticks() -> u64 {
    // utime + stime of /proc/self/stat (fields 14 and 15), in 1/100 s.
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after = stat.rsplit(')').next().unwrap_or("");
    let f: Vec<&str> = after.split_whitespace().collect();
    let get = |i: usize| f.get(i).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    get(11) + get(12)
}

fn main() -> eframe::Result {
    if std::env::var("PROBE_ROLE").is_ok_and(|v| v == "cover") {
        let delay = std::env::var("PROBE_COVER_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        std::thread::sleep(Duration::from_millis(delay));
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("probe cover")
                .with_inner_size([1300.0, 800.0]),
            ..Default::default()
        };
        return eframe::run_native(
            "wayland-covered-root-cover",
            options,
            Box::new(|_cc| Ok(Box::new(CoverApp))),
        );
    }
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

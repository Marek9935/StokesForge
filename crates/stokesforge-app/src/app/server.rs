//! The server thread (#475): one background thread that owns the follow-up of
//! the work that does not depend on painting (autosave, running scripts,
//! recalculations, the progress of an import), over **one** [`AppState`]
//! behind **one** lock.
//!
//! # The lock
//!
//! The state lives in an `Arc<Mutex<AppState>>`. [`StateSlot`] is the UI
//! thread's side: it owns the guard and dereferences to the state, so every
//! `self.state.…` of [`StokesForgeApp`](super::StokesForgeApp) reads as before.
//! The guard is held from `new()` on (a test that drives `ui` or `logic`
//! directly runs on the slot as it is), and [`ServedApp`] — the `eframe::App`
//! the binary runs — takes it at the start of `logic`, `ui` and `on_exit` and
//! gives it back at their end. Between two passes the lock is free, and that is
//! when the thread works.
//!
//! Rules held by `tests/guards.rs` and by the tests below:
//!
//! - the thread only ever `try_lock`s: it never waits for the UI (busy: it
//!   looks again 10 ms later). The UI side may wait: the thread holds the lock
//!   for a tick only;
//! - nothing heavy under the lock. The blocking write of the autosave runs with
//!   the lock **released** ([`serve_once`]: decide under the lock, write
//!   without it, record under the lock);
//! - the thread executes no command (invariant 1) and never drops a script
//!   engine (invariant 9: a finished run only moves it back into the state);
//! - one rule to wake the windows: [`wake`], and only when the tick says that
//!   something they show changed. Focus plays no part;
//! - on exit the UI releases the guard, **then** joins the thread
//!   ([`ServerThread::shut_down`]), before the session is shut down.
//!
//! What stays in the paint pass: applying a finished import (commands),
//! the busy-indicator repaints, everything that needs `Frame` or `Context`.
//!
//! Detached windows keep their fork (`window_detach.rs`): this thread is step 1
//! of #475 and changes nothing they see.
//!
//! # Without the thread
//!
//! A bench or a test has no thread, and a thread that ended must not freeze the
//! application: while [`StokesForgeApp::server_running`] is false the paint
//! pass does the polling itself (`poll_without_server`), as before #475.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui;
use parking_lot::lock_api::ArcMutexGuard;
use parking_lot::{Mutex, RawMutex};
use stokesforge_app_state::state::{AppState, AutosaveJob};
use stokesforge_session::SessionError;

use super::StokesForgeApp;

/// Retry delay when the UI holds the lock.
const BUSY_RETRY: Duration = Duration::from_millis(10);

/// How long the exit waits for the thread (an autosave write in flight).
const SHUTDOWN_WAIT: Duration = Duration::from_secs(30);

type SharedState = Arc<Mutex<AppState>>;
type StateGuard = ArcMutexGuard<RawMutex, AppState>;

/// The UI thread's side of the shared state: the owned guard, dereferencing to
/// the [`AppState`].
pub(super) struct StateSlot {
    /// Declared first: dropped before the `Arc`.
    guard: Option<StateGuard>,
    shared: SharedState,
}

impl StateSlot {
    /// Puts `state` behind the lock, held.
    pub(super) fn new(state: AppState) -> Self {
        let shared = Arc::new(Mutex::new(state));
        Self {
            guard: Some(shared.lock_arc()),
            shared,
        }
    }

    /// The state back, for a test that rebuilds an app on it. No thread may
    /// share the lock.
    #[cfg(test)]
    pub(super) fn into_state(mut self) -> AppState {
        self.guard = None;
        match Arc::try_unwrap(self.shared) {
            Ok(mutex) => mutex.into_inner(),
            Err(_) => unreachable!("a thread still shares the state"),
        }
    }

    /// The lock, for the thread.
    pub(super) fn shared(&self) -> SharedState {
        Arc::clone(&self.shared)
    }

    /// Takes the lock for a pass (waits for the thread's tick, which is short).
    pub(super) fn acquire(&mut self) {
        if self.guard.is_none() {
            self.guard = Some(self.shared.lock_arc());
        }
    }

    /// Gives the lock back.
    pub(super) fn release(&mut self) {
        self.guard = None;
    }
}

impl std::ops::Deref for StateSlot {
    type Target = AppState;
    fn deref(&self) -> &AppState {
        match &self.guard {
            Some(guard) => guard,
            None => unreachable!("the state is read between passes, without the lock"),
        }
    }
}

impl std::ops::DerefMut for StateSlot {
    fn deref_mut(&mut self) -> &mut AppState {
        match &mut self.guard {
            Some(guard) => guard,
            None => unreachable!("the state is written between passes, without the lock"),
        }
    }
}

/// What wakes the windows. A trait so that the tests count the wakes.
pub(super) trait Repaint: Send + 'static {
    fn repaint_root(&self);
}

impl Repaint for egui::Context {
    fn repaint_root(&self) {
        self.request_repaint_of(egui::ViewportId::ROOT);
    }
}

/// The single rule that wakes the windows from the thread: only when the tick
/// found that something they show changed (#475). Focus plays no part.
pub(super) fn wake(repaint: &impl Repaint, changed: bool) {
    if changed {
        repaint.repaint_root();
    }
}

/// How one pass of the thread ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Pass {
    /// The UI held the lock: look again soon.
    Busy,
    /// Done; sleep this long (`None`: until woken; zero: look again at once).
    Done(Option<Duration>),
}

/// The blocking write of an autosave job. A parameter of [`serve_once`] so that
/// a test can hold a write in flight.
pub(super) type RunJob = dyn Fn(&AutosaveJob) -> Result<(), SessionError> + Send + Sync;

/// The clock of the thread.
pub(super) type Clock = dyn Fn() -> Instant + Send + Sync;

fn run_for_real(job: &AutosaveJob) -> Result<(), SessionError> {
    job.run()
}

/// One pass of the thread: tick under the lock, the autosave write without it,
/// its record under the lock again, and the wake.
pub(super) fn serve_once(
    shared: &Mutex<AppState>,
    now: Instant,
    repaint: &impl Repaint,
    run: &RunJob,
) -> Pass {
    let Some(mut guard) = shared.try_lock() else {
        return Pass::Busy;
    };
    let tick = guard.server_tick(now);
    drop(guard);
    let mut changed = tick.changed;
    let mut next = tick.next;
    if let Some(job) = tick.autosave {
        // The lock is released: the write may take as long as the actor needs.
        let result = run(&job);
        changed |= finish_with_retry(shared, &job, result);
        next = Some(Duration::ZERO);
    }
    wake(repaint, changed);
    Pass::Done(next)
}

/// Records the end of a write. The thread never blocks on the lock: it looks
/// again every [`BUSY_RETRY`], also while the application quits (the exit
/// releases the guard before it joins, and waits for this to end).
fn finish_with_retry(
    shared: &Mutex<AppState>,
    job: &AutosaveJob,
    result: Result<(), SessionError>,
) -> bool {
    let mut result = Some(result);
    loop {
        if let Some(mut guard) = shared.try_lock() {
            if let Some(result) = result.take() {
                return guard.finish_autosave(job, result);
            }
            return false;
        }
        std::thread::park_timeout(BUSY_RETRY);
    }
}

/// What the UI side can see of the thread.
#[derive(Debug, Default)]
pub(super) struct ServerStatus {
    /// The thread has ended (normally, or by a panic).
    ended: AtomicBool,
    /// It ended while unwinding.
    panicked: AtomicBool,
    /// Passes done (tests).
    ticks: AtomicU64,
}

impl ServerStatus {
    pub(super) fn running(&self) -> bool {
        !self.ended.load(Ordering::Acquire)
    }
}

/// Marks the end of the thread, also when it unwinds.
struct EndGuard {
    status: Arc<ServerStatus>,
    done: mpsc::Sender<()>,
}

impl Drop for EndGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // #475 panic policy pending: the user has not decided what an
            // end of the server thread should say or do. Until then the UI
            // sees `running() == false` and polls in its own pass.
            self.status.panicked.store(true, Ordering::Release);
        }
        self.status.ended.store(true, Ordering::Release);
        let _ = self.done.send(());
    }
}

/// The running thread.
pub(super) struct ServerThread {
    join: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    status: Arc<ServerStatus>,
    done: mpsc::Receiver<()>,
}

impl ServerThread {
    pub(super) fn spawn(shared: SharedState, repaint: impl Repaint) -> Self {
        Self::spawn_with(
            shared,
            repaint,
            Arc::new(run_for_real),
            Arc::new(Instant::now),
        )
    }

    /// The write and the clock are parameters so that a test can hold a write
    /// in flight and move time.
    pub(super) fn spawn_with(
        shared: SharedState,
        repaint: impl Repaint,
        run: Arc<RunJob>,
        clock: Arc<Clock>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(ServerStatus::default());
        let (done_tx, done) = mpsc::channel();
        let thread_stop = Arc::clone(&stop);
        let thread_status = Arc::clone(&status);
        let join = std::thread::Builder::new()
            .name("stokesforge-server".into())
            .spawn(move || {
                let _end = EndGuard {
                    status: Arc::clone(&thread_status),
                    done: done_tx,
                };
                while !thread_stop.load(Ordering::Acquire) {
                    let pass = serve_once(&shared, clock(), &repaint, run.as_ref());
                    thread_status.ticks.fetch_add(1, Ordering::Relaxed);
                    match pass {
                        Pass::Busy => std::thread::park_timeout(BUSY_RETRY),
                        Pass::Done(Some(zero)) if zero.is_zero() => {}
                        Pass::Done(Some(delay)) => std::thread::park_timeout(delay),
                        Pass::Done(None) => std::thread::park(),
                    }
                }
            })
            .ok();
        if join.is_none() {
            status.ended.store(true, Ordering::Release);
        }
        Self {
            join,
            stop,
            status,
            done,
        }
    }

    pub(super) fn status(&self) -> Arc<ServerStatus> {
        Arc::clone(&self.status)
    }

    /// Asks the thread to look again (the state may have changed).
    pub(super) fn unpark(&self) {
        if let Some(join) = &self.join {
            join.thread().unpark();
        }
    }

    /// Stops the thread: the guard is RELEASED first (a write in flight needs
    /// the lock to record its end), then the thread is joined, for at most
    /// `wait`. Returns false if it did not end in time.
    pub(super) fn shut_down(&mut self, slot: &mut StateSlot, wait: Duration) -> bool {
        self.stop.store(true, Ordering::Release);
        slot.release();
        self.unpark();
        let Some(join) = self.join.take() else {
            return true;
        };
        if self.status.running() && self.done.recv_timeout(wait).is_err() {
            // Left to the process exit; never block the quit for ever.
            self.join = Some(join);
            return false;
        }
        let _ = join.join();
        true
    }
}

/// The application the binary runs: [`StokesForgeApp`] behind the lock, with the
/// server thread (#475).
pub struct ServedApp {
    app: StokesForgeApp,
    thread: ServerThread,
}

impl ServedApp {
    /// Starts the thread. `ctx` is the context the thread wakes.
    pub fn new(mut app: StokesForgeApp, ctx: egui::Context) -> Self {
        let thread = ServerThread::spawn(app.state.shared(), ctx);
        app.server = Some(thread.status());
        Self { app, thread }
    }
}

impl eframe::App for ServedApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.state.acquire();
        self.app.logic(ctx, frame);
        self.app.state.release();
        self.thread.unpark();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.state.acquire();
        self.app.ui(ui, frame);
        self.app.state.release();
        self.thread.unpark();
    }

    fn on_exit(&mut self) {
        self.app.state.acquire();
        self.app.on_exit();
        // Guard released, then joined: a write in flight ends silently and the
        // normal slot removal follows it. All before `session.shutdown()`.
        let _ended = self.thread.shut_down(&mut self.app.state, SHUTDOWN_WAIT);
    }
}

impl StokesForgeApp {
    /// Does the server thread run? Without it (a bench, a test, a thread that
    /// ended) the paint pass polls the background work itself.
    pub(super) fn server_running(&self) -> bool {
        self.server.as_ref().is_some_and(|status| status.running())
    }

    /// The autosave poll of `logic`, when no thread runs.
    pub(super) fn poll_autosave_without_server(&mut self, ctx: &egui::Context) {
        if self.server_running() {
            return;
        }
        if let Some(delay) = self.state.poll_autosave(Instant::now()) {
            ctx.request_repaint_after(delay);
        }
    }

    /// The script and recalculation polls of the paint pass, when no thread runs.
    pub(super) fn poll_jobs_without_server(&mut self) {
        if self.server_running() {
            return;
        }
        self.state.poll_script();
        self.state.poll_recalc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex as StdMutex;

    use stokesforge_app_state::state::Action;
    use stokesforge_core::{Column, ColumnFormula, ColumnMeta, FormulaPart, Project};
    use stokesforge_session::Session;

    use super::super::graph_window::tests::plotted_graph;
    use super::super::tab::Tab;

    const PERIOD: Duration = Duration::from_secs(300);

    /// Counts the wakes.
    #[derive(Clone, Default)]
    struct Counter(Arc<AtomicUsize>);

    impl Repaint for Counter {
        fn repaint_root(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Counter {
        fn count(&self) -> usize {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn real_run() -> &'static RunJob {
        &run_for_real
    }

    /// A slot on `project`, with the recovery folder `dir/recovery`, released:
    /// the lock is free, as between two passes.
    fn free_slot(project: Project, dir: &Path) -> (Session, StateSlot) {
        let session = Session::spawn(project, None).expect("session");
        let mut state = AppState::new(session.handle(), dir.join("a.stf").display().to_string())
            .expect("state");
        state.set_recovery_dir(Some(dir.join("recovery")));
        let mut slot = StateSlot::new(state);
        slot.release();
        (session, slot)
    }

    fn copies(dir: &Path) -> usize {
        std::fs::read_dir(dir.join("recovery")).map_or(0, |entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "stf"))
                .count()
        })
    }

    fn demo() -> Project {
        plotted_graph_project()
    }

    fn plotted_graph_project() -> Project {
        let mut project = Project::new("Demo").expect("project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("book");
        project
    }

    /// The autosave is written by the thread alone: no root pass, no `logic`.
    #[test]
    fn the_thread_writes_the_autosave_without_any_root_pass() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        let counter = Counter::default();
        let t0 = Instant::now();
        assert_eq!(
            serve_once(&shared, t0, &counter, real_run()),
            Pass::Done(Some(PERIOD))
        );
        shared.lock().dispatch(Action::AddGraph);
        serve_once(&shared, t0 + PERIOD, &counter, real_run());
        assert_eq!(copies(dir.path()), 1);
    }

    /// A UI pass holds the lock: the thread does not wait, it asks to look
    /// again soon.
    #[test]
    fn the_thread_never_waits_for_the_ui() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut slot) = free_slot(demo(), dir.path());
        slot.acquire();
        let shared = slot.shared();
        let counter = Counter::default();
        assert_eq!(
            serve_once(&shared, Instant::now(), &counter, real_run()),
            Pass::Busy
        );
        assert_eq!(counter.count(), 0);
    }

    /// Nothing changes: no wake, however many ticks.
    #[test]
    fn idle_ticks_wake_nobody() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        let counter = Counter::default();
        let t0 = Instant::now();
        for second in 0..50 {
            serve_once(
                &shared,
                t0 + Duration::from_secs(second),
                &counter,
                real_run(),
            );
        }
        assert_eq!(counter.count(), 0);
    }

    /// A project whose first full recalculation outlasts the command wait.
    fn heavy_formula_project() -> Project {
        let mut project = Project::new("Heavy").expect("project");
        let root = project.root();
        let book = project.add_workbook(root, Some("Book1")).expect("book");
        let sheet = project
            .tree()
            .child_by_name(book, "Sheet1")
            .expect("read")
            .expect("Sheet1");
        let values: Vec<f64> = (0..200_000).map(f64::from).collect();
        project
            .add_column(sheet, None, Some("A"), Column::from_f64(&values))
            .expect("A");
        let column_a = project.tree().children(sheet).expect("sheet")[0];
        for index in 0..150 {
            let column = project
                .add_column(
                    sheet,
                    None,
                    Some(&format!("F{index}")),
                    Column::new(stokesforge_core::ColumnType::Numeric),
                )
                .expect("column");
            let formula = ColumnFormula::from_parts(vec![
                FormulaPart::Column(column_a),
                FormulaPart::Text("*2+".into()),
                FormulaPart::Column(column_a),
                FormulaPart::Text("*3".into()),
            ]);
            project
                .set_column_meta(
                    column,
                    ColumnMeta {
                        formula: Some(formula),
                        ..Default::default()
                    },
                )
                .expect("formula");
        }
        project
    }

    /// A recalculation ends while no window paints: the thread collects it,
    /// refreshes the snapshot and wakes the windows once.
    #[test]
    fn a_recalculation_ending_without_a_root_pass_is_collected_and_wakes_once() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, slot) = free_slot(heavy_formula_project(), dir.path());
        let shared = slot.shared();
        shared.lock().dispatch(Action::AddGraph);
        assert!(shared.lock().recalc_work().is_some(), "still running");
        let before = shared.lock().tree().len();
        let counter = Counter::default();
        let deadline = Instant::now() + Duration::from_secs(120);
        let t0 = Instant::now();
        while shared.lock().recalc_work().is_some() {
            assert!(Instant::now() < deadline, "recalculation too long");
            serve_once(&shared, t0, &counter, real_run());
            std::thread::sleep(Duration::from_millis(5));
        }
        serve_once(&shared, t0, &counter, real_run());
        serve_once(&shared, t0, &counter, real_run());
        let state = shared.lock();
        assert!(state.recalc_work().is_none());
        assert_eq!(state.tree().len(), before + 1, "the snapshot is fresh");
        assert_eq!(counter.count(), 1, "one wake for the end");
    }

    /// Embedded viewports (the default context): a detached graph is painted
    /// inside the root pass, on the thread that holds the guard. No window
    /// callback may take the shared lock, or the pass never ends.
    #[test]
    fn a_pass_with_a_detached_graph_ends_while_the_thread_ticks() {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (session, graph) = plotted_graph();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let mut app = StokesForgeApp::new(state);
            app.detach_tab(Tab::Graph(graph));
            let ctx = egui::Context::default();
            ctx.set_fonts(stokesforge_render_egui::font_definitions());
            crate::theme::install(&ctx);
            let mut served = ServedApp::new(app, ctx.clone());
            let mut frame = eframe::Frame::_new_kittest();
            for pass in 0..5 {
                let raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    time: Some(f64::from(pass) * 0.05),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(raw, |ui| eframe::App::ui(&mut served, ui, &mut frame));
                output.textures_delta.clear();
                std::thread::sleep(Duration::from_millis(15));
            }
            eframe::App::on_exit(&mut served);
            drop(session);
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(60))
            .expect("the passes ended: no callback takes the lock");
    }

    /// Exit: the guard is released, then the thread is joined; the thread has
    /// ended and its pass counter is frozen. A write in flight is waited for.
    #[test]
    fn exit_releases_the_guard_then_joins_the_thread() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        // A stalled write: the thread holds a job in flight.
        let (started_tx, started) = mpsc::channel();
        let (go_tx, go) = mpsc::channel::<()>();
        let go = StdMutex::new(go);
        let run: Arc<RunJob> = Arc::new(move |job: &AutosaveJob| {
            let _ = started_tx.send(());
            let _ = go.lock().map(|go| go.recv_timeout(Duration::from_secs(10)));
            job.run()
        });
        let t0 = Instant::now();
        let offset = Arc::new(AtomicU64::new(0));
        let clock_offset = Arc::clone(&offset);
        let clock: Arc<Clock> =
            Arc::new(move || t0 + Duration::from_secs(clock_offset.load(Ordering::SeqCst)));
        let mut thread =
            ServerThread::spawn_with(Arc::clone(&shared), Counter::default(), run, clock);
        // The first pass arms the deadline; then the project changes and the
        // clock jumps past the deadline.
        while thread.status().ticks.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(2));
        }
        shared.lock().dispatch(Action::AddGraph);
        offset.store(PERIOD.as_secs() + 1, Ordering::SeqCst);
        thread.unpark();
        started
            .recv_timeout(Duration::from_secs(10))
            .expect("the write is in flight");
        // The UI is in its last pass, holding the guard.
        slot.acquire();
        go_tx.send(()).unwrap();
        assert!(
            thread.shut_down(&mut slot, Duration::from_secs(5)),
            "the thread ended: the guard was released before the join"
        );
        assert!(!thread.status().running());
        let ticks = thread.status().ticks.load(Ordering::Relaxed);
        thread.unpark();
        assert_eq!(thread.status().ticks.load(Ordering::Relaxed), ticks);
        assert_eq!(copies(dir.path()), 1, "the write ended before the exit");
    }
}

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
//! - the thread executes commands only for a finished import, with the lock
//!   released, and never drops a script engine (invariant 9: a finished run only moves it back into the state);
//! - one rule to wake the windows: [`wake`], and only when the tick says that
//!   something they show changed. Focus plays no part;
//! - on exit the UI releases the guard, **then** joins the thread
//!   ([`ServerThread::shut_down`]), before the session is shut down.
//!
//! What stays in the paint pass: the busy-indicator repaints, opening the
//! windows of the workbooks an import created (`take_books_to_open`),
//! everything that needs `Frame` or `Context`.
//!
//! # Four rules of L6
//!
//! - **Quit during an autosave write**: `take_quit_ready` refuses while a job is
//!   in flight and says why once (`status-quit-waiting-autosave`); the window
//!   stays open and painting, the end of the write wakes it, and the quit
//!   follows. The join of `on_exit` stays as the last safety net.
//! - **Panic of the thread**: `EndGuard` marks it; the paint pass takes the polls
//!   back (`poll_*_without_server`) and says it once (`tell_server_stopped`).
//! - **Finished import**: taken out under the lock (`import_apply_begin`), its
//!   commands run by [`serve_once`] with the lock RELEASED, on a scratch
//!   `AppState` over a handle with a sender identity of its own, as a script's (one macro, ordinary
//!   undoable `Command`s, one history entry), then recorded under the lock again
//!   (`import_apply_finish`). Opening windows is flagged for the next paint pass.
//! - **Finished script**: its thread calls the waker (`AppState::set_waker`)
//!   after sending the reply; [`ServedApp::new`] makes it unpark this thread.
//!   The 50 ms look stays as the fallback.
//!
//! Detached windows keep their fork (`window_detach.rs`): this thread is step 1
//! of #475 and changes nothing they see.
//!
//! # Without the thread
//!
//! A bench or a test has no thread, and a thread that ended must not freeze the
//! application: while [`StokesForgeApp::server_running`] is false the paint
//! pass does the polling itself (`poll_without_server`), as before #475.

use std::path::PathBuf;
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

use stokesforge_app_state::tr;

use super::server_stats;
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

/// Records the files of an import in the recent list (a file of the
/// configuration folder). A parameter so that a test does not write it.
pub(super) type RecordImports = dyn Fn(&[PathBuf]) + Send + Sync;

fn record_for_real(paths: &[PathBuf]) {
    super::remember_recent_imports(paths);
}

/// One pass of the thread: tick under the lock, the autosave write and the
/// commands of a finished import without it, their record under the lock again,
/// and the wake.
pub(super) fn serve_once(
    shared: &Mutex<AppState>,
    now: Instant,
    repaint: &impl Repaint,
    run: &RunJob,
    record: &RecordImports,
) -> Pass {
    let held = server_stats::start();
    let Some(mut guard) = shared.try_lock() else {
        server_stats::event("busy", false);
        return Pass::Busy;
    };
    let tick = guard.server_tick(now);
    let garbage = guard.take_garbage();
    drop(guard);
    // Freed with the lock released (the previous snapshot's big columns).
    garbage.dispose();
    server_stats::since("tick", held, tick.changed);
    let mut changed = tick.changed;
    let mut next = tick.next;
    if let Some(job) = tick.autosave {
        // The lock is released: the write may take as long as the actor needs.
        server_stats::write_began();
        let written = server_stats::start();
        let result = run(&job);
        server_stats::write_ended();
        server_stats::since("write", written, result.is_ok());
        changed |= with_lock_retry(shared, |state| {
            let changed = state.finish_autosave(&job, result);
            (changed, state.take_garbage())
        });
        next = Some(Duration::ZERO);
    }
    if let Some(mut import) = tick.import {
        // The lock is released: the commands go through the session as the
        // interface's own, one macro, undoable (invariant 1).
        import.execute();
        let (done, imported) = with_lock_retry(shared, |state| {
            let done = state.import_apply_finish(import);
            ((done, state.take_imported_paths()), state.take_garbage())
        });
        changed |= done;
        if !imported.is_empty() {
            record(&imported);
        }
        next = Some(Duration::ZERO);
    }
    wake(repaint, changed);
    if changed {
        server_stats::event("wake", true);
    }
    Pass::Done(next)
}

/// Runs `body` under the lock. The thread never blocks on the lock: it looks
/// again every [`BUSY_RETRY`], also while the application quits (the exit
/// releases the guard before it joins, and waits for this to end).
fn with_lock_retry<R>(
    shared: &Mutex<AppState>,
    body: impl FnOnce(&mut AppState) -> (R, stokesforge_app_state::state::Garbage),
) -> R {
    let mut body = Some(body);
    loop {
        if let Some(mut guard) = shared.try_lock() {
            if let Some(body) = body.take() {
                let held = server_stats::start();
                let (result, garbage) = body(&mut guard);
                drop(guard);
                server_stats::since("finish", held, true);
                garbage.dispose();
                return result;
            }
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

    /// The thread ended in error: it unwound, or it could not start.
    pub(super) fn panicked(&self) -> bool {
        self.panicked.load(Ordering::Acquire)
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
            // The UI sees `running() == false` and polls in its own pass, and
            // says so once (`tell_server_stopped`).
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
            Arc::new(record_for_real),
        )
    }

    /// The write, the clock and the recent-files record are parameters so that
    /// a test can hold a write in flight, move time and keep the user's files.
    pub(super) fn spawn_with(
        shared: SharedState,
        repaint: impl Repaint,
        run: Arc<RunJob>,
        clock: Arc<Clock>,
        record: Arc<RecordImports>,
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
                    let pass =
                        serve_once(&shared, clock(), &repaint, run.as_ref(), record.as_ref());
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
            // The thread could not start: the same end as a panic for the UI
            // (fallback polls and the one-time warning).
            status.panicked.store(true, Ordering::Release);
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

    /// What a finished script calls to wake the thread at once (#475). Empty
    /// when the thread did not start.
    pub(super) fn waker(&self) -> Option<std::thread::Thread> {
        self.join.as_ref().map(|join| join.thread().clone())
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
    pass_hook: Option<PassHook>,
}

/// What a bench runs at the end of a paint pass, under the lock, where a
/// gesture would be applied.
pub type PassHook = Box<dyn FnMut(&mut AppState, &egui::Context) + Send>;

impl ServedApp {
    /// Starts the thread. `ctx` is the context the thread wakes.
    pub fn new(mut app: StokesForgeApp, ctx: egui::Context) -> Self {
        let thread = ServerThread::spawn(app.state.shared(), ctx);
        if let Some(server) = thread.waker() {
            // The end of a script wakes the thread at once; its 50 ms look
            // stays as the fallback.
            app.state.set_waker(Arc::new(move || server.unpark()));
        }
        app.server = Some(thread.status());
        Self {
            app,
            thread,
            pass_hook: None,
        }
    }

    /// For the bench (`examples/server_bench.rs`): runs `hook` at the end of
    /// each paint pass, with the lock held. Nothing in the application sets it.
    #[doc(hidden)]
    pub fn set_pass_hook(&mut self, hook: PassHook) {
        self.pass_hook = Some(hook);
    }

    /// For the bench: what a paint pass does after a gesture (the thread looks
    /// again), for a change made from a bench thread.
    #[doc(hidden)]
    pub fn bench_waker(&self) -> Arc<dyn Fn() + Send + Sync> {
        match self.thread.waker() {
            Some(server) => Arc::new(move || server.unpark()),
            None => Arc::new(|| {}),
        }
    }

    /// For the bench: the shared state, as the thread sees it.
    #[doc(hidden)]
    pub fn shared_state(&self) -> Arc<Mutex<AppState>> {
        self.app.state.shared()
    }
}

impl eframe::App for ServedApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let waited = server_stats::start();
        self.app.state.acquire();
        server_stats::since("logic", waited, false);
        self.app.logic(ctx, frame);
        self.app.state.release();
        self.thread.unpark();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let waited = server_stats::start();
        self.app.state.acquire();
        server_stats::since("ui", waited, false);
        self.app.ui(ui, frame);
        if let Some(hook) = &mut self.pass_hook {
            hook(&mut self.app.state, ui.ctx());
        }
        self.app.state.release();
        self.thread.unpark();
    }

    fn on_exit(&mut self) {
        self.app.state.acquire();
        self.app.on_exit();
        // Guard released, then joined: a write in flight ends silently and the
        // normal slot removal follows it. All before `session.shutdown()`.
        let _ended = self.thread.shut_down(&mut self.app.state, SHUTDOWN_WAIT);
        server_stats::event("exit", true);
        server_stats::bench_flush_stats();
    }
}

impl StokesForgeApp {
    /// Does the server thread run? Without it (a bench, a test, a thread that
    /// ended) the paint pass polls the background work itself.
    pub(super) fn server_running(&self) -> bool {
        self.server.as_ref().is_some_and(|status| status.running())
    }

    /// Says once, in the status bar, that the server thread died of an internal
    /// error: from then on this window runs the background work itself
    /// (`poll_*_without_server`).
    pub(super) fn tell_server_stopped(&mut self) {
        if !self.server_stop_told && self.server.as_ref().is_some_and(|status| status.panicked()) {
            self.server_stop_told = true;
            self.state.warn(tr!("status-server-stopped"));
        }
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
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex as StdMutex;

    use stokesforge_app_state::state::Action;
    use stokesforge_app_state::tr;
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

    fn no_record(_: &[PathBuf]) {}

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
            serve_once(&shared, t0, &counter, real_run(), &no_record),
            Pass::Done(Some(PERIOD))
        );
        shared.lock().dispatch(Action::AddGraph);
        serve_once(&shared, t0 + PERIOD, &counter, real_run(), &no_record);
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
            serve_once(&shared, Instant::now(), &counter, real_run(), &no_record),
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
                &no_record,
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
            serve_once(&shared, t0, &counter, real_run(), &no_record);
            std::thread::sleep(Duration::from_millis(5));
        }
        serve_once(&shared, t0, &counter, real_run(), &no_record);
        serve_once(&shared, t0, &counter, real_run(), &no_record);
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
        let mut thread = ServerThread::spawn_with(
            Arc::clone(&shared),
            Counter::default(),
            run,
            clock,
            Arc::new(|_: &[PathBuf]| {}),
        );
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

    /// The configuration folder of the process, redirected once (an import
    /// reads and may write the recipe store).
    fn isolate_config() {
        static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let dir = DIR.get_or_init(|| tempfile::tempdir().expect("config dir"));
        std::env::set_var(
            stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
            dir.path(),
        );
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
    }

    /// `files` csv files of `columns` columns each, in `dir`.
    fn csv_files(dir: &Path, files: usize, columns: usize) -> Vec<PathBuf> {
        (0..files)
            .map(|index| {
                let path = dir.join(format!("serie{index}.csv"));
                let row: Vec<String> = (0..columns).map(|c| c.to_string()).collect();
                let text = format!("{}\n", row.join(","));
                std::fs::write(&path, text.repeat(20)).unwrap();
                path
            })
            .collect()
    }

    /// Children of the root: the workbooks (and graphs) of the project.
    fn root_children(state: &AppState) -> usize {
        state
            .tree()
            .node(state.tree().root())
            .map_or(0, |node| node.children().len())
    }

    fn start_new_book_import(shared: &Mutex<AppState>, files: Vec<PathBuf>) {
        shared.lock().dispatch(Action::StartImport {
            destination: stokesforge_app_state::state::ImportDestination::NewBookPerFile,
            recipe: stokesforge_import::ImportRecipe::new("Serie"),
            paths: files,
        });
    }

    /// Passes of the thread until the import is over (analysis, application).
    fn serve_until_import_done(shared: &Mutex<AppState>, counter: &Counter) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            serve_once(shared, Instant::now(), counter, real_run(), &no_record);
            let state = shared.lock();
            if state.import_progress().is_none() && !state.import_is_applying() {
                return;
            }
            drop(state);
            assert!(Instant::now() < deadline, "import too long");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Every window hidden: no root pass at all. The thread alone applies a
    /// finished import (one history entry), and the next root pass opens the
    /// new workbooks as it does for an immediate import.
    #[test]
    fn a_finished_import_is_applied_by_the_thread_and_opened_by_the_next_pass() {
        isolate_config();
        let dir = tempfile::tempdir().unwrap();
        let (_session, slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        let counter = Counter::default();
        let before = root_children(&shared.lock());
        start_new_book_import(&shared, csv_files(dir.path(), 2, 3));
        serve_until_import_done(&shared, &counter);
        {
            let state = shared.lock();
            assert_eq!(root_children(&state), before + 2, "2 workbooks");
            assert!(counter.count() >= 1, "the windows are woken");
        }
        // The history holds one entry.
        shared.lock().dispatch(Action::Undo);
        assert_eq!(root_children(&shared.lock()), before);
        assert!(!shared.lock().can_undo(), "one entry for the import");
        shared.lock().dispatch(Action::Redo);
        drop(shared);
        // The next pass opens them.
        let mut app = StokesForgeApp::new(slot.into_state());
        let ctx = egui::Context::default();
        let initial = app.floating_tabs.len();
        app.poll_background_jobs(&ctx);
        assert_eq!(
            app.floating_tabs.len(),
            initial + 2,
            "{:?}",
            app.floating_tabs
        );
        app.poll_background_jobs(&ctx);
        assert_eq!(app.floating_tabs.len(), initial + 2, "each opened once");
    }

    /// The commands run with the lock released (no heavy work under the lock,
    /// #475): a second thread takes the lock while the state says that an
    /// import is being applied.
    #[test]
    fn the_import_commands_run_with_the_lock_released() {
        isolate_config();
        let dir = tempfile::tempdir().unwrap();
        let (session, slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        start_new_book_import(&shared, csv_files(dir.path(), 1, 400));
        let over = Arc::new(AtomicBool::new(false));
        let observer = {
            let (shared, over) = (Arc::clone(&shared), Arc::clone(&over));
            let handle = session.handle();
            std::thread::spawn(move || {
                // The session revisions seen while the lock was free and the
                // import being applied: the commands progress under our eyes.
                let mut seen = std::collections::BTreeSet::new();
                while !over.load(Ordering::Acquire) {
                    if let Some(state) = shared.try_lock() {
                        if state.import_is_applying() {
                            if let Ok(snapshot) = handle.snapshot() {
                                seen.insert(snapshot.revision);
                            }
                        }
                    }
                    std::thread::yield_now();
                }
                seen.len()
            })
        };
        serve_until_import_done(&shared, &Counter::default());
        over.store(true, Ordering::Release);
        let seen = observer.join().unwrap();
        assert!(
            seen >= 5,
            "the lock was free while the commands ran ({seen} revisions seen)"
        );
    }

    /// The thread dies of an internal error: this window runs the background
    /// work itself (the fallback polls), and says so once.
    #[test]
    fn a_dead_server_thread_is_replaced_by_the_paint_pass_with_one_warning() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::spawn(heavy_formula_project(), None).expect("session");
        let mut state = AppState::new(
            session.handle(),
            dir.path().join("a.stf").display().to_string(),
        )
        .expect("state");
        state.set_recovery_dir(None);
        let mut app = StokesForgeApp::new(state);
        // Test-only injection: the clock of the thread panics at the first pass.
        let clock: Arc<Clock> = Arc::new(|| panic!("injected failure of the server thread"));
        let thread = ServerThread::spawn_with(
            app.state.shared(),
            Counter::default(),
            Arc::new(run_for_real),
            clock,
            Arc::new(|_: &[PathBuf]| {}),
        );
        app.server = Some(thread.status());
        thread
            .done
            .recv_timeout(Duration::from_secs(10))
            .expect("the thread ended");
        assert!(!app.server_running());
        assert!(thread.status().panicked());
        let ctx = egui::Context::default();
        let said = |app: &StokesForgeApp| {
            app.state
                .messages_since(0)
                .filter(|m| m.text == tr!("status-server-stopped"))
                .map(|m| usize::try_from(m.count).unwrap_or(usize::MAX))
                .sum::<usize>()
        };
        // The fallback runs a recalculation to its end, as before #475.
        let before = app.state.tree().len();
        app.state.dispatch(Action::AddGraph);
        assert!(app.state.recalc_work().is_some(), "still running");
        let deadline = Instant::now() + Duration::from_secs(120);
        while app.state.recalc_work().is_some() {
            assert!(Instant::now() < deadline, "recalculation too long");
            app.poll_background_jobs(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        app.poll_background_jobs(&ctx);
        assert_eq!(app.state.tree().len(), before + 1, "the snapshot is fresh");
        for _ in 0..5 {
            app.poll_background_jobs(&ctx);
        }
        assert_eq!(said(&app), 1, "the warning is said once");
        assert!(app.state.status().is_some());
        // A running thread says nothing.
        drop(thread);
    }

    // -- Randomised stress (#475) --

    /// A small seeded generator (xorshift64*): the same seed replays the same
    /// run.
    struct Dice(u64);

    impl Dice {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, bound: usize) -> usize {
            usize::try_from(self.next() % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
        }
    }

    /// The tree as the session has it now.
    fn session_tree(session: &Session) -> Arc<stokesforge_core::Tree> {
        Arc::clone(&session.handle().snapshot().expect("snapshot").tree)
    }

    /// One run of the stress: the real thread ticks (its clock jumps a whole
    /// period at each pass, so every pass makes an autosave due) while the main
    /// side plays `steps` passes of random actions through the app path.
    /// Checks: no deadlock (the caller's watchdog), no panic of the thread, the
    /// last recovery copy reloads equal to the project, the history comes back
    /// where it was after undo then redo, no slot file is left over.
    fn stress_run(seed: u64, steps: usize) {
        isolate_config();
        let dir = tempfile::tempdir().unwrap();
        let (session, mut slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        let files = csv_files(dir.path(), 2, 3);
        let t0 = Instant::now();
        let offset = Arc::new(AtomicU64::new(0));
        let clock_offset = Arc::clone(&offset);
        let clock: Arc<Clock> =
            Arc::new(move || t0 + Duration::from_secs(clock_offset.load(Ordering::SeqCst)));
        let writes = Arc::new(AtomicUsize::new(0));
        let counted_writes = Arc::clone(&writes);
        let run: Arc<RunJob> = Arc::new(move |job: &AutosaveJob| {
            counted_writes.fetch_add(1, Ordering::SeqCst);
            job.run()
        });
        let recorded = Arc::new(AtomicUsize::new(0));
        let counted_records = Arc::clone(&recorded);
        let mut thread = ServerThread::spawn_with(
            Arc::clone(&shared),
            Counter::default(),
            run,
            clock,
            Arc::new(move |paths: &[PathBuf]| {
                counted_records.fetch_add(paths.len(), Ordering::SeqCst);
            }),
        );
        let mut dice = Dice(seed | 1);
        let sheet = shared
            .lock()
            .tree()
            .resolve("[Book1]Sheet1")
            .expect("Sheet1");
        for step in 0..steps {
            slot.acquire();
            for _ in 0..=dice.below(3) {
                let columns: Vec<_> = slot
                    .visible_columns(sheet)
                    .into_iter()
                    .map(|(id, ..)| id)
                    .collect();
                let action = match dice.below(14) {
                    0..=3 if !columns.is_empty() => Some(Action::CommitCell {
                        column: columns[dice.below(columns.len())],
                        row: dice.below(30),
                        text: dice.below(1000).to_string(),
                    }),
                    4 | 5 => Some(Action::Undo),
                    6 | 7 => Some(Action::Redo),
                    8 => Some(Action::AddColumn { sheet }),
                    9 if columns.len() > 1 => Some(Action::DeleteObjects(vec![
                        columns[dice.below(columns.len())],
                    ])),
                    10 => Some(Action::InsertRows {
                        sheet,
                        at: dice.below(10),
                        count: 1 + dice.below(3),
                    }),
                    11 => Some(Action::AddWorkbook),
                    12 => Some(Action::Save),
                    13 if step % 3 == 0 => {
                        slot.dispatch(Action::StartImport {
                            destination:
                                stokesforge_app_state::state::ImportDestination::NewBookPerFile,
                            recipe: stokesforge_import::ImportRecipe::new("Serie"),
                            paths: files.clone(),
                        });
                        None
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    slot.dispatch(action);
                }
            }
            slot.release();
            offset.fetch_add(PERIOD.as_secs() + 1, Ordering::SeqCst);
            thread.unpark();
            match dice.below(4) {
                0 => std::thread::yield_now(),
                1 => std::thread::sleep(Duration::from_micros(dice.below(1500) as u64)),
                _ => {}
            }
        }
        // A last change, so that a recovery copy of the final state must exist.
        slot.acquire();
        slot.dispatch(Action::AddColumn { sheet });
        slot.release();
        // The work in progress ends (analysis, application, recalculation).
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            thread.unpark();
            let busy = {
                let state = shared.lock();
                state.import_progress().is_some()
                    || state.import_is_applying()
                    || state.recalc_work().is_some()
            };
            if !busy {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the work did not end (seed {seed})"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        // History: undo a few entries, redo them: the project is as it was.
        slot.acquire();
        let final_tree = session_tree(&session);
        let mut undone = 0;
        while undone < 20 && slot.can_undo() {
            slot.dispatch(Action::Undo);
            undone += 1;
        }
        for _ in 0..undone {
            slot.dispatch(Action::Redo);
        }
        let again = session_tree(&session);
        assert!(
            *again == *final_tree,
            "undo x{undone} then redo changed the project (seed {seed})"
        );
        // Quit the way the application does: the guard is released, then the
        // thread is joined.
        assert!(
            thread.shut_down(&mut slot, Duration::from_secs(30)),
            "the thread ended (seed {seed})"
        );
        assert!(!thread.status().panicked(), "no panic (seed {seed})");
        // The last pass of the thread is gone: write the copy of the final state.
        let far = t0 + Duration::from_secs(offset.load(Ordering::SeqCst) + 2 * PERIOD.as_secs());
        serve_once(&shared, far, &Counter::default(), real_run(), &no_record);
        serve_once(
            &shared,
            far + PERIOD + Duration::from_secs(1),
            &Counter::default(),
            real_run(),
            &no_record,
        );
        let written: Vec<PathBuf> = std::fs::read_dir(dir.path().join("recovery"))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "stf"))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            written.len(),
            1,
            "one recovery copy (seed {seed}): {written:?}"
        );
        let other = Session::spawn(Project::new("Other").expect("project"), None).expect("session");
        other.handle().load(&written[0]).expect("the copy reloads");
        let reloaded = session_tree(&other);
        assert!(
            *reloaded == *session_tree(&session),
            "the recovery copy reloads equal to the project (seed {seed})"
        );
        other.shutdown().expect("shutdown");
    }

    /// Runs `body` on a thread with a watchdog: a deadlock fails the test
    /// instead of hanging it, a panic is passed on.
    fn with_watchdog(limit: Duration, body: impl FnOnce() + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
            let _ = tx.send(result);
        });
        match rx.recv_timeout(limit) {
            Ok(Ok(())) => {}
            Ok(Err(panic)) => std::panic::resume_unwind(panic),
            Err(_) => panic!("deadlock: the run did not end within {limit:?}"),
        }
    }

    /// Short variant, in every run: the seed can be replaced with
    /// `SF_STRESS_SEED`.
    #[test]
    fn the_server_thread_survives_random_passes() {
        let seed = std::env::var("SF_STRESS_SEED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0x05EE_D475);
        with_watchdog(Duration::from_secs(180), move || stress_run(seed, 250));
    }

    /// Long variant: many seeds, many passes (`cargo test -p stokesforge-app
    /// --lib server:: -- --ignored`).
    #[test]
    #[ignore = "long: 12 seeds x 1500 passes"]
    fn the_server_thread_survives_many_random_passes() {
        with_watchdog(Duration::from_secs(1800), || {
            for seed in 1..=12u64 {
                stress_run(seed * 0x9E37_79B9, 1500);
            }
        });
    }

    /// `ServedApp::new` wires the waker: calling it unparks the server thread,
    /// which then looks again at once (it was parked until its autosave
    /// deadline, minutes away).
    #[test]
    fn the_served_app_waker_unparks_the_server_thread() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::spawn(demo(), None).expect("session");
        let mut state = AppState::new(
            session.handle(),
            dir.path().join("a.stf").display().to_string(),
        )
        .expect("state");
        state.set_recovery_dir(None);
        let app = StokesForgeApp::new(state);
        let mut served = ServedApp::new(app, egui::Context::default());
        // The state is held from `new()` on, like a test that drives `ui`
        // directly; between two passes the lock is free and the thread works.
        served.app.state.release();
        let status = served.thread.status();
        let ticks = || status.ticks.load(Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(10);
        while ticks() == 0 {
            assert!(Instant::now() < deadline, "the first pass");
            std::thread::sleep(Duration::from_millis(2));
        }
        // Let the thread reach its park.
        std::thread::sleep(Duration::from_millis(200));
        let before = ticks();
        served.app.state.acquire();
        served.app.state.wake_server();
        let deadline = Instant::now() + Duration::from_secs(5);
        while ticks() == before {
            assert!(
                Instant::now() < deadline,
                "the waker did not wake the thread"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        served.app.state.acquire();
        let _ = served
            .thread
            .shut_down(&mut served.app.state, Duration::from_secs(5));
    }

    /// What a tick defers is dropped after the lock is released (#475, M2): the
    /// previous snapshot of a big project is freed by its drop.
    #[test]
    fn the_garbage_of_a_tick_is_dropped_outside_the_lock() {
        struct Probe {
            shared: SharedState,
            free_when_dropped: Arc<AtomicBool>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                // parking_lot's lock is not re-entrant: held by this thread, it fails.
                self.free_when_dropped
                    .store(self.shared.try_lock().is_some(), Ordering::SeqCst);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let (_session, slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        let free = Arc::new(AtomicBool::new(false));
        shared.lock().stash_garbage(Probe {
            shared: Arc::clone(&shared),
            free_when_dropped: Arc::clone(&free),
        });
        serve_once(
            &shared,
            Instant::now(),
            &Counter::default(),
            real_run(),
            &no_record,
        );
        assert!(
            free.load(Ordering::SeqCst),
            "the deferred value was dropped under the lock"
        );
        assert!(shared.lock().take_garbage().is_empty());
    }

    /// Quit while the copy is being written: that write is the only one. The
    /// slot is removed by Quit, so the record of the write is discarded; the
    /// deadline has fallen and the project looks dirty, which used to start a
    /// second write while the window closed (M6).
    #[test]
    fn quit_during_an_autosave_write_allows_no_second_write() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, slot) = free_slot(demo(), dir.path());
        let shared = slot.shared();
        let writes = Arc::new(AtomicUsize::new(0));
        let (started_tx, started) = mpsc::channel();
        let (go_tx, go) = mpsc::channel::<()>();
        let go = StdMutex::new(go);
        let counted = Arc::clone(&writes);
        let run: Arc<RunJob> = Arc::new(move |job: &AutosaveJob| {
            counted.fetch_add(1, Ordering::SeqCst);
            let _ = started_tx.send(());
            let _ = go.lock().map(|go| go.recv_timeout(Duration::from_secs(10)));
            job.run()
        });
        let t0 = Instant::now();
        let offset = Arc::new(AtomicU64::new(0));
        let clock_offset = Arc::clone(&offset);
        let clock: Arc<Clock> =
            Arc::new(move || t0 + Duration::from_secs(clock_offset.load(Ordering::SeqCst)));
        let mut thread = ServerThread::spawn_with(
            Arc::clone(&shared),
            Counter::default(),
            run,
            clock,
            Arc::new(|_: &[PathBuf]| {}),
        );
        while thread.status().ticks.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(2));
        }
        shared.lock().dispatch(Action::AddGraph);
        offset.store(PERIOD.as_secs() + 1, Ordering::SeqCst);
        thread.unpark();
        started
            .recv_timeout(Duration::from_secs(10))
            .expect("the write is in flight");
        {
            let mut state = shared.lock();
            state.set_quit_without_prompt(true);
            state.dispatch(Action::Quit {
                pending_edits: false,
            });
        }
        go_tx.send(()).unwrap();
        // The window takes Quit as soon as the write has ended, then closes.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !shared.lock().take_quit_ready() {
            assert!(Instant::now() < deadline, "quit never became ready");
            std::thread::sleep(Duration::from_millis(2));
        }
        // The deadline is long past and the project is dirty: look again.
        offset.store(10 * PERIOD.as_secs(), Ordering::SeqCst);
        let ticks = thread.status().ticks.load(Ordering::Relaxed);
        thread.unpark();
        while thread.status().ticks.load(Ordering::Relaxed) < ticks + 2 {
            assert!(Instant::now() < deadline, "the thread did not look again");
            thread.unpark();
            std::thread::sleep(Duration::from_millis(2));
        }
        let mut slot = slot;
        assert!(thread.shut_down(&mut slot, Duration::from_secs(5)));
        assert_eq!(writes.load(Ordering::SeqCst), 1, "a second write started");
        assert_eq!(copies(dir.path()), 0, "no copy is left behind");
    }
}

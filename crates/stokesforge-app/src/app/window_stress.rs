//! Randomised stress of the windows (#475, step 2, L6): the root, up to
//! three detached graphs and a detachable workbook play seeded random passes
//! while the REAL server
//! thread ticks (its clock jumps a whole autosave period at each step), and
//! the invariants of the window mechanism are checked after every step.
//!
//! What a seed plays: graph gestures that mutate (add or remove a series,
//! change the title) through the window's own action path; Ctrl+Z / Ctrl+Y /
//! Ctrl+S in a random window; painted passes with pointer gestures; a dialog
//! opened from the root or from a graph, then Cancelled; detach, Reattach and
//! Close (a flag followed by a pass of the window, a pass of the root or
//! nothing, and the OS cross); edits of cells of the DOCKED workbook from the
//! root; stale echoes of closed ids; and long stretches where the root is
//! covered (no root pass at all) while the graphs keep acting. Since L2 the
//! second workbook detaches, reattaches and closes like a graph; digits are
//! typed in it and in the docked workbook; a press in another window ends the
//! entry; and a macro of the root opened with no dialog meets a click that
//! would commit the detached entry. Each seed runs
//! on the Wayland path (the windows close themselves) or on the root path
//! (X11, Windows), by its parity.
//!
//! What is checked: no hang (a watchdog), no panic of the thread, no tab
//! both docked and detached, the application's list of detached windows is
//! exactly what the windows are (and each has a live closure after a root
//! pass), an action of a window pass is applied by that very pass (and by
//! nothing while a dialog is open), a Cancel leaves the tree and the history
//! as before the dialog, a closed id is never woken, undoing everything gives
//! the initial tree back, and the last recovery copy reloads equal to the
//! project (a `.stf` round trip); at most one entry is open in the whole
//! application, a press elsewhere ends it (no dialog), and no cell commit is
//! absorbed into a macro the root holds (#21).
//!
//! **Growing it.** The panels (L3) will join as new kinds in [`Rig::step`]
//! (one `match` arm and a `fn` per kind) and in the window lists of
//! [`Rig::check`]; nothing else changes. Replay a failure
//! with `SF_STRESS_SEED=<seed>` (the seed is printed on failure).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use stokesforge_app_state::object_manager::VisibilityTarget;
use stokesforge_app_state::state::{Action, AutosaveJob, PlotSettingsTab};
use stokesforge_core::{Column, NodeId, Project, Tree};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesRef, SeriesSpec};
use stokesforge_session::Session;

use super::detached_book::book_viewport;
use super::detached_graph::graph_viewport;
use super::server::{serve_once, Clock, Repaint, RunJob, ServerThread};
use super::tab::Tab;
use super::test_support::bench::{Bench, Pass};
use super::window_band;
use super::StokesForgeApp;

/// The autosave period of the application, as the clock of the thread sees it.
const PERIOD_SECS: u64 = 300;

/// A small seeded generator (xorshift64*): the same seed replays the same run.
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

    fn coin(&mut self) -> bool {
        self.below(2) == 0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        (!items.is_empty()).then(|| items[self.below(items.len())])
    }
}

/// The windows the thread woke.
#[derive(Clone, Default)]
struct Woken(Arc<Mutex<HashSet<egui::ViewportId>>>);

impl Repaint for Woken {
    fn repaint(&self, window: egui::ViewportId) {
        if let Ok(mut woken) = self.0.lock() {
            woken.insert(window);
        }
    }
}

fn ctrl(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    }
}

fn os_close() -> egui::ViewportInfo {
    egui::ViewportInfo {
        events: vec![egui::ViewportEvent::Close],
        ..Default::default()
    }
}

/// The configuration folder of the process, redirected once (a save records the
/// recent files there).
fn isolate_config() {
    static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let dir = DIR.get_or_init(|| tempfile::tempdir().expect("config dir"));
    std::env::set_var(
        stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
        dir.path(),
    );
    std::env::set_var("XDG_CONFIG_HOME", dir.path());
}

/// What a Cancel must give back: the model and the history as they were.
struct Baseline {
    tree: Arc<Tree>,
    history: String,
}

struct Rig {
    seed: u64,
    wayland: bool,
    bench: Bench,
    dice: Dice,
    graphs: Vec<NodeId>,
    columns: Vec<NodeId>,
    book: NodeId,
    /// The workbook that detaches (the first stays docked).
    book2: NodeId,
    sheet2: NodeId,
    columns2: Vec<NodeId>,
    thread: ServerThread,
    offset: Arc<AtomicU64>,
    t0: Instant,
    woken: Woken,
    /// Root passes still to skip: the root is covered by a window.
    covered: usize,
    counter: usize,
    initial: Arc<Tree>,
    /// Ids of graph windows that are closed (neither listed nor registered).
    closed: HashSet<egui::ViewportId>,
    /// Taken when a dialog is opened by the stress, compared at its Cancel.
    baseline: Option<Baseline>,
}

impl Rig {
    fn new(seed: u64, wayland: bool) -> Self {
        isolate_config();
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::new("Stress").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, Some("Book1")).unwrap();
        let sheet = project.add_worksheet(book, Some("Sheet1")).unwrap();
        let columns: Vec<NodeId> = (0..4u32)
            .map(|k| {
                let values: Vec<f64> = (0..12u32).map(|i| f64::from(i * (k + 1))).collect();
                project
                    .add_column(sheet, None, None, Column::from_f64(&values))
                    .unwrap()
            })
            .collect();
        let book2 = project.add_empty_workbook(root, Some("Book2")).unwrap();
        let sheet2 = project.add_worksheet(book2, Some("Sheet1")).unwrap();
        let columns2: Vec<NodeId> = (0..2u32)
            .map(|k| {
                let values: Vec<f64> = (0..12u32).map(|i| f64::from(100 + i * (k + 3))).collect();
                project
                    .add_column(sheet2, None, None, Column::from_f64(&values))
                    .unwrap()
            })
            .collect();
        let graphs: Vec<NodeId> = ["One", "Two", "Three"]
            .iter()
            .enumerate()
            .map(|(k, name)| {
                let mut panel = PanelSpec::at(0, 0);
                panel
                    .series
                    .push(SeriesSpec::new_xy(columns[0], columns[k + 1]));
                project
                    .add_graph(root, None, PlotSpec::empty(*name).with_panels(panel))
                    .unwrap()
            })
            .collect();
        let session = Session::spawn(project, None).unwrap();
        let path = dir.path().join("keys.stf").to_string_lossy().into_owned();
        let mut state =
            stokesforge_app_state::state::AppState::new(session.handle(), path).expect("state");
        state.set_recovery_dir(Some(dir.path().join("recovery")));
        let mut app = StokesForgeApp::new(state);
        app.windows_close_themselves = wayland;
        // The workbook is DOCKED in the root; the graphs float, the first is
        // detached.
        app.open_tab(Tab::Book(book));
        app.open_tab(Tab::Book(book2));
        for graph in &graphs {
            app.open_tab(Tab::Graph(*graph));
        }
        app.detach_tab(Tab::Graph(graphs[0]));
        let mut bench = Bench::from_app(session, app, graphs[0]);
        bench.dir = Some(dir);
        let offset = Arc::new(AtomicU64::new(0));
        let t0 = Instant::now();
        let clock_offset = Arc::clone(&offset);
        let clock: Arc<Clock> =
            Arc::new(move || t0 + Duration::from_secs(clock_offset.load(Ordering::SeqCst)));
        let run: Arc<RunJob> = Arc::new(|job: &AutosaveJob| job.run());
        let woken = Woken::default();
        let thread = ServerThread::spawn_with(
            bench.app.shared(),
            woken.clone(),
            run,
            clock,
            Arc::new(|_: &[PathBuf]| {}),
        );
        bench.app.server = Some(thread.status());
        let initial = Arc::clone(bench.app.state.tree());
        let mut rig = Self {
            seed,
            wayland,
            bench,
            dice: Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            graphs,
            columns,
            book,
            book2,
            sheet2,
            columns2,
            thread,
            offset,
            t0,
            woken,
            covered: 0,
            counter: 0,
            initial,
            closed: HashSet::new(),
            baseline: None,
        };
        // The first passes: the root publishes, the windows paint.
        for _ in 0..3 {
            rig.root_pass(Pass::root().focused(true));
            for graph in rig.live() {
                rig.window_pass(graph, Pass::child());
            }
        }
        rig.initial = Arc::clone(rig.bench.app.state.tree());
        rig
    }

    // -- The windows ---------------------------------------------------------

    /// The graphs detached right now, in a fixed order.
    fn detached(&self) -> Vec<NodeId> {
        self.graphs
            .iter()
            .copied()
            .filter(|graph| self.bench.app.detached_graphs.contains_key(graph))
            .collect()
    }

    /// The detached graphs whose window the root has published.
    fn live(&self) -> Vec<NodeId> {
        self.detached()
            .into_iter()
            .filter(|graph| self.bench.has_window(graph_viewport(*graph)))
            .collect()
    }

    /// Is the second workbook in its own window, and has the root published it?
    fn book2_live(&self) -> bool {
        self.bench.app.detached_books.contains_key(&self.book2)
            && self.bench.has_window(book_viewport(self.book2))
    }

    /// A pass of the workbook's window, if it still has a closure: a pending
    /// Reattach or Close makes it leave in the pass before.
    fn book2_pass(&mut self, pass: Pass) {
        let id = book_viewport(self.book2);
        if self.bench.has_window(id) {
            let output = self.bench.child_in(id, pass);
            self.after_pass(&output);
        }
    }

    fn may_root(&self) -> bool {
        self.covered == 0
    }

    /// A closed id is never woken: nothing may ask for an immediate repaint of
    /// a window that is gone.
    fn after_pass(&self, output: &egui::FullOutput) {
        for id in &self.closed {
            if let Some(viewport) = output.viewport_output.get(id) {
                assert_ne!(
                    viewport.repaint_delay,
                    Duration::ZERO,
                    "a closed window was woken (seed {})",
                    self.seed
                );
            }
        }
    }

    fn root_pass(&mut self, pass: Pass) {
        let output = self.bench.root(pass);
        self.after_pass(&output);
        // A window the root no longer publishes ends at its next pass (egui
        // drops it; the root woke it once if it had registered itself).
        for graph in self.graphs.clone() {
            let id = graph_viewport(graph);
            if !self.bench.app.detached_graphs.contains_key(&graph) && self.bench.has_window(id) {
                let output = self.bench.child_in(id, Pass::child().focused(false));
                self.after_pass(&output);
            }
        }
        let id = book_viewport(self.book2);
        if !self.bench.app.detached_books.contains_key(&self.book2) && self.bench.has_window(id) {
            let output = self.bench.child_in(id, Pass::child().focused(false));
            self.after_pass(&output);
        }
        for graph in self.detached() {
            assert!(
                self.bench.has_window(graph_viewport(graph)),
                "a detached graph has no live window after a root pass (seed {})",
                self.seed
            );
        }
        if self.bench.app.detached_books.contains_key(&self.book2) {
            assert!(
                self.bench.has_window(id),
                "the detached workbook has no live window after a root pass (seed {})",
                self.seed
            );
        }
    }

    fn window_pass(&mut self, graph: NodeId, pass: Pass) {
        let output = self.bench.child_in(graph_viewport(graph), pass);
        self.after_pass(&output);
    }

    fn blocking(&self) -> bool {
        self.bench.app.state.open_dialog().is_some()
    }

    /// The model and the history right now.
    fn baseline(&self) -> Baseline {
        let state = &self.bench.app.state;
        let history = &state.snapshot().history;
        Baseline {
            tree: Arc::clone(state.tree()),
            history: format!(
                "{:?} {:?} {} {}",
                history.undo_label, history.redo_label, history.can_undo, history.can_redo
            ),
        }
    }

    // -- One step ------------------------------------------------------------

    fn step(&mut self) {
        self.bench.app.acquire();
        if self.covered > 0 {
            self.covered -= 1;
            if self.covered == 0 {
                // The window that covered the root goes away: it paints again.
                self.root_pass(Pass::root().focused(true));
            }
        } else if self.dice.below(100) < 4 {
            self.covered = 5 + self.dice.below(25);
        }
        let dialog_before = self.blocking().then(|| self.baseline());
        let roll = self.dice.below(100);
        match roll {
            0..=15 => self.gesture(),
            16..=22 => self.keys(),
            23..=28 => self.pointer(),
            29..=34 => self.dialog(),
            35..=42 => self.detach(),
            43..=50 => self.leave(true),
            51..=55 => self.leave(false),
            56..=61 => self.docked_edit(),
            62..=69 => self.plain_root(),
            70..=73 => self.sweep(),
            74..=76 => self.echo(),
            77..=81 => self.book_detach(),
            82..=86 => self.book_leave(),
            87..=92 => self.book_type(),
            93..=96 => self.book_click(),
            _ => self.root_macro(),
        }
        // A dialog open the whole step: no window and no key changed the model.
        if let Some(before) = dialog_before {
            if !(29..=34).contains(&roll) && self.blocking() {
                assert!(
                    **self.bench.app.state.tree() == *before.tree,
                    "the model changed while a dialog was open (step kind {roll}, seed {})",
                    self.seed
                );
            }
        }
        self.check();
        self.refresh_closed();
        self.bench.app.release();
        self.offset.fetch_add(PERIOD_SECS + 1, Ordering::SeqCst);
        self.thread.unpark();
        match self.dice.below(4) {
            0 => std::thread::yield_now(),
            1 => std::thread::sleep(Duration::from_micros(self.dice.below(1500) as u64)),
            _ => {}
        }
    }

    fn refresh_closed(&mut self) {
        self.closed = self
            .graphs
            .iter()
            .copied()
            .filter(|graph| !self.bench.app.detached_graphs.contains_key(graph))
            .map(graph_viewport)
            .filter(|id| !self.bench.has_window(*id))
            .collect();
        let id = book_viewport(self.book2);
        if !self.bench.app.detached_books.contains_key(&self.book2) && !self.bench.has_window(id) {
            self.closed.insert(id);
        }
    }

    // -- The kinds of step ---------------------------------------------------

    /// A graph gesture that mutates, through the window's action path: applied
    /// by that very pass, with no root pass, and refused while a dialog is
    /// open.
    fn gesture(&mut self) {
        let live = self.live();
        let Some(graph) = self.dice.pick(&live) else {
            return self.docked_edit();
        };
        self.counter += 1;
        let spec = self.bench.app.state.plot_spec(graph).expect("the graph");
        let action = match self.dice.below(3) {
            0 => Action::AddSeriesToGraph {
                graph,
                panel: 0,
                columns: vec![self.columns[1 + self.dice.below(3)]],
                plot_type: None,
            },
            1 if spec.series_count() > 1 => Action::RemovePlotObject {
                graph,
                target: VisibilityTarget::Series(SeriesRef::new(0, self.dice.below(1))),
            },
            _ => {
                let mut spec = spec;
                spec.title = format!("Title {}", self.counter);
                Action::SetPlotSpec { graph, spec }
            }
        };
        let blocked = self.blocking();
        let before = self.baseline();
        let revision = self.bench.app.state.revision();
        let shown = format!("{action:?}").chars().take(80).collect::<String>();
        self.bench.act_in(graph_viewport(graph), action);
        if blocked {
            assert!(
                **self.bench.app.state.tree() == *before.tree,
                "a window action changed the model under a dialog (seed {})",
                self.seed
            );
            assert!(
                window_band::text_at(&self.bench.ctx, graph_viewport(graph), Instant::now())
                    .is_some(),
                "the refused window does not say why (seed {})",
                self.seed
            );
        } else {
            assert_ne!(
                self.bench.app.state.revision(),
                revision,
                "an action of a window pass was not applied by that pass: {shown} (seed {})",
                self.seed
            );
        }
    }

    /// Ctrl+Z, Ctrl+Y or Ctrl+S in a random window.
    fn keys(&mut self) {
        let key = [egui::Key::Z, egui::Key::Y, egui::Key::S][self.dice.below(3)];
        let live = self.live();
        if self.book2_live() && self.dice.below(3) == 0 {
            return self.book2_pass(Pass::child().events(vec![ctrl(key)]));
        }
        match self.dice.pick(&live) {
            Some(graph) if !self.may_root() || self.dice.coin() => {
                self.window_pass(graph, Pass::child().events(vec![ctrl(key)]));
            }
            _ if self.may_root() => {
                self.root_pass(Pass::root().focused(true).events(vec![ctrl(key)]))
            }
            _ => {}
        }
    }

    /// A painted pass of a window with a pointer gesture (move, click, wheel).
    fn pointer(&mut self) {
        let live = self.live();
        let Some(graph) = self.dice.pick(&live) else {
            return;
        };
        let pos = egui::pos2(
            100.0 + self.dice.below(1000) as f32,
            100.0 + self.dice.below(600) as f32,
        );
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if self.dice.coin() {
            events.push(button(true));
            events.push(button(false));
        } else {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, if self.dice.coin() { 1.0 } else { -1.0 }),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
        self.window_pass(graph, Pass::child().events(events));
    }

    /// A dialog opened (from the root or from a graph) or, if one is open,
    /// Cancelled: the tree and the history are as before it, and the window of
    /// the gesture says where the dialog is.
    fn dialog(&mut self) {
        if self.blocking() {
            // The dialog belongs to the root: it is answered there, and not
            // while a window covers the root.
            if self.may_root() {
                self.cancel();
            }
            return;
        }
        // An entry open in the docked grid closes when a dialog takes the focus
        // (#465): do it before the macro of the dialog opens, as the gesture
        // would.
        self.bench.app.state.commit_open_entries();
        let graph = self.graphs[self.dice.below(self.graphs.len())];
        let open = Action::OpenPlotSettings {
            graph,
            tab: PlotSettingsTab::Graph,
            axis: None,
        };
        let live = self.live();
        let baseline = self.baseline();
        match self.dice.pick(&live) {
            Some(window) if self.dice.coin() || !self.may_root() => {
                let id = graph_viewport(window);
                let open = match open {
                    Action::OpenPlotSettings { tab, axis, .. } => Action::OpenPlotSettings {
                        graph: window,
                        tab,
                        axis,
                    },
                    other => other,
                };
                self.bench.act_in(id, open);
                if self.bench.app.state.plot_settings().is_some() {
                    self.baseline = Some(baseline);
                    assert!(
                        window_band::text_at(&self.bench.ctx, id, Instant::now()).is_some(),
                        "the window of the gesture has no band (seed {})",
                        self.seed
                    );
                    assert!(
                        window_band::text_at(
                            &self.bench.ctx,
                            egui::ViewportId::ROOT,
                            Instant::now()
                        )
                        .is_none(),
                        "the root says where the dialog is (seed {})",
                        self.seed
                    );
                }
            }
            _ if !self.may_root() => {}
            _ => {
                self.bench.app.state.dispatch(open);
                if self.bench.app.state.plot_settings().is_some() {
                    self.baseline = Some(baseline);
                }
            }
        }
        if self.may_root() {
            self.root_pass(Pass::root().focused(true));
        }
    }

    fn cancel(&mut self) {
        if self.bench.app.state.plot_settings().is_some() {
            self.bench.app.state.cancel_plot_settings();
        } else if self.bench.app.state.about_open() {
            self.bench.app.state.dispatch(Action::CloseAbout);
        } else {
            // Another dialog, opened by a gesture: Esc closes it.
            self.root_pass(
                Pass::root()
                    .focused(true)
                    .events(vec![super::test_support::key(egui::Key::Escape)]),
            );
        }
        if let Some(before) = self.baseline.take() {
            let after = self.baseline();
            assert!(
                *after.tree == *before.tree && after.history == before.history,
                "Cancel did not give the model and the history back (seed {})",
                self.seed
            );
        }
        if self.may_root() {
            self.root_pass(Pass::root().focused(true));
        }
    }

    /// A graph that is not detached goes to its own window.
    fn detach(&mut self) {
        let free: Vec<NodeId> = self
            .graphs
            .iter()
            .copied()
            .filter(|graph| !self.bench.app.detached_graphs.contains_key(graph))
            .collect();
        let Some(graph) = self.dice.pick(&free) else {
            return;
        };
        let tab = Tab::Graph(graph);
        if !self.bench.app.floating_tabs.contains(&tab) {
            self.bench.app.open_tab(tab);
        }
        self.bench.app.detach_tab(tab);
        self.closed.remove(&graph_viewport(graph));
        if self.may_root() {
            self.root_pass(Pass::root());
        }
    }

    /// Reattach (`reattach`) or Close of a detached graph: the flag of its
    /// button or its cross, followed by a pass of the window, of the root, of
    /// the OS cross, or by nothing.
    fn leave(&mut self, reattach: bool) {
        let detached = self.detached();
        let Some(graph) = self.dice.pick(&detached) else {
            return;
        };
        let (tab, id) = (Tab::Graph(graph), graph_viewport(graph));
        let window = self.bench.app.detached_graphs.get_mut(&graph).unwrap();
        // A flag left pending by an earlier step may be of the other kind.
        let fresh = !(window.reattach || window.close);
        if reattach {
            window.reattach = true;
        } else {
            window.close = true;
        }
        let own_pass = self.bench.has_window(id);
        let arm_pick = self.dice.below(4);
        match arm_pick {
            0 if own_pass => {
                self.window_pass(graph, Pass::child());
                if self.wayland {
                    assert!(
                        !self.bench.app.is_tab_detached(tab),
                        "the window did not leave in its own pass (seed {})",
                        self.seed
                    );
                }
            }
            1 if own_pass && !reattach => {
                self.window_pass(graph, Pass::child().info(os_close()));
                if self.wayland {
                    assert!(
                        !self.bench.app.is_tab_detached(tab),
                        "the cross did not close the window in its own pass (seed {})",
                        self.seed
                    );
                }
            }
            2 if self.may_root() => {
                self.root_pass(Pass::root());
                assert!(
                    !self.bench.app.is_tab_detached(tab),
                    "the root pass did not carry out the flag (seed {})",
                    self.seed
                );
            }
            _ => return,
        }
        if fresh && !self.bench.app.is_tab_detached(tab) {
            assert_eq!(
                self.bench.app.floating_tabs.contains(&tab),
                reattach,
                "Reattach floats the tab, Close does not (seed {})",
                self.seed
            );
        }
    }

    /// A cell of the DOCKED workbook edited from the root.
    fn docked_edit(&mut self) {
        if !self.may_root() || self.blocking() {
            return;
        }
        self.counter += 1;
        let column = self.columns[self.dice.below(self.columns.len())];
        let revision = self.bench.app.state.revision();
        self.bench.app.state.dispatch(Action::CommitCell {
            column,
            row: self.dice.below(12),
            text: (1000 + self.counter).to_string(),
        });
        assert_ne!(
            self.bench.app.state.revision(),
            revision,
            "a cell edit was lost (seed {})",
            self.seed
        );
        self.root_pass(Pass::root().focused(true));
    }

    fn plain_root(&mut self) {
        if self.may_root() {
            let focused = self.dice.coin();
            self.root_pass(Pass::root().focused(focused));
        }
    }

    // -- The workbook (L2) ------------------------------------------------------

    /// The second workbook goes to its own window.
    fn book_detach(&mut self) {
        let tab = Tab::Book(self.book2);
        if self.bench.app.detached_books.contains_key(&self.book2) {
            return;
        }
        if !self.bench.app.floating_tabs.contains(&tab) {
            self.bench.app.open_tab(tab);
        }
        self.bench.app.detach_tab(tab);
        self.closed.remove(&book_viewport(self.book2));
        if self.may_root() {
            self.root_pass(Pass::root());
        }
    }

    /// Reattach or Close of the detached workbook, as for a graph: the flag,
    /// then a pass of the window, the OS cross, a pass of the root, or nothing.
    fn book_leave(&mut self) {
        let (tab, id) = (Tab::Book(self.book2), book_viewport(self.book2));
        let reattach = self.dice.coin();
        let Some(window) = self.bench.app.detached_books.get_mut(&self.book2) else {
            return;
        };
        let fresh = !(window.reattach || window.close);
        if reattach {
            window.reattach = true;
        } else {
            window.close = true;
        }
        let own_pass = self.bench.has_window(id);
        match self.dice.below(4) {
            0 if own_pass => {
                self.book2_pass(Pass::child());
                if self.wayland {
                    assert!(
                        !self.bench.app.is_tab_detached(tab),
                        "the workbook did not leave in its own pass (seed {})",
                        self.seed
                    );
                }
            }
            1 if own_pass && !reattach => {
                self.book2_pass(Pass::child().info(os_close()));
                if self.wayland {
                    assert!(
                        !self.bench.app.is_tab_detached(tab),
                        "the cross did not close the workbook in its own pass (seed {})",
                        self.seed
                    );
                }
            }
            2 if self.may_root() => {
                self.root_pass(Pass::root());
                assert!(
                    !self.bench.app.is_tab_detached(tab),
                    "the root pass did not carry out the flag (seed {})",
                    self.seed
                );
            }
            _ => return,
        }
        if fresh && !self.bench.app.is_tab_detached(tab) {
            assert_eq!(
                self.bench.app.floating_tabs.contains(&tab),
                reattach,
                "Reattach floats the workbook, Close does not (seed {})",
                self.seed
            );
        }
    }

    /// Digits typed in a cell of the docked workbook (root passes) or of the
    /// detached one (its window): the entry stays open.
    fn book_type(&mut self) {
        let detached = self.book2_live() && (self.dice.coin() || !self.may_root());
        // The docked grid is behind the modal of a dialog of the root.
        if !detached && (!self.may_root() || self.blocking()) {
            return;
        }
        let (book, columns) = if detached {
            (self.book2, self.columns2.clone())
        } else {
            (self.book, self.columns.clone())
        };
        let Some(sheet) = self.bench.app.state.selected_sheet_in(book) else {
            return;
        };
        let column = columns[self.dice.below(columns.len())];
        self.bench
            .app
            .state
            .select_cell(sheet, column, self.dice.below(12));
        let digits = 1 + self.dice.below(3);
        for _ in 0..digits {
            let digit = (b'0' + u8::try_from(self.dice.below(10)).unwrap_or(0)) as char;
            let events = vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                egui::Event::Text(digit.to_string()),
            ];
            for step in 0..3 {
                let events = if step == 0 {
                    events.clone()
                } else {
                    Vec::new()
                };
                if detached {
                    self.book2_pass(Pass::child().events(events));
                } else {
                    self.root_pass(Pass::root().focused(true).events(events));
                }
            }
        }
    }

    /// A press in a window: the entry held elsewhere ends (one entry in the
    /// whole application), unless a dialog is open in the main window. A press
    /// in the detached workbook lands on a painted cell; in a graph window or in
    /// the root, anywhere.
    fn book_click(&mut self) {
        let blocked = self.blocking();
        let holder_docked = self
            .bench
            .app
            .state
            .editing()
            .is_some_and(|edit| self.columns.contains(&edit.column));
        let entry = self.bench.app.state.editing().is_some();
        let live = self.live();
        let press = |pos: egui::Pos2| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let pos = egui::pos2(
            100.0 + self.dice.below(800) as f32,
            100.0 + self.dice.below(500) as f32,
        );
        match self.dice.below(3) {
            0 if self.book2_live() => {
                let output = self
                    .bench
                    .child_in(book_viewport(self.book2), Pass::child());
                if !self.bench.has_window(book_viewport(self.book2)) {
                    return;
                }
                let cells: Vec<egui::Pos2> = super::test_support::texts(&output.shapes)
                    .into_iter()
                    .filter(|text| text.text.chars().all(|c| c.is_ascii_digit()))
                    .map(|text| text.center)
                    .collect();
                let target = self.dice.pick(&cells).unwrap_or(pos);
                self.book2_pass(Pass::child().events(press(target)));
                if entry && holder_docked && !blocked {
                    assert!(
                        self.bench.app.state.editing().is_none(),
                        "a press in the detached workbook left the entry of another window \
                         open (seed {})",
                        self.seed
                    );
                }
            }
            1 => {
                let Some(graph) = self.dice.pick(&live) else {
                    return;
                };
                self.window_pass(graph, Pass::child().events(press(pos)));
                if entry && !blocked {
                    assert!(
                        self.bench.app.state.editing().is_none(),
                        "a press in a graph window left the entry open (seed {})",
                        self.seed
                    );
                }
            }
            // A press in the root under its own dialog is the dialog's: the
            // modal takes it (a docked entry then closes by the rule of #465).
            _ if self.may_root() && !blocked => {
                self.root_pass(Pass::root().focused(true).events(press(pos)));
            }
            _ => {}
        }
    }

    /// #21: an entry is open in the detached workbook, the root opens a macro
    /// with NO dialog, and a click in the workbook would commit the entry. The
    /// commit is refused: the cell is unchanged while the macro is open, and
    /// the macro's rollback gives back the model and the history.
    fn root_macro(&mut self) {
        if self.blocking() || !self.book2_live() {
            return;
        }
        let column = self.columns2[self.dice.below(self.columns2.len())];
        let row = self.dice.below(12);
        self.bench.app.state.select_cell(self.sheet2, column, row);
        for digit in ["4", "2"] {
            let events = vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                egui::Event::Text(digit.into()),
            ];
            self.book2_pass(Pass::child().events(events));
            for _ in 0..3 {
                self.book2_pass(Pass::child());
            }
        }
        if self
            .bench
            .app
            .state
            .editing()
            .is_none_or(|edit| edit.column != column || edit.row != row)
        {
            return;
        }
        let before = self.baseline();
        let text = self.bench.app.state.cell_text(column, row);
        self.bench.app.state.open_root_macro_for_test();
        if !self.bench.has_window(book_viewport(self.book2)) {
            return;
        }
        let output = self
            .bench
            .child_in(book_viewport(self.book2), Pass::child());
        let others: Vec<egui::Pos2> = super::test_support::texts(&output.shapes)
            .into_iter()
            .filter(|t| t.text.chars().all(|c| c.is_ascii_digit()) && t.text != "42")
            .map(|t| t.center)
            .collect();
        if let Some(target) = self.dice.pick(&others) {
            for pressed in [true, false] {
                self.book2_pass(Pass::child().events(vec![
                    egui::Event::PointerMoved(target),
                    egui::Event::PointerButton {
                        pos: target,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]));
            }
        }
        assert_eq!(
            self.bench.app.state.cell_text(column, row),
            text,
            "a cell commit was absorbed into a macro of the root (seed {})",
            self.seed
        );
        self.bench.app.state.cancel_plot_settings();
        let after = self.baseline();
        assert!(
            *after.tree == *before.tree && after.history == before.history,
            "rolling back the root macro did not give the model and the history back (seed {})",
            self.seed
        );
    }

    /// A bare pass of every window, none of them focused.
    fn sweep(&mut self) {
        for graph in self.live() {
            self.window_pass(graph, Pass::child().focused(false));
        }
        if self.book2_live() {
            self.book2_pass(Pass::child().focused(false));
        }
    }

    /// The echo of a closed window in the root's input, 150 ms late.
    fn echo(&mut self) {
        let closed: Vec<egui::ViewportId> = self.closed.iter().copied().collect();
        let mut closed = closed;
        closed.sort_by_key(|id| format!("{id:?}"));
        if let Some(id) = self.dice.pick(&closed) {
            self.bench.stale.push(id);
            if self.may_root() {
                self.root_pass(Pass::root());
            }
        }
    }

    // -- The invariants -------------------------------------------------------

    fn check(&self) {
        let app = &self.bench.app;
        let seed = self.seed;
        assert!(
            !self.thread.status().panicked(),
            "the server thread panicked (seed {seed})"
        );
        for graph in &self.graphs {
            let tab = Tab::Graph(*graph);
            let detached = app.detached_graphs.contains_key(graph);
            assert!(
                !(detached && app.floating_tabs.contains(&tab)),
                "{tab:?} is both docked and detached (seed {seed})"
            );
            assert!(
                !(detached && app.graph_views.contains_key(graph)),
                "{tab:?} has its view in two places (seed {seed})"
            );
        }
        let book = Tab::Book(self.book2);
        assert!(
            !(app.detached_books.contains_key(&self.book2) && app.floating_tabs.contains(&book)),
            "the workbook is both docked and detached (seed {seed})"
        );
        assert!(
            !app.detached_books.contains_key(&self.book),
            "the docked workbook detached by itself (seed {seed})"
        );
        // One entry in the whole application, whatever the window.
        assert!(
            !(app.state.editing().is_some() && app.state.header_editing().is_some()),
            "a cell entry and a header entry are open at once (seed {seed})"
        );
        let mut seen = HashSet::new();
        assert!(
            app.floating_tabs.iter().all(|tab| seen.insert(*tab)),
            "a tab floats twice (seed {seed})"
        );
        // The list of the application is the truth about which windows exist.
        let listed: HashSet<egui::ViewportId> = app.open_viewports().into_iter().collect();
        let mut expected: HashSet<egui::ViewportId> =
            self.detached().into_iter().map(graph_viewport).collect();
        if app.detached_books.contains_key(&self.book2) {
            expected.insert(book_viewport(self.book2));
        }
        assert_eq!(
            listed, expected,
            "the open windows are not the detached ones (seed {seed})"
        );
    }

    // -- The end ----------------------------------------------------------------

    fn finish(mut self) {
        let seed = self.seed;
        self.bench.app.acquire();
        if self.blocking() {
            self.cancel();
        }
        self.covered = 0;
        for _ in 0..3 {
            self.root_pass(Pass::root().focused(true));
            self.sweep();
        }
        // Undoing everything gives the initial tree back.
        let mut undone = 0;
        while self.bench.app.state.can_undo() {
            self.bench.app.state.dispatch(Action::Undo);
            undone += 1;
            assert!(undone < 10_000, "the history never ends (seed {seed})");
        }
        assert!(
            **self.bench.app.state.tree() == *self.initial,
            "undo x{undone} did not give the initial tree back (seed {seed})"
        );
        // A last change, so that a recovery copy of the final state must exist.
        self.bench.app.state.dispatch(Action::AddWorkbook);
        self.bench.app.release();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.thread.unpark();
            self.bench.app.acquire();
            let busy = {
                let state = &self.bench.app.state;
                state.import_progress().is_some()
                    || state.import_is_applying()
                    || state.recalc_work().is_some()
            };
            self.bench.app.release();
            if !busy {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the work did not end (seed {seed})"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        // Quit as the application does: the guard released, then the thread joined.
        self.bench.app.acquire();
        let shared = self.bench.app.shared();
        assert!(
            self.thread
                .shut_down(&mut self.bench.app, Duration::from_secs(30)),
            "the thread ended (seed {seed})"
        );
        assert!(
            !self.thread.status().panicked(),
            "the thread panicked (seed {seed})"
        );
        // Only the root and the windows of the application were ever woken.
        let allowed: HashSet<egui::ViewportId> = std::iter::once(egui::ViewportId::ROOT)
            .chain(self.graphs.iter().copied().map(graph_viewport))
            .chain(std::iter::once(book_viewport(self.book2)))
            .collect();
        let woken = self.woken.0.lock().unwrap().clone();
        assert!(
            woken.is_subset(&allowed),
            "the thread woke a window that never existed (seed {seed})"
        );
        // The last pass of the thread is gone: write the copy of the final state.
        let t0_far =
            self.t0 + Duration::from_secs(self.offset.load(Ordering::SeqCst) + 2 * PERIOD_SECS);
        let record = |_: &[PathBuf]| {};
        let run = |job: &AutosaveJob| job.run();
        serve_once(&shared, t0_far, &Woken::default(), &run, &record);
        serve_once(
            &shared,
            t0_far + Duration::from_secs(PERIOD_SECS + 1),
            &Woken::default(),
            &run,
            &record,
        );
        let dir = self.bench.path().parent().unwrap().to_path_buf();
        let written: Vec<PathBuf> = std::fs::read_dir(dir.join("recovery"))
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
        let other = Session::spawn(Project::new("Other").unwrap(), None).unwrap();
        other.handle().load(&written[0]).expect("the copy reloads");
        let current = Arc::clone(&self.bench.session.handle().snapshot().unwrap().tree);
        let reloaded = Arc::clone(&other.handle().snapshot().unwrap().tree);
        assert!(
            *reloaded == *current,
            "the recovery copy reloads equal to the project (seed {seed})"
        );
        other.shutdown().unwrap();
        drop(shared);
        self.bench.finish();
    }
}

/// The seed being played, for the message of the watchdog.
static RUNNING_SEED: AtomicU64 = AtomicU64::new(0);

/// Runs `body` on a thread with a watchdog: a deadlock fails the test instead
/// of hanging it, a panic is passed on.
fn with_watchdog(limit: Duration, body: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(limit) {
        Ok(Ok(())) => {}
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => panic!(
            "deadlock: the run did not end within {limit:?} (SF_STRESS_SEED={})",
            RUNNING_SEED.load(Ordering::SeqCst)
        ),
    }
}

/// One run of `steps` random steps; the seed is printed on any failure.
fn stress_run(seed: u64, steps: usize) {
    let wayland = seed.is_multiple_of(2);
    RUNNING_SEED.store(seed, Ordering::SeqCst);
    let result = std::panic::catch_unwind(|| {
        let mut rig = Rig::new(seed, wayland);
        for _ in 0..steps {
            rig.step();
        }
        rig.finish();
    });
    if let Err(panic) = result {
        eprintln!("window stress failed: SF_STRESS_SEED={seed} (wayland path: {wayland})");
        std::panic::resume_unwind(panic);
    }
}

fn env_seed() -> Option<u64> {
    std::env::var("SF_STRESS_SEED").ok()?.parse().ok()
}

/// Short variant, in every run: one seed on each path (the Wayland path where
/// the windows close themselves, the root path elsewhere). `SF_STRESS_SEED`
/// replaces the first seed.
#[test]
fn random_window_passes_keep_the_invariants() {
    let first = env_seed().unwrap_or(0x57_0475);
    with_watchdog(Duration::from_secs(120), move || {
        for seed in [first & !1, first | 1] {
            stress_run(seed, 120);
        }
    });
}

mod slow {
    use super::*;

    /// Long variant (`cargo test -p stokesforge-app --lib window_stress::slow -- --ignored`):
    /// many seeds, many steps.
    #[test]
    #[ignore = "long: 12 seeds x 600 steps"]
    fn many_random_window_passes_keep_the_invariants() {
        with_watchdog(Duration::from_secs(1800), || {
            for seed in 1..=12u64 {
                stress_run(seed * 0x9E37_79B9, 600);
            }
        });
    }
}

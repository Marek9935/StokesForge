//! The window that paints the dialogs of the application (#457).
//!
//! A dialog is painted by the pass of one window, the one the user works in. A
//! detached window can cover the root; the root then runs `ui` unpainted about
//! every 100 ms, and a dialog it painted would show late or never. So while a
//! detached window has the focus, the dialogs are painted by **that window's**
//! deferred closure, through the same code, with the same state: no second
//! writing of any dialog, no copy of the state.
//!
//! # How the closure reaches the application
//!
//! `AppHost` shares the [`StokesForgeApp`] through `Rc<RefCell<_>>` so that the
//! closure of a deferred viewport, which must be `'static + Send + Sync` and
//! captures nothing of the application, can reach it again on the main thread
//! through a `thread_local!` `Weak` ([`with_app`]). Every closure runs on the
//! main thread, from the pass of its own viewport and never inside the root's
//! `ui` in production; `try_borrow_mut` makes any nesting (embedded viewports
//! in tests) a no-op instead of a panic. No `unsafe`. Without an `AppHost`
//! (most tests) the `Weak` is dead and [`with_app`] does nothing.
//!
//! # The rule
//!
//! - [`HostState::host`] is `None` (the root paints) or one detached window.
//!   While no dialog is shown it follows the focused window (root first, then
//!   the latest focus reported by a closure); while one is shown it stays. A
//!   host that no longer exists, or is smaller than [`HOST_MIN_SIZE`] (the
//!   footer buttons must fit), falls back to the root.
//! - Each detached closure ends with [`host_pass`]. When it is the host, it
//!   paints `show_edit_dialogs` and the confirmations, then **dispatches the
//!   actions itself**: a queue to the root would leave an answered dialog
//!   painted for up to 110 ms under cover, and a second click would answer it
//!   twice. The root is woken after.
//! - The root skips both painters while a host exists, but still empties the
//!   actions of the frame for a blocking dialog.
//! - While a hosted dialog is open the root paints a veil ([`dialogs::paint_veil`])
//!   so the main window cannot change the model under it. A press on the veil
//!   moves the dialog to the root: the way out when the host is covered.
//! - Any dialog of the application is painted through this host, never
//!   directly in a closure.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use eframe::egui;

use std::collections::HashMap;

use super::tab::Tab;
use super::window_detach::snapshot_stamp;
use super::{dialogs, StokesForgeApp};

thread_local! {
    static APP: RefCell<Weak<RefCell<StokesForgeApp>>> = const { RefCell::new(Weak::new()) };
}

/// What eframe drives: forwards the three overrides of [`StokesForgeApp`].
pub struct AppHost(Rc<RefCell<StokesForgeApp>>);

impl AppHost {
    /// Wraps `app` and publishes it to the closures of this thread.
    #[must_use]
    pub fn new(app: StokesForgeApp) -> Self {
        let shared = Rc::new(RefCell::new(app));
        APP.with(|cell| *cell.borrow_mut() = Rc::downgrade(&shared));
        Self(shared)
    }
}

/// Runs `f` on the application when the call comes from the main thread of a
/// live [`AppHost`] and the application is not already borrowed; otherwise does
/// nothing and returns `None`.
pub(super) fn with_app<R>(f: impl FnOnce(&mut StokesForgeApp) -> R) -> Option<R> {
    let shared = APP.with(|cell| cell.borrow().upgrade())?;
    let mut app = shared.try_borrow_mut().ok()?;
    Some(f(&mut app))
}

impl eframe::App for AppHost {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Ok(mut app) = self.0.try_borrow_mut() {
            app.logic(ctx, frame);
        }
    }

    fn on_exit(&mut self) {
        if let Ok(mut app) = self.0.try_borrow_mut() {
            app.on_exit();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Ok(mut app) = self.0.try_borrow_mut() {
            app.ui(ui, frame);
        }
    }
}

/// Below this size a window does not host a dialog: the footer buttons of the
/// smallest class (480 x 170) must fit inside it with room for the header.
pub(super) const HOST_MIN_SIZE: egui::Vec2 = egui::vec2(560.0, 170.0);

/// A window that can paint the dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum DialogHost {
    Tab(Tab),
    ProjectManager,
    ObjectManager,
    Console,
}

impl DialogHost {
    /// The viewport of the window, to wake it.
    pub(super) fn viewport(self) -> egui::ViewportId {
        match self {
            Self::Tab(tab) => egui::ViewportId::from_hash_of(tab),
            Self::ProjectManager => {
                egui::ViewportId::from_hash_of("stokesforge_project_manager_window")
            }
            Self::ObjectManager => {
                egui::ViewportId::from_hash_of("stokesforge_object_manager_window")
            }
            Self::Console => egui::ViewportId::from_hash_of("stokesforge_script_window"),
        }
    }
}

/// What the closure of a window last said about it.
#[derive(Debug, Clone, Copy, Default)]
struct Report {
    focused: bool,
    size: egui::Vec2,
    /// Rank of the latest focus gain, among all the windows: the latest wins.
    since: u64,
}

/// Which window paints the dialogs, and what it was told to decide.
#[derive(Debug, Default)]
pub(super) struct HostState {
    /// `None`: the root paints.
    pub(super) host: Option<DialogHost>,
    reports: HashMap<DialogHost, Report>,
    focus_seq: u64,
    /// The host painted a modal in its latest pass: the root keeps a veil.
    pub(super) open: bool,
    /// The root painted a modal in its latest pass.
    pub(super) root_painted: bool,
}

impl StokesForgeApp {
    fn host_exists(&self, host: DialogHost) -> bool {
        match host {
            DialogHost::Tab(tab) => self.detached_tabs.contains_key(&tab),
            DialogHost::ProjectManager => self.pm_detached,
            DialogHost::ObjectManager => self.om_detached,
            DialogHost::Console => self.script_detached,
        }
    }

    fn host_fits(&self, host: DialogHost) -> bool {
        self.dialog_hosting
            .reports
            .get(&host)
            .is_some_and(|report| {
                report.size.x >= HOST_MIN_SIZE.x && report.size.y >= HOST_MIN_SIZE.y
            })
    }

    /// A host that is gone (window closed, project load, reattach) or too small
    /// falls back to the root, so the dialog paints there.
    fn drop_unusable_host(&mut self) {
        if let Some(host) = self.dialog_hosting.host {
            if !self.host_exists(host) || !self.host_fits(host) {
                self.move_dialog_to_root();
            }
        }
    }

    /// The dialog shown by the host is the root's from now on. It counts as
    /// shown there at once, so the next pass does not hand it to a focused
    /// window before the root has painted it.
    fn move_dialog_to_root(&mut self) {
        let state = &mut self.dialog_hosting;
        state.root_painted |= state.open;
        state.host = None;
        state.open = false;
    }

    /// Chooses the window that paints the dialogs of this root pass. Called once
    /// the detached windows are drained (a reattach or close is then known).
    pub(super) fn update_dialog_host(&mut self, ctx: &egui::Context) {
        self.drop_unusable_host();
        let state = &self.dialog_hosting;
        if state.open || state.root_painted {
            // A dialog is shown: it stays where it is.
            return;
        }
        // The root first: its own `input.focused`, not the stale
        // `viewports[id].focused`. Then the latest focus a closure reported.
        if ctx.input(|input| input.focused) {
            self.dialog_hosting.host = None;
            return;
        }
        let focused = state
            .reports
            .iter()
            .filter(|(host, report)| report.focused && self.host_exists(**host))
            .max_by_key(|(_, report)| report.since)
            .map(|(host, _)| *host);
        // No window focused (an overwrite question after a file picker lands
        // behind): the host is unchanged, not the root.
        if let Some(host) = focused {
            self.dialog_hosting.host = self.host_fits(host).then_some(host);
        }
    }

    /// Wakes the host after a root pass that may have opened a dialog for it:
    /// an action dispatched, a model change, an alert from a job, a dialog open.
    /// At rest nothing runs, so nothing wakes (#104).
    pub(super) fn wake_dialog_host(&self, ctx: &egui::Context, dispatched: bool, changed: bool) {
        let Some(host) = self.dialog_hosting.host else {
            return;
        };
        if dispatched || changed || self.dialog_hosting.open || self.state.blocking_dialog() {
            ctx.request_repaint_of(host.viewport());
        }
    }

    /// The root's veil over a dialog hosted elsewhere; a press moves the dialog
    /// to the root.
    pub(super) fn hosted_dialog_veil(&mut self, ctx: &egui::Context) {
        if self.dialog_hosting.host.is_none() || !self.dialog_hosting.open {
            return;
        }
        if dialogs::paint_veil(ctx) {
            self.move_dialog_to_root();
            ctx.request_repaint();
        }
    }

    /// The end of a detached window's pass: reports its focus and size, and
    /// paints the dialogs when it is the host.
    fn hosted_pass(&mut self, ctx: &egui::Context, host: DialogHost) {
        let (focused, size) = ctx.input(|input| (input.focused, input.content_rect().size()));
        let seq = self.dialog_hosting.focus_seq + 1;
        let report = self.dialog_hosting.reports.entry(host).or_default();
        if focused && !report.focused {
            report.since = seq;
            self.dialog_hosting.focus_seq = seq;
        }
        report.focused = focused;
        report.size = size;
        if self.dialog_hosting.host != Some(host) {
            return;
        }
        self.drop_unusable_host();
        if self.dialog_hosting.host != Some(host) {
            // Too small to hold the dialog: the root paints it.
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            return;
        }
        let stamp = snapshot_stamp(self.state.snapshot());
        let was_open = self.dialog_hosting.open;
        let mut actions = Vec::new();
        let _ = dialogs::take_modal_painted(ctx);
        let language_changed = self.show_edit_dialogs(ctx, &mut actions);
        if self.state.blocking_dialog() {
            actions.clear();
        }
        super::confirmations::show(ctx, &self.state, &mut actions);
        self.dialog_hosting.open = dialogs::take_modal_painted(ctx);
        let dispatched = !actions.is_empty();
        if dispatched {
            self.dispatch_actions(ctx, actions);
            self.drop_unusable_host();
        }
        if snapshot_stamp(self.state.snapshot()) != stamp {
            self.republish_detached_tabs(ctx);
        }
        if dispatched || language_changed || was_open != self.dialog_hosting.open {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if dispatched {
            ctx.request_repaint();
        }
    }
}

/// Called at the end of the closure of a detached window, with **no lock of
/// the window held** (the dispatch reads them). Does nothing without a live
/// [`AppHost`] or when the application is already borrowed.
pub(super) fn host_pass(ctx: &egui::Context, host: DialogHost) {
    let _ = with_app(|app| app.hosted_pass(ctx, host));
}

#[cfg(test)]
mod tests {
    //! The host rule, driven the way eframe drives it: root passes through the
    //! `AppHost`, child passes through the callbacks the root published. A pass
    //! of the root is counted: the point of the host is that a covered root,
    //! which runs about every 100 ms, is not needed once a dialog is open.
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use eframe::App as _;
    use stokesforge_app_state::state::{Action, AppState, PlotSettingsTab};
    use stokesforge_app_state::tr;
    use stokesforge_core::{NodeId, Project};
    use stokesforge_session::Session;

    use super::super::graph_window::tests::plotted_graph;
    use super::super::test_support::{painted, text_exact, texts};
    use super::*;
    use crate::theme;

    type Callback = Arc<dyn Fn(&mut egui::Ui) + Sync + Send>;

    /// What one pass is fed with.
    #[derive(Clone)]
    struct Pass {
        events: Vec<egui::Event>,
        focused: bool,
        size: egui::Vec2,
        dt: f64,
    }

    impl Pass {
        /// A root pass: not focused (a detached window is), 1400 x 900.
        fn root() -> Self {
            Self {
                events: Vec::new(),
                focused: false,
                size: egui::vec2(1400.0, 900.0),
                dt: 0.05,
            }
        }

        /// A child pass: focused, 1400 x 900.
        fn child() -> Self {
            Self {
                focused: true,
                ..Self::root()
            }
        }

        fn events(mut self, events: Vec<egui::Event>) -> Self {
            self.events = events;
            self
        }

        fn focused(mut self, focused: bool) -> Self {
            self.focused = focused;
            self
        }

        fn size(mut self, width: f32, height: f32) -> Self {
            self.size = egui::vec2(width, height);
            self
        }

        fn dt(mut self, dt: f64) -> Self {
            self.dt = dt;
            self
        }
    }

    struct Bench {
        session: Session,
        host: AppHost,
        ctx: egui::Context,
        frame: eframe::Frame,
        callbacks: HashMap<egui::ViewportId, Callback>,
        time: f64,
        graph: NodeId,
        tab: Tab,
        root_passes: usize,
    }

    impl Bench {
        /// A project with one graph, detached and not yet painted.
        fn new() -> Self {
            let (session, graph) = plotted_graph();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let mut app = StokesForgeApp::new(state);
            let tab = Tab::Graph(graph);
            app.detach_tab(tab);
            let host = AppHost::new(app);
            let ctx = egui::Context::default();
            ctx.set_embed_viewports(false);
            ctx.set_fonts(stokesforge_render_egui::font_definitions());
            theme::install(&ctx);
            egui_extras::install_image_loaders(&ctx);
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            Self {
                session,
                host,
                ctx,
                frame: eframe::Frame::_new_kittest(),
                callbacks: HashMap::new(),
                time: 0.0,
                graph,
                tab,
                root_passes: 0,
            }
        }

        fn app(&self) -> std::cell::RefMut<'_, StokesForgeApp> {
            self.host.0.borrow_mut()
        }

        fn child_id(&self) -> egui::ViewportId {
            egui::ViewportId::from_hash_of(self.tab)
        }

        fn raw(&mut self, pass: Pass, id: egui::ViewportId) -> egui::RawInput {
            self.time += pass.dt;
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, pass.size)),
                time: Some(self.time),
                events: pass.events,
                focused: pass.focused,
                viewport_id: id,
                ..Default::default()
            };
            if id != egui::ViewportId::ROOT {
                raw.viewports.insert(
                    id,
                    egui::ViewportInfo {
                        parent: Some(egui::ViewportId::ROOT),
                        focused: Some(pass.focused),
                        ..Default::default()
                    },
                );
            }
            raw
        }

        /// One pass of the root, through the host.
        fn root(&mut self, pass: Pass) -> egui::FullOutput {
            let raw = self.raw(pass, egui::ViewportId::ROOT);
            let Self {
                host, frame, ctx, ..
            } = self;
            let mut output = ctx.run_ui(raw, |ui| host.ui(ui, frame));
            output.textures_delta.clear();
            for (id, viewport) in &output.viewport_output {
                if let Some(callback) = &viewport.viewport_ui_cb {
                    self.callbacks.insert(*id, callback.clone());
                }
            }
            self.root_passes += 1;
            output
        }

        /// One pass of the detached window, through the callback of the root.
        fn child(&mut self, pass: Pass) -> egui::FullOutput {
            let id = self.child_id();
            let raw = self.raw(pass, id);
            let callback = self
                .callbacks
                .get(&id)
                .expect("the root published it")
                .clone();
            let mut output = self.ctx.run_ui(raw, |ui| callback(ui));
            output.textures_delta.clear();
            output
        }

        /// The first passes: the root publishes the window, the window reports
        /// its focus and size.
        fn start(&mut self) {
            self.root(Pass::root());
            for _ in 0..3 {
                self.child(Pass::child());
            }
        }

        fn queue_in_window(&self, action: Action) {
            let app = self.app();
            let detached = &app.detached_tabs[&self.tab];
            detached.requests.lock().unwrap().actions.push(action);
        }

        fn open_plot_settings(&self) -> Action {
            Action::OpenPlotSettings {
                graph: self.graph,
                tab: PlotSettingsTab::Graph,
                axis: None,
            }
        }

        /// A click on the footer button `label` painted in `last`, with the
        /// passes of the window (or of the root), then a few more to settle.
        fn click(
            &mut self,
            in_root: bool,
            last: &egui::FullOutput,
            label: &str,
        ) -> egui::FullOutput {
            let pos = text_exact(&texts(&last.shapes), label).center;
            let mut output = None;
            for pressed in [None, Some(true), Some(false)] {
                let events = match pressed {
                    None => vec![egui::Event::PointerMoved(pos)],
                    Some(pressed) => vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                };
                output = Some(if in_root {
                    self.root(Pass::root().events(events))
                } else {
                    self.child(Pass::child().events(events))
                });
            }
            output.unwrap()
        }

        fn finish(self) {
            drop(self.host);
            self.session.shutdown().unwrap();
        }
    }

    fn ok() -> String {
        tr!("dialog-ok")
    }

    fn close() -> String {
        tr!("dialog-close")
    }

    fn about() -> String {
        tr!("about-title")
    }

    fn delay(output: &egui::FullOutput, id: egui::ViewportId) -> Duration {
        output.viewport_output[&id].repaint_delay
    }

    /// Without a live host the helper does nothing; with one it reaches the
    /// application, and a nested call (already borrowed) is a no-op, no panic.
    /// Witness: `borrow_mut` in `with_app`.
    #[test]
    fn the_helper_reaches_a_live_host_and_ignores_nesting() {
        assert!(with_app(|_| ()).is_none(), "no host yet");
        let session = Session::spawn(Project::new("Host").unwrap(), None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let host = AppHost::new(StokesForgeApp::new(state));
        assert_eq!(with_app(|_| 1), Some(1));
        let nested = with_app(|_| with_app(|_| 2));
        assert_eq!(nested, Some(None), "the nested call is skipped");
        drop(host);
        assert!(with_app(|_| ()).is_none(), "a dropped host is gone");
        session.shutdown().unwrap();
    }

    /// The bench of the issue: a focused detached graph opens Graph Properties;
    /// one root pass opens it, then the dialog is painted by the window, edited
    /// and closed by it, with no further root pass. Witnesses: removing the
    /// `host_pass` call of the window (no dialog); removing the republish after
    /// a stamp change (the window keeps the old snapshot).
    #[test]
    fn a_dialog_opened_from_a_detached_window_is_painted_answered_and_closed_in_it() {
        let mut bench = Bench::new();
        bench.start();
        bench.queue_in_window(bench.open_plot_settings());
        let root = bench.root(Pass::root());
        assert_eq!(bench.root_passes, 2, "the start pass and the opening pass");
        assert!(bench.app().state.plot_settings().is_some());
        assert_eq!(
            bench.app().dialog_hosting.host,
            Some(DialogHost::Tab(bench.tab))
        );
        assert!(!painted(&root, &ok()), "the root does not paint it");
        assert_eq!(
            delay(&root, bench.child_id()),
            Duration::ZERO,
            "the window is woken to paint it"
        );

        let mut child = bench.child(Pass::child());
        for _ in 0..4 {
            child = bench.child(Pass::child());
        }
        assert!(painted(&child, &ok()), "the window paints the dialog");
        assert!(bench.app().dialog_hosting.open);
        bench.app().state.plot_edit_mut().unwrap().spec.title = "Edited".into();

        let before = snapshot_stamp(bench.app().state.snapshot());
        let mut last = bench.click(false, &child, &ok());
        for _ in 0..3 {
            last = bench.child(Pass::child());
        }
        assert_eq!(bench.root_passes, 2, "no root pass after the opening one");
        let app = bench.app();
        assert!(app.state.plot_settings().is_none(), "OK closed the dialog");
        let after = snapshot_stamp(app.state.snapshot());
        assert_ne!(before, after, "the edit reached the model");
        let detached = &app.detached_tabs[&bench.tab];
        let published = detached
            .view_model
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .map(snapshot_stamp);
        assert_eq!(
            published,
            Some(after),
            "the window already has the new snapshot"
        );
        drop(app);
        assert!(!painted(&last, &ok()), "the dialog is gone");
        bench.finish();
    }

    /// A read-only dialog answered in the window is gone at its next pass, with
    /// no root pass in between: the window dispatches its own answer. Witness:
    /// queue the actions for the root, as a closure did before.
    #[test]
    fn an_answer_in_the_window_is_applied_without_the_root() {
        let mut bench = Bench::new();
        bench.start();
        bench.app().state.dispatch(Action::OpenAbout);
        bench.root(Pass::root());
        let roots = bench.root_passes;
        let mut child = bench.child(Pass::child());
        for _ in 0..3 {
            child = bench.child(Pass::child());
        }
        assert!(painted(&child, &about()), "painted in the window");
        let clicked = bench.click(false, &child, &close());
        assert!(
            !bench.app().state.about_open(),
            "applied by the window itself"
        );
        let next = bench.child(Pass::child());
        assert!(!painted(&clicked, &about()) || !painted(&next, &about()));
        assert!(!painted(&next, &about()), "absent from the next pass");
        assert_eq!(bench.root_passes, roots);
        bench.finish();
    }

    /// F3: an action the root dispatches that opens a dialog wakes the host
    /// although the snapshot stamp did not move; at rest nothing wakes it.
    /// Witness: remove `wake_dialog_host`.
    #[test]
    fn the_root_wakes_the_host_only_when_it_opened_something() {
        let mut bench = Bench::new();
        bench.start();
        let rest = bench.root(Pass::root());
        assert_eq!(
            delay(&rest, bench.child_id()),
            Duration::MAX,
            "asleep at rest"
        );
        bench.queue_in_window(Action::OpenAbout);
        let opened = bench.root(Pass::root());
        assert_eq!(delay(&opened, bench.child_id()), Duration::ZERO);
        bench.finish();
    }

    /// The host follows the focus while no dialog is shown: the root first,
    /// then the window; no focus at all leaves it; a dialog shown pins it.
    /// Witnesses: reading the window before the root; falling back to the root
    /// when no window is focused; always following the focus.
    #[test]
    fn the_host_follows_the_focus_until_a_dialog_is_shown() {
        let mut bench = Bench::new();
        bench.start();
        // The window reported its focus: it hosts.
        bench.root(Pass::root());
        assert_eq!(
            bench.app().dialog_hosting.host,
            Some(DialogHost::Tab(bench.tab))
        );
        // The root is focused (the window still says it is, as a stale report).
        bench.root(Pass::root().focused(true));
        assert_eq!(bench.app().dialog_hosting.host, None, "the root first");
        bench.app().state.dispatch(Action::OpenAbout);
        let mut in_root = bench.root(Pass::root().focused(true));
        for _ in 0..3 {
            in_root = bench.root(Pass::root().focused(true));
        }
        assert!(
            painted(&in_root, &about()),
            "opened from the root: painted there"
        );
        bench.app().state.dispatch(Action::CloseAbout);
        bench.root(Pass::root().focused(true));
        bench.root(Pass::root().focused(true));

        // The window gets the focus back, then nothing is focused.
        bench.child(Pass::child());
        bench.root(Pass::root());
        assert_eq!(
            bench.app().dialog_hosting.host,
            Some(DialogHost::Tab(bench.tab))
        );
        bench.child(Pass::child().focused(false));
        bench.root(Pass::root());
        assert_eq!(
            bench.app().dialog_hosting.host,
            Some(DialogHost::Tab(bench.tab)),
            "no window focused: unchanged, not the root"
        );

        // A dialog shown in the window pins it, whatever gets the focus.
        bench.child(Pass::child());
        bench.app().state.dispatch(Action::OpenAbout);
        bench.root(Pass::root());
        for _ in 0..3 {
            bench.child(Pass::child());
        }
        assert!(bench.app().dialog_hosting.open);
        bench.root(Pass::root().focused(true));
        assert_eq!(
            bench.app().dialog_hosting.host,
            Some(DialogHost::Tab(bench.tab)),
            "the root got the focus: the dialog stays where it is shown"
        );
        bench.finish();
    }

    /// A host that is gone falls back to the root: the next root pass paints
    /// the dialog, the veil is gone and its button answers. Witness: remove the
    /// existence check of `drop_unusable_host`.
    #[test]
    fn a_dialog_whose_host_closed_is_painted_by_the_root() {
        let mut bench = Bench::new();
        bench.start();
        bench.app().state.dispatch(Action::OpenAbout);
        bench.root(Pass::root());
        for _ in 0..3 {
            bench.child(Pass::child());
        }
        assert!(bench.app().dialog_hosting.open);
        let tab = bench.tab;
        bench.app().close_detached_tab(tab);
        let mut root = bench.root(Pass::root());
        for _ in 0..3 {
            root = bench.root(Pass::root());
        }
        assert_eq!(bench.app().dialog_hosting.host, None);
        assert!(!bench.app().dialog_hosting.open);
        assert!(painted(&root, &about()), "the root paints it");
        bench.click(true, &root, &close());
        assert!(!bench.app().state.about_open(), "its button lands");
        bench.finish();
    }

    /// While a dialog is hosted the root is veiled: the topmost layer at any
    /// point is the veil, the workspace shortcuts stand aside, and a press on
    /// the veil moves the dialog to the root, whose button then answers.
    /// Witnesses: drop the veil; drop the `open` test of `collect_shortcuts`;
    /// drop the pull of `hosted_dialog_veil`.
    #[test]
    fn a_hosted_dialog_veils_the_root_and_a_press_moves_it_there() {
        let mut bench = Bench::new();
        bench.start();
        let run_script = || {
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }]
        };
        let collected = |bench: &Bench| {
            let mut actions = Vec::new();
            let raw = egui::RawInput {
                events: run_script(),
                ..Default::default()
            };
            bench
                .ctx
                .run_ui(raw, |ui| {
                    bench.app().collect_shortcuts(ui.ctx(), &mut actions)
                })
                .textures_delta
                .clear();
            actions
        };
        assert!(
            collected(&bench).contains(&Action::RunScript),
            "without a hosted dialog the shortcut acts"
        );

        bench.app().state.dispatch(Action::OpenAbout);
        bench.root(Pass::root());
        for _ in 0..3 {
            bench.child(Pass::child());
        }
        assert!(bench.app().dialog_hosting.open);
        bench.root(Pass::root());
        bench.root(Pass::root());
        let veil = egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("stokesforge_hosted_dialog_veil"),
        );
        let point = egui::pos2(300.0, 200.0);
        bench.root(Pass::root().events(vec![egui::Event::PointerMoved(point)]));
        assert_eq!(
            bench.ctx.layer_id_at(point),
            Some(veil),
            "the veil takes the click"
        );
        assert!(
            !collected(&bench).contains(&Action::RunScript),
            "the shortcut stands aside"
        );

        // A press on the veil: the dialog is the root's from now on.
        bench.root(Pass::root().events(vec![egui::Event::PointerMoved(point)]));
        for pressed in [true, false] {
            bench.root(Pass::root().events(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]));
        }
        assert_eq!(bench.app().dialog_hosting.host, None);
        let mut root = bench.root(Pass::root());
        for _ in 0..3 {
            root = bench.root(Pass::root());
        }
        assert!(painted(&root, &about()), "now painted by the root");
        bench.click(true, &root, &close());
        assert!(
            !bench.app().state.about_open(),
            "its button lands after the move"
        );
        bench.finish();
    }

    /// A window below the floor does not host: the dialog is painted by the
    /// root. A window above it does, with its footer inside the window.
    /// Witness: remove the floor from `host_fits`.
    #[test]
    fn a_window_too_small_for_the_footer_does_not_host() {
        for (width, height) in [(420.0, 360.0), (560.0, 80.0)] {
            let mut bench = Bench::new();
            bench.root(Pass::root());
            for _ in 0..3 {
                bench.child(Pass::child().size(width, height));
            }
            bench.app().state.dispatch(Action::OpenAbout);
            let mut root = bench.root(Pass::root());
            for _ in 0..3 {
                root = bench.root(Pass::root());
            }
            assert_eq!(bench.app().dialog_hosting.host, None, "{width} x {height}");
            assert!(
                painted(&root, &about()),
                "the root paints it at {width} x {height}"
            );
            bench.finish();
        }
        let mut bench = Bench::new();
        bench.root(Pass::root());
        let size = (560.0, 460.0);
        for _ in 0..3 {
            bench.child(Pass::child().size(size.0, size.1));
        }
        bench.queue_in_window(bench.open_plot_settings());
        bench.root(Pass::root());
        let mut child = bench.child(Pass::child().size(size.0, size.1));
        for _ in 0..4 {
            child = bench.child(Pass::child().size(size.0, size.1));
        }
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0, size.1));
        for label in [ok(), tr!("dialog-cancel")] {
            let at = text_exact(&texts(&child.shapes), &label).center;
            assert!(
                window.contains(at),
                "{label} at {at:?} is inside {window:?}"
            );
        }
        bench.finish();
    }

    /// A dialog squeezed by a small window is not remembered at that size.
    /// Witness: remove the `clamped` test from `modal_frame`.
    #[test]
    fn a_dialog_closed_in_a_small_window_keeps_the_size_the_user_chose() {
        let plot = egui::Id::new("stokesforge_plot_settings");
        let mut bench = Bench::new();
        bench.start();
        let close_settings = |bench: &mut Bench, size: (f32, f32)| {
            bench.queue_in_window(bench.open_plot_settings());
            bench.root(Pass::root());
            let mut child = bench.child(Pass::child().size(size.0, size.1));
            for _ in 0..4 {
                child = bench.child(Pass::child().size(size.0, size.1));
            }
            let at = text_exact(&texts(&child.shapes), &tr!("dialog-cancel")).center;
            for pressed in [None, Some(true), Some(false)] {
                let events = match pressed {
                    None => vec![egui::Event::PointerMoved(at)],
                    Some(pressed) => vec![egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                };
                bench.child(Pass::child().size(size.0, size.1).events(events));
            }
            bench.child(Pass::child().size(size.0, size.1));
            assert!(bench.app().state.plot_settings().is_none(), "closed");
        };
        close_settings(&mut bench, (1400.0, 900.0));
        let remembered = dialogs::remembered_size(&bench.ctx, plot).expect("remembered");
        close_settings(&mut bench, (560.0, 300.0));
        assert_eq!(
            dialogs::remembered_size(&bench.ctx, plot),
            Some(remembered),
            "the squeezed size is not remembered"
        );
        bench.finish();
    }

    /// Ctrl+Z pressed in a text field of a hosted dialog undoes the text, not
    /// the model: the window sends no `Undo`. Witness: remove the gate from
    /// `collect_undo_redo_shortcuts`.
    #[test]
    fn ctrl_z_in_a_hosted_field_edits_the_text_and_sends_no_undo() {
        let mut bench = Bench::new();
        bench.start();
        bench.queue_in_window(bench.open_plot_settings());
        bench.root(Pass::root());
        let mut child = bench.child(Pass::child());
        for _ in 0..5 {
            child = bench.child(Pass::child());
        }
        // Focus the title field and type into it.
        let field = text_exact(&texts(&child.shapes), "Figure").center;
        for pressed in [None, Some(true), Some(false)] {
            let events = match pressed {
                None => vec![egui::Event::PointerMoved(field)],
                Some(pressed) => vec![egui::Event::PointerButton {
                    pos: field,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            };
            child = bench.child(Pass::child().events(events));
        }
        bench.child(
            Pass::child()
                .dt(2.0)
                .events(vec![egui::Event::Text("X".into())]),
        );
        for _ in 0..2 {
            bench.child(Pass::child().dt(2.0));
        }
        let typed = bench.app().state.plot_edit().unwrap().spec.title.clone();
        assert!(typed.contains('X'), "typed into the field: {typed:?}");
        let undo = egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        bench.child(Pass::child().dt(2.0).events(vec![undo]));
        let queued = {
            let app = bench.app();
            let requests = app.detached_tabs[&bench.tab].requests.lock().unwrap();
            requests.actions.clone()
        };
        assert!(
            !queued
                .iter()
                .any(|a| matches!(a, Action::Undo | Action::Redo)),
            "no model undo: {queued:?}"
        );
        let _ = child;
        let after = bench.app().state.plot_edit().unwrap().spec.title.clone();
        assert!(
            !after.contains('X'),
            "the field undid its own text: {after:?}"
        );
        bench.finish();
    }

    /// Esc with a dialog hosted in a graph in the Page Layout Mode closes the
    /// dialog and leaves the mode on. Witness: remove `modal_shown` from
    /// `page_layout::handle_keys`.
    #[test]
    fn escape_in_a_hosted_dialog_does_not_leave_the_page_layout_mode() {
        let mut bench = Bench::new();
        let graph = bench.graph;
        bench.app().toggle_layout_mode(graph, Some(true));
        bench.start();
        for _ in 0..3 {
            bench.child(Pass::child());
        }
        bench.app().state.dispatch(Action::OpenAbout);
        bench.root(Pass::root());
        for _ in 0..4 {
            bench.child(Pass::child());
        }
        assert!(bench.app().dialog_hosting.open);
        let escape = vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        bench.child(Pass::child().events(escape));
        bench.child(Pass::child());
        assert!(!bench.app().state.about_open(), "Esc closed the dialog");
        let mode = {
            let app = bench.app();
            let model = app.detached_tabs[&bench.tab].view_model.lock().unwrap();
            model
                .graph_view
                .as_ref()
                .is_some_and(|view| view.layout_mode())
        };
        assert!(mode, "the Page Layout Mode stays");
        bench.finish();
    }

    /// A blocking confirmation is painted by the host, and the root still
    /// empties the actions of the frame, the window's included. Witness: clear
    /// only when the root paints.
    #[test]
    fn a_blocking_confirmation_in_the_host_still_stops_the_other_actions() {
        let mut bench = Bench::new();
        bench.start();
        bench.root(Pass::root());
        bench.app().state.dispatch(Action::NewProject {
            pending_edits: true,
        });
        assert!(bench.app().state.blocking_dialog());
        let books = bench.app().state.books().len();
        bench.queue_in_window(Action::AddWorkbook);
        let root = bench.root(Pass::root());
        assert_eq!(
            bench.app().state.books().len(),
            books,
            "the action was dropped"
        );
        assert!(
            !painted(&root, &tr!("dialog-cancel")),
            "not painted by the root"
        );
        let mut child = bench.child(Pass::child());
        for _ in 0..3 {
            child = bench.child(Pass::child());
        }
        assert!(
            painted(&child, &tr!("dialog-cancel")),
            "painted by the window"
        );
        bench.finish();
    }

    /// Embedded viewports with a live host: no panic, and a dialog is painted
    /// once, not once by the root and once by the window. Witness: `borrow_mut`
    /// in `with_app` (see the first test for the nesting itself).
    #[test]
    fn embedded_viewports_with_a_live_host_paint_a_dialog_once() {
        let mut bench = Bench::new();
        bench.ctx.set_embed_viewports(true);
        bench.app().state.dispatch(Action::OpenAbout);
        let mut count = 0;
        for _ in 0..6 {
            let output = bench.root(Pass::root());
            count = output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(t) if t.galley.job.text == about())
                })
                .count();
        }
        assert!(count <= 1, "painted {count} times");
        assert_eq!(bench.app().dialog_hosting.host, None);
        bench.finish();
    }
}

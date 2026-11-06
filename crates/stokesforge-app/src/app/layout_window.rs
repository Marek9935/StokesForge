//! The window around the Page Layout Mode (#426): what the application does
//! when the mode is entered, left or interrupted.
//!
//! Entering the mode **maximises** the graph window; leaving it gives the
//! previous state back, so a window that was already maximised stays so.
//! Position and size of a floating window never move while it is maximised
//! (see `window_stack.rs`, "Maximizing a tab"): restoring is exactly "as
//! before". The mode is a view state: none of this touches the model nor the
//! undo history, except a nudge in progress, which a leaving commits.
//!
//! - **Floating graph.** [`StokesForgeApp::layout_entries`] holds, for each graph
//!   entered through [`StokesForgeApp::toggle_layout_mode`], the tab that was
//!   maximised before. Each frame [`StokesForgeApp::reconcile_layout_modes`]
//!   leaves the mode of a graph whose window is no longer the maximised one:
//!   the Restore button, the Window menu, a close, a delete and a project load
//!   all end the mode this way, with no maximise left behind.
//! - **Detached graph.** The OS window maximises and restores itself
//!   ([`follow_mode_maximize`]), unless it was maximised at entry; closing or
//!   reattaching leaves the mode, and so does the Restore button of its header
//!   (the same exit the floating window's Restore takes through
//!   [`StokesForgeApp::reconcile_layout_modes`]). A restore done from outside
//!   the application (an OS shortcut) cannot be told from a maximise the
//!   compositor has not applied yet, so it leaves the mode alone: the mode
//!   then works at the smaller size, and Esc still leaves it.
//!   WHY it is safe: a detached window maximised over the main window hides it,
//!   and every action of a detached window is drained in the ROOT frame. On
//!   native Wayland winit only emits `RedrawRequested` once the frame callback
//!   of the last present has come, and a fully covered surface never gets
//!   one, so the root frame, and with it the whole application, stopped. The
//!   vendored winit now reports the ROOT (the only window that opts in) as
//!   occluded after 100 ms without a callback while a redraw is wanted
//!   (`vendor/winit/STOKESFORGE.md`, "Frame starvation"); a detached window is
//!   never reported, as it would get no pass and lose its input, and a covered one
//!   repaints at its callback. eframe then keeps running the occluded root's
//!   `logic` and `ui` unpainted, about every 100 ms, as long as a viewport is
//!   visible. The detached window's request wakes the root, the root applies it
//!   and publishes the result to the viewport in the same pass
//!   ([`StokesForgeApp::republish_detached_tabs`]). Cost: about 100 ms between
//!   a command and its paint while the main window is covered.
//! - **Keys.** The root reads them at the start of the frame, before any
//!   window is painted ([`StokesForgeApp::begin_layout_frame`]); a detached
//!   viewport reads them at the top of its own frame. Esc therefore reaches the
//!   graph before a workbook painted behind it, and never leaves the mode under
//!   a modal window (`page_layout::handle_keys`). A dialog open in the main
//!   window stops the keys of every detached viewport before they get here
//!   ([`super::window_band::stop_keys`], #460).
//! - **Saved state.** While the mode forces the maximise, the window views
//!   written to the project report the state from before the mode
//!   ([`StokesForgeApp::reported_maximized_tab`]): entering the mode and saving
//!   neither marks the project modified nor writes `Maximized` into the file.

use eframe::egui;
use stokesforge_app_state::state::Action;
use stokesforge_core::NodeId;
use stokesforge_render_egui::GraphView;

use super::tab::Tab;
use super::StokesForgeApp;

impl StokesForgeApp {
    /// The start of a frame for the Page Layout Mode: publishes what the graph
    /// windows read, runs the toggles the menus asked for, reads the keys of the
    /// foreground floating graph and reconciles the modes with the windows.
    pub(super) fn begin_layout_frame(&mut self, ctx: &egui::Context, foreground: Option<NodeId>) {
        self.run_layout_mode_requests(ctx, foreground);
        let floating = foreground.filter(|graph| {
            self.is_floating_graph(*graph) && self.read_graph_view(*graph, |_| ()).is_some()
        });
        if let Some(graph) = floating {
            let mut keys = Vec::new();
            self.with_graph_view(graph, |view| {
                super::page_layout::handle_keys(ctx, view, graph, &mut keys)
            });
            self.layout_actions.append(&mut keys);
        }
        self.reconcile_layout_modes();
    }

    /// Moves the actions a leaving committed into the frame's actions.
    pub(super) fn drain_layout_actions(&mut self, actions: &mut Vec<Action>) {
        actions.append(&mut self.layout_actions);
    }

    fn is_floating_graph(&self, graph: NodeId) -> bool {
        !self.detached_tabs.contains_key(&Tab::Graph(graph))
    }

    /// Turns the Page Layout Mode of `graph` on or off (`None`: toggles). A
    /// view state: nothing is sent to the model, whatever the placement. Entering
    /// maximises the window (a detached one maximises its OS window itself).
    pub(super) fn toggle_layout_mode(&mut self, graph: NodeId, on: Option<bool>) {
        let now = self
            .read_graph_view(graph, GraphView::layout_mode)
            .unwrap_or(false);
        let want = on.unwrap_or(!now);
        if want == now {
            return;
        }
        if !want {
            self.leave_layout_mode(graph);
            return;
        }
        let tab = Tab::Graph(graph);
        if self.is_floating_graph(graph) && self.floating_tabs.contains(&tab) {
            self.layout_entries.insert(graph, self.maximized_tab);
            self.maximized_tab = Some(tab);
        }
        self.with_graph_view(graph, |view| view.set_layout_mode(true));
    }

    /// Leaves the mode of `graph` and gives its window back: a nudge in progress
    /// is committed (its action goes out with the frame), and the maximised tab
    /// returns to what it was before the mode, unless the user already changed
    /// it.
    pub(super) fn leave_layout_mode(&mut self, graph: NodeId) {
        // A deleted graph has no view left (and none is made for it).
        if self.read_graph_view(graph, |_| ()).is_some() {
            let mut committed = Vec::new();
            self.with_graph_view(graph, |view| {
                super::page_layout::leave_mode(view, graph, &mut committed)
            });
            if self.state.tree().node(graph).is_some() {
                self.layout_actions.append(&mut committed);
            }
        }
        if let Some(before) = self.layout_entries.remove(&graph) {
            if self.maximized_tab == Some(Tab::Graph(graph)) {
                self.maximized_tab = before.filter(|tab| self.floating_tabs.contains(tab));
            }
        }
    }

    /// Ends the mode of every floating graph whose window is no longer the
    /// maximised one, and gives back the window of a mode that ended by itself.
    pub(super) fn reconcile_layout_modes(&mut self) {
        let entered: Vec<NodeId> = self.layout_entries.keys().copied().collect();
        for graph in entered {
            let in_mode = self
                .read_graph_view(graph, GraphView::layout_mode)
                .unwrap_or(false);
            let maximised = self.maximized_tab == Some(Tab::Graph(graph));
            if !in_mode || !maximised || !self.is_floating_graph(graph) {
                self.leave_layout_mode(graph);
            }
        }
    }

    /// The maximised tab as the project should record it: while the mode forces
    /// the maximise of a window, the one that was maximised before it.
    pub(super) fn reported_maximized_tab(&self) -> Option<Tab> {
        match self.maximized_tab {
            Some(Tab::Graph(graph)) => match self.layout_entries.get(&graph) {
                Some(before) => *before,
                None => self.maximized_tab,
            },
            other => other,
        }
    }
}

/// What a detached graph window remembers of the maximise the mode forced:
/// `None` outside the mode, else whether the OS window was maximised before.
pub(super) type ModeMaximize = Option<bool>;

/// Maximises the OS window of a detached graph when its mode is on and restores
/// it when the mode ends, unless it was maximised at entry. A compositor that
/// refuses the maximise leaves the mode working at the current size.
pub(super) fn follow_mode_maximize(ctx: &egui::Context, state: &mut ModeMaximize, in_mode: bool) {
    match (in_mode, *state) {
        (true, None) => {
            let was = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            *state = Some(was);
            if !was {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            }
        }
        (false, Some(was)) => {
            *state = None;
            if !was {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use stokesforge_app_state::state::{Action, AppState, PlotSettingsTab};
    use stokesforge_app_state::tr;
    use stokesforge_core::NodeId;
    use stokesforge_render_egui::GraphView;
    use stokesforge_session::Session;

    use super::super::tab::Tab;
    use super::super::test_support::key;
    use super::super::StokesForgeApp;

    /// The real application with a workbook painted behind the graph window,
    /// both floating, nothing maximised.
    struct Root {
        session: Session,
        app: StokesForgeApp,
        ctx: egui::Context,
        graph: NodeId,
        sheet: NodeId,
        time: f64,
    }

    impl Root {
        fn new() -> Self {
            let (session, graph) = super::super::graph_window::tests::plotted_graph();
            let mut state = AppState::new(session.handle(), "t.stf".into()).unwrap();
            let book = state
                .books()
                .into_iter()
                .next()
                .map(|(book, _)| book)
                .unwrap();
            let sheet = state.tree().node(book).unwrap().children()[0];
            state.dispatch(Action::SelectAll { sheet });
            let mut app = StokesForgeApp::new(state);
            app.floating_tabs = vec![Tab::Book(book), Tab::Graph(graph)];
            app.window_positions
                .insert(Tab::Book(book), egui::pos2(20.0, 20.0));
            app.window_positions
                .insert(Tab::Graph(graph), egui::pos2(60.0, 60.0));
            let ctx = super::super::test_support::context();
            egui_extras::install_image_loaders(&ctx);
            let mut root = Self {
                session,
                app,
                ctx,
                graph,
                sheet,
                time: 0.0,
            };
            for _ in 0..3 {
                root.frame(Vec::new());
            }
            root
        }

        fn mode(&self) -> bool {
            self.app
                .read_graph_view(self.graph, GraphView::layout_mode)
                .unwrap_or(false)
        }

        /// One frame of the application: what `ui` does around the windows.
        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.1;
            let input = egui::RawInput {
                time: Some(self.time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let mut actions = Vec::new();
            let (app, ctx) = (&mut self.app, self.ctx.clone());
            let active = app
                .maximized_tab
                .or_else(|| app.floating_tabs.last().copied());
            let mut output = ctx.run_ui(input, |ui| {
                app.begin_layout_frame(ui.ctx(), Some(self.graph));
                app.show_workspace(
                    ui,
                    active,
                    &mut actions,
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut Vec::new(),
                );
                app.show_edit_dialogs(ui.ctx(), &mut actions);
                app.drain_layout_actions(&mut actions);
                super::super::page_layout::note_field_focus(ui.ctx());
            });
            output.textures_delta.clear();
            for action in actions {
                self.app.state.dispatch(action);
            }
        }

        fn enter_mode_unmaximised(&mut self) {
            let graph = self.graph;
            self.app
                .with_graph_view(graph, |view| view.set_layout_mode(true));
            for _ in 0..3 {
                self.frame(Vec::new());
            }
            assert!(self.mode());
            assert_eq!(self.app.maximized_tab, None);
        }
    }

    /// Esc from the graph window with a workbook painted behind it leaves the
    /// mode and keeps the sheet selection. The workbook, painted first, used to
    /// take the Esc. Witness: put Esc back in the contents.
    #[test]
    fn escape_from_the_graph_window_leaves_the_mode_over_a_workbook() {
        let mut root = Root::new();
        root.enter_mode_unmaximised();
        root.frame(vec![key(egui::Key::Escape)]);
        root.frame(Vec::new());
        assert!(!root.mode(), "Esc leaves the mode");
        assert!(
            root.app.state.selection(root.sheet).is_some(),
            "the workbook behind keeps its selection"
        );
        root.session.shutdown().unwrap();
    }

    /// With Graph Properties open, Esc closes that window and the mode stays.
    /// Witness: remove `modal_shown` from the key guard.
    #[test]
    fn escape_closes_graph_properties_and_the_mode_stays() {
        let mut root = Root::new();
        root.enter_mode_unmaximised();
        root.app.state.dispatch(Action::OpenPlotSettings {
            graph: root.graph,
            tab: PlotSettingsTab::Graph,
            axis: None,
        });
        for _ in 0..4 {
            root.frame(Vec::new());
        }
        assert!(root.app.state.plot_settings().is_some());
        for _ in 0..2 {
            if root.app.state.plot_settings().is_none() {
                break;
            }
            root.frame(vec![key(egui::Key::Escape)]);
            root.frame(Vec::new());
            assert!(root.mode(), "the mode stays while Properties is closing");
        }
        assert!(root.app.state.plot_settings().is_none(), "Esc closes it");
        assert!(root.mode());
        root.session.shutdown().unwrap();
    }

    impl Root {
        fn tab(&self) -> Tab {
            Tab::Graph(self.graph)
        }

        fn toggle(&mut self, on: bool) {
            let graph = self.graph;
            self.app.toggle_layout_mode(graph, Some(on));
            for _ in 0..2 {
                self.frame(Vec::new());
            }
        }

        fn click_text(&mut self, needle: &str) {
            self.frame(Vec::new());
            let mut output = None;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            };
            let (app, ctx, graph) = (&mut self.app, self.ctx.clone(), self.graph);
            output.replace(ctx.run_ui(input, |ui| {
                app.begin_layout_frame(ui.ctx(), Some(graph));
                app.show_workspace(
                    ui,
                    Some(Tab::Graph(graph)),
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut Vec::new(),
                );
            }));
            let pos = super::super::test_support::texts(&output.unwrap().shapes)
                .into_iter()
                .find(|text| text.text.contains(needle))
                .map(|text| text.center)
                .unwrap_or_else(|| panic!("{needle:?} is not painted"));
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.frame(Vec::new());
        }
    }

    /// Entering maximises the window, Esc restores it: position, size and
    /// stacking come back untouched, and neither way adds an undo step.
    /// Witness: restore nothing on leaving.
    #[test]
    fn entering_maximises_and_leaving_restores_the_window() {
        let mut root = Root::new();
        let (positions, sizes) = (
            root.app.window_positions.clone(),
            root.app.window_sizes.clone(),
        );
        let (revision, undo) = (root.app.state.revision(), root.app.state.undo_label());
        root.toggle(true);
        assert!(root.mode());
        assert_eq!(root.app.maximized_tab, Some(root.tab()), "maximised");
        assert_eq!(root.app.state.undo_label(), undo, "entering: no undo step");
        root.frame(vec![key(egui::Key::Escape)]);
        root.frame(Vec::new());
        assert!(!root.mode());
        assert_eq!(root.app.maximized_tab, None, "restored");
        assert_eq!(
            (&root.app.window_positions, &root.app.window_sizes),
            (&positions, &sizes)
        );
        assert_eq!(root.app.state.undo_label(), undo, "leaving: no undo step");
        assert_eq!(root.app.state.revision(), revision);
        assert!(root.app.layout_entries.is_empty());
        root.session.shutdown().unwrap();
    }

    /// A window that was already maximised stays so on leaving. Witness: always
    /// restore.
    #[test]
    fn a_window_maximised_before_the_mode_stays_maximised_after_it() {
        let mut root = Root::new();
        root.app.maximized_tab = Some(root.tab());
        root.toggle(true);
        assert!(root.mode());
        root.frame(vec![key(egui::Key::Escape)]);
        root.frame(Vec::new());
        assert!(!root.mode());
        assert_eq!(root.app.maximized_tab, Some(root.tab()));
        root.session.shutdown().unwrap();
    }

    /// The Restore button, a close and a project load each end the mode and
    /// leave no maximise behind; a window reopened is not in the mode. Witness:
    /// remove `reconcile_layout_modes`.
    #[test]
    fn restore_close_and_project_load_end_the_mode() {
        // The Restore button of the maximised window.
        let mut root = Root::new();
        root.toggle(true);
        let name = super::super::tab::tab_title(&root.app.state, root.tab());
        let title = tr!("action-restore-window", title = name);
        root.click_text(&title);
        assert_eq!(root.app.maximized_tab, None, "restored by the button");
        assert!(!root.mode(), "the button leaves the mode");
        assert!(root.app.layout_entries.is_empty());
        // Close.
        root.toggle(true);
        let tab = root.tab();
        root.app.close_floating_tabs(vec![tab]);
        root.frame(Vec::new());
        assert!(!root.mode(), "a closed window is out of the mode");
        assert_eq!(root.app.maximized_tab, None);
        root.app.push_floating_tab(tab);
        root.frame(Vec::new());
        assert!(!root.mode(), "reopened: not in the mode");
        assert_eq!(root.app.maximized_tab, None, "no maximise left");
        // Project load.
        root.toggle(true);
        let ctx = root.ctx.clone();
        root.app.forget_project_views(&ctx);
        assert!(root.app.layout_entries.is_empty());
        assert_eq!(root.app.maximized_tab, None);
        assert!(!root.mode());
        root.session.shutdown().unwrap();
    }

    /// Deleting the graph during the mode drops its record and makes no view
    /// for it.
    #[test]
    fn a_graph_deleted_during_the_mode_leaves_nothing_behind() {
        let mut root = Root::new();
        root.toggle(true);
        let graph = root.graph;
        root.app.state.dispatch(Action::DeleteObjects(vec![graph]));
        let tab = root.tab();
        root.app.graph_views.remove(&graph);
        root.app.close_floating_tabs(vec![tab]);
        root.frame(Vec::new());
        assert!(root.app.layout_entries.is_empty());
        assert!(!root.app.graph_views.contains_key(&graph));
        assert_eq!(root.app.maximized_tab, None);
        root.session.shutdown().unwrap();
    }

    /// Entering the mode and saving: the file records the window as it was
    /// before the mode, and the project is not marked modified. Witness:
    /// `sync_window_views` reading `maximized_tab` directly.
    #[test]
    fn saving_in_the_mode_records_the_state_from_before_it() {
        use stokesforge_session::WindowMode;
        let mut root = Root::new();
        let dir = tempfile::tempdir().unwrap();
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 60.0), egui::vec2(500.0, 300.0));
        let tab = root.tab();
        root.app.remember_window_geometry(tab, rect);
        root.app.sync_window_views();
        let first = dir.path().join("before.stf");
        root.app.state.dispatch(Action::SaveAs {
            path: first.to_string_lossy().into_owned(),
        });
        let title = root.app.state.window_title();
        assert!(!title.contains('\u{2022}'), "saved: {title}");
        root.toggle(true);
        root.app.sync_window_views();
        assert_eq!(root.app.maximized_tab, Some(tab), "the window is maximised");
        assert_eq!(
            root.app.state.window_title(),
            title,
            "entering the mode does not mark the project modified"
        );
        let second = dir.path().join("in-mode.stf");
        root.app.state.dispatch(Action::SaveAs {
            path: second.to_string_lossy().into_owned(),
        });
        // Read the file back through a session of its own.
        let reread =
            Session::spawn(stokesforge_core::Project::new("Reread").unwrap(), None).unwrap();
        reread.handle().load(&second).unwrap();
        let geometries = reread
            .handle()
            .snapshot()
            .unwrap()
            .window_geometries
            .clone();
        let record = geometries.get(&root.graph).expect("the graph's window");
        assert_eq!(record.view.unwrap().mode, WindowMode::Open);
        assert_eq!((record.x, record.width), (40.0, 500.0));
        reread.shutdown().unwrap();
        root.session.shutdown().unwrap();
    }

    /// Detaching leaves the mode before the view moves; the maximise is gone;
    /// closing or reattaching a detached window leaves the mode too.
    #[test]
    fn detaching_closing_and_reattaching_leave_the_mode() {
        let mut root = Root::new();
        let tab = root.tab();
        root.toggle(true);
        root.app.detach_tab(tab);
        assert_eq!(root.app.maximized_tab, None, "no maximise left");
        assert!(!root.mode(), "the view moved out of the mode");
        // A mode started on the detached view ends when it comes back.
        root.toggle(true);
        assert!(root.mode());
        assert!(
            root.app.layout_entries.is_empty(),
            "no root maximise for it"
        );
        root.app.reattach_tab(tab);
        assert!(!root.mode(), "reattaching leaves the mode");
        root.app.detach_tab(tab);
        root.toggle(true);
        root.app.close_detached_tab(tab);
        assert!(!root.mode(), "closing leaves the mode");
        root.session.shutdown().unwrap();
    }

    /// The OS window of a detached graph maximises on entering and is restored
    /// on leaving, unless it was already maximised.
    #[test]
    fn a_detached_window_maximises_with_the_mode_and_restores_unless_it_was() {
        use super::{follow_mode_maximize, ModeMaximize};
        let commands =
            |ctx: &egui::Context, maximized: bool, in_mode: bool, state: &mut ModeMaximize| {
                let mut input = egui::RawInput::default();
                input
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .maximized = Some(maximized);
                let mut output =
                    ctx.run_ui(input, |ui| follow_mode_maximize(ui.ctx(), state, in_mode));
                output.textures_delta.clear();
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .filter_map(|command| match command {
                        egui::ViewportCommand::Maximized(on) => Some(*on),
                        _ => None,
                    })
                    .collect::<Vec<bool>>()
            };
        let ctx = egui::Context::default();
        let mut state: ModeMaximize = None;
        assert_eq!(commands(&ctx, false, true, &mut state), [true], "enter");
        assert_eq!(commands(&ctx, true, true, &mut state), Vec::<bool>::new());
        assert_eq!(commands(&ctx, true, false, &mut state), [false], "leave");
        // Already maximised at entry: nothing sent either way.
        let mut state: ModeMaximize = None;
        assert!(commands(&ctx, true, true, &mut state).is_empty());
        assert!(commands(&ctx, true, false, &mut state).is_empty());
        assert_eq!(state, None);
    }

    /// A nudge in progress is committed by a leaving: the `SetFreeLayout`
    /// reaches the frame's actions. Witness: leave without `take_events`.
    #[test]
    fn leaving_commits_a_nudge_in_progress() {
        let mut root = Root::new();
        let graph = root.graph;
        root.app.state.dispatch(Action::AddCell {
            graph,
            panel: 0,
            side: stokesforge_core::compose::CellSide::Right,
            columns: Vec::new(),
        });
        root.app
            .state
            .dispatch(Action::SetLayerPlacement { graph, free: true });
        root.toggle(true);
        let spec = root.app.state.plot_spec(graph).unwrap();
        let now = root.time;
        root.app.with_graph_view(graph, |view| {
            view.set_active_panel(0, &spec);
        });
        root.frame(Vec::new());
        root.app
            .with_graph_view(graph, |view| view.nudge((1, 0), now));
        root.app.toggle_layout_mode(graph, Some(false));
        assert!(
            root.app
                .layout_actions
                .iter()
                .any(|action| matches!(action, Action::SetFreeLayout { .. })),
            "{:?}",
            root.app.layout_actions
        );
        root.session.shutdown().unwrap();
    }
}

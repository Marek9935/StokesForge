//! Test helpers shared by the surfaces of `src/app/` (#128).
//!
//! Each surface used to write its own copies of these helpers (up to four
//! for `painted`, three for `context`); a fix had to be redone everywhere.
//! They live here once, and the `tests` modules import them by name
//! (`use super::super::test_support::{…}`, never `*`: see
//! `no_global_import_outside_tests`, which tolerates the tests but not the
//! vagueness).
//!
//! What stays local to a module, on purpose: the benches (`bench`), which each
//! mount a different state; `grid::tests::Bench::image` and
//! `workbook::tests::click`, which drive a bench or a paint closure rather
//! than a bare context; and `menus::tests::context`, which adds the image
//! loaders and AccessKit to the base from here.

use eframe::egui;
use stokesforge_core::{Column, NodeId, Project};

/// A context with the application's fonts and theme: the width of tabs,
/// labels and menus depends on them.
pub(super) fn context() -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(stokesforge_render_egui::font_definitions());
    crate::theme::install(&context);
    context
}

/// The raw input of a frame of `size` with `events`.
fn raw_input(size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        ..Default::default()
    }
}

/// A frame of `size` with `events`, where `content` draws; returns the full
/// output, textures dropped. Time advances only 1/60 s per frame: see
/// [`image`] for the surfaces whose animations must run to the end.
pub(super) fn frame_of(
    context: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    content: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = context.run_ui(raw_input(size, events), content);
    output.textures_delta.clear();
    output
}

/// [`frame_of`] as seen by the viewport `viewport` (a detached window).
pub(super) fn frame_in(
    context: &egui::Context,
    viewport: egui::ViewportId,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    content: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut raw = raw_input(size, events);
    raw.viewport_id = viewport;
    raw.viewports
        .insert(viewport, egui::ViewportInfo::default());
    let mut output = context.run_ui(raw, content);
    output.textures_delta.clear();
    output
}

/// A 1400 × 900 frame with `events`, 0.1 s after the previous one; returns
/// what was painted. Time advances so that animated scrolling runs to the end
/// (the tab strip shows the tab chosen by `scroll_to_rect`, spread over 0.3 s
/// at most).
pub(super) fn image(
    context: &egui::Context,
    events: Vec<egui::Event>,
    content: impl FnMut(&mut egui::Ui),
) -> Vec<egui::epaint::ClippedShape> {
    let mut raw = raw_input(egui::vec2(1400.0, 900.0), events);
    raw.time = Some(context.input(|input| input.time) + 0.1);
    let mut output = context.run_ui(raw, content);
    output.textures_delta.clear();
    output.shapes
}

/// A key pressed, without modifier.
pub(super) fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// A click of `button` at `pos`, spread over three frames (hover, press,
/// release) as egui's click detection requires.
pub(super) fn click(
    context: &egui::Context,
    pos: egui::Pos2,
    button: egui::PointerButton,
    mut content: impl FnMut(&mut egui::Ui),
) {
    image(context, vec![egui::Event::PointerMoved(pos)], &mut content);
    for pressed in [true, false] {
        image(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::default(),
            }],
            &mut content,
        );
    }
}

/// Was a text containing `needle` painted?
pub(super) fn painted(output: &egui::FullOutput, needle: &str) -> bool {
    output.shapes.iter().any(|painted| {
        matches!(&painted.shape,
            egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
    })
}

/// A painted text: its content, its effective color and its center.
#[derive(Debug)]
pub(super) struct Text {
    pub(super) text: String,
    pub(super) color: egui::Color32,
    pub(super) center: egui::Pos2,
    /// Written in bold (`theme::bold`): the weight is a font variation, not a
    /// color.
    pub(super) bold: bool,
}

/// All the texts painted in `shapes`, with their effective color.
pub(super) fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<Text> {
    shapes
        .iter()
        .filter_map(|painted| match &painted.shape {
            egui::Shape::Text(shape) => {
                let section = shape
                    .galley
                    .job
                    .sections
                    .first()
                    .map_or(egui::Color32::PLACEHOLDER, |s| s.format.color);
                let color =
                    shape
                        .override_text_color
                        .unwrap_or(if section == egui::Color32::PLACEHOLDER {
                            shape.fallback_color
                        } else {
                            section
                        });
                Some(Text {
                    text: shape.galley.job.text.clone(),
                    color,
                    center: shape.pos + shape.galley.rect.center().to_vec2(),
                    bold: shape
                        .galley
                        .job
                        .sections
                        .iter()
                        .any(|s| s.format.coords == crate::theme::bold_variations()),
                })
            }
            _ => None,
        })
        .collect()
}

/// The painted text exactly equal to `wanted`, if there is exactly one.
pub(super) fn text_exact<'a>(texts: &'a [Text], wanted: &str) -> &'a Text {
    let found: Vec<&Text> = texts.iter().filter(|t| t.text == wanted).collect();
    assert_eq!(
        found.len(),
        1,
        "{wanted:?} must be painted once: {:?}",
        texts.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    found[0]
}

/// The labels of a list rendered by `label`, all distinct and non-empty: two
/// choices of one drop-down list that display the same can no longer be told
/// apart.
pub(super) fn distinct<T: std::fmt::Debug>(choices: &[T], label: impl Fn(&T) -> String) {
    let labels: Vec<String> = choices.iter().map(&label).collect();
    for (i, a) in labels.iter().enumerate() {
        assert!(!a.trim().is_empty(), "{:?} has no label", choices[i]);
        for (j, b) in labels.iter().enumerate().skip(i + 1) {
            assert_ne!(
                a, b,
                "{:?} and {:?} display the same",
                choices[i], choices[j]
            );
        }
    }
}

/// A workbook with one sheet of two columns `[0, 1]` under the root: returns
/// the nodes of these columns (x, y), enough to plot a curve.
pub(super) fn xy_columns(project: &mut Project) -> (NodeId, NodeId) {
    let root = project.root();
    let book = project.add_empty_workbook(root, None).unwrap();
    let sheet = project.add_worksheet(book, None).unwrap();
    let x = project
        .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
        .unwrap();
    let y = project
        .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
        .unwrap();
    (x, y)
}

/// A root and one detached graph window driven the way eframe drives them:
/// root passes through [`eframe::App::ui`], window passes through the callback
/// the root published. A pass is fed its own focus, size and events, which is
/// what the rules about dialogs, bands and keys depend on (#472).
pub(super) mod bench {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use eframe::egui;
    use eframe::App as _;
    use stokesforge_app_state::state::{Action, AppState, PlotSettingsTab};
    use stokesforge_core::NodeId;
    use stokesforge_session::Session;

    use super::super::graph_window::tests::plotted_graph;
    use super::super::server::AppSlot;
    use super::super::tab::Tab;
    use super::super::StokesForgeApp;

    type Callback = Arc<dyn Fn(&mut egui::Ui) + Sync + Send>;

    /// What one pass is fed with.
    #[derive(Clone)]
    pub(in crate::app) struct Pass {
        pub(in crate::app) events: Vec<egui::Event>,
        pub(in crate::app) focused: bool,
        pub(in crate::app) dt: f64,
        pub(in crate::app) size: egui::Vec2,
        /// What the OS says about the window (rectangles, monitor, maximised);
        /// the parent and the focus are the bench's.
        pub(in crate::app) info: egui::ViewportInfo,
        /// Files dropped on the window during this pass.
        pub(in crate::app) dropped: Vec<Arc<dyn egui::DroppedFile + Send + Sync>>,
    }

    impl Pass {
        /// A root pass: not focused (a detached window is), 50 ms later.
        pub(in crate::app) fn root() -> Self {
            Self {
                events: Vec::new(),
                focused: false,
                dt: 0.05,
                size: egui::vec2(1400.0, 900.0),
                info: egui::ViewportInfo::default(),
                dropped: Vec::new(),
            }
        }

        /// A window pass: focused.
        pub(in crate::app) fn child() -> Self {
            Self {
                focused: true,
                ..Self::root()
            }
        }

        pub(in crate::app) fn events(mut self, events: Vec<egui::Event>) -> Self {
            self.events = events;
            self
        }

        pub(in crate::app) fn focused(mut self, focused: bool) -> Self {
            self.focused = focused;
            self
        }

        pub(in crate::app) fn size(mut self, width: f32, height: f32) -> Self {
            self.size = egui::vec2(width, height);
            self
        }

        /// Files dropped on the window during this pass.
        pub(in crate::app) fn dropped(
            mut self,
            files: Vec<Arc<dyn egui::DroppedFile + Send + Sync>>,
        ) -> Self {
            self.dropped = files;
            self
        }

        /// The window's OS-reported state for this pass.
        pub(in crate::app) fn info(mut self, info: egui::ViewportInfo) -> Self {
            self.info = info;
            self
        }

        pub(in crate::app) fn dt(mut self, dt: f64) -> Self {
            self.dt = dt;
            self
        }
    }

    pub(in crate::app) struct Bench {
        pub(in crate::app) session: Session,
        /// The application behind its lock, as the binary runs it: the passes of
        /// the windows lock it themselves (#475).
        pub(in crate::app) app: AppSlot,
        pub(in crate::app) ctx: egui::Context,
        frame: eframe::Frame,
        callbacks: HashMap<egui::ViewportId, Callback>,
        time: f64,
        /// The window that has the keyboard, as the root sees it.
        focused_child: Option<egui::ViewportId>,
        /// Ids of removed windows that the root's input still lists, as an
        /// empty entry (the late echo measured by the probe of #475).
        pub(in crate::app) stale: Vec<egui::ViewportId>,
        pub(in crate::app) graph: NodeId,
        pub(in crate::app) tab: Tab,
        /// The folder of the project file of [`Bench::with_every_window`].
        pub(in crate::app) dir: Option<tempfile::TempDir>,
    }

    /// The windows a [`Bench::with_every_window`] detaches (#459).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::app) enum Window {
        Graph,
        Book,
        ProjectManager,
        ObjectManager,
        Console,
    }

    impl Window {
        pub(in crate::app) const ALL: [Window; 5] = [
            Window::Graph,
            Window::Book,
            Window::ProjectManager,
            Window::ObjectManager,
            Window::Console,
        ];
    }

    impl Bench {
        /// A project with one graph, detached and not yet painted.
        pub(in crate::app) fn new() -> Self {
            Self::with_graph(true)
        }

        /// The same project with the graph still a floating window of the root,
        /// to be detached by the test.
        pub(in crate::app) fn floating_graph() -> Self {
            Self::with_graph(false)
        }

        fn with_graph(detached: bool) -> Self {
            let (session, graph) = plotted_graph();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let mut app = StokesForgeApp::new(state);
            let tab = Tab::Graph(graph);
            if detached {
                app.detach_tab(tab);
            }
            Self::from_app(session, app, graph)
        }

        /// A bench on `app`, whose "main" graph is `graph` (the one
        /// [`Self::child`] and [`Self::tab`] stand for). Nothing is detached or
        /// painted yet.
        pub(in crate::app) fn from_app(
            session: Session,
            app: StokesForgeApp,
            graph: NodeId,
        ) -> Self {
            let tab = Tab::Graph(graph);
            let app = AppSlot::new(app);
            let ctx = egui::Context::default();
            ctx.set_embed_viewports(false);
            ctx.set_fonts(stokesforge_render_egui::font_definitions());
            crate::theme::install(&ctx);
            egui_extras::install_image_loaders(&ctx);
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            Self {
                session,
                app,
                ctx,
                frame: eframe::Frame::_new_kittest(),
                callbacks: HashMap::new(),
                time: 0.0,
                focused_child: None,
                stale: Vec::new(),
                graph,
                tab,
                dir: None,
            }
        }

        /// The same project, saved in a file of its own, with the workbook, the
        /// three panels and the graph all detached (#459).
        pub(in crate::app) fn with_every_window() -> Self {
            let mut bench = Self::new();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("keys.stf").to_string_lossy().into_owned();
            let state = AppState::new(bench.session.handle(), path).unwrap();
            let graph = bench.graph;
            let mut app = StokesForgeApp::new(state);
            let book = app.state.books()[0].0;
            for tab in [Tab::Graph(graph), Tab::Book(book)] {
                if !app.floating_tabs.contains(&tab) {
                    app.floating_tabs.push(tab);
                }
                app.detach_tab(tab);
            }
            app.detach_project_manager();
            app.state.set_object_manager_open_for_test(true);
            app.om_detached = true;
            app.detach_script_console();
            bench.app = AppSlot::new(app);
            bench.dir = Some(dir);
            bench
        }

        /// The same saved project as [`Self::with_every_window`], but the
        /// workbook stays docked in the root (a floating window of the main
        /// window) and nothing is detached (#518): the twin to play the same
        /// keys docked and detached.
        pub(in crate::app) fn with_docked_workbook() -> Self {
            let mut bench = Self::new();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("keys.stf").to_string_lossy().into_owned();
            let state = AppState::new(bench.session.handle(), path).unwrap();
            let mut app = StokesForgeApp::new(state);
            let book = app.state.books()[0].0;
            app.open_tab(Tab::Book(book));
            bench.app = AppSlot::new(app);
            bench.dir = Some(dir);
            bench
        }

        /// A few root passes: the workbook is painted docked.
        pub(in crate::app) fn start_docked(&mut self) {
            for _ in 0..4 {
                self.root(Pass::root().focused(true));
            }
        }

        pub(in crate::app) fn path(&self) -> std::path::PathBuf {
            self.dir.as_ref().unwrap().path().join("keys.stf")
        }

        pub(in crate::app) fn book(&self) -> NodeId {
            self.app.state.books()[0].0
        }

        pub(in crate::app) fn id_of(&self, window: Window) -> egui::ViewportId {
            match window {
                Window::Graph => egui::ViewportId::from_hash_of(self.tab),
                Window::Book => egui::ViewportId::from_hash_of(Tab::Book(self.book())),
                Window::ProjectManager => {
                    egui::ViewportId::from_hash_of("stokesforge_project_manager_window")
                }
                Window::ObjectManager => {
                    egui::ViewportId::from_hash_of("stokesforge_object_manager_window")
                }
                Window::Console => egui::ViewportId::from_hash_of("stokesforge_script_window"),
            }
        }

        pub(in crate::app) fn child_id(&self) -> egui::ViewportId {
            egui::ViewportId::from_hash_of(self.tab)
        }

        fn raw(&mut self, pass: Pass, id: egui::ViewportId) -> egui::RawInput {
            self.time += pass.dt;
            let info = pass.info.clone();
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, pass.size)),
                time: Some(self.time),
                events: pass.events,
                dropped_files: pass.dropped,
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
                        ..info
                    },
                );
            } else {
                // The root sees the focus of the windows, as eframe reports it.
                for window in self.callbacks.keys().chain(&self.stale) {
                    raw.viewports.insert(
                        *window,
                        egui::ViewportInfo {
                            parent: Some(egui::ViewportId::ROOT),
                            focused: Some(self.focused_child == Some(*window)),
                            ..Default::default()
                        },
                    );
                }
            }
            raw
        }

        /// One pass of the root.
        pub(in crate::app) fn root(&mut self, pass: Pass) -> egui::FullOutput {
            let raw = self.raw(pass, egui::ViewportId::ROOT);
            let Self {
                app, frame, ctx, ..
            } = self;
            // As `ServedApp::ui`: the windows are published after `ui`, with the
            // lock released, and the pass takes it again for the test.
            app.defer_viewport_publication();
            let mut output = ctx.run_ui(raw, |ui| {
                app.ui(ui, frame);
                let pending = app.take_pending_viewports();
                app.release();
                super::super::window_detach::publish_viewports(ui.ctx(), pending);
                app.acquire();
            });
            output.textures_delta.clear();
            for (id, viewport) in &output.viewport_output {
                if let Some(callback) = &viewport.viewport_ui_cb {
                    self.callbacks.insert(*id, callback.clone());
                }
            }
            output
        }

        /// One pass of the detached graph window.
        pub(in crate::app) fn child(&mut self, pass: Pass) -> egui::FullOutput {
            self.child_in(self.child_id(), pass)
        }

        /// One pass of the detached window `id`.
        pub(in crate::app) fn child_in(
            &mut self,
            id: egui::ViewportId,
            pass: Pass,
        ) -> egui::FullOutput {
            self.child_with(id, pass, |_| {})
        }

        /// [`Self::child_in`], with `inside` run first, inside that window's pass
        /// (a request made from the window to the context).
        pub(in crate::app) fn child_with(
            &mut self,
            id: egui::ViewportId,
            pass: Pass,
            inside: impl FnOnce(&egui::Context),
        ) -> egui::FullOutput {
            self.focused_child = pass.focused.then_some(id);
            let raw = self.raw(pass, id);
            let callback = self
                .callbacks
                .get(&id)
                .expect("the root published it")
                .clone();
            // The window locks the application itself: the test lets go of it
            // for the pass, as the root does between its own.
            self.app.release();
            let mut inside = Some(inside);
            let mut output = self.ctx.run_ui(raw, |ui| {
                if let Some(inside) = inside.take() {
                    inside(ui.ctx());
                }
                callback(ui)
            });
            self.app.acquire();
            output.textures_delta.clear();
            // What the window registered again in its pass is what egui keeps; a
            // window that stopped registering is gone from the output.
            match output.viewport_output.get(&id) {
                Some(viewport) => {
                    if let Some(callback) = &viewport.viewport_ui_cb {
                        self.callbacks.insert(id, callback.clone());
                    }
                }
                None => {
                    self.callbacks.remove(&id);
                }
            }
            output
        }

        /// The first passes: the root publishes the window, the window paints.
        pub(in crate::app) fn start(&mut self) {
            self.root(Pass::root());
            for _ in 0..3 {
                self.child(Pass::child());
            }
        }

        /// [`Self::start`] for every window of [`Self::with_every_window`].
        pub(in crate::app) fn start_every_window(&mut self) {
            self.root(Pass::root());
            for _ in 0..4 {
                for window in Window::ALL {
                    self.child_in(self.id_of(window), Pass::child());
                }
                self.root(Pass::root());
            }
        }

        /// What `window` queued for the root and the root has not drained yet.
        /// The graph queues nothing: its actions are applied in its own pass
        /// (#475, L1).
        pub(in crate::app) fn queued_by(&self, window: Window) -> Vec<Action> {
            match window {
                Window::Graph => Vec::new(),
                Window::Book => self.app.detached_tabs[&Tab::Book(self.book())]
                    .requests
                    .lock()
                    .unwrap()
                    .actions
                    .clone(),
                Window::ProjectManager => self.app.pm_requests.lock().unwrap().actions.clone(),
                Window::ObjectManager => self.app.om_view.as_ref().unwrap().queued_actions(),
                Window::Console => self.app.script_view.as_ref().unwrap().queued_actions(),
            }
        }

        /// Has the root published a window `id` that still registers itself?
        pub(in crate::app) fn has_window(&self, id: egui::ViewportId) -> bool {
            self.callbacks.contains_key(&id)
        }

        /// [`Self::act_in_window`] for the window `id`.
        pub(in crate::app) fn act_in(&mut self, id: egui::ViewportId, action: Action) {
            let ctx = self.ctx.clone();
            self.app
                .apply_pass_actions(&ctx, id, Vec::new(), vec![action], false);
        }

        /// The detached graph window produces `action` this pass: applied at once
        /// by the same function as its pass (`apply_pass_actions`), without
        /// painting a gesture.
        pub(in crate::app) fn act_in_window(&mut self, action: Action) {
            let id = self.child_id();
            let ctx = self.ctx.clone();
            self.app
                .apply_pass_actions(&ctx, id, Vec::new(), vec![action], false);
        }

        pub(in crate::app) fn open_plot_settings(&self) -> Action {
            Action::OpenPlotSettings {
                graph: self.graph,
                tab: PlotSettingsTab::Graph,
                axis: None,
            }
        }

        pub(in crate::app) fn finish(self) {
            drop(self.app);
            drop(self.dir);
            self.session.shutdown().unwrap();
        }
    }

    /// The repaint delay a pass asked for one viewport.
    pub(in crate::app) fn delay(output: &egui::FullOutput, id: egui::ViewportId) -> Duration {
        output.viewport_output[&id].repaint_delay
    }
}

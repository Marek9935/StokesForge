//! Memory of the layout of the three fixed panels between two launches (#148):
//! Project Manager, Object Manager, Script Console. The decisions are pure and
//! live in `stokesforge_app_state::panel_layout`; this module gathers what only
//! the shell knows — the live size and position of a detached window, the number
//! of screens — and applies the result.
//!
//! - **At startup** ([`StokesForgeApp::apply_startup_panels`], once, before the
//!   first frame): the collapsed state is restored; a panel that was detached is
//!   re-detached only if the machine has the same number of screens as when it
//!   was saved, at the saved size and, where the system can place a window
//!   (X11, Windows, macOS), at the saved position. Wayland cannot read or set a
//!   position (winit): the window then opens at its size and the compositor
//!   places it.
//! - **While running** ([`StokesForgeApp::sync_panel_layout`]): every change of a
//!   collapsed or detached state — whatever gesture caused it, the header
//!   buttons, the OS close, a script run expanding the console — sends
//!   `Action::RememberPanelLayout`. The detached windows report their live
//!   size and position ([`LiveWindow`]), which are saved with it and at exit
//!   ([`StokesForgeApp::flush_panel_layout`]).
//! - **The builders** of the detached viewports take the saved size and position
//!   **only when the window opens** ([`StokesForgeApp::detach_init`], constant
//!   for the whole detached session): a value renewed on each frame would fight
//!   the user's drag. A manual Detach reuses the saved size and place too.
//!
//! Nothing here is visible in Preferences, and the `.stf` is not involved.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use stokesforge_app_state::panel_layout::{
    startup_panels, DetachAt, PanelKind, PanelLayout, PanelPrefs,
};
use stokesforge_app_state::state::Action;

use super::StokesForgeApp;

/// How often the number of screens is read again (the system call may be a
/// round trip to the display server).
const MONITOR_REFRESH: Duration = Duration::from_secs(2);

/// Size and position last reported by the OS window of a panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LiveGeometry {
    pub(super) size: [f32; 2],
    /// `None` where the system cannot tell (Wayland).
    pub(super) position: Option<[f32; 2]>,
}

/// Shared between the root and the callback of a detached viewport, which can
/// only capture `'static` data.
#[derive(Clone, Default)]
pub(super) struct LiveWindow(Arc<Mutex<Option<LiveGeometry>>>);

impl LiveWindow {
    /// Called by the viewport callback on each of its frames.
    pub(super) fn report(&self, ctx: &egui::Context) {
        let (inner, outer) = ctx.input(|input| {
            let viewport = input.viewport();
            (viewport.inner_rect, viewport.outer_rect)
        });
        let Some(size) = inner.or(outer).map(|rect| rect.size()) else {
            return;
        };
        if !(size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0) {
            return;
        }
        let position = outer
            .map(|rect| rect.min)
            .filter(|min| min.x.is_finite() && min.y.is_finite())
            .map(|min| [min.x, min.y]);
        *self.0.lock().unwrap_or_else(|err| err.into_inner()) = Some(LiveGeometry {
            size: [size.x, size.y],
            position,
        });
    }

    pub(super) fn get(&self) -> Option<LiveGeometry> {
        *self.0.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn clear(&self) {
        *self.0.lock().unwrap_or_else(|err| err.into_inner()) = None;
    }
}

/// What the shell remembers about the panels' layout.
#[derive(Default)]
pub(super) struct PanelMemory {
    live: [LiveWindow; 3],
    /// Where the detached window of each panel opens (constant while it lasts).
    init: [Option<DetachAt>; 3],
    /// `(collapsed, detached)` of each panel as last handed to the preferences.
    flags_saved: [(bool, bool); 3],
    /// Number of screens, read at most every [`MONITOR_REFRESH`].
    monitors: Option<u32>,
    monitors_at: Option<Instant>,
}

fn index(kind: PanelKind) -> usize {
    match kind {
        PanelKind::Project => 0,
        PanelKind::Object => 1,
        PanelKind::Script => 2,
    }
}

/// The number of screens from what winit reports (`available_monitors().count()`);
/// `None` in a headless test or if the system reports none.
fn monitors_from_count(count: Option<usize>) -> Option<u32> {
    count
        .and_then(|count| u32::try_from(count).ok())
        .filter(|count| *count > 0)
}

impl StokesForgeApp {
    /// `(collapsed, detached)` of each panel, in [`PanelKind::ALL`] order.
    fn panel_flags(&self) -> [(bool, bool); 3] {
        [
            (self.pm_collapsed, self.pm_detached),
            (self.om_collapsed, self.om_detached),
            (self.script_collapsed, self.script_detached),
        ]
    }

    /// The live window of a panel, for its viewport callback.
    pub(super) fn live_window(&self, kind: PanelKind) -> LiveWindow {
        self.panel_memory.live[index(kind)].clone()
    }

    /// Where the OS window of `kind` opens: the saved size and position, `None`
    /// for the default. Constant while the window stays detached.
    pub(super) fn detach_init(&self, kind: PanelKind) -> Option<DetachAt> {
        self.panel_memory.init[index(kind)]
    }

    /// Applies the remembered layout, once, before the first frame:
    /// `screens` is the number of screens now, as the window reports it
    /// (`available_monitors().count()`; `None` without a window).
    pub fn apply_startup_panels(&mut self, screens: Option<usize>) {
        let current_monitors = monitors_from_count(screens);
        let saved = self.state.panel_layout();
        self.panel_memory.monitors = current_monitors;
        self.panel_memory.monitors_at = Some(Instant::now());
        let [project, object, script] = startup_panels(&saved, current_monitors);
        self.pm_collapsed = project.collapsed;
        self.om_collapsed = object.collapsed;
        self.script_collapsed = script.collapsed;
        if let Some(at) = project.detach {
            self.panel_memory.init[index(PanelKind::Project)] = Some(at);
            self.detach_project_manager();
        }
        if let Some(at) = object.detach {
            self.panel_memory.init[index(PanelKind::Object)] = Some(at);
            self.om_detached = true;
        }
        if let Some(at) = script.detach {
            self.panel_memory.init[index(PanelKind::Script)] = Some(at);
            self.detach_script_console();
        }
        // What was just restored is not a change to remember.
        self.panel_memory.flags_saved = self.panel_flags();
    }

    /// Start of a frame: a panel detached by a gesture gets its opening size and
    /// place from the saved layout; an attached one forgets them.
    pub(super) fn prepare_panel_windows(&mut self) {
        let flags = self.panel_flags();
        let saved = self.state.panel_layout();
        for kind in PanelKind::ALL {
            let i = index(kind);
            let (_, detached) = flags[i];
            if !detached {
                self.panel_memory.init[i] = None;
            } else if self.panel_memory.init[i].is_none() {
                let prefs = saved.panel(kind);
                self.panel_memory.live[i].clear();
                self.panel_memory.init[i] = prefs
                    .size
                    .filter(|size| size.iter().all(|side| side.is_finite() && *side > 0.0))
                    .map(|size| DetachAt {
                        size,
                        position: prefs.position,
                    });
            }
        }
    }

    /// The layout as it is now: the flags of the panels and, for each, the last
    /// geometry its window reported (else the saved one).
    fn current_panel_layout(&self) -> PanelLayout {
        let saved = self.state.panel_layout();
        let flags = self.panel_flags();
        let mut layout = PanelLayout {
            monitors: self.panel_memory.monitors,
            ..saved
        };
        for kind in PanelKind::ALL {
            let i = index(kind);
            let (collapsed, detached) = flags[i];
            let previous = saved.panel(kind);
            let (size, position) = match self.panel_memory.live[i].get() {
                Some(live) => (Some(live.size), live.position),
                None => match self.panel_memory.init[i] {
                    Some(at) if detached => (Some(at.size), at.position),
                    _ => (previous.size, previous.position),
                },
            };
            *layout.panel_mut(kind) = PanelPrefs {
                collapsed,
                detached,
                size,
                position,
            };
        }
        layout
    }

    /// Reads the number of screens again if it has been a while; `count` asks
    /// the window (`None` without one).
    pub(super) fn refresh_monitors(&mut self, count: impl FnOnce() -> Option<usize>) {
        let due = self
            .panel_memory
            .monitors_at
            .is_none_or(|at| at.elapsed() >= MONITOR_REFRESH);
        if due {
            if let Some(count) = monitors_from_count(count()) {
                self.panel_memory.monitors = Some(count);
            }
            self.panel_memory.monitors_at = Some(Instant::now());
        }
    }

    /// After the panels are painted: a change of a collapsed or detached state,
    /// by any gesture, is handed to the preferences.
    pub(super) fn sync_panel_layout(&mut self, actions: &mut Vec<Action>) {
        let flags = self.panel_flags();
        if flags == self.panel_memory.flags_saved {
            return;
        }
        self.panel_memory.flags_saved = flags;
        actions.push(Action::RememberPanelLayout(self.current_panel_layout()));
    }

    /// At exit: the sizes and positions the windows reported since the last
    /// change. Read from the state of the panels as they are, before the windows
    /// are torn down.
    pub(super) fn flush_panel_layout(&mut self) {
        let layout = self.current_panel_layout();
        self.state.dispatch(Action::RememberPanelLayout(layout));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_app_state::preferences::Preferences;
    use stokesforge_app_state::state::AppState;
    use stokesforge_core::Project;
    use stokesforge_session::Session;

    /// An application whose preferences are read from (and written to) `file`.
    fn app_on(file: &std::path::Path) -> (Session, StokesForgeApp) {
        let session = Session::spawn(Project::new("Panels").unwrap(), None).unwrap();
        let state = AppState::new_with_preferences(
            session.handle(),
            String::new(),
            Preferences::load_from(Some(file.to_path_buf())),
        )
        .unwrap();
        (session, StokesForgeApp::new(state))
    }

    fn written(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    const SAVED: &str = r#"{"language":"fr","panels":{"monitors":2,
        "project":{"collapsed":true,"detached":false},
        "object":{"collapsed":false,"detached":true,"size":[330,480],"position":[1500,40]},
        "script":{"collapsed":false,"detached":true,"size":[700,440]}}}"#;

    /// Same screens: the detached panels come back detached (the console
    /// through `detach_script_console`, so its view exists), the collapsed state
    /// is restored, and nothing is written back for that.
    #[test]
    fn the_saved_layout_is_restored_at_startup() {
        let (_dir, file) = written(SAVED);
        let (session, mut app) = app_on(&file);
        app.apply_startup_panels(Some(2));
        assert!(app.pm_collapsed, "collapsed is restored");
        assert!(!app.om_collapsed && !app.script_collapsed);
        assert!(app.om_detached && app.script_detached && !app.pm_detached);
        assert!(app.script_view.is_some(), "the console view exists");
        assert_eq!(
            app.detach_init(PanelKind::Object),
            Some(DetachAt {
                size: [330.0, 480.0],
                position: Some([1500.0, 40.0])
            })
        );
        assert_eq!(
            app.detach_init(PanelKind::Script),
            Some(DetachAt {
                size: [700.0, 440.0],
                position: None
            }),
            "Wayland case: the size alone"
        );
        let mut actions = Vec::new();
        app.sync_panel_layout(&mut actions);
        assert!(actions.is_empty(), "a restore is not a change to remember");
        session.shutdown().unwrap();
    }

    /// Another number of screens, or an unknown one: attached, the collapsed
    /// state kept.
    #[test]
    fn another_screen_count_attaches_the_panels() {
        for screens in [Some(1), Some(3), None] {
            let (_dir, file) = written(SAVED);
            let (session, mut app) = app_on(&file);
            app.apply_startup_panels(screens);
            assert!(!app.om_detached && !app.script_detached, "{screens:?}");
            assert!(app.script_view.is_none());
            assert!(app.pm_collapsed, "{screens:?}: collapsed is kept");
            session.shutdown().unwrap();
        }
    }

    /// Collapsing or detaching, by any gesture, hands the layout to the
    /// preferences; an unchanged layout sends nothing.
    #[test]
    fn a_change_of_state_is_remembered_and_a_quiet_frame_is_not() {
        let (_dir, file) = written(r#"{"language":"fr"}"#);
        let (session, mut app) = app_on(&file);
        app.apply_startup_panels(Some(2));
        let mut actions = Vec::new();
        app.sync_panel_layout(&mut actions);
        assert!(actions.is_empty(), "nothing changed");
        app.om_collapsed = true;
        app.sync_panel_layout(&mut actions);
        assert_eq!(actions.len(), 1);
        for action in actions.drain(..) {
            app.state.dispatch(action);
        }
        let saved = Preferences::load_from(Some(file.clone())).panel_layout();
        assert!(saved.object.collapsed);
        assert_eq!(saved.monitors, Some(2));
        assert_eq!(
            Preferences::load_from(Some(file)).language(),
            Some("fr"),
            "the other fields are kept"
        );
        app.sync_panel_layout(&mut actions);
        assert!(actions.is_empty(), "the same state is not sent twice");
        session.shutdown().unwrap();
    }

    /// A manual Detach reuses the saved size and place; a window reattached
    /// forgets them; the live geometry of a window is what gets saved.
    #[test]
    fn a_manual_detach_reuses_the_saved_place_and_the_live_geometry_is_saved() {
        let (_dir, file) = written(SAVED);
        let (session, mut app) = app_on(&file);
        app.apply_startup_panels(Some(1));
        app.pm_detached = true;
        app.prepare_panel_windows();
        assert_eq!(
            app.detach_init(PanelKind::Project),
            None,
            "never saved: default"
        );
        app.om_detached = true;
        app.prepare_panel_windows();
        assert_eq!(
            app.detach_init(PanelKind::Object).map(|at| at.size),
            Some([330.0, 480.0]),
            "the saved size"
        );
        // The window is dragged and resized: the callback reports it.
        let ctx = egui::Context::default();
        let rect = |x, y, w, h| egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
        let mut raw = egui::RawInput::default();
        raw.viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                inner_rect: Some(rect(100.0, 120.0, 400.0, 500.0)),
                outer_rect: Some(rect(100.0, 120.0, 400.0, 500.0)),
                ..Default::default()
            },
        );
        let mut output = ctx.run_ui(raw, |ui| {
            app.live_window(PanelKind::Object).report(ui.ctx())
        });
        output.textures_delta.clear();
        assert_eq!(
            app.live_window(PanelKind::Object).get(),
            Some(LiveGeometry {
                size: [400.0, 500.0],
                position: Some([100.0, 120.0])
            })
        );
        let layout = app.current_panel_layout();
        assert_eq!(layout.object.size, Some([400.0, 500.0]));
        assert_eq!(layout.object.position, Some([100.0, 120.0]));
        // At exit the live geometry reaches the file.
        app.flush_panel_layout();
        let saved = Preferences::load_from(Some(file.clone())).panel_layout();
        assert_eq!(saved.object.size, Some([400.0, 500.0]));
        assert_eq!(saved.object.position, Some([100.0, 120.0]));
        assert!(saved.object.detached);
        // Reattached: the init is forgotten, the geometry stays for next time.
        app.om_detached = false;
        app.prepare_panel_windows();
        assert_eq!(app.detach_init(PanelKind::Object), None);
        assert_eq!(app.current_panel_layout().object.size, Some([400.0, 500.0]));
        session.shutdown().unwrap();
    }

    /// Wayland: the window reports no outer rect, so no position is kept.
    #[test]
    fn a_window_without_a_position_reports_its_size_alone() {
        let live = LiveWindow::default();
        let ctx = egui::Context::default();
        let mut raw = egui::RawInput::default();
        raw.viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                inner_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 240.0),
                )),
                outer_rect: None,
                ..Default::default()
            },
        );
        let mut output = ctx.run_ui(raw, |ui| live.report(ui.ctx()));
        output.textures_delta.clear();
        assert_eq!(
            live.get(),
            Some(LiveGeometry {
                size: [320.0, 240.0],
                position: None
            })
        );
    }

    /// The builder of a detached window carries the saved size and position, and
    /// the defaults when there are none.
    #[test]
    fn the_viewport_builder_takes_the_saved_size_and_position() {
        use super::super::window_detach::panel_viewport;
        let saved = panel_viewport(
            "T".into(),
            [330.0, 480.0],
            [220.0, 140.0],
            Some(DetachAt {
                size: [400.0, 500.0],
                position: Some([100.0, 120.0]),
            }),
        );
        assert_eq!(saved.inner_size, Some(egui::vec2(400.0, 500.0)));
        assert_eq!(saved.position, Some(egui::pos2(100.0, 120.0)));
        let sized = panel_viewport(
            "T".into(),
            [330.0, 480.0],
            [220.0, 140.0],
            Some(DetachAt {
                size: [400.0, 500.0],
                position: None,
            }),
        );
        assert_eq!(sized.position, None, "Wayland: the compositor places it");
        let default = panel_viewport("T".into(), [330.0, 480.0], [220.0, 140.0], None);
        assert_eq!(default.inner_size, Some(egui::vec2(330.0, 480.0)));
    }

    /// Through the real `ui()`: collapsing the Object Manager by its header
    /// ends in the preferences file, without the gesture knowing about it.
    #[test]
    fn collapsing_the_object_manager_in_a_frame_saves_the_layout() {
        let (_dir, file) = written(r#"{"language":"fr"}"#);
        let (session, mut app) = app_on(&file);
        app.state.set_object_manager_open_for_test(true);
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let mut run = |app: &mut StokesForgeApp| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| eframe::App::ui(app, ui, &mut frame),
            );
            output.textures_delta.clear();
        };
        run(&mut app);
        assert!(
            !Preferences::load_from(Some(file.clone()))
                .panel_layout()
                .object
                .collapsed
        );
        app.om_collapsed = true;
        run(&mut app);
        run(&mut app);
        assert!(
            Preferences::load_from(Some(file))
                .panel_layout()
                .object
                .collapsed,
            "the collapse reached preferences.json"
        );
        session.shutdown().unwrap();
    }
}

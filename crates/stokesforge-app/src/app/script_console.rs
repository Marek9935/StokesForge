//! Script console: input area, "Run"/"Stop" buttons, output. Contents of
//! [`StokesForgeApp::script_panel`].
//!
//! Never blocks the window: see the head doc of `app.rs`, § "The script
//! console does not freeze the window".
//! `Action::RunScript`/`InterruptScript` are pushed like any other action
//! and applied after rendering by the main loop — [`AppState::dispatch`]
//! guarantees they do not block.
//!
//! A single content reads AppState in attached mode and `ScriptConsoleModel`
//! in detached mode (#100): no engine migrates into the callback. The input
//! buffer goes up before the actions, the output stays the root's.

use eframe::egui;

use stokesforge_app_state::state::{Action, AppState, ScriptOutput};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::shortcuts::SHORTCUT_RUN_SCRIPT;
use super::window_chrome::{
    panel_header, show_window_contents_bounded, PanelPlacement, WindowHeaderActions,
};
use super::StokesForgeApp;

/// Height of the script output area, in points.
const SCRIPT_OUTPUT_HEIGHT: f32 = 140.0;

/// Title bar of the script console, with the button that collapses or
/// expands it (issue #79), and the one that detaches it (#100).
pub(super) fn script_header(
    ui: &mut egui::Ui,
    state: &AppState,
    collapsed: bool,
) -> WindowHeaderActions {
    let header = panel_header(ui, &tr!("script-title"), PanelPlacement::Bottom, collapsed);
    if !collapsed && !state.script_available() {
        ui.weak(tr!("script-unavailable"));
    }
    header
}

/// Reading of the console for the deferred callback; the engine and its work
/// stay in AppState at the root. Only the input goes up as a request.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ScriptConsoleModel {
    pub(super) source: String,
    pub(super) output: Option<ScriptOutput>,
    pub(super) running: bool,
    pub(super) available: bool,
}

impl ScriptConsoleModel {
    pub(super) fn from_state(state: &AppState) -> Self {
        Self {
            source: state.script_source().to_owned(),
            output: state.script_output().cloned(),
            running: state.script_running(),
            available: state.script_available(),
        }
    }
}

pub(super) fn script_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut AppState,
    actions: &mut Vec<Action>,
) {
    let mut source = state.script_source().to_owned();
    script_fields_contents(
        ui,
        ctx,
        &mut source,
        state.script_output(),
        state.script_running(),
        state.script_available(),
        actions,
    );
    if source != state.script_source() {
        state.set_script_source(source);
    }
}

pub(super) fn script_model_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    model: &mut ScriptConsoleModel,
    actions: &mut Vec<Action>,
) {
    script_fields_contents(
        ui,
        ctx,
        &mut model.source,
        model.output.as_ref(),
        model.running,
        model.available,
        actions,
    );
}

/// The attached console borrows the output: no copy of a whole result on
/// every frame. The detached reading already owns its published copy.
fn script_fields_contents(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    source: &mut String,
    output: Option<&ScriptOutput>,
    running: bool,
    available: bool,
    actions: &mut Vec<Action>,
) {
    if running {
        // Without this, egui redraws only on an input event: the end of the
        // script would show up only at the user's next gesture.
        ctx.request_repaint();
    }

    let unavailable_reason = if running {
        tr!("disabled-script-running")
    } else {
        tr!("disabled-script-unavailable")
    };
    let available = available && !running;
    ui.add_enabled(
        available,
        egui::TextEdit::multiline(source)
            .code_editor()
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    )
    .on_disabled_hover_text(&unavailable_reason);

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                available,
                egui::Button::new(tr!("script-run"))
                    .shortcut_text(ctx.format_shortcut(&SHORTCUT_RUN_SCRIPT)),
            )
            .on_disabled_hover_text(&unavailable_reason)
            .clicked()
        {
            actions.push(Action::RunScript);
        }
        if ui
            .add_enabled(running, egui::Button::new(tr!("script-stop")))
            .on_disabled_hover_text(tr!("disabled-script-not-running"))
            .clicked()
        {
            actions.push(Action::InterruptScript);
        }
        if running {
            super::busy::indicator(ui, &tr!("script-running"));
        }
    });

    ui.separator();
    // RAW-SCROLL(#347): console output log, not a page
    egui::ScrollArea::vertical()
        .max_height(SCRIPT_OUTPUT_HEIGHT)
        .show(ui, |ui| match output {
            None => {
                empty_state(ui, tr!("script-not-run"));
            }
            Some(output) => {
                let mut empty = true;
                if !output.stdout.is_empty() {
                    empty = false;
                    ui.label(tr!("script-stdout"));
                    ui.monospace(&output.stdout);
                }
                if !output.stderr.is_empty() {
                    empty = false;
                    ui.label(tr!("script-stderr"));
                    ui.monospace(&output.stderr);
                }
                if let Some(error) = &output.error {
                    empty = false;
                    ui.colored_label(ui.visuals().warn_fg_color, tr!("script-traceback"));
                    ui.monospace(error);
                }
                if empty {
                    ui.weak(tr!("script-no-output"));
                }
            }
        });
}

/// Default height of the script console's fixed panel — a starting point
/// only, the user can then resize it by dragging its edge
/// (`.resizable(true)`, see [`StokesForgeApp::script_panel`]).
const SCRIPT_PANEL_HEIGHT: f32 = 220.0;

impl StokesForgeApp {
    /// Paints the script console in its fixed panel, anchored at the bottom
    /// — see the head doc of `app.rs`, § What stays here (the order of the
    /// panels). Called by
    /// [`StokesForgeApp::ui`](eframe::App::ui) *before* [`StokesForgeApp::project_manager_panel`]
    /// (`egui::Panel::left`): with `egui::Panel`, the first panel added
    /// is the outermost, so this one, placed first, reserves its band
    /// across the whole window width — including under the Project
    /// Manager, not only under the `CentralPanel` area — instead of
    /// being squeezed between it and the right edge. `egui::Panel::bottom`
    /// is **not** resizable by default (unlike `left`/`right`):
    /// `.resizable(true)` restores dragging the edge in height. Detaching
    /// uses the same console reading and leaves the engine at the root;
    /// hiding it through View does not suspend its work.
    ///
    /// Collapsed (issue #79), the console shrinks to its title bar — a
    /// separate, non-resizable panel, so that the remembered height of the
    /// expanded panel is not overwritten.
    pub(super) fn script_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
    ) {
        if actions
            .iter()
            .any(|action| matches!(action, Action::RunScript))
        {
            self.script_collapsed = false;
        }
        self.drain_script_console(ctx, actions);
        if !self.script_open {
            return;
        }
        if self.script_detached {
            self.show_detached_script_console(ctx);
            return;
        }
        let collapsed = self.script_collapsed;
        let panel = egui::Panel::bottom(if collapsed {
            "stokesforge_script_collapsed"
        } else {
            "stokesforge_script"
        })
        .resizable(!collapsed);
        let panel = if collapsed {
            panel
                .exact_size(super::window_chrome::PANEL_STRIP_THICKNESS)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(0))
                .show_separator_line(false)
        } else {
            panel.default_size(SCRIPT_PANEL_HEIGHT)
        };
        panel.show(ui, |ui| {
            let header = script_header(ui, &self.state, collapsed);
            self.script_collapsed ^= header.toggle_collapse;
            if header.detach {
                self.detach_script_console();
            }
            if !collapsed {
                // The content alone may be shorter than the chosen height:
                // filling the reserved space keeps it (#100).
                show_window_contents_bounded(ui, |ui| {
                    script_contents(ui, ctx, &mut self.state, actions);
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::Project;
    use stokesforge_session::Session;

    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::super::test_support::painted;
    use super::super::window_chrome::tests::find_icon_centers;

    fn state() -> (Session, AppState) {
        let project = Project::new("Console").unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state)
    }

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 400.0),
            )),
            events,
            ..Default::default()
        }
    }

    /// The title bar (#79): a click on its chevron reports it, a frame
    /// without a click does not; it says when the interpreter is missing from this binary.
    #[test]
    fn the_title_bar_reports_the_chevron_click_and_the_missing_interpreter() {
        let (_session, state) = state();
        for collapsed in [true, false] {
            let context = egui::Context::default();
            egui_extras::install_image_loaders(&context);
            let toggled = std::cell::Cell::new(false);
            let paint = |context: &egui::Context, events| {
                let mut output = context.run_ui(input(events), |ui| {
                    toggled
                        .set(toggled.get() | script_header(ui, &state, collapsed).toggle_collapse);
                });
                output.textures_delta.clear();
                output
            };
            paint(&context, Vec::new());
            let output = paint(&context, Vec::new());
            assert!(!toggled.get(), "no click, no toggle");
            assert!(painted(&output, &tr!("script-title")));
            assert_eq!(
                painted(&output, &tr!("script-unavailable")),
                !collapsed && !state.script_available(),
                "the missing interpreter is reported, and only it"
            );
            let chevron = find_icon_centers(&output.shapes)[0];
            click_at(&context, chevron, egui::Modifiers::NONE, paint);
            assert!(
                toggled.get(),
                "a click on the chevron toggles (collapsed: {collapsed})"
            );
        }
    }

    /// "Run" emits `RunScript` only if an interpreter is there;
    /// "Stop" is greyed out while no script is running. Before any
    /// run, the output says so.
    #[test]
    fn run_emits_only_if_an_interpreter_is_there_and_stop_stays_greyed_out() {
        let (_session, mut state) = state();
        let context = egui::Context::default();
        let actions = std::cell::RefCell::new(Vec::new());
        let state = std::cell::RefCell::new(&mut state);
        let paint = |context: &egui::Context, events| {
            let mut output = context.run_ui(input(events), |ui| {
                let ctx = ui.ctx().clone();
                script_contents(ui, &ctx, &mut state.borrow_mut(), &mut actions.borrow_mut());
            });
            output.textures_delta.clear();
            output
        };
        paint(&context, Vec::new());
        let output = paint(&context, Vec::new());
        assert!(painted(&output, &tr!("script-not-run")));
        let available = state.borrow().script_available();

        click_at(
            &context,
            find_text_center(&output.shapes, &tr!("script-stop")),
            egui::Modifiers::NONE,
            paint,
        );
        assert!(
            actions.borrow().is_empty(),
            "Stop is greyed out with no script running"
        );

        click_at(
            &context,
            find_text_center(&output.shapes, &tr!("script-run")),
            egui::Modifiers::NONE,
            paint,
        );
        let expected = if available {
            vec![Action::RunScript]
        } else {
            Vec::new()
        };
        assert_eq!(
            *actions.borrow(),
            expected,
            "interpreter present: {available}"
        );
    }

    /// Collapsed at startup, the console has only its title bar (#79);
    /// a run requested in the frame expands it, so that its output is
    /// seen, and the expanded panel paints the content.
    #[test]
    fn launching_a_script_expands_the_console() {
        let (_session, state) = state();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        egui_extras::install_image_loaders(&context);
        let paint = |app: &mut StokesForgeApp, mut actions: Vec<Action>| {
            let mut output = context.run_ui(input(Vec::new()), |ui| {
                let ctx = ui.ctx().clone();
                app.script_panel(ui, &ctx, &mut actions);
            });
            output.textures_delta.clear();
            output
        };
        assert!(app.script_collapsed, "collapsed at startup");
        let output = paint(&mut app, Vec::new());
        assert!(app.script_collapsed, "without a run, it stays collapsed");
        assert!(painted(&output, &tr!("script-title")));
        assert!(
            !painted(&output, &tr!("script-not-run")),
            "collapsed: no content"
        );

        let output = paint(&mut app, vec![Action::RunScript]);
        assert!(!app.script_collapsed, "a run expands it");
        assert!(painted(&output, &tr!("script-not-run")));
    }
}

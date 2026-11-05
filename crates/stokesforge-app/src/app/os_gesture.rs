//! Handing a pointer gesture to the OS (#457).
//!
//! `ViewportCommand::StartDrag` (move a detached window by its title) and
//! `ViewportCommand::BeginResize` (resize it by an edge) make the compositor
//! take the pointer: on Wayland (`xdg_toplevel.move` / `.resize`) it keeps the
//! button release for itself. egui never sees it, so its `PointerState` keeps
//! the button down and the widget that started the gesture stays `dragged`.
//! The next press on that widget then gets no `drag_started()` (every second
//! grab did nothing), hover is blocked while a button is "down", and a stuck
//! `primary_down` extends the column selection of a grid on hover.
//!
//! This module is the only place that sends those two commands (guard
//! `no_os_gesture_command_outside_the_helper`). Besides the command, it
//! registers an egui [`egui::Plugin`] whose `input_hook` plays, at the head of
//! the next pass of the same viewport, the release the OS swallowed: a pointer
//! move farther than `max_click_dist` from the press (so the release is not a
//! click: no focus surrendered, no double click), then one release per button
//! still down, at the press position. Events really received in the same batch
//! come after and are handled as usual.
//!
//! `Plugin::input_hook` is used rather than eframe's `raw_input_hook` because
//! it runs for every pass of every viewport, bare `Context::run_ui` included.
//! At that point no pass has begun: the viewport is `RawInput::viewport_id`,
//! never `Context::viewport_id()`.

use eframe::egui;
use std::collections::HashSet;

/// Hand the move of the window to the OS, from the title.
pub(super) fn start_drag(ctx: &egui::Context) {
    hand_over(ctx, egui::ViewportCommand::StartDrag);
}

/// Hand the resize of the window to the OS, from an edge or a corner.
pub(super) fn begin_resize(ctx: &egui::Context, direction: egui::ResizeDirection) {
    hand_over(ctx, egui::ViewportCommand::BeginResize(direction));
}

fn hand_over(ctx: &egui::Context, command: egui::ViewportCommand) {
    ctx.send_viewport_cmd(command);
    ctx.add_plugin(OsGesturePlugin::default());
    let viewport = ctx.viewport_id();
    ctx.with_plugin(|plugin: &mut OsGesturePlugin| {
        plugin.taken.insert(viewport);
    });
    // The release is played in the next pass, which must happen even if the
    // pointer does not move any more (the OS owns it).
    ctx.request_repaint();
}

/// Viewports whose pointer gesture the OS took, and whose release is owed.
#[derive(Default)]
struct OsGesturePlugin {
    taken: HashSet<egui::ViewportId>,
}

impl egui::Plugin for OsGesturePlugin {
    fn debug_name(&self) -> &'static str {
        "os_gesture"
    }

    fn input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if !self.taken.remove(&input.viewport_id) {
            return;
        }
        let max_click_dist = ctx.options(|options| options.input_options.max_click_dist);
        let released = ctx.input_for(input.viewport_id, |state| {
            release_events(&state.pointer, max_click_dist, state.modifiers)
        });
        input.events.splice(0..0, released);
    }
}

/// The events that end, in egui, a gesture whose release the OS kept.
fn release_events(
    pointer: &egui::PointerState,
    max_click_dist: f32,
    modifiers: egui::Modifiers,
) -> Vec<egui::Event> {
    const BUTTONS: [egui::PointerButton; 5] = [
        egui::PointerButton::Primary,
        egui::PointerButton::Secondary,
        egui::PointerButton::Middle,
        egui::PointerButton::Extra1,
        egui::PointerButton::Extra2,
    ];
    let Some(origin) = pointer.press_origin() else {
        return Vec::new();
    };
    let down: Vec<_> = BUTTONS
        .into_iter()
        .filter(|button| pointer.button_down(*button))
        .collect();
    if down.is_empty() {
        return Vec::new();
    }
    // Too far for a click: egui's `has_moved_too_much_for_a_click`.
    let mut events = vec![egui::Event::PointerMoved(
        origin + egui::vec2(max_click_dist + 1.0, 0.0),
    )];
    events.extend(down.into_iter().map(|button| egui::Event::PointerButton {
        pos: origin,
        button,
        pressed: false,
        modifiers,
    }));
    if pointer.latest_pos().is_none() {
        events.push(egui::Event::PointerGone);
    }
    events
}

#[cfg(test)]
mod tests {
    use super::super::test_support::context;
    use super::super::window_chrome::detached_resize_handles;
    use super::*;
    use std::cell::Cell;

    const P: egui::Pos2 = egui::pos2(100.0, 15.0);
    const SIZE: egui::Vec2 = egui::vec2(400.0, 300.0);

    fn other_viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("other")
    }

    fn pass_in(
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        events: Vec<egui::Event>,
        content: impl FnMut(&mut egui::Ui),
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                viewport_id: viewport,
                viewports: [egui::ViewportId::ROOT, other_viewport(), viewport]
                    .into_iter()
                    .map(|id| (id, egui::ViewportInfo::default()))
                    .collect(),
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                events,
                ..Default::default()
            },
            content,
        );
        output.textures_delta.clear();
        output
    }

    fn pass(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        content: impl FnMut(&mut egui::Ui),
    ) -> egui::FullOutput {
        pass_in(ctx, egui::ViewportId::ROOT, events, content)
    }

    fn count(
        output: &egui::FullOutput,
        matching: impl Fn(&egui::ViewportCommand) -> bool,
    ) -> usize {
        output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .filter(|command| matching(command))
            .count()
    }

    fn start_drags(output: &egui::FullOutput) -> usize {
        count(output, |c| matches!(c, egui::ViewportCommand::StartDrag))
    }

    fn resizes(output: &egui::FullOutput) -> usize {
        count(output, |c| {
            matches!(c, egui::ViewportCommand::BeginResize(_))
        })
    }

    fn moved(pos: egui::Pos2) -> egui::Event {
        egui::Event::PointerMoved(pos)
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn any_down(ctx: &egui::Context) -> bool {
        ctx.input(|input| input.pointer.any_down())
    }

    /// A `Sense::drag` zone at the top (the shape of a window title) that hands
    /// its drag to the OS, and one below that does not.
    #[derive(Default)]
    struct Probe {
        started: Cell<usize>,
        dragged: Cell<bool>,
        other_dragged: Cell<bool>,
        clicks: Cell<usize>,
        double_clicks: Cell<usize>,
    }

    impl Probe {
        fn show(&self, ui: &mut egui::Ui) {
            let title = ui.interact(
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 30.0)),
                egui::Id::new("probe_title"),
                egui::Sense::drag(),
            );
            if title.drag_started() {
                self.started.set(self.started.get() + 1);
                start_drag(ui.ctx());
            }
            self.dragged.set(title.dragged());
            let other = ui.interact(
                egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(300.0, 30.0)),
                egui::Id::new("probe_other"),
                egui::Sense::drag(),
            );
            self.other_dragged.set(other.dragged());
            ui.input(|input| {
                if input.pointer.any_click() {
                    self.clicks.set(self.clicks.get() + 1);
                }
                if input
                    .pointer
                    .button_double_clicked(egui::PointerButton::Primary)
                {
                    self.double_clicks.set(self.double_clicks.get() + 1);
                }
            });
        }
    }

    /// Press the title, let one pass go by (the OS owns the pointer).
    fn grab(ctx: &egui::Context, probe: &Probe) {
        pass(ctx, vec![moved(P), button(P, true)], |ui| probe.show(ui));
        pass(ctx, vec![], |ui| probe.show(ui));
    }

    #[test]
    fn a_second_grab_of_the_title_starts_a_drag_again() {
        let ctx = context();
        let probe = Probe::default();
        pass(&ctx, vec![], |ui| probe.show(ui));
        pass(&ctx, vec![moved(P), button(P, true)], |ui| probe.show(ui));
        assert_eq!(probe.started.get(), 1);
        pass(&ctx, vec![], |ui| probe.show(ui));
        pass(&ctx, vec![button(P, true)], |ui| probe.show(ui));
        assert_eq!(probe.started.get(), 2);
    }

    #[test]
    fn the_gesture_leaves_no_button_down_and_no_dragged_widget() {
        let ctx = context();
        let probe = Probe::default();
        pass(&ctx, vec![], |ui| probe.show(ui));
        grab(&ctx, &probe);
        assert!(!any_down(&ctx));
        assert_eq!(ctx.dragged_id(), None);
        assert!(!probe.dragged.get());
    }

    #[test]
    fn a_resize_zone_hands_the_resize_over_every_time() {
        let ctx = context();
        let edge = egui::pos2(200.0, 3.0);
        pass(&ctx, vec![], |ui| detached_resize_handles(ui));
        let mut total = 0;
        for _ in 0..2 {
            let a = pass(&ctx, vec![moved(edge), button(edge, true)], |ui| {
                detached_resize_handles(ui)
            });
            let b = pass(&ctx, vec![], |ui| detached_resize_handles(ui));
            total += resizes(&a) + resizes(&b);
        }
        assert_eq!(total, 2);
    }

    #[test]
    fn the_detached_panel_title_starts_a_drag_at_every_grab() {
        let ctx = context();
        let title = egui::pos2(120.0, 14.0);
        pass(&ctx, vec![], |ui| {
            super::super::window_detach::detached_panel_contents(ui, "T", |_| {});
        });
        let mut total = 0;
        for _ in 0..2 {
            let a = pass(&ctx, vec![moved(title), button(title, true)], |ui| {
                super::super::window_detach::detached_panel_contents(ui, "T", |_| {});
            });
            let b = pass(&ctx, vec![], |ui| {
                super::super::window_detach::detached_panel_contents(ui, "T", |_| {});
            });
            total += start_drags(&a) + start_drags(&b);
        }
        assert_eq!(total, 2);
    }

    /// egui reports no `drag_started` for a release and a press of the same
    /// batch (the widget stays "dragged" across them), so what is checked is
    /// that the real press comes after the played release and stays down.
    #[test]
    fn a_press_in_the_batch_after_the_gesture_stays_pressed() {
        let ctx = context();
        let probe = Probe::default();
        pass(&ctx, vec![], |ui| probe.show(ui));
        pass(&ctx, vec![moved(P), button(P, true)], |ui| probe.show(ui));
        pass(&ctx, vec![button(P, true)], |ui| probe.show(ui));
        assert!(any_down(&ctx));
        assert_eq!(ctx.input(|input| input.pointer.press_origin()), Some(P));
    }

    #[test]
    fn a_pointer_that_left_the_window_stays_gone() {
        let ctx = context();
        let probe = Probe::default();
        pass(&ctx, vec![], |ui| probe.show(ui));
        pass(&ctx, vec![moved(P), button(P, true)], |ui| probe.show(ui));
        pass(&ctx, vec![egui::Event::PointerGone], |ui| probe.show(ui));
        assert_eq!(ctx.input(|input| input.pointer.latest_pos()), None);
        assert!(!any_down(&ctx));
        // Gone before the OS took the gesture: the release is followed by a
        // `PointerGone` of its own.
        pass(&ctx, vec![moved(P), button(P, true)], |ui| probe.show(ui));
        pass(&ctx, vec![egui::Event::PointerGone], |ui| {
            let _ = ui;
        });
        start_drag(&ctx);
        pass(&ctx, vec![], |ui| probe.show(ui));
        assert_eq!(ctx.input(|input| input.pointer.latest_pos()), None);
        assert!(!any_down(&ctx));
    }

    #[test]
    fn a_focused_text_field_keeps_its_focus_when_the_title_is_grabbed() {
        let ctx = context();
        let probe = Probe::default();
        pass(&ctx, vec![], |ui| probe.show(ui));
        let mut text = String::new();
        let field = egui::Id::new("field");
        let mut show = |ui: &mut egui::Ui, probe: &Probe| {
            probe.show(ui);
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 200.0), egui::vec2(200.0, 24.0));
            ui.put(rect, egui::TextEdit::singleline(&mut text).id(field));
        };
        pass(&ctx, vec![], |ui| {
            ui.memory_mut(|m| m.request_focus(field));
            show(ui, &probe);
        });
        pass(&ctx, vec![], |ui| show(ui, &probe));
        assert!(ctx.memory(|m| m.has_focus(field)));
        pass(&ctx, vec![moved(P), button(P, true)], |ui| show(ui, &probe));
        pass(&ctx, vec![], |ui| show(ui, &probe));
        assert!(ctx.memory(|m| m.has_focus(field)));
        assert_eq!(probe.clicks.get(), 0);
    }

    #[test]
    fn two_grabs_in_a_row_are_not_a_double_click() {
        let ctx = context();
        let probe = Probe::default();
        pass(&ctx, vec![], |ui| probe.show(ui));
        grab(&ctx, &probe);
        grab(&ctx, &probe);
        assert_eq!(probe.double_clicks.get(), 0);
        assert_eq!(probe.clicks.get(), 0);
    }

    #[test]
    fn a_real_release_after_the_gesture_is_not_a_click_and_the_next_press_drags() {
        for later in [false, true] {
            let ctx = context();
            let probe = Probe::default();
            pass(&ctx, vec![], |ui| probe.show(ui));
            pass(&ctx, vec![], |ui| probe.show(ui));
            pass(&ctx, vec![moved(P), button(P, true)], |ui| probe.show(ui));
            if later {
                pass(&ctx, vec![], |ui| probe.show(ui));
                pass(&ctx, vec![button(P, false)], |ui| probe.show(ui));
            } else {
                pass(&ctx, vec![button(P, false)], |ui| probe.show(ui));
            }
            assert_eq!(probe.clicks.get(), 0, "later = {later}");
            pass(&ctx, vec![button(P, true)], |ui| probe.show(ui));
            assert_eq!(probe.started.get(), 2, "later = {later}");
        }
    }

    #[test]
    fn the_release_goes_to_the_viewport_that_handed_the_gesture_over() {
        let ctx = context();
        let probe = Probe::default();
        let other = other_viewport();
        let q = egui::pos2(50.0, 150.0);
        let down = |viewport| ctx.input_for(viewport, |input| input.pointer.any_down());
        pass(&ctx, vec![], |ui| probe.show(ui));
        pass(&ctx, vec![moved(P), button(P, true)], |ui| probe.show(ui));
        // A pass of another viewport comes first: it must not take the release.
        pass_in(&ctx, other, vec![], |_| {});
        pass_in(&ctx, other, vec![moved(q), button(q, true)], |_| {});
        pass_in(&ctx, other, vec![], |_| {});
        assert!(down(other));
        assert!(down(egui::ViewportId::ROOT));
        pass(&ctx, vec![], |ui| probe.show(ui));
        assert!(!down(egui::ViewportId::ROOT));
    }

    #[test]
    fn a_widget_pressed_after_the_gesture_keeps_its_drag() {
        let ctx = context();
        let probe = Probe::default();
        pass(&ctx, vec![], |ui| probe.show(ui));
        grab(&ctx, &probe);
        let other = egui::pos2(50.0, 110.0);
        pass(&ctx, vec![moved(other), button(other, true)], |ui| {
            probe.show(ui)
        });
        for _ in 0..2 {
            pass(&ctx, vec![], |ui| probe.show(ui));
            assert!(probe.other_dragged.get());
        }
        assert!(any_down(&ctx));
    }
}

//! The 5-second band of a detached window (#471, work package #470).
//!
//! A detached window has no status bar. When a gesture made in it is refused
//! (a dialog is open in the main window) or ends in a warning or an error, the
//! window says so itself: a band at the bottom, orange for a warning (and for
//! a refusal), red for an error, for [`BAND_LIFETIME`]. Plain information
//! never shows here: it stays in the main status bar. The message also stays
//! in the status bar and the history of the main window.
//!
//! # The two published values
//!
//! Both live in the egui `Context`, the only channel shared by all the
//! viewports (like `graph_targets`), keyed on [`egui::ViewportId`] and nothing
//! else:
//!
//! - **the dialog signal**: the root publishes
//!   [`AppState::open_dialog`](stokesforge_app_state::state::AppState::open_dialog)
//!   at the start of its pass, after every dispatch and at its end. A viewport
//!   that is not the root reads it with [`main_dialog`]; it is `None` in the
//!   root by construction. The refusal itself is decided on the **state** at
//!   drain time, never on this copy, which only serves the viewports that
//!   must stop a gesture before it leaves them (keys, typed text).
//! - **the bands**: one per viewport, with a deadline. A band is an overlay
//!   `Area`, non-interactive (a click under it reaches the widget) and it
//!   moves no content, so the figure's rectangle, which is in its render
//!   cache key, does not change. One repaint is requested, at the deadline
//!   (#104: no periodic wake).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui;
use stokesforge_app_state::state::open_dialog::OpenDialog;

/// How long a band stays.
pub(super) const BAND_LIFETIME: Duration = Duration::from_secs(5);

/// The colour family of a band, the usual one of the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BandLevel {
    /// Orange: a refusal or a warning.
    Warning,
    /// Red: an error.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Band {
    text: String,
    level: BandLevel,
    until: Instant,
}

#[derive(Debug, Clone, Default)]
struct Bands(HashMap<egui::ViewportId, Band>);

fn bands_id() -> egui::Id {
    egui::Id::new("stokesforge_window_bands")
}

fn dialog_signal_id() -> egui::Id {
    egui::Id::new("stokesforge_open_dialog_signal")
}

/// Publishes the dialog open in the main window (`None`: no dialog). Called by
/// the root only.
pub(super) fn publish_dialog(ctx: &egui::Context, dialog: Option<OpenDialog>) {
    ctx.data_mut(|data| data.insert_temp(dialog_signal_id(), dialog));
}

/// The dialog open in the main window, seen from a viewport other than the
/// root. The root itself gets `None`: its own dialogs are no reason to refuse
/// its own gestures.
pub(super) fn main_dialog(ctx: &egui::Context) -> Option<OpenDialog> {
    if ctx.viewport_id() == egui::ViewportId::ROOT {
        return None;
    }
    ctx.data(|data| data.get_temp::<Option<OpenDialog>>(dialog_signal_id()))
        .flatten()
}

fn refusals_id() -> egui::Id {
    egui::Id::new("stokesforge_window_refusals")
}

/// A viewport other than the root says "this gesture was refused" without
/// being able to touch the main state: the root takes the note at its next
/// pass ([`take_refusals`]) and runs the same refusal as for an action.
pub(super) fn note_refusal(ctx: &egui::Context) {
    let viewport = ctx.viewport_id();
    if viewport == egui::ViewportId::ROOT {
        return;
    }
    ctx.data_mut(|data| {
        let list = data.get_temp_mut_or_default::<Vec<egui::ViewportId>>(refusals_id());
        if !list.contains(&viewport) {
            list.push(viewport);
        }
    });
    ctx.request_repaint_of(egui::ViewportId::ROOT);
}

/// The viewports that noted a refusal since the last call.
pub(super) fn take_refusals(ctx: &egui::Context) -> Vec<egui::ViewportId> {
    ctx.data_mut(|data| data.remove_temp::<Vec<egui::ViewportId>>(refusals_id()))
        .unwrap_or_default()
}

/// Is a typed value refused right now? In a detached window with a dialog open
/// in the main window: the entry stays open, the refusal is noted for the root
/// and this returns `true`. Never in the root.
pub(super) fn refuse_entry(ctx: &egui::Context) -> bool {
    if main_dialog(ctx).is_none() {
        return false;
    }
    note_refusal(ctx);
    true
}

/// The one stop of the keys of a detached window while a dialog is open in the
/// main window (#460). Called first in each detached closure, before any
/// reader of the keys (Page Layout Mode, grid, panels, shortcuts).
///
/// Key, text and clipboard events are removed from the viewport's input, so
/// that nothing below acts on them, and the refusal is noted once when a key
/// was really refused (the root then posts the band, as for a refused
/// action). Left alone: a viewport with a text field that has, or had in the
/// previous frame, the keyboard (typing and Esc keep their meaning; a typed
/// value is guarded at its commit, [`refuse_entry`]), an open popup, and a
/// modal of this same viewport. Never a band for: a key release, a repeat, an
/// Alt combination (Alt+Tab belongs to the window manager). Returns whether
/// the keys were stopped.
pub(super) fn stop_keys(ctx: &egui::Context) -> bool {
    if main_dialog(ctx).is_none()
        || ctx.text_edit_focused()
        || super::page_layout::field_had_focus(ctx)
        || egui::Popup::is_any_open(ctx)
        || super::dialogs::modal_shown(ctx)
    {
        return false;
    }
    let mut refused = false;
    ctx.input_mut(|input| {
        input.events.retain(|event| match event {
            egui::Event::Key {
                pressed,
                repeat,
                modifiers,
                ..
            } => {
                refused |= *pressed && !*repeat && !modifiers.alt;
                false
            }
            egui::Event::Text(_) | egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => {
                refused = true;
                false
            }
            _ => true,
        });
    });
    if refused {
        note_refusal(ctx);
    }
    true
}

/// Shows `text` in the band of `viewport` for [`BAND_LIFETIME`] from `now`,
/// replacing the band already there (the timer restarts), and wakes that
/// viewport so that it paints it. The root never has one.
pub(super) fn post_at(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
    text: String,
    level: BandLevel,
    now: Instant,
) {
    if viewport == egui::ViewportId::ROOT {
        return;
    }
    let band = Band {
        text,
        level,
        until: now + BAND_LIFETIME,
    };
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Bands>(bands_id())
            .0
            .insert(viewport, band);
    });
    ctx.request_repaint_of(viewport);
}

/// [`post_at`] from the clock.
pub(super) fn post(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
    text: String,
    level: BandLevel,
) {
    post_at(ctx, viewport, text, level, Instant::now());
}

/// Drops the band of a window that closed or went back to the main window.
pub(super) fn clear(ctx: &egui::Context, viewport: egui::ViewportId) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Bands>(bands_id())
            .0
            .remove(&viewport);
    });
}

/// The band of `viewport` still alive at `now`; an expired one is dropped.
fn live_band(ctx: &egui::Context, viewport: egui::ViewportId, now: Instant) -> Option<Band> {
    ctx.data_mut(|data| {
        let bands = &mut data.get_temp_mut_or_default::<Bands>(bands_id()).0;
        match bands.get(&viewport) {
            Some(band) if band.until > now => Some(band.clone()),
            Some(_) => {
                bands.remove(&viewport);
                None
            }
            None => None,
        }
    })
}

/// The text a viewport shows right now, for tests.
#[cfg(test)]
pub(super) fn text_at(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
    now: Instant,
) -> Option<(String, BandLevel)> {
    live_band(ctx, viewport, now).map(|band| (band.text, band.level))
}

/// Paints the band of the viewport being drawn, if it has one. To be called at
/// the end of the viewport's pass, so that it sits over the content.
pub(super) fn paint(ctx: &egui::Context) {
    paint_at(ctx, Instant::now());
}

/// [`paint`] at a given instant.
pub(super) fn paint_at(ctx: &egui::Context, now: Instant) {
    let viewport = ctx.viewport_id();
    let Some(band) = live_band(ctx, viewport, now) else {
        return;
    };
    // The single wake: when the band is due to go.
    ctx.request_repaint_after(band.until.saturating_duration_since(now));
    let screen = ctx.input(|input| input.content_rect());
    let dark = ctx.global_style().visuals.dark_mode;
    let colour = match band.level {
        BandLevel::Warning => crate::theme::warning_color(dark),
        BandLevel::Error => ctx.global_style().visuals.error_fg_color,
    };
    let margin = 12.0;
    egui::Area::new(egui::Id::new("stokesforge_window_band"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -margin))
        .show(ctx, |ui| {
            ui.set_max_width((screen.width() - 2.0 * margin).max(80.0));
            egui::Frame::new()
                .fill(ui.visuals().window_fill)
                .stroke(egui::Stroke::new(1.5, colour))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(colour, &band.text);
                    });
                });
        });
}

#[cfg(test)]
fn raw_for(viewport: egui::ViewportId) -> egui::RawInput {
    let mut input = egui::RawInput {
        viewport_id: viewport,
        ..Default::default()
    };
    input
        .viewports
        .insert(viewport, egui::ViewportInfo::default());
    input
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("band-test")
    }

    #[test]
    fn a_band_is_alive_for_five_seconds_then_gone() {
        let ctx = egui::Context::default();
        let start = Instant::now();
        post_at(&ctx, viewport(), "x".into(), BandLevel::Warning, start);
        let alive = text_at(&ctx, viewport(), start + Duration::from_millis(4900));
        assert_eq!(alive, Some(("x".to_string(), BandLevel::Warning)));
        assert_eq!(
            text_at(&ctx, viewport(), start + Duration::from_millis(5100)),
            None
        );
        // Dropped for good, not only hidden.
        assert_eq!(text_at(&ctx, viewport(), start), None);
    }

    #[test]
    fn a_new_message_replaces_the_band_and_restarts_the_timer() {
        let ctx = egui::Context::default();
        let start = Instant::now();
        post_at(&ctx, viewport(), "a".into(), BandLevel::Warning, start);
        let later = start + Duration::from_secs(4);
        post_at(&ctx, viewport(), "b".into(), BandLevel::Error, later);
        let check = start + Duration::from_secs(7);
        assert_eq!(
            text_at(&ctx, viewport(), check),
            Some(("b".to_string(), BandLevel::Error))
        );
    }

    #[test]
    fn the_root_never_has_a_band_and_each_window_has_its_own() {
        let ctx = egui::Context::default();
        let now = Instant::now();
        post_at(
            &ctx,
            egui::ViewportId::ROOT,
            "r".into(),
            BandLevel::Error,
            now,
        );
        assert_eq!(text_at(&ctx, egui::ViewportId::ROOT, now), None);
        post_at(&ctx, viewport(), "a".into(), BandLevel::Warning, now);
        let other = egui::ViewportId::from_hash_of("other");
        assert_eq!(text_at(&ctx, other, now), None);
        clear(&ctx, viewport());
        assert_eq!(text_at(&ctx, viewport(), now), None);
    }

    #[test]
    fn the_root_reads_no_signal_and_another_window_reads_the_published_one() {
        let ctx = egui::Context::default();
        publish_dialog(&ctx, Some(OpenDialog::About));
        assert_eq!(main_dialog(&ctx), None, "the root: by construction");
        let input = raw_for(viewport());
        ctx.begin_pass(input);
        assert_eq!(main_dialog(&ctx), Some(OpenDialog::About));
        publish_dialog(&ctx, None);
        assert_eq!(main_dialog(&ctx), None);
        ctx.end_pass().textures_delta.clear();
    }

    /// One pass of a viewport that draws the band.
    fn pass(ctx: &egui::Context, now: Instant) -> egui::FullOutput {
        let mut input = raw_for(viewport());
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        ));
        let mut output = ctx.run_ui(input, |ui| {
            paint_at(ui.ctx(), now);
        });
        output.textures_delta.clear();
        output
    }

    #[test]
    fn painting_asks_for_one_wake_at_the_deadline_and_none_after() {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let start = Instant::now();
        post_at(&ctx, viewport(), "x".into(), BandLevel::Warning, start);
        // The overlay lays itself out over its first passes.
        for _ in 0..3 {
            pass(&ctx, start + Duration::from_secs(1));
        }
        let first = pass(&ctx, start + Duration::from_secs(1));
        let delay = first.viewport_output[&viewport()].repaint_delay;
        assert!(
            delay > Duration::from_millis(3500) && delay <= Duration::from_secs(4),
            "one wake at the deadline, not each pass: {delay:?}"
        );
        let after = pass(&ctx, start + Duration::from_secs(6));
        assert!(
            after.viewport_output[&viewport()].repaint_delay > Duration::from_secs(3600),
            "nothing is asked once the band is gone"
        );
    }
}

#[cfg(test)]
mod refusal_tests {
    use super::raw_for;
    use super::*;

    #[test]
    fn a_refused_entry_is_noted_for_the_root_once_per_window() {
        let ctx = egui::Context::default();
        publish_dialog(&ctx, Some(OpenDialog::GraphProperties));
        let input = raw_for(egui::ViewportId::from_hash_of("w"));
        ctx.begin_pass(input);
        assert!(refuse_entry(&ctx));
        assert!(refuse_entry(&ctx));
        ctx.end_pass().textures_delta.clear();
        assert_eq!(
            take_refusals(&ctx),
            vec![egui::ViewportId::from_hash_of("w")]
        );
        assert!(take_refusals(&ctx).is_empty());
    }

    #[test]
    fn nothing_is_refused_without_a_dialog_or_in_the_root() {
        let ctx = egui::Context::default();
        assert!(!refuse_entry(&ctx), "root: never");
        publish_dialog(&ctx, Some(OpenDialog::About));
        assert!(!refuse_entry(&ctx), "root: never, even with a dialog");
        publish_dialog(&ctx, None);
        let input = raw_for(egui::ViewportId::from_hash_of("w"));
        ctx.begin_pass(input);
        assert!(!refuse_entry(&ctx));
        ctx.end_pass().textures_delta.clear();
    }
}

#[cfg(test)]
mod overlay_tests {
    use super::raw_for;
    use super::*;

    /// One pass with a button under where the band sits; returns the free
    /// rectangle before the band is painted and whether the button was clicked.
    fn pass(ctx: &egui::Context, now: Instant, events: Vec<egui::Event>) -> (egui::Rect, bool) {
        let mut input = raw_for(egui::ViewportId::from_hash_of("band-test"));
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        ));
        input.events = events;
        let mut free = egui::Rect::NOTHING;
        let mut clicked = false;
        let mut output = ctx.run_ui(input, |ui| {
            free = ui.available_rect_before_wrap();
            let button =
                egui::Rect::from_min_size(egui::pos2(160.0, 262.0), egui::vec2(80.0, 20.0));
            clicked = ui.put(button, egui::Button::new("under")).clicked();
            paint_at(ui.ctx(), now);
        });
        output.textures_delta.clear();
        (free, clicked)
    }

    /// The band is an overlay: the free rectangle (the figure's, in a detached
    /// graph) is the same with and without it, and a click under it reaches the
    /// widget. Witness: reserving a row, or making the area interactable.
    #[test]
    fn the_band_moves_nothing_and_lets_a_click_through() {
        let viewport = egui::ViewportId::from_hash_of("band-test");
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let start = Instant::now();
        let at = start + Duration::from_secs(1);
        let (without, _) = pass(&ctx, at, Vec::new());
        post_at(
            &ctx,
            viewport,
            "A fairly long sentence".into(),
            BandLevel::Warning,
            start,
        );
        for _ in 0..3 {
            pass(&ctx, at, Vec::new());
        }
        let (with, _) = pass(&ctx, at, Vec::new());
        assert_eq!(with, without, "the band moves no content");
        let target = egui::pos2(200.0, 272.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        pass(&ctx, at, vec![egui::Event::PointerMoved(target)]);
        let (_, down) = pass(&ctx, at, vec![press(true)]);
        let (_, up) = pass(&ctx, at, vec![press(false)]);
        assert!(down || up, "the click under the band reached the button");
    }

    fn outline(level: BandLevel) -> Vec<egui::Color32> {
        let viewport = egui::ViewportId::from_hash_of("band-test");
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let start = Instant::now();
        post_at(&ctx, viewport, "x".into(), level, start);
        let mut input = raw_for(viewport);
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        ));
        let mut output = ctx.run_ui(input.clone(), |ui| paint_at(ui.ctx(), start));
        for _ in 0..3 {
            output.textures_delta.clear();
            output = ctx.run_ui(input.clone(), |ui| paint_at(ui.ctx(), start));
        }
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.stroke.width > 1.0 => Some(rect.stroke.color),
                _ => None,
            })
            .collect()
    }

    /// Orange for a warning, red for an error, the status bar's colours.
    /// Witness: swapping the two arms.
    #[test]
    fn a_warning_is_orange_and_an_error_is_red() {
        let ctx = egui::Context::default();
        let dark = ctx.global_style().visuals.dark_mode;
        let red = ctx.global_style().visuals.error_fg_color;
        assert!(outline(BandLevel::Warning).contains(&crate::theme::warning_color(dark)));
        assert!(outline(BandLevel::Error).contains(&red));
    }
}

#[cfg(test)]
mod stop_keys_tests {
    use super::raw_for;
    use super::*;

    fn w() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("w")
    }

    fn key(key: egui::Key, pressed: bool, repeat: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers,
        }
    }

    /// One pass of window `w`: runs `stop_keys` first, then `body`; returns the
    /// events the rest of the pass still saw and whether the keys were stopped.
    fn pass(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        mut body: impl FnMut(&mut egui::Ui),
    ) -> (Vec<egui::Event>, bool) {
        let mut input = raw_for(w());
        input.events = events;
        let mut seen = Vec::new();
        let mut stopped = false;
        let mut output = ctx.run_ui(input, |ui| {
            stopped = stop_keys(ui.ctx());
            seen = ui.ctx().input(|input| input.events.clone());
            body(ui);
        });
        output.textures_delta.clear();
        (seen, stopped)
    }

    fn with_dialog() -> egui::Context {
        let ctx = egui::Context::default();
        publish_dialog(&ctx, Some(OpenDialog::About));
        ctx
    }

    /// Keys, text and clipboard are removed and the refusal is noted once.
    /// Witness: drop the `retain`, or the `note_refusal` call.
    #[test]
    fn a_dialog_in_the_main_window_removes_keys_text_and_clipboard() {
        let ctx = with_dialog();
        let pointer = egui::Event::PointerMoved(egui::pos2(3.0, 3.0));
        let (seen, stopped) = pass(
            &ctx,
            vec![
                key(egui::Key::Z, true, false, egui::Modifiers::COMMAND),
                egui::Event::Text("z".into()),
                egui::Event::Copy,
                egui::Event::Paste("x".into()),
                pointer.clone(),
            ],
            |_| {},
        );
        assert!(stopped);
        assert_eq!(seen, vec![pointer], "only the pointer is left");
        assert_eq!(take_refusals(&ctx), vec![w()]);
    }

    /// A key release, a repeat, a lone Alt combination (Alt+Tab) and a pass
    /// with no key never ask for the band; the events are still removed.
    /// Witnesses: drop the `pressed`, `repeat` or `alt` test.
    #[test]
    fn releases_repeats_and_alt_combinations_never_trigger_the_band() {
        let ctx = with_dialog();
        for event in [
            key(egui::Key::Escape, false, false, egui::Modifiers::NONE),
            key(egui::Key::Tab, true, false, egui::Modifiers::ALT),
        ] {
            let (seen, stopped) = pass(&ctx, vec![event.clone()], |_| {});
            assert!(stopped);
            assert!(seen.is_empty());
            assert!(take_refusals(&ctx).is_empty(), "{event:?}");
        }
        let (_, stopped) = pass(&ctx, Vec::new(), |_| {});
        assert!(stopped);
        assert!(take_refusals(&ctx).is_empty());
        // A key held down: egui marks the second press as a repeat itself.
        let held = key(egui::Key::A, true, false, egui::Modifiers::NONE);
        pass(&ctx, vec![held.clone()], |_| {});
        assert_eq!(take_refusals(&ctx), vec![w()], "the first press asks");
        pass(&ctx, vec![held], |_| {});
        assert!(take_refusals(&ctx).is_empty(), "the repeat does not");
    }

    /// Without a dialog, in the root, or with the dialog gone: nothing touched.
    /// Witness: clear the signal check.
    #[test]
    fn without_a_dialog_the_keys_are_left_alone() {
        let ctx = egui::Context::default();
        let event = key(egui::Key::A, true, false, egui::Modifiers::NONE);
        let (seen, stopped) = pass(&ctx, vec![event.clone()], |_| {});
        assert!(!stopped);
        assert_eq!(seen, vec![event.clone()]);
        publish_dialog(&ctx, Some(OpenDialog::About));
        publish_dialog(&ctx, None);
        assert!(!pass(&ctx, vec![event.clone()], |_| {}).1);
        // The root never stops its own keys.
        publish_dialog(&ctx, Some(OpenDialog::About));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(!stop_keys(ui.ctx()));
        });
        output.textures_delta.clear();
        assert!(take_refusals(&ctx).is_empty());
    }

    /// Letters typed in an open field keep working with no band, and the Esc
    /// that gives the field back is not taken from it.
    /// Witnesses: drop the `text_edit_focused` test; drop `field_had_focus`.
    #[test]
    fn a_focused_text_field_keeps_its_keys() {
        let ctx = with_dialog();
        let mut text = String::new();
        let id = egui::Id::new("field");
        let field = |ui: &mut egui::Ui, text: &mut String| {
            let response = ui.add(egui::TextEdit::singleline(text).id(id));
            response.request_focus();
            super::super::page_layout::note_field_focus(ui.ctx());
        };
        pass(&ctx, Vec::new(), |ui| field(ui, &mut text));
        let (_, stopped) = pass(&ctx, Vec::new(), |ui| field(ui, &mut text));
        assert!(!stopped, "the field has the keyboard");
        let typed = vec![
            key(egui::Key::A, true, false, egui::Modifiers::NONE),
            egui::Event::Text("a".into()),
        ];
        let (seen, stopped) = pass(&ctx, typed.clone(), |ui| field(ui, &mut text));
        assert!(!stopped);
        assert_eq!(seen, typed);
        assert_eq!(text, "a", "the letter reached the field");
        assert!(take_refusals(&ctx).is_empty(), "no band");
        // Esc: egui drops the focus first, the mark of the previous frame says
        // the field had it.
        let esc = key(egui::Key::Escape, true, false, egui::Modifiers::NONE);
        let (seen, stopped) = pass(&ctx, vec![esc.clone()], |_| {});
        assert!(!stopped);
        assert_eq!(seen, vec![esc]);
        assert!(take_refusals(&ctx).is_empty());
    }

    /// A field focused right now counts even before its end-of-frame mark.
    /// Witness: drop the `text_edit_focused` test.
    #[test]
    fn a_field_focused_this_frame_keeps_its_keys() {
        let ctx = with_dialog();
        let mut text = String::new();
        let field = |ui: &mut egui::Ui, text: &mut String| {
            ui.add(egui::TextEdit::singleline(text).id(egui::Id::new("f")))
                .request_focus();
        };
        pass(&ctx, Vec::new(), |ui| field(ui, &mut text));
        let (_, stopped) = pass(&ctx, Vec::new(), |ui| field(ui, &mut text));
        assert!(!stopped, "no mark was recorded, the field still has it");
    }

    /// An open popup owns the keys. Witness: drop the popup test.
    #[test]
    fn an_open_popup_keeps_its_keys() {
        let ctx = with_dialog();
        let id = egui::Id::new("popup");
        let open = |ui: &mut egui::Ui| {
            egui::Popup::open_id(ui.ctx(), id);
            let response = ui.label("anchor");
            egui::Popup::from_response(&response)
                .id(id)
                .open(true)
                .show(|ui| {
                    ui.label("inside");
                });
        };
        pass(&ctx, Vec::new(), open);
        let event = key(egui::Key::ArrowDown, true, false, egui::Modifiers::NONE);
        let (seen, stopped) = pass(&ctx, vec![event.clone()], open);
        assert!(!stopped, "popup open");
        assert_eq!(seen, vec![event]);
    }

    /// A modal of this same viewport (Object properties) owns the keys.
    /// Witness: drop the `modal_shown` test.
    #[test]
    fn a_modal_of_the_same_window_keeps_its_keys() {
        let ctx = with_dialog();
        let event = key(egui::Key::Escape, true, false, egui::Modifiers::NONE);
        let mut output = ctx.run_ui(raw_for(w()), |ui| {
            super::super::dialogs::mark_modal_shown(ui.ctx());
        });
        output.textures_delta.clear();
        let (seen, stopped) = pass(&ctx, vec![event.clone()], |_| {});
        assert!(!stopped);
        assert_eq!(seen, vec![event]);
    }
}

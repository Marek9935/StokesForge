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
#[allow(dead_code)] // read by the typed-text guard and by the key stop of #460
pub(super) fn main_dialog(ctx: &egui::Context) -> Option<OpenDialog> {
    if ctx.viewport_id() == egui::ViewportId::ROOT {
        return None;
    }
    ctx.data(|data| data.get_temp::<Option<OpenDialog>>(dialog_signal_id()))
        .flatten()
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
        post_at(&ctx, egui::ViewportId::ROOT, "r".into(), BandLevel::Error, now);
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
        let mut input = egui::RawInput::default();
        input.viewport_id = viewport();
        ctx.begin_pass(input);
        assert_eq!(main_dialog(&ctx), Some(OpenDialog::About));
        publish_dialog(&ctx, None);
        assert_eq!(main_dialog(&ctx), None);
        let _ = ctx.end_pass();
    }

    /// One pass of a viewport that draws the band.
    fn pass(ctx: &egui::Context, now: Instant) -> egui::FullOutput {
        let mut input = egui::RawInput::default();
        input.viewport_id = viewport();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        ));
        ctx.run_ui(input, |ui| {
            paint_at(ui.ctx(), now);
        })
    }

    #[test]
    fn painting_asks_for_one_wake_at_the_deadline_and_none_after() {
        let ctx = egui::Context::default();
        let start = Instant::now();
        post_at(&ctx, viewport(), "x".into(), BandLevel::Warning, start);
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

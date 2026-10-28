//! State of an input field: **refused** (red frame) or **brought back**
//! (orange frame), a single building block for all fields (#219, #217
//! S-N6/S-N7/S-N11, S-T2, S-T3).
//!
//! Rules (`docs/interface-standard.md`, § Input refused or brought back):
//!
//! - an **unreadable** input, or one that violates a constraint between two
//!   fields, is *refused*: red frame, a bubble that states the rule, text
//!   kept as long as the field stays open, old value restored on exit;
//! - a value **outside a fixed bound** is *brought back* to the bound:
//!   orange frame for [`CLAMPED_SECONDS`], bubble "Set to the maximum, 20";
//! - never an erasure or a silent correction.
//!
//! # Three levels, from lowest to highest
//!
//! 1. [`show_issue`] — paints the frame and bubble of a [`FieldIssue`] on the
//!    response of a field that is **already placed**. No memory: this is the
//!    level of a field whose error comes from elsewhere (Typst message of a
//!    title, S-T3).
//! 2. [`flag`] / [`active`] / [`clear`] — egui's memory (per field
//!    identifier) of a state that must outlive the input: the 2 s orange
//!    frame, the red frame of an abandoned input.
//! 3. [`edit_text`] — the full cycle of a validated text field (draft,
//!    Enter, focus loss, Escape) around a reading function
//!    ([`Verdict`]). The "number" (#220) and "text" (#224) building blocks
//!    are calls of [`edit_text`] with their reading; the decision of the
//!    cycle, [`decide`], is a pure function tested without a window.
//!
//! Red and orange come from `theme.rs` (`field_refused_color`,
//! `field_clamped_color`); the messages are `tr!` keys of the catalog.

use eframe::egui;

use crate::theme;
use stokesforge_app_state::tr;

/// How long the orange frame of a brought-back value shows, in seconds.
pub const CLAMPED_SECONDS: f64 = 2.0;

/// Thickness of the frame of a faulty field, in points.
const FRAME_WIDTH: f32 = 1.5;

/// Which bound brought the value back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    Min,
    Max,
}

/// What is wrong in a field, with the already translated sentence of the bubble.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldIssue {
    /// Input refused: red frame. `message` states the rule.
    Refused { message: String },
    /// Value brought back to a bound: orange frame. `message` says which.
    Clamped { message: String },
}

impl FieldIssue {
    /// A refused input; `message` is a `tr!` sentence that states the rule.
    pub fn refused(message: impl Into<String>) -> Self {
        Self::Refused {
            message: message.into(),
        }
    }

    /// A value brought back to `bound`, whose displayed value is `limit`
    /// ("20" in "Set to the maximum, 20").
    pub fn clamped(bound: Bound, limit: &str) -> Self {
        let message = match bound {
            Bound::Min => tr!("field-clamped-min", limit = limit),
            Bound::Max => tr!("field-clamped-max", limit = limit),
        };
        Self::Clamped { message }
    }

    /// The sentence of the bubble.
    pub fn message(&self) -> &str {
        match self {
            Self::Refused { message } | Self::Clamped { message } => message,
        }
    }

    /// The color of the frame: red for a refusal, orange for a brought-back value.
    pub fn color(&self) -> egui::Color32 {
        match self {
            Self::Refused { .. } => theme::field_refused_color(),
            Self::Clamped { .. } => theme::field_clamped_color(),
        }
    }
}

/// Paints the frame of `issue` around field `response`, and its bubble while the
/// field is hovered or has focus.
pub fn show_issue(ui: &egui::Ui, response: &egui::Response, issue: &FieldIssue) {
    let corner = ui.visuals().widgets.inactive.corner_radius;
    ui.painter().rect_stroke(
        response.rect.expand(1.0),
        corner,
        egui::Stroke::new(FRAME_WIDTH, issue.color()),
        egui::StrokeKind::Outside,
    );
    if response.hovered() || response.has_focus() {
        response.show_tooltip_text(issue.message());
    }
}

/// The state kept in memory for a field: what is still to show, and
/// until when (egui time).
#[derive(Clone, Debug)]
struct Flag {
    issue: FieldIssue,
    until: f64,
}

/// Remembers `issue` for field `id` for [`CLAMPED_SECONDS`] starting from
/// `now`, and wakes the interface at its deadline.
pub fn flag(ctx: &egui::Context, id: egui::Id, issue: FieldIssue, now: f64) {
    ctx.data_mut(|data| {
        data.insert_temp(
            id,
            Flag {
                issue,
                until: now + CLAMPED_SECONDS,
            },
        );
    });
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(CLAMPED_SECONDS));
}

/// The state still to show for field `id` at time `now`. An expired state
/// is forgotten; otherwise the interface is woken at its deadline, without which the
/// frame would stay painted until the next event.
pub fn active(ctx: &egui::Context, id: egui::Id, now: f64) -> Option<FieldIssue> {
    let flag = ctx.data(|data| data.get_temp::<Flag>(id))?;
    if now >= flag.until {
        clear(ctx, id);
        return None;
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(flag.until - now));
    Some(flag.issue)
}

/// Forgets the remembered state of field `id`.
pub fn clear(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|data| data.remove::<Flag>(id));
}

/// What the reading of an entered text decided.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict<T> {
    /// Readable and within bounds.
    Valid(T),
    /// Readable but outside a fixed bound: `value` is the brought-back value.
    Clamped {
        value: T,
        bound: Bound,
        /// The bound as it is displayed in the bubble.
        limit: String,
    },
    /// Unreadable or against a constraint: `message` states the rule.
    Refused(String),
}

/// What a field's cycle does with a frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Step<T> {
    /// Nothing to validate: the input continues, or the text did not change.
    Keep,
    /// Escape, or focus loss with no change: the old value comes back.
    Revert,
    /// Validated value.
    Commit(T),
    /// Value validated after being brought back: orange frame.
    CommitClamped(T, FieldIssue),
    /// Input refused. On Enter the field stays open with its text; on
    /// focus loss the old value comes back, the refusal staying flagged.
    Reject { issue: FieldIssue, stay_open: bool },
}

/// The cycle of a field, without egui: `draft` is the entered text, `current` the
/// value displayed before the input, `lost_focus` leaving the field (Enter
/// included, `enter` tells it from a click elsewhere), `escape` Escape.
pub fn decide<T>(
    draft: &str,
    current: &str,
    enter: bool,
    escape: bool,
    lost_focus: bool,
    parse: impl FnOnce(&str) -> Verdict<T>,
) -> Step<T> {
    if escape {
        return Step::Revert;
    }
    if !lost_focus {
        return Step::Keep;
    }
    if draft == current {
        return Step::Revert;
    }
    match parse(draft) {
        Verdict::Valid(value) => Step::Commit(value),
        Verdict::Clamped {
            value,
            bound,
            limit,
        } => Step::CommitClamped(value, FieldIssue::clamped(bound, &limit)),
        Verdict::Refused(message) => Step::Reject {
            issue: FieldIssue::refused(message),
            stay_open: enter,
        },
    }
}

/// The draft of a field during input.
#[derive(Clone, Debug)]
struct Draft(String);

/// The identifier of a field placed by [`edit_text`] under `id_salt`: that of
/// its `TextEdit` and of its remembered state.
pub fn field_id(ui: &egui::Ui, id_salt: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    ui.make_persistent_id(id_salt)
}

/// Abandons the draft of field `id` and the state it showed: the displayed
/// value starts again from the caller's. For a change that does not come
/// from typing (arrows, wheel, choice in a list).
pub fn discard_draft(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|data| data.remove::<Draft>(id));
    clear(ctx, id);
    ctx.request_repaint();
}

/// What [`edit_text`] returns: the value validated this frame, if there is one,
/// and the field's response (focus, hover).
pub struct Edited<T> {
    pub value: Option<T>,
    pub response: egui::Response,
}

/// A one-line text field following the standard's cycle: `current` is the value
/// displayed outside input, `parse` reads it. Returns the value read when it is
/// validated (Enter or leaving the field), `None` otherwise.
///
/// - the input is a draft kept in egui's memory under `id_salt`; nothing
///   leaves it before validation;
/// - an unreadable draft paints the red frame from the first keystroke, with the rule
///   in a bubble;
/// - Escape returns the old value; so does leaving without a change.
/// - `width` is the width of the field, in points.
pub fn edit_text<T>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    current: &str,
    parse: impl Fn(&str) -> Verdict<T>,
) -> Edited<T> {
    edit_text_hinted(ui, id_salt, width, current, "", parse)
}

/// [`edit_text`] with greyed-out text when the field is empty: a field that has an
/// Auto shows "Auto (0–4000)" in it (#221).
pub fn edit_text_hinted<T>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    current: &str,
    hint: &str,
    parse: impl Fn(&str) -> Verdict<T>,
) -> Edited<T> {
    edit_text_with_arrow(ui, id_salt, width, current, hint, 0.0, parse)
}

/// [`edit_text_hinted`] with `arrow_room` free points to the right of the text, for
/// the arrow of a list field painted in the frame (#234).
pub fn edit_text_with_arrow<T>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    current: &str,
    hint: &str,
    arrow_room: f32,
    parse: impl Fn(&str) -> Verdict<T>,
) -> Edited<T> {
    let id = field_id(ui, id_salt);
    let ctx = ui.ctx().clone();
    let now = ui.input(|input| input.time);

    let draft = ctx.data(|data| data.get_temp::<Draft>(id));
    // Leaving the field counts only for an open input: egui may
    // announce the focus loss on two frames in a row, and the second must
    // neither revalidate nor erase the state the first one set.
    let editing = draft.is_some();
    let mut buffer = draft.map_or_else(|| current.to_owned(), |draft| draft.0);
    let response = ui.add(
        egui::TextEdit::singleline(&mut buffer)
            .id(id)
            .hint_text(hint)
            .margin(egui::Margin {
                right: 4 + arrow_room as i8,
                ..egui::Margin::symmetric(4, 2)
            })
            .desired_width(width),
    );
    let focused = response.has_focus();

    // egui itself gives focus back on Escape: the field "had" focus also
    // when it has just lost it.
    let escape = editing
        && (focused || response.lost_focus())
        && ui.input_mut(|i| i.consume_shortcut(&crate::app::shortcuts::KEY_ESCAPE));
    let left = response.lost_focus() && editing;
    // Enter is consumed: a dialog that validates on Enter must not close
    // with the input (it looks at the focus, which the field has just lost).
    let enter = left && ui.input_mut(|i| i.consume_shortcut(&crate::app::shortcuts::KEY_ENTER));
    let step = decide(&buffer, current, enter, escape, left || escape, &parse);

    // Typing was already painted with the draft: a step that returns it is
    // visible only on the next frame, which must therefore be requested.
    if !matches!(step, Step::Keep) {
        ctx.request_repaint();
    }
    let mut committed = None;
    match step {
        Step::Keep => {
            if focused || response.changed() {
                ctx.data_mut(|data| data.insert_temp(id, Draft(buffer.clone())));
            }
        }
        Step::Revert => {
            ctx.data_mut(|data| data.remove::<Draft>(id));
            clear(&ctx, id);
            if escape {
                response.surrender_focus();
            }
        }
        Step::Commit(value) => {
            ctx.data_mut(|data| data.remove::<Draft>(id));
            clear(&ctx, id);
            committed = Some(value);
        }
        Step::CommitClamped(value, issue) => {
            ctx.data_mut(|data| data.remove::<Draft>(id));
            flag(&ctx, id, issue, now);
            committed = Some(value);
        }
        Step::Reject { issue, stay_open } => {
            if stay_open {
                ctx.data_mut(|data| data.insert_temp(id, Draft(buffer.clone())));
                response.request_focus();
            } else {
                ctx.data_mut(|data| data.remove::<Draft>(id));
                flag(&ctx, id, issue, now);
            }
        }
    }

    // What shows this frame: an unreadable draft while the field
    // is open (the rule is learned while typing), otherwise the remembered state.
    let live = (focused && buffer != current)
        .then(|| match parse(&buffer) {
            Verdict::Refused(message) => Some(FieldIssue::refused(message)),
            _ => None,
        })
        .flatten();
    if let Some(issue) = live.or_else(|| active(&ctx, id, now)) {
        show_issue(ui, &response, &issue);
    }
    Edited {
        value: committed,
        response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context with the application theme (the interface fonts change
    /// nothing here: we read colors and texts, not positions).
    fn context() -> egui::Context {
        let context = egui::Context::default();
        theme::install(&context);
        context
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Has a text containing `needle` been painted?
    fn painted(output: &egui::FullOutput, needle: &str) -> bool {
        output.shapes.iter().any(|painted_shape| {
            matches!(&painted_shape.shape,
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
        })
    }

    fn read(text: &str) -> Verdict<f64> {
        match text.trim().parse::<f64>() {
            Ok(v) if v > 20.0 => Verdict::Clamped {
                value: 20.0,
                bound: Bound::Max,
                limit: "20".into(),
            },
            Ok(v) => Verdict::Valid(v),
            Err(_) => Verdict::Refused("Enter a number".into()),
        }
    }

    #[test]
    fn an_input_in_progress_is_not_read() {
        let step = decide("abc", "1", false, false, false, read);
        assert_eq!(step, Step::Keep);
    }

    #[test]
    fn escape_restores_the_old_value_even_for_an_unreadable_text() {
        assert_eq!(decide("abc", "1", false, true, false, read), Step::Revert);
    }

    #[test]
    fn leaving_without_a_change_validates_nothing() {
        assert_eq!(decide("1", "1", false, false, true, read), Step::Revert);
    }

    #[test]
    fn a_readable_number_is_valid() {
        assert_eq!(decide("5", "1", true, false, true, read), Step::Commit(5.0));
    }

    #[test]
    fn outside_the_bound_the_value_is_brought_back_with_its_bubble() {
        let Step::CommitClamped(value, issue) = decide("99", "1", true, false, true, read) else {
            panic!("a value outside the bound is brought back");
        };
        assert_eq!(value, 20.0);
        assert_eq!(issue, FieldIssue::clamped(Bound::Max, "20"));
        assert!(issue.message().contains("20"), "{}", issue.message());
    }

    /// Enter on an unreadable text keeps the field open; a click elsewhere
    /// restores the old value. In both cases the refusal is flagged.
    #[test]
    fn an_unreadable_text_is_refused_open_on_enter_and_closed_on_exit() {
        let expected = FieldIssue::refused("Enter a number");
        assert_eq!(
            decide("abc", "1", true, false, true, read),
            Step::Reject {
                issue: expected.clone(),
                stay_open: true
            }
        );
        assert_eq!(
            decide("abc", "1", false, false, true, read),
            Step::Reject {
                issue: expected,
                stay_open: false
            }
        );
    }

    #[test]
    fn red_and_orange_are_distinct() {
        assert_ne!(
            FieldIssue::refused("x").color(),
            FieldIssue::clamped(Bound::Min, "0").color()
        );
    }

    #[test]
    fn a_remembered_state_expires_after_two_seconds() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("field");
        flag(&ctx, id, FieldIssue::clamped(Bound::Max, "20"), 10.0);
        assert!(active(&ctx, id, 11.9).is_some());
        assert!(active(&ctx, id, 12.0).is_none());
        assert!(active(&ctx, id, 12.1).is_none(), "forgetting is final");
    }

    // — The painted shapes: the frame and the bubble are read in the shapes. —

    /// The number of frames of color `color` painted in `shapes`.
    fn frames(output: &egui::FullOutput, color: egui::Color32) -> usize {
        output
            .shapes
            .iter()
            .filter(|painted_shape| match &painted_shape.shape {
                egui::Shape::Rect(rect) => rect.stroke.color == color,
                _ => false,
            })
            .count()
    }

    /// A frame at time `time` where `events` arrive; the field is the
    /// only content, displayed value "1".
    fn image(
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
        out: &mut Option<f64>,
    ) -> egui::FullOutput {
        frame_with(ctx, time, events, out, None)
    }

    /// Like [`image`], and `focus` gives (`Some(true)`) or takes away (`Some(false)`)
    /// the field's focus before it is placed.
    fn frame_with(
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
        out: &mut Option<f64>,
        focus: Option<bool>,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 200.0),
            )),
            events,
            time: Some(time),
            ..Default::default()
        };
        input.focused = true;
        let mut output = ctx.run_ui(input, |ui| {
            let id = ui.make_persistent_id("field");
            match focus {
                Some(true) => ui.memory_mut(|m| m.request_focus(id)),
                Some(false) => ui.memory_mut(|m| m.surrender_focus(id)),
                None => {}
            }
            if let Some(value) = edit_text(ui, "field", 80.0, "1", read).value {
                *out = Some(value);
            }
        });
        output.textures_delta.clear();
        output
    }

    fn type_text(text: &str) -> egui::Event {
        egui::Event::Text(text.into())
    }

    /// Gives the field focus by requesting it in a frame, then types.
    fn open_and_type(ctx: &egui::Context, text: &str, out: &mut Option<f64>) {
        frame_with(ctx, 0.0, vec![], out, Some(true));
        image(ctx, 0.1, vec![], out);
        image(ctx, 0.2, vec![type_text(text)], out);
    }

    #[test]
    fn an_unreadable_draft_paints_the_red_frame_and_the_rule() {
        let ctx = context();
        let mut out = None;
        open_and_type(&ctx, "abc", &mut out);
        let frame_out = image(&ctx, 0.3, vec![], &mut out);
        assert_eq!(frames(&frame_out, theme::field_refused_color()), 1);
        assert!(painted(&frame_out, "Enter a number"));
        assert_eq!(out, None, "nothing comes out before validation");
    }

    /// Witness: a valid input paints no fault frame.
    #[test]
    fn a_valid_input_paints_no_frame() {
        let ctx = context();
        let mut out = None;
        open_and_type(&ctx, "5", &mut out);
        let frame_out = image(&ctx, 0.3, vec![], &mut out);
        assert_eq!(frames(&frame_out, theme::field_refused_color()), 0);
        assert_eq!(frames(&frame_out, theme::field_clamped_color()), 0);
    }

    #[test]
    fn enter_validates_a_brought_back_number_and_the_orange_frame_expires() {
        let ctx = context();
        let mut out = None;
        open_and_type(&ctx, "9", &mut out); // "19": valid
        image(&ctx, 0.3, vec![type_text("9")], &mut out); // "199": outside the bound
        image(&ctx, 0.4, vec![key(egui::Key::Enter)], &mut out);
        assert_eq!(out, Some(20.0), "brought back to the bound");

        let before = image(&ctx, 1.0, vec![], &mut out);
        assert_eq!(frames(&before, theme::field_clamped_color()), 1);
        let after = image(&ctx, 3.0, vec![], &mut out);
        assert_eq!(frames(&after, theme::field_clamped_color()), 0);
    }

    #[test]
    fn leaving_an_unreadable_text_restores_the_old_value() {
        let ctx = context();
        let mut out = None;
        open_and_type(&ctx, "abc", &mut out);
        // The focus goes elsewhere without Enter.
        frame_with(&ctx, 0.4, vec![], &mut out, Some(false));
        let after = image(&ctx, 0.5, vec![], &mut out);
        assert_eq!(out, None);
        assert!(!painted(&after, "abc"), "the draft is given back");
        assert_eq!(
            frames(&after, theme::field_refused_color()),
            1,
            "the refusal stays flagged for a moment"
        );
        let later = image(&ctx, 3.0, vec![], &mut out);
        assert_eq!(frames(&later, theme::field_refused_color()), 0);
    }

    /// Enter on an unreadable text: the field stays open, the text kept,
    /// the red frame painted (S-T2); nothing comes out.
    #[test]
    fn enter_on_an_unreadable_text_keeps_the_field_open() {
        let ctx = context();
        let mut out = None;
        open_and_type(&ctx, "abc", &mut out);
        image(&ctx, 0.3, vec![key(egui::Key::Enter)], &mut out);
        let after = image(&ctx, 0.4, vec![], &mut out);
        let still = image(&ctx, 0.5, vec![], &mut out);
        assert_eq!(out, None);
        assert!(painted(&after, "abc") && painted(&still, "abc"));
        assert_eq!(frames(&still, theme::field_refused_color()), 1);
    }

    #[test]
    fn escape_restores_the_old_value_without_a_frame() {
        let ctx = context();
        let mut out = None;
        open_and_type(&ctx, "abc", &mut out);
        image(&ctx, 0.3, vec![key(egui::Key::Escape)], &mut out);
        let after = image(&ctx, 0.4, vec![], &mut out);
        assert!(!painted(&after, "abc"));
        assert_eq!(frames(&after, theme::field_refused_color()), 0);
    }
}

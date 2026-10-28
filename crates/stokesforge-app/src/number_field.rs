//! The "number" building block (#220, S-N1–S-N3, S-N8, S-F7): **the only**
//! numeric field of the interface. No more `DragValue` or `Slider` (guard
//! `no_numeric_field_outside_the_building_block`, `tests/guards.rs`).
//!
//! # What a field does
//!
//! - a text field, as wide as [`FIELD_WIDTH`]; a `▼` only if the [`Quantity`]
//!   offers values ([`Quantity::presets`]);
//! - a click in the text selects everything, without opening the list;
//! - the entry applies on Enter or on leaving the field, Escape gives back the
//!   old value; an unreadable entry is **refused**, an out-of-bounds value
//!   **clamped** (red or orange frame, bubble): this whole cycle is that of
//!   [`crate::field_state::edit_text`];
//! - closed list: ↑/↓ by a step, Shift+↑/↓ by a big step; Alt+↓ opens the list;
//! - open list: the wheel scrolls it, ↑/↓ highlights, Enter picks, Escape
//!   closes it (without touching the value);
//! - wheel (#233, #235): **hovering** is enough, without focus, and **only** on
//!   a field with a list: one notch = the neighboring preset (`step_preset`),
//!   an off-list value jumps to the neighboring entry, at the ends it stays.
//!   A field without a list never reacts: the window scrolls;
//! - look (#234): a single frame, a filled arrow inside on the right, the
//!   theme's input background; see [`crate::list_frame`].
//!
//! The list is an `egui::Area` of its own, not `egui::Popup`: egui keeps only
//! one popup open per viewport, so in a modal dialog the list would close the
//! parent window (issue #9, like `color_button`).
//!
//! # Auto (#221)
//!
//! A quantity has an Auto ([`AutoKind`]): `Field` — an emptied field or the
//! word `auto` (any case, or the single letter `a`/`A`) gives back the
//! automatic value ([`auto_number_field`], grayed text "Auto (…)"); `Group` —
//! the word `auto` ticks the group's Auto ([`NumberResponse::auto`]), an
//! emptied field is refused. Without Auto, the word and the emptied field are
//! refused.
//!
//! # History
//!
//! Each arrow or wheel step is a change returned to the caller
//! ([`NumberResponse::changed`]). Notches in a row make only **one** history
//! entry because the caller writes them into its draft or into the macro of
//! its dialog (graph properties): the building block does not touch the model.
//!
//! The formatter of the displayed numbers is provisional (#222): decimals of
//! the quantity, trailing zeros removed. The decimal comma arrives with #223.

use eframe::egui;
use stokesforge_core::number_format::{format_exact, format_grid};

use crate::app::shortcuts;
use crate::field_state::{self, Bound, Verdict};
use crate::list_frame;
use stokesforge_app_state::quantities::{AutoKind, Quantity};
use stokesforge_app_state::tr;

/// Width of a numeric field, in points (the "short" one of S-L2, #227).
pub const FIELD_WIDTH: f32 = crate::theme::FIELD_WIDTH_SHORT;

/// A value the building block can read and write. Integers round.
pub trait Number: Copy {
    fn to_f64(self) -> f64;
    fn from_f64(value: f64) -> Self;
}

impl Number for f64 {
    fn to_f64(self) -> f64 {
        self
    }
    fn from_f64(value: f64) -> Self {
        value
    }
}

impl Number for f32 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_f64(value: f64) -> Self {
        value as f32
    }
}

macro_rules! integer_number {
    ($($type:ty),*) => {$(
        impl Number for $type {
            fn to_f64(self) -> f64 {
                self as f64
            }
            fn from_f64(value: f64) -> Self {
                value.round() as $type
            }
        }
    )*};
}
integer_number!(i32, i64, u8, u32, usize);

/// What a numeric field returns.
pub struct NumberResponse {
    /// The input field (hover, help).
    pub response: egui::Response,
    /// The value changed this frame: entry confirmed, arrow step, wheel notch
    /// or choice in the list.
    pub changed: bool,
    /// The user asked for Auto this frame (emptied field or the word `auto`,
    /// depending on [`AutoKind`]): the caller gives back the automatic value or group.
    pub auto: bool,
}

/// What an entry says: a number, or Auto (#221).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    Value(f64),
    Auto,
}

/// The state of a field's list of values.
#[derive(Clone, Copy, Debug, Default)]
struct Presets {
    open: bool,
    highlighted: usize,
}

/// Places a numeric field on the quantity `quantity`, in the current row.
/// `id_salt` identifies the field (draft, list); the value is written into
/// `value` as soon as it changes.
pub fn number_field<T: Number>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    value: &mut T,
    quantity: &Quantity,
) -> NumberResponse {
    number_field_core(ui, id_salt, value, quantity, None)
}

/// A field whose value can be automatic (#221): `None` shows `hint` in gray
/// ("Auto (0–4000)", the computed value); emptying the field or typing `auto`
/// sets it back to `None` (quantity [`AutoKind::Field`]). A readable entry
/// sets it to `Some`.
pub fn auto_number_field<T: Number>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    value: &mut Option<T>,
    quantity: &Quantity,
    hint: &str,
) -> NumberResponse {
    let mut scratch = value.map_or(0.0, |v| v.to_f64());
    let shown_auto = value.is_none().then_some(hint);
    let mut response = number_field_core(ui, id_salt, &mut scratch, quantity, shown_auto);
    if response.auto {
        *value = None;
        response.changed = true;
    } else if response.changed {
        *value = Some(T::from_f64(scratch));
    }
    response
}

fn number_field_core<T: Number>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    value: &mut T,
    quantity: &Quantity,
    auto_hint: Option<&str>,
) -> NumberResponse {
    let ctx = ui.ctx().clone();
    let id = field_state::field_id(ui, id_salt);
    let list_id = id.with("presets");
    let mut presets = ctx
        .data(|data| data.get_temp::<Presets>(list_id))
        .unwrap_or_default();
    let has_presets = !quantity.presets.is_empty();
    let mut current = value.to_f64();
    let mut changed = false;
    let mut auto = false;
    // An Auto of the value itself (`AutoKind::Field`) opens the list with an
    // "Auto" entry; the presets are then shifted by one notch.
    let offset = usize::from(quantity.auto == AutoKind::Field);
    let focused = ctx.memory(|memory| memory.has_focus(id));

    // — Keyboard, before the field: the `TextEdit` would take ↑/↓ and Enter. —
    // Escape closes the list without giving the value back (S-F7). egui itself
    // removes the focus at the start of the frame where Escape is pressed: the
    // open list, not the focus, says that the Escape is for it, and the field
    // takes the focus back.
    let close_list = presets.open && key(ui, &shortcuts::KEY_ESCAPE);
    if close_list {
        presets.open = false;
    }
    if focused && !close_list {
        // Alt+↓ is always taken: a step must not read it (egui matches "no
        // modifier" with any combination).
        let alt_down = key(ui, &shortcuts::KEY_ALT_ARROW_DOWN);
        if alt_down && has_presets && !presets.open {
            presets.open = true;
            presets.highlighted = nearest_preset(quantity, current) + offset;
        } else if presets.open {
            if key(ui, &shortcuts::KEY_ARROW_DOWN) {
                presets.highlighted =
                    (presets.highlighted + 1).min(quantity.presets.len() - 1 + offset);
            }
            if key(ui, &shortcuts::KEY_ARROW_UP) {
                presets.highlighted = presets.highlighted.saturating_sub(1);
            }
            if key(ui, &shortcuts::KEY_ENTER) {
                if presets.highlighted < offset {
                    auto = true;
                } else {
                    current = quantity.presets[presets.highlighted - offset];
                    changed = true;
                }
                presets.open = false;
            }
        } else {
            let steps = i32::from(key(ui, &shortcuts::KEY_SHIFT_ARROW_UP)) * 10
                - i32::from(key(ui, &shortcuts::KEY_SHIFT_ARROW_DOWN)) * 10
                + i32::from(key(ui, &shortcuts::KEY_ARROW_UP))
                - i32::from(key(ui, &shortcuts::KEY_ARROW_DOWN));
            if steps != 0 {
                current = step_value(quantity, current, steps);
                changed = true;
            }
        }
    }

    // — The field. —
    let shown = match auto_hint {
        Some(_) => String::new(),
        // Outside editing: 6 significant digits; in the field: the whole
        // value, so that a Tab without typing loses none of it (#222).
        None if focused => format_exact(current),
        None => format_grid(current),
    };
    let arrow_room = if has_presets {
        list_frame::ARROW_ROOM
    } else {
        0.0
    };
    let edited = field_state::edit_text_with_arrow(
        ui,
        id_salt,
        if quantity.is_compact() {
            crate::theme::FIELD_WIDTH_DIGITS
        } else if quantity.is_compact_signed() {
            crate::theme::FIELD_WIDTH_SIGNED_DIGITS
        } else {
            FIELD_WIDTH
        },
        &shown,
        auto_hint.unwrap_or(""),
        arrow_room,
        |text| read(quantity, text),
    );
    let response = edited.response;
    if close_list {
        response.request_focus();
    }
    let typed = edited.value.is_some();
    match edited.value {
        Some(Entry::Value(value)) => {
            current = value;
            changed = true;
        }
        Some(Entry::Auto) => auto = true,
        None => {}
    }
    if response.gained_focus() {
        select_all(&ctx, id, shown.chars().count());
    }

    // — Wheel (#233, #235): hovering is enough, without focus; one notch = the
    // neighboring preset. A field without a list never reacts: the window
    // scrolls. —
    if has_presets && response.hovered() && !presets.open {
        let notches = wheel_notches(ui);
        if notches != 0 {
            let moved = step_preset(quantity, current, notches);
            if moved != current {
                current = moved;
                changed = true;
            }
        }
    }

    // — The list of values. —
    if has_presets {
        // The filled arrow is in the field's frame (#234), clickable.
        let arrow = list_frame::arrow_rect(response.rect);
        let button = ui
            .interact(arrow, list_id.with("arrow"), egui::Sense::click())
            .on_hover_text(tr!("number-presets"));
        list_frame::paint_arrow(ui, arrow, &button);
        if button.clicked() {
            presets.open = !presets.open;
            presets.highlighted = if auto_hint.is_some() {
                0
            } else {
                nearest_preset(quantity, current) + offset
            };
        }
        if presets.open {
            let anchor = response.rect.left_bottom() + egui::vec2(0.0, 2.0);
            let mut picked = None;
            let area = egui::Area::new(list_id)
                .order(egui::Order::Foreground)
                .fixed_pos(anchor)
                .constrain(true)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        picked = preset_popup_contents(ui, quantity, presets.highlighted);
                    });
                });
            let pressed_at = ui.input(|input| {
                input
                    .pointer
                    .any_pressed()
                    .then(|| input.pointer.interact_pos())
                    .flatten()
            });
            let outside = pressed_at.is_some_and(|pos| {
                !response.rect.contains(pos)
                    && !button.rect.contains(pos)
                    && !area.response.rect.contains(pos)
            });
            if let Some(choice) = picked {
                match choice {
                    Some(preset) => {
                        current = preset;
                        changed = true;
                    }
                    None => auto = true,
                }
                presets.open = false;
                response.request_focus();
            } else if outside {
                presets.open = false;
            }
        }
    }
    ctx.data_mut(|data| data.insert_temp(list_id, presets));

    if changed {
        *value = T::from_f64(current);
        // A step, a notch or a choice starts again from the value: no more
        // draft, no more frame. A confirmed entry keeps its own (clamped value).
        if !typed {
            field_state::discard_draft(&ctx, id);
        }
    }
    NumberResponse {
        response,
        changed,
        auto,
    }
}

/// Consumes the key `shortcut` of the registry.
fn key(ui: &egui::Ui, shortcut: &egui::KeyboardShortcut) -> bool {
    ui.input_mut(|input| input.consume_shortcut(shortcut))
}

/// Selects all the text of the field `id` (`len` characters).
fn select_all(ctx: &egui::Context, id: egui::Id, len: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(len),
            )));
        state.store(ctx, id);
    }
}

/// The wheel notches of this frame (positive upward), and the wheel consumed:
/// the window under the field does not scroll. Ctrl+wheel is the zoom (S-X1)
/// and is not taken.
fn wheel_notches(ui: &mut egui::Ui) -> i32 {
    let total: f32 = ui.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } if !modifiers.command => Some(match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Page => delta.y * 10.0,
                    egui::MouseWheelUnit::Point => delta.y / 50.0,
                }),
                _ => None,
            })
            .sum()
    });
    if total == 0.0 {
        return 0;
    }
    ui.input_mut(|input| {
        input.smooth_scroll_delta = egui::Vec2::ZERO;
    });
    let magnitude = total.abs().round().max(1.0) as i32;
    magnitude * total.signum() as i32
}

/// `value` moved by `steps` steps (a big step is worth ten notches), clamped
/// to the bounds and rounded to the step's decimals so that 0.1 + 0.2 stays 0.3.
fn step_value(quantity: &Quantity, value: f64, steps: i32) -> f64 {
    let unit = if steps.abs() >= 10 {
        quantity.big_step * f64::from(steps / 10)
    } else {
        quantity.step * f64::from(steps)
    };
    let moved = (value + unit).clamp(quantity.min, quantity.max);
    (moved * 1e9).round() / 1e9
}

/// `value` moved by `notches` entries of the preset list (positive = upward).
/// An off-list value jumps to the neighboring entry in the direction of the
/// notch; at the ends, it stays.
fn step_preset(quantity: &Quantity, value: f64, notches: i32) -> f64 {
    const EPS: f64 = 1e-9;
    let mut current = value;
    for _ in 0..notches.unsigned_abs() {
        let next = if notches > 0 {
            quantity
                .presets
                .iter()
                .copied()
                .find(|p| *p > current + EPS)
        } else {
            quantity
                .presets
                .iter()
                .copied()
                .rev()
                .find(|p| *p < current - EPS)
        };
        match next {
            Some(preset) => current = preset,
            None => break,
        }
    }
    current
}

/// The index of the preset closest to `value`.
fn nearest_preset(quantity: &Quantity, value: f64) -> usize {
    quantity
        .presets
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - value).abs().total_cmp(&(*b - value).abs()))
        .map_or(0, |(index, _)| index)
}

/// The contents of the list of values (the name contains "popup": it is an
/// exclusive choice, `no_selectable_button_outside_a_menu`). Returns the
/// clicked value.
fn preset_popup_contents(
    ui: &mut egui::Ui,
    quantity: &Quantity,
    highlighted: usize,
) -> Option<Option<f64>> {
    let offset = usize::from(quantity.auto == AutoKind::Field);
    let mut picked = None;
    // RAW-SCROLL(#347): drop-down list, menu-like: no end margin
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            ui.set_min_width(FIELD_WIDTH);
            if offset == 1
                && ui
                    .selectable_label(highlighted == 0, tr!("common-auto"))
                    .clicked()
            {
                picked = Some(None);
            }
            for (index, preset) in quantity.presets.iter().enumerate() {
                let index = index + offset;
                let entry = ui.selectable_label(
                    index == highlighted,
                    format_value(*preset, quantity.decimals),
                );
                if entry.clicked() {
                    picked = Some(Some(*preset));
                }
                if index == highlighted
                    && ui.input(|input| {
                        input.key_pressed(egui::Key::ArrowDown)
                            || input.key_pressed(egui::Key::ArrowUp)
                    })
                {
                    entry.scroll_to_me(None);
                }
            }
        });
    picked
}

/// Reads an entry: a finite number, an integer if the quantity is one, clamped
/// to the bounds.
fn read(quantity: &Quantity, text: &str) -> Verdict<Entry> {
    let trimmed = text.trim();
    // Auto: emptied field (the value itself has one) or the word `auto` (the
    // value or its group). Elsewhere, neither is readable.
    if quantity.auto == AutoKind::Field && trimmed.is_empty() {
        return Verdict::Valid(Entry::Auto);
    }
    if quantity.auto != AutoKind::None
        && (trimmed.eq_ignore_ascii_case("auto") || trimmed.eq_ignore_ascii_case("a"))
    {
        return Verdict::Valid(Entry::Auto);
    }
    if quantity.auto == AutoKind::Group && trimmed.is_empty() {
        return Verdict::Refused(tr!("field-refused-use-auto"));
    }
    let Some(parsed) = stokesforge_core::number_format::parse_input(trimmed) else {
        return refused(quantity);
    };
    if quantity.integer && parsed.fract() != 0.0 {
        return refused(quantity);
    }
    if parsed < quantity.min {
        return Verdict::Clamped {
            value: Entry::Value(quantity.min),
            bound: Bound::Min,
            limit: format_value(quantity.min, quantity.decimals),
        };
    }
    if parsed > quantity.max {
        return Verdict::Clamped {
            value: Entry::Value(quantity.max),
            bound: Bound::Max,
            limit: format_value(quantity.max, quantity.decimals),
        };
    }
    Verdict::Valid(Entry::Value(parsed))
}

fn refused(quantity: &Quantity) -> Verdict<Entry> {
    Verdict::Refused(if quantity.integer {
        tr!("field-refused-integer")
    } else {
        tr!("field-refused-number")
    })
}

/// A number as it is displayed: at most `decimals` decimals, no trailing
/// zeros, never "-0".
pub fn format_value(value: f64, decimals: usize) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.truncate(text.trim_end_matches('0').trim_end_matches('.').len());
    }
    if text == "-0" {
        text = "0".into();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_app_state::quantities;

    fn key_event(key: egui::Key) -> egui::Event {
        key_event_with(key, egui::Modifiers::NONE)
    }

    #[test]
    fn a_three_digit_integer_without_list_takes_the_compact_width() {
        for compact in [
            &quantities::COLOR_CHANNEL,
            &quantities::HUE_DEGREES,
            &quantities::PERCENT,
            &quantities::OPACITY_PERCENT,
        ] {
            assert!(compact.is_compact());
        }
        // More than three digits, real or signed: short width.
        assert!(quantities::ROTATION.is_compact_signed());
        assert!(!quantities::ROTATION.is_compact());
        assert!(!quantities::FREE.is_compact_signed());
        assert!(!quantities::TEXT_FORMAT_SIZE.is_compact_signed());
        assert!(!quantities::ITERATIONS.is_compact());
        assert!(!quantities::FREE.is_compact());
        assert!(!quantities::COUNT.is_compact());
    }

    #[test]
    fn format_removes_trailing_zeros_and_minus_zero() {
        assert_eq!(format_value(2.5, 2), "2.5");
        assert_eq!(format_value(2.0, 2), "2");
        assert_eq!(format_value(-0.0, 2), "0");
        assert_eq!(format_value(-0.001, 2), "0");
        assert_eq!(format_value(100.0, 0), "100");
        assert_eq!(format_value(0.3, 8), "0.3");
    }

    #[test]
    fn decimal_comma_is_read_as_the_point() {
        // #223: "2,5" is 2.5; an ambiguous comma stays refused.
        assert_eq!(
            read(&quantities::AXIS_BOUND, "2,5"),
            Verdict::Valid(Entry::Value(2.5))
        );
        assert!(matches!(
            read(&quantities::AXIS_BOUND, "1,2,3"),
            Verdict::Refused(_)
        ));
    }

    #[test]
    fn an_unreadable_entry_is_refused() {
        for text in ["abc", "1..2", "", "inf", "nan", "1e999"] {
            assert!(
                matches!(read(&quantities::LINE_WIDTH, text), Verdict::Refused(_)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn the_format_bar_body_size_accepts_auto() {
        // #245: the format bar must be able to put the body size back to Auto,
        // by the word, by the emptied field or by the list entry.
        let body = quantities::TEXT_FORMAT_SIZE;
        assert_eq!(body.auto, AutoKind::Field);
        for text in ["auto", ""] {
            assert_eq!(read(&body, text), Verdict::Valid(Entry::Auto));
        }
        // The in-place editor's body size has no Auto: it is not this one.
        assert_eq!(quantities::TEXT_SIZE.auto, AutoKind::None);
    }

    #[test]
    fn auto_is_accepted_according_to_the_quantity() {
        let field = quantities::FREE.with_auto(AutoKind::Field);
        for text in ["", "  ", "auto", "Auto", " AUTO ", "a", "A"] {
            assert_eq!(read(&field, text), Verdict::Valid(Entry::Auto), "{text:?}");
        }
        assert_eq!(read(&field, "5"), Verdict::Valid(Entry::Value(5.0)));
        // A group bound accepts the word, not the emptied field.
        let group = &quantities::AXIS_BOUND;
        assert_eq!(read(group, "auto"), Verdict::Valid(Entry::Auto));
        assert!(matches!(read(group, ""), Verdict::Refused(_)));
        assert!(matches!(read(group, "  "), Verdict::Refused(_)));
    }

    /// Witness: a quantity without Auto refuses the word like the emptied field.
    #[test]
    fn without_auto_the_word_and_the_empty_field_are_refused() {
        for text in ["auto", "Auto", "a", "A", ""] {
            assert!(
                matches!(read(&quantities::LINE_WIDTH, text), Verdict::Refused(_)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn an_integer_refuses_decimals_but_not_point_zero() {
        assert!(matches!(
            read(&quantities::MAX_PEAKS, "2.5"),
            Verdict::Refused(_)
        ));
        assert_eq!(
            read(&quantities::MAX_PEAKS, "3.0"),
            Verdict::Valid(Entry::Value(3.0))
        );
    }

    #[test]
    fn out_of_bounds_the_value_is_clamped_to_the_bound() {
        let Verdict::Clamped {
            value,
            bound,
            limit,
        } = read(&quantities::LINE_WIDTH, "35")
        else {
            panic!("35 exceeds 20");
        };
        assert_eq!(
            (value, bound, limit.as_str()),
            (Entry::Value(20.0), Bound::Max, "20")
        );
        assert!(matches!(
            read(&quantities::LINE_WIDTH, "-1"),
            Verdict::Clamped {
                bound: Bound::Min,
                ..
            }
        ));
    }

    /// #260: the caps of the analysis fields are those of
    /// `stokesforge-analysis`, which `Operation::validate` also applies. The
    /// "Max peaks" field goes up from 50 to 100 peaks.
    #[test]
    fn analysis_fields_follow_the_caps_of_the_analysis_crate() {
        use stokesforge_analysis::ops::{MAX_COMPONENTS, MAX_DEGREE, MAX_ITERATIONS, MAX_PEAKS};
        assert_eq!(quantities::POLYNOMIAL_DEGREE.max, f64::from(MAX_DEGREE));
        assert_eq!(quantities::ITERATIONS.max, MAX_ITERATIONS as f64);
        assert_eq!(quantities::MAX_PEAKS.max, MAX_PEAKS as f64);
        assert_eq!(quantities::COMPONENTS.max, MAX_COMPONENTS as f64);
        assert_eq!(
            MAX_PEAKS, 100,
            "decision of 2026-10-15: 100 peaks everywhere"
        );

        assert_eq!(
            read(&quantities::MAX_PEAKS, "100"),
            Verdict::Valid(Entry::Value(100.0))
        );
        let Verdict::Clamped {
            value,
            bound,
            limit,
        } = read(&quantities::MAX_PEAKS, "101")
        else {
            panic!("101 exceeds 100");
        };
        assert_eq!(
            (value, bound, limit.as_str()),
            (Entry::Value(100.0), Bound::Max, "100")
        );
    }

    #[test]
    fn decimals_typed_on_the_keyboard_are_kept() {
        assert_eq!(
            read(&quantities::LINE_WIDTH, "2.75"),
            Verdict::Valid(Entry::Value(2.75))
        );
    }

    #[test]
    fn a_step_is_rounded_and_bounded() {
        let width = &quantities::LINE_WIDTH;
        assert_eq!(step_value(width, 0.2, 1), 0.3);
        assert_eq!(step_value(width, 19.95, 1), 20.0, "bound reached");
        assert_eq!(step_value(width, 0.0, -1), 0.0);
        assert_eq!(step_value(width, 1.0, 10), 2.0, "a big step");
        assert_eq!(step_value(width, 5.0, -10), 4.0);
    }

    #[test]
    fn the_nearest_preset_is_highlighted() {
        assert_eq!(nearest_preset(&quantities::LINE_WIDTH, 1.4), 2);
        assert_eq!(nearest_preset(&quantities::LINE_WIDTH, 99.0), 5);
    }

    #[test]
    fn quantities_are_consistent() {
        for quantity in [
            quantities::FREE,
            quantities::LINE_WIDTH,
            quantities::SHAPE_LINE_WIDTH,
            quantities::SYMBOL_SIZE,
            quantities::TEXT_SIZE,
            quantities::TEXT_FORMAT_SIZE,
            quantities::ROTATION,
            quantities::COLOR_CHANNEL,
            quantities::OPACITY_PERCENT,
            quantities::HUE_DEGREES,
            quantities::PERCENT,
            quantities::POLYNOMIAL_DEGREE,
            quantities::PROMINENCE_PERCENT,
            quantities::MAX_PEAKS,
            quantities::COMPONENTS,
            quantities::ITERATIONS,
            quantities::COUNT,
            quantities::LINE_NUMBER,
        ] {
            assert!(quantity.min < quantity.max, "{quantity:?}");
            assert!(quantity.step > 0.0 && quantity.big_step >= quantity.step);
            for preset in quantity.presets {
                assert!(
                    (quantity.min..=quantity.max).contains(preset),
                    "{preset} outside {quantity:?}"
                );
            }
        }
    }

    /// The text-body range is the model's: two definitions would diverge
    /// silently.
    #[test]
    fn text_size_follows_the_model_range() {
        let (low, high) = stokesforge_render_egui::AnnotationSpec::SIZE_RANGE;
        assert_eq!(
            (quantities::TEXT_SIZE.min, quantities::TEXT_SIZE.max),
            (low, high)
        );
    }

    // — The painted field: keyboard, list, wheel. —

    fn key_event_with(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn notch(delta: f32) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, delta),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// A bench: a field for the line thickness (with a list of values) or for
    /// `quantity`, its value, the clock.
    struct Bench {
        ctx: egui::Context,
        value: f64,
        quantity: Quantity,
        time: f64,
        id: Option<egui::Id>,
        field: Option<egui::Rect>,
    }

    impl Bench {
        fn new(value: f64, quantity: Quantity) -> Self {
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            Self {
                ctx,
                value,
                quantity,
                time: 0.0,
                id: None,
                field: None,
            }
        }

        /// A frame with `events`; `focus` gives the focus to the field before
        /// it is placed. Returns what was painted and whether the value changed.
        fn image(&mut self, events: Vec<egui::Event>, focus: bool) -> (egui::FullOutput, bool) {
            self.time += 0.1;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                events,
                time: Some(self.time),
                focused: true,
                ..Default::default()
            };
            let (value, quantity, field_id_slot, field_rect_slot) = (
                &mut self.value,
                self.quantity,
                &mut self.id,
                &mut self.field,
            );
            let mut changed = false;
            let mut output = self.ctx.run_ui(input, |ui| {
                ui.horizontal(|ui| {
                    let id = field_state::field_id(ui, "field");
                    *field_id_slot = Some(id);
                    if focus {
                        ui.memory_mut(|m| m.request_focus(id));
                    }
                    let response = number_field(ui, "field", value, &quantity);
                    *field_rect_slot = Some(response.response.rect);
                    changed = response.changed;
                });
            });
            output.textures_delta.clear();
            (output, changed)
        }

        /// The field takes the focus and settles.
        fn focus_field(&mut self) {
            self.image(Vec::new(), true);
            self.image(Vec::new(), false);
        }

        fn painted(&mut self, text: &str) -> bool {
            let (output, _) = self.image(Vec::new(), false);
            output.shapes.iter().any(|painted| {
                matches!(&painted.shape,
                    egui::Shape::Text(text_shape) if text_shape.galley.job.text == text)
            })
        }

        fn frames(output: &egui::FullOutput, color: egui::Color32) -> usize {
            output
                .shapes
                .iter()
                .filter(|painted| matches!(&painted.shape, egui::Shape::Rect(r) if r.stroke.color == color))
                .count()
        }
    }

    #[test]
    fn arrows_change_by_a_step_and_shift_by_a_big_step() {
        let mut bench = Bench::new(2.0, quantities::LINE_WIDTH);
        bench.focus_field();
        bench.image(
            vec![key_event_with(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            false,
        );
        assert!((bench.value - 2.1).abs() < 1e-9, "{}", bench.value);
        bench.image(
            vec![key_event_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            false,
        );
        assert!((bench.value - 2.0).abs() < 1e-9);
        bench.image(
            vec![key_event_with(egui::Key::ArrowUp, egui::Modifiers::SHIFT)],
            false,
        );
        assert!((bench.value - 3.0).abs() < 1e-9, "{}", bench.value);
    }

    #[test]
    fn without_focus_arrows_do_nothing() {
        let mut bench = Bench::new(2.0, quantities::LINE_WIDTH);
        bench.image(
            vec![key_event_with(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(bench.value, 2.0);
    }

    #[test]
    fn alt_down_opens_the_list_enter_picks_and_escape_closes_it() {
        let mut bench = Bench::new(1.4, quantities::LINE_WIDTH);
        bench.focus_field();
        assert!(!bench.painted("3"), "list closed: no preset offered");
        bench.image(
            vec![key_event_with(egui::Key::ArrowDown, egui::Modifiers::ALT)],
            false,
        );
        assert!(bench.painted("3"), "list open");
        assert_eq!(bench.value, 1.4, "opening changes nothing");
        // The preset closest to 1.4 is 1.5; two notches down.
        bench.image(
            vec![key_event_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            false,
        );
        bench.image(
            vec![key_event_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(bench.value, 1.4, "highlighting changes nothing");
        let (_, changed) = bench.image(vec![key_event(egui::Key::Enter)], false);
        assert!(changed);
        assert_eq!(bench.value, 3.0);
        assert!(!bench.painted("4"), "the list is closed after the choice");

        // Escape closes the list without giving back or changing the value.
        bench.image(
            vec![key_event_with(egui::Key::ArrowDown, egui::Modifiers::ALT)],
            false,
        );
        assert!(bench.painted("4"));
        let (_, changed) = bench.image(vec![key_event(egui::Key::Escape)], false);
        assert!(!changed);
        assert!(!bench.painted("4"), "Escape closes the list");
        assert_eq!(bench.value, 3.0);
    }

    #[test]
    fn a_quantity_without_presets_opens_no_list() {
        let mut bench = Bench::new(2.0, quantities::FREE);
        bench.focus_field();
        bench.image(
            vec![key_event_with(egui::Key::ArrowDown, egui::Modifiers::ALT)],
            false,
        );
        // Alt+↓ is not a step: the value does not move, nothing opens.
        assert_eq!(bench.value, 2.0);
    }

    #[test]
    fn click_selects_all_the_text() {
        let mut bench = Bench::new(12.5, quantities::LINE_WIDTH);
        bench.image(Vec::new(), false);
        let center = egui::pos2(20.0, 12.0);
        for pressed in [None, Some(true), Some(false)] {
            let events = match pressed {
                None => vec![egui::Event::PointerMoved(center)],
                Some(pressed) => vec![egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            };
            bench.image(events, false);
        }
        bench.image(Vec::new(), false);
        let state =
            egui::TextEdit::load_state(&bench.ctx, bench.id.expect("id")).expect("field state");
        let range = state.cursor.char_range().expect("selection");
        let (low, high) = (
            range.primary.index.min(range.secondary.index),
            range.primary.index.max(range.secondary.index),
        );
        assert_eq!((low.0, high.0), (0, 4), "\"12.5\" entirely");
    }

    #[test]
    fn wheel_follows_the_list_on_hover_without_focus() {
        let mut bench = Bench::new(2.0, quantities::LINE_WIDTH);
        let center = egui::pos2(20.0, 12.0);
        // Cursor elsewhere: the wheel stays with the window.
        bench.image(
            vec![egui::Event::PointerMoved(egui::pos2(300.0, 250.0))],
            false,
        );
        bench.image(vec![notch(1.0)], false);
        assert_eq!(bench.value, 2.0);
        // Hovering, without focus: one notch = the neighboring entry (2 → 3, then 1.5).
        bench.image(vec![egui::Event::PointerMoved(center)], false);
        bench.image(vec![notch(1.0)], false);
        assert_eq!(bench.value, 3.0);
        bench.image(vec![notch(-2.0)], false);
        assert_eq!(bench.value, 1.5);
    }

    #[test]
    fn step_preset_jumps_off_list_and_stops_at_the_ends() {
        let q = quantities::LINE_WIDTH; // 0.5 · 1 · 1.5 · 2 · 3 · 4
        assert_eq!(step_preset(&q, 2.3, 1), 3.0);
        assert_eq!(step_preset(&q, 2.3, -1), 2.0);
        assert_eq!(step_preset(&q, 4.0, 1), 4.0);
        assert_eq!(step_preset(&q, 0.5, -3), 0.5);
        assert_eq!(step_preset(&q, 7.0, 1), 7.0);
        assert_eq!(step_preset(&q, 7.0, -1), 4.0);
        assert_eq!(step_preset(&q, 1.0, 3), 3.0);
    }

    #[test]
    fn a_field_without_list_ignores_the_wheel() {
        let mut bench = Bench::new(2.0, quantities::FREE);
        bench.image(
            vec![egui::Event::PointerMoved(egui::pos2(20.0, 12.0))],
            false,
        );
        bench.image(vec![notch(1.0)], false);
        assert_eq!(bench.value, 2.0);
        bench.focus_field();
        let (_, changed) = bench.image(vec![notch(1.0)], false);
        assert!(!changed);
        assert_eq!(bench.value, 2.0);
    }

    #[test]
    fn the_filled_arrow_is_inside_the_frame_of_a_list_field() {
        let mut bench = Bench::new(2.0, quantities::LINE_WIDTH);
        let (output, _) = bench.image(Vec::new(), false);
        let field = bench.field.expect("rect of the field");
        let arrow = output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Path(path) if path.fill != egui::Color32::TRANSPARENT => {
                    Some(egui::Rect::from_points(&path.points))
                }
                _ => None,
            });
        let arrow = arrow.expect("filled triangle");
        assert!(field.contains_rect(arrow), "{arrow:?} outside {field:?}");
    }

    #[test]
    fn an_out_of_bound_entry_is_clamped_and_an_unreadable_one_refused() {
        let mut bench = Bench::new(2.0, quantities::LINE_WIDTH);
        bench.focus_field();
        bench.image(vec![egui::Event::Text("35".into())], false);
        let (_, changed) = bench.image(vec![key_event(egui::Key::Enter)], false);
        assert!(changed);
        assert_eq!(bench.value, 20.0);
        let (output, _) = bench.image(Vec::new(), false);
        assert_eq!(
            Bench::frames(&output, crate::theme::field_clamped_color()),
            1
        );

        bench.focus_field();
        bench.image(vec![egui::Event::Text("abc".into())], false);
        let (output, _) = bench.image(Vec::new(), false);
        assert_eq!(
            Bench::frames(&output, crate::theme::field_refused_color()),
            1
        );
        assert_eq!(bench.value, 20.0);
    }

    #[test]
    fn an_integer_number_writes_an_integer() {
        let mut bench = Bench::new(3.0, quantities::POLYNOMIAL_DEGREE);
        bench.focus_field();
        bench.image(
            vec![key_event_with(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(bench.value, 4.0);
        assert!(bench.painted("4"));
    }
}

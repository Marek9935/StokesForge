//! **The** contract of in-place editing (#224, `docs/standard-interface.md`,
//! § Text fields and in-place editing, S-T4): the grid, sheet tabs, Project
//! Manager, Object Manager and graph titles all call it.
//!
//! The contract, identical everywhere:
//!
//! - on opening the field takes the focus and **all the text is selected**
//!   (except when the keystroke that opens it is already its content: the grid,
//!   [`Opening::KeepCursor`]);
//! - Enter and a click elsewhere **validate**, Escape **cancels**, Tab
//!   **validates and moves to the next** (Shift+Tab to the previous);
//! - a refused entry ([`Resolution::Stay`]) keeps the field open and its text,
//!   red frame of `field_state`; a click elsewhere on a refused entry gives back
//!   the old value.
//!
//! The building block owns neither the buffer nor the model: the caller keeps
//! its `String` and its state, sets its layout (the block creates no `Ui`, so
//! changes no egui identifier) and translates [`Resolution`] into an action.
//! Enter, Tab and Escape are **consumed** here: no shortcut further down the
//! frame sees them again (the trap of the grid's double move, #38).
//!
//! Why a pure function ([`resolve`]): the decision "validate, stay, give back
//! the old value" is tested without a window; [`show`] is only the reading of
//! keys and focus.

use eframe::egui;

use crate::field_state::{show_issue, FieldIssue};

/// The keys the field keeps for itself: Tab must not make the focus leave for
/// the rest of the interface (the field reads it, #38). Escape stays with egui,
/// which gives the focus back: [`show`] reads it first.
pub(super) const KEYS: egui::EventFilter = egui::EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// How the field opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opening {
    /// All the text is selected: typing replaces it.
    SelectAll,
    /// The cursor stays where egui put it (at the end): the text has just been
    /// typed, it must not be selected to be replaced.
    KeepCursor,
}

/// What losing the focus does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusLoss {
    /// A click elsewhere ends the entry ([`EditEnd::Elsewhere`]): the general
    /// case.
    Ends,
    /// The field may lose the focus without the entry ending: an editor that
    /// carries other settings (size, rotation, color) and decides itself what an
    /// "elsewhere" is (graph titles, free texts). The field does not take the focus
    /// back by force.
    Ignored,
}

/// What ended the entry this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditEnd {
    /// Enter; `shift` if Shift was held (the grid goes up one row).
    Enter { shift: bool },
    /// Tab; `shift` for Shift+Tab (the previous field).
    Tab { shift: bool },
    /// A click elsewhere, or the focus taken by another field.
    Elsewhere,
    /// Escape.
    Cancel,
}

/// What the caller does with the end of the entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Resolution {
    /// The field stays open with its text (Enter or Tab on a refused
    /// entry).
    Stay,
    /// The field closes. `write`: the text is to be applied (it changed and it is
    /// readable); otherwise the old value comes back. `end` says how to continue
    /// (Tab moves to the next, Enter advances in the grid).
    Close { write: bool, end: EditEnd },
}

/// The cycle's decision, without egui. `unchanged`: the text still equals the
/// original; `refused`: the caller refuses the text (empty name, already taken…).
pub(super) fn resolve(end: EditEnd, unchanged: bool, refused: bool) -> Resolution {
    match end {
        EditEnd::Cancel => Resolution::Close { write: false, end },
        EditEnd::Enter { .. } | EditEnd::Tab { .. } if refused => Resolution::Stay,
        EditEnd::Elsewhere if refused => Resolution::Close { write: false, end },
        _ => Resolution::Close {
            write: !unchanged,
            end,
        },
    }
}

/// What [`show`] returns.
pub(super) struct Shown {
    pub(super) response: egui::Response,
    /// The end of the entry of this frame, if there is one.
    pub(super) end: Option<EditEnd>,
}

/// Last frame in which the field `id` was painted: a way to recognize its
/// opening (the `TextEdit` of a stable identifier keeps its cursor from one
/// edit to the next, so it does not say whether it is new).
#[derive(Clone, Copy)]
struct Seen(u64);

/// Is the opening of the field `id` this frame? To be called **before**
/// painting the `TextEdit`, once per frame and per field.
pub(super) fn begin(ctx: &egui::Context, id: egui::Id) -> bool {
    let pass = ctx.cumulative_pass_nr();
    let seen_id = id.with("inplace_seen");
    let fresh = ctx
        .data(|data| data.get_temp::<Seen>(seen_id))
        .is_none_or(|Seen(last)| pass.saturating_sub(last) > 2);
    ctx.data_mut(|data| data.insert_temp(seen_id, Seen(pass)));
    fresh
}

/// Reads the end of the entry of an already painted field (`response`): focus
/// and selection on opening, Enter, Tab, Escape, loss of focus. `fresh` comes
/// from [`begin`], the `TextEdit` carried [`KEYS`] and the identifier
/// `response.id`. `refusal`: the sentence that says why the current text is
/// refused, painted as a red frame.
pub(super) fn finish(
    ui: &egui::Ui,
    response: &egui::Response,
    fresh: bool,
    opening: Opening,
    loss: FocusLoss,
    refusal: Option<&str>,
) -> Option<EditEnd> {
    let ctx = ui.ctx().clone();
    let id = response.id;
    // Read before any focus request: asking for the focus again would erase the
    // loss that Enter signals (the trap of `cell_widget`).
    let lost = response.lost_focus();
    if fresh {
        response.request_focus();
        if opening == Opening::SelectAll {
            if let Some(mut state) = egui::TextEdit::load_state(&ctx, id) {
                let end = state_text_len(&ctx, id);
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(end),
                    )));
                state.store(&ctx, id);
            }
        }
    }
    if let Some(message) = refusal {
        show_issue(ui, response, &FieldIssue::refused(message));
    }

    let focused = response.has_focus();
    let (escape, enter, tab, shift) = ctx.input(|input| {
        (
            input.key_pressed(egui::Key::Escape),
            input.key_pressed(egui::Key::Enter),
            input.key_pressed(egui::Key::Tab),
            input.modifiers.shift,
        )
    });
    let consume = |key: &egui::KeyboardShortcut| {
        ctx.input_mut(|input| input.consume_shortcut(key));
    };
    let end = if !fresh && (focused || lost) && escape {
        consume(&super::shortcuts::KEY_ESCAPE);
        Some(EditEnd::Cancel)
    } else if !fresh && focused && tab {
        consume(&super::shortcuts::KEY_TAB);
        Some(EditEnd::Tab { shift })
    } else if !fresh && lost && (enter || loss == FocusLoss::Ends) {
        if enter {
            consume(&super::shortcuts::KEY_ENTER);
            Some(EditEnd::Enter { shift })
        } else {
            Some(EditEnd::Elsewhere)
        }
    } else {
        // An open field without focus (after a window change) takes it back: there is
        // no "open but inert" state.
        if !fresh && !focused && !lost && loss == FocusLoss::Ends {
            response.request_focus();
        }
        None
    };
    if end.is_some() {
        response.surrender_focus();
        ctx.data_mut(|data| data.remove::<Seen>(id.with("inplace_seen")));
        ctx.request_repaint();
    }
    end
}

/// The field's text, measured in characters, to select it all.
fn state_text_len(ctx: &egui::Context, id: egui::Id) -> usize {
    ctx.data(|data| data.get_temp::<TextLen>(id.with("inplace_len")))
        .map_or(0, |TextLen(len)| len)
}

/// Text length seen by [`show`] or [`measured`] this frame.
#[derive(Clone, Copy)]
struct TextLen(usize);

/// Records the text length of a field placed outside [`show`] (before
/// [`finish`], which selects everything on opening).
pub(super) fn measured(ctx: &egui::Context, id: egui::Id, text: &str) {
    ctx.data_mut(|data| data.insert_temp(id.with("inplace_len"), TextLen(text.chars().count())));
}

/// Paints the field and reads its end of entry. `edit` is the caller's
/// `TextEdit` (width, font, frame, hint text): the block only sets its
/// identifier and the key filter on it. For a field placed by `ui.put` or in a
/// particular layout: [`begin`] then [`finish`].
pub(super) fn show(
    ui: &mut egui::Ui,
    id: egui::Id,
    edit: egui::TextEdit<'_>,
    opening: Opening,
    loss: FocusLoss,
    refusal: Option<&str>,
) -> Shown {
    let fresh = begin(ui.ctx(), id);
    let output = edit.id(id).event_filter(KEYS).show(ui);
    measured(ui.ctx(), id, output.galley.text());
    let response = output.response.response;
    let end = finish(ui, &response, fresh, opening, loss, refusal);
    Shown { response, end }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{context, image, key};
    use super::*;

    const ENTER: EditEnd = EditEnd::Enter { shift: false };
    const TAB: EditEnd = EditEnd::Tab { shift: false };

    #[test]
    fn escape_always_cancels() {
        for refused in [false, true] {
            assert_eq!(
                resolve(EditEnd::Cancel, false, refused),
                Resolution::Close {
                    write: false,
                    end: EditEnd::Cancel
                }
            );
        }
    }

    #[test]
    fn enter_and_tab_validate_what_changed() {
        for end in [ENTER, TAB] {
            assert_eq!(
                resolve(end, false, false),
                Resolution::Close { write: true, end }
            );
            // Without change, nothing is written but the gesture continues (Tab moves to
            // the next).
            assert_eq!(
                resolve(end, true, false),
                Resolution::Close { write: false, end }
            );
        }
    }

    #[test]
    fn enter_and_tab_on_a_refused_entry_keep_the_field_open() {
        for end in [ENTER, TAB] {
            assert_eq!(resolve(end, false, true), Resolution::Stay);
        }
    }

    #[test]
    fn a_click_elsewhere_validates_or_restores_the_old_value_if_refused() {
        assert_eq!(
            resolve(EditEnd::Elsewhere, false, false),
            Resolution::Close {
                write: true,
                end: EditEnd::Elsewhere
            }
        );
        assert_eq!(
            resolve(EditEnd::Elsewhere, false, true),
            Resolution::Close {
                write: false,
                end: EditEnd::Elsewhere
            }
        );
    }

    /// A field opened on "Spectrum", driven frame by frame.
    struct Bench {
        context: egui::Context,
        text: String,
        ends: Vec<EditEnd>,
        opening: Opening,
        refusal: Option<String>,
    }

    impl Bench {
        fn new(opening: Opening) -> Self {
            Self {
                context: context(),
                text: "Spectrum".to_string(),
                ends: Vec::new(),
                opening,
                refusal: None,
            }
        }

        fn image(&mut self, events: Vec<egui::Event>) {
            let Self {
                context,
                text,
                ends,
                opening,
                refusal,
            } = self;
            image(context, events, |ui| {
                let shown = show(
                    ui,
                    egui::Id::new("field"),
                    egui::TextEdit::singleline(text),
                    *opening,
                    FocusLoss::Ends,
                    refusal.as_deref(),
                );
                ends.extend(shown.end);
            });
        }

        fn open(&mut self) {
            self.image(Vec::new());
            self.image(Vec::new());
        }
    }

    #[test]
    fn opening_selects_everything_and_typing_replaces() {
        let mut bench = Bench::new(Opening::SelectAll);
        bench.open();
        bench.image(vec![egui::Event::Text("New".to_string())]);
        assert_eq!(bench.text, "New");
        assert!(bench.ends.is_empty());
    }

    #[test]
    fn keeping_the_cursor_does_not_erase_the_typed_text() {
        let mut bench = Bench::new(Opening::KeepCursor);
        bench.open();
        bench.image(vec![egui::Event::Text("!".to_string())]);
        assert_eq!(bench.text, "Spectrum!");
    }

    #[test]
    fn enter_validates_and_is_consumed() {
        let mut bench = Bench::new(Opening::SelectAll);
        bench.open();
        bench.image(vec![key(egui::Key::Enter)]);
        assert_eq!(bench.ends, [ENTER]);
    }

    #[test]
    fn escape_cancels() {
        let mut bench = Bench::new(Opening::SelectAll);
        bench.open();
        bench.image(vec![key(egui::Key::Escape)]);
        assert_eq!(bench.ends, [EditEnd::Cancel]);
    }

    #[test]
    fn tab_validates_and_moves_to_the_next_without_egui_moving_the_focus() {
        let mut bench = Bench::new(Opening::SelectAll);
        bench.open();
        bench.image(vec![key(egui::Key::Tab)]);
        assert_eq!(bench.ends, [TAB]);
    }

    #[test]
    fn a_refused_field_is_painted_red() {
        let mut bench = Bench::new(Opening::SelectAll);
        bench.refusal = Some("Name already used".to_string());
        bench.open();
        let red = crate::theme::field_refused_color();
        let mut painted = false;
        let context = bench.context.clone();
        let Bench {
            text,
            opening,
            refusal,
            ..
        } = &mut bench;
        let clipped_shapes = image(&context, Vec::new(), |ui| {
            show(
                ui,
                egui::Id::new("field"),
                egui::TextEdit::singleline(text),
                *opening,
                FocusLoss::Ends,
                refusal.as_deref(),
            );
        });
        for clipped in clipped_shapes {
            if let egui::Shape::Rect(rect) = clipped.shape {
                painted |= rect.stroke.color == red;
            }
        }
        assert!(painted, "a red frame surrounds the refused field");
    }
}

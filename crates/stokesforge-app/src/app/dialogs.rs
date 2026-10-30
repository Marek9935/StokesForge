//! **Modal** dialogs: [`modal_dialog`], [`modal_dialog_with`],
//! [`modal_wizard`], [`modal_live_settings`], [`modal_read_only`], [`modal_confirm`],
//! [`modal_save_changes`] and [`modal_alert`], the building block of every
//! dialog box and every wizard — same frame, same title bar, same blocking of
//! the rest of the interface. It places the window through the content-window
//! building block (`window_chrome::show_window_in_layer`).
//!
//! Moved out of `app.rs` as is (#117), then extended by #228
//! (`docs/interface-standard.md`, § Windows and dialogs):
//!
//! - **nature** (#300, S-F11): [`DialogNature`] says what the window is —
//!   settings, draft, choice, wizard, read-only, confirmation,
//!   save, alert — and derives from it, through exhaustive `match`es without
//!   `_`, the footer, the default size (a [`SizeClass`]: Small 480 x 170,
//!   Medium 480 x 420, Large 640 x 500), the initial focus and the default
//!   button. The size is a field of the variant: an interactive window
//!   ([`InteractiveSize`]) takes a class or a free size justified by
//!   `// FREE-SIZE(#300):`; a window without rich interaction
//!   ([`PlainSize`]) may take the size of its content without justification;
//! - **button order** (S-F6): a dialog puts its secondary buttons on the left,
//!   then Cancel, then the main one on the right; a wizard puts Cancel on the
//!   left and groups Back · Next · Finish on the right; a read-only window
//!   has only a Close button (S-F2);
//! - **message area** of the footer, to the left of the buttons (S-M7): the
//!   error of a dialog gesture (never in the status bar; `AppState::dialog_error`,
//!   published by [`sync_dialogs`]), otherwise the **reason** the main button
//!   is greyed out (S-F10 → S-R14) — which is also its tooltip. The reason comes
//!   from the caller: `AcceptButton::Gated` (#226, `DialogFooter::gated`) when
//!   the button is greyed out; a button that never is, is `AcceptButton::Always`;
//! - **Escape** (S-F7): the one that leaves a text field does not close the
//!   dialog, the next one does Cancel;
//! - **size and position** remembered per kind of dialog (S-F8), in the
//!   preferences: restored on opening, handed back on closing by
//!   [`sync_dialogs`] (`Action::RememberDialogGeometry`). The kind is drawn
//!   from the dialog's identifier;
//! - **confirmations** (#229, S-F4, S-F5 → S-R4, S-R5): [`modal_confirm`] — Cancel
//!   is the default button (framed; Enter and Escape trigger it), the
//!   verb (Delete…) has the normal style and turns red on hover **and** on
//!   keyboard focus; [`modal_save_changes`] — "Discard Changes · Cancel · Save",
//!   the only written exception: Enter = Save; [`modal_alert`] — the serious
//!   failure (S-M8), a Close button and one possible action;
//! - **initial focus** (S-F9, S-F11): depending on the nature ([`InitialFocus`]),
//!   the first input field, the default button (confirmation, save,
//!   alert) or nothing (read-only). For the first input field:
//!   egui does not say which widgets are text fields: on opening, an
//!   invisible anchor takes the focus at the head of the content, then the
//!   focus advances one widget per pass, all in the same frame
//!   (`request_discard`: nothing is painted between two steps; egui's Tab
//!   navigation) up to the first text field of the dialog (a `TextEdit`,
//!   recognized by the state it keeps in egui's memory). It stops — focus
//!   given back — at the footer, outside the dialog, after
//!   [`FOCUS_WALK_STEPS`] widgets, or as soon as the user types or clicks
//!   (the gesture then applies with no focus).

use std::collections::BTreeMap;

use eframe::egui;

use crate::theme;
use stokesforge_app_state::preferences::DialogGeometry as SavedGeometry;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::grid::keyboard_focus_outside_grid;
use super::window_chrome::{
    floating_resize_handles, show_window_contents_bounded, show_window_in_layer, window_header,
    HeaderPlacement, WindowHeaderActions, MIN_CONTENT_WINDOW_SIZE,
};

/// Outcome of one frame of [`modal_dialog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DialogOutcome {
    /// Toujours ouvert.
    Open,
    /// "Accept" (default button: Enter), or Close of a read-only
    /// window.
    Accept,
    /// "Cancel", cross or Escape.
    Cancel,
}

/// Height of the strip of the "Accept" / "Cancel" buttons.
const DIALOG_FOOTER_HEIGHT: f32 = 38.0;

/// Minimum size of a footer button.
pub(super) const FOOTER_BUTTON_SIZE: egui::Vec2 = egui::vec2(84.0, 24.0);

/// Duration of a blocked-click alert ([`modal_dialog`]): two pulses.
const MODAL_FLASH_SECONDS: f64 = 0.6;

/// Number of widgets the initial focus (S-F9) walks through at most before
/// giving up.
const FOCUS_WALK_STEPS: u8 = 32;

/// Position and size of a modal dialog, kept in egui memory for as long as it
/// is open.
#[derive(Clone, Copy)]
struct DialogGeometry {
    pos: egui::Pos2,
    size: egui::Vec2,
}

/// The three **size classes** of a dialog (S-F11): the default size is not
/// invented, it fits into one of them. The size the user left (S-F8) then
/// takes precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SizeClass {
    /// 480 x 170: a question, a message, a few buttons.
    Small,
    /// 480 x 420: a form or settings of a few pages.
    Medium,
    /// 640 x 500: a list and its editor side by side.
    Large,
}

impl SizeClass {
    pub(super) const fn size(self) -> egui::Vec2 {
        match self {
            Self::Small => egui::vec2(480.0, 170.0),
            Self::Medium => egui::vec2(480.0, 420.0),
            Self::Large => egui::vec2(640.0, 500.0),
        }
    }
}

/// The size of an **interactive** window (settings, draft, wizard):
/// the default class of its nature, another class, or a free size
/// that the guard `no_free_size_without_justification` only lets through with
/// a `// FREE-SIZE(#300): <justification>` right before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum InteractiveSize {
    /// The default class of the nature ([`DialogNature::default_class`]).
    Default,
    Class(SizeClass),
    /// Outside the classes, justified in the code (S-F11).
    Free(egui::Vec2),
}

impl InteractiveSize {
    /// The size, `default` being the class of the nature.
    fn resolve(self, default: SizeClass) -> egui::Vec2 {
        match self {
            Self::Default => default.size(),
            Self::Class(class) => class.size(),
            Self::Free(size) => size,
        }
    }
}

/// The size of a window **without rich interaction** (choice, read-only,
/// confirmation, save, alert): no form to lay out, a badly chosen size costs
/// little there. It can therefore take the size of its content **without
/// justification** (S-F11) — which the interactive window cannot: the type
/// does not offer it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PlainSize {
    /// The default class of the nature ([`DialogNature::default_class`]).
    Default,
    Class(SizeClass),
    /// The size the content asks for.
    Content(egui::Vec2),
}

impl PlainSize {
    /// The size, `default` being the class of the nature.
    fn resolve(self, default: SizeClass) -> egui::Vec2 {
        match self {
            Self::Default => default.size(),
            Self::Class(class) => class.size(),
            Self::Content(size) => size,
        }
    }
}

/// The **nature** of a modal window (S-F11): what it is fixes its footer
/// (button order, S-F2, S-F4, S-F6), its default size, its initial focus
/// (S-F9) and the default button. Every `match` on the nature is exhaustive,
/// without `_`: a new nature does not compile before it has said all of this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum DialogNature {
    /// Live settings of a visible object: `[Cancel] [OK]`.
    Settings(InteractiveSize),
    /// Creation on a draft: `[secondary] [message] [Cancel] [verb]`.
    Draft(InteractiveSize),
    /// Choice with no "yes" answer: same footer, secondary = the other answers.
    Choice(PlainSize),
    /// Wizard: `[Cancel] [message] [Back · Next] [Finish]`.
    Wizard(InteractiveSize),
    /// Settings **applied live**, with nothing to confirm or cancel:
    /// `[message] [Close]` (S-F2). Interactive (fields to fill in: focus
    /// on the first one, size by class), but with the footer of a read-only window.
    LiveSettings(InteractiveSize),
    /// Read-only: `[secondary] [message] [Close]`.
    ReadOnly(PlainSize),
    /// Confirmation: `[message] [Cancel] [verb]`, Cancel by default.
    Confirm(PlainSize),
    /// Save?: `[Discard Changes] [message] [Cancel] [Save]`.
    SaveChanges(PlainSize),
    /// Serious failure: `[action] [message] [Close]`.
    Alert(PlainSize),
}

/// Where the focus goes on opening (S-F9, S-F11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InitialFocus {
    /// The first input field, found by the building block.
    FirstField,
    /// The default button of the footer: Cancel of a confirmation, Save, Close.
    DefaultButton,
    /// Nothing: the window is read, Enter closes.
    Nothing,
}

/// The layout of the footer buttons of a nature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FooterShape {
    /// `[secondary] [message] [Cancel] [main]`.
    Dialog,
    /// `[Cancel] [message] [Back · Next] [Finish]`.
    Wizard,
    /// `[secondary] [message] [Close]`.
    ReadOnly,
    /// `[message] [Cancel] [verb]`.
    Confirm,
    /// `[Discard Changes] [message] [Cancel] [Save]`.
    SaveChanges,
}

impl DialogNature {
    /// The footer layout (S-F2, S-F4, S-F6).
    pub(super) fn footer(self) -> FooterShape {
        match self {
            Self::Settings(_) | Self::Draft(_) | Self::Choice(_) => FooterShape::Dialog,
            Self::Wizard(_) => FooterShape::Wizard,
            Self::LiveSettings(_) | Self::ReadOnly(_) | Self::Alert(_) => FooterShape::ReadOnly,
            Self::Confirm(_) => FooterShape::Confirm,
            Self::SaveChanges(_) => FooterShape::SaveChanges,
        }
    }

    /// The size class when the caller does not say.
    pub(super) fn default_class(self) -> SizeClass {
        match self {
            Self::Settings(_) | Self::Draft(_) | Self::LiveSettings(_) | Self::ReadOnly(_) => {
                SizeClass::Medium
            }
            Self::Wizard(_) => SizeClass::Large,
            Self::Choice(_) | Self::Confirm(_) | Self::SaveChanges(_) | Self::Alert(_) => {
                SizeClass::Small
            }
        }
    }

    /// The default size of the window.
    pub(super) fn size(self) -> egui::Vec2 {
        let default = self.default_class();
        match self {
            Self::Settings(size)
            | Self::Draft(size)
            | Self::LiveSettings(size)
            | Self::Wizard(size) => size.resolve(default),
            Self::Choice(size)
            | Self::ReadOnly(size)
            | Self::Confirm(size)
            | Self::SaveChanges(size)
            | Self::Alert(size) => size.resolve(default),
        }
    }

    /// The initial focus (S-F9): a draft, settings, a wizard or a choice have
    /// fields to fill in; a confirmation, a save or an alert wait for a
    /// button; a read-only window has nothing to enter.
    pub(super) fn initial_focus(self) -> InitialFocus {
        match self {
            Self::Settings(_)
            | Self::Draft(_)
            | Self::LiveSettings(_)
            | Self::Wizard(_)
            | Self::Choice(_) => InitialFocus::FirstField,
            Self::Confirm(_) | Self::SaveChanges(_) | Self::Alert(_) => InitialFocus::DefaultButton,
            Self::ReadOnly(_) => InitialFocus::Nothing,
        }
    }

    /// Is Cancel the default button (S-F5)? Otherwise it is the main one.
    fn cancel_is_default(self) -> bool {
        match self {
            Self::Confirm(_) => true,
            Self::Settings(_)
            | Self::Draft(_)
            | Self::Choice(_)
            | Self::Wizard(_)
            | Self::LiveSettings(_)
            | Self::ReadOnly(_)
            | Self::SaveChanges(_)
            | Self::Alert(_) => false,
        }
    }
}

/// Outcome of one frame of the common body: that of a [`DialogOutcome`], plus
/// the secondary button of the building block (Discard Changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameOutcome {
    Open,
    Accept,
    Cancel,
    Secondary,
}

impl FrameOutcome {
    /// For dialogs without a building-block secondary.
    fn dialog(self) -> DialogOutcome {
        match self {
            Self::Open | Self::Secondary => DialogOutcome::Open,
            Self::Accept => DialogOutcome::Accept,
            Self::Cancel => DialogOutcome::Cancel,
        }
    }
}

/// The footer message (S-M7, S-F10): the error takes precedence over the reason
/// of the greyed-out main button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FooterMessage<'a> {
    Error(&'a str),
    Reason(&'a str),
}

/// The footer decision, pure: an error if there is one; otherwise, if the main
/// button is greyed out, its reason; otherwise nothing.
fn footer_message<'a>(
    error: Option<&'a str>,
    accept_enabled: bool,
    reason: &'a str,
) -> Option<FooterMessage<'a>> {
    match error.filter(|error| !error.is_empty()) {
        Some(error) => Some(FooterMessage::Error(error)),
        None if !accept_enabled && !reason.is_empty() => Some(FooterMessage::Reason(reason)),
        None => None,
    }
}

/// **Modal** dialog: in the foreground, the rest of the interface no longer
/// receives any click, and a click elsewhere makes the dialog's outline flash
/// dark red ([`theme::blocked_flash_color`]) to point it out. It closes through
/// "Accept" (default button, Enter outside text input) or through "Cancel"
/// (cross, Escape): see [`DialogOutcome`].
///
/// Same building block as the content windows: [`show_window_in_layer`] (frame,
/// active outline, clipping), [`window_header`] (placement
/// [`HeaderPlacement::Dialog`]) and [`floating_resize_handles`]. It differs
/// only by the layer order (`Foreground`, above the `Middle` windows) and by
/// the full-screen layer that captures clicks. Drop-down menus and the color
/// picker opened from the dialog stay usable: they are created after it, hence
/// above.
///
/// Limit: detached OS windows are other viewports, which this layer does not
/// cover.
pub(super) fn modal_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: InteractiveSize,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    // The default footer: "OK", always enabled — what the graph properties
    // use. The texts come from the catalog; the footer borrows them.
    let accept_label = tr!("dialog-ok");
    let accept_hint = tr!("dialog-ok", "hint");
    modal_dialog_with(
        ctx,
        id,
        title,
        DialogNature::Settings(size),
        DialogFooter::always(&accept_label, &accept_hint),
        |_| {},
        add_contents,
    )
}

/// The main button of a footer: either **always enabled** (it only has a
/// tooltip), or **can be greyed out** — and then both the tooltip of the
/// enabled button and the reason for the greying are mandatory (S-F10, S-G1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcceptButton<'a> {
    /// Never greyed out.
    Always { hint: &'a str },
    /// Greyed out when `enabled` is false. `reason` says why and what to do; it
    /// is written in the footer's message area **and** as the tooltip of the greyed-out button.
    Gated {
        enabled: bool,
        hint: &'a str,
        reason: &'a str,
    },
}

/// The footer of a [`modal_dialog_with`]: label and state of the default
/// button. [`modal_dialog`] puts "OK" in it, always enabled — what the graph
/// properties use.
pub(super) struct DialogFooter<'a> {
    pub(super) accept_label: &'a str,
    pub(super) accept: AcceptButton<'a>,
}

impl<'a> DialogFooter<'a> {
    /// A main button that is never greyed out.
    pub(super) fn always(label: &'a str, hint: &'a str) -> Self {
        Self {
            accept_label: label,
            accept: AcceptButton::Always { hint },
        }
    }

    /// A main button that can be greyed out, with its reason (#226).
    pub(super) fn gated(label: &'a str, hint: &'a str, enabled: bool, reason: &'a str) -> Self {
        debug_assert!(
            enabled || !reason.is_empty(),
            "a greyed-out button without a reason (S-G1)"
        );
        Self {
            accept_label: label,
            accept: AcceptButton::Gated {
                enabled,
                hint,
                reason,
            },
        }
    }

    fn enabled(&self) -> bool {
        match self.accept {
            AcceptButton::Always { .. } => true,
            AcceptButton::Gated { enabled, .. } => enabled,
        }
    }

    /// The tooltip of the enabled button.
    fn hint(&self) -> &str {
        match self.accept {
            AcceptButton::Always { hint } | AcceptButton::Gated { hint, .. } => hint,
        }
    }

    /// The reason for the greying, which is also the tooltip of the greyed-out
    /// button; a button that never is has none.
    fn disabled_reason(&self) -> &str {
        match self.accept {
            AcceptButton::Always { .. } => "",
            AcceptButton::Gated { reason, .. } => reason,
        }
    }
}

/// [`modal_dialog`] with a custom default button (`footer`) and **secondary**
/// buttons (`extra_buttons`), placed on the left of the footer, from left to
/// right, before the message area; Cancel then the main button stay on the
/// right (S-F6). `nature` is that of a dialog footer: settings, draft or
/// choice.
pub(super) fn modal_dialog_with(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    nature: DialogNature,
    footer_spec: DialogFooter<'_>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    modal_frame(
        ctx,
        id,
        title,
        nature,
        footer_spec,
        extra_buttons,
        add_contents,
    )
    .dialog()
}

/// A **wizard** (S-F6): Cancel on the left, the step buttons
/// (`step_buttons`, added **from right to left**: Next then Back) grouped on
/// the right against the final button (`footer`, Finish). Same window as
/// [`modal_dialog_with`]; the import wizard is its use.
pub(super) fn modal_wizard(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: InteractiveSize,
    footer_spec: DialogFooter<'_>,
    step_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::Wizard(size),
        footer_spec,
        step_buttons,
        add_contents,
    )
    .dialog()
}

/// A window of **settings applied live** (S-F2, S-F11): nothing to
/// confirm or cancel, hence a single button, Close, like the read-only window;
/// but it has fields, so the initial focus goes to the first one (S-F9) and the
/// size fits into a class ([`InteractiveSize`]). Same outcomes as
/// [`modal_read_only`].
pub(super) fn modal_live_settings(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: InteractiveSize,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::LiveSettings(size),
        DialogFooter::always(&label, &hint),
        |_| {},
        add_contents,
    )
    .dialog()
}

/// A modal window in **read-only** mode (S-F2): a single button, Close
/// (Enter), which returns [`DialogOutcome::Accept`]; Escape and the cross return
/// [`DialogOutcome::Cancel`]. Both close without changing anything.
pub(super) fn modal_read_only(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: PlainSize,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::ReadOnly(size),
        DialogFooter::always(&label, &hint),
        |_| {},
        add_contents,
    )
    .dialog()
}

/// [`modal_read_only`] with secondary buttons to the left of Close
/// (`[secondary] [Close]`): a read-only window that also offers a gesture
/// (#306: "Undo").
pub(super) fn modal_read_only_with(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: PlainSize,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> DialogOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    modal_frame(
        ctx,
        id,
        title,
        DialogNature::ReadOnly(size),
        DialogFooter::always(&label, &hint),
        extra_buttons,
        add_contents,
    )
    .dialog()
}

/// Outcome of one frame of [`modal_confirm`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfirmOutcome {
    /// Still open.
    Open,
    /// The verb (Delete…): only through its button, never through Enter.
    Confirm,
    /// Cancel (default button, Enter), cross or Escape.
    Cancel,
}

/// A **confirmation** (S-F3–S-F5 → S-R4, S-R5): only for what Ctrl+Z does not
/// recover. Footer `[Cancel] [verb]`: Cancel is the default button, framed,
/// and Enter as well as Escape trigger it; the verb (`verb`, its tooltip
/// `verb_hint`) has the normal style and turns red on hover and on keyboard focus.
pub(super) fn modal_confirm(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: PlainSize,
    verb: &str,
    verb_hint: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> ConfirmOutcome {
    match modal_frame(
        ctx,
        id,
        title,
        DialogNature::Confirm(size),
        DialogFooter::always(verb, verb_hint),
        |_| {},
        add_contents,
    ) {
        FrameOutcome::Open | FrameOutcome::Secondary => ConfirmOutcome::Open,
        FrameOutcome::Accept => ConfirmOutcome::Confirm,
        FrameOutcome::Cancel => ConfirmOutcome::Cancel,
    }
}

/// Outcome of one frame of [`modal_save_changes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveChangesOutcome {
    /// Still open.
    Open,
    /// Discard Changes.
    Discard,
    /// Cancel, cross or Escape.
    Cancel,
    /// Save (default button: Enter).
    Save,
}

/// The dialog for unsaved changes (S-F4): "Discard Changes · Cancel · Save".
/// Written exception to S-R4: Enter = Save, Escape = Cancel;
/// Discard Changes, on the left, turns red on hover and on keyboard focus.
pub(super) fn modal_save_changes(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: PlainSize,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> SaveChangesOutcome {
    let label = tr!("unsaved-save");
    let hint = tr!("unsaved-save", "hint");
    match modal_frame(
        ctx,
        id,
        title,
        DialogNature::SaveChanges(size),
        DialogFooter::always(&label, &hint),
        |_| {},
        add_contents,
    ) {
        FrameOutcome::Open => SaveChangesOutcome::Open,
        FrameOutcome::Accept => SaveChangesOutcome::Save,
        FrameOutcome::Cancel => SaveChangesOutcome::Cancel,
        FrameOutcome::Secondary => SaveChangesOutcome::Discard,
    }
}

/// Outcome of one frame of [`modal_alert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AlertOutcome {
    /// Still open.
    Open,
    /// Close (Enter), cross or Escape: the message is read.
    Close,
    /// The proposed action.
    Action,
}

/// A **serious failure** outside a window (S-M8): blocking dialog, the message
/// in the content, a Close button (Enter, Escape) and the possible action
/// (`action`: label and tooltip), on the left.
pub(super) fn modal_alert(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    size: PlainSize,
    action: (&str, &str),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> AlertOutcome {
    let label = tr!("dialog-close");
    let hint = tr!("dialog-close", "hint");
    let mut acted = false;
    let outcome = modal_frame(
        ctx,
        id,
        title,
        DialogNature::Alert(size),
        DialogFooter::always(&label, &hint),
        |ui| {
            acted = ui
                .add(egui::Button::new(action.0).min_size(FOOTER_BUTTON_SIZE))
                .on_hover_text(action.1)
                .clicked();
        },
        add_contents,
    );
    match outcome {
        _ if acted => AlertOutcome::Action,
        FrameOutcome::Open | FrameOutcome::Secondary => AlertOutcome::Open,
        FrameOutcome::Accept | FrameOutcome::Cancel => AlertOutcome::Close,
    }
}

/// The kind of a dialog, key of its geometry in the preferences (S-F8):
/// its egui identifier, stable from one session to the next.
fn dialog_kind(id: egui::Id) -> String {
    format!("{:016x}", id.value())
}

/// Sets the size of the open modal dialog `id`, keeping it centred on the
/// screen. Used by the manual screenshot bench (#367) to show a dialog whose
/// content would otherwise need its inner scroll bar.
#[cfg(test)]
pub(super) fn set_dialog_size(ctx: &egui::Context, id: egui::Id, size: egui::Vec2) {
    let screen = ctx.content_rect();
    // Keep the resize border inside the screen: the bench crops the frame exactly.
    let size =
        size.min(screen.size() - egui::Vec2::splat(2.0 * super::window_chrome::RESIZE_BORDER));
    ctx.data_mut(|data| {
        data.insert_temp(
            id.with("geometry"),
            DialogGeometry {
                pos: screen.center() - size / 2.0,
                size,
            },
        );
    });
}

/// The remembered geometries, copied from the preferences into egui's memory.
#[derive(Clone, Default)]
struct RememberedGeometries(BTreeMap<String, SavedGeometry>);

fn remembered_id() -> egui::Id {
    egui::Id::new("stokesforge_dialog_geometries")
}

/// The geometries of the closed dialogs, to hand back to the preferences.
#[derive(Clone, Default)]
struct ClosedGeometries(Vec<(String, SavedGeometry)>);

fn closed_id() -> egui::Id {
    egui::Id::new("stokesforge_dialog_geometries_closed")
}

/// The error of the open dialog (S-M7), published by [`sync_dialogs`].
fn dialog_error_id() -> egui::Id {
    egui::Id::new("stokesforge_dialog_error")
}

/// The link between the dialogs and the state, once per frame after the
/// dialogs: publishes the error of the open dialog for its footer (S-M7), copies
/// the remembered geometries once (S-F8) and turns into actions those of the
/// dialogs that have just closed.
pub(super) fn sync_dialogs(ctx: &egui::Context, state: &AppState, actions: &mut Vec<Action>) {
    let error = state.dialog_error().map(str::to_owned);
    let closed = ctx.data_mut(|data| {
        data.insert_temp(dialog_error_id(), error);
        if data
            .get_temp::<RememberedGeometries>(remembered_id())
            .is_none()
        {
            data.insert_temp(
                remembered_id(),
                RememberedGeometries(state.dialog_geometries().clone()),
            );
        }
        data.remove_temp::<ClosedGeometries>(closed_id())
    });
    for (kind, geometry) in closed.map(|closed| closed.0).unwrap_or_default() {
        actions.push(Action::RememberDialogGeometry { kind, geometry });
    }
}

/// The initial focus in progress (S-F9): how many widgets it has already passed,
/// and the footer buttons of the previous frame (it stops on them).
#[derive(Clone, Default)]
struct FocusWalk {
    steps: u8,
    footer: Vec<egui::Id>,
}

/// Is `id` a text field placed in `layer`?
fn text_field_in(ctx: &egui::Context, id: egui::Id, layer: egui::LayerId) -> bool {
    egui::TextEdit::load_state(ctx, id).is_some()
        && ctx
            .read_response(id)
            .is_some_and(|response| response.layer_id == layer)
}

/// Does the frame have a gesture (key, text, click)?
fn any_gesture(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key { pressed: true, .. }
                    | egui::Event::Text(_)
                    | egui::Event::PointerButton { pressed: true, .. }
            )
        })
    })
}

/// The common body of modal dialogs.
/// Memory key of the "a modal was painted" mark of one viewport.
fn modal_mark_id(ctx: &egui::Context) -> egui::Id {
    egui::Id::new(("stokesforge_modal_shown", ctx.viewport_id()))
}

/// Records that a modal window is painted in this viewport, in this frame.
fn mark_modal_shown(ctx: &egui::Context) {
    // Ids read before the lock: `viewport_id` locks the context too.
    let (frame, mark) = (ctx.cumulative_frame_nr(), modal_mark_id(ctx));
    ctx.data_mut(|data| data.insert_temp(mark, frame));
}

/// Whether a modal window is open in this viewport (#406): painted in this frame
/// or in the previous one. The workspace (grids, Project Manager) is painted
/// **before** the dialogs, so it can only read the mark of the previous frame;
/// the one-frame lag after a close is intended. Every workspace key consumer
/// (Esc, Del, F2, Enter, typing, arrows, clipboard) stands aside while this is
/// true, otherwise it takes the key from the modal. The frame number is used,
/// not the pass number: focus setup re-runs passes within one frame.
pub(super) fn modal_shown(ctx: &egui::Context) -> bool {
    let (frame, mark) = (ctx.cumulative_frame_nr(), modal_mark_id(ctx));
    ctx.data(|data| data.get_temp::<u64>(mark))
        .is_some_and(|shown| shown + 1 >= frame)
}

fn modal_frame(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    nature: DialogNature,
    footer_spec: DialogFooter<'_>,
    extra_buttons: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui),
) -> FrameOutcome {
    mark_modal_shown(ctx);
    let screen = ctx.input(|input| input.content_rect());
    let geometry_id = id.with("geometry");
    let flash_id = id.with("flash_start");
    let walk_id = id.with("initial_focus");
    let anchor_id = id.with("focus_anchor");
    let kind = dialog_kind(id);
    let stored = ctx.data(|data| data.get_temp::<DialogGeometry>(geometry_id));
    let just_opened = stored.is_none();
    let mut geometry = stored.unwrap_or_else(|| {
        let remembered = ctx
            .data(|data| data.get_temp::<RememberedGeometries>(remembered_id()))
            .and_then(|remembered| remembered.0.get(&kind).copied())
            .filter(SavedGeometry::is_valid);
        match remembered {
            Some(saved) => DialogGeometry {
                pos: egui::pos2(saved.x, saved.y),
                size: egui::vec2(saved.width, saved.height),
            },
            None => {
                let size = nature.size().min(screen.size());
                DialogGeometry {
                    pos: screen.center() - size / 2.0,
                    size,
                }
            }
        }
    });
    geometry.size = geometry
        .size
        .max(MIN_CONTENT_WINDOW_SIZE)
        .min(screen.size());
    geometry.pos = geometry
        .pos
        .clamp(screen.min, (screen.max - geometry.size).max(screen.min));
    let layer = egui::LayerId::new(egui::Order::Foreground, id);

    // S-F9: one step of the initial focus, before the widgets are placed (egui
    // follows a focus direction only during the pass where it is given).
    let focus = nature.initial_focus();
    if just_opened {
        if focus == InitialFocus::FirstField {
            ctx.data_mut(|data| data.insert_temp(walk_id, FocusWalk::default()));
        }
    } else {
        advance_initial_focus(ctx, walk_id, anchor_id, layer);
    }
    // S-F7: the Escape that leaves a text field of the dialog does not close it.
    // egui gives the focus back on Escape from the start of the pass: so we read
    // the one from the end of the previous frame.
    let text_focus_id = id.with("text_focus");
    let editing_before = ctx
        .data(|data| data.get_temp::<bool>(text_focus_id))
        .unwrap_or(false);

    // The layer that captures clicks: everything else in the interface is under it.
    // It is **invisible**: no dark veil over the rest of the application,
    // so that one keeps seeing the graph rendering while adjusting its
    // properties (issue #20). The flashing outline already says that the
    // rest is blocked.
    let backdrop = egui::Area::new(id.with("backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .sense(egui::Sense::click_and_drag())
        .show(ctx, |ui| {
            ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
        });
    let blocked_click =
        backdrop.response.contains_pointer() && ctx.input(|input| input.pointer.any_pressed());
    let now = ctx.input(|input| input.time);
    if blocked_click {
        ctx.data_mut(|data| data.insert_temp(flash_id, now));
    }
    if blocked_click || just_opened {
        ctx.move_to_top(layer);
    }
    let flash = ctx
        .data(|data| data.get_temp::<f64>(flash_id))
        .map(|start| now - start)
        .filter(|elapsed| *elapsed < MODAL_FLASH_SECONDS)
        .map(|elapsed| {
            ctx.request_repaint();
            // Two pulses over the duration: 0 → 1 → 0, twice.
            0.5 * (1.0 - (std::f64::consts::TAU * elapsed / (MODAL_FLASH_SECONDS / 2.0)).cos())
        });

    let error = ctx
        .data(|data| data.get_temp::<Option<String>>(dialog_error_id()))
        .flatten();
    let popup_open = egui::Popup::is_any_open(ctx);
    let window_rect = egui::Rect::from_min_size(geometry.pos, geometry.size);
    let mut header_actions = WindowHeaderActions::default();
    let mut clicks = FooterClicks::default();
    let mut position = geometry.pos;
    let mut size = geometry.size;
    show_window_in_layer(
        ctx,
        id,
        egui::Order::Foreground,
        window_rect,
        screen,
        true,
        |ui| {
            header_actions = window_header(ui, id, title, None, false, HeaderPlacement::Dialog);
            show_window_contents_bounded(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                let split = (full.bottom() - DIALOG_FOOTER_HEIGHT).max(full.top());
                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            full.min,
                            egui::pos2(full.right(), split),
                        ))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                body.set_clip_rect(ui.clip_rect().intersect(body.max_rect()));
                // The anchor of the initial focus (S-F9): invisible, at the head of the content.
                if focus == InitialFocus::FirstField {
                    body.interact(
                        egui::Rect::from_min_size(body.max_rect().min, egui::Vec2::ZERO),
                        anchor_id,
                        egui::Sense::focusable_noninteractive(),
                    );
                }
                add_contents(&mut body);
                let mut footer = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            egui::pos2(full.left(), split),
                            full.max,
                        ))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                footer.painter().hline(
                    full.x_range(),
                    split,
                    footer.visuals().widgets.noninteractive.bg_stroke,
                );
                let message = footer_message(
                    error.as_deref(),
                    footer_spec.enabled(),
                    footer_spec.disabled_reason(),
                );
                let focus_default = just_opened && focus == InitialFocus::DefaultButton;
                clicks = paint_footer(
                    &mut footer,
                    nature,
                    &footer_spec,
                    message,
                    focus_default,
                    extra_buttons,
                );
            });
        },
        |ui, rect| {
            floating_resize_handles(ui, id, rect, &mut position, &mut size);
            if let Some(intensity) = flash {
                let style = ui.style();
                ui.painter().rect_stroke(
                    rect,
                    style.visuals.window_corner_radius,
                    egui::Stroke::new(
                        2.5,
                        theme::blocked_flash_color().gamma_multiply(intensity as f32),
                    ),
                    egui::StrokeKind::Inside,
                );
            }
        },
    );
    position += header_actions.drag_delta;
    geometry = DialogGeometry {
        pos: position,
        size,
    };
    let editing_now = ctx
        .memory(|memory| memory.focused())
        .is_some_and(|focused| text_field_in(ctx, focused, layer));
    ctx.data_mut(|data| {
        data.insert_temp(geometry_id, geometry);
        data.insert_temp(text_focus_id, editing_now);
    });
    if let Some(mut walk) = ctx.data(|data| data.get_temp::<FocusWalk>(walk_id)) {
        // The anchor takes the focus on opening; the footer bounds the search.
        if just_opened {
            ctx.memory_mut(|memory| memory.request_focus(anchor_id));
        }
        walk.footer = clicks.ids.clone();
        ctx.data_mut(|data| data.insert_temp(walk_id, walk));
        // The focus steps follow one another in the same frame: egui redoes the
        // pass (without painting it) instead of showing the focus jumping from
        // widget to widget. The cap on passes is raised accordingly.
        ctx.options_mut(|options| {
            options.max_passes = std::num::NonZeroUsize::new(usize::from(FOCUS_WALK_STEPS) + 2)
                .unwrap_or(options.max_passes);
        });
        ctx.request_discard("initial focus");
        ctx.request_repaint();
    }

    // Escape: consumed by the color picker or an open list (they read it
    // before), otherwise it cancels — except the one that has just left a text
    // field (S-F7). Enter triggers the default button, except during a text
    // input (where it ends the input): the main one, or Cancel of a
    // confirmation (S-R4). A footer button with keyboard focus receives Enter
    // itself: its click takes precedence.
    let escape = !popup_open
        && ctx.input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_ESCAPE))
        && !editing_before;
    let enter = !popup_open
        && !keyboard_focus_outside_grid(ctx)
        && ctx.input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_ENTER));
    let outcome = if header_actions.close || escape {
        FrameOutcome::Cancel
    } else if clicks.accept {
        FrameOutcome::Accept
    } else if clicks.secondary {
        FrameOutcome::Secondary
    } else if clicks.cancel || (enter && nature.cancel_is_default()) {
        FrameOutcome::Cancel
    } else if enter && footer_spec.enabled() {
        FrameOutcome::Accept
    } else {
        FrameOutcome::Open
    };
    if outcome != FrameOutcome::Open {
        let saved = SavedGeometry {
            x: geometry.pos.x,
            y: geometry.pos.y,
            width: geometry.size.x,
            height: geometry.size.y,
        };
        ctx.data_mut(|data| {
            data.remove::<DialogGeometry>(geometry_id);
            data.remove::<f64>(flash_id);
            data.remove::<FocusWalk>(walk_id);
            data.remove::<bool>(text_focus_id);
            let remembered = data.get_temp_mut_or_default::<RememberedGeometries>(remembered_id());
            remembered.0.insert(kind.clone(), saved);
            data.get_temp_mut_or_default::<ClosedGeometries>(closed_id())
                .0
                .push((kind, saved));
        });
    }
    outcome
}

/// The footer buttons clicked in the frame, and their identifiers (the initial
/// focus stops at the footer).
#[derive(Default)]
struct FooterClicks {
    accept: bool,
    cancel: bool,
    /// Discard Changes (the only secondary placed by the building block).
    secondary: bool,
    ids: Vec<egui::Id>,
}

/// The footer of a dialog, in `footer` (right-to-left layout): buttons
/// arranged according to `layout` (S-F2, S-F6) and a message area to the left
/// of the buttons (S-M7, S-F10).
fn paint_footer(
    footer: &mut egui::Ui,
    nature: DialogNature,
    spec: &DialogFooter<'_>,
    message: Option<FooterMessage<'_>>,
    focus_default: bool,
    extra_buttons: impl FnOnce(&mut egui::Ui),
) -> FooterClicks {
    let mut clicks = FooterClicks::default();
    let default_fill = footer.visuals().selection.bg_fill;
    // First added = rightmost: the main button — default, except in a
    // confirmation, where it is destructive and Cancel is the default (S-R4).
    let confirm = nature.cancel_is_default();
    let response = footer
        .scope(|ui| {
            let mut accept = egui::Button::new(spec.accept_label).min_size(FOOTER_BUTTON_SIZE);
            if confirm {
                theme::destructive_button_visuals(ui.visuals_mut());
            } else {
                accept = accept.fill(default_fill);
            }
            ui.add_enabled(spec.enabled(), accept)
                .on_hover_text(spec.hint())
                .on_disabled_hover_text(spec.disabled_reason())
        })
        .inner;
    // S-F11: the default button takes the focus on opening (the main one;
    // Cancel in a confirmation, further down).
    if focus_default && !confirm {
        response.request_focus();
    }
    clicks.ids.push(response.id);
    clicks.accept = response.clicked();
    let cancel = |ui: &mut egui::Ui, clicks: &mut FooterClicks| {
        let mut button = egui::Button::new(tr!("dialog-cancel")).min_size(FOOTER_BUTTON_SIZE);
        if confirm {
            button = button.fill(default_fill);
        }
        let response = ui.add(button).on_hover_text(tr!("dialog-cancel", "hint"));
        if focus_default && confirm {
            response.request_focus();
        }
        clicks.ids.push(response.id);
        clicks.cancel = response.clicked();
    };
    match nature.footer() {
        FooterShape::Dialog => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                extra_buttons(ui);
                paint_footer_message(ui, message);
            });
        }
        FooterShape::Confirm => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                paint_footer_message(ui, message);
            });
        }
        FooterShape::SaveChanges => {
            cancel(footer, &mut clicks);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let response = ui
                    .scope(|ui| {
                        theme::destructive_button_visuals(ui.visuals_mut());
                        ui.add(
                            egui::Button::new(tr!("unsaved-discard")).min_size(FOOTER_BUTTON_SIZE),
                        )
                    })
                    .inner
                    .on_hover_text(tr!("unsaved-discard", "hint"));
                clicks.ids.push(response.id);
                clicks.secondary = response.clicked();
                paint_footer_message(ui, message);
            });
        }
        FooterShape::Wizard => {
            extra_buttons(footer);
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                cancel(ui, &mut clicks);
                paint_footer_message(ui, message);
            });
        }
        FooterShape::ReadOnly => {
            footer.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                extra_buttons(ui);
                paint_footer_message(ui, message);
            });
        }
    }
    clicks
}

/// The footer message area: the error in the error color, the reason for a
/// greying faded; cut to the space that remains (full text on hover).
fn paint_footer_message(ui: &mut egui::Ui, message: Option<FooterMessage<'_>>) {
    let text = match message {
        Some(FooterMessage::Error(error)) => {
            egui::RichText::new(error).color(ui.visuals().error_fg_color)
        }
        Some(FooterMessage::Reason(reason)) => egui::RichText::new(reason).weak(),
        None => return,
    };
    ui.add(egui::Label::new(text).truncate());
}

/// One step of the initial focus (S-F9), at the start of a frame: see the
/// module doc. A gesture interrupts it before the widgets see the focus; a
/// focus set along the way on something other than a text field is then given back.
fn advance_initial_focus(
    ctx: &egui::Context,
    walk_id: egui::Id,
    anchor_id: egui::Id,
    layer: egui::LayerId,
) {
    let Some(walk) = ctx.data(|data| data.get_temp::<FocusWalk>(walk_id)) else {
        return;
    };
    let focused = ctx.memory(|memory| memory.focused());
    let stop = |ctx: &egui::Context, release: Option<egui::Id>| {
        if let Some(focused) = release {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }
        ctx.data_mut(|data| data.remove::<FocusWalk>(walk_id));
    };
    match focused {
        Some(focused) if text_field_in(ctx, focused, layer) => stop(ctx, None),
        _ if any_gesture(ctx) => stop(ctx, focused),
        Some(focused)
            if walk.steps < FOCUS_WALK_STEPS
                && (focused == anchor_id
                    || (!walk.footer.contains(&focused)
                        && ctx
                            .read_response(focused)
                            .is_some_and(|response| response.layer_id == layer))) =>
        {
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::Next));
            ctx.data_mut(|data| {
                data.insert_temp(
                    walk_id,
                    FocusWalk {
                        steps: walk.steps + 1,
                        ..walk
                    },
                );
            });
        }
        other => stop(ctx, other),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{frame_of, key, texts, Text};
    use super::*;

    /// The center of the painted text `wanted`.
    fn center(painted: &[Text], wanted: &str) -> egui::Pos2 {
        painted
            .iter()
            .find(|t| t.text == wanted)
            .unwrap_or_else(|| panic!("{wanted:?} not painted"))
            .center
    }

    /// A draft nature, of the free size used by the tests.
    fn draft() -> DialogNature {
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(600.0, 300.0)))
    }

    /// A wizard nature, of the free size used by the tests.
    fn wizard() -> DialogNature {
        DialogNature::Wizard(InteractiveSize::Free(egui::vec2(600.0, 300.0)))
    }

    /// A dialog of `nature` whose main button is `primary` (greyed out with the
    /// reason `reason` if `enabled` is false), plus an extra button "Other";
    /// returns the outcome of the frame and its texts.
    fn footer_frame(
        context: &egui::Context,
        nature: DialogNature,
        enabled: bool,
        events: Vec<egui::Event>,
        mut contents: impl FnMut(&mut egui::Ui),
    ) -> (DialogOutcome, Vec<Text>) {
        let mut outcome = DialogOutcome::Open;
        let output = frame_of(context, egui::vec2(800.0, 600.0), events, |ui| {
            outcome = modal_frame(
                ui.ctx(),
                egui::Id::new("dialog_footer"),
                "Title",
                nature,
                DialogFooter::gated("Main", "Tooltip", enabled, "Reason"),
                |ui| {
                    let _ = ui.button("Other");
                },
                |ui| {
                    contents(ui);
                },
            )
            .dialog();
        });
        (outcome, texts(&output.shapes))
    }

    /// S-F6: a dialog puts the secondary button on the left, then Cancel, then
    /// the main one on the right; a wizard puts Cancel on the far left and groups
    /// its step buttons against the main one. Read from the painted shapes.
    #[test]
    fn the_button_order_follows_the_kind_of_dialog() {
        let cancel_label = tr!("dialog-cancel");
        let context = egui::Context::default();
        let mut painted = Vec::new();
        for _ in 0..3 {
            painted = footer_frame(&context, draft(), true, Vec::new(), |_| {}).1;
        }
        let (other, cancel, primary) = (
            center(&painted, "Other"),
            center(&painted, &cancel_label),
            center(&painted, "Main"),
        );
        assert!(other.x < cancel.x && cancel.x < primary.x, "dialog");
        assert!((other.y - primary.y).abs() < 1.0, "a single row");

        let context = egui::Context::default();
        for _ in 0..3 {
            painted = footer_frame(&context, wizard(), true, Vec::new(), |_| {}).1;
        }
        let (other, cancel, primary) = (
            center(&painted, "Other"),
            center(&painted, &cancel_label),
            center(&painted, "Main"),
        );
        assert!(cancel.x < other.x && other.x < primary.x, "wizard");
        assert!(
            primary.x - other.x < other.x - cancel.x,
            "the step buttons are grouped against the main one"
        );
    }

    /// S-F2: a read-only window has only one button, Close, which Enter
    /// triggers; no Cancel.
    #[test]
    fn a_read_only_window_has_only_close() {
        let context = egui::Context::default();
        let id = egui::Id::new("read_only");
        let mut outcome = DialogOutcome::Open;
        let mut painted = Vec::new();
        for events in [vec![], vec![], vec![], vec![key(egui::Key::Enter)]] {
            let output = frame_of(&context, egui::vec2(800.0, 600.0), events, |ui| {
                outcome = modal_read_only(
                    ui.ctx(),
                    id,
                    "Title",
                    PlainSize::Content(egui::vec2(300.0, 200.0)),
                    |ui| {
                        ui.label("content");
                    },
                );
            });
            if outcome == DialogOutcome::Open {
                painted = texts(&output.shapes);
            }
        }
        assert!(painted.iter().any(|t| t.text == tr!("dialog-close")));
        assert!(!painted.iter().any(|t| t.text == tr!("dialog-cancel")));
        assert_eq!(outcome, DialogOutcome::Accept, "Enter closes");
    }

    /// S-M7, S-F10 (pure decision): the error takes precedence over the reason;
    /// the reason is only given with the button greyed out; with neither, nothing.
    #[test]
    fn the_error_takes_precedence_over_the_reason_for_greying() {
        assert_eq!(
            footer_message(Some("Error"), false, "Reason"),
            Some(FooterMessage::Error("Error"))
        );
        assert_eq!(
            footer_message(None, false, "Reason"),
            Some(FooterMessage::Reason("Reason"))
        );
        assert_eq!(footer_message(None, true, "Reason"), None);
        assert_eq!(footer_message(Some(""), false, ""), None);
    }

    /// S-F10, S-M7 painted: main button greyed out, its reason is in the footer;
    /// a published error replaces it, and the reason comes back when the error
    /// disappears. Witness: enabled button, the reason is not painted.
    #[test]
    fn the_footer_shows_the_error_then_the_reason_for_greying() {
        let context = egui::Context::default();
        let mut painted = Vec::new();
        for _ in 0..3 {
            painted = footer_frame(&context, draft(), false, Vec::new(), |_| {}).1;
        }
        assert!(painted.iter().any(|t| t.text == "Reason"));
        context.data_mut(|data| {
            data.insert_temp(dialog_error_id(), Some("Computation failed".to_string()));
        });
        painted = footer_frame(&context, draft(), false, Vec::new(), |_| {}).1;
        assert!(painted.iter().any(|t| t.text == "Computation failed"));
        assert!(!painted.iter().any(|t| t.text == "Reason"));
        context.data_mut(|data| data.insert_temp(dialog_error_id(), None::<String>));
        painted = footer_frame(&context, draft(), false, Vec::new(), |_| {}).1;
        assert!(
            painted.iter().any(|t| t.text == "Reason"),
            "the reason comes back"
        );

        let enabled = egui::Context::default();
        for _ in 0..3 {
            painted = footer_frame(&enabled, draft(), true, Vec::new(), |_| {}).1;
        }
        assert!(!painted.iter().any(|t| t.text == "Reason"), "witness");
    }

    /// A dialog whose content is a checkbox then a text field.
    fn dialog_with_field(
        context: &egui::Context,
        text: &mut String,
        events: Vec<egui::Event>,
    ) -> (DialogOutcome, Option<egui::Id>) {
        let mut field = None;
        let mut checked = false;
        let outcome = footer_frame(context, draft(), true, events, |ui| {
            ui.checkbox(&mut checked, "checkbox");
            field = Some(ui.text_edit_singleline(text).id);
        })
        .0;
        (outcome, field)
    }

    /// S-F9: on opening, the first input field takes the focus, even behind a
    /// checkbox; S-F7: the first Escape leaves the field without closing, the
    /// second does Cancel.
    #[test]
    fn the_first_field_has_the_focus_and_the_first_escape_only_leaves_it() {
        let context = egui::Context::default();
        let mut text = String::from("abc");
        let mut field = None;
        for _ in 0..8 {
            field = dialog_with_field(&context, &mut text, Vec::new()).1;
        }
        let field = field.expect("field painted");
        assert_eq!(context.memory(|memory| memory.focused()), Some(field));
        let (outcome, _) = dialog_with_field(&context, &mut text, vec![key(egui::Key::Escape)]);
        assert_eq!(
            outcome,
            DialogOutcome::Open,
            "the first Escape leaves the field"
        );
        assert_ne!(context.memory(|memory| memory.focused()), Some(field));
        let (outcome, _) = dialog_with_field(&context, &mut text, vec![key(egui::Key::Escape)]);
        assert_eq!(outcome, DialogOutcome::Cancel, "the second one cancels");
    }

    /// S-F9: the focus steps follow one another within the frame (discarded
    /// passes), not one widget per frame — here two frames suffice behind a checkbox.
    #[test]
    fn the_initial_focus_does_not_take_one_frame_per_widget() {
        let context = egui::Context::default();
        let mut text = String::from("abc");
        let mut field = None;
        for _ in 0..2 {
            field = dialog_with_field(&context, &mut text, Vec::new()).1;
        }
        assert_eq!(
            context.memory(|memory| memory.focused()),
            Some(field.expect("field painted"))
        );
    }

    /// S-F9, without an input field: the initial focus leaves nothing
    /// focused (Enter still confirms). A gesture during the search
    /// interrupts it: the key acts with no focus.
    #[test]
    fn without_a_field_the_initial_focus_leaves_nothing_focused() {
        let context = egui::Context::default();
        for _ in 0..8 {
            footer_frame(&context, draft(), true, Vec::new(), |ui| {
                ui.label("content");
            });
        }
        assert_eq!(context.memory(|memory| memory.focused()), None);

        let context = egui::Context::default();
        footer_frame(&context, draft(), true, Vec::new(), |_| {});
        let (outcome, _) =
            footer_frame(&context, draft(), true, vec![key(egui::Key::Enter)], |_| {});
        assert_eq!(
            outcome,
            DialogOutcome::Accept,
            "Enter acts during the search"
        );
    }

    /// S-F8: closed, a dialog hands back its geometry (for the preferences) and
    /// takes it again on reopening, even with another default size.
    #[test]
    fn a_reopened_dialog_takes_back_its_size_and_position() {
        let context = egui::Context::default();
        let id = egui::Id::new("remembered_dialog");
        let mut size = egui::vec2(300.0, 200.0);
        let open = |events: Vec<egui::Event>, size: egui::Vec2| {
            let mut outcome = DialogOutcome::Open;
            frame_of(&context, egui::vec2(800.0, 600.0), events, |ui| {
                outcome = modal_dialog(ui.ctx(), id, "Title", InteractiveSize::Free(size), |ui| {
                    ui.label("content");
                });
            });
            outcome
        };
        open(Vec::new(), size);
        assert_eq!(
            open(vec![key(egui::Key::Escape)], size),
            DialogOutcome::Cancel
        );
        let closed_geometries = context
            .data(|data| data.get_temp::<ClosedGeometries>(closed_id()))
            .unwrap_or_default()
            .0;
        assert_eq!(closed_geometries.len(), 1);
        let (kind, returned) = &closed_geometries[0];
        assert_eq!(kind, &dialog_kind(id));
        assert_eq!((returned.width, returned.height), (300.0, 200.0));

        // A remembered geometry (coming from the preferences) takes precedence
        // over the default size.
        let remembered = SavedGeometry {
            x: 40.0,
            y: 50.0,
            width: 420.0,
            height: 260.0,
        };
        context.data_mut(|data| {
            data.insert_temp(
                remembered_id(),
                RememberedGeometries(BTreeMap::from([(dialog_kind(id), remembered)])),
            );
        });
        size = egui::vec2(500.0, 500.0);
        open(Vec::new(), size);
        let opened = context
            .data(|data| data.get_temp::<DialogGeometry>(id.with("geometry")))
            .expect("open");
        assert_eq!(opened.pos, egui::pos2(40.0, 50.0));
        assert_eq!(opened.size, egui::vec2(420.0, 260.0));
    }

    /// A confirmation (#229): one frame with `events`; returns the outcome and
    /// the painted shapes.
    fn confirmation(
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (ConfirmOutcome, egui::FullOutput) {
        let mut outcome = ConfirmOutcome::Open;
        let output = frame_of(context, egui::vec2(800.0, 600.0), events, |ui| {
            outcome = modal_confirm(
                ui.ctx(),
                egui::Id::new("confirmation"),
                "Delete template",
                PlainSize::Content(egui::vec2(400.0, 160.0)),
                "Delete",
                "Irreversible",
                |ui| {
                    ui.label("Delete \"A\"?");
                },
            );
        });
        (outcome, output)
    }

    /// The "Discard Changes · Cancel · Save" trio: one frame with `events`.
    fn trio(
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (SaveChangesOutcome, egui::FullOutput) {
        let mut outcome = SaveChangesOutcome::Open;
        let output = frame_of(context, egui::vec2(800.0, 600.0), events, |ui| {
            outcome = modal_save_changes(
                ui.ctx(),
                egui::Id::new("trio"),
                "Changes",
                PlainSize::Content(egui::vec2(400.0, 160.0)),
                |ui| {
                    ui.label("Save?");
                },
            );
        });
        (outcome, output)
    }

    /// Clicks `wanted` (a text painted by `paint`) over three frames; returns
    /// the first outcome other than `still_open`.
    fn click_text<T: PartialEq + Copy>(
        context: &egui::Context,
        wanted: &str,
        still_open: T,
        paint: impl Fn(&egui::Context, Vec<egui::Event>) -> (T, egui::FullOutput),
    ) -> T {
        let (_, output) = paint(context, Vec::new());
        let pos = center(&texts(&output.shapes), wanted);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        [
            vec![egui::Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
        ]
        .into_iter()
        .map(|events| paint(context, events).0)
        .find(|outcome| *outcome != still_open)
        .unwrap_or(still_open)
    }

    /// S-R4 (#229): in a confirmation, Enter **and** Escape do Cancel —
    /// the default button; the verb is only obtained through its button
    /// (witness: the click confirms).
    #[test]
    fn a_confirmation_cancels_with_enter_as_with_escape() {
        for key_code in [egui::Key::Enter, egui::Key::Escape] {
            let context = egui::Context::default();
            for _ in 0..3 {
                confirmation(&context, Vec::new());
            }
            assert_eq!(
                confirmation(&context, vec![key(key_code)]).0,
                ConfirmOutcome::Cancel,
                "{key_code:?}"
            );
        }
        let context = egui::Context::default();
        for _ in 0..3 {
            confirmation(&context, Vec::new());
        }
        assert_eq!(
            click_text(&context, "Delete", ConfirmOutcome::Open, confirmation),
            ConfirmOutcome::Confirm
        );
    }

    /// S-F4, written exception to S-R4: in "Discard Changes · Cancel · Save",
    /// Enter = Save, Escape = Cancel; Discard Changes through its button, to
    /// the left of Cancel and Save.
    #[test]
    fn the_trio_saves_with_enter_and_cancels_with_escape() {
        for (key_code, expected) in [
            (egui::Key::Enter, SaveChangesOutcome::Save),
            (egui::Key::Escape, SaveChangesOutcome::Cancel),
        ] {
            let context = egui::Context::default();
            for _ in 0..3 {
                trio(&context, Vec::new());
            }
            assert_eq!(
                trio(&context, vec![key(key_code)]).0,
                expected,
                "{key_code:?}"
            );
        }
        let context = egui::Context::default();
        let mut output = None;
        for _ in 0..3 {
            output = Some(trio(&context, Vec::new()).1);
        }
        let painted = texts(&output.expect("painted").shapes);
        let (discard, cancel_label, save) = (
            center(&painted, &tr!("unsaved-discard")),
            center(&painted, &tr!("dialog-cancel")),
            center(&painted, &tr!("unsaved-save")),
        );
        assert!(discard.x < cancel_label.x && cancel_label.x < save.x);
        assert_eq!(
            click_text(
                &context,
                &tr!("unsaved-discard"),
                SaveChangesOutcome::Open,
                trio
            ),
            SaveChangesOutcome::Discard
        );
    }

    /// Was a filled rectangle of `color` painted?
    fn has_filled_rect(output: &egui::FullOutput, color: egui::Color32) -> bool {
        output
            .shapes
            .iter()
            .any(|painted| matches!(&painted.shape, egui::Shape::Rect(rect) if rect.fill == color))
    }

    /// S-R5 (#229): the destructive verb has the normal style at rest, and turns
    /// red on hover **and** on keyboard focus (Tab), pointer elsewhere.
    #[test]
    fn the_destructive_verb_turns_red_on_hover_and_focus() {
        let context = egui::Context::default();
        let red = context.global_style().visuals.error_fg_color;
        let mut output = None;
        for _ in 0..3 {
            output = Some(confirmation(&context, Vec::new()).1);
        }
        let output = output.expect("painted");
        assert!(!has_filled_rect(&output, red), "at rest: normal style");
        let verb = center(&texts(&output.shapes), "Delete");
        let hovered = confirmation(&context, vec![egui::Event::PointerMoved(verb)]).1;
        assert!(has_filled_rect(&hovered, red), "hover");
        let elsewhere = egui::pos2(5.0, 5.0);
        let resting = confirmation(&context, vec![egui::Event::PointerMoved(elsewhere)]).1;
        assert!(!has_filled_rect(&resting, red), "witness: pointer gone");
        let focus = (0..6).any(|_| {
            // Cancel has the focus on opening (S-F11): the verb is its
            // previous neighbor, Shift + Tab leads there.
            let back = egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            };
            let output = confirmation(&context, vec![back]).1;
            has_filled_rect(&confirmation(&context, Vec::new()).1, red)
                || has_filled_rect(&output, red)
        });
        assert!(focus, "keyboard focus");
    }

    /// S-M8: the serious failure closes with Enter (Close); its action is a
    /// separate button.
    #[test]
    fn a_serious_failure_closes_with_enter_and_offers_its_action() {
        let alert = |context: &egui::Context, events: Vec<egui::Event>| {
            let mut outcome = AlertOutcome::Open;
            let output = frame_of(context, egui::vec2(800.0, 600.0), events, |ui| {
                outcome = modal_alert(
                    ui.ctx(),
                    egui::Id::new("alert"),
                    "Failure",
                    PlainSize::Content(egui::vec2(400.0, 160.0)),
                    ("Retry", "Help"),
                    |ui| {
                        ui.label("Failed");
                    },
                );
            });
            (outcome, output)
        };
        let context = egui::Context::default();
        for _ in 0..3 {
            alert(&context, Vec::new());
        }
        assert_eq!(
            alert(&context, vec![key(egui::Key::Enter)]).0,
            AlertOutcome::Close
        );
        let context = egui::Context::default();
        for _ in 0..3 {
            alert(&context, Vec::new());
        }
        assert_eq!(
            click_text(&context, "Retry", AlertOutcome::Open, alert),
            AlertOutcome::Action
        );
    }

    /// S-F11: each nature says its footer, its default size (a class), its
    /// initial focus; the size is that of the field when the caller gives one.
    /// Classes: Small 480 x 170, Medium 480 x 420, Large 640 x 500.
    #[test]
    fn each_nature_fixes_its_footer_size_and_focus() {
        use InitialFocus::{DefaultButton, FirstField, Nothing};
        let small = egui::vec2(480.0, 170.0);
        let medium = egui::vec2(480.0, 420.0);
        let large = egui::vec2(640.0, 500.0);
        let interactive = InteractiveSize::Default;
        let plain = PlainSize::Default;
        for (nature, shape, size, focus) in [
            (
                DialogNature::Settings(interactive),
                FooterShape::Dialog,
                medium,
                FirstField,
            ),
            (
                DialogNature::Draft(interactive),
                FooterShape::Dialog,
                medium,
                FirstField,
            ),
            (
                DialogNature::Choice(plain),
                FooterShape::Dialog,
                small,
                FirstField,
            ),
            (
                DialogNature::Wizard(interactive),
                FooterShape::Wizard,
                large,
                FirstField,
            ),
            (
                DialogNature::LiveSettings(interactive),
                FooterShape::ReadOnly,
                medium,
                FirstField,
            ),
            (
                DialogNature::ReadOnly(plain),
                FooterShape::ReadOnly,
                medium,
                Nothing,
            ),
            (
                DialogNature::Confirm(plain),
                FooterShape::Confirm,
                small,
                DefaultButton,
            ),
            (
                DialogNature::SaveChanges(plain),
                FooterShape::SaveChanges,
                small,
                DefaultButton,
            ),
            (
                DialogNature::Alert(plain),
                FooterShape::ReadOnly,
                small,
                DefaultButton,
            ),
        ] {
            assert_eq!(nature.footer(), shape, "{nature:?}: footer");
            assert_eq!(nature.size(), size, "{nature:?}: default size");
            assert_eq!(nature.initial_focus(), focus, "{nature:?}: focus");
        }
        // The field of the variant takes precedence over the default class.
        assert_eq!(
            DialogNature::Choice(PlainSize::Class(SizeClass::Large)).size(),
            large
        );
        assert_eq!(
            DialogNature::Choice(PlainSize::Content(egui::vec2(560.0, 170.0))).size(),
            egui::vec2(560.0, 170.0)
        );
        assert_eq!(
            DialogNature::Draft(InteractiveSize::Free(egui::vec2(1000.0, 660.0))).size(),
            egui::vec2(1000.0, 660.0)
        );
        assert_eq!(
            DialogNature::Settings(InteractiveSize::Class(SizeClass::Small)).size(),
            small
        );
    }

    /// Is the focused widget the painted button `text`?
    fn focus_is_on_button(context: &egui::Context, painted: &[Text], text: &str) -> bool {
        let Some(focused) = context.memory(|memory| memory.focused()) else {
            return false;
        };
        context
            .read_response(focused)
            .is_some_and(|response| response.rect.contains(center(painted, text)))
    }

    /// S-F11: on opening, a confirmation gives the focus to **Cancel**, a
    /// save and an alert to their main button — never to a field,
    /// even if the content has one; a read-only window gives it to nothing.
    /// Witness: a draft gives it to the field.
    #[test]
    fn the_initial_focus_follows_the_nature() {
        let mut text = String::from("abc");
        let open = |nature: DialogNature, text: &mut String| {
            let context = egui::Context::default();
            let mut painted = Vec::new();
            let mut field = None;
            for _ in 0..6 {
                let output = frame_of(&context, egui::vec2(800.0, 600.0), Vec::new(), |ui| {
                    modal_frame(
                        ui.ctx(),
                        egui::Id::new("nature_focus"),
                        "Title",
                        nature,
                        DialogFooter::always("Main", "Tooltip"),
                        |_| {},
                        |ui| {
                            field = Some(ui.text_edit_singleline(text).id);
                        },
                    );
                });
                painted = texts(&output.shapes);
            }
            (context, painted, field)
        };
        let plain = PlainSize::Default;

        let (context, painted, field) = open(DialogNature::Confirm(plain), &mut text);
        assert!(
            focus_is_on_button(&context, &painted, &tr!("dialog-cancel")),
            "Cancel"
        );
        assert_ne!(context.memory(|memory| memory.focused()), field);

        for nature in [DialogNature::SaveChanges(plain), DialogNature::Alert(plain)] {
            let (context, painted, field) = open(nature, &mut text);
            assert!(focus_is_on_button(&context, &painted, "Main"), "{nature:?}");
            assert_ne!(context.memory(|memory| memory.focused()), field);
        }

        let (context, _, _) = open(DialogNature::ReadOnly(plain), &mut text);
        assert_eq!(context.memory(|memory| memory.focused()), None, "read-only");

        // Live settings have fields: focus on the first one, like
        // a draft, despite the "Close only" footer of a read-only window.
        let (context, _, field) = open(
            DialogNature::LiveSettings(InteractiveSize::Default),
            &mut text,
        );
        assert_eq!(
            context.memory(|memory| memory.focused()),
            field,
            "live settings: the first field"
        );

        let (context, _, field) = open(DialogNature::Draft(InteractiveSize::Default), &mut text);
        assert_eq!(
            context.memory(|memory| memory.focused()),
            field,
            "witness: the draft goes to the field"
        );
    }

    /// S-F10, S-F11: a greyed-out "can be greyed out" footer shows its **reason**,
    /// never the tooltip of the enabled button; an "always enabled" footer has no
    /// reason. (The tooltip is only painted on hover: so we read the footer.)
    #[test]
    fn a_greyed_out_footer_shows_its_reason_and_not_its_tooltip() {
        let context = egui::Context::default();
        let mut painted = Vec::new();
        for _ in 0..3 {
            painted = footer_frame(&context, draft(), false, Vec::new(), |_| {}).1;
        }
        assert!(painted.iter().any(|t| t.text == "Reason"));
        assert!(!painted.iter().any(|t| t.text == "Tooltip"));
        let greyed = DialogFooter::gated("P", "Tooltip", false, "Reason");
        assert!(!greyed.enabled());
        assert_eq!(greyed.disabled_reason(), "Reason");
        assert_eq!(greyed.hint(), "Tooltip");
        let always = DialogFooter::always("P", "Tooltip");
        assert!(always.enabled());
        assert_eq!(always.disabled_reason(), "", "witness: never a reason");
        assert_eq!(
            footer_message(None, always.enabled(), always.disabled_reason()),
            None
        );
    }

    /// A 800 × 600 frame with `events`, where `ui` draws the background.
    fn image(context: &egui::Context, events: Vec<egui::Event>, ui: impl FnMut(&mut egui::Ui)) {
        frame_of(context, egui::vec2(800.0, 600.0), events, ui);
    }

    /// #14: while a modal dialog is open, a click on the rest of the
    /// interface has no effect and triggers the outline flashing;
    /// Escape closes it; without a dialog, the same click acts (witness). A click
    /// **in** the dialog, on the other hand, does not flash.
    #[test]
    fn a_modal_dialog_blocks_the_background_and_flashes() {
        let id = egui::Id::new("test_dialog");
        let clicks = std::cell::Cell::new(0);
        let closed = std::cell::Cell::new(DialogOutcome::Open);
        let run = |context: &egui::Context, modal: bool, events: Vec<egui::Event>| {
            image(context, events, |ui| {
                if ui.button("background").clicked() {
                    clicks.set(clicks.get() + 1);
                }
                if modal {
                    let close = modal_dialog(
                        ui.ctx(),
                        id,
                        "Test",
                        InteractiveSize::Free(egui::vec2(300.0, 200.0)),
                        |ui| {
                            ui.label("content");
                        },
                    );
                    if close != DialogOutcome::Open {
                        closed.set(close);
                    }
                }
            });
        };
        let click = |context: &egui::Context, modal: bool, at: egui::Pos2| {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            run(context, modal, vec![egui::Event::PointerMoved(at)]);
            run(context, modal, vec![button(true)]);
            run(context, modal, vec![button(false)]);
        };
        let background = egui::pos2(20.0, 14.0);
        let flash = |context: &egui::Context| {
            context.data(|data| data.get_temp::<f64>(id.with("flash_start")))
        };

        let free = egui::Context::default();
        run(&free, false, vec![]);
        click(&free, false, background);
        assert_eq!(clicks.get(), 1, "without a dialog, the button responds");

        let context = egui::Context::default();
        for _ in 0..3 {
            run(&context, true, vec![]);
        }
        // The dialog is centered on the screen: its middle is in its body.
        click(&context, true, egui::pos2(400.0, 300.0));
        assert!(
            flash(&context).is_none(),
            "a click in the dialog is not a blocked click"
        );
        assert_eq!(closed.get(), DialogOutcome::Open);
        click(&context, true, background);
        assert_eq!(clicks.get(), 1, "the dialog blocked the click");
        assert!(
            flash(&context).is_some(),
            "the blocked click triggers the flashing"
        );
        assert_eq!(closed.get(), DialogOutcome::Open);
        run(&context, true, vec![key(egui::Key::Escape)]);
        assert_eq!(closed.get(), DialogOutcome::Cancel, "Escape cancels");
        assert!(
            flash(&context).is_none(),
            "closed, the dialog forgets its flashing"
        );

        // Accept is the default button: Enter (outside text input).
        closed.set(DialogOutcome::Open);
        let again = egui::Context::default();
        for _ in 0..3 {
            run(&again, true, vec![]);
        }
        run(&again, true, vec![key(egui::Key::Enter)]);
        assert_eq!(closed.get(), DialogOutcome::Accept, "Enter confirms");
    }

    /// The greyed-out default button (wizard at an incomplete step): Enter
    /// no longer confirms, Escape still cancels.
    #[test]
    fn enter_does_not_confirm_a_dialog_whose_default_button_is_greyed_out() {
        let id = egui::Id::new("greyed_dialog");
        for (accept_enabled, expected) in
            [(false, DialogOutcome::Open), (true, DialogOutcome::Accept)]
        {
            let context = egui::Context::default();
            let mut outcome = DialogOutcome::Open;
            let mut run = |events: Vec<egui::Event>| {
                image(&context, events, |ui| {
                    outcome = modal_dialog_with(
                        ui.ctx(),
                        id,
                        "Test",
                        draft(),
                        DialogFooter::gated("Next", "", accept_enabled, "reason"),
                        |_| {},
                        |ui| {
                            ui.label("content");
                        },
                    );
                });
                outcome
            };
            for _ in 0..3 {
                run(Vec::new());
            }
            assert_eq!(
                run(vec![key(egui::Key::Enter)]),
                expected,
                "accept_enabled={accept_enabled}"
            );
            if !accept_enabled {
                assert_eq!(run(vec![key(egui::Key::Escape)]), DialogOutcome::Cancel);
            }
        }
    }
}

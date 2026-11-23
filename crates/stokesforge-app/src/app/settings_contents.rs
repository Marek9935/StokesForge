//! Contents of a Script Settings window (#548): the **Form** and **Text** tabs
//! of an object that holds the TOML a script reads.
//!
//! One implementation for the floating window: like the note, the object never
//! detaches (#473), so there is no second caller.
//!
//! # What the window does
//!
//! - **Form**: one page, fixed bold section headings (never collapsible,
//!   S-L5), a label column and one field per value kind. The page is read from
//!   the parsed text ([`AppState::settings_view`], parsed once per text, never
//!   per frame); a field that validates sends **one**
//!   [`Action::ChangeScriptSettings`], which rewrites only that value (every
//!   other byte, comment and blank line stays) and makes one history entry.
//! - **Text**: the raw text in a multi-line editor. It reaches the model only
//!   when the editor loses focus, as the note's does ([`SettingsEdit`]); Esc
//!   abandons. A text that is not exact TOML is **kept** and saved: the
//!   parser's line is shown under the editor, and the Form tab freezes (greyed,
//!   not editable, with a red banner) until the text reads again.
//! - **Hints** (comments above a key) shape the fields: a choice list, bounds.
//!   A value typed outside its bounds is brought back to the bound (orange,
//!   S-N7); a value that is **stored** outside (written by a script or in
//!   Text) shows orange with a bubble and is never rewritten.
//!
//! Numbers read through [`field_state::edit_text`] with their own reading
//! ([`read_integer`], [`read_float`]) rather than through the quantity-based
//! number field: an integer of a script is an `i64`, and a pass through `f64`
//! would round it beyond 2^53.

use eframe::egui;
use stokesforge_core::number_format::{format_exact, parse_input};
use stokesforge_core::script_settings::{
    Entry, EntryValue, Hint, ParseError, Scalar, ScalarKind, Section, SettingsDoc,
};
use stokesforge_core::NodeId;

use stokesforge_app_state::state::{Action, AppState, SettingsChange, SettingsView};
use stokesforge_app_state::{errors, tr};

use crate::field_state::{self, Bound, FieldIssue, Verdict};
use crate::icons::{icon_button, Icon, IconRole};
use crate::theme::{self, FieldWidth};

use super::form::{form, Form};
use super::tab_strip::{tab_strip, TabStripSide};

/// The two pages of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Page {
    Form,
    Text,
}

/// Contents of a [`Tab::ScriptSettings`](super::tab::Tab::ScriptSettings).
pub(super) fn settings_contents(ui: &mut egui::Ui, state: &mut AppState, settings: NodeId) {
    let Some(view) = state.settings_view(settings) else {
        ui.centered_and_justified(|ui| {
            ui.label(theme::dimmed_text(tr!("settings-deleted")));
        });
        return;
    };
    let memory = ui.id().with(("settings_page", settings));
    let mut page = ui
        .data(|data| data.get_temp::<Page>(memory))
        .unwrap_or(Page::Form);
    let mut picked = None;
    tab_strip(
        ui,
        TabStripSide::Top,
        &[
            (Page::Form, tr!("settings-tab-form")),
            (Page::Text, tr!("settings-tab-text")),
        ],
        page,
        |chosen| picked = Some(chosen),
        None,
        || {},
    );
    if let Some(chosen) = picked.filter(|chosen| *chosen != page) {
        // Leaving the Text tab commits what was typed: its editor is not
        // painted this frame, so it cannot lose focus by itself.
        if state
            .settings_editing()
            .is_some_and(|edit| edit.settings == settings)
        {
            state.commit_settings_edit();
        }
        page = chosen;
        ui.data_mut(|data| data.insert_temp(memory, page));
        ui.ctx().request_repaint();
    }
    ui.add_space(theme::SPACE_SECTION);
    // The stored text may have just changed (the tab switch commits): read it again.
    let view = state.settings_view(settings).unwrap_or(view);
    match page {
        Page::Form => form_page(ui, state, settings, &view),
        Page::Text => text_page(ui, state, settings, view.error.as_ref()),
    }
}

// ---------------------------------------------------------------- Text tab

fn text_page(
    ui: &mut egui::Ui,
    state: &mut AppState,
    settings: NodeId,
    error: Option<&ParseError>,
) {
    let editing_here = state
        .settings_editing()
        .is_some_and(|edit| edit.settings == settings);
    let mut buffer = if editing_here {
        state
            .settings_editing()
            .map(|edit| edit.buffer.clone())
            .unwrap_or_default()
    } else {
        state.script_settings_text(settings).unwrap_or_default()
    };
    theme::page_scroll_area()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let response = ui.add(
                egui::TextEdit::multiline(&mut buffer)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(16)
                    .desired_width(f32::INFINITY),
            );
            if response.gained_focus() && !editing_here {
                state.begin_settings_edit(settings);
            }
            if let Some(edit) = state
                .settings_editing_mut()
                .filter(|edit| edit.settings == settings)
            {
                edit.buffer = buffer;
            }
            if response.lost_focus() {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    state.cancel_settings_edit();
                } else if state
                    .settings_editing()
                    .is_some_and(|edit| edit.settings == settings)
                {
                    // Through the state, not an action: a blocking dialog that
                    // took the focus empties the actions of the frame (#465).
                    state.commit_settings_edit();
                }
            }
            if let Some(error) = error {
                ui.add_space(theme::SPACE_ITEM);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(errors::settings::parse_error_message(error))
                            .color(theme::field_refused_color()),
                    )
                    .wrap(),
                );
                ui.add(egui::Label::new(theme::secondary_text(tr!("settings-text-kept"))).wrap());
            }
        });
}

// ---------------------------------------------------------------- Form tab

fn form_page(ui: &mut egui::Ui, state: &mut AppState, settings: NodeId, view: &SettingsView) {
    let frozen = view.error.is_some();
    if let Some(error) = &view.error {
        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(1.0, theme::field_refused_color()))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(tr!(
                            "settings-frozen-banner",
                            line = error.line,
                            detail = error.message.lines().collect::<Vec<_>>().join(" ")
                        ))
                        .color(theme::field_refused_color()),
                    )
                    .wrap(),
                );
            });
        ui.add_space(theme::SPACE_SECTION);
    }
    let Some(doc) = view.doc.clone() else {
        return;
    };
    let mut change = None;
    theme::page_scroll_area()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // A frozen form keeps its last readable values, greyed and inert.
            ui.add_enabled_ui(!frozen, |ui| {
                change = form_fields(ui, settings, &doc);
            });
        });
    if let Some(change) = change {
        // Through the state, like the Text tab: one validation, one entry.
        state.dispatch(Action::ChangeScriptSettings { settings, change });
    }
}

/// The fields of the document. At most one change per frame: a validated
/// field, a toggled box, Add Value or a bin.
fn form_fields(ui: &mut egui::Ui, settings: NodeId, doc: &SettingsDoc) -> Option<SettingsChange> {
    if doc
        .sections
        .iter()
        .all(|section| section.entries.is_empty())
    {
        ui.label(theme::dimmed_text(tr!("settings-empty")));
        return None;
    }
    let mut change = None;
    form(ui, ("settings_form", settings), |form| {
        for section in &doc.sections {
            if section.entries.is_empty() {
                continue;
            }
            if let Some(title) = heading_of(section) {
                form.section(title);
            }
            for entry in &section.entries {
                if let Some(found) = entry_row(form, settings, entry) {
                    change = Some(found);
                }
            }
        }
    });
    change
}

/// "fit › baseline", "peak 2" for the second `[[peak]]`; none for the root keys.
fn heading_of(section: &Section) -> Option<String> {
    let names = section.names();
    if names.is_empty() {
        return None;
    }
    let title = names.join(" › ");
    Some(match section.block {
        Some(block) => format!("{title} {block}"),
        None => title,
    })
}

fn entry_row(form: &mut Form<'_>, settings: NodeId, entry: &Entry) -> Option<SettingsChange> {
    let salt = format!("{settings:?}{:?}", entry.path);
    let label = egui::RichText::new(entry.key.as_str()).monospace();
    let mut change = None;
    match &entry.value {
        EntryValue::Scalar(scalar) => {
            form.row(label, |ui| {
                if let Some(value) = scalar_field(ui, &salt, scalar, &entry.hint) {
                    change = Some(SettingsChange::Scalar {
                        path: entry.path.clone(),
                        value,
                    });
                }
            });
        }
        EntryValue::Array { items, .. } => {
            form.row(label, |ui| {
                change = array_field(ui, &salt, entry, items);
            });
        }
        EntryValue::MixedArray => {
            form.row(label, |ui| {
                ui.label(theme::dimmed_text(tr!("settings-array-read-only")));
            });
        }
    }
    let help = help_text(entry);
    if !help.is_empty() {
        form.note(help);
    }
    change
}

/// The comment above the key, then the bounds the hint states.
fn help_text(entry: &Entry) -> String {
    let mut lines: Vec<String> = Vec::new();
    if !entry.help.is_empty() {
        lines.push(entry.help.clone());
    }
    let (min, max) = (
        entry.hint.min.map(plain_number),
        entry.hint.max.map(plain_number),
    );
    match (min, max) {
        (Some(min), Some(max)) => lines.push(tr!("settings-help-between", min = min, max = max)),
        (Some(min), None) => lines.push(tr!("settings-help-at-least", min = min)),
        (None, Some(max)) => lines.push(tr!("settings-help-at-most", max = max)),
        (None, None) => {}
    }
    lines.join("\n")
}

/// A list: one small field and a bin per element, then an Add Value button.
fn array_field(
    ui: &mut egui::Ui,
    salt: &str,
    entry: &Entry,
    items: &[Scalar],
) -> Option<SettingsChange> {
    let mut change = None;
    ui.horizontal_wrapped(|ui| {
        for (index, item) in items.iter().enumerate() {
            if let Some(value) = scalar_field(ui, &format!("{salt}#{index}"), item, &entry.hint) {
                change = Some(SettingsChange::Element {
                    path: entry.path.clone(),
                    index,
                    value,
                });
            }
            // The analysis lists' twin: a red bin, never a `×` that reads like the `+`.
            if icon_button(
                ui,
                Icon::Delete,
                IconRole::Danger,
                tr!("settings-array-remove", "hint"),
            )
            .clicked()
            {
                change = Some(SettingsChange::Remove {
                    path: entry.path.clone(),
                    index,
                });
            }
        }
        // An empty list has no element to copy the type from.
        let can_add = !items.is_empty();
        let add = ui
            .add_enabled(can_add, egui::Button::new(tr!("settings-array-add")))
            .on_hover_text(tr!("settings-array-add", "hint"))
            .on_disabled_hover_text(tr!("settings-array-add", "disabled"));
        if add.clicked() {
            change = Some(SettingsChange::Append {
                path: entry.path.clone(),
            });
        }
    });
    change
}

/// One scalar field, by kind. Returns the value when it is validated this frame.
fn scalar_field(ui: &mut egui::Ui, salt: &str, scalar: &Scalar, hint: &Hint) -> Option<Scalar> {
    let rejected = hint.rejects(scalar);
    let mut validated = None;
    let response = match scalar {
        Scalar::Bool(stored) => {
            let mut value = *stored;
            let response = ui.checkbox(&mut value, "");
            if response.changed() {
                validated = Some(Scalar::Bool(value));
            }
            response
        }
        _ if hint.choices.is_some() && choice_text(scalar).is_some() => {
            choice_field(ui, salt, scalar, hint, &mut validated)
        }
        Scalar::String(text) => {
            let edited =
                field_state::edit_text(ui, salt, FieldWidth::Full.points(ui), text, |typed| {
                    Verdict::Valid(Scalar::String(typed.to_string()))
                });
            validated = edited.value;
            edited.response
        }
        Scalar::Integer(stored) => {
            let edited = field_state::edit_text(
                ui,
                salt,
                FieldWidth::Short.points(ui),
                &stored.to_string(),
                |typed| read_integer(typed, hint),
            );
            validated = edited.value.map(Scalar::Integer);
            edited.response
        }
        Scalar::Float(stored) => {
            let edited = field_state::edit_text(
                ui,
                salt,
                FieldWidth::Short.points(ui),
                &format_exact(*stored),
                |typed| read_float(typed, hint),
            );
            validated = edited.value.map(Scalar::Float);
            edited.response
        }
        Scalar::OffsetDateTime(text)
        | Scalar::LocalDateTime(text)
        | Scalar::LocalDate(text)
        | Scalar::LocalTime(text) => {
            let kind = scalar.kind();
            let edited =
                field_state::edit_text(ui, salt, FieldWidth::Medium.points(ui), text, |typed| {
                    read_date(typed, kind)
                });
            validated = edited.value;
            edited.response
        }
    };
    if rejected {
        let message = if hint.choices.is_some()
            && choice_text(scalar).is_some_and(|text| {
                hint.choices
                    .as_ref()
                    .is_some_and(|choices| !choices.contains(&text))
            }) {
            tr!(
                "settings-out-of-choices",
                choices = hint
                    .choices
                    .as_ref()
                    .map_or_else(String::new, |c| c.join(", "))
            )
        } else {
            tr!("settings-out-of-limits")
        };
        field_state::show_issue(ui, &response, &FieldIssue::Clamped { message });
    }
    validated
}

/// The text a choice list shows for a scalar, `None` for a kind that has no choice list.
fn choice_text(scalar: &Scalar) -> Option<String> {
    match scalar {
        Scalar::String(text) => Some(text.clone()),
        Scalar::Integer(value) => Some(value.to_string()),
        Scalar::Float(value) => Some(format_exact(*value)),
        _ => None,
    }
}

/// The scalar a picked choice stands for, of the kind of the stored value.
fn choice_scalar(kind: ScalarKind, text: &str) -> Option<Scalar> {
    match kind {
        ScalarKind::String => Some(Scalar::String(text.to_string())),
        ScalarKind::Integer => text.trim().parse().ok().map(Scalar::Integer),
        ScalarKind::Float => text.trim().parse().ok().map(Scalar::Float),
        _ => None,
    }
}

/// The drop-down list of a `choices:` hint. A stored value that is not in the
/// list is shown as it is (the caller paints it orange): it is never rewritten.
fn choice_field(
    ui: &mut egui::Ui,
    salt: &str,
    scalar: &Scalar,
    hint: &Hint,
    validated: &mut Option<Scalar>,
) -> egui::Response {
    let shown = choice_text(scalar).unwrap_or_default();
    let choices = hint.choices.clone().unwrap_or_default();
    let response = egui::ComboBox::from_id_salt(salt)
        .selected_text(shown.as_str())
        .show_ui(ui, |ui| {
            for choice in &choices {
                if ui
                    .add(egui::Button::selectable(*choice == shown, choice.as_str()))
                    .clicked()
                    && *choice != shown
                {
                    *validated = choice_scalar(scalar.kind(), choice);
                }
            }
        });
    response.response
}

// ------------------------------------------------------------ number reading

/// Reads an integer (S-N4, S-N6, S-N7): the comma is accepted but a fraction
/// is not, an unreadable text is refused, a value outside the hint's bounds is
/// brought back to the bound. The value is an `i64` end to end: a text of 19
/// digits comes back unchanged, which a pass through `f64` would round.
fn read_integer(typed: &str, hint: &Hint) -> Verdict<i64> {
    let refused = || Verdict::Refused(tr!("field-refused-integer"));
    let trimmed = typed.trim();
    let value = match trimmed.parse::<i64>() {
        Ok(value) => value,
        Err(_) => match parse_input(trimmed) {
            // "3,0" and "3.0" are 3; a fraction, or a value `f64` cannot hold
            // exactly, is refused.
            Some(read) if read.fract() == 0.0 && read.abs() < 9_007_199_254_740_992.0 => {
                read as i64
            }
            _ => return refused(),
        },
    };
    let number = value as f64;
    if let Some(min) = hint.min.filter(|min| number < *min) {
        let bound = min.ceil() as i64;
        return Verdict::Clamped {
            value: bound,
            bound: Bound::Min,
            limit: bound.to_string(),
        };
    }
    if let Some(max) = hint.max.filter(|max| number > *max) {
        let bound = max.floor() as i64;
        return Verdict::Clamped {
            value: bound,
            bound: Bound::Max,
            limit: bound.to_string(),
        };
    }
    Verdict::Valid(value)
}

/// A bound as written in a help line: `1000`, never `1e3`.
fn plain_number(value: f64) -> String {
    format!("{value}")
}

/// Reads a real number: the comma is accepted, an unreadable text is refused,
/// a value outside the hint's bounds is brought back to the bound.
fn read_float(typed: &str, hint: &Hint) -> Verdict<f64> {
    let Some(value) = parse_input(typed.trim()) else {
        return Verdict::Refused(tr!("field-refused-number"));
    };
    if let Some(min) = hint.min.filter(|min| value < *min) {
        return Verdict::Clamped {
            value: min,
            bound: Bound::Min,
            limit: plain_number(min),
        };
    }
    if let Some(max) = hint.max.filter(|max| value > *max) {
        return Verdict::Clamped {
            value: max,
            bound: Bound::Max,
            limit: plain_number(max),
        };
    }
    Verdict::Valid(value)
}

/// Reads a date or time of the stored kind; the sentence of a refusal shows the
/// form the field expects.
fn read_date(typed: &str, kind: ScalarKind) -> Verdict<Scalar> {
    match Scalar::date_of_kind(kind, typed) {
        Some(scalar) => Verdict::Valid(scalar),
        None => Verdict::Refused(match kind {
            ScalarKind::LocalDate => tr!("settings-refused-date"),
            ScalarKind::LocalTime => tr!("settings-refused-time"),
            ScalarKind::OffsetDateTime => tr!("settings-refused-date-time-offset"),
            _ => tr!("settings-refused-date-time"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    use stokesforge_core::Project;
    use stokesforge_session::Session;

    use super::super::project_manager::tests::find_text_center;
    use super::super::test_support::painted;

    /// A project holding Script Settings "Cfg" with `text`.
    fn project_with(text: &str) -> (Session, AppState, NodeId) {
        let mut project = Project::new("Settings").unwrap();
        let root = project.root();
        let settings = project
            .add_script_settings(root, Some("Cfg"), text)
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state, settings)
    }

    /// A frame of the window on an 800 x 800 screen.
    fn paint(
        context: &egui::Context,
        state: &mut AppState,
        settings: NodeId,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| settings_contents(ui, state, settings));
        output.textures_delta.clear();
        output
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    const BOUNDED: &str = "# Iterations\n# between 1 and 1000\nn = 200\n";

    /// Clicks the field showing "200", selects all and types `typed`; the
    /// field stays open. Returns where the field is.
    fn type_in_the_field(
        context: &egui::Context,
        state: &RefCell<AppState>,
        settings: NodeId,
        typed: &str,
    ) -> egui::Pos2 {
        let image = |events| paint(context, &mut state.borrow_mut(), settings, events);
        let first = image(Vec::new());
        let at = find_text_center(&first.shapes, "200");
        image(vec![egui::Event::PointerMoved(at), pointer(at, true)]);
        image(vec![pointer(at, false)]);
        image(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
        image(vec![egui::Event::Text(typed.to_owned())]);
        at
    }

    /// S-N3: typing never writes; Enter validates, in **one** history entry
    /// that Undo takes back. Witness: validate on every change of the field.
    #[test]
    fn a_number_is_written_on_enter_in_one_entry() {
        let (_session, state, settings) = project_with(BOUNDED);
        let state = RefCell::new(state);
        let context = egui::Context::default();
        let at = type_in_the_field(&context, &state, settings, "500");
        assert_eq!(
            state.borrow().script_settings_text(settings).as_deref(),
            Some(BOUNDED),
            "typing writes nothing"
        );
        paint(
            &context,
            &mut state.borrow_mut(),
            settings,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        paint(&context, &mut state.borrow_mut(), settings, Vec::new());
        assert_eq!(
            state.borrow().script_settings_text(settings).as_deref(),
            Some(BOUNDED.replace("200", "500").as_str())
        );
        state.borrow_mut().dispatch(Action::Undo);
        assert_eq!(
            state.borrow().script_settings_text(settings).as_deref(),
            Some(BOUNDED),
            "one Undo, one entry"
        );
        let _ = at;
    }

    /// Escape abandons the typed value: nothing is written.
    #[test]
    fn escape_abandons_the_typed_number() {
        let (_session, state, settings) = project_with(BOUNDED);
        let state = RefCell::new(state);
        let context = egui::Context::default();
        type_in_the_field(&context, &state, settings, "500");
        paint(
            &context,
            &mut state.borrow_mut(),
            settings,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert_eq!(
            state.borrow().script_settings_text(settings).as_deref(),
            Some(BOUNDED)
        );
    }

    /// S-N7 and S-N6 in the window: 5000 is brought back to 1000 (orange
    /// bubble), "abc" is refused (the old value stays).
    #[test]
    fn a_number_out_of_bounds_is_brought_back_and_a_word_is_refused() {
        let (_session, state, settings) = project_with(BOUNDED);
        let state = RefCell::new(state);
        let context = egui::Context::default();
        type_in_the_field(&context, &state, settings, "5000");
        paint(
            &context,
            &mut state.borrow_mut(),
            settings,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        let output = paint(&context, &mut state.borrow_mut(), settings, Vec::new());
        assert_eq!(
            state.borrow().script_settings_text(settings).as_deref(),
            Some(BOUNDED.replace("200", "1000").as_str())
        );
        assert!(
            painted(&output, &tr!("field-clamped-max", limit = "1000")),
            "the orange bubble says the bound"
        );

        let context = egui::Context::default();
        let (_session2, state2, settings2) = project_with(BOUNDED);
        let state2 = RefCell::new(state2);
        type_in_the_field(&context, &state2, settings2, "abc");
        paint(
            &context,
            &mut state2.borrow_mut(),
            settings2,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(
            state2.borrow().script_settings_text(settings2).as_deref(),
            Some(BOUNDED),
            "refused: nothing written"
        );
    }

    /// The form shows the section headings, the keys, the help comment and the
    /// bounds line; a stored value outside the bounds is shown and not rewritten.
    #[test]
    fn the_form_paints_headings_keys_help_and_keeps_an_out_of_bounds_value() {
        let text = "title = 'x'\n[fit]\n# Number of rounds\n# between 1 and 10\nrounds = 99\n";
        let (_session, mut state, settings) = project_with(text);
        let context = egui::Context::default();
        let output = paint(&context, &mut state, settings, Vec::new());
        for needle in [
            "fit",
            "title",
            "rounds",
            "Number of rounds",
            "Between 1 and 10",
            "99",
        ] {
            assert!(painted(&output, needle), "{needle}");
        }
        assert_eq!(
            state.script_settings_text(settings).as_deref(),
            Some(text),
            "painting rewrites nothing"
        );
    }

    /// A text that is not exact TOML freezes the form under a banner that
    /// names the line; an empty document says how to start.
    #[test]
    fn a_broken_text_freezes_the_form_and_an_empty_one_says_how_to_start() {
        let (_session, mut state, settings) = project_with("a = 1\n");
        let context = egui::Context::default();
        let output = paint(&context, &mut state, settings, Vec::new());
        assert!(painted(&output, "a"));
        state.dispatch(Action::SetScriptSettingsText {
            settings,
            text: "a = 1\nb = = 2\n".to_string(),
        });
        let output = paint(&context, &mut state, settings, Vec::new());
        assert!(painted(&output, "line 2"), "the banner names the line");
        assert!(painted(&output, "a"), "the last readable form stays");

        let (_session, mut empty, settings) = project_with("");
        let output = paint(&egui::Context::default(), &mut empty, settings, Vec::new());
        assert!(painted(&output, &tr!("settings-empty")));
    }

    /// The Text tab: typing writes nothing until the editor loses focus, then
    /// one entry; a malformed text is kept and its line is shown under it.
    #[test]
    fn the_text_tab_applies_on_leaving_and_keeps_a_malformed_text() {
        let (_session, state, settings) = project_with("a = 1\n");
        let state = RefCell::new(state);
        let context = egui::Context::default();
        let image = |events| paint(&context, &mut state.borrow_mut(), settings, events);
        let first = image(Vec::new());
        let tab = find_text_center(&first.shapes, &tr!("settings-tab-text"));
        image(vec![egui::Event::PointerMoved(tab), pointer(tab, true)]);
        image(vec![pointer(tab, false)]);
        let second = image(Vec::new());
        let editor = find_text_center(&second.shapes, "a = 1");
        image(vec![
            egui::Event::PointerMoved(editor),
            pointer(editor, true),
        ]);
        image(vec![pointer(editor, false)]);
        image(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
        image(vec![egui::Event::Text("b = = 2".to_owned())]);
        assert_eq!(
            state.borrow().script_settings_text(settings).as_deref(),
            Some("a = 1\n"),
            "typing writes nothing"
        );
        // Leaving the editor (a click elsewhere) commits it, malformed or not.
        let outside = egui::pos2(400.0, 790.0);
        image(vec![
            egui::Event::PointerMoved(outside),
            pointer(outside, true),
        ]);
        image(vec![pointer(outside, false)]);
        let after = image(Vec::new());
        assert_eq!(
            state.borrow().script_settings_text(settings).as_deref(),
            Some("b = = 2"),
            "the malformed text is kept"
        );
        assert!(
            painted(&after, "Line 1"),
            "its line is shown under the editor"
        );
    }

    fn bounded(min: Option<f64>, max: Option<f64>) -> Hint {
        Hint {
            choices: None,
            min,
            max,
        }
    }

    /// S-N4 and S-N6: a whole number is read, a comma fraction or a word is
    /// refused (the old value comes back).
    #[test]
    fn an_integer_field_refuses_what_is_not_a_whole_number() {
        let free = Hint::default();
        assert_eq!(read_integer(" 42 ", &free), Verdict::Valid(42));
        assert_eq!(read_integer("3,0", &free), Verdict::Valid(3));
        for typed in ["abc", "2,5", "2.5", "", "1e400"] {
            assert!(
                matches!(read_integer(typed, &free), Verdict::Refused(_)),
                "{typed:?}"
            );
        }
    }

    /// Witness of the exactness: 2^53 + 1 is the first integer an `f64` cannot
    /// hold. A pass through `f64` gives 2^53.
    #[test]
    fn an_integer_beyond_two_to_the_53_comes_back_unchanged() {
        let free = Hint::default();
        let beyond = 9_007_199_254_740_993_i64;
        assert_eq!(
            read_integer(&beyond.to_string(), &free),
            Verdict::Valid(beyond)
        );
        assert_eq!(
            read_integer(&i64::MAX.to_string(), &free),
            Verdict::Valid(i64::MAX)
        );
        assert_eq!(
            read_integer(&i64::MIN.to_string(), &free),
            Verdict::Valid(i64::MIN)
        );
        // A float literal that big is not an exact integer: refused, never rounded.
        assert!(matches!(
            read_integer("9007199254740993.0", &free),
            Verdict::Refused(_)
        ));
    }

    /// S-N7: out of bounds is brought back to the bound, never refused.
    #[test]
    fn a_number_outside_its_bounds_is_brought_back_to_the_bound() {
        let hint = bounded(Some(1.0), Some(1000.0));
        match read_integer("5000", &hint) {
            Verdict::Clamped {
                value,
                bound,
                limit,
            } => {
                assert_eq!((value, bound, limit.as_str()), (1000, Bound::Max, "1000"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            read_integer("0", &hint),
            Verdict::Clamped {
                value: 1,
                bound: Bound::Min,
                ..
            }
        ));
        assert_eq!(read_integer("200", &hint), Verdict::Valid(200));
        let at_least = bounded(Some(0.0), None);
        assert!(matches!(
            read_float("-0,5", &at_least),
            Verdict::Clamped { value, bound: Bound::Min, .. } if value == 0.0
        ));
    }

    #[test]
    fn a_real_field_accepts_the_comma_and_refuses_words() {
        let free = Hint::default();
        assert_eq!(read_float("2,5", &free), Verdict::Valid(2.5));
        assert_eq!(read_float("1e-6", &free), Verdict::Valid(1e-6));
        for typed in ["abc", "", "inf", "1,2,3"] {
            assert!(
                matches!(read_float(typed, &free), Verdict::Refused(_)),
                "{typed:?}"
            );
        }
    }

    #[test]
    fn a_date_is_read_in_the_stored_kind_or_refused() {
        assert!(matches!(
            read_date("2026-11-17", ScalarKind::LocalDate),
            Verdict::Valid(Scalar::LocalDate(_))
        ));
        assert!(matches!(
            read_date("hello", ScalarKind::LocalDate),
            Verdict::Refused(_)
        ));
        assert!(matches!(
            read_date("2026-11-17", ScalarKind::LocalTime),
            Verdict::Refused(_)
        ));
    }
}

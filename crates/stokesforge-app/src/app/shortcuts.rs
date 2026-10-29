//! Keyboard shortcuts of the application: the `SHORTCUT_*` constants (those
//! that are consumed and the decorative ones, which only serve for the reminder
//! shown in a menu), their reminders spelled out in full, and the consumption
//! of undo/redo in the active viewport.
//!
//! Moved out of `app.rs` as is (#117). The global consumption of shortcuts,
//! `collect_shortcuts`, stays in `app.rs` (#115).
//!
//! **Single registry of keys** (#231, S-K1): every key the application
//! consumes is a constant of this module (`SHORTCUT_*` for shortcuts, `KEY_*`
//! for the bare keys of a field or a list), read by
//! `input.consume_shortcut(&…)`. No `consume_key` elsewhere (guard
//! `no_consume_key_outside_the_registry`, `tests/guards.rs`). The Help ›
//! Keyboard Shortcuts window is generated from [`HELP`], at the bottom of
//! this file: a new shortcut is added there, otherwise it stays invisible to
//! the user.

use eframe::egui;

use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;

/// Escape — cancels the current input, closes the list or the dialog.
pub(crate) const KEY_ESCAPE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Escape);
/// Enter — confirms the current input or the dialog; in a list (Project
/// Manager, Object Manager), opens the selected row (S-K9).
pub(crate) const KEY_ENTER: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Enter);
/// Tab — confirms the input and moves to the next field.
pub(crate) const KEY_TAB: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Tab);
/// Delete — deletes the selection (the grid first clears it, S-K2).
pub(crate) const KEY_DELETE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Delete);
/// Backspace — clears the grid selection.
pub(crate) const KEY_BACKSPACE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Backspace);
/// ↑ — neighboring value of a number field or a list; previous row of a
/// manager (Project Manager, Object Manager, S-K9).
pub(crate) const KEY_ARROW_UP: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::ArrowUp);
/// ↓ — neighboring value of a number field or a list; next row of a manager
/// (S-K9).
pub(crate) const KEY_ARROW_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::ArrowDown);
/// Shift+↑ — ten steps up in a number field.
pub(crate) const KEY_SHIFT_ARROW_UP: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::ArrowUp);
/// Shift+↓ — ten steps down in a number field.
pub(crate) const KEY_SHIFT_ARROW_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
/// Alt+↓ — opens the list of suggested values of a number field.
pub(crate) const KEY_ALT_ARROW_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::ArrowDown);

/// Ctrl+Z — undo.
pub(super) const SHORTCUT_UNDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
/// Ctrl+Y — redo (Windows convention).
pub(super) const SHORTCUT_REDO_Y: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
/// Ctrl+Shift+Z — redo (Unix / macOS convention). Both are accepted.
const SHORTCUT_REDO_Z: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Z,
);

/// Undo/redo in the active viewport, root or detached workbook.
pub(super) fn collect_undo_redo_shortcuts(ctx: &egui::Context, actions: &mut Vec<Action>) {
    ctx.input_mut(|input| {
        // Redo is tested before undo: Ctrl+Shift+Z would also match Ctrl+Z
        // with the extra modifiers.
        if input.consume_shortcut(&SHORTCUT_REDO_Z) || input.consume_shortcut(&SHORTCUT_REDO_Y) {
            actions.push(Action::Redo);
        } else if input.consume_shortcut(&SHORTCUT_UNDO) {
            actions.push(Action::Undo);
        }
    });
}

/// Ctrl+S — save (into the already known file, with no dialog).
pub(super) const SHORTCUT_SAVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
/// Ctrl+Shift+S — choose a destination and save there.
pub(super) const SHORTCUT_SAVE_AS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::S,
);
/// Ctrl+O — choose a project to open.
pub(super) const SHORTCUT_OPEN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
/// Ctrl+N — new project (confirmation if changes would be lost).
pub(super) const SHORTCUT_NEW_PROJECT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
/// Ctrl+E — export the active graph as SVG, PDF or PNG (issue #34). No effect
/// when the active tab is not a graph.
pub(super) const SHORTCUT_EXPORT_GRAPH: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
/// Ctrl+" — open the import wizard. On an AZERTY keyboard, `"` is the digit 3
/// key **without** Shift: egui has no `"` key and falls back to the physical
/// key, so the shortcut is consumed as Ctrl+3. On a QWERTY keyboard, `"` is
/// Shift+apostrophe ([`SHORTCUT_IMPORT_QWERTY`]).
pub(super) const SHORTCUT_IMPORT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num3);
/// Ctrl+Shift+' — the same import shortcut on a QWERTY keyboard, where `"` is
/// typed with Shift.
pub(super) const SHORTCUT_IMPORT_QWERTY: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Quote,
);
/// Reminder shown in the menu: the character, not the name of the physical
/// key.
pub(super) const SHORTCUT_IMPORT_TEXT: &str = "Ctrl+\"";
/// Ctrl+Enter — run the console script.
pub(super) const SHORTCUT_RUN_SCRIPT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter);

/// F2 — rename the selection of the Project Manager or the Object Manager.
pub(super) const SHORTCUT_RENAME_OBJECT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F2);
/// Ctrl+↑ — move the selected series up one rank (Object Manager and
/// figure; consumed by `series_shortcuts`).
pub(super) const SHORTCUT_MOVE_SERIES_UP: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::ArrowUp);
/// Ctrl+↓ — move the selected series down one rank.
pub(super) const SHORTCUT_MOVE_SERIES_DOWN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::ArrowDown);
/// F2 — edit a cell or a header field in the focused grid (#143).
pub(super) const SHORTCUT_EDIT_CELL: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::F2);
/// Ctrl+D — duplicate the displayed sheet of the active workbook. No Delete
/// to delete a sheet: too many false positives, the menu alone is enough.
pub(super) const SHORTCUT_DUPLICATE_SHEET: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::D);

/// Ctrl+D — duplicate the selected objects of the Project Manager.
pub(super) const SHORTCUT_DUPLICATE_OBJECT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::D);

/// Ctrl+C — copy the grid selection.
///
/// # These five shortcuts are **never** consumed
///
/// Unlike [`SHORTCUT_UNDO`] and the like, the five constants that follow are
/// purely **decorative**: they exist only to write the shortcut reminder to
/// the right of a context menu entry (`Context::format_shortcut`, which
/// renders "Ctrl+C" or "⌘C" depending on the platform). No call to
/// `consume_shortcut` takes them, and one must not try: `egui-winit`
/// intercepts Ctrl+C/X/V when translating the window events and never lets an
/// `Event::Key` through — an earlier attempt, based on the opposite
/// assumption, produced shortcuts that never fired. The keyboard block of
/// [`grid_contents`](crate::app::grid::grid_contents) reads the matching
/// `Event::Copy`/`Event::Cut`/`Event::Paste`, and that is where the real logic
/// lives.
pub(super) const SHORTCUT_COPY: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C);
/// Ctrl+Shift+C — copy the touched columns in full (decorative, see
/// [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_COPY_FULL_COLUMNS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::C,
);
/// Ctrl+A — select all in the grid and in Edit (#159), same effect as the
/// corner box.
pub(super) const SHORTCUT_SELECT_ALL: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::A);
/// Ctrl+X — cut (decorative, see [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_CUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::X);
/// Ctrl+V — paste from the anchor (decorative, see [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_PASTE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::V);
/// Ctrl+Shift+V — paste in place (decorative, see [`SHORTCUT_COPY`]).
pub(super) const SHORTCUT_PASTE_ABSOLUTE: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::V,
);

/// Shortcut reminder for Backspace, spelled out in full rather than through
/// `Context::format_shortcut`.
///
/// egui names its keys in English ("Backspace", "Delete"): passing them
/// through `format_shortcut` would put two English words in the middle of a
/// fully French menu. Shortcuts with a modifier, on the other hand, render
/// identically in all languages ("Ctrl+C") and are better off going through
/// `format_shortcut`, which knows how to adapt them to macOS.
pub(super) fn shortcut_text_backspace() -> String {
    tr!("shortcut-backspace")
}

/// Counterpart of [`shortcut_text_backspace`] for Delete.
pub(super) fn shortcut_text_delete() -> String {
    tr!("shortcut-delete")
}

/// Where a shortcut applies: the sections of Help › Keyboard Shortcuts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum HelpArea {
    General,
    Workbook,
    Graph,
    Managers,
    Fields,
}

impl HelpArea {
    pub(super) const ALL: [HelpArea; 5] = [
        HelpArea::General,
        HelpArea::Workbook,
        HelpArea::Graph,
        HelpArea::Managers,
        HelpArea::Fields,
    ];

    pub(super) fn title(self) -> String {
        match self {
            HelpArea::General => tr!("help-area-general"),
            HelpArea::Workbook => tr!("help-area-workbook"),
            HelpArea::Graph => tr!("help-area-graph"),
            HelpArea::Managers => tr!("help-area-managers"),
            HelpArea::Fields => tr!("help-area-fields"),
        }
    }
}

/// What is shown to the right of a help row.
pub(super) enum HelpKeys {
    /// A shortcut with a modifier, written by `Context::format_shortcut`.
    Shortcut(&'static egui::KeyboardShortcut),
    /// Two equivalent shortcuts.
    Either(
        &'static egui::KeyboardShortcut,
        &'static egui::KeyboardShortcut,
    ),
    /// A key or gesture that is not a shortcut with a modifier (Delete, the
    /// wheel): translated text.
    Text(fn() -> String),
}

/// A row of Help › Keyboard Shortcuts.
pub(super) struct HelpEntry {
    pub(super) area: HelpArea,
    pub(super) label: fn() -> String,
    pub(super) keys: HelpKeys,
}

impl HelpEntry {
    /// The keys of the row, as they are displayed.
    pub(super) fn keys_text(&self, ctx: &egui::Context) -> String {
        self.keys_text_with(|shortcut| ctx.format_shortcut(shortcut))
    }

    /// Same as [`keys_text`](Self::keys_text) with another spelling of a
    /// shortcut: the manual pages (#364) spell them the same way on every
    /// host (Ctrl, never the Cmd symbol).
    pub(super) fn keys_text_with(
        &self,
        format: impl Fn(&egui::KeyboardShortcut) -> String,
    ) -> String {
        match &self.keys {
            HelpKeys::Shortcut(shortcut) => format(shortcut),
            HelpKeys::Either(a, b) => {
                tr!("help-keys-either", first = format(a), second = format(b))
            }
            HelpKeys::Text(text) => text(),
        }
    }
}

macro_rules! entry {
    ($area:ident, $label:literal, shortcut $shortcut:expr) => {
        HelpEntry {
            area: HelpArea::$area,
            label: || tr!($label),
            keys: HelpKeys::Shortcut(&$shortcut),
        }
    };
    ($area:ident, $label:literal, either $a:expr, $b:expr) => {
        HelpEntry {
            area: HelpArea::$area,
            label: || tr!($label),
            keys: HelpKeys::Either(&$a, &$b),
        }
    };
    ($area:ident, $label:literal, text $text:expr) => {
        HelpEntry {
            area: HelpArea::$area,
            label: || tr!($label),
            keys: HelpKeys::Text($text),
        }
    };
}

/// The shortcuts the help shows, in the display order of each section. A new
/// shortcut of the registry is added here, otherwise the user does not
/// discover it (S-K5).
pub(super) const HELP: &[HelpEntry] = &[
    entry!(General, "help-undo", shortcut SHORTCUT_UNDO),
    entry!(General, "help-redo", either SHORTCUT_REDO_Y, SHORTCUT_REDO_Z),
    entry!(General, "help-new-project", shortcut SHORTCUT_NEW_PROJECT),
    entry!(General, "help-open", shortcut SHORTCUT_OPEN),
    entry!(General, "help-save", shortcut SHORTCUT_SAVE),
    entry!(General, "help-save-as", shortcut SHORTCUT_SAVE_AS),
    entry!(
        General,
        "help-import",
        text || SHORTCUT_IMPORT_TEXT.to_string()
    ),
    entry!(General, "help-export-graph", shortcut SHORTCUT_EXPORT_GRAPH),
    entry!(General, "help-run-script", shortcut SHORTCUT_RUN_SCRIPT),
    entry!(Workbook, "help-edit-cell", shortcut SHORTCUT_EDIT_CELL),
    entry!(Workbook, "help-select-all", shortcut SHORTCUT_SELECT_ALL),
    entry!(Workbook, "help-copy", shortcut SHORTCUT_COPY),
    entry!(Workbook, "help-copy-full-columns", shortcut SHORTCUT_COPY_FULL_COLUMNS),
    entry!(Workbook, "help-cut", shortcut SHORTCUT_CUT),
    entry!(Workbook, "help-paste", shortcut SHORTCUT_PASTE),
    entry!(Workbook, "help-paste-absolute", shortcut SHORTCUT_PASTE_ABSOLUTE),
    entry!(Workbook, "help-clear-cells", text shortcut_text_delete),
    entry!(Workbook, "help-duplicate-sheet", shortcut SHORTCUT_DUPLICATE_SHEET),
    entry!(Graph, "help-copy-graph", shortcut SHORTCUT_COPY),
    entry!(Graph, "help-delete-object", text shortcut_text_delete),
    entry!(Graph, "help-move-series-up", shortcut SHORTCUT_MOVE_SERIES_UP),
    entry!(Graph, "help-move-series-down", shortcut SHORTCUT_MOVE_SERIES_DOWN),
    entry!(Graph, "help-zoom", text || tr!("help-keys-ctrl-wheel")),
    entry!(Managers, "help-rename", shortcut SHORTCUT_RENAME_OBJECT),
    entry!(Managers, "help-delete-selection", text shortcut_text_delete),
    entry!(Managers, "help-duplicate", shortcut SHORTCUT_DUPLICATE_OBJECT),
    entry!(
        Managers,
        "help-select-neighbour",
        text || tr!("help-keys-arrows")
    ),
    entry!(Managers, "help-open-entry", text || tr!("help-keys-enter")),
    entry!(Fields, "help-validate", text || tr!("help-keys-enter")),
    entry!(Fields, "help-cancel", text || tr!("help-keys-escape")),
    entry!(Fields, "help-next-field", text || tr!("help-keys-tab")),
    entry!(Fields, "help-step-value", text || tr!("help-keys-arrows")),
    entry!(
        Fields,
        "help-step-value-ten",
        text || tr!("help-keys-shift-arrows")
    ),
    entry!(Fields, "help-open-list", shortcut KEY_ALT_ARROW_DOWN),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// What [`collect_undo_redo_shortcuts`] draws from a frame where `key` is
    /// pressed with `modifiers`, then what is left of it for a second reader
    /// in the same frame.
    fn actions_for(key: egui::Key, modifiers: egui::Modifiers) -> (Vec<Action>, Vec<Action>) {
        let context = egui::Context::default();
        let mut first = Vec::new();
        let mut leftovers = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ui| {
                collect_undo_redo_shortcuts(ui.ctx(), &mut first);
                collect_undo_redo_shortcuts(ui.ctx(), &mut leftovers);
            },
        );
        output.textures_delta.clear();
        (first, leftovers)
    }

    /// Ctrl+Z undoes; Ctrl+Y and Ctrl+Shift+Z redo. Ctrl+Shift+Z contains
    /// Ctrl+Z: it must redo **without** undoing (redo is tested first). The
    /// shortcut is consumed: a second reader of the same frame sees nothing of
    /// it, and Z without Ctrl is not a shortcut.
    #[test]
    fn undo_and_redo_are_read_from_the_keyboard_only_once() {
        let ctrl = egui::Modifiers::COMMAND.plus(egui::Modifiers::CTRL);
        let ctrl_shift = ctrl.plus(egui::Modifiers::SHIFT);
        for (key, modifiers, expected) in [
            (egui::Key::Z, ctrl, vec![Action::Undo]),
            (egui::Key::Y, ctrl, vec![Action::Redo]),
            (egui::Key::Z, ctrl_shift, vec![Action::Redo]),
            (egui::Key::Z, egui::Modifiers::NONE, vec![]),
            (egui::Key::A, ctrl, vec![]),
        ] {
            let (first, leftovers) = actions_for(key, modifiers);
            assert_eq!(first, expected, "{key:?} {modifiers:?}");
            assert!(leftovers.is_empty(), "{key:?} {modifiers:?} read twice");
        }
    }
}

#[cfg(test)]
mod help_tests {
    use super::*;

    /// Each help section has rows, each row a non-empty key text, in both
    /// languages.
    #[test]
    fn the_shortcut_help_is_complete() {
        let ctx = egui::Context::default();
        for area in HelpArea::ALL {
            assert!(
                HELP.iter().any(|entry| entry.area == area),
                "empty section: {area:?}"
            );
        }
        for entry in HELP {
            assert!(!(entry.label)().is_empty());
            assert!(!entry.keys_text(&ctx).is_empty());
        }
    }

    /// The help is generated from the registry: every consumed `SHORTCUT_*`
    /// constant appears in it. A forgotten shortcut would fail this test.
    #[test]
    fn every_consumed_shortcut_appears_in_the_help() {
        let ctx = egui::Context::default();
        let shown: Vec<String> = HELP.iter().map(|entry| entry.keys_text(&ctx)).collect();
        for shortcut in [
            &SHORTCUT_UNDO,
            &SHORTCUT_REDO_Y,
            &SHORTCUT_SAVE,
            &SHORTCUT_SAVE_AS,
            &SHORTCUT_OPEN,
            &SHORTCUT_NEW_PROJECT,
            &SHORTCUT_EXPORT_GRAPH,
            &SHORTCUT_RUN_SCRIPT,
            &SHORTCUT_RENAME_OBJECT,
            &SHORTCUT_EDIT_CELL,
            &SHORTCUT_MOVE_SERIES_UP,
            &SHORTCUT_MOVE_SERIES_DOWN,
            &SHORTCUT_DUPLICATE_SHEET,
            &SHORTCUT_DUPLICATE_OBJECT,
            &SHORTCUT_SELECT_ALL,
        ] {
            let text = ctx.format_shortcut(shortcut);
            assert!(
                shown.iter().any(|keys| keys.contains(&text)),
                "{text} missing from the help"
            );
        }
    }
}

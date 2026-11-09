//! The import window (`Import…`, File menu): **one screen that creates**.
//! The file tabs, what was understood, the left panel (the format sentence,
//! the points to check, what was found) and the file cut into a grid you mark
//! directly. This module only paints [`AppState`] and turns events into calls
//! of it; every decision (what is a doubt, what a role does to the recipe,
//! which template fits) lives in `stokesforge-app-state`.
//! A single entry point for `app.rs`: [`import_window`], a `modal_dialog_with`
//! (never a raw `egui::Window`).

use eframe::egui;

mod grid;
mod templates;
#[cfg(test)]
mod test_bench;

use stokesforge_app_state::errors;
use stokesforge_app_state::state::{Action, AppState, GridCmd, LineRoleCmd};
use stokesforge_app_state::tr;
use stokesforge_import::sniff::{AutoWhat, SniffRefusal};
use stokesforge_import::{PropField, PropTarget};

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize,
};

/// Colour of a role (its button, its text in the grid and in the role popup).
fn role_color(role: LineRoleCmd, dark: bool) -> Option<egui::Color32> {
    use crate::theme::{import_color, ImportColor};
    Some(match role {
        LineRoleCmd::DataStart => crate::theme::accent(),
        LineRoleCmd::LongName => import_color(dark, ImportColor::LongName),
        LineRoleCmd::Units => import_color(dark, ImportColor::Units),
        LineRoleCmd::Comments => import_color(dark, ImportColor::Comments),
        LineRoleCmd::XPrime => import_color(dark, ImportColor::XPrime),
        LineRoleCmd::Names => import_color(dark, ImportColor::ColumnNames),
        LineRoleCmd::DataEnd => import_color(dark, ImportColor::Footer),
        LineRoleCmd::Clear => return None,
    })
}

/// Label of the role of a column in the result table.
fn column_role_label(role: stokesforge_import::ColumnRoleChoice) -> String {
    use stokesforge_import::ColumnRoleChoice;
    match role {
        ColumnRoleChoice::Auto => tr!("common-auto"),
        ColumnRoleChoice::X => "X".to_string(),
        ColumnRoleChoice::Y => "Y".to_string(),
        ColumnRoleChoice::XError => tr!("column-role-x-error"),
        ColumnRoleChoice::YError => tr!("column-role-y-error"),
        ColumnRoleChoice::Ignore => tr!("import-option-none"),
    }
}

/// Two options of the same variant, whatever they carry: choosing one again
/// keeps the text or number already typed.
fn same_variant<T>(a: &T, b: &T) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Label of a forced encoding.
fn encoding_label(encoding: stokesforge_import::Encoding) -> String {
    use stokesforge_import::Encoding;
    // Encoding names are technical names: only "Auto" is translated.
    match encoding {
        Encoding::Auto => tr!("common-auto"),
        Encoding::Utf8 => "UTF-8".to_string(),
        Encoding::Windows1252 => "Windows-1252".to_string(),
        Encoding::Latin1 => "ISO 8859-1".to_string(),
        Encoding::Utf16Le => "UTF-16 LE".to_string(),
        Encoding::Utf16Be => "UTF-16 BE".to_string(),
    }
}

/// Letters of a column, A, B… Z, AA…
fn column_letters(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.iter().rev().collect()
}

/// "Result" tab: one row per imported column, from the real
/// parse of the file shown.
fn import_result_table(ui: &mut egui::Ui, state: &AppState) {
    let table = match state.import_preview() {
        None => {
            ui.weak(tr!("import-preview-empty"));
            return;
        }
        Some(Err(message)) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                errors::import::read_message(&message),
            );
            return;
        }
        Some(Ok(table)) => table,
    };
    let Some(import) = state.import_state() else {
        return;
    };
    ui.weak(tr!("import-result-count", count = table.columns.len()));
    let dash = "–";
    let opt = |text: &Option<String>| text.clone().unwrap_or_else(|| dash.to_string());
    // RAW-SCROLL(#347): table preview, rows are the content: no end margin
    egui::ScrollArea::both()
        .id_salt("stokesforge_import_result_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("stokesforge_import_result_grid")
                .striped(true)
                .spacing([14.0, 3.0])
                .show(ui, |ui| {
                    for title in [
                        "#".to_string(),
                        tr!("column-long-name"),
                        tr!("column-units"),
                        tr!("column-comments"),
                        tr!("column-x-prime"),
                        tr!("column-role"),
                        tr!("import-columns-values"),
                    ] {
                        ui.label(crate::theme::table_header(title));
                    }
                    ui.end_row();
                    for (index, column) in table.columns.iter().enumerate() {
                        ui.label(column_letters(index));
                        ui.label(opt(&column.long_name));
                        ui.label(opt(&column.unit));
                        ui.label(opt(&column.comment));
                        ui.label(
                            column
                                .x_prime
                                .map_or(dash.to_string(), |value| format!("{value}")),
                        );
                        ui.label(column_role_label(import.recipe.column_role(index)));
                        ui.label(column.values.len().to_string());
                        ui.end_row();
                    }
                });
        });
}

// -- Automatic choices ---------------------------------------------------

const PILL_WIDTH: f32 = 34.0;

fn prop_field_label(field: PropField) -> String {
    match field {
        PropField::LongName => tr!("column-long-name"),
        PropField::Units => tr!("column-units"),
        PropField::Comment => tr!("column-comments"),
    }
}

fn prop_target_label(target: PropTarget) -> String {
    match target {
        PropTarget::X => "X".to_string(),
        PropTarget::Y => "Y".to_string(),
        PropTarget::All => tr!("import-auto-target-all"),
    }
}

fn prop_color(dark: bool, field: PropField) -> egui::Color32 {
    use crate::theme::{import_color, ImportColor};
    import_color(
        dark,
        match field {
            PropField::LongName => ImportColor::LongName,
            PropField::Units => ImportColor::Units,
            PropField::Comment => ImportColor::Comments,
        },
    )
}

/// The preview of a file that is not readable as a table: a message card.
fn refused_card(ui: &mut egui::Ui, refusal: SniffRefusal) {
    let (title, text) = match refusal {
        SniffRefusal::Binary => (
            tr!("import-mark-refused-binary"),
            tr!("import-refused-binary"),
        ),
        SniffRefusal::Empty => (
            tr!("import-mark-refused-empty"),
            tr!("import-refused-empty"),
        ),
        SniffRefusal::NoNumbers => (
            tr!("import-mark-refused-numbers"),
            tr!("import-refused-numbers"),
        ),
        SniffRefusal::Nmr => (tr!("import-mark-refused-nmr"), tr!("import-refused-nmr")),
    };
    let red = ui.visuals().error_fg_color;
    ui.vertical_centered(|ui| {
        ui.add_space(crate::theme::SPACE_SECTION * 12.0);
        egui::Frame::new()
            .fill(red.gamma_multiply(0.08))
            .stroke(egui::Stroke::new(1.0, red))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_max_width(420.0);
                ui.colored_label(red, crate::theme::bold(title));
                ui.label(text);
            });
    });
}

/// What the window asks of the application after a frame, once the borrow of
/// the state is released.
#[derive(Default)]
struct ImportWizardRequests {
    cancel: bool,
    import: bool,
    /// File pickers requested (#192), pushed as actions.
    picks: Vec<Action>,
    /// Automatic choices taken back by their pill in the grid .
    auto: Vec<AutoWhat>,
    /// Commands of the cut sheet grid .
    grid: Vec<GridCmd>,
    /// Clicks of the left panel (review, templates).
    checks: Vec<grid::CheckCmd>,
}

/// The import window (« Import… », File menu): **one screen that creates**.
/// The file tabs, what was understood, the left panel (the format sentence, the
/// points to check, what was found) and the file cut into a grid you mark
/// directly; « Import » is always there, the doubts are a warning in the footer,
/// not a gate. A draft dialog of the common building block ([`modal_dialog_with`]),
/// painted from [`StokesForgeApp::ui`](eframe::App::ui) as long as
/// [`AppState::import_state`] is `Some`; the close cross and Escape mean
/// "Cancel", Enter "Import". Footer (S-F6): Destination and Template on the
/// left, the message area, Cancel, Import. The windows of the templates are
/// painted **inside** the content (see [`templates::windows`]) to read Enter and
/// Escape before it.
pub(super) fn import_window(ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
    if state.import_state().is_none() {
        return;
    }
    let has_files = state
        .import_state()
        .is_some_and(|import| !import.paths.is_empty());
    // The two closures of the building block (content, footer buttons) write
    // into the same requests, one after the other: `RefCell`.
    let requests = std::cell::RefCell::new(ImportWizardRequests::default());
    // Escape clears the selection of the grid first.
    grid::grid_escape(ctx);
    // Doubts left do not block the import: the footer says so.
    let doubts = state.to_check_count();
    let reviewing = state.check_review_kind().is_some();
    let warning = if doubts > 0 && !reviewing {
        tr!("import-footer-unchecked", count = doubts)
    } else {
        String::new()
    };
    let finish_label = tr!("import-grid-import");
    let finish_hint = tr!("import-button-finish", "hint");
    let add_file_first = tr!("disabled-add-file-first");
    let state_cell = std::cell::RefCell::new(&mut *state);
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new("stokesforge_import_wizard"),
        &tr!("import-window-title"),
        // FREE-SIZE(#300): the one-screen import (tabs, left panel, grid, result line) needs the room; a smaller screen shrinks it
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(1320.0, 840.0))),
        DialogFooter::gated(&finish_label, &finish_hint, has_files, &add_file_first)
            .with_warning(&warning),
        |footer| {
            let mut guard = state_cell.borrow_mut();
            let state: &mut AppState = &mut guard;
            let mut requests = requests.borrow_mut();
            grid::footer_buttons(footer, state, &mut requests);
        },
        |ui| {
            let mut requests = requests.borrow_mut();
            let mut guard = state_cell.borrow_mut();
            let state: &mut AppState = &mut guard;
            grid::import_screen(ui, state, &mut requests);
            templates::windows(ui.ctx(), state);
        },
    );
    // Previews are recomputed after a pause in typing (S-T1, #224).
    let now = ctx.input(|input| input.time);
    if let Some(wait) = state
        .import_state_mut()
        .and_then(|import| import.debounce_preview(ctx.text_edit_focused(), now))
    {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait + 0.01));
    }
    let mut requests = requests.into_inner();
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => requests.import = true,
        DialogOutcome::Cancel => requests.cancel = true,
    }

    // Everything that needs the whole state is applied here, once the window
    // is painted.
    actions.append(&mut requests.picks);
    // The pills of the grid take an automatic choice back.
    for what in std::mem::take(&mut requests.auto) {
        state.import_auto_undo(&what);
    }
    for cmd in std::mem::take(&mut requests.grid) {
        state.import_grid_cmd(cmd);
    }
    for cmd in std::mem::take(&mut requests.checks) {
        grid::apply_check(state, cmd);
    }
    if requests.import {
        if let Some(action) = state.import_finish() {
            actions.push(action);
        }
    } else if requests.cancel {
        state.cancel_import();
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use stokesforge_app_state::state::{Action, ImportDestination};
    use stokesforge_app_state::tr;

    use super::test_bench::{
        bench, click_at, click_text, enter, escape, paint, settle, DOUBTS, SURE,
    };

    /// Closes the first-use guide with Escape (it is open at every first
    /// paint), then paints until the layout settles.
    fn without_guide(
        ctx: &egui::Context,
        state: &mut stokesforge_app_state::state::AppState,
        actions: &mut Vec<Action>,
    ) -> Vec<crate::app::test_support::Text> {
        for _ in 0..3 {
            paint(ctx, state, actions, Vec::new());
        }
        paint(ctx, state, actions, vec![escape()]);
        settle(ctx, state, actions)
    }

    /// One screen: the format sentence, the groups of what was found, the
    /// footer with the two menus, Cancel and Import (S-F6).
    #[test]
    fn the_one_screen_paints_its_panel_and_its_footer() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        let painted = settle(&ctx, &mut bench.state, &mut actions);
        for wanted in [
            tr!("import-format-short-delimiter"),
            tr!("import-format-short-decimal"),
            tr!("import-format-short-thousands"),
            tr!("import-format-short-encoding"),
            tr!("import-found-group-data"),
            tr!("import-found-group-columns"),
            tr!("import-found-group-sheet"),
            tr!("import-grid-import"),
            tr!("dialog-cancel"),
            tr!("import-footer-to", value = tr!("import-destination-new")),
        ] {
            // The footer menu buttons leave room for their caret icon.
            assert!(
                painted.iter().any(|t| t.text.trim_end() == wanted),
                "not painted: {wanted:?}"
            );
        }
        // The instrument is recognised and nothing is left to check: the offer.
        let offer = tr!("import-template-offer-title", name = "JASCO V-770");
        assert!(painted.iter().any(|t| t.text == offer), "{offer}");
    }

    /// Finish imports the recipe the state chose for each file, as it is, and
    /// Cancel imports nothing; Escape first closes the guide, which is over
    /// the window, and only then the window.
    #[test]
    fn finish_imports_the_recipe_as_it_is_and_cancel_imports_nothing() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut complete = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        without_guide(&ctx, &mut complete.state, &mut actions);
        let (each, skipped) = complete.state.import_each_recipe();
        assert_eq!(each.len(), 1);
        paint(&ctx, &mut complete.state, &mut actions, vec![enter()]);
        assert_eq!(
            actions,
            vec![Action::StartImportEach {
                destination: ImportDestination::NewBookForAll,
                each,
                skipped,
            }]
        );
        assert!(complete.state.import_state().is_none(), "Finish closes");
        drop(complete);

        let mut empty = bench("");
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        paint(&ctx, &mut empty.state, &mut actions, vec![enter()]);
        assert!(actions.is_empty(), "without a file, nothing to import");
        assert!(empty.state.import_state().is_some(), "nor closed");
        drop(empty);

        let mut cancelled = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        for _ in 0..3 {
            paint(&ctx, &mut cancelled.state, &mut actions, Vec::new());
        }
        assert!(cancelled.state.import_guide_open(), "the guide is open");
        paint(&ctx, &mut cancelled.state, &mut actions, vec![escape()]);
        assert!(!cancelled.state.import_guide_open(), "Escape closed it");
        assert!(
            cancelled.state.import_state().is_some(),
            "and not the window"
        );
        paint(&ctx, &mut cancelled.state, &mut actions, vec![escape()]);
        assert!(actions.is_empty(), "Cancel imports nothing");
        assert!(cancelled.state.import_state().is_none(), "Cancel closes");
    }

    /// The guide opens with the window until « Don't show again » is ticked,
    /// and « Show Guide » brings it back.
    #[test]
    fn the_guide_shows_until_dont_show_again_and_show_guide_brings_it_back() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        let step = |n: &str| tr!("import-guide-step", n = n, total = "3");
        let painted = settle(&ctx, &mut bench.state, &mut actions);
        assert!(painted.iter().any(|t| t.text == step("1")), "shown first");
        // Done without ticking: it comes back with the next window.
        for button in [
            tr!("import-guide-next"),
            tr!("import-guide-next"),
            tr!("import-guide-done"),
        ] {
            click_text(&ctx, &mut bench.state, &mut actions, &button);
        }
        assert!(!bench.state.import_guide_open());
        bench.state.cancel_import();
        bench.state.open_import(None::<stokesforge_core::NodeId>);
        assert!(bench.state.import_guide_open(), "every time until ticked");

        // Ticked: not shown again.
        bench.state.import_guide_dont_show_again();
        bench.state.cancel_import();
        bench.state.open_import(None::<stokesforge_core::NodeId>);
        bench.state.set_import_paths(bench.files.clone());
        let painted = settle(&ctx, &mut bench.state, &mut actions);
        assert!(!bench.state.import_guide_open());
        assert!(!painted.iter().any(|t| t.text == step("1")));
        // « Show Guide » brings it back, small, over the grid.
        click_text(
            &ctx,
            &mut bench.state,
            &mut actions,
            &tr!("import-guide-show"),
        );
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(bench.state.import_guide_open());
        assert!(painted.iter().any(|t| t.text == step("1")));
        // Its box starts ticked, as the preference has it.
        click_text(
            &ctx,
            &mut bench.state,
            &mut actions,
            &tr!("import-guide-dont"),
        );
        for button in [
            tr!("import-guide-next"),
            tr!("import-guide-next"),
            tr!("import-guide-done"),
        ] {
            click_text(&ctx, &mut bench.state, &mut actions, &button);
        }
        assert!(
            !bench.state.import_guide_hidden(),
            "unticked and closed: it comes back with the next window"
        );
    }

    /// The guide is a small card over the grid; a template window (Save as
    /// Template, Manage Templates) is a modal and the guide does not paint
    /// over it.
    #[test]
    fn the_guide_waits_behind_a_template_window() {
        use stokesforge_app_state::i18n::{self, Language};
        let _language = i18n::test_language(Language::English);
        let mut bench = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        let step = tr!("import-guide-step", n = "1", total = "3");
        let painted = settle(&ctx, &mut bench.state, &mut actions);
        assert!(painted.iter().any(|t| t.text == step), "shown first");
        bench.state.template_save_open();
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(bench.state.template_save_draft().is_some());
        assert!(
            !painted.iter().any(|t| t.text == step),
            "the guide is not painted over the Save window"
        );
        bench.state.template_save_cancel();
        bench.state.template_manage_open();
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(!painted.iter().any(|t| t.text == step));
        bench.state.template_manage_close();
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(painted.iter().any(|t| t.text == step), "and comes back");
    }

    /// The Advanced fold is closed by default; a setting changed there reads
    /// as a sentence in the left panel.
    #[test]
    fn the_advanced_fold_is_closed_by_default_and_a_change_shows_as_a_sentence() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        let painted = without_guide(&ctx, &mut bench.state, &mut actions);
        let label = tr!("import-advanced-skip-chars");
        assert!(painted
            .iter()
            .any(|t| t.text == tr!("import-advanced-title")));
        assert!(
            !painted.iter().any(|t| t.text == label),
            "closed: its rows are not painted"
        );
        click_text(
            &ctx,
            &mut bench.state,
            &mut actions,
            &tr!("import-advanced-title"),
        );
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(painted.iter().any(|t| t.text == label), "open: its rows");

        let sentence = tr!("import-advanced-sentence-skip-chars", count = 3usize);
        assert!(!painted.iter().any(|t| t.text.contains(&sentence)));
        bench
            .state
            .import_set_advanced(stokesforge_app_state::state::AdvancedChange::SkipChars(3));
        paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(
            painted.iter().any(|t| t.text.contains(&sentence)),
            "the changed setting is a sentence of the left panel"
        );
    }

    /// Forgetting a template asks first, in a modal confirmation: Enter
    /// cancels it (and does not import), the verb forgets.
    #[test]
    fn deleting_a_template_goes_through_a_modal_confirmation() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        without_guide(&ctx, &mut bench.state, &mut actions);
        bench.state.template_save_open();
        bench.state.template_save_commit();
        let name = bench.state.template_rows()[0].name.clone();
        bench.state.template_manage_open();
        for _ in 0..4 {
            paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        }
        click_text(
            &ctx,
            &mut bench.state,
            &mut actions,
            &tr!("import-template-forget-button"),
        );
        // The confirmation takes its focus over a few frames, then Enter
        // reads its default button.
        let mut painted = Vec::new();
        for _ in 0..4 {
            painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        }
        assert_eq!(bench.state.template_forget_pending(), Some(name.as_str()));
        assert!(painted
            .iter()
            .any(|t| t.text == tr!("import-template-forget-title")));
        paint(&ctx, &mut bench.state, &mut actions, vec![enter()]);
        assert!(
            bench.state.template_forget_pending().is_none(),
            "Enter = Cancel"
        );
        assert_eq!(bench.state.template_rows().len(), 1, "nothing forgotten");
        assert!(
            bench.state.import_state().is_some(),
            "the import stays open"
        );
        assert!(actions.is_empty(), "the import validated nothing");

        bench.state.template_forget_ask(&name);
        for _ in 0..3 {
            paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        }
        // The verb has the title's words: the button is painted after it.
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        let verb = painted
            .iter()
            .rev()
            .find(|t| t.text == tr!("import-template-forget-yes"))
            .expect("the verb is painted")
            .center;
        click_at(&ctx, &mut bench.state, &mut actions, verb);
        assert!(bench.state.template_rows().is_empty(), "the verb forgets");
        assert!(actions.is_empty());
    }

    /// Clicking the « auto » button of the grid only explains the automatic
    /// choice; it is dropped through the explicit « Drop » button of the card
    /// and never by the click that opens it.
    #[test]
    fn clicking_the_auto_button_never_deletes_the_choice() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        let painted = without_guide(&ctx, &mut bench.state, &mut actions);
        let pills = bench.state.import_auto_info().unwrap().pills.len();
        assert!(pills > 0, "the file gives automatic choices");
        let auto = tr!("import-auto-pill");
        let at = painted
            .iter()
            .find(|t| t.text == auto)
            .expect("an auto button is painted")
            .center;
        click_at(&ctx, &mut bench.state, &mut actions, at);
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(painted
            .iter()
            .any(|t| t.text == tr!("import-auto-pick-why-title")));
        assert_eq!(
            bench.state.import_auto_info().unwrap().pills.len(),
            pills,
            "the click kept the choice"
        );
        click_text(
            &ctx,
            &mut bench.state,
            &mut actions,
            &tr!("import-auto-pick-drop"),
        );
        paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert_eq!(
            bench.state.import_auto_info().unwrap().pills.len(),
            pills - 1,
            "the explicit action drops it"
        );
    }

    /// A point to check shows its sentence with « Mark as Checked » beside
    /// it; marking settles that point only, and Import stays possible.
    #[test]
    fn a_point_to_check_is_marked_beside_its_sentence() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(DOUBTS);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        let painted = without_guide(&ctx, &mut bench.state, &mut actions);
        let points = bench.state.to_check();
        assert!(points.len() >= 2, "{points:?}");
        let title = tr!("import-check-warn-title", count = points.len());
        assert!(painted.iter().any(|t| t.text == title), "{title}");
        assert!(painted
            .iter()
            .any(|t| t.text == tr!("import-check-warn-check-now")));
        let mark = tr!("import-check-mark-checked");
        let at = painted
            .iter()
            .find(|t| t.text == mark)
            .expect("Mark as Checked is painted")
            .center;
        click_at(&ctx, &mut bench.state, &mut actions, at);
        assert_eq!(
            bench.state.to_check().len(),
            points.len() - 1,
            "one point settled, the others stay"
        );
        paint(&ctx, &mut bench.state, &mut actions, vec![enter()]);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::StartImportEach { .. })),
            "the points left do not block the import"
        );
    }

    /// In the pseudo-locale (texts 30 % longer) nothing of the left panel is
    /// cut at its edge: « Mark as Checked » of the « To check » block and the
    /// « To check » tags of the rows stay inside the panel, whose edge is the
    /// one of the frame of the « To check » block.
    #[test]
    fn long_texts_of_the_left_panel_are_never_cut() {
        use stokesforge_app_state::i18n::{self, Language};
        let _language = i18n::test_language(Language::PseudoEnglish);
        let mut bench = bench(DOUBTS);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        // The default window of the application, 1100 x 700.
        let mut frame = |events: Vec<egui::Event>| {
            crate::app::test_support::frame_of(&ctx, egui::vec2(1100.0, 700.0), events, |ui| {
                super::import_window(ui.ctx(), &mut bench.state, &mut actions)
            })
        };
        frame(Vec::new());
        // The size the window takes on that screen: the left panel keeps its
        // least width, 380 points.
        crate::app::dialogs::set_dialog_size(
            &ctx,
            egui::Id::new("stokesforge_import_wizard"),
            egui::vec2(1040.0, 640.0),
        );
        for _ in 0..3 {
            frame(Vec::new());
        }
        frame(vec![escape()]);
        for _ in 0..3 {
            frame(Vec::new());
        }
        let shapes = frame(Vec::new()).shapes;
        let frame_right = shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Rect(rect)
                    if rect.stroke.width == 1.5 && rect.fill != egui::Color32::TRANSPARENT =>
                {
                    Some(rect.rect.right())
                }
                _ => None,
            })
            .fold(f32::NAN, f32::max);
        assert!(frame_right.is_finite(), "the frame of the block is painted");
        let wanted = [
            tr!("import-check-mark-checked"),
            tr!("import-check-tag-check"),
        ];
        let mut seen = 0;
        for painted in &shapes {
            let egui::Shape::Text(text) = &painted.shape else {
                continue;
            };
            if !wanted.contains(&text.galley.job.text) {
                continue;
            }
            seen += 1;
            let right = text.pos.x + text.galley.rect.right();
            assert!(
                right <= frame_right + 0.5 && right <= painted.clip_rect.right() + 0.5,
                "{:?} ends at {right}, past the panel edge {frame_right} or its clip edge {}",
                text.galley.job.text,
                painted.clip_rect.right(),
            );
        }
        assert!(seen >= 2, "both texts are painted, saw {seen}");
        // The buttons at the end of the top rows (« Add Files… » of the file
        // row, « Show Guide » of the grid tabs) are not pushed out of the
        // window by longer texts: the file row keeps the room its end needs.
        let buttons = [tr!("import-files-add-files"), tr!("import-guide-show")];
        let mut seen = 0;
        for painted in &shapes {
            let egui::Shape::Text(text) = &painted.shape else {
                continue;
            };
            if !buttons.iter().any(|w| text.galley.job.text.trim_end() == w) {
                continue;
            }
            seen += 1;
            let right = text.pos.x + text.galley.rect.right();
            assert!(
                right <= painted.clip_rect.right() + 0.5,
                "{:?} ends at {right}, past its clip edge {}",
                text.galley.job.text,
                painted.clip_rect.right(),
            );
        }
        assert!(seen >= 2, "both buttons are painted, saw {seen}");
    }

    /// Two templates that fit the file as well make a point to check that
    /// offers the other one.
    #[test]
    fn a_tie_between_templates_offers_the_other_one() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(SURE);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        without_guide(&ctx, &mut bench.state, &mut actions);
        for name in ["Alpha", "Beta"] {
            bench.state.template_save_open();
            bench.state.template_save_draft_mut().unwrap().name = name.to_string();
            bench.state.template_save_commit();
        }
        bench.state.template_use_auto();
        paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        let points = bench.state.to_check();
        let tie = points
            .iter()
            .find_map(|p| match &p.source {
                stokesforge_app_state::state::ToCheckSource::Tie { others } => Some(others.clone()),
                _ => None,
            })
            .expect("a tie is a point to check");
        let other = tie[0].clone();
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        let label = tr!("import-tie-use", name = other.clone());
        let at = painted
            .iter()
            .find(|t| t.text == label)
            .expect("the other template is offered")
            .center;
        click_at(&ctx, &mut bench.state, &mut actions, at);
        assert_eq!(bench.state.template_in_use(), Some(other));
    }

    /// Manage Templates lists the answers remembered per kind of file, each
    /// with a Forget action that brings the doubt back.
    #[test]
    fn manage_templates_lists_the_remembered_answers_and_forgets_one() {
        let _language = stokesforge_app_state::i18n::test_language(
            stokesforge_app_state::i18n::Language::English,
        );
        let mut bench = bench(DOUBTS);
        let ctx = crate::app::test_support::context();
        let mut actions = Vec::new();
        without_guide(&ctx, &mut bench.state, &mut actions);
        let before = bench.state.to_check_count();
        assert!(before > 0);
        let first = bench.state.to_check()[0].id.clone();
        bench.state.to_check_mark(&first);
        assert_eq!(bench.state.to_check_count(), before - 1);
        let answers = bench.state.remembered_answers();
        assert_eq!(answers.len(), 1, "one answer is remembered");
        bench.state.template_manage_open();
        paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        let painted = paint(&ctx, &mut bench.state, &mut actions, Vec::new());
        assert!(painted
            .iter()
            .any(|t| t.text == tr!("import-answers-title")));
        assert!(painted
            .iter()
            .any(|t| t.text.contains(&answers[0].doubt_text)));
        click_text(
            &ctx,
            &mut bench.state,
            &mut actions,
            &tr!("import-answers-forget"),
        );
        assert!(bench.state.remembered_answers().is_empty());
        assert_eq!(bench.state.to_check_count(), before, "the doubt is back");
    }
}

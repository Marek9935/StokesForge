//! The **Apply Theme** window (#541): the picker of themes, the four boxes of
//! what to apply, and a preview of the first target with the theme applied.
//!
//! Type: **creation on a draft** (`DialogNature::Draft`, Large): `[Cancel]
//! [Apply]`, the button carries its verb, one undo step, Esc and Cancel close
//! it without a trace. The state (targets, chosen theme, ticked families, the
//! hovered override) is `AppState::apply_theme_window`; this file paints it and
//! writes the clicks back (principle 8).
//!
//! # Hover first
//!
//! What the pointer rests on is previewed **before** any click, as in the import
//! window: a tile shows that theme, a box shows the choice with that box
//! flipped; the pointer on the preview itself shows the graph **Before**. Only
//! a click changes the choice. Right after a click on a box the preview keeps
//! the new state until the pointer leaves the box (otherwise it would show the
//! effect of clicking again).
//!
//! # The boxes
//!
//! Style (a three-state box that unfolds into its six parts), Size, Texts, Axis
//! ranges, then « Add missing layers » (off). Each carries a « ? » help.

use eframe::egui;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::theme_window::{
    style_state, ApplyThemeWindow, StyleState, ThemeSource, STYLE_PARTS,
};
use stokesforge_app_state::tr;
use stokesforge_plot::{ThemeFamilies, ThemeFamily};

use super::dialogs::{
    modal_dialog_with, DialogFooter, DialogNature, DialogOutcome, InteractiveSize,
};
use super::hover_help::help_beside;
use super::theme_picker::{retain_listed, theme_picker, DrawKey, Drawings};
use crate::theme;

const WINDOW_ID: &str = "stokesforge_apply_theme";

/// Widest the left column (picker and boxes) grows, and the narrowest it
/// shrinks to so that the preview always keeps room.
const LEFT_MAX: f32 = 300.0;
const LEFT_MIN: f32 = 210.0;
/// Most the preview enlarges a small figure.
const PREVIEW_MAX_SCALE: f32 = 1.5;

/// Paints the window while it is open; hands the drawings back to the
/// application when it closes.
pub(super) fn apply_theme_window(
    ctx: &egui::Context,
    state: &mut AppState,
    drawings: &mut Option<Drawings>,
    actions: &mut Vec<Action>,
) {
    let Some(opened) = state.apply_theme_window().cloned() else {
        if let Some(set) = drawings {
            set.retain(|key| !matches!(key, DrawKey::Preview(_)));
        }
        return;
    };
    let drawings = drawings.get_or_insert_with(Drawings::new);
    let entries = state.picker_entries();
    retain_listed(drawings, &entries);
    let mut window = opened.clone();
    let reason = window
        .apply_blocked()
        .unwrap_or_else(|| tr!("error-theme-unknown"));
    let usable = window.apply_blocked().is_none() && state.theme_of(window.chosen()).is_some();
    let verb = tr!("theme-apply-verb");
    let hint = tr!("theme-apply-verb", "hint");
    // Type: creation on a draft (S-F1), `[Cancel] [Apply]`.
    let outcome = modal_dialog_with(
        ctx,
        egui::Id::new(WINDOW_ID),
        &tr!("theme-apply-title"),
        // FREE-SIZE(#300): the picker (three rows of tiles), the boxes under it and a preview beside them need about 700 x 560, beyond the Large class
        DialogNature::Draft(InteractiveSize::Free(egui::vec2(700.0, 560.0))),
        DialogFooter::gated(&verb, &hint, usable, &reason),
        |_| {},
        |ui| contents(ui, &*state, drawings, &entries, &mut window),
    );
    if window != opened {
        if let Some(stored) = state.apply_theme_window_mut() {
            *stored = window;
        }
    }
    match outcome {
        DialogOutcome::Open => {}
        DialogOutcome::Accept => actions.push(Action::ConfirmApplyTheme),
        DialogOutcome::Cancel => actions.push(Action::CloseApplyTheme),
    }
}

/// What the pointer rests on in the left column: it drives the preview.
#[derive(Default)]
struct Hover {
    theme: Option<ThemeSource>,
    families: Option<ThemeFamilies>,
    add_missing: Option<bool>,
}

fn contents(
    ui: &mut egui::Ui,
    state: &AppState,
    drawings: &mut Drawings,
    entries: &stokesforge_app_state::theme_window::PickerEntries,
    window: &mut ApplyThemeWindow,
) {
    let mut hover = Hover::default();
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        let left = (ui.available_width() * 0.5).clamp(LEFT_MIN, LEFT_MAX);
        ui.allocate_ui_with_layout(
            egui::vec2(left, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                theme::page_scroll_area()
                    .id_salt("apply_theme_left")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut chosen = window.chosen().clone();
                        hover.theme = theme_picker(ui, "apply", drawings, entries, &mut chosen);
                        if chosen != *window.chosen() {
                            window.choose(chosen);
                        }
                        theme::section_title(ui, tr!("theme-what-to-apply"));
                        boxes(ui, window, &mut hover);
                    });
            },
        );
        // The hovered override is part of the window: the preview reads it.
        window.set_hover(hover.theme.take(), hover.families, hover.add_missing);
        ui.vertical(|ui| preview(ui, state, drawings, window));
    });
}

/// A box whose hover previews what its click would give; right after a click
/// the preview keeps the new state until the pointer leaves the box.
fn hover_flip<T>(
    ui: &egui::Ui,
    response: &egui::Response,
    clicked: bool,
    flipped: T,
    hover: &mut Option<T>,
) {
    let quiet = egui::Id::new("apply_theme_box_clicked");
    if clicked {
        ui.data_mut(|data| data.insert_temp(quiet, response.id));
    }
    let after_click = ui.data(|data| data.get_temp::<egui::Id>(quiet)) == Some(response.id);
    if response.hovered() {
        if !after_click {
            *hover = Some(flipped);
        }
    } else if after_click {
        ui.data_mut(|data| data.remove::<egui::Id>(quiet));
    }
}

/// The label and the « ? » help of a family box.
fn texts_of(family: ThemeFamily) -> (String, String) {
    match family {
        ThemeFamily::Background => (
            tr!("theme-part-background"),
            tr!("theme-part-background", "hint"),
        ),
        ThemeFamily::TitleFonts => (
            tr!("theme-part-title-fonts"),
            tr!("theme-part-title-fonts", "hint"),
        ),
        ThemeFamily::Axes => (tr!("theme-part-axes"), tr!("theme-part-axes", "hint")),
        ThemeFamily::Legend => (tr!("theme-part-legend"), tr!("theme-part-legend", "hint")),
        ThemeFamily::Curves => (tr!("theme-part-curves"), tr!("theme-part-curves", "hint")),
        ThemeFamily::ColorBar => (
            tr!("theme-part-color-bar"),
            tr!("theme-part-color-bar", "hint"),
        ),
        ThemeFamily::Texts => (tr!("theme-what-texts"), tr!("theme-what-texts", "hint")),
        ThemeFamily::SizeAndPage => (tr!("theme-what-size"), tr!("theme-what-size", "hint")),
        ThemeFamily::AxisRanges => (
            tr!("theme-what-axis-ranges"),
            tr!("theme-what-axis-ranges", "hint"),
        ),
    }
}

/// Width the fold arrow of a header row takes (the gap after it is the item gap).
fn fold_arrow_width(ui: &egui::Ui) -> f32 {
    ui.spacing().icon_width.min(ui.spacing().interact_size.y)
}

/// One box that is a leaf: Size, Texts, Axis ranges and each part of Style.
fn family_box(
    ui: &mut egui::Ui,
    window: &mut ApplyThemeWindow,
    family: ThemeFamily,
    indented: bool,
    hover: &mut Hover,
) {
    let (label, hint) = texts_of(family);
    let mut on = window.families().contains(family);
    let response = if indented {
        // Under the box of Style: the width of the fold arrow of its row.
        ui.horizontal(|ui| {
            ui.add_space(fold_arrow_width(ui));
            help_beside(ui, hint, |ui| ui.checkbox(&mut on, label))
        })
        .inner
    } else {
        help_beside(ui, hint, |ui| ui.checkbox(&mut on, label))
    };
    let flipped = window.families_if_family_flipped(family);
    if response.changed() {
        window.set_family(family, on);
    }
    hover_flip(
        ui,
        &response,
        response.changed(),
        flipped,
        &mut hover.families,
    );
}

/// The four boxes and « Add missing layers ».
fn boxes(ui: &mut egui::Ui, window: &mut ApplyThemeWindow, hover: &mut Hover) {
    let state = style_state(window.families());
    let fold = ui.make_persistent_id("apply_theme_style_fold");
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), fold, false)
        .show_header(ui, |ui| {
            let mut on = state == StyleState::All;
            let response = help_beside(ui, tr!("theme-what-style", "hint"), |ui| {
                ui.add(
                    egui::Checkbox::new(&mut on, tr!("theme-what-style"))
                        .indeterminate(state == StyleState::Mixed),
                )
            });
            let flipped = window.families_if_style_flipped();
            if response.changed() {
                window.set_style(state != StyleState::All);
            }
            hover_flip(
                ui,
                &response,
                response.changed(),
                flipped,
                &mut hover.families,
            );
        })
        .body(|ui| {
            for family in STYLE_PARTS {
                family_box(ui, window, family, false, hover);
            }
        });
    family_box(ui, window, ThemeFamily::SizeAndPage, true, hover);
    family_box(ui, window, ThemeFamily::Texts, true, hover);
    family_box(ui, window, ThemeFamily::AxisRanges, true, hover);
    let mut on = window.add_missing();
    let response = ui
        .horizontal(|ui| {
            ui.add_space(fold_arrow_width(ui));
            help_beside(ui, tr!("theme-add-missing", "hint"), |ui| {
                ui.checkbox(&mut on, tr!("theme-add-missing"))
            })
        })
        .inner;
    let flipped = !window.add_missing();
    if response.changed() {
        window.set_add_missing(on);
    }
    hover_flip(
        ui,
        &response,
        response.changed(),
        flipped,
        &mut hover.add_missing,
    );
}

/// The preview column: a caption (After or Before), the first target drawn
/// through the ordinary rendering at its own size then scaled whole, a note
/// when there are several targets. The pointer on the preview shows Before.
fn preview(
    ui: &mut egui::Ui,
    state: &AppState,
    drawings: &mut Drawings,
    window: &ApplyThemeWindow,
) {
    let width = ui.available_width().max(120.0);
    let caption_height = ui.text_style_height(&egui::TextStyle::Small);
    let (caption, _) =
        ui.allocate_exact_size(egui::vec2(width, caption_height), egui::Sense::hover());
    let note = window.graphs().len() > 1;
    let note_height = if note {
        caption_height + theme::SPACE_ITEM
    } else {
        0.0
    };
    let box_height = (ui.available_height() - note_height).max(80.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, box_height), egui::Sense::hover());
    let before = response.hovered();
    // A child over the caption's strip: `ui.put` would move the cursor back
    // under the caption, and the note below would land above the figure.
    ui.new_child(egui::UiBuilder::new().max_rect(caption))
        .label(theme::secondary_text(if before {
            tr!("theme-preview-before")
        } else {
            tr!("theme-preview-after")
        }));
    let drawn = state.apply_preview_for(window).and_then(|preview| {
        let spec = if before {
            preview.before
        } else {
            preview.after
        };
        let key = DrawKey::Preview(!before);
        let ctx = ui.ctx().clone();
        drawings.draw_scaled(
            &ctx,
            &ui.painter_at(rect),
            rect,
            PREVIEW_MAX_SCALE,
            key,
            state.tree(),
            &spec,
            &state.figure_texts(),
        )
    });
    if drawn.is_none() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr!("theme-preview-failed"),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
    }
    if note {
        ui.label(theme::secondary_text(tr!(
            "theme-preview-first",
            count = (window.graphs().len() as i64)
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable};
    use stokesforge_app_state::user_themes::ThemeRef;

    struct Bench {
        _session: stokesforge_session::Session,
        _dir: tempfile::TempDir,
        state: AppState,
        drawings: Option<Drawings>,
        seen: Vec<Action>,
    }

    fn graphs(state: &AppState) -> Vec<stokesforge_core::NodeId> {
        state.graphs().into_iter().map(|(id, _)| id).collect()
    }

    /// An Apply Theme window on `count` graphs of a fresh project, in a
    /// `size` screen; every action it pushes is dispatched and remembered.
    fn bench(count: usize, size: egui::Vec2) -> egui_kittest::Harness<'static, Bench> {
        let dir = tempfile::tempdir().unwrap();
        let session = stokesforge_session::Session::spawn(
            stokesforge_core::Project::new("Themes").unwrap(),
            None,
        )
        .unwrap();
        let mut state = AppState::new_with_preferences(
            session.handle(),
            String::new(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        state.use_user_themes_file(dir.path().join("themes.json"));
        for _ in 0..count {
            state.dispatch(Action::AddGraph);
        }
        let targets = graphs(&state);
        state.dispatch(Action::OpenApplyTheme { graphs: targets });
        let mut harness = egui_kittest::Harness::builder()
            .with_size(size)
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut actions = Vec::new();
                    apply_theme_window(
                        ui.ctx(),
                        &mut bench.state,
                        &mut bench.drawings,
                        &mut actions,
                    );
                    for action in actions {
                        bench.seen.push(action.clone());
                        bench.state.dispatch(action);
                    }
                },
                Bench {
                    _session: session,
                    _dir: dir,
                    state,
                    drawings: None,
                    seen: Vec::new(),
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn window(harness: &egui_kittest::Harness<'static, Bench>) -> ApplyThemeWindow {
        harness
            .state()
            .state
            .apply_theme_window()
            .expect("the window is open")
            .clone()
    }

    /// The tile named `name` (its name is also read by the accessibility tree
    /// of the drop-down and of the large view: the button is the tile).
    fn tile_of<'a>(
        harness: &'a egui_kittest::Harness<'static, Bench>,
        name: &'a str,
    ) -> egui_kittest::Node<'a> {
        harness
            .get_all_by_label(name)
            .find(|node| node.accesskit_node().role() == egui::accesskit::Role::Button)
            .expect("a tile")
    }

    /// Esc closes the Apply Theme window and nothing else: no other action,
    /// the copied theme and the history are untouched.
    #[test]
    fn escape_closes_only_the_apply_window() {
        let mut harness = bench(2, egui::vec2(900.0, 700.0));
        let graph = graphs(&harness.state().state)[0];
        harness
            .state_mut()
            .state
            .dispatch(Action::CopyTheme { graph });
        let undo = harness.state().state.undo_label();
        harness.key_press(egui::Key::Escape);
        harness.run();
        assert_eq!(harness.state().seen, [Action::CloseApplyTheme]);
        assert!(harness.state().state.apply_theme_window().is_none());
        assert!(harness.state().state.theme_clipboard().is_some());
        assert_eq!(harness.state().state.undo_label(), undo);
    }

    /// A click on a tile chooses it; resting on one only previews it.
    #[test]
    fn a_click_chooses_a_tile_and_resting_on_it_only_previews_it() {
        let mut harness = bench(1, egui::vec2(900.0, 700.0));
        let dark = ThemeSource::Theme(ThemeRef::Builtin(1));
        let light = ThemeSource::Theme(ThemeRef::Builtin(0));
        assert_eq!(window(&harness).chosen(), &light);
        let at = tile_of(&harness, &tr!("plot-theme-dark")).rect().center();
        harness.hover_at(at);
        harness.run();
        let hovered = window(&harness);
        assert_eq!(
            hovered.shown_theme(),
            &dark,
            "the preview follows the pointer"
        );
        assert_eq!(hovered.chosen(), &light, "the choice does not");
        tile_of(&harness, &tr!("plot-theme-dark")).click();
        harness.run();
        assert_eq!(window(&harness).chosen(), &dark);
    }

    /// Unticking every box greys Apply and the footer says what to do.
    #[test]
    fn unticking_every_box_greys_apply_with_its_reason() {
        let mut harness = bench(1, egui::vec2(900.0, 700.0));
        for label in [
            tr!("theme-what-style"),
            tr!("theme-what-size"),
            tr!("theme-what-texts"),
        ] {
            harness.get_by_label(&label).click();
            harness.run();
        }
        assert!(window(&harness).families().is_empty());
        harness.get_by_label(&tr!("theme-apply-nothing-checked"));
        assert!(harness
            .get_by_label(&tr!("theme-apply-verb"))
            .accesskit_node()
            .is_disabled());
    }

    /// The tiles draw their specimen once; a later frame hits the cache, and
    /// a theme that goes takes its drawing with it.
    #[test]
    fn the_specimens_are_drawn_once_and_let_go_with_their_theme() {
        let mut harness = bench(1, egui::vec2(900.0, 700.0));
        let drawn = harness.state().drawings.as_ref().expect("drawn").draws();
        assert!(drawn >= 5, "five tiles and the preview: {drawn}");
        harness.hover_at(egui::pos2(5.0, 5.0));
        harness.run();
        assert_eq!(harness.state().drawings.as_ref().unwrap().draws(), drawn);
        // A project theme gets a tile (and a drawing); deleting it lets go.
        let target = graphs(&harness.state().state)[0];
        let before = harness.state().drawings.as_ref().unwrap().len();
        harness.state_mut().state.dispatch(Action::AddProjectTheme {
            graph: target,
            name: "Project one".into(),
        });
        harness.run();
        harness.run();
        let with = harness.state().drawings.as_ref().unwrap().len();
        assert!(
            with > before,
            "a tile for the new theme: {before} -> {with}"
        );
        harness
            .state_mut()
            .state
            .dispatch(Action::DeleteProjectTheme {
                name: "Project one".into(),
            });
        harness.run();
        harness.run();
        assert_eq!(
            harness.state().drawings.as_ref().unwrap().len(),
            before,
            "its drawing is let go"
        );
    }

    /// With several targets the preview says it shows the first.
    #[test]
    fn several_targets_say_which_one_is_previewed() {
        let harness = bench(2, egui::vec2(900.0, 700.0));
        harness.get_by_label(&tr!("theme-preview-first", count = 2_i64));
    }

    /// A narrow screen still leaves the preview its room.
    #[test]
    fn a_narrow_screen_still_shows_the_preview() {
        let harness = bench(1, egui::vec2(480.0, 640.0));
        // The caption sits at the left edge of the preview column.
        let caption = harness.get_by_label(&tr!("theme-preview-after")).rect();
        assert!(480.0 - caption.left() >= 200.0, "{caption:?}");
    }
}

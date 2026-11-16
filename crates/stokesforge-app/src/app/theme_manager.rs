//! The Theme tab of Graph Properties as a **theme manager** (#542, S5 of #514):
//! the shared theme picker, then what can be done with the selected theme.
//!
//! # Layout
//!
//! The picker (rows Project and Mine, specimen tiles, More themes) comes first
//! and chooses the theme. Under it, in one wrapping row of standard buttons:
//! Save Theme As… (the style of this graph, as the window holds it, under a
//! new name), Rename…, Update From This Graph, Reset (an edited built-in) and
//! Delete (destructive look, S-R5). Then the box « Use for new graphs » with
//! its « ? », then Apply Theme…, which opens the Apply Theme window **into the
//! open draft**: the OK or Cancel of Graph Properties decides.
//!
//! Every greyed control says why on hover; the reasons are decided by
//! [`AppState::theme_buttons`], never here (principle 8). A modified built-in
//! carries its distinctive mark on its tile (the accent dot of the picker).
//!
//! The page asks, it does not do: every click becomes an [`Action`] that the
//! caller dispatches once the page is painted.

use eframe::egui;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::theme_window::{PickerEntries, ThemeButtons, ThemeSource};
use stokesforge_app_state::tr;
use stokesforge_app_state::user_themes::ThemeRef;
use stokesforge_core::NodeId;

use super::cursors;
use super::hover_help::help_beside;
use super::theme_picker::{theme_picker, Drawings};
use crate::theme;

/// What the page needs from the state, read before the draft is borrowed.
pub(super) struct ThemeTab {
    graph: NodeId,
    entries: PickerEntries,
    selected: ThemeSource,
    /// What each listed theme allows, so that a click on a tile moves the
    /// buttons in the same frame.
    buttons: Vec<(ThemeSource, ThemeButtons)>,
    default_theme: Option<ThemeRef>,
}

impl ThemeTab {
    /// Reads the state for the Theme tab of `graph`. A selection that no longer
    /// exists (a deleted theme) falls back to Light.
    pub(super) fn read(state: &AppState, graph: NodeId) -> Self {
        let entries = state.picker_entries();
        let mut selected = state.theme_selection().clone();
        if !entries.sources().any(|source| *source == selected) {
            selected = ThemeSource::Theme(ThemeRef::Builtin(0));
        }
        let buttons = entries
            .sources()
            .map(|source| (source.clone(), state.theme_buttons(source)))
            .collect();
        let default_theme = state.new_graph_theme().map(|(reference, _)| reference);
        Self {
            graph,
            entries,
            selected,
            buttons,
            default_theme,
        }
    }

    /// What the selected theme allows.
    fn buttons_for(&self) -> ThemeButtons {
        self.buttons
            .iter()
            .find(|(source, _)| *source == self.selected)
            .map(|(_, buttons)| buttons.clone())
            // The selection is always a listed theme (`ThemeTab::read`); a
            // theme that vanished anyway leaves everything greyed, saying so.
            .unwrap_or_else(|| {
                let gone = || Some(tr!("error-theme-unknown"));
                ThemeButtons {
                    rename: gone(),
                    update: gone(),
                    reset: gone(),
                    delete: gone(),
                    use_for_new: gone(),
                }
            })
    }

    /// The selection the page ended with (to give back to the state).
    pub(super) fn selected(&self) -> &ThemeSource {
        &self.selected
    }
}

/// A standard button that says why it is greyed.
fn action_button(
    ui: &mut egui::Ui,
    label: String,
    hint: String,
    reason: &Option<String>,
) -> egui::Response {
    cursors::clickable(
        ui.add_enabled(reason.is_none(), egui::Button::new(label))
            .on_hover_text(hint)
            .on_disabled_hover_text(reason.clone().unwrap_or_default()),
    )
}

/// Paints the page; clicks go to `actions`.
///
/// The picker scrolls in the room that is left; the controls stay at the foot
/// of the page, in view whatever the number of themes (a page with a Project
/// row and a Mine row is taller than the window) and whatever the width (the
/// buttons wrap).
pub(super) fn theme_manager(
    ui: &mut egui::Ui,
    drawings: &mut Drawings,
    tab: &mut ThemeTab,
    actions: &mut Vec<Action>,
) {
    let before = tab.selected.clone();
    // The controls follow the choice of the previous frame; a click on a tile
    // repaints at once.
    let selected_ref = match &before {
        ThemeSource::Theme(reference) => Some(reference.clone()),
        ThemeSource::Copied => None,
    };
    let buttons = tab.buttons_for();
    // The height the controls took in the previous frame: they wrap, so it is
    // known only once they are painted (a bottom-up layout does not grow a
    // wrapping row upward).
    let height_id = ui.id().with("theme_controls_height");
    let controls_height: f32 = ui.data(|data| data.get_temp(height_id)).unwrap_or(60.0);
    // Each allocation is followed by one item spacing: the picker and the gap
    // take two of them before the controls, which would otherwise be cut by
    // the footer line.
    let gaps = theme::SPACE_ITEM + 2.0 * ui.spacing().item_spacing.y;
    let picker_height = (ui.available_height() - controls_height - gaps).max(80.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), picker_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            theme::page_scroll_area()
                .id_salt("theme_tab_picker")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    theme_picker(ui, "tab", drawings, &tab.entries, &mut tab.selected);
                });
        },
    );
    ui.add_space(theme::SPACE_ITEM);
    let controls = ui
        .scope(|ui| {
            button_row(ui, tab, &selected_ref, &buttons, actions);
            ui.add_space(theme::SPACE_ITEM);
            use_and_apply_row(ui, tab, &selected_ref, &buttons, actions);
        })
        .response
        .rect;
    if (controls.height() - controls_height).abs() > 0.5 {
        ui.data_mut(|data| data.insert_temp(height_id, controls.height()));
        ui.ctx().request_repaint();
    }
    if tab.selected != before {
        ui.ctx().request_repaint();
    }
}

/// Save Theme As…, Rename…, Update From This Graph, Reset and Delete: one row
/// of standard buttons that wraps whole when the window is narrow.
fn button_row(
    ui: &mut egui::Ui,
    tab: &ThemeTab,
    selected: &Option<ThemeRef>,
    buttons: &ThemeButtons,
    actions: &mut Vec<Action>,
) {
    ui.horizontal_wrapped(|ui| {
        // A button that does not fit goes to the next row whole.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.spacing_mut().item_spacing.x = theme::SPACE_ITEM;
        if ui
            .button(tr!("action-save-theme-as"))
            .on_hover_text(tr!("action-save-theme-as", "hint"))
            .clicked()
        {
            actions.push(Action::OpenSaveTheme { graph: tab.graph });
        }
        if action_button(
            ui,
            tr!("theme-mgr-rename"),
            tr!("theme-mgr-rename", "hint"),
            &buttons.rename,
        )
        .clicked()
        {
            if let Some(theme) = selected.clone() {
                actions.push(Action::OpenRenameTheme { theme });
            }
        }
        if action_button(
            ui,
            tr!("theme-mgr-update"),
            tr!("theme-mgr-update", "hint"),
            &buttons.update,
        )
        .clicked()
        {
            if let Some(theme) = selected.clone() {
                actions.push(Action::UpdateTheme {
                    theme,
                    graph: tab.graph,
                });
            }
        }
        if action_button(
            ui,
            tr!("theme-mgr-reset"),
            tr!("theme-mgr-reset", "hint"),
            &buttons.reset,
        )
        .clicked()
        {
            if let Some(theme) = selected.clone() {
                actions.push(Action::ResetTheme { theme });
            }
        }
        // Delete: the destructive look (S-R5), red on hover and on focus.
        let delete = ui
            .scope(|ui| {
                theme::destructive_button_visuals(ui.visuals_mut());
                action_button(
                    ui,
                    tr!("theme-mgr-delete"),
                    tr!("theme-mgr-delete", "hint"),
                    &buttons.delete,
                )
            })
            .inner;
        if delete.clicked() {
            if let Some(theme) = selected.clone() {
                actions.push(Action::DeleteTheme { theme });
            }
        }
    });
}

/// « Use for new graphs » with its « ? », and Apply Theme… opening the window
/// into the draft.
fn use_and_apply_row(
    ui: &mut egui::Ui,
    tab: &ThemeTab,
    selected: &Option<ThemeRef>,
    buttons: &ThemeButtons,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        let mut for_new = selected.is_some() && tab.default_theme == *selected;
        let response = help_beside(ui, tr!("theme-mgr-use-for-new", "hint"), |ui| {
            ui.add_enabled(
                buttons.use_for_new.is_none(),
                egui::Checkbox::new(&mut for_new, tr!("theme-mgr-use-for-new")),
            )
            .on_disabled_hover_text(buttons.use_for_new.clone().unwrap_or_default())
        });
        if response.changed() {
            actions.push(Action::SetNewGraphTheme(if for_new {
                selected.clone()
            } else {
                None
            }));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if cursors::clickable(
                ui.button(tr!("action-apply-theme"))
                    .on_hover_text(tr!("action-apply-theme", "hint")),
            )
            .clicked()
            {
                actions.push(Action::OpenApplyThemeIntoDraft { graph: tab.graph });
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable};
    use stokesforge_app_state::state::PlotSettingsTab;
    use stokesforge_app_state::theme_window::ThemeTarget;

    struct Bench {
        _session: stokesforge_session::Session,
        _dir: tempfile::TempDir,
        state: AppState,
        drawings: Option<Drawings>,
        graph: NodeId,
    }

    /// Graph Properties open on the Theme tab of a fresh graph, with the
    /// windows and the confirmations the application paints around it; every
    /// action they push is dispatched.
    fn bench() -> egui_kittest::Harness<'static, Bench> {
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
        state.dispatch(Action::AddGraph);
        let graph = state.graphs().last().unwrap().0;
        state.dispatch(Action::OpenPlotSettings {
            graph,
            tab: PlotSettingsTab::Theme,
            axis: None,
        });
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 800.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut actions = Vec::new();
                    super::super::apply_theme_window::apply_theme_window(
                        ui.ctx(),
                        &mut bench.state,
                        &mut bench.drawings,
                        &mut actions,
                    );
                    super::super::save_theme_window::save_theme_window(
                        ui.ctx(),
                        &mut bench.state,
                        &mut actions,
                    );
                    super::super::rename_theme_window::rename_theme_window(
                        ui.ctx(),
                        &mut bench.state,
                        &mut actions,
                    );
                    super::super::confirmations::theme_delete_window(
                        ui.ctx(),
                        &bench.state,
                        &mut actions,
                    );
                    super::super::plot_settings::plot_settings_window(
                        ui.ctx(),
                        &mut bench.state,
                        &mut bench.drawings,
                    );
                    for action in actions {
                        bench.state.dispatch(action);
                    }
                },
                Bench {
                    _session: session,
                    _dir: dir,
                    state,
                    drawings: None,
                    graph,
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn save_user(harness: &mut egui_kittest::Harness<'static, Bench>, name: &str) -> ThemeRef {
        let graph = harness.state().graph;
        harness.state_mut().state.dispatch(Action::SaveThemeAs {
            graph,
            name: name.into(),
            target: ThemeTarget::Mine,
        });
        harness.run();
        harness.run();
        match harness.state().state.theme_selection() {
            ThemeSource::Theme(reference) => reference.clone(),
            ThemeSource::Copied => panic!("a theme was selected"),
        }
    }

    /// On an unmodified built-in the buttons that cannot act are greyed, the
    /// ones that can are not; the reason is the state's (one place).
    #[test]
    fn the_buttons_are_greyed_by_what_the_selected_theme_allows() {
        let harness = bench();
        let greyed = |label: String| harness.get_by_label(&label).accesskit_node().is_disabled();
        assert!(greyed(tr!("theme-mgr-reset")), "Light is unmodified");
        assert!(greyed(tr!("theme-mgr-delete")), "a built-in stays");
        assert!(greyed(tr!("theme-mgr-rename")), "a built-in keeps its name");
        assert!(!greyed(tr!("theme-mgr-update")));
        assert!(!greyed(tr!("action-save-theme-as")));
        assert!(!greyed(tr!("action-apply-theme")));
        let why = harness
            .state()
            .state
            .theme_buttons(&ThemeSource::Theme(ThemeRef::Builtin(0)));
        assert_eq!(why.reset, Some(tr!("disabled-theme-reset-unchanged")));
    }

    /// A project theme cannot be the default of new graphs: the box is greyed.
    #[test]
    fn use_for_new_graphs_is_greyed_for_a_project_theme_and_not_for_a_user_one() {
        let mut harness = bench();
        let graph = harness.state().graph;
        harness.state_mut().state.dispatch(Action::SaveThemeAs {
            graph,
            name: "Shared".into(),
            target: ThemeTarget::Project,
        });
        harness.run();
        harness.run();
        assert!(harness
            .get_by_label(&tr!("theme-mgr-use-for-new"))
            .accesskit_node()
            .is_disabled());
        save_user(&mut harness, "Mine one");
        let label = tr!("theme-mgr-use-for-new");
        let checkbox = harness.get_by_label(&label);
        assert!(!checkbox.accesskit_node().is_disabled());
        checkbox.click();
        harness.run();
        assert!(harness.state().state.new_graph_theme().is_some());
    }

    /// Delete of a user theme asks first; the library has no Undo.
    #[test]
    fn deleting_a_user_theme_from_the_tab_asks_for_a_confirmation() {
        let mut harness = bench();
        let theme = save_user(&mut harness, "Mine one");
        harness.get_by_label(&tr!("theme-mgr-delete")).click();
        harness.run();
        harness.run();
        assert!(harness.state().state.user_themes().get(&theme).is_some());
        assert_eq!(harness.state().state.pending_theme_delete(), Some(&theme));
        harness.get_by_label(&tr!("theme-delete-body", name = "Mine one"));
        // « Delete » is also the label of the tab's own button: the footer
        // button of the confirmation is the wider one.
        let verb = tr!("theme-delete-verb");
        let footer = harness
            .get_all_by_label(&verb)
            .max_by(|a, b| a.rect().width().total_cmp(&b.rect().width()))
            .expect("the verb");
        footer.click();
        harness.run();
        harness.run();
        assert!(harness.state().state.user_themes().get(&theme).is_none());
    }

    /// Apply Theme… opens the window into the draft of Graph Properties.
    #[test]
    fn apply_theme_opens_the_window_into_the_draft() {
        let mut harness = bench();
        harness.get_by_label(&tr!("action-apply-theme")).click();
        harness.run();
        harness.run();
        assert!(harness
            .state()
            .state
            .apply_theme_window()
            .is_some_and(|window| window.into_draft()));
    }

    /// Escape on a window opened from the tab closes that window and not Graph
    /// Properties under it (the child is painted first, so it takes the key).
    #[test]
    fn escape_closes_only_the_window_opened_from_the_tab() {
        let mut harness = bench();
        harness.get_by_label(&tr!("action-apply-theme")).click();
        harness.run();
        harness.run();
        assert!(harness.state().state.apply_theme_window().is_some());
        harness.key_press(egui::Key::Escape);
        harness.run();
        harness.run();
        assert!(harness.state().state.apply_theme_window().is_none());
        assert!(
            harness.state().state.plot_settings().is_some(),
            "Graph Properties is still open"
        );

        let theme = save_user(&mut harness, "Mine one");
        harness.get_by_label(&tr!("theme-mgr-delete")).click();
        harness.run();
        harness.run();
        assert_eq!(harness.state().state.pending_theme_delete(), Some(&theme));
        harness.key_press(egui::Key::Escape);
        harness.run();
        harness.run();
        assert!(harness.state().state.pending_theme_delete().is_none());
        assert!(harness.state().state.user_themes().get(&theme).is_some());
        assert!(harness.state().state.plot_settings().is_some());
    }
}

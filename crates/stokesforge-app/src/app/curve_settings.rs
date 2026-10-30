//! Series properties page (#177-#179): the separate window, opened by
//! double-clicking a curve or its row in the Object Manager, which sets the
//! series of a layer (list on the left, settings on the right) and their
//! **groups**. A group is the set of series plotted together
//! ([`stokesforge_plot::PanelSpec::push_series_batch`]) or gathered by hand;
//! it sets their common style and spreads a color gradient over them.
//!
//! The window is that of [`super::plot_settings`] (same draft, same
//! validation, same history macro): [`PlotSettingsTab::Series`] no longer
//! appears in the graph's tab strip, it chooses this page. A series of a
//! group is edited according to a **scope** chosen in plain words
//! ([`EditScope`]): "This series only" (nothing touches the others, this is
//! the exception) or "Whole group" (line and symbol style copied into each
//! member, gradient applied; the settings specific to a series (name, type,
//! visibility, colors) are grayed out with a tooltip). The common style is
//! **written into each series**: no exception flag to keep track of, and
//! "Leave Group" gives a series back to itself.
//!
//! Color gradients (#178): built in, saved in the project or for the user
//! (same pattern as colors, `SavedColors::gradients`), or custom; they are
//! deleted from the same list.
//!
//! [`PlotSettingsTab::Series`]: stokesforge_app_state::state::PlotSettingsTab::Series

use eframe::egui;
use stokesforge_core::Tree;
use stokesforge_plot::{
    gradient_colors, PanelSpec, SavedColors, SeriesSlot, SeriesSpec, GRADIENT_PRESETS,
};

use crate::color_picker::{color_field, gradient_strip, gradient_swatch, ColorUsage, PickerEnv};
use crate::icons::ICON_SIZE;
use crate::theme;
use stokesforge_app_state::object_drag::{step_panel, DragItem, DropOver};
use stokesforge_app_state::object_manager::group_title;
use stokesforge_app_state::state::{series_list_label, Action, SaveTarget};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::form::{choice, form, ChoiceOption, Form};
use super::hover_help::help_text;
use super::list_row::{
    list_row_draggable, raise_to_list_row_height, restore_row_height, with_list_row_height,
};
use super::object_dnd::{self, Hover};
use super::plot_settings::series_editor;
use super::reorder_list::reorder_row_with;

/// Width of the series list, on the left.
const LIST_WIDTH: f32 = 190.0;
/// Height taken off the list for the Move Up / Move Down row.
const LIST_FOOTER: f32 = 34.0;
/// Starting stops of a custom color gradient: from blue to red.
const CUSTOM_STOPS: [&str; 2] = ["#0000ff", "#ff0000"];
/// Number of samples in the preview of a color gradient.
const PREVIEW_STEPS: usize = 24;
/// Number of samples in the swatch of a list entry.
const ENTRY_STEPS: usize = 12;

/// What the settings of a series that is in a group apply to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditScope {
    /// This series only: the other members do not move.
    Series,
    /// Whole group.
    Group,
}

/// The color gradient chosen in the list (window state, never written to the model).
#[derive(Debug, Clone, PartialEq, Eq)]
enum GradientChoice {
    /// A built-in color gradient: rank in `GRADIENT_PRESETS`.
    Preset(usize),
    /// A saved color gradient: where and under what name.
    Saved(SaveTarget, String),
    /// The stops entered in the window.
    Custom,
}

/// Series page of the layer `panel`: the list, then the group and the
/// settings of the series `selected`. Returns `true` if the draft must be
/// validated; see the head doc of `plot_settings_window`. Saves and deletions
/// of color gradients go out in `actions`.
pub(super) fn curve_settings_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    tree: &Tree,
    selected: &mut usize,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    if panel.series.is_empty() {
        empty_state(ui, tr!("plot-series-empty"));
        return false;
    }
    *selected = (*selected).min(panel.series.len() - 1);
    let mut commit = false;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(LIST_WIDTH);
            commit |= series_list(ui, panel, layer, tree, selected);
        });
        ui.separator();
        ui.vertical(|ui| {
            theme::page_scroll_area()
                .id_salt("curve_settings_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    commit |= series_page(ui, panel, layer, tree, selected, env, actions);
                });
        });
    });
    commit
}

/// Whole group: copies into the members of `group` what the series `index`
/// has just changed in its line and symbol style relative to `before` (only
/// what changed: the exceptions already set on the other fields are kept).
fn propagate_style(panel: &mut PanelSpec, group: u32, before: &SeriesSpec, index: usize) {
    let edited = panel.series[index].clone();
    for member in panel.group_members(group) {
        let other = &mut panel.series[member];
        if edited.line.style != before.line.style {
            other.line.style = edited.line.style;
        }
        if edited.line.width != before.line.width {
            other.line.width = edited.line.width;
        }
        if edited.symbol.shape != before.symbol.shape {
            other.symbol.shape = edited.symbol.shape;
        }
        if edited.symbol.size != before.symbol.size {
            other.symbol.size = edited.symbol.size;
        }
        // Bins and bar width (#359) only reach the members they concern.
        if edited.histogram_bins != before.histogram_bins
            && other.plot_type == stokesforge_plot::PlotType::Histogram
        {
            other.histogram_bins = edited.histogram_bins;
        }
        if edited.bar_width != before.bar_width
            && other.plot_type == stokesforge_plot::PlotType::Bar
        {
            other.bar_width = edited.bar_width;
        }
    }
}

/// The drag and drop of the list: the item grabbed (the draft changes right
/// away, without preview) and the first row hovered in the frame.
struct ListDnd {
    layer: usize,
    item: Option<DragItem>,
    hover: Hover,
}

/// A series row of the list; returns the vertical center of the row. Its
/// handle drags it: within its group, to another group, out of the group, or
/// among the single series.
fn series_row(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    index: usize,
    selected: &mut usize,
    dnd: &ListDnd,
    commit: &mut bool,
) -> f32 {
    let label = series_list_label(tree, &panel.series[index]);
    let row = ui
        .push_id(index, |ui| {
            with_list_row_height(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut clicked = false;
                    let outcome = reorder_row_with(
                        ui,
                        ui.id().with("row"),
                        matches!(
                            dnd.item,
                            Some(DragItem::Series { current, .. }) if current == index
                        ),
                        |ui| {
                            // "Show series": the series' box, at the head of the row.
                            let mut visible = panel.series[index].visible;
                            if ui
                                .add(egui::Checkbox::new(&mut visible, ""))
                                .on_hover_text(tr!("plot-series-show"))
                                .changed()
                            {
                                panel.series[index].visible = visible;
                                *commit = true;
                            }
                            // The whole row, outside the box, is clicked and dragged.
                            let response = list_row_draggable(ui, &label, *selected == index);
                            clicked = response.clicked();
                            Some(response)
                        },
                    );
                    (outcome, clicked)
                })
            })
        })
        .inner;
    let (outcome, clicked) = row.inner;
    if clicked {
        *selected = index;
    }
    if outcome.grabbed {
        *selected = index;
        object_dnd::begin(
            ui.ctx(),
            None,
            DragItem::Series {
                panel: dnd.layer,
                origin: index,
                current: index,
            },
        );
    }
    if let Some(rect) = outcome.hover {
        dnd.hover.offer(DropOver::Series {
            panel: dnd.layer,
            index,
            after: object_dnd::pointer_after(ui, rect),
        });
    }
    row.response.rect.center().y
}

/// The series list, arranged like the Object Manager (#197): a single series
/// is a row, a group is a collapsible node (collapsing hides no curve) whose
/// members are in a branch. Below, the row that changes the order. Returns
/// `true` if the order changed.
fn series_list(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    tree: &Tree,
    selected: &mut usize,
) -> bool {
    let mut commit = false;
    let dnd = ListDnd {
        layer,
        item: object_dnd::live(ui.ctx())
            .filter(|live| live.graph.is_none())
            .map(|live| live.item),
        hover: Hover::default(),
    };
    // RAW-SCROLL(#347): list whose height is computed with LIST_FOOTER
    egui::ScrollArea::vertical()
        .id_salt("curve_settings_list")
        .auto_shrink([false, false])
        .max_height((ui.available_height() - LIST_FOOTER).max(60.0))
        .show(ui, |ui| {
            crate::tree_lines::begin_tree(ui);
            // #169: a list, so list rows (`list_row`).
            for slot in panel.slots() {
                match slot {
                    SeriesSlot::Single(index) => {
                        series_row(ui, panel, tree, index, selected, &dnd, &mut commit);
                    }
                    SeriesSlot::Group { id, members } => {
                        let state = ui.make_persistent_id(("curve_list_group", layer, id));
                        let header =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                state,
                                true,
                            );
                        let previous_height = raise_to_list_row_height(ui);
                        header
                            .show_header(ui, |ui| {
                                // As in the Object Manager (#208): the header is
                                // highlighted when the group is what is being edited, and a
                                // click chooses the group (its first series, scope
                                // "Whole group").
                                let scope_id = egui::Id::new("curve_edit_scope");
                                let scope = ui
                                    .data(|d| d.get_temp(scope_id))
                                    .unwrap_or(EditScope::Group);
                                let on_group =
                                    scope == EditScope::Group && members.contains(selected);
                                let title = group_title(panel, id);
                                let mut clicked = false;
                                let outcome = reorder_row_with(
                                    ui,
                                    ui.id().with("row"),
                                    matches!(
                                        dnd.item,
                                        Some(DragItem::Group { group, .. }) if group == id
                                    ),
                                    |ui| {
                                        let response = list_row_draggable(ui, &title, on_group);
                                        clicked = response.clicked();
                                        Some(response)
                                    },
                                );
                                if clicked || outcome.grabbed {
                                    if let Some(first) = members.first() {
                                        *selected = *first;
                                    }
                                    ui.data_mut(|d| d.insert_temp(scope_id, EditScope::Group));
                                }
                                if outcome.grabbed {
                                    object_dnd::begin(
                                        ui.ctx(),
                                        None,
                                        DragItem::Group {
                                            panel: layer,
                                            group: id,
                                        },
                                    );
                                }
                                if let Some(rect) = outcome.hover {
                                    dnd.hover.offer(DropOver::Header {
                                        panel: layer,
                                        group: id,
                                        after: object_dnd::pointer_after(ui, rect),
                                    });
                                }
                            })
                            .body(|ui| {
                                crate::tree_lines::tree_children(ui, |ui, branch| {
                                    for &member in &members {
                                        let y = series_row(
                                            ui,
                                            panel,
                                            tree,
                                            member,
                                            selected,
                                            &dnd,
                                            &mut commit,
                                        );
                                        branch.mark(y);
                                    }
                                })
                            });
                        restore_row_height(ui, previous_height);
                    }
                }
            }
            // The exit of a group that ends the list, and the drop at the end of
            // the list: the zone exists only during a drag.
            if dnd.item.is_some() {
                let row = ui.horizontal(|ui| ui.weak(tr!("object-drag-end"))).response;
                if ui.rect_contains_pointer(row.rect) {
                    dnd.hover.offer(DropOver::End { panel: layer });
                }
            }
        });
    if let (Some(item), Some(over)) = (dnd.item, dnd.hover.take()) {
        if let Some((item, order)) = step_panel(panel, item, over) {
            object_dnd::set_item(ui.ctx(), item);
            // The edited series follows its place in the new order.
            if let Some(rank) = order.iter().position(|old| old == selected) {
                *selected = rank;
            }
            commit = true;
        }
    }
    ui.horizontal(|ui| {
        for (label, up) in [
            (tr!("action-move-series-up"), true),
            (tr!("action-move-series-down"), false),
        ] {
            let movable = panel.can_move_series(*selected, up);
            if ui
                .add_enabled(movable, egui::Button::new(label))
                .on_disabled_hover_text(if up {
                    tr!("disabled-move-first")
                } else {
                    tr!("disabled-move-last")
                })
                .clicked()
            {
                if let Some(index) = panel.move_series(*selected, up) {
                    *selected = index;
                    commit = true;
                }
            }
        }
    });
    commit
}

/// The right side: group, scope, color gradient, then the series settings.
fn series_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    tree: &Tree,
    index: &mut usize,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    let scope_id = egui::Id::new("curve_edit_scope");
    let mut scope = ui
        .data(|d| d.get_temp(scope_id))
        .unwrap_or(EditScope::Group);
    let mut commit = membership_editor(ui, panel, layer, index, &mut scope);
    ui.data_mut(|d| d.insert_temp(scope_id, scope));
    // Deleting a group can empty the layer: nothing to edit afterwards.
    if panel.series.is_empty() {
        return commit;
    }
    *index = (*index).min(panel.series.len() - 1);
    let group = panel.series[*index].group;
    let whole_group = group.is_some() && scope == EditScope::Group;
    // Outside a group there is nothing to spread: no color gradient (#213).
    if let Some(group) = group {
        commit |= gradient_editor(ui, panel, group, whole_group, env, actions);
    }

    let before = panel.series[*index].clone();
    // The width "Auto (...)" shows is the one the rendering would deduce.
    let auto_bar_width = {
        let resolved = stokesforge_core::resolve::resolve_series(tree, &panel.series[*index]);
        stokesforge_core::resolve::bar_width(resolved.x.as_ref().map(|x| x.as_slice()), None)
    };
    let error_columns = {
        let series = &panel.series[*index];
        stokesforge_app_state::analysis::error_candidates(
            tree,
            series.y,
            [series.x_error, series.y_error],
        )
    };
    commit |= series_editor(
        ui,
        &mut panel.series[*index],
        env,
        whole_group,
        auto_bar_width,
        panel.overlay.is_some(),
        &error_columns,
    );
    // The Y axis (#358) goes through the panel, which makes the right axis
    // visible; in "whole group" scope every member gets it.
    let axis = panel.series[*index].axis;
    if axis != before.axis {
        panel.set_series_axis(*index, axis);
        if let (true, Some(group)) = (whole_group, group) {
            for member in panel.group_members(group) {
                panel.set_series_axis(member, axis);
            }
        }
    }
    if let (true, Some(group)) = (whole_group, group) {
        propagate_style(panel, group, &before, *index);
    }
    commit
}

/// Membership of the series `index`: dropdown, "Leave Group", then, if it is
/// in a group, its gestures (name, order, ungroup, delete) and the scope
/// choice with its explanatory sentence. A group stays contiguous (#197):
/// changing membership moves the series, `index` follows it.
fn membership_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    index: &mut usize,
    scope: &mut EditScope,
) -> bool {
    let mut commit = false;
    let group = panel.series[*index].group;
    let mut chosen = group;
    let mut new_group = false;
    form(ui, "curve_membership", |f| {
        f.section(tr!("curve-group-section"));
        f.row(tr!("curve-group-field"), |ui| {
            let entry = |id: u32| {
                tr!(
                    "curve-group-entry",
                    group = group_title(panel, id),
                    count = panel.group_members(id).len()
                )
            };
            egui::ComboBox::from_id_salt("curve_group_choice")
                .selected_text(match group {
                    Some(id) => entry(id),
                    None => tr!("curve-group-none"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, tr!("curve-group-none"));
                    for id in panel.group_ids() {
                        ui.selectable_value(&mut chosen, Some(id), entry(id));
                    }
                    if ui.selectable_label(false, tr!("curve-group-new")).clicked() {
                        new_group = true;
                    }
                });
            if ui
                .add_enabled(group.is_some(), egui::Button::new(tr!("curve-group-leave")))
                .on_hover_text(tr!("curve-group-leave", "hint"))
                .on_disabled_hover_text(tr!("curve-group-leave", "disabled"))
                .clicked()
            {
                chosen = None;
            }
        });
        if new_group {
            chosen = Some(panel.next_group_id());
        }
        if chosen != group {
            if let Some(moved) = panel.set_series_group(*index, chosen) {
                *index = moved;
                commit = true;
            }
        }
        if let Some(group) = panel.series[*index].group {
            commit |= group_editor(f, panel, layer, group, index);
            let count = panel.group_members(group).len();
            f.row(tr!("curve-scope-field"), |ui| {
                // The two options are one under the other: the "?" sits
                // at the level of the last one, "Whole group".
                ui.horizontal(|ui| {
                    let options = ui.vertical(|ui| {
                        choice(
                            ui,
                            "curve_scope",
                            scope,
                            &[
                                ChoiceOption::new(EditScope::Series, tr!("curve-scope-series")),
                                ChoiceOption::new(
                                    EditScope::Group,
                                    tr!("curve-scope-group", count = count),
                                ),
                            ],
                        );
                    });
                    // What applies depending on the scope: under the "?" (#213),
                    // no longer in a grayed note.
                    let text = match scope {
                        EditScope::Group => tr!("curve-scope-group-note"),
                        EditScope::Series => tr!("curve-scope-series-note"),
                    };
                    let rect = options.response.rect;
                    let at = egui::pos2(
                        ui.cursor().min.x,
                        rect.bottom() - ICON_SIZE.max(ui.spacing().interact_size.y),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            at,
                            egui::vec2(
                                ICON_SIZE * 2.0,
                                ICON_SIZE.max(ui.spacing().interact_size.y),
                            ),
                        )),
                        |ui| {
                            ui.centered_and_justified(|ui| {
                                help_text(ui, text);
                            });
                        },
                    );
                });
            });
        }
        commit
    })
}

/// The gestures of the group `group` (#197): its name, its rank in the
/// drawing order, its dissolution (the series stay) and its deletion with its
/// series. `index` is the edited series: it follows its content when the
/// group changes rank, and falls back on the group's place when it is
/// deleted. Returns `true` if the group changed.
fn group_editor(
    f: &mut Form<'_>,
    panel: &mut PanelSpec,
    layer: usize,
    group: u32,
    index: &mut usize,
) -> bool {
    let mut commit = false;
    // The text being typed lives only as long as the focus: the rest of the
    // time, the field rereads the model (name changed elsewhere, other group).
    let name_id = egui::Id::new(("curve_group_name", layer, group));
    let mut name: String = f
        .ui()
        .data(|d| d.get_temp(name_id))
        .unwrap_or_else(|| panel.group_label(group).unwrap_or_default().to_string());
    f.row(tr!("curve-group-name-field"), |ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text(tr!("curve-group-name", number = group))
                .desired_width(theme::FieldWidth::Medium.points(ui)),
        );
        help_text(ui, tr!("curve-group-name-field", "hint"));
        if field.lost_focus() && panel.rename_group(group, &name) {
            commit = true;
        }
        if field.has_focus() {
            ui.data_mut(|d| d.insert_temp(name_id, name.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(name_id));
        }
    });
    f.field(|ui| {
        for up in [true, false] {
            let (label, hint) = if up {
                (
                    tr!("curve-group-move-up"),
                    tr!("curve-group-move-up", "hint"),
                )
            } else {
                (
                    tr!("curve-group-move-down"),
                    tr!("curve-group-move-down", "hint"),
                )
            };
            if ui
                .add_enabled(panel.can_move_group(group, up), egui::Button::new(label))
                .on_hover_text(hint)
                .on_disabled_hover_text(if up {
                    tr!("disabled-move-first")
                } else {
                    tr!("disabled-move-last")
                })
                .clicked()
            {
                let place = panel
                    .group_members(group)
                    .iter()
                    .position(|member| member == index);
                if panel.move_group(group, up) {
                    if let Some(member) =
                        place.and_then(|p| panel.group_members(group).get(p).copied())
                    {
                        *index = member;
                    }
                    commit = true;
                }
            }
        }
    });
    f.field(|ui| {
        if ui
            .button(tr!("curve-group-ungroup"))
            .on_hover_text(tr!("curve-group-ungroup", "hint"))
            .clicked()
            && panel.dissolve_group(group)
        {
            commit = true;
        }
        if ui
            .button(tr!("curve-group-delete"))
            .on_hover_text(tr!("curve-group-delete", "hint"))
            .clicked()
        {
            let first = panel.group_members(group).first().copied().unwrap_or(0);
            if panel.remove_group(group) {
                *index = first.min(panel.series.len().saturating_sub(1));
                commit = true;
            }
        }
    });
    commit
}

/// The name of a built-in color gradient.
fn gradient_label(id: &str) -> String {
    match id {
        "blue-red" => tr!("curve-gradient-blue-red"),
        "black-red" => tr!("curve-gradient-black-red"),
        "black-blue" => tr!("curve-gradient-black-blue"),
        "blue-yellow" => tr!("curve-gradient-blue-yellow"),
        "green-red" => tr!("curve-gradient-green-red"),
        "blue-white-red" => tr!("curve-gradient-blue-white-red"),
        "viridis" => tr!("curve-gradient-viridis"),
        "plasma" => tr!("curve-gradient-plasma"),
        _ => id.to_string(),
    }
}

/// The saved colors of the target `target`.
fn saved_of(env: &PickerEnv, target: SaveTarget) -> &SavedColors {
    match target {
        SaveTarget::Project => &env.project,
        SaveTarget::User => &env.user,
    }
}

/// The stops of the chosen color gradient; `custom` for a custom one (or a
/// saved one that has disappeared).
fn choice_stops(choice: &GradientChoice, env: &PickerEnv, custom: &[String]) -> Vec<String> {
    match choice {
        GradientChoice::Preset(index) => GRADIENT_PRESETS.get(*index).map_or_else(
            || custom.to_vec(),
            |preset| preset.stops.iter().map(|s| (*s).to_string()).collect(),
        ),
        GradientChoice::Saved(target, name) => saved_of(env, *target)
            .gradients
            .iter()
            .find(|gradient| &gradient.name == name)
            .map_or_else(|| custom.to_vec(), |gradient| gradient.stops.clone()),
        GradientChoice::Custom => custom.to_vec(),
    }
}

/// The choice that matches the `stops` retained by the group (#213): the one
/// the window remembers if it gives these stops, otherwise a built-in or
/// saved color gradient that carries them, otherwise the custom one.
fn identify_choice(
    stops: &[String],
    remembered: Option<&GradientChoice>,
    env: &PickerEnv,
    custom: &[String],
) -> GradientChoice {
    let same = |other: &[String]| {
        other.len() == stops.len()
            && other
                .iter()
                .zip(stops)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
    };
    if let Some(choice) = remembered {
        if same(&choice_stops(choice, env, custom)) {
            return choice.clone();
        }
    }
    if let Some(index) = GRADIENT_PRESETS.iter().position(|preset| {
        let preset: Vec<String> = preset.stops.iter().map(|s| (*s).to_string()).collect();
        same(&preset)
    }) {
        return GradientChoice::Preset(index);
    }
    for target in [SaveTarget::Project, SaveTarget::User] {
        if let Some(gradient) = saved_of(env, target)
            .gradients
            .iter()
            .find(|gradient| same(&gradient.stops))
        {
            return GradientChoice::Saved(target, gradient.name.clone());
        }
    }
    GradientChoice::Custom
}

/// An entry of the color gradient list: its colors, then its name (#213).
fn gradient_popup_entry(
    ui: &mut egui::Ui,
    choice: &mut Option<GradientChoice>,
    value: GradientChoice,
    label: String,
    stops: &[String],
) {
    ui.horizontal(|ui| {
        // The whole row is clicked (#213): the swatch as well as the name.
        let swatch = gradient_swatch(ui, &gradient_colors(stops, ENTRY_STEPS));
        if swatch.clicked() {
            *choice = Some(value.clone());
        }
        ui.selectable_value(choice, Some(value), label);
    });
}

/// Color gradients (#178, #213): choose (built-in, saved, custom), see their
/// colors in the list and as a strip, save or delete a saved gradient. It
/// spreads the colors evenly over the series of the group in the order of the
/// list, and the group **retains** it: the list shows the group's one, empty
/// until one has been chosen (the strip is then grayed out and nothing is
/// applied). A series that joins the group receives its color. Grayed out
/// unless the scope is "Whole group" (`enabled`), with the tooltip that says
/// so. The gradient applies as soon as it is chosen or a stop changes (draft,
/// validated by OK). Outside a group, the page does not call it: there is
/// nothing to spread.
fn gradient_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    group: u32,
    enabled: bool,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    let choice_id = egui::Id::new("curve_gradient_choice");
    let stops_id = egui::Id::new("curve_gradient_stops");
    let name_id = egui::Id::new("curve_gradient_name");
    let target_id = egui::Id::new("curve_gradient_target");
    let remembered: Option<GradientChoice> = ui.data(|d| d.get_temp(choice_id));
    let mut custom: Vec<String> = ui
        .data(|d| d.get_temp(stops_id))
        .unwrap_or_else(|| CUSTOM_STOPS.iter().map(|s| (*s).to_string()).collect());
    let mut name: String = ui.data(|d| d.get_temp(name_id)).unwrap_or_default();
    let mut target: SaveTarget = ui.data(|d| d.get_temp(target_id)).unwrap_or_default();

    // What the list shows comes from the group, not from the window: empty if
    // it has received no color gradient.
    let retained: Option<Vec<String>> = panel.group_gradient(group).map(<[String]>::to_vec);
    let mut choice: Option<GradientChoice> = retained
        .as_deref()
        .map(|stops| identify_choice(stops, remembered.as_ref(), env, &custom));
    if let (Some(GradientChoice::Custom), Some(stops)) = (&choice, &retained) {
        custom.clone_from(stops);
    }

    let choice_before = choice.clone();
    let custom_before = custom.clone();
    let mut commit = false;
    let out = ui.add_enabled_ui(enabled, |ui| {
        form(ui, "curve_gradient_form", |f| {
            f.section(tr!("curve-gradient-section"));
            f.row(tr!("curve-gradient-field"), |ui| {
                let selected_text = match &choice {
                    None => String::new(),
                    Some(GradientChoice::Preset(index)) => GRADIENT_PRESETS
                        .get(*index)
                        .map_or_else(String::new, |preset| gradient_label(preset.id)),
                    Some(GradientChoice::Saved(target, name)) => tr!(
                        "curve-gradient-saved",
                        name = name.as_str(),
                        target = target.label()
                    ),
                    Some(GradientChoice::Custom) => tr!("curve-gradient-custom"),
                };
                egui::ComboBox::from_id_salt("curve_gradient_combo")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for (index, preset) in GRADIENT_PRESETS.iter().enumerate() {
                            let stops: Vec<String> =
                                preset.stops.iter().map(|s| (*s).to_string()).collect();
                            gradient_popup_entry(
                                ui,
                                &mut choice,
                                GradientChoice::Preset(index),
                                gradient_label(preset.id),
                                &stops,
                            );
                        }
                        for saved_target in [SaveTarget::Project, SaveTarget::User] {
                            for gradient in &saved_of(env, saved_target).gradients {
                                gradient_popup_entry(
                                    ui,
                                    &mut choice,
                                    GradientChoice::Saved(saved_target, gradient.name.clone()),
                                    tr!(
                                        "curve-gradient-saved",
                                        name = gradient.name.as_str(),
                                        target = saved_target.label()
                                    ),
                                    &gradient.stops,
                                );
                            }
                        }
                        gradient_popup_entry(
                            ui,
                            &mut choice,
                            GradientChoice::Custom,
                            tr!("curve-gradient-custom"),
                            &custom,
                        );
                    });
                let saved = match &choice {
                    Some(GradientChoice::Saved(target, name)) => Some((*target, name.clone())),
                    _ => None,
                };
                if ui
                    .add_enabled(
                        saved.is_some(),
                        egui::Button::new(tr!("curve-gradient-delete")),
                    )
                    .on_hover_text(tr!("curve-gradient-delete", "hint"))
                    .on_disabled_hover_text(tr!("curve-gradient-delete", "disabled"))
                    .clicked()
                {
                    if let Some((target, name)) = saved {
                        actions.push(Action::RemoveSavedGradient { name, target });
                        // The colors already set stay: the list rereads them
                        // as a custom color gradient.
                        choice = Some(GradientChoice::Custom);
                    }
                }
            });
            if choice == Some(GradientChoice::Custom) {
                let mut remove = None;
                for (index, stop) in custom.iter_mut().enumerate() {
                    f.ui().push_id(index, |ui| {
                        color_field(
                            ui,
                            &tr!("curve-gradient-stop", number = (index + 1)),
                            stop,
                            ColorUsage::Curves,
                            None,
                            env,
                        );
                    });
                }
                f.field(|ui| {
                    if ui.button(tr!("curve-gradient-add-stop")).clicked() {
                        let last = custom.last().cloned().unwrap_or_default();
                        custom.push(last);
                    }
                    if ui
                        .add_enabled(
                            custom.len() > 2,
                            egui::Button::new(tr!("curve-gradient-remove-stop")),
                        )
                        .on_disabled_hover_text(tr!("disabled-gradient-min-colors"))
                        .clicked()
                    {
                        remove = Some(custom.len() - 1);
                    }
                });
                if let Some(index) = remove {
                    custom.remove(index);
                }
                f.row(tr!("curve-gradient-name-field"), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text(tr!("curve-gradient-name-field", "hint"))
                            .desired_width(theme::FieldWidth::Short.points(ui)),
                    );
                    super::form::choice(
                        ui,
                        "curve_gradient_target",
                        &mut target,
                        &[
                            ChoiceOption::new(SaveTarget::Project, SaveTarget::Project.label()),
                            ChoiceOption::new(SaveTarget::User, SaveTarget::User.label()),
                        ],
                    );
                    if ui
                        .add_enabled(
                            !name.trim().is_empty(),
                            egui::Button::new(tr!("curve-gradient-save")),
                        )
                        .on_hover_text(tr!("curve-gradient-save", "hint"))
                        .on_disabled_hover_text(tr!("curve-gradient-save", "disabled"))
                        .clicked()
                    {
                        actions.push(Action::SaveGradient {
                            name: name.trim().to_string(),
                            stops: custom.clone(),
                            target,
                        });
                        choice = Some(GradientChoice::Saved(target, name.trim().to_string()));
                    }
                });
            }
            // No "Apply" button: like all the other settings, the color
            // gradient is written into the draft as soon as it changes
            // (choice, color stop); Cancel discards it, OK validates it (#213).
            let active = choice
                .as_ref()
                .map(|choice| choice_stops(choice, env, &custom));
            if enabled && (choice != choice_before || custom != custom_before) {
                if let Some(active) = &active {
                    commit |= panel.set_group_gradient(group, active);
                }
            }
            // The strip shows the group's color gradient; grayed out, empty, while it
            // has received none.
            let strip = active
                .as_deref()
                .map(|stops| gradient_colors(stops, PREVIEW_STEPS))
                .unwrap_or_default();
            gradient_strip(f.ui(), &strip);
        })
    });
    if !enabled {
        out.response.on_hover_text(tr!("curve-gradient-locked"));
    }
    ui.data_mut(|d| {
        if let Some(choice) = choice {
            d.insert_temp(choice_id, choice);
        } else {
            d.remove::<GradientChoice>(choice_id);
        }
        d.insert_temp(stops_id, custom);
        d.insert_temp(name_id, name);
        d.insert_temp(target_id, target);
    });
    commit
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::NodeId;
    use stokesforge_plot::LineStyle;

    fn sample_panel() -> PanelSpec {
        let mut panel = PanelSpec::default();
        panel.push_series_batch(vec![SeriesSpec::new(NodeId::from_raw(1))]);
        panel.push_series_batch(
            (2..=4)
                .map(|n| SeriesSpec::new(NodeId::from_raw(n)))
                .collect(),
        );
        panel
    }

    #[test]
    fn the_changed_style_is_copied_to_the_group_without_overwriting_the_rest() {
        let mut panel = sample_panel();
        panel.series[2].line.width = 4.0; // exception set by hand
        let before = panel.series[1].clone();
        panel.series[1].line.style = LineStyle::Dash;
        panel.series[1].symbol.size = 11.0;
        propagate_style(&mut panel, 1, &before, 1);
        for member in [1, 2, 3] {
            assert_eq!(panel.series[member].line.style, LineStyle::Dash);
            assert_eq!(panel.series[member].symbol.size, 11.0);
        }
        assert_eq!(panel.series[2].line.width, 4.0, "untouched field: kept");
        assert_eq!(
            panel.series[0].line.style,
            LineStyle::Solid,
            "outside the group"
        );
        assert_eq!(
            panel.series[0].symbol.size,
            SeriesSpec::new(NodeId::from_raw(1)).symbol.size
        );
    }

    /// A layer "A, [B C D]" whose group is called "Batch"; the series
    /// carry the name of their letter.
    fn layer_with_group() -> PanelSpec {
        let mut panel = PanelSpec::default();
        for (n, name) in [(1, "A"), (2, "B"), (3, "C"), (4, "D")] {
            let mut series = SeriesSpec::new(NodeId::from_raw(n));
            series.name = name.into();
            panel.series.push(series);
        }
        for member in 1..=3 {
            panel.series[member].group = Some(1);
        }
        assert!(panel.rename_group(1, "Batch"));
        panel
    }

    /// A frame of the series page: returns what was painted and whether it
    /// asks for validation.
    fn page(
        ctx: &egui::Context,
        panel: &mut PanelSpec,
        selected: &mut usize,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, bool) {
        let project = stokesforge_core::Project::new("Series").unwrap();
        let mut env = PickerEnv::default();
        let mut actions = Vec::new();
        let mut commit = false;
        let output =
            super::super::test_support::frame_of(ctx, egui::vec2(900.0, 700.0), events, |ui| {
                commit |= curve_settings_page(
                    ui,
                    panel,
                    0,
                    project.tree(),
                    selected,
                    &mut env,
                    &mut actions,
                );
            });
        (output, commit)
    }

    fn center_of(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
        super::super::test_support::texts(&output.shapes)
            .into_iter()
            .find(|t| t.text == text)
            .unwrap_or_else(|| panic!("text absent: {text}"))
            .center
    }

    /// Left edge of the painted text: the alignment of a row, whatever the
    /// length of its label.
    fn left_of(output: &egui::FullOutput, text: &str) -> f32 {
        output
            .shapes
            .iter()
            .find_map(|painted_shape| match &painted_shape.shape {
                egui::Shape::Text(shape) if shape.galley.text() == text => {
                    Some(shape.visual_bounding_rect().left())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("text absent: {text}"))
    }

    fn click_at(
        ctx: &egui::Context,
        panel: &mut PanelSpec,
        selected: &mut usize,
        pos: egui::Pos2,
    ) -> bool {
        let mut commit = page(ctx, panel, selected, vec![egui::Event::PointerMoved(pos)]).1;
        for pressed in [true, false] {
            commit |= page(
                ctx,
                panel,
                selected,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
            .1;
        }
        commit
    }

    /// #197: the series list arranges a group like the Object Manager, with a
    /// header that carries its name and its members indented below, and no
    /// longer shows it as a suffix on each row.
    #[test]
    fn the_list_arranges_a_group_under_its_named_header() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut selected = 0;
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let (single, header, member) = (
            center_of(&output, "A"),
            center_of(&output, "Batch"),
            center_of(&output, "B"),
        );
        assert!(single.y < header.y && header.y < member.y);
        assert!(
            left_of(&output, "B") > left_of(&output, "A") + 10.0,
            "members are indented relative to a single series"
        );
        assert!(
            super::super::test_support::texts(&output.shapes)
                .iter()
                .all(|t| !t.text.contains('·')),
            "no group as a suffix any more"
        );
    }

    /// #170, point 5: a series row with its checkbox has the single height
    /// of list rows (the box is a square of `LIST_ROW_HEIGHT`, centered on the
    /// label) and the rows follow one another with a constant step, whether
    /// the series is single, in a group or the group header. Witness: without
    /// the imposed height, the box falls back to 18 px.
    #[test]
    fn the_series_row_has_the_single_height_with_its_checkbox() {
        use super::super::list_row::LIST_ROW_HEIGHT;
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut selected = 0;
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        for name in ["A", "B"] {
            let label = center_of(&output, name);
            let boxes: Vec<egui::Rect> = output
                .shapes
                .iter()
                .filter_map(|painted_shape| match &painted_shape.shape {
                    egui::Shape::Rect(rect)
                        if (rect.rect.width() - rect.rect.height()).abs() < 1.0
                            && (rect.rect.width() - LIST_ROW_HEIGHT).abs() < 1.0
                            && (rect.rect.center().y - label.y).abs() < 1.0
                            && rect.rect.center().x < label.x =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert!(!boxes.is_empty(), "{name}: box of LIST_ROW_HEIGHT missing");
        }
        let (a, batch, b) = (
            center_of(&output, "A"),
            center_of(&output, "Batch"),
            center_of(&output, "B"),
        );
        let step = batch.y - a.y;
        assert!(step >= LIST_ROW_HEIGHT, "rows too tight: {step}");
        assert!(
            ((b.y - batch.y) - step).abs() < 1.0,
            "constant step between single series, header and member: {step} / {}",
            b.y - batch.y
        );
    }

    /// #197: Ungroup makes its series single (style kept), Delete Group
    /// removes them; both ask for validation of the draft.
    #[test]
    fn ungroup_and_delete_a_group_from_the_window() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        panel.series[1].line.width = 4.5;
        let mut selected = 1;
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = center_of(&output, &tr!("curve-group-ungroup"));
        assert!(click_at(&ctx, &mut panel, &mut selected, pos));
        assert!(panel.series.iter().all(|s| s.group.is_none()));
        assert_eq!(panel.series[1].line.width, 4.5, "the style stays");
        assert_eq!(panel.series.len(), 4);

        let mut panel = layer_with_group();
        let mut selected = 3;
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = center_of(&output, &tr!("curve-group-delete"));
        assert!(click_at(&ctx, &mut panel, &mut selected, pos));
        assert_eq!(panel.series.len(), 1, "A stays single");
        assert!(panel.group_names.is_empty());
        let (_, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        assert_eq!(
            selected, 0,
            "the selection falls back on an existing series"
        );
    }

    /// #197: joining a group places the series after its members, and
    /// the selection follows the series.
    #[test]
    fn joining_a_group_moves_the_series_and_the_selection_follows() {
        let mut panel = layer_with_group();
        let mut ui_selected = 0;
        let ctx = super::super::test_support::context();
        // "A" (rank 0) joins group 1 by the same path as the dropdown.
        let moved = panel.set_series_group(ui_selected, Some(1)).unwrap();
        ui_selected = moved;
        assert_eq!(ui_selected, 3);
        assert_eq!(
            panel
                .series
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["B", "C", "D", "A"]
        );
        let (output, _) = page(&ctx, &mut panel, &mut ui_selected, Vec::new());
        assert!(center_of(&output, "A").y > center_of(&output, "D").y);
    }

    /// #209: joining a group **through the window's dropdown** (and not by
    /// calling `set_series_group` directly) places the series after the
    /// members, and the editor keeps the same series (`*index = moved`
    /// in `membership_editor`). Witness: before the choice, the edited series is
    /// at rank 0, which it leaves.
    #[test]
    fn the_dropdown_joins_a_group_and_the_editor_follows_the_series() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut selected = 0; // "A", outside a group
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let list = center_of(&output, &tr!("curve-group-none"));
        assert!(
            !click_at(&ctx, &mut panel, &mut selected, list),
            "opening the list changes nothing"
        );
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = center_of(
            &output,
            &tr!("curve-group-entry", group = "Batch", count = 3),
        );
        assert!(
            click_at(&ctx, &mut panel, &mut selected, pos),
            "the choice asks for validation"
        );
        let names: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["B", "C", "D", "A"], "A joins the end of the group");
        assert_eq!(panel.series[3].group, Some(1));
        assert_eq!(selected, 3);
        assert_eq!(
            panel.series[selected].name, "A",
            "the editor follows the series"
        );
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        assert!(
            super::super::test_support::texts(&output.shapes)
                .iter()
                .any(|t| t.text == tr!("curve-group-entry", group = "Batch", count = 4)),
            "the list now shows the series' group"
        );
    }

    /// #197: Move Group Down keeps the edited series (the same one, not the same
    /// rank) in the editor.
    #[test]
    fn moving_the_group_down_keeps_the_edited_series() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut extra = SeriesSpec::new(NodeId::from_raw(5));
        extra.name = "E".into();
        panel.series.push(extra);
        let mut selected = 1; // "B", first member of the group
        page(&ctx, &mut panel, &mut selected, Vec::new());
        let (output, _) = page(&ctx, &mut panel, &mut selected, Vec::new());
        let pos = center_of(&output, &tr!("curve-group-move-down"));
        assert!(click_at(&ctx, &mut panel, &mut selected, pos));
        let names: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["A", "E", "B", "C", "D"], "witness: the group moved");
        assert_eq!(panel.series[selected].name, "B");
    }

    /// The live series page, to drive a drag and drop frame by
    /// frame: the draft changes right away, without preview.
    struct Bench {
        project: stokesforge_core::Project,
        panel: PanelSpec,
        selected: usize,
        committed: bool,
    }

    fn bench(panel: PanelSpec, selected: usize) -> egui_kittest::Harness<'static, Bench> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    let mut env = PickerEnv::default();
                    let mut actions = Vec::new();
                    bench.committed |= curve_settings_page(
                        ui,
                        &mut bench.panel,
                        0,
                        bench.project.tree(),
                        &mut bench.selected,
                        &mut env,
                        &mut actions,
                    );
                },
                Bench {
                    project: stokesforge_core::Project::new("Series").unwrap(),
                    panel,
                    selected,
                    committed: false,
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    fn handle(harness: &egui_kittest::Harness<'static, Bench>, name: &str) -> egui::Rect {
        use egui_kittest::kittest::Queryable;
        harness.get_by_label(name).rect()
    }

    /// Grabs the handle of `name` and leads it onto the top of the row of `onto`.
    fn drag(harness: &mut egui_kittest::Harness<'static, Bench>, name: &str, onto: &str) {
        let from = handle(harness, name).center();
        let target = handle(harness, onto);
        let to = egui::pos2(target.center().x, target.top() + 1.0);
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from + egui::vec2(14.0, 0.0));
        harness.step();
        harness.hover_at(from + egui::vec2(20.0, 0.0));
        harness.step();
        for _ in 0..3 {
            harness.hover_at(to);
            harness.step();
        }
        harness.drop_at(to);
        harness.run();
    }

    fn names(panel: &PanelSpec) -> Vec<&str> {
        panel.series.iter().map(|s| s.name.as_str()).collect()
    }

    /// A single series dragged onto a member joins the group; the draft
    /// is changed right away and the edited series follows it.
    #[test]
    fn dragging_a_single_series_onto_a_member_makes_it_join_the_group() {
        let mut harness = bench(layer_with_group(), 0);
        drag(&mut harness, "A", "B");
        let bench = harness.state();
        assert!(bench.committed, "the draft is to be validated");
        let a = bench
            .panel
            .series
            .iter()
            .position(|s| s.name == "A")
            .unwrap();
        assert_eq!(bench.panel.series[a].group, Some(1));
        assert_eq!(bench.panel.group_members(1).len(), 4);
        assert_eq!(bench.selected, a, "the edited series follows its place");
    }

    /// A member dragged onto a single series leaves its group.
    #[test]
    fn dragging_a_member_onto_a_single_series_makes_it_leave_the_group() {
        let mut harness = bench(layer_with_group(), 1);
        drag(&mut harness, "B", "A");
        let bench = harness.state();
        assert_eq!(names(&bench.panel), ["B", "A", "C", "D"]);
        assert_eq!(bench.panel.series[0].group, None);
        assert_eq!(bench.panel.group_members(1), [2, 3]);
        assert_eq!(bench.selected, 0);
    }

    /// A single series and a group swap; the group is dragged as a block.
    #[test]
    fn dragging_a_group_moves_it_as_a_block_and_the_selection_follows() {
        let mut harness = bench(layer_with_group(), 2);
        drag(&mut harness, "Batch", "A");
        let bench = harness.state();
        assert_eq!(names(&bench.panel), ["B", "C", "D", "A"]);
        assert_eq!(bench.panel.group_members(1), [0, 1, 2]);
        // Grabbing the header chooses the group (its first series, "B"), which follows.
        assert_eq!(bench.panel.series[bench.selected].name, "B");
    }

    /// Between two independent series, two series swap.
    #[test]
    fn dragging_an_independent_series_onto_another_swaps_them() {
        let mut panel = PanelSpec::default();
        for (n, name) in [(1, "A"), (2, "B"), (3, "C")] {
            let mut series = SeriesSpec::new(NodeId::from_raw(n));
            series.name = name.into();
            panel.series.push(series);
        }
        let mut harness = bench(panel, 0);
        drag(&mut harness, "A", "C");
        assert_eq!(names(&harness.state().panel), ["B", "C", "A"]);
        assert_eq!(harness.state().selected, 2);
    }
}

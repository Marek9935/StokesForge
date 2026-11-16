//! Series properties page (#177-#179): the separate window, opened by
//! double-clicking a curve or its row in the Object Manager, which sets the
//! series of a layer (list on the left, settings on the right) and their
//! **groups**. A group is the set of series plotted together
//! ([`stokesforge_plot::PanelSpec::push_series_batch`]) or gathered by hand;
//! it sets their common style and spreads a color gradient over them.
//!
//! The window is that of [`super::plot_settings`] (same draft, same
//! validation, same history macro): [`PlotSettingsTab::Series`] no longer
//! appears in the graph's tab strip, it chooses this page. **The selection in
//! the list chooses the page** (#510): a group header opens the **group page**
//! (its name, its gradient, and the line and symbol
//! of all its curves), a curve opens the **curve page** (its own fields, its
//! group membership, and its own line and symbol). The common style is
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
use crate::theme;
use stokesforge_app_state::object_drag::{step_panel, DragItem, DropOver};
use stokesforge_app_state::object_manager::group_title;
use stokesforge_app_state::state::{series_list_entry, Action, SaveTarget};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::form::{form, ChoiceOption, Form};
use super::hover_help::help_text;
use super::list_row::{
    list_row_draggable, list_row_draggable_dimmed, raise_to_list_row_height, restore_row_height,
    with_list_row_height,
};
use super::object_dnd::{self, Hover};
use super::plot_settings::{series_curve_editor, series_group_editor, series_style_editor};
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

/// Series page of the layer `panel`: the list, then the page of what it
/// selects: the group `selected_group` if any, else the series `selected`.
/// Returns `true` if the draft must be validated; see the head doc of
/// `plot_settings_window`. Saves and deletions of color gradients go out in
/// `actions`.
#[allow(clippy::too_many_arguments)]
pub(super) fn curve_settings_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    tree: &Tree,
    selected: &mut usize,
    selected_group: &mut Option<u32>,
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    if panel.series.is_empty() {
        empty_state(ui, tr!("plot-series-empty"));
        return false;
    }
    *selected = (*selected).min(panel.series.len() - 1);
    // A group that is gone (deleted, or taken away by an Undo under the draft)
    // leaves the page to a curve that exists.
    if selected_group.is_some_and(|group| panel.group_members(group).is_empty()) {
        *selected_group = None;
    }
    let mut commit = false;
    // A click in the list is applied after the page is drawn (#498): a
    // draft being typed is validated on the series it was typed for.
    let mut pick: ListPick = None;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(LIST_WIDTH);
            commit |= series_list(
                ui,
                panel,
                layer,
                tree,
                (selected, selected_group),
                &mut pick,
            );
        });
        ui.separator();
        ui.vertical(|ui| {
            theme::page_scroll_area()
                .id_salt("curve_settings_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    commit |= match *selected_group {
                        Some(group) => group_page(
                            ui,
                            panel,
                            layer,
                            tree,
                            group,
                            (selected, selected_group),
                            env,
                            actions,
                        ),
                        None => curve_page(ui, panel, layer, tree, selected, env),
                    };
                });
        });
    });
    if let Some(pick) = pick {
        if crate::field_state::pick_allowed(ui.ctx()) {
            match pick {
                PagePick::Curve(index) => {
                    *selected = index;
                    *selected_group = None;
                }
                PagePick::Group(group) => {
                    *selected_group = Some(group);
                    if let Some(first) = panel.group_members(group).first() {
                        *selected = *first;
                    }
                }
            }
            ui.ctx().request_repaint();
        }
    }
    commit
}

/// The page a click in the list chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PagePick {
    /// The page of the series of this rank.
    Curve(usize),
    /// The page of this group.
    Group(u32),
}

/// A row clicked in the list, applied once the page is drawn (#498).
type ListPick = Option<PagePick>;

/// Group page: copies into the members of `group` what the series `index`
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
        // The symbol only reaches the members that draw one (#547).
        if other.plot_type.draws_symbols() {
            if edited.symbol.shape != before.symbol.shape {
                other.symbol.shape = edited.symbol.shape;
            }
            if edited.symbol.size != before.symbol.size {
                other.symbol.size = edited.symbol.size;
            }
            if edited.symbol.every != before.symbol.every {
                other.symbol.every = edited.symbol.every;
            }
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
#[allow(clippy::too_many_arguments)]
fn series_row(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    tree: &Tree,
    index: usize,
    (selected, selected_group): (&mut usize, &mut Option<u32>),
    dnd: &ListDnd,
    commit: &mut bool,
    pick: &mut ListPick,
) -> f32 {
    // One highlight in the list: a row only on a curve page.
    let highlighted = selected_group.is_none() && *selected == index;
    let (label, fallback) = series_list_entry(tree, &panel.series[index], &panel.legend.text);
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
                            let response =
                                list_row_draggable_dimmed(ui, &label, highlighted, fallback);
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
        if !highlighted {
            crate::field_state::pick_pending(ui.ctx());
        }
        *pick = Some(PagePick::Curve(index));
    }
    if outcome.grabbed {
        *selected = index;
        *selected_group = None;
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
    (selected, selected_group): (&mut usize, &mut Option<u32>),
    pick: &mut ListPick,
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
                        series_row(
                            ui,
                            panel,
                            tree,
                            index,
                            (&mut *selected, &mut *selected_group),
                            &dnd,
                            &mut commit,
                            pick,
                        );
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
                                // highlighted when the group page is shown, and a
                                // click chooses that page.
                                let on_group = *selected_group == Some(id);
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
                                // A click waits for the page (#498); a grab starts
                                // a drag, the draft is let go.
                                if outcome.grabbed {
                                    if let Some(first) = members.first() {
                                        *selected = *first;
                                    }
                                    *selected_group = Some(id);
                                } else if clicked {
                                    if !on_group {
                                        crate::field_state::pick_pending(ui.ctx());
                                    }
                                    *pick = Some(PagePick::Group(id));
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
                                            (&mut *selected, &mut *selected_group),
                                            &dnd,
                                            &mut commit,
                                            pick,
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
            if let Some(first) =
                selected_group.and_then(|group| panel.group_members(group).first().copied())
            {
                *selected = first;
            }
            commit = true;
        }
    }
    ui.horizontal(|ui| {
        for (label, up) in [
            (tr!("action-move-series-up"), true),
            (tr!("action-move-series-down"), false),
        ] {
            // The buttons move what is selected: the group on a group page,
            // the series on a curve page.
            let movable = match *selected_group {
                Some(group) => panel.can_move_group(group, up),
                None => panel.can_move_series(*selected, up),
            };
            if ui
                .add_enabled(movable, egui::Button::new(label))
                .on_disabled_hover_text(if up {
                    tr!("disabled-move-first")
                } else {
                    tr!("disabled-move-last")
                })
                .clicked()
            {
                match *selected_group {
                    Some(group) => {
                        if panel.move_group(group, up) {
                            if let Some(first) = panel.group_members(group).first() {
                                *selected = *first;
                            }
                            commit = true;
                        }
                    }
                    None => {
                        if let Some(index) = panel.move_series(*selected, up) {
                            *selected = index;
                            commit = true;
                        }
                    }
                }
            }
        }
    });
    commit
}

/// The curve page (#510): the fields of the series `index`, its group
/// membership, then its own line and symbol (this curve only). Its colors are
/// grayed out, with their reason, while the color bar of the layer gives them.
fn curve_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    tree: &Tree,
    index: &mut usize,
    env: &mut PickerEnv,
) -> bool {
    let mut commit = false;
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
    let has_overlay = panel.overlay.is_some();
    // The key tells a curve page from a group page: group ids can equal ranks.
    commit |= crate::field_state::for_target(ui, (layer, false, *index), |ui| {
        series_curve_editor(
            ui,
            &mut panel.series[*index],
            auto_bar_width,
            has_overlay,
            &error_columns,
        )
    });
    // The Y axis (#358) goes through the panel, which makes the right axis
    // visible. It is this curve's only (#510 D1: a group page would copy it).
    let axis = panel.series[*index].axis;
    if axis != before.axis {
        panel.set_series_axis(*index, axis);
    }
    commit |= membership_editor(ui, panel, index);
    // The curves of a group on a visible bar take their color from it (#499).
    let group = panel.series[*index].group;
    let color_reason = group
        .filter(|group| {
            panel
                .color_bar
                .as_ref()
                .is_some_and(|bar| bar.visible && bar.segment_of(*group).is_some())
        })
        .map(|_| tr!("curve-color-by-bar"));
    let symbols = panel.series[*index].plot_type.draws_symbols();
    commit |= crate::field_state::for_target(ui, (layer, false, *index), |ui| {
        // A color set here overrides the gradient of the group (#510 D4).
        let hint = (color_reason.is_none()
            && group.is_some_and(|group| panel.group_gradient(group).is_some()))
        .then(|| tr!("curve-color-overrides-gradient"));
        series_style_editor(
            ui,
            &mut panel.series[*index],
            env,
            (color_reason, hint),
            true,
            symbols,
        )
    });
    commit
}

/// The group page (#510): its gestures (name, ungroup, delete), its colors
/// (gradient) and the line and symbol of
/// all its curves, written into each member. `selected` follows the first
/// member; a group that disappears gives the page back to a curve.
#[allow(clippy::too_many_arguments)]
fn group_page(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    layer: usize,
    tree: &Tree,
    group: u32,
    (selected, selected_group): (&mut usize, &mut Option<u32>),
    env: &mut PickerEnv,
    actions: &mut Vec<Action>,
) -> bool {
    let Some(first) = panel.group_members(group).first().copied() else {
        *selected_group = None;
        return false;
    };
    *selected = first;
    let mut commit = form(ui, "curve_group_form", |f| {
        f.section(tr!("curve-group-section"));
        group_editor(f, panel, layer, group, (selected, selected_group))
    });
    // Ungroup or Delete Group: the page goes back to a curve.
    if selected_group.is_none() || panel.series.is_empty() {
        return commit;
    }
    commit |= gradient_editor(ui, panel, group, env, actions);
    // What the curves share: the first member is shown, what changes is copied
    // into every member. Bins and Width only when EVERY curve has that type.
    let members = panel.group_members(group);
    let all = |plot_type: stokesforge_plot::PlotType| {
        members
            .iter()
            .all(|member| panel.series[*member].plot_type == plot_type)
    };
    let shared = (
        all(stokesforge_plot::PlotType::Histogram),
        all(stokesforge_plot::PlotType::Bar),
    );
    let auto_bar_width = {
        let resolved = stokesforge_core::resolve::resolve_series(tree, &panel.series[first]);
        stokesforge_core::resolve::bar_width(resolved.x.as_ref().map(|x| x.as_slice()), None)
    };
    let has_overlay = panel.overlay.is_some();
    // The Symbol section shows if at least one member draws symbols, with the
    // values of the first of them (#547). The page edits `first`: when `first`
    // draws none, it borrows those values for the time of the page.
    let symbol_source = panel
        .group_members(group)
        .into_iter()
        .find(|&member| panel.series[member].plot_type.draws_symbols());
    let own_symbol = panel.series[first].symbol.clone();
    if let Some(source) = symbol_source {
        panel.series[first].symbol = panel.series[source].symbol.clone();
    }
    let before = panel.series[first].clone();
    commit |= crate::field_state::for_target(ui, (layer, true, group), |ui| {
        series_group_editor(
            ui,
            &mut panel.series[first],
            shared,
            auto_bar_width,
            has_overlay,
        )
    });
    // The Y axis goes through the panel, which makes the right axis visible.
    let axis = panel.series[first].axis;
    if axis != before.axis {
        for member in panel.group_members(group) {
            panel.set_series_axis(member, axis);
        }
    }
    commit |= crate::field_state::for_target(ui, (layer, true, group), |ui| {
        series_style_editor(
            ui,
            &mut panel.series[first],
            env,
            (None, None),
            false,
            symbol_source.is_some(),
        )
    });
    propagate_style(panel, group, &before, first);
    if !panel.series[first].plot_type.draws_symbols() {
        panel.series[first].symbol = own_symbol;
    }
    commit
}

/// Membership of the series `index`: dropdown and « Leave Group ». A group
/// stays contiguous (#197): changing membership moves the series, `index`
/// follows it.
fn membership_editor(ui: &mut egui::Ui, panel: &mut PanelSpec, index: &mut usize) -> bool {
    let mut commit = false;
    let group = panel.series[*index].group;
    let mut chosen = group;
    let mut new_group = false;
    form(ui, "curve_membership", |f| {
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
        commit
    })
}

/// The gestures of the group `group` (#197): its name, its dissolution (the
/// series stay) and its deletion with its series. Its place in the drawing
/// order is changed from the list (Move Up / Move Down). After either
/// gesture the page is a curve's: `selected_group` is cleared and `selected`
/// falls back on the group's place.
fn group_editor(
    f: &mut Form<'_>,
    panel: &mut PanelSpec,
    layer: usize,
    group: u32,
    (selected, selected_group): (&mut usize, &mut Option<u32>),
) -> bool {
    let mut commit = false;
    // The text being typed lives only as long as the focus: the rest of the
    // time, the field rereads the model (name changed elsewhere).
    let name_id = egui::Id::new(("curve_group_name", layer, group));
    // An explicit identifier holding the group: the click that moves to
    // another group is applied after this page, so the name is validated
    // on the group it was typed for (#498).
    let name_field_id = egui::Id::new(("curve_group_name_field", layer, group));
    let mut name: String = f
        .ui()
        .data(|d| d.get_temp(name_id))
        .unwrap_or_else(|| panel.group_label(group).unwrap_or_default().to_string());
    f.row(tr!("curve-group-name-field"), |ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut name)
                .id(name_field_id)
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
        if ui
            .button(tr!("curve-group-ungroup"))
            .on_hover_text(tr!("curve-group-ungroup", "hint"))
            .clicked()
            && panel.dissolve_group(group)
        {
            *selected_group = None;
            commit = true;
        }
        if ui
            .button(tr!("curve-group-delete"))
            .on_hover_text(tr!("curve-group-delete", "hint"))
            .clicked()
        {
            let first = panel.group_members(group).first().copied().unwrap_or(0);
            if panel.remove_group(group) {
                *selected = first.min(panel.series.len().saturating_sub(1));
                *selected_group = None;
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
/// applied). A series that joins the group receives its color. The gradient
/// applies as soon as it is chosen or a stop changes (draft, validated by
/// OK). Only the group page calls it (#510).
fn gradient_editor(
    ui: &mut egui::Ui,
    panel: &mut PanelSpec,
    group: u32,
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
    {
        form(ui, "curve_gradient_form", |f| {
            f.section(tr!("curve-colors-section"));
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
            if choice != choice_before || custom != custom_before {
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
        });
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
    use stokesforge_plot::{LineStyle, PlotType};

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
        for series in &mut panel.series {
            series.plot_type = PlotType::LineScatter;
        }
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

    /// #547: the symbol edits of a group page reach only the members that
    /// draw symbols; a Line member keeps its own.
    #[test]
    fn symbol_edits_skip_the_group_members_that_draw_none() {
        let mut panel = sample_panel();
        panel.series[1].plot_type = PlotType::Scatter;
        panel.series[2].plot_type = PlotType::Line;
        panel.series[3].plot_type = PlotType::LineScatter;
        let own = panel.series[2].symbol.clone();
        let before = panel.series[1].clone();
        panel.series[1].symbol.every = stokesforge_plot::SymbolEvery::new(5).unwrap();
        panel.series[1].symbol.size = 12.0;
        panel.series[1].symbol.shape = stokesforge_plot::SymbolShape::Square;
        propagate_style(&mut panel, 1, &before, 1);
        assert_eq!(panel.series[3].symbol.every.get(), 5);
        assert_eq!(panel.series[3].symbol.size, 12.0);
        assert_eq!(
            panel.series[3].symbol.shape,
            stokesforge_plot::SymbolShape::Square
        );
        assert_eq!(panel.series[2].symbol, own, "the Line member is untouched");
    }

    /// #547: a group page shows the Symbol section when at least one member
    /// draws symbols, and not when none does; painting the page never
    /// changes the symbol of a member that draws none.
    #[test]
    fn the_group_page_shows_the_symbol_section_when_one_member_draws_symbols() {
        let ctx = super::super::test_support::context();
        let section = tr!("plot-series-symbol-section");
        let painted = |panel: &mut PanelSpec| {
            let mut selected = 1;
            let mut group: Option<u32> = Some(1);
            page(&ctx, panel, (&mut selected, &mut group), Vec::new());
            let (output, _) = page(&ctx, panel, (&mut selected, &mut group), Vec::new());
            super::super::test_support::texts(&output.shapes)
                .iter()
                .any(|t| t.text == section)
        };
        let mut panel = layer_with_group();
        for member in 1..=3 {
            panel.series[member].plot_type = PlotType::Line;
        }
        assert!(!painted(&mut panel), "only Line members: hidden");
        panel.series[3].plot_type = PlotType::Scatter;
        panel.series[3].symbol.size = 13.0;
        let first_symbol = panel.series[1].symbol.clone();
        assert!(painted(&mut panel), "a Scatter member: shown");
        assert_eq!(
            panel.series[1].symbol, first_symbol,
            "the Line first member keeps its own symbol"
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
        (selected, group): (&mut usize, &mut Option<u32>),
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
                    group,
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
        (selected, group): (&mut usize, &mut Option<u32>),
        pos: egui::Pos2,
    ) -> bool {
        let mut commit = page(
            ctx,
            panel,
            (&mut *selected, &mut *group),
            vec![egui::Event::PointerMoved(pos)],
        )
        .1;
        for pressed in [true, false] {
            commit |= page(
                ctx,
                panel,
                (&mut *selected, &mut *group),
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
        let mut group: Option<u32> = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
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
        let mut group: Option<u32> = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
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
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let pos = center_of(&output, &tr!("curve-group-ungroup"));
        assert!(click_at(&ctx, &mut panel, (&mut selected, &mut group), pos));
        assert!(panel.series.iter().all(|s| s.group.is_none()));
        assert_eq!(panel.series[1].line.width, 4.5, "the style stays");
        assert_eq!(panel.series.len(), 4);
        assert_eq!(group, None, "the page is a curve's now");

        let mut panel = layer_with_group();
        let mut selected = 3;
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let pos = center_of(&output, &tr!("curve-group-delete"));
        assert!(click_at(&ctx, &mut panel, (&mut selected, &mut group), pos));
        assert_eq!(panel.series.len(), 1, "A stays single");
        assert!(panel.group_names.is_empty());
        let (_, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert_eq!(
            selected, 0,
            "the selection falls back on an existing series"
        );
        assert_eq!(group, None, "the page is a curve's now");
    }

    /// A line width typed for one series and a click on another series row:
    /// the width goes to the series it was typed for, the clicked one opens
    /// untouched (#498). Witness: `*selected = index` applied before the page.
    #[test]
    fn a_width_typed_for_one_series_stays_on_it_when_another_row_is_clicked() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut selected = 0; // "A", outside a group
        let mut group: Option<u32> = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let width = format!("{}", panel.series[0].line.width);
        let field = center_of(&output, &width);
        click_at(&ctx, &mut panel, (&mut selected, &mut group), field);
        page(
            &ctx,
            &mut panel,
            (&mut selected, &mut group),
            vec![egui::Event::Text("7".into())],
        );
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let row = center_of(&output, "C");
        let untouched = panel.series[2].line.width;
        click_at(&ctx, &mut panel, (&mut selected, &mut group), row);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert_eq!(panel.series[0].line.width, 7.0, "A took the width");
        assert_eq!(panel.series[2].line.width, untouched, "C is untouched");
        assert_eq!(selected, 2, "the clicked row is the edited series");
    }

    /// A group name typed and a click on another group header: the name is
    /// validated on the first group (#498). Witness: the pick applied before
    /// the page.
    #[test]
    fn a_group_name_typed_for_one_group_is_kept_when_another_group_is_clicked() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        for (n, name) in [(5, "E"), (6, "F")] {
            let mut series = SeriesSpec::new(NodeId::from_raw(n));
            series.name = name.into();
            series.group = Some(2);
            panel.series.push(series);
        }
        assert!(panel.rename_group(2, "Second"));
        let mut selected = 1; // "B", first member of "Batch"
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        // "Batch" is painted twice: the list header (left) and the name field.
        let field = super::super::test_support::texts(&output.shapes)
            .into_iter()
            .filter(|t| t.text == "Batch")
            .max_by(|a, b| a.center.x.total_cmp(&b.center.x))
            .expect("the name field")
            .center;
        click_at(&ctx, &mut panel, (&mut selected, &mut group), field);
        page(
            &ctx,
            &mut panel,
            (&mut selected, &mut group),
            vec![egui::Event::Text("Foo".into())],
        );
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let header = center_of(&output, "Second");
        click_at(&ctx, &mut panel, (&mut selected, &mut group), header);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        // The caret sits inside the old name: the typed text is in the middle.
        assert!(
            panel
                .group_label(1)
                .is_some_and(|name| name.contains("Foo")),
            "the first group is renamed: {:?}",
            panel.group_label(1)
        );
        assert_eq!(
            panel.group_label(2),
            Some("Second"),
            "the second is untouched"
        );
        assert_eq!(group, Some(2), "the clicked group's page is open");
    }

    /// #197: joining a group places the series after its members, and
    /// the selection follows the series.
    #[test]
    fn joining_a_group_moves_the_series_and_the_selection_follows() {
        let mut panel = layer_with_group();
        let mut ui_selected = 0;
        let mut group: Option<u32> = None;
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
        let (output, _) = page(&ctx, &mut panel, (&mut ui_selected, &mut group), Vec::new());
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
        let mut group: Option<u32> = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        // Other rows of the page also read "None": the one of the Group row.
        let row_y = center_of(&output, &tr!("curve-group-field")).y;
        let list = super::super::test_support::texts(&output.shapes)
            .into_iter()
            .find(|t| t.text == tr!("curve-group-none") && (t.center.y - row_y).abs() < 4.0)
            .expect("the Group dropdown")
            .center;
        assert!(
            !click_at(&ctx, &mut panel, (&mut selected, &mut group), list),
            "opening the list changes nothing"
        );
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let pos = center_of(
            &output,
            &tr!("curve-group-entry", group = "Batch", count = 3),
        );
        assert!(
            click_at(&ctx, &mut panel, (&mut selected, &mut group), pos),
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
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert!(
            super::super::test_support::texts(&output.shapes)
                .iter()
                .any(|t| t.text == tr!("curve-group-entry", group = "Batch", count = 4)),
            "the list now shows the series' group"
        );
    }

    /// The texts painted at the left of the list (its header and rows).
    fn bold_in_the_list(output: &egui::FullOutput, text: &str) -> Option<bool> {
        super::super::test_support::texts(&output.shapes)
            .into_iter()
            .filter(|t| t.text == text)
            .min_by(|a, b| a.center.x.total_cmp(&b.center.x))
            .map(|t| t.bold)
    }

    /// #510: the list selection chooses the page. A group header opens the
    /// group page (Name, Ungroup, gradient; no curve fields), a member the
    /// curve page (curve fields, Leave Group; no Ungroup, no gradient).
    /// Witness: a header pick that sets only the series leaves the curve page.
    #[test]
    fn the_list_selection_chooses_the_group_page_or_the_curve_page() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut selected = 0;
        let mut group: Option<u32> = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let header = center_of(&output, "Batch");
        click_at(&ctx, &mut panel, (&mut selected, &mut group), header);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert_eq!(group, Some(1));
        let shown = |output: &egui::FullOutput, key: String| {
            super::super::test_support::texts(&output.shapes)
                .iter()
                .any(|t| t.text == key)
        };
        assert!(shown(&output, tr!("curve-group-ungroup")));
        assert!(shown(&output, tr!("curve-gradient-field")));
        assert!(!shown(&output, tr!("plot-series-plot-type")));
        assert!(!shown(&output, tr!("curve-group-leave")));

        // One highlight: the header, no row.
        assert_eq!(bold_in_the_list(&output, "Batch"), Some(true));
        for row in ["A", "B", "C", "D"] {
            assert_eq!(bold_in_the_list(&output, row), Some(false), "{row}");
        }

        let member = center_of(&output, "C");
        click_at(&ctx, &mut panel, (&mut selected, &mut group), member);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert_eq!((selected, group), (2, None));
        assert!(shown(&output, tr!("curve-group-leave")));
        assert!(shown(&output, tr!("plot-series-plot-type")));
        assert!(!shown(&output, tr!("curve-group-ungroup")));
        assert!(!shown(&output, tr!("curve-gradient-field")));
        // The reverse: the row, no header.
        assert_eq!(bold_in_the_list(&output, "C"), Some(true));
        assert_eq!(bold_in_the_list(&output, "Batch"), Some(false));
        assert_eq!(bold_in_the_list(&output, "B"), Some(false));
    }

    /// #510: a width typed on the group page reaches every member, and the
    /// member page opened by the next click is untouched; the reverse, a width
    /// typed on a curve page, changes that member only. Witness: a group page
    /// that uses the same target as its first member leaves the draft
    /// unvalidated when the click moves on.
    #[test]
    fn a_width_typed_on_the_group_page_reaches_every_member_and_not_the_reverse() {
        let ctx = super::super::test_support::context();
        // Group page, then the first member.
        let mut panel = layer_with_group();
        let mut selected = 1;
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let width = format!("{}", panel.series[1].line.width);
        let field = center_of(&output, &width);
        click_at(&ctx, &mut panel, (&mut selected, &mut group), field);
        page(
            &ctx,
            &mut panel,
            (&mut selected, &mut group),
            vec![egui::Event::Text("7".into())],
        );
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let row = center_of(&output, "B");
        click_at(&ctx, &mut panel, (&mut selected, &mut group), row);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        for member in 1..=3 {
            assert_eq!(panel.series[member].line.width, 7.0, "member {member}");
        }
        assert_eq!(
            panel.series[0].line.width,
            SeriesSpec::new(NodeId::from_raw(1)).line.width
        );
        assert_eq!((selected, group), (1, None), "the member page is open");

        // Curve page of C, then the header.
        let mut panel = layer_with_group();
        let mut selected = 2;
        let mut group: Option<u32> = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let before = panel.series[2].line.width;
        let field = center_of(&output, &format!("{before}"));
        click_at(&ctx, &mut panel, (&mut selected, &mut group), field);
        page(
            &ctx,
            &mut panel,
            (&mut selected, &mut group),
            vec![egui::Event::Text("9".into())],
        );
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let header = center_of(&output, "Batch");
        click_at(&ctx, &mut panel, (&mut selected, &mut group), header);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert_eq!(panel.series[2].line.width, 9.0, "C took the width");
        assert_eq!(panel.series[1].line.width, before, "B untouched");
        assert_eq!(panel.series[3].line.width, before, "D untouched");
        assert_eq!(group, Some(1), "the group page is open");

        // A draft left open on the group page does not show on the page of its
        // first member when the page changes by another way than a pick: the
        // two pages have different targets.
        let mut panel = layer_with_group();
        let mut selected = 1;
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let width = format!("{}", panel.series[1].line.width);
        let field = center_of(&output, &width);
        click_at(&ctx, &mut panel, (&mut selected, &mut group), field);
        page(
            &ctx,
            &mut panel,
            (&mut selected, &mut group),
            vec![egui::Event::Text("9".into())],
        );
        group = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert!(
            super::super::test_support::texts(&output.shapes)
                .iter()
                .any(|t| t.text == width),
            "the member page shows its own width"
        );
    }

    /// #510: the list's Move Up / Move Down move what is selected: the whole
    /// group on a group page (which stays on it), one series within its group
    /// on a curve page. Witness: `move_series` whatever the page.
    #[test]
    fn the_list_buttons_move_the_selected_group_or_the_selected_series() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut extra = SeriesSpec::new(NodeId::from_raw(5));
        extra.name = "E".into();
        panel.series.push(extra);
        let mut selected = 1;
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let down = center_of(&output, &tr!("action-move-series-down"));
        assert!(click_at(
            &ctx,
            &mut panel,
            (&mut selected, &mut group),
            down
        ));
        let names: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["A", "E", "B", "C", "D"], "the group moved");
        assert_eq!(group, Some(1), "the group page stays");
        assert_eq!(panel.series[selected].name, "B");

        // On a member: it moves within its group.
        let mut panel = layer_with_group();
        let mut selected = 1;
        let mut group: Option<u32> = None;
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let down = center_of(&output, &tr!("action-move-series-down"));
        assert!(click_at(
            &ctx,
            &mut panel,
            (&mut selected, &mut group),
            down
        ));
        let names: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["A", "C", "B", "D"], "B moved within the group");
        assert_eq!(panel.series[selected].name, "B");
        assert_eq!(group, None);
    }

    /// #510: the colour bar is not on the group page: a group that IS on the
    /// bar shows its gradient editor and nothing of the bar (no box, no
    /// range, no break label, no chain, no link). Graph Properties › Legend
    /// holds them. Witness: the box (or any bar row) drawn again fails.
    #[test]
    fn the_group_page_has_no_color_bar_rows() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        for (n, name) in [(5, "E"), (6, "F")] {
            let mut series = SeriesSpec::new(NodeId::from_raw(n));
            series.name = name.into();
            series.group = Some(2);
            panel.series.push(series);
        }
        super::super::color_bar_settings::put_group_on_bar(&mut panel, 1);
        super::super::color_bar_settings::put_group_on_bar(&mut panel, 2);
        let mut selected = 1;
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        let painted = super::super::test_support::texts(&output.shapes);
        let shown = |key: String| painted.iter().any(|t| t.text == key);
        assert!(shown(tr!("curve-gradient-field")), "the page is drawn");
        for key in [
            tr!("plot-color-bar-group-on"),
            tr!("plot-color-bar-chain"),
            tr!("plot-color-bar-group-break"),
            tr!("plot-axes-from"),
        ] {
            assert!(!shown(key.clone()), "{key}");
        }
    }

    /// #510: a group that no longer exists under the draft (an Undo, a
    /// script) leaves the group page for the page of a curve that exists.
    /// Witness: a stale `selected_group` keeps drawing a page of nothing.
    #[test]
    fn a_group_removed_under_the_draft_gives_the_page_back_to_a_curve() {
        let ctx = super::super::test_support::context();
        let mut panel = layer_with_group();
        let mut selected = 1;
        let mut group: Option<u32> = Some(1);
        page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert!(panel.remove_group(1));
        let (output, _) = page(&ctx, &mut panel, (&mut selected, &mut group), Vec::new());
        assert_eq!(group, None);
        assert!(selected < panel.series.len());
        assert!(super::super::test_support::texts(&output.shapes)
            .iter()
            .any(|t| t.text == tr!("plot-series-plot-type")));
    }

    /// Opens the color picker of the line color on the curve page of series
    /// `selected`: true if the picker (its « Custom color » box) appears.
    fn line_color_opens(panel: &mut PanelSpec, selected: usize) -> bool {
        let ctx = super::super::test_support::context();
        let mut selected = selected;
        let mut group: Option<u32> = None;
        page(&ctx, panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, panel, (&mut selected, &mut group), Vec::new());
        let label = center_of(&output, &tr!("plot-series-line-color"));
        let swatch = output
            .shapes
            .iter()
            .filter_map(|painted| match &painted.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.width() <= 24.0
                        && (rect.rect.center().y - label.y).abs() < 3.0
                        && rect.rect.center().x > label.x + 20.0 =>
                {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .min_by(|a, b| a.x.total_cmp(&b.x))
            .expect("the color swatch");
        click_at(&ctx, panel, (&mut selected, &mut group), swatch);
        page(&ctx, panel, (&mut selected, &mut group), Vec::new());
        let (output, _) = page(&ctx, panel, (&mut selected, &mut group), Vec::new());
        super::super::test_support::texts(&output.shapes)
            .iter()
            .any(|t| t.text == tr!("color-custom"))
    }

    /// #510 D4: a member of a gradient group keeps an editable color on its
    /// curve page; it is greyed only while a visible color bar gives it.
    /// Witness: a reason taken from the gradient too greys the first case.
    #[test]
    fn a_member_of_a_gradient_group_keeps_its_own_color_unless_the_bar_gives_it() {
        let mut panel = layer_with_group();
        let first: Vec<&str> = stokesforge_plot::GRADIENT_PRESETS[0].stops.to_vec();
        assert!(panel.set_group_gradient(1, &first));
        assert!(line_color_opens(&mut panel, 1), "gradient only: editable");
        super::super::color_bar_settings::put_group_on_bar(&mut panel, 1);
        assert!(!line_color_opens(&mut panel, 1), "bar: greyed");
    }

    /// The live series page, to drive a drag and drop frame by
    /// frame: the draft changes right away, without preview.
    struct Bench {
        project: stokesforge_core::Project,
        panel: PanelSpec,
        selected: usize,
        group: Option<u32>,
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
                        &mut bench.group,
                        &mut env,
                        &mut actions,
                    );
                },
                Bench {
                    project: stokesforge_core::Project::new("Series").unwrap(),
                    panel,
                    selected,
                    group: None,
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
        // Grabbing the header chooses the group page, on its first series "B".
        assert_eq!(bench.group, Some(1));
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

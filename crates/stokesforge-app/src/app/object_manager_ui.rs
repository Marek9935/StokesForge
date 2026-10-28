//! Painted Object Manager (#81–#85, #102, #111, #113): the
//! [`stokesforge_app_state::object_manager::ObjectTree`] tree as rows,
//! visibility checkboxes, context menus, drag by handle (series, groups, free
//! objects; `object_dnd.rs`) and keyboard shortcuts (Delete, F2, Ctrl+arrows).
//! The model (projection of the `PlotSpec`, filters) stays in
//! `stokesforge_app_state::object_manager`; the selection lives in the
//! `GraphView`. What touches the view (selection, properties, renaming) goes up
//! as an [`ObjectManagerRequest`], run after painting by
//! [`StokesForgeApp::run_object_manager_request`], which also serves the menu
//! "Graph › Selected Content". One piece of code for the fixed panel and the
//! detached window. The settings (#139) come from the single table
//! `preferences::ObjectManagerSettings`; egui keeps priority for the groups
//! already seen, including after Reset.
//! The legend sample receives the same gestures as the series name (#151): a
//! zone that was only painted left its right click to the background.
//!
//! The fixed or detached panel (#100),
//! [`StokesForgeApp::object_manager_panel`], and the running of requests have
//! joined it (#122). Headers, layers, series, axes and objects are painted by
//! sections (#124), without changing the egui scopes and ids nor the order of
//! requests and tree connectors.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::GraphView;

use crate::icons::{icon_image, Icon, IconRole};
use stokesforge_app_state::object_drag::{DragItem, DropOver};
use stokesforge_app_state::object_manager::{
    GroupRow, LayerRow, LayerSlot, ObjectRow, ObjectTree, SeriesRow, Shown, VisibilityTarget as T,
};
use stokesforge_app_state::preferences::{ObjectManagerGroup, ObjectManagerSettings};
use stokesforge_app_state::state::{active_graph, Action};
use stokesforge_app_state::tr;

use super::empty_state::empty_state;
use super::graph_text_edit::{
    open_annotation_editor, open_legend_entry_editor, open_series_name_editor, open_text_editor,
    ContextText,
};
use super::graph_window::open_shape_properties;
use super::inplace_edit;
use super::list_row::{
    list_row_grip, list_row_text, list_row_text_color, neighbour, read_navigation,
    with_list_row_height, ListRowBackground, NavKey,
};
use super::menu_sections::MenuSections;
use super::object_dnd::{self, Hover};
use super::reorder_list::reorder_row_with;
use super::shortcuts::{
    shortcut_text_delete, SHORTCUT_MOVE_SERIES_DOWN, SHORTCUT_MOVE_SERIES_UP,
    SHORTCUT_RENAME_OBJECT,
};
use super::tab::active_tab_kind;
use super::tab::Tab;
use super::window_chrome::{panel_header, PanelPlacement, PANEL_STRIP_THICKNESS};
use super::window_detach::ObjectManagerModel;
use super::StokesForgeApp;

#[derive(Clone, Copy)]
pub(super) enum ObjectManagerRequest {
    Select(stokesforge_app_state::object_manager::VisibilityTarget),
    Analysis(
        stokesforge_app_state::object_manager::VisibilityTarget,
        stokesforge_app_state::analysis::Category,
    ),
    Properties(stokesforge_app_state::object_manager::VisibilityTarget),
    Rename(stokesforge_plot::SeriesRef),
    /// Edits a series group's name in place (#197).
    RenameGroup(stokesforge_plot::SeriesGroupRef),
    /// Edits the title, the axis title or the legend entry in place.
    EditText(ContextText),
}
/// Label of an Object Manager header node (graph, layer, groups): a
/// double-click opens its settings, like the other rows (#113).
fn manager_header_label(
    ui: &mut egui::Ui,
    text: &str,
    target: stokesforge_app_state::object_manager::VisibilityTarget,
    requests: &mut Vec<ObjectManagerRequest>,
) -> egui::Response {
    let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    // A click on a layer's node (or its group) makes it active (#161).
    if response.clicked()
        && matches!(
            target,
            stokesforge_app_state::object_manager::VisibilityTarget::Panel(_)
                | stokesforge_app_state::object_manager::VisibilityTarget::Group(_)
                | stokesforge_app_state::object_manager::VisibilityTarget::Axes(_)
        )
    {
        requests.push(ObjectManagerRequest::Select(target));
    }
    if response.double_clicked() {
        requests.push(ObjectManagerRequest::Properties(target));
    }
    response
}

pub(super) fn selected_manager_target(
    view: &GraphView,
) -> Option<stokesforge_app_state::object_manager::VisibilityTarget> {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    view.selected_group()
        .map(T::SeriesGroup)
        .or_else(|| view.selected_series().map(T::Series))
        .or_else(|| view.selected_annotation().map(T::Annotation))
        .or_else(|| view.selected_shape().map(T::Shape))
        .or_else(|| view.selected_text().map(T::Text))
}
pub(super) fn select_manager_target(
    view: &mut GraphView,
    target: Option<stokesforge_app_state::object_manager::VisibilityTarget>,
    spec: &stokesforge_plot::PlotSpec,
) {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    match target {
        Some(T::Series(t)) => view.select_series(Some(t), spec),
        Some(T::SeriesGroup(t)) => view.select_series_group(Some(t), spec),
        Some(T::Annotation(t)) => view.select_annotation(Some(t), spec),
        Some(T::Shape(t)) if t.get(spec).is_some() => {
            view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
            view.select_shape(Some(t));
        }
        Some(T::Text(t)) => view.select_text(Some(t), spec),
        Some(T::Panel(i) | T::Group(i) | T::Axes(i)) => view.set_active_panel(i, spec),
        Some(T::Axis(axis)) => {
            view.set_active_panel(axis.panel, spec);
        }
        _ => {
            view.select_annotation(None, spec);
        }
    }
}

/// Analyzable series of the target, in plotting order. A "row" series is
/// analyzed through the transposed sheet linked to its sheet (#253); a group
/// that mixes column curves and row curves cannot be analyzed in one go
/// (`analysis-needs-same-curves`).
pub(super) fn analysis_sources(
    spec: &stokesforge_plot::PlotSpec,
    target: T,
) -> Vec<stokesforge_plot::SeriesRef> {
    match target {
        T::Series(source) => spec
            .panels
            .get(source.panel)
            .and_then(|p| p.series.get(source.index))
            .map(|_| vec![source])
            .unwrap_or_default(),
        T::SeriesGroup(group) => spec
            .panels
            .get(group.panel)
            .map(|panel| {
                let members: Vec<_> = panel
                    .series
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.group == Some(group.group))
                    .collect();
                let rows = members.iter().filter(|(_, s)| s.row.is_some()).count();
                if rows != 0 && rows != members.len() {
                    return Vec::new();
                }
                members
                    .into_iter()
                    .map(|(index, _)| stokesforge_plot::SeriesRef::new(group.panel, index))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// **The** menu of a graph object (#152): the same on a right click in the
/// figure, on its Object Manager node and under Graph › Selection (which wraps
/// it in its submenu; the right click shows it flat). Sections of the grammar
/// of `docs/menus.md`, a section with nothing for the target being **absent**:
///
/// 1. main — Show Data / Analysis (curve), Analysis (group),
///    Edit Text / Reset Position (texts),
///    Select Members (group);
/// 2. organize — Rename (curve, group), Move Up/Down (curve, group),
///    Ungroup (group), Hide (curve), Bring to Front / Send to Back (object),
///    Hide (text, axis, group);
///    Closed Frame (layer and axes group, toggle, #344);
/// 3. delete — Delete (curve, free text, object), Delete Group (group
///    and its series);
/// 4. `Properties…`, always last.
pub(super) fn object_node_menu(
    ui: &mut egui::Ui,
    graph: NodeId,
    target: T,
    spec: &stokesforge_plot::PlotSpec,
    shown: Shown,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    let exists = target.exists(spec);
    let visibility = target.visibility_shown(spec, shown);
    let editable_text = match target {
        T::Annotation(_) => Some(true),
        T::Text(text) => Some(!matches!(text, stokesforge_plot::TextTarget::Legend { .. })),
        _ => None,
    };
    let text_moved = match target {
        T::Text(text) => Some(text.anchor(spec).is_some()),
        _ => None,
    };
    let hideable = !matches!(target, T::Annotation(_) | T::Shape(_));
    let deletable = target.removable(spec) && !matches!(target, T::Text(_));
    let mut sections = MenuSections::new();
    sections.section(
        ui,
        matches!(target, T::Series(_) | T::SeriesGroup(_))
            || text_moved.is_some()
            || editable_text == Some(true),
        |ui| {
            if matches!(target, T::SeriesGroup(_))
                && ui
                    .add_enabled(exists, egui::Button::new(tr!("action-select-group-series")))
                    .on_hover_text(tr!("action-select-group-series", "hint"))
                    .on_disabled_hover_text(tr!("disabled-object-gone"))
                    .clicked()
            {
                requests.push(ObjectManagerRequest::Select(target));
                ui.close();
            }
            if let T::Series(series) = target {
                if ui
                    .add_enabled(exists, egui::Button::new(tr!("action-show-graph-data")))
                    .on_disabled_hover_text(tr!("disabled-object-gone"))
                    .clicked()
                {
                    actions.push(Action::ShowSeriesData { graph, series });
                    ui.close();
                }
            }
            if matches!(target, T::Series(_) | T::SeriesGroup(_)) {
                super::analysis_menu::analysis_open_menu(
                    ui,
                    !analysis_sources(spec, target).is_empty(),
                    tr!("analysis-needs-same-curves"),
                    |category| requests.push(ObjectManagerRequest::Analysis(target, category)),
                );
            }
            if editable_text == Some(true)
                && ui
                    .add_enabled(exists, egui::Button::new(tr!("action-edit-text")))
                    .on_disabled_hover_text(tr!("disabled-object-gone"))
                    .clicked()
            {
                match target {
                    T::Annotation(_) => requests.push(ObjectManagerRequest::Properties(target)),
                    T::Text(text) => {
                        requests.push(ObjectManagerRequest::EditText(ContextText::Title(text)))
                    }
                    _ => {}
                }
                ui.close();
            }
            if let (T::Text(text), Some(moved)) = (target, text_moved) {
                if ui
                    .add_enabled(moved, egui::Button::new(tr!("action-reset-text-position")))
                    .on_hover_text(tr!("action-reset-text-position", "hint"))
                    .on_disabled_hover_text(tr!("disabled-text-not-moved"))
                    .clicked()
                {
                    actions.push(Action::SetTextAnchor {
                        graph,
                        target: text,
                        anchor: None,
                    });
                    ui.close();
                }
            }
        },
    );
    sections.section(
        ui,
        matches!(target, T::Series(_) | T::Shape(_)) || hideable,
        |ui| {
            if let T::Series(series) = target {
                if ui
                    .add_enabled(
                        exists,
                        egui::Button::new(tr!("action-rename-series"))
                            .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_RENAME_OBJECT)),
                    )
                    .on_disabled_hover_text(tr!("disabled-object-gone"))
                    .clicked()
                {
                    requests.push(ObjectManagerRequest::Rename(series));
                    ui.close();
                }
                let panel = spec.panels.get(series.panel);
                let moves = [
                    (tr!("action-move-series-up"), SHORTCUT_MOVE_SERIES_UP, true),
                    (
                        tr!("action-move-series-down"),
                        SHORTCUT_MOVE_SERIES_DOWN,
                        false,
                    ),
                ];
                for (label, shortcut, up) in moves {
                    let movable = panel.is_some_and(|p| p.can_move_series(series.index, up));
                    if ui
                        .add_enabled(
                            movable,
                            egui::Button::new(label)
                                .shortcut_text(ui.ctx().format_shortcut(&shortcut)),
                        )
                        .on_disabled_hover_text(move_reason(up))
                        .clicked()
                    {
                        actions.push(Action::MoveSeries { graph, series, up });
                        ui.close();
                    }
                }
            }
            if let T::SeriesGroup(group) = target {
                if ui
                    .add_enabled(
                        exists,
                        egui::Button::new(tr!("action-rename-group"))
                            .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_RENAME_OBJECT)),
                    )
                    .on_disabled_hover_text(tr!("disabled-object-gone"))
                    .clicked()
                {
                    requests.push(ObjectManagerRequest::RenameGroup(group));
                    ui.close();
                }
                let panel = spec.panels.get(group.panel);
                let moves = [
                    (tr!("action-move-series-up"), SHORTCUT_MOVE_SERIES_UP, true),
                    (
                        tr!("action-move-series-down"),
                        SHORTCUT_MOVE_SERIES_DOWN,
                        false,
                    ),
                ];
                for (label, shortcut, up) in moves {
                    let movable = panel.is_some_and(|p| p.can_move_group(group.group, up));
                    if ui
                        .add_enabled(
                            movable,
                            egui::Button::new(label)
                                .shortcut_text(ui.ctx().format_shortcut(&shortcut)),
                        )
                        .on_disabled_hover_text(move_reason(up))
                        .clicked()
                    {
                        actions.push(Action::MoveSeriesGroup { graph, group, up });
                        ui.close();
                    }
                }
                if ui
                    .add_enabled(exists, egui::Button::new(tr!("action-ungroup-series")))
                    .on_hover_text(tr!("action-ungroup-series", "hint"))
                    .on_disabled_hover_text(tr!("disabled-object-gone"))
                    .clicked()
                {
                    actions.push(Action::UngroupSeries { graph, group });
                    ui.close();
                }
            }
            if let T::Shape(object) = target {
                let count = spec
                    .panels
                    .get(object.panel)
                    .map_or(0, |panel| panel.shapes.len());
                for order in [
                    stokesforge_app_state::state::ShapeOrder::ToFront,
                    stokesforge_app_state::state::ShapeOrder::ToBack,
                ] {
                    let (label, hint, index) = match order {
                        stokesforge_app_state::state::ShapeOrder::ToFront => (
                            tr!("action-bring-to-front"),
                            tr!("action-bring-to-front", "hint"),
                            count.saturating_sub(1),
                        ),
                        stokesforge_app_state::state::ShapeOrder::ToBack => (
                            tr!("action-send-to-back"),
                            tr!("action-send-to-back", "hint"),
                            0,
                        ),
                    };
                    if ui
                        .add_enabled(exists, egui::Button::new(label))
                        .on_hover_text(hint)
                        .on_disabled_hover_text(tr!("disabled-object-gone"))
                        .clicked()
                    {
                        actions.push(Action::ReorderShape {
                            graph,
                            target: object,
                            order,
                        });
                        // The rank changes: the selection follows the object.
                        requests.push(ObjectManagerRequest::Select(T::Shape(
                            stokesforge_plot::ShapeRef { index, ..object },
                        )));
                        ui.close();
                    }
                }
            }
            if hideable
                && ui
                    .add_enabled(
                        visibility.enabled,
                        egui::Button::new(if visibility.checked {
                            tr!("action-hide-content")
                        } else {
                            tr!("action-show-content")
                        }),
                    )
                    .on_disabled_hover_text(visibility_reason(target, spec))
                    .clicked()
            {
                actions.push(Action::SetPlotVisibility {
                    graph,
                    target,
                    visible: !visibility.checked,
                });
                ui.close();
            }
        },
    );
    sections.section(ui, deletable, |ui| {
        if ui
            .add(
                egui::Button::new(tr!("action-delete-content"))
                    .shortcut_text(shortcut_text_delete()),
            )
            .clicked()
        {
            actions.push(Action::RemovePlotObject { graph, target });
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(exists, egui::Button::new(tr!("action-properties")))
            .on_hover_text(tr!("action-properties", "hint"))
            .on_disabled_hover_text(tr!("disabled-object-gone"))
            .clicked()
        {
            requests.push(ObjectManagerRequest::Properties(target));
            ui.close();
        }
    });
}

/// Why a series or a group can no longer move up or down (#226).
fn move_reason(up: bool) -> String {
    if up {
        tr!("disabled-move-first")
    } else {
        tr!("disabled-move-last")
    }
}

/// Why an object's Show/Hide checkbox or entry is grayed out (#226): a mirror
/// axis already drawn by a layer above, otherwise nothing to toggle.
fn visibility_reason(
    target: stokesforge_app_state::object_manager::VisibilityTarget,
    spec: &stokesforge_plot::PlotSpec,
) -> String {
    if target.is_blocked_mirror(spec) {
        tr!("disabled-axis-blocked")
    } else {
        tr!("disabled-visibility-empty")
    }
}

fn object_visibility(
    ui: &mut egui::Ui,
    graph: NodeId,
    target: stokesforge_app_state::object_manager::VisibilityTarget,
    spec: &stokesforge_plot::PlotSpec,
    shown: Shown,
    actions: &mut Vec<Action>,
) {
    let state = target.visibility_shown(spec, shown);
    let mut checked = state.checked;
    if ui
        .add_enabled(
            state.enabled,
            egui::Checkbox::new(&mut checked, "").indeterminate(state.mixed),
        )
        .on_hover_text(tr!("object-manager-visibility"))
        .on_disabled_hover_text(visibility_reason(target, spec))
        .changed()
    {
        actions.push(Action::SetPlotVisibility {
            graph,
            target,
            visible: checked,
        });
    }
}

/// The Object Manager settings menu, **flat** (#152): it is the right-click
/// menu on the panel background, without a "Settings ▸" wrapper. The
/// checkboxes keep the menu open (#114). The settings touch neither the
/// `PlotSpec` nor egui's current collapse states.
pub(super) fn object_manager_settings_contents(
    ui: &mut egui::Ui,
    settings: ObjectManagerSettings,
    actions: &mut Vec<Action>,
) {
    let config = egui::containers::menu::MenuConfig::new()
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        let mut show = settings.show_scales;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-axes"))
            .on_hover_text(tr!("object-manager-show-axes", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowScales(show));
        }
        let mut show = settings.show_objects;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-objects"))
            .on_hover_text(tr!("object-manager-show-objects", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowObjects(show));
        }
        egui::containers::menu::SubMenuButton::new(tr!("object-manager-default-expansion"))
            .config(config.clone())
            .ui(ui, |ui| {
                for group in ObjectManagerGroup::ALL {
                    let label = match group {
                        ObjectManagerGroup::Graph => tr!("window-kind-graph"),
                        ObjectManagerGroup::Layers => tr!("object-manager-expand-layers"),
                        ObjectManagerGroup::Plots => tr!("object-manager-expand-plots"),
                        ObjectManagerGroup::Axes => tr!("object-manager-axes"),
                        ObjectManagerGroup::Objects => tr!("object-manager-objects"),
                    };
                    let mut open = settings.group_open(group);
                    if ui.checkbox(&mut open, label).changed() {
                        actions.push(Action::SetObjectManagerGroupOpen { group, open });
                    }
                }
            })
            .0
            .on_hover_text(tr!("object-manager-default-expansion", "hint"));
    });
    sections.section(ui, true, |ui| {
        if ui
            .button(tr!("object-manager-reset"))
            .on_hover_text(tr!("object-manager-reset", "hint"))
            .clicked()
        {
            actions.push(Action::ResetObjectManagerSettings);
            ui.close();
        }
    });
}

/// The same settings under View › Object Manager: the top menu's wrapper, the
/// content being that of the right click on the background. Also works in a
/// detached window.
pub(super) fn object_manager_settings_menu(
    ui: &mut egui::Ui,
    settings: ObjectManagerSettings,
    actions: &mut Vec<Action>,
) {
    object_manager_settings_contents(ui, settings, actions);
}

/// One content for the Object Manager, wherever it sits.
pub(super) fn object_manager_contents(
    ui: &mut egui::Ui,
    tree: &Tree,
    model: Option<&stokesforge_app_state::object_manager::ObjectTree>,
    spec: Option<&stokesforge_plot::PlotSpec>,
    settings: ObjectManagerSettings,
    actions: &mut Vec<Action>,
) -> Vec<ObjectManagerRequest> {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    let mut requests = Vec::new();
    let background = ui.interact(
        ui.available_rect_before_wrap(),
        ui.make_persistent_id("object_manager_background"),
        egui::Sense::click(),
    );
    egui::Popup::context_menu(&background)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| object_manager_settings_contents(ui, settings, actions));
    let (Some(model), Some(spec)) = (model, spec) else {
        empty_state(ui, tr!("object-manager-no-graph"));
        return requests;
    };
    // During a drag, the list is painted from the gesture's preview (the real
    // spec only changes on release).
    let live = object_dnd::live(ui.ctx()).filter(|live| live.graph == Some(model.graph));
    let item = live.as_ref().map(|live| live.item);
    let preview_model;
    let (model, shown_spec) = match live.as_ref().and_then(|live| live.preview.as_ref()) {
        Some(preview) => {
            preview_model = stokesforge_app_state::object_manager::object_tree_of_spec(
                tree,
                model.graph,
                &model.title,
                preview,
                item.map(DragItem::target),
                settings.show_scales,
                settings.show_objects,
            );
            (&preview_model, preview)
        }
        None => (model, spec),
    };
    let focus = ui.make_persistent_id(("object_manager_keys", model.graph));
    // #126: register the anchor before the rows. egui gives its focus back on
    // click release, since Rect::NOTHING is never hovered; the clicked row
    // then claims it. A click elsewhere always gives it back, and an edit
    // field keeps its own keys.
    ui.interact(
        egui::Rect::NOTHING,
        focus,
        egui::Sense::focusable_noninteractive(),
    );
    let mut selected = None;
    let paint = ObjectManagerPaint {
        tree,
        spec: shown_spec,
        graph: model.graph,
        focus,
        settings,
        item,
        hover: Hover::default(),
        visible: std::cell::RefCell::new(Vec::new()),
        has_focus: ui.memory(|m| m.focused()) == Some(focus),
    };
    egui::ScrollArea::both().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        paint.graph(ui, model, actions, &mut requests, &mut selected);
    });
    object_dnd::end_of_frame(ui.ctx(), model.graph, spec, paint.hover.take(), actions);
    if ui.memory(|m| m.focused()) == Some(focus) {
        // ↑ ↓: the neighboring visible selectable row; Enter: its properties,
        // like the double-click (#170, point 6). Ctrl+↑ / Ctrl+↓ (move a
        // series) do not go through here: `series_shortcuts` below.
        match ui.input_mut(read_navigation) {
            Some(key @ (NavKey::Up | NavKey::Down)) => {
                let step = if key == NavKey::Up { -1 } else { 1 };
                if let Some(next) = neighbour(&paint.visible.borrow(), selected, step) {
                    requests.push(ObjectManagerRequest::Select(next));
                    ui.ctx().request_repaint();
                }
            }
            Some(NavKey::Enter) => {
                if let Some(target) = selected {
                    requests.push(ObjectManagerRequest::Properties(target));
                }
            }
            None => {}
        }
        if let Some(target) = selected {
            if target.removable(spec)
                && ui.input_mut(|i| i.consume_shortcut(&super::shortcuts::KEY_DELETE))
            {
                actions.push(Action::RemovePlotObject {
                    graph: model.graph,
                    target,
                });
            }
            match target {
                T::Series(series) => {
                    series_shortcuts(ui.ctx(), model.graph, series, spec, actions, &mut requests)
                }
                T::SeriesGroup(group) => {
                    group_shortcuts(ui.ctx(), model.graph, group, spec, actions, &mut requests)
                }
                _ => {}
            }
        }
    }
    requests
}

// Painting arguments common to the sections: no extra Ui nor egui scope. The
// selection and the events stay accumulated by the bodies actually unfolded,
// in the order of their rows.
struct ObjectManagerPaint<'a> {
    tree: &'a Tree,
    spec: &'a stokesforge_plot::PlotSpec,
    graph: NodeId,
    focus: egui::Id,
    settings: ObjectManagerSettings,
    /// The dragged item, at its place in the preview.
    item: Option<DragItem>,
    /// The row the dragged item hovers, seen during painting.
    hover: Hover,
    /// The selectable rows **painted** this frame, in reading order: what ↑ and
    /// ↓ walk through (#170, point 6); a collapsed section is not in it.
    visible: std::cell::RefCell<Vec<T>>,
    /// The panel has keyboard focus: its selected row carries the focus
    /// outline.
    has_focus: bool,
}

impl ObjectManagerPaint<'_> {
    fn header(
        &self,
        ui: &mut egui::Ui,
        text: &str,
        target: T,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
    ) {
        object_visibility(
            ui,
            self.graph,
            target,
            self.spec,
            Shown::from(self.settings),
            actions,
        );
        let response = manager_header_label(ui, text, target, requests);
        response.context_menu(|ui| {
            object_node_menu(
                ui,
                self.graph,
                target,
                self.spec,
                Shown::from(self.settings),
                actions,
                requests,
            )
        });
    }

    fn graph(
        &self,
        ui: &mut egui::Ui,
        model: &ObjectTree,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
        selected: &mut Option<T>,
    ) {
        let id = ui.make_persistent_id(("object_manager_graph", self.graph));
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            self.settings.group_open(ObjectManagerGroup::Graph),
        )
        .show_header(ui, |ui| {
            self.header(ui, &model.title, T::Graph, actions, requests);
        })
        .body(|ui| {
            crate::tree_lines::tree_children(ui, |ui, branch| {
                for layer in &model.layers {
                    let y = self.layer(ui, layer, actions, requests, selected);
                    branch.mark(y);
                }
            })
        });
    }

    fn layer(
        &self,
        ui: &mut egui::Ui,
        layer: &LayerRow,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
        selected: &mut Option<T>,
    ) -> f32 {
        let id = ui.make_persistent_id((self.graph, layer.index));
        let (layer_toggle, _, _) =
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                self.settings.group_open(ObjectManagerGroup::Layers),
            )
            .show_header(ui, |ui| {
                self.header(
                    ui,
                    &tr!("object-manager-layer", index = (layer.index + 1)),
                    T::Panel(layer.index),
                    actions,
                    requests,
                );
            })
            .body(|ui| {
                crate::tree_lines::tree_children(ui, |ui, branch| {
                    let y = self.series(ui, layer, actions, requests, selected);
                    branch.mark(y);
                    if !layer.axes.is_empty() {
                        let y = self.axes(ui, layer, actions, requests, selected);
                        branch.mark(y);
                    }
                    self.object_group(
                        ui,
                        Some(layer.index),
                        &layer.objects,
                        branch,
                        actions,
                        requests,
                        selected,
                    );
                })
            });

        layer_toggle.rect.center().y
    }

    fn series(
        &self,
        ui: &mut egui::Ui,
        layer: &LayerRow,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
        selected: &mut Option<T>,
    ) -> f32 {
        let id = ui.make_persistent_id((self.graph, layer.index, "group"));
        let (group_toggle, _, _) =
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                self.settings.group_open(ObjectManagerGroup::Plots),
            )
            .show_header(ui, |ui| {
                self.header(
                    ui,
                    &tr!("object-manager-group"),
                    T::Group(layer.index),
                    actions,
                    requests,
                );
            })
            .body(|ui| {
                crate::tree_lines::tree_children(ui, |ui, branch| {
                    // A series group (#177, #197) is a parent node like the
                    // others: collapsible header, members in a branch. A single
                    // series stays one level up.
                    for slot in &layer.slots {
                        let y = match slot {
                            LayerSlot::Single(rank) => {
                                let series = &layer.series[*rank];
                                if series.selected {
                                    *selected = Some(T::Series(series.target));
                                }
                                self.series_row(ui, series, actions, requests)
                            }
                            LayerSlot::Group(group) => {
                                self.series_group(ui, layer, group, actions, requests, selected)
                            }
                        };
                        branch.mark(y);
                    }
                    // Exit of a group that ends the list (and drop at the end
                    // of the list): the zone exists only during a drag of a
                    // series or a group of this layer.
                    if matches!(
                        self.item,
                        Some(DragItem::Series { panel, .. } | DragItem::Group { panel, .. })
                            if panel == layer.index
                    ) {
                        let y = self.end_zone(ui, layer.index);
                        branch.mark(y);
                    }
                })
            });

        group_toggle.rect.center().y
    }

    /// The "end of the list" zone: a grayed row under the last series.
    fn end_zone(&self, ui: &mut egui::Ui, panel: usize) -> f32 {
        let row = ui.horizontal(|ui| ui.weak(tr!("object-drag-end"))).response;
        if ui.rect_contains_pointer(row.rect) {
            self.hover.offer(DropOver::End { panel });
        }
        row.rect.center().y
    }

    /// A series group (#197): the node, its mixed visibility checkbox, its name
    /// (editable in place) and its members in a `tree_lines` branch. The
    /// initial collapse state is that of "Series" (#139).
    fn series_group(
        &self,
        ui: &mut egui::Ui,
        layer: &LayerRow,
        group: &GroupRow,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
        selected: &mut Option<T>,
    ) -> f32 {
        let id =
            ui.make_persistent_id((self.graph, layer.index, "series-group", group.target.group));
        let (toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            self.settings.group_open(ObjectManagerGroup::Plots),
        )
        .show_header(ui, |ui| {
            if group.selected {
                *selected = Some(T::SeriesGroup(group.target));
            }
            self.group_header(ui, group, actions, requests);
        })
        .body(|ui| {
            crate::tree_lines::tree_children(ui, |ui, branch| {
                for rank in &group.members {
                    let series = &layer.series[*rank];
                    if series.selected {
                        *selected = Some(T::Series(series.target));
                    }
                    let y = self.series_row(ui, series, actions, requests);
                    branch.mark(y);
                }
            })
        });
        toggle.rect.center().y
    }

    /// A group's header: a list row (#169) under the checkbox and the name,
    /// selectable, opening the series window on double-click. Its handle moves
    /// the group as a block; a series dragged onto it joins it (lower half) or
    /// is placed before it (upper half).
    fn group_header(
        &self,
        ui: &mut egui::Ui,
        group: &GroupRow,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
    ) {
        let target = T::SeriesGroup(group.target);
        self.visible.borrow_mut().push(target);
        let background = ListRowBackground::reserve(ui);
        let row = with_list_row_height(ui, |ui| {
            ui.horizontal(|ui| {
                let renaming = group_rename_of(ui.ctx(), self.graph, group.target);
                if let Some(rename) = renaming {
                    object_visibility(
                        ui,
                        self.graph,
                        target,
                        self.spec,
                        Shown::from(self.settings),
                        actions,
                    );
                    group_name_editor(ui, self.graph, group, rename, actions);
                    return;
                }
                let outcome = reorder_row_with(
                    ui,
                    ui.id().with("row"),
                    self.item.is_some_and(|item| item.target() == target),
                    |ui| {
                        object_visibility(
                            ui,
                            self.graph,
                            target,
                            self.spec,
                            Shown::from(self.settings),
                            actions,
                        );
                        let text = list_row_text(&group.name, group.selected)
                            .color(list_row_text_color(ui, group.selected, false));
                        let response = list_row_grip(ui, text, group.selected);
                        if response.clicked() || response.secondary_clicked() {
                            requests.push(ObjectManagerRequest::Select(target));
                            ui.memory_mut(|memory| memory.request_focus(self.focus));
                        }
                        if response.double_clicked() {
                            requests.push(ObjectManagerRequest::Properties(target));
                        }
                        response.context_menu(|ui| {
                            object_node_menu(
                                ui,
                                self.graph,
                                target,
                                self.spec,
                                Shown::from(self.settings),
                                actions,
                                requests,
                            )
                        });
                        Some(response)
                    },
                );
                if outcome.grabbed {
                    object_dnd::begin(
                        ui.ctx(),
                        Some(self.graph),
                        DragItem::Group {
                            panel: group.target.panel,
                            group: group.target.group,
                        },
                    );
                    requests.push(ObjectManagerRequest::Select(target));
                    ui.memory_mut(|memory| memory.request_focus(self.focus));
                }
                if let Some(rect) = outcome.hover {
                    self.hover.offer(DropOver::Header {
                        panel: group.target.panel,
                        group: group.target.group,
                        after: object_dnd::pointer_after(ui, rect),
                    });
                }
            })
        })
        .response
        .rect;
        background.paint(ui, row, group.selected, group.selected && self.has_focus);
    }

    fn axes(
        &self,
        ui: &mut egui::Ui,
        layer: &LayerRow,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
        selected: &mut Option<T>,
    ) -> f32 {
        let id = ui.make_persistent_id((self.graph, layer.index, "axes"));
        // The default comes from the preferences (#139), without overriding a collapse state already chosen.
        let (axes_toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            self.settings.group_open(ObjectManagerGroup::Axes),
        )
        .show_header(ui, |ui| {
            self.header(
                ui,
                &tr!("object-manager-axes"),
                T::Axes(layer.index),
                actions,
                requests,
            );
        })
        .body(|ui| {
            crate::tree_lines::tree_children(ui, |ui, branch| {
                self.objects(ui, &layer.axes, branch, actions, requests, selected);
            })
        });

        axes_toggle.rect.center().y
    }

    #[allow(clippy::too_many_arguments)]
    fn object_group(
        &self,
        ui: &mut egui::Ui,
        layer: Option<usize>,
        objects: &[ObjectRow],
        branch: &mut crate::tree_lines::TreeBranch,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
        selected: &mut Option<T>,
    ) {
        if objects.is_empty() {
            return;
        }
        let id = ui.make_persistent_id((self.graph, layer, "objects"));
        let (toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            self.settings.group_open(ObjectManagerGroup::Objects),
        )
        .show_header(ui, |ui| {
            ui.label(tr!("object-manager-objects"));
        })
        .body(|ui| {
            crate::tree_lines::tree_children(ui, |ui, branch| {
                self.objects(ui, objects, branch, actions, requests, selected);
            });
        });
        branch.mark(toggle.rect.center().y);
    }

    fn objects(
        &self,
        ui: &mut egui::Ui,
        objects: &[ObjectRow],
        branch: &mut crate::tree_lines::TreeBranch,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
        selected: &mut Option<T>,
    ) {
        for object in objects {
            let y = object_manager_object_row(
                ui, self.graph, object, self.spec, self.focus, self, actions, requests,
            );
            branch.mark(y);
            if object.selected {
                *selected = Some(object.target);
            }
        }
    }

    fn series_row(
        &self,
        ui: &mut egui::Ui,
        series: &SeriesRow,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
    ) -> f32 {
        self.visible.borrow_mut().push(T::Series(series.target));
        ui.push_id(series.target, |ui| {
            // #169: a list row, background placed under the checkbox and the swatch.
            let background = ListRowBackground::reserve(ui);
            let target = series.target;
            let row = with_list_row_height(ui, |ui| {
                ui.horizontal(|ui| {
                    // The handle first (like the copy formats of the
                    // preferences): the row is dragged, the others move aside.
                    let outcome = reorder_row_with(
                        ui,
                        ui.id().with("row"),
                        self.item
                            .is_some_and(|item| item.target() == T::Series(target)),
                        |ui| {
                            object_visibility(
                                ui,
                                self.graph,
                                T::Series(target),
                                self.spec,
                                Shown::from(self.settings),
                                actions,
                            );
                            let (rect, swatch) = ui.allocate_exact_size(
                                egui::vec2(28.0, 18.0),
                                egui::Sense::click_and_drag(),
                            );
                            ui.painter().extend(stokesforge_render_egui::series_swatch(
                                self.tree,
                                &series.spec,
                                rect,
                            ));
                            if series.stale {
                                let image = crate::icons::icon_image(
                                    crate::icons::Icon::Stale,
                                    crate::icons::IconRole::Neutral,
                                )
                                .sense(egui::Sense::hover());
                                ui.add(image).on_hover_text(tr!("stale-series-hint"));
                            }
                            let text = list_row_text(&series.label, series.selected)
                                .color(list_row_text_color(ui, series.selected, false));
                            // #151: the legend swatch is part of the target,
                            // like the name. The whole row, checkbox aside, grabs
                            // and clicks: we join the gestures before handling
                            // selection and properties.
                            let response = list_row_grip(ui, text, series.selected).union(swatch);
                            if response.clicked() || response.secondary_clicked() {
                                requests.push(ObjectManagerRequest::Select(T::Series(target)));
                                ui.memory_mut(|memory| memory.request_focus(self.focus));
                            }
                            if response.double_clicked() {
                                requests.push(ObjectManagerRequest::Properties(T::Series(target)));
                            }
                            response.context_menu(|ui| {
                                object_node_menu(
                                    ui,
                                    self.graph,
                                    T::Series(target),
                                    self.spec,
                                    Shown::from(self.settings),
                                    actions,
                                    requests,
                                )
                            });
                            Some(response)
                        },
                    );
                    if outcome.grabbed {
                        object_dnd::begin(
                            ui.ctx(),
                            Some(self.graph),
                            DragItem::Series {
                                panel: target.panel,
                                origin: target.index,
                                current: target.index,
                            },
                        );
                        requests.push(ObjectManagerRequest::Select(T::Series(target)));
                        ui.memory_mut(|memory| memory.request_focus(self.focus));
                    }
                    if let Some(rect) = outcome.hover {
                        self.hover.offer(DropOver::Series {
                            panel: target.panel,
                            index: target.index,
                            after: object_dnd::pointer_after(ui, rect),
                        });
                    }
                })
            })
            .response
            .rect;
            background.paint(ui, row, series.selected, series.selected && self.has_focus);
            row.center().y
        })
        .inner
    }
}

pub(super) fn series_shortcuts(
    ctx: &egui::Context,
    graph: NodeId,
    series: stokesforge_plot::SeriesRef,
    spec: &stokesforge_plot::PlotSpec,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    if ctx.input_mut(|i| i.consume_shortcut(&super::shortcuts::SHORTCUT_RENAME_OBJECT)) {
        requests.push(ObjectManagerRequest::Rename(series));
    }
    for (shortcut, up) in [
        (&super::shortcuts::SHORTCUT_MOVE_SERIES_UP, true),
        (&super::shortcuts::SHORTCUT_MOVE_SERIES_DOWN, false),
    ] {
        if ctx.input_mut(|i| i.consume_shortcut(shortcut))
            && spec
                .panels
                .get(series.panel)
                .is_some_and(|p| p.can_move_series(series.index, up))
        {
            actions.push(Action::MoveSeries { graph, series, up });
        }
    }
}

/// Shortcuts of a selected series group (#197): F2 renames,
/// Ctrl+↑/↓ moves it as a block. Delete is the one of all contents.
pub(super) fn group_shortcuts(
    ctx: &egui::Context,
    graph: NodeId,
    group: stokesforge_plot::SeriesGroupRef,
    spec: &stokesforge_plot::PlotSpec,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    if ctx.input_mut(|i| i.consume_shortcut(&super::shortcuts::SHORTCUT_RENAME_OBJECT)) {
        requests.push(ObjectManagerRequest::RenameGroup(group));
    }
    for (shortcut, up) in [
        (&super::shortcuts::SHORTCUT_MOVE_SERIES_UP, true),
        (&super::shortcuts::SHORTCUT_MOVE_SERIES_DOWN, false),
    ] {
        if ctx.input_mut(|i| i.consume_shortcut(shortcut))
            && spec
                .panels
                .get(group.panel)
                .is_some_and(|p| p.can_move_group(group.group, up))
        {
            actions.push(Action::MoveSeriesGroup { graph, group, up });
        }
    }
}

/// The group rename in progress (#197), held in egui's memory under the
/// graph's identity: the fixed panel and its detached copy share the same
/// editor, which exists only in the tree.
#[derive(Clone)]
struct GroupRename {
    target: stokesforge_plot::SeriesGroupRef,
    original: String,
    buffer: String,
}

fn group_rename_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("object_manager_group_rename", graph))
}

fn group_rename_of(
    ctx: &egui::Context,
    graph: NodeId,
    target: stokesforge_plot::SeriesGroupRef,
) -> Option<GroupRename> {
    ctx.data(|data| data.get_temp::<GroupRename>(group_rename_id(graph)))
        .filter(|rename| rename.target == target)
}

/// For the tests of the other surfaces (graph shortcuts): is a rename of the
/// group `target` pending?
#[cfg(test)]
pub(super) fn group_rename_pending(
    ctx: &egui::Context,
    graph: NodeId,
    target: stokesforge_plot::SeriesGroupRef,
) -> bool {
    group_rename_of(ctx, graph, target).is_some()
}

/// Opens the editor of the name of the group `group` in the tree.
pub(super) fn open_group_name_editor(
    ctx: &egui::Context,
    spec: &stokesforge_plot::PlotSpec,
    graph: NodeId,
    group: stokesforge_plot::SeriesGroupRef,
) {
    let Some(panel) = spec.panels.get(group.panel) else {
        return;
    };
    if panel.group_members(group.group).is_empty() {
        return;
    }
    let name = stokesforge_app_state::object_manager::group_title(panel, group.group);
    ctx.data_mut(|data| {
        data.insert_temp(
            group_rename_id(graph),
            GroupRename {
                target: group,
                original: name.clone(),
                buffer: name,
            },
        );
    });
}

/// The name field in place of the label, under the common contract of
/// [`inplace_edit`](super::inplace_edit): Enter, Tab or a click elsewhere
/// commit (a single action, and none if the name did not change), Escape cancels.
fn group_name_editor(
    ui: &mut egui::Ui,
    graph: NodeId,
    group: &GroupRow,
    mut rename: GroupRename,
    actions: &mut Vec<Action>,
) {
    let id = group_rename_id(graph);
    let shown = inplace_edit::show(
        ui,
        id.with("field"),
        egui::TextEdit::singleline(&mut rename.buffer).desired_width(160.0),
        inplace_edit::Opening::SelectAll,
        inplace_edit::FocusLoss::Ends,
        None,
    );
    match shown
        .end
        .map(|end| inplace_edit::resolve(end, rename.buffer == rename.original, false))
    {
        Some(inplace_edit::Resolution::Close { write, .. }) => {
            if write {
                actions.push(Action::RenameSeriesGroup {
                    graph,
                    group: group.target,
                    name: rename.buffer.clone(),
                });
            }
            ui.data_mut(|data| data.remove::<GroupRename>(id));
        }
        _ => ui.data_mut(|data| {
            data.insert_temp(id, rename);
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn object_manager_object_row(
    ui: &mut egui::Ui,
    graph: NodeId,
    object: &stokesforge_app_state::object_manager::ObjectRow,
    spec: &stokesforge_plot::PlotSpec,
    focus: egui::Id,
    dnd: &ObjectManagerPaint<'_>,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) -> f32 {
    dnd.visible.borrow_mut().push(object.target);
    ui.push_id(object.target, |ui| {
        // #169: a list row, background placed under the checkbox and the icon.
        let background = ListRowBackground::reserve(ui);
        // A free text or a shape is reordered **within its category**; the
        // axes do not move.
        let draggable = match object.target {
            T::Annotation(r) => Some((
                DragItem::Text {
                    panel: r.panel,
                    origin: r.index,
                    current: r.index,
                },
                DropOver::Text {
                    panel: r.panel,
                    index: r.index,
                },
            )),
            T::Shape(r) => Some((
                DragItem::Shape {
                    panel: r.panel,
                    origin: r.index,
                    current: r.index,
                },
                DropOver::Shape {
                    panel: r.panel,
                    index: r.index,
                },
            )),
            _ => None,
        };
        let row = with_list_row_height(ui, |ui| {
            ui.horizontal(|ui| {
                let mut body = |ui: &mut egui::Ui| {
                    object_visibility(ui, graph, object.target, spec, Shown::BASE, actions);
                    let icon = match object.kind {
                        stokesforge_app_state::object_manager::ObjectKind::Text => Icon::TextTool,
                        stokesforge_app_state::object_manager::ObjectKind::Axis => Icon::Axis,
                        stokesforge_app_state::object_manager::ObjectKind::Shape(kind) => {
                            match kind {
                                stokesforge_plot::ShapeKind::Rectangle => Icon::ShapeRectangle,
                                stokesforge_plot::ShapeKind::Ellipse => Icon::ShapeEllipse,
                                stokesforge_plot::ShapeKind::Line => Icon::ShapeLine,
                                stokesforge_plot::ShapeKind::Arrow => Icon::ShapeArrow,
                            }
                        }
                    };
                    let icon = ui
                        .add(
                            icon_image(icon, IconRole::Neutral)
                                .sense(egui::Sense::click_and_drag()),
                        )
                        .on_hover_text(&object.label);
                    let text = list_row_text(&object.label, object.selected)
                        .color(list_row_text_color(ui, object.selected, false));
                    let response = list_row_grip(ui, text, object.selected).union(icon);
                    if response.clicked() || response.secondary_clicked() {
                        requests.push(ObjectManagerRequest::Select(object.target));
                        ui.memory_mut(|m| m.request_focus(focus));
                    }
                    if response.double_clicked() {
                        requests.push(ObjectManagerRequest::Properties(object.target));
                    }
                    response.context_menu(|ui| {
                        object_node_menu(
                            ui,
                            graph,
                            object.target,
                            spec,
                            Shown::BASE,
                            actions,
                            requests,
                        )
                    });
                    response
                };
                let Some((item, over)) = draggable else {
                    body(ui);
                    return;
                };
                let outcome = reorder_row_with(
                    ui,
                    ui.id().with("row"),
                    dnd.item
                        .is_some_and(|dragged| dragged.target() == object.target),
                    |ui| Some(body(ui)),
                );
                if outcome.grabbed {
                    object_dnd::begin(ui.ctx(), Some(graph), item);
                    requests.push(ObjectManagerRequest::Select(object.target));
                    ui.memory_mut(|m| m.request_focus(focus));
                }
                if outcome.hover.is_some() {
                    dnd.hover.offer(over);
                }
            })
        })
        .response
        .rect;
        background.paint(ui, row, object.selected, object.selected && dnd.has_focus);
        row.center().y
    })
    .inner
}

impl StokesForgeApp {
    pub(super) fn object_manager_panel(
        &mut self,
        ui: &mut egui::Ui,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        let ctx = ui.ctx().clone();
        self.drain_object_manager(&ctx, actions);
        if !self.state.object_manager_open() {
            return;
        }
        let graph = active_graph(active_tab_kind(active_tab));
        let tree = self.state.snapshot().tree.clone();
        let settings = self.state.object_manager_settings();
        let selection = graph.and_then(|graph| {
            self.read_graph_view(graph, selected_manager_target)
                .flatten()
        });
        let spec =
            graph.and_then(|graph| stokesforge_core::compose::current_plot_spec(&tree, graph));
        let model = graph.and_then(|graph| {
            stokesforge_app_state::object_manager::object_tree_with_selection(
                &tree,
                graph,
                selection,
                settings.show_scales,
                settings.show_objects,
            )
        });
        if self.om_detached {
            self.show_detached_object_manager(
                &ctx,
                ObjectManagerModel {
                    stamp: super::window_detach::snapshot_stamp(self.state.snapshot()),
                    tree,
                    graph,
                    selection,
                    model,
                    spec,
                    settings,
                },
            );
            return;
        }
        let mut requests = Vec::new();
        let collapsed = self.om_collapsed;
        let panel = if collapsed {
            egui::Panel::right("stokesforge_object_manager_collapsed")
                .exact_size(PANEL_STRIP_THICKNESS)
                .resizable(false)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(0))
                .show_separator_line(false)
        } else {
            egui::Panel::right("stokesforge_object_manager")
                .default_size(300.0)
                .size_range(180.0..=500.0)
        };
        panel.show(ui, |ui| {
            let header = panel_header(
                ui,
                &tr!("action-object-manager"),
                PanelPlacement::Right,
                collapsed,
            );
            self.om_collapsed ^= header.toggle_collapse;
            if header.detach {
                self.om_detached = true;
            }
            if !collapsed {
                requests = object_manager_contents(
                    ui,
                    &tree,
                    model.as_ref(),
                    spec.as_ref(),
                    settings,
                    actions,
                );
            }
        });
        if let Some(graph) = graph {
            for request in requests {
                self.run_object_manager_request(&ctx, graph, request, actions);
            }
        }
    }

    /// The tree and the Graph menu share the same view gestures and Action.
    pub(super) fn run_object_manager_request(
        &mut self,
        ctx: &egui::Context,
        graph: NodeId,
        request: ObjectManagerRequest,
        actions: &mut Vec<Action>,
    ) {
        let tree = self.state.tree().clone();
        self.with_graph_view(graph, |view| {
            apply_object_request(ctx, view, &tree, graph, request, actions)
        });
        ctx.request_repaint();
    }
}

/// Runs an object request on the graph view: the same for the panel,
/// Graph › Selection and the right click in the figure (#152).
pub(super) fn apply_object_request(
    ctx: &egui::Context,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    request: ObjectManagerRequest,
    actions: &mut Vec<Action>,
) {
    let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph) else {
        return;
    };
    match request {
        ObjectManagerRequest::Select(target) => select_manager_target(view, Some(target), &spec),
        ObjectManagerRequest::Analysis(target, category) => {
            actions.push(Action::OpenGraphAnalysis {
                preset: super::analysis_menu::category_preset(ctx, category),
                graph,
                series: analysis_sources(&spec, target),
                spec: Box::new(view.framed_spec(&spec)),
            });
        }
        ObjectManagerRequest::Rename(series) => {
            open_series_name_editor(ctx, view, tree, graph, series)
        }
        ObjectManagerRequest::RenameGroup(group) => {
            open_group_name_editor(ctx, &spec, graph, group)
        }
        ObjectManagerRequest::EditText(ContextText::Title(target)) => {
            open_text_editor(ctx, view, tree, graph, target)
        }
        ObjectManagerRequest::EditText(ContextText::LegendEntry { panel, series }) => {
            open_legend_entry_editor(ctx, view, tree, graph, panel, series)
        }
        ObjectManagerRequest::Properties(target) => {
            select_manager_target(view, Some(target), &spec);
            match target {
                T::Annotation(t) => open_annotation_editor(ctx, view, tree, graph, t),
                T::Shape(t) => open_shape_properties(ctx, view, &spec, t),
                _ => actions.push(Action::OpenObjectProperties { graph, target }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::frame_of;
    use super::*;
    use egui_kittest::kittest::Queryable;
    use stokesforge_core::Project;
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesRef, SeriesSpec, ShapeKind,
        ShapeRef, ShapeSpec, TextTarget,
    };

    use stokesforge_app_state::object_manager::VisibilityTarget as T;

    #[test]
    fn group_analysis_keeps_its_members_and_the_view_spec() {
        let (project, graph, mut spec, _) = three_series();
        spec.panels.get_mut(0).unwrap().series[0].group = Some(7);
        spec.panels.get_mut(0).unwrap().series[2].group = Some(7);
        let target = T::SeriesGroup(stokesforge_plot::SeriesGroupRef::new(0, 7));
        let sources = analysis_sources(&spec, target);
        assert_eq!(sources, vec![SeriesRef::new(0, 0), SeriesRef::new(0, 2)]);
        let ctx = egui::Context::default();
        let mut view = GraphView::default();
        let mut actions = Vec::new();
        apply_object_request(
            &ctx,
            &mut view,
            project.tree(),
            graph,
            ObjectManagerRequest::Analysis(
                T::Series(SeriesRef::new(0, 0)),
                stokesforge_app_state::analysis::Category::DataManipulation,
            ),
            &mut actions,
        );
        assert!(
            matches!(actions.as_slice(), [Action::OpenGraphAnalysis { graph: g, series, spec: s, .. }]
            if *g == graph && series == &[SeriesRef::new(0, 0)] && **s == view.framed_spec(&stokesforge_core::compose::current_plot_spec(project.tree(), graph).unwrap()))
        );
        spec.panels.get_mut(0).unwrap().series[2].row = Some(0);
        assert!(
            analysis_sources(&spec, target).is_empty(),
            "a group mixing columns and rows is not analyzed in one go"
        );
        spec.panels.get_mut(0).unwrap().series[0].row = Some(1);
        assert_eq!(
            analysis_sources(&spec, target),
            vec![SeriesRef::new(0, 0), SeriesRef::new(0, 2)],
            "a group of rows is analyzed through its transposed sheet (#253)"
        );
        assert_eq!(
            analysis_sources(&spec, T::Series(SeriesRef::new(0, 0))),
            vec![SeriesRef::new(0, 0)],
            "a row curve is analyzable"
        );
    }

    /// A graph of three series ("First", "Second", "Last"), the first selected,
    /// and its Object Manager tree.
    fn three_series() -> (
        Project,
        NodeId,
        PlotSpec,
        stokesforge_app_state::object_manager::ObjectTree,
    ) {
        let mut project = Project::new("Objects").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        for name in ["First", "Second", "Last"] {
            let mut series = SeriesSpec::new(NodeId::from_raw(900));
            series.name = name.into();
            panel.series.push(series);
        }
        let spec = PlotSpec::default().with_panels(panel);
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let model = stokesforge_app_state::object_manager::object_tree(
            project.tree(),
            graph,
            Some(SeriesRef::new(0, 0)),
        )
        .unwrap();
        (project, graph, spec, model)
    }

    /// A frame of the Object Manager on a 600 × 500 screen: the output, the
    /// actions and the requests of the frame.
    fn image(
        ctx: &egui::Context,
        tree: &Tree,
        model: Option<&stokesforge_app_state::object_manager::ObjectTree>,
        spec: Option<&PlotSpec>,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<Action>, Vec<ObjectManagerRequest>) {
        let mut actions = Vec::new();
        let mut requests = Vec::new();
        let output = frame_of(ctx, egui::vec2(600.0, 500.0), events, |ui| {
            requests = object_manager_contents(
                ui,
                tree,
                model,
                spec,
                ObjectManagerSettings::default(),
                &mut actions,
            );
        });
        (output, actions, requests)
    }

    fn text_center(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(shape) if shape.galley.text() == text => {
                    Some(shape.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("text missing: {text}"))
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Drives a real click on a painted surface, without calling the menu
    /// directly: root popups and submenus take part in the gesture.
    fn click_menu(
        paint: &impl Fn(Vec<egui::Event>) -> (egui::FullOutput, Vec<Action>, Vec<ObjectManagerRequest>),
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) -> (egui::FullOutput, Vec<Action>, Vec<ObjectManagerRequest>) {
        paint(vec![egui::Event::PointerMoved(pos)]);
        paint(vec![egui::Event::PointerButton {
            pos,
            button,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        paint(vec![egui::Event::PointerButton {
            pos,
            button,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }])
    }

    #[test]
    fn right_click_on_the_display_background_toggles_the_filters_and_keeps_menus_open() {
        use super::super::test_support::painted;
        let (project, _, spec, model) = three_series();
        let ctx = super::super::test_support::context();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        paint(Vec::new());
        click_menu(
            &paint,
            egui::pos2(300.0, 300.0),
            egui::PointerButton::Secondary,
        );
        let output = paint(Vec::new()).0;
        assert!(
            painted(&output, &tr!("object-manager-show-axes")),
            "the settings are at the first level, without a wrapper"
        );
        assert!(painted(&output, &tr!("object-manager-reset")));
        {
            let key = tr!("object-manager-show-axes");
            let expected = Action::SetObjectManagerShowScales(true);
            let output = paint(Vec::new()).0;
            let (_, actions, requests) = click_menu(
                &paint,
                text_center(&output, &key),
                egui::PointerButton::Primary,
            );
            assert_eq!(actions, vec![expected]);
            assert!(requests.is_empty());
            assert!(egui::Popup::is_any_open(&ctx));
            let output = paint(Vec::new()).0;
            assert!(
                painted(&output, &tr!("object-manager-reset")),
                "the root menu stays open"
            );
            assert!(painted(&output, &tr!("object-manager-show-axes")));
            assert!(painted(&output, &tr!("object-manager-show-axes")));
        }
    }

    /// The background settings stay available without a graph; the five
    /// toggles keep their chain open, while Reset closes it.
    #[test]
    fn right_click_without_a_graph_sets_the_five_defaults_then_resets() {
        use super::super::test_support::painted;
        let project = Project::new("Settings").unwrap();
        let settings = ObjectManagerSettings::default();
        let ctx = super::super::test_support::context();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), None, None, events);
        paint(Vec::new());
        paint(Vec::new());
        click_menu(
            &paint,
            egui::pos2(200.0, 150.0),
            egui::PointerButton::Secondary,
        );
        let output = paint(Vec::new()).0;
        click_menu(
            &paint,
            text_center(&output, &tr!("object-manager-default-expansion")),
            egui::PointerButton::Primary,
        );
        paint(Vec::new());
        for (group, label) in [
            (ObjectManagerGroup::Graph, tr!("window-kind-graph")),
            (
                ObjectManagerGroup::Layers,
                tr!("object-manager-expand-layers"),
            ),
            (
                ObjectManagerGroup::Plots,
                tr!("object-manager-expand-plots"),
            ),
            (ObjectManagerGroup::Axes, tr!("object-manager-axes")),
            (ObjectManagerGroup::Objects, tr!("object-manager-objects")),
        ] {
            let output = paint(Vec::new()).0;
            let (_, actions, requests) = click_menu(
                &paint,
                text_center(&output, &label),
                egui::PointerButton::Primary,
            );
            assert_eq!(
                actions,
                vec![Action::SetObjectManagerGroupOpen {
                    group,
                    open: !settings.group_open(group)
                }]
            );
            assert!(requests.is_empty());
            let output = paint(Vec::new()).0;
            assert!(egui::Popup::is_any_open(&ctx));
            assert!(painted(&output, &tr!("object-manager-show-axes")));
            assert!(painted(&output, &tr!("object-manager-default-expansion")));
            assert!(painted(&output, &label));
        }
        let output = paint(Vec::new()).0;
        let (_, actions, requests) = click_menu(
            &paint,
            text_center(&output, &tr!("object-manager-reset")),
            egui::PointerButton::Primary,
        );
        assert_eq!(actions, vec![Action::ResetObjectManagerSettings]);
        assert!(requests.is_empty());
        paint(Vec::new());
        assert!(!egui::Popup::is_any_open(&ctx));
        let output = paint(Vec::new()).0;
        assert!(!painted(&output, &tr!("object-manager-reset")));
    }

    #[test]
    fn right_click_on_a_series_grays_the_extremes_and_closes_after_a_move() {
        use super::super::test_support::{painted, text_exact, texts};
        for (row, source, disabled, enabled, up) in [
            (
                "First",
                SeriesRef::new(0, 0),
                tr!("action-move-series-up"),
                tr!("action-move-series-down"),
                false,
            ),
            (
                "Last",
                SeriesRef::new(0, 2),
                tr!("action-move-series-down"),
                tr!("action-move-series-up"),
                true,
            ),
        ] {
            let (project, graph, spec, model) = three_series();
            let ctx = super::super::test_support::context();
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
            paint(Vec::new());
            let output = paint(Vec::new()).0;
            let (_, actions, requests) = click_menu(
                &paint,
                text_center(&output, row),
                egui::PointerButton::Secondary,
            );
            assert!(actions.is_empty());
            assert!(requests.iter().any(|request| matches!(request,
                ObjectManagerRequest::Select(T::Series(series)) if *series == source)));
            let output = paint(Vec::new()).0;
            assert!(
                painted(&output, &enabled) && painted(&output, &disabled),
                "the moves are flat, without an Order submenu"
            );
            let labels = texts(&output.shapes);
            assert_ne!(
                text_exact(&labels, &disabled).color,
                text_exact(&labels, &enabled).color,
                "the extreme is grayed out"
            );
            let (_, actions, requests) = click_menu(
                &paint,
                text_center(&output, &disabled),
                egui::PointerButton::Primary,
            );
            assert!(
                actions.is_empty() && requests.is_empty(),
                "the forbidden move produces nothing"
            );
            // A click on a grayed-out item can close egui's popup; the useful
            // gesture is tested after reopening the same series.
            if !egui::Popup::is_any_open(&ctx) {
                let output = paint(Vec::new()).0;
                click_menu(
                    &paint,
                    text_center(&output, row),
                    egui::PointerButton::Secondary,
                );
            }
            let output = paint(Vec::new()).0;
            let (_, actions, requests) = click_menu(
                &paint,
                text_center(&output, &enabled),
                egui::PointerButton::Primary,
            );
            assert_eq!(
                actions,
                vec![Action::MoveSeries {
                    graph,
                    series: source,
                    up
                }]
            );
            assert!(requests.is_empty());
            paint(Vec::new());
            assert!(
                !egui::Popup::is_any_open(&ctx),
                "the whole menu chain is closed"
            );
            let output = paint(Vec::new()).0;
            assert!(!painted(&output, &enabled));
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
    /// Groups have independent identities; collapsing the series leaves the
    /// layer's axes and objects in place, and the other way round.
    #[test]
    fn collapsing_a_group_preserves_the_other_tree_sections() {
        let mut project = Project::new("Objects").unwrap();
        let mut spec = PlotSpec::empty("Global title");
        let panel = spec.panels.get_mut(0).unwrap();
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = "Curve".into();
        panel.series.push(series);
        panel
            .annotations
            .push(AnnotationSpec::new("Free note", 0.0, 0.0));
        let graph = project
            .add_graph(project.root(), None, spec.clone())
            .unwrap();
        let model = stokesforge_app_state::object_manager::object_tree_with_selection(
            project.tree(),
            graph,
            None,
            true,
            true,
        )
        .unwrap();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        let labels = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Text(text) if !text.galley.text().is_empty() => {
                        Some(text.galley.text().to_string())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let toggle = |output: &egui::FullOutput, label: &str| {
            let center = text_center(output, label);
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Path(path) if path.closed && path.points.len() == 3 => {
                        let rect = egui::Rect::from_points(&path.points);
                        ((rect.center().y - center.y).abs() < 4.0 && rect.right() < center.x)
                            .then_some(rect.center())
                    }
                    _ => None,
                })
                .expect("collapse arrow painted on the header")
        };
        paint(Vec::new());
        let (output, actions, requests) = paint(Vec::new());
        assert!(actions.is_empty() && requests.is_empty());
        let mut expected = vec![model.title.clone()];
        expected.extend([
            tr!("object-manager-layer", index = 1),
            tr!("object-manager-group"),
            "Curve".into(),
            tr!("object-manager-axes"),
        ]);
        for axis in &model.layers[0].axes {
            assert!(
                !labels(&output).contains(&axis.label),
                "the axes are collapsed by default"
            );
        }
        assert!(!model.layers[0].objects.is_empty());
        expected.push(tr!("object-manager-objects"));
        assert_eq!(
            labels(&output),
            expected,
            "the sections follow the graph order, \"Objects\" collapsed by default"
        );

        let click_toggle = |output: &egui::FullOutput, label: &str| {
            let pos = toggle(output, label);
            for events in [
                vec![egui::Event::PointerMoved(pos)],
                vec![pointer(pos, true)],
                vec![pointer(pos, false)],
            ] {
                let (_, actions, requests) = paint(events);
                assert!(
                    actions.is_empty() && requests.is_empty(),
                    "collapsing is not a model mutation"
                );
            }
            paint(Vec::new()).0
        };
        let collapsed = click_toggle(&output, &tr!("object-manager-group"));
        let texts = labels(&collapsed);
        assert!(!texts.iter().any(|text| text == "Curve"));
        assert!(texts.contains(&tr!("object-manager-objects")));
        for axis in &model.layers[0].axes {
            assert!(
                !texts.contains(&axis.label),
                "collapsing the plots group does not expand the axes"
            );
        }
        let expanded = click_toggle(&collapsed, &tr!("object-manager-group"));
        assert!(labels(&expanded).iter().any(|text| text == "Curve"));
        let with_axes = click_toggle(&expanded, &tr!("object-manager-axes"));
        let texts = labels(&with_axes);
        assert!(texts.iter().any(|text| text == "Curve"));
        assert!(texts.contains(&tr!("object-manager-objects")));
        for axis in &model.layers[0].axes {
            assert!(
                texts.contains(&axis.label),
                "witness: a click on \"Axes\" expands the scales"
            );
        }
        let collapsed = click_toggle(&with_axes, &tr!("object-manager-axes"));
        let texts = labels(&collapsed);
        assert!(texts.iter().any(|text| text == "Curve"));
        for axis in &model.layers[0].axes {
            assert!(
                !texts.contains(&axis.label),
                "a second click collapses the axes, and only them"
            );
        }
    }

    /// Each family has a painted witness; changing the default must change a
    /// new context, without replacing the states of the groups already seen.
    #[test]
    fn the_initial_settings_change_each_new_group_and_preserve_the_groups_already_seen() {
        use crate::app::test_support::painted;
        let mut project = Project::new("Settings").unwrap();
        let mut spec = PlotSpec::empty("Title");
        spec.title = "Global text".into();
        let panel = spec.panels.get_mut(0).unwrap();
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = "Witness curve".into();
        panel.series.push(series);
        panel
            .annotations
            .push(AnnotationSpec::new("Local text", 0.0, 0.0));
        let graph = project
            .add_graph(project.root(), None, spec.clone())
            .unwrap();
        let settings = ObjectManagerSettings {
            show_scales: true,
            ..Default::default()
        };
        let model = stokesforge_app_state::object_manager::object_tree_with_selection(
            project.tree(),
            graph,
            None,
            true,
            true,
        )
        .unwrap();
        let paint = |ctx: &egui::Context, settings| {
            let mut actions = Vec::new();
            let output = frame_of(ctx, egui::vec2(800.0, 700.0), Vec::new(), |ui| {
                let requests = object_manager_contents(
                    ui,
                    project.tree(),
                    Some(&model),
                    Some(&spec),
                    settings,
                    &mut actions,
                );
                assert!(requests.is_empty());
            });
            assert!(
                actions.is_empty(),
                "a view default does not mutate the model"
            );
            output
        };
        for (group, witness) in [
            (
                ObjectManagerGroup::Graph,
                tr!("object-manager-layer", index = 1),
            ),
            (ObjectManagerGroup::Layers, tr!("object-manager-group")),
            (ObjectManagerGroup::Plots, "Witness curve".into()),
            (
                ObjectManagerGroup::Axes,
                model.layers[0].axes[0].label.clone(),
            ),
            (ObjectManagerGroup::Objects, "Local text".into()),
        ] {
            let ctx = crate::app::test_support::context();
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            paint(&ctx, settings);
            let baseline = paint(&ctx, settings);
            if group == ObjectManagerGroup::Objects {
                assert!(
                    !painted(&baseline, "Global text"),
                    "the graph title is never listed"
                );
            }
            let expected = settings.group_open(group);
            assert_eq!(
                painted(&baseline, &witness),
                expected,
                "default witness {group:?}"
            );
            let mut changed = settings;
            changed.set_group_open(group, !expected);
            paint(&ctx, changed);
            assert_eq!(
                painted(&paint(&ctx, changed), &witness),
                expected,
                "the group already seen keeps its state {group:?}"
            );
            let fresh = crate::app::test_support::context();
            fresh.all_styles_mut(|style| style.animation_time = 0.0);
            paint(&fresh, changed);
            let output = paint(&fresh, changed);
            assert_eq!(
                painted(&output, &witness),
                !expected,
                "the new group reads the setting {group:?}"
            );
            if group == ObjectManagerGroup::Objects {
                assert!(
                    !painted(&output, "Global text"),
                    "the global group follows the same setting but owns its own ID"
                );
            }
            // Reset gives back the defaults without changing the groups seen.
            paint(&fresh, ObjectManagerSettings::default());
            assert_eq!(
                painted(&paint(&fresh, ObjectManagerSettings::default()), &witness),
                !expected
            );
        }
    }

    /// F2 renames and Ctrl+↓ moves down the selected series (keyboard, no drag).
    #[test]
    fn f2_and_ctrl_arrow_act_on_the_selected_series() {
        let (project, _graph, spec, model) = three_series();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        // A click on the row gives the panel keyboard focus.
        let on = text_center(&output, "First");
        image(vec![egui::Event::PointerMoved(on)]);
        image(vec![pointer(on, true)]);
        image(vec![pointer(on, false)]);
        let (_, actions, requests) = image(vec![key(egui::Key::F2, egui::Modifiers::NONE)]);
        assert!(actions.is_empty());
        assert!(requests.iter().any(|r| matches!(
            r,
            ObjectManagerRequest::Rename(SeriesRef { panel: 0, index: 0 })
        )));
        let (_, actions, _) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::CTRL)]);
        assert!(matches!(
            actions.as_slice(),
            [Action::MoveSeries {
                series: SeriesRef { panel: 0, index: 0 },
                up: false,
                ..
            }]
        ));
    }

    /// A live Object Manager: the spec and its tree do not change (the model is
    /// written only on release), the actions accumulate.
    struct Bench {
        project: Project,
        model: stokesforge_app_state::object_manager::ObjectTree,
        spec: PlotSpec,
        settings: ObjectManagerSettings,
        actions: Vec<Action>,
    }

    type Fixture = (
        Project,
        NodeId,
        PlotSpec,
        stokesforge_app_state::object_manager::ObjectTree,
    );

    fn bench(fixture: Fixture) -> egui_kittest::Harness<'static, Bench> {
        bench_with(fixture, ObjectManagerSettings::default())
    }

    fn bench_with(
        (project, _graph, spec, model): Fixture,
        settings: ObjectManagerSettings,
    ) -> egui_kittest::Harness<'static, Bench> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(420.0, 420.0))
            .build_ui_state(
                |ui, bench: &mut Bench| {
                    object_manager_contents(
                        ui,
                        bench.project.tree(),
                        Some(&bench.model),
                        Some(&bench.spec),
                        bench.settings,
                        &mut bench.actions,
                    );
                },
                Bench {
                    project,
                    model,
                    spec,
                    settings,
                    actions: Vec::new(),
                },
            );
        harness
            .ctx
            .set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&harness.ctx);
        harness.run();
        harness
    }

    /// A row's grab area (its label), by name.
    fn handle(harness: &egui_kittest::Harness<'static, Bench>, name: &str) -> egui::Rect {
        harness.get_by_label(name).rect()
    }

    /// Grabs the handle of `name`, takes it to the point `target_of` (over
    /// several frames, like a hand) and releases. The emitted actions are returned.
    fn drag(
        harness: &mut egui_kittest::Harness<'static, Bench>,
        name: &str,
        target_of: impl Fn(&egui_kittest::Harness<'static, Bench>) -> egui::Pos2,
    ) -> Vec<Action> {
        let from = handle(harness, name).center();
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from + egui::vec2(14.0, 0.0));
        harness.step();
        harness.hover_at(from + egui::vec2(20.0, 0.0));
        harness.step();
        // The target is read once: the list reorders under the pointer.
        let to = target_of(harness);
        for _ in 0..3 {
            harness.hover_at(to);
            harness.step();
        }
        assert!(
            harness.state().actions.is_empty(),
            "the preview does not write the model"
        );
        harness.drop_at(to);
        harness.run();
        std::mem::take(&mut harness.state_mut().actions)
    }

    /// The top of the row of `name` (upper half).
    fn top_of(name: &'static str) -> impl Fn(&egui_kittest::Harness<'static, Bench>) -> egui::Pos2 {
        move |harness| {
            let rect = handle(harness, name);
            egui::pos2(rect.center().x, rect.top() + 1.0)
        }
    }

    /// Drag "First" onto "Last": a single action, on release, with the final
    /// rank.
    #[test]
    fn dragging_a_single_series_emits_one_placement_on_release() {
        let mut harness = bench(three_series());
        let actions = drag(&mut harness, "First", |h| handle(h, "Last").center());
        assert!(
            matches!(
                actions.as_slice(),
                [Action::PlaceSeries {
                    source: SeriesRef { panel: 0, index: 0 },
                    group: None,
                    position: 2,
                    ..
                }]
            ),
            "{actions:?}"
        );
        harness.run();
        assert!(
            harness.state().actions.is_empty(),
            "the drop does not repeat on the next frame"
        );
    }

    /// Dropping a series where it was asks for nothing.
    #[test]
    fn releasing_in_place_asks_for_nothing() {
        let mut harness = bench(three_series());
        let actions = drag(&mut harness, "Second", |h| handle(h, "Second").center());
        assert!(actions.is_empty());
    }

    /// A single series dragged onto a group member enters it, at the rank of
    /// the hovered row.
    #[test]
    fn dragging_a_single_series_onto_a_member_makes_it_enter_the_group() {
        let mut harness = bench(named_group());
        let actions = drag(&mut harness, "First", top_of("Second"));
        assert!(
            matches!(
                actions.as_slice(),
                [Action::PlaceSeries {
                    source: SeriesRef { panel: 0, index: 0 },
                    group: Some(1),
                    ..
                }]
            ),
            "{actions:?}"
        );
    }

    /// A member dragged onto a single series leaves its group.
    #[test]
    fn dragging_a_member_onto_a_single_series_makes_it_leave_the_group() {
        let mut harness = bench(named_group());
        let actions = drag(&mut harness, "Last", top_of("First"));
        assert!(
            matches!(
                actions.as_slice(),
                [Action::PlaceSeries {
                    source: SeriesRef { panel: 0, index: 2 },
                    group: None,
                    position: 0,
                    ..
                }]
            ),
            "{actions:?}"
        );
    }

    /// The "end of the list" zone exists only during a drag, and takes a member
    /// out of a group that ends the list.
    #[test]
    fn the_end_zone_takes_a_member_out_of_a_group_that_ends_the_list() {
        let mut harness = bench(named_group());
        assert!(
            harness.query_by_label(&tr!("object-drag-end")).is_none(),
            "no zone outside a drag"
        );
        let actions = drag(&mut harness, "Last", |h| {
            h.get_by_label(&tr!("object-drag-end")).rect().center()
        });
        assert!(
            matches!(
                actions.as_slice(),
                [Action::PlaceSeries {
                    source: SeriesRef { panel: 0, index: 2 },
                    group: None,
                    position: 2,
                    ..
                }]
            ),
            "{actions:?}"
        );
    }

    /// A group is dragged as a block by the handle of its header.
    #[test]
    fn dragging_a_group_header_moves_it_as_a_block() {
        let mut harness = bench(named_group());
        let actions = drag(&mut harness, "Sets", top_of("First"));
        assert!(
            matches!(
                actions.as_slice(),
                [Action::PlaceSeriesGroup {
                    group: stokesforge_plot::SeriesGroupRef { panel: 0, group: 1 },
                    position: 0,
                    ..
                }]
            ),
            "{actions:?}"
        );
    }

    /// A free text is reordered among the texts; never among the shapes.
    #[test]
    fn free_objects_keep_their_category_when_dragged() {
        let mut project = Project::new("Objects").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        for note in ["Peak", "Trough"] {
            panel.annotations.push(AnnotationSpec::new(note, 0.0, 0.0));
        }
        for kind in [ShapeKind::Line, ShapeKind::Arrow] {
            panel.shapes.push(ShapeSpec::new(kind, 0.0, 0.0, 1.0, 1.0));
        }
        let spec = PlotSpec::default().with_panels(panel);
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let model = stokesforge_app_state::object_manager::object_tree_with_selection(
            project.tree(),
            graph,
            None,
            false,
            true,
        )
        .unwrap();
        let settings = ObjectManagerSettings {
            objects_open: true,
            ..ObjectManagerSettings::default()
        };
        let mut harness = bench_with((project, graph, spec, model), settings);
        let actions = drag(&mut harness, "Trough", top_of("Peak"));
        assert!(
            matches!(
                actions.as_slice(),
                [Action::ReorderObject {
                    target: T::Annotation(AnnotationRef { panel: 0, index: 1 }),
                    index: 0,
                    ..
                }]
            ),
            "{actions:?}"
        );
        // A text onto a shape: nothing moves.
        let actions = drag(&mut harness, "Peak", |h| {
            handle(h, &tr!("tool-line")).center()
        });
        assert!(actions.is_empty(), "{actions:?}");
    }

    /// #151: the right click must target the aimed row, even if another curve
    /// is selected. The painted menu is common to the detached panel.
    #[test]
    fn right_click_deletes_the_targeted_curve_and_closes_the_menu() {
        use super::super::test_support::painted;
        for swatch in [false, true] {
            let (project, graph, spec, model) = three_series();
            let ctx = super::super::test_support::context();
            let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
            paint(Vec::new());
            let (output, _, _) = paint(Vec::new());
            let row = if swatch {
                let text_rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Second" => {
                            Some(text.visual_bounding_rect())
                        }
                        _ => None,
                    })
                    .unwrap();
                // The legend line is right to the left of the button.
                egui::pos2(text_rect.left() - 24.0, text_rect.center().y)
            } else {
                text_center(&output, "Second")
            };
            let (_, actions, requests) = click_menu(&paint, row, egui::PointerButton::Secondary);
            assert!(actions.is_empty());
            assert!(requests.iter().any(|request| matches!(request,
                ObjectManagerRequest::Select(T::Series(series)) if *series == SeriesRef::new(0, 1)
            )));
            let (output, _, _) = paint(Vec::new());
            let delete = text_center(&output, &tr!("action-delete-content"));
            let (_, actions, _) = click_menu(&paint, delete, egui::PointerButton::Primary);
            assert_eq!(
                actions,
                vec![Action::RemovePlotObject {
                    graph,
                    target: T::Series(SeriesRef::new(0, 1)),
                }]
            );
            let (output, actions, _) = paint(Vec::new());
            assert!(actions.is_empty());
            assert!(!painted(&output, &tr!("action-delete-content")));
        }
    }

    #[test]
    fn right_click_analyzes_the_targeted_curve_in_the_object_manager() {
        let (project, _, spec, model) = three_series();
        let ctx = super::super::test_support::context();
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let row = text_center(&output, "Second");
        click_menu(&paint, row, egui::PointerButton::Secondary);
        let (output, _, _) = paint(Vec::new());
        let submenu = text_center(&output, &tr!("menu-analysis"));
        click_menu(&paint, submenu, egui::PointerButton::Primary);
        let (output, _, _) = paint(Vec::new());
        let entry = text_center(&output, &tr!("action-analysis-open-data-manipulation"));
        let (_, actions, requests) = click_menu(&paint, entry, egui::PointerButton::Primary);
        assert!(
            actions.is_empty(),
            "the view must still provide the framing"
        );
        assert!(requests.iter().any(|request| matches!(request,
            ObjectManagerRequest::Analysis(T::Series(series), stokesforge_app_state::analysis::Category::DataManipulation) if *series == SeriesRef::new(0, 1)
        )));
    }

    /// #169: the selected series is a list row — pale accent background under
    /// the checkbox, the swatch and the name, accent bar on the left, name in
    /// bold —, as in the Project Manager; no longer the `selection.bg_fill`
    /// of a button. Witness: the unselected series.
    #[test]
    fn the_selected_series_is_a_list_row_with_a_bar() {
        let (project, _graph, spec, model) = three_series();
        let ctx = super::super::test_support::context();
        image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let (output, _, _) = image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let rects = |fill: egui::Color32| -> Vec<(usize, egui::Rect)> {
            output
                .shapes
                .iter()
                .enumerate()
                .filter_map(|(index, shape)| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill == fill => Some((index, rect.rect)),
                    _ => None,
                })
                .collect()
        };
        let rows = rects(crate::theme::list_row_selected_fill());
        let marks = rects(crate::theme::accent());
        assert_eq!(rows.len(), 1, "a single selected row: {rows:?}");
        assert_eq!(marks.len(), 1, "a single bar: {marks:?}");
        let (row_index, row) = rows[0];
        let (_, mark) = marks[0];
        assert_eq!(mark.left(), row.left(), "the bar is at the left of the row");
        assert_eq!(mark.width(), super::super::list_row::LIST_ROW_MARK);
        assert_eq!(mark.height(), row.height());
        let first = text_center(&output, "First");
        assert!(row.y_range().contains(first.y), "the row carries the name");
        assert!(row.left() < first.x - 30.0, "and the checkbox, the swatch");
        assert!(
            row.right() > 400.0,
            "over the whole remaining width: {row:?}"
        );
        let first_index = output
            .shapes
            .iter()
            .position(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "First"))
            .unwrap();
        assert!(
            row_index < first_index,
            "the background goes under the name"
        );
        let button_fill = ctx.global_style().visuals.selection.bg_fill;
        assert!(rects(button_fill).is_empty(), "no more button background");
        let painted_texts = super::super::test_support::texts(&output.shapes);
        let is_bold = |name: &str| painted_texts.iter().any(|t| t.text == name && t.bold);
        assert!(is_bold("First"), "the selected series is bold");
        assert!(
            !is_bold("Second") && !is_bold("Last"),
            "witness: the others"
        );
    }

    /// #170, point 5: a selected group, series and free object paint a
    /// background of the same height, `LIST_ROW_HEIGHT`; the L connectors
    /// (#112) aim at the center of that height. Witness: without the imposed
    /// height, the row falls back to that of its widgets (18 px) and the test
    /// fails.
    #[test]
    fn group_series_and_object_have_the_same_row_height() {
        use super::super::list_row::LIST_ROW_HEIGHT;
        let (mut project, graph, mut spec, _) = three_series();
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series[1].group = Some(1);
        panel.series[2].group = Some(1);
        assert!(panel.rename_group(1, "Sets"));
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 1.0));
        project
            .set_plot_spec(graph, spec.clone())
            .expect("spec with group and shape");
        let settings = ObjectManagerSettings {
            objects_open: true,
            ..ObjectManagerSettings::default()
        };
        let targets = [
            (
                "group",
                T::SeriesGroup(stokesforge_plot::SeriesGroupRef::new(0, 1)),
            ),
            ("series", T::Series(SeriesRef::new(0, 0))),
            ("object", T::Shape(ShapeRef { panel: 0, index: 0 })),
        ];
        for (kind, target) in targets {
            let model = stokesforge_app_state::object_manager::object_tree_with_selection(
                project.tree(),
                graph,
                Some(target),
                false,
                true,
            )
            .unwrap();
            let ctx = super::super::test_support::context();
            let mut actions = Vec::new();
            let mut output = None;
            for _ in 0..3 {
                output = Some(frame_of(&ctx, egui::vec2(600.0, 500.0), Vec::new(), |ui| {
                    object_manager_contents(
                        ui,
                        project.tree(),
                        Some(&model),
                        Some(&spec),
                        settings,
                        &mut actions,
                    );
                }));
            }
            let output = output.unwrap();
            let backgrounds: Vec<egui::Rect> = output
                .shapes
                .iter()
                .filter_map(|item| match &item.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == crate::theme::list_row_selected_fill() =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(backgrounds.len(), 1, "{kind}: one selected row");
            assert_eq!(backgrounds[0].height(), LIST_ROW_HEIGHT, "{kind}");
            // The background imposes its minimum height: the row itself must
            // fill it, so the row's text is centered in the background.
            let centers: Vec<f32> = output
                .shapes
                .iter()
                .filter_map(|item| match &item.shape {
                    egui::Shape::Text(text) if !text.galley.text().is_empty() => {
                        let y = text.pos.y + text.galley.rect.center().y;
                        backgrounds[0].y_range().contains(y).then_some(y)
                    }
                    _ => None,
                })
                .collect();
            assert!(!centers.is_empty(), "{kind}: a text on the row");
            for y in centers {
                assert!(
                    (y - backgrounds[0].center().y).abs() < 1.0,
                    "{kind}: text at {y}, row centered at {}",
                    backgrounds[0].center().y
                );
            }
        }
    }

    /// Three series, the last two in a group named "Sets" (#197), and the
    /// Object Manager tree of that graph.
    fn named_group() -> (
        Project,
        NodeId,
        PlotSpec,
        stokesforge_app_state::object_manager::ObjectTree,
    ) {
        let (mut project, graph, mut spec, _) = three_series();
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series[1].group = Some(1);
        panel.series[2].group = Some(1);
        assert!(panel.rename_group(1, "Sets"));
        project
            .set_plot_spec(graph, spec.clone())
            .expect("spec with group");
        let model = stokesforge_app_state::object_manager::object_tree(project.tree(), graph, None)
            .unwrap();
        (project, graph, spec, model)
    }

    /// #177, #197: a group is a parent node like the others — a header that
    /// carries its name, then its series in a branch, further right than the
    /// header; a single series stays one level up.
    #[test]
    fn a_group_is_a_parent_node_whose_members_are_in_a_branch() {
        let (project, _, spec, model) = named_group();
        let ctx = super::super::test_support::context();
        image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let (output, _, _) = image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let single = text_center(&output, "First");
        let member = text_center(&output, "Second");
        let header = text_center(&output, "Sets");
        assert!(
            header.y > single.y && header.y < member.y,
            "the header precedes its members: {header:?}"
        );
        assert!(
            member.x > header.x && text_center(&output, "Last").x > header.x,
            "the members are further right than the header"
        );
        assert_eq!(
            super::super::test_support::texts(&output.shapes)
                .iter()
                .filter(|t| t.text == tr!("curve-group-name", number = 1))
                .count(),
            0,
            "the given name replaces \"Group 1\" everywhere"
        );
    }

    /// #197: collapsing a group hides its rows, never its curves — the group's
    /// visibility checkbox is mixed when its members differ.
    #[test]
    fn collapsing_a_group_hides_its_rows_and_not_its_curves() {
        let (project, _, mut spec, _) = named_group();
        spec.panels.get_mut(0).unwrap().series[2].visible = false;
        let mut project = project;
        let graph = project.tree().node(project.root()).unwrap().children()[0];
        project.set_plot_spec(graph, spec.clone()).unwrap();
        let model = stokesforge_app_state::object_manager::object_tree(project.tree(), graph, None)
            .unwrap();
        let ctx = super::super::test_support::context();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let header = text_center(&output, "Sets");
        let target = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let state = T::SeriesGroup(target).visibility(&spec);
        assert!(state.mixed && !state.checked, "mixed checkbox: {state:?}");
        // The collapse triangle is left of the checkbox, in the header's row.
        let arrow = egui::pos2(header.x - 46.0, header.y);
        click_menu(&paint, arrow, egui::PointerButton::Primary);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        assert!(
            super::super::test_support::texts(&output.shapes)
                .iter()
                .all(|t| t.text != "Second" && t.text != "Last"),
            "the members of a collapsed group are no longer listed"
        );
        assert!(
            super::super::test_support::texts(&output.shapes)
                .iter()
                .any(|t| t.text == "First"),
            "witness: the single series stays"
        );
        assert_eq!(
            T::SeriesGroup(target).contents(&spec).len(),
            2,
            "collapsing removes no curve from the group"
        );
    }

    /// #197: the header is selected, opens on double-click and carries the
    /// group gestures on right click — sections of `docs/menus.md`.
    #[test]
    fn a_group_header_is_selected_opens_and_has_its_menu() {
        let (project, graph, spec, model) = named_group();
        let target = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let ctx = super::super::test_support::context();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let header = text_center(&output, "Sets");
        let (_, actions, requests) = click_menu(&paint, header, egui::PointerButton::Primary);
        assert!(actions.is_empty());
        assert!(requests
            .iter()
            .any(|r| matches!(r, ObjectManagerRequest::Select(T::SeriesGroup(g)) if *g == target)));
        // Right click: the group's menu.
        click_menu(&paint, header, egui::PointerButton::Secondary);
        let output = paint(Vec::new()).0;
        use super::super::test_support::painted;
        for entry in [
            tr!("action-select-group-series"),
            tr!("action-rename-group"),
            tr!("action-ungroup-series"),
            tr!("action-delete-content"),
            tr!("action-properties"),
        ] {
            assert!(painted(&output, &entry), "{entry}");
        }
        let (_, actions, _) = click_menu(
            &paint,
            text_center(&output, &tr!("action-ungroup-series")),
            egui::PointerButton::Primary,
        );
        assert_eq!(
            actions,
            vec![Action::UngroupSeries {
                graph,
                group: target
            }]
        );
    }

    /// #197: renaming a group is done in place, in the tree — Enter commits in
    /// one action, an unchanged name emits none.
    #[test]
    fn renaming_a_group_in_place_emits_one_action_on_commit() {
        let (project, graph, spec, model) = named_group();
        let target = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let ctx = super::super::test_support::context();
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        open_group_name_editor(&ctx, &spec, graph, target);
        paint(Vec::new());
        paint(Vec::new());
        paint(vec![egui::Event::Text("Blanks".into())]);
        let (_, actions, _) = paint(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert_eq!(
            actions,
            vec![Action::RenameSeriesGroup {
                graph,
                group: target,
                name: "Blanks".into()
            }]
        );
        assert!(
            group_rename_of(&ctx, graph, target).is_none(),
            "the editor closes"
        );
    }

    #[test]
    fn the_colored_line_takes_focus_for_delete_and_opens_the_properties() {
        let (project, graph, spec, model) = three_series();
        let ctx = super::super::test_support::context();
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let text_rect = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "First" => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .unwrap();
        let swatch = egui::pos2(text_rect.left() - 24.0, text_rect.center().y);
        let (_, actions, requests) = click_menu(&paint, swatch, egui::PointerButton::Primary);
        assert!(actions.is_empty());
        assert!(requests.iter().any(|r| matches!(r,
            ObjectManagerRequest::Select(T::Series(s)) if *s == SeriesRef::new(0, 0)
        )));
        paint(Vec::new());
        let (_, actions, _) = paint(vec![key(egui::Key::Delete, egui::Modifiers::NONE)]);
        assert_eq!(
            actions,
            vec![Action::RemovePlotObject {
                graph,
                target: T::Series(SeriesRef::new(0, 0)),
            }]
        );
        let mut properties = false;
        for _ in 0..2 {
            let (_, _, requests) = click_menu(&paint, swatch, egui::PointerButton::Primary);
            properties |= requests.iter().any(|r| {
                matches!(r,
                    ObjectManagerRequest::Properties(T::Series(s)) if *s == SeriesRef::new(0, 0)
                )
            });
        }
        assert!(
            properties,
            "the double-click on the line opens the properties"
        );
    }

    /// #170, point 6: in the focused Object Manager, ↑ and ↓ select the
    /// neighboring visible row (bounded), Enter asks for the selection's
    /// properties like the double-click, Shift+↓ does nothing, and Ctrl+↓ stays
    /// the series move (no selection). Without focus, no key.
    #[test]
    fn arrows_and_enter_walk_through_the_object_manager_rows() {
        let (project, graph, spec, _model) = three_series();
        // The middle series is the selection: ↑ and ↓ each have a neighbor.
        let model = stokesforge_app_state::object_manager::object_tree(
            project.tree(),
            graph,
            Some(SeriesRef::new(0, 1)),
        )
        .unwrap();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        let selects = |requests: &[ObjectManagerRequest]| -> Vec<SeriesRef> {
            requests
                .iter()
                .filter_map(|request| match request {
                    ObjectManagerRequest::Select(T::Series(series)) => Some(*series),
                    _ => None,
                })
                .collect()
        };
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        // Without panel focus, the arrows are not for us.
        let (_, _, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
        assert!(selects(&requests).is_empty(), "without focus");

        // A click on the middle row gives the panel focus.
        let row = text_center(&output, "Second");
        image(vec![egui::Event::PointerMoved(row)]);
        image(vec![pointer(row, true)]);
        image(vec![pointer(row, false)]);
        image(Vec::new());

        let (_, _, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
        assert_eq!(
            selects(&requests),
            vec![SeriesRef::new(0, 2)],
            "↓: the next one"
        );
        let (_, _, requests) = image(vec![key(egui::Key::ArrowUp, egui::Modifiers::NONE)]);
        assert_eq!(
            selects(&requests),
            vec![SeriesRef::new(0, 0)],
            "↑: the previous one"
        );
        let (_, _, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::SHIFT)]);
        assert!(selects(&requests).is_empty(), "Shift+↓ does not move");

        // Ctrl+↓ stays "move the series down": one action, no selection.
        let (_, actions, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::CTRL)]);
        assert_eq!(
            actions,
            vec![Action::MoveSeries {
                graph,
                series: SeriesRef::new(0, 1),
                up: false,
            }]
        );
        assert!(selects(&requests).is_empty(), "Ctrl+↓ does not change row");

        // Enter: the properties of the selected series, like the double-click.
        let (_, actions, requests) = image(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert!(actions.is_empty());
        assert!(requests.iter().any(|request| matches!(
            request,
            ObjectManagerRequest::Properties(T::Series(series)) if *series == SeriesRef::new(0, 1)
        )));
    }

    /// #170, point 6: the selected row carries the focus outline when the panel
    /// has keyboard focus, and only then.
    #[test]
    fn the_selected_row_has_the_focus_outline_when_the_panel_is_focused() {
        let (project, _graph, spec, model) = three_series();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        let stroke = ctx.global_style().visuals.selection.stroke;
        let contours = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke == stroke)
                })
                .count()
        };
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        assert_eq!(contours(&output), 0, "without focus, no outline");
        let row = text_center(&output, "First");
        image(vec![egui::Event::PointerMoved(row)]);
        image(vec![pointer(row, true)]);
        image(vec![pointer(row, false)]);
        let (output, _, _) = image(Vec::new());
        assert_eq!(contours(&output), 1, "focused: the selected row");
        // Witness: a click elsewhere gives the focus back, the outline disappears.
        let away = egui::pos2(590.0, 490.0);
        image(vec![egui::Event::PointerMoved(away)]);
        image(vec![pointer(away, true)]);
        image(vec![pointer(away, false)]);
        let (output, _, _) = image(Vec::new());
        assert_eq!(contours(&output), 0, "focus given back, no more outline");
    }

    /// Delete removes the selected object, but only when the panel has keyboard
    /// focus (a click on a row gives it): without it, the key belongs to another
    /// widget. The double-click asks for the properties.
    #[test]
    fn delete_acts_only_on_the_focused_panel_and_the_double_click_opens_the_properties() {
        let (project, graph, spec, model) = three_series();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        let (_, actions, _) = image(vec![key(egui::Key::Delete, egui::Modifiers::NONE)]);
        assert!(actions.is_empty(), "without focus, Delete is not for us");

        // #126: a single click must suffice, without starting a drag.
        let row = text_center(&output, "First");
        image(vec![egui::Event::PointerMoved(row)]);
        image(vec![pointer(row, true)]);
        let (_, actions, requests) = image(vec![pointer(row, false)]);
        assert!(actions.is_empty(), "a click does not move the series");
        assert!(requests.iter().any(|r| {
            matches!(r, ObjectManagerRequest::Select(T::Series(s)) if *s == SeriesRef::new(0, 0))
        }));
        // A frame without an event must not give the focus back.
        image(Vec::new());
        let (_, actions, requests) = image(vec![key(egui::Key::F2, egui::Modifiers::NONE)]);
        assert!(actions.is_empty());
        assert!(requests.iter().any(|r| matches!(
            r,
            ObjectManagerRequest::Rename(SeriesRef { panel: 0, index: 0 })
        )));
        for (key_name, up) in [
            (egui::Key::ArrowDown, Some(false)),
            (egui::Key::ArrowUp, None),
        ] {
            let (_, actions, _) = image(vec![key(key_name, egui::Modifiers::CTRL)]);
            let expected: Vec<_> = up
                .into_iter()
                .map(|up| Action::MoveSeries {
                    graph,
                    series: SeriesRef::new(0, 0),
                    up,
                })
                .collect();
            assert_eq!(actions, expected);
        }
        let (_, actions, _) = image(vec![key(egui::Key::Delete, egui::Modifiers::NONE)]);
        assert_eq!(
            actions,
            vec![Action::RemovePlotObject {
                graph,
                target: T::Series(SeriesRef::new(0, 0)),
            }]
        );

        // Double-click: two close clicks on the same row.
        let mut properties = false;
        for _ in 0..2 {
            image(vec![pointer(row, true)]);
            let (_, _, requests) = image(vec![pointer(row, false)]);
            properties |= requests.iter().any(|r| {
                matches!(r, ObjectManagerRequest::Properties(T::Series(s)) if *s == SeriesRef::new(0, 0))
            });
        }
        assert!(
            properties,
            "the double-click asks for the series properties"
        );

        // Witness: a click outside the rows gives the panel focus back.
        let elsewhere = egui::pos2(550.0, 450.0);
        image(vec![egui::Event::PointerMoved(elsewhere)]);
        image(vec![pointer(elsewhere, true)]);
        image(vec![pointer(elsewhere, false)]);
        for (pressed_key, modifiers) in [
            (egui::Key::F2, egui::Modifiers::NONE),
            (egui::Key::ArrowDown, egui::Modifiers::CTRL),
            (egui::Key::Delete, egui::Modifiers::NONE),
        ] {
            let (_, actions, requests) = image(vec![key(pressed_key, modifiers)]);
            assert!(
                actions.is_empty() && requests.is_empty(),
                "the panel gave its keys back"
            );
        }
    }

    /// A row's checkbox toggles its object's visibility: one action, to the
    /// opposite of the state shown.
    #[test]
    fn a_row_checkbox_toggles_the_visibility_of_its_object() {
        for visible in [true, false] {
            let mut project = Project::new("Objects").unwrap();
            let root = project.root();
            let mut panel = PanelSpec::default();
            for name in ["First", "Second"] {
                let mut series = SeriesSpec::new(NodeId::from_raw(900));
                series.name = name.into();
                panel.series.push(series);
            }
            panel.series[1].visible = visible;
            let spec = PlotSpec::default().with_panels(panel);
            let graph = project.add_graph(root, None, spec.clone()).unwrap();
            let model =
                stokesforge_app_state::object_manager::object_tree(project.tree(), graph, None);
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            let image = |events| image(&ctx, project.tree(), model.as_ref(), Some(&spec), events);
            image(Vec::new());
            let (output, _, _) = image(Vec::new());
            // The checkbox is the square painted on the row, left of the label.
            let label = text_center(&output, "Second");
            let at = output
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Rect(rect)
                        // The checkbox is one row high (#170); the handle,
                        // on its left, 16 px.
                        if (rect.rect.width() - rect.rect.height()).abs() < 1.0
                            && (rect.rect.width() - super::super::list_row::LIST_ROW_HEIGHT)
                                .abs()
                                < 1.0
                            && (rect.rect.center().y - label.y).abs() < 4.0
                            && rect.rect.center().x < label.x =>
                    {
                        Some(rect.rect.center())
                    }
                    _ => None,
                })
                .min_by(|a, b| a.x.total_cmp(&b.x))
                .expect("visibility checkbox painted on the row");
            image(vec![egui::Event::PointerMoved(at)]);
            image(vec![pointer(at, true)]);
            let (_, actions, _) = image(vec![pointer(at, false)]);
            assert_eq!(
                actions,
                vec![Action::SetPlotVisibility {
                    graph,
                    target: T::Series(SeriesRef::new(0, 1)),
                    visible: !visible,
                }],
                "visible = {visible}"
            );
        }
    }

    /// Without an active graph, the panel says so and asks for nothing.
    /// The panel ([`StokesForgeApp::object_manager_panel`]) is painted only when
    /// open: anchored to the right edge, or in its OS viewport.
    #[test]
    fn the_object_manager_is_painted_only_when_open_anchored_right_or_detached() {
        use egui::containers::panel::PanelState;
        let (project, graph, _, _) = three_series();
        let session = stokesforge_session::Session::spawn(project, None).unwrap();
        let state =
            stokesforge_app_state::state::AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let paint = |app: &mut StokesForgeApp, context: &egui::Context| {
            let mut actions = Vec::new();
            let mut output = egui::FullOutput::default();
            for _ in 0..2 {
                output.textures_delta.clear();
                output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ui| app.object_manager_panel(ui, Some(Tab::Graph(graph)), &mut actions),
                );
            }
            output.textures_delta.clear();
            assert!(actions.is_empty(), "painting asks for nothing");
            output
        };
        let is_painted = |output: &egui::FullOutput, needle: &str| {
            output.shapes.iter().any(|painted| {
                matches!(&painted.shape,
                    egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
            })
        };
        let id = egui::Id::new("stokesforge_object_manager");

        app.state.set_object_manager_open_for_test(false);
        let context = egui::Context::default();
        egui_extras::install_image_loaders(&context);
        let output = paint(&mut app, &context);
        assert!(!is_painted(&output, "First"), "closed: nothing is painted");
        assert!(PanelState::load(&context, id).is_none());

        app.state.set_object_manager_open_for_test(true);
        let output = paint(&mut app, &context);
        assert!(is_painted(&output, &tr!("action-object-manager")));
        assert!(is_painted(&output, "First"), "witness: the tree is painted");
        let docked = PanelState::load(&context, id).unwrap().outer_rect;
        assert_eq!(docked.right(), screen.right(), "anchored to the right edge");

        app.om_detached = true;
        let context = egui::Context::default();
        egui_extras::install_image_loaders(&context);
        let output = paint(&mut app, &context);
        assert!(is_painted(&output, "First"));
        assert!(
            PanelState::load(&context, id).is_none(),
            "no more anchored panel"
        );
        assert!(app.om_view.is_some(), "the detached reader exists");
        session.shutdown().unwrap();
    }

    #[test]
    fn without_a_graph_the_panel_says_so_and_asks_for_nothing() {
        let (project, _, spec, model) = three_series();
        let ctx = egui::Context::default();
        let (output, actions, requests) =
            image(&ctx, project.tree(), None, Some(&spec), Vec::new());
        assert!(actions.is_empty() && requests.is_empty());
        text_center(&output, &tr!("object-manager-no-graph"));
        // Witness: with a graph, the message is not painted.
        let (output, _, _) = image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == tr!("object-manager-no-graph"))));
    }

    /// The Object Manager's selection and the view's are the same: selecting a
    /// target then reading it back returns the same target, for each kind of
    /// object; a vanished target selects nothing.
    #[test]
    fn selecting_then_reading_back_a_target_returns_the_same_target() {
        let mut spec = PlotSpec::empty("Title");
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        panel
            .annotations
            .push(AnnotationSpec::new("Text", 0.0, 0.0));
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 1.0));
        let annotation = AnnotationRef { panel: 0, index: 0 };
        let shape = ShapeRef { panel: 0, index: 0 };
        let mut view = GraphView::new();
        for target in [
            T::Series(SeriesRef::new(0, 0)),
            T::Annotation(annotation),
            T::Shape(shape),
            T::Text(TextTarget::Title),
        ] {
            select_manager_target(&mut view, Some(target), &spec);
            assert_eq!(selected_manager_target(&view), Some(target));
        }
        select_manager_target(&mut view, None, &spec);
        assert_eq!(selected_manager_target(&view), None, "None deselects");

        select_manager_target(&mut view, Some(T::Shape(shape)), &spec);
        let gone = ShapeRef { index: 5, ..shape };
        select_manager_target(&mut view, Some(T::Shape(gone)), &spec);
        assert_eq!(
            selected_manager_target(&view),
            None,
            "a vanished object does not stay selected"
        );
    }
}

//! Object Manager peint (#81–#85, #102, #111, #113) : l'arbre de
//! [`stokesforge_app_state::object_manager::ObjectTree`] en lignes, cases de visibilité, menus
//! contextuels, glisser par poignée (séries, groupes, objets libres ; `object_dnd.rs`) et raccourcis clavier (Suppr, F2,
//! Ctrl+flèches). Le modèle (projection du `PlotSpec`, filtres) reste dans
//! `stokesforge_app_state::object_manager` ; la sélection vit dans la `GraphView`. Ce qui
//! touche la vue (sélection, propriétés, renommage) remonte en
//! [`ObjectManagerRequest`], exécuté après la peinture par
//! [`StokesForgeApp::run_object_manager_request`], qui sert aussi le menu
//! « Graph › Selected Content ». Une seule écriture pour le panneau fixe et la
//! fenêtre détachée. Les réglages (#139) viennent de la table unique
//! `preferences::ObjectManagerSettings` ; egui garde la priorité pour les
//! groupes déjà vus, y compris après Réinitialiser.
//! L'échantillon de légende reçoit les mêmes gestes que le nom de série
//! (#151) : une zone seulement peinte laissait son clic droit au fond.
//!
//! Le panneau fixe ou détaché (#100),
//! [`StokesForgeApp::object_manager_panel`], et l'exécution des demandes l'ont
//! rejoint (#122). Les en-têtes, calques, séries, axes et objets se peignent
//! par sections (#124), sans changer les portées et identifiants egui ni
//! l'ordre des demandes et des connecteurs d'arbre.

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
    /// Édite sur place le nom d'un groupe de séries (#197).
    RenameGroup(stokesforge_plot::SeriesGroupRef),
    /// Édite sur place le titre, le titre d'axe ou l'entrée de légende.
    EditText(ContextText),
}
/// Libellé d'un nœud d'en-tête de l'Object Manager (graphe, calque, groupes) :
/// un double-clic ouvre ses paramètres, comme sur les autres lignes (#113).
fn manager_header_label(
    ui: &mut egui::Ui,
    text: &str,
    target: stokesforge_app_state::object_manager::VisibilityTarget,
    requests: &mut Vec<ObjectManagerRequest>,
) -> egui::Response {
    let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    // Un clic sur le nœud d'un calque (ou son groupe) le rend actif (#161).
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

/// Séries analysables de la cible, dans leur ordre de tracé. Une série
/// « ligne » s'analyse par la feuille transposée liée à sa feuille (#253) ;
/// un groupe qui mêle des courbes de colonnes et de lignes n'est pas
/// analysable d'un coup (`analysis-needs-same-curves`).
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

/// **Le** menu d'un objet du graphe (#152) : le même au clic droit dans la
/// figure, sur son nœud de l'Object Manager et sous Graph › Selection (qui
/// l'enveloppe dans son sous-menu, le clic droit l'affiche à plat). Sections
/// de la grammaire de `docs/menus.md`, une section sans objet pour la cible
/// étant **absente** :
///
/// 1. principale — Show Data / Analysis (courbe), Analysis (groupe),
///    Edit Text / Reset Position (textes),
///    Select Members (groupe) ;
/// 2. organiser — Rename (courbe, groupe), Move Up/Down (courbe, groupe),
///    Ungroup (groupe), Hide (courbe), Bring to Front / Send to Back (objet),
///    Hide (texte, axe, groupe) ;
///    Closed Frame (calque et groupe d'axes, bascule, #344) ;
/// 3. supprimer — Delete (courbe, texte libre, objet), Delete Group (groupe
///    et ses séries) ;
/// 4. `Properties…`, toujours en dernier.
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
                        // Le rang change : la sélection suit l'objet.
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

/// Pourquoi une série ou un groupe ne monte ou ne descend plus (#226).
fn move_reason(up: bool) -> String {
    if up {
        tr!("disabled-move-first")
    } else {
        tr!("disabled-move-last")
    }
}

/// Pourquoi la case ou l'entrée Show/Hide d'un objet est grisée (#226) : un
/// axe miroir déjà dessiné par un calque du dessus, sinon rien à basculer.
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

/// Le menu des réglages d'Object Manager **à plat** (#152) : c'est le menu
/// du clic droit sur le fond du panneau, sans enveloppe « Settings ▸ ». Les
/// cases gardent le menu ouvert (#114). Les réglages ne touchent ni le
/// `PlotSpec` ni les replis courants d'egui.
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

/// Les mêmes réglages sous View › Object Manager : l'enveloppe du menu du
/// haut, le contenu étant celui du clic droit sur le fond. Fonctionne aussi
/// en fenêtre détachée.
pub(super) fn object_manager_settings_menu(
    ui: &mut egui::Ui,
    settings: ObjectManagerSettings,
    actions: &mut Vec<Action>,
) {
    object_manager_settings_contents(ui, settings, actions);
}

/// Un seul contenu pour Object Manager, quel que soit son emplacement.
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
    // Pendant un glisser, la liste se peint d'après l'aperçu du geste (le spec
    // réel, lui, ne change qu'au relâchement).
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
    // #126 : enregistrer l'ancre avant les lignes. egui rend son focus
    // au relâchement d'un clic, car Rect::NOTHING n'est jamais survolé ;
    // la ligne cliquée le réclame ensuite. Un clic ailleurs le rend toujours,
    // et un champ d'édition garde ses propres touches.
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
        // ↑ ↓ : la ligne sélectionnable visible voisine ; Entrée : ses propriétés,
        // comme le double-clic (#170, point 6). Ctrl+↑ / Ctrl+↓ (déplacer une
        // série) ne passent pas par ici : `series_shortcuts` plus bas.
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

// Arguments de peinture communs aux sections : aucune Ui ni portée egui
// supplémentaire. La sélection et les événements restent accumulés par les
// corps effectivement dépliés, dans l'ordre de leurs lignes.
struct ObjectManagerPaint<'a> {
    tree: &'a Tree,
    spec: &'a stokesforge_plot::PlotSpec,
    graph: NodeId,
    focus: egui::Id,
    settings: ObjectManagerSettings,
    /// L'élément glissé, à sa place dans l'aperçu.
    item: Option<DragItem>,
    /// La ligne que l'élément glissé survole, vue pendant la peinture.
    hover: Hover,
    /// Les lignes sélectionnables **peintes** cette image, dans l'ordre de
    /// lecture : ce que ↑ et ↓ parcourent (#170, point 6) ; une section
    /// repliée n'y figure pas.
    visible: std::cell::RefCell<Vec<T>>,
    /// Le panneau a le focus clavier : sa ligne sélectionnée porte le contour
    /// de focus.
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
                    // Un groupe de séries (#177, #197) est un nœud parent comme
                    // les autres : en-tête repliable, membres en branche. Une
                    // série seule reste au niveau du dessus.
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
                    // Sortie d'un groupe qui finit la liste (et dépôt en fin de
                    // liste) : la zone n'existe que pendant un glisser de série
                    // ou de groupe de ce calque.
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

    /// La zone « fin de la liste » : une ligne grisée sous la dernière série.
    fn end_zone(&self, ui: &mut egui::Ui, panel: usize) -> f32 {
        let row = ui.horizontal(|ui| ui.weak(tr!("object-drag-end"))).response;
        if ui.rect_contains_pointer(row.rect) {
            self.hover.offer(DropOver::End { panel });
        }
        row.rect.center().y
    }

    /// Un groupe de séries (#197) : le nœud, sa case de visibilité mixte, son
    /// nom (éditable sur place) et ses membres en branche `tree_lines`. Le
    /// repli initial est celui de « Series » (#139).
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

    /// L'en-tête d'un groupe : une ligne de liste (#169) sous la case et le
    /// nom, sélectionnable, ouvrant la fenêtre des séries au double-clic. Sa
    /// poignée déplace le groupe en bloc ; une série glissée dessus le rejoint
    /// (moitié basse) ou se pose avant lui (moitié haute).
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
        // Le défaut vient des préférences (#139), sans écraser un repli déjà choisi.
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
            // #169 : une ligne de liste, fond posé sous la case et l'échantillon.
            let background = ListRowBackground::reserve(ui);
            let target = series.target;
            let row = with_list_row_height(ui, |ui| {
                ui.horizontal(|ui| {
                    // La poignée en tête (comme les formats de copie des
                    // préférences) : la ligne se glisse, les autres s'écartent.
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
                            // #151 : l'échantillon de légende fait partie de la cible,
                            // comme le nom. Toute la ligne, hors case, saisit et
                            // clique : on unit les gestes avant de traiter sélection
                            // et propriétés.
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

/// Les raccourcis d'un groupe de séries sélectionné (#197) : F2 renomme,
/// Ctrl+↑/↓ le déplace en bloc. Suppr est celui de tous les contenus.
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

/// Le renommage d'un groupe en cours (#197), tenu dans la mémoire d'egui sous
/// l'identité du graphe : le panneau fixe et sa copie détachée partagent le
/// même éditeur, qui n'existe que dans l'arbre.
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

/// Pour les tests des autres surfaces (raccourcis du graphe) : un renommage
/// du groupe `target` est-il en attente ?
#[cfg(test)]
pub(super) fn group_rename_pending(
    ctx: &egui::Context,
    graph: NodeId,
    target: stokesforge_plot::SeriesGroupRef,
) -> bool {
    group_rename_of(ctx, graph, target).is_some()
}

/// Ouvre l'éditeur du nom du groupe `group` dans l'arbre.
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

/// Le champ du nom à la place du libellé, au contrat commun de
/// [`inplace_edit`](super::inplace_edit) : Entrée, Tab ou un clic ailleurs
/// valident (une seule action, et aucune si le nom n'a pas changé), Échap annule.
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
        // #169 : une ligne de liste, fond posé sous la case et l'icône.
        let background = ListRowBackground::reserve(ui);
        // Un texte libre ou une forme se réordonne **dans sa catégorie** ; les
        // axes ne bougent pas.
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

    /// L'arbre et le menu Graph partagent les mêmes gestes de vue et Action.
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

/// Exécute une demande d'objet sur la vue du graphe : la même pour le
/// panneau, Graph › Selection et le clic droit dans la figure (#152).
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
    use super::super::test_support::image_de;
    use super::*;
    use egui_kittest::kittest::Queryable;
    use stokesforge_core::Project;
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesRef, SeriesSpec, ShapeKind,
        ShapeRef, ShapeSpec, TextTarget,
    };

    use stokesforge_app_state::object_manager::VisibilityTarget as T;

    #[test]
    fn analyse_du_groupe_garde_ses_membres_et_le_spec_de_la_vue() {
        let (project, graph, mut spec, _) = trois_series();
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
            "un groupe mêlant colonnes et lignes ne s'analyse pas d'un coup"
        );
        spec.panels.get_mut(0).unwrap().series[0].row = Some(1);
        assert_eq!(
            analysis_sources(&spec, target),
            vec![SeriesRef::new(0, 0), SeriesRef::new(0, 2)],
            "un groupe de lignes s'analyse par sa feuille transposée (#253)"
        );
        assert_eq!(
            analysis_sources(&spec, T::Series(SeriesRef::new(0, 0))),
            vec![SeriesRef::new(0, 0)],
            "une courbe ligne est analysable"
        );
    }

    /// Un graphe de trois séries (« Premier », « Deuxième », « Dernier »), la
    /// première sélectionnée, et son arbre d'Object Manager.
    fn trois_series() -> (
        Project,
        NodeId,
        PlotSpec,
        stokesforge_app_state::object_manager::ObjectTree,
    ) {
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        for name in ["Premier", "Deuxième", "Dernier"] {
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

    /// Une image de l'Object Manager sur un écran de 600 × 500 : la sortie,
    /// les actions et les demandes de l'image.
    fn image(
        ctx: &egui::Context,
        tree: &Tree,
        model: Option<&stokesforge_app_state::object_manager::ObjectTree>,
        spec: Option<&PlotSpec>,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<Action>, Vec<ObjectManagerRequest>) {
        let mut actions = Vec::new();
        let mut requests = Vec::new();
        let output = image_de(ctx, egui::vec2(600.0, 500.0), events, |ui| {
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
            .unwrap_or_else(|| panic!("texte absent : {text}"))
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Pilote un vrai clic sur une surface peinte, sans appeler le menu
    /// directement : les popup racine et les sous-menus participent au geste.
    fn clic_menu(
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
    fn clic_droit_du_fond_affichage_bascule_les_filtres_et_garde_les_menus_ouverts() {
        use super::super::test_support::painted;
        let (project, _, spec, model) = trois_series();
        let ctx = super::super::test_support::contexte();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        paint(Vec::new());
        clic_menu(
            &paint,
            egui::pos2(300.0, 300.0),
            egui::PointerButton::Secondary,
        );
        let output = paint(Vec::new()).0;
        assert!(
            painted(&output, &tr!("object-manager-show-axes")),
            "les réglages sont au premier niveau, sans enveloppe"
        );
        assert!(painted(&output, &tr!("object-manager-reset")));
        {
            let key = tr!("object-manager-show-axes");
            let expected = Action::SetObjectManagerShowScales(true);
            let output = paint(Vec::new()).0;
            let (_, actions, requests) = clic_menu(
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
                "le menu racine reste ouvert"
            );
            assert!(painted(&output, &tr!("object-manager-show-axes")));
            assert!(painted(&output, &tr!("object-manager-show-axes")));
        }
    }

    /// Les réglages du fond restent disponibles sans graphe ; les cinq
    /// bascules gardent leur chaîne ouverte, tandis que Réinitialiser ferme.
    #[test]
    fn clic_droit_sans_graphe_regle_les_cinq_defauts_puis_reinitialise() {
        use super::super::test_support::painted;
        let project = Project::new("Réglages").unwrap();
        let settings = ObjectManagerSettings::default();
        let ctx = super::super::test_support::contexte();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), None, None, events);
        paint(Vec::new());
        paint(Vec::new());
        clic_menu(
            &paint,
            egui::pos2(200.0, 150.0),
            egui::PointerButton::Secondary,
        );
        let output = paint(Vec::new()).0;
        clic_menu(
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
            let (_, actions, requests) = clic_menu(
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
        let (_, actions, requests) = clic_menu(
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
    fn clic_droit_serie_ordre_grise_les_extremes_et_ferme_apres_deplacement() {
        use super::super::test_support::{painted, texte_exact, textes};
        for (row, source, disabled, enabled, up) in [
            (
                "Premier",
                SeriesRef::new(0, 0),
                tr!("action-move-series-up"),
                tr!("action-move-series-down"),
                false,
            ),
            (
                "Dernier",
                SeriesRef::new(0, 2),
                tr!("action-move-series-down"),
                tr!("action-move-series-up"),
                true,
            ),
        ] {
            let (project, graph, spec, model) = trois_series();
            let ctx = super::super::test_support::contexte();
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
            paint(Vec::new());
            let output = paint(Vec::new()).0;
            let (_, actions, requests) = clic_menu(
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
                "les déplacements sont à plat, sans sous-menu Ordre"
            );
            let labels = textes(&output.shapes);
            assert_ne!(
                texte_exact(&labels, &disabled).couleur,
                texte_exact(&labels, &enabled).couleur,
                "l'extrême est grisé"
            );
            let (_, actions, requests) = clic_menu(
                &paint,
                text_center(&output, &disabled),
                egui::PointerButton::Primary,
            );
            assert!(
                actions.is_empty() && requests.is_empty(),
                "le déplacement interdit ne produit rien"
            );
            // Un clic sur un item grisé peut fermer le popup d'egui ; le
            // geste utile est testé après réouverture de la même série.
            if !egui::Popup::is_any_open(&ctx) {
                let output = paint(Vec::new()).0;
                clic_menu(
                    &paint,
                    text_center(&output, row),
                    egui::PointerButton::Secondary,
                );
            }
            let output = paint(Vec::new()).0;
            let (_, actions, requests) = clic_menu(
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
                "toute la chaîne de menus est fermée"
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
    /// Les groupes ont des identités indépendantes ; replier les séries
    /// laisse les axes et les objets du calque à leur place, et inversement.
    #[test]
    fn replier_un_groupe_preserve_les_autres_sections_de_l_arbre() {
        let mut project = Project::new("Objets").unwrap();
        let mut spec = PlotSpec::empty("Titre global");
        let panel = spec.panels.get_mut(0).unwrap();
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = "Courbe".into();
        panel.series.push(series);
        panel
            .annotations
            .push(AnnotationSpec::new("Note libre", 0.0, 0.0));
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
                .expect("flèche de repli peinte sur l'en-tête")
        };
        paint(Vec::new());
        let (output, actions, requests) = paint(Vec::new());
        assert!(actions.is_empty() && requests.is_empty());
        let mut expected = vec![model.title.clone()];
        expected.extend([
            tr!("object-manager-layer", index = 1),
            tr!("object-manager-group"),
            "Courbe".into(),
            tr!("object-manager-axes"),
        ]);
        for axis in &model.layers[0].axes {
            assert!(
                !labels(&output).contains(&axis.label),
                "les axes sont repliés par défaut"
            );
        }
        assert!(!model.layers[0].objects.is_empty());
        expected.push(tr!("object-manager-objects"));
        assert_eq!(
            labels(&output),
            expected,
            "les sections suivent l'ordre du graphique, « Objects » replié par défaut"
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
                    "replier n'est pas une mutation du modèle"
                );
            }
            paint(Vec::new()).0
        };
        let collapsed = click_toggle(&output, &tr!("object-manager-group"));
        let texts = labels(&collapsed);
        assert!(!texts.iter().any(|text| text == "Courbe"));
        assert!(texts.contains(&tr!("object-manager-objects")));
        for axis in &model.layers[0].axes {
            assert!(
                !texts.contains(&axis.label),
                "replier le groupe de tracés ne déplie pas les axes"
            );
        }
        let expanded = click_toggle(&collapsed, &tr!("object-manager-group"));
        assert!(labels(&expanded).iter().any(|text| text == "Courbe"));
        let with_axes = click_toggle(&expanded, &tr!("object-manager-axes"));
        let texts = labels(&with_axes);
        assert!(texts.iter().any(|text| text == "Courbe"));
        assert!(texts.contains(&tr!("object-manager-objects")));
        for axis in &model.layers[0].axes {
            assert!(
                texts.contains(&axis.label),
                "témoin : un clic sur « Axes » déplie les échelles"
            );
        }
        let collapsed = click_toggle(&with_axes, &tr!("object-manager-axes"));
        let texts = labels(&collapsed);
        assert!(texts.iter().any(|text| text == "Courbe"));
        for axis in &model.layers[0].axes {
            assert!(
                !texts.contains(&axis.label),
                "un second clic replie les axes, et seulement eux"
            );
        }
    }

    /// Chaque famille a un témoin peint ; modifier le défaut doit changer
    /// un contexte neuf, sans remplacer les états des groupes déjà vus.
    #[test]
    fn les_reglages_initiaux_changent_chaque_groupe_neuf_et_preservent_les_groupes_vus() {
        use crate::app::test_support::painted;
        let mut project = Project::new("Réglages").unwrap();
        let mut spec = PlotSpec::empty("Titre");
        spec.title = "Texte global".into();
        let panel = spec.panels.get_mut(0).unwrap();
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = "Courbe témoin".into();
        panel.series.push(series);
        panel
            .annotations
            .push(AnnotationSpec::new("Texte local", 0.0, 0.0));
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
            let output = image_de(ctx, egui::vec2(800.0, 700.0), Vec::new(), |ui| {
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
            assert!(actions.is_empty(), "un défaut de vue ne mute pas le modèle");
            output
        };
        for (group, witness) in [
            (
                ObjectManagerGroup::Graph,
                tr!("object-manager-layer", index = 1),
            ),
            (ObjectManagerGroup::Layers, tr!("object-manager-group")),
            (ObjectManagerGroup::Plots, "Courbe témoin".into()),
            (
                ObjectManagerGroup::Axes,
                model.layers[0].axes[0].label.clone(),
            ),
            (ObjectManagerGroup::Objects, "Texte local".into()),
        ] {
            let ctx = crate::app::test_support::contexte();
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            paint(&ctx, settings);
            let baseline = paint(&ctx, settings);
            if group == ObjectManagerGroup::Objects {
                assert!(
                    !painted(&baseline, "Texte global"),
                    "le titre du graphique n'est jamais listé"
                );
            }
            let expected = settings.group_open(group);
            assert_eq!(
                painted(&baseline, &witness),
                expected,
                "témoin par défaut {group:?}"
            );
            let mut changed = settings;
            changed.set_group_open(group, !expected);
            paint(&ctx, changed);
            assert_eq!(
                painted(&paint(&ctx, changed), &witness),
                expected,
                "le groupe déjà vu garde son état {group:?}"
            );
            let fresh = crate::app::test_support::contexte();
            fresh.all_styles_mut(|style| style.animation_time = 0.0);
            paint(&fresh, changed);
            let output = paint(&fresh, changed);
            assert_eq!(
                painted(&output, &witness),
                !expected,
                "le nouveau groupe lit le réglage {group:?}"
            );
            if group == ObjectManagerGroup::Objects {
                assert!(
                    !painted(&output, "Texte global"),
                    "le groupe global suit le même réglage mais possède son propre ID"
                );
            }
            // Réinitialiser rend les défauts sans modifier les groupes vus.
            paint(&fresh, ObjectManagerSettings::default());
            assert_eq!(
                painted(&paint(&fresh, ObjectManagerSettings::default()), &witness),
                !expected
            );
        }
    }

    /// F2 renomme et Ctrl+↓ descend la série sélectionnée (clavier, sans glisser).
    #[test]
    fn f2_et_ctrl_fleche_agissent_sur_la_serie_selectionnee() {
        let (project, _graph, spec, model) = trois_series();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        // Un clic sur la ligne donne le focus clavier au panneau.
        let on = text_center(&output, "Premier");
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

    /// Un Object Manager vivant : le spec et son arbre ne changent pas (le
    /// modèle n'est écrit qu'au relâchement), les actions s'accumulent.
    struct Banc {
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

    fn banc(fixture: Fixture) -> egui_kittest::Harness<'static, Banc> {
        banc_avec(fixture, ObjectManagerSettings::default())
    }

    fn banc_avec(
        (project, _graph, spec, model): Fixture,
        settings: ObjectManagerSettings,
    ) -> egui_kittest::Harness<'static, Banc> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(420.0, 420.0))
            .build_ui_state(
                |ui, banc: &mut Banc| {
                    object_manager_contents(
                        ui,
                        banc.project.tree(),
                        Some(&banc.model),
                        Some(&banc.spec),
                        banc.settings,
                        &mut banc.actions,
                    );
                },
                Banc {
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

    /// La zone de saisie d'une ligne (son libellé), par son nom.
    fn poignee(harness: &egui_kittest::Harness<'static, Banc>, nom: &str) -> egui::Rect {
        harness.get_by_label(nom).rect()
    }

    /// Saisit la poignée de `nom`, la mène au point `vers` (en plusieurs
    /// images, comme une main) et relâche. Les actions émises sont rendues.
    fn glisser(
        harness: &mut egui_kittest::Harness<'static, Banc>,
        nom: &str,
        vers: impl Fn(&egui_kittest::Harness<'static, Banc>) -> egui::Pos2,
    ) -> Vec<Action> {
        let from = poignee(harness, nom).center();
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from + egui::vec2(14.0, 0.0));
        harness.step();
        harness.hover_at(from + egui::vec2(20.0, 0.0));
        harness.step();
        // La cible se lit une fois : la liste se réordonne sous le pointeur.
        let to = vers(harness);
        for _ in 0..3 {
            harness.hover_at(to);
            harness.step();
        }
        assert!(
            harness.state().actions.is_empty(),
            "l'aperçu n'écrit pas le modèle"
        );
        harness.drop_at(to);
        harness.run();
        std::mem::take(&mut harness.state_mut().actions)
    }

    /// Le haut de la ligne de `nom` (moitié haute).
    fn haut_de(nom: &'static str) -> impl Fn(&egui_kittest::Harness<'static, Banc>) -> egui::Pos2 {
        move |harness| {
            let rect = poignee(harness, nom);
            egui::pos2(rect.center().x, rect.top() + 1.0)
        }
    }

    /// Glisser « Premier » sur « Dernier » : une seule action, au relâchement,
    /// avec le rang final.
    #[test]
    fn glisser_une_serie_seule_emet_un_seul_placement_au_relachement() {
        let mut harness = banc(trois_series());
        let actions = glisser(&mut harness, "Premier", |h| poignee(h, "Dernier").center());
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
            "le dépôt ne se répète pas à l'image suivante"
        );
    }

    /// Lâcher une série là où elle était ne demande rien.
    #[test]
    fn relacher_sur_place_ne_demande_rien() {
        let mut harness = banc(trois_series());
        let actions = glisser(&mut harness, "Deuxième", |h| {
            poignee(h, "Deuxième").center()
        });
        assert!(actions.is_empty());
    }

    /// Une série seule glissée sur un membre d'un groupe y entre, au rang de
    /// la ligne survolée.
    #[test]
    fn glisser_une_serie_seule_sur_un_membre_la_fait_entrer_dans_le_groupe() {
        let mut harness = banc(groupe_nomme());
        let actions = glisser(&mut harness, "Premier", haut_de("Deuxième"));
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

    /// Un membre glissé sur une série seule quitte son groupe.
    #[test]
    fn glisser_un_membre_sur_une_serie_seule_le_fait_sortir_du_groupe() {
        let mut harness = banc(groupe_nomme());
        let actions = glisser(&mut harness, "Dernier", haut_de("Premier"));
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

    /// La zone « fin de la liste » n'existe que pendant un glisser, et sort
    /// d'un groupe qui finit la liste.
    #[test]
    fn la_zone_de_fin_sort_un_membre_d_un_groupe_qui_finit_la_liste() {
        let mut harness = banc(groupe_nomme());
        assert!(
            harness.query_by_label(&tr!("object-drag-end")).is_none(),
            "pas de zone hors glisser"
        );
        let actions = glisser(&mut harness, "Dernier", |h| {
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

    /// Un groupe se glisse en bloc par la poignée de son en-tête.
    #[test]
    fn glisser_l_entete_d_un_groupe_le_deplace_en_bloc() {
        let mut harness = banc(groupe_nomme());
        let actions = glisser(&mut harness, "Lots", haut_de("Premier"));
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

    /// Un texte libre se réordonne parmi les textes ; jamais parmi les formes.
    #[test]
    fn les_objets_libres_gardent_leur_categorie_au_glisser() {
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        for note in ["Pic", "Creux"] {
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
        let mut harness = banc_avec((project, graph, spec, model), settings);
        let actions = glisser(&mut harness, "Creux", haut_de("Pic"));
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
        // Un texte sur une forme : rien ne bouge.
        let actions = glisser(&mut harness, "Pic", |h| {
            poignee(h, &tr!("tool-line")).center()
        });
        assert!(actions.is_empty(), "{actions:?}");
    }

    /// #151 : le clic droit doit cibler la ligne visée, même si une autre
    /// courbe est sélectionnée. Le menu peint est commun au panneau détaché.
    #[test]
    fn clic_droit_supprime_la_courbe_visee_et_ferme_le_menu() {
        use super::super::test_support::painted;
        for swatch in [false, true] {
            let (project, graph, spec, model) = trois_series();
            let ctx = super::super::test_support::contexte();
            let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
            paint(Vec::new());
            let (output, _, _) = paint(Vec::new());
            let row = if swatch {
                let text_rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Deuxième" => {
                            Some(text.visual_bounding_rect())
                        }
                        _ => None,
                    })
                    .unwrap();
                // Le trait de légende est immédiatement à gauche du bouton.
                egui::pos2(text_rect.left() - 24.0, text_rect.center().y)
            } else {
                text_center(&output, "Deuxième")
            };
            let (_, actions, requests) = clic_menu(&paint, row, egui::PointerButton::Secondary);
            assert!(actions.is_empty());
            assert!(requests.iter().any(|request| matches!(request,
                ObjectManagerRequest::Select(T::Series(series)) if *series == SeriesRef::new(0, 1)
            )));
            let (output, _, _) = paint(Vec::new());
            let delete = text_center(&output, &tr!("action-delete-content"));
            let (_, actions, _) = clic_menu(&paint, delete, egui::PointerButton::Primary);
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
    fn clic_droit_analyse_la_courbe_visee_dans_object_manager() {
        let (project, _, spec, model) = trois_series();
        let ctx = super::super::test_support::contexte();
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let row = text_center(&output, "Deuxième");
        clic_menu(&paint, row, egui::PointerButton::Secondary);
        let (output, _, _) = paint(Vec::new());
        let submenu = text_center(&output, &tr!("menu-analysis"));
        clic_menu(&paint, submenu, egui::PointerButton::Primary);
        let (output, _, _) = paint(Vec::new());
        let entry = text_center(&output, &tr!("action-analysis-open-data-manipulation"));
        let (_, actions, requests) = clic_menu(&paint, entry, egui::PointerButton::Primary);
        assert!(actions.is_empty(), "la vue doit encore fournir le cadrage");
        assert!(requests.iter().any(|request| matches!(request,
            ObjectManagerRequest::Analysis(T::Series(series), stokesforge_app_state::analysis::Category::DataManipulation) if *series == SeriesRef::new(0, 1)
        )));
    }

    /// #169 : la série sélectionnée est une ligne de liste — fond pâle
    /// d'accent sous la case, l'échantillon et le nom, filet d'accent à
    /// gauche, nom en gras —, comme dans le Project Manager ; plus le fond
    /// `selection.bg_fill` d'un bouton. Témoin : les séries non choisies.
    #[test]
    fn la_serie_selectionnee_est_une_ligne_de_liste_a_filet() {
        let (project, _graph, spec, model) = trois_series();
        let ctx = super::super::test_support::contexte();
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
        assert_eq!(rows.len(), 1, "une seule ligne sélectionnée : {rows:?}");
        assert_eq!(marks.len(), 1, "un seul filet : {marks:?}");
        let (row_index, row) = rows[0];
        let (_, mark) = marks[0];
        assert_eq!(mark.left(), row.left(), "le filet est à gauche de la ligne");
        assert_eq!(mark.width(), super::super::list_row::LIST_ROW_MARK);
        assert_eq!(mark.height(), row.height());
        let premier = text_center(&output, "Premier");
        assert!(row.y_range().contains(premier.y), "la ligne porte le nom");
        assert!(row.left() < premier.x - 30.0, "et la case, l'échantillon");
        assert!(
            row.right() > 400.0,
            "sur toute la largeur restante : {row:?}"
        );
        let premier_index = output
            .shapes
            .iter()
            .position(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "Premier"))
            .unwrap();
        assert!(row_index < premier_index, "le fond passe sous le nom");
        let button_fill = ctx.global_style().visuals.selection.bg_fill;
        assert!(rects(button_fill).is_empty(), "plus de fond de bouton");
        let peints = super::super::test_support::textes(&output.shapes);
        let gras = |nom: &str| peints.iter().any(|t| t.texte == nom && t.gras);
        assert!(gras("Premier"), "la série sélectionnée est en gras");
        assert!(!gras("Deuxième") && !gras("Dernier"), "témoin : les autres");
    }

    /// #170, point 5 : un groupe, une série et un objet libre sélectionnés
    /// peignent un fond de la même hauteur, `LIST_ROW_HEIGHT` ; les connecteurs
    /// en L (#112) visent le centre de cette hauteur. Témoin : sans la hauteur
    /// imposée, la rangée retombe à celle de ses widgets (18 px) et le test
    /// échoue.
    #[test]
    fn groupe_serie_et_objet_ont_la_meme_hauteur_de_ligne() {
        use super::super::list_row::LIST_ROW_HEIGHT;
        let (mut project, graph, mut spec, _) = trois_series();
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series[1].group = Some(1);
        panel.series[2].group = Some(1);
        assert!(panel.rename_group(1, "Lots"));
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 1.0));
        project
            .set_plot_spec(graph, spec.clone())
            .expect("spec avec groupe et forme");
        let settings = ObjectManagerSettings {
            objects_open: true,
            ..ObjectManagerSettings::default()
        };
        let cibles = [
            (
                "groupe",
                T::SeriesGroup(stokesforge_plot::SeriesGroupRef::new(0, 1)),
            ),
            ("série", T::Series(SeriesRef::new(0, 0))),
            ("objet", T::Shape(ShapeRef { panel: 0, index: 0 })),
        ];
        for (nature, cible) in cibles {
            let model = stokesforge_app_state::object_manager::object_tree_with_selection(
                project.tree(),
                graph,
                Some(cible),
                false,
                true,
            )
            .unwrap();
            let ctx = super::super::test_support::contexte();
            let mut actions = Vec::new();
            let mut sortie = None;
            for _ in 0..3 {
                sortie = Some(image_de(&ctx, egui::vec2(600.0, 500.0), Vec::new(), |ui| {
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
            let sortie = sortie.unwrap();
            let fonds: Vec<egui::Rect> = sortie
                .shapes
                .iter()
                .filter_map(|peint| match &peint.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == crate::theme::list_row_selected_fill() =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(fonds.len(), 1, "{nature} : une ligne sélectionnée");
            assert_eq!(fonds[0].height(), LIST_ROW_HEIGHT, "{nature}");
            // Le fond impose sa hauteur minimale : la rangée elle-même doit la
            // remplir, donc le texte de la ligne est centré dans le fond.
            let centres: Vec<f32> = sortie
                .shapes
                .iter()
                .filter_map(|peint| match &peint.shape {
                    egui::Shape::Text(texte) if !texte.galley.text().is_empty() => {
                        let y = texte.pos.y + texte.galley.rect.center().y;
                        fonds[0].y_range().contains(y).then_some(y)
                    }
                    _ => None,
                })
                .collect();
            assert!(!centres.is_empty(), "{nature} : un texte sur la ligne");
            for y in centres {
                assert!(
                    (y - fonds[0].center().y).abs() < 1.0,
                    "{nature} : texte à {y}, ligne centrée à {}",
                    fonds[0].center().y
                );
            }
        }
    }

    /// Trois séries, les deux dernières en groupe nommé « Lots » (#197), et
    /// l'arbre d'Object Manager de ce graphe.
    fn groupe_nomme() -> (
        Project,
        NodeId,
        PlotSpec,
        stokesforge_app_state::object_manager::ObjectTree,
    ) {
        let (mut project, graph, mut spec, _) = trois_series();
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series[1].group = Some(1);
        panel.series[2].group = Some(1);
        assert!(panel.rename_group(1, "Lots"));
        project
            .set_plot_spec(graph, spec.clone())
            .expect("spec avec groupe");
        let model = stokesforge_app_state::object_manager::object_tree(project.tree(), graph, None)
            .unwrap();
        (project, graph, spec, model)
    }

    /// #177, #197 : un groupe est un nœud parent comme les autres — un
    /// en-tête qui porte son nom, puis ses séries en branche, plus à droite
    /// que l'en-tête ; une série seule reste au niveau du dessus.
    #[test]
    fn un_groupe_est_un_noeud_parent_dont_les_membres_sont_en_branche() {
        let (project, _, spec, model) = groupe_nomme();
        let ctx = super::super::test_support::contexte();
        image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let (output, _, _) = image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let seule = text_center(&output, "Premier");
        let membre = text_center(&output, "Deuxième");
        let en_tete = text_center(&output, "Lots");
        assert!(
            en_tete.y > seule.y && en_tete.y < membre.y,
            "l'en-tête précède ses membres : {en_tete:?}"
        );
        assert!(
            membre.x > en_tete.x && text_center(&output, "Dernier").x > en_tete.x,
            "les membres sont plus à droite que l'en-tête"
        );
        assert_eq!(
            super::super::test_support::textes(&output.shapes)
                .iter()
                .filter(|t| t.texte == tr!("curve-group-name", number = 1))
                .count(),
            0,
            "le nom donné remplace « Group 1 » partout"
        );
    }

    /// #197 : replier un groupe cache ses membres, jamais ses courbes — la
    /// case de visibilité du groupe est mixte quand ses membres divergent.
    #[test]
    fn replier_un_groupe_cache_ses_lignes_et_pas_ses_courbes() {
        let (project, _, mut spec, _) = groupe_nomme();
        spec.panels.get_mut(0).unwrap().series[2].visible = false;
        let mut project = project;
        let graph = project.tree().node(project.root()).unwrap().children()[0];
        project.set_plot_spec(graph, spec.clone()).unwrap();
        let model = stokesforge_app_state::object_manager::object_tree(project.tree(), graph, None)
            .unwrap();
        let ctx = super::super::test_support::contexte();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let en_tete = text_center(&output, "Lots");
        let target = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let state = T::SeriesGroup(target).visibility(&spec);
        assert!(state.mixed && !state.checked, "case mixte : {state:?}");
        // Le triangle de repli est à gauche de la case, dans la rangée de l'en-tête.
        let arrow = egui::pos2(en_tete.x - 46.0, en_tete.y);
        clic_menu(&paint, arrow, egui::PointerButton::Primary);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        assert!(
            super::super::test_support::textes(&output.shapes)
                .iter()
                .all(|t| t.texte != "Deuxième" && t.texte != "Dernier"),
            "les membres d'un groupe replié ne sont plus listés"
        );
        assert!(
            super::super::test_support::textes(&output.shapes)
                .iter()
                .any(|t| t.texte == "Premier"),
            "témoin : la série seule reste"
        );
        assert_eq!(
            T::SeriesGroup(target).contents(&spec).len(),
            2,
            "replier ne retire aucune courbe du groupe"
        );
    }

    /// #197 : l'en-tête se sélectionne, s'ouvre au double-clic et porte les
    /// gestes de groupe au clic droit — sections de `docs/menus.md`.
    #[test]
    fn l_en_tete_d_un_groupe_se_selectionne_s_ouvre_et_a_son_menu() {
        let (project, graph, spec, model) = groupe_nomme();
        let target = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let ctx = super::super::test_support::contexte();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let en_tete = text_center(&output, "Lots");
        let (_, actions, requests) = clic_menu(&paint, en_tete, egui::PointerButton::Primary);
        assert!(actions.is_empty());
        assert!(requests
            .iter()
            .any(|r| matches!(r, ObjectManagerRequest::Select(T::SeriesGroup(g)) if *g == target)));
        // Clic droit : le menu du groupe.
        clic_menu(&paint, en_tete, egui::PointerButton::Secondary);
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
        let (_, actions, _) = clic_menu(
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

    /// #197 : renommer un groupe se fait sur place, dans l'arbre — Entrée
    /// valide en une action, un nom inchangé n'en émet aucune.
    #[test]
    fn renommer_un_groupe_sur_place_emet_une_action_a_la_validation() {
        let (project, graph, spec, model) = groupe_nomme();
        let target = stokesforge_plot::SeriesGroupRef::new(0, 1);
        let ctx = super::super::test_support::contexte();
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        open_group_name_editor(&ctx, &spec, graph, target);
        paint(Vec::new());
        paint(Vec::new());
        paint(vec![egui::Event::Text("Blancs".into())]);
        let (_, actions, _) = paint(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert_eq!(
            actions,
            vec![Action::RenameSeriesGroup {
                graph,
                group: target,
                name: "Blancs".into()
            }]
        );
        assert!(
            group_rename_of(&ctx, graph, target).is_none(),
            "l'éditeur se referme"
        );
    }

    #[test]
    fn le_trait_colore_prend_le_focus_pour_suppr_et_ouvre_les_proprietes() {
        let (project, graph, spec, model) = trois_series();
        let ctx = super::super::test_support::contexte();
        let paint = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        paint(Vec::new());
        let (output, _, _) = paint(Vec::new());
        let text_rect = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Premier" => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .unwrap();
        let swatch = egui::pos2(text_rect.left() - 24.0, text_rect.center().y);
        let (_, actions, requests) = clic_menu(&paint, swatch, egui::PointerButton::Primary);
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
            let (_, _, requests) = clic_menu(&paint, swatch, egui::PointerButton::Primary);
            properties |= requests.iter().any(|r| {
                matches!(r,
                    ObjectManagerRequest::Properties(T::Series(s)) if *s == SeriesRef::new(0, 0)
                )
            });
        }
        assert!(
            properties,
            "le double-clic sur le trait ouvre les propriétés"
        );
    }

    /// #170, point 6 : dans l'Object Manager focalisé, ↑ et ↓ sélectionnent la
    /// ligne visible voisine (bornée), Entrée demande les propriétés de la
    /// sélection comme le double-clic, Maj+↓ ne fait rien, et Ctrl+↓ reste le
    /// déplacement de la série (aucune sélection). Sans focus, aucune touche.
    #[test]
    fn fleches_et_entree_parcourent_les_lignes_de_l_object_manager() {
        let (project, graph, spec, _model) = trois_series();
        // La série du milieu est la sélection : ↑ et ↓ ont chacune un voisin.
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
        // Sans focus du panneau, les flèches ne sont pas pour nous.
        let (_, _, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
        assert!(selects(&requests).is_empty(), "sans focus");

        // Un clic sur la ligne du milieu donne le focus au panneau.
        let row = text_center(&output, "Deuxième");
        image(vec![egui::Event::PointerMoved(row)]);
        image(vec![pointer(row, true)]);
        image(vec![pointer(row, false)]);
        image(Vec::new());

        let (_, _, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
        assert_eq!(
            selects(&requests),
            vec![SeriesRef::new(0, 2)],
            "↓ : la suivante"
        );
        let (_, _, requests) = image(vec![key(egui::Key::ArrowUp, egui::Modifiers::NONE)]);
        assert_eq!(
            selects(&requests),
            vec![SeriesRef::new(0, 0)],
            "↑ : la précédente"
        );
        let (_, _, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::SHIFT)]);
        assert!(selects(&requests).is_empty(), "Maj+↓ ne déplace pas");

        // Ctrl+↓ reste « descendre la série » : une action, aucune sélection.
        let (_, actions, requests) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::CTRL)]);
        assert_eq!(
            actions,
            vec![Action::MoveSeries {
                graph,
                series: SeriesRef::new(0, 1),
                up: false,
            }]
        );
        assert!(
            selects(&requests).is_empty(),
            "Ctrl+↓ ne change pas de ligne"
        );

        // Entrée : les propriétés de la série sélectionnée, comme le double-clic.
        let (_, actions, requests) = image(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert!(actions.is_empty());
        assert!(requests.iter().any(|request| matches!(
            request,
            ObjectManagerRequest::Properties(T::Series(series)) if *series == SeriesRef::new(0, 1)
        )));
    }

    /// #170, point 6 : la ligne sélectionnée porte le contour de focus quand le
    /// panneau a le focus clavier, et seulement alors.
    #[test]
    fn la_ligne_selectionnee_a_le_contour_de_focus_quand_le_panneau_est_focalise() {
        let (project, _graph, spec, model) = trois_series();
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
        assert_eq!(contours(&output), 0, "sans focus, pas de contour");
        let row = text_center(&output, "Premier");
        image(vec![egui::Event::PointerMoved(row)]);
        image(vec![pointer(row, true)]);
        image(vec![pointer(row, false)]);
        let (output, _, _) = image(Vec::new());
        assert_eq!(contours(&output), 1, "focalisé : la ligne sélectionnée");
        // Témoin : un clic ailleurs rend le focus, le contour disparaît.
        let away = egui::pos2(590.0, 490.0);
        image(vec![egui::Event::PointerMoved(away)]);
        image(vec![pointer(away, true)]);
        image(vec![pointer(away, false)]);
        let (output, _, _) = image(Vec::new());
        assert_eq!(contours(&output), 0, "le focus rendu, plus de contour");
    }

    /// Suppr retire l'objet sélectionné, mais seulement quand le panneau a le
    /// focus clavier (un clic sur une ligne le lui donne) : sans lui, la touche
    /// appartient à un autre widget. Le double-clic demande les propriétés.
    #[test]
    fn suppr_n_agit_que_sur_le_panneau_focalise_et_le_double_clic_ouvre_les_proprietes() {
        let (project, graph, spec, model) = trois_series();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        let (_, actions, _) = image(vec![key(egui::Key::Delete, egui::Modifiers::NONE)]);
        assert!(actions.is_empty(), "sans focus, Suppr n'est pas pour nous");

        // #126 : un clic simple doit suffire, sans amorcer de glisser.
        let row = text_center(&output, "Premier");
        image(vec![egui::Event::PointerMoved(row)]);
        image(vec![pointer(row, true)]);
        let (_, actions, requests) = image(vec![pointer(row, false)]);
        assert!(actions.is_empty(), "un clic ne déplace pas la série");
        assert!(requests.iter().any(|r| {
            matches!(r, ObjectManagerRequest::Select(T::Series(s)) if *s == SeriesRef::new(0, 0))
        }));
        // Une image sans événement ne doit pas rendre le focus.
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

        // Double-clic : deux clics rapprochés sur la même ligne.
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
            "le double-clic demande les propriétés de la série"
        );

        // Témoin : un clic hors des lignes rend le focus du panneau.
        let ailleurs = egui::pos2(550.0, 450.0);
        image(vec![egui::Event::PointerMoved(ailleurs)]);
        image(vec![pointer(ailleurs, true)]);
        image(vec![pointer(ailleurs, false)]);
        for (touche, modifiers) in [
            (egui::Key::F2, egui::Modifiers::NONE),
            (egui::Key::ArrowDown, egui::Modifiers::CTRL),
            (egui::Key::Delete, egui::Modifiers::NONE),
        ] {
            let (_, actions, requests) = image(vec![key(touche, modifiers)]);
            assert!(
                actions.is_empty() && requests.is_empty(),
                "le panneau a rendu ses touches"
            );
        }
    }

    /// La case d'une ligne bascule la visibilité de son objet : une action,
    /// vers l'état inverse de celui affiché.
    #[test]
    fn la_case_d_une_ligne_bascule_la_visibilite_de_son_objet() {
        for visible in [true, false] {
            let mut project = Project::new("Objets").unwrap();
            let root = project.root();
            let mut panel = PanelSpec::default();
            for name in ["Premier", "Deuxième"] {
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
            // La case est le carré peint sur la ligne, à gauche du libellé.
            let label = text_center(&output, "Deuxième");
            let at = output
                .shapes
                .iter()
                .filter_map(|painted| match &painted.shape {
                    egui::Shape::Rect(rect)
                        // La case fait la hauteur d'une ligne (#170) ; la
                        // poignée, à sa gauche, 16 px.
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
                .expect("case de visibilité peinte sur la ligne");
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

    /// Sans graphe actif, le panneau le dit et ne demande rien.
    /// Le panneau ([`StokesForgeApp::object_manager_panel`]) ne se peint
    /// qu'ouvert : ancré au bord droit, ou dans son viewport OS.
    #[test]
    fn l_object_manager_ne_se_peint_qu_ouvert_ancre_a_droite_ou_detache() {
        use egui::containers::panel::PanelState;
        let (project, graph, _, _) = trois_series();
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
            assert!(actions.is_empty(), "peindre ne demande rien");
            output
        };
        let peint = |output: &egui::FullOutput, needle: &str| {
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
        assert!(!peint(&output, "Premier"), "fermé : rien n'est peint");
        assert!(PanelState::load(&context, id).is_none());

        app.state.set_object_manager_open_for_test(true);
        let output = paint(&mut app, &context);
        assert!(peint(&output, &tr!("action-object-manager")));
        assert!(peint(&output, "Premier"), "témoin : l'arbre est peint");
        let docked = PanelState::load(&context, id).unwrap().outer_rect;
        assert_eq!(docked.right(), screen.right(), "ancré au bord droit");

        app.om_detached = true;
        let context = egui::Context::default();
        egui_extras::install_image_loaders(&context);
        let output = paint(&mut app, &context);
        assert!(peint(&output, "Premier"));
        assert!(
            PanelState::load(&context, id).is_none(),
            "plus de panneau ancré"
        );
        assert!(app.om_view.is_some(), "le lecteur détaché existe");
        session.shutdown().unwrap();
    }

    #[test]
    fn sans_graphe_le_panneau_le_dit_et_ne_demande_rien() {
        let (project, _, spec, model) = trois_series();
        let ctx = egui::Context::default();
        let (output, actions, requests) =
            image(&ctx, project.tree(), None, Some(&spec), Vec::new());
        assert!(actions.is_empty() && requests.is_empty());
        text_center(&output, &tr!("object-manager-no-graph"));
        // Témoin : avec un graphe, le message n'est pas peint.
        let (output, _, _) = image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == tr!("object-manager-no-graph"))));
    }

    /// La sélection de l'Object Manager et celle de la vue sont la même :
    /// sélectionner une cible puis la relire rend la même cible, pour chaque
    /// nature d'objet ; une cible disparue ne sélectionne rien.
    #[test]
    fn selectionner_puis_relire_une_cible_rend_la_meme_cible() {
        let mut spec = PlotSpec::empty("Titre");
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        panel
            .annotations
            .push(AnnotationSpec::new("Texte", 0.0, 0.0));
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
        assert_eq!(selected_manager_target(&view), None, "None désélectionne");

        select_manager_target(&mut view, Some(T::Shape(shape)), &spec);
        let gone = ShapeRef { index: 5, ..shape };
        select_manager_target(&mut view, Some(T::Shape(gone)), &spec);
        assert_eq!(
            selected_manager_target(&view),
            None,
            "un objet disparu ne reste pas sélectionné"
        );
    }
}

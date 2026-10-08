//! Object Manager peint (#81–#85, #102, #111, #113) : l'arbre de
//! [`crate::object_manager::ObjectTree`] en lignes, cases de visibilité, menus
//! contextuels, glisser des séries et raccourcis clavier (Suppr, F2,
//! Ctrl+flèches). Le modèle (projection du `PlotSpec`, filtres) reste dans
//! `crate::object_manager` ; la sélection vit dans la `GraphView`. Ce qui
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
use crate::object_manager::{
    LayerRow, ObjectRow, ObjectTree, SeriesRow, Shown, VisibilityTarget as T,
};
use crate::preferences::{ObjectManagerGroup, ObjectManagerSettings};
use crate::state::{active_graph, Action};
use crate::tr;

use super::graph_text_edit::{
    open_annotation_editor, open_legend_entry_editor, open_series_name_editor, open_text_editor,
    ContextText,
};
use super::graph_window::open_shape_properties;
use super::list_row::{list_row_label, list_row_text, list_row_text_color, ListRowBackground};
use super::menu_sections::MenuSections;
use super::shortcuts::{
    shortcut_text_delete, SHORTCUT_MOVE_SERIES_DOWN, SHORTCUT_MOVE_SERIES_UP,
    SHORTCUT_RENAME_OBJECT,
};
use super::tab::active_tab_kind;
use super::tab::Tab;
use super::window_chrome::{panel_header, PanelPlacement, PANEL_STRIP_THICKNESS};
use super::window_detach::ObjectManagerModel;
use super::StokesForgeApp;

/// Retrait des séries d'un groupe de séries (#177), en points.
const GROUP_INDENT: f32 = 14.0;

#[derive(Clone, Copy)]
pub(super) enum ObjectManagerRequest {
    Select(crate::object_manager::VisibilityTarget),
    Properties(crate::object_manager::VisibilityTarget),
    Rename(stokesforge_plot::SeriesRef),
    /// Édite sur place le titre, le titre d'axe ou l'entrée de légende.
    EditText(ContextText),
}
#[derive(Clone, Copy)]
struct SeriesDrag {
    graph: NodeId,
    source: stokesforge_plot::SeriesRef,
}

/// Libellé d'un nœud d'en-tête de l'Object Manager (graphe, calque, groupes) :
/// un double-clic ouvre ses paramètres, comme sur les autres lignes (#113).
fn manager_header_label(
    ui: &mut egui::Ui,
    text: &str,
    target: crate::object_manager::VisibilityTarget,
    requests: &mut Vec<ObjectManagerRequest>,
) -> egui::Response {
    let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    // Un clic sur le nœud d'un calque (ou son groupe) le rend actif (#161).
    if response.clicked()
        && matches!(
            target,
            crate::object_manager::VisibilityTarget::Panel(_)
                | crate::object_manager::VisibilityTarget::Group(_)
                | crate::object_manager::VisibilityTarget::Axes(_)
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
) -> Option<crate::object_manager::VisibilityTarget> {
    use crate::object_manager::VisibilityTarget as T;
    view.selected_series()
        .map(T::Series)
        .or_else(|| view.selected_annotation().map(T::Annotation))
        .or_else(|| view.selected_shape().map(T::Shape))
        .or_else(|| view.selected_text().map(T::Text))
}
pub(super) fn select_manager_target(
    view: &mut GraphView,
    target: Option<crate::object_manager::VisibilityTarget>,
    spec: &stokesforge_plot::PlotSpec,
) {
    use crate::object_manager::VisibilityTarget as T;
    match target {
        Some(T::Series(t)) => view.select_series(Some(t), spec),
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

/// **Le** menu d'un objet du graphe (#152) : le même au clic droit dans la
/// figure, sur son nœud de l'Object Manager et sous Graph › Selection (qui
/// l'enveloppe dans son sous-menu, le clic droit l'affiche à plat). Sections
/// de la grammaire de `docs/menus.md`, une section sans objet pour la cible
/// étant **absente** :
///
/// 1. principale — Show Data (courbe), Edit Text / Reset Position (textes) ;
/// 2. organiser — Rename, Move Up/Down, Hide (courbe), Bring to Front /
///    Send to Back (objet), Hide (texte, axe, groupes) ;
/// 3. supprimer — Delete (courbe, texte libre, objet) ;
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
        matches!(target, T::Series(_)) || text_moved.is_some() || editable_text == Some(true),
        |ui| {
            if let T::Series(series) = target {
                if ui
                    .add_enabled(exists, egui::Button::new(tr!("action-show-graph-data")))
                    .clicked()
                {
                    actions.push(Action::ShowSeriesData { graph, series });
                    ui.close();
                }
            }
            if editable_text == Some(true)
                && ui
                    .add_enabled(exists, egui::Button::new(tr!("action-edit-text")))
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
                    .clicked()
                {
                    requests.push(ObjectManagerRequest::Rename(series));
                    ui.close();
                }
                let last = spec.panels.get(series.panel).map_or(0, |p| p.series.len());
                let moves = [
                    (
                        tr!("action-move-series-up"),
                        SHORTCUT_MOVE_SERIES_UP,
                        series.index.checked_sub(1),
                    ),
                    (
                        tr!("action-move-series-down"),
                        SHORTCUT_MOVE_SERIES_DOWN,
                        series.index.checked_add(1).filter(|i| *i < last),
                    ),
                ];
                for (label, shortcut, index) in moves {
                    if ui
                        .add_enabled(
                            index.is_some(),
                            egui::Button::new(label)
                                .shortcut_text(ui.ctx().format_shortcut(&shortcut)),
                        )
                        .clicked()
                    {
                        if let Some(index) = index {
                            actions.push(Action::ReorderSeries {
                                graph,
                                source: series,
                                index,
                            });
                        }
                        ui.close();
                    }
                }
            }
            if let T::Shape(object) = target {
                let count = spec
                    .panels
                    .get(object.panel)
                    .map_or(0, |panel| panel.shapes.len());
                for order in [
                    crate::state::ShapeOrder::ToFront,
                    crate::state::ShapeOrder::ToBack,
                ] {
                    let (label, hint, index) = match order {
                        crate::state::ShapeOrder::ToFront => (
                            tr!("action-bring-to-front"),
                            tr!("action-bring-to-front", "hint"),
                            count.saturating_sub(1),
                        ),
                        crate::state::ShapeOrder::ToBack => (
                            tr!("action-send-to-back"),
                            tr!("action-send-to-back", "hint"),
                            0,
                        ),
                    };
                    if ui
                        .add_enabled(exists, egui::Button::new(label))
                        .on_hover_text(hint)
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
            .clicked()
        {
            requests.push(ObjectManagerRequest::Properties(target));
            ui.close();
        }
    });
}

fn object_visibility(
    ui: &mut egui::Ui,
    graph: NodeId,
    target: crate::object_manager::VisibilityTarget,
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
        let mut show = settings.show_texts;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-texts"))
            .on_hover_text(tr!("object-manager-show-texts", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowTexts(show));
        }
        let mut show = settings.show_scales;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-axes"))
            .on_hover_text(tr!("object-manager-show-axes", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowScales(show));
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

/// Les mêmes réglages sous View › Object Manager, grisés quand le panneau
/// est fermé : l'enveloppe du menu du haut, le contenu étant celui du clic
/// droit sur le fond. Fonctionne aussi en fenêtre détachée.
pub(super) fn object_manager_settings_menu(
    ui: &mut egui::Ui,
    settings: ObjectManagerSettings,
    enabled: bool,
    actions: &mut Vec<Action>,
) {
    ui.add_enabled_ui(enabled, |ui| {
        object_manager_settings_contents(ui, settings, actions);
    });
}

/// Un seul contenu pour Object Manager, quel que soit son emplacement.
pub(super) fn object_manager_contents(
    ui: &mut egui::Ui,
    tree: &Tree,
    model: Option<&crate::object_manager::ObjectTree>,
    spec: Option<&stokesforge_plot::PlotSpec>,
    settings: ObjectManagerSettings,
    actions: &mut Vec<Action>,
) -> Vec<ObjectManagerRequest> {
    use crate::object_manager::VisibilityTarget as T;
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
        ui.label(tr!("object-manager-no-graph"));
        return requests;
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
        spec,
        graph: model.graph,
        focus,
        settings,
    };
    egui::ScrollArea::both().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        paint.graph(ui, model, actions, &mut requests, &mut selected);
    });
    if ui.memory(|m| m.focused()) == Some(focus) {
        if let Some(target) = selected {
            if target.removable(spec)
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            {
                actions.push(Action::RemovePlotObject {
                    graph: model.graph,
                    target,
                });
            }
            if let T::Series(series) = target {
                series_shortcuts(ui.ctx(), model.graph, series, spec, actions, &mut requests);
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
                self.object_group(
                    ui,
                    None,
                    &model.objects,
                    branch,
                    actions,
                    requests,
                    selected,
                );
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
                    // Un groupe de séries (#177) : un en-tête, puis ses membres
                    // en retrait ; une série seule reste au niveau du dessus.
                    let mut previous: Option<u32> = None;
                    for series in &layer.series {
                        if series.selected {
                            *selected = Some(T::Series(series.target));
                        }
                        let group = series.spec.group;
                        if let (Some(group), true) = (group, group != previous) {
                            let y = ui
                                .horizontal(|ui| {
                                    ui.weak(super::curve_settings::group_name(group));
                                })
                                .response
                                .rect
                                .center()
                                .y;
                            branch.mark(y);
                        }
                        previous = group;
                        let indent = if group.is_some() { GROUP_INDENT } else { 0.0 };
                        let y = self.series_row(ui, series, indent, actions, requests);
                        branch.mark(y);
                    }
                })
            });

        group_toggle.rect.center().y
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
                ui, self.graph, object, self.spec, self.focus, actions, requests,
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
        indent: f32,
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
    ) -> f32 {
        ui.push_id(series.target, |ui| {
            // #169 : une ligne de liste, fond posé sous la case et l'échantillon.
            let background = ListRowBackground::reserve(ui);
            let row = ui
                .horizontal(|ui| {
                    ui.add_space(indent);
                    object_visibility(
                        ui,
                        self.graph,
                        T::Series(series.target),
                        self.spec,
                        Shown::from(self.settings),
                        actions,
                    );
                    let (rect, swatch) = ui
                        .allocate_exact_size(egui::vec2(28.0, 18.0), egui::Sense::click_and_drag());
                    ui.painter().extend(stokesforge_render_egui::series_swatch(
                        self.tree,
                        &series.spec,
                        rect,
                    ));
                    let text = list_row_text(&series.label, series.selected)
                        .color(list_row_text_color(ui, series.selected, false));
                    // #151 : l'échantillon de légende fait partie de la cible,
                    // comme le nom. Garder l'ID du bouton pour son menu et unir
                    // les gestes avant de traiter sélection, glisser et propriétés.
                    let response = ui
                        .add(
                            list_row_label(text, series.selected)
                                .sense(egui::Sense::click_and_drag()),
                        )
                        .union(swatch);
                    if response.clicked() || response.secondary_clicked() || response.drag_started()
                    {
                        requests.push(ObjectManagerRequest::Select(T::Series(series.target)));
                        ui.memory_mut(|memory| memory.request_focus(self.focus));
                    }
                    if response.double_clicked() {
                        requests.push(ObjectManagerRequest::Properties(T::Series(series.target)));
                    }
                    response.dnd_set_drag_payload(SeriesDrag {
                        graph: self.graph,
                        source: series.target,
                    });
                    let drop_index = response
                        .dnd_hover_payload::<SeriesDrag>()
                        .filter(|drag| drag.graph == self.graph)
                        .and_then(|drag| {
                            let after = ui
                                .input(|i| i.pointer.interact_pos())
                                .is_some_and(|p| p.y > response.rect.center().y);
                            crate::object_manager::series_drop(
                                self.spec,
                                drag.source,
                                series.target,
                                after,
                            )
                        });
                    if let Some(index) = drop_index {
                        let after = ui
                            .input(|i| i.pointer.interact_pos())
                            .is_some_and(|p| p.y > response.rect.center().y);
                        let y = if after {
                            response.rect.bottom()
                        } else {
                            response.rect.top()
                        };
                        ui.painter().hline(
                            response.rect.x_range(),
                            y,
                            ui.visuals().selection.stroke,
                        );
                        if let Some(drag) = response.dnd_release_payload::<SeriesDrag>() {
                            actions.push(Action::ReorderSeries {
                                graph: self.graph,
                                source: drag.source,
                                index,
                            });
                        }
                    }
                    response.context_menu(|ui| {
                        object_node_menu(
                            ui,
                            self.graph,
                            T::Series(series.target),
                            self.spec,
                            Shown::from(self.settings),
                            actions,
                            requests,
                        )
                    });
                })
                .response
                .rect;
            background.paint(ui, row, series.selected);
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
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
        requests.push(ObjectManagerRequest::Rename(series));
    }
    for (key, index) in [
        (egui::Key::ArrowUp, series.index.checked_sub(1)),
        (egui::Key::ArrowDown, series.index.checked_add(1)),
    ] {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, key)) {
            if let Some(index) = index.filter(|i| {
                spec.panels
                    .get(series.panel)
                    .is_some_and(|p| *i < p.series.len())
            }) {
                actions.push(Action::ReorderSeries {
                    graph,
                    source: series,
                    index,
                });
            }
        }
    }
}

fn object_manager_object_row(
    ui: &mut egui::Ui,
    graph: NodeId,
    object: &crate::object_manager::ObjectRow,
    spec: &stokesforge_plot::PlotSpec,
    focus: egui::Id,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) -> f32 {
    ui.push_id(object.target, |ui| {
        // #169 : une ligne de liste, fond posé sous la case et l'icône.
        let background = ListRowBackground::reserve(ui);
        let row = ui
            .horizontal(|ui| {
                object_visibility(ui, graph, object.target, spec, Shown::TEXTS, actions);
                let icon = match object.kind {
                    crate::object_manager::ObjectKind::Text => Icon::TextTool,
                    crate::object_manager::ObjectKind::Axis => Icon::Axis,
                    crate::object_manager::ObjectKind::Shape(kind) => match kind {
                        stokesforge_plot::ShapeKind::Rectangle => Icon::ShapeRectangle,
                        stokesforge_plot::ShapeKind::Ellipse => Icon::ShapeEllipse,
                        stokesforge_plot::ShapeKind::Line => Icon::ShapeLine,
                        stokesforge_plot::ShapeKind::Arrow => Icon::ShapeArrow,
                    },
                };
                ui.add(icon_image(icon, IconRole::Neutral))
                    .on_hover_text(&object.label);
                let text = list_row_text(&object.label, object.selected)
                    .color(list_row_text_color(ui, object.selected, false));
                let response = ui.add(list_row_label(text, object.selected));
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
                        Shown::TEXTS,
                        actions,
                        requests,
                    )
                });
            })
            .response
            .rect;
        background.paint(ui, row, object.selected);
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
        let spec = graph.and_then(|graph| crate::state::current_plot_spec(&tree, graph));
        let model = graph.and_then(|graph| {
            crate::object_manager::object_tree_with_selection(
                &tree,
                graph,
                selection,
                settings.show_texts,
                settings.show_scales,
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
    let Some(spec) = crate::state::current_plot_spec(tree, graph) else {
        return;
    };
    match request {
        ObjectManagerRequest::Select(target) => select_manager_target(view, Some(target), &spec),
        ObjectManagerRequest::Rename(series) => {
            open_series_name_editor(ctx, view, tree, graph, series)
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
    use stokesforge_core::Project;
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesRef, SeriesSpec, ShapeKind,
        ShapeRef, ShapeSpec, TextTarget,
    };

    use crate::object_manager::VisibilityTarget as T;

    /// Un graphe de trois séries (« Premier », « Deuxième », « Dernier »), la
    /// première sélectionnée, et son arbre d'Object Manager.
    fn trois_series() -> (Project, NodeId, PlotSpec, crate::object_manager::ObjectTree) {
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
        let model =
            crate::object_manager::object_tree(project.tree(), graph, Some(SeriesRef::new(0, 0)))
                .unwrap();
        (project, graph, spec, model)
    }

    /// Une image de l'Object Manager sur un écran de 600 × 500 : la sortie,
    /// les actions et les demandes de l'image.
    fn image(
        ctx: &egui::Context,
        tree: &Tree,
        model: Option<&crate::object_manager::ObjectTree>,
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
            painted(&output, &tr!("object-manager-show-texts")),
            "les réglages sont au premier niveau, sans enveloppe"
        );
        assert!(painted(&output, &tr!("object-manager-reset")));
        for (key, expected) in [
            (
                tr!("object-manager-show-texts"),
                Action::SetObjectManagerShowTexts(true),
            ),
            (
                tr!("object-manager-show-axes"),
                Action::SetObjectManagerShowScales(true),
            ),
        ] {
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
            assert!(painted(&output, &tr!("object-manager-show-texts")));
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
            assert!(painted(&output, &tr!("object-manager-show-texts")));
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
        for (row, source, disabled, enabled, index) in [
            (
                "Premier",
                SeriesRef::new(0, 0),
                tr!("action-move-series-up"),
                tr!("action-move-series-down"),
                1,
            ),
            (
                "Dernier",
                SeriesRef::new(0, 2),
                tr!("action-move-series-down"),
                tr!("action-move-series-up"),
                1,
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
                vec![Action::ReorderSeries {
                    graph,
                    source,
                    index
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
        let model = crate::object_manager::object_tree_with_selection(
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
        if !model.objects.is_empty() {
            expected.push(tr!("object-manager-objects"));
        }
        expected.extend(model.objects.iter().map(|object| object.label.clone()));
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
        if !model.layers[0].objects.is_empty() {
            expected.push(tr!("object-manager-objects"));
        }
        expected.extend(
            model.layers[0]
                .objects
                .iter()
                .map(|object| object.label.clone()),
        );
        assert_eq!(
            labels(&output),
            expected,
            "les sections suivent l'ordre du graphique"
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
        assert!(texts.iter().any(|text| text == "Note libre"));
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
        assert!(texts.iter().any(|text| text == "Note libre"));
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
            show_texts: true,
            ..Default::default()
        };
        let model = crate::object_manager::object_tree_with_selection(
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
                assert!(painted(&baseline, "Texte global"));
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

    #[test]
    fn glisser_une_serie_emet_un_seul_deplacement_au_rang_final() {
        let (project, _graph, spec, model) = trois_series();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let image = |events| image(&ctx, project.tree(), Some(&model), Some(&spec), events);
        image(Vec::new());
        let (output, _, _) = image(Vec::new());
        let from = text_center(&output, "Premier");
        let to = text_center(&output, "Dernier") + egui::vec2(0.0, 7.0);
        image(vec![egui::Event::PointerMoved(from)]);
        image(vec![pointer(from, true)]);
        let (_, actions, _) = image(vec![egui::Event::PointerMoved(from + egui::vec2(0.0, 8.0))]);
        assert!(actions.is_empty(), "l'aperçu n'écrit pas le modèle");
        image(vec![egui::Event::PointerMoved(to)]);
        let (_, actions, _) = image(vec![pointer(to, false)]);
        assert_eq!(actions.len(), 1, "une commande au relâchement");
        assert!(matches!(
            actions[0],
            Action::ReorderSeries {
                source: SeriesRef { panel: 0, index: 0 },
                index: 2,
                ..
            }
        ));
        let (_, actions, _) = image(Vec::new());
        assert!(
            actions.is_empty(),
            "le dépôt ne se répète pas à l'image suivante"
        );
        let (_, actions, requests) = image(vec![key(egui::Key::F2, egui::Modifiers::NONE)]);
        assert!(actions.is_empty());
        assert!(requests.iter().any(|r| matches!(
            r,
            ObjectManagerRequest::Rename(SeriesRef { panel: 0, index: 0 })
        )));
        let (_, actions, _) = image(vec![key(egui::Key::ArrowDown, egui::Modifiers::CTRL)]);
        assert!(matches!(
            actions.as_slice(),
            [Action::ReorderSeries {
                source: SeriesRef { panel: 0, index: 0 },
                index: 1,
                ..
            }]
        ));
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

    /// #177 : les séries d'un groupe se rangent sous un en-tête « Group n » et
    /// en retrait ; une série seule reste au niveau des en-têtes de groupe.
    #[test]
    fn les_series_d_un_groupe_sont_en_retrait_sous_leur_en_tete() {
        let (project, graph, mut spec, _) = trois_series();
        spec.panels.get_mut(0).unwrap().series[1].group = Some(1);
        spec.panels.get_mut(0).unwrap().series[2].group = Some(1);
        let mut project = project;
        project
            .set_plot_spec(graph, spec.clone())
            .expect("spec avec groupe");
        let model = crate::object_manager::object_tree(project.tree(), graph, None).unwrap();
        let ctx = super::super::test_support::contexte();
        image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let (output, _, _) = image(&ctx, project.tree(), Some(&model), Some(&spec), Vec::new());
        let seule = text_center(&output, "Premier");
        let membre = text_center(&output, "Deuxième");
        let en_tete = text_center(&output, &super::super::curve_settings::group_name(1));
        assert!(
            membre.x > seule.x + GROUP_INDENT - 1.0,
            "le membre est en retrait : {membre:?} contre {seule:?}"
        );
        assert!(
            en_tete.y > seule.y && en_tete.y < membre.y,
            "l'en-tête précède ses membres"
        );
        assert!(
            text_center(&output, "Dernier").x > seule.x + GROUP_INDENT - 4.0,
            "le dernier membre aussi"
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
        for (key_name, index) in [(egui::Key::ArrowDown, Some(1)), (egui::Key::ArrowUp, None)] {
            let (_, actions, _) = image(vec![key(key_name, egui::Modifiers::CTRL)]);
            let expected: Vec<_> = index
                .into_iter()
                .map(|index| Action::ReorderSeries {
                    graph,
                    source: SeriesRef::new(0, 0),
                    index,
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
            let model = crate::object_manager::object_tree(project.tree(), graph, None);
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
                        if (rect.rect.width() - rect.rect.height()).abs() < 1.0
                            && (8.0..=24.0).contains(&rect.rect.width())
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
        let state = crate::state::AppState::new(session.handle(), String::new()).unwrap();
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

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
use crate::object_manager::{LayerRow, ObjectRow, ObjectTree, SeriesRow, VisibilityTarget as T};
use crate::preferences::{ObjectManagerGroup, ObjectManagerSettings};
use crate::state::{active_graph, Action};
use crate::tr;

use super::graph_text_edit::{open_annotation_editor, open_series_name_editor};
use super::graph_window::open_shape_properties;
use super::shortcuts::shortcut_text_delete;
use super::tab::active_tab_kind;
use super::tab::Tab;
use super::window_chrome::{panel_header, PanelPlacement, PANEL_STRIP_THICKNESS};
use super::window_detach::ObjectManagerModel;
use super::StokesForgeApp;

#[derive(Clone, Copy)]
pub(super) enum ObjectManagerRequest {
    Select(crate::object_manager::VisibilityTarget),
    Properties(crate::object_manager::VisibilityTarget),
    Rename(stokesforge_plot::SeriesRef),
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
        _ => {
            view.select_annotation(None, spec);
        }
    }
}

/// Le grisage et les Action sont identiques dans Graph et au clic droit.
pub(super) fn object_node_menu(
    ui: &mut egui::Ui,
    graph: NodeId,
    target: crate::object_manager::VisibilityTarget,
    spec: &stokesforge_plot::PlotSpec,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    use crate::object_manager::VisibilityTarget as T;
    if ui
        .add_enabled(
            target.exists(spec),
            egui::Button::new(tr!("action-properties")),
        )
        .clicked()
    {
        requests.push(ObjectManagerRequest::Properties(target));
        ui.close();
    }
    if let T::Series(series) = target {
        if ui
            .add_enabled(
                target.exists(spec),
                egui::Button::new(tr!("action-rename-series")).shortcut_text("F2"),
            )
            .clicked()
        {
            requests.push(ObjectManagerRequest::Rename(series));
            ui.close();
        }
        let moves = [
            (
                tr!("action-move-series-up"),
                "Ctrl+Up",
                series.index.checked_sub(1),
            ),
            (
                tr!("action-move-series-down"),
                "Ctrl+Down",
                series.index.checked_add(1).filter(|i| {
                    spec.panels
                        .get(series.panel)
                        .is_some_and(|p| *i < p.series.len())
                }),
            ),
        ];
        ui.menu_button(tr!("menu-order"), |ui| {
            for (label, shortcut, index) in moves {
                if ui
                    .add_enabled(
                        index.is_some(),
                        egui::Button::new(label).shortcut_text(shortcut),
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
        });
    }
    let visibility = target.visibility(spec);
    if ui
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
    if ui
        .add_enabled(
            target.removable(spec),
            egui::Button::new(tr!("action-delete-content")).shortcut_text(shortcut_text_delete()),
        )
        .clicked()
    {
        actions.push(Action::RemovePlotObject { graph, target });
        ui.close();
    }
}

fn object_visibility(
    ui: &mut egui::Ui,
    graph: NodeId,
    target: crate::object_manager::VisibilityTarget,
    spec: &stokesforge_plot::PlotSpec,
    actions: &mut Vec<Action>,
) {
    let state = target.visibility(spec);
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

/// Même menu depuis Affichage et le fond, y compris en fenêtre détachée.
/// Les réglages ne touchent ni le PlotSpec ni les replis courants d'egui.
pub(super) fn object_manager_settings_menu(
    ui: &mut egui::Ui,
    settings: ObjectManagerSettings,
    enabled: bool,
    actions: &mut Vec<Action>,
) {
    ui.add_enabled_ui(enabled, |ui| {
        let config = egui::containers::menu::MenuConfig::new()
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        egui::containers::menu::SubMenuButton::from_button(
            egui::Button::image_and_text(
                icon_image(Icon::Properties, IconRole::Neutral),
                tr!("object-manager-settings"),
            )
            .right_text(egui::containers::menu::SubMenuButton::RIGHT_ARROW),
        )
        .config(config.clone())
        .ui(ui, |ui| {
            egui::containers::menu::SubMenuButton::new(tr!("object-manager-display"))
                .config(config.clone())
                .ui(ui, |ui| {
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
                        .checkbox(&mut show, tr!("object-manager-show-scales"))
                        .on_hover_text(tr!("object-manager-show-scales", "hint"))
                        .changed()
                    {
                        actions.push(Action::SetObjectManagerShowScales(show));
                    }
                });
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
            ui.separator();
            if ui
                .add(egui::Button::image_and_text(
                    icon_image(Icon::ResetZoom, IconRole::Neutral),
                    tr!("object-manager-reset"),
                ))
                .on_hover_text(tr!("object-manager-reset", "hint"))
                .clicked()
            {
                actions.push(Action::ResetObjectManagerSettings);
                ui.close();
            }
        });
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
        .show(|ui| object_manager_settings_menu(ui, settings, true, actions));
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
        object_visibility(ui, self.graph, target, self.spec, actions);
        let response = manager_header_label(ui, text, target, requests);
        response.context_menu(|ui| {
            object_node_menu(ui, self.graph, target, self.spec, actions, requests)
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
                    for series in &layer.series {
                        if series.selected {
                            *selected = Some(T::Series(series.target));
                        }
                        let y = self.series_row(ui, series, actions, requests);
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
        actions: &mut Vec<Action>,
        requests: &mut Vec<ObjectManagerRequest>,
    ) -> f32 {
        ui.push_id(series.target, |ui| {
            ui.horizontal(|ui| {
                object_visibility(ui, self.graph, T::Series(series.target), self.spec, actions);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(28.0, 18.0), egui::Sense::hover());
                ui.painter().extend(stokesforge_render_egui::series_swatch(
                    self.tree,
                    &series.spec,
                    rect,
                ));
                let text = egui::RichText::new(&series.label);
                let text = if series.selected { text.strong() } else { text };
                let response = ui.add(
                    egui::Button::selectable(series.selected, text)
                        .sense(egui::Sense::click_and_drag()),
                );
                if response.clicked() || response.secondary_clicked() || response.drag_started() {
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
                    ui.painter()
                        .hline(response.rect.x_range(), y, ui.visuals().selection.stroke);
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
                        actions,
                        requests,
                    )
                });
            })
            .response
            .rect
            .center()
            .y
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
        ui.horizontal(|ui| {
            object_visibility(ui, graph, object.target, spec, actions);
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
            let text = egui::RichText::new(&object.label);
            let text = if object.selected { text.strong() } else { text };
            let response = ui.selectable_label(object.selected, text);
            if response.clicked() || response.secondary_clicked() {
                requests.push(ObjectManagerRequest::Select(object.target));
                ui.memory_mut(|m| m.request_focus(focus));
            }
            if response.double_clicked() {
                requests.push(ObjectManagerRequest::Properties(object.target));
            }
            response.context_menu(|ui| {
                object_node_menu(ui, graph, object.target, spec, actions, requests)
            });
        })
        .response
        .rect
        .center()
        .y
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
        use crate::object_manager::VisibilityTarget as T;
        let tree = self.state.tree().clone();
        let Some(spec) = crate::state::current_plot_spec(&tree, graph) else {
            return;
        };
        self.with_graph_view(graph, |view| match request {
            ObjectManagerRequest::Select(target) => {
                select_manager_target(view, Some(target), &spec)
            }
            ObjectManagerRequest::Rename(series) => {
                open_series_name_editor(ctx, view, &tree, graph, series)
            }
            ObjectManagerRequest::Properties(target) => {
                select_manager_target(view, Some(target), &spec);
                match target {
                    T::Annotation(t) => open_annotation_editor(ctx, view, &tree, graph, t),
                    T::Shape(t) => open_shape_properties(ctx, view, &spec, t),
                    _ => actions.push(Action::OpenObjectProperties { graph, target }),
                }
            }
        });
        ctx.request_repaint();
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
        assert!(painted(&output, &tr!("object-manager-settings")));
        assert!(!painted(&output, &tr!("object-manager-show-texts")));
        clic_menu(
            &paint,
            text_center(&output, &tr!("object-manager-settings")),
            egui::PointerButton::Primary,
        );
        let output = paint(Vec::new()).0;
        clic_menu(
            &paint,
            text_center(&output, &tr!("object-manager-display")),
            egui::PointerButton::Primary,
        );
        paint(Vec::new());
        for (key, expected) in [
            (
                tr!("object-manager-show-texts"),
                Action::SetObjectManagerShowTexts(true),
            ),
            (
                tr!("object-manager-show-scales"),
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
                painted(&output, &tr!("object-manager-settings")),
                "le menu racine reste ouvert"
            );
            assert!(painted(&output, &tr!("object-manager-show-texts")));
            assert!(painted(&output, &tr!("object-manager-show-scales")));
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
        for label in [
            tr!("object-manager-settings"),
            tr!("object-manager-default-expansion"),
        ] {
            let output = paint(Vec::new()).0;
            clic_menu(
                &paint,
                text_center(&output, &label),
                egui::PointerButton::Primary,
            );
            paint(Vec::new());
        }
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
            assert!(painted(&output, &tr!("object-manager-settings")));
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
        assert!(!painted(&output, &tr!("object-manager-settings")));
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
                !painted(&output, &enabled),
                "les déplacements sont dans Ordre"
            );
            clic_menu(
                &paint,
                text_center(&output, &tr!("menu-order")),
                egui::PointerButton::Primary,
            );
            paint(Vec::new());
            let output = paint(Vec::new()).0;
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
                let output = paint(Vec::new()).0;
                clic_menu(
                    &paint,
                    text_center(&output, &tr!("menu-order")),
                    egui::PointerButton::Primary,
                );
                paint(Vec::new());
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
            assert!(!painted(&output, &tr!("menu-order")));
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
        let annotation = AnnotationRef {
            row: 0,
            column: 0,
            index: 0,
        };
        let shape = ShapeRef {
            row: 0,
            column: 0,
            index: 0,
        };
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

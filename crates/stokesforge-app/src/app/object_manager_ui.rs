//! Object Manager peint (#81–#85, #102, #111, #113) : l'arbre de
//! [`crate::object_manager::ObjectTree`] en lignes, cases de visibilité, menus
//! contextuels, glisser des séries et raccourcis clavier (Suppr, F2,
//! Ctrl+flèches). Le modèle (projection du `PlotSpec`, filtres) reste dans
//! `crate::object_manager` ; la sélection vit dans la `GraphView`. Ce qui
//! touche la vue (sélection, propriétés, renommage) remonte en
//! [`ObjectManagerRequest`], exécuté par `StokesForgeApp` après la peinture.
//! Une seule écriture pour le panneau fixe et la fenêtre détachée.
//!
//! Sorti d'`app.rs` tel quel (#121).

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::GraphView;

use crate::icons::{icon_image, Icon, IconRole};
use crate::state::Action;
use crate::tr;

use super::shortcuts::shortcut_text_delete;

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
        for (label, shortcut, index) in [
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
        ] {
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

/// Un seul contenu pour Object Manager, quel que soit son emplacement.
pub(super) fn object_manager_contents(
    ui: &mut egui::Ui,
    tree: &Tree,
    model: Option<&crate::object_manager::ObjectTree>,
    spec: Option<&stokesforge_plot::PlotSpec>,
    show_texts: bool,
    show_scales: bool,
    actions: &mut Vec<Action>,
) -> Vec<ObjectManagerRequest> {
    use crate::object_manager::VisibilityTarget as T;
    let mut requests = Vec::new();
    let (Some(model), Some(spec)) = (model, spec) else {
        ui.label(tr!("object-manager-no-graph"));
        return requests;
    };
    let focus = ui.make_persistent_id(("object_manager_keys", model.graph));
    let mut selected = None;
    // Fond du panneau, posé sous les lignes : son menu contextuel porte la
    // bascule des textes automatiques (même action que le menu View).
    let background = ui.interact(
        ui.available_rect_before_wrap(),
        ui.make_persistent_id(("object_manager_background", model.graph)),
        egui::Sense::click(),
    );
    background.context_menu(|ui| {
        let mut show = show_texts;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-texts"))
            .on_hover_text(tr!("object-manager-show-texts", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowTexts(show));
            ui.close();
        }
        let mut show = show_scales;
        if ui
            .checkbox(&mut show, tr!("object-manager-show-scales"))
            .on_hover_text(tr!("object-manager-show-scales", "hint"))
            .changed()
        {
            actions.push(Action::SetObjectManagerShowScales(show));
            ui.close();
        }
    });
    egui::ScrollArea::both().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        let id = ui.make_persistent_id(("object_manager_graph", model.graph));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                object_visibility(ui, model.graph, T::Graph, spec, actions);
                let response = manager_header_label(ui, &model.title, T::Graph, &mut requests);
                response.context_menu(|ui| {
                    object_node_menu(ui, model.graph, T::Graph, spec, actions, &mut requests)
                });
            })
            .body(|ui| crate::tree_lines::tree_children(ui, |ui, branch| {
                for object in &model.objects {
                    let y = object_manager_object_row(
                        ui,
                        model.graph,
                        object,
                        spec,
                        focus,
                        actions,
                        &mut requests,
                    );
                    branch.mark(y);
                    if object.selected {
                        selected = Some(object.target);
                    }
                }
                for layer in &model.layers {
                    let id = ui.make_persistent_id((model.graph, layer.index));
                    let (layer_toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        id,
                        true,
                    )
                    .show_header(ui, |ui| {
                        object_visibility(ui, model.graph, T::Panel(layer.index), spec, actions);
                        manager_header_label(
                            ui,
                            &tr!("object-manager-layer", index = (layer.index + 1)),
                            T::Panel(layer.index),
                            &mut requests,
                        )
                            .context_menu(|ui| {
                                object_node_menu(
                                    ui,
                                    model.graph,
                                    T::Panel(layer.index),
                                    spec,
                                    actions,
                                    &mut requests,
                                )
                            });
                    })
                    .body(|ui| crate::tree_lines::tree_children(ui, |ui, branch| {
                        let id = ui.make_persistent_id((model.graph, layer.index, "group"));
                        let (group_toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            id,
                            true,
                        )
                        .show_header(ui, |ui| {
                            object_visibility(
                                ui,
                                model.graph,
                                T::Group(layer.index),
                                spec,
                                actions,
                            );
                            manager_header_label(
                                ui,
                                &tr!("object-manager-group"),
                                T::Group(layer.index),
                                &mut requests,
                            )
                            .context_menu(|ui| {
                                object_node_menu(
                                    ui,
                                    model.graph,
                                    T::Group(layer.index),
                                    spec,
                                    actions,
                                    &mut requests,
                                )
                            });
                        })
                        .body(|ui| crate::tree_lines::tree_children(ui, |ui, branch| {
                            for series in &layer.series {
                                if series.selected {
                                    selected = Some(T::Series(series.target));
                                }
                                let y = ui.push_id(series.target, |ui| {
                                    ui.horizontal(|ui| {
                                        object_visibility(
                                            ui,
                                            model.graph,
                                            T::Series(series.target),
                                            spec,
                                            actions,
                                        );
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(28.0, 18.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().extend(
                                            stokesforge_render_egui::series_swatch(
                                                tree,
                                                &series.spec,
                                                rect,
                                            ),
                                        );
                                        let text = egui::RichText::new(&series.label);
                                        let text =
                                            if series.selected { text.strong() } else { text };
                                        let response = ui.add(
                                            egui::Button::selectable(series.selected, text)
                                                .sense(egui::Sense::click_and_drag()),
                                        );
                                        if response.clicked()
                                            || response.secondary_clicked()
                                            || response.drag_started()
                                        {
                                            requests.push(ObjectManagerRequest::Select(T::Series(
                                                series.target,
                                            )));
                                            ui.memory_mut(|memory| memory.request_focus(focus));
                                        }
                                        if response.double_clicked() {
                                            requests.push(ObjectManagerRequest::Properties(
                                                T::Series(series.target),
                                            ));
                                        }
                                        response.dnd_set_drag_payload(SeriesDrag {
                                            graph: model.graph,
                                            source: series.target,
                                        });
                                        let drop_index = response
                                            .dnd_hover_payload::<SeriesDrag>()
                                            .filter(|drag| drag.graph == model.graph)
                                            .and_then(|drag| {
                                                let after = ui
                                                    .input(|i| i.pointer.interact_pos())
                                                    .is_some_and(|p| {
                                                        p.y > response.rect.center().y
                                                    });
                                                crate::object_manager::series_drop(
                                                    spec,
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
                                            if let Some(drag) =
                                                response.dnd_release_payload::<SeriesDrag>()
                                            {
                                                actions.push(Action::ReorderSeries {
                                                    graph: model.graph,
                                                    source: drag.source,
                                                    index,
                                                });
                                            }
                                        }
                                        response.context_menu(|ui| {
                                            object_node_menu(
                                                ui,
                                                model.graph,
                                                T::Series(series.target),
                                                spec,
                                                actions,
                                                &mut requests,
                                            )
                                        });
                                    })
                                    .response
                                    .rect
                                    .center()
                                    .y
                                })
                                .inner;
                                branch.mark(y);
                            }
                        }));
                        branch.mark(group_toggle.rect.center().y);
                        if !layer.axes.is_empty() {
                            let id = ui.make_persistent_id((model.graph, layer.index, "axes"));
                            let (axes_toggle, _, _) =
                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                    ui.ctx(),
                                    id,
                                    true,
                                )
                                .show_header(ui, |ui| {
                                    object_visibility(
                                        ui,
                                        model.graph,
                                        T::Axes(layer.index),
                                        spec,
                                        actions,
                                    );
                                    manager_header_label(
                                        ui,
                                        &tr!("object-manager-axes"),
                                        T::Axes(layer.index),
                                        &mut requests,
                                    )
                                    .context_menu(|ui| {
                                        object_node_menu(
                                            ui,
                                            model.graph,
                                            T::Axes(layer.index),
                                            spec,
                                            actions,
                                            &mut requests,
                                        )
                                    });
                                })
                                .body(|ui| {
                                    crate::tree_lines::tree_children(ui, |ui, branch| {
                                        for object in &layer.axes {
                                            let y = object_manager_object_row(
                                                ui,
                                                model.graph,
                                                object,
                                                spec,
                                                focus,
                                                actions,
                                                &mut requests,
                                            );
                                            branch.mark(y);
                                            if object.selected {
                                                selected = Some(object.target);
                                            }
                                        }
                                    })
                                });
                            branch.mark(axes_toggle.rect.center().y);
                        }
                        for object in &layer.objects {
                            let y = object_manager_object_row(
                                ui,
                                model.graph,
                                object,
                                spec,
                                focus,
                                actions,
                                &mut requests,
                            );
                            branch.mark(y);
                            if object.selected {
                                selected = Some(object.target);
                            }
                        }
                    }));
                    branch.mark(layer_toggle.rect.center().y);
                }
            }));
    });
    // L'ancrage appartient au panneau ; aucun champ d'édition ne perd ses touches.
    ui.interact(
        egui::Rect::NOTHING,
        focus,
        egui::Sense::focusable_noninteractive(),
    );
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

#[cfg(test)]
mod tests {
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
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 500.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                requests =
                    object_manager_contents(ui, tree, model, spec, false, false, &mut actions);
            },
        );
        output.textures_delta.clear();
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

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
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

        // Le focus est pris au début d'un glisser sur la ligne (relâché sur
        // elle-même : aucun déplacement), comme dans le test du glisser. Un
        // simple clic le demande aussi, mais dans l'image du clic l'ancre du
        // panneau, jamais survolée, le rend aussitôt (egui lâche au clic le
        // focus d'un widget non survolé) : ce chemin-là n'est pas éprouvé ici.
        let row = text_center(&output, "Premier");
        let nudged = row + egui::vec2(0.0, 8.0);
        image(vec![egui::Event::PointerMoved(row)]);
        image(vec![pointer(row, true)]);
        let (_, _, requests) = image(vec![egui::Event::PointerMoved(nudged)]);
        assert!(requests
            .iter()
            .any(|r| matches!(r, ObjectManagerRequest::Select(T::Series(s)) if *s == SeriesRef::new(0, 0))));
        let (_, actions, _) = image(vec![pointer(nudged, false)]);
        assert!(
            actions.is_empty(),
            "relâchée sur elle-même, la série ne bouge pas"
        );
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

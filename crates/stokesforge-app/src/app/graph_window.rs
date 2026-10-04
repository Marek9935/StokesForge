//! Fenêtre de graphe : la figure ([`graph_view_contents`], une seule écriture
//! pour la fenêtre flottante et la fenêtre détachée), la zone de dépôt des
//! colonnes ([`graph_drop_target`]), le menu contextuel de la figure, les
//! signaux de la vue ([`annotation_interactions`] : éditeurs sur place,
//! glisser, Suppr), l'ordre et les propriétés des objets graphiques, et la
//! demande d'export ([`request_graph_export`]). Les éditeurs sur place eux-mêmes
//! vivent dans `graph_text_edit.rs`.
//!
//! Sorti d'`app.rs` tel quel (#121).

use std::collections::HashMap;

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::{
    AxisSide, GraphView, PlotComponent, TextTarget, DEFAULT_FIGURE_SIZE,
};
use stokesforge_session::Snapshot;

use crate::color_picker::{color_field, ColorUsage, PickerEnv};
use crate::state::{
    drop_payload_action, export_file_name, Action, AppState, ColumnDragPayload, PlotAxis,
    PlotTarget, ShapeOrder,
};
use crate::theme;
use crate::tr;

use super::dialogs::{modal_dialog, DialogOutcome};
use super::graph_text_edit::{
    add_annotation_at, annotation_editor_id, annotation_editor_ui, open_annotation_editor,
    open_legend_entry_editor, open_series_name_editor, open_text_editor, text_editor_id,
    text_editor_ui, AnnotationEditor, ContextText, TextEditor,
};
use super::object_manager_ui::{series_shortcuts, ObjectManagerRequest};
use super::shortcuts::{shortcut_text_delete, SHORTCUT_EXPORT_GRAPH};
use super::{color_request_action, keyboard_focus_outside_grid, saved_colors_id};

/// Hauteur du seul contrôle conservé sous une figure : le retour au cadrage
/// initial. Réserver sa hauteur garde le rectangle de la figure stable d'une
/// image à l'autre — sans quoi le cache de formes serait invalidé au moindre
/// survol.
pub(super) const GRAPH_CONTROLS_HEIGHT: f32 = 28.0;

/// Enrobe [`graph_contents`] d'une zone de dépôt pour le glisser d'une
/// sélection de colonnes de classeur — pendant, côté cible, de la zone de
/// prise posée par [`grid_contents`] en bord de grille. Appelée par
/// [`tab_contents`], donc identiquement pour un onglet en fenêtre flottante
/// et pour un onglet détaché ([`StokesForgeApp::show_detached_tabs`]) : c'est le
/// seul point
/// d'entrée que les deux utilisent, voir la doc de tête de ce dernier — pas
/// de code de dépôt séparé à maintenir pour le cas détaché.
///
/// Un survol avec une [`ColumnDragPayload`] active dessine un contour sobre
/// autour du graphe (teinte de sélection de `theme`, pas de palette
/// inventée) ; un relâchement au-dessus pousse
/// [`crate::state::drop_payload_action`] dans `actions`, converti en
/// [`Action::AddSeriesToGraph`]. Relâcher ailleurs qu'au-dessus d'un graphe
/// ne pousse rien : `dnd_release_payload` ne répond que si le pointeur est
/// contenu dans ce rectangle au moment du relâchement.
pub(super) fn graph_drop_target(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let tab_rect = ui.max_rect();
    // Une `Response` couvrant tout l'onglet, même avec `Sense::hover()`,
    // participe à la sélection de cible d'egui et gagne le glissé principal
    // sur le `GraphView`. C'était la différence avec la fenêtre détachée,
    // qui appelle directement `graph_view_contents`, et la cause du pan
    // inopérant dans la fenêtre principale. La cible de dépôt n'a pas besoin
    // d'être un widget : lire le pointeur et la charge d'egui suffit.
    let pointer_in_tab = ui.rect_contains_pointer(tab_rect);
    let hovering =
        pointer_in_tab && egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ui.ctx());
    if pointer_in_tab && ui.input(|input| input.pointer.any_released()) {
        if let Some(payload) = egui::DragAndDrop::take_payload::<ColumnDragPayload>(ui.ctx()) {
            actions.push(drop_payload_action(&payload, graph_id));
        }
    }
    graph_contents(ui, state, actions, graph_views, graph_id);
    if hovering {
        ui.painter().rect_stroke(
            tab_rect,
            4.0,
            egui::Stroke::new(2.5, theme::header_letter_row_selected_fill()),
            egui::StrokeKind::Inside,
        );
    }
}

/// Ouvre le dialogue des propriétés d'un objet graphique : l'Object Manager
/// (double-clic sur sa ligne) et la figure (double-clic sur l'objet) y vont
/// par ici (#113).
pub(super) fn open_shape_properties(
    ctx: &egui::Context,
    view: &mut GraphView,
    spec: &stokesforge_plot::PlotSpec,
    target: stokesforge_plot::ShapeRef,
) {
    view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
    view.select_shape(Some(target));
    if let Some(draft) = target.get(spec).cloned() {
        ctx.data_mut(|data| {
            data.insert_temp(
                view.id().with("shape_properties"),
                ShapeProperties { target, draft },
            )
        });
    }
}

#[derive(Clone)]
struct ShapeProperties {
    target: stokesforge_plot::ShapeRef,
    draft: stokesforge_plot::ShapeSpec,
}

/// Brouillon seulement : Valider émet une action, Annuler ne touche rien.
fn shape_properties_ui(
    ctx: &egui::Context,
    view: &GraphView,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    let id = view.id().with("shape_properties");
    let Some(mut edit) = ctx.data(|data| data.get_temp::<ShapeProperties>(id)) else {
        return;
    };
    let (project, user): (stokesforge_plot::SavedColors, stokesforge_plot::SavedColors) = ctx
        .data(|data| data.get_temp(saved_colors_id()))
        .unwrap_or_default();
    let mut env = PickerEnv {
        project,
        user,
        requests: Vec::new(),
    };
    let outcome = modal_dialog(
        ctx,
        id.with("dialog"),
        &tr!("object-properties-title"),
        egui::vec2(360.0, 280.0),
        |ui| {
            for (coordinate, value) in [
                ("X1", &mut edit.draft.x1),
                ("Y1", &mut edit.draft.y1),
                ("X2", &mut edit.draft.x2),
                ("Y2", &mut edit.draft.y2),
            ] {
                ui.horizontal(|ui| {
                    ui.label(tr!("object-coordinate", coordinate = coordinate));
                    ui.add(egui::DragValue::new(value).speed(0.1));
                });
            }
            color_field(
                ui,
                &tr!("format-object-line-color"),
                &mut edit.draft.line_color,
                ColorUsage::Curves,
                Some(&tr!("format-object-line-color", "hint")),
                &mut env,
            );
            if edit.draft.kind.is_closed() {
                color_field(
                    ui,
                    &tr!("format-object-fill-color"),
                    &mut edit.draft.fill_color,
                    ColorUsage::Background,
                    Some(&tr!("format-object-fill-color", "hint")),
                    &mut env,
                );
            }
            ui.horizontal(|ui| {
                ui.label(tr!("format-object-line-width"));
                ui.add(
                    egui::DragValue::new(&mut edit.draft.line_width)
                        .range(0.0..=100.0)
                        .speed(0.1),
                );
            });
        },
    );
    actions.extend(env.requests.into_iter().map(color_request_action));
    match outcome {
        DialogOutcome::Accept => {
            actions.push(Action::SetShape {
                graph,
                target: edit.target,
                shape: Box::new(edit.draft),
            });
            ctx.data_mut(|data| data.remove::<ShapeProperties>(id));
        }
        DialogOutcome::Cancel => {
            ctx.data_mut(|data| data.remove::<ShapeProperties>(id));
        }
        DialogOutcome::Open => {
            ctx.data_mut(|data| data.insert_temp(id, edit));
        }
    }
}

/// Un graphe ouvert — le **rendu visible** de la phase R4 (tâche 4.6).
/// Contenu d'un [`Tab::Graph`].
///
/// Ce n'est pas une ébauche de la fenêtre de graphe finale : comme tout ce
/// banc, c'est la preuve que l'intégration bout en bout tient. Ce qu'il
/// montre vraiment, et qu'aucun test ne montre :
///
/// - la figure elle-même, avec ses vraies polices et son vrai HiDPI ;
/// - le retour au cadrage initial, pour abandonner un zoom ou un pan sans
///   modifier les données ni la description du graphe.
pub(super) fn graph_contents(
    ui: &mut egui::Ui,
    state: &AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    graph_id: NodeId,
) {
    let view = graph_views.entry(graph_id).or_default();
    graph_view_contents(ui, view, state.snapshot(), graph_id, actions);
}

/// Corps de [`graph_contents`], factorisé sur une [`GraphView`] et un
/// [`Snapshot`] déjà en main plutôt que sur `&AppState`/la table
/// [`StokesForgeApp::graph_views`] : c'est ce qui permet à
/// [`StokesForgeApp::show_detached_tabs`] de peindre un `Tab::Graph` détaché avec
/// **exactement** cette fonction — pas de version dégradée, voir la doc de
/// tête du module. `GraphView` n'a pas le problème de verrou global à
/// `AppState` qu'ont les tampons de saisie de cellule/note (son cache et sa
/// navigation sont déjà scopés par graphe) : nul besoin d'une variante
/// « détachée » séparée, à la différence de [`grid_contents`]/
/// [`note_contents`].
pub(super) fn graph_view_contents(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph_id: NodeId,
    actions: &mut Vec<Action>,
) {
    let available = ui.available_size_before_wrap();
    // La figure prend tout ce qui reste après ses contrôles. Réserver leur
    // hauteur complète évite le même mécanisme d'auto-agrandissement qui a
    // déjà touché les fenêtres de classeur : un dépassement ferait changer
    // `rect` à chaque image, donc provoquerait un raté de cache au survol.
    let (outer, _) = ui.allocate_exact_size(
        egui::vec2(available.x, (available.y - GRAPH_CONTROLS_HEIGHT).max(0.0)),
        egui::Sense::hover(),
    );
    // Le rectangle affiché suit l'espace disponible (F11) avec le ratio de
    // `DEFAULT_FIGURE_SIZE` : mise à l'échelle « contain », bornée par la
    // dimension limitante. `paint_scaled` garde la disposition à 480×360 et
    // agrandit ou réduit ensuite les formes, texte compris : aucune
    // graduation n'est recalculée pour la taille de la fenêtre (point 3 de
    // `retours-2026-09-24-bugs.md`). Le sous-Ui borne les
    // interactions sans faire grandir la fenêtre parente.
    let (fig_w, fig_h) = DEFAULT_FIGURE_SIZE;
    let scale = if fig_w > 0 && fig_h > 0 {
        (outer.width() / fig_w as f32)
            .min(outer.height() / fig_h as f32)
            .max(0.0)
    } else {
        0.0
    };
    let rect = egui::Rect::from_center_size(
        outer.center(),
        egui::vec2(fig_w as f32 * scale, fig_h as f32 * scale),
    );
    let mut figure_ui = ui.new_child(egui::UiBuilder::new().max_rect(outer));
    figure_ui.set_clip_rect(ui.clip_rect().intersect(outer));
    // Un fond transparent (« Aucune » couleur, ou opacité partielle) se voit
    // sur du blanc : sans lui, la figure prendrait la couleur de la fenêtre,
    // qui n'est pas celle de l'export. Ce blanc n'est peint **qu'ici**, sous
    // la figure, jamais dans `stokesforge-render` : l'export SVG/PNG/PDF d'un
    // graphe sans fond reste transparent (invariant 5, lu dans l'autre sens ;
    // test `un_fond_sans_couleur_donne_un_png_transparent`).
    let background = crate::state::current_plot_spec(&snapshot.tree, graph_id)
        .map(|spec| spec.background)
        .unwrap_or_default();
    if let Some(backdrop) = display_backdrop(&background) {
        figure_ui.painter().rect_filled(rect, 0.0, backdrop);
    }
    match view.paint_scaled(&figure_ui, rect, snapshot, graph_id) {
        Ok(_) => {
            graph_context_menu(ui, view, &snapshot.tree, graph_id, actions);
            annotation_interactions(ui, view, &snapshot.tree, graph_id, actions);
            if !keyboard_focus_outside_grid(ui.ctx()) {
                if let Some(spec) = crate::state::current_plot_spec(&snapshot.tree, graph_id) {
                    if let Some(series) = view.selected_series() {
                        let mut requests = Vec::new();
                        series_shortcuts(ui.ctx(), graph_id, series, &spec, actions, &mut requests);
                        for request in requests {
                            if let ObjectManagerRequest::Rename(series) = request {
                                open_series_name_editor(
                                    ui.ctx(),
                                    view,
                                    &snapshot.tree,
                                    graph_id,
                                    series,
                                );
                            }
                        }
                        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
                        {
                            actions.push(Action::RemovePlotObject {
                                graph: graph_id,
                                target: crate::object_manager::VisibilityTarget::Series(series),
                            });
                        }
                    }
                }
            }
            shape_properties_ui(ui.ctx(), view, graph_id, actions);
            if ui.small_button(tr!("plot-view-reset-zoom")).clicked() {
                view.reset_view();
            }
        }
        Err(err) => {
            ui.colored_label(
                egui::Color32::RED,
                crate::errors::render::error_message(&err),
            );
        }
    }
}

/// Blanc d'affichage sous une figure dont le fond est (semi-)transparent —
/// `None` sous un fond opaque, qui couvre déjà tout. Le fond transparent
/// est donc vu composé sur du blanc, comme l'encre automatique le juge
/// (`stokesforge_render::color::over_white`).
pub(super) fn display_backdrop(background: &str) -> Option<egui::Color32> {
    crate::color_picker::parse_hex(background)
        .is_some_and(|[_, _, _, alpha]| alpha < 255)
        .then_some(egui::Color32::WHITE)
}

/// Menu contextuel de la figure : un clic droit (sans glisser — le glisser
/// secondaire reste le zoom élastique de `stokesforge-render-egui`) sur un
/// composant du graphe propose ses propriétés, qui ouvrent la fenêtre des
/// propriétés sur l'onglet de ce composant ([`PlotTarget::settings_page`]).
/// Le composant est lu **au moment du clic** et mémorisé : le menu reste
/// ouvert plusieurs images, pendant lesquelles le pointeur bouge.
fn graph_context_menu(
    ui: &egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph_id: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(response) = view.response().cloned() else {
        return;
    };
    let target_id = response.id.with("plot_target");
    let point_id = response.id.with("plot_point");
    let text_id = response.id.with("plot_text");
    if response.secondary_clicked() {
        let pos = response.interact_pointer_pos();
        let target = pos.map_or(PlotTarget::Background, |pos| {
            view.annotation_at(pos).map_or_else(
                || {
                    view.shape_at(pos)
                        .map_or_else(|| plot_target(view.component_at(pos)), PlotTarget::Object)
                },
                PlotTarget::Annotation,
            )
        });
        let point = pos.and_then(|pos| view.data_at(pos));
        // Titre, titre d'axe, légende ou entrée de légende visé (issue #53) :
        // lu au clic, comme la cible, pour que le menu reste cohérent pendant
        // que le pointeur bouge.
        let text = pos.and_then(|pos| match view.component_at(pos) {
            Some(PlotComponent::Text(target)) => Some(ContextText::Title(target)),
            Some(PlotComponent::LegendEntry { panel, series }) => view
                .last_drawn()
                .and_then(|last| last.panels.get(panel))
                .map(|panel| ContextText::LegendEntry {
                    row: panel.row,
                    column: panel.column,
                    series,
                }),
            _ => None,
        });
        ui.memory_mut(|memory| {
            memory.data.insert_temp(target_id, target);
            memory.data.insert_temp(point_id, point);
            memory.data.insert_temp(text_id, text);
        });
    }
    response.context_menu(|ui| {
        let target: PlotTarget = ui
            .memory(|memory| memory.data.get_temp(target_id))
            .unwrap_or(PlotTarget::Background);
        let point: Option<(usize, usize, f64, f64)> =
            ui.memory(|memory| memory.data.get_temp(point_id)).flatten();
        let text: Option<ContextText> = ui.memory(|memory| memory.data.get_temp(text_id)).flatten();
        ui.weak(target.label());
        ui.separator();
        // Titres, légende et entrées de légende (issue #53) : modifier le
        // texte, remettre à sa place — le pendant du double-clic et du
        // glisser, et du menu *Graph* pour la remise à zéro générale.
        if let Some(text) = text {
            let editable = !matches!(text, ContextText::Title(TextTarget::Legend { .. }));
            if editable && ui.button(tr!("action-edit-text")).clicked() {
                match text {
                    ContextText::Title(target) => {
                        open_text_editor(ui.ctx(), view, tree, graph_id, target)
                    }
                    ContextText::LegendEntry {
                        row,
                        column,
                        series,
                    } => open_legend_entry_editor(
                        ui.ctx(),
                        view,
                        tree,
                        graph_id,
                        row,
                        column,
                        series,
                    ),
                }
                ui.close();
            }
            if let ContextText::Title(target) = text {
                let moved = crate::state::current_plot_spec(tree, graph_id)
                    .is_some_and(|spec| target.anchor(&spec).is_some());
                if ui
                    .add_enabled(moved, egui::Button::new(tr!("action-reset-text-position")))
                    .on_hover_text(tr!("action-reset-text-position", "hint"))
                    .clicked()
                {
                    actions.push(Action::SetTextAnchor {
                        graph: graph_id,
                        target,
                        anchor: None,
                    });
                    ui.close();
                }
            }
            ui.separator();
        }
        // Textes libres (issue #35) : les mêmes gestes que la barre de menus
        // et le clavier (double-clic, Suppr).
        if let PlotTarget::Annotation(annotation) = target {
            if ui.button(tr!("action-edit-text")).clicked() {
                open_annotation_editor(ui.ctx(), view, tree, graph_id, annotation);
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new(tr!("action-delete-text"))
                        .shortcut_text(shortcut_text_delete()),
                )
                .clicked()
            {
                actions.push(Action::RemoveAnnotation {
                    graph: graph_id,
                    target: annotation,
                });
                view.clear_annotation_selection();
                ui.close();
            }
            ui.separator();
        } else if let PlotTarget::Object(object) = target {
            // Objets graphiques (issue #76) : les mêmes gestes que la barre
            // de menus et le clavier (Suppr).
            view.select_shape(Some(object));
            shape_order_buttons(ui, view, tree, graph_id, object, actions);
            ui.separator();
        } else if ui
            .add_enabled(point.is_some(), egui::Button::new(tr!("action-add-text")))
            .on_hover_text(tr!("action-add-text", "hint"))
            .on_disabled_hover_text(tr!("action-add-text", "disabled"))
            .clicked()
        {
            if let Some(point) = point {
                add_annotation_at(ui.ctx(), view, tree, graph_id, point);
            }
            ui.close();
        }
        if ui
            .button(tr!("action-properties"))
            .on_hover_text(tr!("action-properties", "hint"))
            .clicked()
        {
            actions.push(target.open_settings(graph_id));
            ui.close();
        }
        if ui
            .button(tr!("action-reset-zoom"))
            .on_hover_text(tr!("action-reset-zoom", "hint"))
            .clicked()
        {
            view.reset_view();
            ui.close();
        }
        if ui
            .add(
                egui::Button::new(tr!("action-export-graph"))
                    .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_EXPORT_GRAPH)),
            )
            .on_hover_text(tr!("action-export-graph", "hint"))
            .clicked()
        {
            request_graph_export(Some(view), tree, graph_id, actions);
            ui.close();
        }
    });
}

/// Signaux de la vue, éditeur sur place et Suppr — après la peinture de la
/// figure, dans la fenêtre flottante comme dans la fenêtre détachée.
/// Les trois entrées d'un objet graphique sélectionné : premier plan,
/// arrière-plan, suppression (barre de menus et menu contextuel).
pub(super) fn shape_order_buttons(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    object: stokesforge_render_egui::ShapeRef,
    actions: &mut Vec<Action>,
) {
    let count = crate::state::current_plot_spec(tree, graph)
        .and_then(|spec| {
            spec.panels
                .iter()
                .find(|panel| panel.row == object.row && panel.column == object.column)
                .map(|panel| panel.shapes.len())
        })
        .unwrap_or(0);
    for order in [ShapeOrder::ToFront, ShapeOrder::ToBack] {
        let label = match order {
            ShapeOrder::ToFront => tr!("action-bring-to-front"),
            ShapeOrder::ToBack => tr!("action-send-to-back"),
        };
        let hint = match order {
            ShapeOrder::ToFront => tr!("action-bring-to-front", "hint"),
            ShapeOrder::ToBack => tr!("action-send-to-back", "hint"),
        };
        if ui.button(label).on_hover_text(hint).clicked() {
            actions.push(Action::ReorderShape {
                graph,
                target: object,
                order,
            });
            // Le rang change : la sélection suit l'objet.
            let index = match order {
                ShapeOrder::ToFront => count.saturating_sub(1),
                ShapeOrder::ToBack => 0,
            };
            view.select_shape(Some(stokesforge_render_egui::ShapeRef { index, ..object }));
            ui.close();
        }
    }
    if ui
        .add(egui::Button::new(tr!("action-delete-object")).shortcut_text(shortcut_text_delete()))
        .clicked()
    {
        actions.push(Action::RemoveShape {
            graph,
            target: object,
        });
        view.select_shape(None);
        ui.close();
    }
}

/// Les mêmes entrées, grisées, quand aucun objet n'est sélectionné.
pub(super) fn disabled_shape_order_buttons(ui: &mut egui::Ui) {
    ui.add_enabled(false, egui::Button::new(tr!("action-bring-to-front")))
        .on_disabled_hover_text(tr!("action-bring-to-front", "disabled"));
    ui.add_enabled(false, egui::Button::new(tr!("action-send-to-back")))
        .on_disabled_hover_text(tr!("action-send-to-back", "disabled"));
    ui.add_enabled(
        false,
        egui::Button::new(tr!("action-delete-object")).shortcut_text(shortcut_text_delete()),
    );
}

fn annotation_interactions(
    ui: &egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    for event in view.take_events() {
        match event {
            stokesforge_render_egui::GraphEvent::PlaceText { row, column, x, y } => {
                add_annotation_at(ui.ctx(), view, tree, graph, (row, column, x, y));
                view.set_tool(stokesforge_render_egui::GraphTool::Pointer);
            }
            stokesforge_render_egui::GraphEvent::EditAnnotation(target) => {
                open_annotation_editor(ui.ctx(), view, tree, graph, target);
            }
            stokesforge_render_egui::GraphEvent::AnnotationMoved { target, annotation } => {
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
            }
            stokesforge_render_egui::GraphEvent::ShapeChanged { target, shape } => {
                actions.push(Action::SetShape {
                    graph,
                    target,
                    shape: Box::new(shape),
                });
            }
            stokesforge_render_egui::GraphEvent::EditText(target) => {
                open_text_editor(ui.ctx(), view, tree, graph, target);
            }
            stokesforge_render_egui::GraphEvent::EditLegendEntry {
                row,
                column,
                series,
            } => open_legend_entry_editor(ui.ctx(), view, tree, graph, row, column, series),
            stokesforge_render_egui::GraphEvent::OpenAxisProperties { panel, side } => {
                actions.push(
                    plot_target(Some(PlotComponent::Axis { panel, side })).open_settings(graph),
                );
            }
            stokesforge_render_egui::GraphEvent::OpenSeriesProperties(series) => {
                actions.push(Action::OpenObjectProperties {
                    graph,
                    target: crate::object_manager::VisibilityTarget::Series(series),
                });
            }
            stokesforge_render_egui::GraphEvent::OpenShapeProperties(shape) => {
                if let Some(spec) = crate::state::current_plot_spec(tree, graph) {
                    open_shape_properties(ui.ctx(), view, &spec, shape);
                }
            }
            stokesforge_render_egui::GraphEvent::TextMoved { target, anchor } => {
                actions.push(Action::SetTextAnchor {
                    graph,
                    target,
                    anchor: Some(anchor),
                });
            }
        }
    }
    // L'éditeur d'un titre ou d'un nom de légende, s'il y en a un d'ouvert.
    let text_editor: Option<TextEditor> = ui.ctx().data(|data| data.get_temp(text_editor_id(view)));
    if let Some(editor) = text_editor {
        text_editor_ui(ui.ctx(), view, graph, editor, actions);
        return;
    }
    let id = annotation_editor_id(view);
    let editor: Option<AnnotationEditor> = ui.ctx().data(|data| data.get_temp(id));
    match editor {
        Some(editor) => annotation_editor_ui(ui.ctx(), view, graph, editor, actions),
        None => {
            // Suppr retire le texte sélectionné, quand aucun champ n'a le
            // focus (sinon la touche appartient à la saisie).
            let typing = keyboard_focus_outside_grid(ui.ctx());
            if let Some(target) = view.selected_annotation() {
                if !typing
                    && ui.ctx().input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    })
                {
                    actions.push(Action::RemoveAnnotation { graph, target });
                    view.clear_annotation_selection();
                }
            } else if let Some(target) = view.selected_shape() {
                // Objets graphiques (issue #76) : même geste.
                if !typing
                    && ui.ctx().input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    })
                {
                    actions.push(Action::RemoveShape { graph, target });
                    view.select_shape(None);
                }
            }
        }
    }
}

/// Demande où exporter le graphe, puis confie l'export à
/// [`AppState::dispatch`] (issue #34). Le spec exporté est celui que la vue
/// montre, zoom compris ([`GraphView::framed_spec`]) ; sans vue (graphe pas
/// encore peint), le cadrage enregistré. Le format se lit sur l'extension
/// choisie.
pub(super) fn request_graph_export(
    view: Option<&GraphView>,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(spec) = crate::state::current_plot_spec(tree, graph) else {
        return;
    };
    let spec = match view {
        Some(view) => view.framed_spec(&spec),
        None => spec,
    };
    if let Some(path) = rfd::FileDialog::new()
        .set_title(tr!("export-dialog-title"))
        .add_filter("SVG", &["svg"])
        .add_filter("PDF", &["pdf"])
        .add_filter("PNG", &["png"])
        .set_file_name(export_file_name(tree, graph))
        .save_file()
    {
        actions.push(Action::ExportGraph {
            path,
            spec: Box::new(spec),
        });
    }
}

/// Traduit le composant désigné par le rendu en cible d'interface. Seul le
/// premier panneau est éditable (pas de multi-panneaux), l'indice de
/// panneau est donc ignoré.
fn plot_target(component: Option<PlotComponent>) -> PlotTarget {
    let axis = |side| match side {
        AxisSide::Bottom => PlotAxis::Bottom,
        AxisSide::Left => PlotAxis::Left,
        AxisSide::Right => PlotAxis::Right,
    };
    match component {
        None => PlotTarget::Background,
        Some(PlotComponent::Title) => PlotTarget::Title,
        Some(PlotComponent::PlotArea { .. }) => PlotTarget::PlotArea,
        Some(PlotComponent::Axis { side, .. }) => PlotTarget::Axis(axis(side)),
        Some(PlotComponent::AxisTitle { side, .. }) => PlotTarget::AxisTitle(axis(side)),
        // Le menu contextuel lit le texte libre par `GraphView::annotation_at`,
        // qui donne son adresse dans le spec ; ce repli ne sert pas.
        Some(PlotComponent::Annotation { .. }) => PlotTarget::PlotArea,
        // Idem pour un objet graphique (`GraphView::shape_at`).
        Some(PlotComponent::Shape { .. }) => PlotTarget::PlotArea,
        // Titres et légende (issue #53) : leurs propriétés sont celles du
        // composant, comme avant qu'ils soient désignables en propre.
        Some(PlotComponent::Text(target)) => match target {
            TextTarget::Title | TextTarget::PanelTitle { .. } => PlotTarget::Title,
            TextTarget::AxisTitle { side, .. } => PlotTarget::AxisTitle(match side {
                stokesforge_plot::AxisSide::Bottom => PlotAxis::Bottom,
                stokesforge_plot::AxisSide::Left => PlotAxis::Left,
                stokesforge_plot::AxisSide::Right | stokesforge_plot::AxisSide::Top => {
                    PlotAxis::Right
                }
            }),
            TextTarget::Legend { .. } => PlotTarget::Legend,
        },
        Some(PlotComponent::LegendEntry { .. }) => PlotTarget::Legend,
    }
}

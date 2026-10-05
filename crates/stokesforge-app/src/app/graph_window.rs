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
use crate::object_manager::{AxisRef, VisibilityTarget};
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
use super::grid::keyboard_focus_outside_grid;
use super::object_manager_ui::{series_shortcuts, ObjectManagerRequest};
use super::shortcuts::{shortcut_text_delete, SHORTCUT_EXPORT_GRAPH};
use super::{color_request_action, saved_colors_id};

/// Hauteur du seul contrôle conservé sous une figure : le retour au cadrage
/// initial. Réserver sa hauteur garde le rectangle de la figure stable d'une
/// image à l'autre — sans quoi le cache de formes serait invalidé au moindre
/// survol.
const GRAPH_CONTROLS_HEIGHT: f32 = 28.0;

/// Enrobe [`graph_contents`] d'une zone de dépôt pour le glisser d'une
/// sélection de colonnes de classeur — pendant, côté cible, de la zone de
/// prise posée par [`grid_contents`](crate::app::grid::grid_contents) en bord de grille. Appelée par
/// [`tab_contents`](crate::app::contents::tab_contents), donc identiquement pour un onglet en fenêtre flottante
/// et pour un onglet détaché ([`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs)) : c'est le
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
/// Contenu d'un [`Tab::Graph`](crate::app::tab::Tab::Graph).
///
/// Ce n'est pas une ébauche de la fenêtre de graphe finale : comme tout ce
/// banc, c'est la preuve que l'intégration bout en bout tient. Ce qu'il
/// montre vraiment, et qu'aucun test ne montre :
///
/// - la figure elle-même, avec ses vraies polices et son vrai HiDPI ;
/// - le retour au cadrage initial, pour abandonner un zoom ou un pan sans
///   modifier les données ni la description du graphe.
fn graph_contents(
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
/// [`StokesForgeApp::graph_views`](crate::app::StokesForgeApp::graph_views) : c'est ce qui permet à
/// [`StokesForgeApp::show_detached_tabs`](crate::app::StokesForgeApp::show_detached_tabs) de peindre un `Tab::Graph` détaché avec
/// **exactement** cette fonction — pas de version dégradée, voir la doc de
/// tête de `window_detach.rs`. `GraphView` n'a pas le problème de verrou global à
/// `AppState` qu'ont les tampons de saisie de cellule/note (son cache et sa
/// navigation sont déjà scopés par graphe) : nul besoin d'une variante
/// « détachée » séparée, à la différence de [`grid_contents`](crate::app::grid::grid_contents)/
/// [`note_contents`](crate::app::contents::note_contents).
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
fn display_backdrop(background: &str) -> Option<egui::Color32> {
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
    let axis_id = response.id.with("plot_axis");
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
        let axis = pos.and_then(|pos| match view.component_at(pos) {
            Some(
                PlotComponent::Axis { panel, side } | PlotComponent::AxisTitle { panel, side },
            ) => axis_target(view, panel, side),
            Some(PlotComponent::Text(TextTarget::AxisTitle { row, column, side })) => {
                Some(VisibilityTarget::Axis(AxisRef { row, column, side }))
            }
            _ => None,
        });
        // Figer l'adresse au clic : ni le déplacement du pointeur ni un
        // nouveau rendu pendant le menu ne doivent changer le calque visé.
        ui.ctx().data_mut(|data| data.insert_temp(axis_id, axis));
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
        if ui
            .add_enabled(
                !crate::state::graph_data_books(tree, graph_id).is_empty(),
                egui::Button::new(tr!("action-show-graph-data")),
            )
            .clicked()
        {
            actions.push(Action::ShowGraphData { graph: graph_id });
            ui.close();
        }

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
            let axis: Option<VisibilityTarget> =
                ui.memory(|memory| memory.data.get_temp(axis_id)).flatten();
            actions.push(axis.map_or_else(
                || target.open_settings(graph_id),
                |target| Action::OpenObjectProperties {
                    graph: graph_id,
                    target,
                },
            ));
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
                if let Some(target) = axis_target(view, panel, side) {
                    actions.push(Action::OpenObjectProperties { graph, target });
                }
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

/// Le rang rendu peut différer du rang dans le spec (panneaux omis) :
/// adresser l'axe par la position du calque, comme l'Object Manager (#113).
fn axis_target(view: &GraphView, panel: usize, side: AxisSide) -> Option<VisibilityTarget> {
    let drawn = view.last_drawn()?.panels.get(panel)?;
    Some(VisibilityTarget::Axis(AxisRef {
        row: drawn.row,
        column: drawn.column,
        side: match side {
            AxisSide::Bottom => stokesforge_plot::AxisSide::Bottom,
            AxisSide::Left => stokesforge_plot::AxisSide::Left,
            AxisSide::Right => stokesforge_plot::AxisSide::Right,
        },
    }))
}

/// Libellé et onglet du composant. L'adresse des axes est conservée à part
/// par `axis_target`, pour ouvrir les propriétés du calque visé.
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

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    use stokesforge_core::{Column, Project};
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec, ShapeKind, ShapeRef, ShapeSpec};
    use stokesforge_session::Session;

    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::super::tab::Tab;
    use super::super::window_chrome::{show_floating_window, window_header, HeaderPlacement};
    use super::super::window_placement::GRAPH_WINDOW_SIZE;

    /// Un classeur de deux colonnes et un graphe titré « Figure » qui les
    /// trace — le projet commun des tests de `app` qui ont besoin d'un
    /// graphe tracé (`format.rs`, `window_detach.rs`, `app.rs`).
    pub(in crate::app) fn graphe() -> (Session, NodeId) {
        let mut project = Project::new("Graphe").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, None).unwrap();
        let sheet = project.add_worksheet(book, None).unwrap();
        let x = project
            .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
            .unwrap();
        let y = project
            .add_column(sheet, None, None, Column::from_f64(&[0.0, 1.0]))
            .unwrap();
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        let graph = project
            .add_graph(root, None, PlotSpec::empty("Figure").with_panels(panel))
            .unwrap();
        (Session::spawn(project, None).unwrap(), graph)
    }
    /// #12 : un fond sans couleur ou semi-transparent s'affiche sur du
    /// blanc ; un fond opaque n'a pas besoin de dessous.
    #[test]
    fn le_blanc_d_affichage_ne_double_que_les_fonds_transparents() {
        assert_eq!(
            display_backdrop(stokesforge_plot::TRANSPARENT),
            Some(egui::Color32::WHITE)
        );
        assert_eq!(display_backdrop("#12345680"), Some(egui::Color32::WHITE));
        assert_eq!(display_backdrop("#123456"), None);
    }

    #[test]
    fn le_rendu_garde_son_ratio_et_se_met_a_l_echelle_dans_la_zone_disponible() {
        // F11 : le graphe conserve le ratio de `DEFAULT_FIGURE_SIZE` mais est
        // mis à l'échelle « contain » — maximisé dans la zone disponible, en
        // prenant la dimension limitante (largeur ou hauteur) — plutôt que
        // peint à taille fixe et rogné. Point 3 du rapport du 2026-09-24 :
        // c'est un **zoom** du rendu de référence, jamais une nouvelle mise
        // en page à la taille affichée — les formes mises en cache doivent
        // être identiques quelle que soit la taille de la fenêtre.
        let (session, graph) = graphe();
        let snapshot = session.handle().snapshot().unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let (fig_w, fig_h) = DEFAULT_FIGURE_SIZE;
        let mut reference_shapes: Option<Vec<egui::Shape>> = None;

        for size in [
            GRAPH_WINDOW_SIZE,
            egui::vec2(320.0, 260.0),
            egui::vec2(200.0, 100.0),
        ] {
            let contents = egui::Rect::from_min_size(egui::pos2(300.0, 300.0), size);
            let visible = egui::Rect::from_min_max(
                contents.min,
                contents.max - egui::vec2(0.0, GRAPH_CONTROLS_HEIGHT),
            );
            let scale = (visible.width() / fig_w as f32).min(visible.height() / fig_h as f32);
            let expected_size = egui::vec2(fig_w as f32 * scale, fig_h as f32 * scale);
            for frame in 0..3 {
                // La molette se trouve sous la zone visible, hors de la
                // figure et de ses contrôles : elle ne doit pas la zoomer.
                let pos = egui::pos2(visible.center().x, visible.bottom() + 10.0);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 1200.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, 4.0),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                    ..Default::default()
                };
                let misses = view.cache().misses();
                let mut output = context.run_ui(input, |ui| {
                    let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                    content_ui.set_clip_rect(contents);
                    graph_view_contents(
                        &mut content_ui,
                        &mut view,
                        &snapshot,
                        graph,
                        &mut Vec::new(),
                    );
                });
                output.textures_delta.clear();
                let rect = view.cache().key().unwrap().rect;
                assert!(
                    (rect.size() - egui::vec2(fig_w as f32, fig_h as f32)).length() < 0.1,
                    "la disposition doit rester à la taille de référence, obtenu {:?} pour une \
                     zone de {size:?}",
                    rect.size()
                );
                let display = view.display_rect().unwrap();
                assert!(
                    (display.size() - expected_size).length() < 0.1,
                    "taille affichée attendue pour une zone de {size:?} : {expected_size:?}, \
                     obtenu {:?}",
                    display.size()
                );
                assert!(
                    visible.contains_rect(display),
                    "la figure mise à l'échelle doit tenir dans la zone visible"
                );
                assert_eq!(
                    view.view().generation(),
                    0,
                    "interaction hors de la figure et de ses contrôles"
                );
                if frame > 0 {
                    assert_eq!(view.cache().misses(), misses, "recalcul sans changement");
                }
                // Vérifier les formes réellement envoyées à egui, pas
                // seulement le rectangle calculé avant le rendu.
                assert!(!view.cache().shapes().is_empty());
                if let Some(reference) = &reference_shapes {
                    assert_eq!(
                        view.cache().shapes(),
                        reference.as_slice(),
                        "la taille affichée ne doit pas redisposer la figure (zone de {size:?})"
                    );
                } else {
                    reference_shapes = Some(view.cache().shapes().to_vec());
                }
                let transform = egui::emath::TSTransform::new(
                    display.min.to_vec2(),
                    display.width() / fig_w as f32,
                );
                for shape in view.cache().shapes() {
                    let mut displayed = shape.clone();
                    // Le texte est remis en page à la taille affichée (#18) :
                    // même transformation que `GraphView::paint_scaled`. Un
                    // texte Typst (titres, légende : #32) y change de
                    // texture, rastérisée à la taille affichée (#31) : un
                    // maillage se compare donc sur sa géométrie seule, à un
                    // pixel près : il est calé sur la grille des pixels
                    // physiques à l'écran (#86).
                    stokesforge_render_egui::scaled::transform_shape(
                        &context,
                        &mut displayed,
                        transform,
                        &mut |_| None,
                    );
                    let same = |painted: &egui::Shape| match (painted, &displayed) {
                        (egui::Shape::Mesh(a), egui::Shape::Mesh(b)) => {
                            a.indices == b.indices
                                && a.vertices.len() == b.vertices.len()
                                && a.vertices.iter().zip(&b.vertices).all(|(p, q)| {
                                    (p.pos - q.pos).abs().max_elem() <= 1.0 && p.uv == q.uv
                                })
                        }
                        (a, b) => a == b,
                    };
                    assert!(
                        output.shapes.iter().any(|painted| same(&painted.shape)
                            && visible.contains_rect(painted.clip_rect)),
                        "forme du graphe hors de la zone visible"
                    );
                }
            }
        }
    }

    #[test]
    fn le_pan_principal_atteint_la_figure_sous_la_zone_de_depot() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut graph_views = HashMap::new();
        let mut actions = Vec::new();
        let contents =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(560.0, 540.0));

        let mut paint = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                content_ui.set_clip_rect(contents);
                graph_drop_target(
                    &mut content_ui,
                    &state,
                    &mut actions,
                    &mut graph_views,
                    graph,
                );
            });
            output.textures_delta.clear();
        };

        let start = egui::pos2(380.0, 322.0);
        paint(vec![egui::Event::PointerMoved(start)]);
        let end = start + egui::vec2(30.0, -20.0);
        paint(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        paint(vec![egui::Event::PointerMoved(end)]);
        paint(vec![egui::Event::PointerMoved(
            end + egui::vec2(20.0, -10.0),
        )]);

        assert!(
            graph_views.get(&graph).unwrap().view().generation() > 0,
            "la cible de dépôt ne doit pas intercepter le pan de la figure"
        );
    }

    #[test]
    fn le_pan_principal_atteint_la_figure_dans_la_fenetre_flottante() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut graph_views = HashMap::new();
        let mut actions = Vec::new();
        let tab = Tab::Graph(graph);
        let id = egui::Id::new(("content_window", tab));
        let panel_center = std::cell::Cell::new(None);

        let mut paint = |events: Vec<egui::Event>| {
            let is_initial_frame = events.is_empty();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                show_floating_window(
                    ui.ctx(),
                    id,
                    egui::Rect::from_min_size(egui::pos2(100.0, 100.0), GRAPH_WINDOW_SIZE),
                    ui.ctx().content_rect(),
                    true,
                    |ui| {
                        let _ = window_header(
                            ui,
                            id,
                            "Figure",
                            Some(tab),
                            false,
                            HeaderPlacement::Floating,
                        );
                        graph_drop_target(ui, &state, &mut actions, &mut graph_views, graph);
                    },
                    |_, _| {},
                );
            });
            output.textures_delta.clear();
            if is_initial_frame {
                let view = graph_views.get(&graph).unwrap();
                let panel = &view.last_drawn().unwrap().panels[0];
                let figure = view.display_rect().unwrap();
                let scale = figure.width() / DEFAULT_FIGURE_SIZE.0 as f32;
                panel_center.set(Some(egui::pos2(
                    figure.min.x
                        + (panel.pixel_origin.0 as f32 + panel.pixel_size.0 as f32 / 2.0) * scale,
                    figure.min.y
                        + (panel.pixel_origin.1 as f32 + panel.pixel_size.1 as f32 / 2.0) * scale,
                )));
            }
        };

        paint(Vec::new());
        let start = panel_center.get().unwrap();
        paint(vec![egui::Event::PointerMoved(start)]);
        let end = start + egui::vec2(30.0, -20.0);
        paint(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        paint(vec![egui::Event::PointerMoved(end)]);
        paint(vec![egui::Event::PointerMoved(
            end + egui::vec2(20.0, -10.0),
        )]);

        assert!(
            graph_views.get(&graph).unwrap().view().generation() > 0,
            "la fenetre flottante ne doit pas intercepter le pan de la figure"
        );
    }

    /// #113 : les deux gestes doivent adresser le calque et le côté visés.
    /// La ligne dans la grille diffère ici de l'indice de calque.
    #[test]
    fn gestes_axe_ouvrent_le_bon_calque() {
        for context_menu in [false, true] {
            let (session, graph) = graphe();
            let mut state = AppState::new(session.handle(), "test.stf".into()).unwrap();
            let mut spec = crate::state::current_plot_spec(state.tree(), graph).unwrap();
            spec.panels.push(PanelSpec::at(0, 1));
            state.dispatch(Action::SetPlotSpec { graph, spec });
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut view = GraphView::new();
            let mut actions = Vec::new();
            {
                let mut paint = |events| {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    };
                    let mut output = context.run_ui(input, |ui| {
                        graph_view_contents(ui, &mut view, state.snapshot(), graph, &mut actions);
                    });
                    output.textures_delta.clear();
                };
                paint(Vec::new());
            }
            let drawn = &view.last_drawn().unwrap().panels[1];
            let figure = view.response().unwrap().rect;
            let scale = figure.width() / DEFAULT_FIGURE_SIZE.0 as f32;
            let pos = figure.min
                + egui::vec2(
                    (drawn.pixel_origin.0 as f32 - 5.0) * scale,
                    (drawn.pixel_origin.1 as f32 + drawn.pixel_size.1 as f32 / 2.0) * scale,
                );
            assert_eq!(
                view.component_at(pos),
                Some(PlotComponent::Axis {
                    panel: 1,
                    side: AxisSide::Left
                })
            );
            let button = if context_menu {
                egui::PointerButton::Secondary
            } else {
                egui::PointerButton::Primary
            };
            let mut events = vec![egui::Event::PointerMoved(pos)];
            let presses: &[bool] = if context_menu {
                &[true, false]
            } else {
                &[true, false, true, false]
            };
            for &pressed in presses {
                events.push(egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut paint = |events| {
                let mut output = context.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        graph_view_contents(ui, &mut view, state.snapshot(), graph, &mut actions);
                    },
                );
                output.textures_delta.clear();
                output
            };
            paint(events);
            if context_menu {
                let output = paint(Vec::new());
                let properties = find_text_center(&output.shapes, &tr!("action-properties"));
                // Le menu doit garder le calque d'origine quand le pointeur le quitte.
                click_at(&context, properties, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
            }
            assert_eq!(actions.len(), 1);
            state.dispatch(actions.pop().unwrap());
            assert_eq!(state.plot_settings().unwrap().axis, PlotAxis::Left);
            assert_eq!(state.plot_edit().unwrap().selected_panel, 1);
        }
    }

    /// Chaque composant désigné par le rendu ouvre l'onglet de propriétés qui
    /// le règle ; un clic dans le vide ouvre le fond.
    #[test]
    fn plot_target_traduit_chaque_composant_vers_ses_proprietes() {
        use stokesforge_render_egui::TextTarget as T;
        assert_eq!(plot_target(None), PlotTarget::Background);
        assert_eq!(plot_target(Some(PlotComponent::Title)), PlotTarget::Title);
        assert_eq!(
            plot_target(Some(PlotComponent::PlotArea { panel: 0 })),
            PlotTarget::PlotArea
        );
        for (side, axis) in [
            (AxisSide::Bottom, PlotAxis::Bottom),
            (AxisSide::Left, PlotAxis::Left),
            (AxisSide::Right, PlotAxis::Right),
        ] {
            assert_eq!(
                plot_target(Some(PlotComponent::Axis { panel: 0, side })),
                PlotTarget::Axis(axis)
            );
            assert_eq!(
                plot_target(Some(PlotComponent::AxisTitle { panel: 0, side })),
                PlotTarget::AxisTitle(axis)
            );
        }
        assert_eq!(
            plot_target(Some(PlotComponent::Text(T::Title))),
            PlotTarget::Title
        );
        assert_eq!(
            plot_target(Some(PlotComponent::Text(T::PanelTitle {
                row: 0,
                column: 0
            }))),
            PlotTarget::Title
        );
        for (side, axis) in [
            (stokesforge_plot::AxisSide::Bottom, PlotAxis::Bottom),
            (stokesforge_plot::AxisSide::Left, PlotAxis::Left),
            (stokesforge_plot::AxisSide::Right, PlotAxis::Right),
        ] {
            assert_eq!(
                plot_target(Some(PlotComponent::Text(T::AxisTitle {
                    row: 0,
                    column: 0,
                    side
                }))),
                PlotTarget::AxisTitle(axis)
            );
        }
        assert_eq!(
            plot_target(Some(PlotComponent::Text(T::Legend { row: 0, column: 0 }))),
            PlotTarget::Legend
        );
        assert_eq!(
            plot_target(Some(PlotComponent::LegendEntry {
                panel: 0,
                series: 0
            })),
            PlotTarget::Legend
        );
    }

    /// Le glisser de colonnes depuis un classeur : relâché sur le graphe, il
    /// ajoute une série (une action) ; relâché ailleurs, rien. Le survol avec
    /// une charge entoure le graphe d'un contour, absent sans charge.
    #[test]
    fn deposer_des_colonnes_sur_le_graphe_ajoute_une_serie_et_ailleurs_rien() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let contents =
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(560.0, 540.0));
        let payload = ColumnDragPayload {
            sheet: NodeId::from_raw(7),
            columns: vec![NodeId::from_raw(8), NodeId::from_raw(9)],
        };
        let mut graph_views = HashMap::new();
        // Une image ; `drag` pose la charge du glisser (le classeur le fait
        // quand le geste commence, hors de ce banc).
        let mut paint = |events: Vec<egui::Event>, drag: bool| {
            let mut actions = Vec::new();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1200.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                if drag {
                    egui::DragAndDrop::set_payload(ui.ctx(), payload.clone());
                }
                let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(contents));
                content_ui.set_clip_rect(contents);
                graph_drop_target(
                    &mut content_ui,
                    &state,
                    &mut actions,
                    &mut graph_views,
                    graph,
                );
            });
            output.textures_delta.clear();
            (output, actions)
        };
        let outline = |output: &egui::FullOutput| {
            output.shapes.iter().any(|painted| {
                matches!(&painted.shape,
                    egui::Shape::Rect(rect) if rect.rect == contents && rect.stroke.width == 2.5)
            })
        };
        let release = |pos| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        };

        let inside = egui::pos2(380.0, 322.0);
        let (output, actions) = paint(vec![egui::Event::PointerMoved(inside)], false);
        assert!(!outline(&output), "pas de contour sans glisser en cours");
        assert!(actions.is_empty());
        let (output, actions) = paint(vec![egui::Event::PointerMoved(inside)], true);
        assert!(
            outline(&output),
            "le survol avec une charge entoure le graphe"
        );
        assert!(actions.is_empty(), "le survol seul n'ajoute rien");
        let (_, actions) = paint(vec![release(inside)], false);
        assert_eq!(
            actions,
            vec![drop_payload_action(&payload, graph)],
            "un relâchement sur le graphe ajoute la série"
        );

        let outside = egui::pos2(900.0, 900.0);
        paint(vec![egui::Event::PointerMoved(outside)], true);
        let (_, actions) = paint(vec![release(outside)], false);
        assert!(actions.is_empty(), "relâché hors du graphe, rien");
    }

    /// Premier plan, arrière-plan et suppression d'un objet graphique :
    /// une action chacun, et la sélection suit l'objet à son nouveau rang.
    #[test]
    fn l_ordre_et_la_suppression_d_un_objet_font_suivre_la_selection() {
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Objets");
        for _ in 0..3 {
            spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
                ShapeKind::Rectangle,
                0.0,
                0.0,
                1.0,
                1.0,
            ));
        }
        let graph = project.add_graph(root, None, spec).unwrap();
        let tree = project.tree().clone();
        let object = ShapeRef {
            row: 0,
            column: 0,
            index: 1,
        };
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let mut actions = Vec::new();
        let mut paint = |context: &egui::Context, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                shape_order_buttons(ui, &mut view, &tree, graph, object, &mut actions);
            });
            output.textures_delta.clear();
            output
        };
        let first = paint(&context, Vec::new());
        let front = find_text_center(&first.shapes, &tr!("action-bring-to-front"));
        let back = find_text_center(&first.shapes, &tr!("action-send-to-back"));
        let delete = find_text_center(&first.shapes, &tr!("action-delete-object"));

        click_at(&context, front, egui::Modifiers::NONE, &mut paint);
        click_at(&context, back, egui::Modifiers::NONE, &mut paint);
        click_at(&context, delete, egui::Modifiers::NONE, &mut paint);
        assert_eq!(
            actions,
            vec![
                Action::ReorderShape {
                    graph,
                    target: object,
                    order: ShapeOrder::ToFront,
                },
                Action::ReorderShape {
                    graph,
                    target: object,
                    order: ShapeOrder::ToBack,
                },
                Action::RemoveShape {
                    graph,
                    target: object,
                },
            ]
        );
        assert_eq!(
            view.selected_shape(),
            None,
            "l'objet supprimé n'est plus sélectionné"
        );
    }

    /// Le rang que la sélection prend après « premier plan » (le dernier) et
    /// « arrière-plan » (le premier), vérifié bouton par bouton.
    #[test]
    fn la_selection_prend_le_rang_du_premier_ou_de_l_arriere_plan() {
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Objets");
        for _ in 0..3 {
            spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
                ShapeKind::Ellipse,
                0.0,
                0.0,
                1.0,
                1.0,
            ));
        }
        let graph = project.add_graph(root, None, spec).unwrap();
        let tree = project.tree().clone();
        let object = ShapeRef {
            row: 0,
            column: 0,
            index: 1,
        };
        for (label, expected) in [
            (tr!("action-bring-to-front"), 2),
            (tr!("action-send-to-back"), 0),
        ] {
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut view = GraphView::new();
            view.select_shape(Some(object));
            let mut paint = |context: &egui::Context, events: Vec<egui::Event>| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    shape_order_buttons(ui, &mut view, &tree, graph, object, &mut Vec::new());
                });
                output.textures_delta.clear();
                output
            };
            let first = paint(&context, Vec::new());
            let at = find_text_center(&first.shapes, &label);
            click_at(&context, at, egui::Modifiers::NONE, &mut paint);
            assert_eq!(
                view.selected_shape(),
                Some(ShapeRef {
                    index: expected,
                    ..object
                }),
                "{label}"
            );
        }
    }

    /// Suppr retire le texte libre ou l'objet sélectionné quand aucun champ
    /// n'a le focus ; pendant une saisie, la touche appartient au champ.
    #[test]
    fn suppr_retire_la_selection_sauf_pendant_une_saisie() {
        use stokesforge_render_egui::AnnotationRef;
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Objets");
        spec.panels.get_mut(0).unwrap().annotations.push(
            stokesforge_render_egui::AnnotationSpec::new("Pic", 0.0, 0.0),
        );
        spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
            ShapeKind::Line,
            0.0,
            0.0,
            1.0,
            1.0,
        ));
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let tree = project.tree().clone();
        let text = AnnotationRef {
            row: 0,
            column: 0,
            index: 0,
        };
        let shape = ShapeRef {
            row: 0,
            column: 0,
            index: 0,
        };
        let delete = egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        // Deux images : la première pose (ou non) le focus sur un champ, la
        // seconde envoie Suppr.
        let run = |view: &mut GraphView, typing: bool| {
            let context = egui::Context::default();
            let mut actions = Vec::new();
            let mut buffer = String::new();
            for events in [Vec::new(), vec![delete.clone()]] {
                let first = events.is_empty();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let field = ui.text_edit_singleline(&mut buffer);
                    if typing && first {
                        field.request_focus();
                    }
                    annotation_interactions(ui, view, &tree, graph, &mut actions);
                });
                output.textures_delta.clear();
            }
            actions
        };

        let mut view = GraphView::new();
        view.select_annotation(Some(text), &spec);
        assert!(run(&mut view, true).is_empty(), "Suppr appartient au champ");
        assert_eq!(view.selected_annotation(), Some(text));
        assert_eq!(
            run(&mut view, false),
            vec![Action::RemoveAnnotation {
                graph,
                target: text
            }]
        );
        assert_eq!(view.selected_annotation(), None);

        let mut view = GraphView::new();
        view.select_shape(Some(shape));
        assert!(run(&mut view, true).is_empty(), "Suppr appartient au champ");
        assert_eq!(
            run(&mut view, false),
            vec![Action::RemoveShape {
                graph,
                target: shape
            }]
        );
        assert_eq!(view.selected_shape(), None);

        // Témoin : sans sélection, Suppr ne retire rien.
        assert!(run(&mut GraphView::new(), false).is_empty());
    }
}

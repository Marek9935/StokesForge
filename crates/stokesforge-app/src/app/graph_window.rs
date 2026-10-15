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
use crate::number_field::number_field;
use crate::object_manager::{AxisRef, VisibilityTarget};
use crate::quantities;
use crate::state::{
    drop_payload_action, drop_rows_action, export_file_name, Action, AppState, ColumnDragPayload,
    PlotAxis, PlotSettingsTab, PlotTarget, RowDragPayload,
};
use crate::theme;
use crate::tr;

use super::analysis_handles::calculation_handles;
use super::dialogs::{modal_dialog, DialogOutcome};
use super::graph_text_edit::{
    add_annotation_at, annotation_editor_id, annotation_editor_ui, open_annotation_editor,
    open_group_name_editor_in_graph, open_legend_entry_editor, open_series_name_editor,
    open_text_editor, text_editor_id, text_editor_ui, AnnotationEditor, ContextText, TextEditor,
};
use super::grid::keyboard_focus_outside_grid;
use super::menu_sections::MenuSections;
use super::object_manager_ui::{
    apply_object_request, group_shortcuts, object_node_menu, select_manager_target,
    series_shortcuts, ObjectManagerRequest,
};
use super::shortcuts::SHORTCUT_EXPORT_GRAPH;
use crate::color_picker::{saved_colors_id, SavedColorRequest};

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
    let hovering = pointer_in_tab
        && (egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ui.ctx())
            || egui::DragAndDrop::has_payload_of_type::<RowDragPayload>(ui.ctx()));
    if pointer_in_tab && ui.input(|input| input.pointer.any_released()) {
        // Calque visé : le calque actif du graphe (#161).
        // `has_payload_of_type` avant `take_payload` : celui-ci vide la charge
        // même quand son type ne correspond pas (les lignes étaient perdues).
        if egui::DragAndDrop::has_payload_of_type::<ColumnDragPayload>(ui.ctx()) {
            if let Some(payload) = egui::DragAndDrop::take_payload::<ColumnDragPayload>(ui.ctx()) {
                let panel = published_active_panel(ui.ctx(), graph_id);
                actions.push(drop_payload_action(&payload, graph_id, panel));
            }
        } else if let Some(payload) = egui::DragAndDrop::take_payload::<RowDragPayload>(ui.ctx()) {
            // Des lignes (#166) : l'état décide de l'axe, et pose la question
            // si le graphe a déjà un X.
            let panel = published_active_panel(ui.ctx(), graph_id);
            actions.push(drop_rows_action(&payload, graph_id, panel));
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
                ShapeProperties {
                    target,
                    draft,
                    macro_open: false,
                },
            )
        });
    }
}

#[derive(Clone)]
struct ShapeProperties {
    target: stokesforge_plot::ShapeRef,
    draft: stokesforge_plot::ShapeSpec,
    /// La macro d'historique est ouverte (premier réglage appliqué).
    macro_open: bool,
}

/// Réglages en direct (F1) : chaque changement du brouillon s'applique tout
/// de suite dans une macro d'historique ; OK la garde (une seule entrée),
/// Cancel l'avorte et l'objet retrouve son état d'ouverture.
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
    let before = edit.draft.clone();
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
                    number_field(
                        ui,
                        ("shape_coordinate", coordinate),
                        value,
                        &quantities::FREE,
                    );
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
                number_field(
                    ui,
                    "shape_line_width",
                    &mut edit.draft.line_width,
                    &quantities::SHAPE_LINE_WIDTH,
                );
            });
        },
    );
    actions.extend(env.requests.into_iter().map(SavedColorRequest::into_action));
    let changed = edit.draft != before;
    if changed {
        if !edit.macro_open {
            actions.push(Action::BeginShapeEdit);
            edit.macro_open = true;
        }
        actions.push(Action::SetShape {
            graph,
            target: edit.target,
            shape: Box::new(edit.draft.clone()),
        });
    }
    match outcome {
        DialogOutcome::Accept => {
            if edit.macro_open {
                actions.push(Action::EndShapeEdit { keep: true });
            }
            ctx.data_mut(|data| data.remove::<ShapeProperties>(id));
        }
        DialogOutcome::Cancel => {
            if edit.macro_open {
                actions.push(Action::EndShapeEdit { keep: false });
            }
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
            publish_axis_bounds(ui.ctx(), view, graph_id);
            if let Some(spec) = crate::state::current_plot_spec(&snapshot.tree, graph_id) {
                layer_badges(&figure_ui, view, &spec, graph_id, outer);
            }
            graph_context_menu(ui, view, &snapshot.tree, graph_id, actions);
            annotation_interactions(ui, view, &snapshot.tree, graph_id, actions);
            if let Some(spec) = crate::state::current_plot_spec(&snapshot.tree, graph_id) {
                calculation_handles(ui, view, &snapshot.tree, graph_id, &spec, actions);
            }
            if !keyboard_focus_outside_grid(ui.ctx()) {
                if let Some(spec) = crate::state::current_plot_spec(&snapshot.tree, graph_id) {
                    if let Some(group) = view.selected_group() {
                        // Un groupe sélectionné (#197) : mêmes gestes qu'au
                        // clavier de l'Object Manager ; le renommage se fait
                        // sur place dans le graphe (#206), l'arbre pouvant
                        // être masqué ou replié.
                        let mut requests = Vec::new();
                        group_shortcuts(ui.ctx(), graph_id, group, &spec, actions, &mut requests);
                        for request in requests {
                            if let ObjectManagerRequest::RenameGroup(group) = request {
                                open_group_name_editor_in_graph(ui.ctx(), view, &spec, group);
                            }
                        }
                        if ui.input_mut(|i| i.consume_shortcut(&super::shortcuts::KEY_DELETE)) {
                            actions.push(Action::RemovePlotObject {
                                graph: graph_id,
                                target: crate::object_manager::VisibilityTarget::SeriesGroup(group),
                            });
                        }
                    } else if let Some(series) = view.selected_series() {
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
                        if ui.input_mut(|i| i.consume_shortcut(&super::shortcuts::KEY_DELETE)) {
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
                ui.visuals().error_fg_color,
                crate::errors::render::error_message(&err),
            );
        }
    }
}

/// Les bornes que le dernier tracé a retenues pour les axes d'un calque, en
/// valeurs de données et dans l'ordre croissant (#221). Calculées par
/// `stokesforge-render` (`DrawnPanel`), jamais refaites ici.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct PanelBounds {
    /// Indice du calque dans `PlotSpec::panels`.
    pub index: usize,
    pub bottom: Option<(f64, f64)>,
    pub left: Option<(f64, f64)>,
    pub right: Option<(f64, f64)>,
}

fn axis_bounds_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_axis_bounds", graph))
}

fn ascending((a, b): (f64, f64)) -> (f64, f64) {
    (a.min(b), a.max(b))
}

/// Publie les bornes du dernier tracé de `graph` : la fenêtre des propriétés,
/// qui n'a pas la [`GraphView`], les montre grisées sous « Auto range ».
fn publish_axis_bounds(ctx: &egui::Context, view: &GraphView, graph: NodeId) {
    let Some(drawn) = view.last_drawn() else {
        return;
    };
    let bounds: Vec<PanelBounds> = drawn
        .panels
        .iter()
        .map(|panel| PanelBounds {
            index: panel.index,
            bottom: Some(ascending(panel.x_range)),
            left: Some(ascending(panel.y_range)),
            right: panel.y2_range.map(ascending),
        })
        .collect();
    ctx.data_mut(|data| data.insert_temp(axis_bounds_id(graph), bounds));
}

/// Les bornes publiées pour le calque `panel` de `graph` ; `None` faute de vue
/// peinte.
pub(super) fn published_axis_bounds(
    ctx: &egui::Context,
    graph: NodeId,
    panel: usize,
) -> Option<PanelBounds> {
    ctx.data(|data| data.get_temp::<Vec<PanelBounds>>(axis_bounds_id(graph)))?
        .into_iter()
        .find(|bounds| bounds.index == panel)
}

/// Le calque actif de `graph`, tel que sa dernière vue peinte l'a publié
/// (#161) : lisible depuis un menu de classeur ou une fenêtre voisine, qui
/// n'ont pas accès à la [`GraphView`]. Le premier calque faute de vue peinte.
pub(super) fn published_active_panel(ctx: &egui::Context, graph: NodeId) -> usize {
    ctx.data(|data| data.get_temp(active_panel_id(graph)))
        .unwrap_or(0)
}

fn active_panel_id(graph: NodeId) -> egui::Id {
    egui::Id::new(("stokesforge_active_panel", graph))
}

/// Repère des calques (#161), à la manière d'Origin : un petit numéro par
/// calque dans le coin de la fenêtre, celui du calque actif en évidence ;
/// cliquer un numéro active le calque. Repère d'interface peint par-dessus la
/// figure : jamais dans l'export ni dans la copie de la figure.
fn layer_badges(
    ui: &egui::Ui,
    view: &mut GraphView,
    spec: &stokesforge_plot::PlotSpec,
    graph: NodeId,
    outer: egui::Rect,
) {
    let active = view.active_panel(spec);
    ui.ctx()
        .data_mut(|data| data.insert_temp(active_panel_id(graph), active));
    let side = 18.0;
    let gap = 3.0;
    for index in 0..spec.panels.len() {
        let rect = egui::Rect::from_min_size(
            outer.left_top() + egui::vec2(4.0 + index as f32 * (side + gap), 4.0),
            egui::vec2(side, side),
        );
        if !outer.contains_rect(rect) {
            break;
        }
        let number = index + 1;
        let response = ui
            .interact(
                rect,
                view.id().with(("layer_badge", index)),
                egui::Sense::click(),
            )
            .on_hover_text(tr!("plot-layer-badge", "hint", n = number.to_string()));
        let visuals = ui.visuals();
        let (fill, ink) = if index == active {
            (visuals.selection.bg_fill, visuals.selection.stroke.color)
        } else if response.hovered() {
            (
                visuals.widgets.hovered.bg_fill,
                visuals.widgets.hovered.fg_stroke.color,
            )
        } else {
            (
                visuals.widgets.inactive.bg_fill,
                visuals.widgets.inactive.fg_stroke.color,
            )
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, fill);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            number.to_string(),
            egui::FontId::proportional(11.0),
            ink,
        );
        if response.clicked() {
            view.set_active_panel(index, spec);
            ui.ctx()
                .data_mut(|data| data.insert_temp(active_panel_id(graph), index));
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

/// Ce que le clic droit a visé dans la figure, lu **au moment du clic** et
/// mémorisé : le menu reste ouvert plusieurs images, pendant lesquelles le
/// pointeur bouge.
#[derive(Debug, Clone, Copy)]
struct ContextHit {
    /// Le composant, pour l'en-tête grisé et la page des propriétés du fond.
    target: PlotTarget,
    /// L'objet qui a son propre menu (courbe, texte libre, objet, titre,
    /// légende, axe) — le même que dans l'Object Manager.
    object: Option<VisibilityTarget>,
    /// Une entrée de légende : pas de nœud d'Object Manager, son propre menu.
    legend_entry: Option<ContextText>,
    /// Le pixel egui du clic : point d'insertion d'un texte ou d'une forme.
    pos: Option<egui::Pos2>,
    /// Le point de données sous le clic, pour « Add Text ».
    point: Option<(usize, f64, f64)>,
}

/// Menu contextuel de la figure (#152) : un clic droit (sans glisser — le
/// glisser secondaire reste le zoom élastique de `stokesforge-render-egui`)
/// ouvre **le menu de l'objet visé**, celui de l'Object Manager
/// ([`object_node_menu`]), ou, sur le fond et la zone de tracé, Add Text,
/// Insert Object, Export et Properties…. Un clic droit sur une courbe la
/// sélectionne. L'en-tête grisé nomme la cible : la figure est le seul endroit
/// où elle n'est pas évidente.
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
    let hit_id = response.id.with("context_hit");
    if response.secondary_clicked() {
        let spec = crate::state::current_plot_spec(tree, graph_id);
        let pos = response.interact_pointer_pos();
        let component = pos.and_then(|pos| view.component_at(pos));
        let annotation = pos.and_then(|pos| view.annotation_at(pos));
        let shape = pos.and_then(|pos| view.shape_at(pos));
        let series = pos
            .filter(|_| matches!(component, None | Some(PlotComponent::PlotArea { .. })))
            .zip(spec.as_ref())
            .and_then(|(pos, spec)| view.series_at(pos, spec));
        let target = match (annotation, shape) {
            (Some(annotation), _) => PlotTarget::Annotation(annotation),
            (None, Some(shape)) => PlotTarget::Object(shape),
            (None, None) => plot_target(component),
        };
        let object = if let Some(annotation) = annotation {
            Some(VisibilityTarget::Annotation(annotation))
        } else if let Some(shape) = shape {
            Some(VisibilityTarget::Shape(shape))
        } else if let Some(series) = series {
            Some(VisibilityTarget::Series(series))
        } else {
            match component {
                Some(PlotComponent::Text(text)) => Some(VisibilityTarget::Text(text)),
                Some(
                    PlotComponent::Axis { panel, side } | PlotComponent::AxisTitle { panel, side },
                ) => axis_target(view, panel, side),
                _ => None,
            }
        };
        // Titre, titre d'axe, légende ou entrée de légende visé (issue #53).
        let legend_entry = match component {
            Some(PlotComponent::LegendEntry { panel, series }) => view
                .last_drawn()
                .and_then(|last| last.panels.get(panel))
                .map(|drawn| ContextText::LegendEntry {
                    panel: drawn.index,
                    series,
                }),
            _ => None,
        };
        // Un clic droit sur un objet le sélectionne, comme un clic simple.
        if let (Some(spec), Some(object)) = (spec.as_ref(), object) {
            select_manager_target(view, Some(object), spec);
        }
        let hit = ContextHit {
            target,
            object,
            legend_entry,
            pos,
            point: pos.and_then(|pos| view.data_at(pos)),
        };
        ui.memory_mut(|memory| memory.data.insert_temp(hit_id, hit));
    }
    let mut requests = Vec::new();
    let mut insert = None;
    response.context_menu(|ui| {
        let hit: ContextHit =
            ui.memory(|memory| memory.data.get_temp(hit_id))
                .unwrap_or(ContextHit {
                    target: PlotTarget::Background,
                    object: None,
                    legend_entry: None,
                    pos: None,
                    point: None,
                });
        ui.weak(hit.target.label());
        let spec = crate::state::current_plot_spec(tree, graph_id);
        match (hit.object, hit.legend_entry, spec.as_ref()) {
            (Some(object), _, Some(spec)) => object_node_menu(
                ui,
                graph_id,
                object,
                spec,
                crate::object_manager::Shown::BASE,
                actions,
                &mut requests,
            ),
            (None, Some(entry), _) => {
                legend_entry_menu(ui, graph_id, entry, hit.target, actions, &mut requests)
            }
            _ => background_menu(ui, view, tree, graph_id, &hit, actions, &mut insert),
        }
    });
    let ctx = ui.ctx().clone();
    for request in requests {
        apply_object_request(&ctx, view, tree, graph_id, request, actions);
    }
    if let Some((pos, kind)) = insert {
        if let Some(spec) = crate::state::current_plot_spec(tree, graph_id) {
            view.insert_shape_at(pos, kind, &spec);
        }
    }
}

/// Entrée de légende : modifier son texte, puis `Properties…` (page Légende).
fn legend_entry_menu(
    ui: &mut egui::Ui,
    graph_id: NodeId,
    entry: ContextText,
    target: PlotTarget,
    actions: &mut Vec<Action>,
    requests: &mut Vec<ObjectManagerRequest>,
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        if ui.button(tr!("action-edit-text")).clicked() {
            requests.push(ObjectManagerRequest::EditText(entry));
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .button(tr!("action-properties"))
            .on_hover_text(tr!("action-properties", "hint"))
            .clicked()
        {
            actions.push(target.open_settings(graph_id));
            ui.close();
        }
    });
}

/// Fond de la figure et zone de tracé : Add Text, Insert Object ▸, Export ▸,
/// puis `Properties…` sur la page du composant.
fn background_menu(
    ui: &mut egui::Ui,
    view: &mut GraphView,
    tree: &Tree,
    graph_id: NodeId,
    hit: &ContextHit,
    actions: &mut Vec<Action>,
    insert: &mut Option<(egui::Pos2, stokesforge_render_egui::ShapeKind)>,
) {
    let mut sections = MenuSections::new();
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                hit.point.is_some(),
                egui::Button::new(tr!("action-add-text")),
            )
            .on_hover_text(tr!("action-add-text", "hint"))
            .on_disabled_hover_text(tr!("action-add-text", "disabled"))
            .clicked()
        {
            if let Some(point) = hit.point {
                add_annotation_at(ui.ctx(), view, tree, graph_id, point);
            }
            ui.close();
        }
        ui.menu_button(tr!("menu-insert-object"), |ui| {
            for (label, kind) in [
                (
                    tr!("tool-rectangle"),
                    stokesforge_render_egui::ShapeKind::Rectangle,
                ),
                (
                    tr!("tool-ellipse"),
                    stokesforge_render_egui::ShapeKind::Ellipse,
                ),
                (tr!("tool-line"), stokesforge_render_egui::ShapeKind::Line),
                (tr!("tool-arrow"), stokesforge_render_egui::ShapeKind::Arrow),
            ] {
                if ui
                    .add_enabled(hit.point.is_some(), egui::Button::new(label))
                    .on_disabled_hover_text(tr!("action-add-text", "disabled"))
                    .clicked()
                {
                    *insert = hit.pos.map(|pos| (pos, kind));
                    ui.close();
                }
            }
        });
    });
    sections.section(ui, true, |ui| {
        ui.menu_button(tr!("menu-export"), |ui| {
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
            if ui
                .button(tr!("action-copy-graph"))
                .on_hover_text(tr!("action-copy-graph", "hint"))
                .clicked()
            {
                if let Some(spec) = crate::state::current_plot_spec(tree, graph_id) {
                    actions.push(Action::CopyGraphFigure {
                        spec: Box::new(view.framed_spec(&spec)),
                    });
                }
                ui.close();
            }
        });
    });
    sections.section(ui, true, |ui| {
        if ui
            .button(tr!("action-properties"))
            .on_hover_text(tr!("action-properties", "hint"))
            .clicked()
        {
            actions.push(hit.target.open_settings(graph_id));
            ui.close();
        }
    });
}

/// Pages des propriétés : un seul ensemble pour Graphique et son clic droit.
pub(super) fn graph_properties_menu(ui: &mut egui::Ui, graph: NodeId, actions: &mut Vec<Action>) {
    for (label, hint, tab) in [
        (
            tr!("menu-graph-properties"),
            tr!("menu-graph-properties", "hint"),
            PlotSettingsTab::Graph,
        ),
        (
            tr!("menu-graph-plot-area"),
            tr!("menu-graph-plot-area", "hint"),
            PlotSettingsTab::PlotArea,
        ),
        (
            tr!("menu-graph-axes"),
            tr!("menu-graph-axes", "hint"),
            PlotSettingsTab::Axes,
        ),
        (
            tr!("menu-graph-series"),
            tr!("menu-graph-series", "hint"),
            PlotSettingsTab::Series,
        ),
        (
            tr!("menu-graph-legend"),
            tr!("menu-graph-legend", "hint"),
            PlotSettingsTab::Legend,
        ),
        (
            tr!("menu-graph-theme"),
            tr!("menu-graph-theme", "hint"),
            PlotSettingsTab::Theme,
        ),
    ] {
        if ui.button(label).on_hover_text(hint).clicked() {
            actions.push(Action::OpenPlotSettings {
                graph,
                tab,
                axis: None,
            });
            ui.close();
        }
    }
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
            stokesforge_render_egui::GraphEvent::PlaceText { panel, x, y } => {
                add_annotation_at(ui.ctx(), view, tree, graph, (panel, x, y));
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
            stokesforge_render_egui::GraphEvent::EditLegendEntry { panel, series } => {
                open_legend_entry_editor(ui.ctx(), view, tree, graph, panel, series)
            }
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
            stokesforge_render_egui::GraphEvent::TextResized { target, width } => {
                if let Some(mut font) =
                    crate::state::current_plot_spec(tree, graph).and_then(|spec| target.font(&spec))
                {
                    font.width = Some(width);
                    actions.push(Action::SetTextFont {
                        graph,
                        target,
                        font,
                    });
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
                    && ui
                        .ctx()
                        .input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_DELETE))
                {
                    actions.push(Action::RemoveAnnotation { graph, target });
                    view.clear_annotation_selection();
                }
            } else if let Some(target) = view.selected_shape() {
                // Objets graphiques (issue #76) : même geste.
                if !typing
                    && ui
                        .ctx()
                        .input_mut(|input| input.consume_shortcut(&super::shortcuts::KEY_DELETE))
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
    actions.push(Action::PickFile(
        crate::file_picker::PickRequest::ExportGraph {
            spec: Box::new(spec),
            file_name: export_file_name(tree, graph),
        },
    ));
}

/// Le rang rendu peut différer du rang dans le spec (panneaux omis) :
/// adresser l'axe par la position du calque, comme l'Object Manager (#113).
fn axis_target(view: &GraphView, panel: usize, side: AxisSide) -> Option<VisibilityTarget> {
    let drawn = view.last_drawn()?.panels.get(panel)?;
    Some(VisibilityTarget::Axis(AxisRef {
        panel: drawn.index,
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
    use super::super::test_support::colonnes_xy;
    use super::*;

    use crate::state::ShapeOrder;
    use stokesforge_core::Project;
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
        let (x, y) = colonnes_xy(&mut project);
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
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
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
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
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
            plot_target(Some(PlotComponent::Text(T::PanelTitle { panel: 0 }))),
            PlotTarget::Title
        );
        for (side, axis) in [
            (stokesforge_plot::AxisSide::Bottom, PlotAxis::Bottom),
            (stokesforge_plot::AxisSide::Left, PlotAxis::Left),
            (stokesforge_plot::AxisSide::Right, PlotAxis::Right),
        ] {
            assert_eq!(
                plot_target(Some(PlotComponent::Text(T::AxisTitle { panel: 0, side }))),
                PlotTarget::AxisTitle(axis)
            );
        }
        assert_eq!(
            plot_target(Some(PlotComponent::Text(T::Legend { panel: 0 }))),
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

    /// #161 : le repère numéroté active un calque, le dépôt de colonnes le
    /// vise, un calque qui disparaît ramène au premier.
    #[test]
    fn le_calque_actif_suit_le_repere_et_cible_le_depot() {
        let (session, graph) = graphe();
        let mut state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let mut spec = crate::state::current_plot_spec(state.tree(), graph).unwrap();
        spec.panels.push(PanelSpec::at(0, 1));
        state.dispatch(Action::SetPlotSpec { graph, spec });
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut view = GraphView::new();
        let origin = std::cell::Cell::new(egui::Pos2::ZERO);
        let paint = |view: &mut GraphView, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                origin.set(ui.available_rect_before_wrap().min);
                graph_view_contents(ui, view, state.snapshot(), graph, &mut Vec::new());
            });
            output.textures_delta.clear();
        };
        let spec = crate::state::current_plot_spec(state.tree(), graph).unwrap();
        paint(&mut view, Vec::new());
        assert_eq!(view.active_panel(&spec), 0, "graphe neuf : le premier");

        let badge = origin.get() + egui::vec2(4.0 + 21.0 + 9.0, 4.0 + 9.0);
        let click = |pressed| egui::Event::PointerButton {
            pos: badge,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        paint(&mut view, vec![egui::Event::PointerMoved(badge)]);
        paint(&mut view, vec![click(true)]);
        paint(&mut view, vec![click(false)]);
        assert_eq!(
            view.active_panel(&spec),
            1,
            "le repère 2 active le calque 2"
        );
        assert_eq!(published_active_panel(&context, graph), 1);

        let payload = ColumnDragPayload {
            sheet: NodeId::from_raw(7),
            columns: vec![NodeId::from_raw(8)],
        };
        assert_eq!(
            drop_payload_action(&payload, graph, published_active_panel(&context, graph)),
            drop_payload_action(&payload, graph, 1),
            "le dépôt vise le calque actif"
        );

        let mut one = spec.clone();
        one.panels = stokesforge_plot::Panels::one(PanelSpec::at(0, 0));
        assert_eq!(view.active_panel(&one), 0, "calque disparu : le premier");
    }

    /// #161 : un nœud de calque de l'Object Manager le rend actif, sans
    /// toucher à la sélection ; un calque inexistant est ignoré.
    #[test]
    fn un_noeud_de_calque_de_l_object_manager_l_active() {
        let mut spec = stokesforge_plot::PlotSpec::empty("");
        spec.panels.push(PanelSpec::at(0, 1));
        let mut view = GraphView::new();
        select_manager_target(&mut view, Some(VisibilityTarget::Panel(1)), &spec);
        assert_eq!(view.active_panel(&spec), 1);
        select_manager_target(&mut view, Some(VisibilityTarget::Group(7)), &spec);
        assert_eq!(view.active_panel(&spec), 1, "calque inexistant : inchangé");
        select_manager_target(&mut view, Some(VisibilityTarget::Axes(0)), &spec);
        assert_eq!(view.active_panel(&spec), 0);
    }

    /// Le glisser de colonnes depuis un classeur : relâché sur le graphe, il
    /// ajoute une série (une action) ; relâché ailleurs, rien. Le survol avec
    /// une charge entoure le graphe d'un contour, absent sans charge.
    #[test]
    fn deposer_des_colonnes_sur_le_graphe_ajoute_une_serie_et_ailleurs_rien() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
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
            vec![drop_payload_action(&payload, graph, 0)],
            "un relâchement sur le graphe ajoute la série"
        );

        let outside = egui::pos2(900.0, 900.0);
        paint(vec![egui::Event::PointerMoved(outside)], true);
        let (_, actions) = paint(vec![release(outside)], false);
        assert!(actions.is_empty(), "relâché hors du graphe, rien");

        // Des lignes (#166) : le dépôt produit l'action de lignes. Régression :
        // `take_payload::<ColumnDragPayload>` vidait la charge de lignes avant
        // qu'elle ne soit lue, et rien ne se traçait.
        let rows = RowDragPayload {
            sheet: NodeId::from_raw(7),
            rows: vec![0, 2],
        };
        egui::DragAndDrop::set_payload(&context, rows.clone());
        paint(vec![egui::Event::PointerMoved(inside)], false);
        let (_, actions) = paint(vec![release(inside)], false);
        assert_eq!(
            actions,
            vec![drop_rows_action(&rows, graph, 0)],
            "un relâchement de lignes sur le graphe trace les lignes"
        );
    }

    #[test]
    fn clic_droit_sur_forme_selectionne_avant_sous_menu_et_garde_sa_cible() {
        let mut project = Project::new("Cible du menu").unwrap();
        let mut spec = PlotSpec::empty("Formes");
        for (a, b) in [(0.2, 0.4), (0.6, 0.8)] {
            spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
                ShapeKind::Rectangle,
                a,
                a,
                b,
                b,
            ));
        }
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let object = ShapeRef { panel: 0, index: 0 };
        let other = ShapeRef { index: 1, ..object };
        for properties in [true, false] {
            let context = super::super::test_support::contexte();
            context.all_styles_mut(|style| style.animation_time = 0.0);
            let view = std::cell::RefCell::new(GraphView::new());
            let actions = std::cell::RefCell::new(Vec::new());
            let paint = |events| {
                super::super::test_support::image_de(
                    &context,
                    egui::vec2(1200.0, 900.0),
                    events,
                    |ui| {
                        graph_view_contents(
                            ui,
                            &mut view.borrow_mut(),
                            state.snapshot(),
                            graph,
                            &mut actions.borrow_mut(),
                        );
                    },
                )
            };
            paint(Vec::new());
            paint(Vec::new());
            let pos = {
                let view = view.borrow();
                let shape = view.drawn_shape(object).unwrap();
                let display = view.display_rect().unwrap();
                let scale = display.width() / DEFAULT_FIGURE_SIZE.0 as f32;
                display.min
                    + egui::vec2(
                        shape.p1.0 as f32 * scale,
                        ((shape.p1.1 + shape.p2.1) / 2.0) as f32 * scale,
                    )
            };
            assert_eq!(
                view.borrow().shape_at(pos),
                Some(object),
                "témoin : le clic atteint la forme"
            );
            view.borrow_mut().select_shape(Some(other));
            paint(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            let output = paint(Vec::new());
            assert_eq!(
                view.borrow().selected_shape(),
                Some(object),
                "la sélection précède toute ouverture de sous-menu"
            );
            assert!(actions.borrow().is_empty());
            // Le menu de l'objet est à plat : ni « Properties ▸ » ni « Objects ▸ ».
            let label = if properties {
                tr!("action-properties")
            } else {
                tr!("action-bring-to-front")
            };
            let pos = find_text_center(&output.shapes, &label);
            click_at(&context, pos, egui::Modifiers::NONE, |_, events| {
                paint(events)
            });
            if properties {
                paint(Vec::new());
                assert!(actions.borrow().is_empty());
                let id = view.borrow().id().with("shape_properties");
                assert!(
                    context
                        .data(|data| data.get_temp::<ShapeProperties>(id))
                        .is_some(),
                    "Properties… ouvre la fenêtre de l'objet visé"
                );
            } else {
                assert_eq!(
                    *actions.borrow(),
                    vec![Action::ReorderShape {
                        graph,
                        target: object,
                        order: ShapeOrder::ToFront,
                    }]
                );
            }
            assert!(!egui::Popup::is_any_open(&context));
        }
        session.shutdown().unwrap();
    }

    /// #152 : le clic droit ouvre le menu **de l'objet visé**. Sur une courbe,
    /// Show Data, les gestes d'organisation, Delete et Properties… — sans
    /// Add Text, Export ni Reset Zoom ; sur le fond, Add Text, Insert Object ▸,
    /// Export ▸ et Properties… — sans Show Data, Reset Zoom ni « Properties ▸ ».
    #[test]
    fn le_clic_droit_de_la_figure_ouvre_le_menu_de_la_courbe_ou_celui_du_fond() {
        use super::super::test_support::{contexte, image_de};
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let spec = crate::state::current_plot_spec(state.tree(), graph).unwrap();
        let series = stokesforge_plot::SeriesRef::new(0, 0);
        for on_curve in [true, false] {
            let context = contexte();
            context.all_styles_mut(|style| style.animation_time = 0.0);
            let view = std::cell::RefCell::new(GraphView::new());
            let actions = std::cell::RefCell::new(Vec::new());
            let paint = |events| {
                image_de(&context, egui::vec2(1200.0, 900.0), events, |ui| {
                    graph_view_contents(
                        ui,
                        &mut view.borrow_mut(),
                        state.snapshot(),
                        graph,
                        &mut actions.borrow_mut(),
                    );
                })
            };
            paint(Vec::new());
            paint(Vec::new());
            // Un pixel de la courbe, ou de la zone de tracé hors de toute courbe.
            let pos = {
                let view = view.borrow();
                let display = view.display_rect().unwrap();
                let mut found = None;
                'scan: for iy in 0..120 {
                    for ix in 0..160 {
                        let pos = display.min
                            + egui::vec2(
                                display.width() * ix as f32 / 160.0,
                                display.height() * iy as f32 / 120.0,
                            );
                        let hit = view.series_at(pos, &spec);
                        let area =
                            matches!(view.component_at(pos), Some(PlotComponent::PlotArea { .. }));
                        if area && (hit == Some(series)) == on_curve {
                            found = Some(pos);
                            break 'scan;
                        }
                    }
                }
                found.expect("un pixel du cas visé")
            };
            paint(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                paint(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            let output = paint(Vec::new());
            let peint = |label: &str| {
                output.shapes.iter().any(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => shape.galley.job.text == label,
                    _ => false,
                })
            };
            let remises = output
                .shapes
                .iter()
                .filter(|painted| match &painted.shape {
                    egui::Shape::Text(shape) => shape.galley.job.text == tr!("action-reset-zoom"),
                    _ => false,
                })
                .count();
            assert_eq!(
                remises, 1,
                "Reset Zoom reste le bouton du graphe, plus une entrée du clic droit"
            );
            assert!(!peint(&tr!("menu-properties")), "pas de « Properties ▸ »");
            assert!(peint(&tr!("action-properties")), "Properties… direct");
            if on_curve {
                assert_eq!(view.borrow().selected_series(), Some(series));
                for present in [
                    tr!("menu-analysis"),
                    tr!("action-show-graph-data"),
                    tr!("action-rename-series"),
                    tr!("action-move-series-up"),
                    tr!("action-hide-content"),
                    tr!("action-delete-content"),
                ] {
                    assert!(peint(&present), "{present} sur une courbe");
                }
                for absent in [tr!("action-add-text"), tr!("menu-export")] {
                    assert!(!peint(&absent), "{absent} n'a pas de sens sur une courbe");
                }
                let at = find_text_center(&output.shapes, &tr!("action-show-graph-data"));
                click_at(&context, at, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
                assert_eq!(
                    *actions.borrow(),
                    vec![Action::ShowSeriesData { graph, series }]
                );
            } else {
                for present in [
                    tr!("action-add-text"),
                    tr!("menu-insert-object"),
                    tr!("menu-export"),
                ] {
                    assert!(peint(&present), "{present} sur le fond");
                }
                assert!(
                    !peint(&tr!("action-show-graph-data")),
                    "Show Data seulement sur une courbe"
                );
                let at = find_text_center(&output.shapes, &tr!("menu-insert-object"));
                click_at(&context, at, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
                let output = paint(Vec::new());
                let at = find_text_center(&output.shapes, &tr!("tool-rectangle"));
                click_at(&context, at, egui::Modifiers::NONE, |_, events| {
                    paint(events)
                });
                paint(Vec::new());
                assert!(
                    actions
                        .borrow()
                        .iter()
                        .any(|action| matches!(action, Action::SetShape { .. })),
                    "la forme est posée au point cliqué : {:?}",
                    actions.borrow()
                );
            }
        }
        session.shutdown().unwrap();
    }

    /// Premier plan, arrière-plan et suppression d'un objet graphique :
    /// une action chacun, et la sélection suit l'objet à son nouveau rang.
    #[test]
    fn l_ordre_et_la_suppression_d_un_objet_passent_par_son_menu_unique() {
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
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let _ = project.tree().clone();
        let object = ShapeRef { panel: 0, index: 1 };
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        let mut actions = Vec::new();
        let mut requests = Vec::new();
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
                object_node_menu(
                    ui,
                    graph,
                    VisibilityTarget::Shape(object),
                    &spec,
                    crate::object_manager::Shown::BASE,
                    &mut actions,
                    &mut requests,
                );
            });
            output.textures_delta.clear();
            output
        };
        let first = paint(&context, Vec::new());
        let front = find_text_center(&first.shapes, &tr!("action-bring-to-front"));
        let back = find_text_center(&first.shapes, &tr!("action-send-to-back"));
        let delete = find_text_center(&first.shapes, &tr!("action-delete-content"));

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
                Action::RemovePlotObject {
                    graph,
                    target: VisibilityTarget::Shape(object),
                },
            ]
        );
        let suivis: Vec<_> = requests
            .iter()
            .filter_map(|request| match request {
                ObjectManagerRequest::Select(VisibilityTarget::Shape(shape)) => Some(shape.index),
                _ => None,
            })
            .collect();
        assert_eq!(suivis, vec![2, 0], "la sélection suit l'objet déplacé");
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
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let object = ShapeRef { panel: 0, index: 1 };
        for (label, expected) in [
            (tr!("action-bring-to-front"), 2),
            (tr!("action-send-to-back"), 0),
        ] {
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut requests = Vec::new();
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
                    object_node_menu(
                        ui,
                        graph,
                        VisibilityTarget::Shape(object),
                        &spec,
                        crate::object_manager::Shown::BASE,
                        &mut Vec::new(),
                        &mut requests,
                    );
                });
                output.textures_delta.clear();
                output
            };
            let first = paint(&context, Vec::new());
            let at = find_text_center(&first.shapes, &label);
            click_at(&context, at, egui::Modifiers::NONE, &mut paint);
            assert!(
                requests.iter().any(|request| matches!(
                    request,
                    ObjectManagerRequest::Select(VisibilityTarget::Shape(shape))
                        if *shape == ShapeRef { index: expected, ..object }
                )),
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
        let text = AnnotationRef { panel: 0, index: 0 };
        let shape = ShapeRef { panel: 0, index: 0 };
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

    /// #209 : les raccourcis d'un **groupe** sélectionné dans le graphe (#197)
    /// — Suppr, F2, Ctrl+↑/↓ — visent le groupe, pas sa première série ;
    /// Ctrl+↑/↓ ne demande rien quand le groupe est déjà au bord, et une
    /// saisie en cours garde ses touches. Calque « A, [B C] ».
    #[test]
    fn les_raccourcis_d_un_groupe_selectionne_visent_le_groupe() {
        use super::super::object_manager_ui::group_rename_pending;
        use crate::object_manager::VisibilityTarget;
        use stokesforge_plot::{SeriesGroupRef, SeriesRef};

        let mut project = Project::new("Groupes").unwrap();
        let root = project.root();
        let (x, y) = colonnes_xy(&mut project);
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        panel.push_series_batch(vec![SeriesSpec::new_xy(x, y), SeriesSpec::new_xy(x, y)]);
        let spec = PlotSpec::empty("Groupes").with_panels(panel);
        let group = SeriesGroupRef::new(0, spec.panels[0].group_ids()[0]);
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let snapshot = session.handle().snapshot().unwrap();

        let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        // Deux images : la première peint le graphe (et pose ou non le focus
        // sur un champ), la seconde envoie la touche. Rend les actions et le
        // contexte, qui garde l'éditeur de nom éventuel.
        let run = |view: &mut GraphView, event: egui::Event, typing: bool| {
            let context = egui::Context::default();
            context.set_fonts(stokesforge_render_egui::font_definitions());
            let mut actions = Vec::new();
            let mut buffer = String::new();
            for events in [Vec::new(), vec![event.clone()]] {
                let first = events.is_empty();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    let field = ui.text_edit_singleline(&mut buffer);
                    if typing && first {
                        field.request_focus();
                    }
                    graph_view_contents(ui, view, &snapshot, graph, &mut actions);
                });
                output.textures_delta.clear();
            }
            (actions, context)
        };
        let group_view = || {
            let mut view = GraphView::new();
            view.select_series_group(Some(group), &spec);
            assert_eq!(view.selected_group(), Some(group));
            view
        };

        let delete = key(egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(
            run(&mut group_view(), delete.clone(), false).0,
            vec![Action::RemovePlotObject {
                graph,
                target: VisibilityTarget::SeriesGroup(group),
            }]
        );
        // Témoin : la même touche sur la première série seule ne vise qu'elle.
        let mut view = GraphView::new();
        view.select_series(Some(SeriesRef::new(0, 1)), &spec);
        assert_eq!(
            run(&mut view, delete.clone(), false).0,
            vec![Action::RemovePlotObject {
                graph,
                target: VisibilityTarget::Series(SeriesRef::new(0, 1)),
            }]
        );
        assert!(
            run(&mut group_view(), delete, true).0.is_empty(),
            "Suppr appartient au champ"
        );

        assert_eq!(
            run(
                &mut group_view(),
                key(egui::Key::ArrowUp, egui::Modifiers::CTRL),
                false
            )
            .0,
            vec![Action::MoveSeriesGroup {
                graph,
                group,
                up: true
            }]
        );
        assert!(
            run(
                &mut group_view(),
                key(egui::Key::ArrowDown, egui::Modifiers::CTRL),
                false
            )
            .0
            .is_empty(),
            "le groupe est déjà en bas"
        );

        let mut f2_view = group_view();
        let (actions, context) = run(
            &mut f2_view,
            key(egui::Key::F2, egui::Modifiers::NONE),
            false,
        );
        assert!(actions.is_empty(), "F2 n'agit pas sur le modèle");
        // #206 : l'éditeur s'ouvre dans le graphe, pas dans l'arbre.
        assert!(!group_rename_pending(&context, graph, group));
        let editor: super::super::graph_text_edit::TextEditor = context
            .data(|data| data.get_temp(text_editor_id(&f2_view)))
            .expect("F2 ouvre l'éditeur de nom dans le graphe");
        assert_eq!(
            editor.target,
            super::super::graph_text_edit::TextEditTarget::GroupName(group)
        );
        // Témoin : F2 sur une série seule ne renomme pas son groupe.
        let mut view = GraphView::new();
        view.select_series(Some(SeriesRef::new(0, 1)), &spec);
        let (_, context) = run(&mut view, key(egui::Key::F2, egui::Modifiers::NONE), false);
        assert!(!group_rename_pending(&context, graph, group));
        assert!(context
            .data(|data| data
                .get_temp::<super::super::graph_text_edit::TextEditor>(text_editor_id(&view)))
            .is_some_and(|editor| editor.target
                != super::super::graph_text_edit::TextEditTarget::GroupName(group)));
        session.shutdown().unwrap();
    }
}

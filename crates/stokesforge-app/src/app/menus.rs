//! Barre de menu et barre d'état de la fenêtre principale :
//! [`StokesForgeApp::menu_bar`] (« Fichier », « Édition », « Classeur »,
//! « Graphique », « Affichage », « Fenêtre ») et [`StokesForgeApp::status_bar`].
//!
//! Le grisage de chaque entrée se décide ici, d'après l'état et l'onglet
//! actif que [`StokesForgeApp::ui`](eframe::App::ui) calcule une fois par image ; les entrées
//! poussent des [`Action`], appliquées après la peinture par la boucle de
//! dispatch d'`app.rs`. L'ordre des panneaux (la barre de menu est le premier
//! `egui::Panel::top`, la barre d'état le premier `egui::Panel::bottom`) reste
//! décidé dans `ui()`.
//!
//! Chaque menu possède sa fonction (#124), appelée dans la même Ui et dans
//! le même ordre ; les demandes de l'Object Manager restent différées après
//! la fermeture du panneau. Aucun scope egui ne dépend du découpage Rust.

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_render_egui::{GraphView, DEFAULT_FIGURE_SIZE};

use crate::i18n;
use crate::state::{active_graph, Action, PlotSettingsTab};
use crate::toolbar::ToolbarId;
use crate::tr;

use super::graph_text_edit::add_annotation_at;
use super::graph_window::{disabled_shape_order_buttons, shape_order_buttons};
use super::grid::{metadata_row_visible, set_metadata_row_visible};
use super::object_manager_ui::{object_node_menu, selected_manager_target, ObjectManagerRequest};
use super::shortcuts::{
    shortcut_text_delete, SHORTCUT_EXPORT_GRAPH, SHORTCUT_IMPORT_TEXT, SHORTCUT_NEW_PROJECT,
    SHORTCUT_OPEN, SHORTCUT_REDO_Y, SHORTCUT_SAVE, SHORTCUT_SAVE_AS, SHORTCUT_UNDO,
};
use super::tab::Tab;
use super::workbook::workbook_menu;
use super::{active_tab_kind, tab_title, StokesForgeApp};

impl StokesForgeApp {
    /// Barre de menu : « Fichier », « Édition », « Classeur » (seulement
    /// quand un classeur est au premier plan), « Graphique » (seulement
    /// quand l'onglet actif est un graphe, voir [`active_graph`]),
    /// « Affichage » et « Fenêtre ».
    ///
    /// « Graphique » ouvre la fenêtre des propriétés du graphe sur chacun de
    /// ses onglets ([`plot_settings_window`](crate::app::plot_settings::plot_settings_window)) — les mêmes pages que le clic
    /// droit sur un composant du graphe ([`graph_view_contents`](crate::app::graph_window::graph_view_contents)), pour
    /// qu'aucun réglage ne soit atteignable **seulement** au clic droit — et
    /// réinitialise le zoom.
    ///
    /// « Fenêtre » porte les deux fonctionnalités qui touchent à la
    /// disposition elle-même plutôt qu'au modèle : maximiser/restaurer
    /// l'onglet actif, et détacher/réattacher le Project Manager en fenêtre
    /// OS séparée — voir la doc de tête de `window_stack.rs` et de
    /// `window_detach.rs`.
    pub(super) fn menu_bar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
        active_tab: Option<Tab>,
    ) {
        let mut object_requests = Vec::new();
        egui::Panel::top("stokesforge_menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                self.file_menu(ui, ctx, actions);
                self.edit_menu(ui, ctx, actions);
                self.workbook_menu(ui, ctx, actions);
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    self.graph_menu(ui, ctx, graph, actions, &mut object_requests);
                }
                self.view_menu(ui, actions);
                self.window_menu(ui, ctx);
            });
        });
        if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
            for request in object_requests {
                self.run_object_manager_request(ctx, graph, request, actions);
            }
        }
    }

    // Chaque fonction garde les mêmes Ui de menu : extraire du Rust ne doit
    // ajouter aucun scope egui ni déplacer l'application différée des demandes.
    fn file_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        ui.menu_button(tr!("menu-file"), |ui| {
            if ui
                .add_enabled(
                    self.state.can_new_project(),
                    egui::Button::new(tr!("action-new-project"))
                        .shortcut_text(ctx.format_shortcut(&SHORTCUT_NEW_PROJECT)),
                )
                .on_hover_text(tr!("action-new-project", "hint"))
                .clicked()
            {
                actions.push(Action::NewProject {
                    pending_edits: false,
                });
                ui.close();
            }
            if ui
                .button(tr!("action-add-workbook"))
                .on_hover_text(tr!("action-add-workbook", "hint"))
                .clicked()
            {
                actions.push(Action::AddWorkbook);
                ui.close();
            }
            if ui
                .button(tr!("action-add-graph"))
                .on_hover_text(tr!("action-add-graph", "hint"))
                .clicked()
            {
                actions.push(Action::AddGraph);
                ui.close();
            }
            if ui
                .add(egui::Button::new(tr!("action-import")).shortcut_text(SHORTCUT_IMPORT_TEXT))
                .on_hover_text(tr!("action-import", "hint"))
                .clicked()
            {
                self.open_import_dialog();
                ui.close();
            }
            let active_sheet = self
                .active_book()
                .and_then(|book| self.state.selected_sheet_in(book));
            if ui
                .add_enabled(
                    active_sheet.is_some_and(|sheet| self.state.can_refresh_from_source(sheet)),
                    egui::Button::new(tr!("action-refresh-from-source")),
                )
                .on_hover_text(tr!("action-refresh-from-source", "hint"))
                .clicked()
            {
                if let Some(sheet) = active_sheet {
                    actions.push(Action::RefreshFromSource { sheet });
                }
                ui.close();
            }
            if ui
                .add_enabled(
                    self.state.can_reopen_last_import(),
                    egui::Button::new(tr!("action-reopen-last-import")),
                )
                .on_hover_text(tr!("action-reopen-last-import", "hint"))
                .clicked()
            {
                actions.push(Action::ReopenLastImport);
                ui.close();
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new(tr!("action-open"))
                        .shortcut_text(ctx.format_shortcut(&SHORTCUT_OPEN)),
                )
                .clicked()
            {
                self.choose_open_path(actions);
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new(tr!("action-save"))
                        .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE)),
                )
                .clicked()
            {
                actions.push(Action::Save);
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new(tr!("action-save-as"))
                        .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE_AS)),
                )
                .clicked()
            {
                self.choose_save_path(actions);
                ui.close();
            }
            ui.separator();
            ui.menu_button(tr!("menu-language"), |ui| {
                for language in [i18n::Language::English, i18n::Language::French] {
                    let current = i18n::language() == language;
                    if ui
                        .selectable_label(current, language.native_name())
                        .clicked()
                    {
                        self.choose_language(language);
                        ui.close();
                    }
                }
            });
            ui.separator();
            if ui.button(tr!("action-quit")).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                ui.close();
            }
        });
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        ui.menu_button(tr!("menu-edit"), |ui| {
            // Le libellé complet (« Undo Edit Cells in Column A ») et le
            // grisage sont deux lignes distinctes ici ; Qt les tenait
            // ensemble dans une seule `QAction`.
            let undo = self
                .state
                .undo_label()
                .unwrap_or_else(|| tr!("history-undo"));
            if ui
                .add_enabled(
                    self.state.can_undo(),
                    egui::Button::new(undo).shortcut_text(ctx.format_shortcut(&SHORTCUT_UNDO)),
                )
                .clicked()
            {
                actions.push(Action::Undo);
                ui.close();
            }
            let redo = self
                .state
                .redo_label()
                .unwrap_or_else(|| tr!("history-redo"));
            if ui
                .add_enabled(
                    self.state.can_redo(),
                    egui::Button::new(redo).shortcut_text(ctx.format_shortcut(&SHORTCUT_REDO_Y)),
                )
                .clicked()
            {
                actions.push(Action::Redo);
                ui.close();
            }
        });
    }

    fn workbook_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if let Some(book) = self.active_book() {
            if let Some(sheet) = self.state.selected_sheet_in(book) {
                ui.menu_button(tr!("menu-workbook"), |ui| {
                    workbook_menu(ui, ctx, &self.state, book, sheet, actions);
                });
            }
        }
    }

    fn graph_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        graph: NodeId,
        actions: &mut Vec<Action>,
        object_requests: &mut Vec<ObjectManagerRequest>,
    ) {
        ui.menu_button(tr!("menu-graph"), |ui| {
            let spec = crate::state::current_plot_spec(self.state.tree(), graph);
            let target = self
                .read_graph_view(graph, selected_manager_target)
                .flatten();
            ui.menu_button(tr!("menu-selected-content"), |ui| {
                if let (Some(target), Some(spec)) = (target, spec.as_ref()) {
                    object_node_menu(ui, graph, target, spec, actions, object_requests);
                } else {
                    ui.add_enabled(false, egui::Button::new(tr!("action-properties")));
                    ui.add_enabled(false, egui::Button::new(tr!("action-rename-series")));
                    ui.add_enabled(false, egui::Button::new(tr!("action-delete-content")));
                }
            });
            ui.separator();
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
            ui.separator();
            if ui
                .button(tr!("action-reset-zoom"))
                .on_hover_text(tr!("action-reset-zoom", "hint"))
                .clicked()
            {
                if let Some(view) = self.graph_views.get_mut(&graph) {
                    view.reset_view();
                }
                ui.close();
            }
            let moved = crate::state::current_plot_spec(self.state.tree(), graph)
                .is_some_and(|spec| spec.has_text_anchors());
            if ui
                .add_enabled(moved, egui::Button::new(tr!("action-reset-text-positions")))
                .on_hover_text(tr!("action-reset-text-positions", "hint"))
                .on_disabled_hover_text(tr!("action-reset-text-positions", "disabled"))
                .clicked()
            {
                actions.push(Action::ResetTextAnchors { graph });
                ui.close();
            }
            ui.separator();
            // Textes libres (issue #35) : le pendant, dans la
            // barre de menus, du clic droit et de Suppr.
            let selected = self
                .read_graph_view(graph, GraphView::selected_annotation)
                .flatten();
            if ui
                .button(tr!("action-add-text"))
                .on_hover_text(tr!("action-add-text", "hint"))
                .clicked()
            {
                let tree = self.state.tree().clone();
                if let Some(view) = self.graph_views.get_mut(&graph) {
                    if let Some(point) = view.panel_center_data() {
                        add_annotation_at(ctx, view, &tree, graph, point);
                    }
                }
                ui.close();
            }
            if ui
                .add_enabled(
                    selected.is_some(),
                    egui::Button::new(tr!("action-delete-text"))
                        .shortcut_text(shortcut_text_delete()),
                )
                .clicked()
            {
                if let Some(target) = selected {
                    actions.push(Action::RemoveAnnotation { graph, target });
                }
                if let Some(view) = self.graph_views.get_mut(&graph) {
                    view.clear_annotation_selection();
                }
                ui.close();
            }
            ui.separator();
            // Objets graphiques (issue #76) : l'ordre
            // d'empilement et la suppression de l'objet
            // sélectionné ; les outils de tracé sont dans la barre
            // d'outils, comme l'outil Texte.
            let object = self
                .graph_views
                .get(&graph)
                .and_then(GraphView::selected_shape);
            if let Some(view) = self.graph_views.get_mut(&graph) {
                let tree = self.state.tree().clone();
                match object {
                    Some(object) => shape_order_buttons(ui, view, &tree, graph, object, actions),
                    None => disabled_shape_order_buttons(ui),
                }
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new(tr!("action-export-graph"))
                        .shortcut_text(ctx.format_shortcut(&SHORTCUT_EXPORT_GRAPH)),
                )
                .on_hover_text(tr!("action-export-graph", "hint"))
                .clicked()
            {
                self.request_graph_export(graph, actions);
                ui.close();
            }
            ui.menu_button(tr!("menu-graph-png-resolution"), |ui| {
                let current = self.state.png_export_scale();
                for scale in crate::state::PNG_EXPORT_SCALES {
                    let (w, h) = DEFAULT_FIGURE_SIZE;
                    let (width, height) = ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
                    let label = tr!(
                        "menu-graph-png-resolution-option",
                        scale = scale,
                        width = width,
                        height = height
                    );
                    if ui.radio(current == scale, label).clicked() {
                        actions.push(Action::SetPngExportScale(scale));
                        ui.close();
                    }
                }
            });
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.menu_button(tr!("menu-view"), |ui| {
            ui.checkbox(&mut self.pm_open, tr!("project-manager-title"));
            ui.checkbox(&mut self.script_open, tr!("script-title"));
            let mut open = self.state.object_manager_open();
            if ui
                .checkbox(&mut open, tr!("action-object-manager"))
                .on_hover_text(tr!("action-object-manager", "hint"))
                .changed()
            {
                actions.push(Action::SetObjectManagerOpen(open));
            }
            let mut show_texts = self.state.object_manager_show_texts();
            if ui
                .checkbox(&mut show_texts, tr!("object-manager-show-texts"))
                .on_hover_text(tr!("object-manager-show-texts", "hint"))
                .changed()
            {
                actions.push(Action::SetObjectManagerShowTexts(show_texts));
            }
            let mut show_scales = self.state.object_manager_show_scales();
            if ui
                .checkbox(&mut show_scales, tr!("object-manager-show-scales"))
                .on_hover_text(tr!("object-manager-show-scales", "hint"))
                .changed()
            {
                actions.push(Action::SetObjectManagerShowScales(show_scales));
            }
            ui.separator();
            for id in ToolbarId::ALL {
                let mut visible = self.toolbars.layout().is_visible(id);
                if ui
                    .checkbox(&mut visible, id.label())
                    .on_hover_text(id.hint())
                    .changed()
                {
                    self.toolbars.set_visible(id, visible);
                }
            }
        });
    }

    fn window_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button(tr!("menu-window"), |ui| {
            let mut show_metadata = metadata_row_visible(ctx);
            if ui
                .checkbox(&mut show_metadata, tr!("menu-window-show-metadata-row"))
                .on_hover_text(tr!("menu-window-show-metadata-row", "hint"))
                .changed()
            {
                set_metadata_row_visible(ctx, show_metadata);
            }
            ui.separator();
            if let Some(tab) = self.maximized_tab {
                if ui
                    .button(tr!(
                        "action-restore-window",
                        title = tab_title(&self.state, tab)
                    ))
                    .on_hover_text(tr!("action-restore-window", "hint"))
                    .clicked()
                {
                    self.maximized_tab = None;
                    ui.close();
                }
            } else {
                // La fenêtre flottante la plus récemment focalisée —
                // dernier élément de `floating_tabs`, voir la doc de
                // tête de `window_stack.rs`. Le Project Manager n'y
                // figure jamais, ce n'est pas une fenêtre flottante,
                // donc rien de spécial à exclure ici.
                let active = self.floating_tabs.last().copied();
                let can_maximize = active.is_some();
                if ui
                    .add_enabled(
                        can_maximize,
                        egui::Button::new(tr!("action-maximize-window")),
                    )
                    .on_hover_text(tr!("action-maximize-window", "hint"))
                    .clicked()
                {
                    self.maximized_tab = active;
                    ui.close();
                }
            }
            ui.separator();
            if self.pm_detached {
                if ui
                    .button(tr!("action-reattach-project-manager"))
                    .on_hover_text(tr!("action-reattach-project-manager", "hint"))
                    .clicked()
                {
                    self.reattach_project_manager();
                    ui.close();
                }
            } else if ui
                .button(tr!("action-detach-project-manager"))
                .on_hover_text(tr!("action-detach-project-manager", "hint"))
                .clicked()
            {
                self.detach_project_manager();
                ui.close();
            }
            // Pas d'« Attacher à gauche/à droite » ici : le Project
            // Manager est un panneau fixe, figé à gauche, depuis
            // cette tâche (voir la doc de tête d'`app.rs`, § attacher
            // à gauche ou à droite — le côté droit et le
            // repositionnement par glisser sont un travail futur
            // délibérément hors périmètre).
        });
    }

    /// Barre d'état : dernier message, révision.
    ///
    /// Les dimensions (lignes × colonnes) qu'elle affichait ont disparu avec
    /// « la » feuille sélectionnée globale : chaque onglet de classeur les
    /// montre pour sa propre feuille, dans [`book_contents`](crate::app::workbook::book_contents).
    pub(super) fn status_bar(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::bottom("stokesforge_status")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match self.state.status() {
                        Some(status) if status.is_error => {
                            ui.colored_label(ui.visuals().error_fg_color, &status.text);
                        }
                        Some(status) => {
                            ui.label(&status.text);
                        }
                        None => {
                            ui.weak(tr!("status-ready"));
                        }
                    }
                    if self.state.can_reopen_last_import()
                        && self.state.import_state().is_none()
                        && ui.link(tr!("action-reopen-last-import")).clicked()
                    {
                        actions.push(Action::ReopenLastImport);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(tr!(
                            "status-revision",
                            revision = self.state.snapshot().revision
                        ));
                    });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::super::project_manager::tests::find_text_center;
    use super::*;
    use crate::state::AppState;
    use crate::theme;
    use stokesforge_core::{Column, NodeId, Project};
    use stokesforge_plot::{AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesSpec};
    use stokesforge_session::Session;

    /// Un classeur (une feuille, deux colonnes) et un graphe qui les trace,
    /// avec un texte libre. `StokesForgeApp::new` ouvre le classeur puis le
    /// graphe : le graphe est au premier plan.
    fn banc() -> (Session, StokesForgeApp, NodeId) {
        let mut project = Project::new("Menus").unwrap();
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
        panel
            .annotations
            .push(AnnotationSpec::new("Texte", 0.5, 0.5));
        let graph = project
            .add_graph(root, None, PlotSpec::empty("Figure").with_panels(panel))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stokesforge".into()).unwrap();
        (session, StokesForgeApp::new(state), graph)
    }

    /// Un fichier importable, et l'état après son import dans un classeur
    /// neuf : « Reopen Last Import » a de quoi rouvrir, et ce classeur a une
    /// source à relire.
    fn importe(state: &mut AppState, dir: &tempfile::TempDir) {
        let file = dir.path().join("mesure.csv");
        std::fs::write(&file, "1,10\n2,20\n").unwrap();
        state.dispatch(Action::ImportFiles {
            destination: crate::state::ImportDestination::NewBookForAll,
            recipe: stokesforge_import::ImportRecipe::new("Sans titre"),
            paths: vec![file],
        });
    }

    /// Une entrée de la barre de menu (titre ou contenu d'un menu ouvert),
    /// telle qu'egui la décrit à l'accessibilité : son libellé, son grisage,
    /// l'état d'une case à cocher et le centre de son rectangle.
    #[derive(Debug)]
    struct Entree {
        label: String,
        enabled: bool,
        toggled: Option<bool>,
        center: egui::Pos2,
    }

    /// Les textes de raccourci qu'un bouton de menu ajoute à son libellé
    /// d'accessibilité (« Save Ctrl+S ») : retirés, pour comparer au seul
    /// libellé du catalogue.
    fn raccourcis() -> Vec<String> {
        let ctx = egui::Context::default();
        let mut textes: Vec<String> = [
            SHORTCUT_NEW_PROJECT,
            SHORTCUT_OPEN,
            SHORTCUT_SAVE,
            SHORTCUT_SAVE_AS,
            SHORTCUT_UNDO,
            SHORTCUT_REDO_Y,
            SHORTCUT_EXPORT_GRAPH,
        ]
        .iter()
        .map(|shortcut| ctx.format_shortcut(shortcut))
        .collect();
        textes.push(SHORTCUT_IMPORT_TEXT.to_string());
        textes.push(shortcut_text_delete());
        textes
    }

    fn entrees(output: &egui::FullOutput) -> Vec<Entree> {
        let raccourcis = raccourcis();
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("l'accessibilité est activée par `peint`");
        update
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                let mut label = node.label()?.to_string();
                if let Some(court) = raccourcis
                    .iter()
                    .map(|r| format!(" {r}"))
                    .chain([" ⏵".to_string()])
                    .find_map(|suffixe| label.strip_suffix(&suffixe).map(str::to_string))
                {
                    label = court;
                }
                let bounds = node.bounds()?;
                Some(Entree {
                    label,
                    enabled: !node.is_disabled(),
                    toggled: node
                        .toggled()
                        .map(|toggled| toggled == egui::accesskit::Toggled::True),
                    center: egui::pos2(
                        ((bounds.x0 + bounds.x1) / 2.0) as f32,
                        ((bounds.y0 + bounds.y1) / 2.0) as f32,
                    ),
                })
            })
            .collect()
    }

    fn entree<'a>(entrees: &'a [Entree], label: &str) -> Option<&'a Entree> {
        entrees.iter().find(|entree| entree.label == label)
    }

    fn active<'a>(entrees: &'a [Entree], label: &str) -> &'a Entree {
        entree(entrees, label).unwrap_or_else(|| panic!("entrée {label:?} absente de {entrees:#?}"))
    }

    /// Une image de la seule barre de menu, sur l'onglet actif `tab`.
    fn peint(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<Action>) {
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx().clone();
                app.menu_bar(ui, &ctx, &mut actions, tab);
            },
        );
        output.textures_delta.clear();
        (output, actions)
    }

    fn contexte() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        ctx
    }

    /// Les titres de la barre, menus fermés.
    fn titres(app: &mut StokesForgeApp, tab: Option<Tab>) -> Vec<String> {
        let ctx = contexte();
        let (output, _) = peint(app, &ctx, tab, Vec::new());
        entrees(&output).into_iter().map(|e| e.label).collect()
    }

    /// Clique `pos`, en recueillant les actions de toutes les images du clic.
    fn clique(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        pos: egui::Pos2,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            actions.extend(peint(app, ctx, tab, events).1);
        }
        actions
    }

    /// Ouvre le menu `titre` et rend son contenu, avec le contexte où il est
    /// ouvert (pour y cliquer ensuite).
    fn ouvre(
        app: &mut StokesForgeApp,
        tab: Option<Tab>,
        titre: &str,
    ) -> (egui::Context, Vec<Entree>) {
        let ctx = contexte();
        let (output, _) = peint(app, &ctx, tab, Vec::new());
        let pos = active(&entrees(&output), titre).center;
        clique(app, &ctx, tab, pos);
        let (output, _) = peint(app, &ctx, tab, Vec::new());
        let contenu = entrees(&output);
        assert!(
            contenu.len() > titres(app, tab).len(),
            "le menu {titre:?} doit s'être ouvert : {contenu:#?}"
        );
        (ctx, contenu)
    }

    /// Les extractions de menus gardent leur ordre ; changer le contenu
    /// actif remplace seulement le menu propre au classeur ou au graphique.
    #[test]
    fn les_menus_gardent_leur_ordre_selon_le_contenu_actif() {
        let (session, mut app, graph) = banc();
        assert_eq!(
            titres(&mut app, Some(Tab::Graph(graph))),
            [
                tr!("menu-file"),
                tr!("menu-edit"),
                tr!("menu-graph"),
                tr!("menu-view"),
                tr!("menu-window")
            ],
        );
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Book(book));
        assert_eq!(
            titres(&mut app, Some(Tab::Book(book))),
            [
                tr!("menu-file"),
                tr!("menu-edit"),
                tr!("menu-workbook"),
                tr!("menu-view"),
                tr!("menu-window")
            ],
            "témoin : le menu propre au contenu actif change à la même place",
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn le_menu_fichier_grise_selon_le_classeur_actif_et_le_dernier_import() {
        // Sans import : rien à relire ni à rouvrir.
        let (session, mut app, _) = banc();
        let tab = app.floating_tabs.last().copied();
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-file"));
        assert!(active(&menu, &tr!("action-save")).enabled, "témoin");
        assert!(active(&menu, &tr!("action-add-workbook")).enabled);
        assert!(!active(&menu, &tr!("action-reopen-last-import")).enabled);
        assert!(!active(&menu, &tr!("action-refresh-from-source")).enabled);
        assert_eq!(
            active(&menu, &tr!("action-new-project")).enabled,
            app.state.can_new_project()
        );
        session.shutdown().unwrap();

        // Après un import dans un classeur neuf : la relecture suit le
        // classeur au premier plan, pas la seule existence d'une source.
        let (session, app, _) = banc();
        let mut state = app.state;
        let dir = tempfile::tempdir().unwrap();
        importe(&mut state, &dir);
        let imported = state.books().last().unwrap().0;
        let mut app = StokesForgeApp::new(state);
        let tab = app.floating_tabs.last().copied();
        assert!(matches!(tab, Some(Tab::Graph(_))));
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-file"));
        assert!(active(&menu, &tr!("action-reopen-last-import")).enabled);
        assert!(
            !active(&menu, &tr!("action-refresh-from-source")).enabled,
            "un graphe au premier plan n'a pas de feuille à relire"
        );
        app.open_tab(Tab::Book(imported));
        let tab = app.floating_tabs.last().copied();
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-file"));
        assert!(active(&menu, &tr!("action-refresh-from-source")).enabled);

        // Une entrée cliquée pousse son action, rien de plus.
        let pos = active(&menu, &tr!("action-add-workbook")).center;
        assert_eq!(clique(&mut app, &ctx, tab, pos), vec![Action::AddWorkbook]);
        session.shutdown().unwrap();
    }

    #[test]
    fn le_menu_edition_nomme_et_grise_annuler_et_retablir() {
        let (session, mut app, _) = banc();
        let tab = app.floating_tabs.last().copied();
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-edit"));
        assert!(!active(&menu, &tr!("history-undo")).enabled);
        assert!(!active(&menu, &tr!("history-redo")).enabled);

        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[0].0;
        app.state.dispatch(Action::CommitCell {
            column,
            row: 0,
            text: "42".into(),
        });
        let undo = app.state.undo_label().expect("une commande à annuler");
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-edit"));
        assert!(active(&menu, &undo).enabled, "le libellé nomme la commande");
        assert!(entree(&menu, &tr!("history-undo")).is_none());
        assert!(!active(&menu, &tr!("history-redo")).enabled);

        app.state.dispatch(Action::Undo);
        let redo = app.state.redo_label().expect("une commande à rétablir");
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-edit"));
        assert!(!active(&menu, &tr!("history-undo")).enabled);
        let pos = active(&menu, &redo).center;
        assert_eq!(clique(&mut app, &ctx, tab, pos), vec![Action::Redo]);
        session.shutdown().unwrap();
    }

    #[test]
    fn le_menu_classeur_n_existe_qu_avec_un_classeur_au_premier_plan() {
        let (session, mut app, graph) = banc();
        let book = app.state.books()[0].0;
        assert_eq!(app.floating_tabs.last(), Some(&Tab::Graph(graph)));
        let menus = titres(&mut app, Some(Tab::Graph(graph)));
        assert!(menus.contains(&tr!("menu-file")), "témoin : {menus:?}");
        assert!(!menus.contains(&tr!("menu-workbook")), "{menus:?}");

        app.open_tab(Tab::Book(book));
        let (_, menu) = ouvre(&mut app, Some(Tab::Book(book)), &tr!("menu-workbook"));
        assert!(
            menu.iter()
                .any(|entree| entree.label != tr!("menu-workbook")
                    && !titres(&mut app, Some(Tab::Book(book))).contains(&entree.label)),
            "le menu Classeur a un contenu : {menu:#?}"
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn le_menu_graphique_suit_le_graphe_actif_et_sa_selection() {
        let (session, mut app, graph) = banc();
        let book = app.state.books()[0].0;
        let menus = titres(&mut app, Some(Tab::Book(book)));
        assert!(!menus.contains(&tr!("menu-graph")), "{menus:?}");

        let tab = Some(Tab::Graph(graph));
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-graph"));
        assert!(active(&menu, &tr!("action-add-text")).enabled, "témoin");
        assert!(active(&menu, &tr!("action-export-graph")).enabled);
        assert!(!active(&menu, &tr!("action-delete-text")).enabled);
        assert!(
            !active(&menu, &tr!("action-reset-text-positions")).enabled,
            "aucun texte déplacé"
        );

        let target = AnnotationRef {
            row: 0,
            column: 0,
            index: 0,
        };
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_annotation(Some(target), &spec));
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-graph"));
        let pos = active(&menu, &tr!("action-delete-text")).center;
        assert!(active(&menu, &tr!("action-delete-text")).enabled);
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::RemoveAnnotation { graph, target }]
        );
        assert_eq!(
            app.read_graph_view(graph, GraphView::selected_annotation)
                .flatten(),
            None,
            "la sélection supprimée est oubliée"
        );
        session.shutdown().unwrap();
    }

    /// « Contenu sélectionné » reprend le menu de l'Object Manager : grisé
    /// sans sélection, et ses demandes passent par
    /// `run_object_manager_request`, comme celles de l'arbre.
    #[test]
    fn le_contenu_selectionne_du_menu_graphique_passe_par_l_object_manager() {
        use crate::object_manager::VisibilityTarget;
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let ouvre_le_contenu = |app: &mut StokesForgeApp| {
            let (ctx, menu) = ouvre(app, tab, &tr!("menu-graph"));
            let pos = active(&menu, &tr!("menu-selected-content")).center;
            clique(app, &ctx, tab, pos);
            let (output, _) = peint(app, &ctx, tab, Vec::new());
            (ctx, entrees(&output))
        };
        let (_, menu) = ouvre_le_contenu(&mut app);
        assert!(!active(&menu, &tr!("action-properties")).enabled);

        let series = stokesforge_plot::SeriesRef::new(0, 0);
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_series(Some(series), &spec));
        let (ctx, menu) = ouvre_le_contenu(&mut app);
        let properties = active(&menu, &tr!("action-properties"));
        assert!(properties.enabled);
        let pos = properties.center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::OpenObjectProperties {
                graph,
                target: VisibilityTarget::Series(series),
            }]
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn le_menu_affichage_reflete_et_bascule_panneaux_et_barres() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        for open in [false, true] {
            app.state.set_object_manager_open_for_test(open);
            let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-view"));
            let case = active(&menu, &tr!("action-object-manager"));
            assert_eq!(case.toggled, Some(open));
            assert!(
                entree(&menu, &tr!("project-manager-title")).is_some()
                    && entree(&menu, &tr!("script-title")).is_some(),
                "les trois panneaux sont accessibles dans Affichage"
            );
            assert_eq!(
                active(&menu, &tr!("object-manager-show-texts")).toggled,
                Some(app.state.object_manager_show_texts())
            );
            assert_eq!(
                active(&menu, &tr!("object-manager-show-scales")).toggled,
                Some(app.state.object_manager_show_scales())
            );
            for id in ToolbarId::ALL {
                assert_eq!(
                    active(&menu, &id.label()).toggled,
                    Some(app.toolbars.layout().is_visible(id)),
                    "{id:?}"
                );
            }
            let pos = case.center;
            assert_eq!(
                clique(&mut app, &ctx, tab, pos),
                vec![Action::SetObjectManagerOpen(!open)]
            );
        }

        // Une barre se masque depuis le menu, sans passer par une action.
        let id = ToolbarId::ALL[0];
        let visible = app.toolbars.layout().is_visible(id);
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-view"));
        let pos = active(&menu, &id.label()).center;
        assert!(clique(&mut app, &ctx, tab, pos).is_empty());
        assert_eq!(app.toolbars.layout().is_visible(id), !visible);
        session.shutdown().unwrap();
    }

    #[test]
    fn le_menu_fenetre_suit_la_fenetre_active_et_le_project_manager() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-window"));
        assert!(active(&menu, &tr!("action-detach-project-manager")).enabled);
        assert!(entree(&menu, &tr!("action-reattach-project-manager")).is_none());
        let maximize = active(&menu, &tr!("action-maximize-window"));
        assert!(maximize.enabled);
        let pos = maximize.center;
        assert!(clique(&mut app, &ctx, tab, pos).is_empty());
        assert_eq!(
            app.maximized_tab,
            Some(Tab::Graph(graph)),
            "maximise la fenêtre au premier plan"
        );

        let restore = tr!(
            "action-restore-window",
            title = tab_title(&app.state, Tab::Graph(graph))
        );
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-window"));
        assert!(entree(&menu, &tr!("action-maximize-window")).is_none());
        let pos = active(&menu, &restore).center;
        clique(&mut app, &ctx, tab, pos);
        assert_eq!(app.maximized_tab, None);

        // Aucune fenêtre flottante : rien à maximiser.
        app.floating_tabs.clear();
        let (_, menu) = ouvre(&mut app, None, &tr!("menu-window"));
        assert!(!active(&menu, &tr!("action-maximize-window")).enabled);

        // Project Manager détaché : l'entrée devient « réattacher ».
        app.pm_detached = true;
        let (_, menu) = ouvre(&mut app, None, &tr!("menu-window"));
        assert!(entree(&menu, &tr!("action-detach-project-manager")).is_none());
        assert!(active(&menu, &tr!("action-reattach-project-manager")).enabled);
        session.shutdown().unwrap();
    }

    /// Une image de la seule barre d'état.
    fn barre_d_etat(app: &StokesForgeApp, ctx: &egui::Context) -> egui::FullOutput {
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 600.),
                )),
                ..Default::default()
            },
            |ui| app.status_bar(ui, &mut actions),
        );
        output.textures_delta.clear();
        assert!(actions.is_empty());
        output
    }

    /// La couleur du texte peint qui contient `needle`, s'il est peint.
    fn couleur_du_texte(output: &egui::FullOutput, needle: &str) -> Option<egui::Color32> {
        output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle) => Some(
                    shape
                        .galley
                        .job
                        .sections
                        .first()
                        .map(|section| section.format.color)
                        .filter(|color| *color != egui::Color32::PLACEHOLDER)
                        .unwrap_or(shape.fallback_color),
                ),
                _ => None,
            })
    }

    /// « Prêt » sans message, la révision à droite, et une erreur dans la
    /// couleur d'erreur du thème — un simple avis, non.
    #[test]
    fn la_barre_d_etat_dit_pret_la_revision_et_distingue_l_erreur() {
        let (session, mut app, _) = banc();
        let ctx = contexte();
        let output = barre_d_etat(&app, &ctx);
        let revision = tr!("status-revision", revision = app.state.snapshot().revision);
        assert!(couleur_du_texte(&output, &tr!("status-ready")).is_some());
        assert!(couleur_du_texte(&output, &revision).is_some());
        assert!(
            couleur_du_texte(&output, &tr!("action-reopen-last-import")).is_none(),
            "rien à rouvrir, pas de lien"
        );
        let erreur = ctx.global_style().visuals.error_fg_color;

        app.state.notice("Avis".into());
        let output = barre_d_etat(&app, &ctx);
        assert!(couleur_du_texte(&output, &tr!("status-ready")).is_none());
        assert_ne!(couleur_du_texte(&output, "Avis"), Some(erreur), "témoin");

        app.state.set_path("/chemin/introuvable/projet.stf".into());
        app.state.dispatch(Action::Load);
        let status = app.state.status().expect("un chargement raté se dit");
        assert!(status.is_error);
        let text = status.text.clone();
        let output = barre_d_etat(&app, &ctx);
        assert_eq!(couleur_du_texte(&output, &text), Some(erreur));
        session.shutdown().unwrap();
    }

    /// Le lien « rouvrir le dernier import » se retire pendant que
    /// l'assistant d'import est ouvert.
    #[test]
    fn le_lien_de_statut_se_retire_pendant_l_import() {
        let (session, app, _) = banc();
        let mut state = app.state;
        let dir = tempfile::tempdir().unwrap();
        importe(&mut state, &dir);
        let mut app = StokesForgeApp::new(state);
        let ctx = contexte();
        let link = tr!("action-reopen-last-import");
        assert!(couleur_du_texte(&barre_d_etat(&app, &ctx), &link).is_some());
        app.state.open_import(None);
        assert!(couleur_du_texte(&barre_d_etat(&app, &ctx), &link).is_none());
        session.shutdown().unwrap();
    }

    /// Le lien de la barre d'état produit exactement l'action du menu Fichier.
    #[test]
    fn le_lien_de_statut_reprend_le_dernier_import() {
        let (session, app, _) = banc();
        let mut state = app.state;
        let dir = tempfile::tempdir().unwrap();
        importe(&mut state, &dir);
        let app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        crate::theme::install(&context);
        let mut actions = Vec::new();
        let mut paint = |events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.status_bar(ui, &mut actions),
            );
            output.textures_delta.clear();
            output
        };
        let output = paint(Vec::new());
        let pos = find_text_center(&output.shapes, &tr!("action-reopen-last-import"));
        paint(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            paint(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            }]);
        }
        assert_eq!(actions, vec![Action::ReopenLastImport]);
        session.shutdown().unwrap();
    }
}

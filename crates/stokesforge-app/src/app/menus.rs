//! Barre de menu et barre d'état de la fenêtre principale :
//! [`StokesForgeApp::menu_bar`] (« Fichier », « Édition », « Classeur »,
//! « Graphique », « Affichage », « Fenêtre ») et [`StokesForgeApp::status_bar`].
//!
//! Le grisage de chaque entrée se décide ici, d'après l'état et l'onglet
//! actif que [`StokesForgeApp::ui`] calcule une fois par image ; les entrées
//! poussent des [`Action`], appliquées après la peinture par la boucle de
//! dispatch d'`app.rs`. L'ordre des panneaux (la barre de menu est le premier
//! `egui::Panel::top`, la barre d'état le premier `egui::Panel::bottom`) reste
//! décidé dans `ui()`.
//!
//! Sorti d'`app.rs` tel quel (#122) ; la redécoupe de `menu_bar` est #124.

use eframe::egui;
use stokesforge_render_egui::{GraphView, DEFAULT_FIGURE_SIZE};

use crate::i18n;
use crate::state::{active_graph, Action, PlotSettingsTab};
use crate::toolbar::ToolbarId;
use crate::tr;

use super::graph_text_edit::add_annotation_at;
use super::graph_window::{disabled_shape_order_buttons, shape_order_buttons};
use super::grid::{metadata_row_visible, set_metadata_row_visible};
use super::object_manager_ui::{object_node_menu, selected_manager_target};
use super::shortcuts::{
    shortcut_text_delete, SHORTCUT_EXPORT_GRAPH, SHORTCUT_IMPORT_TEXT, SHORTCUT_NEW_PROJECT,
    SHORTCUT_OPEN, SHORTCUT_REDO_Y, SHORTCUT_SAVE, SHORTCUT_SAVE_AS, SHORTCUT_UNDO,
};
use super::tab::Tab;
use super::workbook::workbook_menu;
use super::{active_tab_kind, tab_title, StokesForgeApp};

impl StokesForgeApp {
    /// Barre de menu : « Fichier », « Édition », « Graphique » (seulement
    /// quand l'onglet actif est un graphe, voir [`active_graph`]) et
    /// « Fenêtre ».
    ///
    /// « Graphique » ouvre la fenêtre des propriétés du graphe sur chacun de
    /// ses onglets ([`plot_settings_window`]) — les mêmes pages que le clic
    /// droit sur un composant du graphe ([`graph_view_contents`]), pour
    /// qu'aucun réglage ne soit atteignable **seulement** au clic droit — et
    /// réinitialise le zoom.
    ///
    /// « Fenêtre » porte les deux fonctionnalités qui touchent à la
    /// disposition elle-même plutôt qu'au modèle : maximiser/restaurer
    /// l'onglet actif, et détacher/réattacher le Project Manager en fenêtre
    /// OS séparée — voir la doc de tête du module.
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
                        .add(
                            egui::Button::new(tr!("action-import"))
                                .shortcut_text(SHORTCUT_IMPORT_TEXT),
                        )
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
                            active_sheet
                                .is_some_and(|sheet| self.state.can_refresh_from_source(sheet)),
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
                            egui::Button::new(undo)
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_UNDO)),
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
                            egui::Button::new(redo)
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_REDO_Y)),
                        )
                        .clicked()
                    {
                        actions.push(Action::Redo);
                        ui.close();
                    }
                });
                if let Some(book) = self.active_book() {
                    if let Some(sheet) = self.state.selected_sheet_in(book) {
                        ui.menu_button(tr!("menu-workbook"), |ui| {
                            workbook_menu(ui, ctx, &self.state, book, sheet, actions);
                        });
                    }
                }
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    ui.menu_button(tr!("menu-graph"), |ui| {
                        let spec = crate::state::current_plot_spec(self.state.tree(), graph);
                        let target = self
                            .read_graph_view(graph, selected_manager_target)
                            .flatten();
                        ui.menu_button(tr!("menu-selected-content"), |ui| {
                            if let (Some(target), Some(spec)) = (target, spec.as_ref()) {
                                object_node_menu(
                                    ui,
                                    graph,
                                    target,
                                    spec,
                                    actions,
                                    &mut object_requests,
                                );
                            } else {
                                ui.add_enabled(false, egui::Button::new(tr!("action-properties")));
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(tr!("action-rename-series")),
                                );
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(tr!("action-delete-content")),
                                );
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
                            .add_enabled(
                                moved,
                                egui::Button::new(tr!("action-reset-text-positions")),
                            )
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
                                Some(object) => {
                                    shape_order_buttons(ui, view, &tree, graph, object, actions)
                                }
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
                                let (width, height) =
                                    ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
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
                ui.menu_button(tr!("menu-view"), |ui| {
                    let mut open = self.state.object_manager_open();
                    if ui
                        .checkbox(&mut open, tr!("action-object-manager"))
                        .on_hover_text(tr!("action-object-manager", "hint"))
                        .changed()
                    {
                        actions.push(Action::SetObjectManagerOpen(open));
                    }
                    if open
                        && ui
                            .button(if self.object_manager_floating {
                                tr!("action-object-manager-dock")
                            } else {
                                tr!("action-object-manager-float")
                            })
                            .clicked()
                    {
                        self.object_manager_floating = !self.object_manager_floating;
                        ui.close();
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
                        // tête du module, § z-order. Le Project Manager n'y
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
                    // cette tâche (voir la doc de tête du module, § attacher
                    // à gauche ou à droite — le côté droit et le
                    // repositionnement par glisser sont un travail futur
                    // délibérément hors périmètre).
                });
            });
        });
        if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
            for request in object_requests {
                self.run_object_manager_request(ctx, graph, request, actions);
            }
        }
    }

    /// Barre d'état : dernier message, révision.
    ///
    /// Les dimensions (lignes × colonnes) qu'elle affichait ont disparu avec
    /// « la » feuille sélectionnée globale : chaque onglet de classeur les
    /// montre pour sa propre feuille, dans [`book_contents`].
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

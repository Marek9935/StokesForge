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
//! # Sous-menus (#114)
//! Les listes se regroupent par thème, avec les mêmes composants que les
//! clics droits. Les bascules d'affichage doivent rester ouvertes, les actions
//! et les choix exclusifs fermer la chaîne : `CloseOnClickOutside` hérité par
//! les sous-menus évite la fermeture automatique d'egui sur une case ; chaque
//! action appelle `ui.close()`, propagé par egui aux parents.
//!
//! # Grammaire (#152)
//! Chaque menu range ses entrées par sections (`MenuSections::section`,
//! `menu_sections.rs`) selon `docs/menus.md` : ici, la barre **enveloppe**
//! les contenus partagés avec les clics droits (Selection, Sheet, Columns…),
//! qui, eux, les affichent à plat.
//!
//! Chaque menu possède sa fonction (#124), appelée dans la même Ui et dans
//! le même ordre ; les demandes de l'Object Manager restent différées après
//! la fermeture du panneau. Aucun scope egui ne dépend du découpage Rust.

use eframe::egui;
use stokesforge_core::compose::overlays_full;
use stokesforge_core::NodeId;
use stokesforge_render_egui::DEFAULT_FIGURE_SIZE;
use stokesforge_render_svg::SvgTextMode;

use stokesforge_app_state::i18n;
use stokesforge_app_state::state::{active_graph, Action};
use stokesforge_app_state::tr;

use super::busy::{work_in_progress, Work};
use super::graph_text_edit::add_annotation_at;
use super::graph_window::{graph_properties_menu, published_active_panel};
use super::menu_sections::{menu_choice, MenuSections};
use super::object_manager_ui::{
    object_manager_settings_menu, object_node_menu, selected_manager_target, ObjectManagerRequest,
};
use super::shortcuts::{
    SHORTCUT_EXPORT_GRAPH, SHORTCUT_IMPORT_TEXT, SHORTCUT_NEW_PROJECT, SHORTCUT_OPEN,
    SHORTCUT_REDO_Y, SHORTCUT_SAVE, SHORTCUT_SAVE_AS, SHORTCUT_SELECT_ALL, SHORTCUT_UNDO,
};
use super::tab::tab_node;
use super::tab::Tab;
use super::tab::{active_tab_kind, tab_title};
use super::workbook::workbook_menu;
use super::StokesForgeApp;

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
            egui::MenuBar::new()
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                )
                .ui(ui, |ui| {
                    self.file_menu(ui, ctx, actions);
                    self.edit_menu(ui, ctx, actions);
                    self.workbook_menu(ui, ctx, actions);
                    if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                        self.graph_menu(ui, ctx, graph, actions, &mut object_requests);
                    }
                    self.analysis_menu(ui, actions, active_graph(active_tab_kind(active_tab)));
                    self.view_menu(ui, actions);
                    self.window_menu(ui);
                    help_menu(ui, actions);
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
            let mut sections = MenuSections::new();
            sections.section(ui, true, |ui| {
                ui.menu_button(tr!("menu-new"), |ui| {
                    if ui
                        .add_enabled(
                            self.state.can_new_project(),
                            egui::Button::new(tr!("action-new-project"))
                                .shortcut_text(ctx.format_shortcut(&SHORTCUT_NEW_PROJECT)),
                        )
                        .on_hover_text(tr!("action-new-project", "hint"))
                        .on_disabled_hover_text(tr!("disabled-new-project-busy"))
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
                });
                ui.menu_button(tr!("menu-import"), |ui| {
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
                        .on_disabled_hover_text(tr!("disabled-no-source-file"))
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
                        .on_disabled_hover_text(tr!("disabled-no-last-import"))
                        .clicked()
                    {
                        actions.push(Action::ReopenLastImport);
                        ui.close();
                    }
                });
            });
            sections.section(ui, true, |ui| {
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
                    .add_enabled(
                        !self.state.script_running(),
                        egui::Button::new(tr!("action-save"))
                            .shortcut_text(ctx.format_shortcut(&SHORTCUT_SAVE)),
                    )
                    .on_disabled_hover_text(tr!("disabled-script-running"))
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
            });
            sections.section(ui, true, |ui| {
                ui.menu_button(tr!("menu-language"), |ui| {
                    for language in [i18n::Language::English, i18n::Language::French] {
                        let current = i18n::language() == language;
                        if menu_choice(ui, current, language.native_name()).clicked() {
                            self.choose_language(language);
                            ui.close();
                        }
                    }
                });
            });
            sections.section(ui, true, |ui| {
                if ui
                    .button(tr!("action-preferences"))
                    .on_hover_text(tr!("action-preferences", "hint"))
                    .clicked()
                {
                    actions.push(Action::OpenPreferences);
                    ui.close();
                }
            });
            sections.section(ui, true, |ui| {
                if ui.button(tr!("action-quit")).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.close();
                }
            });
        });
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        ui.menu_button(tr!("menu-edit"), |ui| {
            // Le libellé complet (« Undo Edit Cells in Column A ») et le
            // grisage sont deux lignes distinctes ici ; Qt les tenait
            // ensemble dans une seule `QAction`.
            let mut sections = MenuSections::new();
            sections.section(ui, true, |ui| {
                let undo = self
                    .state
                    .undo_label()
                    .unwrap_or_else(|| tr!("history-undo"));
                if ui
                    .add_enabled(
                        self.state.can_undo(),
                        egui::Button::new(undo).shortcut_text(ctx.format_shortcut(&SHORTCUT_UNDO)),
                    )
                    .on_disabled_hover_text(tr!("disabled-nothing-to-undo"))
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
                    .on_disabled_hover_text(tr!("disabled-nothing-to-redo"))
                    .clicked()
                {
                    actions.push(Action::Redo);
                    ui.close();
                }
            });
            let active_sheet = self
                .active_book()
                .and_then(|book| self.state.selected_sheet_in(book));
            sections.section(ui, true, |ui| {
                if ui
                    .add_enabled(
                        active_sheet.is_some(),
                        egui::Button::new(tr!("action-select-all"))
                            .shortcut_text(ctx.format_shortcut(&SHORTCUT_SELECT_ALL)),
                    )
                    .on_hover_text(tr!("action-select-all", "hint"))
                    .on_disabled_hover_text(tr!("disabled-needs-sheet"))
                    .clicked()
                {
                    if let Some(sheet) = active_sheet {
                        actions.push(Action::SelectAll { sheet });
                    }
                    ui.close();
                }
            });
            let ids: Vec<_> = self
                .state
                .tree()
                .walk_all()
                .into_iter()
                .filter(|id| self.pm_selection.iter().any(|tab| tab_node(*tab) == *id))
                .collect();
            let rename_viewport = if self.pm_detached {
                super::project_manager::viewport_id()
            } else {
                egui::ViewportId::ROOT
            };
            let root = self.state.tree().root();
            sections.section(ui, true, |ui| {
                ui.menu_button(tr!("menu-selected-content"), |ui| {
                    super::project_manager::object_menu(
                        ui,
                        &mut MenuSections::new(),
                        super::project_manager::ObjectMenuTarget::Windows,
                        &ids,
                        root,
                        actions,
                        rename_viewport,
                    );
                });
            });
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

    /// Menu Analysis (#181, #210) : la sélection du graphe actif prend
    /// priorité ; sinon les colonnes Y du classeur. Sans cible : grisé.
    fn analysis_menu(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        graph: Option<NodeId>,
    ) {
        let book = self.active_book();
        let sheet = book.and_then(|book| self.state.selected_sheet_in(book));
        let columns = sheet
            .and_then(|sheet| {
                self.state
                    .selection(sheet)
                    .map(|selection| selection.columns(self.state.tree(), sheet))
            })
            .unwrap_or_default();
        let row_pairs: Vec<(NodeId, usize)> = sheet
            .and_then(|sheet| self.state.row_drag_payload(sheet))
            .map(|payload| {
                payload
                    .rows
                    .iter()
                    .map(|row| (payload.sheet, *row))
                    .collect()
            })
            .unwrap_or_default();
        let graph_target = graph.and_then(|graph| {
            let spec = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)?;
            self.read_graph_view(graph, |view| {
                let sources = selected_manager_target(view)
                    .map(|target| super::object_manager_ui::analysis_sources(&spec, target))
                    .unwrap_or_default();
                (graph, sources, view.framed_spec(&spec))
            })
        });
        ui.menu_button(tr!("menu-analysis"), |ui| {
            let target = match graph_target.as_ref() {
                Some((graph, series, spec)) => Some(super::analysis_menu::AnalysisTarget::Graph {
                    graph: *graph,
                    series,
                    spec,
                }),
                None if graph.is_some() => None,
                // Des lignes entières d'une feuille à x' (#253) : on analyse
                // des lignes, pas les colonnes qu'elles traversent.
                None if !row_pairs.is_empty() => {
                    Some(super::analysis_menu::AnalysisTarget::Rows { rows: &row_pairs })
                }
                None => sheet.map(|sheet| super::analysis_menu::AnalysisTarget::Columns {
                    sheet,
                    columns: &columns,
                }),
            };
            super::analysis_menu::analysis_menu_contents(ui, &self.state, actions, target);
        });
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
            let spec = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph);
            let shown = stokesforge_app_state::object_manager::Shown::from(
                self.state.object_manager_settings(),
            );
            let target = self
                .read_graph_view(graph, selected_manager_target)
                .flatten();
            let has_data =
                !stokesforge_app_state::state::graph_data_books(self.state.tree(), graph)
                    .is_empty();
            let moved = spec.as_ref().is_some_and(|spec| spec.has_text_anchors());
            let mut sections = MenuSections::new();
            sections.section(ui, true, |ui| {
                if ui
                    .add_enabled(has_data, egui::Button::new(tr!("action-show-graph-data")))
                    .on_disabled_hover_text(tr!("disabled-no-graph-data"))
                    .clicked()
                {
                    actions.push(Action::ShowGraphData { graph });
                    ui.close();
                }
            });
            // Textes libres (issue #35) : le pendant, dans la barre de menus,
            // du clic droit sur le fond.
            sections.section(ui, true, |ui| {
                if ui
                    .button(tr!("action-add-text"))
                    .on_hover_text(tr!("action-add-text", "hint"))
                    .clicked()
                {
                    let tree = self.state.tree().clone();
                    if let (Some(view), Some(spec)) =
                        (self.graph_views.get_mut(&graph), spec.as_ref())
                    {
                        if let Some(point) = view.panel_center_data(spec) {
                            add_annotation_at(ctx, view, &tree, graph, point);
                        }
                    }
                    ui.close();
                }
                // Calque superposé (#167), à la manière du « New Layer (Axes) »
                // d'Origin, sur le calque actif.
                ui.menu_button(tr!("menu-new-layer"), |ui| {
                    let panel = published_active_panel(ctx, graph);
                    for (label, hint, x_top, y_right) in [
                        (
                            tr!("action-new-layer-top-x"),
                            tr!("action-new-layer-top-x", "hint"),
                            true,
                            false,
                        ),
                        (
                            tr!("action-new-layer-right-y"),
                            tr!("action-new-layer-right-y", "hint"),
                            false,
                            true,
                        ),
                        (
                            tr!("action-new-layer-top-x-right-y"),
                            tr!("action-new-layer-top-x-right-y", "hint"),
                            true,
                            true,
                        ),
                    ] {
                        // Trois calques superposés au plus par case (#168).
                        let full = spec.as_ref().is_some_and(|spec| overlays_full(spec, panel));
                        if ui
                            .add_enabled(!full, egui::Button::new(label))
                            .on_hover_text(hint)
                            .on_disabled_hover_text(tr!("menu-new-layer", "disabled"))
                            .clicked()
                        {
                            actions.push(Action::AddLayer {
                                graph,
                                panel,
                                x_top,
                                y_right,
                            });
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(tr!("menu-new-layer", "hint"));
            });
            // Le menu de l'objet sélectionné est celui du clic droit et de
            // l'Object Manager ; sans sélection, l'entrée est grisée.
            sections.section(ui, true, |ui| {
                if let (Some(target), Some(spec)) = (target, spec.as_ref()) {
                    ui.menu_button(tr!("menu-selected-content"), |ui| {
                        object_node_menu(ui, graph, target, spec, shown, actions, object_requests);
                    });
                } else {
                    ui.add_enabled(false, egui::Button::new(tr!("menu-selected-content")))
                        .on_disabled_hover_text(tr!("menu-selected-content", "disabled"));
                }
                if ui
                    .add_enabled(moved, egui::Button::new(tr!("action-reset-text-positions")))
                    .on_hover_text(tr!("action-reset-text-positions", "hint"))
                    .on_disabled_hover_text(tr!("action-reset-text-positions", "disabled"))
                    .clicked()
                {
                    actions.push(Action::ResetTextAnchors { graph });
                    ui.close();
                }
            });
            // Axes miroirs (#176) du calque actif : de vraies bascules
            // d'affichage, elles gardent le menu ouvert.
            sections.section(ui, true, |ui| {
                let panel = published_active_panel(ctx, graph);
                for (side, label, hint) in [
                    (
                        stokesforge_plot::AxisSide::Top,
                        tr!("action-mirror-axis-top"),
                        tr!("action-mirror-axis-top", "hint"),
                    ),
                    (
                        stokesforge_plot::AxisSide::Right,
                        tr!("action-mirror-axis-right"),
                        tr!("action-mirror-axis-right", "hint"),
                    ),
                ] {
                    let target = stokesforge_app_state::object_manager::VisibilityTarget::Axis(
                        stokesforge_app_state::object_manager::AxisRef { panel, side },
                    );
                    let ordinary = spec
                        .as_ref()
                        .and_then(|spec| spec.panels.get(panel))
                        .is_some_and(|layer| layer.overlay.is_none());
                    let state = spec
                        .as_ref()
                        .map(|spec| target.visibility(spec))
                        .unwrap_or_default();
                    let mut checked = state.checked;
                    if ui
                        .add_enabled(
                            ordinary && state.enabled,
                            egui::Checkbox::new(&mut checked, label),
                        )
                        .on_hover_text(hint)
                        .on_disabled_hover_text(if ordinary {
                            tr!("disabled-axis-blocked")
                        } else {
                            tr!("disabled-axis-on-overlay")
                        })
                        .changed()
                    {
                        actions.push(Action::SetPlotVisibility {
                            graph,
                            target,
                            visible: checked,
                        });
                    }
                }
            });
            sections.section(ui, true, |ui| {
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
            });
            sections.section(ui, true, |ui| {
                ui.menu_button(tr!("menu-export"), |ui| {
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
                    if ui
                        .button(tr!("action-copy-graph"))
                        .on_hover_text(tr!("action-copy-graph", "hint"))
                        .clicked()
                    {
                        self.request_graph_figure_copy(graph, actions);
                        ui.close();
                    }
                    ui.menu_button(tr!("menu-graph-svg-text"), |ui| {
                        let current = self.state.svg_text_mode();
                        if menu_choice(
                            ui,
                            current == SvgTextMode::Text,
                            tr!("menu-graph-svg-text-text"),
                        )
                        .on_hover_text(tr!("menu-graph-svg-text-text", "hint"))
                        .clicked()
                        {
                            actions.push(Action::SetSvgTextMode(SvgTextMode::Text));
                            ui.close();
                        }
                        if menu_choice(
                            ui,
                            current == SvgTextMode::Outlines,
                            tr!("menu-graph-svg-text-outlines"),
                        )
                        .on_hover_text(tr!("menu-graph-svg-text-outlines", "hint"))
                        .clicked()
                        {
                            actions.push(Action::SetSvgTextMode(SvgTextMode::Outlines));
                            ui.close();
                        }
                    });
                    ui.menu_button(tr!("menu-graph-png-resolution"), |ui| {
                        let current = self.state.png_export_scale();
                        for scale in stokesforge_app_state::state::PNG_EXPORT_SCALES {
                            let (w, h) = DEFAULT_FIGURE_SIZE;
                            let (width, height) =
                                ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
                            let label = tr!(
                                "menu-graph-png-resolution-option",
                                scale = scale,
                                width = width,
                                height = height
                            );
                            if menu_choice(ui, current == scale, label).clicked() {
                                actions.push(Action::SetPngExportScale(scale));
                                ui.close();
                            }
                        }
                    });
                });
            });
            sections.section(ui, true, |ui| {
                ui.menu_button(tr!("menu-properties"), |ui| {
                    graph_properties_menu(ui, graph, actions);
                });
            });
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.menu_button(tr!("menu-view"), |ui| {
            // Ni panneaux ni barres d'outils à masquer : le menu ne porte que
            // les réglages de chaque panneau (Object Manager, plus tard
            // Project Manager et console de script).
            ui.menu_button(tr!("action-object-manager"), |ui| {
                object_manager_settings_menu(ui, self.state.object_manager_settings(), actions);
            });
        });
    }

    fn window_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(tr!("menu-window"), |ui| {
            let mut sections = MenuSections::new();
            sections.section(ui, true, |ui| {
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
                        .on_disabled_hover_text(tr!("disabled-no-window"))
                        .clicked()
                    {
                        self.maximized_tab = active;
                        ui.close();
                    }
                }
            });
            sections.section(ui, true, |ui| {
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
            });
            // Pas d'« Attacher à gauche/à droite » ici : le Project
            // Manager est un panneau fixe, figé à gauche, depuis
            // cette tâche (voir la doc de tête d'`app.rs`, § attacher
            // à gauche ou à droite — le côté droit et le
            // repositionnement par glisser sont un travail futur
            // délibérément hors périmètre).
        });
    }

    /// Barre d'état : le dernier message, à trois niveaux (S-M4) ; un clic ouvre
    /// l'historique des 50 derniers (S-M6). Ni révision ni « Reopen Last
    /// Import… » (S-R15 : il reste dans File).
    ///
    /// Les dimensions (lignes × colonnes) qu'elle affichait ont disparu avec
    /// « la » feuille sélectionnée globale : chaque onglet de classeur les
    /// montre pour sa propre feuille, dans [`book_contents`](crate::app::workbook::book_contents).
    pub(super) fn status_bar(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::bottom("stokesforge_status")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Un travail qui dure remplace le message (#232, S-P1/S-P2) ;
                    // sous 0,5 s la brique ne peint rien et le message
                    // « Importing… » reste la ligne d'état.
                    let line = ui.scope(|ui| {
                        if let Some(elapsed) = self.state.recalc_work() {
                            let work = Work {
                                label: tr!("status-recalculating"),
                                elapsed,
                                progress: None,
                                stop: Some(Action::StopRecalc),
                            };
                            if work_in_progress(ui, &work, actions) {
                                return;
                            }
                        }
                        if let Some((elapsed, done, total)) = self.state.import_work() {
                            let work = Work {
                                label: tr!("status-import-progress", done = done, total = total),
                                elapsed,
                                progress: Some((done, total)),
                                stop: Some(Action::StopImport),
                            };
                            if work_in_progress(ui, &work, actions) {
                                return;
                            }
                        }
                        match self.state.status() {
                            Some(status) => status_line(ui, status),
                            None => {
                                ui.weak(tr!("status-ready"));
                            }
                        }
                    });
                    // La zone cliquable couvre toute la ligne, par-dessus ses
                    // éléments (un `Label` ne porte pas le clic).
                    let response = super::cursors::clickable(
                        ui.interact(
                            line.response.rect,
                            egui::Id::new("stokesforge_status_history_click"),
                            egui::Sense::click(),
                        )
                        .on_hover_text(tr!("status-history")),
                    );
                    if response.clicked() {
                        actions.push(Action::OpenStatusHistory);
                    }
                    // La pastille des R² faibles du projet (#290) : un état,
                    // visible tant qu'il en reste, même si la boîte s'est
                    // ouverte ou n'a pas pu s'ouvrir.
                    let total = self.state.low_r2_total();
                    if total > 0 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            low_r2_badge(ui, total, actions);
                        });
                    }
                });
            });
    }
}

/// Texte et teinte de la pastille des R² faibles (#290), sans contexte egui :
/// `None` quand il n'y en a aucun (pas de pastille).
fn low_r2_badge_content(total: usize, dark_mode: bool) -> Option<(String, egui::Color32)> {
    use stokesforge_app_state::r_squared::{badge_text, RSquaredLevel};
    (total > 0).then(|| {
        (
            badge_text(total),
            crate::theme::r_squared_color(RSquaredLevel::Dialog, dark_mode),
        )
    })
}

/// La pastille « ⚠ 13 R² faibles » (#290) : icône d'avertissement et texte dans
/// la teinte rouge de R² (`theme::r_squared_color`) ; un clic ouvre la liste par
/// type, l'infobulle le dit.
fn low_r2_badge(ui: &mut egui::Ui, total: usize, actions: &mut Vec<Action>) {
    use crate::icons::{icon_image, Icon, IconRole};
    let Some((text, color)) = low_r2_badge_content(total, ui.visuals().dark_mode) else {
        return;
    };
    let response = ui
        .scope(|ui| {
            ui.add(icon_image(Icon::Stale, IconRole::Neutral));
            ui.label(crate::theme::bold(text).color(color));
        })
        .response;
    let response = super::cursors::clickable(
        ui.interact(
            response.rect,
            egui::Id::new("stokesforge_low_r2_badge"),
            egui::Sense::click(),
        )
        .on_hover_text(tr!("status-low-r2-badge-hint")),
    );
    if response.clicked() {
        actions.push(Action::OpenLowR2List);
    }
}

/// Une ligne de message selon son niveau : texte courant, texte en teinte
/// d'avertissement avec son triangle, ou texte rouge avec sa croix. Partagée
/// par la barre d'état et la fenêtre d'historique.
pub(super) fn status_line(ui: &mut egui::Ui, status: &stokesforge_app_state::state::StatusMessage) {
    use crate::icons::{icon_image, Icon, IconRole};
    use stokesforge_app_state::state::StatusLevel;
    match status.level {
        StatusLevel::Info => {
            ui.label(&status.text);
        }
        StatusLevel::Warning => {
            ui.add(icon_image(Icon::Stale, IconRole::Neutral))
                .on_hover_text(tr!("status-level-warning"));
            ui.colored_label(ui.visuals().warn_fg_color, &status.text);
        }
        StatusLevel::Error => {
            ui.add(icon_image(Icon::Close, IconRole::Danger))
                .on_hover_text(tr!("status-level-error"));
            ui.colored_label(ui.visuals().error_fg_color, &status.text);
        }
    }
}

/// Menu Help (#231, S-K5) : l'aide qui se génère du code, aujourd'hui les
/// raccourcis clavier. Une seule entrée, directe.
fn help_menu(ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    ui.menu_button(tr!("menu-help"), |ui| {
        let mut sections = MenuSections::new();
        sections.section(ui, true, |ui| {
            if ui
                .button(tr!("action-keyboard-shortcuts"))
                .on_hover_text(tr!("action-keyboard-shortcuts", "hint"))
                .clicked()
            {
                actions.push(Action::OpenKeyboardShortcuts);
                ui.close();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::super::project_manager::tests::find_text_center;
    use super::super::shortcuts::shortcut_text_delete;
    use super::super::test_support::{self, colonnes_xy};
    use super::*;
    use stokesforge_app_state::state::AppState;
    use stokesforge_core::{NodeId, Project};
    use stokesforge_plot::{AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesSpec};
    use stokesforge_session::Session;

    /// La langue est globale : les tests de la pastille la fixent à l'anglais
    /// sous ce verrou.
    static LANGUE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn la_pastille_des_r2_faibles_n_existe_que_s_il_y_en_a() {
        assert_eq!(low_r2_badge_content(0, false), None);
        assert_eq!(low_r2_badge_content(0, true), None);
    }

    #[test]
    fn la_pastille_des_r2_faibles_dit_le_compte() {
        use stokesforge_app_state::i18n::{set_language, Language};
        let _verrou = LANGUE.lock().unwrap_or_else(|e| e.into_inner());
        set_language(Language::English);
        for (total, attendu) in [(1, "1 low R²"), (2, "2 low R²"), (13, "13 low R²")] {
            let (texte, _) = low_r2_badge_content(total, false).unwrap();
            assert_eq!(texte, attendu);
        }
    }

    #[test]
    fn la_pastille_des_r2_faibles_prend_la_teinte_de_la_boite_et_contraste() {
        use stokesforge_app_state::r_squared::RSquaredLevel::Dialog;
        use stokesforge_render::color::contrast_ratio;
        let rgb = |c: egui::Color32| (c.r(), c.g(), c.b());
        let mut teintes = Vec::new();
        for (dark, fond) in [(false, (255, 255, 255)), (true, (27, 27, 27))] {
            let (_, couleur) = low_r2_badge_content(3, dark).unwrap();
            assert_eq!(couleur, crate::theme::r_squared_color(Dialog, dark));
            let ratio = contrast_ratio(rgb(couleur), fond);
            assert!(ratio >= 3.0, "dark = {dark} : contraste {ratio:.2} < 3");
            teintes.push(couleur);
        }
        assert_ne!(teintes[0], teintes[1]);
    }

    /// Un classeur (une feuille, deux colonnes) et un graphe qui les trace,
    /// avec un texte libre. `StokesForgeApp::new` ouvre le classeur puis le
    /// graphe : le graphe est au premier plan.
    fn banc() -> (Session, StokesForgeApp, NodeId) {
        let mut project = Project::new("Menus").unwrap();
        let root = project.root();
        let (x, y) = colonnes_xy(&mut project);
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        panel
            .annotations
            .push(AnnotationSpec::new("Texte", 0.5, 0.5));
        let graph = project
            .add_graph(root, None, PlotSpec::empty("Figure").with_panels(panel))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        (session, StokesForgeApp::new(state), graph)
    }

    /// Un fichier importable, et l'état après son import dans un classeur
    /// neuf : « Reopen Last Import » a de quoi rouvrir, et ce classeur a une
    /// source à relire.
    fn importe(state: &mut AppState, dir: &tempfile::TempDir) {
        let file = dir.path().join("mesure.csv");
        std::fs::write(&file, "1,10\n2,20\n").unwrap();
        state.dispatch(Action::ImportFiles {
            destination: stokesforge_app_state::state::ImportDestination::NewBookForAll,
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
        textes.push(ctx.format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::C,
        )));
        textes.push(ctx.format_shortcut(&super::super::SHORTCUT_DUPLICATE_SHEET));
        textes.push("F2".into());
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
        entrees
            .iter()
            .filter(|entree| entree.label == label)
            .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
            .unwrap_or_else(|| panic!("entrée {label:?} absente de {entrees:#?}"))
    }

    /// Une image de la seule barre de menu, sur l'onglet actif `tab`.
    fn peint(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<Action>) {
        peint_avec(app, ctx, tab, events, egui::Modifiers::NONE)
    }

    /// Comme [`peint`], touches de modification tenues (Ctrl+clic).
    fn peint_avec(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> (egui::FullOutput, Vec<Action>) {
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                // egui 0.36 : les touches tenues passent par un événement.
                events: std::iter::once(egui::Event::ModifiersChanged(modifiers))
                    .chain(events)
                    .collect(),
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

    /// Le contexte commun des tests, avec en plus les chargeurs d'images, des
    /// animations instantanées et AccessKit (les entrées de menu se lisent
    /// par l'arbre d'accessibilité).
    fn contexte() -> egui::Context {
        let ctx = test_support::contexte();
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
        clique_avec(app, ctx, tab, pos, egui::Modifiers::NONE)
    }

    /// Un clic gauche, `modifiers` tenues pendant tout le geste.
    fn clique_avec(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        pos: egui::Pos2,
        modifiers: egui::Modifiers,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        ] {
            actions.extend(peint_avec(app, ctx, tab, events, modifiers).1);
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

    /// Chaque niveau s'ouvre par un vrai clic, avec le contexte des parents.
    fn ouvre_chemin(
        app: &mut StokesForgeApp,
        tab: Option<Tab>,
        chemin: &[String],
    ) -> (egui::Context, Vec<Entree>) {
        let (ctx, mut menu) = ouvre(app, tab, &chemin[0]);
        for parent in &chemin[1..] {
            let pos = menu
                .iter()
                .filter(|entree| &entree.label == parent)
                .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
                .unwrap_or_else(|| panic!("sous-menu {parent} absent : {menu:#?}"))
                .center;
            assert!(clique(app, &ctx, tab, pos).is_empty());
            menu = entrees(&peint(app, &ctx, tab, Vec::new()).0);
        }
        (ctx, menu)
    }

    fn assert_ferme(app: &mut StokesForgeApp, ctx: &egui::Context, tab: Option<Tab>) {
        let menu = entrees(&peint(app, ctx, tab, Vec::new()).0);
        let mut visibles: Vec<_> = menu.iter().map(|e| e.label.clone()).collect();
        let mut fermes = titres(app, tab);
        visibles.sort();
        fermes.sort();
        assert_eq!(visibles, fermes, "le choix ferme tous les niveaux");
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
                tr!("menu-analysis"),
                tr!("menu-view"),
                tr!("menu-window"),
                tr!("menu-help")
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
                tr!("menu-analysis"),
                tr!("menu-view"),
                tr!("menu-window"),
                tr!("menu-help")
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
        assert!(entree(&menu, &tr!("action-add-workbook")).is_none());
        assert!(entree(&menu, &tr!("action-refresh-from-source")).is_none());
        let (_, nouveau) = ouvre_chemin(&mut app, tab, &[tr!("menu-file"), tr!("menu-new")]);
        assert!(active(&nouveau, &tr!("action-add-workbook")).enabled);
        assert_eq!(
            active(&nouveau, &tr!("action-new-project")).enabled,
            app.state.can_new_project()
        );
        let (_, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-file"), tr!("menu-import")]);
        assert!(!active(&menu, &tr!("action-reopen-last-import")).enabled);
        assert!(!active(&menu, &tr!("action-refresh-from-source")).enabled);
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
        let (_, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-file"), tr!("menu-import")]);
        assert!(active(&menu, &tr!("action-reopen-last-import")).enabled);
        assert!(
            !active(&menu, &tr!("action-refresh-from-source")).enabled,
            "un graphe au premier plan n'a pas de feuille à relire"
        );
        app.open_tab(Tab::Book(imported));
        let tab = app.floating_tabs.last().copied();
        let (_, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-file"), tr!("menu-import")]);
        assert!(active(&menu, &tr!("action-refresh-from-source")).enabled);

        // Une entrée cliquée pousse son action, rien de plus.
        let (ctx, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-file"), tr!("menu-new")]);
        let pos = active(&menu, &tr!("action-add-workbook")).center;
        assert_eq!(clique(&mut app, &ctx, tab, pos), vec![Action::AddWorkbook]);
        assert_ferme(&mut app, &ctx, tab);
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
    fn selection_d_edition_garde_les_actions_et_le_grisage_du_project_manager() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-edit"));
        assert!(entree(&menu, &tr!("action-delete-project-object")).is_none());
        app.pm_selection.clear();
        let (_, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[tr!("menu-edit"), tr!("menu-selected-content")],
        );
        assert!(!active(&menu, &tr!("action-delete-project-object")).enabled);
        assert!(!active(&menu, &tr!("action-copy")).enabled);
        assert!(
            active(&menu, &tr!("action-paste")).enabled,
            "témoin indépendant de la sélection"
        );
        app.pm_selection.insert(Tab::Graph(graph));
        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[tr!("menu-edit"), tr!("menu-selected-content")],
        );
        let copy = active(&menu, &tr!("action-copy"));
        assert!(copy.enabled);
        assert_eq!(
            clique(&mut app, &ctx, tab, copy.center),
            vec![Action::CopyObjects(vec![graph])]
        );
        assert_ferme(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    /// Menu Analysis (#181) : grisé tant qu'aucune colonne Y n'est visée, puis
    /// chaque entrée ouvre la fenêtre sur sa famille et sa méthode ;
    /// « Recalculate Out of Date » reste grisé quand tout est à jour.
    #[test]
    fn le_menu_analyse_ouvre_la_serie_du_graphe_actif() {
        let (_session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_series(Some(stokesforge_plot::SeriesRef::new(0, 0)), &spec)
        });
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-analysis"));
        let entry = active(&menu, &tr!("action-analysis-integrate"));
        assert!(entry.enabled);
        assert_eq!(
            clique(&mut app, &ctx, tab, entry.center),
            vec![Action::OpenGraphAnalysis {
                preset: stokesforge_app_state::analysis::Preset::Integrate,
                graph,
                series: vec![stokesforge_plot::SeriesRef::new(0, 0)],
                spec: Box::new(spec),
            }]
        );
    }

    #[test]
    fn le_menu_analyse_se_grise_sans_colonne_y_et_ouvre_la_fenetre() {
        use stokesforge_app_state::analysis::Preset;
        let (session, mut app, _) = banc();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Book(book));
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let tab = Some(Tab::Book(book));
        app.state.deselect_all(sheet);

        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-analysis"));
        for label in [
            tr!("analysis-category-data-manipulation"),
            tr!("analysis-category-fitting"),
            tr!("analysis-category-peaks"),
            tr!("action-analysis-integrate"),
        ] {
            assert!(!active(&menu, &label).enabled, "sans colonne : {label}");
        }
        assert!(
            !active(&menu, &tr!("action-analysis-recalculate-stale")).enabled,
            "tout est à jour"
        );

        let columns: Vec<_> = app
            .state
            .visible_columns(sheet)
            .iter()
            .map(|c| c.0)
            .collect();
        app.state.select_column(sheet, columns[0]);
        app.state.extend_column_selection(sheet, columns[1]);
        let selected = app
            .state
            .selection(sheet)
            .map(|selection| selection.columns(app.state.tree(), sheet))
            .unwrap();
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-analysis"));
        let integrate = active(&menu, &tr!("action-analysis-integrate"));
        assert!(integrate.enabled, "une colonne Y est sélectionnée");
        assert_eq!(
            clique(&mut app, &ctx, tab, integrate.center),
            vec![Action::OpenAnalysis {
                preset: Preset::Integrate,
                sheet,
                columns: selected.clone(),
            }]
        );
        assert_ferme(&mut app, &ctx, tab);

        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[
                tr!("menu-analysis"),
                tr!("analysis-category-data-manipulation"),
            ],
        );
        let anchors = active(&menu, &tr!("analysis-baseline-anchors"));
        assert_eq!(
            clique(&mut app, &ctx, tab, anchors.center),
            vec![Action::OpenAnalysis {
                preset: Preset::BaselineAnchors,
                sheet,
                columns: selected,
            }]
        );
        assert_ferme(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    /// #248 : le menu Analysis a une entrée par catégorie, et les méthodes de
    /// chaque catégorie sont dans son sous-menu — « Correct by Reference » est
    /// du ressort de Data Manipulation, pas de Fitting.
    #[test]
    fn le_menu_analyse_range_les_methodes_par_categorie() {
        let (session, mut app, _) = banc();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Book(book));
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let tab = Some(Tab::Book(book));
        let columns: Vec<_> = app
            .state
            .visible_columns(sheet)
            .iter()
            .map(|c| c.0)
            .collect();
        app.state.select_column(sheet, columns[0]);
        app.state.extend_column_selection(sheet, columns[1]);

        let chemin = |app: &mut StokesForgeApp, categorie: &str| {
            let (_, menu) = ouvre_chemin(app, tab, &[tr!("menu-analysis"), categorie.to_string()]);
            menu.into_iter()
                .map(|entree| entree.label)
                .collect::<Vec<_>>()
        };
        let data = chemin(&mut app, &tr!("analysis-category-data-manipulation"));
        for attendu in [
            tr!("analysis-baseline-constant"),
            tr!("analysis-baseline-linear"),
            tr!("analysis-baseline-polynomial"),
            tr!("analysis-baseline-anchors"),
            tr!("action-analysis-correct"),
        ] {
            assert!(
                data.contains(&attendu),
                "Data Manipulation : {attendu} dans {data:?}"
            );
        }
        let fitting = chemin(&mut app, &tr!("analysis-category-fitting"));
        assert!(
            fitting.contains(&tr!("analysis-fit-gaussian")),
            "{fitting:?}"
        );
        assert!(fitting.contains(&tr!("analysis-fit-custom")), "{fitting:?}");
        assert!(
            !fitting.contains(&tr!("action-analysis-correct")),
            "témoin : la correction n'est pas un ajustement"
        );
        let peaks = chemin(&mut app, &tr!("analysis-category-peaks"));
        for attendu in [
            tr!("action-analysis-peaks"),
            tr!("action-analysis-reconvolution"),
            tr!("action-analysis-direct"),
        ] {
            assert!(peaks.contains(&attendu), "{attendu} dans {peaks:?}");
        }
        session.shutdown().unwrap();
    }

    /// #249 : les dernières analyses lancées sont listées en bas du menu ; un
    /// clic ouvre la fenêtre, Ctrl+clic lance (même geste, `run` en plus).
    #[test]
    fn le_menu_analyse_liste_les_dernieres_analyses_et_ctrl_clic_les_lance() {
        use stokesforge_app_state::analysis::{Preset, RecentTarget};
        let (session, mut app, _) = banc();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Book(book));
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let tab = Some(Tab::Book(book));
        let columns: Vec<_> = app
            .state
            .visible_columns(sheet)
            .iter()
            .map(|c| c.0)
            .collect();
        app.state.select_column(sheet, columns[0]);
        app.state.extend_column_selection(sheet, columns[1]);
        let selected = app
            .state
            .selection(sheet)
            .map(|selection| selection.columns(app.state.tree(), sheet))
            .unwrap();

        // Témoin : rien n'a été lancé, rien n'est listé.
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-analysis"));
        let numerote = |label: &str| label.starts_with("1 ");
        assert!(
            menu.iter().all(|entree| !numerote(&entree.label)),
            "{menu:?}"
        );

        app.state.dispatch(Action::OpenAnalysis {
            preset: Preset::Integrate,
            sheet,
            columns: selected.clone(),
        });
        app.state.dispatch(Action::ApplyAnalysis);
        assert_eq!(app.state.analysis_recent().len(), 1);

        let label = tr!(
            "analysis-recent-entry",
            rank = 1_i64,
            label = app.state.analysis_recent()[0].label()
        );
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-analysis"));
        let entry = active(&menu, &label);
        assert!(entry.enabled);
        let target = RecentTarget::Columns {
            sheet,
            columns: selected,
        };
        assert_eq!(
            clique(&mut app, &ctx, tab, entry.center),
            vec![Action::OpenRecentAnalysis {
                index: 0,
                run: false,
                target: target.clone(),
            }]
        );
        assert_ferme(&mut app, &ctx, tab);

        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-analysis"));
        let entry = active(&menu, &label);
        assert_eq!(
            clique_avec(&mut app, &ctx, tab, entry.center, egui::Modifiers::COMMAND),
            vec![Action::OpenRecentAnalysis {
                index: 0,
                run: true,
                target,
            }]
        );
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
    fn le_classeur_range_feuilles_colonnes_et_metadata_sans_changer_les_actions() {
        use stokesforge_core::{ColumnRole, ColumnType};
        let (session, mut app, _) = banc();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Book(book));
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let columns: Vec<_> = app
            .state
            .visible_columns(sheet)
            .iter()
            .map(|c| c.0)
            .collect();
        let tab = Some(Tab::Book(book));
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-workbook"));
        assert!(entree(&menu, &tr!("action-rename-sheet")).is_none());
        let (ctx, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-workbook"), tr!("menu-sheet")]);
        assert!(!active(&menu, &tr!("action-delete-sheet")).enabled);
        let pos = active(&menu, &tr!("action-duplicate-sheet")).center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::DuplicateSheet(sheet)]
        );
        assert_ferme(&mut app, &ctx, tab);

        app.state.deselect_all(sheet);
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-workbook"));
        assert!(!active(&menu, &tr!("workbook-plot-menu")).enabled);
        let (ctx, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-workbook"), tr!("menu-columns")]);
        assert!(active(&menu, &tr!("workbook-column-add")).enabled);
        for label in [
            tr!("column-type"),
            tr!("column-role"),
            tr!("workbook-plot-menu"),
        ] {
            assert!(!active(&menu, &label).enabled, "aucune sélection : {label}");
        }
        let pos = active(&menu, &tr!("workbook-column-add")).center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::AddColumn { sheet }]
        );
        assert_ferme(&mut app, &ctx, tab);

        app.state.select_column(sheet, columns[0]);
        app.state.extend_column_selection(sheet, columns[1]);
        app.state.dispatch(Action::SetColumnType {
            column: columns[0],
            column_type: ColumnType::Mixed,
        });
        app.state.dispatch(Action::SetColumnRole {
            column: columns[0],
            role: ColumnRole::X,
        });
        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[
                tr!("menu-workbook"),
                tr!("menu-columns"),
                tr!("column-type"),
            ],
        );
        assert_eq!(
            active(&menu, &tr!("column-type-numeric")).toggled,
            Some(false)
        );
        assert_eq!(
            active(&menu, &tr!("column-type-mixed")).toggled,
            Some(false)
        );
        let pos = active(&menu, &tr!("column-type-mixed")).center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::SetColumnsType {
                columns: columns.clone(),
                column_type: ColumnType::Mixed
            }]
        );
        assert_ferme(&mut app, &ctx, tab);
        // X et Y sont différents : aucun rôle n'est indiqué comme sélectionné.
        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[
                tr!("menu-workbook"),
                tr!("menu-columns"),
                tr!("column-role"),
            ],
        );
        for label in [tr!("workbook-role-x"), tr!("workbook-role-y")] {
            assert_eq!(active(&menu, &label).toggled, Some(false));
        }
        let pos = active(&menu, &tr!("workbook-role-z")).center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::SetColumnsRole {
                columns: columns.clone(),
                role: ColumnRole::Z
            }]
        );
        assert_ferme(&mut app, &ctx, tab);
        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[
                tr!("menu-workbook"),
                tr!("menu-columns"),
                tr!("workbook-plot-menu"),
            ],
        );
        let pos = active(&menu, &tr!("plot-type-line")).center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::PlotColumns {
                sheet,
                columns: columns.clone(),
                plot_type: stokesforge_plot::PlotType::Line
            }]
        );
        assert_ferme(&mut app, &ctx, tab);

        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-workbook"));
        let label = tr!("menu-window-show-metadata-row");
        let case = active(&menu, &label);
        let visible = case.toggled.unwrap();
        assert!(clique(&mut app, &ctx, tab, case.center).is_empty());
        let menu = entrees(&peint(&mut app, &ctx, tab, Vec::new()).0);
        assert_eq!(active(&menu, &label).toggled, Some(!visible));
        session.shutdown().unwrap();
    }

    /// #319 : `Move ▸` est dans Workbook › Columns comme au clic droit des
    /// lettres (une seule peinture, `column_type_role_menus`), avec ses quatre
    /// entrées et le même grisage ; entre Add Column et Type.
    #[test]
    fn le_menu_colonnes_porte_move_avec_ses_quatre_entrees_et_le_meme_grisage() {
        let (session, mut app, _) = banc();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Book(book));
        let sheet = app.state.selected_sheet_in(book).unwrap();
        app.state.dispatch(Action::AddColumn { sheet });
        let columns: Vec<_> = app
            .state
            .visible_columns(sheet)
            .iter()
            .map(|c| c.0)
            .collect();
        assert!(columns.len() >= 3, "une colonne au milieu : {columns:?}");
        let tab = Some(Tab::Book(book));
        let chemin = [
            tr!("menu-workbook"),
            tr!("menu-columns"),
            tr!("column-move"),
        ];
        let quatre = [
            tr!("action-move-columns-first"),
            tr!("action-move-columns-left"),
            tr!("action-move-columns-right"),
            tr!("action-move-columns-last"),
        ];

        // Une colonne du milieu : tout est possible, dans l'ordre du menu.
        app.state.select_column(sheet, columns[1]);
        let (ctx, menu) = ouvre_chemin(&mut app, tab, &chemin);
        let y = |label: &str| active(&menu, label).center.y;
        assert!(y(&tr!("workbook-column-add")) < y(&tr!("column-move")));
        assert!(y(&tr!("column-move")) < y(&tr!("column-type")));
        assert!(y(&tr!("column-type")) < y(&tr!("column-role")));
        for label in &quatre {
            assert!(active(&menu, label).enabled, "{label} : {menu:#?}");
        }
        let ys: Vec<f32> = quatre.iter().map(|label| y(label)).collect();
        assert!(ys.windows(2).all(|paire| paire[0] < paire[1]), "{ys:?}");
        let pos = active(&menu, &tr!("action-move-columns-right")).center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::MoveColumns {
                sheet,
                columns: vec![columns[1]],
                to: stokesforge_app_state::column_move::ColumnMove::Right
            }]
        );
        assert_ferme(&mut app, &ctx, tab);

        // La première colonne : ni First ni Left (même règle qu'au clic droit).
        app.state.select_column(sheet, columns[0]);
        let (_, menu) = ouvre_chemin(&mut app, tab, &chemin);
        let actives: Vec<bool> = quatre
            .iter()
            .map(|label| active(&menu, label).enabled)
            .collect();
        assert_eq!(actives, [false, false, true, true], "{menu:#?}");
        session.shutdown().unwrap();
    }

    #[test]
    fn afficher_les_donnees_suit_les_references_du_graphique() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-graph"));
        let entry = active(&menu, &tr!("action-show-graph-data"));
        assert!(entry.enabled);
        assert_eq!(
            clique(&mut app, &ctx, tab, entry.center),
            vec![Action::ShowGraphData { graph }]
        );
        app.state.dispatch(Action::AddGraph);
        let graph = app.state.graphs().last().unwrap().0;
        app.open_tab(Tab::Graph(graph));
        let (_, menu) = ouvre(&mut app, Some(Tab::Graph(graph)), &tr!("menu-graph"));
        assert!(!active(&menu, &tr!("action-show-graph-data")).enabled);
        session.shutdown().unwrap();
    }

    /// #176 : les axes miroirs se cochent depuis le menu Graphique, comme la
    /// case de l'Object Manager (même `Action::SetPlotVisibility`), et la case
    /// garde le menu ouvert.
    #[test]
    fn les_axes_miroirs_se_cochent_depuis_le_menu_graphique() {
        use stokesforge_app_state::object_manager::{AxisRef, VisibilityTarget};
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        for (label, side) in [
            (
                tr!("action-mirror-axis-top"),
                stokesforge_plot::AxisSide::Top,
            ),
            (
                tr!("action-mirror-axis-right"),
                stokesforge_plot::AxisSide::Right,
            ),
        ] {
            let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-graph"));
            let case = active(&menu, &label);
            assert_eq!(case.toggled, Some(false));
            assert_eq!(
                clique(&mut app, &ctx, tab, case.center),
                vec![Action::SetPlotVisibility {
                    graph,
                    target: VisibilityTarget::Axis(AxisRef { panel: 0, side }),
                    visible: true,
                }]
            );
            let ouvert = entrees(&peint(&mut app, &ctx, tab, Vec::new()).0);
            assert!(entree(&ouvert, &label).is_some(), "la case garde le menu");
        }
        session.shutdown().unwrap();
    }

    /// #149 : « Copier le graphique » est dans le menu Graphique, comme
    /// l'export, et pousse la figure cadrée par la vue — le même chemin que
    /// Ctrl+C, sans condition de sélection (un clic de menu est explicite).
    #[test]
    fn copier_le_graphique_est_dans_le_menu_graphique() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        // Une série sélectionnée ne grise pas l'entrée : Ctrl+C se tait alors,
        // le menu non.
        app.with_graph_view(graph, |view| {
            view.select_series(Some(stokesforge_plot::SeriesRef::new(0, 0)), &spec)
        });
        let (ctx, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-graph"), tr!("menu-export")]);
        let entry = active(&menu, &tr!("action-copy-graph"));
        assert!(entry.enabled);
        let attendu = app
            .read_graph_view(graph, |view| view.framed_spec(&spec))
            .unwrap();
        assert_eq!(
            clique(&mut app, &ctx, tab, entry.center),
            vec![Action::CopyGraphFigure {
                spec: Box::new(attendu)
            }]
        );
        assert_ferme(&mut app, &ctx, tab);
        // Témoin : sans graphique actif, le menu Graphique n'existe pas.
        let book = app.state.books()[0].0;
        assert!(!titres(&mut app, Some(Tab::Book(book))).contains(&tr!("menu-graph")));
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
        assert!(
            !active(&menu, &tr!("menu-selected-content")).enabled,
            "aucune sélection"
        );
        let (_, export) = ouvre_chemin(&mut app, tab, &[tr!("menu-graph"), tr!("menu-export")]);
        assert!(active(&export, &tr!("action-export-graph")).enabled);
        assert!(
            !active(&menu, &tr!("action-reset-text-positions")).enabled,
            "aucun texte déplacé"
        );

        let target = AnnotationRef { panel: 0, index: 0 };
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_annotation(Some(target), &spec));
        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[tr!("menu-graph"), tr!("menu-selected-content")],
        );
        let pos = active(&menu, &tr!("action-delete-content")).center;
        assert!(active(&menu, &tr!("action-delete-content")).enabled);
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::RemovePlotObject {
                graph,
                target: stokesforge_app_state::object_manager::VisibilityTarget::Annotation(target),
            }]
        );
        session.shutdown().unwrap();
    }

    /// « Contenu sélectionné » reprend le menu de l'Object Manager : grisé
    /// sans sélection, et ses demandes passent par
    /// `run_object_manager_request`, comme celles de l'arbre.
    #[test]
    fn le_contenu_selectionne_du_menu_graphique_passe_par_l_object_manager() {
        use stokesforge_app_state::object_manager::VisibilityTarget;
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let ouvre_le_contenu = |app: &mut StokesForgeApp| {
            let (ctx, menu) = ouvre(app, tab, &tr!("menu-graph"));
            let pos = active(&menu, &tr!("menu-selected-content")).center;
            clique(app, &ctx, tab, pos);
            let (output, _) = peint(app, &ctx, tab, Vec::new());
            (ctx, entrees(&output))
        };
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-graph"));
        assert!(
            !active(&menu, &tr!("menu-selected-content")).enabled,
            "sans sélection, l'entrée est grisée et n'ouvre pas de sous-menu"
        );

        let series = stokesforge_plot::SeriesRef::new(0, 0);
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
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
    fn le_menu_affichage_ne_porte_que_les_reglages_des_panneaux() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-view"));
        assert!(entree(&menu, &tr!("project-manager-title")).is_none());
        assert!(entree(&menu, &tr!("script-title")).is_none());
        assert!(
            entree(&menu, &tr!("action-object-manager")).is_some(),
            "seul l'Object Manager porte des réglages aujourd'hui (#214)"
        );
        for open in [false, true] {
            app.state.set_object_manager_open_for_test(open);
            let (_, menu) = ouvre_chemin(
                &mut app,
                tab,
                &[tr!("menu-view"), tr!("action-object-manager")],
            );
            assert!(entree(&menu, &tr!("action-show-content")).is_none());
            for label in [
                tr!("object-manager-show-axes"),
                tr!("object-manager-show-objects"),
            ] {
                assert!(
                    active(&menu, &label).enabled,
                    "réglage actif, panneau ouvert ou non"
                );
            }
        }
        session.shutdown().unwrap();
    }

    /// #162 : « Texte SVG » est un choix exclusif sous Export ; le choix
    /// courant (Texte, par défaut) ne fait rien de plus qu'un clic sur l'autre.
    #[test]
    fn le_texte_svg_est_un_choix_exclusif_sous_export() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[
                tr!("menu-graph"),
                tr!("menu-export"),
                tr!("menu-graph-svg-text"),
            ],
        );
        assert!(entree(&menu, &tr!("menu-graph-svg-text-text")).is_some());
        assert_eq!(
            clique(
                &mut app,
                &ctx,
                tab,
                active(&menu, &tr!("menu-graph-svg-text-outlines")).center
            ),
            vec![Action::SetSvgTextMode(SvgTextMode::Outlines)]
        );
        assert_ferme(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    #[test]
    fn la_resolution_png_est_un_choix_exclusif_qui_ferme_trois_niveaux() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = ouvre_chemin(
            &mut app,
            tab,
            &[
                tr!("menu-graph"),
                tr!("menu-export"),
                tr!("menu-graph-png-resolution"),
            ],
        );
        let scale = stokesforge_app_state::state::PNG_EXPORT_SCALES
            .iter()
            .copied()
            .find(|scale| *scale != app.state.png_export_scale())
            .unwrap();
        let (w, h) = DEFAULT_FIGURE_SIZE;
        let label = tr!(
            "menu-graph-png-resolution-option",
            scale = scale,
            width = ((w as f32 * scale) as u32),
            height = ((h as f32 * scale) as u32)
        );
        assert_eq!(
            clique(&mut app, &ctx, tab, active(&menu, &label).center),
            vec![Action::SetPngExportScale(scale)]
        );
        assert_ferme(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    #[test]
    fn les_proprietes_du_graphique_ouvrent_le_bon_onglet_et_ferment_les_parents() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (_, menu) = ouvre(&mut app, tab, &tr!("menu-graph"));
        for feuille in [
            tr!("menu-graph-axes"),
            tr!("action-export-graph"),
            tr!("action-delete-content"),
        ] {
            assert!(
                entree(&menu, &feuille).is_none(),
                "la feuille {feuille} doit être rangée"
            );
        }
        let (ctx, menu) = ouvre_chemin(&mut app, tab, &[tr!("menu-graph"), tr!("menu-properties")]);
        let pos = active(&menu, &tr!("menu-graph-axes")).center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::OpenPlotSettings {
                graph,
                tab: stokesforge_app_state::state::PlotSettingsTab::Axes,
                axis: None
            }]
        );
        assert_ferme(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    #[test]
    fn les_sous_menus_des_reglages_emettent_les_actions_communes() {
        use stokesforge_app_state::preferences::ObjectManagerGroup;
        let (session, mut app, _) = banc();
        app.state.set_object_manager_open_for_test(true);
        // Les réglages restent accessibles sans graphique actif.
        let open = |app: &mut StokesForgeApp, child: Option<&str>| {
            let mut path = vec![tr!("menu-view"), tr!("action-object-manager")];
            if let Some(label) = child {
                path.push(label.to_string());
            }
            ouvre_chemin(app, None, &path)
        };
        for (label, current, action) in [
            (
                tr!("object-manager-show-axes"),
                app.state.object_manager_show_scales(),
                Action::SetObjectManagerShowScales(!app.state.object_manager_show_scales()),
            ),
            (
                tr!("object-manager-show-objects"),
                app.state.object_manager_settings().show_objects,
                Action::SetObjectManagerShowObjects(
                    !app.state.object_manager_settings().show_objects,
                ),
            ),
        ] {
            let (ctx, menu) = open(&mut app, None);
            let row = active(&menu, &label);
            assert!(row.enabled);
            assert_eq!(row.toggled, Some(current));
            assert_eq!(clique(&mut app, &ctx, None, row.center), vec![action]);
            let still_open = entrees(&peint(&mut app, &ctx, None, Vec::new()).0);
            assert!(
                entree(&still_open, &label).is_some(),
                "la bascule garde le menu ouvert"
            );
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
            let current = app.state.object_manager_settings().group_open(group);
            let (ctx, menu) = open(&mut app, Some(&tr!("object-manager-default-expansion")));
            let row = active(&menu, &label);
            assert_eq!(row.toggled, Some(current));
            assert_eq!(
                clique(&mut app, &ctx, None, row.center),
                vec![Action::SetObjectManagerGroupOpen {
                    group,
                    open: !current
                }]
            );
            let still_open = entrees(&peint(&mut app, &ctx, None, Vec::new()).0);
            assert!(
                entree(&still_open, &label).is_some(),
                "la bascule garde Expand by Default ouvert"
            );
        }
        let (ctx, menu) = open(&mut app, None);
        assert_eq!(
            clique(
                &mut app,
                &ctx,
                None,
                active(&menu, &tr!("object-manager-reset")).center
            ),
            vec![Action::ResetObjectManagerSettings]
        );
        assert_ferme(&mut app, &ctx, None);
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

    /// Help › Keyboard Shortcuts (#231, S-K5) : l'entrée existe, s'active, et
    /// demande l'ouverture de la fenêtre des raccourcis.
    #[test]
    fn le_menu_aide_ouvre_les_raccourcis_clavier() {
        let (session, mut app, graph) = banc();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = ouvre(&mut app, tab, &tr!("menu-help"));
        let entry = active(&menu, &tr!("action-keyboard-shortcuts"));
        let pos = entry.center;
        assert_eq!(
            clique(&mut app, &ctx, tab, pos),
            vec![Action::OpenKeyboardShortcuts]
        );
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

    /// « Prêt » sans message ; trois niveaux (S-M4) : l'information garde la
    /// couleur courante, l'avertissement la teinte d'avertissement, l'erreur
    /// la couleur d'erreur ; ni révision ni lien d'import (S-R15).
    #[test]
    fn la_barre_d_etat_distingue_les_trois_niveaux() {
        let (session, mut app, _) = banc();
        let ctx = contexte();
        let output = barre_d_etat(&app, &ctx);
        assert!(couleur_du_texte(&output, &tr!("status-ready")).is_some());
        assert!(couleur_du_texte(&output, "Revision").is_none());
        let visuals = &ctx.global_style().visuals;
        let (erreur, avertissement) = (visuals.error_fg_color, visuals.warn_fg_color);

        app.state.notice("Avis".into());
        let output = barre_d_etat(&app, &ctx);
        assert!(couleur_du_texte(&output, &tr!("status-ready")).is_none());
        assert_ne!(couleur_du_texte(&output, "Avis"), Some(erreur), "témoin");
        assert_ne!(
            couleur_du_texte(&output, "Avis"),
            Some(avertissement),
            "témoin"
        );

        app.state.warn("Attention".into());
        let output = barre_d_etat(&app, &ctx);
        assert_eq!(couleur_du_texte(&output, "Attention"), Some(avertissement));

        // Un geste refusé (la dernière feuille d'un classeur) ; un projet
        // illisible, lui, est un échec grave dit par un dialogue (S-M8, #229).
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        app.state.dispatch(Action::DeleteSheet(sheet));
        let status = app.state.status().expect("un refus se dit");
        assert!(status.is_error());
        let text = status.text.clone();
        let output = barre_d_etat(&app, &ctx);
        assert_eq!(couleur_du_texte(&output, &text), Some(erreur));
        session.shutdown().unwrap();
    }

    /// Un clic sur la barre d'état demande l'historique des messages (S-M6).
    #[test]
    fn un_clic_sur_la_barre_d_etat_ouvre_l_historique() {
        let (session, mut app, _) = banc();
        app.state.notice("Message".into());
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
        let pos = find_text_center(&output.shapes, "Message");
        paint(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            paint(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            }]);
        }
        assert_eq!(actions, vec![Action::OpenStatusHistory]);
        session.shutdown().unwrap();
    }
}

//! Menu bar and status bar of the main window:
//! [`StokesForgeApp::menu_bar`] ("File", "Edit", "Workbook", "Graph", "View",
//! "Window") and [`StokesForgeApp::status_bar`].
//!
//! The greying of each entry is decided here, from the state and the active
//! tab that [`StokesForgeApp::ui`](eframe::App::ui) computes once per frame; the
//! entries push [`Action`]s, applied after painting by the dispatch loop of
//! `app.rs`. The order of the panels (the menu bar is the first
//! `egui::Panel::top`, the status bar the first `egui::Panel::bottom`) stays
//! decided in `ui()`.
//!
//! # Submenus (#114)
//! Lists are grouped by theme, with the same components as the right-clicks.
//! Display toggles must stay open, actions and exclusive choices close the
//! chain: `CloseOnClickOutside` inherited by the submenus avoids egui's
//! automatic closing on a checkbox; each action calls `ui.close()`, propagated
//! by egui to the parents.
//!
//! # Grammar (#152)
//! Each menu arranges its entries by sections (`MenuSections::section`,
//! `menu_sections.rs`) according to `docs/menus.md`: here, the bar **wraps**
//! the contents shared with the right-clicks (Selection, Sheet, Columns…),
//! which display them flat.
//!
//! Each menu has its own function (#124), called in the same Ui and in the same
//! order; the Object Manager requests stay deferred after the panel is closed.
//! No egui scope depends on the Rust split.

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
use super::tab::Tab;
use super::tab::{active_tab_kind, tab_title};
use super::workbook::workbook_menu;
use super::StokesForgeApp;

impl StokesForgeApp {
    /// Menu bar: "File", "Edit", "Workbook" (only when a workbook is in the
    /// foreground), "Graph" (only when the active tab is a graph, see
    /// [`active_graph`]), "View" and "Window".
    ///
    /// "Graph" opens the graph properties window on each of its tabs
    /// ([`plot_settings_window`](crate::app::plot_settings::plot_settings_window)) — the same pages as the right-click
    /// on a graph component ([`graph_view_contents`](crate::app::graph_window::graph_view_contents)), so
    /// that no setting is reachable **only** by right-click — and resets the
    /// zoom.
    ///
    /// "Window" carries the two features that concern the layout itself
    /// rather than the model: maximize/restore the active tab, and
    /// detach/reattach the Project Manager in a separate OS window — see the
    /// head doc of `window_stack.rs` and `window_detach.rs`.
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
                    let mut titles = vec![self.file_menu(ui, ctx, actions)];
                    titles.push(self.edit_menu(ui, ctx, actions));
                    titles.extend(self.workbook_menu(ui, ctx, actions));
                    if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                        titles.push(self.graph_menu(ui, ctx, graph, actions, &mut object_requests));
                    }
                    titles.push(self.analysis_menu(
                        ui,
                        actions,
                        active_graph(active_tab_kind(active_tab)),
                    ));
                    titles.push(self.view_menu(ui, actions));
                    titles.push(self.window_menu(ui));
                    titles.push(help_menu(ui, actions));
                    toggle_on_hover(ctx, &titles);
                });
        });
        if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
            for request in object_requests {
                self.run_object_manager_request(ctx, graph, request, actions);
            }
        }
    }

    // Each function keeps the same menu Uis: extracting Rust must not add any
    // egui scope nor move the deferred application of the requests.
    fn file_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
    ) -> egui::Response {
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
                    if ui
                        .button(tr!("action-add-folder"))
                        .on_hover_text(tr!("action-add-folder", "hint"))
                        .clicked()
                    {
                        actions.push(Action::AddFolder {
                            parent: self.state.tree().root(),
                        });
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
        })
        .response
    }

    fn edit_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
    ) -> egui::Response {
        ui.menu_button(tr!("menu-edit"), |ui| {
            // The full label ("Undo Edit Cells in Column A") and the greying
            // are two distinct lines here; Qt kept them together in a single
            // `QAction`.
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
                .filter(|id| self.pm_selection.iter().any(|entry| entry.node() == *id))
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
                        super::project_manager::paste_parent(&self.pm_selection, root),
                        super::project_manager::selection_has_folder(&self.pm_selection),
                        actions,
                        rename_viewport,
                    );
                });
            });
        })
        .response
    }

    fn workbook_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Vec<Action>,
    ) -> Option<egui::Response> {
        let book = self.active_book()?;
        let sheet = self.state.selected_sheet_in(book)?;
        Some(
            ui.menu_button(tr!("menu-workbook"), |ui| {
                workbook_menu(ui, ctx, &self.state, book, sheet, actions);
            })
            .response,
        )
    }

    /// Analysis menu (#181, #210): the selection of the active graph takes
    /// priority; otherwise the Y columns of the workbook. With no target: greyed out.
    fn analysis_menu(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        graph: Option<NodeId>,
    ) -> egui::Response {
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
                // Whole rows of a sheet at x' (#253): we analyze
                // rows, not the columns they cross.
                None if !row_pairs.is_empty() => {
                    Some(super::analysis_menu::AnalysisTarget::Rows { rows: &row_pairs })
                }
                None => sheet.map(|sheet| super::analysis_menu::AnalysisTarget::Columns {
                    sheet,
                    columns: &columns,
                }),
            };
            super::analysis_menu::analysis_menu_contents(ui, &self.state, actions, target);
        })
        .response
    }

    fn graph_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        graph: NodeId,
        actions: &mut Vec<Action>,
        object_requests: &mut Vec<ObjectManagerRequest>,
    ) -> egui::Response {
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
            // Free texts (issue #35): the counterpart, in the menu bar, of
            // the right-click on the background.
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
                // Overlaid layer (#167), in the manner of Origin's "New Layer (Axes)",
                // on the active layer.
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
                        // At most three overlaid layers per box (#168).
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
            // The menu of the selected object is that of the right-click and of
            // the Object Manager; with no selection, the entry is greyed out.
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
            // Mirror axes (#176) of the active layer: real display toggles,
            // they keep the menu open.
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
        })
        .response
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) -> egui::Response {
        ui.menu_button(tr!("menu-view"), |ui| {
            // No panels or toolbars to hide: the menu only carries the
            // settings of each panel (Object Manager, later Project Manager
            // and script console).
            ui.menu_button(tr!("action-object-manager"), |ui| {
                object_manager_settings_menu(ui, self.state.object_manager_settings(), actions);
            });
        })
        .response
    }

    fn window_menu(&mut self, ui: &mut egui::Ui) -> egui::Response {
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
                    // The most recently focused floating window —
                    // last element of `floating_tabs`, see the head doc of
                    // `window_stack.rs`. The Project Manager never appears
                    // there, it is not a floating window, so there is
                    // nothing special to exclude here.
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
            // No "Attach Left/Right" here: the Project Manager is a fixed
            // panel, pinned to the left, since this task (see the head doc of
            // `app.rs`, § attaching left or right — the right side and the
            // repositioning by dragging are future work deliberately out of
            // scope).
        })
        .response
    }

    /// Status bar: the last message, at three levels (S-M4); a click opens
    /// the history of the last 50 (S-M6). No revision nor "Reopen Last
    /// Import…" (S-R15: it stays in File).
    ///
    /// The dimensions (rows × columns) it used to show disappeared with the
    /// global selected "the" sheet: each workbook tab shows them for its own
    /// sheet, in [`book_contents`](crate::app::workbook::book_contents).
    pub(super) fn status_bar(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::bottom("stokesforge_status")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // A long-running task replaces the message (#232, S-P1/S-P2);
                    // under 0.5 s the building block paints nothing and the
                    // "Importing…" message stays the status line.
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
                    // The clickable area covers the whole line, over its
                    // elements (a `Label` does not carry the click).
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
                    // The badge of the project's weak R² (#290): a state,
                    // visible as long as any remain, even if the box was
                    // opened or could not open.
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

/// Text and tint of the weak R² badge (#290), without an egui context:
/// `None` when there are none (no badge).
fn low_r2_badge_content(total: usize, dark_mode: bool) -> Option<(String, egui::Color32)> {
    use stokesforge_app_state::r_squared::{badge_text, RSquaredLevel};
    (total > 0).then(|| {
        (
            badge_text(total),
            crate::theme::r_squared_color(RSquaredLevel::Dialog, dark_mode),
        )
    })
}

/// The "⚠ 13 low R²" badge (#290): warning icon and text in
/// the red tint of R² (`theme::r_squared_color`); a click opens the list by
/// type, the tooltip says so.
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

/// A message line according to its level: normal text, text in the warning
/// tint with its triangle, or red text with its cross. Shared by the status
/// bar and the history window.
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

/// Toggle on hover (#343): a bar title is already open and the pointer
/// passes over another title, which opens while the open one closes,
/// without a click. egui only does this for submenus. Bar at rest: nothing
/// (no opening on a mere hover). The open title is recognized by the
/// identifier of its popup, never by `Popup::is_any_open` (an open context
/// menu must not toggle anything). When the pointer is heading toward the
/// open popup (diagonal toward an entry), we do not toggle and we repaint to
/// detect the end of the gesture, like egui does for its submenus.
fn toggle_on_hover(ctx: &egui::Context, titles: &[egui::Response]) {
    let popup_of = |title: &egui::Response| egui::Popup::default_response_id(title);
    let Some(open_title) = titles
        .iter()
        .find(|title| egui::Popup::is_id_open(ctx, popup_of(title)))
    else {
        return;
    };
    let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    let Some(target_title) = titles
        .iter()
        .find(|title| title.id != open_title.id && title.rect.contains(pos))
    else {
        return;
    };
    let toward_popup = ctx
        .memory(|mem| mem.area_rect(popup_of(open_title)))
        .is_some_and(|rect| {
            ctx.input(|input| {
                let pointer = &input.pointer;
                // `Rect::intersects_ray` ignores the other axis when the
                // direction is exactly horizontal or vertical: we
                // rule out these cases here (the ray misses the rectangle).
                let direction = pointer.direction();
                let misses = (direction.y == 0.0 && !(rect.min.y..=rect.max.y).contains(&pos.y))
                    || (direction.x == 0.0 && !(rect.min.x..=rect.max.x).contains(&pos.x));
                !misses && pointer.is_moving_towards_rect(&rect)
            })
        });
    if toward_popup {
        ctx.request_repaint();
        return;
    }
    egui::Popup::open_id(ctx, popup_of(target_title));
    ctx.request_repaint();
}

/// Help menu (#231, S-K5): the help generated from the code (keyboard
/// shortcuts) and About StokesForge (#382). Direct entries.
fn help_menu(ui: &mut egui::Ui, actions: &mut Vec<Action>) -> egui::Response {
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
            if ui
                .button(tr!("action-about"))
                .on_hover_text(tr!("action-about", "hint"))
                .clicked()
            {
                actions.push(Action::OpenAbout);
                ui.close();
            }
        });
    })
    .response
}

#[cfg(test)]
mod tests {
    use super::super::project_manager::tests::find_text_center;
    use super::super::shortcuts::shortcut_text_delete;
    use super::super::test_support::{self, xy_columns};
    use super::*;
    use stokesforge_app_state::state::AppState;
    use stokesforge_core::{NodeId, Project};
    use stokesforge_plot::{AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesSpec};
    use stokesforge_session::Session;

    /// The language is global: the badge tests set it to English
    /// under this lock.
    static LANGUAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_weak_r2_badge_exists_only_if_there_are_any() {
        assert_eq!(low_r2_badge_content(0, false), None);
        assert_eq!(low_r2_badge_content(0, true), None);
    }

    #[test]
    fn the_weak_r2_badge_states_the_count() {
        use stokesforge_app_state::i18n::{set_language, Language};
        let _lock = LANGUAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_language(Language::English);
        for (total, expected) in [(1, "1 low R²"), (2, "2 low R²"), (13, "13 low R²")] {
            let (text, _) = low_r2_badge_content(total, false).unwrap();
            assert_eq!(text, expected);
        }
    }

    #[test]
    fn the_weak_r2_badge_takes_the_box_tint_and_contrasts() {
        use stokesforge_app_state::r_squared::RSquaredLevel::Dialog;
        use stokesforge_render::color::contrast_ratio;
        let rgb = |c: egui::Color32| (c.r(), c.g(), c.b());
        let mut tints = Vec::new();
        for (dark, background) in [(false, (255, 255, 255)), (true, (27, 27, 27))] {
            let (_, color) = low_r2_badge_content(3, dark).unwrap();
            assert_eq!(color, crate::theme::r_squared_color(Dialog, dark));
            let ratio = contrast_ratio(rgb(color), background);
            assert!(ratio >= 3.0, "dark = {dark}: contrast {ratio:.2} < 3");
            tints.push(color);
        }
        assert_ne!(tints[0], tints[1]);
    }

    /// A workbook (one sheet, two columns) and a graph that plots them,
    /// with a free text. `StokesForgeApp::new` opens the workbook then the
    /// graph: the graph is in the foreground.
    fn bench() -> (Session, StokesForgeApp, NodeId) {
        let mut project = Project::new("Menus").unwrap();
        let root = project.root();
        let (x, y) = xy_columns(&mut project);
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        panel
            .annotations
            .push(AnnotationSpec::new("Text", 0.5, 0.5));
        let graph = project
            .add_graph(root, None, PlotSpec::empty("Figure").with_panels(panel))
            .unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        (session, StokesForgeApp::new(state), graph)
    }

    /// An importable file, and the state after its import into a new
    /// workbook: "Reopen Last Import" has something to reopen, and this workbook
    /// has a source to re-read.
    fn import_file(state: &mut AppState, dir: &tempfile::TempDir) {
        let file = dir.path().join("measurement.csv");
        std::fs::write(&file, "1,10\n2,20\n").unwrap();
        state.dispatch(Action::ImportFiles {
            destination: stokesforge_app_state::state::ImportDestination::NewBookForAll,
            recipe: stokesforge_import::ImportRecipe::new("Untitled"),
            paths: vec![file],
        });
    }

    /// An entry of the menu bar (title or content of an open menu),
    /// as egui describes it to accessibility: its label, its greying,
    /// the state of a checkbox and the center of its rectangle.
    #[derive(Debug)]
    struct MenuEntry {
        label: String,
        enabled: bool,
        toggled: Option<bool>,
        center: egui::Pos2,
    }

    /// The shortcut texts that a menu button adds to its accessibility
    /// label ("Save Ctrl+S"): removed, to compare with the catalog label
    /// alone.
    fn shortcut_texts() -> Vec<String> {
        let ctx = egui::Context::default();
        let mut texts: Vec<String> = [
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
        texts.push(SHORTCUT_IMPORT_TEXT.to_string());
        texts.push(shortcut_text_delete());
        texts.push(ctx.format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::C,
        )));
        texts.push(ctx.format_shortcut(&super::super::SHORTCUT_DUPLICATE_SHEET));
        texts.push("F2".into());
        texts
    }

    fn entries(output: &egui::FullOutput) -> Vec<MenuEntry> {
        let shortcut_texts = shortcut_texts();
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accessibility is enabled by `paint_bar`");
        update
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                let mut label = node.label()?.to_string();
                if let Some(short) = shortcut_texts
                    .iter()
                    .map(|r| format!(" {r}"))
                    .chain([" ⏵".to_string()])
                    .find_map(|suffix| label.strip_suffix(&suffix).map(str::to_string))
                {
                    label = short;
                }
                let bounds = node.bounds()?;
                Some(MenuEntry {
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

    fn find_entry<'a>(entries: &'a [MenuEntry], label: &str) -> Option<&'a MenuEntry> {
        entries.iter().find(|entry| entry.label == label)
    }

    fn active<'a>(entries: &'a [MenuEntry], label: &str) -> &'a MenuEntry {
        entries
            .iter()
            .filter(|entry| entry.label == label)
            .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
            .unwrap_or_else(|| panic!("entry {label:?} missing from {entries:#?}"))
    }

    /// A frame of the menu bar alone, on the active tab `tab`.
    fn paint_bar(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<Action>) {
        paint_bar_with(app, ctx, tab, events, egui::Modifiers::NONE)
    }

    /// Like [`paint_bar`], with modifier keys held (Ctrl+click).
    fn paint_bar_with(
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
                // egui 0.36: held keys go through an event.
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

    /// The common test context, plus image loaders, instant
    /// animations and AccessKit (menu entries are read through the
    /// accessibility tree).
    fn context() -> egui::Context {
        let ctx = test_support::context();
        egui_extras::install_image_loaders(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        ctx.enable_accesskit();
        ctx
    }

    /// The bar titles, menus closed.
    fn titles(app: &mut StokesForgeApp, tab: Option<Tab>) -> Vec<String> {
        let ctx = context();
        let (output, _) = paint_bar(app, &ctx, tab, Vec::new());
        entries(&output).into_iter().map(|e| e.label).collect()
    }

    /// Clicks `pos`, collecting the actions of all the frames of the click.
    fn click_at(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        pos: egui::Pos2,
    ) -> Vec<Action> {
        click_at_with(app, ctx, tab, pos, egui::Modifiers::NONE)
    }

    /// A left click, `modifiers` held during the whole gesture.
    fn click_at_with(
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
            actions.extend(paint_bar_with(app, ctx, tab, events, modifiers).1);
        }
        actions
    }

    /// Opens the menu `title` and returns its content, with the context where it
    /// is open (to click in it afterwards).
    fn open_menu(
        app: &mut StokesForgeApp,
        tab: Option<Tab>,
        title: &str,
    ) -> (egui::Context, Vec<MenuEntry>) {
        let ctx = context();
        let (output, _) = paint_bar(app, &ctx, tab, Vec::new());
        let pos = active(&entries(&output), title).center;
        click_at(app, &ctx, tab, pos);
        let (output, _) = paint_bar(app, &ctx, tab, Vec::new());
        let content = entries(&output);
        assert!(
            content.len() > titles(app, tab).len(),
            "menu {title:?} must have opened: {content:#?}"
        );
        (ctx, content)
    }

    /// Each level opens through a real click, with the parents' context.
    fn open_path(
        app: &mut StokesForgeApp,
        tab: Option<Tab>,
        path: &[String],
    ) -> (egui::Context, Vec<MenuEntry>) {
        let (ctx, mut menu) = open_menu(app, tab, &path[0]);
        for parent in &path[1..] {
            let pos = menu
                .iter()
                .filter(|entry| &entry.label == parent)
                .max_by(|a, b| a.center.y.total_cmp(&b.center.y))
                .unwrap_or_else(|| panic!("submenu {parent} missing: {menu:#?}"))
                .center;
            assert!(click_at(app, &ctx, tab, pos).is_empty());
            menu = entries(&paint_bar(app, &ctx, tab, Vec::new()).0);
        }
        (ctx, menu)
    }

    fn assert_closed(app: &mut StokesForgeApp, ctx: &egui::Context, tab: Option<Tab>) {
        let menu = entries(&paint_bar(app, ctx, tab, Vec::new()).0);
        let mut visible_labels: Vec<_> = menu.iter().map(|e| e.label.clone()).collect();
        let mut closed_titles = titles(app, tab);
        visible_labels.sort();
        closed_titles.sort();
        assert_eq!(
            visible_labels, closed_titles,
            "the choice closes all levels"
        );
    }

    /// One frame per position: the pointer drags without clicking. Returns the
    /// output of the last frame.
    fn drag_pointer(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        positions: &[egui::Pos2],
    ) -> egui::FullOutput {
        let mut output = None;
        for pos in positions {
            output = Some(paint_bar(app, ctx, tab, vec![egui::Event::PointerMoved(*pos)]).0);
        }
        output.expect("at least one position")
    }

    /// #343: a bar menu is open, the pointer slides horizontally
    /// onto the neighboring title: it opens, the first one closes.
    #[test]
    fn hovering_another_title_toggles_the_open_menu() {
        let (session, mut app, _) = bench();
        let tab = app.floating_tabs.last().copied();
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-file"));
        let file_title = active(&menu, &tr!("menu-file")).center;
        let edit_title = active(&menu, &tr!("menu-edit")).center;
        assert!(find_entry(&menu, &tr!("action-save")).is_some(), "witness");
        let output = drag_pointer(
            &mut app,
            &ctx,
            tab,
            &[
                file_title,
                egui::pos2((file_title.x + edit_title.x) / 2.0, file_title.y),
                edit_title,
                edit_title,
            ],
        );
        drop(output);
        let after = entries(&paint_bar(&mut app, &ctx, tab, Vec::new()).0);
        assert!(
            find_entry(&after, &tr!("action-save")).is_none(),
            "File closed"
        );
        assert!(
            find_entry(&after, &tr!("history-undo")).is_some(),
            "Edit open: {after:#?}"
        );
        session.shutdown().unwrap();
    }

    /// #343: bar at rest, hovering alone opens nothing.
    #[test]
    fn hovering_a_bar_at_rest_opens_nothing() {
        let (session, mut app, _) = bench();
        let tab = app.floating_tabs.last().copied();
        let ctx = context();
        let (output, _) = paint_bar(&mut app, &ctx, tab, Vec::new());
        let edit_title = active(&entries(&output), &tr!("menu-edit")).center;
        drag_pointer(
            &mut app,
            &ctx,
            tab,
            &[edit_title - egui::vec2(30.0, 0.0), edit_title, edit_title],
        );
        assert_closed(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    /// The entries visible after `frames` frames without movement: the pointer
    /// speed drops, it ends up stationary.
    fn wait_frames(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        frames: usize,
    ) -> Vec<MenuEntry> {
        let mut output = None;
        for _ in 0..frames {
            output = Some(paint_bar(app, ctx, tab, Vec::new()).0);
        }
        entries(&output.expect("at least one frame"))
    }

    /// #343: when aiming at the open popup (diagonal crossing the
    /// neighboring title), the open menu stays; once the pointer is still on the
    /// neighbor, the toggle happens.
    #[test]
    fn the_diagonal_toward_the_popup_does_not_toggle_then_stillness_toggles() {
        let (session, mut app, _) = bench();
        let tab = app.floating_tabs.last().copied();
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-file"));
        let edit_title = active(&menu, &tr!("menu-edit")).center;
        let output = drag_pointer(
            &mut app,
            &ctx,
            tab,
            &[
                edit_title + egui::vec2(-34.0, -3.0),
                edit_title + egui::vec2(-17.0, 2.0),
                edit_title + egui::vec2(0.0, 7.0),
            ],
        );
        drop(output);
        let on_the_way = wait_frames(&mut app, &ctx, tab, 1);
        assert!(
            find_entry(&on_the_way, &tr!("action-save")).is_some(),
            "File stays open: {on_the_way:#?}"
        );
        assert!(find_entry(&on_the_way, &tr!("history-undo")).is_none());

        let after = wait_frames(&mut app, &ctx, tab, 40);
        assert!(
            find_entry(&after, &tr!("action-save")).is_none(),
            "File closed"
        );
        assert!(
            find_entry(&after, &tr!("history-undo")).is_some(),
            "Edit open"
        );
        session.shutdown().unwrap();
    }

    /// The menu extractions keep their order; changing the active content
    /// only replaces the menu specific to the workbook or the graph.
    #[test]
    fn the_menus_keep_their_order_according_to_the_active_content() {
        let (session, mut app, graph) = bench();
        assert_eq!(
            titles(&mut app, Some(Tab::Graph(graph))),
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
            titles(&mut app, Some(Tab::Book(book))),
            [
                tr!("menu-file"),
                tr!("menu-edit"),
                tr!("menu-workbook"),
                tr!("menu-analysis"),
                tr!("menu-view"),
                tr!("menu-window"),
                tr!("menu-help")
            ],
            "witness: the menu specific to the active content changes in the same place",
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn the_file_menu_greys_out_according_to_the_active_workbook_and_the_last_import() {
        // Without an import: nothing to re-read or reopen.
        let (session, mut app, _) = bench();
        let tab = app.floating_tabs.last().copied();
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-file"));
        assert!(active(&menu, &tr!("action-save")).enabled, "witness");
        assert!(find_entry(&menu, &tr!("action-add-workbook")).is_none());
        assert!(find_entry(&menu, &tr!("action-refresh-from-source")).is_none());
        let (_, new_menu) = open_path(&mut app, tab, &[tr!("menu-file"), tr!("menu-new")]);
        assert!(active(&new_menu, &tr!("action-add-workbook")).enabled);
        assert!(active(&new_menu, &tr!("action-add-folder")).enabled);
        assert_eq!(
            active(&new_menu, &tr!("action-new-project")).enabled,
            app.state.can_new_project()
        );
        let (_, menu) = open_path(&mut app, tab, &[tr!("menu-file"), tr!("menu-import")]);
        assert!(!active(&menu, &tr!("action-reopen-last-import")).enabled);
        assert!(!active(&menu, &tr!("action-refresh-from-source")).enabled);
        session.shutdown().unwrap();

        // After an import into a new workbook: the re-read follows the
        // foreground workbook, not the mere existence of a source.
        let (session, app, _) = bench();
        let mut state = app.state;
        let dir = tempfile::tempdir().unwrap();
        import_file(&mut state, &dir);
        let imported = state.books().last().unwrap().0;
        let mut app = StokesForgeApp::new(state);
        let tab = app.floating_tabs.last().copied();
        assert!(matches!(tab, Some(Tab::Graph(_))));
        let (_, menu) = open_path(&mut app, tab, &[tr!("menu-file"), tr!("menu-import")]);
        assert!(active(&menu, &tr!("action-reopen-last-import")).enabled);
        assert!(
            !active(&menu, &tr!("action-refresh-from-source")).enabled,
            "a graph in the foreground has no sheet to re-read"
        );
        app.open_tab(Tab::Book(imported));
        let tab = app.floating_tabs.last().copied();
        let (_, menu) = open_path(&mut app, tab, &[tr!("menu-file"), tr!("menu-import")]);
        assert!(active(&menu, &tr!("action-refresh-from-source")).enabled);

        // A clicked entry pushes its action, nothing more.
        let (ctx, menu) = open_path(&mut app, tab, &[tr!("menu-file"), tr!("menu-new")]);
        let pos = active(&menu, &tr!("action-add-workbook")).center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::AddWorkbook]
        );
        assert_closed(&mut app, &ctx, tab);

        // #326: Add Folder targets the project root.
        let root = app.state.tree().root();
        let (ctx, menu) = open_path(&mut app, tab, &[tr!("menu-file"), tr!("menu-new")]);
        let pos = active(&menu, &tr!("action-add-folder")).center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::AddFolder { parent: root }]
        );
        assert_closed(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    #[test]
    fn the_edit_menu_names_and_greys_out_undo_and_redo() {
        let (session, mut app, _) = bench();
        let tab = app.floating_tabs.last().copied();
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-edit"));
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
        let undo = app.state.undo_label().expect("a command to undo");
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-edit"));
        assert!(active(&menu, &undo).enabled, "the label names the command");
        assert!(find_entry(&menu, &tr!("history-undo")).is_none());
        assert!(!active(&menu, &tr!("history-redo")).enabled);

        app.state.dispatch(Action::Undo);
        let redo = app.state.redo_label().expect("a command to redo");
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-edit"));
        assert!(!active(&menu, &tr!("history-undo")).enabled);
        let pos = active(&menu, &redo).center;
        assert_eq!(click_at(&mut app, &ctx, tab, pos), vec![Action::Redo]);
        session.shutdown().unwrap();
    }

    #[test]
    fn edit_selection_keeps_the_actions_and_greying_of_the_project_manager() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-edit"));
        assert!(find_entry(&menu, &tr!("action-delete-project-object")).is_none());
        app.pm_selection.clear();
        let (_, menu) = open_path(
            &mut app,
            tab,
            &[tr!("menu-edit"), tr!("menu-selected-content")],
        );
        assert!(!active(&menu, &tr!("action-delete-project-object")).enabled);
        assert!(!active(&menu, &tr!("action-copy")).enabled);
        assert!(
            active(&menu, &tr!("action-paste")).enabled,
            "witness independent of the selection"
        );
        app.pm_selection
            .insert(super::super::project_manager::PmEntry::Window(Tab::Graph(
                graph,
            )));
        let (ctx, menu) = open_path(
            &mut app,
            tab,
            &[tr!("menu-edit"), tr!("menu-selected-content")],
        );
        let copy = active(&menu, &tr!("action-copy"));
        assert!(copy.enabled);
        assert_eq!(
            click_at(&mut app, &ctx, tab, copy.center),
            vec![Action::CopyObjects(vec![graph])]
        );
        assert_closed(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    /// Analysis menu (#181): greyed out as long as no Y column is targeted, then
    /// each entry opens the window on its family and method;
    /// "Recalculate Out of Date" stays greyed out when everything is up to date.
    #[test]
    fn the_analysis_menu_opens_the_series_of_the_active_graph() {
        let (_session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_series(Some(stokesforge_plot::SeriesRef::new(0, 0)), &spec)
        });
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-analysis"));
        let entry = active(&menu, &tr!("action-analysis-integrate"));
        assert!(entry.enabled);
        assert_eq!(
            click_at(&mut app, &ctx, tab, entry.center),
            vec![Action::OpenGraphAnalysis {
                preset: stokesforge_app_state::analysis::Preset::Integrate,
                graph,
                series: vec![stokesforge_plot::SeriesRef::new(0, 0)],
                spec: Box::new(spec),
            }]
        );
    }

    #[test]
    fn the_analysis_menu_greys_out_without_a_y_column_and_opens_the_window() {
        use stokesforge_app_state::analysis::Preset;
        let (session, mut app, _) = bench();
        let book = app.state.books()[0].0;
        app.open_tab(Tab::Book(book));
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let tab = Some(Tab::Book(book));
        app.state.deselect_all(sheet);

        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-analysis"));
        for label in [
            tr!("analysis-category-data-manipulation"),
            tr!("analysis-category-fitting"),
            tr!("analysis-category-peaks"),
            tr!("action-analysis-integrate"),
        ] {
            assert!(!active(&menu, &label).enabled, "no column: {label}");
        }
        assert!(
            !active(&menu, &tr!("action-analysis-recalculate-stale")).enabled,
            "everything is up to date"
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
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-analysis"));
        let integrate = active(&menu, &tr!("action-analysis-integrate"));
        assert!(integrate.enabled, "a Y column is selected");
        assert_eq!(
            click_at(&mut app, &ctx, tab, integrate.center),
            vec![Action::OpenAnalysis {
                preset: Preset::Integrate,
                sheet,
                columns: selected.clone(),
            }]
        );
        assert_closed(&mut app, &ctx, tab);

        let (ctx, menu) = open_path(
            &mut app,
            tab,
            &[
                tr!("menu-analysis"),
                tr!("analysis-category-data-manipulation"),
            ],
        );
        let anchors = active(&menu, &tr!("analysis-baseline-anchors"));
        assert_eq!(
            click_at(&mut app, &ctx, tab, anchors.center),
            vec![Action::OpenAnalysis {
                preset: Preset::BaselineAnchors,
                sheet,
                columns: selected,
            }]
        );
        assert_closed(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    /// #248: the Analysis menu has one entry per category, and the methods of
    /// each category are in its submenu — "Reference Correction" belongs to
    /// Data Manipulation, not to Fitting.
    #[test]
    fn the_analysis_menu_files_methods_by_category() {
        let (session, mut app, _) = bench();
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

        let path = |app: &mut StokesForgeApp, category: &str| {
            let (_, menu) = open_path(app, tab, &[tr!("menu-analysis"), category.to_string()]);
            menu.into_iter()
                .map(|entry| entry.label)
                .collect::<Vec<_>>()
        };
        let data = path(&mut app, &tr!("analysis-category-data-manipulation"));
        for expected in [
            tr!("analysis-baseline-constant"),
            tr!("analysis-baseline-linear"),
            tr!("analysis-baseline-polynomial"),
            tr!("analysis-baseline-anchors"),
            tr!("action-analysis-correct"),
        ] {
            assert!(
                data.contains(&expected),
                "Data Manipulation: {expected} in {data:?}"
            );
        }
        let fitting = path(&mut app, &tr!("analysis-category-fitting"));
        assert!(
            fitting.contains(&tr!("analysis-fit-gaussian")),
            "{fitting:?}"
        );
        assert!(fitting.contains(&tr!("analysis-fit-custom")), "{fitting:?}");
        assert!(
            !fitting.contains(&tr!("action-analysis-correct")),
            "witness: the correction is not a fit"
        );
        let peaks = path(&mut app, &tr!("analysis-category-peaks"));
        for expected in [
            tr!("action-analysis-peaks"),
            tr!("action-analysis-reconvolution"),
            tr!("action-analysis-direct"),
        ] {
            assert!(peaks.contains(&expected), "{expected} in {peaks:?}");
        }
        session.shutdown().unwrap();
    }

    /// #249: the last analyses run are listed at the bottom of the menu; a
    /// click opens the window, Ctrl+click runs (same gesture, `run` in addition).
    #[test]
    fn the_analysis_menu_lists_the_last_analyses_and_ctrl_click_runs_them() {
        use stokesforge_app_state::analysis::{Preset, RecentTarget};
        let (session, mut app, _) = bench();
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

        // Witness: nothing has been run, nothing is listed.
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-analysis"));
        let numbered = |label: &str| label.starts_with("1 ");
        assert!(menu.iter().all(|entry| !numbered(&entry.label)), "{menu:?}");

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
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-analysis"));
        let entry = active(&menu, &label);
        assert!(entry.enabled);
        let target = RecentTarget::Columns {
            sheet,
            columns: selected,
        };
        assert_eq!(
            click_at(&mut app, &ctx, tab, entry.center),
            vec![Action::OpenRecentAnalysis {
                index: 0,
                run: false,
                target: target.clone(),
            }]
        );
        assert_closed(&mut app, &ctx, tab);

        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-analysis"));
        let entry = active(&menu, &label);
        assert_eq!(
            click_at_with(&mut app, &ctx, tab, entry.center, egui::Modifiers::COMMAND),
            vec![Action::OpenRecentAnalysis {
                index: 0,
                run: true,
                target,
            }]
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn the_workbook_menu_exists_only_with_a_workbook_in_the_foreground() {
        let (session, mut app, graph) = bench();
        let book = app.state.books()[0].0;
        assert_eq!(app.floating_tabs.last(), Some(&Tab::Graph(graph)));
        let menus = titles(&mut app, Some(Tab::Graph(graph)));
        assert!(menus.contains(&tr!("menu-file")), "witness: {menus:?}");
        assert!(!menus.contains(&tr!("menu-workbook")), "{menus:?}");

        app.open_tab(Tab::Book(book));
        let (_, menu) = open_menu(&mut app, Some(Tab::Book(book)), &tr!("menu-workbook"));
        assert!(
            menu.iter().any(|entry| entry.label != tr!("menu-workbook")
                && !titles(&mut app, Some(Tab::Book(book))).contains(&entry.label)),
            "the Workbook menu has content: {menu:#?}"
        );
        session.shutdown().unwrap();
    }

    /// #330: the width reset is in Workbook > Sheet (the whole
    /// sheet) and Workbook > Columns (the selection), greyed out with no remembered
    /// width. Witnesses tried out: always-enabled entry, or a single column
    /// targeted for the sheet (red).
    #[test]
    fn the_width_reset_is_in_workbook_sheet_and_columns() {
        let (session, mut app, _) = bench();
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
        let sheet_path = [tr!("menu-workbook"), tr!("menu-sheet")];
        let columns_path = [tr!("menu-workbook"), tr!("menu-columns")];
        let sheet_label = tr!("action-reset-sheet-column-widths");
        let column_label = tr!("action-reset-column-widths");

        app.state.select_column(sheet, columns[0]);
        let (_, menu) = open_path(&mut app, tab, &sheet_path);
        assert!(!active(&menu, &sheet_label).enabled, "no width remembered");
        let (_, menu) = open_path(&mut app, tab, &columns_path);
        assert!(!active(&menu, &column_label).enabled, "no width remembered");

        app.state.dispatch(Action::SetColumnWidth {
            column: columns[0],
            width: 80.0,
        });
        let (ctx, menu) = open_path(&mut app, tab, &sheet_path);
        let entry = active(&menu, &sheet_label);
        assert!(entry.enabled);
        assert_eq!(
            click_at(&mut app, &ctx, tab, entry.center),
            vec![Action::ResetColumnWidths {
                columns: columns.clone()
            }]
        );
        assert_closed(&mut app, &ctx, tab);

        let (ctx, menu) = open_path(&mut app, tab, &columns_path);
        let entry = active(&menu, &column_label);
        assert!(entry.enabled);
        assert_eq!(
            click_at(&mut app, &ctx, tab, entry.center),
            vec![Action::ResetColumnWidths {
                columns: vec![columns[0]]
            }]
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn the_workbook_files_sheets_columns_and_metadata_without_changing_the_actions() {
        use stokesforge_core::{ColumnRole, ColumnType};
        let (session, mut app, _) = bench();
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
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-workbook"));
        assert!(find_entry(&menu, &tr!("action-rename-sheet")).is_none());
        let (ctx, menu) = open_path(&mut app, tab, &[tr!("menu-workbook"), tr!("menu-sheet")]);
        assert!(!active(&menu, &tr!("action-delete-sheet")).enabled);
        let pos = active(&menu, &tr!("action-duplicate-sheet")).center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::DuplicateSheet(sheet)]
        );
        assert_closed(&mut app, &ctx, tab);

        app.state.deselect_all(sheet);
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-workbook"));
        assert!(!active(&menu, &tr!("workbook-plot-menu")).enabled);
        let (ctx, menu) = open_path(&mut app, tab, &[tr!("menu-workbook"), tr!("menu-columns")]);
        assert!(active(&menu, &tr!("workbook-column-add")).enabled);
        for label in [
            tr!("column-type"),
            tr!("column-role"),
            tr!("workbook-plot-menu"),
        ] {
            assert!(!active(&menu, &label).enabled, "no selection: {label}");
        }
        let pos = active(&menu, &tr!("workbook-column-add")).center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::AddColumn { sheet }]
        );
        assert_closed(&mut app, &ctx, tab);

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
        let (ctx, menu) = open_path(
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
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::SetColumnsType {
                columns: columns.clone(),
                column_type: ColumnType::Mixed
            }]
        );
        assert_closed(&mut app, &ctx, tab);
        // X and Y are different: no role is shown as selected.
        let (ctx, menu) = open_path(
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
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::SetColumnsRole {
                columns: columns.clone(),
                role: ColumnRole::Z
            }]
        );
        assert_closed(&mut app, &ctx, tab);
        let (ctx, menu) = open_path(
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
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::PlotColumns {
                sheet,
                columns: columns.clone(),
                plot_type: stokesforge_plot::PlotType::Line
            }]
        );
        assert_closed(&mut app, &ctx, tab);

        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-workbook"));
        let label = tr!("menu-window-show-metadata-row");
        let checkbox = active(&menu, &label);
        let visible = checkbox.toggled.unwrap();
        assert!(click_at(&mut app, &ctx, tab, checkbox.center).is_empty());
        let menu = entries(&paint_bar(&mut app, &ctx, tab, Vec::new()).0);
        assert_eq!(active(&menu, &label).toggled, Some(!visible));
        session.shutdown().unwrap();
    }

    /// #319: `Move ▸` is in Workbook › Columns as in the right-click of the
    /// letters (a single painting, `column_type_role_menus`), with its four
    /// entries and the same greying; between Add Column and Type.
    #[test]
    fn the_columns_menu_carries_move_with_its_four_entries_and_the_same_greying() {
        let (session, mut app, _) = bench();
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
        assert!(columns.len() >= 3, "a column in the middle: {columns:?}");
        let tab = Some(Tab::Book(book));
        let path = [
            tr!("menu-workbook"),
            tr!("menu-columns"),
            tr!("column-move"),
        ];
        let four = [
            tr!("action-move-columns-first"),
            tr!("action-move-columns-left"),
            tr!("action-move-columns-right"),
            tr!("action-move-columns-last"),
        ];

        // A middle column: everything is possible, in the order of the menu.
        app.state.select_column(sheet, columns[1]);
        let (ctx, menu) = open_path(&mut app, tab, &path);
        let y = |label: &str| active(&menu, label).center.y;
        assert!(y(&tr!("workbook-column-add")) < y(&tr!("column-move")));
        assert!(y(&tr!("column-move")) < y(&tr!("column-type")));
        assert!(y(&tr!("column-type")) < y(&tr!("column-role")));
        for label in &four {
            assert!(active(&menu, label).enabled, "{label}: {menu:#?}");
        }
        let ys: Vec<f32> = four.iter().map(|label| y(label)).collect();
        assert!(ys.windows(2).all(|pair| pair[0] < pair[1]), "{ys:?}");
        let pos = active(&menu, &tr!("action-move-columns-right")).center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::MoveColumns {
                sheet,
                columns: vec![columns[1]],
                to: stokesforge_app_state::column_move::ColumnMove::Right
            }]
        );
        assert_closed(&mut app, &ctx, tab);

        // The first column: neither First nor Left (same rule as in the right-click).
        app.state.select_column(sheet, columns[0]);
        let (_, menu) = open_path(&mut app, tab, &path);
        let actives: Vec<bool> = four
            .iter()
            .map(|label| active(&menu, label).enabled)
            .collect();
        assert_eq!(actives, [false, false, true, true], "{menu:#?}");
        session.shutdown().unwrap();
    }

    #[test]
    fn show_data_follows_the_graph_references() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-graph"));
        let entry = active(&menu, &tr!("action-show-graph-data"));
        assert!(entry.enabled);
        assert_eq!(
            click_at(&mut app, &ctx, tab, entry.center),
            vec![Action::ShowGraphData { graph }]
        );
        app.state.dispatch(Action::AddGraph);
        let graph = app.state.graphs().last().unwrap().0;
        app.open_tab(Tab::Graph(graph));
        let (_, menu) = open_menu(&mut app, Some(Tab::Graph(graph)), &tr!("menu-graph"));
        assert!(!active(&menu, &tr!("action-show-graph-data")).enabled);
        session.shutdown().unwrap();
    }

    /// #176: the mirror axes are checked from the Graph menu, like the
    /// Object Manager checkbox (same `Action::SetPlotVisibility`), and the checkbox
    /// keeps the menu open.
    #[test]
    fn mirror_axes_are_checked_from_the_graph_menu() {
        use stokesforge_app_state::object_manager::{AxisRef, VisibilityTarget};
        let (session, mut app, graph) = bench();
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
            let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-graph"));
            let checkbox = active(&menu, &label);
            assert_eq!(checkbox.toggled, Some(false));
            assert_eq!(
                click_at(&mut app, &ctx, tab, checkbox.center),
                vec![Action::SetPlotVisibility {
                    graph,
                    target: VisibilityTarget::Axis(AxisRef { panel: 0, side }),
                    visible: true,
                }]
            );
            let opened = entries(&paint_bar(&mut app, &ctx, tab, Vec::new()).0);
            assert!(
                find_entry(&opened, &label).is_some(),
                "the checkbox keeps the menu open"
            );
        }
        session.shutdown().unwrap();
    }

    /// #149: "Copy Graph" is in the Graph menu, like
    /// the export, and pushes the figure framed by the view — the same path as
    /// Ctrl+C, with no selection condition (a menu click is explicit).
    #[test]
    fn copy_graph_is_in_the_graph_menu() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        // A selected series does not grey out the entry: Ctrl+C is then silent,
        // the menu is not.
        app.with_graph_view(graph, |view| {
            view.select_series(Some(stokesforge_plot::SeriesRef::new(0, 0)), &spec)
        });
        let (ctx, menu) = open_path(&mut app, tab, &[tr!("menu-graph"), tr!("menu-export")]);
        let entry = active(&menu, &tr!("action-copy-graph"));
        assert!(entry.enabled);
        let expected = app
            .read_graph_view(graph, |view| view.framed_spec(&spec))
            .unwrap();
        assert_eq!(
            click_at(&mut app, &ctx, tab, entry.center),
            vec![Action::CopyGraphFigure {
                spec: Box::new(expected)
            }]
        );
        assert_closed(&mut app, &ctx, tab);
        // Witness: with no active graph, the Graph menu does not exist.
        let book = app.state.books()[0].0;
        assert!(!titles(&mut app, Some(Tab::Book(book))).contains(&tr!("menu-graph")));
        session.shutdown().unwrap();
    }

    #[test]
    fn the_graph_menu_follows_the_active_graph_and_its_selection() {
        let (session, mut app, graph) = bench();
        let book = app.state.books()[0].0;
        let menus = titles(&mut app, Some(Tab::Book(book)));
        assert!(!menus.contains(&tr!("menu-graph")), "{menus:?}");

        let tab = Some(Tab::Graph(graph));
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-graph"));
        assert!(active(&menu, &tr!("action-add-text")).enabled, "witness");
        assert!(
            !active(&menu, &tr!("menu-selected-content")).enabled,
            "no selection"
        );
        let (_, export) = open_path(&mut app, tab, &[tr!("menu-graph"), tr!("menu-export")]);
        assert!(active(&export, &tr!("action-export-graph")).enabled);
        assert!(
            !active(&menu, &tr!("action-reset-text-positions")).enabled,
            "no text moved"
        );

        let target = AnnotationRef { panel: 0, index: 0 };
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_annotation(Some(target), &spec));
        let (ctx, menu) = open_path(
            &mut app,
            tab,
            &[tr!("menu-graph"), tr!("menu-selected-content")],
        );
        let pos = active(&menu, &tr!("action-delete-content")).center;
        assert!(active(&menu, &tr!("action-delete-content")).enabled);
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::RemovePlotObject {
                graph,
                target: stokesforge_app_state::object_manager::VisibilityTarget::Annotation(target),
            }]
        );
        session.shutdown().unwrap();
    }

    /// "Selection" reuses the Object Manager menu: greyed out
    /// without a selection, and its requests go through
    /// `run_object_manager_request`, like those of the tree.
    #[test]
    fn the_selection_of_the_graph_menu_goes_through_the_object_manager() {
        use stokesforge_app_state::object_manager::VisibilityTarget;
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let open_the_content = |app: &mut StokesForgeApp| {
            let (ctx, menu) = open_menu(app, tab, &tr!("menu-graph"));
            let pos = active(&menu, &tr!("menu-selected-content")).center;
            click_at(app, &ctx, tab, pos);
            let (output, _) = paint_bar(app, &ctx, tab, Vec::new());
            (ctx, entries(&output))
        };
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-graph"));
        assert!(
            !active(&menu, &tr!("menu-selected-content")).enabled,
            "with no selection, the entry is greyed out and opens no submenu"
        );

        let series = stokesforge_plot::SeriesRef::new(0, 0);
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_series(Some(series), &spec));
        let (ctx, menu) = open_the_content(&mut app);
        let properties = active(&menu, &tr!("action-properties"));
        assert!(properties.enabled);
        let pos = properties.center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::OpenObjectProperties {
                graph,
                target: VisibilityTarget::Series(series),
            }]
        );
        session.shutdown().unwrap();
    }

    #[test]
    fn the_view_menu_carries_only_the_panel_settings() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-view"));
        assert!(find_entry(&menu, &tr!("project-manager-title")).is_none());
        assert!(find_entry(&menu, &tr!("script-title")).is_none());
        assert!(
            find_entry(&menu, &tr!("action-object-manager")).is_some(),
            "only the Object Manager carries settings today (#214)"
        );
        for open in [false, true] {
            app.state.set_object_manager_open_for_test(open);
            let (_, menu) = open_path(
                &mut app,
                tab,
                &[tr!("menu-view"), tr!("action-object-manager")],
            );
            assert!(find_entry(&menu, &tr!("action-show-content")).is_none());
            for label in [
                tr!("object-manager-show-axes"),
                tr!("object-manager-show-objects"),
            ] {
                assert!(
                    active(&menu, &label).enabled,
                    "setting enabled, panel open or not"
                );
            }
        }
        session.shutdown().unwrap();
    }

    /// #162: "SVG Text" is an exclusive choice under Export; the current
    /// choice (Text, by default) does nothing more than a click on the other.
    #[test]
    fn svg_text_is_an_exclusive_choice_under_export() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = open_path(
            &mut app,
            tab,
            &[
                tr!("menu-graph"),
                tr!("menu-export"),
                tr!("menu-graph-svg-text"),
            ],
        );
        assert!(find_entry(&menu, &tr!("menu-graph-svg-text-text")).is_some());
        assert_eq!(
            click_at(
                &mut app,
                &ctx,
                tab,
                active(&menu, &tr!("menu-graph-svg-text-outlines")).center
            ),
            vec![Action::SetSvgTextMode(SvgTextMode::Outlines)]
        );
        assert_closed(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    #[test]
    fn png_resolution_is_an_exclusive_choice_that_closes_three_levels() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = open_path(
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
            click_at(&mut app, &ctx, tab, active(&menu, &label).center),
            vec![Action::SetPngExportScale(scale)]
        );
        assert_closed(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    #[test]
    fn the_graph_properties_open_the_right_tab_and_close_the_parents() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (_, menu) = open_menu(&mut app, tab, &tr!("menu-graph"));
        for page in [
            tr!("menu-graph-axes"),
            tr!("action-export-graph"),
            tr!("action-delete-content"),
        ] {
            assert!(
                find_entry(&menu, &page).is_none(),
                "the page {page} must be filed away"
            );
        }
        let (ctx, menu) = open_path(&mut app, tab, &[tr!("menu-graph"), tr!("menu-properties")]);
        let pos = active(&menu, &tr!("menu-graph-axes")).center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::OpenPlotSettings {
                graph,
                tab: stokesforge_app_state::state::PlotSettingsTab::Axes,
                axis: None
            }]
        );
        assert_closed(&mut app, &ctx, tab);
        session.shutdown().unwrap();
    }

    #[test]
    fn the_settings_submenus_emit_the_common_actions() {
        use stokesforge_app_state::preferences::ObjectManagerGroup;
        let (session, mut app, _) = bench();
        app.state.set_object_manager_open_for_test(true);
        // The settings stay accessible without an active graph.
        let open = |app: &mut StokesForgeApp, child: Option<&str>| {
            let mut path = vec![tr!("menu-view"), tr!("action-object-manager")];
            if let Some(label) = child {
                path.push(label.to_string());
            }
            open_path(app, None, &path)
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
            assert_eq!(click_at(&mut app, &ctx, None, row.center), vec![action]);
            let still_open = entries(&paint_bar(&mut app, &ctx, None, Vec::new()).0);
            assert!(
                find_entry(&still_open, &label).is_some(),
                "the toggle keeps the menu open"
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
                click_at(&mut app, &ctx, None, row.center),
                vec![Action::SetObjectManagerGroupOpen {
                    group,
                    open: !current
                }]
            );
            let still_open = entries(&paint_bar(&mut app, &ctx, None, Vec::new()).0);
            assert!(
                find_entry(&still_open, &label).is_some(),
                "the toggle keeps Expand by Default open"
            );
        }
        let (ctx, menu) = open(&mut app, None);
        assert_eq!(
            click_at(
                &mut app,
                &ctx,
                None,
                active(&menu, &tr!("object-manager-reset")).center
            ),
            vec![Action::ResetObjectManagerSettings]
        );
        assert_closed(&mut app, &ctx, None);
        session.shutdown().unwrap();
    }

    #[test]
    fn the_window_menu_follows_the_active_window_and_the_project_manager() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-window"));
        assert!(active(&menu, &tr!("action-detach-project-manager")).enabled);
        assert!(find_entry(&menu, &tr!("action-reattach-project-manager")).is_none());
        let maximize = active(&menu, &tr!("action-maximize-window"));
        assert!(maximize.enabled);
        let pos = maximize.center;
        assert!(click_at(&mut app, &ctx, tab, pos).is_empty());
        assert_eq!(
            app.maximized_tab,
            Some(Tab::Graph(graph)),
            "maximizes the foreground window"
        );

        let restore = tr!(
            "action-restore-window",
            title = tab_title(&app.state, Tab::Graph(graph))
        );
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-window"));
        assert!(find_entry(&menu, &tr!("action-maximize-window")).is_none());
        let pos = active(&menu, &restore).center;
        click_at(&mut app, &ctx, tab, pos);
        assert_eq!(app.maximized_tab, None);

        // No floating window: nothing to maximize.
        app.floating_tabs.clear();
        let (_, menu) = open_menu(&mut app, None, &tr!("menu-window"));
        assert!(!active(&menu, &tr!("action-maximize-window")).enabled);

        // Detached Project Manager: the entry becomes "reattach".
        app.pm_detached = true;
        let (_, menu) = open_menu(&mut app, None, &tr!("menu-window"));
        assert!(find_entry(&menu, &tr!("action-detach-project-manager")).is_none());
        assert!(active(&menu, &tr!("action-reattach-project-manager")).enabled);
        session.shutdown().unwrap();
    }

    /// Help › Keyboard Shortcuts (#231, S-K5): the entry exists, is enabled, and
    /// requests the opening of the shortcuts window.
    #[test]
    fn the_help_menu_opens_the_keyboard_shortcuts() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-help"));
        let entry = active(&menu, &tr!("action-keyboard-shortcuts"));
        let pos = entry.center;
        assert_eq!(
            click_at(&mut app, &ctx, tab, pos),
            vec![Action::OpenKeyboardShortcuts]
        );
        session.shutdown().unwrap();
    }

    /// Help › About StokesForge (#382): the entry exists and requests the window.
    #[test]
    fn the_help_menu_opens_the_about_window() {
        let (session, mut app, graph) = bench();
        let tab = Some(Tab::Graph(graph));
        let (ctx, menu) = open_menu(&mut app, tab, &tr!("menu-help"));
        let entry = active(&menu, &tr!("action-about"));
        let pos = entry.center;
        assert_eq!(click_at(&mut app, &ctx, tab, pos), vec![Action::OpenAbout]);
        session.shutdown().unwrap();
    }

    /// A frame of the status bar alone.
    fn status_bar_frame(app: &StokesForgeApp, ctx: &egui::Context) -> egui::FullOutput {
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

    /// The color of the painted text that contains `needle`, if it is painted.
    fn text_color_of(output: &egui::FullOutput, needle: &str) -> Option<egui::Color32> {
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

    /// "Ready" with no message; three levels (S-M4): information keeps the
    /// current color, the warning the warning tint, the error
    /// the error color; no revision nor import link (S-R15).
    #[test]
    fn the_status_bar_distinguishes_the_three_levels() {
        let (session, mut app, _) = bench();
        let ctx = context();
        let output = status_bar_frame(&app, &ctx);
        assert!(text_color_of(&output, &tr!("status-ready")).is_some());
        assert!(text_color_of(&output, "Revision").is_none());
        let visuals = &ctx.global_style().visuals;
        let (error, warning) = (visuals.error_fg_color, visuals.warn_fg_color);

        app.state.notice("Notice".into());
        let output = status_bar_frame(&app, &ctx);
        assert!(text_color_of(&output, &tr!("status-ready")).is_none());
        assert_ne!(text_color_of(&output, "Notice"), Some(error), "witness");
        assert_ne!(text_color_of(&output, "Notice"), Some(warning), "witness");

        app.state.warn("Warning".into());
        let output = status_bar_frame(&app, &ctx);
        assert_eq!(text_color_of(&output, "Warning"), Some(warning));

        // A refused gesture (the last sheet of a workbook); an unreadable
        // project, on the other hand, is a serious failure told by a dialog (S-M8, #229).
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        app.state.dispatch(Action::DeleteSheet(sheet));
        let status = app.state.status().expect("a refusal is reported");
        assert!(status.is_error());
        let text = status.text.clone();
        let output = status_bar_frame(&app, &ctx);
        assert_eq!(text_color_of(&output, &text), Some(error));
        session.shutdown().unwrap();
    }

    /// A click on the status bar requests the message history (S-M6).
    #[test]
    fn a_click_on_the_status_bar_opens_the_history() {
        let (session, mut app, _) = bench();
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

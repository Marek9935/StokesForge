//! Text and object format bars, and the contextual toolbar: what they show
//! ([`StokesForgeApp::format_target`], [`StokesForgeApp::text_format_state`],
//! [`StokesForgeApp::object_format_state`], [`StokesForgeApp::tools_context`])
//! and what they do ([`StokesForgeApp::run_toolbar_request`]).
//!
//! The buttons themselves live in `crate::toolbar`; the Typst markup of a
//! passage in `stokesforge_app_state::text_format`; here, the link between the
//! two and the selection of the graph view (floating or detached). The font,
//! and for a title or the legend the size and the color, are fields of the
//! zone ([`StokesForgeApp::format_zone`], #77/#78/#90), never a
//! `#text(font: …)` tag in the content.
//!
//! Moved out of `app.rs` as is (#122).

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_render_egui::{GraphView, TextTarget};

use crate::toolbar::ToolbarRequest;
use stokesforge_app_state::state::{active_graph, Action, ActiveTabKind};

use super::graph_text_edit::{
    annotation_editor_id, pending_format_id, pending_zone_font_id, text_editor_id,
    AnnotationEditor, TextEditTarget, TextEditor,
};
use super::tab::active_tab_kind;
use super::tab::Tab;
use super::StokesForgeApp;

impl StokesForgeApp {
    /// What the contextual bar shows for the foreground window.
    pub(super) fn tools_context(&self, active_tab: Option<Tab>) -> crate::toolbar::ToolsContext {
        use crate::toolbar::ToolsContext;
        match active_tab_kind(active_tab) {
            ActiveTabKind::Graph(graph) => ToolsContext::Graph {
                graph,
                tool: self
                    .read_graph_view(graph, GraphView::tool)
                    .unwrap_or_default(),
            },
            ActiveTabKind::Book(book) => {
                let sheet = self.state.selected_sheet_in(book);
                let columns = sheet
                    .and_then(|sheet| {
                        self.state
                            .selection(sheet)
                            .map(|selection| selection.columns(self.state.tree(), sheet))
                    })
                    .unwrap_or_default();
                ToolsContext::Book {
                    book,
                    sheet,
                    columns,
                }
            }
            _ => ToolsContext::None,
        }
    }

    /// What the format bar acts on: a field being edited in place, otherwise a
    /// selected free text, otherwise nothing (issue #63).
    pub(super) fn format_target(
        &self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) -> crate::toolbar::FormatTarget {
        use crate::toolbar::FormatTarget;
        let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
            return FormatTarget::None;
        };
        self.read_graph_view(graph, |view| {
            let editing = ctx.data(|data| {
                data.get_temp::<TextEditor>(text_editor_id(view)).is_some()
                    || data
                        .get_temp::<AnnotationEditor>(annotation_editor_id(view))
                        .is_some()
            });
            if editing {
                FormatTarget::Field
            } else if view.selected_annotation().is_some() {
                FormatTarget::Block
            } else if view.selected_text().is_some() {
                FormatTarget::Zone
            } else {
                FormatTarget::None
            }
        })
        .unwrap_or(FormatTarget::None)
    }

    /// Reads the settings of the targeted zone and the defaults of the shared
    /// pipeline. The draft being edited takes precedence over the model, in a
    /// detached window too.
    pub(super) fn text_format_state(
        &self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) -> crate::toolbar::TextFormatState {
        use crate::toolbar::TextFormatState;
        let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
            return TextFormatState::default();
        };
        let Some(spec) = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
        else {
            return TextFormatState::default();
        };
        let state = self
            .read_graph_view(graph, |view| {
                let annotation_editor =
                    ctx.data(|data| data.get_temp::<AnnotationEditor>(annotation_editor_id(view)));
                let annotation = annotation_editor
                    .as_ref()
                    .map(|editor| &editor.draft)
                    .or_else(|| {
                        view.selected_annotation()
                            .and_then(|target| target.get(&spec))
                    });
                if let Some(annotation) = annotation {
                    return Some(
                        TextFormatState {
                            source: Some(egui::Id::new((
                                graph,
                                annotation_editor
                                    .as_ref()
                                    .map(|e| e.target)
                                    .or_else(|| view.selected_annotation()),
                                annotation_editor.is_some(),
                            ))),
                            family: Some(if annotation.font.trim().is_empty() {
                                stokesforge_render_egui::FontSet::default()
                                    .sans_family_name()
                                    .into()
                            } else {
                                annotation.font.trim().into()
                            }),
                            size: Some(annotation.effective_size()),
                            color: Some(annotation.color.clone()),
                            auto_family: annotation.font.trim().is_empty(),
                            auto_size: annotation.size
                                == stokesforge_plot::AnnotationSpec::DEFAULT_SIZE,
                            align: Some(annotation.align),
                        }
                        .with_markup(&annotation.text),
                    );
                }
                let editor = ctx.data(|data| data.get_temp::<TextEditor>(text_editor_id(view)));
                let target = editor
                    .as_ref()
                    .map(|editor| match editor.target {
                        TextEditTarget::Title(target) => Some(target),
                        TextEditTarget::LegendEntry(address) => Some(TextTarget::Legend {
                            panel: address.panel,
                        }),
                        // A group name has no font of its own.
                        TextEditTarget::GroupName(_) => None,
                    })
                    .unwrap_or_else(|| view.selected_text())?;
                let font = target.font(&spec)?;
                let default_size = match target {
                    TextTarget::Title => stokesforge_render_egui::TITLE_SIZE,
                    TextTarget::PanelTitle { .. } => stokesforge_render_egui::PANEL_TITLE_SIZE,
                    TextTarget::AxisTitle { .. } => stokesforge_render_egui::AXIS_DESC_SIZE,
                    TextTarget::Legend { .. } => stokesforge_render_egui::LABEL_SIZE,
                };
                let text = editor
                    .as_ref()
                    .map(|editor| editor.buffer.clone())
                    .or_else(|| target.text(&spec))
                    .unwrap_or_default();
                Some(
                    TextFormatState {
                        source: Some(egui::Id::new((
                            graph,
                            target,
                            editor.as_ref().map(|editor| editor.target),
                        ))),
                        family: Some(
                            font.family()
                                .unwrap_or(
                                    stokesforge_render_egui::FontSet::default().sans_family_name(),
                                )
                                .into(),
                        ),
                        size: Some(font.effective_size(default_size)),
                        color: target.color(&spec),
                        auto_family: font.family().is_none(),
                        auto_size: font.size.is_none(),
                        align: Some(font.align),
                    }
                    .with_markup(&text),
                )
            })
            .flatten();
        state.unwrap_or_default()
    }

    /// The selected graphic object of the foreground graph, with its address
    /// (issue #64).
    fn selected_object(
        &self,
        active_tab: Option<Tab>,
    ) -> Option<(
        NodeId,
        stokesforge_render_egui::ShapeRef,
        stokesforge_render_egui::ShapeSpec,
    )> {
        let graph = active_graph(active_tab_kind(active_tab))?;
        let target = self
            .read_graph_view(graph, GraphView::selected_shape)
            .flatten()?;
        let shape = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
            .and_then(|spec| target.get(&spec).cloned())?;
        Some((graph, target, shape))
    }

    /// What the object format bar shows.
    pub(super) fn object_format_state(
        &self,
        active_tab: Option<Tab>,
    ) -> Option<crate::toolbar::ObjectFormatState> {
        let graph = active_graph(active_tab_kind(active_tab))?;
        if let Some(target) = self.selected_series(graph) {
            let spec = stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)?;
            let series = spec.series_at(target)?;
            return Some(crate::toolbar::ObjectFormatState {
                closed: false,
                line_color: series.line.color.clone(),
                fill_color: String::new(),
                line_width: series.line.width,
            });
        }
        let (_, _, shape) = self.selected_object(active_tab)?;
        Some(crate::toolbar::ObjectFormatState {
            closed: shape.kind.is_closed(),
            line_color: shape.line_color,
            fill_color: shape.fill_color,
            line_width: shape.line_width,
        })
    }

    /// Sets the selected object: a single undoable action, like a move
    /// (`ObjectFormat::apply` says whether something changed).
    fn format_object(
        &mut self,
        format: &stokesforge_app_state::object_format::ObjectFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
            if let Some(target) = self.selected_series(graph) {
                if let Some(mut spec) =
                    stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                {
                    if spec
                        .series_at_mut(target)
                        .is_some_and(|series| format.apply_series(series))
                    {
                        actions.push(Action::SetPlotSpec { graph, spec });
                    }
                }
                return;
            }
        }
        let Some((graph, target, mut shape)) = self.selected_object(active_tab) else {
            return;
        };
        if format.apply(&mut shape) {
            actions.push(Action::SetShape {
                graph,
                target,
                shape: Box::new(shape),
            });
        }
    }

    /// Sets the font (or size) of the targeted text **zone**, without touching
    /// its content. Returns `true` if the request was handled; `false` lets
    /// [`StokesForgeApp::format_text`] place tags (the size of a passage of the
    /// field being edited, the color, bold…).
    ///
    /// - a free text being edited: the editor's draft
    ///   (`apply_pending_zone_font` (`graph_text_edit.rs`));
    /// - a title or a legend name being edited: the title, or the legend,
    ///   directly;
    /// - a selected title, axis title or legend: same, font and size;
    /// - a selected free text: its font;
    /// - the color of a selected title, axis title or legend: that of the
    ///   whole zone (#90); while editing, a passage (tag).
    fn format_zone(
        &mut self,
        ctx: &egui::Context,
        format: &stokesforge_app_state::text_format::TextFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) -> bool {
        use stokesforge_app_state::text_format::TextFormat;
        let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
            return false;
        };
        let Some((annotation_editing, title_editing, selected_text, selected_annotation)) = self
            .read_graph_view(graph, |view| {
                (
                    ctx.data(|data| {
                        data.get_temp::<AnnotationEditor>(annotation_editor_id(view))
                            .is_some()
                    }),
                    ctx.data(|data| data.get_temp::<TextEditor>(text_editor_id(view))),
                    view.selected_text(),
                    view.selected_annotation(),
                )
            })
        else {
            return false;
        };
        // Which title zone? (`None`: no title targeted)
        let zone = match (&title_editing, selected_text) {
            (Some(editor), _) => Some(match editor.target {
                TextEditTarget::Title(target) => target,
                TextEditTarget::LegendEntry(address) => {
                    stokesforge_render_egui::TextTarget::Legend {
                        panel: address.panel,
                    }
                }
                TextEditTarget::GroupName(_) => return false,
            }),
            (None, selected) if !annotation_editing => selected,
            _ => None,
        };
        if matches!(format, TextFormat::AutoSize) {
            if annotation_editing {
                self.read_graph_view(graph, |view| {
                    let id = annotation_editor_id(view);
                    ctx.data_mut(|data| {
                        if let Some(mut editor) = data.get_temp::<AnnotationEditor>(id) {
                            editor.draft.size = stokesforge_plot::AnnotationSpec::DEFAULT_SIZE;
                            data.insert_temp(id, editor);
                        }
                    });
                });
                return true;
            }
            if let Some(target) = zone {
                return self.set_zone_font(graph, target, actions, |font| font.size = None);
            }
        }
        // Alignment (#244) is a field of the box: the draft of a free text
        // being edited, otherwise the zone of a title (edited or selected),
        // otherwise the selected free text.
        if let TextFormat::Align(align) = *format {
            if annotation_editing {
                self.read_graph_view(graph, |view| {
                    let id = annotation_editor_id(view);
                    ctx.data_mut(|data| {
                        if let Some(mut editor) = data.get_temp::<AnnotationEditor>(id) {
                            editor.draft.align = align;
                            data.insert_temp(id, editor);
                        }
                    });
                });
                return true;
            }
            if let Some(target) = zone {
                return self.set_zone_font(graph, target, actions, |font| font.align = align);
            }
            if let Some(target) = selected_annotation {
                let Some(mut annotation) =
                    stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                        .and_then(|spec| target.get(&spec).cloned())
                else {
                    return true;
                };
                annotation.align = align;
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
                return true;
            }
            return false;
        }
        if let TextFormat::Font(name) = format {
            let name = name.trim().to_owned();
            if annotation_editing {
                ctx.data_mut(|data| data.insert_temp(pending_zone_font_id(), Some(name)));
                return true;
            }
            if let Some(target) = zone {
                return self.set_zone_font(graph, target, actions, |font| font.family = name);
            }
            if let Some(target) = selected_annotation {
                let Some(mut annotation) =
                    stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                        .and_then(|spec| target.get(&spec).cloned())
                else {
                    return true;
                };
                annotation.font = name;
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
                return true;
            }
            return false;
        }
        // Bold and italic of a selected title outside editing (#243): the tags
        // wrap the whole text, as for a free text; a text already fully
        // wrapped is unwrapped. The legend has no text of its own
        // (`TextTarget::text` returns `None`): nothing to wrap.
        if let (TextFormat::Bold | TextFormat::Italic, Some(target), None, false) =
            (format, zone, &title_editing, annotation_editing)
        {
            if let Some(text) =
                stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                    .and_then(|spec| target.text(&spec))
            {
                let length = text.chars().count();
                actions.push(Action::SetTextContent {
                    graph,
                    target,
                    text: stokesforge_app_state::text_format::apply(&text, 0..length, format).text,
                });
            }
            return true;
        }
        // The color: that of the zone of a selected title; in a field being
        // edited it stays a tag on the highlighted passage (#90).
        if let (TextFormat::Color(color), Some(target), None, false) =
            (format, zone, &title_editing, annotation_editing)
        {
            actions.push(Action::SetTextColor {
                graph,
                target,
                color: color.clone(),
            });
            return true;
        }
        // The size: that of the zone of a selected title; in a field being
        // edited it stays a tag on the highlighted passage.
        if let (TextFormat::Size(size), Some(target), None, false) =
            (format, zone, &title_editing, annotation_editing)
        {
            if size.is_finite() && *size > 0.0 {
                let size = *size;
                return self.set_zone_font(graph, target, actions, |font| font.size = Some(size));
            }
        }
        false
    }

    /// Sets the font or size of the title zone `target`: reads the current
    /// font from the model, changes it with `edit`, and pushes **one**
    /// undoable action. Always `true` (the request is handled even if the
    /// graph no longer has this text).
    fn set_zone_font(
        &self,
        graph: NodeId,
        target: stokesforge_render_egui::TextTarget,
        actions: &mut Vec<Action>,
        edit: impl FnOnce(&mut stokesforge_plot::TextFont),
    ) -> bool {
        if let Some(mut font) =
            stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                .and_then(|spec| target.font(&spec))
        {
            edit(&mut font);
            actions.push(Action::SetTextFont {
                graph,
                target,
                font,
            });
        }
        true
    }

    /// Formats the targeted text: in the field being edited (the field places
    /// the tags at its next paint, see `apply_pending_format`
    /// (`graph_text_edit.rs`)), or on the whole selected free text — a single
    /// undoable action.
    fn format_text(
        &mut self,
        ctx: &egui::Context,
        format: stokesforge_app_state::text_format::TextFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        use stokesforge_app_state::text_format::{apply, TextFormat};
        // The font is an attribute of the zone, never a tag (issue #78); the
        // size too for a title or the legend (issue #77).
        if self.format_zone(ctx, &format, active_tab, actions) {
            return;
        }
        match self.format_target(ctx, active_tab) {
            crate::toolbar::FormatTarget::None | crate::toolbar::FormatTarget::Zone => {}
            crate::toolbar::FormatTarget::Field => {
                ctx.data_mut(|data| data.insert_temp(pending_format_id(), Some(format)));
            }
            crate::toolbar::FormatTarget::Block => {
                let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
                    return;
                };
                let Some(target) = self
                    .read_graph_view(graph, GraphView::selected_annotation)
                    .flatten()
                else {
                    return;
                };
                let Some(mut annotation) =
                    stokesforge_core::compose::current_plot_spec(self.state.tree(), graph)
                        .and_then(|spec| target.get(&spec).cloned())
                else {
                    return;
                };
                // Color and size of a free text are model fields.
                match &format {
                    TextFormat::Color(color) => annotation.color = color.clone(),
                    TextFormat::Size(size) => annotation.size = *size,
                    TextFormat::AutoSize => {
                        annotation.size = stokesforge_plot::AnnotationSpec::DEFAULT_SIZE
                    }
                    _ => {
                        let length = annotation.text.chars().count();
                        annotation.text = apply(&annotation.text, 0..length, &format).text;
                    }
                }
                actions.push(Action::SetAnnotation {
                    graph,
                    target,
                    annotation: Box::new(annotation),
                });
            }
        }
    }

    /// Runs what a toolbar button asked for: the same paths as the matching
    /// menu entries, never a second write.
    pub(super) fn run_toolbar_request(
        &mut self,
        ctx: &egui::Context,
        request: ToolbarRequest,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        match request {
            ToolbarRequest::Action(action) => actions.push(*action),
            ToolbarRequest::SetGraphTool(tool) => {
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    self.with_graph_view(graph, |view| view.set_tool(tool));
                }
            }
            ToolbarRequest::ResetZoom => {
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    self.with_graph_view(graph, GraphView::reset_view);
                }
            }
            ToolbarRequest::FormatText(format) => {
                self.format_text(ctx, format, active_tab, actions);
            }
            ToolbarRequest::FormatObject(format) => {
                self.format_object(&format, active_tab, actions);
            }
            ToolbarRequest::Import => self.open_import_dialog(),
            ToolbarRequest::Open => self.choose_open_path(actions),
            ToolbarRequest::SaveAs => self.choose_save_path(actions),
            ToolbarRequest::ExportGraph => {
                if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
                    self.request_graph_export(graph, actions);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::graph_text_edit::open_text_editor;
    use super::super::graph_window::graph_drop_target;
    use super::super::graph_window::tests::plotted_graph;
    use super::super::object_manager_ui::{selected_manager_target, ObjectManagerRequest};
    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::*;
    use crate::theme;
    use crate::toolbar::{BarEnv, FormatTarget, ToolbarId, ToolsContext};
    use stokesforge_app_state::object_format::ObjectFormat;
    use stokesforge_app_state::state::AppState;
    use stokesforge_app_state::text_format::{apply, TextFormat};
    use stokesforge_core::Project;
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, AxisSide, PanelSpec, PlotSpec, SeriesRef, SeriesSpec,
        ShapeKind, ShapeRef, ShapeSpec, TextFont,
    };
    use stokesforge_render_egui::GraphTool;
    use stokesforge_session::Session;

    /// The free text of the graph from [`setup`].
    const FREE_TEXT: AnnotationRef = AnnotationRef { panel: 0, index: 0 };

    /// The actions of a format request, without sending them.
    fn request_format(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        graph: NodeId,
        format: TextFormat,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        app.format_text(ctx, format, Some(Tab::Graph(graph)), &mut actions);
        actions
    }

    fn pending_format(ctx: &egui::Context) -> Option<TextFormat> {
        ctx.data(|data| data.get_temp::<Option<TextFormat>>(pending_format_id()))
            .flatten()
    }

    fn pending_font(ctx: &egui::Context) -> Option<String> {
        ctx.data(|data| data.get_temp::<Option<String>>(pending_zone_font_id()))
            .flatten()
    }

    /// Opens the title's in-place editor, like a double-click: the graph is
    /// painted first, so the title has a box on screen.
    fn edit_the_title(app: &mut StokesForgeApp, ctx: &egui::Context, graph: NodeId) {
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        graph_drop_target(
                            ui,
                            &app.state,
                            &mut Vec::new(),
                            &mut app.graph_views,
                            graph,
                        );
                    });
                },
            );
            output.textures_delta.clear();
        }
        let tree = app.state.tree().clone();
        app.with_graph_view(graph, |view| {
            open_text_editor(ctx, view, &tree, graph, TextTarget::Title)
        });
        assert_eq!(
            app.format_target(ctx, Some(Tab::Graph(graph))),
            FormatTarget::Field,
            "the title editor must be open"
        );
    }

    /// #97: the bar's fields show the format of the targeted zone.
    #[test]
    fn the_bar_shows_the_font_and_size_of_the_selected_title() {
        use eframe::App as _;
        let (session, graph) = plotted_graph();
        let mut state = AppState::new(session.handle(), String::new()).unwrap();
        state.dispatch(Action::SetTextFont {
            graph,
            target: TextTarget::Title,
            font: stokesforge_plot::TextFont {
                family: "Fira Sans".into(),
                size: Some(28.0),
                ..Default::default()
            },
        });
        let mut app = StokesForgeApp::new(state);
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.open_tab(Tab::Graph(graph));
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let mut output = egui::FullOutput::default();
        for _ in 0..3 {
            output.textures_delta.clear();
            output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
        }
        output.textures_delta.clear();
        find_text_center(&output.shapes, "Fira Sans");
        find_text_center(&output.shapes, "28");
        session.shutdown().unwrap();
    }

    /// #99: the same choice in the picker acts on two successive zones.
    #[test]
    fn the_same_color_applies_to_two_successive_zones_through_the_bar() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        egui_extras::install_image_loaders(&ctx);
        let paint = |app: &mut StokesForgeApp, ctx: &egui::Context, events| {
            let env = BarEnv {
                text_format: app.format_target(ctx, Some(Tab::Graph(graph))),
                text_style: app.text_format_state(ctx, Some(Tab::Graph(graph))),
                ..Default::default()
            };
            let mut requests = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        crate::toolbar::bar_contents(ui, ToolbarId::Text, &env, &mut requests)
                    });
                },
            );
            output.textures_delta.clear();
            let mut actions = Vec::new();
            for request in requests {
                app.run_toolbar_request(ctx, request, Some(Tab::Graph(graph)), &mut actions);
            }
            for action in actions {
                app.state.dispatch(action);
            }
            output
        };
        let swatch = |output: &egui::FullOutput, palette: bool| {
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Rect(rect)
                        if if palette {
                            rect.fill == egui::Color32::from_rgb(0x8b, 0x1a, 0x1a)
                                && rect.rect.width() > 12.0
                        } else {
                            (rect.rect.width() - 12.0).abs() < 0.01 && rect.fill.a() > 0
                        } =>
                    {
                        Some(rect.rect.center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("swatch painted (palette={palette})"))
        };
        for target in [
            TextTarget::Title,
            TextTarget::AxisTitle {
                panel: 0,
                side: stokesforge_plot::AxisSide::Bottom,
            },
        ] {
            let spec =
                stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
            app.with_graph_view(graph, |view| view.select_text(Some(target), &spec));
            paint(&mut app, &ctx, Vec::new());
            paint(&mut app, &ctx, Vec::new());
            let output = paint(&mut app, &ctx, Vec::new());
            let button = swatch(&output, false);
            click_at(&ctx, button, egui::Modifiers::NONE, |ctx, events| {
                paint(&mut app, ctx, events)
            });
            paint(&mut app, &ctx, Vec::new());
            let output = paint(&mut app, &ctx, Vec::new());
            let red = swatch(&output, true);
            click_at(&ctx, red, egui::Modifiers::NONE, |ctx, events| {
                paint(&mut app, ctx, events)
            });
            let spec =
                stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
            assert_eq!(
                target.color(&spec).as_deref(),
                Some("#8b1a1a"),
                "the zone {target:?} gets the red"
            );
        }
        session.shutdown().unwrap();
    }

    fn setup() -> (Session, StokesForgeApp, NodeId, egui::Context) {
        let mut project = Project::new("Formats").unwrap();
        let mut spec = PlotSpec::empty("Title");
        spec.panels
            .get_mut(0)
            .unwrap()
            .annotations
            .push(AnnotationSpec::new("Text", 0.0, 0.0));
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (
            session,
            StokesForgeApp::new(state),
            graph,
            egui::Context::default(),
        )
    }

    #[test]
    fn inherited_and_auto_sizes_follow_the_floating_and_detached_selection() {
        for detached in [false, true] {
            let (session, mut app, graph, ctx) = setup();
            if detached {
                app.detach_tab(Tab::Graph(graph));
            }
            for (target, size) in [
                (TextTarget::Title, stokesforge_render_egui::TITLE_SIZE),
                (
                    TextTarget::PanelTitle { panel: 0 },
                    stokesforge_render_egui::PANEL_TITLE_SIZE,
                ),
                (
                    TextTarget::AxisTitle {
                        panel: 0,
                        side: AxisSide::Bottom,
                    },
                    stokesforge_render_egui::AXIS_DESC_SIZE,
                ),
                (
                    TextTarget::Legend { panel: 0 },
                    stokesforge_render_egui::LABEL_SIZE,
                ),
            ] {
                let spec =
                    stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
                app.with_graph_view(graph, |view| view.select_text(Some(target), &spec));
                let inherited = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
                assert_eq!(inherited.size, Some(size));
                assert_eq!(
                    inherited.family.as_deref(),
                    Some(stokesforge_render_egui::FontSet::default().sans_family_name())
                );
                assert!(inherited.auto_size && inherited.auto_family);
                app.state.dispatch(Action::SetTextFont {
                    graph,
                    target,
                    font: TextFont {
                        family: "Fira Sans".into(),
                        size: Some(28.0),
                        ..Default::default()
                    },
                });
                let explicit = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
                assert_eq!(explicit.size, Some(28.0));
                assert_eq!(explicit.family.as_deref(), Some("Fira Sans"));
                assert!(!explicit.auto_size && !explicit.auto_family);
                let mut actions = Vec::new();
                app.format_text(
                    &ctx,
                    TextFormat::AutoSize,
                    Some(Tab::Graph(graph)),
                    &mut actions,
                );
                assert_eq!(actions.len(), 1);
                app.state.dispatch(actions.pop().unwrap());
                let reset = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
                assert_eq!(reset.size, Some(size));
                assert!(reset.auto_size);
                assert_eq!(
                    reset.family, explicit.family,
                    "Auto does not change the font"
                );
                app.state.dispatch(Action::Undo);
                assert_eq!(
                    app.text_format_state(&ctx, Some(Tab::Graph(graph))),
                    explicit
                );
                app.state.dispatch(Action::Undo);
            }
            session.shutdown().unwrap();
        }
    }

    #[test]
    fn the_bar_reads_the_draft_and_auto_restores_the_size_of_the_free_text() {
        let (session, mut app, graph, ctx) = setup();
        let target = AnnotationRef { panel: 0, index: 0 };
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        let mut draft = target.get(&spec).unwrap().clone();
        draft.font = "Libertinus Sans".into();
        draft.size = 36.0;
        let id = app
            .with_graph_view(graph, |view| {
                view.select_annotation(Some(target), &spec);
                annotation_editor_id(view)
            })
            .unwrap();
        ctx.data_mut(|data| {
            data.insert_temp(
                id,
                AnnotationEditor {
                    target,
                    draft,
                    original: target.get(&spec).cloned(),
                    focus: false,
                    anchor: None,
                },
            )
        });
        let style = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
        assert_eq!(style.family.as_deref(), Some("Libertinus Sans"));
        assert_eq!(style.size, Some(36.0));
        assert_eq!(style.color.as_deref(), Some(""));
        let mut actions = Vec::new();
        app.format_text(
            &ctx,
            TextFormat::AutoSize,
            Some(Tab::Graph(graph)),
            &mut actions,
        );
        assert!(
            actions.is_empty(),
            "only the draft changes before validation"
        );
        assert_eq!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph))).size,
            Some(AnnotationSpec::DEFAULT_SIZE)
        );
        ctx.data_mut(|data| data.remove::<AnnotationEditor>(id));
        assert!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph)))
                .auto_family
        );
        session.shutdown().unwrap();
    }

    /// The bar's alignment buttons (#244) set `align`: on the selected free
    /// text (box field), on a selected title (field of its zone), each in a
    /// single action.
    #[test]
    fn alignment_is_set_on_the_box_or_the_zone() {
        use stokesforge_plot::TextAlign;
        let (session, mut app, graph, ctx) = setup();
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_annotation(Some(FREE_TEXT), &spec));
        match request_format(&mut app, &ctx, graph, TextFormat::Align(TextAlign::Right)).as_slice()
        {
            [Action::SetAnnotation { annotation, .. }] => {
                assert_eq!(annotation.align, TextAlign::Right);
                assert_eq!(annotation.text, "Text", "no tag");
            }
            other => panic!("one SetAnnotation expected, not {other:?}"),
        }
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        match request_format(&mut app, &ctx, graph, TextFormat::Align(TextAlign::Left)).as_slice() {
            [Action::SetTextFont { font, .. }] => assert_eq!(font.align, TextAlign::Left),
            other => panic!("one SetTextFont expected, not {other:?}"),
        }
        // The button of the current alignment stays pressed: the state reads it.
        assert_eq!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph))).align,
            Some(TextAlign::Center)
        );
        app.with_graph_view(graph, |view| view.select_annotation(Some(FREE_TEXT), &spec));
        assert_eq!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph))).align,
            Some(TextAlign::Center)
        );
        session.shutdown().unwrap();
    }

    /// A selected title outside editing (#243): B / I wrap all its text in a
    /// single action; a half-wrapped box is wrapped entirely, a fully wrapped
    /// box is unwrapped.
    #[test]
    fn bold_and_italic_wrap_a_whole_selected_title() {
        let (session, mut app, graph, ctx) = setup();
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let text_of = |actions: Vec<Action>| match actions.as_slice() {
            [Action::SetTextContent { text, .. }] => text.clone(),
            other => panic!("a single SetTextContent expected, not {other:?}"),
        };
        assert_eq!(
            text_of(request_format(&mut app, &ctx, graph, TextFormat::Bold)),
            "#strong[Title]"
        );
        assert_eq!(
            text_of(request_format(&mut app, &ctx, graph, TextFormat::Italic)),
            "#emph[Title]"
        );
        let mut set_title_text = |text: &str| {
            app.state.dispatch(Action::SetTextContent {
                graph,
                target: TextTarget::Title,
                text: text.into(),
            });
            text_of(request_format(&mut app, &ctx, graph, TextFormat::Bold))
        };
        assert_eq!(set_title_text("#strong[a] b"), "#strong[#strong[a] b]");
        assert_eq!(set_title_text("#strong[Title]"), "Title");
        session.shutdown().unwrap();
    }

    #[test]
    fn local_formats_show_a_neutral_state() {
        let (session, mut app, graph, ctx) = setup();
        app.state.dispatch(Action::SetTextContent {
            graph,
            target: TextTarget::Title,
            text: "Title #text(size: 28pt, font: \"Fira Sans\", fill: red)[local]".into(),
        });
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let style = app.text_format_state(&ctx, Some(Tab::Graph(graph)));
        assert!(style.family.is_none() && style.size.is_none());
        assert!(style.color.is_none());
        assert!(!style.auto_family && !style.auto_size);
        session.shutdown().unwrap();
    }

    #[test]
    fn the_selection_and_the_format_reach_the_same_detached_view() {
        let mut project = Project::new("Objects").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        panel.series.push(SeriesSpec::new(NodeId::from_raw(100)));
        let spec = PlotSpec::empty("Objects").with_panels(panel);
        let graph = project.add_graph(root, None, spec.clone()).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let target = SeriesRef::new(0, 0);
        app.with_graph_view(graph, |view| view.select_series(Some(target), &spec));
        app.detach_tab(Tab::Graph(graph));
        assert!(!app.graph_views.contains_key(&graph));
        assert_eq!(app.selected_series(graph), Some(target));
        let ctx = egui::Context::default();
        app.detached_tabs[&Tab::Graph(graph)]
            .view_model
            .lock()
            .unwrap()
            .focused = true;
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: false,
                ..Default::default()
            },
            |_| {
                assert_eq!(app.active_content_tab(&ctx), Some(Tab::Graph(graph)));
            },
        );
        output.textures_delta.clear();
        // Going back into the panel keeps the graph's native target.
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            assert_eq!(app.active_content_tab(&ctx), Some(Tab::Graph(graph)));
        });
        output.textures_delta.clear();
        let mut actions = Vec::new();
        app.format_object(
            &stokesforge_app_state::object_format::ObjectFormat::LineWidth(4.0),
            Some(Tab::Graph(graph)),
            &mut actions,
        );
        assert_eq!(actions.len(), 1);
        app.state.dispatch(actions.pop().unwrap());
        assert_eq!(
            app.object_format_state(Some(Tab::Graph(graph)))
                .unwrap()
                .line_width,
            4.0
        );
        app.state.dispatch(Action::Undo);
        assert_eq!(
            stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap(),
            spec
        );
        app.reattach_tab(Tab::Graph(graph));
        assert_eq!(app.selected_series(graph), Some(target));
        assert_eq!(app.graph_views[&graph].selected_series(), Some(target));
    }

    #[test]
    fn hidden_objects_and_the_format_follow_the_detached_view() {
        use stokesforge_app_state::object_manager::VisibilityTarget as T;
        use stokesforge_plot::{
            AnnotationRef, AnnotationSpec, ShapeKind, ShapeRef, ShapeSpec, TextTarget,
        };
        let mut project = Project::new("Objects").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Title");
        spec.title_visible = false;
        let panel = spec.panels.get_mut(0).unwrap();
        panel
            .annotations
            .push(AnnotationSpec::new("Text", 0.0, 0.0));
        panel.annotations[0].visible = false;
        panel
            .shapes
            .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 1.0));
        panel.shapes[0].visible = false;
        let graph = project.add_graph(root, None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.detach_tab(Tab::Graph(graph));
        let ctx = egui::Context::default();
        let text = AnnotationRef { panel: 0, index: 0 };
        let shape = ShapeRef { panel: 0, index: 0 };
        let mut actions = Vec::new();
        for target in [
            T::Annotation(text),
            T::Shape(shape),
            T::Text(TextTarget::Title),
        ] {
            app.run_object_manager_request(
                &ctx,
                graph,
                ObjectManagerRequest::Select(target),
                &mut actions,
            );
            assert_eq!(
                app.read_graph_view(graph, selected_manager_target)
                    .flatten(),
                Some(target)
            );
            assert!(
                !app.graph_views.contains_key(&graph),
                "no second view created"
            );
        }
        assert_eq!(
            app.format_target(&ctx, Some(Tab::Graph(graph))),
            crate::toolbar::FormatTarget::Zone
        );
        app.format_text(
            &ctx,
            stokesforge_app_state::text_format::TextFormat::Size(28.0),
            Some(Tab::Graph(graph)),
            &mut actions,
        );
        assert!(matches!(
            actions.pop(),
            Some(Action::SetTextFont {
                target: TextTarget::Title,
                ..
            })
        ));
        app.run_object_manager_request(
            &ctx,
            graph,
            ObjectManagerRequest::Select(T::Shape(shape)),
            &mut actions,
        );
        assert!(app.object_format_state(Some(Tab::Graph(graph))).is_some());
        app.run_object_manager_request(
            &ctx,
            graph,
            ObjectManagerRequest::Select(T::Annotation(text)),
            &mut actions,
        );
        assert_eq!(
            app.format_target(&ctx, Some(Tab::Graph(graph))),
            crate::toolbar::FormatTarget::Block
        );
        app.reattach_tab(Tab::Graph(graph));
        assert_eq!(
            selected_manager_target(&app.graph_views[&graph]),
            Some(T::Annotation(text))
        );
        assert!(actions.is_empty(), "selection sends no command");
    }

    /// A selected free text (no editor open): bold and italic are tags placed
    /// on **all** its text by `text_format::apply`, color and size are model
    /// fields; each request is a single undoable action.
    #[test]
    fn bold_italic_color_and_size_of_a_selected_free_text() {
        let (session, mut app, graph, ctx) = setup();
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_annotation(Some(FREE_TEXT), &spec));
        assert_eq!(
            app.format_target(&ctx, Some(Tab::Graph(graph))),
            FormatTarget::Block
        );
        let annotation = |actions: Vec<Action>| match actions.as_slice() {
            [Action::SetAnnotation {
                graph: g,
                target,
                annotation,
            }] if *g == graph && *target == FREE_TEXT => (**annotation).clone(),
            other => panic!("a single SetAnnotation expected, not {other:?}"),
        };
        for format in [TextFormat::Bold, TextFormat::Italic] {
            let placed = annotation(request_format(&mut app, &ctx, graph, format.clone()));
            assert_eq!(placed.text, apply("Text", 0..4, &format).text);
            assert_ne!(placed.text, "Text", "{format:?} places a tag");
        }
        let colored = annotation(request_format(
            &mut app,
            &ctx,
            graph,
            TextFormat::Color("#8b1a1a".into()),
        ));
        assert_eq!(
            (colored.color.as_str(), colored.text.as_str()),
            ("#8b1a1a", "Text")
        );
        let enlarged = annotation(request_format(
            &mut app,
            &ctx,
            graph,
            TextFormat::Size(30.0),
        ));
        assert_eq!((enlarged.size, enlarged.text.as_str()), (30.0, "Text"));
        assert_eq!(pending_format(&ctx), None, "nothing is waiting for a field");
        session.shutdown().unwrap();
    }

    /// A field being edited (#63): the bar sends nothing, it deposits the
    /// format so the field places the tag on the highlighted passage at its
    /// next paint (`apply_pending_format`) — color and size included, which
    /// target the whole zone only outside editing (#90).
    #[test]
    fn in_a_field_being_edited_the_format_waits_for_the_next_paint() {
        let (session, mut app, graph, ctx) = setup();
        edit_the_title(&mut app, &ctx, graph);
        for format in [
            TextFormat::Bold,
            TextFormat::Italic,
            TextFormat::Color("#8b1a1a".into()),
            TextFormat::Size(30.0),
        ] {
            assert!(
                request_format(&mut app, &ctx, graph, format.clone()).is_empty(),
                "{format:?} sends nothing before the field is validated"
            );
            assert_eq!(pending_format(&ctx), Some(format));
        }
        session.shutdown().unwrap();
    }

    /// Outside editing, color and size of a selected title go to the whole
    /// zone (#77, #90); an invalid size sends nothing.
    #[test]
    fn the_color_and_size_of_a_selected_title_go_to_the_zone() {
        let (session, mut app, graph, ctx) = setup();
        let target = TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Bottom,
        };
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_text(Some(target), &spec));
        assert_eq!(
            request_format(&mut app, &ctx, graph, TextFormat::Color("#8b1a1a".into())),
            vec![Action::SetTextColor {
                graph,
                target,
                color: "#8b1a1a".into(),
            }]
        );
        match request_format(&mut app, &ctx, graph, TextFormat::Size(30.0)).as_slice() {
            [Action::SetTextFont {
                target: t, font, ..
            }] => {
                assert_eq!(*t, target);
                assert_eq!(font.size, Some(30.0));
            }
            other => panic!("a single SetTextFont expected, not {other:?}"),
        }
        for invalid in [0.0, -4.0, f64::NAN] {
            assert!(request_format(&mut app, &ctx, graph, TextFormat::Size(invalid)).is_empty());
        }
        assert_eq!(pending_format(&ctx), None);
        session.shutdown().unwrap();
    }

    /// The font is a field of the zone (#78), never a `#text(font: …)` tag in
    /// the content: selected or edited title, selected or edited free text.
    #[test]
    fn the_font_is_a_zone_field_never_a_tag() {
        let font_format = || TextFormat::Font("  Fira Sans ".into());
        let without_tag = |actions: &[Action]| {
            assert!(
                !format!("{actions:?}").contains("#text(font"),
                "no font tag in {actions:?}"
            );
        };
        let title_action = |actions: Vec<Action>| match actions.as_slice() {
            [Action::SetTextFont { target, font, .. }] => {
                assert_eq!(*target, TextTarget::Title);
                assert_eq!(font.family, "Fira Sans");
            }
            other => panic!("a single SetTextFont expected, not {other:?}"),
        };

        // Selected title.
        let (session, mut app, graph, ctx) = setup();
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let actions = request_format(&mut app, &ctx, graph, font_format());
        without_tag(&actions);
        title_action(actions);

        // Selected free text: its `font` field, text unchanged.
        app.with_graph_view(graph, |view| view.select_annotation(Some(FREE_TEXT), &spec));
        let actions = request_format(&mut app, &ctx, graph, font_format());
        without_tag(&actions);
        match actions.as_slice() {
            [Action::SetAnnotation { annotation, .. }] => {
                assert_eq!(annotation.font, "Fira Sans");
                assert_eq!(annotation.text, "Text");
            }
            other => panic!("a single SetAnnotation expected, not {other:?}"),
        }

        // Free text being edited: the draft will receive the font.
        let id = app
            .with_graph_view(graph, |view| annotation_editor_id(view))
            .unwrap();
        ctx.data_mut(|data| {
            data.insert_temp(
                id,
                AnnotationEditor {
                    target: FREE_TEXT,
                    draft: FREE_TEXT.get(&spec).unwrap().clone(),
                    original: FREE_TEXT.get(&spec).cloned(),
                    focus: false,
                    anchor: None,
                },
            )
        });
        assert!(request_format(&mut app, &ctx, graph, font_format()).is_empty());
        assert_eq!(pending_font(&ctx).as_deref(), Some("Fira Sans"));
        assert_eq!(pending_format(&ctx), None, "no tag pending");
        session.shutdown().unwrap();

        // Title being edited: the zone directly, nothing pending.
        let (session, mut app, graph, ctx) = setup();
        edit_the_title(&mut app, &ctx, graph);
        let actions = request_format(&mut app, &ctx, graph, font_format());
        without_tag(&actions);
        title_action(actions);
        assert_eq!(pending_format(&ctx), None, "no tag pending");
        session.shutdown().unwrap();
    }

    /// The contextual bar follows the foreground window, and the tool chosen
    /// by a button is that of the graph view, floating or detached.
    #[test]
    fn the_toolbar_follows_the_active_window_and_its_tool() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let ctx = egui::Context::default();
        assert!(matches!(app.tools_context(None), ToolsContext::None));

        for detached in [false, true] {
            if detached {
                app.detach_tab(Tab::Graph(graph));
            }
            let mut actions = Vec::new();
            app.run_toolbar_request(
                &ctx,
                ToolbarRequest::SetGraphTool(GraphTool::Pointer),
                Some(Tab::Graph(graph)),
                &mut actions,
            );
            assert!(matches!(
                app.tools_context(Some(Tab::Graph(graph))),
                ToolsContext::Graph { graph: g, tool: GraphTool::Pointer } if g == graph
            ));
            app.run_toolbar_request(
                &ctx,
                ToolbarRequest::SetGraphTool(GraphTool::Text),
                Some(Tab::Graph(graph)),
                &mut actions,
            );
            assert!(matches!(
                app.tools_context(Some(Tab::Graph(graph))),
                ToolsContext::Graph {
                    tool: GraphTool::Text,
                    ..
                }
            ));
            assert!(actions.is_empty(), "choosing a tool is not a command");
            assert_eq!(
                app.read_graph_view(graph, GraphView::tool),
                Some(GraphTool::Text),
                "detached = {detached}"
            );
        }

        // Workbook: the displayed sheet and its selected columns.
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[1].0;
        let ToolsContext::Book { columns, .. } = app.tools_context(Some(Tab::Book(book))) else {
            panic!("workbook context expected");
        };
        assert!(columns.is_empty());
        app.state.select_column(sheet, column);
        match app.tools_context(Some(Tab::Book(book))) {
            ToolsContext::Book {
                book: b,
                sheet: s,
                columns,
            } => assert_eq!((b, s, columns), (book, Some(sheet), vec![column])),
            other => panic!("workbook context expected, not {other:?}"),
        }

        // A button that carries an action pushes it as is.
        let mut actions = Vec::new();
        app.run_toolbar_request(
            &ctx,
            ToolbarRequest::Action(Box::new(Action::AddWorkbook)),
            None,
            &mut actions,
        );
        assert_eq!(actions, vec![Action::AddWorkbook]);
        session.shutdown().unwrap();
    }

    /// The object format targets the selected graphic object: a single action,
    /// and nothing without a selection.
    #[test]
    fn the_object_format_targets_the_selected_object() {
        let mut project = Project::new("Objects").unwrap();
        let mut spec = PlotSpec::empty("Objects");
        spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
            ShapeKind::Rectangle,
            0.0,
            0.0,
            1.0,
            1.0,
        ));
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let tab = Some(Tab::Graph(graph));
        let mut actions = Vec::new();
        app.format_object(&ObjectFormat::LineWidth(3.0), tab, &mut actions);
        assert!(actions.is_empty(), "without a selection, nothing");
        assert!(app.object_format_state(tab).is_none());

        let target = ShapeRef { panel: 0, index: 0 };
        app.with_graph_view(graph, |view| view.select_shape(Some(target)));
        let state = app.object_format_state(tab).unwrap();
        assert!(state.closed, "a rectangle can be filled");
        app.format_object(&ObjectFormat::LineWidth(3.0), tab, &mut actions);
        match actions.as_slice() {
            [Action::SetShape {
                graph: g,
                target: t,
                shape,
            }] => {
                assert_eq!((*g, *t), (graph, target));
                assert_eq!(shape.line_width, 3.0);
            }
            other => panic!("a single SetShape expected, not {other:?}"),
        }
        session.shutdown().unwrap();
    }
}

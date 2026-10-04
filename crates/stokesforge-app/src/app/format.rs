//! Barres de format de texte et d'objet, et barre d'outils contextuelle :
//! ce qu'elles montrent ([`StokesForgeApp::format_target`],
//! [`StokesForgeApp::text_format_state`], [`StokesForgeApp::object_format_state`],
//! [`StokesForgeApp::tools_context`]) et ce qu'elles font
//! ([`StokesForgeApp::run_toolbar_request`]).
//!
//! Les boutons eux-mêmes vivent dans `crate::toolbar` ; la mise en balises
//! Typst d'un passage dans `crate::text_format` ; ici, le lien entre les deux
//! et la sélection de la vue de graphe (flottante ou détachée). La police, et
//! pour un titre ou la légende le corps et la couleur, sont des champs de la
//! zone ([`StokesForgeApp::format_zone`], #77/#78/#90), jamais une balise
//! `#text(font: …)` dans le contenu.
//!
//! Sorti d'`app.rs` tel quel (#122).

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_render_egui::{GraphView, TextTarget};

use crate::state::{active_graph, Action, ActiveTabKind};
use crate::toolbar::ToolbarRequest;

use super::graph_text_edit::{
    annotation_editor_id, pending_format_id, pending_zone_font_id, text_editor_id,
    AnnotationEditor, TextEditTarget, TextEditor,
};
use super::tab::Tab;
use super::{active_tab_kind, StokesForgeApp};

impl StokesForgeApp {
    /// Ce que la barre contextuelle montre pour la fenêtre au premier plan.
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

    /// Sur quoi la barre de format agit : un champ en édition sur place, sinon
    /// un texte libre sélectionné, sinon rien (issue #63).
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

    /// Lit les réglages de la zone visée et les défauts du pipeline partagé.
    /// Le brouillon en édition prime sur le modèle, aussi en fenêtre détachée.
    pub(super) fn text_format_state(
        &self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) -> crate::toolbar::TextFormatState {
        use crate::toolbar::TextFormatState;
        let Some(graph) = active_graph(active_tab_kind(active_tab)) else {
            return TextFormatState::default();
        };
        let Some(spec) = crate::state::current_plot_spec(self.state.tree(), graph) else {
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
                        }
                        .with_markup(&annotation.text),
                    );
                }
                let editor = ctx.data(|data| data.get_temp::<TextEditor>(text_editor_id(view)));
                let target = editor
                    .as_ref()
                    .map(|editor| match editor.target {
                        TextEditTarget::Title(target) => target,
                        TextEditTarget::LegendEntry(_, (row, column)) => {
                            TextTarget::Legend { row, column }
                        }
                    })
                    .or_else(|| view.selected_text())?;
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
                    }
                    .with_markup(&text),
                )
            })
            .flatten();
        state.unwrap_or_default()
    }

    /// L'objet graphique sélectionné du graphe au premier plan, avec son
    /// adresse (issue #64).
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
        let shape = crate::state::current_plot_spec(self.state.tree(), graph)
            .and_then(|spec| target.get(&spec).cloned())?;
        Some((graph, target, shape))
    }

    /// Ce que la barre de format d'objet montre.
    pub(super) fn object_format_state(
        &self,
        active_tab: Option<Tab>,
    ) -> Option<crate::toolbar::ObjectFormatState> {
        let graph = active_graph(active_tab_kind(active_tab))?;
        if let Some(target) = self.selected_series(graph) {
            let spec = crate::state::current_plot_spec(self.state.tree(), graph)?;
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

    /// Règle l'objet sélectionné : une seule action annulable, comme un
    /// déplacement (`ObjectFormat::apply` dit si quelque chose a changé).
    pub(super) fn format_object(
        &mut self,
        format: &crate::object_format::ObjectFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        if let Some(graph) = active_graph(active_tab_kind(active_tab)) {
            if let Some(target) = self.selected_series(graph) {
                if let Some(mut spec) = crate::state::current_plot_spec(self.state.tree(), graph) {
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

    /// Règle la police (ou le corps) de la **zone** de texte visée, sans
    /// toucher à son contenu. Rend `true` si la demande a été prise en charge ;
    /// `false` laisse [`StokesForgeApp::format_text`] poser des balises (le corps
    /// d'un passage du champ en édition, la couleur, le gras…).
    ///
    /// - un texte libre en édition : le brouillon de l'éditeur
    ///   ([`apply_pending_zone_font`]) ;
    /// - un titre ou un nom de légende en édition : le titre, ou la légende,
    ///   directement ;
    /// - un titre, un titre d'axe ou la légende sélectionné : idem, police et
    ///   corps ;
    /// - un texte libre sélectionné : sa police ;
    /// - la couleur d'un titre, d'un titre d'axe ou de la légende sélectionné :
    ///   celle de la zone entière (#90) ; en édition, un passage (balise).
    fn format_zone(
        &mut self,
        ctx: &egui::Context,
        format: &crate::text_format::TextFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) -> bool {
        use crate::text_format::TextFormat;
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
        // Quelle zone de titre ? (`None` : pas de titre visé)
        let zone = match (&title_editing, selected_text) {
            (Some(editor), _) => Some(match editor.target {
                TextEditTarget::Title(target) => target,
                TextEditTarget::LegendEntry(_, (row, column)) => {
                    stokesforge_render_egui::TextTarget::Legend { row, column }
                }
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
                    crate::state::current_plot_spec(self.state.tree(), graph)
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
        // La couleur : celle de la zone d'un titre sélectionné ; dans un champ
        // en édition elle reste une balise sur le passage surligné (#90).
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
        // Le corps : celui de la zone d'un titre sélectionné ; dans un champ
        // en édition il reste une balise sur le passage surligné.
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

    /// Pose la police ou le corps de la zone de titre `target` : lit la
    /// police actuelle dans le modèle, la change par `edit`, et pousse
    /// **une** action annulable. `true` toujours (la demande est prise en
    /// charge même si le graphe n'a plus ce texte).
    fn set_zone_font(
        &self,
        graph: NodeId,
        target: stokesforge_render_egui::TextTarget,
        actions: &mut Vec<Action>,
        edit: impl FnOnce(&mut stokesforge_plot::TextFont),
    ) -> bool {
        if let Some(mut font) = crate::state::current_plot_spec(self.state.tree(), graph)
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

    /// Met en forme le texte visé : dans le champ en édition (le champ pose
    /// les balises à son prochain tracé, voir [`apply_pending_format`]), ou
    /// sur tout le texte libre sélectionné — une seule action annulable.
    pub(super) fn format_text(
        &mut self,
        ctx: &egui::Context,
        format: crate::text_format::TextFormat,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        use crate::text_format::{apply, TextFormat};
        // La police est un attribut de la zone, jamais une balise (issue
        // #78) ; le corps aussi pour un titre ou la légende (issue #77).
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
                    crate::state::current_plot_spec(self.state.tree(), graph)
                        .and_then(|spec| target.get(&spec).cloned())
                else {
                    return;
                };
                // Couleur et corps d'un texte libre sont des champs du modèle.
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

    /// Exécute ce qu'un bouton de barre d'outils a demandé : les mêmes
    /// chemins que les entrées de menu correspondantes, jamais une seconde
    /// écriture.
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

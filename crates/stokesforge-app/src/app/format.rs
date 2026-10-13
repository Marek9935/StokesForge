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
use super::tab::active_tab_kind;
use super::tab::Tab;
use super::StokesForgeApp;

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
                        // Un nom de groupe n'a pas de police propre.
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
    fn format_object(
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
    ///   (`apply_pending_zone_font` (`graph_text_edit.rs`)) ;
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
        // L'alignement (#244) est un champ de la boîte : le brouillon d'un
        // texte libre en édition, sinon la zone d'un titre (édité ou
        // sélectionné), sinon le texte libre sélectionné.
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
                    crate::state::current_plot_spec(self.state.tree(), graph)
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
        // Gras et italique d'un titre sélectionné hors édition (#243) : les
        // balises enveloppent tout le texte, comme pour un texte libre ; un
        // texte déjà entièrement enveloppé se déballe. La légende n'a pas de
        // texte propre (`TextTarget::text` rend `None`) : rien à envelopper.
        if let (TextFormat::Bold | TextFormat::Italic, Some(target), None, false) =
            (format, zone, &title_editing, annotation_editing)
        {
            if let Some(text) = crate::state::current_plot_spec(self.state.tree(), graph)
                .and_then(|spec| target.text(&spec))
            {
                let length = text.chars().count();
                actions.push(Action::SetTextContent {
                    graph,
                    target,
                    text: crate::text_format::apply(&text, 0..length, format).text,
                });
            }
            return true;
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
    /// les balises à son prochain tracé, voir `apply_pending_format` (`graph_text_edit.rs`)), ou
    /// sur tout le texte libre sélectionné — une seule action annulable.
    fn format_text(
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

#[cfg(test)]
mod tests {
    use super::super::graph_text_edit::open_text_editor;
    use super::super::graph_window::graph_drop_target;
    use super::super::graph_window::tests::graphe;
    use super::super::object_manager_ui::{selected_manager_target, ObjectManagerRequest};
    use super::super::project_manager::tests::{click_at, find_text_center};
    use super::*;
    use crate::object_format::ObjectFormat;
    use crate::state::AppState;
    use crate::text_format::{apply, TextFormat};
    use crate::theme;
    use crate::toolbar::{BarEnv, FormatTarget, ToolbarId, ToolsContext};
    use stokesforge_core::Project;
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, AxisSide, PanelSpec, PlotSpec, SeriesRef, SeriesSpec,
        ShapeKind, ShapeRef, ShapeSpec, TextFont,
    };
    use stokesforge_render_egui::GraphTool;
    use stokesforge_session::Session;

    /// Le texte libre du graphe de [`setup`].
    const TEXTE: AnnotationRef = AnnotationRef { panel: 0, index: 0 };

    /// Les actions d'une demande de format, sans les envoyer.
    fn formate(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        graph: NodeId,
        format: TextFormat,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        app.format_text(ctx, format, Some(Tab::Graph(graph)), &mut actions);
        actions
    }

    fn format_en_attente(ctx: &egui::Context) -> Option<TextFormat> {
        ctx.data(|data| data.get_temp::<Option<TextFormat>>(pending_format_id()))
            .flatten()
    }

    fn police_en_attente(ctx: &egui::Context) -> Option<String> {
        ctx.data(|data| data.get_temp::<Option<String>>(pending_zone_font_id()))
            .flatten()
    }

    /// Ouvre l'éditeur sur place du titre, comme un double-clic : le graphe
    /// est d'abord peint, pour que le titre ait une boîte à l'écran.
    fn edite_le_titre(app: &mut StokesForgeApp, ctx: &egui::Context, graph: NodeId) {
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
            "l'éditeur du titre doit être ouvert"
        );
    }

    /// #97 : les champs de la barre affichent le format de la zone visée.
    #[test]
    fn la_barre_affiche_la_police_et_le_corps_du_titre_selectionne() {
        use eframe::App as _;
        let (session, graph) = graphe();
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
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
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

    /// #99 : le même choix dans le sélecteur agit sur deux zones successives.
    #[test]
    fn la_meme_couleur_s_applique_a_deux_zones_successives_par_la_barre() {
        let (session, graph) = graphe();
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
                .unwrap_or_else(|| panic!("échantillon peint (palette={palette})"))
        };
        for target in [
            TextTarget::Title,
            TextTarget::AxisTitle {
                panel: 0,
                side: stokesforge_plot::AxisSide::Bottom,
            },
        ] {
            let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
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
            let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
            assert_eq!(
                target.color(&spec).as_deref(),
                Some("#8b1a1a"),
                "la zone {target:?} reçoit le rouge"
            );
        }
        session.shutdown().unwrap();
    }

    fn setup() -> (Session, StokesForgeApp, NodeId, egui::Context) {
        let mut project = Project::new("Formats").unwrap();
        let mut spec = PlotSpec::empty("Titre");
        spec.panels
            .get_mut(0)
            .unwrap()
            .annotations
            .push(AnnotationSpec::new("Texte", 0.0, 0.0));
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
    fn les_corps_herites_et_auto_suivent_la_selection_flottante_et_detachee() {
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
                let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
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
                    "Auto ne change pas la police"
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
    fn la_barre_lit_le_brouillon_et_auto_retablit_le_corps_du_texte_libre() {
        let (session, mut app, graph, ctx) = setup();
        let target = AnnotationRef { panel: 0, index: 0 };
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
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
            "seul le brouillon change avant validation"
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

    /// Les boutons d'alignement de la barre (#244) posent `align` : sur le
    /// texte libre sélectionné (champ de la boîte), sur un titre sélectionné
    /// (champ de sa zone), chacun en une seule action.
    #[test]
    fn l_alignement_se_pose_sur_la_boite_ou_la_zone() {
        use stokesforge_plot::TextAlign;
        let (session, mut app, graph, ctx) = setup();
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_annotation(Some(TEXTE), &spec));
        match formate(&mut app, &ctx, graph, TextFormat::Align(TextAlign::Right)).as_slice() {
            [Action::SetAnnotation { annotation, .. }] => {
                assert_eq!(annotation.align, TextAlign::Right);
                assert_eq!(annotation.text, "Texte", "aucune balise");
            }
            other => panic!("une SetAnnotation attendue, pas {other:?}"),
        }
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        match formate(&mut app, &ctx, graph, TextFormat::Align(TextAlign::Left)).as_slice() {
            [Action::SetTextFont { font, .. }] => assert_eq!(font.align, TextAlign::Left),
            other => panic!("une SetTextFont attendue, pas {other:?}"),
        }
        // Le bouton de l'alignement courant reste enfoncé : l'état le lit.
        assert_eq!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph))).align,
            Some(TextAlign::Center)
        );
        app.with_graph_view(graph, |view| view.select_annotation(Some(TEXTE), &spec));
        assert_eq!(
            app.text_format_state(&ctx, Some(Tab::Graph(graph))).align,
            Some(TextAlign::Center)
        );
        session.shutdown().unwrap();
    }

    /// Un titre sélectionné hors édition (#243) : B / I enveloppent tout son
    /// texte en une seule action ; une boîte à moitié enveloppée est
    /// enveloppée en entier, une boîte entièrement enveloppée se déballe.
    #[test]
    fn gras_et_italique_enveloppent_tout_un_titre_selectionne() {
        let (session, mut app, graph, ctx) = setup();
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let texte = |actions: Vec<Action>| match actions.as_slice() {
            [Action::SetTextContent { text, .. }] => text.clone(),
            other => panic!("une seule SetTextContent attendue, pas {other:?}"),
        };
        assert_eq!(
            texte(formate(&mut app, &ctx, graph, TextFormat::Bold)),
            "#strong[Titre]"
        );
        assert_eq!(
            texte(formate(&mut app, &ctx, graph, TextFormat::Italic)),
            "#emph[Titre]"
        );
        let mut poser = |text: &str| {
            app.state.dispatch(Action::SetTextContent {
                graph,
                target: TextTarget::Title,
                text: text.into(),
            });
            texte(formate(&mut app, &ctx, graph, TextFormat::Bold))
        };
        assert_eq!(poser("#strong[a] b"), "#strong[#strong[a] b]");
        assert_eq!(poser("#strong[Titre]"), "Titre");
        session.shutdown().unwrap();
    }

    #[test]
    fn les_formats_locaux_affichent_un_etat_neutre() {
        let (session, mut app, graph, ctx) = setup();
        app.state.dispatch(Action::SetTextContent {
            graph,
            target: TextTarget::Title,
            text: "Titre #text(size: 28pt, font: \"Fira Sans\", fill: red)[local]".into(),
        });
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
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
    fn la_selection_et_le_format_rejoignent_la_meme_vue_detachee() {
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut panel = PanelSpec::default();
        panel.series.push(SeriesSpec::new(NodeId::from_raw(100)));
        let spec = PlotSpec::empty("Objets").with_panels(panel);
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
        // Revenir dans le panneau conserve la cible native du graphe.
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            assert_eq!(app.active_content_tab(&ctx), Some(Tab::Graph(graph)));
        });
        output.textures_delta.clear();
        let mut actions = Vec::new();
        app.format_object(
            &crate::object_format::ObjectFormat::LineWidth(4.0),
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
            crate::state::current_plot_spec(app.state.tree(), graph).unwrap(),
            spec
        );
        app.reattach_tab(Tab::Graph(graph));
        assert_eq!(app.selected_series(graph), Some(target));
        assert_eq!(app.graph_views[&graph].selected_series(), Some(target));
    }
    #[test]
    fn les_objets_masques_et_le_format_suivent_la_vue_detachee() {
        use crate::object_manager::VisibilityTarget as T;
        use stokesforge_plot::{
            AnnotationRef, AnnotationSpec, ShapeKind, ShapeRef, ShapeSpec, TextTarget,
        };
        let mut project = Project::new("Objets").unwrap();
        let root = project.root();
        let mut spec = PlotSpec::empty("Titre");
        spec.title_visible = false;
        let panel = spec.panels.get_mut(0).unwrap();
        panel
            .annotations
            .push(AnnotationSpec::new("Texte", 0.0, 0.0));
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
                "aucune seconde vue créée"
            );
        }
        assert_eq!(
            app.format_target(&ctx, Some(Tab::Graph(graph))),
            crate::toolbar::FormatTarget::Zone
        );
        app.format_text(
            &ctx,
            crate::text_format::TextFormat::Size(28.0),
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
        assert!(actions.is_empty(), "la sélection n'envoie aucune commande");
    }

    /// Un texte libre sélectionné (sans éditeur ouvert) : gras et italique
    /// sont des balises posées sur **tout** son texte par
    /// `text_format::apply`, couleur et corps des champs du modèle ; chaque
    /// demande est une seule action annulable.
    #[test]
    fn gras_italique_couleur_et_corps_d_un_texte_libre_selectionne() {
        let (session, mut app, graph, ctx) = setup();
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_annotation(Some(TEXTE), &spec));
        assert_eq!(
            app.format_target(&ctx, Some(Tab::Graph(graph))),
            FormatTarget::Block
        );
        let annotation = |actions: Vec<Action>| match actions.as_slice() {
            [Action::SetAnnotation {
                graph: g,
                target,
                annotation,
            }] if *g == graph && *target == TEXTE => (**annotation).clone(),
            other => panic!("une seule SetAnnotation attendue, pas {other:?}"),
        };
        for format in [TextFormat::Bold, TextFormat::Italic] {
            let posee = annotation(formate(&mut app, &ctx, graph, format.clone()));
            assert_eq!(posee.text, apply("Texte", 0..5, &format).text);
            assert_ne!(posee.text, "Texte", "{format:?} pose une balise");
        }
        let colore = annotation(formate(
            &mut app,
            &ctx,
            graph,
            TextFormat::Color("#8b1a1a".into()),
        ));
        assert_eq!(
            (colore.color.as_str(), colore.text.as_str()),
            ("#8b1a1a", "Texte")
        );
        let agrandi = annotation(formate(&mut app, &ctx, graph, TextFormat::Size(30.0)));
        assert_eq!((agrandi.size, agrandi.text.as_str()), (30.0, "Texte"));
        assert_eq!(format_en_attente(&ctx), None, "rien n'attend de champ");
        session.shutdown().unwrap();
    }

    /// Un champ en édition (#63) : la barre n'envoie rien, elle dépose le
    /// format pour que le champ pose la balise sur le passage surligné à son
    /// prochain tracé (`apply_pending_format`) — couleur et corps compris,
    /// qui ne visent la zone entière que hors édition (#90).
    #[test]
    fn dans_un_champ_en_edition_le_format_attend_le_prochain_trace() {
        let (session, mut app, graph, ctx) = setup();
        edite_le_titre(&mut app, &ctx, graph);
        for format in [
            TextFormat::Bold,
            TextFormat::Italic,
            TextFormat::Color("#8b1a1a".into()),
            TextFormat::Size(30.0),
        ] {
            assert!(
                formate(&mut app, &ctx, graph, format.clone()).is_empty(),
                "{format:?} n'envoie rien avant la validation du champ"
            );
            assert_eq!(format_en_attente(&ctx), Some(format));
        }
        session.shutdown().unwrap();
    }

    /// Hors édition, couleur et corps d'un titre sélectionné vont à la zone
    /// entière (#77, #90) ; un corps invalide n'envoie rien.
    #[test]
    fn la_couleur_et_le_corps_d_un_titre_selectionne_vont_a_la_zone() {
        let (session, mut app, graph, ctx) = setup();
        let target = TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Bottom,
        };
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| view.select_text(Some(target), &spec));
        assert_eq!(
            formate(&mut app, &ctx, graph, TextFormat::Color("#8b1a1a".into())),
            vec![Action::SetTextColor {
                graph,
                target,
                color: "#8b1a1a".into(),
            }]
        );
        match formate(&mut app, &ctx, graph, TextFormat::Size(30.0)).as_slice() {
            [Action::SetTextFont {
                target: t, font, ..
            }] => {
                assert_eq!(*t, target);
                assert_eq!(font.size, Some(30.0));
            }
            other => panic!("une seule SetTextFont attendue, pas {other:?}"),
        }
        for invalide in [0.0, -4.0, f64::NAN] {
            assert!(formate(&mut app, &ctx, graph, TextFormat::Size(invalide)).is_empty());
        }
        assert_eq!(format_en_attente(&ctx), None);
        session.shutdown().unwrap();
    }

    /// La police est un champ de la zone (#78), jamais une balise
    /// `#text(font: …)` dans le contenu : titre sélectionné ou en édition,
    /// texte libre sélectionné ou en édition.
    #[test]
    fn la_police_est_un_champ_de_la_zone_jamais_une_balise() {
        let police = || TextFormat::Font("  Fira Sans ".into());
        let sans_balise = |actions: &[Action]| {
            assert!(
                !format!("{actions:?}").contains("#text(font"),
                "aucune balise de police dans {actions:?}"
            );
        };
        let titre = |actions: Vec<Action>| match actions.as_slice() {
            [Action::SetTextFont { target, font, .. }] => {
                assert_eq!(*target, TextTarget::Title);
                assert_eq!(font.family, "Fira Sans");
            }
            other => panic!("une seule SetTextFont attendue, pas {other:?}"),
        };

        // Titre sélectionné.
        let (session, mut app, graph, ctx) = setup();
        let spec = crate::state::current_plot_spec(app.state.tree(), graph).unwrap();
        app.with_graph_view(graph, |view| {
            view.select_text(Some(TextTarget::Title), &spec)
        });
        let actions = formate(&mut app, &ctx, graph, police());
        sans_balise(&actions);
        titre(actions);

        // Texte libre sélectionné : son champ `font`, texte inchangé.
        app.with_graph_view(graph, |view| view.select_annotation(Some(TEXTE), &spec));
        let actions = formate(&mut app, &ctx, graph, police());
        sans_balise(&actions);
        match actions.as_slice() {
            [Action::SetAnnotation { annotation, .. }] => {
                assert_eq!(annotation.font, "Fira Sans");
                assert_eq!(annotation.text, "Texte");
            }
            other => panic!("une seule SetAnnotation attendue, pas {other:?}"),
        }

        // Texte libre en édition : le brouillon recevra la police.
        let id = app
            .with_graph_view(graph, |view| annotation_editor_id(view))
            .unwrap();
        ctx.data_mut(|data| {
            data.insert_temp(
                id,
                AnnotationEditor {
                    target: TEXTE,
                    draft: TEXTE.get(&spec).unwrap().clone(),
                    original: TEXTE.get(&spec).cloned(),
                    focus: false,
                    anchor: None,
                },
            )
        });
        assert!(formate(&mut app, &ctx, graph, police()).is_empty());
        assert_eq!(police_en_attente(&ctx).as_deref(), Some("Fira Sans"));
        assert_eq!(format_en_attente(&ctx), None, "pas de balise en attente");
        session.shutdown().unwrap();

        // Titre en édition : la zone directement, rien en attente.
        let (session, mut app, graph, ctx) = setup();
        edite_le_titre(&mut app, &ctx, graph);
        let actions = formate(&mut app, &ctx, graph, police());
        sans_balise(&actions);
        titre(actions);
        assert_eq!(format_en_attente(&ctx), None, "pas de balise en attente");
        session.shutdown().unwrap();
    }

    /// La barre contextuelle suit la fenêtre au premier plan, et l'outil
    /// choisi par un bouton est celui de la vue du graphe, flottante ou
    /// détachée.
    #[test]
    fn la_barre_d_outils_suit_la_fenetre_active_et_son_outil() {
        let (session, graph) = graphe();
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
            assert!(
                actions.is_empty(),
                "choisir un outil n'est pas une commande"
            );
            assert_eq!(
                app.read_graph_view(graph, GraphView::tool),
                Some(GraphTool::Text),
                "détachée = {detached}"
            );
        }

        // Classeur : la feuille affichée et ses colonnes sélectionnées.
        let book = app.state.books()[0].0;
        let sheet = app.state.selected_sheet_in(book).unwrap();
        let column = app.state.visible_columns(sheet)[1].0;
        let ToolsContext::Book { columns, .. } = app.tools_context(Some(Tab::Book(book))) else {
            panic!("contexte de classeur attendu");
        };
        assert!(columns.is_empty());
        app.state.select_column(sheet, column);
        match app.tools_context(Some(Tab::Book(book))) {
            ToolsContext::Book {
                book: b,
                sheet: s,
                columns,
            } => assert_eq!((b, s, columns), (book, Some(sheet), vec![column])),
            other => panic!("contexte de classeur attendu, pas {other:?}"),
        }

        // Un bouton qui porte une action la pousse telle quelle.
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

    /// Le format d'objet vise l'objet graphique sélectionné : une seule
    /// action, et rien sans sélection.
    #[test]
    fn le_format_d_objet_vise_l_objet_selectionne() {
        let mut project = Project::new("Objets").unwrap();
        let mut spec = PlotSpec::empty("Objets");
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
        assert!(actions.is_empty(), "sans sélection, rien");
        assert!(app.object_format_state(tab).is_none());

        let target = ShapeRef { panel: 0, index: 0 };
        app.with_graph_view(graph, |view| view.select_shape(Some(target)));
        let state = app.object_format_state(tab).unwrap();
        assert!(state.closed, "un rectangle se remplit");
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
            other => panic!("une seule SetShape attendue, pas {other:?}"),
        }
        session.shutdown().unwrap();
    }
}

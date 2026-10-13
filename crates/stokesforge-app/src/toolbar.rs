//! Barres d'outils (issues #60, #108, chantier #59).
//!
//! # Des barres fixes
//!
//! Le contenu des barres est figé dans le logiciel, et leur place aussi : la
//! zone du haut, sous la barre de menu, rangée par rangée ([`ToolbarId::row`]).
//! Pas de poignée, pas de glisser-déposer, pas de barre flottante, pas de
//! barre verticale ni de logique de fenêtre étroite (#108). La zone est un
//! `egui::Panel::top` : ni `egui::Window` brute (interdite,
//! `aucune_fenetre_n_est_une_egui_window_brute`) ni `show_window_in_layer`.
//!
//! La disposition par défaut est écrite en dur : rangée 0 = `File`, `Tools` ;
//! rangée 1 = `Text`, large, seule sur sa rangée. Le panneau Object Manager
//! se commande par View et son en-tête (#100) : sa barre est supprimée (#109).
//!
//! # Ce qui est pur, ce qui est egui
//!
//! [`ToolbarLayout`] (la visibilité par barre) est pur et testé sans fenêtre.
//! [`ToolbarState`] n'est que la glu qui peint. Une barre ne contient que des
//! boutons ([`ToolbarButton`]) qui poussent les mêmes `Action` que les menus :
//! jamais une seconde écriture. Les barres `Tools` et `Text` se peignent
//! toujours : sans fenêtre à servir, leurs boutons sont grisés, avec une
//! info-bulle qui dit comment les employer.
//!
//! # Mémoire
//!
//! Seule la **visibilité** de chaque barre (menu View) vit dans les préférences
//! de l'utilisateur ([`crate::preferences`], champ `toolbars`), pas dans le
//! `.stf`. La lecture tolère un fichier écrit par une autre version : barre
//! inconnue ignorée, barre absente visible, ancien champ `placement` ignoré.

use eframe::egui::{self, Pos2, Rect};
use serde::{Deserialize, Serialize};

use crate::icons::{icon_image, Icon, IconRole};
use crate::state::Action;
use crate::tr;
use stokesforge_core::NodeId;
use stokesforge_plot::{PlotType, TextAlign};
use stokesforge_render_egui::GraphTool;

use crate::color_picker::{ColorUsage, PickerEnv};
use crate::number_field::{auto_number_field, number_field};
use crate::object_format::ObjectFormat;
use crate::quantities;
use crate::text_format::TextFormat;

/// Les barres existantes — un enum fermé : ajouter une barre, c'est ajouter une
/// variante, son nom (`toolbar-*`) et son contenu dans [`bar_contents`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarId {
    File,
    /// La barre contextuelle (issue #62) : son contenu suit la fenêtre au
    /// premier plan. Toujours affichée (#93) ; elle porte aussi le format de
    /// l'objet graphique sélectionné (issue #64), fusionné avec elle.
    Tools,
    /// Le format du texte (issue #63) : propre au graphe au premier plan.
    Text,
    /// Annuler / Rétablir (issue #238).
    Edit,
}

impl ToolbarId {
    pub const ALL: [ToolbarId; 4] = [
        ToolbarId::File,
        ToolbarId::Edit,
        ToolbarId::Tools,
        ToolbarId::Text,
    ];

    /// Nom de la barre : entrée du menu View.
    pub fn label(self) -> String {
        match self {
            ToolbarId::File => tr!("toolbar-file"),
            ToolbarId::Tools => tr!("toolbar-tools"),
            ToolbarId::Text => tr!("toolbar-text"),
            ToolbarId::Edit => tr!("toolbar-edit"),
        }
    }

    /// Info-bulle de l'entrée du menu View.
    pub fn hint(self) -> String {
        match self {
            ToolbarId::File => tr!("toolbar-file", "hint"),
            ToolbarId::Tools => tr!("toolbar-tools", "hint"),
            ToolbarId::Text => tr!("toolbar-text", "hint"),
            ToolbarId::Edit => tr!("toolbar-edit", "hint"),
        }
    }
}

impl ToolbarId {
    /// Rangée fixe de la barre dans la zone du haut (0 = la plus haute). Les
    /// barres de format sont larges : `Text` a sa propre rangée.
    pub const fn row(self) -> usize {
        match self {
            ToolbarId::Text => 1,
            ToolbarId::File | ToolbarId::Edit | ToolbarId::Tools => 0,
        }
    }
}

/// Une barre et sa visibilité. Un ancien champ `placement` (avant #108) est
/// ignoré à la lecture.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: ToolbarId,
    pub visible: bool,
}

/// Visibilité de toutes les barres. Leur place est fixe ([`ToolbarId::row`],
/// ordre de [`ToolbarId::ALL`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolbarLayout {
    entries: Vec<Entry>,
}

impl Default for ToolbarLayout {
    fn default() -> Self {
        Self {
            entries: ToolbarId::ALL
                .iter()
                .map(|&id| Entry { id, visible: true })
                .collect(),
        }
    }
}

impl ToolbarLayout {
    fn entry(&self, id: ToolbarId) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn is_visible(&self, id: ToolbarId) -> bool {
        self.entry(id).is_some_and(|entry| entry.visible)
    }

    pub fn set_visible(&mut self, id: ToolbarId, visible: bool) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.visible = visible;
        }
    }

    /// Les barres visibles, rangée par rangée, dans l'ordre de
    /// [`ToolbarId::ALL`]. Une rangée sans barre visible n'apparaît pas.
    pub fn rows(&self) -> Vec<Vec<ToolbarId>> {
        let mut rows: std::collections::BTreeMap<usize, Vec<ToolbarId>> = Default::default();
        for id in ToolbarId::ALL {
            if self.is_visible(id) {
                rows.entry(id.row()).or_default().push(id);
            }
        }
        rows.into_values().collect()
    }

    /// Lit une disposition écrite par cette version ou une autre : ce qui ne se
    /// lit pas est ignoré, toute barre manquante revient visible.
    pub fn from_json(value: &serde_json::Value) -> Self {
        let mut layout = ToolbarLayout {
            entries: Vec::new(),
        };
        let items = value
            .get("entries")
            .and_then(|entries| entries.as_array())
            .map_or(&[][..], Vec::as_slice);
        for item in items {
            if let Ok(entry) = serde_json::from_value::<Entry>(item.clone()) {
                if layout.entry(entry.id).is_none() {
                    layout.entries.push(entry);
                }
            }
        }
        for default in Self::default().entries {
            if layout.entry(default.id).is_none() {
                layout.entries.push(default);
            }
        }
        layout
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Un bouton de barre : icône (composant commun), info-bulle obligatoire,
/// raccourci rappelé. Il rend `true` au clic ; l'appelant pousse l'`Action`.
pub struct ToolbarButton {
    pub icon: Icon,
    pub role: IconRole,
    /// Le libellé de l'action (`action-*`).
    pub label: String,
    /// Le `.hint` de l'action, ou le `.disabled` quand elle est grisée.
    pub hint: Option<String>,
    /// Le raccourci tel que les menus l'écrivent.
    pub shortcut: Option<String>,
    pub enabled: bool,
}

impl ToolbarButton {
    pub fn show(self, ui: &mut egui::Ui) -> bool {
        let mut tooltip = self.label;
        if let Some(shortcut) = self.shortcut {
            tooltip.push_str(&format!(" ({shortcut})"));
        }
        if let Some(hint) = self.hint {
            tooltip.push('\n');
            tooltip.push_str(&hint);
        }
        let button =
            egui::Button::image(icon_image(self.icon, self.role)).frame_when_inactive(false);
        ui.add_enabled(self.enabled, button)
            .on_hover_text(&tooltip)
            .on_disabled_hover_text(tooltip)
            .clicked()
    }
}

/// Ce qu'un bouton demande à l'application. Les gestes qui ouvrent un
/// sélecteur de fichier ou un assistant, ou qui règlent la vue d'un graphe,
/// ne sont pas des `Action` : l'application les exécute par les mêmes méthodes
/// que les entrées de menu.
#[derive(Debug, Clone)]
pub enum ToolbarRequest {
    /// Une action du modèle ou de l'état, la même que celle du menu.
    Action(Box<Action>),
    Import,
    Open,
    SaveAs,
    ExportGraph,
    /// Change l'outil du graphe au premier plan.
    SetGraphTool(GraphTool),
    /// Réinitialise le zoom du graphe au premier plan.
    ResetZoom,
    /// Met en forme le texte visé du graphe au premier plan.
    FormatText(TextFormat),
    /// Règle l'objet graphique sélectionné du graphe au premier plan.
    FormatObject(ObjectFormat),
}

/// Les raccourcis tels que les menus les écrivent (rappel dans l'info-bulle).
#[derive(Debug, Clone, Default)]
pub struct ShortcutTexts {
    pub new_project: String,
    pub open: String,
    pub save: String,
    pub save_as: String,
    pub import: String,
    pub export: String,
    pub undo: String,
    pub redo: String,
}

/// Sur quoi la barre de format agit (issue #63).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FormatTarget {
    /// Rien n'est sélectionné ni en édition : les boutons sont grisés.
    #[default]
    None,
    /// Un champ est en édition sur place : le surlignage, ou le curseur.
    Field,
    /// Un texte est sélectionné sans être édité : tout le texte.
    Block,
    /// Un titre, un titre d'axe ou la légende est sélectionné (issue #77) :
    /// la police et le corps de la zone, rien dans son contenu.
    Zone,
}

/// Les réglages de l'objet graphique sélectionné, tels que la barre de format
/// d'objet les montre (issue #64).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFormatState {
    /// Vrai pour un rectangle ou une ellipse : seuls ceux-là se remplissent.
    pub closed: bool,
    pub line_color: String,
    pub fill_color: String,
    pub line_width: f64,
}

/// Format lu sur la sélection, jamais la dernière valeur choisie (#97).
/// Une valeur absente laisse le libellé neutre pour un format non uniforme.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFormatState {
    /// Identité de la zone, pour borner le brouillon du sélecteur pendant une saisie.
    pub source: Option<egui::Id>,
    pub family: Option<String>,
    pub size: Option<f64>,
    /// `None` = couleur locale/mixte ; chaîne vide = automatique.
    pub color: Option<String>,
    pub auto_family: bool,
    pub auto_size: bool,
    /// Alignement de la boîte visée (#244) ; `None` sans cible. Allume le
    /// bouton correspondant.
    pub align: Option<TextAlign>,
}

impl TextFormatState {
    /// Un passage réglé dans le markup ne se résume pas au format de la zone.
    pub fn with_markup(mut self, text: &str) -> Self {
        let overrides = stokesforge_render_egui::formula::local_text_overrides(text);
        if overrides.font {
            self.family = None;
            self.auto_family = false;
        }
        if overrides.size {
            self.size = None;
            self.auto_size = false;
        }
        if overrides.color {
            self.color = None;
        }
        self
    }
}

/// Ce que la barre contextuelle montre : décidé par la fenêtre au premier plan.
#[derive(Debug, Clone, Default)]
pub enum ToolsContext {
    /// Matrice, note, ou aucune fenêtre : les outils du graphe, grisés.
    #[default]
    None,
    Graph {
        graph: NodeId,
        tool: GraphTool,
    },
    Book {
        book: NodeId,
        /// La feuille affichée, si le classeur en a une.
        sheet: Option<NodeId>,
        /// Les colonnes sélectionnées de cette feuille.
        columns: Vec<NodeId>,
    },
}

/// Ce que les barres lisent de l'application, calculé une fois par image.
#[derive(Debug, Clone, Default)]
pub struct BarEnv {
    /// Même garde que File > New Project et Ctrl+N.
    /// Libellé d'historique (« Undo Rename ») ; `None` = pile vide (grisé).
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub new_project_enabled: bool,
    /// Un graphe est au premier plan (sinon *Export* est grisé, comme le menu
    /// Graph est absent).
    pub export_enabled: bool,
    pub shortcuts: ShortcutTexts,
    pub tools: ToolsContext,
    pub text_format: FormatTarget,
    pub text_style: TextFormatState,
    /// L'objet graphique sélectionné, s'il y en a un (issue #64).
    pub object_format: Option<ObjectFormatState>,
}

/// Le contenu d'une barre. Chaque bouton répond à un geste que les menus
/// offrent déjà, avec le même libellé (`action-*`).
pub fn bar_contents(
    ui: &mut egui::Ui,
    id: ToolbarId,
    env: &BarEnv,
    requests: &mut Vec<ToolbarRequest>,
) {
    match id {
        ToolbarId::File => {
            let button = |ui: &mut egui::Ui,
                          icon,
                          role,
                          label: String,
                          hint: Option<String>,
                          shortcut: &str,
                          enabled| {
                ToolbarButton {
                    icon,
                    role,
                    label,
                    hint,
                    shortcut: (!shortcut.is_empty()).then(|| shortcut.to_string()),
                    enabled,
                }
                .show(ui)
            };
            let shortcuts = &env.shortcuts;
            if button(
                ui,
                Icon::NewProject,
                IconRole::Neutral,
                tr!("action-new-project"),
                Some(tr!("action-new-project", "hint")),
                &shortcuts.new_project,
                env.new_project_enabled,
            ) {
                requests.push(ToolbarRequest::Action(Box::new(Action::NewProject {
                    pending_edits: false,
                })));
            }
            if button(
                ui,
                Icon::Workbook,
                IconRole::Workbook,
                tr!("action-add-workbook"),
                Some(tr!("action-add-workbook", "hint")),
                "",
                true,
            ) {
                requests.push(ToolbarRequest::Action(Box::new(Action::AddWorkbook)));
            }
            if button(
                ui,
                Icon::Graph,
                IconRole::Graph,
                tr!("action-add-graph"),
                Some(tr!("action-add-graph", "hint")),
                "",
                true,
            ) {
                requests.push(ToolbarRequest::Action(Box::new(Action::AddGraph)));
            }
            if button(
                ui,
                Icon::Import,
                IconRole::Neutral,
                tr!("action-import"),
                Some(tr!("action-import", "hint")),
                &shortcuts.import,
                true,
            ) {
                requests.push(ToolbarRequest::Import);
            }
            ui.separator();
            if button(
                ui,
                Icon::Open,
                IconRole::Neutral,
                tr!("action-open"),
                None,
                &shortcuts.open,
                true,
            ) {
                requests.push(ToolbarRequest::Open);
            }
            if button(
                ui,
                Icon::Save,
                IconRole::Neutral,
                tr!("action-save"),
                None,
                &shortcuts.save,
                true,
            ) {
                requests.push(ToolbarRequest::Action(Box::new(Action::Save)));
            }
            if button(
                ui,
                Icon::SaveAs,
                IconRole::Neutral,
                tr!("action-save-as"),
                None,
                &shortcuts.save_as,
                true,
            ) {
                requests.push(ToolbarRequest::SaveAs);
            }
            ui.separator();
            let (export_hint, export_enabled) = if env.export_enabled {
                (tr!("action-export-graph", "hint"), true)
            } else {
                (tr!("action-export-graph", "disabled"), false)
            };
            if button(
                ui,
                Icon::Export,
                IconRole::Neutral,
                tr!("action-export-graph"),
                Some(export_hint),
                &shortcuts.export,
                export_enabled,
            ) {
                requests.push(ToolbarRequest::ExportGraph);
            }
        }
        ToolbarId::Edit => {
            for (icon, label, name, shortcut, disabled) in [
                (
                    Icon::Undo,
                    &env.undo_label,
                    tr!("history-undo"),
                    &env.shortcuts.undo,
                    tr!("disabled-nothing-to-undo"),
                ),
                (
                    Icon::Redo,
                    &env.redo_label,
                    tr!("history-redo"),
                    &env.shortcuts.redo,
                    tr!("disabled-nothing-to-redo"),
                ),
            ] {
                let clicked = ToolbarButton {
                    icon,
                    role: IconRole::Neutral,
                    label: label.clone().unwrap_or(name),
                    hint: label.is_none().then_some(disabled),
                    shortcut: Some(shortcut.clone()),
                    enabled: label.is_some(),
                }
                .show(ui);
                if clicked {
                    requests.push(ToolbarRequest::Action(Box::new(if icon == Icon::Undo {
                        Action::Undo
                    } else {
                        Action::Redo
                    })));
                }
            }
        }
        ToolbarId::Tools => tools_contents(ui, env, requests),
        ToolbarId::Text => text_format_contents(ui, env, requests),
    }
}

/// Bouton d'outil (pour les barres sans raccourci à rappeler).
fn tool_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: String,
    hint: Option<String>,
    enabled: bool,
    selected: bool,
) -> bool {
    let mut tooltip = label;
    if let Some(hint) = hint {
        tooltip.push('\n');
        tooltip.push_str(&hint);
    }
    let button = egui::Button::image(icon_image(icon, IconRole::Neutral))
        .frame_when_inactive(selected)
        .selected(selected);
    ui.add_enabled(enabled, button)
        .on_hover_text(&tooltip)
        .on_disabled_hover_text(tooltip)
        .clicked()
}

/// La barre contextuelle : chaque bouton répond à un geste que les menus ou
/// le menu contextuel offrent déjà.
fn tools_contents(ui: &mut egui::Ui, env: &BarEnv, requests: &mut Vec<ToolbarRequest>) {
    match &env.tools {
        ToolsContext::None => graph_tools(ui, None, None, requests),
        ToolsContext::Graph { graph, tool } => graph_tools(ui, Some(*graph), Some(*tool), requests),
        ToolsContext::Book {
            book,
            sheet,
            columns,
        } => {
            if tool_button(
                ui,
                Icon::AddSheet,
                tr!("workbook-worksheet-add"),
                None,
                true,
                false,
            ) {
                requests.push(ToolbarRequest::Action(Box::new(Action::AddWorksheet {
                    book: *book,
                })));
            }
            if tool_button(
                ui,
                Icon::AddColumn,
                tr!("workbook-column-add"),
                None,
                sheet.is_some(),
                false,
            ) {
                if let Some(sheet) = sheet {
                    requests.push(ToolbarRequest::Action(Box::new(Action::AddColumn {
                        sheet: *sheet,
                    })));
                }
            }
            ui.separator();
            for (icon, plot_type, label) in [
                (Icon::PlotLine, PlotType::Line, tr!("plot-type-line")),
                (
                    Icon::PlotScatter,
                    PlotType::Scatter,
                    tr!("plot-type-scatter"),
                ),
                (Icon::PlotBar, PlotType::Bar, tr!("plot-type-bar")),
            ] {
                let enabled = sheet.is_some() && !columns.is_empty();
                let hint = if enabled {
                    tr!("toolbar-plot-selection", "hint")
                } else {
                    tr!("toolbar-plot-selection", "disabled")
                };
                if tool_button(ui, icon, label, Some(hint), enabled, false) {
                    if let Some(sheet) = sheet {
                        requests.push(ToolbarRequest::Action(Box::new(Action::PlotColumns {
                            sheet: *sheet,
                            columns: columns.clone(),
                            plot_type,
                        })));
                    }
                }
            }
        }
    }
    ui.separator();
    object_format_contents(ui, env, requests);
}

/// Les outils du graphe. Sans graphe au premier plan (`graph` vide), ils
/// restent affichés mais grisés, avec l'info-bulle qui dit comment les
/// employer (#93) : la barre ne change pas de forme quand on change de fenêtre.
fn graph_tools(
    ui: &mut egui::Ui,
    graph: Option<NodeId>,
    tool: Option<GraphTool>,
    requests: &mut Vec<ToolbarRequest>,
) {
    let enabled = graph.is_some();
    let hint = |hint: String| {
        if enabled {
            hint
        } else {
            tr!("toolbar-tools", "disabled")
        }
    };
    let mut tool_entry = |ui: &mut egui::Ui, kind, icon, label: String, tip: String| {
        if tool_button(
            ui,
            icon,
            label,
            Some(hint(tip)),
            enabled,
            tool == Some(kind),
        ) {
            requests.push(ToolbarRequest::SetGraphTool(kind));
        }
    };
    tool_entry(
        ui,
        GraphTool::Pointer,
        Icon::Pointer,
        tr!("tool-pointer"),
        tr!("tool-pointer", "hint"),
    );
    tool_entry(
        ui,
        GraphTool::Text,
        Icon::TextTool,
        tr!("tool-text"),
        tr!("tool-text", "hint"),
    );
    // Objets graphiques (issue #76) : un outil par sorte, comme
    // Origin.
    for (kind, icon, label, hint) in [
        (
            GraphTool::Rectangle,
            Icon::ShapeRectangle,
            tr!("tool-rectangle"),
            tr!("tool-rectangle", "hint"),
        ),
        (
            GraphTool::Ellipse,
            Icon::ShapeEllipse,
            tr!("tool-ellipse"),
            tr!("tool-ellipse", "hint"),
        ),
        (
            GraphTool::Line,
            Icon::ShapeLine,
            tr!("tool-line"),
            tr!("tool-line", "hint"),
        ),
        (
            GraphTool::Arrow,
            Icon::ShapeArrow,
            tr!("tool-arrow"),
            tr!("tool-arrow", "hint"),
        ),
    ] {
        tool_entry(ui, kind, icon, label, hint);
    }
    ui.separator();
    // Le groupe zoom : l'outil et sa remise à zéro côte à côte.
    tool_entry(
        ui,
        GraphTool::ZoomBox,
        Icon::ZoomTool,
        tr!("tool-zoom"),
        tr!("tool-zoom", "hint"),
    );
    if tool_button(
        ui,
        Icon::ResetZoom,
        tr!("action-reset-zoom"),
        Some(hint(tr!("action-reset-zoom", "hint"))),
        enabled,
        false,
    ) {
        requests.push(ToolbarRequest::ResetZoom);
    }
    ui.separator();
    if tool_button(
        ui,
        Icon::Properties,
        tr!("menu-graph-properties"),
        Some(hint(tr!("menu-graph-properties", "hint"))),
        enabled,
        false,
    ) {
        if let Some(graph) = graph {
            requests.push(ToolbarRequest::Action(Box::new(Action::OpenPlotSettings {
                graph,
                tab: crate::state::PlotSettingsTab::Graph,
                axis: None,
            })));
        }
    }
}

/// Le format d'objet, à la fin de la barre « Outils » (#93) : couleur de trait, couleur de remplissage
/// (l'opacité se règle dans le sélecteur, `#RRGGBBAA`), épaisseur. Grisée tant
/// qu'aucun objet n'est sélectionné.
fn object_format_contents(ui: &mut egui::Ui, env: &BarEnv, requests: &mut Vec<ToolbarRequest>) {
    let state = env.object_format.clone();
    let enabled = state.is_some();
    let state = state.unwrap_or(ObjectFormatState {
        closed: false,
        line_color: String::new(),
        fill_color: String::new(),
        line_width: stokesforge_plot::ShapeSpec::DEFAULT_WIDTH,
    });
    let ctx = ui.ctx().clone();
    ui.add_enabled_ui(enabled, |ui| {
        let on_hover = |response: egui::Response, label: String, hint: Option<String>| {
            let mut tooltip = label;
            if let Some(hint) = hint {
                tooltip.push('\n');
                tooltip.push_str(&hint);
            }
            if !enabled {
                tooltip.push('\n');
                tooltip.push_str(&tr!("format-object-disabled"));
            }
            response.on_hover_text(tooltip)
        };
        let (project, user) = ctx
            .data(|data| data.get_temp(crate::color_picker::saved_colors_id()))
            .unwrap_or_default();
        let mut picker = PickerEnv {
            project,
            user,
            requests: Vec::new(),
        };

        // Trait.
        let mut line_color = state.line_color.clone();
        ui.scope(|ui| {
            crate::color_picker::color_icon_button(
                ui,
                (Icon::LineColor, &tr!("format-object-line-color")),
                "stokesforge_object_line_color_button",
                &mut line_color,
                ColorUsage::Curves,
                Some(&tr!("plot-color-auto-contrast")),
                &mut picker,
            );
        })
        .response
        .on_hover_text(format!(
            "{}\n{}",
            tr!("format-object-line-color"),
            tr!("format-object-line-color", "hint")
        ));
        if line_color != state.line_color {
            requests.push(ToolbarRequest::FormatObject(ObjectFormat::LineColor(
                line_color,
            )));
        }

        // Remplissage : grisé pour une ligne ou une flèche.
        let mut fill_color = state.fill_color.clone();
        ui.add_enabled_ui(state.closed || !enabled, |ui| {
            ui.scope(|ui| {
                crate::color_picker::color_icon_button(
                    ui,
                    (Icon::FillColor, &tr!("format-object-fill-color")),
                    "stokesforge_object_fill_color_button",
                    &mut fill_color,
                    ColorUsage::Curves,
                    Some(&tr!("format-object-no-fill")),
                    &mut picker,
                );
            })
            .response
            .on_hover_text(format!(
                "{}\n{}",
                tr!("format-object-fill-color"),
                tr!("format-object-fill-color", "hint")
            ));
        });
        if fill_color != state.fill_color {
            requests.push(ToolbarRequest::FormatObject(ObjectFormat::FillColor(
                fill_color,
            )));
        }
        for request in picker.requests {
            requests.push(ToolbarRequest::Action(Box::new(request.into_action())));
        }

        // Épaisseur : un champ numérique éditable avec ses valeurs courantes
        // (choisir 3 ou taper 2,5), N9.
        let mut width = state.line_width;
        let field = number_field(
            ui,
            "stokesforge_object_line_width",
            &mut width,
            &quantities::SHAPE_LINE_WIDTH,
        );
        on_hover(
            field.response.clone(),
            tr!("format-object-line-width"),
            Some(tr!("format-object-line-width", "hint")),
        );
        if field.changed {
            requests.push(ToolbarRequest::FormatObject(ObjectFormat::LineWidth(width)));
        }
    });
}

/// Où la barre de format s'est peinte à l'image précédente : un clic dessus ne
/// ferme pas l'éditeur sur place, sinon le bouton n'agirait jamais.
fn format_bar_id() -> egui::Id {
    egui::Id::new("stokesforge_text_format_bar_rect")
}

/// Le pointeur est-il sur la barre de format ? (cadre de l'image précédente)
pub fn over_format_bar(ctx: &egui::Context, pos: Pos2) -> bool {
    ctx.data(|data| data.get_temp::<Rect>(format_bar_id()))
        .is_some_and(|rect| rect.contains(pos))
}

/// Pourquoi les boutons de la barre de format sont grisés : sans graphe au
/// premier plan, le même conseil que la barre « Outils » (#108) ; avec un
/// graphe, ce qu'il faut sélectionner.
fn text_disabled_hint(env: &BarEnv) -> String {
    if matches!(env.tools, ToolsContext::Graph { .. }) {
        tr!("format-disabled")
    } else {
        tr!("toolbar-tools", "disabled")
    }
}

/// La barre de format du texte : polices, corps, couleur, puis les balises.
fn text_format_contents(ui: &mut egui::Ui, env: &BarEnv, requests: &mut Vec<ToolbarRequest>) {
    let enabled = env.text_format != FormatTarget::None;
    let ctx = ui.ctx().clone();
    ui.add_enabled_ui(enabled, |ui| {
        let on_hover = |response: egui::Response, label: String, hint: Option<String>| {
            let mut tooltip = label;
            if let Some(hint) = hint {
                tooltip.push('\n');
                tooltip.push_str(&hint);
            }
            if !enabled {
                tooltip.push('\n');
                tooltip.push_str(&text_disabled_hint(env));
            }
            response.on_hover_text(tooltip)
        };

        // Police.
        let rich = matches!(env.text_format, FormatTarget::Field | FormatTarget::Block);
        let chosen = egui::ComboBox::from_id_salt("stokesforge_format_font")
            .selected_text(
                env.text_style
                    .family
                    .clone()
                    .unwrap_or_else(|| tr!("format-font")),
            )
            .width(110.0)
            .show_ui(ui, |ui| {
                let mut chosen = None;
                let mut shown_system_title = false;
                if ui
                    .selectable_label(env.text_style.auto_family, tr!("format-font-default"))
                    .clicked()
                {
                    chosen = Some(String::new());
                }
                ui.separator();
                ui.label(egui::RichText::new(tr!("format-font-embedded")).weak());
                for family in stokesforge_render_egui::available_families() {
                    if !family.embedded && !shown_system_title {
                        ui.separator();
                        ui.label(egui::RichText::new(tr!("format-font-system")).weak());
                        shown_system_title = true;
                    }
                    if ui
                        .selectable_label(
                            !env.text_style.auto_family
                                && env.text_style.family.as_ref() == Some(&family.name),
                            &family.name,
                        )
                        .clicked()
                    {
                        chosen = Some(family.name.clone());
                    }
                }
                chosen
            });
        on_hover(
            chosen.response.clone(),
            tr!("format-font"),
            Some(tr!("format-font", "hint")),
        );
        if let Some(name) = chosen.inner.flatten() {
            requests.push(ToolbarRequest::FormatText(TextFormat::Font(name)));
        }

        // Corps : liste éditable avec « Auto » (choisir 12 ou taper 15, N9 ; vider
        // le champ ou taper `auto` rend le corps automatique, C4).
        let mut size = if env.text_style.auto_size {
            None
        } else {
            env.text_style.size
        };
        let hint = if env.text_style.auto_size {
            tr!("common-auto")
        } else {
            tr!("format-size")
        };
        let field = auto_number_field(
            ui,
            "stokesforge_format_size",
            &mut size,
            &quantities::TEXT_SIZE,
            &hint,
        );
        on_hover(
            field.response.clone(),
            tr!("format-size"),
            Some(tr!("format-size", "hint")),
        );
        if field.changed {
            requests.push(ToolbarRequest::FormatText(match size {
                Some(size) => TextFormat::Size(size),
                None => TextFormat::AutoSize,
            }));
        }

        // #99 : la couleur vient de la zone sélectionnée. Seul un champ en
        // édition garde un brouillon jusqu'à la fin du geste : le poser à
        // chaque caractère empilerait des balises Typst dans le passage.
        let draft_id = egui::Id::new("stokesforge_text_color_picker_draft");
        let mut color = env.text_style.color.clone().unwrap_or_default();
        if env.text_format == FormatTarget::Field {
            if let Some((source, draft)) =
                ctx.data(|data| data.get_temp::<(Option<egui::Id>, String)>(draft_id))
            {
                if source == env.text_style.source {
                    color = draft;
                }
            }
        } else {
            ctx.data_mut(|data| data.remove::<(Option<egui::Id>, String)>(draft_id));
        }
        let before = color.clone();
        let (project, user) = ctx
            .data(|data| data.get_temp(crate::color_picker::saved_colors_id()))
            .unwrap_or_default();
        let mut picker = PickerEnv {
            project,
            user,
            requests: Vec::new(),
        };
        // Aussi sur une zone sélectionnée (titre, axe, légende) : sa couleur
        // entière, pas un passage (#90).
        let chosen = ui.add_enabled_ui(enabled, |ui| {
            crate::color_picker::color_icon_button(
                ui,
                (Icon::TextColor, &tr!("format-color")),
                "stokesforge_format_color_button",
                &mut color,
                ColorUsage::Text,
                Some(&tr!("plot-color-auto-contrast")),
                &mut picker,
            )
        });
        let committed = chosen.inner;
        chosen.response.on_hover_text(tr!("format-color"));
        for request in picker.requests {
            requests.push(ToolbarRequest::Action(Box::new(request.into_action())));
        }
        let field = env.text_format == FormatTarget::Field;
        if field {
            ctx.data_mut(|data| {
                if committed {
                    data.remove::<(Option<egui::Id>, String)>(draft_id);
                } else {
                    data.insert_temp(draft_id, (env.text_style.source, color.clone()));
                }
            });
        }
        if (field && committed)
            || (!field && (color != before || (committed && env.text_style.color.is_none())))
        {
            requests.push(ToolbarRequest::FormatText(TextFormat::Color(color)));
        }

        ui.separator();
        for (icon, align, label) in [
            (
                Icon::AlignLeft,
                TextAlign::Left,
                tr!("plot-text-align-left"),
            ),
            (
                Icon::AlignCenter,
                TextAlign::Center,
                tr!("plot-text-align-center"),
            ),
            (
                Icon::AlignRight,
                TextAlign::Right,
                tr!("plot-text-align-right"),
            ),
        ] {
            let hint = tr!("plot-text-align", "hint");
            let hint = if enabled {
                hint
            } else {
                format!("{hint}\n{}", text_disabled_hint(env))
            };
            let current = env.text_style.align == Some(align);
            if tool_button(ui, icon, label, Some(hint), enabled, current) {
                requests.push(ToolbarRequest::FormatText(TextFormat::Align(align)));
            }
        }
        ui.separator();
        for (icon, format, label, hint) in [
            (
                Icon::Bold,
                TextFormat::Bold,
                tr!("format-bold"),
                tr!("format-bold", "hint"),
            ),
            (
                Icon::Italic,
                TextFormat::Italic,
                tr!("format-italic"),
                tr!("format-italic", "hint"),
            ),
            (
                Icon::Subscript,
                TextFormat::Subscript,
                tr!("format-subscript"),
                tr!("format-subscript", "hint"),
            ),
            (
                Icon::Superscript,
                TextFormat::Superscript,
                tr!("format-superscript"),
                tr!("format-superscript", "hint"),
            ),
            (
                Icon::Formula,
                TextFormat::Formula,
                tr!("format-formula"),
                tr!("format-formula", "hint"),
            ),
        ] {
            // Gras et italique agissent aussi sur tout un titre sélectionné (#243).
            let usable = rich
                || (env.text_format == FormatTarget::Zone
                    && matches!(format, TextFormat::Bold | TextFormat::Italic));
            let hint = if usable {
                hint
            } else {
                format!("{hint}\n{}", text_disabled_hint(env))
            };
            if tool_button(ui, icon, label, Some(hint), usable, false) {
                requests.push(ToolbarRequest::FormatText(format));
            }
        }
    });
}

/// La visibilité des barres, et le dernier changement à enregistrer.
#[derive(Debug, Default)]
pub struct ToolbarState {
    layout: ToolbarLayout,
    changed: bool,
}

impl ToolbarState {
    pub fn new(layout: ToolbarLayout) -> Self {
        Self {
            layout,
            changed: false,
        }
    }

    pub fn layout(&self) -> &ToolbarLayout {
        &self.layout
    }

    pub fn set_visible(&mut self, id: ToolbarId, visible: bool) {
        if self.layout.is_visible(id) != visible {
            self.layout.set_visible(id, visible);
            self.changed = true;
        }
    }

    /// Vrai une fois après chaque changement de visibilité : l'appelant
    /// l'enregistre dans les préférences.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Peint la zone du haut (panneau sous la barre de menu) : une ligne par
    /// rangée de [`ToolbarLayout::rows`].
    pub fn show(&mut self, ui: &mut egui::Ui, env: &BarEnv, requests: &mut Vec<ToolbarRequest>) {
        let ctx = ui.ctx().clone();
        let rows = self.layout.rows();
        if rows.is_empty() {
            return;
        }
        egui::Panel::top("stokesforge_toolbars").show(ui, |ui| {
            for row in &rows {
                ui.horizontal(|ui| {
                    for &id in row {
                        let rect = ui
                            .horizontal(|ui| bar_contents(ui, id, env, requests))
                            .response
                            .rect;
                        remember_format_bar(&ctx, id, rect);
                    }
                });
            }
        });
    }
}

fn remember_format_bar(ctx: &egui::Context, id: ToolbarId, rect: Rect) {
    if id == ToolbarId::Text {
        ctx.data_mut(|data| data.insert_temp(format_bar_id(), rect));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn par_defaut_toutes_les_barres_sont_visibles_a_leur_place_fixe() {
        let layout = ToolbarLayout::default();
        assert_eq!(
            layout.rows(),
            vec![
                vec![ToolbarId::File, ToolbarId::Edit, ToolbarId::Tools],
                vec![ToolbarId::Text]
            ]
        );
    }

    /// Sans graphe au premier plan, la barre `Text` est peinte (cadre mémorisé
    /// pour `over_format_bar`) et ne pousse aucune demande : boutons grisés.
    #[test]
    fn la_barre_du_texte_se_peint_sans_graphe_boutons_grises() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let mut state = ToolbarState::default();
        let mut requests = Vec::new();
        let screen = Rect::from_min_size(Pos2::ZERO, egui::Vec2::new(1200.0, 600.0));
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| state.show(ui, &BarEnv::default(), &mut requests),
            );
            output.textures_delta.clear();
        }
        let rect = ctx
            .data(|data| data.get_temp::<Rect>(format_bar_id()))
            .expect("la barre Text ne s'est pas peinte");
        assert!(rect.width() > 0.0 && rect.height() > 0.0);
        assert!(over_format_bar(&ctx, rect.center()));
        assert!(requests.is_empty());
        assert_eq!(BarEnv::default().text_format, FormatTarget::None);
    }

    #[test]
    fn une_ancienne_disposition_avec_la_barre_objet_se_relit_sans_elle() {
        // Les préférences d'avant la fusion (#93) portent une entrée « object » :
        // elle est ignorée, les autres barres gardent leur visibilité.
        let value = serde_json::json!({ "entries": [
            { "id": "object", "visible": false },
            { "id": "tools", "visible": true },
        ]});
        let layout = ToolbarLayout::from_json(&value);
        assert!(layout.is_visible(ToolbarId::Tools));
        assert_eq!(layout.entries.len(), ToolbarId::ALL.len());
    }

    /// #108 : un `preferences.json` d'avant, qui porte `placement` (rangé ou
    /// flottant), se lit sans erreur ; le champ est ignoré, la visibilité gardée.
    #[test]
    fn l_ancien_champ_placement_est_ignore_la_visibilite_est_gardee() {
        let value = serde_json::json!({ "entries": [
            { "id": "file", "visible": false,
              "placement": { "kind": "floating", "x": 120.0, "y": 80.0 } },
            { "id": "tools", "visible": true,
              "placement": { "kind": "docked", "row": 3 } },
            { "id": "text", "visible": true,
              "placement": { "kind": "docked", "row": 0 } },
            { "id": "object_manager", "visible": false,
              "placement": { "kind": "docked", "row": 0 } },
        ]});
        let layout = ToolbarLayout::from_json(&value);
        assert!(!layout.is_visible(ToolbarId::File));
        assert!(layout.is_visible(ToolbarId::Tools));
        assert_eq!(layout.entries.len(), ToolbarId::ALL.len());
        assert!(!layout.to_json().to_string().contains("object_manager"));
        // La place reste celle du logiciel, quoi que dise le fichier.
        assert_eq!(
            layout.rows(),
            vec![
                vec![ToolbarId::Edit, ToolbarId::Tools],
                vec![ToolbarId::Text]
            ]
        );
        assert!(!layout.to_json().to_string().contains("placement"));
    }

    #[test]
    fn une_barre_masquee_ne_se_peint_nulle_part() {
        let mut layout = ToolbarLayout::default();
        layout.set_visible(ToolbarId::Text, false);
        layout.set_visible(ToolbarId::File, false);
        assert_eq!(layout.rows(), vec![vec![ToolbarId::Edit, ToolbarId::Tools]]);
        layout.set_visible(ToolbarId::File, true);
        assert!(layout.is_visible(ToolbarId::File));
    }

    #[test]
    fn la_visibilite_survit_a_l_ecriture() {
        let mut layout = ToolbarLayout::default();
        layout.set_visible(ToolbarId::File, false);
        assert_eq!(ToolbarLayout::from_json(&layout.to_json()), layout);
    }

    #[test]
    fn une_disposition_illisible_ou_etrangere_retombe_sur_le_defaut() {
        for value in [
            serde_json::Value::Null,
            serde_json::json!("n'importe quoi"),
            serde_json::json!({ "entries": [{ "id": "barre_future", "visible": true }] }),
            serde_json::json!({ "entries": 3 }),
        ] {
            assert_eq!(ToolbarLayout::from_json(&value), ToolbarLayout::default());
        }
    }

    #[test]
    fn changer_la_visibilite_signale_un_enregistrement() {
        let mut state = ToolbarState::default();
        state.set_visible(ToolbarId::Text, true);
        assert!(!state.take_changed());
        state.set_visible(ToolbarId::Text, false);
        assert!(state.take_changed());
        assert!(!state.take_changed());
    }
}

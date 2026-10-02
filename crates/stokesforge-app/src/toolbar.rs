//! Barres d'outils : le socle (issue #60, chantier #59).
//!
//! # Pourquoi un conteneur maison
//!
//! Les barres se déplacent à la souris (décision de l'utilisateur). Aucun
//! conteneur egui n'y suffit : `egui::Panel` ne se déplace pas, une
//! `egui::Window` brute est interdite (`aucune_fenetre_n_est_une_egui_window_brute`)
//! et `show_window_in_layer` impose une barre de titre. Chaque barre porte donc
//! une **poignée** à gauche ; la saisir la détache dans une `egui::Area` du
//! calque Foreground qui suit le pointeur. Au relâchement :
//!
//! - sur la zone d'accueil (le panneau sous la barre de menu, plus une demi-rangée
//!   en dessous qui ouvre une nouvelle rangée) : la barre s'y range, à la rangée
//!   et au rang visés ;
//! - ailleurs : elle devient **flottante**, une `Area` à cadre léger posée là.
//!
//! Version 1 : accueil en haut seulement ; les bords gauche, droit et bas
//! viendront dans un ticket suivant.
//!
//! # Ce qui est pur, ce qui est egui
//!
//! La disposition ([`ToolbarLayout`]) et la décision de dépôt ([`drop_target`])
//! sont pures et testées sans fenêtre. [`ToolbarState`] n'est que la glu qui
//! les peint. Une barre ne contient que des boutons ([`ToolbarButton`]) qui
//! poussent les mêmes `Action` que les menus : jamais une seconde écriture.
//!
//! # Mémoire
//!
//! Emplacement et visibilité vivent dans les préférences de l'utilisateur
//! ([`crate::preferences`], champ `toolbars`), pas dans le `.stf` : la
//! disposition suit la personne, pas le projet. La lecture tolère un fichier
//! écrit par une autre version (barre inconnue ignorée, barre absente remise à
//! sa place par défaut).

use std::collections::HashMap;

use eframe::egui::{self, Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};

use crate::icons::{icon_image, Icon, IconRole};
use crate::state::Action;
use crate::tr;
use stokesforge_core::NodeId;
use stokesforge_plot::PlotType;
use stokesforge_render_egui::GraphTool;

use crate::color_picker::{ColorUsage, PickerEnv};
use crate::object_format::{ObjectFormat, LINE_WIDTHS};
use crate::text_format::TextFormat;

/// Les barres existantes — un enum fermé : ajouter une barre, c'est ajouter une
/// variante, son nom (`toolbar-*`) et son contenu dans [`bar_contents`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarId {
    File,
    /// La barre contextuelle (issue #62) : son contenu suit la fenêtre au
    /// premier plan.
    Tools,
    /// Le format du texte (issue #63) : propre au graphe au premier plan.
    Text,
    /// Le format de l'objet graphique sélectionné (issue #64).
    Object,
}

impl ToolbarId {
    pub const ALL: [ToolbarId; 4] = [
        ToolbarId::File,
        ToolbarId::Tools,
        ToolbarId::Text,
        ToolbarId::Object,
    ];

    /// Nom de la barre : entrée du menu View.
    pub fn label(self) -> String {
        match self {
            ToolbarId::File => tr!("toolbar-file"),
            ToolbarId::Tools => tr!("toolbar-tools"),
            ToolbarId::Text => tr!("toolbar-text"),
            ToolbarId::Object => tr!("toolbar-object"),
        }
    }

    /// Info-bulle de l'entrée du menu View.
    pub fn hint(self) -> String {
        match self {
            ToolbarId::File => tr!("toolbar-file", "hint"),
            ToolbarId::Tools => tr!("toolbar-tools", "hint"),
            ToolbarId::Text => tr!("toolbar-text", "hint"),
            ToolbarId::Object => tr!("toolbar-object", "hint"),
        }
    }
}

/// Où se trouve une barre.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Placement {
    /// Rangée `row` de la zone d'accueil (0 = la plus haute).
    Docked { row: usize },
    /// Détachée, coin haut-gauche en `(x, y)` points d'écran.
    Floating { x: f32, y: f32 },
}

/// Une barre, sa visibilité et son emplacement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: ToolbarId,
    pub visible: bool,
    pub placement: Placement,
}

/// Disposition de toutes les barres. L'ordre du vecteur est l'ordre d'une
/// rangée, de gauche à droite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolbarLayout {
    entries: Vec<Entry>,
}

impl Default for ToolbarLayout {
    fn default() -> Self {
        Self {
            entries: ToolbarId::ALL
                .iter()
                .map(|&id| Entry {
                    id,
                    visible: true,
                    // Les barres de format sont larges : elles ont leur propre rangée.
                    placement: Placement::Docked {
                        row: usize::from(matches!(id, ToolbarId::Text | ToolbarId::Object)),
                    },
                })
                .collect(),
        }
    }
}

/// Où tombe une barre lâchée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropTarget {
    /// Dans la zone d'accueil : rangée visée (== nombre de rangées : nouvelle
    /// rangée) et rang parmi les barres de cette rangée.
    Docked { row: usize, index: usize },
    /// Hors de la zone d'accueil.
    Floating,
}

/// Géométrie de la zone d'accueil au moment du dépôt.
#[derive(Debug, Clone)]
pub struct DockGeometry {
    /// Le panneau d'accueil.
    pub zone: Rect,
    /// Hauteur d'une rangée.
    pub row_height: f32,
    /// Pour chaque rangée visible, les cadres de ses barres (sans la barre
    /// qu'on déplace), de gauche à droite.
    pub rows: Vec<Vec<Rect>>,
}

/// Décide où tombe une barre lâchée en `pointer`. La zone d'accueil s'étend
/// d'une demi-rangée vers le bas : l'y lâcher ouvre une nouvelle rangée.
pub fn drop_target(pointer: Pos2, geometry: &DockGeometry) -> DropTarget {
    let zone = geometry
        .zone
        .with_max_y(geometry.zone.max.y + geometry.row_height / 2.0);
    if !zone.contains(pointer) || geometry.row_height <= 0.0 {
        return DropTarget::Floating;
    }
    let slot = ((pointer.y - geometry.zone.min.y) / geometry.row_height).floor();
    let row = (slot.max(0.0) as usize).min(geometry.rows.len());
    let index = geometry.rows.get(row).map_or(0, |bars| {
        bars.iter()
            .filter(|rect| rect.center().x < pointer.x)
            .count()
    });
    DropTarget::Docked { row, index }
}

impl Default for DockGeometry {
    fn default() -> Self {
        Self {
            zone: Rect::NOTHING,
            row_height: 0.0,
            rows: Vec::new(),
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

    pub fn placement(&self, id: ToolbarId) -> Option<Placement> {
        self.entry(id).map(|entry| entry.placement)
    }

    /// Les barres visibles rangées dans la zone d'accueil, rangée par rangée.
    /// Une rangée sans barre visible n'apparaît pas.
    pub fn docked_rows(&self) -> Vec<Vec<ToolbarId>> {
        self.docked_rows_where(|_| true)
    }

    /// Comme [`ToolbarLayout::docked_rows`], sans les barres que `shown`
    /// écarte (une barre contextuelle sans contenu pour la fenêtre active).
    pub fn docked_rows_where(&self, shown: impl Fn(ToolbarId) -> bool) -> Vec<Vec<ToolbarId>> {
        let mut rows: std::collections::BTreeMap<usize, Vec<ToolbarId>> = Default::default();
        for entry in self
            .entries
            .iter()
            .filter(|entry| entry.visible && shown(entry.id))
        {
            if let Placement::Docked { row } = entry.placement {
                rows.entry(row).or_default().push(entry.id);
            }
        }
        rows.into_values().collect()
    }

    /// Les barres visibles flottantes et leur position.
    pub fn floating(&self) -> Vec<(ToolbarId, Pos2)> {
        self.entries
            .iter()
            .filter(|entry| entry.visible)
            .filter_map(|entry| match entry.placement {
                Placement::Floating { x, y } => Some((entry.id, Pos2::new(x, y))),
                Placement::Docked { .. } => None,
            })
            .collect()
    }

    /// Range `id` dans la rangée visible `row` (à `index` parmi ses barres
    /// visibles). `row` égal au nombre de rangées visibles ouvre une rangée.
    pub fn dock(&mut self, id: ToolbarId, row: usize, index: usize) {
        self.dock_where(id, row, index, |_| true);
    }

    /// [`ToolbarLayout::dock`] quand `row` compte les rangées sans les barres
    /// que `shown` écarte — c'est ainsi que l'écran les a numérotées.
    pub fn dock_where(
        &mut self,
        id: ToolbarId,
        row: usize,
        index: usize,
        shown: impl Fn(ToolbarId) -> bool,
    ) {
        let Some(position) = self.entries.iter().position(|entry| entry.id == id) else {
            return;
        };
        // Les rangées se comptent avec la barre encore en place : c'est ainsi
        // que la zone d'accueil les a numérotées (le trou garde la place).
        let rows = self.docked_rows_where(|bar| bar == id || shown(bar));
        let mut moved = self.entries.remove(position);
        let neighbours: Vec<ToolbarId> = rows
            .get(row)
            .map(|bars| bars.iter().copied().filter(|&bar| bar != id).collect())
            .unwrap_or_default();
        let target_row = match (neighbours.first(), rows.get(row)) {
            (Some(&first), _) => self.row_number_of(first),
            // Seule dans sa rangée : elle y reste.
            (None, Some(_)) => self.row_number_of_entry(&moved),
            // Rangée neuve, après la dernière.
            (None, None) => usize::MAX / 2,
        };
        moved.visible = true;
        moved.placement = Placement::Docked { row: target_row };
        let at = match neighbours.get(index) {
            Some(&next) => self.entries.iter().position(|entry| entry.id == next),
            None => neighbours
                .last()
                .and_then(|&last| self.entries.iter().position(|entry| entry.id == last))
                .map(|last| last + 1),
        }
        .unwrap_or(self.entries.len());
        self.entries.insert(at, moved);
        self.compact_rows();
    }

    /// Détache `id` en `pos`.
    pub fn float(&mut self, id: ToolbarId, pos: Pos2) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.visible = true;
            entry.placement = Placement::Floating { x: pos.x, y: pos.y };
        }
        self.compact_rows();
    }

    fn row_number_of_entry(&self, entry: &Entry) -> usize {
        match entry.placement {
            Placement::Docked { row } => row,
            Placement::Floating { .. } => usize::MAX / 2,
        }
    }

    fn row_number_of(&self, id: ToolbarId) -> usize {
        match self.placement(id) {
            Some(Placement::Docked { row }) => row,
            _ => 0,
        }
    }

    /// Renumérote les rangées 0, 1, 2… sans trou.
    fn compact_rows(&mut self) {
        let mut used: Vec<usize> = self
            .entries
            .iter()
            .filter_map(|entry| match entry.placement {
                Placement::Docked { row } => Some(row),
                Placement::Floating { .. } => None,
            })
            .collect();
        used.sort_unstable();
        used.dedup();
        for entry in &mut self.entries {
            if let Placement::Docked { row } = &mut entry.placement {
                *row = used.binary_search(row).unwrap_or(0);
            }
        }
    }

    /// Lit une disposition écrite par cette version ou une autre : ce qui ne se
    /// lit pas est ignoré, toute barre manquante revient à sa place par défaut.
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
        layout.compact_rows();
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
            .on_hover_text(tooltip)
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

/// Ce que la barre contextuelle montre : décidé par la fenêtre au premier plan.
#[derive(Debug, Clone, Default)]
pub enum ToolsContext {
    /// Matrice, note, ou aucune fenêtre : pas de barre (jamais de bouton mort).
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
    pub new_project_enabled: bool,
    /// Un graphe est au premier plan (sinon *Export* est grisé, comme le menu
    /// Graph est absent).
    pub export_enabled: bool,
    pub shortcuts: ShortcutTexts,
    pub tools: ToolsContext,
    pub text_format: FormatTarget,
    /// L'objet graphique sélectionné, s'il y en a un (issue #64).
    pub object_format: Option<ObjectFormatState>,
}

impl BarEnv {
    /// Une barre sans contenu pour cette fenêtre ne se peint pas.
    fn shows(&self, id: ToolbarId) -> bool {
        match id {
            ToolbarId::File => true,
            ToolbarId::Tools => !matches!(self.tools, ToolsContext::None),
            ToolbarId::Text | ToolbarId::Object => {
                matches!(self.tools, ToolsContext::Graph { .. })
            }
        }
    }
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
        ToolbarId::Tools => tools_contents(ui, env, requests),
        ToolbarId::Text => text_format_contents(ui, env, requests),
        ToolbarId::Object => object_format_contents(ui, env, requests),
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
        .on_hover_text(tooltip)
        .clicked()
}

/// La barre contextuelle : chaque bouton répond à un geste que les menus ou
/// le menu contextuel offrent déjà.
fn tools_contents(ui: &mut egui::Ui, env: &BarEnv, requests: &mut Vec<ToolbarRequest>) {
    match &env.tools {
        ToolsContext::None => {}
        ToolsContext::Graph { graph, tool } => {
            let mut tool_entry = |ui: &mut egui::Ui, kind, icon, label: String, hint: String| {
                if tool_button(ui, icon, label, Some(hint), true, *tool == kind) {
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
                Some(tr!("action-reset-zoom", "hint")),
                true,
                false,
            ) {
                requests.push(ToolbarRequest::ResetZoom);
            }
            ui.separator();
            if tool_button(
                ui,
                Icon::Properties,
                tr!("menu-graph-properties"),
                Some(tr!("menu-graph-properties", "hint")),
                true,
                false,
            ) {
                requests.push(ToolbarRequest::Action(Box::new(Action::OpenPlotSettings {
                    graph: *graph,
                    tab: crate::state::PlotSettingsTab::Graph,
                    axis: None,
                })));
            }
        }
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
}

/// La barre de format d'objet : couleur de trait, couleur de remplissage
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
            .data(|data| data.get_temp(crate::app::saved_colors_id()))
            .unwrap_or_default();
        let mut picker = PickerEnv {
            project,
            user,
            requests: Vec::new(),
        };

        // Trait.
        let mut line_color = state.line_color.clone();
        ui.scope(|ui| {
            crate::color_picker::color_button(
                ui,
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
                crate::color_picker::color_button(
                    ui,
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

        // Épaisseur.
        let chosen = egui::ComboBox::from_id_salt("stokesforge_object_line_width")
            .selected_text(format!("{}", state.line_width))
            .width(54.0)
            .show_ui(ui, |ui| {
                let mut chosen = None;
                for width in LINE_WIDTHS {
                    if ui
                        .selectable_label(width == state.line_width, format!("{width}"))
                        .clicked()
                    {
                        chosen = Some(width);
                    }
                }
                chosen
            });
        on_hover(
            chosen.response.clone(),
            tr!("format-object-line-width"),
            Some(tr!("format-object-line-width", "hint")),
        );
        if let Some(width) = chosen.inner.flatten() {
            requests.push(ToolbarRequest::FormatObject(ObjectFormat::LineWidth(width)));
        }
    });
}

/// Corps proposés par la barre de format, en points.
const FORMAT_SIZES: [f64; 14] = [
    6.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0, 48.0,
];

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
                tooltip.push_str(&tr!("format-disabled"));
            }
            response.on_hover_text(tooltip)
        };

        // Police.
        let rich = matches!(env.text_format, FormatTarget::Field | FormatTarget::Block);
        let chosen = egui::ComboBox::from_id_salt("stokesforge_format_font")
            .selected_text(tr!("format-font"))
            .width(110.0)
            .show_ui(ui, |ui| {
                let mut chosen = None;
                let mut shown_system_title = false;
                if ui
                    .selectable_label(false, tr!("format-font-default"))
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
                    if ui.selectable_label(false, &family.name).clicked() {
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

        // Corps.
        let chosen = egui::ComboBox::from_id_salt("stokesforge_format_size")
            .selected_text(tr!("format-size"))
            .width(54.0)
            .show_ui(ui, |ui| {
                let mut chosen = None;
                for size in FORMAT_SIZES {
                    if ui.selectable_label(false, format!("{size}")).clicked() {
                        chosen = Some(size);
                    }
                }
                chosen
            });
        on_hover(
            chosen.response.clone(),
            tr!("format-size"),
            Some(tr!("format-size", "hint")),
        );
        if let Some(size) = chosen.inner.flatten() {
            requests.push(ToolbarRequest::FormatText(TextFormat::Size(size)));
        }

        // Couleur : la dernière choisie est mémorisée pour la durée de la session.
        let color_id = egui::Id::new("stokesforge_format_color");
        let mut color: String = ctx.data(|data| data.get_temp(color_id)).unwrap_or_default();
        let before = color.clone();
        let (project, user) = ctx
            .data(|data| data.get_temp(crate::app::saved_colors_id()))
            .unwrap_or_default();
        let mut picker = PickerEnv {
            project,
            user,
            requests: Vec::new(),
        };
        ui.add_enabled_ui(rich, |ui| {
            crate::color_picker::color_button(
                ui,
                "stokesforge_format_color_button",
                &mut color,
                ColorUsage::Text,
                Some(&tr!("plot-color-auto-contrast")),
                &mut picker,
            );
        })
        .response
        .on_hover_text(tr!("format-color"));
        for request in picker.requests {
            requests.push(ToolbarRequest::Action(Box::new(request.into_action())));
        }
        if color != before {
            ctx.data_mut(|data| data.insert_temp(color_id, color.clone()));
            requests.push(ToolbarRequest::FormatText(TextFormat::Color(color)));
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
            let hint = if rich {
                hint
            } else {
                format!("{hint}\n{}", tr!("format-disabled"))
            };
            if tool_button(ui, icon, label, Some(hint), rich, false) {
                requests.push(ToolbarRequest::FormatText(format));
            }
        }
    });
}

/// Barre en cours de déplacement : sa prise, relative à son coin haut-gauche.
#[derive(Debug, Clone, Copy)]
struct Drag {
    id: ToolbarId,
    grab: Vec2,
    size: Vec2,
}

/// La disposition, et le geste en cours.
#[derive(Debug, Default)]
pub struct ToolbarState {
    layout: ToolbarLayout,
    drag: Option<Drag>,
    /// Dernier cadre de chaque barre rangée, pour la prise et le dépôt.
    bar_rects: HashMap<ToolbarId, Rect>,
    geometry: DockGeometry,
    /// Les barres sans contenu pour la fenêtre active, à l'image précédente.
    not_shown: Vec<ToolbarId>,
    changed: bool,
}

impl ToolbarState {
    pub fn new(layout: ToolbarLayout) -> Self {
        Self {
            layout,
            ..Self::default()
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

    /// Vrai une fois après chaque changement de disposition : l'appelant
    /// l'enregistre dans les préférences.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Peint la zone d'accueil (panneau sous la barre de menu) et les barres
    /// flottantes, et mène le glisser-déposer.
    pub fn show(&mut self, ui: &mut egui::Ui, env: &BarEnv, requests: &mut Vec<ToolbarRequest>) {
        let ctx = ui.ctx().clone();
        self.finish_drag_if_released(&ctx);

        let rows = self.layout.docked_rows_where(|id| env.shows(id));
        self.not_shown = ToolbarId::ALL
            .into_iter()
            .filter(|&id| !env.shows(id))
            .collect();
        if !rows.is_empty() || self.drag.is_some() {
            let mut started: Option<Drag> = None;
            let mut row_rects: Vec<Vec<Rect>> = Vec::new();
            let panel = egui::Panel::top("stokesforge_toolbars").show(ui, |ui| {
                for row in &rows {
                    let mut rects = Vec::new();
                    ui.horizontal(|ui| {
                        for &id in row {
                            if self.drag.is_some_and(|drag| drag.id == id) {
                                // Le trou garde la place : le panneau ne saute pas
                                // sous le pointeur pendant le geste.
                                let size = self.drag.map_or(Vec2::ZERO, |drag| drag.size);
                                ui.allocate_space(size);
                                continue;
                            }
                            let (rect, grip) = bar(ui, id, env, requests);
                            self.bar_rects.insert(id, rect);
                            remember_format_bar(&ctx, id, rect);
                            rects.push(rect);
                            if grip.drag_started() {
                                started = Some(self.grab(&ctx, id, rect));
                            }
                        }
                    });
                    row_rects.push(rects);
                }
                if rows.is_empty() {
                    ui.allocate_space(Vec2::new(0.0, EMPTY_ROW_HEIGHT));
                }
            });
            let zone = panel.response.rect;
            self.geometry = DockGeometry {
                zone,
                row_height: zone.height() / rows.len().max(1) as f32,
                // Une rangée vidée par le geste garde sa place dans le décompte.
                rows: row_rects,
            };
            if let Some(drag) = started {
                self.drag = Some(drag);
            }
        }

        for (id, pos) in self.layout.floating() {
            if !env.shows(id) || self.drag.is_some_and(|drag| drag.id == id) {
                continue;
            }
            let area = egui::Area::new(egui::Id::new(("stokesforge_toolbar", id)))
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| bar(ui, id, env, requests))
                });
            let (rect, grip) = area.inner.inner;
            self.bar_rects.insert(id, rect);
            remember_format_bar(&ctx, id, rect);
            if grip.drag_started() {
                self.drag = Some(self.grab(&ctx, id, rect));
            }
        }

        self.paint_drag(&ctx);
    }

    fn grab(&self, ctx: &egui::Context, id: ToolbarId, rect: Rect) -> Drag {
        let pointer = ctx.input(|input| input.pointer.latest_pos());
        Drag {
            id,
            grab: pointer.map_or(Vec2::ZERO, |pointer| pointer - rect.min),
            size: rect.size(),
        }
    }

    /// À l'image qui suit le relâchement : range ou détache la barre.
    fn finish_drag_if_released(&mut self, ctx: &egui::Context) {
        let Some(drag) = self.drag else { return };
        let (down, escape, pointer) = ctx.input(|input| {
            (
                input.pointer.primary_down(),
                input.key_pressed(egui::Key::Escape),
                input.pointer.latest_pos(),
            )
        });
        if escape {
            self.drag = None;
            return;
        }
        if down {
            return;
        }
        self.drag = None;
        let Some(pointer) = pointer else { return };
        match drop_target(pointer, &self.geometry) {
            DropTarget::Docked { row, index } => {
                let not_shown = self.not_shown.clone();
                self.layout
                    .dock_where(drag.id, row, index, |bar| !not_shown.contains(&bar));
            }
            DropTarget::Floating => {
                let screen = ctx.content_rect();
                let top_left = pointer - drag.grab;
                let pos = Pos2::new(
                    top_left.x.clamp(0.0, (screen.max.x - 40.0).max(0.0)),
                    top_left.y.clamp(0.0, (screen.max.y - 30.0).max(0.0)),
                );
                self.layout.float(drag.id, pos);
            }
        }
        self.changed = true;
    }

    /// La barre saisie suit le pointeur ; la zone d'accueil s'éclaire quand le
    /// dépôt s'y ferait.
    fn paint_drag(&self, ctx: &egui::Context) {
        let Some(drag) = self.drag else { return };
        let Some(pointer) = ctx.input(|input| input.pointer.latest_pos()) else {
            return;
        };
        if matches!(
            drop_target(pointer, &self.geometry),
            DropTarget::Docked { .. }
        ) {
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("toolbar_zone"));
            ctx.layer_painter(layer).rect_stroke(
                self.geometry.zone,
                2.0,
                ctx.global_style().visuals.selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
        let mut sink = Vec::new();
        egui::Area::new(egui::Id::new("stokesforge_toolbar_drag"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer - drag.grab)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(0.85);
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.add_enabled_ui(false, |ui| bar(ui, drag.id, &BarEnv::default(), &mut sink));
                });
            });
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
}

fn remember_format_bar(ctx: &egui::Context, id: ToolbarId, rect: Rect) {
    if id == ToolbarId::Text {
        ctx.data_mut(|data| data.insert_temp(format_bar_id(), rect));
    }
}

/// Hauteur d'une rangée vide, pendant un glisser sans aucune barre rangée.
const EMPTY_ROW_HEIGHT: f32 = 24.0;

/// Une barre : sa poignée puis son contenu. Rend son cadre et la réponse de la
/// poignée.
fn bar(
    ui: &mut egui::Ui,
    id: ToolbarId,
    env: &BarEnv,
    requests: &mut Vec<ToolbarRequest>,
) -> (Rect, egui::Response) {
    let inner = ui.horizontal(|ui| {
        let grip = grip(ui);
        bar_contents(ui, id, env, requests);
        grip
    });
    (inner.response.rect, inner.inner)
}

/// La poignée de déplacement : deux colonnes de points, curseur « main ».
fn grip(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(10.0, 20.0), egui::Sense::drag());
    let color = if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    for column in [-2.0, 2.0] {
        for row in [-5.0, 0.0, 5.0] {
            ui.painter()
                .circle_filled(rect.center() + Vec2::new(column, row), 1.0, color);
        }
    }
    response
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text(tr!("toolbar-move"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    fn geometry() -> DockGeometry {
        DockGeometry {
            zone: rect(0.0, 20.0, 400.0, 70.0),
            row_height: 25.0,
            rows: vec![
                vec![rect(0.0, 20.0, 100.0, 45.0), rect(100.0, 20.0, 200.0, 45.0)],
                vec![rect(0.0, 45.0, 100.0, 70.0)],
            ],
        }
    }

    #[test]
    fn par_defaut_toutes_les_barres_sont_rangees_et_visibles() {
        let layout = ToolbarLayout::default();
        assert_eq!(
            layout.docked_rows(),
            vec![
                vec![ToolbarId::File, ToolbarId::Tools],
                vec![ToolbarId::Text, ToolbarId::Object]
            ]
        );
        assert!(layout.floating().is_empty());
    }

    #[test]
    fn lacher_dans_la_zone_vise_la_rangee_et_le_rang() {
        let g = geometry();
        assert_eq!(
            drop_target(Pos2::new(150.0, 30.0), &g),
            DropTarget::Docked { row: 0, index: 1 }
        );
        assert_eq!(
            drop_target(Pos2::new(300.0, 30.0), &g),
            DropTarget::Docked { row: 0, index: 2 }
        );
        assert_eq!(
            drop_target(Pos2::new(10.0, 60.0), &g),
            DropTarget::Docked { row: 1, index: 0 }
        );
    }

    #[test]
    fn lacher_sous_la_derniere_rangee_en_ouvre_une_et_plus_bas_detache() {
        let g = geometry();
        assert_eq!(
            drop_target(Pos2::new(10.0, 78.0), &g),
            DropTarget::Docked { row: 2, index: 0 }
        );
        assert_eq!(drop_target(Pos2::new(10.0, 90.0), &g), DropTarget::Floating);
        assert_eq!(drop_target(Pos2::new(10.0, 5.0), &g), DropTarget::Floating);
    }

    #[test]
    fn detacher_puis_reranger_garde_la_barre_visible_et_compacte_les_rangees() {
        let mut layout = ToolbarLayout::default();
        layout.float(ToolbarId::File, Pos2::new(120.0, 80.0));
        assert_eq!(
            layout.docked_rows(),
            vec![
                vec![ToolbarId::Tools],
                vec![ToolbarId::Text, ToolbarId::Object]
            ]
        );
        assert_eq!(
            layout.floating(),
            vec![(ToolbarId::File, Pos2::new(120.0, 80.0))]
        );
        // Une rangée « 7 » inexistante ouvre une nouvelle rangée, numérotée sans trou.
        layout.dock(ToolbarId::File, 7, 0);
        assert_eq!(
            layout.docked_rows(),
            vec![
                vec![ToolbarId::Tools],
                vec![ToolbarId::Text, ToolbarId::Object],
                vec![ToolbarId::File]
            ]
        );
        // Rangée 0, avant la barre qui s'y trouve.
        layout.dock(ToolbarId::File, 0, 0);
        assert_eq!(
            layout.docked_rows(),
            vec![
                vec![ToolbarId::File, ToolbarId::Tools],
                vec![ToolbarId::Text, ToolbarId::Object]
            ]
        );
    }

    #[test]
    fn les_rangees_se_comptent_sans_les_barres_sans_contenu() {
        let mut layout = ToolbarLayout::default();
        layout.set_visible(ToolbarId::Text, false);
        layout.set_visible(ToolbarId::Object, false);
        // Tools seule en rangée 0, File en rangée 1.
        layout.dock(ToolbarId::Tools, 0, 0);
        layout.dock(ToolbarId::File, 1, 0);
        assert_eq!(
            layout.docked_rows(),
            vec![vec![ToolbarId::Tools], vec![ToolbarId::File]]
        );
        // Tools n'a rien à montrer : File est alors la rangée 0 à l'écran.
        let not_tools = |id| id != ToolbarId::Tools;
        assert_eq!(
            layout.docked_rows_where(not_tools),
            vec![vec![ToolbarId::File]]
        );
        layout.dock_where(ToolbarId::File, 0, 0, not_tools);
        assert_eq!(
            layout.docked_rows(),
            vec![vec![ToolbarId::Tools], vec![ToolbarId::File]]
        );
    }

    #[test]
    fn une_barre_masquee_ne_se_peint_nulle_part() {
        let mut layout = ToolbarLayout::default();
        layout.set_visible(ToolbarId::Text, false);
        layout.set_visible(ToolbarId::Object, false);
        layout.set_visible(ToolbarId::File, false);
        assert_eq!(layout.docked_rows(), vec![vec![ToolbarId::Tools]]);
        layout.set_visible(ToolbarId::File, true);
        assert!(layout.is_visible(ToolbarId::File));
    }

    #[test]
    fn la_disposition_survit_a_l_ecriture() {
        let mut layout = ToolbarLayout::default();
        layout.float(ToolbarId::File, Pos2::new(5.0, 6.0));
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

    /// Le geste complet, sans fenêtre : saisir la poignée, tirer hors de la zone
    /// d'accueil, relâcher — la barre devient flottante, et le changement est
    /// signalé pour l'enregistrement.
    #[test]
    fn tirer_la_poignee_hors_de_la_zone_detache_la_barre() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let mut state = ToolbarState::default();
        let mut requests = Vec::new();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        macro_rules! frame {
            ($events:expr) => {{
                let input = egui::RawInput {
                    screen_rect: Some(screen),
                    events: $events,
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    state.show(ui, &BarEnv::default(), &mut requests)
                });
                output.textures_delta.clear();
            }};
        }
        let button = egui::PointerButton::Primary;
        let at = |x: f32, y: f32| egui::Event::PointerMoved(Pos2::new(x, y));
        frame!(vec![at(5.0, 5.0)]);
        frame!(vec![at(5.0, 5.0)]);
        // La poignée est le premier élément de la barre, en haut à gauche.
        let grip = Pos2::new(12.0, 14.0);
        frame!(vec![at(grip.x, grip.y)]);
        frame!(vec![egui::Event::PointerButton {
            pos: grip,
            button,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame!(vec![at(grip.x + 4.0, grip.y + 4.0)]);
        frame!(vec![at(300.0, 400.0)]);
        assert!(state.drag.is_some(), "la poignée n'a pas été saisie");
        frame!(vec![egui::Event::PointerButton {
            pos: Pos2::new(300.0, 400.0),
            button,
            pressed: false,
            modifiers: Default::default(),
        }]);
        frame!(vec![]);
        assert!(state.drag.is_none());
        assert!(matches!(
            state.layout().placement(ToolbarId::File),
            Some(Placement::Floating { .. })
        ));
        assert!(state.take_changed());
    }
}

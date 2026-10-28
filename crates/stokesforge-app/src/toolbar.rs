//! Toolbars (issues #60, #108, work package #59).
//!
//! # Fixed bars
//!
//! The content of the bars is fixed in the software, and so is their place:
//! the top area, under the menu bar, row by row ([`ToolbarId::row`]).
//! No handle, no drag and drop, no floating bar, no
//! vertical bar and no narrow-window logic (#108). The area is an
//! `egui::Panel::top`: neither a raw `egui::Window` (forbidden,
//! `no_window_is_a_raw_egui_window`) nor `show_window_in_layer`.
//!
//! The default layout is hard-coded: row 0 = `File`, `Tools`;
//! row 1 = `Text`, wide, alone on its row. The Object Manager panel
//! is controlled by View and its header (#100): its bar is removed (#109).
//!
//! # What is pure, what is egui
//!
//! [`ToolbarLayout`] (the visibility per bar) is pure and tested without a window.
//! [`ToolbarState`] is only the glue that paints. A bar contains only
//! buttons ([`ToolbarButton`]) that push the same `Action`s as the menus:
//! never a second implementation. The `Tools` and `Text` bars are always
//! painted: with no window to serve, their buttons are greyed out, with a
//! tooltip that says how to use them.
//!
//! # Memory
//!
//! Only the **visibility** of each bar (View menu) lives in the user's
//! preferences ([`stokesforge_app_state::preferences`], field `toolbars`), not in the
//! `.stf`. Reading tolerates a file written by another version: unknown
//! bar ignored, missing bar visible, old `placement` field ignored.

use eframe::egui::{self, Pos2, Rect};
use serde::{Deserialize, Serialize};

use crate::icons::{icon_image, Icon, IconRole};
use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;
use stokesforge_plot::{PlotType, TextAlign};
use stokesforge_render_egui::GraphTool;

use crate::color_picker::{ColorUsage, PickerEnv};
use crate::number_field::{auto_number_field, number_field};
use stokesforge_app_state::object_format::ObjectFormat;
use stokesforge_app_state::quantities;
use stokesforge_app_state::text_format::TextFormat;

/// The existing bars — a closed enum: adding a bar means adding a
/// variant, its name (`toolbar-*`) and its content in [`bar_contents`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarId {
    File,
    /// The contextual bar (issue #62): its content follows the foreground
    /// window. Always shown (#93); it also carries the format of the
    /// selected graphical object (issue #64), merged into it.
    Tools,
    /// The text format (issue #63): specific to the foreground graph.
    Text,
    /// Undo / Redo (issue #238).
    Edit,
}

impl ToolbarId {
    pub const ALL: [ToolbarId; 4] = [
        ToolbarId::File,
        ToolbarId::Edit,
        ToolbarId::Tools,
        ToolbarId::Text,
    ];

    /// Name of the bar: entry of the View menu.
    pub fn label(self) -> String {
        match self {
            ToolbarId::File => tr!("toolbar-file"),
            ToolbarId::Tools => tr!("toolbar-tools"),
            ToolbarId::Text => tr!("toolbar-text"),
            ToolbarId::Edit => tr!("toolbar-edit"),
        }
    }

    /// Tooltip of the View menu entry.
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
    /// Fixed row of the bar in the top area (0 = the highest). The
    /// format bars are wide: `Text` has its own row.
    pub const fn row(self) -> usize {
        match self {
            ToolbarId::Text => 1,
            ToolbarId::File | ToolbarId::Edit | ToolbarId::Tools => 0,
        }
    }
}

/// A bar and its visibility. An old `placement` field (before #108) is
/// ignored on reading.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: ToolbarId,
    pub visible: bool,
}

/// Visibility of all the bars. Their place is fixed ([`ToolbarId::row`],
/// order of [`ToolbarId::ALL`]).
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

    /// The visible bars, row by row, in the order of
    /// [`ToolbarId::ALL`]. A row with no visible bar does not appear.
    pub fn rows(&self) -> Vec<Vec<ToolbarId>> {
        let mut rows: std::collections::BTreeMap<usize, Vec<ToolbarId>> = Default::default();
        for id in ToolbarId::ALL {
            if self.is_visible(id) {
                rows.entry(id.row()).or_default().push(id);
            }
        }
        rows.into_values().collect()
    }

    /// Reads a layout written by this version or another: what cannot be
    /// read is ignored, any missing bar comes back visible.
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

/// A bar button: icon (common component), mandatory tooltip,
/// shortcut recalled. It returns `true` on click; the caller pushes the `Action`.
pub struct ToolbarButton {
    pub icon: Icon,
    pub role: IconRole,
    /// The label of the action (`action-*`).
    pub label: String,
    /// The `.hint` of the action, or the `.disabled` when it is greyed out.
    pub hint: Option<String>,
    /// The shortcut as the menus write it.
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

/// What a button asks of the application. The gestures that open a
/// file picker or a wizard, or that adjust a graph's view,
/// are not `Action`s: the application runs them through the same methods
/// as the menu entries.
#[derive(Debug, Clone)]
pub enum ToolbarRequest {
    /// A model or state action, the same as the menu's.
    Action(Box<Action>),
    Import,
    Open,
    SaveAs,
    ExportGraph,
    /// Changes the tool of the foreground graph.
    SetGraphTool(GraphTool),
    /// Resets the zoom of the foreground graph.
    ResetZoom,
    /// Formats the targeted text of the foreground graph.
    FormatText(TextFormat),
    /// Adjusts the selected graphical object of the foreground graph.
    FormatObject(ObjectFormat),
}

/// The shortcuts as the menus write them (recalled in the tooltip).
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

/// What the format bar acts on (issue #63).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FormatTarget {
    /// Nothing is selected or being edited: the buttons are greyed out.
    #[default]
    None,
    /// A field is being edited in place: the highlight, or the cursor.
    Field,
    /// A text is selected without being edited: the whole text.
    Block,
    /// A title, an axis title or the legend is selected (issue #77):
    /// the font and size of the area, nothing in its content.
    Zone,
}

/// The settings of the selected graphical object, as the object format bar
/// shows them (issue #64).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFormatState {
    /// True for a rectangle or an ellipse: only those can be filled.
    pub closed: bool,
    pub line_color: String,
    pub fill_color: String,
    pub line_width: f64,
}

/// Format read from the selection, never the last value chosen (#97).
/// An absent value leaves the label neutral for a non-uniform format.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFormatState {
    /// Identity of the area, to bound the picker's draft during an input.
    pub source: Option<egui::Id>,
    pub family: Option<String>,
    pub size: Option<f64>,
    /// `None` = local/mixed color; empty string = automatic.
    pub color: Option<String>,
    pub auto_family: bool,
    pub auto_size: bool,
    /// Alignment of the targeted box (#244); `None` with no target. Lights up the
    /// matching button.
    pub align: Option<TextAlign>,
}

impl TextFormatState {
    /// A passage set in the markup does not boil down to the format of the area.
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

/// What the contextual bar shows: decided by the foreground window.
#[derive(Debug, Clone, Default)]
pub enum ToolsContext {
    /// Matrix, note, or no window: the graph tools, greyed out.
    #[default]
    None,
    Graph {
        graph: NodeId,
        tool: GraphTool,
    },
    Book {
        book: NodeId,
        /// The displayed sheet, if the workbook has one.
        sheet: Option<NodeId>,
        /// The selected columns of this sheet.
        columns: Vec<NodeId>,
    },
}

/// What the bars read from the application, computed once per frame.
#[derive(Debug, Clone, Default)]
pub struct BarEnv {
    /// The root folder of the project: destination of *Add Folder* (#326).
    pub root: Option<NodeId>,
    /// Same guard as File > New Project and Ctrl+N.
    /// History label ("Undo Rename"); `None` = empty stack (greyed out).
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub new_project_enabled: bool,
    /// A script is running: *Save* is greyed out (#315).
    pub save_blocked: bool,
    /// A graph is in the foreground (otherwise *Export* is greyed out, as the
    /// Graph menu is absent).
    pub export_enabled: bool,
    pub shortcuts: ShortcutTexts,
    pub tools: ToolsContext,
    pub text_format: FormatTarget,
    pub text_style: TextFormatState,
    /// The selected graphical object, if there is one (issue #64).
    pub object_format: Option<ObjectFormatState>,
}

/// The content of a bar. Each button answers a gesture that the menus
/// already offer, with the same label (`action-*`).
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
                Icon::Folder,
                IconRole::Folder,
                tr!("action-add-folder"),
                Some(tr!("action-add-folder", "hint")),
                "",
                env.root.is_some(),
            ) {
                if let Some(parent) = env.root {
                    requests.push(ToolbarRequest::Action(Box::new(Action::AddFolder {
                        parent,
                    })));
                }
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
                env.save_blocked.then(|| tr!("disabled-script-running")),
                &shortcuts.save,
                !env.save_blocked,
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

/// Tool button (for bars with no shortcut to recall).
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

/// The contextual bar: each button answers a gesture that the menus or
/// the context menu already offer.
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

/// The graph tools. With no graph in the foreground (`graph` empty), they
/// stay shown but greyed out, with the tooltip that says how to
/// use them (#93): the bar does not change shape when the window changes.
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
    // Graphical objects (issue #76): one tool per kind, like
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
    // The zoom group: the tool and its reset side by side.
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
                tab: stokesforge_app_state::state::PlotSettingsTab::Graph,
                axis: None,
            })));
        }
    }
}

/// The object format, at the end of the "Tools" bar (#93): stroke color, fill color
/// (opacity is set in the picker, `#RRGGBBAA`), thickness. Greyed out while
/// no object is selected.
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

        // Stroke.
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

        // Fill: greyed out for a line or an arrow.
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

        // Thickness: an editable numeric field with its current values
        // (pick 3 or type 2.5), S-N9.
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

/// Where the format bar was painted on the previous frame: a click on it does not
/// close the in-place editor, otherwise the button would never act.
fn format_bar_id() -> egui::Id {
    egui::Id::new("stokesforge_text_format_bar_rect")
}

/// Is the pointer on the format bar? (frame rectangle of the previous frame)
pub fn over_format_bar(ctx: &egui::Context, pos: Pos2) -> bool {
    ctx.data(|data| data.get_temp::<Rect>(format_bar_id()))
        .is_some_and(|rect| rect.contains(pos))
}

/// The three text alignment buttons (#244), shared with the bar of
/// the in-place editor (#246). `current` lights up the one of the targeted box.
pub(crate) fn align_buttons(
    ui: &mut egui::Ui,
    current: Option<TextAlign>,
    disabled_hint: Option<String>,
    requests: &mut Vec<ToolbarRequest>,
) {
    let enabled = disabled_hint.is_none();
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
        let hint = match &disabled_hint {
            None => hint,
            Some(why) => format!("{hint}\n{why}"),
        };
        if tool_button(ui, icon, label, Some(hint), enabled, current == Some(align)) {
            requests.push(ToolbarRequest::FormatText(TextFormat::Align(align)));
        }
    }
}

/// Why the format bar buttons are greyed out: with no graph in the
/// foreground, the same advice as the "Tools" bar (#108); with a
/// graph, what has to be selected.
fn text_disabled_hint(env: &BarEnv) -> String {
    if matches!(env.tools, ToolsContext::Graph { .. }) {
        tr!("format-disabled")
    } else {
        tr!("toolbar-tools", "disabled")
    }
}

/// The text format bar: fonts, size, color, then the tags.
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

        // Font.
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

        // Size: editable list with "Auto" (pick 12 or type 15, S-N9; emptying
        // the field or typing `auto` makes the size automatic, S-C4).
        let mut size = if env.text_style.auto_size {
            None
        } else {
            env.text_style.size
        };
        let hint = if env.text_style.auto_size {
            // The size actually applied stays readable: "Auto (12)".
            match env.text_style.size {
                Some(effective) => format!(
                    "{} ({})",
                    tr!("common-auto"),
                    crate::number_field::format_value(
                        effective,
                        quantities::TEXT_FORMAT_SIZE.decimals
                    )
                ),
                None => tr!("common-auto"),
            }
        } else {
            tr!("format-size")
        };
        let field = auto_number_field(
            ui,
            "stokesforge_format_size",
            &mut size,
            &quantities::TEXT_FORMAT_SIZE,
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

        // #99: the color comes from the selected area. Only a field being
        // edited keeps a draft until the end of the gesture: setting it on
        // every character would stack Typst tags in the passage.
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
        // Also on a selected area (title, axis, legend): its whole color,
        // not a passage (#90).
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
        let why = (!enabled).then(|| text_disabled_hint(env));
        align_buttons(ui, env.text_style.align, why, requests);
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
                tr!("format-math"),
                tr!("format-math", "hint"),
            ),
        ] {
            // Bold and italic also act on a whole selected title (#243).
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

/// The visibility of the bars, and the last change to save.
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

    /// True once after each visibility change: the caller
    /// saves it in the preferences.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Paints the top area (panel under the menu bar): one line per
    /// row of [`ToolbarLayout::rows`].
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
    fn by_default_all_the_bars_are_visible_in_their_fixed_place() {
        let layout = ToolbarLayout::default();
        assert_eq!(
            layout.rows(),
            vec![
                vec![ToolbarId::File, ToolbarId::Edit, ToolbarId::Tools],
                vec![ToolbarId::Text]
            ]
        );
    }

    /// With no graph in the foreground, the `Text` bar is painted (frame remembered
    /// for `over_format_bar`) and pushes no request: buttons greyed out.
    #[test]
    fn the_text_bar_paints_without_a_graph_with_buttons_greyed_out() {
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
            .expect("the Text bar was not painted");
        assert!(rect.width() > 0.0 && rect.height() > 0.0);
        assert!(over_format_bar(&ctx, rect.center()));
        assert!(requests.is_empty());
        assert_eq!(BarEnv::default().text_format, FormatTarget::None);
    }

    /// Clicks, at each step along the *File* bar, and returns what the
    /// buttons ask: a folder's button is found by what it pushes.
    fn clicks_along_the_file_bar(env: &BarEnv) -> Vec<ToolbarRequest> {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let mut requests = Vec::new();
        let frame = |events: Vec<egui::Event>, requests: &mut Vec<ToolbarRequest>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| bar_contents(ui, ToolbarId::File, env, requests));
                },
            );
            output.textures_delta.clear();
        };
        frame(Vec::new(), &mut requests);
        frame(Vec::new(), &mut requests);
        requests.clear();
        for x in (0..700).step_by(4) {
            let pos = Pos2::new(x as f32, 16.0);
            for pressed in [None, Some(true), Some(false)] {
                let event = match pressed {
                    None => egui::Event::PointerMoved(pos),
                    Some(pressed) => egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                };
                frame(vec![event], &mut requests);
            }
        }
        requests
    }

    /// #326: the Add Folder button pushes `AddFolder` on the project root.
    #[test]
    fn the_add_folder_button_targets_the_root() {
        let root = NodeId::from_raw(1);
        let env = BarEnv {
            root: Some(root),
            ..Default::default()
        };
        let folder_actions: Vec<_> = clicks_along_the_file_bar(&env)
            .into_iter()
            .filter_map(|request| match request {
                ToolbarRequest::Action(action) => Some(*action),
                _ => None,
            })
            .filter(|action| matches!(action, Action::AddFolder { .. }))
            .collect();
        assert!(!folder_actions.is_empty(), "the button exists");
        assert!(folder_actions
            .iter()
            .all(|action| *action == Action::AddFolder { parent: root }));
    }

    /// Witness: with no known root, the button is greyed out and pushes nothing.
    #[test]
    fn the_add_folder_button_is_greyed_out_without_a_root() {
        let env = BarEnv::default();
        assert!(clicks_along_the_file_bar(&env)
            .into_iter()
            .all(|request| !matches!(
                request,
                ToolbarRequest::Action(action) if matches!(*action, Action::AddFolder { .. })
            )));
    }

    #[test]
    fn an_old_layout_with_the_object_bar_reads_back_without_it() {
        // Preferences from before the merge (#93) carry an "object" entry:
        // it is ignored, the other bars keep their visibility.
        let value = serde_json::json!({ "entries": [
            { "id": "object", "visible": false },
            { "id": "tools", "visible": true },
        ]});
        let layout = ToolbarLayout::from_json(&value);
        assert!(layout.is_visible(ToolbarId::Tools));
        assert_eq!(layout.entries.len(), ToolbarId::ALL.len());
    }

    /// #108: an earlier `preferences.json`, which carries `placement` (docked or
    /// floating), reads without error; the field is ignored, the visibility kept.
    #[test]
    fn the_old_placement_field_is_ignored_and_the_visibility_kept() {
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
        // The place stays the software's, whatever the file says.
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
    fn a_hidden_bar_is_painted_nowhere() {
        let mut layout = ToolbarLayout::default();
        layout.set_visible(ToolbarId::Text, false);
        layout.set_visible(ToolbarId::File, false);
        assert_eq!(layout.rows(), vec![vec![ToolbarId::Edit, ToolbarId::Tools]]);
        layout.set_visible(ToolbarId::File, true);
        assert!(layout.is_visible(ToolbarId::File));
    }

    #[test]
    fn the_visibility_survives_writing() {
        let mut layout = ToolbarLayout::default();
        layout.set_visible(ToolbarId::File, false);
        assert_eq!(ToolbarLayout::from_json(&layout.to_json()), layout);
    }

    #[test]
    fn an_unreadable_or_foreign_layout_falls_back_to_the_default() {
        for value in [
            serde_json::Value::Null,
            serde_json::json!("anything"),
            serde_json::json!({ "entries": [{ "id": "future_bar", "visible": true }] }),
            serde_json::json!({ "entries": 3 }),
        ] {
            assert_eq!(ToolbarLayout::from_json(&value), ToolbarLayout::default());
        }
    }

    #[test]
    fn changing_the_visibility_signals_a_save() {
        let mut state = ToolbarState::default();
        state.set_visible(ToolbarId::Text, true);
        assert!(!state.take_changed());
        state.set_visible(ToolbarId::Text, false);
        assert!(state.take_changed());
        assert!(!state.take_changed());
    }
}

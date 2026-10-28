//! The Project Manager tree: one piece of code for the fixed panel and the
//! detached OS window, on an already built tree
//! ([`stokesforge_app_state::state::ProjectTreeNode`]). What the clicks ask for
//! (open, close, select) accumulates in
//! [`PmInteraction`], applied by `StokesForgeApp` after painting.
//!
//! The title belongs to the shared `panel_header` frame, not to the tree:
//! docked or detached, no double header and the same gestures (#100).
//! A selection taken over by the active window must also move the Shift-click
//! anchor in both viewports: the selected background alone does not define the
//! next range after a click in the workspace (#154).
//! Open and Close live in an entry's context menu and apply to the whole
//! selection; the tree starts directly under the header.
//!
//! A selectable entry is a folder **or** a window ([`PmEntry`], #170 point 4).
//! A click on a folder selects it and, if it is closed, expands it; a click on
//! an open folder does not collapse it (a selection click must not make the
//! tree jump); the double-click and the triangle collapse it. Delete and
//! Rename act on it, a folder selection pastes **into** that folder. **Copying
//! and duplicating a folder are refused for now**: a local copy would keep its
//! references to the original workbooks (outside the folder); the menu entry is
//! grayed out with its reason, and `AppState` refuses the keyboard gesture with
//! the same sentence. To be taken up in a separate issue.
//!
//! **Keyboard navigation (#170, point 6).** With the panel focused
//! (`PmMemory::focus`, set by a click), ↑ and ↓ move the selection from one
//! **visible** row to the other (`PmMemory::visible`, collected while painting:
//! collapsed folders are skipped, unlike `order`, which serves the Shift
//! ranges), and Enter opens the selected window(s), expands or collapses the
//! folders. The arrow touches no window and not the history. The keys are
//! consumed here, before a workbook's grid (painted after the panel), which
//! reads the arrows without consuming them; the Shift and Alt+↓ variants, which
//! egui would confuse with ↑ and ↓ (it ignores extra Shift and Alt), are
//! consumed first and left without effect. The row that carries the keyboard
//! cursor has the focus outline of list rows (`list_row::paint_focus_outline`).

use std::collections::HashSet;

use eframe::egui;

use crate::icons::{icon_image, Icon, IconRole};
use stokesforge_app_state::state::{Action, ProjectTreeNode, WindowKind};
use stokesforge_app_state::tr;
use stokesforge_core::NodeId;

use super::inplace_edit;
use super::list_row::{
    list_row_focused, neighbour, raise_to_list_row_height, read_navigation, restore_row_height,
    with_list_row_height, NavKey,
};
use super::menu_sections::MenuSections;
use super::shortcuts::{shortcut_text_delete, SHORTCUT_DUPLICATE_OBJECT, SHORTCUT_RENAME_OBJECT};
use super::tab::{tab_for_window, tab_node, Tab};
use super::window_chrome::{panel_header, PanelPlacement, PANEL_STRIP_THICKNESS};
use super::StokesForgeApp;

/// What the Project Manager tree rendering accumulates or reads, through
/// [`project_manager_contents_from_tree`]/[`project_tree_node_ui`] — one single
/// bundle rather than five parameters, for the same two contexts as the rest of
/// the module (fixed panel and detached OS window, see the head doc of
/// [`StokesForgeApp::show_detached_project_manager`]).
pub(super) struct PmInteraction<'a> {
    pub(super) actions: &'a mut Vec<Action>,
    pub(super) root: NodeId,
    /// Tab opened or focused by a single click — see
    /// [`StokesForgeApp::open_tab`].
    pub(super) open_requests: &'a mut Vec<Tab>,
    /// Current selection in the tree — Ctrl/Cmd-click toggles the membership
    /// without opening the tab.
    pub(super) selection: &'a mut HashSet<PmEntry>,
    /// Tabs currently open (floating or detached): the other rows are dimmed
    /// in the tree (#327).
    pub(super) open: &'a HashSet<Tab>,
    /// Tabs that "Close" asks to close, through the same path as the window's
    /// cross (floating or detached).
    pub(super) close_requests: &'a mut Vec<Tab>,
}

/// The selection is shared with the viewport; the editor and the focus stay in
/// the memory of the viewport that receives the keyboard.
#[derive(Clone, Default)]
struct PmMemory {
    focus: bool,
    anchor: Option<PmEntry>,
    rename: Vec<NodeId>,
    renamed: Vec<(NodeId, String)>,
    text: Option<String>,
    /// Last single click on a window already alone in the selection: if it is
    /// not followed by a second click before [`SLOW_CLICK_SECONDS`], it is a
    /// slow click, hence a rename (S-K4, #224).
    slow_click: Option<(PmEntry, f64)>,
    /// The tree's folders and windows in reading order, collapsed folders
    /// included (#327): the Shift range and the acting ids depend on it.
    order: Vec<PmEntry>,
    /// Folders to expand on their next painting (#326): the parent of a
    /// folder just created must show its new child.
    reveal: Vec<NodeId>,
    /// The **visible** rows of the previous frame, in reading order: what ↑
    /// and ↓ walk through (a collapsed folder hides its children, unlike
    /// `order`).
    visible: Vec<PmEntry>,
    /// Folders that Enter must expand or collapse on their next painting.
    toggle: Vec<NodeId>,
    /// Row the keyboard has just reached: to make appear in the scroll area
    /// on its next painting.
    scroll_to: Option<PmEntry>,
}

/// A selectable entry of the Project Manager tree (#170, point 4): a folder,
/// or a window (its tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PmEntry {
    Folder(NodeId),
    Window(Tab),
}

impl PmEntry {
    /// The model node the entry designates.
    pub(super) fn node(self) -> NodeId {
        match self {
            PmEntry::Folder(id) => id,
            PmEntry::Window(tab) => tab_node(tab),
        }
    }

    fn is_folder(self) -> bool {
        matches!(self, PmEntry::Folder(_))
    }
}

/// The folders and windows of `tree`, in depth-first order, a folder before
/// its children.
fn tree_order(tree: &[ProjectTreeNode], out: &mut Vec<PmEntry>) {
    for node in tree {
        match node {
            ProjectTreeNode::Folder { id, children, .. } => {
                out.push(PmEntry::Folder(*id));
                tree_order(children, out);
            }
            ProjectTreeNode::Window { id, kind, .. } => {
                out.push(PmEntry::Window(tab_for_window(*id, *kind)));
            }
        }
    }
}

/// Where the clipboard is pasted: **into** the folder when the selection is
/// that single folder, at the root otherwise (as before #170).
pub(super) fn paste_parent(selection: &HashSet<PmEntry>, root: NodeId) -> NodeId {
    let mut entries = selection.iter();
    match (entries.next(), entries.next()) {
        (Some(PmEntry::Folder(id)), None) => *id,
        _ => root,
    }
}

/// The row that carries the keyboard cursor: the anchor of the last gesture if
/// it is selected, otherwise the only selected entry.
fn cursor_of(selection: &HashSet<PmEntry>, anchor: Option<PmEntry>) -> Option<PmEntry> {
    anchor
        .filter(|entry| selection.contains(entry))
        .or_else(|| {
            let mut entries = selection.iter();
            match (entries.next(), entries.next()) {
                (Some(entry), None) => Some(*entry),
                _ => None,
            }
        })
}

/// Notes that a row is visible in this frame (see `PmMemory::visible`).
fn note_visible(ctx: &egui::Context, memory_key: egui::Id, entry: PmEntry) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<PmMemory>(memory_key)
            .visible
            .push(entry);
    });
}

/// Does the selection contain a folder (copy refused, see the head doc)?
pub(super) fn selection_has_folder(selection: &HashSet<PmEntry>) -> bool {
    selection.iter().any(|entry| entry.is_folder())
}

pub(super) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("stokesforge_project_manager_window")
}

/// No editor can survive a replacement of the project's identity.
pub(super) fn forget_views(ctx: &egui::Context) {
    let paste_keys = [egui::ViewportId::ROOT, viewport_id()]
        .map(|viewport| egui::Id::new(("project_paste_parent", viewport)));
    ctx.data_mut(|data| {
        data.remove_by_type::<PmMemory>();
        for key in paste_keys {
            data.remove::<NodeId>(key);
        }
    });
}

fn memory_id(ctx: &egui::Context) -> egui::Id {
    egui::Id::new(("project_manager_gestures", ctx.viewport_id()))
}

/// An external activation replaces the anchor, without touching the focus nor
/// the panel's editor. Internal gestures keep their own anchor, notably when a
/// Shift range comes back to a single entry.
pub(super) fn follow_selection_anchor(ctx: &egui::Context, tab: Option<Tab>) {
    let anchor = tab.map(PmEntry::Window);
    ctx.data_mut(|data| {
        for viewport in [egui::ViewportId::ROOT, viewport_id()] {
            let key = egui::Id::new(("project_manager_gestures", viewport));
            let mut memory = data.get_temp::<PmMemory>(key).unwrap_or_default();
            memory.anchor = anchor;
            data.insert_temp(key, memory);
        }
    });
}

/// After *Add Folder* (#326): expands `parent` and opens the name field on
/// `folder`, in the panel that is on screen (`viewport`).
pub(super) fn begin_rename_of_new_folder(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
    folder: NodeId,
    parent: NodeId,
) {
    let key = egui::Id::new(("project_manager_gestures", viewport));
    ctx.data_mut(|data| {
        let mut memory = data.get_temp::<PmMemory>(key).unwrap_or_default();
        memory.rename = vec![folder];
        memory.renamed.clear();
        memory.text = None;
        memory.focus = true;
        memory.reveal.push(parent);
        data.insert_temp(key, memory);
    });
    ctx.request_repaint_of(viewport);
}

/// The names being edited in the panel of `viewport` (tests).
#[cfg(test)]
pub(super) fn renaming_in(ctx: &egui::Context, viewport: egui::ViewportId) -> Vec<NodeId> {
    let key = egui::Id::new(("project_manager_gestures", viewport));
    ctx.data(|data| data.get_temp::<PmMemory>(key))
        .map(|memory| memory.rename)
        .unwrap_or_default()
}

pub(super) fn has_focus(ctx: &egui::Context) -> bool {
    let memory_key = memory_id(ctx);
    ctx.data(|data| {
        data.get_temp::<PmMemory>(memory_key)
            .is_some_and(|m| m.focus || !m.rename.is_empty())
    })
}

fn selected_ids(selection: &HashSet<PmEntry>, order: &[PmEntry]) -> Vec<NodeId> {
    order
        .iter()
        .filter(|entry| selection.contains(entry))
        .map(|entry| entry.node())
        .collect()
}

/// The target of a Project Manager object menu: windows, or a folder (which
/// additionally offers Add Folder, and whose Copy and Duplicate are grayed out
/// for now, as for any selection that contains one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ObjectMenuTarget {
    Windows,
    Folder,
}

/// The content of the object menu (#157), **flat**: the right click shows it
/// as is, Edit › Selection wraps it. Sections of `docs/menus.md` —
/// clipboard (Copy · Paste), organize (Rename · Duplicate), delete.
/// `sections` is the calling menu's: the entries specific to the panel
/// (Open · Close) are painted before, under the same count of separators.
/// Pasting triggers a read request to egui-winit, hence also uses the Wayland
/// receiver. `has_folder`: the selection contains a folder, so Copy and
/// Duplicate are grayed out (folder copy refused, #170).
#[allow(clippy::too_many_arguments)]
pub(super) fn object_menu(
    ui: &mut egui::Ui,
    sections: &mut MenuSections,
    target: ObjectMenuTarget,
    ids: &[NodeId],
    parent: NodeId,
    has_folder: bool,
    actions: &mut Vec<Action>,
    rename_viewport: egui::ViewportId,
) {
    let memory_key = egui::Id::new(("project_manager_gestures", rename_viewport));
    let paste_key = egui::Id::new(("project_paste_parent", ui.ctx().viewport_id()));
    let enabled = !ids.is_empty();
    // Copy and duplicate: refused as soon as a folder is targeted (see the head doc).
    let has_folder = has_folder || target == ObjectMenuTarget::Folder;
    let copy_enabled = enabled && !has_folder;
    let copy_reason = || {
        if has_folder {
            tr!("disabled-copy-folder")
        } else {
            tr!("project-manager-selection-needed")
        }
    };
    if target == ObjectMenuTarget::Folder {
        // The only gesture that creates in a subfolder (#326, `docs/menus.md`).
        sections.section(ui, true, |ui| {
            if ui
                .button(tr!("action-add-folder"))
                .on_hover_text(tr!("action-add-folder", "hint"))
                .clicked()
            {
                actions.push(Action::AddFolder { parent });
                ui.close();
            }
        });
    }
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                copy_enabled,
                egui::Button::new(tr!("action-copy"))
                    .shortcut_text(ui.ctx().format_shortcut(&super::shortcuts::SHORTCUT_COPY)),
            )
            .on_disabled_hover_text(copy_reason())
            .clicked()
        {
            actions.push(Action::CopyObjects(ids.to_vec()));
            ui.close();
        }
        if ui.button(tr!("action-paste")).clicked() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(paste_key, parent);
            });
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(tr!("action-rename-project-object"))
                    .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_RENAME_OBJECT)),
            )
            .on_disabled_hover_text(tr!("project-manager-selection-needed"))
            .clicked()
        {
            ui.ctx().data_mut(|data| {
                let mut memory = data.get_temp::<PmMemory>(memory_key).unwrap_or_default();
                memory.rename = ids.to_vec();
                memory.renamed.clear();
                memory.text = None;
                memory.focus = true;
                data.insert_temp(memory_key, memory);
            });
            ui.ctx().request_repaint_of(rename_viewport);
            if rename_viewport != ui.ctx().viewport_id() {
                ui.ctx()
                    .send_viewport_cmd_to(rename_viewport, egui::ViewportCommand::Focus);
            }
            ui.close();
        }
        if ui
            .add_enabled(
                copy_enabled,
                egui::Button::new(tr!("action-duplicate-project-object"))
                    .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_DUPLICATE_OBJECT)),
            )
            .on_disabled_hover_text(copy_reason())
            .clicked()
        {
            actions.push(Action::DuplicateObjects(ids.to_vec()));
            ui.close();
        }
    });
    sections.section(ui, true, |ui| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(tr!("action-delete-project-object"))
                    .shortcut_text(shortcut_text_delete()),
            )
            .on_disabled_hover_text(tr!("project-manager-selection-needed"))
            .clicked()
        {
            actions.push(Action::DeleteObjects(ids.to_vec()));
            ui.close();
        }
    });
}

/// Delay of a slow click (S-K4): longer than egui's double-click interval, so
/// that a double-click (open) never opens the field.
const SLOW_CLICK_SECONDS: f64 = 0.5;

fn object_shortcuts(ui: &egui::Ui, interaction: &mut PmInteraction) {
    let ctx = ui.ctx();
    let memory_key = memory_id(ctx);
    let paste_key = egui::Id::new(("project_paste_parent", ctx.viewport_id()));
    let mut memory = ctx
        .data(|data| data.get_temp::<PmMemory>(memory_key))
        .unwrap_or_default();
    let pending = ctx.data_mut(|data| {
        let id = paste_key;
        let value = data.get_temp::<NodeId>(id);
        data.remove::<NodeId>(id);
        value
    });
    let ids = selected_ids(interaction.selection, &memory.order);
    // Slow click: the delay elapsed without a second click, the name is renamed like on F2.
    if let Some((entry, at)) = memory.slow_click {
        let now = ctx.input(|input| input.time);
        if now - at >= SLOW_CLICK_SECONDS {
            memory.slow_click = None;
            if memory.rename.is_empty() && interaction.selection.len() == 1 {
                memory.rename = ids.clone();
                memory.renamed.clear();
                memory.text = None;
                ctx.request_repaint();
            }
        } else if !interaction.selection.contains(&entry) {
            memory.slow_click = None;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                SLOW_CLICK_SECONDS - (now - at) + 0.02,
            ));
        }
    }
    if pending.is_some() || (memory.focus && memory.rename.is_empty() && !ctx.text_edit_focused()) {
        let parent =
            pending.unwrap_or_else(|| paste_parent(interaction.selection, interaction.root));
        let mut paste = None;
        let mut navigation = None;
        ctx.input_mut(|input| {
            if memory.focus {
                navigation = read_navigation(input);
            }
            if memory.focus && !ids.is_empty() {
                if input.consume_shortcut(&super::shortcuts::SHORTCUT_RENAME_OBJECT) {
                    memory.rename = ids.clone();
                    memory.renamed.clear();
                    memory.text = None;
                }
                if input.consume_shortcut(&super::shortcuts::SHORTCUT_DUPLICATE_OBJECT) {
                    interaction
                        .actions
                        .push(Action::DuplicateObjects(ids.clone()));
                }
                if input.consume_shortcut(&super::shortcuts::KEY_DELETE) {
                    interaction.actions.push(Action::DeleteObjects(ids.clone()));
                }
            }
            input.events.retain(|event| match event {
                egui::Event::Copy if memory.focus => {
                    interaction.actions.push(Action::CopyObjects(ids.clone()));
                    false
                }
                egui::Event::Paste(text) => {
                    paste = Some(text.clone());
                    false
                }
                _ => true,
            });
        });
        if let Some(text) = paste {
            interaction
                .actions
                .push(Action::PasteObjects { parent, text });
        } else if let Some(parent) = pending {
            ctx.data_mut(|data| data.insert_temp(paste_key, parent));
        }
        match navigation {
            Some(key @ (NavKey::Up | NavKey::Down)) => {
                let step = if key == NavKey::Up { -1 } else { 1 };
                let cursor = cursor_of(interaction.selection, memory.anchor);
                if let Some(entry) = neighbour(&memory.visible, cursor, step) {
                    interaction.selection.clear();
                    interaction.selection.insert(entry);
                    memory.anchor = Some(entry);
                    memory.slow_click = None;
                    memory.scroll_to = Some(entry);
                    ctx.request_repaint();
                }
            }
            Some(NavKey::Enter) => {
                // Like the menu's "Open": each selected window opens,
                // each folder toggles.
                for entry in memory
                    .order
                    .iter()
                    .filter(|e| interaction.selection.contains(e))
                {
                    match entry {
                        PmEntry::Window(tab) => interaction.open_requests.push(*tab),
                        PmEntry::Folder(id) => memory.toggle.push(*id),
                    }
                }
                ctx.request_repaint();
            }
            None => {}
        }
    }
    ctx.data_mut(|data| data.insert_temp(memory_key, memory));
}

/// The body of the Project Manager, from an already built tree rather than a
/// live `&AppState` — only the `show_viewport_deferred` callback of
/// [`StokesForgeApp::show_detached_project_manager`] needs this form: it
/// carries only a copy of the tree
/// ([`ProjectManagerViewModel`](crate::app::window_detach::ProjectManagerViewModel)),
/// never `AppState` itself. The fixed panel ([`StokesForgeApp::project_manager_panel`])
/// passes `&self.state.project_tree()` directly — same function, two
/// sources of tree.
pub(super) fn project_manager_contents_from_tree(
    ui: &mut egui::Ui,
    tree: &[ProjectTreeNode],
    interaction: &mut PmInteraction,
) {
    let memory_key = memory_id(ui.ctx());
    object_shortcuts(ui, interaction);
    ui.ctx().data_mut(|data| {
        let mut memory = data.get_temp::<PmMemory>(memory_key).unwrap_or_default();
        memory.order.clear();
        memory.visible.clear();
        tree_order(tree, &mut memory.order);
        // A deleted folder does not stay selected (a script, Ctrl+Z).
        interaction
            .selection
            .retain(|entry| !entry.is_folder() || memory.order.contains(entry));
        data.insert_temp(memory_key, memory);
    });
    let response = egui::ScrollArea::vertical().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        for node in tree {
            project_tree_node_ui(ui, node, tree, interaction);
        }
    });
    // A keyboard request is valid only for the next frame: a hidden folder
    // (collapsed under another) must not keep it in reserve.
    ui.ctx().data_mut(|data| {
        let memory = data.get_temp_mut_or_default::<PmMemory>(memory_key);
        memory.toggle.clear();
        memory.scroll_to = None;
    });
    if ui.input(|input| input.pointer.any_pressed()) {
        let inside = ui
            .input(|input| input.pointer.interact_pos())
            .is_some_and(|pos| response.inner_rect.contains(pos));
        ui.ctx().data_mut(|data| {
            let mut memory = data.get_temp::<PmMemory>(memory_key).unwrap_or_default();
            memory.focus = inside;
            data.insert_temp(memory_key, memory);
        });
    }
}

/// True if `id` is being renamed in this panel.
fn is_renaming(ctx: &egui::Context, memory_key: egui::Id, id: NodeId) -> bool {
    ctx.data(|data| data.get_temp::<PmMemory>(memory_key))
        .is_some_and(|m| m.rename.first() == Some(&id))
}

/// Why this name is refused (S-T4, red frame): empty, or already taken by
/// another child of the same folder.
fn rename_refusal(siblings: &[ProjectTreeNode], id: NodeId, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return Some(tr!("field-refused-name-empty"));
    }
    let taken = siblings.iter().any(|node| {
        let (other, text) = match node {
            ProjectTreeNode::Folder { id, name, .. } | ProjectTreeNode::Window { id, name, .. } => {
                (*id, name.as_str())
            }
        };
        other != id && text == name
    });
    taken.then(|| tr!("field-refused-name-taken"))
}

/// A row's in-place rename field (window or folder), under the common
/// contract of [`inplace_edit`](super::inplace_edit): the whole name selected,
/// Enter, Tab or a click elsewhere commit and move to the next row of the
/// selection, Escape abandons everything. An empty or already taken name stays
/// open under a red frame. All the typed names go out in a single
/// `Action::RenameObjects` after the last row.
fn rename_editor(
    ui: &mut egui::Ui,
    interaction: &mut PmInteraction,
    memory_key: egui::Id,
    id: NodeId,
    name: &str,
    siblings: &[ProjectTreeNode],
) -> egui::Response {
    let mut memory = ui
        .ctx()
        .data(|data| data.get_temp::<PmMemory>(memory_key))
        .unwrap_or_default();
    let text = memory.text.get_or_insert_with(|| name.to_string());
    let refusal = rename_refusal(siblings, id, text);
    let shown = inplace_edit::show(
        ui,
        egui::Id::new(("pm_rename", id)),
        egui::TextEdit::singleline(text).desired_width(ui.available_width()),
        inplace_edit::Opening::SelectAll,
        inplace_edit::FocusLoss::Ends,
        refusal.as_deref(),
    );
    if let Some(end) = shown.end {
        let refused = rename_refusal(siblings, id, text).is_some();
        match inplace_edit::resolve(end, text.trim() == name, refused) {
            inplace_edit::Resolution::Stay => {}
            inplace_edit::Resolution::Close {
                end: inplace_edit::EditEnd::Cancel,
                ..
            } => {
                memory.rename.clear();
                memory.renamed.clear();
                memory.text = None;
            }
            inplace_edit::Resolution::Close { write, .. } => {
                if write {
                    memory.renamed.push((id, text.clone()));
                }
                memory.rename.remove(0);
                if memory.rename.is_empty() && !memory.renamed.is_empty() {
                    interaction
                        .actions
                        .push(Action::RenameObjects(std::mem::take(&mut memory.renamed)));
                }
                memory.text = None;
            }
        }
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(memory_key, memory));
    shown.response
}

/// What a row received as a selection gesture (see [`select_gestures`]).
#[derive(Default)]
struct RowGestures {
    /// Single click without modifier: the row becomes the only one selected.
    plain: bool,
    /// Double-click without modifier.
    double: bool,
}

/// A row's selection gestures, common to folders and windows: right click
/// (selects if needed), Shift (range), Ctrl/Cmd (toggle), single click
/// (replaces, arms the slow click of the rename) and double-click (disarms the
/// slow click). What the single click then **does** — open the window,
/// expand the folder — is up to the caller.
fn select_gestures(
    ui: &egui::Ui,
    interaction: &mut PmInteraction,
    memory_key: egui::Id,
    entry: PmEntry,
    response: &egui::Response,
    selected: bool,
) -> RowGestures {
    let mut gestures = RowGestures::default();
    if response.clicked() || response.secondary_clicked() {
        let modifiers = ui.input(|input| input.modifiers);
        let mut memory = ui
            .ctx()
            .data(|data| data.get_temp::<PmMemory>(memory_key))
            .unwrap_or_default();
        if response.secondary_clicked() {
            if !selected {
                interaction.selection.clear();
                interaction.selection.insert(entry);
            }
        } else if modifiers.shift {
            let anchor = memory.anchor.unwrap_or(entry);
            if let (Some(a), Some(b)) = (
                memory.order.iter().position(|t| *t == anchor),
                memory.order.iter().position(|t| *t == entry),
            ) {
                // The complete order of the previous frame stays available in
                // the list of row ranks, including after the target.
                if !modifiers.command {
                    interaction.selection.clear();
                }
                interaction
                    .selection
                    .extend(memory.order[a.min(b)..=a.max(b)].iter().copied());
            }
        } else if modifiers.command {
            if !interaction.selection.remove(&entry) {
                interaction.selection.insert(entry);
            }
            memory.anchor = Some(entry);
        } else {
            let alone = selected && interaction.selection.len() == 1;
            memory.slow_click = alone.then(|| (entry, ui.input(|input| input.time)));
            interaction.selection.clear();
            interaction.selection.insert(entry);
            memory.anchor = Some(entry);
            gestures.plain = true;
        }
        memory.focus = true;
        ui.ctx()
            .data_mut(|data| data.insert_temp(memory_key, memory));
        // The open request applies after painting: without a frame claimed
        // here, the highlight and the active window would appear only on
        // the next mouse move.
        ui.ctx().request_repaint();
    }
    if response.double_clicked() {
        ui.ctx().data_mut(|data| {
            data.get_temp_mut_or_default::<PmMemory>(memory_key)
                .slow_click = None;
        });
        gestures.double = ui.input(|input| input.modifiers.is_none());
    }
    gestures
}

/// The row that carries the focus outline: the keyboard cursor, as long as the
/// panel has the focus (`PmMemory::focus`, set by a click inside). Returns
/// `None` when the panel is not focused or while renaming.
fn keyboard_cursor(
    ctx: &egui::Context,
    memory_key: egui::Id,
    selection: &HashSet<PmEntry>,
) -> Option<PmEntry> {
    let memory = ctx.data(|data| data.get_temp::<PmMemory>(memory_key))?;
    (memory.focus && memory.rename.is_empty())
        .then(|| cursor_of(selection, memory.anchor))
        .flatten()
}

/// Scrolls the area to the row the keyboard has just reached.
fn reveal_if_reached(
    ctx: &egui::Context,
    memory_key: egui::Id,
    entry: PmEntry,
    response: &egui::Response,
) {
    let reached = ctx.data_mut(|data| {
        let memory = data.get_temp_mut_or_default::<PmMemory>(memory_key);
        (memory.scroll_to == Some(entry)).then(|| memory.scroll_to.take())
    });
    if reached.is_some() {
        response.scroll_to_me(None);
    }
}

/// Recursively paints a [`ProjectTreeNode`] node and its children; returns
/// the vertical center of its row, for its parent's connectors.
fn project_tree_node_ui(
    ui: &mut egui::Ui,
    node: &ProjectTreeNode,
    siblings: &[ProjectTreeNode],
    interaction: &mut PmInteraction,
) -> f32 {
    let memory_key = memory_id(ui.ctx());
    match node {
        ProjectTreeNode::Folder { id, name, children } => {
            let entry = PmEntry::Folder(*id);
            note_visible(ui.ctx(), memory_key, entry);
            let state_id = ui.make_persistent_id(("project_tree_folder", id));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                state_id,
                false,
            );
            let (revealed, toggled) = ui.ctx().data_mut(|data| {
                let memory = data.get_temp_mut_or_default::<PmMemory>(memory_key);
                let before = memory.reveal.len();
                memory.reveal.retain(|folder| folder != id);
                let toggles = memory.toggle.len();
                memory.toggle.retain(|folder| folder != id);
                (
                    memory.reveal.len() != before,
                    memory.toggle.len() != toggles,
                )
            });
            if revealed {
                state.set_open(true);
            }
            if toggled {
                // Enter (#170, point 6): expands a closed folder, collapses an open one.
                state.set_open(!state.is_open());
            }
            // #170: the header row has the height of a list row; the height is
            // given back after the body (the response borrows the `Ui`).
            let previous_height = raise_to_list_row_height(ui);
            let mut gestures = RowGestures::default();
            let mut header = state.show_header(ui, |ui| {
                if is_renaming(ui.ctx(), memory_key, *id) {
                    ui.add(icon_image(Icon::Folder, IconRole::Folder));
                    rename_editor(ui, interaction, memory_key, *id, name, siblings);
                    return;
                }
                // #170 point 4: a folder is a list row like a window
                // (background, bar, hover), selectable.
                let selected = interaction.selection.contains(&entry);
                let focused =
                    keyboard_cursor(ui.ctx(), memory_key, interaction.selection) == Some(entry);
                let response = list_row_focused(
                    ui,
                    Some(icon_image(Icon::Folder, IconRole::Folder)),
                    name,
                    selected,
                    false,
                    focused,
                );
                reveal_if_reached(ui.ctx(), memory_key, entry, &response);
                gestures = select_gestures(ui, interaction, memory_key, entry, &response, selected);
                response.context_menu(|ui| {
                    let viewport = ui.ctx().viewport_id();
                    let memory = ui
                        .ctx()
                        .data(|data| data.get_temp::<PmMemory>(memory_key))
                        .unwrap_or_default();
                    let ids = selected_ids(interaction.selection, &memory.order);
                    let has_folder = selection_has_folder(interaction.selection);
                    object_menu(
                        ui,
                        &mut MenuSections::new(),
                        ObjectMenuTarget::Folder,
                        &ids,
                        *id,
                        has_folder,
                        interaction.actions,
                        viewport,
                    )
                });
            });
            // A click on a closed folder expands it, on an open folder it does
            // not collapse it; the double-click collapses it (the triangle too).
            if gestures.plain && !header.is_open() {
                header.set_open(true);
            }
            if gestures.double {
                header.set_open(false);
            }
            let (toggle, _, _) = header.body(|ui| {
                crate::tree_lines::tree_children(ui, |ui, branch| {
                    if children.is_empty() {
                        branch.mark(ui.weak(tr!("project-folder-empty")).rect.center().y);
                    }
                    for child in children {
                        branch.mark(project_tree_node_ui(ui, child, children, interaction));
                    }
                });
            });
            restore_row_height(ui, previous_height);
            toggle.rect.center().y
        }
        ProjectTreeNode::Window { id, name, kind } => {
            let icon = match kind {
                WindowKind::Book => (Icon::Workbook, IconRole::Workbook),
                WindowKind::Graph => (Icon::Graph, IconRole::Graph),
                WindowKind::Matrix => (Icon::Matrix, IconRole::Matrix),
                WindowKind::Note => (Icon::Note, IconRole::Note),
            };
            let tab = tab_for_window(*id, *kind);
            let entry = PmEntry::Window(tab);
            note_visible(ui.ctx(), memory_key, entry);
            if is_renaming(ui.ctx(), memory_key, *id) {
                return with_list_row_height(ui, |ui| {
                    rename_editor(ui, interaction, memory_key, *id, name, siblings)
                })
                .rect
                .center()
                .y;
            }
            let selected = interaction.selection.contains(&entry);
            let is_closed = !interaction.open.contains(&tab);
            let focused =
                keyboard_cursor(ui.ctx(), memory_key, interaction.selection) == Some(entry);
            // The per-tab salt prevents all the rows from sharing the same Id.
            let response = ui
                .push_id(tab, |ui| {
                    // #169: a list row (`list_row`), never a `.selected()` button.
                    list_row_focused(
                        ui,
                        Some(icon_image(icon.0, icon.1)),
                        name,
                        selected,
                        is_closed,
                        focused,
                    )
                })
                .inner;
            reveal_if_reached(ui.ctx(), memory_key, entry, &response);
            let gestures = select_gestures(ui, interaction, memory_key, entry, &response, selected);
            if gestures.plain {
                interaction.open_requests.push(tab);
            }
            response.context_menu(|ui| {
                let mut sections = MenuSections::new();
                sections.section(ui, true, |ui| {
                    if ui
                        .button(tr!("project-manager-open"))
                        .on_hover_text(tr!("project-manager-open", "hint"))
                        .clicked()
                    {
                        for entry in interaction.selection.iter() {
                            if let PmEntry::Window(tab) = entry {
                                interaction.open_requests.push(*tab);
                            }
                        }
                        ui.close();
                    }
                    if ui
                        .button(tr!("project-manager-close"))
                        .on_hover_text(tr!("project-manager-close", "hint"))
                        .clicked()
                    {
                        for entry in interaction.selection.iter() {
                            if let PmEntry::Window(tab) = entry {
                                interaction.close_requests.push(*tab);
                            }
                        }
                        ui.close();
                    }
                });
                let memory = ui
                    .ctx()
                    .data(|data| data.get_temp::<PmMemory>(memory_key))
                    .unwrap_or_default();
                let ids = selected_ids(interaction.selection, &memory.order);
                let viewport = ui.ctx().viewport_id();
                let parent = paste_parent(interaction.selection, interaction.root);
                let has_folder = selection_has_folder(interaction.selection);
                object_menu(
                    ui,
                    &mut sections,
                    ObjectMenuTarget::Windows,
                    &ids,
                    parent,
                    has_folder,
                    interaction.actions,
                    viewport,
                );
            });
            // S-K3: a double-click activates or restores the window, it never
            // closes it (closing is done by Close or the header).
            if gestures.double {
                interaction.open_requests.push(tab);
            }
            response.rect.center().y
        }
    }
}

/// Default width of the fixed Project Manager panel, and of the separate OS
/// window when it is detached.
pub(super) const PROJECT_MANAGER_WIDTH: f32 = 260.0;

impl StokesForgeApp {
    /// #146: follow only the active window changes. Redoing the selection on
    /// every frame would overwrite the panel's click before its open request is
    /// applied. A multiple selection stays the user's as long as no explicit
    /// click on an entry or a workspace window replaces it (#154,
    /// `Workspace::activate_clicked_tab`).
    pub(super) fn follow_project_manager_active_tab(
        &mut self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
    ) {
        if active_tab != self.pm_last_active_tab {
            self.pm_last_active_tab = active_tab;
            if self.pm_selection.len() <= 1 {
                self.pm_selection.clear();
                self.pm_selection.extend(active_tab.map(PmEntry::Window));
                follow_selection_anchor(ctx, active_tab);
            }
        }
    }

    /// Paints the Project Manager in its fixed panel, anchored left — see
    /// the head doc of `app.rs`, § What stays here (the order of the panels).
    /// Called by
    /// [`StokesForgeApp::ui`](eframe::App::ui) *before* the `CentralPanel`, but
    /// *after* [`StokesForgeApp::script_panel`] — see its head doc for
    /// why this precise order. Like [`StokesForgeApp::menu_bar`]
    /// (`egui::Panel::top`): painting an anchored panel before the rest
    /// guarantees by construction that it is never covered nor covers
    /// what comes after. Not called when [`StokesForgeApp::pm_detached`]
    /// is `true` — that frame, nothing replaces this panel, the real
    /// separate OS window ([`StokesForgeApp::show_detached_project_manager`])
    /// takes its place. Stays resizable in width by dragging the edge
    /// (default behavior of `egui::Panel::left`) but fixed in
    /// position: no switch to the right nor repositioning by
    /// drag. A separate strip when collapsed keeps the expanded width.
    pub(super) fn project_manager_panel(
        &mut self,
        ui: &mut egui::Ui,
        open_requests: &mut Vec<Tab>,
        close_requests: &mut Vec<Tab>,
        actions: &mut Vec<Action>,
    ) {
        if self.pm_collapsed {
            egui::Panel::left("stokesforge_pm_collapsed")
                .exact_size(PANEL_STRIP_THICKNESS)
                .resizable(false)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(0))
                .show_separator_line(false)
                .show(ui, |ui| {
                    let header = panel_header(
                        ui,
                        &tr!("project-manager-title"),
                        PanelPlacement::Left,
                        true,
                    );
                    self.pm_collapsed ^= header.toggle_collapse;
                    if header.detach {
                        self.detach_project_manager();
                    }
                });
            return;
        }
        egui::Panel::left("stokesforge_pm")
            .default_size(PROJECT_MANAGER_WIDTH)
            .show(ui, |ui| {
                let header = panel_header(
                    ui,
                    &tr!("project-manager-title"),
                    PanelPlacement::Left,
                    false,
                );
                self.pm_collapsed ^= header.toggle_collapse;
                if header.detach {
                    self.detach_project_manager();
                }
                let tree = self.state.project_tree();
                let open = self.open_tabs();
                let mut interaction = PmInteraction {
                    actions,
                    root: self.state.tree().root(),
                    open_requests,
                    selection: &mut self.pm_selection,
                    open: &open,
                    close_requests,
                };
                project_manager_contents_from_tree(ui, &tree, &mut interaction);
            });
    }
}

/// `pub(super)`: [`tests::find_text_center`] and [`tests::click_at`] also serve
/// the tests of the other `app` modules that click a painted text.
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::cell::RefCell;

    use stokesforge_core::{NodeId, Project};
    use stokesforge_plot::PlotSpec;
    use stokesforge_session::Session;

    use super::super::list_row::LIST_ROW_MARK;
    use super::super::test_support::painted;
    use crate::theme;
    use stokesforge_app_state::state::AppState;

    /// Finds the center of the painted text containing `needle` in `shapes` —
    /// used to click a Project Manager entry without assuming its exact
    /// position (auto layout), and by the tests of the other `app` modules
    /// (`graph_window.rs`, `format.rs`, `menus.rs`…). Same technique as
    /// `window_chrome::tests::the_graph_header_keeps_its_title_centered_and_its_arrow_on_the_left`.
    pub(in crate::app) fn find_text_center(
        shapes: &[egui::epaint::ClippedShape],
        needle: &str,
    ) -> egui::Pos2 {
        shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle) => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no text containing {needle:?} was painted"))
    }

    /// A click (zero drag, press then release at the same place) on
    /// `pos`, spread over two frames as egui's click detection requires — see
    /// `graph_window::tests::le_pan_principal_atteint_la_figure_dans_la_fenetre_flottante`
    /// for the same pattern applied to a drag.
    pub(in crate::app) fn click_at<R>(
        context: &egui::Context,
        pos: egui::Pos2,
        modifiers: egui::Modifiers,
        mut paint: impl FnMut(&egui::Context, Vec<egui::Event>) -> R,
    ) {
        // `PointerButton::modifiers` does not feed `InputState::modifiers`
        // (see `InputState::begin_pass`, which reads it only on
        // `Event::ModifiersChanged`/`Event::Key`): without this explicit
        // event, `ui.input(|i| i.modifiers.command)` would always stay
        // false, whatever the `modifiers` set on the pointer events
        // themselves.
        paint(
            context,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::PointerMoved(pos),
            ],
        );
        paint(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
        );
        paint(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        if modifiers != egui::Modifiers::NONE {
            paint(
                context,
                vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
            );
        }
    }

    /// Builds the tree of a Book+Graph project, read by `AppState` the way the
    /// panel reads it — common to the Project Manager's double-click and
    /// multiple-selection tests.
    fn project_manager_fixture() -> (Session, Tab, Tab, Vec<ProjectTreeNode>) {
        let mut project = Project::new("Project Manager").unwrap();
        let root = project.root();
        project.add_empty_workbook(root, None).unwrap();
        let graph = project.add_graph(root, None, PlotSpec::default()).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let tree = state.project_tree();
        let book_id = tree
            .iter()
            .find_map(|node| match node {
                ProjectTreeNode::Window {
                    id,
                    kind: WindowKind::Book,
                    ..
                } => Some(*id),
                _ => None,
            })
            .expect("the fixture project carries a workbook");
        (session, Tab::Book(book_id), Tab::Graph(graph), tree)
    }

    /// A selection of windows (the historical tests have no other).
    pub(in crate::app) fn windows<const N: usize>(tabs: [Tab; N]) -> HashSet<PmEntry> {
        tabs.into_iter().map(PmEntry::Window).collect()
    }

    /// The tabs of the windows of a selection (folders are left out).
    fn tabs_of(selection: &HashSet<PmEntry>) -> HashSet<Tab> {
        selection
            .iter()
            .filter_map(|entry| match entry {
                PmEntry::Window(tab) => Some(*tab),
                PmEntry::Folder(_) => None,
            })
            .collect()
    }

    /// The accumulators of a test's [`PmInteraction`], and a painted frame
    /// of the tree on a 400 × 400 screen.
    struct Bench {
        actions: RefCell<Vec<Action>>,
        open_requests: RefCell<Vec<Tab>>,
        selection: RefCell<HashSet<PmEntry>>,
        open: RefCell<HashSet<Tab>>,
        close_requests: RefCell<Vec<Tab>>,
        /// Was ↓ still readable after the panel, in the same frame?
        /// What a grid painted afterwards would see (it reads the arrows without
        /// consuming them, #170 point 6).
        arrow_down_left: std::cell::Cell<bool>,
    }

    impl Bench {
        fn new(selection: HashSet<PmEntry>) -> Self {
            Bench {
                actions: RefCell::new(Vec::new()),
                open_requests: RefCell::new(Vec::new()),
                selection: RefCell::new(selection),
                open: RefCell::new(HashSet::new()),
                close_requests: RefCell::new(Vec::new()),
                arrow_down_left: std::cell::Cell::new(false),
            }
        }

        fn paint(
            &self,
            context: &egui::Context,
            tree: &[ProjectTreeNode],
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            self.paint_at(context, tree, events, None)
        }

        /// Like [`Bench::paint`], at instant `time`: the gestures that depend
        /// on the delay (slow click) cannot be driven by the real clock.
        fn paint_at(
            &self,
            context: &egui::Context,
            tree: &[ProjectTreeNode],
            events: Vec<egui::Event>,
            time: Option<f64>,
        ) -> egui::FullOutput {
            let input = egui::RawInput {
                time,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut interaction = PmInteraction {
                actions: &mut self.actions.borrow_mut(),
                root: NodeId::from_raw(1),
                open_requests: &mut self.open_requests.borrow_mut(),
                selection: &mut self.selection.borrow_mut(),
                open: &self.open.borrow(),
                close_requests: &mut self.close_requests.borrow_mut(),
            };
            let mut output = context.run_ui(input, |ui| {
                project_manager_contents_from_tree(ui, tree, &mut interaction);
                self.arrow_down_left
                    .set(ui.input(|input| input.key_pressed(egui::Key::ArrowDown)));
            });
            output.textures_delta.clear();
            output
        }
    }

    /// #170, point 5: a folder's header has the same height as a window
    /// (`LIST_ROW_HEIGHT`); the L connectors aim at the row's center.
    /// Measured by the step between the centers of consecutive rows (folder ->
    /// window -> window -> folder): constant, and equal to the row height
    /// plus the vertical gap. Witness: without the height imposed on the header,
    /// the step from the folder to the window is shorter.
    #[test]
    fn a_folder_has_the_height_of_a_window() {
        use super::super::list_row::LIST_ROW_HEIGHT;
        let mut project = Project::new("Heights").unwrap();
        let root = project.root();
        project.add_folder(root, "Folder A").unwrap();
        project.add_graph(root, None, PlotSpec::default()).unwrap();
        project.add_note(root, None, "").unwrap();
        project.add_folder(root, "Folder B").unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let tree = state.project_tree();
        let context = super::super::test_support::context();
        let bench = Bench::new(HashSet::new());
        bench.paint(&context, &tree, Vec::new());
        let output = bench.paint(&context, &tree, Vec::new());
        let mut centers: Vec<f32> = super::super::test_support::texts(&output.shapes)
            .iter()
            .filter(|t| {
                t.text.starts_with("Folder") || t.text.contains("Graph") || t.text.contains("Note")
            })
            .map(|t| t.center.y)
            .collect();
        centers.sort_by(f32::total_cmp);
        assert_eq!(centers.len(), 4, "{centers:?}");
        let steps: Vec<f32> = centers.windows(2).map(|w| w[1] - w[0]).collect();
        let spacing = context.global_style().spacing.item_spacing.y;
        for p in &steps {
            assert!(
                (p - (LIST_ROW_HEIGHT + spacing)).abs() < 1.0,
                "steps {p} attendu {} : {steps:?}",
                LIST_ROW_HEIGHT + spacing
            );
        }
    }

    /// #169: a selected entry is a list row — pale accent background over the
    /// whole remaining width and accent bar on the left — and no longer the
    /// buttons' `selection.bg_fill` background, which the other selections of
    /// the application keep. Witness: without selection, neither background
    /// nor bar.
    #[test]
    fn the_project_manager_selection_is_a_list_row_with_a_bar() {
        let mut project = Project::new("List selection").unwrap();
        let root = project.root();
        let book = Tab::Book(project.add_empty_workbook(root, None).unwrap());
        let graph = Tab::Graph(project.add_graph(root, None, PlotSpec::default()).unwrap());
        let note = Tab::Note(project.add_note(root, None, "").unwrap());
        let matrix = Tab::Matrix(project.add_matrix(root, None).unwrap());
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let tree = state.project_tree();
        let context = super::super::test_support::context();
        let bench = Bench::new(windows([book, graph, note, matrix]));
        bench.paint(&context, &tree, Vec::new());
        let output = bench.paint(&context, &tree, Vec::new());
        let rects = |output: &egui::FullOutput, fill: egui::Color32| -> Vec<egui::Rect> {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                    _ => None,
                })
                .collect()
        };
        let rows = rects(&output, theme::list_row_selected_fill());
        let marks = rects(&output, theme::accent());
        assert_eq!(rows.len(), 4, "a single background per selected entry");
        assert_eq!(marks.len(), 4, "a bar per selected entry");
        for (row, mark) in rows.iter().zip(&marks) {
            assert_eq!(mark.left(), row.left(), "the bar is at the left of the row");
            assert_eq!(mark.height(), row.height(), "the bar covers the row");
            assert_eq!(mark.width(), LIST_ROW_MARK);
            assert!(
                row.width() > 300.0,
                "the row runs over the whole remaining width: {row:?}"
            );
        }
        let button_fill = context.global_style().visuals.selection.bg_fill;
        assert!(
            rects(&output, button_fill).is_empty(),
            "no more selected button background"
        );
        let painted_texts = super::super::test_support::texts(&output.shapes);
        for name in ["Book", "Graph1"] {
            assert!(
                painted_texts
                    .iter()
                    .filter(|t| t.text.contains(name))
                    .all(|t| t.bold),
                "{name}: the selected entry is bold"
            );
        }
        bench.selection.borrow_mut().clear();
        let output = bench.paint(&context, &tree, Vec::new());
        assert!(rects(&output, theme::list_row_selected_fill()).is_empty());
        assert!(rects(&output, theme::accent()).is_empty());
        assert!(
            !super::super::test_support::texts(&output.shapes)
                .iter()
                .any(|t| t.bold),
            "witness: without selection, nothing is bold"
        );
    }

    #[test]
    fn following_the_active_content_preserves_the_multiple_selection() {
        let (session, book, graph, _tree) = project_manager_fixture();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let mut app = StokesForgeApp::new(state);
        let context = egui::Context::default();
        let detached_key = egui::Id::new(("project_manager_gestures", viewport_id()));
        context.data_mut(|data| {
            data.insert_temp(
                detached_key,
                PmMemory {
                    focus: true,
                    rename: vec![tab_node(book)],
                    ..Default::default()
                },
            )
        });
        let assert_anchor = |anchor| {
            for viewport in [egui::ViewportId::ROOT, viewport_id()] {
                let key = egui::Id::new(("project_manager_gestures", viewport));
                context.data(|data| {
                    assert_eq!(data.get_temp::<PmMemory>(key).unwrap().anchor, anchor)
                });
            }
        };
        app.follow_project_manager_active_tab(&context, Some(book));
        assert_eq!(app.pm_selection, windows([book]));
        assert_anchor(Some(PmEntry::Window(book)));
        app.follow_project_manager_active_tab(&context, Some(graph));
        assert_eq!(app.pm_selection, windows([graph]));
        assert_anchor(Some(PmEntry::Window(graph)));
        app.pm_selection.insert(PmEntry::Window(book));
        app.follow_project_manager_active_tab(&context, Some(book));
        assert_eq!(app.pm_selection, windows([book, graph]));
        assert_anchor(Some(PmEntry::Window(graph)));
        app.follow_project_manager_active_tab(&context, None);
        assert_eq!(app.pm_selection, windows([book, graph]));
        // The existing single click replaces the selection. Same active window:
        // do not overwrite that click before its open request is applied.
        app.pm_selection = windows([book]);
        app.follow_project_manager_active_tab(&context, None);
        assert_eq!(app.pm_selection, windows([book]));
        app.follow_project_manager_active_tab(&context, Some(graph));
        assert_eq!(app.pm_selection, windows([graph]));
        app.follow_project_manager_active_tab(&context, None);
        assert!(app.pm_selection.is_empty());
        assert_anchor(None);
        context.data(|data| {
            let memory = data.get_temp::<PmMemory>(detached_key).unwrap();
            assert!(memory.focus, "the other viewport's focus is kept");
            assert_eq!(memory.rename, vec![tab_node(book)], "the editor is kept");
        });
    }

    #[test]
    fn shift_after_a_workspace_click_starts_from_the_selected_window() {
        verify_shift_after_workspace_click(false);
    }

    #[test]
    fn shift_after_a_workspace_click_on_the_same_entry_does_not_select_the_old_anchor() {
        verify_shift_after_workspace_click(true);
    }

    fn verify_shift_after_workspace_click(same_target: bool) {
        for maximized in [false, true] {
            let (session, book, graph, tree) = project_manager_fixture();
            let state = AppState::new(session.handle(), String::new()).unwrap();
            let app = RefCell::new(StokesForgeApp::new(state));
            app.borrow_mut().floating_tabs = vec![book, graph];
            app.borrow_mut()
                .window_positions
                .insert(graph, egui::pos2(30.0, 30.0));
            app.borrow_mut()
                .window_positions
                .insert(book, egui::pos2(750.0, 30.0));
            app.borrow_mut().maximized_tab = maximized.then_some(graph);
            let context = super::super::test_support::context();
            egui_extras::install_image_loaders(&context);
            let paint = |ctx: &egui::Context, events| {
                super::super::test_support::frame_of(ctx, egui::vec2(1400.0, 900.0), events, |ui| {
                    let mut app = app.borrow_mut();
                    egui::Panel::left("pm_test")
                        .exact_size(260.0)
                        .show(ui, |ui| {
                            let mut interaction = PmInteraction {
                                actions: &mut Vec::new(),
                                root: app.state.tree().root(),
                                open_requests: &mut Vec::new(),
                                selection: &mut app.pm_selection,
                                open: &HashSet::new(),
                                close_requests: &mut Vec::new(),
                            };
                            project_manager_contents_from_tree(ui, &tree, &mut interaction);
                        });
                    app.show_workspace(
                        ui,
                        Some(graph),
                        &mut Vec::new(),
                        &mut Vec::new(),
                        &mut Vec::new(),
                        &mut Vec::new(),
                    );
                })
            };
            paint(&context, vec![]);
            let first = paint(&context, vec![]);
            let book_pos = find_text_center(&first.shapes, "Book");
            let graph_pos = find_text_center(&first.shapes, "Graph");
            // The anchor comes from a real click on the panel, then the same
            // already active window takes over the selection through the workspace.
            click_at(&context, book_pos, egui::Modifiers::NONE, paint);
            assert_eq!(app.borrow().pm_selection, windows([book]));
            click_at(
                &context,
                egui::pos2(410.0, 150.0),
                egui::Modifiers::NONE,
                paint,
            );
            assert_eq!(app.borrow().pm_selection, windows([graph]));
            let detached_key = egui::Id::new(("project_manager_gestures", viewport_id()));
            context.data(|data| {
                assert_eq!(
                    data.get_temp::<PmMemory>(detached_key).unwrap().anchor,
                    Some(PmEntry::Window(graph)),
                    "the workspace click also moves the detached panel's anchor"
                )
            });
            click_at(
                &context,
                if same_target { graph_pos } else { book_pos },
                egui::Modifiers::SHIFT,
                paint,
            );
            assert_eq!(
                app.borrow().pm_selection,
                if same_target {
                    windows([graph])
                } else {
                    windows([book, graph])
                },
                "maximized={maximized}, same target={same_target}"
            );
            session.shutdown().unwrap();
        }
    }

    #[test]
    fn shift_selects_the_range_and_the_shortcuts_act_on_the_selection() {
        let (_session, book, graph, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let book_pos = find_text_center(&first.shapes, "Book");
        let graph_pos = find_text_center(&first.shapes, "Graph");
        click_at(&context, book_pos, egui::Modifiers::NONE, paint);
        click_at(&context, graph_pos, egui::Modifiers::SHIFT, paint);
        assert_eq!(*bench.selection.borrow(), windows([book, graph]));
        paint(&context, vec![egui::Event::Copy]);
        assert_eq!(
            bench.actions.take(),
            vec![Action::CopyObjects(vec![tab_node(book), tab_node(graph)])]
        );
        paint(
            &context,
            vec![egui::Event::Key {
                key: egui::Key::D,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        assert_eq!(
            bench.actions.take(),
            vec![Action::DuplicateObjects(vec![
                tab_node(book),
                tab_node(graph)
            ])]
        );
        paint(
            &context,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                egui::Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(
            bench.actions.take(),
            vec![Action::DeleteObjects(vec![tab_node(book), tab_node(graph)])]
        );
    }

    #[test]
    fn f2_opens_the_field_and_enter_commits_one_action() {
        let (_session, book, _graph, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let bench = Bench::new(windows([book]));
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let memory_key = memory_id(&context);
        context.data_mut(|data| {
            let mut memory = data.get_temp::<PmMemory>(memory_key).unwrap();
            memory.focus = true;
            data.insert_temp(memory_key, memory);
        });
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        paint(&context, vec![key(egui::Key::F2)]);
        assert!(context.text_edit_focused());
        // The name is selected on opening: typing replaces it.
        paint(&context, vec![egui::Event::Text("New".to_string())]);
        paint(&context, vec![key(egui::Key::Enter)]);
        assert_eq!(bench.actions.take().len(), 1);
        assert!(
            !has_focus(&egui::Context::default()),
            "another context does not own this focus"
        );
    }

    /// Effective color of the first painted text containing `needle`.
    fn text_color(shapes: &[egui::epaint::ClippedShape], needle: &str) -> egui::Color32 {
        shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle) => {
                    let color = shape.galley.job.sections[0].format.color;
                    Some(if color == egui::Color32::PLACEHOLDER {
                        shape.fallback_color
                    } else {
                        color
                    })
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no text containing {needle:?} was painted"))
    }

    /// The fixed panel ([`StokesForgeApp::project_manager_panel`]) occupies the
    /// left edge and links the tree to the application's fields: a
    /// Ctrl-click selects in `pm_selection`, a single click requests the
    /// opening, applied later by `ui()`.
    #[test]
    fn the_fixed_panel_links_the_tree_to_the_application_fields() {
        let (session, book_tab, _graph_tab, _tree) = project_manager_fixture();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let app = RefCell::new(StokesForgeApp::new(state));
        let open = RefCell::new(Vec::new());
        let context = egui::Context::default();
        let paint = |context: &egui::Context, events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 500.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                app.borrow_mut().project_manager_panel(
                    ui,
                    &mut open.borrow_mut(),
                    &mut Vec::new(),
                    &mut Vec::new(),
                );
            });
            output.textures_delta.clear();
            output
        };
        paint(&context, Vec::new());
        let output = paint(&context, Vec::new());
        assert!(painted(&output, &tr!("project-manager-title")));
        let panel =
            egui::containers::panel::PanelState::load(&context, egui::Id::new("stokesforge_pm"))
                .expect("the panel is placed")
                .outer_rect;
        assert_eq!(panel.left(), 0.0, "anchored left");
        assert!(panel.width() < 800.0, "it leaves room for the rest");

        let book = find_text_center(&output.shapes, "Book");
        click_at(&context, book, egui::Modifiers::COMMAND, paint);
        assert!(app
            .borrow()
            .pm_selection
            .contains(&PmEntry::Window(book_tab)));
        assert!(
            open.borrow().is_empty(),
            "Ctrl-click selects without opening"
        );
        click_at(&context, book, egui::Modifiers::NONE, paint);
        assert!(open.borrow().contains(&book_tab));
        session.shutdown().unwrap();
    }

    #[test]
    fn double_click_activates_the_entry_without_modifier() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let double_click = |target: &str, modifiers| {
            // Fresh context: in the same one, egui would count these clicks
            // after the previous ones (triple click, then more) and the
            // gesture would no longer be a double-click.
            let context = egui::Context::default();
            let bench = Bench::new(HashSet::new());
            let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);
            let first = paint(&context, Vec::new());
            let pos = find_text_center(&first.shapes, target);
            click_at(&context, pos, modifiers, paint);
            click_at(&context, pos, modifiers, paint);
            let open = bench.open_requests.borrow().clone();
            open
        };
        // S-K3: the double-click activates the window (the Project Manager has
        // no closing path anymore, see `PmInteraction`).
        let open = double_click("Graph1", egui::Modifiers::NONE);
        assert!(open.contains(&graph_tab), "open_requests={open:?}");
        let open = double_click("Book", egui::Modifiers::NONE);
        assert!(open.contains(&book_tab), "open_requests={open:?}");
        // Two quick Ctrl-clicks toggle the selection twice: this is not an
        // opening.
        let open = double_click("Book", egui::Modifiers::COMMAND);
        assert!(open.is_empty(), "open_requests={open:?}");
    }

    /// Ctrl/Cmd-click toggles the membership in the selection: it adds, then
    /// removes on the second click, without ever opening the tab.
    #[test]
    fn ctrl_click_selects_without_opening() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, paint);

        assert_eq!(
            *bench.selection.borrow(),
            windows([graph_tab]),
            "Ctrl/Cmd-click must add the entry to the selection"
        );
        assert!(
            bench.open_requests.borrow().is_empty(),
            "Ctrl/Cmd-click must never open the tab, open_requests={:?}",
            bench.open_requests.borrow()
        );

        // The selection made the grouped bar appear above the tree: the entry
        // moved down, we read it again before clicking again.
        let with_toolbar = paint(&context, Vec::new());
        let graph_pos = find_text_center(&with_toolbar.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, paint);
        assert!(
            bench.selection.borrow().is_empty(),
            "a second Ctrl/Cmd-click removes the entry from the selection, selection={:?}",
            bench.selection.borrow()
        );
        assert!(bench.open_requests.borrow().is_empty());
    }

    #[test]
    fn close_and_open_act_on_the_whole_selection() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        // Selection set directly rather than rebuilt by two successive
        // Ctrl-clicks (see `ctrl_click_selects_without_opening`).
        let bench = Bench::new(windows([graph_tab, book_tab]));
        let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);

        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph");
        let menu = open_context_menu(&context, graph_pos, paint);
        assert_eq!(*bench.selection.borrow(), windows([graph_tab, book_tab]));
        let close_pos = find_text_center(&menu.shapes, &tr!("project-manager-close"));
        click_at(&context, close_pos, egui::Modifiers::NONE, paint);
        let closed: HashSet<Tab> = bench.close_requests.borrow().iter().copied().collect();
        assert_eq!(
            closed,
            HashSet::from([graph_tab, book_tab]),
            "\"Close\" must apply to the whole selection"
        );
        assert!(
            bench.open_requests.borrow().is_empty(),
            "\"Close\" opens nothing"
        );
        assert!(
            !painted(&paint(&context, Vec::new()), &tr!("project-manager-close")),
            "Close closes the context menu"
        );

        let menu = open_context_menu(&context, graph_pos, paint);
        let open_pos = find_text_center(&menu.shapes, &tr!("project-manager-open"));
        click_at(&context, open_pos, egui::Modifiers::NONE, paint);
        assert!(
            bench.open_requests.borrow().contains(&graph_tab)
                && bench.open_requests.borrow().contains(&book_tab),
            "\"Open\" guarantees that each tab is open, open_requests={:?}",
            bench.open_requests.borrow()
        );
        assert!(
            !painted(&paint(&context, Vec::new()), &tr!("project-manager-open")),
            "Open closes the context menu"
        );
    }

    /// #327: a tree whose folder (collapsed by default) hides two windows
    /// between the first and the last visible rows.
    fn tree_with_collapsed_folder() -> (Vec<ProjectTreeNode>, [Tab; 4]) {
        let window = |raw, name: &str, kind| ProjectTreeNode::Window {
            id: NodeId::from_raw(raw),
            name: name.into(),
            kind,
        };
        let tree = vec![
            window(2, "Alpha", WindowKind::Book),
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(3),
                name: "Folder".into(),
                children: vec![
                    window(4, "Beta", WindowKind::Graph),
                    window(5, "Gamma", WindowKind::Note),
                ],
            },
            window(6, "Delta", WindowKind::Matrix),
        ];
        let tabs = [
            Tab::Book(NodeId::from_raw(2)),
            Tab::Graph(NodeId::from_raw(4)),
            Tab::Note(NodeId::from_raw(5)),
            Tab::Matrix(NodeId::from_raw(6)),
        ];
        (tree, tabs)
    }

    /// #327: the Shift range crosses a collapsed folder, and Copy, Delete,
    /// Open and Close act on the whole range, hidden windows included.
    #[test]
    fn the_shift_range_crosses_a_collapsed_folder_and_the_actions_follow_it() {
        let (tree, [alpha, beta, gamma, delta]) = tree_with_collapsed_folder();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(!painted(&first, "Beta"), "witness: the folder is collapsed");
        let alpha_pos = find_text_center(&first.shapes, "Alpha");
        let delta_pos = find_text_center(&first.shapes, "Delta");
        // From bottom to top, then from top to bottom.
        click_at(&context, delta_pos, egui::Modifiers::NONE, paint);
        click_at(&context, alpha_pos, egui::Modifiers::SHIFT, paint);
        // The collapsed folder is also in the range (#170, point 4).
        let folder = PmEntry::Folder(NodeId::from_raw(3));
        let mut all = windows([alpha, beta, gamma, delta]);
        all.insert(folder);
        assert_eq!(*bench.selection.borrow(), all);
        click_at(&context, alpha_pos, egui::Modifiers::NONE, paint);
        click_at(&context, delta_pos, egui::Modifiers::SHIFT, paint);
        assert_eq!(*bench.selection.borrow(), all);

        // In reading order: the folder precedes its children.
        let ids: Vec<NodeId> = vec![
            tab_node(alpha),
            folder.node(),
            tab_node(beta),
            tab_node(gamma),
            tab_node(delta),
        ];
        bench.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Copy]);
        assert_eq!(bench.actions.take(), vec![Action::CopyObjects(ids.clone())]);
        paint(
            &context,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                egui::Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(bench.actions.take(), vec![Action::DeleteObjects(ids)]);

        bench.open_requests.borrow_mut().clear();
        let menu = open_context_menu(&context, alpha_pos, paint);
        let close_pos = find_text_center(&menu.shapes, &tr!("project-manager-close"));
        click_at(&context, close_pos, egui::Modifiers::NONE, paint);
        let closed: HashSet<Tab> = bench.close_requests.borrow().iter().copied().collect();
        assert_eq!(
            closed,
            tabs_of(&all),
            "Close also closes the windows of the collapsed folder"
        );
        let menu = open_context_menu(&context, alpha_pos, paint);
        let open_pos = find_text_center(&menu.shapes, &tr!("project-manager-open"));
        click_at(&context, open_pos, egui::Modifiers::NONE, paint);
        let opened: HashSet<Tab> = bench.open_requests.borrow().iter().copied().collect();
        assert_eq!(
            opened,
            tabs_of(&all),
            "Open also opens those of the collapsed folder"
        );
    }

    /// #327: a bound of the range may itself be in a collapsed folder
    /// (the anchor comes from the active window, whose row is not painted).
    #[test]
    fn a_shift_range_bound_in_a_collapsed_folder_counts() {
        let (tree, [_alpha, beta, gamma, delta]) = tree_with_collapsed_folder();
        let context = egui::Context::default();
        let bench = Bench::new(windows([beta]));
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        follow_selection_anchor(&context, Some(beta));
        click_at(
            &context,
            find_text_center(&first.shapes, "Delta"),
            egui::Modifiers::SHIFT,
            paint,
        );
        assert_eq!(*bench.selection.borrow(), windows([beta, gamma, delta]));
    }

    fn open_context_menu(
        context: &egui::Context,
        pos: egui::Pos2,
        mut paint: impl FnMut(&egui::Context, Vec<egui::Event>) -> egui::FullOutput,
    ) -> egui::FullOutput {
        paint(context, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            paint(
                context,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        paint(context, Vec::new())
    }

    /// The visibility actions open on right click. The single click
    /// still opens its entry, without moving the tree when selecting.
    #[test]
    fn the_visibility_actions_are_in_the_context_menu_and_the_single_click_opens() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        assert!(
            !painted(&first, &tr!("project-manager-close"))
                && !painted(&first, &tr!("project-manager-open")),
            "without selection, no grouped button to look at"
        );
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*bench.open_requests.borrow(), vec![graph_tab]);
        assert_eq!(
            *bench.selection.borrow(),
            windows([graph_tab]),
            "a single click selects its target (#105)"
        );

        // The selection does not move the entries under the mouse.
        let with_selection = paint(&context, Vec::new());
        assert!(!painted(&with_selection, &tr!("project-manager-close")));
        assert!(!painted(&with_selection, &tr!("project-manager-open")));
        assert_eq!(
            find_text_center(&with_selection.shapes, "Graph1"),
            graph_pos
        );
        let menu = open_context_menu(&context, graph_pos, paint);
        assert!(painted(&menu, &tr!("project-manager-close")));
        assert!(painted(&menu, &tr!("project-manager-open")));
    }

    /// #157: a window's right click is **flat** — Open · Close ─ Copy ·
    /// Paste ─ Rename · Duplicate ─ Delete, without "Selection ▸".
    #[test]
    fn a_window_right_click_is_flat_in_section_order() {
        let (_session, _book_tab, _graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);
        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        let menu = open_context_menu(&context, graph_pos, paint);
        let order = [
            tr!("project-manager-open"),
            tr!("project-manager-close"),
            tr!("action-copy"),
            tr!("action-paste"),
            tr!("action-rename-project-object"),
            tr!("action-duplicate-project-object"),
            tr!("action-delete-project-object"),
        ];
        let heights: Vec<f32> = order
            .iter()
            .map(|label| find_text_center(&menu.shapes, label).y)
            .collect();
        assert!(
            heights.windows(2).all(|pair| pair[0] < pair[1]),
            "the section order: {heights:?}"
        );
        assert!(
            !painted(&menu, &tr!("menu-selected-content")),
            "no wrapper on right click"
        );
    }

    /// #157, #326: a folder offers Add Folder, Paste, Rename and Delete
    /// only, and its renaming is edited in place like a window's.
    #[test]
    fn a_folder_right_click_offers_add_paste_rename_delete() {
        let folder = NodeId::from_raw(7);
        let tree = vec![ProjectTreeNode::Folder {
            id: folder,
            name: "Measurements".into(),
            children: Vec::new(),
        }];
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);
        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let pos = find_text_center(&first.shapes, "Measurements");
        let menu = open_context_menu(&context, pos, paint);
        for present in [
            tr!("action-add-folder"),
            tr!("action-paste"),
            tr!("action-rename-project-object"),
            tr!("action-delete-project-object"),
        ] {
            assert!(painted(&menu, &present), "{present} on a folder");
        }
        assert!(
            find_text_center(&menu.shapes, &tr!("action-add-folder")).y
                < find_text_center(&menu.shapes, &tr!("action-paste")).y,
            "the creation comes first"
        );
        for absent in [tr!("project-manager-open"), tr!("project-manager-close")] {
            assert!(!painted(&menu, &absent), "{absent} makes no sense here");
        }
        // Copying and duplicating a folder are refused for now: the entries
        // stay visible, grayed out (their click pushes nothing).
        for grayed in [tr!("action-copy"), tr!("action-duplicate-project-object")] {
            assert!(painted(&menu, &grayed), "{grayed} grayed out on a folder");
            let at = find_text_center(&menu.shapes, &grayed);
            click_at(&context, at, egui::Modifiers::NONE, paint);
            assert!(
                bench.actions.borrow().is_empty(),
                "{grayed} grayed out: no action, {:?}",
                bench.actions.borrow()
            );
            open_context_menu(&context, pos, paint);
        }
        let rename = find_text_center(&menu.shapes, &tr!("action-rename-project-object"));
        click_at(&context, rename, egui::Modifiers::NONE, paint);
        paint(&context, Vec::new());
        paint(&context, Vec::new());
        paint(&context, vec![egui::Event::Text("Series".to_string())]);
        paint(
            &context,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            *bench.actions.borrow(),
            vec![Action::RenameObjects(vec![(folder, "Series".into())])],
            "the folder is renamed in place"
        );
    }

    /// A closed window stays listed, dimmed: its color differs from that of
    /// the same open window, and its neighbors do not change; a click on the
    /// dimmed row opens it (#327).
    #[test]
    fn a_closed_window_stays_listed_but_dimmed_and_a_click_opens_it() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        bench.open.borrow_mut().extend([book_tab, graph_tab]);
        bench.paint(&context, &tree, Vec::new());
        let open_frame = bench.paint(&context, &tree, Vec::new());
        bench.open.borrow_mut().remove(&graph_tab);
        let closed_frame = bench.paint(&context, &tree, Vec::new());
        assert_ne!(
            text_color(&closed_frame.shapes, "Graph1"),
            text_color(&open_frame.shapes, "Graph1"),
            "the closed window must stand out from the open one"
        );
        assert_eq!(
            text_color(&closed_frame.shapes, "Book"),
            text_color(&open_frame.shapes, "Book"),
            "the other entries do not change"
        );
        let pos = find_text_center(&closed_frame.shapes, "Graph1");
        click_at(&context, pos, egui::Modifiers::NONE, |ctx, events| {
            bench.paint(ctx, &tree, events)
        });
        assert_eq!(*bench.open_requests.borrow(), vec![graph_tab]);
    }

    /// #326: "Add Folder" on a folder's right click creates *inside* that
    /// folder, and a window's right click does not offer the creation.
    #[test]
    fn a_folder_right_click_adds_a_folder_inside() {
        let folder = NodeId::from_raw(7);
        let tree = vec![ProjectTreeNode::Folder {
            id: folder,
            name: "Measurements".into(),
            children: Vec::new(),
        }];
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);
        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let pos = find_text_center(&first.shapes, "Measurements");
        let menu = open_context_menu(&context, pos, paint);
        let add = find_text_center(&menu.shapes, &tr!("action-add-folder"));
        click_at(&context, add, egui::Modifiers::NONE, paint);
        assert_eq!(
            *bench.actions.borrow(),
            vec![Action::AddFolder { parent: folder }],
            "in the clicked folder, not at the root"
        );

        let window = vec![ProjectTreeNode::Window {
            id: NodeId::from_raw(8),
            name: "Graph1".into(),
            kind: WindowKind::Graph,
        }];
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |context: &egui::Context, events| bench.paint(context, &window, events);
        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let pos = find_text_center(&first.shapes, "Graph1");
        let menu = open_context_menu(&context, pos, paint);
        assert!(
            painted(&menu, &tr!("action-delete-project-object")),
            "witness"
        );
        assert!(!painted(&menu, &tr!("action-add-folder")));
    }

    /// #326: after the creation, the parent expands and the new folder's name
    /// is edited in place; without the request (witness), the parent stays
    /// collapsed.
    #[test]
    fn a_new_folder_expands_its_parent_and_is_edited_in_place() {
        let parent = NodeId::from_raw(1);
        let fresh = NodeId::from_raw(5);
        let tree = vec![ProjectTreeNode::Folder {
            id: parent,
            name: "Measurements".into(),
            children: vec![ProjectTreeNode::Folder {
                id: fresh,
                name: "Folder1".into(),
                children: Vec::new(),
            }],
        }];
        let witness_context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        bench.paint(&witness_context, &tree, Vec::new());
        let output = bench.paint(&witness_context, &tree, Vec::new());
        assert!(
            !painted(&output, "Folder1"),
            "witness: the parent is collapsed"
        );

        let context = egui::Context::default();
        bench.paint(&context, &tree, Vec::new());
        begin_rename_of_new_folder(&context, egui::ViewportId::ROOT, fresh, parent);
        bench.paint(&context, &tree, Vec::new());
        let output = bench.paint(&context, &tree, Vec::new());
        assert!(painted(&output, "Folder1"), "the parent is expanded");
        let memory = context
            .data(|data| data.get_temp::<PmMemory>(memory_id(&context)))
            .expect("panel memory");
        assert_eq!(memory.rename, vec![fresh], "the name is being edited");
        assert!(memory.reveal.is_empty(), "the request is consumed");
    }

    /// A folder is collapsed by default: its children are not painted.
    /// Expanded (click on its triangle), it paints them, and an empty folder
    /// says it is empty instead of showing nothing.
    #[test]
    fn a_folder_is_collapsed_by_default_and_an_empty_folder_says_so() {
        let tree = vec![
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(1),
                name: "Measurements".into(),
                children: vec![ProjectTreeNode::Window {
                    id: NodeId::from_raw(2),
                    name: "Spectrum".into(),
                    kind: WindowKind::Graph,
                }],
            },
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(3),
                name: "Void".into(),
                children: Vec::new(),
            },
        ];
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |context: &egui::Context, events| bench.paint(context, &tree, events);
        paint(&context, Vec::new());
        let closed = paint(&context, Vec::new());
        assert!(painted(&closed, "Measurements") && painted(&closed, "Void"));
        assert!(
            !painted(&closed, "Spectrum"),
            "a collapsed folder does not paint its children"
        );
        assert!(!painted(&closed, &tr!("project-folder-empty")));
        assert!(
            bench.open_requests.borrow().is_empty(),
            "painting opens nothing"
        );

        // A folder's triangle is the first element of its row, left of its
        // icon: we click at mid-height of the row, at the left edge of the
        // tree.
        let toggle = |shapes: &[egui::epaint::ClippedShape], name: &str| {
            let label = find_text_center(shapes, name);
            let left = shapes
                .iter()
                .map(|painted| painted.shape.visual_bounding_rect())
                .filter(|rect| rect.is_finite() && (rect.center().y - label.y).abs() < 4.0)
                .map(|rect| rect.min.x)
                .fold(f32::INFINITY, f32::min);
            egui::pos2(left + 4.0, label.y)
        };
        let at = toggle(&closed.shapes, "Measurements");
        click_at(&context, at, egui::Modifiers::NONE, paint);
        let opened = paint(&context, Vec::new());
        assert!(
            painted(&opened, "Spectrum"),
            "the expanded folder paints its child"
        );
        let at = toggle(&opened.shapes, "Void");
        click_at(&context, at, egui::Modifiers::NONE, paint);
        let opened = paint(&context, Vec::new());
        assert!(painted(&opened, &tr!("project-folder-empty")));
        assert!(
            bench.open_requests.borrow().is_empty(),
            "expanding a folder opens no window"
        );
    }

    /// A slow click (S-K4) on a window already alone in the selection opens
    /// its rename; a double-click, never; the field opens on the selected
    /// name.
    #[test]
    fn a_slow_click_on_a_selected_name_renames_it_not_a_double_click() {
        let (_session, book, _graph, tree) = project_manager_fixture();
        for double in [false, true] {
            let context = egui::Context::default();
            let bench = Bench::new(windows([book]));
            let clock = std::cell::Cell::new(1.0);
            let paint = |ctx: &egui::Context, events| {
                clock.set(clock.get() + 0.01);
                bench.paint_at(ctx, &tree, events, Some(clock.get()))
            };
            let first = paint(&context, Vec::new());
            let pos = find_text_center(&first.shapes, "Book1");
            click_at(&context, pos, egui::Modifiers::NONE, paint);
            if double {
                click_at(&context, pos, egui::Modifiers::NONE, paint);
            }
            paint(&context, Vec::new());
            assert!(!context.text_edit_focused(), "not yet (double={double})");
            clock.set(clock.get() + 0.6);
            paint(&context, Vec::new());
            paint(&context, Vec::new());
            assert_eq!(
                context.text_edit_focused(),
                !double,
                "the rename opens after a slow click only (double={double})"
            );
        }
    }

    /// A name already taken by a sibling keeps the field open: Enter sends
    /// nothing; a free name is written.
    #[test]
    fn a_name_already_taken_keeps_the_field_open() {
        let (_session, book, _graph, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let bench = Bench::new(windows([book]));
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let memory_key = memory_id(&context);
        context.data_mut(|data| {
            let mut memory = data.get_temp::<PmMemory>(memory_key).unwrap();
            memory.focus = true;
            data.insert_temp(memory_key, memory);
        });
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        paint(&context, vec![key(egui::Key::F2)]);
        paint(&context, vec![egui::Event::Text("Graph1".to_string())]);
        paint(&context, vec![key(egui::Key::Enter)]);
        paint(&context, Vec::new());
        assert!(
            bench.actions.borrow().is_empty(),
            "name taken: nothing written"
        );
        assert!(context.text_edit_focused(), "the field stays open");

        paint(&context, vec![egui::Event::Text("Free".to_string())]);
        paint(&context, vec![key(egui::Key::Enter)]);
        assert_eq!(bench.actions.borrow().len(), 1);
    }

    // -- Selectable folders (#170, point 4) ----------------------------------

    fn pm_key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// The folder of `tree_with_collapsed_folder`.
    fn tree_folder() -> NodeId {
        NodeId::from_raw(3)
    }

    /// Opens the folder on the next draw (as after *Add Folder*).
    fn expand(context: &egui::Context, folder: NodeId) {
        let key = memory_id(context);
        context.data_mut(|data| {
            data.get_temp_mut_or_default::<PmMemory>(key)
                .reveal
                .push(folder);
        });
    }

    /// A click on a folder selects it and, if it is closed, expands it;
    /// it opens no window. Witness: without inserting the folder in the
    /// selection, nothing is selected.
    #[test]
    fn a_click_on_a_folder_selects_and_expands_it() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let folder = tree_folder();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(!painted(&first, "Beta"), "witness: the folder is collapsed");
        let pos = find_text_center(&first.shapes, "Folder");
        click_at(&context, pos, egui::Modifiers::NONE, paint);
        assert_eq!(
            *bench.selection.borrow(),
            HashSet::from([PmEntry::Folder(folder)])
        );
        paint(&context, vec![]);
        let after = paint(&context, vec![]);
        assert!(painted(&after, "Beta"), "the click expanded the folder");
        assert!(
            bench.open_requests.borrow().is_empty(),
            "a folder opens no window"
        );
    }

    /// A selected folder is painted like a window: pale accent background
    /// and bar on the left; hovered only, gray background. Witness: neither
    /// selected nor hovered, neither of the two.
    #[test]
    fn a_selected_folder_carries_background_bar_and_hover_like_a_window() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let folder = tree_folder();
        let context = super::super::test_support::context();
        let bench = Bench::new(HashSet::new());
        let rects = |output: &egui::FullOutput, fill: egui::Color32| -> usize {
            output
                .shapes
                .iter()
                .filter(
                    |shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == fill),
                )
                .count()
        };
        bench.paint(&context, &tree, vec![]);
        let resting = bench.paint(&context, &tree, vec![]);
        assert_eq!(rects(&resting, theme::list_row_selected_fill()), 0);
        assert_eq!(rects(&resting, theme::list_row_hover_fill()), 0);
        let pos = find_text_center(&resting.shapes, "Folder");
        let hovered = bench.paint(&context, &tree, vec![egui::Event::PointerMoved(pos)]);
        assert_eq!(
            rects(&hovered, theme::list_row_hover_fill()),
            1,
            "the hovered folder has the hover background"
        );
        bench.selection.borrow_mut().insert(PmEntry::Folder(folder));
        let chosen = bench.paint(
            &context,
            &tree,
            vec![egui::Event::PointerMoved(egui::pos2(380.0, 390.0))],
        );
        assert_eq!(rects(&chosen, theme::list_row_selected_fill()), 1);
        assert_eq!(rects(&chosen, theme::accent()), 1, "accent bar");
        assert!(
            super::super::test_support::texts(&chosen.shapes)
                .iter()
                .any(|t| t.text == "Folder" && t.bold),
            "label in bold"
        );
    }

    /// A click on an open folder selects it **without** collapsing it, even
    /// when it is already alone in the selection. Witness: a click on a closed
    /// folder opens it (other test).
    #[test]
    fn a_click_on_an_open_folder_selects_it_without_collapsing_it() {
        let (tree, [alpha, ..]) = tree_with_collapsed_folder();
        let folder = tree_folder();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let clock = std::cell::Cell::new(1.0);
        let paint = |ctx: &egui::Context, events| {
            clock.set(clock.get() + 0.01);
            bench.paint_at(ctx, &tree, events, Some(clock.get()))
        };
        expand(&context, folder);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(painted(&first, "Beta"), "precondition: folder open");
        let alpha_pos = find_text_center(&first.shapes, "Alpha");
        let dossier_pos = find_text_center(&first.shapes, "Folder");
        click_at(&context, alpha_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*bench.selection.borrow(), windows([alpha]));
        for _ in 0..2 {
            // Outside the double-click interval: two single clicks.
            clock.set(clock.get() + 2.0);
            click_at(&context, dossier_pos, egui::Modifiers::NONE, paint);
            // A collapse would be animated: we let time pass.
            clock.set(clock.get() + 0.3);
            paint(&context, vec![]);
            let after = paint(&context, vec![]);
            assert_eq!(
                *bench.selection.borrow(),
                HashSet::from([PmEntry::Folder(folder)])
            );
            assert!(painted(&after, "Beta"), "the click does not collapse");
        }
    }

    /// A double-click collapses the folder, whether it was open or closed at
    /// the start (the first click of a double-click expands a closed folder).
    #[test]
    fn a_double_click_collapses_the_folder() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let folder = tree_folder();
        for open_at_start in [false, true] {
            // Fresh context: see `double_click_activates_the_entry_without_modifier`.
            let context = egui::Context::default();
            let bench = Bench::new(HashSet::new());
            let clock = std::cell::Cell::new(1.0);
            let paint = |ctx: &egui::Context, events| {
                clock.set(clock.get() + 0.01);
                bench.paint_at(ctx, &tree, events, Some(clock.get()))
            };
            if open_at_start {
                expand(&context, folder);
            }
            paint(&context, vec![]);
            let first = paint(&context, vec![]);
            assert_eq!(painted(&first, "Beta"), open_at_start);
            let pos = find_text_center(&first.shapes, "Folder");
            click_at(&context, pos, egui::Modifiers::NONE, paint);
            click_at(&context, pos, egui::Modifiers::NONE, paint);
            // The collapse is animated: we let time pass.
            clock.set(clock.get() + 2.0);
            paint(&context, vec![]);
            let after = paint(&context, vec![]);
            assert!(
                !painted(&after, "Beta"),
                "double-click: collapsed (open at start: {open_at_start})"
            );
            assert_eq!(
                *bench.selection.borrow(),
                HashSet::from([PmEntry::Folder(folder)])
            );
        }
    }

    /// The triangle collapses an open folder (egui's gesture, kept).
    #[test]
    fn the_triangle_collapses_an_open_folder() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let folder = tree_folder();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let clock = std::cell::Cell::new(1.0);
        let paint = |ctx: &egui::Context, events| {
            clock.set(clock.get() + 0.01);
            bench.paint_at(ctx, &tree, events, Some(clock.get()))
        };
        expand(&context, folder);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(painted(&first, "Beta"));
        let label = find_text_center(&first.shapes, "Folder");
        let triangle = egui::pos2(label.x - 62.0, label.y);
        click_at(&context, triangle, egui::Modifiers::NONE, paint);
        // The collapse is animated: we let time pass.
        clock.set(clock.get() + 2.0);
        paint(&context, vec![]);
        let after = paint(&context, vec![]);
        assert!(!painted(&after, "Beta"), "the triangle collapses");
        assert!(
            bench.selection.borrow().is_empty(),
            "the triangle does not select"
        );
    }

    /// Delete on a selected folder deletes the folder (one history entry,
    /// `DeleteObjects`); F2 opens its in-place rename.
    #[test]
    fn delete_and_f2_act_on_a_selected_folder() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let folder = tree_folder();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let pos = find_text_center(&first.shapes, "Folder");
        click_at(&context, pos, egui::Modifiers::NONE, paint);
        bench.actions.borrow_mut().clear();
        paint(&context, vec![pm_key(egui::Key::Delete)]);
        assert_eq!(
            bench.actions.take(),
            vec![Action::DeleteObjects(vec![folder])]
        );
        paint(&context, vec![pm_key(egui::Key::F2)]);
        paint(&context, vec![]);
        assert_eq!(
            renaming_in(&context, context.viewport_id()),
            vec![folder],
            "F2 opens the folder rename"
        );
        paint(&context, vec![egui::Event::Text("Sample".to_string())]);
        paint(&context, vec![pm_key(egui::Key::Enter)]);
        assert_eq!(
            bench.actions.take(),
            vec![Action::RenameObjects(vec![(folder, "Sample".into())])]
        );
    }

    /// Copying and duplicating a folder are refused (see the head doc):
    /// Ctrl+C and Ctrl+D pass the gesture on to the state, which refuses it
    /// with its reason (`stokesforge-app-state/tests/project_objects.rs`); the
    /// menu entry is grayed out (test of a folder's right click).
    #[test]
    fn ctrl_c_on_a_folder_goes_through_the_state_which_refuses_it() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let folder = tree_folder();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let pos = find_text_center(&first.shapes, "Folder");
        click_at(&context, pos, egui::Modifiers::NONE, paint);
        bench.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Copy]);
        assert_eq!(
            bench.actions.take(),
            vec![Action::CopyObjects(vec![folder])]
        );
    }

    /// Ctrl+V with a single folder selected pastes **into** that folder;
    /// with a window selected, at the root as before. Witness: the two
    /// parents differ.
    #[test]
    fn ctrl_v_with_a_selected_folder_pastes_into_the_folder() {
        let (tree, [alpha, ..]) = tree_with_collapsed_folder();
        let folder = tree_folder();
        let root = NodeId::from_raw(1);
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| bench.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let dossier_pos = find_text_center(&first.shapes, "Folder");
        let alpha_pos = find_text_center(&first.shapes, "Alpha");
        click_at(&context, dossier_pos, egui::Modifiers::NONE, paint);
        bench.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Paste("data".into())]);
        assert_eq!(
            bench.actions.take(),
            vec![Action::PasteObjects {
                parent: folder,
                text: "data".into()
            }]
        );
        click_at(&context, alpha_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*bench.selection.borrow(), windows([alpha]));
        bench.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Paste("data".into())]);
        assert_eq!(
            bench.actions.take(),
            vec![Action::PasteObjects {
                parent: root,
                text: "data".into()
            }]
        );
    }

    /// `paste_parent`: the single folder, otherwise the root.
    #[test]
    fn the_paste_parent_is_the_single_folder_or_the_root() {
        let root = NodeId::from_raw(1);
        let folder = PmEntry::Folder(NodeId::from_raw(3));
        let window = PmEntry::Window(Tab::Book(NodeId::from_raw(2)));
        assert_eq!(
            paste_parent(&HashSet::from([folder]), root),
            NodeId::from_raw(3)
        );
        assert_eq!(paste_parent(&HashSet::from([window]), root), root);
        assert_eq!(paste_parent(&HashSet::from([folder, window]), root), root);
        assert_eq!(paste_parent(&HashSet::new(), root), root);
    }

    // -- Keyboard navigation (#170, point 6) ----------------------------------

    fn key_with(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// A bench whose panel has the focus (a click on "Alpha"), the tree
    /// `tree_with_collapsed_folder`: Alpha, Folder (collapsed: Beta, Gamma),
    /// Delta. `f` receives the bench, the context and the painter.
    fn with_focused_panel(
        f: impl FnOnce(
            &Bench,
            &egui::Context,
            &dyn Fn(&egui::Context, Vec<egui::Event>) -> egui::FullOutput,
            [Tab; 4],
        ),
    ) {
        let (tree, tabs) = tree_with_collapsed_folder();
        let context = super::super::test_support::context();
        let bench = Bench::new(HashSet::new());
        let clock = std::cell::Cell::new(1.0);
        let paint = |ctx: &egui::Context, events| {
            clock.set(clock.get() + 0.1);
            bench.paint_at(ctx, &tree, events, Some(clock.get()))
        };
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let alpha = find_text_center(&first.shapes, "Alpha");
        click_at(&context, alpha, egui::Modifiers::NONE, paint);
        paint(&context, vec![]);
        bench.open_requests.borrow_mut().clear();
        f(&bench, &context, &paint, tabs);
    }

    /// ↓ twice from Alpha reaches the **third visible row** (Delta), the
    /// collapsed folder counting for one row and its children for none;
    /// the arrow requests neither an opening nor an action. Witness: following
    /// `order` (hidden children included) would stop on Beta.
    #[test]
    fn two_downs_reach_the_third_visible_row() {
        with_focused_panel(|bench, context, paint, [alpha, _beta, _gamma, delta]| {
            assert_eq!(*bench.selection.borrow(), windows([alpha]));
            paint(
                context,
                vec![key_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *bench.selection.borrow(),
                HashSet::from([PmEntry::Folder(tree_folder())]),
                "the first arrow goes to the folder"
            );
            paint(
                context,
                vec![key_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *bench.selection.borrow(),
                windows([delta]),
                "the second skips the children of the collapsed folder"
            );
            assert!(
                bench.open_requests.borrow().is_empty(),
                "an arrow opens nothing"
            );
            assert!(
                bench.actions.borrow().is_empty(),
                "neither action nor history"
            );
            // At the bottom of the list: ↓ stays in place; ↑ goes up one visible row.
            paint(
                context,
                vec![key_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_eq!(*bench.selection.borrow(), windows([delta]));
            paint(
                context,
                vec![key_with(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *bench.selection.borrow(),
                HashSet::from([PmEntry::Folder(tree_folder())])
            );
        });
    }

    /// The arrows apply only to a panel that has the focus: without a click
    /// inside, ↓ moves nothing and stays readable by the rest of the application.
    #[test]
    fn without_focus_the_arrows_do_not_touch_the_panel() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let context = egui::Context::default();
        let bench = Bench::new(HashSet::new());
        bench.paint(&context, &tree, vec![]);
        bench.paint(
            &context,
            &tree,
            vec![key_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
        );
        assert!(bench.selection.borrow().is_empty());
        assert!(
            bench.arrow_down_left.get(),
            "the key stays available to whoever wants it"
        );
    }

    /// From the focused panel, the arrow is consumed: a workbook's grid,
    /// painted afterwards, reads the arrows without consuming them and would
    /// move its cell at the same time. Witness: without focus, the arrow stays readable.
    #[test]
    fn a_panel_arrow_does_not_reach_the_grid() {
        with_focused_panel(|bench, context, paint, _| {
            paint(
                context,
                vec![key_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert!(
                !bench.arrow_down_left.get(),
                "↓ consumed by the panel: the grid does not see it"
            );
        });
    }

    /// Shift+↓ and Alt+↓ do not pass for ↓ (egui ignores extra Shift and Alt):
    /// they are consumed and do not move the selection. Witness: ↓
    /// alone moves it.
    #[test]
    fn shift_and_alt_down_do_not_move_the_selection() {
        with_focused_panel(|bench, context, paint, [alpha, ..]| {
            for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::ALT] {
                paint(context, vec![key_with(egui::Key::ArrowDown, modifiers)]);
                assert_eq!(*bench.selection.borrow(), windows([alpha]), "{modifiers:?}");
                assert!(!bench.arrow_down_left.get(), "{modifiers:?} consumed");
            }
            paint(
                context,
                vec![key_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_ne!(*bench.selection.borrow(), windows([alpha]), "witness");
        });
    }

    /// Enter opens the selected window (`open_requests`), expands a closed
    /// folder and collapses an open one. Witness: without Enter, nothing.
    #[test]
    fn enter_opens_the_window_and_toggles_the_folder() {
        with_focused_panel(|bench, context, paint, [alpha, ..]| {
            paint(context, vec![]);
            assert!(bench.open_requests.borrow().is_empty(), "witness");
            paint(
                context,
                vec![key_with(egui::Key::Enter, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *bench.open_requests.borrow(),
                vec![alpha],
                "Enter opens Alpha"
            );

            paint(
                context,
                vec![key_with(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            let closed_frame = paint(context, vec![]);
            assert!(
                !painted(&closed_frame, "Beta"),
                "precondition: folder collapsed"
            );
            paint(
                context,
                vec![key_with(egui::Key::Enter, egui::Modifiers::NONE)],
            );
            let open_frame = paint(context, vec![]);
            assert!(painted(&open_frame, "Beta"), "Enter expands the folder");
            assert_eq!(
                bench.open_requests.borrow().len(),
                1,
                "a folder opens no window"
            );
            paint(
                context,
                vec![key_with(egui::Key::Enter, egui::Modifiers::NONE)],
            );
            // The collapse is animated: we let time pass.
            let mut after = paint(context, vec![]);
            for _ in 0..10 {
                after = paint(context, vec![]);
            }
            assert!(!painted(&after, "Beta"), "Enter collapses the open folder");
        });
    }

    /// The row that carries the keyboard cursor has the focus outline of
    /// `selection.stroke`, like a list row that has egui's focus;
    /// without panel focus, no outline. Witness: the outline removed from the
    /// painting of rows makes this test fail.
    #[test]
    fn the_keyboard_cursor_row_has_the_focus_outline() {
        let (tree, _tabs) = tree_with_collapsed_folder();
        let context = super::super::test_support::context();
        let bench = Bench::new(HashSet::new());
        let outline = |output: &egui::FullOutput| -> usize {
            let stroke = context.global_style().visuals.selection.stroke;
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke == stroke)
                })
                .count()
        };
        bench.paint(&context, &tree, vec![]);
        let first = bench.paint(&context, &tree, vec![]);
        bench
            .selection
            .borrow_mut()
            .insert(PmEntry::Folder(tree_folder()));
        let without_focus = bench.paint(&context, &tree, vec![]);
        assert_eq!(
            outline(&without_focus),
            0,
            "panel without focus: no outline"
        );
        let pos = find_text_center(&first.shapes, "Alpha");
        click_at(&context, pos, egui::Modifiers::NONE, |ctx, events| {
            bench.paint(ctx, &tree, events)
        });
        let with_focus = bench.paint(&context, &tree, vec![]);
        assert_eq!(outline(&with_focus), 1, "a single row, the cursor's");
    }

    /// `neighbour`: bounded visible neighbor; without a cursor ↓ goes to the start and ↑
    /// to the end; empty list, nothing.
    #[test]
    fn the_neighbor_row_is_bounded_and_without_a_cursor_we_start_from_one_end() {
        use super::super::list_row::neighbour;
        let rows = [10, 20, 30];
        assert_eq!(neighbour(&rows, Some(20), 1), Some(30));
        assert_eq!(neighbour(&rows, Some(20), -1), Some(10));
        assert_eq!(
            neighbour(&rows, Some(30), 1),
            Some(30),
            "bounded at the bottom"
        );
        assert_eq!(
            neighbour(&rows, Some(10), -1),
            Some(10),
            "bounded at the top"
        );
        assert_eq!(neighbour(&rows, None, 1), Some(10));
        assert_eq!(neighbour(&rows, None, -1), Some(30));
        assert_eq!(neighbour(&rows, Some(99), 1), Some(10), "invisible cursor");
        assert_eq!(neighbour::<i32>(&[], Some(1), 1), None);
    }
}

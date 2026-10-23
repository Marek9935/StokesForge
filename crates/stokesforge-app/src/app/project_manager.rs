//! Arbre du Project Manager : une seule écriture pour le panneau fixe et la
//! fenêtre OS détachée, sur un arbre déjà construit
//! ([`stokesforge_app_state::state::ProjectTreeNode`]). Ce que les clics demandent (ouvrir,
//! fermer, sélectionner) s'accumule dans
//! [`PmInteraction`], appliqué par `StokesForgeApp` après la peinture.
//!
//! Le titre appartient au cadre partagé `panel_header`, pas à l'arbre :
//! attaché ou détaché, aucun double en-tête et les mêmes gestes (#100).
//! Une sélection reprise par la fenêtre active doit aussi déplacer l'ancre
//! de Maj-clic dans les deux viewports : le seul fond sélectionné ne suffit
//! pas à définir la prochaine plage après un clic dans le workspace (#154).
//! Open et Close vivent dans le menu contextuel d'une entrée et portent
//! sur la sélection entière ; l'arbre commence directement sous l'en-tête.
//!
//! Une entrée sélectionnable est un dossier **ou** une fenêtre ([`PmEntry`],
//! #170 point 4). Un clic sur un dossier le sélectionne et, s'il est fermé, le
//! déplie ; un clic sur un dossier ouvert ne le replie pas (un clic de
//! sélection ne doit pas faire sauter l'arbre) ; le double-clic et le
//! triangle le replient. Supprimer et Renommer agissent sur lui, une sélection
//! de dossier collée **dans** ce dossier. **Copier et dupliquer un dossier sont
//! refusés pour l'instant** : une copie locale garderait ses références vers
//! les classeurs d'origine (hors dossier) ; l'entrée de menu est grisée avec sa
//! raison, et `AppState` refuse le geste au clavier avec la même phrase. À
//! reprendre dans une issue à part.
//!
//! **Navigation au clavier (#170, point 6).** Panneau focalisé (`PmMemory::focus`,
//! posé par un clic), ↑ et ↓ déplacent la sélection d'une ligne **visible** à
//! l'autre (`PmMemory::visible`, relevée à la peinture : les dossiers repliés
//! sont sautés, contrairement à `order`, qui sert aux plages Maj), et Entrée
//! ouvre la ou les fenêtres sélectionnées, déplie ou replie les dossiers. La
//! flèche ne touche à aucune fenêtre ni à l'historique. Les touches sont
//! consommées ici, avant la grille d'un classeur (peinte après le panneau), qui
//! lit les flèches sans les consommer ; les variantes Maj et Alt+↓, qu'egui
//! confondrait avec ↑ et ↓ (il ignore Maj et Alt en trop), sont consommées
//! d'abord et laissées sans effet. La ligne qui porte le curseur clavier a le
//! contour de focus des lignes de liste (`list_row::paint_focus_outline`).

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

/// Ce que le rendu de l'arbre du Project Manager accumule ou lit, à travers
/// [`project_manager_contents_from_tree`]/[`project_tree_node_ui`] — un seul
/// paquet plutôt que cinq paramètres, pour les deux mêmes contextes que le
/// reste du module (panneau fixe et fenêtre OS détachée, voir la doc de tête
/// de [`StokesForgeApp::show_detached_project_manager`]).
pub(super) struct PmInteraction<'a> {
    pub(super) actions: &'a mut Vec<Action>,
    pub(super) root: NodeId,
    /// Onglet ouvert ou focalisé par un simple clic — voir
    /// [`StokesForgeApp::open_tab`].
    pub(super) open_requests: &'a mut Vec<Tab>,
    /// Sélection courante dans l'arbre — Ctrl/Cmd-clic en bascule
    /// l'appartenance sans ouvrir l'onglet.
    pub(super) selection: &'a mut HashSet<PmEntry>,
    /// Onglets actuellement ouverts (flottants ou détachés) : les autres
    /// lignes sont atténuées dans l'arbre (#327).
    pub(super) open: &'a HashSet<Tab>,
    /// Onglets que « Close » demande de fermer, par le même chemin que la
    /// croix de la fenêtre (flottante ou détachée).
    pub(super) close_requests: &'a mut Vec<Tab>,
}

/// La sélection est partagée avec le viewport ; l'éditeur et le focus restent
/// dans la mémoire du viewport qui reçoit le clavier.
#[derive(Clone, Default)]
struct PmMemory {
    focus: bool,
    anchor: Option<PmEntry>,
    rename: Vec<NodeId>,
    renamed: Vec<(NodeId, String)>,
    text: Option<String>,
    /// Dernier clic simple sur une fenêtre déjà seule sélectionnée : s'il n'est
    /// pas suivi d'un second clic avant [`SLOW_CLICK_SECONDS`], c'est un clic
    /// lent, donc un renommage (S-K4, #224).
    slow_click: Option<(PmEntry, f64)>,
    /// Les dossiers et fenêtres de l'arbre dans l'ordre de lecture, dossiers
    /// repliés compris (#327) : la plage Maj et les ids agissants en dépendent.
    order: Vec<PmEntry>,
    /// Dossiers à déplier à leur prochaine peinture (#326) : le parent d'un
    /// dossier qu'on vient de créer doit montrer son nouvel enfant.
    reveal: Vec<NodeId>,
    /// Les lignes **visibles** de l'image précédente, dans l'ordre de lecture :
    /// ce que ↑ et ↓ parcourent (un dossier replié cache ses enfants, à la
    /// différence de `order`).
    visible: Vec<PmEntry>,
    /// Dossiers que Entrée doit déplier ou replier à leur prochaine peinture.
    toggle: Vec<NodeId>,
    /// Ligne que le clavier vient d'atteindre : à faire apparaître dans la
    /// zone de défilement à sa prochaine peinture.
    scroll_to: Option<PmEntry>,
}

/// Une entrée sélectionnable de l'arbre du Project Manager (#170, point 4) :
/// un dossier, ou une fenêtre (son onglet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PmEntry {
    Folder(NodeId),
    Window(Tab),
}

impl PmEntry {
    /// Le nœud du modèle que désigne l'entrée.
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

/// Les dossiers et fenêtres de `tree`, en parcours en profondeur, un dossier
/// avant ses enfants.
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

/// Où se colle le presse-papiers : **dans** le dossier quand la sélection est
/// ce seul dossier, à la racine sinon (comme avant #170).
pub(super) fn paste_parent(selection: &HashSet<PmEntry>, root: NodeId) -> NodeId {
    let mut entries = selection.iter();
    match (entries.next(), entries.next()) {
        (Some(PmEntry::Folder(id)), None) => *id,
        _ => root,
    }
}

/// La ligne qui porte le curseur clavier : l'ancre du dernier geste si elle est
/// sélectionnée, sinon l'unique entrée sélectionnée.
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

/// Note qu'une ligne est visible dans cette image (voir `PmMemory::visible`).
fn note_visible(ctx: &egui::Context, memory_key: egui::Id, entry: PmEntry) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<PmMemory>(memory_key)
            .visible
            .push(entry);
    });
}

/// La sélection contient-elle un dossier (copie refusée, voir la doc de tête) ?
pub(super) fn selection_has_folder(selection: &HashSet<PmEntry>) -> bool {
    selection.iter().any(|entry| entry.is_folder())
}

pub(super) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("stokesforge_project_manager_window")
}

/// Aucun éditeur ne peut survivre à un remplacement d'identité du projet.
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

/// Une activation externe remplace l'ancre, sans toucher au focus ni à
/// l'éditeur du panneau. Les gestes internes gardent leur propre ancre,
/// notamment lorsqu'une plage Maj revient à une seule entrée.
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

/// Après *Add Folder* (#326) : déplie `parent` et ouvre le champ de nom sur
/// `folder`, dans le panneau qui est à l'écran (`viewport`).
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

/// Les noms en cours d'édition dans le panneau de `viewport` (tests).
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

/// La cible d'un menu d'objets du Project Manager : des fenêtres, ou un
/// dossier (qui offre en plus Add Folder, et dont Copier et Dupliquer sont
/// grisés pour l'instant, comme pour toute sélection qui en contient un).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ObjectMenuTarget {
    Windows,
    Folder,
}

/// Le contenu du menu d'objets (#157), **à plat** : le clic droit l'affiche
/// tel quel, Edit › Selection l'enveloppe. Sections de `docs/menus.md` —
/// presse-papiers (Copy · Paste), organiser (Rename · Duplicate), supprimer.
/// `sections` est celui du menu appelant : les entrées propres au panneau
/// (Open · Close) se peignent avant, sous le même compte de séparateurs.
/// Le collage déclenche une demande de lecture à egui-winit, donc utilise
/// aussi le récepteur Wayland. `has_folder` : la sélection contient un dossier,
/// donc Copy et Duplicate sont grisés (copie de dossier refusée, #170).
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
    // Copier et dupliquer : refusés dès qu'un dossier est visé (voir la doc de tête).
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
        // Seul geste qui crée dans un sous-dossier (#326, `docs/menus.md`).
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

/// Délai d'un clic lent (S-K4) : plus long que l'intervalle d'un double-clic
/// d'egui, pour qu'un double-clic (ouvrir) n'ouvre jamais le champ.
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
    // Clic lent : le délai écoulé sans second clic, le nom se renomme comme à F2.
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
                // Comme « Open » du menu : chaque fenêtre sélectionnée s'ouvre,
                // chaque dossier bascule.
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

/// Le corps du Project Manager, à partir d'un arbre déjà construit plutôt
/// que d'un `&AppState` vivant — seul le callback `show_viewport_deferred`
/// de [`StokesForgeApp::show_detached_project_manager`] a besoin de cette forme :
/// il ne porte qu'une copie de l'arbre ([`ProjectManagerViewModel`](crate::app::window_detach::ProjectManagerViewModel)), jamais
/// `AppState` elle-même. Le panneau fixe ([`StokesForgeApp::project_manager_panel`])
/// passe directement `&self.state.project_tree()` — même fonction, deux
/// sources d'arbre.
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
        // Un dossier supprimé ne reste pas sélectionné (un script, Ctrl+Z).
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
    // Une demande de clavier ne vaut que pour l'image qui suit : un dossier
    // caché (replié sous un autre) ne doit pas la garder en réserve.
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

/// Vrai si `id` est en cours de renommage dans ce panneau.
fn is_renaming(ctx: &egui::Context, memory_key: egui::Id, id: NodeId) -> bool {
    ctx.data(|data| data.get_temp::<PmMemory>(memory_key))
        .is_some_and(|m| m.rename.first() == Some(&id))
}

/// Pourquoi ce nom est refusé (S-T4, cadre rouge) : vide, ou déjà pris par un
/// autre enfant du même dossier.
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

/// Le champ de renommage en place d'une ligne (fenêtre ou dossier), au contrat
/// commun de [`inplace_edit`](super::inplace_edit) : tout le nom sélectionné,
/// Entrée, Tab ou un clic ailleurs valident et passent à la ligne suivante de la
/// sélection, Échap abandonne tout. Un nom vide ou déjà pris reste ouvert sous
/// un cadre rouge. Tous les noms saisis partent en une seule
/// `Action::RenameObjects` après la dernière ligne.
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

/// Ce qu'une ligne a reçu comme geste de sélection (voir [`select_gestures`]).
#[derive(Default)]
struct RowGestures {
    /// Clic simple sans modificateur : la ligne devient la seule sélectionnée.
    plain: bool,
    /// Double-clic sans modificateur.
    double: bool,
}

/// Les gestes de sélection d'une ligne, communs aux dossiers et aux fenêtres :
/// clic droit (sélectionne si besoin), Maj (plage), Ctrl/Cmd (bascule), clic
/// simple (remplace, arme le clic lent du renommage) et double-clic (désarme
/// le clic lent). Ce que le clic simple **fait** ensuite — ouvrir la fenêtre,
/// déplier le dossier — est à l'appelant.
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
                // L'ordre complet de l'image précédente reste disponible dans
                // la liste de rangs des lignes, y compris après la cible.
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
        // La demande d'ouverture s'applique après la peinture : sans
        // image réclamée ici, le surlignage et la fenêtre active
        // n'apparaîtraient qu'au prochain mouvement de souris.
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

/// La ligne qui porte le contour de focus : le curseur clavier, tant que le
/// panneau a le focus (`PmMemory::focus`, posé par un clic dedans). Rend `None`
/// panneau non focalisé ou en renommage.
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

/// Fait défiler la zone jusqu'à la ligne que le clavier vient d'atteindre.
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

/// Peint récursivement un nœud de [`ProjectTreeNode`] et ses enfants ; rend
/// le centre vertical de sa rangée, pour les connecteurs de son parent.
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
                // Entrée (#170, point 6) : déplie un dossier fermé, replie un ouvert.
                state.set_open(!state.is_open());
            }
            // #170 : la ligne d'en-tête a la hauteur d'une ligne de liste ; la
            // hauteur est rendue après le corps (la réponse emprunte l'`Ui`).
            let previous_height = raise_to_list_row_height(ui);
            let mut gestures = RowGestures::default();
            let mut header = state.show_header(ui, |ui| {
                if is_renaming(ui.ctx(), memory_key, *id) {
                    ui.add(icon_image(Icon::Folder, IconRole::Folder));
                    rename_editor(ui, interaction, memory_key, *id, name, siblings);
                    return;
                }
                // #170 point 4 : un dossier est une ligne de liste comme une
                // fenêtre (fond, filet, survol), sélectionnable.
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
            // Un clic sur un dossier fermé le déplie, sur un dossier ouvert il
            // ne le replie pas ; le double-clic le replie (le triangle aussi).
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
            // Le salt par onglet évite que toutes les lignes partagent le même Id.
            let response = ui
                .push_id(tab, |ui| {
                    // #169 : une ligne de liste (`list_row`), jamais un bouton `.selected()`.
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
            // S-K3 : un double-clic active ou restaure la fenêtre, il ne la
            // ferme jamais (fermer se fait par Close ou l'en-tête).
            if gestures.double {
                interaction.open_requests.push(tab);
            }
            response.rect.center().y
        }
    }
}

/// Largeur par défaut du panneau Project Manager fixe, et de la fenêtre OS
/// séparée quand il est détaché.
pub(super) const PROJECT_MANAGER_WIDTH: f32 = 260.0;

impl StokesForgeApp {
    /// #146 : suivre seulement les changements de fenêtre active. Refaire la
    /// sélection à chaque image écraserait le clic du panneau avant que sa
    /// demande d'ouverture soit appliquée. Une sélection multiple reste à
    /// l'utilisateur tant qu'aucun clic explicite sur une entrée ou une fenêtre
    /// du workspace ne la remplace (#154, `Workspace::activate_clicked_tab`).
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

    /// Peint le Project Manager dans son panneau fixe, ancré à gauche — voir
    /// la doc de tête d'`app.rs`, § Ce qui reste ici (l'ordre des panneaux).
    /// Appelé par
    /// [`StokesForgeApp::ui`](eframe::App::ui) *avant* le `CentralPanel`, mais
    /// *après* [`StokesForgeApp::script_panel`] — voir sa doc de tête pour
    /// pourquoi cet ordre précis. Comme [`StokesForgeApp::menu_bar`]
    /// (`egui::Panel::top`) : peindre un panneau ancré avant le reste
    /// garantit par construction qu'il n'est jamais recouvert ni ne recouvre
    /// ce qui vient après. N'est pas appelé quand [`StokesForgeApp::pm_detached`]
    /// vaut `true` — cette image-là, rien ne remplace ce panneau, la vraie
    /// fenêtre OS séparée ([`StokesForgeApp::show_detached_project_manager`]) en
    /// tient lieu. Reste redimensionnable en largeur par glisser du bord
    /// (comportement par défaut d'`egui::Panel::left`) mais figé en
    /// position : pas de bascule vers la droite ni de repositionnement par
    /// glisser. Une bande séparée au repli conserve la largeur déployée.
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

/// `pub(super)` : [`tests::find_text_center`] et [`tests::click_at`] servent
/// aussi aux tests des autres modules d'`app` qui cliquent un texte peint.
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

    /// Trouve le centre du texte peint contenant `needle` dans `shapes` —
    /// utilisé pour cliquer une entrée du Project Manager sans supposer sa
    /// position exacte (mise en page auto), et par les tests des autres
    /// modules d'`app` (`graph_window.rs`, `format.rs`, `menus.rs`…). Même
    /// technique que
    /// `window_chrome::tests::len_tete_du_graphe_garde_son_titre_au_centre_et_sa_fleche_a_gauche`.
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
            .unwrap_or_else(|| panic!("aucun texte contenant {needle:?} n'a été peint"))
    }

    /// Un clic (glissé nul, pression puis relâchement au même endroit) sur
    /// `pos`, réparti sur deux images comme le veut la détection de clic
    /// d'egui — voir
    /// `graph_window::tests::le_pan_principal_atteint_la_figure_dans_la_fenetre_flottante`
    /// pour le même schéma appliqué à un glissé.
    pub(in crate::app) fn click_at<R>(
        context: &egui::Context,
        pos: egui::Pos2,
        modifiers: egui::Modifiers,
        mut paint: impl FnMut(&egui::Context, Vec<egui::Event>) -> R,
    ) {
        // `PointerButton::modifiers` n'alimente pas `InputState::modifiers`
        // (voir `InputState::begin_pass`, qui ne le lit que sur
        // `Event::ModifiersChanged`/`Event::Key`) : sans cet événement
        // explicite, `ui.input(|i| i.modifiers.command)` resterait toujours
        // faux, quels que soient les `modifiers` posés sur les événements
        // de pointeur eux-mêmes.
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

    /// Construit l'arbre d'un projet Book+Graph, lu par `AppState` comme le
    /// panneau le lit — commun aux tests de double-clic et de sélection
    /// multiple du Project Manager.
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
            .expect("le projet du fixture porte un classeur");
        (session, Tab::Book(book_id), Tab::Graph(graph), tree)
    }

    /// Une sélection de fenêtres (les tests historiques n'en ont pas d'autre).
    pub(in crate::app) fn fenetres<const N: usize>(tabs: [Tab; N]) -> HashSet<PmEntry> {
        tabs.into_iter().map(PmEntry::Window).collect()
    }

    /// Les onglets des fenêtres d'une sélection (les dossiers sont écartés).
    fn onglets(selection: &HashSet<PmEntry>) -> HashSet<Tab> {
        selection
            .iter()
            .filter_map(|entry| match entry {
                PmEntry::Window(tab) => Some(*tab),
                PmEntry::Folder(_) => None,
            })
            .collect()
    }

    /// Les accumulateurs de [`PmInteraction`] d'un test, et une image peinte
    /// de l'arbre sur un écran de 400 × 400.
    struct Banc {
        actions: RefCell<Vec<Action>>,
        open_requests: RefCell<Vec<Tab>>,
        selection: RefCell<HashSet<PmEntry>>,
        open: RefCell<HashSet<Tab>>,
        close_requests: RefCell<Vec<Tab>>,
        /// ↓ était-elle encore lisible après le panneau, dans la même image ?
        /// Ce qu'une grille peinte ensuite verrait (elle lit les flèches sans
        /// les consommer, #170 point 6).
        arrow_down_left: std::cell::Cell<bool>,
    }

    impl Banc {
        fn new(selection: HashSet<PmEntry>) -> Self {
            Banc {
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

        /// Comme [`Banc::paint`], à l'instant `time` : les gestes qui dépendent
        /// du délai (clic lent) ne peuvent pas se piloter à l'horloge réelle.
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

    /// #170, point 5 : l'en-tête d'un dossier a la même hauteur qu'une fenêtre
    /// (`LIST_ROW_HEIGHT`) ; les connecteurs en L visent le centre de la ligne.
    /// Mesuré par le pas entre les centres de lignes consécutives (dossier ->
    /// fenêtre -> fenêtre -> dossier) : constant, et égal à la hauteur de ligne
    /// plus l'écart vertical. Témoin : sans la hauteur imposée à l'en-tête, le
    /// pas du dossier à la fenêtre est plus court.
    #[test]
    fn un_dossier_a_la_hauteur_d_une_fenetre() {
        use super::super::list_row::LIST_ROW_HEIGHT;
        let mut project = Project::new("Hauteurs").unwrap();
        let root = project.root();
        project.add_folder(root, "Dossier A").unwrap();
        project.add_graph(root, None, PlotSpec::default()).unwrap();
        project.add_note(root, None, "").unwrap();
        project.add_folder(root, "Dossier B").unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let tree = state.project_tree();
        let context = super::super::test_support::contexte();
        let banc = Banc::new(HashSet::new());
        banc.paint(&context, &tree, Vec::new());
        let output = banc.paint(&context, &tree, Vec::new());
        let mut centres: Vec<f32> = super::super::test_support::textes(&output.shapes)
            .iter()
            .filter(|t| {
                t.texte.starts_with("Dossier")
                    || t.texte.contains("Graph")
                    || t.texte.contains("Note")
            })
            .map(|t| t.centre.y)
            .collect();
        centres.sort_by(f32::total_cmp);
        assert_eq!(centres.len(), 4, "{centres:?}");
        let pas: Vec<f32> = centres.windows(2).map(|w| w[1] - w[0]).collect();
        let spacing = context.global_style().spacing.item_spacing.y;
        for p in &pas {
            assert!(
                (p - (LIST_ROW_HEIGHT + spacing)).abs() < 1.0,
                "pas {p} attendu {} : {pas:?}",
                LIST_ROW_HEIGHT + spacing
            );
        }
    }

    /// #169 : une entrée sélectionnée est une ligne de liste — fond pâle
    /// d'accent sur toute la largeur restante et filet d'accent à gauche —
    /// et non plus le fond `selection.bg_fill` des boutons, que gardent les
    /// autres sélections de l'application. Témoin : sans sélection, ni fond
    /// ni filet.
    #[test]
    fn la_selection_du_project_manager_est_une_ligne_de_liste_a_filet() {
        let mut project = Project::new("Sélection en liste").unwrap();
        let root = project.root();
        let book = Tab::Book(project.add_empty_workbook(root, None).unwrap());
        let graph = Tab::Graph(project.add_graph(root, None, PlotSpec::default()).unwrap());
        let note = Tab::Note(project.add_note(root, None, "").unwrap());
        let matrix = Tab::Matrix(project.add_matrix(root, None).unwrap());
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stf".to_string()).unwrap();
        let tree = state.project_tree();
        let context = super::super::test_support::contexte();
        let banc = Banc::new(fenetres([book, graph, note, matrix]));
        banc.paint(&context, &tree, Vec::new());
        let output = banc.paint(&context, &tree, Vec::new());
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
        assert_eq!(rows.len(), 4, "un seul fond par entrée sélectionnée");
        assert_eq!(marks.len(), 4, "un filet par entrée sélectionnée");
        for (row, mark) in rows.iter().zip(&marks) {
            assert_eq!(mark.left(), row.left(), "le filet est à gauche de la ligne");
            assert_eq!(mark.height(), row.height(), "le filet couvre la ligne");
            assert_eq!(mark.width(), LIST_ROW_MARK);
            assert!(
                row.width() > 300.0,
                "la ligne court sur toute la largeur restante : {row:?}"
            );
        }
        let button_fill = context.global_style().visuals.selection.bg_fill;
        assert!(
            rects(&output, button_fill).is_empty(),
            "plus de fond de bouton sélectionné"
        );
        let peints = super::super::test_support::textes(&output.shapes);
        for nom in ["Book", "Graph1"] {
            assert!(
                peints
                    .iter()
                    .filter(|t| t.texte.contains(nom))
                    .all(|t| t.gras),
                "{nom} : l'entrée sélectionnée est en gras"
            );
        }
        banc.selection.borrow_mut().clear();
        let output = banc.paint(&context, &tree, Vec::new());
        assert!(rects(&output, theme::list_row_selected_fill()).is_empty());
        assert!(rects(&output, theme::accent()).is_empty());
        assert!(
            !super::super::test_support::textes(&output.shapes)
                .iter()
                .any(|t| t.gras),
            "témoin : sans sélection, rien n'est en gras"
        );
    }

    #[test]
    fn le_suivi_du_contenu_actif_preserve_la_selection_multiple() {
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
        assert_eq!(app.pm_selection, fenetres([book]));
        assert_anchor(Some(PmEntry::Window(book)));
        app.follow_project_manager_active_tab(&context, Some(graph));
        assert_eq!(app.pm_selection, fenetres([graph]));
        assert_anchor(Some(PmEntry::Window(graph)));
        app.pm_selection.insert(PmEntry::Window(book));
        app.follow_project_manager_active_tab(&context, Some(book));
        assert_eq!(app.pm_selection, fenetres([book, graph]));
        assert_anchor(Some(PmEntry::Window(graph)));
        app.follow_project_manager_active_tab(&context, None);
        assert_eq!(app.pm_selection, fenetres([book, graph]));
        // Le clic simple existant remplace la sélection. Même fenêtre active :
        // ne pas écraser ce clic avant l'application de sa demande d'ouverture.
        app.pm_selection = fenetres([book]);
        app.follow_project_manager_active_tab(&context, None);
        assert_eq!(app.pm_selection, fenetres([book]));
        app.follow_project_manager_active_tab(&context, Some(graph));
        assert_eq!(app.pm_selection, fenetres([graph]));
        app.follow_project_manager_active_tab(&context, None);
        assert!(app.pm_selection.is_empty());
        assert_anchor(None);
        context.data(|data| {
            let memory = data.get_temp::<PmMemory>(detached_key).unwrap();
            assert!(memory.focus, "le focus de l'autre viewport est conservé");
            assert_eq!(
                memory.rename,
                vec![tab_node(book)],
                "l'éditeur est conservé"
            );
        });
    }

    #[test]
    fn maj_apres_un_clic_workspace_part_de_la_fenetre_selectionnee() {
        verifier_maj_apres_clic_workspace(false);
    }

    #[test]
    fn maj_apres_un_clic_workspace_sur_la_meme_entree_ne_selectionne_pas_l_ancienne_ancre() {
        verifier_maj_apres_clic_workspace(true);
    }

    fn verifier_maj_apres_clic_workspace(same_target: bool) {
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
            let context = super::super::test_support::contexte();
            egui_extras::install_image_loaders(&context);
            let paint = |ctx: &egui::Context, events| {
                super::super::test_support::image_de(ctx, egui::vec2(1400.0, 900.0), events, |ui| {
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
            // L'ancre vient d'un vrai clic du panneau, puis la même fenêtre
            // déjà active reprend la sélection par le workspace.
            click_at(&context, book_pos, egui::Modifiers::NONE, paint);
            assert_eq!(app.borrow().pm_selection, fenetres([book]));
            click_at(
                &context,
                egui::pos2(410.0, 150.0),
                egui::Modifiers::NONE,
                paint,
            );
            assert_eq!(app.borrow().pm_selection, fenetres([graph]));
            let detached_key = egui::Id::new(("project_manager_gestures", viewport_id()));
            context.data(|data| {
                assert_eq!(
                    data.get_temp::<PmMemory>(detached_key).unwrap().anchor,
                    Some(PmEntry::Window(graph)),
                    "le clic workspace déplace aussi l'ancre du panneau détaché"
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
                    fenetres([graph])
                } else {
                    fenetres([book, graph])
                },
                "maximisée={maximized}, même cible={same_target}"
            );
            session.shutdown().unwrap();
        }
    }

    #[test]
    fn maj_selectionne_la_plage_et_les_raccourcis_agissent_sur_la_selection() {
        let (_session, book, graph, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let book_pos = find_text_center(&first.shapes, "Book");
        let graph_pos = find_text_center(&first.shapes, "Graph");
        click_at(&context, book_pos, egui::Modifiers::NONE, paint);
        click_at(&context, graph_pos, egui::Modifiers::SHIFT, paint);
        assert_eq!(*banc.selection.borrow(), fenetres([book, graph]));
        paint(&context, vec![egui::Event::Copy]);
        assert_eq!(
            banc.actions.take(),
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
            banc.actions.take(),
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
            banc.actions.take(),
            vec![Action::DeleteObjects(vec![tab_node(book), tab_node(graph)])]
        );
    }

    #[test]
    fn f2_ouvre_le_champ_et_entree_valide_une_seule_action() {
        let (_session, book, _graph, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(fenetres([book]));
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
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
        // Le nom est sélectionné à l'ouverture : la frappe le remplace.
        paint(&context, vec![egui::Event::Text("Neuf".to_string())]);
        paint(&context, vec![key(egui::Key::Enter)]);
        assert_eq!(banc.actions.take().len(), 1);
        assert!(
            !has_focus(&egui::Context::default()),
            "un autre contexte ne possède pas ce focus"
        );
    }

    /// Couleur effective du premier texte peint contenant `needle`.
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
            .unwrap_or_else(|| panic!("aucun texte contenant {needle:?} n'a été peint"))
    }

    /// Le panneau fixe ([`StokesForgeApp::project_manager_panel`]) occupe le
    /// bord gauche et relie l'arbre aux champs de l'application : un
    /// Ctrl-clic sélectionne dans `pm_selection`, un clic simple demande
    /// l'ouverture, appliquée plus tard par `ui()`.
    #[test]
    fn le_panneau_fixe_relie_l_arbre_aux_champs_de_l_application() {
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
                .expect("le panneau est posé")
                .outer_rect;
        assert_eq!(panel.left(), 0.0, "ancré à gauche");
        assert!(panel.width() < 800.0, "il laisse la place au reste");

        let book = find_text_center(&output.shapes, "Book");
        click_at(&context, book, egui::Modifiers::COMMAND, paint);
        assert!(app
            .borrow()
            .pm_selection
            .contains(&PmEntry::Window(book_tab)));
        assert!(
            open.borrow().is_empty(),
            "Ctrl-clic sélectionne sans ouvrir"
        );
        click_at(&context, book, egui::Modifiers::NONE, paint);
        assert!(open.borrow().contains(&book_tab));
        session.shutdown().unwrap();
    }

    #[test]
    fn le_double_clic_active_lentree_sans_modificateur() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let double_click = |target: &str, modifiers| {
            // Contexte neuf : dans le même, egui compterait ces clics à la
            // suite des précédents (triple clic, puis davantage) et le geste
            // ne serait plus un double-clic.
            let context = egui::Context::default();
            let banc = Banc::new(HashSet::new());
            let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
            let first = paint(&context, Vec::new());
            let pos = find_text_center(&first.shapes, target);
            click_at(&context, pos, modifiers, paint);
            click_at(&context, pos, modifiers, paint);
            let open = banc.open_requests.borrow().clone();
            open
        };
        // S-K3 : le double-clic active la fenêtre (le Project Manager n'a plus
        // aucun chemin de fermeture, voir `PmInteraction`).
        let open = double_click("Graph1", egui::Modifiers::NONE);
        assert!(open.contains(&graph_tab), "open_requests={open:?}");
        let open = double_click("Book", egui::Modifiers::NONE);
        assert!(open.contains(&book_tab), "open_requests={open:?}");
        // Deux Ctrl-clics rapides basculent la sélection deux fois : ce n'est
        // pas une ouverture.
        let open = double_click("Book", egui::Modifiers::COMMAND);
        assert!(open.is_empty(), "open_requests={open:?}");
    }

    /// Ctrl/Cmd-clic bascule l'appartenance à la sélection : il ajoute, puis
    /// retire au second clic, sans jamais ouvrir l'onglet.
    #[test]
    fn ctrl_clic_selectionne_sans_ouvrir() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, paint);

        assert_eq!(
            *banc.selection.borrow(),
            fenetres([graph_tab]),
            "Ctrl/Cmd-clic doit ajouter l'entrée à la sélection"
        );
        assert!(
            banc.open_requests.borrow().is_empty(),
            "Ctrl/Cmd-clic ne doit jamais ouvrir l'onglet, open_requests={:?}",
            banc.open_requests.borrow()
        );

        // La sélection a fait apparaître la barre groupée au-dessus de
        // l'arbre : l'entrée a descendu, on la relit avant de recliquer.
        let with_toolbar = paint(&context, Vec::new());
        let graph_pos = find_text_center(&with_toolbar.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, paint);
        assert!(
            banc.selection.borrow().is_empty(),
            "un second Ctrl/Cmd-clic retire l'entrée de la sélection, selection={:?}",
            banc.selection.borrow()
        );
        assert!(banc.open_requests.borrow().is_empty());
    }

    #[test]
    fn close_et_open_agissent_sur_toute_la_selection() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        // Sélection posée directement plutôt que reconstruite par deux
        // Ctrl-clics successifs (voir `ctrl_clic_selectionne_sans_ouvrir`).
        let banc = Banc::new(fenetres([graph_tab, book_tab]));
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph");
        let menu = ouvrir_menu_contextuel(&context, graph_pos, paint);
        assert_eq!(*banc.selection.borrow(), fenetres([graph_tab, book_tab]));
        let close_pos = find_text_center(&menu.shapes, &tr!("project-manager-close"));
        click_at(&context, close_pos, egui::Modifiers::NONE, paint);
        let closed: HashSet<Tab> = banc.close_requests.borrow().iter().copied().collect();
        assert_eq!(
            closed,
            HashSet::from([graph_tab, book_tab]),
            "« Close » doit s'appliquer à toute la sélection"
        );
        assert!(
            banc.open_requests.borrow().is_empty(),
            "« Close » n'ouvre rien"
        );
        assert!(
            !painted(&paint(&context, Vec::new()), &tr!("project-manager-close")),
            "Close ferme le menu contextuel"
        );

        let menu = ouvrir_menu_contextuel(&context, graph_pos, paint);
        let open_pos = find_text_center(&menu.shapes, &tr!("project-manager-open"));
        click_at(&context, open_pos, egui::Modifiers::NONE, paint);
        assert!(
            banc.open_requests.borrow().contains(&graph_tab)
                && banc.open_requests.borrow().contains(&book_tab),
            "« Open » garantit que chaque onglet est ouvert, open_requests={:?}",
            banc.open_requests.borrow()
        );
        assert!(
            !painted(&paint(&context, Vec::new()), &tr!("project-manager-open")),
            "Open ferme le menu contextuel"
        );
    }

    /// #327 : un arbre dont le dossier (replié par défaut) cache deux fenêtres
    /// entre la première et la dernière ligne visibles.
    fn arbre_avec_dossier_replie() -> (Vec<ProjectTreeNode>, [Tab; 4]) {
        let fenetre = |raw, name: &str, kind| ProjectTreeNode::Window {
            id: NodeId::from_raw(raw),
            name: name.into(),
            kind,
        };
        let tree = vec![
            fenetre(2, "Alpha", WindowKind::Book),
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(3),
                name: "Dossier".into(),
                children: vec![
                    fenetre(4, "Beta", WindowKind::Graph),
                    fenetre(5, "Gamma", WindowKind::Note),
                ],
            },
            fenetre(6, "Delta", WindowKind::Matrix),
        ];
        let tabs = [
            Tab::Book(NodeId::from_raw(2)),
            Tab::Graph(NodeId::from_raw(4)),
            Tab::Note(NodeId::from_raw(5)),
            Tab::Matrix(NodeId::from_raw(6)),
        ];
        (tree, tabs)
    }

    /// #327 : la plage Maj traverse un dossier replié, et Copier, Supprimer,
    /// Open et Close agissent sur toute la plage, fenêtres cachées comprises.
    #[test]
    fn la_plage_maj_traverse_un_dossier_replie_et_les_actions_la_suivent() {
        let (tree, [alpha, beta, gamma, delta]) = arbre_avec_dossier_replie();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(!painted(&first, "Beta"), "témoin : le dossier est replié");
        let alpha_pos = find_text_center(&first.shapes, "Alpha");
        let delta_pos = find_text_center(&first.shapes, "Delta");
        // Du bas vers le haut, puis du haut vers le bas.
        click_at(&context, delta_pos, egui::Modifiers::NONE, paint);
        click_at(&context, alpha_pos, egui::Modifiers::SHIFT, paint);
        // Le dossier replié est lui aussi dans la plage (#170, point 4).
        let dossier = PmEntry::Folder(NodeId::from_raw(3));
        let mut tout = fenetres([alpha, beta, gamma, delta]);
        tout.insert(dossier);
        assert_eq!(*banc.selection.borrow(), tout);
        click_at(&context, alpha_pos, egui::Modifiers::NONE, paint);
        click_at(&context, delta_pos, egui::Modifiers::SHIFT, paint);
        assert_eq!(*banc.selection.borrow(), tout);

        // Dans l'ordre de lecture : le dossier précède ses enfants.
        let ids: Vec<NodeId> = vec![
            tab_node(alpha),
            dossier.node(),
            tab_node(beta),
            tab_node(gamma),
            tab_node(delta),
        ];
        banc.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Copy]);
        assert_eq!(banc.actions.take(), vec![Action::CopyObjects(ids.clone())]);
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
        assert_eq!(banc.actions.take(), vec![Action::DeleteObjects(ids)]);

        banc.open_requests.borrow_mut().clear();
        let menu = ouvrir_menu_contextuel(&context, alpha_pos, paint);
        let close_pos = find_text_center(&menu.shapes, &tr!("project-manager-close"));
        click_at(&context, close_pos, egui::Modifiers::NONE, paint);
        let closed: HashSet<Tab> = banc.close_requests.borrow().iter().copied().collect();
        assert_eq!(
            closed,
            onglets(&tout),
            "Close ferme aussi les fenêtres du dossier replié"
        );
        let menu = ouvrir_menu_contextuel(&context, alpha_pos, paint);
        let open_pos = find_text_center(&menu.shapes, &tr!("project-manager-open"));
        click_at(&context, open_pos, egui::Modifiers::NONE, paint);
        let opened: HashSet<Tab> = banc.open_requests.borrow().iter().copied().collect();
        assert_eq!(
            opened,
            onglets(&tout),
            "Open ouvre aussi celles du dossier replié"
        );
    }

    /// #327 : une borne de la plage peut elle-même être dans un dossier replié
    /// (l'ancre vient de la fenêtre active, dont la ligne n'est pas peinte).
    #[test]
    fn une_borne_de_plage_maj_dans_un_dossier_replie_compte() {
        let (tree, [_alpha, beta, gamma, delta]) = arbre_avec_dossier_replie();
        let context = egui::Context::default();
        let banc = Banc::new(fenetres([beta]));
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        follow_selection_anchor(&context, Some(beta));
        click_at(
            &context,
            find_text_center(&first.shapes, "Delta"),
            egui::Modifiers::SHIFT,
            paint,
        );
        assert_eq!(*banc.selection.borrow(), fenetres([beta, gamma, delta]));
    }

    fn ouvrir_menu_contextuel(
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

    /// Les actions de visibilité s'ouvrent au clic droit. Le clic simple
    /// ouvre toujours son entrée, sans déplacer l'arbre lors de la sélection.
    #[test]
    fn les_actions_de_visibilite_sont_dans_le_menu_contextuel_et_le_clic_simple_ouvre() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        assert!(
            !painted(&first, &tr!("project-manager-close"))
                && !painted(&first, &tr!("project-manager-open")),
            "sans sélection, aucun bouton groupé à regarder"
        );
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*banc.open_requests.borrow(), vec![graph_tab]);
        assert_eq!(
            *banc.selection.borrow(),
            fenetres([graph_tab]),
            "un clic simple sélectionne sa cible (#105)"
        );

        // La sélection ne déplace pas les entrées sous la souris.
        let with_selection = paint(&context, Vec::new());
        assert!(!painted(&with_selection, &tr!("project-manager-close")));
        assert!(!painted(&with_selection, &tr!("project-manager-open")));
        assert_eq!(
            find_text_center(&with_selection.shapes, "Graph1"),
            graph_pos
        );
        let menu = ouvrir_menu_contextuel(&context, graph_pos, paint);
        assert!(painted(&menu, &tr!("project-manager-close")));
        assert!(painted(&menu, &tr!("project-manager-open")));
    }

    /// #157 : le clic droit d'une fenêtre est **à plat** — Open · Close ─ Copy ·
    /// Paste ─ Rename · Duplicate ─ Delete, sans « Selection ▸ ».
    #[test]
    fn le_clic_droit_d_une_fenetre_est_a_plat_dans_l_ordre_des_sections() {
        let (_session, _book_tab, _graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        let menu = ouvrir_menu_contextuel(&context, graph_pos, paint);
        let ordre = [
            tr!("project-manager-open"),
            tr!("project-manager-close"),
            tr!("action-copy"),
            tr!("action-paste"),
            tr!("action-rename-project-object"),
            tr!("action-duplicate-project-object"),
            tr!("action-delete-project-object"),
        ];
        let hauteurs: Vec<f32> = ordre
            .iter()
            .map(|label| find_text_center(&menu.shapes, label).y)
            .collect();
        assert!(
            hauteurs.windows(2).all(|pair| pair[0] < pair[1]),
            "l'ordre des sections : {hauteurs:?}"
        );
        assert!(
            !painted(&menu, &tr!("menu-selected-content")),
            "pas d'enveloppe au clic droit"
        );
    }

    /// #157, #326 : un dossier propose Add Folder, Paste, Rename et Delete
    /// seulement, et son renommage s'édite sur place comme celui d'une fenêtre.
    #[test]
    fn le_clic_droit_d_un_dossier_propose_ajouter_coller_renommer_supprimer() {
        let dossier = NodeId::from_raw(7);
        let tree = vec![ProjectTreeNode::Folder {
            id: dossier,
            name: "Mesures".into(),
            children: Vec::new(),
        }];
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let pos = find_text_center(&first.shapes, "Mesures");
        let menu = ouvrir_menu_contextuel(&context, pos, paint);
        for present in [
            tr!("action-add-folder"),
            tr!("action-paste"),
            tr!("action-rename-project-object"),
            tr!("action-delete-project-object"),
        ] {
            assert!(painted(&menu, &present), "{present} sur un dossier");
        }
        assert!(
            find_text_center(&menu.shapes, &tr!("action-add-folder")).y
                < find_text_center(&menu.shapes, &tr!("action-paste")).y,
            "la création vient en tête"
        );
        for absent in [tr!("project-manager-open"), tr!("project-manager-close")] {
            assert!(!painted(&menu, &absent), "{absent} n'a pas de sens ici");
        }
        // Copier et Dupliquer un dossier sont refusés pour l'instant : les
        // entrées restent visibles, grisées (leur clic ne pousse rien).
        for grisee in [tr!("action-copy"), tr!("action-duplicate-project-object")] {
            assert!(painted(&menu, &grisee), "{grisee} grisée sur un dossier");
            let at = find_text_center(&menu.shapes, &grisee);
            click_at(&context, at, egui::Modifiers::NONE, paint);
            assert!(
                banc.actions.borrow().is_empty(),
                "{grisee} grisée : aucune action, {:?}",
                banc.actions.borrow()
            );
            ouvrir_menu_contextuel(&context, pos, paint);
        }
        let rename = find_text_center(&menu.shapes, &tr!("action-rename-project-object"));
        click_at(&context, rename, egui::Modifiers::NONE, paint);
        paint(&context, Vec::new());
        paint(&context, Vec::new());
        paint(&context, vec![egui::Event::Text("Séries".to_string())]);
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
            *banc.actions.borrow(),
            vec![Action::RenameObjects(vec![(dossier, "Séries".into())])],
            "le dossier se renomme sur place"
        );
    }

    /// Une fenêtre fermée reste listée, atténuée : sa couleur diffère de celle
    /// de la même fenêtre ouverte, et ses voisines ne changent pas ; un clic
    /// sur la ligne atténuée l'ouvre (#327).
    #[test]
    fn une_fenetre_fermee_reste_listee_mais_attenuee_et_un_clic_l_ouvre() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        banc.open.borrow_mut().extend([book_tab, graph_tab]);
        banc.paint(&context, &tree, Vec::new());
        let ouverte = banc.paint(&context, &tree, Vec::new());
        banc.open.borrow_mut().remove(&graph_tab);
        let fermee = banc.paint(&context, &tree, Vec::new());
        assert_ne!(
            text_color(&fermee.shapes, "Graph1"),
            text_color(&ouverte.shapes, "Graph1"),
            "la fenêtre fermée doit se distinguer de l'ouverte"
        );
        assert_eq!(
            text_color(&fermee.shapes, "Book"),
            text_color(&ouverte.shapes, "Book"),
            "les autres entrées ne changent pas"
        );
        let pos = find_text_center(&fermee.shapes, "Graph1");
        click_at(&context, pos, egui::Modifiers::NONE, |ctx, events| {
            banc.paint(ctx, &tree, events)
        });
        assert_eq!(*banc.open_requests.borrow(), vec![graph_tab]);
    }

    /// #326 : « Add Folder » au clic droit d'un dossier crée *dans* ce dossier,
    /// et le clic droit d'une fenêtre ne propose pas la création.
    #[test]
    fn le_clic_droit_d_un_dossier_ajoute_un_dossier_dedans() {
        let dossier = NodeId::from_raw(7);
        let tree = vec![ProjectTreeNode::Folder {
            id: dossier,
            name: "Mesures".into(),
            children: Vec::new(),
        }];
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let pos = find_text_center(&first.shapes, "Mesures");
        let menu = ouvrir_menu_contextuel(&context, pos, paint);
        let add = find_text_center(&menu.shapes, &tr!("action-add-folder"));
        click_at(&context, add, egui::Modifiers::NONE, paint);
        assert_eq!(
            *banc.actions.borrow(),
            vec![Action::AddFolder { parent: dossier }],
            "dans le dossier cliqué, pas à la racine"
        );

        let fenetre = vec![ProjectTreeNode::Window {
            id: NodeId::from_raw(8),
            name: "Graph1".into(),
            kind: WindowKind::Graph,
        }];
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &fenetre, events);
        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let pos = find_text_center(&first.shapes, "Graph1");
        let menu = ouvrir_menu_contextuel(&context, pos, paint);
        assert!(
            painted(&menu, &tr!("action-delete-project-object")),
            "témoin"
        );
        assert!(!painted(&menu, &tr!("action-add-folder")));
    }

    /// #326 : après la création, le parent se déplie et le nom du dossier neuf
    /// s'édite sur place ; sans la demande (témoin), le parent reste replié.
    #[test]
    fn un_dossier_neuf_deplie_son_parent_et_s_edite_sur_place() {
        let parent = NodeId::from_raw(1);
        let neuf = NodeId::from_raw(5);
        let tree = vec![ProjectTreeNode::Folder {
            id: parent,
            name: "Mesures".into(),
            children: vec![ProjectTreeNode::Folder {
                id: neuf,
                name: "Folder1".into(),
                children: Vec::new(),
            }],
        }];
        let temoin = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        banc.paint(&temoin, &tree, Vec::new());
        let sortie = banc.paint(&temoin, &tree, Vec::new());
        assert!(
            !painted(&sortie, "Folder1"),
            "témoin : le parent est replié"
        );

        let context = egui::Context::default();
        banc.paint(&context, &tree, Vec::new());
        begin_rename_of_new_folder(&context, egui::ViewportId::ROOT, neuf, parent);
        banc.paint(&context, &tree, Vec::new());
        let sortie = banc.paint(&context, &tree, Vec::new());
        assert!(painted(&sortie, "Folder1"), "le parent est déplié");
        let memoire = context
            .data(|data| data.get_temp::<PmMemory>(memory_id(&context)))
            .expect("mémoire du panneau");
        assert_eq!(memoire.rename, vec![neuf], "le nom est en édition");
        assert!(memoire.reveal.is_empty(), "la demande est consommée");
    }

    /// Un dossier est replié par défaut : ses enfants ne sont pas peints.
    /// Déplié (clic sur son triangle), il les peint, et un dossier vide dit
    /// qu'il l'est au lieu de ne rien montrer.
    #[test]
    fn un_dossier_est_replie_par_defaut_et_un_dossier_vide_le_dit() {
        let tree = vec![
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(1),
                name: "Mesures".into(),
                children: vec![ProjectTreeNode::Window {
                    id: NodeId::from_raw(2),
                    name: "Spectre".into(),
                    kind: WindowKind::Graph,
                }],
            },
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(3),
                name: "Vide".into(),
                children: Vec::new(),
            },
        ];
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
        paint(&context, Vec::new());
        let closed = paint(&context, Vec::new());
        assert!(painted(&closed, "Mesures") && painted(&closed, "Vide"));
        assert!(
            !painted(&closed, "Spectre"),
            "un dossier replié ne peint pas ses enfants"
        );
        assert!(!painted(&closed, &tr!("project-folder-empty")));
        assert!(
            banc.open_requests.borrow().is_empty(),
            "peindre n'ouvre rien"
        );

        // Le triangle d'un dossier est le premier élément de sa ligne, à
        // gauche de son icône : on clique à mi-hauteur de la ligne, au bord
        // gauche de l'arbre.
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
        let at = toggle(&closed.shapes, "Mesures");
        click_at(&context, at, egui::Modifiers::NONE, paint);
        let opened = paint(&context, Vec::new());
        assert!(
            painted(&opened, "Spectre"),
            "le dossier déplié peint son enfant"
        );
        let at = toggle(&opened.shapes, "Vide");
        click_at(&context, at, egui::Modifiers::NONE, paint);
        let opened = paint(&context, Vec::new());
        assert!(painted(&opened, &tr!("project-folder-empty")));
        assert!(
            banc.open_requests.borrow().is_empty(),
            "déplier un dossier n'ouvre aucune fenêtre"
        );
    }

    /// Un clic lent (S-K4) sur une fenêtre déjà seule sélectionnée ouvre son
    /// renommage ; un double-clic, jamais ; le champ s'ouvre sur le nom
    /// sélectionné.
    #[test]
    fn un_clic_lent_sur_un_nom_selectionne_le_renomme_pas_un_double_clic() {
        let (_session, book, _graph, tree) = project_manager_fixture();
        for double in [false, true] {
            let context = egui::Context::default();
            let banc = Banc::new(fenetres([book]));
            let horloge = std::cell::Cell::new(1.0);
            let paint = |ctx: &egui::Context, events| {
                horloge.set(horloge.get() + 0.01);
                banc.paint_at(ctx, &tree, events, Some(horloge.get()))
            };
            let first = paint(&context, Vec::new());
            let pos = find_text_center(&first.shapes, "Book1");
            click_at(&context, pos, egui::Modifiers::NONE, paint);
            if double {
                click_at(&context, pos, egui::Modifiers::NONE, paint);
            }
            paint(&context, Vec::new());
            assert!(!context.text_edit_focused(), "pas encore (double={double})");
            horloge.set(horloge.get() + 0.6);
            paint(&context, Vec::new());
            paint(&context, Vec::new());
            assert_eq!(
                context.text_edit_focused(),
                !double,
                "le renommage s'ouvre après un clic lent seulement (double={double})"
            );
        }
    }

    /// Un nom déjà pris par un frère garde le champ ouvert : Entrée n'envoie
    /// rien ; un nom libre est écrit.
    #[test]
    fn un_nom_deja_pris_garde_le_champ_ouvert() {
        let (_session, book, _graph, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(fenetres([book]));
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
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
        assert!(banc.actions.borrow().is_empty(), "nom pris : rien d'écrit");
        assert!(context.text_edit_focused(), "le champ reste ouvert");

        paint(&context, vec![egui::Event::Text("Libre".to_string())]);
        paint(&context, vec![key(egui::Key::Enter)]);
        assert_eq!(banc.actions.borrow().len(), 1);
    }

    // -- Dossiers sélectionnables (#170, point 4) -----------------------------

    fn touche_pm(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Le dossier de `arbre_avec_dossier_replie`.
    fn dossier_de_l_arbre() -> NodeId {
        NodeId::from_raw(3)
    }

    /// Ouvre le dossier au prochain dessin (comme après *Add Folder*).
    fn deplier(context: &egui::Context, dossier: NodeId) {
        let key = memory_id(context);
        context.data_mut(|data| {
            data.get_temp_mut_or_default::<PmMemory>(key)
                .reveal
                .push(dossier);
        });
    }

    /// Un clic sur un dossier le sélectionne et, s'il est fermé, le déplie ;
    /// il n'ouvre aucune fenêtre. Témoin : sans insertion du dossier dans la
    /// sélection, rien n'est sélectionné.
    #[test]
    fn un_clic_sur_un_dossier_le_selectionne_et_le_deplie() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(!painted(&first, "Beta"), "témoin : le dossier est replié");
        let pos = find_text_center(&first.shapes, "Dossier");
        click_at(&context, pos, egui::Modifiers::NONE, paint);
        assert_eq!(
            *banc.selection.borrow(),
            HashSet::from([PmEntry::Folder(dossier)])
        );
        paint(&context, vec![]);
        let apres = paint(&context, vec![]);
        assert!(painted(&apres, "Beta"), "le clic a déplié le dossier");
        assert!(
            banc.open_requests.borrow().is_empty(),
            "un dossier n'ouvre aucune fenêtre"
        );
    }

    /// Un dossier sélectionné se peint comme une fenêtre : fond pâle d'accent
    /// et filet à gauche ; survolé seulement, fond gris. Témoin : non
    /// sélectionné et non survolé, ni l'un ni l'autre.
    #[test]
    fn un_dossier_selectionne_porte_fond_filet_et_survol_comme_une_fenetre() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        let context = super::super::test_support::contexte();
        let banc = Banc::new(HashSet::new());
        let rects = |output: &egui::FullOutput, fill: egui::Color32| -> usize {
            output
                .shapes
                .iter()
                .filter(
                    |shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == fill),
                )
                .count()
        };
        banc.paint(&context, &tree, vec![]);
        let repos = banc.paint(&context, &tree, vec![]);
        assert_eq!(rects(&repos, theme::list_row_selected_fill()), 0);
        assert_eq!(rects(&repos, theme::list_row_hover_fill()), 0);
        let pos = find_text_center(&repos.shapes, "Dossier");
        let survol = banc.paint(&context, &tree, vec![egui::Event::PointerMoved(pos)]);
        assert_eq!(
            rects(&survol, theme::list_row_hover_fill()),
            1,
            "le dossier survolé a le fond de survol"
        );
        banc.selection.borrow_mut().insert(PmEntry::Folder(dossier));
        let choisi = banc.paint(
            &context,
            &tree,
            vec![egui::Event::PointerMoved(egui::pos2(380.0, 390.0))],
        );
        assert_eq!(rects(&choisi, theme::list_row_selected_fill()), 1);
        assert_eq!(rects(&choisi, theme::accent()), 1, "filet d'accent");
        assert!(
            super::super::test_support::textes(&choisi.shapes)
                .iter()
                .any(|t| t.texte == "Dossier" && t.gras),
            "libellé en gras"
        );
    }

    /// Un clic sur un dossier ouvert le sélectionne **sans** le replier, même
    /// quand il est déjà seul sélectionné. Témoin : un clic sur un dossier
    /// fermé l'ouvre (autre test).
    #[test]
    fn un_clic_sur_un_dossier_ouvert_le_selectionne_sans_le_replier() {
        let (tree, [alpha, ..]) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let horloge = std::cell::Cell::new(1.0);
        let paint = |ctx: &egui::Context, events| {
            horloge.set(horloge.get() + 0.01);
            banc.paint_at(ctx, &tree, events, Some(horloge.get()))
        };
        deplier(&context, dossier);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(painted(&first, "Beta"), "précondition : dossier ouvert");
        let alpha_pos = find_text_center(&first.shapes, "Alpha");
        let dossier_pos = find_text_center(&first.shapes, "Dossier");
        click_at(&context, alpha_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*banc.selection.borrow(), fenetres([alpha]));
        for _ in 0..2 {
            // Hors de l'intervalle de double-clic : deux clics simples.
            horloge.set(horloge.get() + 2.0);
            click_at(&context, dossier_pos, egui::Modifiers::NONE, paint);
            // Un repli serait animé : on laisse passer le temps.
            horloge.set(horloge.get() + 0.3);
            paint(&context, vec![]);
            let apres = paint(&context, vec![]);
            assert_eq!(
                *banc.selection.borrow(),
                HashSet::from([PmEntry::Folder(dossier)])
            );
            assert!(painted(&apres, "Beta"), "le clic ne replie pas");
        }
    }

    /// Un double-clic replie le dossier, qu'il fût ouvert ou fermé au départ
    /// (le premier clic d'un double-clic déplie un dossier fermé).
    #[test]
    fn un_double_clic_replie_le_dossier() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        for ouvert_au_depart in [false, true] {
            // Contexte neuf : voir `le_double_clic_active_lentree_sans_modificateur`.
            let context = egui::Context::default();
            let banc = Banc::new(HashSet::new());
            let horloge = std::cell::Cell::new(1.0);
            let paint = |ctx: &egui::Context, events| {
                horloge.set(horloge.get() + 0.01);
                banc.paint_at(ctx, &tree, events, Some(horloge.get()))
            };
            if ouvert_au_depart {
                deplier(&context, dossier);
            }
            paint(&context, vec![]);
            let first = paint(&context, vec![]);
            assert_eq!(painted(&first, "Beta"), ouvert_au_depart);
            let pos = find_text_center(&first.shapes, "Dossier");
            click_at(&context, pos, egui::Modifiers::NONE, paint);
            click_at(&context, pos, egui::Modifiers::NONE, paint);
            // Le repli est animé : on laisse passer le temps.
            horloge.set(horloge.get() + 2.0);
            paint(&context, vec![]);
            let apres = paint(&context, vec![]);
            assert!(
                !painted(&apres, "Beta"),
                "double-clic : replié (ouvert au départ : {ouvert_au_depart})"
            );
            assert_eq!(
                *banc.selection.borrow(),
                HashSet::from([PmEntry::Folder(dossier)])
            );
        }
    }

    /// Le triangle replie un dossier ouvert (geste d'egui, conservé).
    #[test]
    fn le_triangle_replie_un_dossier_ouvert() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let horloge = std::cell::Cell::new(1.0);
        let paint = |ctx: &egui::Context, events| {
            horloge.set(horloge.get() + 0.01);
            banc.paint_at(ctx, &tree, events, Some(horloge.get()))
        };
        deplier(&context, dossier);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        assert!(painted(&first, "Beta"));
        let libelle = find_text_center(&first.shapes, "Dossier");
        let triangle = egui::pos2(libelle.x - 62.0, libelle.y);
        click_at(&context, triangle, egui::Modifiers::NONE, paint);
        // Le repli est animé : on laisse passer le temps.
        horloge.set(horloge.get() + 2.0);
        paint(&context, vec![]);
        let apres = paint(&context, vec![]);
        assert!(!painted(&apres, "Beta"), "le triangle replie");
        assert!(
            banc.selection.borrow().is_empty(),
            "le triangle ne sélectionne pas"
        );
    }

    /// Suppr sur un dossier sélectionné supprime le dossier (une entrée
    /// d'historique, `DeleteObjects`) ; F2 ouvre son renommage sur place.
    #[test]
    fn suppr_et_f2_agissent_sur_un_dossier_selectionne() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let pos = find_text_center(&first.shapes, "Dossier");
        click_at(&context, pos, egui::Modifiers::NONE, paint);
        banc.actions.borrow_mut().clear();
        paint(&context, vec![touche_pm(egui::Key::Delete)]);
        assert_eq!(
            banc.actions.take(),
            vec![Action::DeleteObjects(vec![dossier])]
        );
        paint(&context, vec![touche_pm(egui::Key::F2)]);
        paint(&context, vec![]);
        assert_eq!(
            renaming_in(&context, context.viewport_id()),
            vec![dossier],
            "F2 ouvre le renommage du dossier"
        );
        paint(&context, vec![egui::Event::Text("Série".to_string())]);
        paint(&context, vec![touche_pm(egui::Key::Enter)]);
        assert_eq!(
            banc.actions.take(),
            vec![Action::RenameObjects(vec![(dossier, "Série".into())])]
        );
    }

    /// Copier et dupliquer un dossier sont refusés (voir la doc de tête) :
    /// Ctrl+C et Ctrl+D transmettent le geste à l'état, qui le refuse avec sa
    /// raison (`stokesforge-app-state/tests/project_objects.rs`) ; l'entrée de
    /// menu, elle, est grisée (test du clic droit d'un dossier).
    #[test]
    fn ctrl_c_sur_un_dossier_passe_par_l_etat_qui_le_refuse() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let pos = find_text_center(&first.shapes, "Dossier");
        click_at(&context, pos, egui::Modifiers::NONE, paint);
        banc.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Copy]);
        assert_eq!(
            banc.actions.take(),
            vec![Action::CopyObjects(vec![dossier])]
        );
    }

    /// Ctrl+V avec un dossier seul sélectionné colle **dans** ce dossier ;
    /// avec une fenêtre sélectionnée, à la racine comme avant. Témoin : les
    /// deux parents diffèrent.
    #[test]
    fn ctrl_v_avec_un_dossier_selectionne_colle_dans_le_dossier() {
        let (tree, [alpha, ..]) = arbre_avec_dossier_replie();
        let dossier = dossier_de_l_arbre();
        let racine = NodeId::from_raw(1);
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let dossier_pos = find_text_center(&first.shapes, "Dossier");
        let alpha_pos = find_text_center(&first.shapes, "Alpha");
        click_at(&context, dossier_pos, egui::Modifiers::NONE, paint);
        banc.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Paste("données".into())]);
        assert_eq!(
            banc.actions.take(),
            vec![Action::PasteObjects {
                parent: dossier,
                text: "données".into()
            }]
        );
        click_at(&context, alpha_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*banc.selection.borrow(), fenetres([alpha]));
        banc.actions.borrow_mut().clear();
        paint(&context, vec![egui::Event::Paste("données".into())]);
        assert_eq!(
            banc.actions.take(),
            vec![Action::PasteObjects {
                parent: racine,
                text: "données".into()
            }]
        );
    }

    /// `paste_parent` : le dossier seul, sinon la racine.
    #[test]
    fn le_parent_du_collage_est_le_dossier_seul_ou_la_racine() {
        let racine = NodeId::from_raw(1);
        let dossier = PmEntry::Folder(NodeId::from_raw(3));
        let fenetre = PmEntry::Window(Tab::Book(NodeId::from_raw(2)));
        assert_eq!(
            paste_parent(&HashSet::from([dossier]), racine),
            NodeId::from_raw(3)
        );
        assert_eq!(paste_parent(&HashSet::from([fenetre]), racine), racine);
        assert_eq!(
            paste_parent(&HashSet::from([dossier, fenetre]), racine),
            racine
        );
        assert_eq!(paste_parent(&HashSet::new(), racine), racine);
    }

    // -- Navigation au clavier (#170, point 6) --------------------------------

    fn touche_avec(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Un banc dont le panneau a le focus (un clic sur « Alpha »), l'arbre
    /// `arbre_avec_dossier_replie` : Alpha, Dossier (replié : Beta, Gamma),
    /// Delta. `f` reçoit le banc, le contexte et le peintre.
    fn avec_panneau_focalise(
        f: impl FnOnce(
            &Banc,
            &egui::Context,
            &dyn Fn(&egui::Context, Vec<egui::Event>) -> egui::FullOutput,
            [Tab; 4],
        ),
    ) {
        let (tree, tabs) = arbre_avec_dossier_replie();
        let context = super::super::test_support::contexte();
        let banc = Banc::new(HashSet::new());
        let horloge = std::cell::Cell::new(1.0);
        let paint = |ctx: &egui::Context, events| {
            horloge.set(horloge.get() + 0.1);
            banc.paint_at(ctx, &tree, events, Some(horloge.get()))
        };
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let alpha = find_text_center(&first.shapes, "Alpha");
        click_at(&context, alpha, egui::Modifiers::NONE, paint);
        paint(&context, vec![]);
        banc.open_requests.borrow_mut().clear();
        f(&banc, &context, &paint, tabs);
    }

    /// ↓ deux fois depuis Alpha atteint la **troisième ligne visible** (Delta),
    /// le dossier replié comptant pour une ligne et ses enfants pour aucune ;
    /// la flèche ne demande ni ouverture ni action. Témoin : suivre `order`
    /// (enfants cachés compris) s'arrêterait sur Beta.
    #[test]
    fn deux_fois_bas_atteignent_la_troisieme_ligne_visible() {
        avec_panneau_focalise(|banc, context, paint, [alpha, _beta, _gamma, delta]| {
            assert_eq!(*banc.selection.borrow(), fenetres([alpha]));
            paint(
                context,
                vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *banc.selection.borrow(),
                HashSet::from([PmEntry::Folder(dossier_de_l_arbre())]),
                "la première flèche va au dossier"
            );
            paint(
                context,
                vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *banc.selection.borrow(),
                fenetres([delta]),
                "la seconde saute les enfants du dossier replié"
            );
            assert!(
                banc.open_requests.borrow().is_empty(),
                "une flèche n'ouvre rien"
            );
            assert!(banc.actions.borrow().is_empty(), "ni action, ni historique");
            // En bas de liste : ↓ reste sur place ; ↑ remonte d'une ligne visible.
            paint(
                context,
                vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_eq!(*banc.selection.borrow(), fenetres([delta]));
            paint(
                context,
                vec![touche_avec(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *banc.selection.borrow(),
                HashSet::from([PmEntry::Folder(dossier_de_l_arbre())])
            );
        });
    }

    /// Les flèches ne s'appliquent qu'à un panneau qui a le focus : sans clic
    /// dedans, ↓ ne bouge rien et reste lisible par le reste de l'application.
    #[test]
    fn sans_focus_les_fleches_ne_touchent_pas_au_panneau() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let context = egui::Context::default();
        let banc = Banc::new(HashSet::new());
        banc.paint(&context, &tree, vec![]);
        banc.paint(
            &context,
            &tree,
            vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
        );
        assert!(banc.selection.borrow().is_empty());
        assert!(banc.arrow_down_left.get(), "la touche reste à qui la veut");
    }

    /// Depuis le panneau focalisé, la flèche est consommée : la grille d'un
    /// classeur, peinte après, lit les flèches sans les consommer et déplacerait
    /// sa cellule en même temps. Témoin : sans focus, la flèche reste lisible.
    #[test]
    fn une_fleche_du_panneau_n_atteint_pas_la_grille() {
        avec_panneau_focalise(|banc, context, paint, _| {
            paint(
                context,
                vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert!(
                !banc.arrow_down_left.get(),
                "↓ consommée par le panneau : la grille ne la voit pas"
            );
        });
    }

    /// Maj+↓ et Alt+↓ ne se prennent pas pour ↓ (egui ignore Maj et Alt en
    /// trop) : ils sont consommés et ne bougent pas la sélection. Témoin : ↓
    /// seule la bouge.
    #[test]
    fn maj_et_alt_bas_ne_deplacent_pas_la_selection() {
        avec_panneau_focalise(|banc, context, paint, [alpha, ..]| {
            for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::ALT] {
                paint(context, vec![touche_avec(egui::Key::ArrowDown, modifiers)]);
                assert_eq!(*banc.selection.borrow(), fenetres([alpha]), "{modifiers:?}");
                assert!(!banc.arrow_down_left.get(), "{modifiers:?} consommée");
            }
            paint(
                context,
                vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            assert_ne!(*banc.selection.borrow(), fenetres([alpha]), "témoin");
        });
    }

    /// Entrée ouvre la fenêtre sélectionnée (`open_requests`), déplie un
    /// dossier fermé et le replie ouvert. Témoin : sans Entrée, rien.
    #[test]
    fn entree_ouvre_la_fenetre_et_bascule_le_dossier() {
        avec_panneau_focalise(|banc, context, paint, [alpha, ..]| {
            paint(context, vec![]);
            assert!(banc.open_requests.borrow().is_empty(), "témoin");
            paint(
                context,
                vec![touche_avec(egui::Key::Enter, egui::Modifiers::NONE)],
            );
            assert_eq!(
                *banc.open_requests.borrow(),
                vec![alpha],
                "Entrée ouvre Alpha"
            );

            paint(
                context,
                vec![touche_avec(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            );
            let ferme = paint(context, vec![]);
            assert!(!painted(&ferme, "Beta"), "précondition : dossier replié");
            paint(
                context,
                vec![touche_avec(egui::Key::Enter, egui::Modifiers::NONE)],
            );
            let ouvert = paint(context, vec![]);
            assert!(painted(&ouvert, "Beta"), "Entrée déplie le dossier");
            assert_eq!(
                banc.open_requests.borrow().len(),
                1,
                "un dossier n'ouvre aucune fenêtre"
            );
            paint(
                context,
                vec![touche_avec(egui::Key::Enter, egui::Modifiers::NONE)],
            );
            // Le repli est animé : on laisse passer le temps.
            let mut apres = paint(context, vec![]);
            for _ in 0..10 {
                apres = paint(context, vec![]);
            }
            assert!(!painted(&apres, "Beta"), "Entrée replie le dossier ouvert");
        });
    }

    /// La ligne qui porte le curseur clavier a le contour de focus de
    /// `selection.stroke`, comme une ligne de liste qui a le focus d'egui ;
    /// sans focus du panneau, aucun contour. Témoin : le contour retiré de la
    /// peinture des lignes fait échouer ce test.
    #[test]
    fn la_ligne_du_curseur_clavier_a_le_contour_de_focus() {
        let (tree, _tabs) = arbre_avec_dossier_replie();
        let context = super::super::test_support::contexte();
        let banc = Banc::new(HashSet::new());
        let contour = |output: &egui::FullOutput| -> usize {
            let stroke = context.global_style().visuals.selection.stroke;
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke == stroke)
                })
                .count()
        };
        banc.paint(&context, &tree, vec![]);
        let first = banc.paint(&context, &tree, vec![]);
        banc.selection
            .borrow_mut()
            .insert(PmEntry::Folder(dossier_de_l_arbre()));
        let sans_focus = banc.paint(&context, &tree, vec![]);
        assert_eq!(
            contour(&sans_focus),
            0,
            "panneau sans focus : pas de contour"
        );
        let pos = find_text_center(&first.shapes, "Alpha");
        click_at(&context, pos, egui::Modifiers::NONE, |ctx, events| {
            banc.paint(ctx, &tree, events)
        });
        let avec_focus = banc.paint(&context, &tree, vec![]);
        assert_eq!(contour(&avec_focus), 1, "une seule ligne, celle du curseur");
    }

    /// `neighbour` : voisine visible, bornée ; sans curseur ↓ va au début et ↑
    /// à la fin ; liste vide, rien.
    #[test]
    fn la_ligne_voisine_est_bornee_et_sans_curseur_on_part_d_un_bout() {
        use super::super::list_row::neighbour;
        let lignes = [10, 20, 30];
        assert_eq!(neighbour(&lignes, Some(20), 1), Some(30));
        assert_eq!(neighbour(&lignes, Some(20), -1), Some(10));
        assert_eq!(neighbour(&lignes, Some(30), 1), Some(30), "borné en bas");
        assert_eq!(neighbour(&lignes, Some(10), -1), Some(10), "borné en haut");
        assert_eq!(neighbour(&lignes, None, 1), Some(10));
        assert_eq!(neighbour(&lignes, None, -1), Some(30));
        assert_eq!(
            neighbour(&lignes, Some(99), 1),
            Some(10),
            "curseur invisible"
        );
        assert_eq!(neighbour::<i32>(&[], Some(1), 1), None);
    }
}

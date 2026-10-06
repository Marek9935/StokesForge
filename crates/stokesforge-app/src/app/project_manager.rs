//! Arbre du Project Manager : une seule écriture pour le panneau fixe et la
//! fenêtre OS détachée, sur un arbre déjà construit
//! ([`crate::state::ProjectTreeNode`]). Ce que les clics demandent (ouvrir,
//! fermer, sélectionner, afficher, masquer) s'accumule dans
//! [`PmInteraction`], appliqué par `StokesForgeApp` après la peinture.
//!
//! Le titre appartient au cadre partagé `panel_header`, pas à l'arbre :
//! attaché ou détaché, aucun double en-tête et les mêmes gestes (#100).
//! Une sélection reprise par la fenêtre active doit aussi déplacer l'ancre
//! de Maj-clic dans les deux viewports : le seul fond sélectionné ne suffit
//! pas à définir la prochaine plage après un clic dans le workspace (#154).
//! Afficher et Masquer vivent dans le menu contextuel d'une entrée et portent
//! sur la sélection entière ; l'arbre commence directement sous l'en-tête.

use std::collections::HashSet;

use eframe::egui;

use crate::icons::{icon_image, Icon, IconRole};
use crate::state::{Action, ProjectTreeNode, WindowKind};
use crate::tr;
use stokesforge_core::NodeId;

use super::shortcuts::{shortcut_text_delete, SHORTCUT_DUPLICATE_SHEET, SHORTCUT_RENAME_OBJECT};
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
    /// Onglet fermé par un double-clic sur une entrée déjà au premier plan,
    /// ou par « Masquer » sur une entrée maximisée (garde de cohérence, même
    /// esprit que celle de `detach_tab`).
    pub(super) close_requests: &'a mut Vec<Tab>,
    /// L'onglet actuellement au premier plan — voir la doc de tête du
    /// module, § z-order. `None` si aucune fenêtre flottante n'est ouverte.
    pub(super) active_tab: Option<Tab>,
    /// Sélection courante dans l'arbre — Ctrl/Cmd-clic en bascule
    /// l'appartenance sans ouvrir l'onglet.
    pub(super) selection: &'a mut HashSet<Tab>,
    /// Onglets actuellement masqués (voir [`StokesForgeApp::hidden_tabs`]) —
    /// affichés atténués dans l'arbre, et ce que « Afficher »/« Masquer »
    /// modifient pour la sélection courante.
    pub(super) hidden: &'a mut HashSet<Tab>,
}

/// La sélection est partagée avec le viewport ; l'éditeur et le focus restent
/// dans la mémoire du viewport qui reçoit le clavier.
#[derive(Clone, Default)]
struct PmMemory {
    focus: bool,
    anchor: Option<Tab>,
    rename: Vec<NodeId>,
    renamed: Vec<(NodeId, String)>,
    text: Option<String>,
    order: Vec<Tab>,
    next_order: Vec<Tab>,
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
    ctx.data_mut(|data| {
        for viewport in [egui::ViewportId::ROOT, viewport_id()] {
            let key = egui::Id::new(("project_manager_gestures", viewport));
            let mut memory = data.get_temp::<PmMemory>(key).unwrap_or_default();
            memory.anchor = tab;
            data.insert_temp(key, memory);
        }
    });
}

pub(super) fn has_focus(ctx: &egui::Context) -> bool {
    let memory_key = memory_id(ctx);
    ctx.data(|data| {
        data.get_temp::<PmMemory>(memory_key)
            .is_some_and(|m| m.focus || !m.rename.is_empty())
    })
}

fn selected_ids(selection: &HashSet<Tab>, order: &[Tab]) -> Vec<NodeId> {
    order
        .iter()
        .filter(|tab| selection.contains(tab))
        .map(|tab| tab_node(*tab))
        .collect()
}

/// Même menu pour la barre Edit et le clic droit. Le collage déclenche une
/// demande de lecture à egui-winit, donc utilise aussi le récepteur Wayland.
pub(super) fn object_menu(
    ui: &mut egui::Ui,
    ids: &[NodeId],
    parent: NodeId,
    actions: &mut Vec<Action>,
    rename_viewport: egui::ViewportId,
) {
    let memory_key = egui::Id::new(("project_manager_gestures", rename_viewport));
    let paste_key = egui::Id::new(("project_paste_parent", ui.ctx().viewport_id()));
    ui.menu_button(tr!("menu-selected-content"), |ui| {
        let enabled = !ids.is_empty();
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(tr!("action-rename-project-object"))
                    .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_RENAME_OBJECT)),
            )
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
                enabled,
                egui::Button::new(tr!("action-duplicate-project-object"))
                    .shortcut_text(ui.ctx().format_shortcut(&SHORTCUT_DUPLICATE_SHEET)),
            )
            .clicked()
        {
            actions.push(Action::DuplicateObjects(ids.to_vec()));
            ui.close();
        }
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(tr!("action-copy")).shortcut_text(ui.ctx().format_shortcut(
                    &egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C),
                )),
            )
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
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(tr!("action-delete-project-object"))
                    .shortcut_text(shortcut_text_delete()),
            )
            .clicked()
        {
            actions.push(Action::DeleteObjects(ids.to_vec()));
            ui.close();
        }
    });
}

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
    if pending.is_some() || (memory.focus && memory.rename.is_empty() && !ctx.text_edit_focused()) {
        let parent = pending.unwrap_or(interaction.root);
        let mut paste = None;
        ctx.input_mut(|input| {
            if memory.focus && !ids.is_empty() {
                if input.consume_key(egui::Modifiers::NONE, egui::Key::F2) {
                    memory.rename = ids.clone();
                    memory.renamed.clear();
                    memory.text = None;
                }
                if input.consume_key(egui::Modifiers::COMMAND, egui::Key::D) {
                    interaction
                        .actions
                        .push(Action::DuplicateObjects(ids.clone()));
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Delete) {
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
        memory.next_order.clear();
        data.insert_temp(memory_key, memory);
    });
    let response = egui::ScrollArea::vertical().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        for node in tree {
            project_tree_node_ui(ui, node, interaction);
        }
    });
    ui.ctx().data_mut(|data| {
        let mut memory = data.get_temp::<PmMemory>(memory_key).unwrap_or_default();
        memory.order = std::mem::take(&mut memory.next_order);
        data.insert_temp(memory_key, memory);
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

/// Peint récursivement un nœud de [`ProjectTreeNode`] et ses enfants ; rend
/// le centre vertical de sa rangée, pour les connecteurs de son parent.
fn project_tree_node_ui(
    ui: &mut egui::Ui,
    node: &ProjectTreeNode,
    interaction: &mut PmInteraction,
) -> f32 {
    let memory_key = memory_id(ui.ctx());
    match node {
        ProjectTreeNode::Folder { id, name, children } => {
            let state_id = ui.make_persistent_id(("project_tree_folder", id));
            let (toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                state_id,
                false,
            )
            .show_header(ui, |ui| {
                ui.add(icon_image(Icon::Folder, IconRole::Folder));
                let response = ui.label(name);
                response.context_menu(|ui| {
                    object_menu(ui, &[], *id, interaction.actions, ui.ctx().viewport_id())
                });
            })
            .body(|ui| {
                crate::tree_lines::tree_children(ui, |ui, branch| {
                    if children.is_empty() {
                        branch.mark(ui.weak(tr!("project-folder-empty")).rect.center().y);
                    }
                    for child in children {
                        branch.mark(project_tree_node_ui(ui, child, interaction));
                    }
                });
            });
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
            ui.ctx().data_mut(|data| {
                let mut memory = data.get_temp::<PmMemory>(memory_key).unwrap_or_default();
                memory.next_order.push(tab);
                data.insert_temp(memory_key, memory);
            });
            let editing = ui
                .ctx()
                .data(|data| data.get_temp::<PmMemory>(memory_key))
                .is_some_and(|m| m.rename.first() == Some(id));
            if editing {
                let mut memory = ui
                    .ctx()
                    .data(|data| data.get_temp::<PmMemory>(memory_key))
                    .unwrap_or_default();
                let fresh = memory.text.is_none();
                let text = memory.text.get_or_insert_with(|| name.clone());
                let response = ui.add(
                    egui::TextEdit::singleline(text)
                        .id(egui::Id::new(("pm_rename", id)))
                        .desired_width(ui.available_width()),
                );
                if fresh {
                    response.request_focus();
                }
                let escape = ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                let enter = ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                if escape {
                    memory.rename.clear();
                    memory.renamed.clear();
                    memory.text = None;
                } else if enter || (!fresh && response.lost_focus()) {
                    response.surrender_focus();
                    memory.renamed.push((*id, text.clone()));
                    memory.rename.remove(0);
                    if memory.rename.is_empty() {
                        interaction
                            .actions
                            .push(Action::RenameObjects(std::mem::take(&mut memory.renamed)));
                    }
                    memory.text = None;
                }
                ui.ctx()
                    .data_mut(|data| data.insert_temp(memory_key, memory));
                return response.rect.center().y;
            }
            let selected = interaction.selection.contains(&tab);
            let is_hidden = interaction.hidden.contains(&tab);
            // #144 : le bouton porte la sélection dans sa propre forme arrondie,
            // y compris au survol ; le cadre ne garde que l'espacement. Le salt
            // par onglet évite que toutes les lignes partagent le même Id.
            let response = ui
                .push_id(tab, |ui| {
                    egui::Frame::NONE
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            let text = if is_hidden {
                                egui::RichText::new(name).weak()
                            } else {
                                egui::RichText::new(name)
                            };
                            ui.add(
                                egui::Button::image_and_text(icon_image(icon.0, icon.1), text)
                                    .selected(selected),
                            )
                        })
                        .inner
                })
                .inner;
            if response.clicked() || response.secondary_clicked() {
                let modifiers = ui.input(|input| input.modifiers);
                let mut memory = ui
                    .ctx()
                    .data(|data| data.get_temp::<PmMemory>(memory_key))
                    .unwrap_or_default();
                if response.secondary_clicked() {
                    if !selected {
                        interaction.selection.clear();
                        interaction.selection.insert(tab);
                    }
                } else if modifiers.shift {
                    let anchor = memory.anchor.unwrap_or(tab);
                    if let (Some(a), Some(b)) = (
                        memory.order.iter().position(|t| *t == anchor),
                        memory.order.iter().position(|t| *t == tab),
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
                    if !interaction.selection.remove(&tab) {
                        interaction.selection.insert(tab);
                    }
                    memory.anchor = Some(tab);
                } else {
                    interaction.selection.clear();
                    interaction.selection.insert(tab);
                    memory.anchor = Some(tab);
                    interaction.open_requests.push(tab);
                }
                memory.focus = true;
                ui.ctx()
                    .data_mut(|data| data.insert_temp(memory_key, memory));
            }
            response.context_menu(|ui| {
                if ui
                    .button(tr!("project-manager-show"))
                    .on_hover_text(tr!("project-manager-show", "hint"))
                    .clicked()
                {
                    for &tab in interaction.selection.iter() {
                        interaction.hidden.remove(&tab);
                        interaction.open_requests.push(tab);
                    }
                    ui.close();
                }
                if ui
                    .button(tr!("project-manager-hide"))
                    .on_hover_text(tr!("project-manager-hide", "hint"))
                    .clicked()
                {
                    for &tab in interaction.selection.iter() {
                        interaction.hidden.insert(tab);
                    }
                    ui.close();
                }
                ui.separator();
                let memory = ui
                    .ctx()
                    .data(|data| data.get_temp::<PmMemory>(memory_key))
                    .unwrap_or_default();
                let ids = selected_ids(interaction.selection, &memory.order);
                object_menu(
                    ui,
                    &ids,
                    interaction.root,
                    interaction.actions,
                    ui.ctx().viewport_id(),
                );
            });
            if response.double_clicked() && interaction.active_tab == Some(tab) {
                interaction.close_requests.push(tab);
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
                self.pm_selection.extend(active_tab);
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
        active_tab: Option<Tab>,
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
                let mut interaction = PmInteraction {
                    actions,
                    root: self.state.tree().root(),
                    open_requests,
                    close_requests,
                    active_tab,
                    selection: &mut self.pm_selection,
                    hidden: &mut self.hidden_tabs,
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

    use super::super::test_support::painted;
    use crate::state::AppState;

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
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
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

    /// Les accumulateurs de [`PmInteraction`] d'un test, et une image peinte
    /// de l'arbre sur un écran de 400 × 400.
    struct Banc {
        actions: RefCell<Vec<Action>>,
        open_requests: RefCell<Vec<Tab>>,
        close_requests: RefCell<Vec<Tab>>,
        active_tab: Option<Tab>,
        selection: RefCell<HashSet<Tab>>,
        hidden: RefCell<HashSet<Tab>>,
    }

    impl Banc {
        fn new(active_tab: Option<Tab>, selection: HashSet<Tab>) -> Self {
            Banc {
                actions: RefCell::new(Vec::new()),
                open_requests: RefCell::new(Vec::new()),
                close_requests: RefCell::new(Vec::new()),
                active_tab,
                selection: RefCell::new(selection),
                hidden: RefCell::new(HashSet::new()),
            }
        }

        fn paint(
            &self,
            context: &egui::Context,
            tree: &[ProjectTreeNode],
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            let input = egui::RawInput {
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
                close_requests: &mut self.close_requests.borrow_mut(),
                active_tab: self.active_tab,
                selection: &mut self.selection.borrow_mut(),
                hidden: &mut self.hidden.borrow_mut(),
            };
            let mut output = context.run_ui(input, |ui| {
                project_manager_contents_from_tree(ui, tree, &mut interaction);
            });
            output.textures_delta.clear();
            output
        }
    }

    #[test]
    fn la_selection_du_project_manager_utilise_la_forme_arrondie_des_boutons() {
        let mut project = Project::new("Sélection arrondie").unwrap();
        let root = project.root();
        let book = Tab::Book(project.add_empty_workbook(root, None).unwrap());
        let graph = Tab::Graph(project.add_graph(root, None, PlotSpec::default()).unwrap());
        let note = Tab::Note(project.add_note(root, None, "").unwrap());
        let matrix = Tab::Matrix(project.add_matrix(root, None).unwrap());
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let tree = state.project_tree();
        let context = super::super::test_support::contexte();
        let banc = Banc::new(None, HashSet::from([book, graph, note, matrix]));
        banc.paint(&context, &tree, Vec::new());
        let output = banc.paint(&context, &tree, Vec::new());
        let fill = context.global_style().visuals.selection.bg_fill;
        let selected_rects: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == fill => Some(rect),
                _ => None,
            })
            .collect();
        assert_eq!(
            selected_rects.len(),
            4,
            "un seul fond par entrée sélectionnée"
        );
        for rect in selected_rects {
            assert_ne!(
                rect.corner_radius,
                egui::CornerRadius::ZERO,
                "la sélection épouse l'arrondi du bouton"
            );
        }
        banc.selection.borrow_mut().clear();
        let output = banc.paint(&context, &tree, Vec::new());
        assert!(
            !output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill == fill)),
            "le témoin sans sélection n'a pas cette teinte"
        );
    }

    #[test]
    fn le_suivi_du_contenu_actif_preserve_la_selection_multiple() {
        let (session, book, graph, _tree) = project_manager_fixture();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
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
        assert_eq!(app.pm_selection, HashSet::from([book]));
        assert_anchor(Some(book));
        app.follow_project_manager_active_tab(&context, Some(graph));
        assert_eq!(app.pm_selection, HashSet::from([graph]));
        assert_anchor(Some(graph));
        app.pm_selection.insert(book);
        app.follow_project_manager_active_tab(&context, Some(book));
        assert_eq!(app.pm_selection, HashSet::from([book, graph]));
        assert_anchor(Some(graph));
        app.follow_project_manager_active_tab(&context, None);
        assert_eq!(app.pm_selection, HashSet::from([book, graph]));
        // Le clic simple existant remplace la sélection. Même fenêtre active :
        // ne pas écraser ce clic avant l'application de sa demande d'ouverture.
        app.pm_selection = HashSet::from([book]);
        app.follow_project_manager_active_tab(&context, None);
        assert_eq!(app.pm_selection, HashSet::from([book]));
        app.follow_project_manager_active_tab(&context, Some(graph));
        assert_eq!(app.pm_selection, HashSet::from([graph]));
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
                                close_requests: &mut Vec::new(),
                                active_tab: Some(graph),
                                selection: &mut app.pm_selection,
                                hidden: &mut HashSet::new(),
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
            assert_eq!(app.borrow().pm_selection, HashSet::from([book]));
            click_at(
                &context,
                egui::pos2(410.0, 150.0),
                egui::Modifiers::NONE,
                paint,
            );
            assert_eq!(app.borrow().pm_selection, HashSet::from([graph]));
            let detached_key = egui::Id::new(("project_manager_gestures", viewport_id()));
            context.data(|data| {
                assert_eq!(
                    data.get_temp::<PmMemory>(detached_key).unwrap().anchor,
                    Some(graph),
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
                    HashSet::from([graph])
                } else {
                    HashSet::from([book, graph])
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
        let banc = Banc::new(None, HashSet::new());
        let paint = |ctx: &egui::Context, events| banc.paint(ctx, &tree, events);
        paint(&context, vec![]);
        let first = paint(&context, vec![]);
        let book_pos = find_text_center(&first.shapes, "Book");
        let graph_pos = find_text_center(&first.shapes, "Graph");
        click_at(&context, book_pos, egui::Modifiers::NONE, paint);
        click_at(&context, graph_pos, egui::Modifiers::SHIFT, paint);
        assert_eq!(*banc.selection.borrow(), HashSet::from([book, graph]));
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
        let banc = Banc::new(None, HashSet::from([book]));
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
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let app = RefCell::new(StokesForgeApp::new(state));
        let open = RefCell::new(Vec::new());
        let close = RefCell::new(Vec::new());
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
                    &mut close.borrow_mut(),
                    None,
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
        assert!(app.borrow().pm_selection.contains(&book_tab));
        assert!(
            open.borrow().is_empty(),
            "Ctrl-clic sélectionne sans ouvrir"
        );
        click_at(&context, book, egui::Modifiers::NONE, paint);
        assert!(open.borrow().contains(&book_tab));
        assert!(close.borrow().is_empty());
        session.shutdown().unwrap();
    }

    #[test]
    fn le_double_clic_ferme_lentree_deja_au_premier_plan_mais_pas_une_autre() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(Some(graph_tab), HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        let book_pos = find_text_center(&first.shapes, "Book");

        // Double-clic sur l'entrée déjà au premier plan (`active_tab` =
        // `graph_tab`) : doit fermer, pas seulement (re)focaliser.
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        assert!(
            banc.close_requests.borrow().contains(&graph_tab),
            "le double-clic sur l'onglet déjà au premier plan doit le fermer, close_requests={:?}",
            banc.close_requests.borrow()
        );

        // Double-clic sur une entrée qui n'est *pas* au premier plan
        // (`active_tab` reste `graph_tab`) : ouvre/focalise, ne ferme rien.
        // Contexte neuf : dans le même, egui compterait ces clics à la suite
        // des deux précédents (triple clic, puis davantage) et le geste ne
        // serait plus un double-clic — la vérification ne prouverait rien.
        let double_click_on_book = |active_tab| {
            let context = egui::Context::default();
            let banc = Banc::new(Some(active_tab), HashSet::new());
            let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
            paint(&context, Vec::new());
            click_at(&context, book_pos, egui::Modifiers::NONE, paint);
            click_at(&context, book_pos, egui::Modifiers::NONE, paint);
            let open = banc.open_requests.borrow().clone();
            let close = banc.close_requests.borrow().clone();
            (open, close)
        };
        let (open, close) = double_click_on_book(graph_tab);
        assert!(
            !close.contains(&book_tab),
            "le double-clic sur une entrée qui n'est pas au premier plan ne doit pas la fermer"
        );
        assert!(
            open.contains(&book_tab),
            "le double-clic doit tout de même ouvrir/focaliser l'entrée, open_requests={open:?}"
        );
        // Témoin : le même geste, l'entrée étant au premier plan, la ferme —
        // c'est donc bien un double-clic qui a été envoyé.
        let (_, close) = double_click_on_book(book_tab);
        assert!(close.contains(&book_tab), "close_requests={close:?}");
    }

    /// Ctrl/Cmd-clic bascule l'appartenance à la sélection : il ajoute, puis
    /// retire au second clic, sans jamais ouvrir l'onglet.
    #[test]
    fn ctrl_clic_selectionne_sans_ouvrir() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(None, HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, paint);

        assert_eq!(
            *banc.selection.borrow(),
            HashSet::from([graph_tab]),
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
    fn masquer_et_afficher_agissent_sur_toute_la_selection() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        // Sélection posée directement plutôt que reconstruite par deux
        // Ctrl-clics successifs (voir `ctrl_clic_selectionne_sans_ouvrir`
        // pour ce comportement-là, testé isolément) : ce test porte sur ce
        // que « Masquer »/« Afficher » font d'une sélection déjà en place,
        // pas sur la façon dont elle s'est constituée.
        let banc = Banc::new(None, HashSet::from([graph_tab, book_tab]));
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        paint(&context, Vec::new());
        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph");
        let menu = ouvrir_menu_contextuel(&context, graph_pos, paint);
        assert_eq!(
            *banc.selection.borrow(),
            HashSet::from([graph_tab, book_tab])
        );
        let hide_pos = find_text_center(&menu.shapes, &tr!("project-manager-hide"));
        click_at(&context, hide_pos, egui::Modifiers::NONE, paint);
        assert_eq!(
            *banc.hidden.borrow(),
            HashSet::from([graph_tab, book_tab]),
            "« Masquer » doit s'appliquer à toute la sélection, hidden={:?}",
            banc.hidden.borrow()
        );
        assert!(
            !painted(&paint(&context, Vec::new()), &tr!("project-manager-hide")),
            "Masquer ferme le menu contextuel"
        );

        let menu = ouvrir_menu_contextuel(&context, graph_pos, paint);
        let show_pos = find_text_center(&menu.shapes, &tr!("project-manager-show"));
        click_at(&context, show_pos, egui::Modifiers::NONE, paint);
        assert!(
            banc.hidden.borrow().is_empty(),
            "« Afficher » doit vider le masquage de toute la sélection, hidden={:?}",
            banc.hidden.borrow()
        );
        assert!(
            banc.open_requests.borrow().contains(&graph_tab)
                && banc.open_requests.borrow().contains(&book_tab),
            "« Afficher » doit aussi garantir que chaque onglet est ouvert, open_requests={:?}",
            banc.open_requests.borrow()
        );
        assert!(
            !painted(&paint(&context, Vec::new()), &tr!("project-manager-show")),
            "Afficher ferme le menu contextuel"
        );
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
        let banc = Banc::new(None, HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        assert!(
            !painted(&first, &tr!("project-manager-hide"))
                && !painted(&first, &tr!("project-manager-show")),
            "sans sélection, aucun bouton groupé à regarder"
        );
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*banc.open_requests.borrow(), vec![graph_tab]);
        assert_eq!(
            *banc.selection.borrow(),
            HashSet::from([graph_tab]),
            "un clic simple sélectionne sa cible (#105)"
        );
        assert!(banc.close_requests.borrow().is_empty());

        // La sélection ne déplace pas les entrées sous la souris.
        let with_selection = paint(&context, Vec::new());
        assert!(!painted(&with_selection, &tr!("project-manager-hide")));
        assert!(!painted(&with_selection, &tr!("project-manager-show")));
        assert_eq!(
            find_text_center(&with_selection.shapes, "Graph1"),
            graph_pos
        );
        let menu = ouvrir_menu_contextuel(&context, graph_pos, paint);
        assert!(painted(&menu, &tr!("project-manager-hide")));
        assert!(painted(&menu, &tr!("project-manager-show")));
    }

    /// Une entrée masquée reste listée, atténuée : sa couleur diffère de celle
    /// de la même entrée affichée, et ses voisines ne changent pas.
    #[test]
    fn une_entree_masquee_reste_listee_mais_attenuee() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(None, HashSet::new());
        banc.paint(&context, &tree, Vec::new());
        let shown = banc.paint(&context, &tree, Vec::new());
        banc.hidden.borrow_mut().insert(graph_tab);
        let hidden = banc.paint(&context, &tree, Vec::new());
        assert_ne!(
            text_color(&hidden.shapes, "Graph1"),
            text_color(&shown.shapes, "Graph1"),
            "l'entrée masquée doit se distinguer de l'entrée affichée"
        );
        assert_eq!(
            text_color(&hidden.shapes, "Book"),
            text_color(&shown.shapes, "Book"),
            "les autres entrées ne changent pas"
        );
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
        let banc = Banc::new(None, HashSet::new());
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
}

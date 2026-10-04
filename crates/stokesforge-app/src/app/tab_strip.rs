//! **La** bande d'onglets de l'application (standard du projet, issue #19) :
//! [`tab_strip`] / [`tab_strip_with`], sa mémoire de débordement, ses gestes
//! de renommage, duplication et suppression ([`TabOps`], [`TabEvent`]) et son
//! défilement automatique ([`scroll_tab_strip_to_selected`]).
//!
//! Sorti d'`app.rs` tel quel (#117). La bande des feuilles d'un classeur
//! (`sheet_tabs`) et le menu Classeur restent avec la grille.

use eframe::egui;

use crate::icons::{icon_button, Icon, IconRole};
use crate::tr;

use super::shortcuts::{SHORTCUT_DUPLICATE_SHEET, SHORTCUT_RENAME_SHEET};

/// Hauteur fixe du panneau des onglets de feuille sous la grille.
/// `book_contents` la réserve avant de construire la table : la hauteur
/// intrinsèque de cette dernière ne peut plus repousser les onglets hors
/// de la fenêtre. Juste la hauteur d'un onglet et un filet : la bande n'a
/// plus de barre de défilement à loger (boutons « < » « > », voir
/// `sheet_tabs`), et la barre horizontale de la grille est en dessous.
pub(super) const TAB_STRIP_HEIGHT: f32 = 24.0;

/// Mémoire d'une image à l'autre de la bande d'onglets de feuille : de quoi
/// savoir, **avant** de la dessiner (donc avant de placer les boutons « < »
/// et « > »), si des onglets sont cachés à gauche ou à droite — ce qui ne se
/// constate qu'en dessinant les onglets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct TabStripOverflow {
    pub(super) hidden_left: bool,
    pub(super) hidden_right: bool,
}

/// Ce que [`sheet_tabs`] a dessiné : décalage de la bande, onglets cachés,
/// et rectangles des boutons « < » et « > » s'ils sont affichés. Seuls les
/// tests le lisent ; l'interface n'en a pas besoin.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TabStripOutput {
    pub(super) offset: f32,
    pub(super) overflow: TabStripOverflow,
    pub(super) arrows: Option<[egui::Rect; 2]>,
    /// Indices du premier et du dernier onglet entièrement visibles.
    pub(super) fully_visible: Option<(usize, usize)>,
}

/// **La** bande d'onglets de l'application (standard du projet, issue #19) :
/// tout jeu d'onglets — feuilles d'un classeur, pages de la fenêtre des
/// propriétés du graphe — passe par elle, jamais par une rangée de
/// `selectable_label`. Style d'origine (bouton pleine taille, sans
/// troncature) : quand elle ne tient pas dans la largeur disponible, elle
/// défile horizontalement plutôt que de rétrécir ses onglets ou de replier
/// les feuilles cachées dans un menu « … ». Elle n'a **pas** de barre de
/// défilement : deux boutons « < » et « > », à gauche de la bande, font
/// apparaître l'onglet caché suivant de ce côté — et n'apparaissent que si
/// des onglets sont cachés. Changer de feuille sélectionnée (au clavier, par
/// script, ou par clic) fait défiler la bande juste assez pour montrer
/// l'onglet actif et ses voisins ([`scroll_tab_strip_to_selected`]).
pub(super) fn tab_strip<K: Copy + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    tabs: &[(K, String)],
    selected: K,
    on_select: impl FnMut(K),
    add_label: Option<&str>,
    on_add: impl FnMut(),
) -> TabStripOutput {
    tab_strip_with(
        ui,
        tabs,
        selected,
        on_select,
        add_label,
        on_add,
        None,
        |_| {},
    )
}

/// [`tab_strip`] avec, en option, les gestes sur une feuille : double-clic
/// (renommage en place, validé par Entrée ou en quittant le champ, abandonné
/// par Échap), menu contextuel Renommer / Dupliquer / Supprimer. Les demandes
/// remontent par `on_event` ; la bande ne touche jamais au modèle.
#[allow(clippy::too_many_arguments)]
pub(super) fn tab_strip_with<K: Copy + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    tabs: &[(K, String)],
    selected: K,
    mut on_select: impl FnMut(K),
    add_label: Option<&str>,
    mut on_add: impl FnMut(),
    ops: Option<&TabOps<K>>,
    mut on_event: impl FnMut(TabEvent<K>),
) -> TabStripOutput {
    let mut events: Vec<TabEvent<K>> = Vec::new();
    let add_menu = |ui: &mut egui::Ui, requested_add: &mut bool| {
        if let Some(label) = add_label {
            if ui.button(label).clicked() {
                *requested_add = true;
                ui.close();
            }
        }
    };
    let tab_menu =
        |ui: &mut egui::Ui, id: K, events: &mut Vec<TabEvent<K>>, requested_add: &mut bool| {
            if let Some(ops) = ops {
                let ctx = ui.ctx().clone();
                if ui
                    .add(
                        egui::Button::new(tr!("action-rename-sheet"))
                            .shortcut_text(ctx.format_shortcut(&SHORTCUT_RENAME_SHEET)),
                    )
                    .on_hover_text(tr!("action-rename-sheet", "hint"))
                    .clicked()
                {
                    events.push(TabEvent::BeginRename(id));
                    ui.close();
                }
                if ui
                    .add(
                        egui::Button::new(tr!("action-duplicate-sheet"))
                            .shortcut_text(ctx.format_shortcut(&SHORTCUT_DUPLICATE_SHEET)),
                    )
                    .on_hover_text(tr!("action-duplicate-sheet", "hint"))
                    .clicked()
                {
                    events.push(TabEvent::Duplicate(id));
                    ui.close();
                }
                if ui
                    .add_enabled(
                        ops.can_delete,
                        egui::Button::new(tr!("action-delete-sheet")),
                    )
                    .on_hover_text(tr!("action-delete-sheet", "hint"))
                    .on_disabled_hover_text(tr!("action-delete-sheet", "disabled"))
                    .clicked()
                {
                    events.push(TabEvent::Delete(id));
                    ui.close();
                }
                ui.separator();
            }
            add_menu(ui, requested_add);
        };
    if tabs.is_empty() {
        let mut requested_add = false;
        ui.allocate_response(
            egui::vec2(ui.available_width(), TAB_STRIP_HEIGHT),
            egui::Sense::click(),
        )
        .context_menu(|ui| add_menu(ui, &mut requested_add));
        if requested_add {
            on_add();
        }
        return TabStripOutput::default();
    }
    // Mémorisée par instance de bande (scopée par la pile d'`Id` du parent,
    // donc distincte entre fenêtre flottante et fenêtre détachée) : ne
    // redéclenche le défilement automatique que lorsque la sélection change
    // réellement d'une image à l'autre, pas à chaque image où elle est
    // simplement reconduite — sinon un défilement manuel de l'utilisateur
    // serait annulé en continu.
    let last_selected_id = ui.id().with("tab_strip_last_selected");
    let last_selected: Option<K> = ui.memory(|memory| memory.data.get_temp(last_selected_id));
    let should_scroll_to_selected = last_selected != Some(selected);
    ui.memory_mut(|memory| memory.data.insert_temp(last_selected_id, selected));
    let overflow_id = ui.id().with("tab_strip_overflow");
    let overflow: TabStripOverflow = ui
        .memory(|memory| memory.data.get_temp(overflow_id))
        .unwrap_or_default();

    let mut requested = None;
    let mut requested_add = false;
    // -1 : faire apparaître l'onglet caché à gauche ; +1 : à droite.
    let mut step: i8 = 0;
    let mut new_overflow = TabStripOverflow::default();
    let mut arrows = None;
    let mut offset = 0.0;
    let mut fully_visible: Option<(usize, usize)> = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if overflow.hidden_left || overflow.hidden_right {
            let previous = ui
                .add_enabled_ui(overflow.hidden_left, |ui| {
                    icon_button(
                        ui,
                        Icon::CaretLeft,
                        IconRole::Neutral,
                        tr!("tabs-previous-worksheets"),
                    )
                })
                .inner;
            let next = ui
                .add_enabled_ui(overflow.hidden_right, |ui| {
                    icon_button(
                        ui,
                        Icon::CaretRight,
                        IconRole::Neutral,
                        tr!("tabs-next-worksheets"),
                    )
                })
                .inner;
            if previous.clicked() {
                step = -1;
            }
            if next.clicked() {
                step = 1;
            }
            arrows = Some([previous.rect, next.rect]);
        }
        let band_width = ui.available_width();
        offset = egui::ScrollArea::horizontal()
            .id_salt("tab_strip_scroll")
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let visible = ui.clip_rect();
                    let mut last_hidden_left = None;
                    let mut first_hidden_right = None;
                    let mut tab_rects = Vec::with_capacity(tabs.len());
                    let mut selected_index = None;
                    for (index, (id, name)) in tabs.iter().enumerate() {
                        let renaming = ops.is_some_and(|ops| ops.renaming == Some(*id));
                        let response = if renaming {
                            let buffer_id = ui.id().with("tab_rename_buffer");
                            let fresh = ui
                                .memory(|memory| memory.data.get_temp::<String>(buffer_id))
                                .is_none();
                            let mut text = ui
                                .memory(|memory| memory.data.get_temp::<String>(buffer_id))
                                .unwrap_or_else(|| name.clone());
                            let width = (name.chars().count().max(6) as f32 * 8.0).min(240.0);
                            let edit_id = buffer_id.with("edit");
                            let output = egui::TextEdit::singleline(&mut text)
                                .id(edit_id)
                                .font(egui::FontId::proportional(12.0))
                                .desired_width(width)
                                .show(ui);
                            if fresh {
                                output.response.request_focus();
                                let mut state = output.state;
                                state
                                    .cursor
                                    .set_char_range(Some(egui::text::CCursorRange::two(
                                        egui::text::CCursor::new(0),
                                        egui::text::CCursor::new(text.chars().count()),
                                    )));
                                state.store(ui.ctx(), edit_id);
                            }
                            let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
                            if escape {
                                ui.memory_mut(|memory| memory.data.remove::<String>(buffer_id));
                                events.push(TabEvent::CancelRename);
                            } else if !fresh && output.response.lost_focus() {
                                ui.memory_mut(|memory| memory.data.remove::<String>(buffer_id));
                                events.push(TabEvent::CommitRename(*id, text));
                            } else {
                                ui.memory_mut(|memory| memory.data.insert_temp(buffer_id, text));
                            }
                            output.response.response
                        } else {
                            let tab = egui::Button::new(egui::RichText::new(name).size(12.0))
                                .small()
                                .fill(if selected == *id {
                                    ui.visuals().selection.bg_fill
                                } else {
                                    egui::Color32::TRANSPARENT
                                });
                            let response = ui.add(tab);
                            response.context_menu(|ui| {
                                tab_menu(ui, *id, &mut events, &mut requested_add)
                            });
                            if response.clicked() {
                                requested = Some(*id);
                            }
                            if ops.is_some() && response.double_clicked() {
                                events.push(TabEvent::BeginRename(*id));
                            }
                            response
                        };
                        tab_rects.push(response.rect);
                        if selected == *id {
                            selected_index = Some(index);
                        }
                        if response.rect.left() >= visible.left() - 0.5
                            && response.rect.right() <= visible.right() + 0.5
                        {
                            let first = fully_visible.map_or(index, |(first, _)| first);
                            fully_visible = Some((first, index));
                        }
                        if response.rect.left() < visible.left() - 0.5 {
                            last_hidden_left = Some(response.clone());
                        }
                        if first_hidden_right.is_none()
                            && response.rect.right() > visible.right() + 0.5
                        {
                            first_hidden_right = Some(response.clone());
                        }
                    }
                    new_overflow = TabStripOverflow {
                        hidden_left: last_hidden_left.is_some(),
                        hidden_right: first_hidden_right.is_some(),
                    };
                    if should_scroll_to_selected {
                        if let Some(index) = selected_index {
                            scroll_tab_strip_to_selected(ui, &tab_rects, index, visible);
                        }
                    }
                    match step {
                        -1 => {
                            if let Some(tab) = &last_hidden_left {
                                tab.scroll_to_me(Some(egui::Align::Min));
                            }
                        }
                        1 => {
                            if let Some(tab) = &first_hidden_right {
                                tab.scroll_to_me(Some(egui::Align::Max));
                            }
                        }
                        _ => {}
                    }
                    let used = ui.cursor().left() - ui.min_rect().left();
                    let empty = ui.allocate_response(
                        egui::vec2((band_width - used).max(1.0), TAB_STRIP_HEIGHT),
                        egui::Sense::click(),
                    );
                    empty.context_menu(|ui| add_menu(ui, &mut requested_add));
                });
            })
            .state
            .offset
            .x;
    });
    ui.memory_mut(|memory| memory.data.insert_temp(overflow_id, new_overflow));
    if step != 0 {
        ui.ctx().request_repaint();
    }
    if let Some(id) = requested {
        on_select(id);
    }
    if requested_add {
        on_add();
    }
    for event in events {
        on_event(event);
    }
    TabStripOutput {
        offset,
        overflow: new_overflow,
        arrows,
        fully_visible,
    }
}

/// Ce que la bande d'onglets demande sur une feuille ; l'appelant en fait des
/// [`Action`].
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TabEvent<K> {
    BeginRename(K),
    CommitRename(K, String),
    CancelRename,
    Duplicate(K),
    Delete(K),
}

/// Gestes de renommage, de duplication et de suppression d'une bande d'onglets
/// de feuilles.
pub(super) struct TabOps<K> {
    /// Onglet dont le nom est en cours d'édition.
    pub(super) renaming: Option<K>,
    /// Faux sur la dernière feuille d'un classeur : « Supprimer » grisé.
    pub(super) can_delete: bool,
}

/// Défilement automatique de la bande d'onglets quand la feuille
/// sélectionnée change : **le plus petit décalage** qui rend entièrement
/// visibles l'onglet sélectionné et ses deux voisins immédiats (pour qu'on
/// voie toujours ce qui vient ensuite). Rien ne bouge s'ils le sont déjà ; sinon
/// la bande s'aligne sur le bord dépassé. Si la bande est trop étroite pour
/// ces trois onglets, l'onglet sélectionné est centré.
///
/// Issue #1 : la bande centrait auparavant l'onglet sélectionné à chaque
/// changement — cliquer sur le dernier onglet visible décalait alors de deux
/// onglets ou plus au lieu d'un.
fn scroll_tab_strip_to_selected(
    ui: &egui::Ui,
    tab_rects: &[egui::Rect],
    selected: usize,
    visible: egui::Rect,
) {
    let current = tab_rects[selected];
    let first = tab_rects[selected.saturating_sub(1)];
    let last = tab_rects[(selected + 1).min(tab_rects.len() - 1)];
    let target = first.union(last);
    if target.width() > visible.width() {
        ui.scroll_to_rect(current, Some(egui::Align::Center));
    } else if target.right() > visible.right() + 0.5 {
        ui.scroll_to_rect(target, Some(egui::Align::Max));
    } else if target.left() < visible.left() - 0.5 {
        ui.scroll_to_rect(target, Some(egui::Align::Min));
    }
}

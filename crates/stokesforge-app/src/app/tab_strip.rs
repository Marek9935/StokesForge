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

#[cfg(test)]
mod tests {
    use super::*;

    /// Dix onglets « Feuille N », de clés `0..10`.
    fn dix_onglets() -> Vec<(u32, String)> {
        (0..10)
            .map(|index| (index, format!("Feuille {index}")))
            .collect()
    }

    /// Un contexte aux polices et au thème de l'application : la largeur des
    /// onglets en dépend.
    fn contexte() -> egui::Context {
        let context = egui::Context::default();
        context.set_fonts(stokesforge_render_egui::font_definitions());
        crate::theme::install(&context);
        context
    }

    /// Peint une bande d'onglets dans `largeur` points, à l'instant
    /// `*time + 0.1`, avec `events`. Le défilement automatique est animé
    /// (`Response::scroll_to_me`) : il s'étale sur `ScrollAnimation::duration`
    /// (jusqu'à 0.3 s par défaut) ; faire avancer `RawInput::time`
    /// explicitement simule ce laps de temps sans dépendre de l'horloge réelle.
    fn peindre_onglets(
        context: &egui::Context,
        largeur: f32,
        tabs: &[(u32, String)],
        selected: u32,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> (TabStripOutput, egui::FullOutput) {
        *time += 0.1;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(largeur, 60.0),
            )),
            time: Some(*time),
            events,
            ..Default::default()
        };
        let mut sortie = TabStripOutput::default();
        let mut output = context.run_ui(input, |ui| {
            sortie = tab_strip(ui, tabs, selected, |_| {}, None, || {});
        });
        output.textures_delta.clear();
        (sortie, output)
    }

    /// [`peindre_onglets`] dans 400 points (cinq onglets environ, sur dix).
    fn peindre(
        context: &egui::Context,
        tabs: &[(u32, String)],
        selected: u32,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> TabStripOutput {
        peindre_onglets(context, 400.0, tabs, selected, time, events).0
    }

    /// Centre du texte peint `needle`, s'il l'est.
    fn position(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        fn walk(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == needle => {
                    Some(text.visual_bounding_rect().center())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| walk(s, needle)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|clipped| walk(&clipped.shape, needle))
    }

    fn clic(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// #19 : tous les onglets d'une bande qui tient dans sa largeur sont
    /// peints, et un clic sur l'un d'eux le choisit. Le témoin : tant qu'on
    /// ne clique pas, rien n'est choisi — la sélection reconduite d'une image
    /// à l'autre n'est pas un choix. (C'est la bande des propriétés du
    /// graphe ; qu'elles l'emploient est tenu par
    /// `un_clic_sur_un_onglet_des_proprietes_change_de_page`, dans
    /// `app/plot_settings.rs`.)
    #[test]
    fn un_clic_sur_un_onglet_le_choisit() {
        let context = contexte();
        let tabs: Vec<(u32, String)> = ["Graphe", "Axes", "Séries", "Thème"]
            .iter()
            .enumerate()
            .map(|(index, name)| (index as u32, name.to_string()))
            .collect();
        let chosen = std::cell::Cell::new(None);
        let run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 200.0),
                )),
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                tab_strip(ui, &tabs, 0, |tab| chosen.set(Some(tab)), None, || {});
            });
            output.textures_delta.clear();
            output
        };
        run(vec![]);
        let output = run(vec![]);
        for (_, name) in &tabs {
            assert!(position(&output, name).is_some(), "{name}");
        }
        let theme = position(&output, "Thème").unwrap();
        run(vec![egui::Event::PointerMoved(theme)]);
        assert_eq!(chosen.get(), None, "survoler un onglet ne le choisit pas");
        run(vec![clic(theme, true)]);
        run(vec![clic(theme, false)]);
        assert_eq!(chosen.get(), Some(3));
    }

    /// Dix onglets en taille naturelle ne tiennent pas dans 200 points : la
    /// bande doit défiler plutôt que rétrécir ses boutons ou replier les
    /// feuilles cachées dans un menu. Chaque onglet peint garde son nom
    /// entier, à la même largeur que dans une bande assez large pour tous
    /// (témoin), et les onglets qui ne tiennent pas sont cachés à droite.
    #[test]
    fn les_onglets_defilent_plutot_que_de_retrecir() {
        let tabs = dix_onglets();
        let largeur_du_nom = |output: &egui::FullOutput, needle: &str| {
            output
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(text) if text.galley.text() == needle => {
                        Some(text.galley.rect.width())
                    }
                    _ => None,
                })
        };

        let large = contexte();
        let mut time = 0.0;
        let mut au_large = None;
        for _ in 0..3 {
            au_large = Some(peindre_onglets(
                &large,
                2000.0,
                &tabs,
                0,
                &mut time,
                Vec::new(),
            ));
        }
        let (sortie_large, output_large) = au_large.unwrap();
        assert_eq!(sortie_large.fully_visible, Some((0, 9)));
        assert!(sortie_large.arrows.is_none());

        let etroit = contexte();
        let mut a_l_etroit = None;
        for _ in 0..3 {
            a_l_etroit = Some(peindre_onglets(
                &etroit,
                200.0,
                &tabs,
                0,
                &mut time,
                Vec::new(),
            ));
        }
        let (sortie, output) = a_l_etroit.unwrap();
        assert!(
            sortie.overflow.hidden_right,
            "des onglets doivent être cachés à droite : {sortie:?}"
        );
        let (premier, dernier) = sortie.fully_visible.expect("des onglets visibles");
        assert!(premier == 0 && dernier < 9, "{sortie:?}");
        for (_, name) in &tabs[..=dernier] {
            assert_eq!(
                largeur_du_nom(&output, name),
                largeur_du_nom(&output_large, name),
                "{name} doit garder sa largeur naturelle"
            );
        }
    }

    #[test]
    fn selectionner_un_onglet_hors_vue_fait_defiler_la_bande_pour_le_montrer() {
        let context = contexte();
        let tabs = dix_onglets();
        let mut time = 0.0;
        // Sélectionner d'abord le premier onglet, laisser le défilement se
        // stabiliser, puis basculer sur le dernier : hors vue, il doit
        // déclencher un nouveau défilement (`should_scroll_to_selected`).
        let mut avant = TabStripOutput::default();
        for _ in 0..3 {
            avant = peindre(&context, &tabs, 0, &mut time, Vec::new());
        }
        let mut apres = TabStripOutput::default();
        for _ in 0..8 {
            apres = peindre(&context, &tabs, 9, &mut time, Vec::new());
        }
        assert!(
            apres.offset > avant.offset,
            "sélectionner un onglet hors vue doit faire défiler la bande vers lui : {avant:?} -> {apres:?}"
        );
        assert_eq!(apres.fully_visible.map(|(_, dernier)| dernier), Some(9));
    }

    /// Issue #1 : cliquer sur le dernier onglet entièrement visible décale la
    /// bande juste assez pour montrer son voisin de droite — un onglet de
    /// plus, pas deux (l'ancien comportement centrait l'onglet sélectionné).
    /// Symétriquement vers la gauche.
    #[test]
    fn selectionner_le_dernier_onglet_visible_ne_decale_que_d_un_onglet() {
        let context = contexte();
        let tabs = dix_onglets();
        let mut time = 0.0;
        let mut sortie = TabStripOutput::default();
        for _ in 0..4 {
            sortie = peindre(&context, &tabs, 0, &mut time, Vec::new());
        }
        let (_, dernier) = sortie.fully_visible.expect("des onglets visibles");
        assert!(
            dernier + 2 < tabs.len(),
            "il faut des onglets cachés à droite"
        );
        for _ in 0..8 {
            sortie = peindre(&context, &tabs, dernier as u32, &mut time, Vec::new());
        }
        let (premier_apres, dernier_apres) = sortie.fully_visible.unwrap();
        assert_eq!(
            dernier_apres,
            dernier + 1,
            "seul le voisin de droite de l'onglet sélectionné doit apparaître : {sortie:?}"
        );

        // Le témoin : sélectionner un onglet déjà visible au milieu de la
        // bande, voisins compris, ne la fait pas bouger.
        let decalage = sortie.offset;
        for _ in 0..8 {
            sortie = peindre(
                &context,
                &tabs,
                (premier_apres + 2) as u32,
                &mut time,
                Vec::new(),
            );
        }
        assert_eq!(sortie.offset, decalage, "{sortie:?}");

        // Vers la gauche : sélectionner le premier onglet visible montre son
        // voisin de gauche, et rien de plus.
        for _ in 0..8 {
            sortie = peindre(&context, &tabs, premier_apres as u32, &mut time, Vec::new());
        }
        assert_eq!(
            sortie.fully_visible.unwrap().0,
            premier_apres - 1,
            "seul le voisin de gauche doit apparaître : {sortie:?}"
        );

        // Un onglet sélectionné loin de la vue (par script) redevient visible.
        for _ in 0..8 {
            sortie = peindre(&context, &tabs, 9, &mut time, Vec::new());
        }
        assert_eq!(sortie.fully_visible.unwrap().1, 9, "{sortie:?}");
    }

    /// Les boutons « < » et « > » n'apparaissent que si des onglets sont
    /// cachés ; « > » fait défiler la bande vers la droite, puis « < »
    /// devient actif.
    #[test]
    fn les_fleches_d_onglets_n_apparaissent_que_si_des_onglets_sont_caches() {
        let context = contexte();
        let tabs = dix_onglets();
        let mut time = 0.0;
        let mut sortie = TabStripOutput::default();
        for _ in 0..3 {
            sortie = peindre(&context, &tabs, 0, &mut time, Vec::new());
        }
        assert_eq!(
            sortie.overflow,
            TabStripOverflow {
                hidden_left: false,
                hidden_right: true
            }
        );
        let [_, next] = sortie
            .arrows
            .expect("des onglets cachés : flèches affichées");
        let pos = next.center();
        peindre(
            &context,
            &tabs,
            0,
            &mut time,
            vec![egui::Event::PointerMoved(pos), clic(pos, true)],
        );
        peindre(&context, &tabs, 0, &mut time, vec![clic(pos, false)]);
        for _ in 0..6 {
            sortie = peindre(&context, &tabs, 0, &mut time, Vec::new());
        }
        assert!(
            sortie.offset > 0.0,
            "« > » doit faire défiler la bande : {sortie:?}"
        );
        assert!(
            sortie.overflow.hidden_left,
            "« < » devient utile : {sortie:?}"
        );

        // Deux onglets tiennent dans la bande : pas de flèches.
        let context = contexte();
        let deux = &tabs[..2];
        for _ in 0..3 {
            sortie = peindre(&context, deux, 0, &mut time, Vec::new());
        }
        assert!(
            sortie.arrows.is_none(),
            "aucun onglet caché : pas de flèches"
        );
    }

    /// Peint trois onglets avec [`tab_strip_with`], une image par lot
    /// d'événements, et rend ce que la bande a demandé : onglets choisis et
    /// [`TabEvent`] émis, dans l'ordre.
    fn gestes(
        ops: Option<&TabOps<u32>>,
        lots: Vec<Vec<egui::Event>>,
    ) -> (Vec<u32>, Vec<TabEvent<u32>>) {
        let context = contexte();
        let tabs: Vec<(u32, String)> = (0..3).map(|i| (i, format!("Feuille {i}"))).collect();
        let mut choisis = Vec::new();
        let mut demandes = Vec::new();
        for events in lots {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 60.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                tab_strip_with(
                    ui,
                    &tabs,
                    0,
                    |tab| choisis.push(tab),
                    None,
                    || {},
                    ops,
                    |event| demandes.push(event),
                );
            });
            output.textures_delta.clear();
        }
        (choisis, demandes)
    }

    /// Le double-clic sur un onglet demande son renommage — seulement quand
    /// la bande porte des gestes ([`TabOps`]). Le témoin : sans eux, le même
    /// double-clic ne fait que choisir l'onglet.
    #[test]
    fn le_double_clic_demande_le_renommage_seulement_avec_des_gestes() {
        // Repérer la position de « Feuille 1 » : la bande ne la rend pas.
        let context = contexte();
        let tabs: Vec<(u32, String)> = (0..3).map(|i| (i, format!("Feuille {i}"))).collect();
        let mut time = 0.0;
        peindre_onglets(&context, 800.0, &tabs, 0, &mut time, Vec::new());
        let (_, output) = peindre_onglets(&context, 800.0, &tabs, 0, &mut time, Vec::new());
        let pos = position(&output, "Feuille 1").expect("onglet peint");
        let double_clic = || {
            vec![
                Vec::new(),
                vec![egui::Event::PointerMoved(pos)],
                vec![clic(pos, true)],
                vec![clic(pos, false)],
                vec![clic(pos, true)],
                vec![clic(pos, false)],
            ]
        };

        let ops = TabOps {
            renaming: None,
            can_delete: true,
        };
        let (choisis, demandes) = gestes(Some(&ops), double_clic());
        assert_eq!(demandes, vec![TabEvent::BeginRename(1)]);
        assert!(choisis.iter().all(|&tab| tab == 1));

        let (choisis, demandes) = gestes(None, double_clic());
        assert!(
            demandes.is_empty(),
            "sans gestes, pas de renommage : {demandes:?}"
        );
        assert!(!choisis.is_empty() && choisis.iter().all(|&tab| tab == 1));
    }

    /// Le renommage en place : le champ s'ouvre avec le nom entier
    /// sélectionné, la saisie le remplace et Entrée la valide ; Échap
    /// l'abandonne sans rien valider.
    #[test]
    fn le_renommage_en_place_se_valide_par_entree_et_s_abandonne_par_echap() {
        let ops = TabOps {
            renaming: Some(1),
            can_delete: true,
        };
        let touche = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };

        let (_, demandes) = gestes(
            Some(&ops),
            vec![
                Vec::new(),
                vec![egui::Event::Text("Spectres".to_string())],
                vec![touche(egui::Key::Enter)],
            ],
        );
        assert_eq!(
            demandes,
            vec![TabEvent::CommitRename(1, "Spectres".to_string())]
        );

        let (_, demandes) = gestes(
            Some(&ops),
            vec![
                Vec::new(),
                vec![egui::Event::Text("Spectres".to_string())],
                vec![touche(egui::Key::Escape)],
            ],
        );
        assert_eq!(demandes, vec![TabEvent::CancelRename]);
    }
}

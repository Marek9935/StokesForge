//! Une liste de lignes que l'on réordonne au glisser (#148, formats de copie
//! du graphique).
//!
//! Le geste : la poignée en tête de ligne se saisit ; la ligne
//! entière se glisse (son contenu suit le pointeur depuis l'endroit saisi, dans
//! une couche flottante) et sa place d'origine reste réservée, vide. Les autres
//! lignes s'écartent **tout de suite** quand le pointeur en survole une : la
//! liste est déjà dans son nouvel ordre au relâchement, sans trait de repère
//! ni dépôt à traiter.
//!
//! Deux pièges levés ici. egui centre sa ligne glissée sur le pointeur
//! (`dnd_drag_source`) : la ligne sauterait de la poignée vers son milieu ;
//! on retient donc le point saisi, relatif à la ligne, et on recale la couche à
//! chaque image. Et comme la liste se réordonne pendant le glisser, la charge
//! utile est l'identifiant de la ligne, jamais son indice.
//!
//! Le module ne touche pas aux données : [`reorder_row`] peint une ligne et dit
//! si une ligne glissée la survole ; [`reorder_move`] traduit cela en
//! déplacement ; l'appelant l'applique par [`move_before`].

use eframe::egui;

use super::cursors::grab_handle;
use crate::icons::{icon_image, Icon, IconRole};

/// La charge utile d'un glisser : la ligne saisie.
struct RowDrag {
    id: egui::Id,
}

/// Peint une ligne : la poignée (`grip_alt` pour l'accessibilité, `grip_tip` en
/// info-bulle) puis `body`. Rend `true` si une autre ligne, glissée, survole
/// celle-ci : l'appelant mémorise son indice pour [`reorder_move`].
pub(super) fn reorder_row(
    ui: &mut egui::Ui,
    id: egui::Id,
    grip_alt: String,
    grip_tip: &str,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let dragged = egui::DragAndDrop::payload::<RowDrag>(ui.ctx()).is_some_and(|drag| drag.id == id);
    // Seule la poignée se saisit : la ligne garde ses cases cliquables.
    let started = std::cell::Cell::new(false);
    let content = |ui: &mut egui::Ui| {
        let grip = ui.add(
            icon_image(Icon::Grip, IconRole::Neutral)
                .alt_text(grip_alt)
                .sense(egui::Sense::drag()),
        );
        let grip = grab_handle(grip).on_hover_text(grip_tip);
        grip.dnd_set_drag_payload(RowDrag { id });
        started.set(grip.drag_started());
        body(ui);
    };
    if dragged {
        let layer = egui::LayerId::new(egui::Order::Tooltip, id);
        let slot = ui
            .scope_builder(egui::UiBuilder::new().layer_id(layer), content)
            .response
            .rect;
        ui.allocate_space(slot.size());
        let grab: egui::Vec2 = ui.data(|data| data.get_temp(id)).unwrap_or_default();
        if let Some(now) = ui.input(|input| input.pointer.interact_pos()) {
            ui.ctx().transform_layer_shapes(
                layer,
                egui::emath::TSTransform::from_translation(now - grab - slot.min),
            );
        }
        return false;
    }
    let row = ui.scope(content).response;
    if started.get() {
        if let Some(start) = ui.input(|input| input.pointer.press_origin()) {
            ui.data_mut(|data| data.insert_temp(id, start - row.rect.min));
        }
    }
    row.dnd_hover_payload::<RowDrag>().is_some()
}

/// Le déplacement à faire cette image : `ids` sont les identifiants des lignes
/// dans l'ordre affiché, `over` l'indice de la ligne survolée par la ligne
/// glissée. Rend `(from, to)` pour [`move_before`], ou `None`.
pub(super) fn reorder_move(
    ctx: &egui::Context,
    ids: &[egui::Id],
    over: Option<usize>,
) -> Option<(usize, usize)> {
    let over = over?;
    let drag = egui::DragAndDrop::payload::<RowDrag>(ctx)?;
    let from = ids.iter().position(|id| *id == drag.id)?;
    Some((from, if over > from { over + 1 } else { over }))
}

/// Déplace l'élément `from` pour qu'il se retrouve avant l'élément `to` de la
/// liste d'origine (`to == len` : tout à la fin). Pure, pour être testée.
pub(super) fn move_before<T>(list: &mut Vec<T>, from: usize, to: usize) {
    if from >= list.len() || to > list.len() || to == from || to == from + 1 {
        return;
    }
    let item = list.remove(from);
    list.insert(if to > from { to - 1 } else { to }, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_before_place_avant_la_cible() {
        let mut list = vec!['a', 'b', 'c'];
        move_before(&mut list, 2, 0);
        assert_eq!(list, ['c', 'a', 'b']);
        move_before(&mut list, 0, 3);
        assert_eq!(list, ['a', 'b', 'c']);
        move_before(&mut list, 1, 1);
        move_before(&mut list, 1, 2);
        move_before(&mut list, 5, 0);
        assert_eq!(list, ['a', 'b', 'c']);
    }

    #[test]
    fn sans_glisser_rien_ne_bouge() {
        let ctx = egui::Context::default();
        let ids = [egui::Id::new(1), egui::Id::new(2)];
        assert_eq!(reorder_move(&ctx, &ids, Some(0)), None);
        assert_eq!(reorder_move(&ctx, &ids, None), None);
    }
}

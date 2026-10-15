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

/// Où l'on retient le point saisi de la ligne glissée, relatif à la ligne : un
/// seul glisser à la fois, donc une seule clé — l'identifiant d'une ligne peut
/// changer pendant le geste (Object Manager : il suit le rang).
const GRAB: &str = "reorder_row_grab";
/// La couche flottante de la ligne glissée.
const FLOATING: &str = "reorder_row_floating";

/// Ce qu'une image d'une ligne a vu.
pub(super) struct RowOutcome {
    /// La ligne qu'une autre ligne, glissée, survole : son cadre.
    pub hover: Option<egui::Rect>,
    /// La poignée vient d'être saisie.
    pub grabbed: bool,
}

/// Un glisser de ligne est en cours (ou vient d'être relâché, à l'image du
/// relâchement).
pub(super) fn dragging(ctx: &egui::Context) -> bool {
    egui::DragAndDrop::has_payload_of_type::<RowDrag>(ctx)
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
    reorder_row_with(ui, id, dragged, |ui| {
        let grip = ui.add(
            icon_image(Icon::Grip, IconRole::Neutral)
                .alt_text(grip_alt)
                .sense(egui::Sense::drag()),
        );
        let grip = grab_handle(grip).on_hover_text(grip_tip);
        body(ui);
        Some(grip)
    })
    .hover
    .is_some()
}

/// [`reorder_row`] sans poignée imposée : `body` peint la ligne et rend la
/// réponse qui **saisit** la ligne (une poignée, ou toute la zone du libellé :
/// elle doit porter `Sense::drag`), `None` pour une ligne qui ne se glisse pas.
/// L'appelant dit si la ligne est celle qu'on glisse : quand la liste se
/// réordonne pendant le geste et que l'identifiant d'une ligne suit son rang,
/// la charge utile ne la reconnaît plus.
pub(super) fn reorder_row_with(
    ui: &mut egui::Ui,
    id: egui::Id,
    dragged: bool,
    body: impl FnOnce(&mut egui::Ui) -> Option<egui::Response>,
) -> RowOutcome {
    let started = std::cell::Cell::new(false);
    let content = |ui: &mut egui::Ui| {
        if let Some(handle) = body(ui) {
            handle.dnd_set_drag_payload(RowDrag { id });
            started.set(handle.drag_started());
        }
    };
    if dragged {
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new(FLOATING));
        let slot = ui
            .scope_builder(egui::UiBuilder::new().layer_id(layer), content)
            .response
            .rect;
        ui.allocate_space(slot.size());
        let grab: egui::Vec2 = ui
            .data(|data| data.get_temp(egui::Id::new(GRAB)))
            .unwrap_or_default();
        if let Some(now) = ui.input(|input| input.pointer.interact_pos()) {
            ui.ctx().transform_layer_shapes(
                layer,
                egui::emath::TSTransform::from_translation(now - grab - slot.min),
            );
        }
        return RowOutcome {
            hover: None,
            grabbed: false,
        };
    }
    let row = ui.scope(content).response;
    if started.get() {
        if let Some(start) = ui.input(|input| input.pointer.press_origin()) {
            ui.data_mut(|data| data.insert_temp(egui::Id::new(GRAB), start - row.rect.min));
        }
    }
    RowOutcome {
        hover: row
            .dnd_hover_payload::<RowDrag>()
            .is_some()
            .then_some(row.rect),
        grabbed: started.get(),
    }
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

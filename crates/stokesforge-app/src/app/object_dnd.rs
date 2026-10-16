//! Le glisser-déposer des séries, des groupes et des objets libres, côté
//! interface : l'état du geste en cours, partagé par l'Object Manager et la
//! fenêtre des séries (`curve_settings.rs`).
//!
//! La décision — où va l'élément selon la ligne survolée — est pure et vit
//! dans `stokesforge_app_state::object_drag`. Ici : retenir ce qui est saisi, lire où est le
//! pointeur, et, pour l'Object Manager, tenir l'**aperçu**.
//!
//! L'Object Manager lit le spec de l'instantané, qu'une commande seule peut
//! changer ; or le geste réordonne la liste à chaque ligne survolée. Il peint
//! donc un spec d'aperçu (`Live::preview`), cloné du spec réel au premier
//! déplacement, et ne demande **qu'une** action au relâchement (une entrée
//! d'historique par geste), calculée par `object_drag::finish`. La fenêtre des
//! séries travaille sur un brouillon : elle le modifie directement.
//!
//! Un seul glisser à la fois : une clé unique dans la mémoire d'egui. L'état
//! n'est lu que tant qu'une charge utile de glisser existe — Échap ou un
//! relâchement hors fenêtre l'abandonnent sans rien nettoyer.

use std::cell::RefCell;

use eframe::egui;
use stokesforge_core::NodeId;
use stokesforge_plot::PlotSpec;

use super::reorder_list::dragging;
use stokesforge_app_state::object_drag::{self, DragItem, Drop, DropOver};
use stokesforge_app_state::state::Action;

const LIVE: &str = "object_dnd_live";

/// Le glisser en cours.
#[derive(Clone)]
pub(super) struct Live {
    /// Le graphe de l'Object Manager ; `None` pour la fenêtre des séries.
    pub graph: Option<NodeId>,
    /// L'élément saisi, à sa place actuelle.
    pub item: DragItem,
    /// Le spec tel que le geste l'a laissé (Object Manager), absent tant que
    /// rien n'a bougé.
    pub preview: Option<PlotSpec>,
}

/// La poignée vient d'être saisie.
pub(super) fn begin(ctx: &egui::Context, graph: Option<NodeId>, item: DragItem) {
    store(
        ctx,
        Live {
            graph,
            item,
            preview: None,
        },
    );
}

fn store(ctx: &egui::Context, live: Live) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(LIVE), live));
}

/// Le glisser en cours, s'il y en a un.
pub(super) fn live(ctx: &egui::Context) -> Option<Live> {
    if !dragging(ctx) {
        return None;
    }
    ctx.data(|data| data.get_temp(egui::Id::new(LIVE)))
}

/// L'élément glissé change de place (fenêtre des séries).
pub(super) fn set_item(ctx: &egui::Context, item: DragItem) {
    if let Some(live) = live(ctx) {
        store(ctx, Live { item, ..live });
    }
}

/// Le relâchement a lieu à cette image.
fn released(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.pointer.any_released())
}

/// Le pointeur est dans la moitié basse de `rect`.
pub(super) fn pointer_after(ui: &egui::Ui, rect: egui::Rect) -> bool {
    ui.input(|input| input.pointer.interact_pos())
        .is_some_and(|pos| pos.y > rect.center().y)
}

/// La première ligne survolée de l'image : celle qui décide du déplacement,
/// comme dans la liste des formats de copie.
#[derive(Default)]
pub(super) struct Hover(RefCell<Option<DropOver>>);

impl Hover {
    pub fn offer(&self, over: DropOver) {
        let mut slot = self.0.borrow_mut();
        if slot.is_none() {
            *slot = Some(over);
        }
    }

    pub fn take(&self) -> Option<DropOver> {
        self.0.borrow_mut().take()
    }
}

/// Fin d'image de l'Object Manager : applique le survol à l'aperçu, et au
/// relâchement rend l'action unique du geste. `base` est le spec réel.
pub(super) fn end_of_frame(
    ctx: &egui::Context,
    graph: NodeId,
    base: &PlotSpec,
    hover: Option<DropOver>,
    actions: &mut Vec<Action>,
) {
    let Some(mut live) = live(ctx).filter(|live| live.graph == Some(graph)) else {
        return;
    };
    if released(ctx) {
        let drop = live
            .preview
            .as_ref()
            .and_then(|preview| object_drag::finish(base, preview, live.item));
        if let Some(drop) = drop {
            actions.push(action_of(graph, drop));
        }
        ctx.data_mut(|data| data.remove::<Live>(egui::Id::new(LIVE)));
        return;
    }
    let Some(over) = hover else {
        return;
    };
    let mut working = live.preview.clone().unwrap_or_else(|| base.clone());
    if let Some(item) = object_drag::step(&mut working, live.item, over) {
        live.item = item;
        live.preview = Some(working);
        store(ctx, live);
        ctx.request_repaint();
    }
}

/// L'action du modèle pour une destination.
fn action_of(graph: NodeId, drop: Drop) -> Action {
    match drop {
        Drop::Series {
            source,
            group,
            position,
        } => Action::PlaceSeries {
            graph,
            source,
            group,
            position,
        },
        Drop::Group { group, position } => Action::PlaceSeriesGroup {
            graph,
            group,
            position,
        },
        Drop::Object { target, index } => Action::ReorderObject {
            graph,
            target,
            index,
        },
    }
}

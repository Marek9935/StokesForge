//! Déplacer des colonnes dans leur feuille (#319) : la logique pure du sous-menu
//! `Move ▸` — quel ordre vise chaque geste, et quelle suite de `Command::Move`
//! y mène. Aucun état d'interface, aucune commande exécutée ici : `AppState`
//! l'appelle, le menu l'interroge pour griser ses entrées.
//!
//! # Règles (décisions du 2026-10-21)
//!
//! - **Même feuille seulement.** Un ordre est celui des colonnes visibles d'une
//!   feuille.
//! - **Left / Right** : chaque colonne sélectionnée avance d'une place, et
//!   l'écart entre colonnes non contiguës reste (`A [B] C [D] E` → `B A D C E`,
//!   et non un bloc resserré `B D A C E`).
//! - **First / Last** : les colonnes sélectionnées se regroupent en tête ou en
//!   fin, dans leur ordre.
//! - **Tout ou rien** : si le geste ne changerait rien — ou si une seule des
//!   colonnes sélectionnées ne peut pas avancer (`[A] C` ou `[A] [C]`, Left) —
//!   il n'y a pas d'ordre cible. Le menu grise alors l'entrée, avec sa raison.
//!
//! # De l'ordre cible aux commandes
//!
//! `Command::Move { id, new_parent, index }` compte l'index **après retrait** du
//! nœud (`Tree::move_node`). Un enfant `Unknown` peut vivre sous une feuille
//! (invariant 10) sans être une colonne visible : l'ordre se calcule sur les
//! colonnes visibles, puis l'index se traduit dans la liste **complète** des
//! enfants ([`plan`]). Seules les colonnes sélectionnées bougent, de proche en
//! proche à partir du bord vers lequel elles vont : l'ordre est celui d'un
//! avancement sans enjambement (ascendant pour Left et First, descendant pour
//! Right et Last), de sorte que chaque colonne trouve sa place finale sans que
//! les suivantes la défassent.

use stokesforge_core::NodeId;

/// Où va la sélection : les quatre entrées du sous-menu `Move ▸`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnMove {
    /// En tête de feuille, dans l'ordre de la sélection.
    First,
    /// D'une place vers la gauche, chaque colonne.
    Left,
    /// D'une place vers la droite, chaque colonne.
    Right,
    /// En fin de feuille, dans l'ordre de la sélection.
    Last,
}

/// Une étape : `Command::Move { id, new_parent: <la feuille>, index }`, l'index
/// comptant **après retrait** de `id` dans la liste complète des enfants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveStep {
    /// Colonne à déplacer.
    pub id: NodeId,
    /// Position d'accueil, après retrait.
    pub index: usize,
}

/// L'ordre visé par `direction` pour la sélection `selected` sur les colonnes
/// visibles `order`, ou `None` si le geste ne changerait rien (grisé).
/// Les identifiants de `selected` absents de `order` sont ignorés ; sans
/// colonne sélectionnée dans `order`, `None`.
pub fn target_order(
    order: &[NodeId],
    selected: &[NodeId],
    direction: ColumnMove,
) -> Option<Vec<NodeId>> {
    let is_selected = |id: &NodeId| selected.contains(id);
    if !order.iter().any(is_selected) {
        return None;
    }
    let target = match direction {
        ColumnMove::First | ColumnMove::Last => {
            let (chosen, others): (Vec<NodeId>, Vec<NodeId>) =
                order.iter().copied().partition(is_selected);
            if direction == ColumnMove::First {
                [chosen, others].concat()
            } else {
                [others, chosen].concat()
            }
        }
        ColumnMove::Left => {
            if order.first().is_some_and(is_selected) {
                return None;
            }
            let mut target = order.to_vec();
            for position in 1..target.len() {
                if is_selected(&target[position]) {
                    target.swap(position - 1, position);
                }
            }
            target
        }
        ColumnMove::Right => {
            if order.last().is_some_and(is_selected) {
                return None;
            }
            let mut target = order.to_vec();
            for position in (0..target.len().saturating_sub(1)).rev() {
                if is_selected(&target[position]) {
                    target.swap(position, position + 1);
                }
            }
            target
        }
    };
    (target != order).then_some(target)
}

/// La suite de déplacements qui mène `children` (tous les enfants de la feuille,
/// colonnes visibles et autres) à l'ordre `direction` pour `selected` sur les
/// colonnes visibles `order`. `None` quand [`target_order`] n'a rien à faire.
/// Les étapes sont à exécuter dans l'ordre, chacune sur l'état laissé par la
/// précédente ; une colonne déjà à sa place n'a pas d'étape.
pub fn plan(
    children: &[NodeId],
    order: &[NodeId],
    selected: &[NodeId],
    direction: ColumnMove,
) -> Option<Vec<MoveStep>> {
    let target = target_order(order, selected, direction)?;
    let mut moving: Vec<NodeId> = target
        .iter()
        .copied()
        .filter(|id| selected.contains(id))
        .collect();
    if matches!(direction, ColumnMove::Right | ColumnMove::Last) {
        moving.reverse();
    }
    let mut current = children.to_vec();
    let mut steps = Vec::new();
    for id in moving {
        let wanted = target.iter().position(|other| *other == id)?;
        let before = current.clone();
        current.retain(|child| *child != id);
        let index = full_index(&current, &target, wanted);
        current.insert(index, id);
        if current != before {
            steps.push(MoveStep { id, index });
        }
    }
    Some(steps)
}

/// Traduit « la position `wanted` parmi les colonnes visibles » en position dans
/// la liste complète `current` (sans la colonne qui bouge) : juste avant la
/// colonne visible qui occupe `wanted`, ou à la suite de la dernière colonne
/// visible quand `wanted` est la fin.
fn full_index(current: &[NodeId], target: &[NodeId], wanted: usize) -> usize {
    let visible: Vec<usize> = current
        .iter()
        .enumerate()
        .filter(|(_, child)| target.contains(child))
        .map(|(position, _)| position)
        .collect();
    match visible.get(wanted) {
        Some(position) => *position,
        None => visible.last().map_or(current.len(), |last| last + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &str) -> Vec<NodeId> {
        names
            .chars()
            .map(|c| NodeId::from_raw(u64::from(c as u32 - 'A' as u32) + 1))
            .collect()
    }

    fn names(ids: &[NodeId]) -> String {
        ids.iter()
            .map(|id| char::from_u32(id.raw() as u32 - 1 + 'A' as u32).unwrap())
            .collect()
    }

    fn target(order: &str, selected: &str, direction: ColumnMove) -> Option<String> {
        target_order(&ids(order), &ids(selected), direction).map(|t| names(&t))
    }

    /// `A [B] C [D] E`, Left : chaque colonne recule d'une place, l'écart
    /// reste. Témoin : resserrer le bloc donnerait `B D A C E`.
    #[test]
    fn left_keeps_the_gap_between_non_contiguous_columns() {
        assert_eq!(target("ABCDE", "BD", ColumnMove::Left).unwrap(), "BADCE");
        assert_eq!(target("ABCDE", "BD", ColumnMove::Right).unwrap(), "ACBED");
    }

    #[test]
    fn first_and_last_gather_the_selection_in_its_order() {
        assert_eq!(target("ABCDE", "BD", ColumnMove::First).unwrap(), "BDACE");
        assert_eq!(target("ABCDE", "BD", ColumnMove::Last).unwrap(), "ACEBD");
        // L'ordre est celui de la feuille, pas celui où la sélection est donnée.
        assert_eq!(target("ABCDE", "DB", ColumnMove::First).unwrap(), "BDACE");
    }

    /// Tout ou rien : une colonne qui ne peut pas avancer bloque le geste.
    /// Témoin : ne bouger que C.
    #[test]
    fn a_column_that_cannot_move_greys_the_whole_gesture() {
        assert_eq!(target("ABC", "AC", ColumnMove::Left), None);
        assert_eq!(target("AC", "A", ColumnMove::Left), None);
        assert_eq!(target("ABC", "AB", ColumnMove::Left), None);
        assert_eq!(target("ABC", "BC", ColumnMove::Right), None);
        assert_eq!(target("ABC", "A", ColumnMove::Right).unwrap(), "BAC");
    }

    #[test]
    fn a_gesture_that_changes_nothing_has_no_target() {
        assert_eq!(target("ABCDE", "AB", ColumnMove::First), None);
        assert_eq!(target("ABCDE", "DE", ColumnMove::Last), None);
        assert_eq!(target("ABCDE", "ABCDE", ColumnMove::First), None);
        assert_eq!(target("ABCDE", "", ColumnMove::Left), None);
        // Une colonne d'une autre feuille n'est pas dans l'ordre : ignorée.
        assert_eq!(target("ABC", "Z", ColumnMove::First), None);
    }

    /// Rejoue une suite d'étapes sur une liste de nœuds, en comptant l'index
    /// après retrait comme `Tree::move_node`.
    fn replay(children: &[NodeId], steps: &[MoveStep]) -> Vec<NodeId> {
        let mut list = children.to_vec();
        for step in steps {
            list.retain(|id| *id != step.id);
            assert!(step.index <= list.len(), "index hors des bornes");
            list.insert(step.index, step.id);
        }
        list
    }

    /// Les étapes mènent exactement à l'ordre cible, pour toute sélection et
    /// les quatre sens, sur une feuille de six colonnes. Témoin : compter
    /// l'index avant retrait (hors borne ou mauvaise place).
    #[test]
    fn the_steps_reach_the_target_order_for_every_selection() {
        let order = ids("ABCDEF");
        for mask in 1u32..(1 << 6) {
            let selected: Vec<NodeId> = (0..6)
                .filter(|bit| mask & (1 << bit) != 0)
                .map(|bit| order[bit])
                .collect();
            for direction in [
                ColumnMove::First,
                ColumnMove::Left,
                ColumnMove::Right,
                ColumnMove::Last,
            ] {
                let wanted = target_order(&order, &selected, direction);
                let steps = plan(&order, &order, &selected, direction);
                assert_eq!(wanted.is_some(), steps.is_some());
                if let (Some(wanted), Some(steps)) = (wanted, steps) {
                    assert_eq!(
                        replay(&order, &steps),
                        wanted,
                        "{mask:b} {direction:?} : {steps:?}"
                    );
                    // Seules les colonnes sélectionnées bougent.
                    assert!(steps.iter().all(|step| selected.contains(&step.id)));
                    assert!(!steps.is_empty());
                }
            }
        }
    }

    /// Un enfant `Unknown` sous la feuille n'est pas une colonne visible : il
    /// garde sa place relative, et l'index se compte dans la liste complète.
    #[test]
    fn an_unknown_child_keeps_its_place_and_indexes_are_full_list_indexes() {
        // Enfants : A, ?, B, C, ? ; colonnes visibles : A B C.
        let unknown_1 = NodeId::from_raw(100);
        let unknown_2 = NodeId::from_raw(101);
        let [a, b, c] = [ids("A")[0], ids("B")[0], ids("C")[0]];
        let children = vec![a, unknown_1, b, c, unknown_2];
        let order = vec![a, b, c];
        for (selected, direction) in [
            (vec![b], ColumnMove::Left),
            (vec![b], ColumnMove::First),
            (vec![b, c], ColumnMove::First),
            (vec![a], ColumnMove::Right),
            (vec![a], ColumnMove::Last),
            (vec![a, b], ColumnMove::Last),
        ] {
            let wanted = target_order(&order, &selected, direction).unwrap();
            let steps = plan(&children, &order, &selected, direction).unwrap();
            let after = replay(&children, &steps);
            let visible: Vec<NodeId> = after
                .iter()
                .copied()
                .filter(|id| order.contains(id))
                .collect();
            assert_eq!(visible, wanted, "{selected:?} {direction:?}");
            let unknowns: Vec<NodeId> = after
                .iter()
                .copied()
                .filter(|id| !order.contains(id))
                .collect();
            assert_eq!(unknowns, vec![unknown_1, unknown_2]);
            assert_eq!(after.len(), children.len());
        }
    }

    /// Une colonne seule qui bouge : une seule étape, sur cette colonne.
    #[test]
    fn one_selected_column_makes_one_step() {
        let order = ids("ABC");
        let steps = plan(&order, &order, &ids("B"), ColumnMove::Right).unwrap();
        assert_eq!(
            steps,
            vec![MoveStep {
                id: ids("B")[0],
                index: 2
            }]
        );
        let steps = plan(&order, &order, &ids("C"), ColumnMove::First).unwrap();
        assert_eq!(
            steps,
            vec![MoveStep {
                id: ids("C")[0],
                index: 0
            }]
        );
    }
}

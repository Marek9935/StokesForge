//! Sélection de cellules dans la grille d'une feuille.
//!
//! État UI **pur** : jamais envoyé à l'acteur, jamais dans l'historique
//! (voir la règle 2 en tête de [`crate::state`]). [`crate::state::AppState`]
//! en porte une par feuille affichée, dans une table qu'elle purge comme le
//! reste au fil de [`crate::state::AppState::revalidate_selection`].
//!
//! # Vocabulaire de l'en-tête
//!
//! L'en-tête empilée d'une colonne a 5 rangées : la **rangée des lettres**
//! (A, B, C…, celle qui sélectionne la colonne entière) puis 4 **champs
//! d'en-tête** — Nom long, Unité, Commentaire, Formule
//! ([`crate::state::HeaderField`]) — chacun une rangée de sa propre valeur,
//! répétée par colonne.

use plotyx_core::{NodeId, Tree};

use crate::state::{visible_columns_in, HeaderField};

/// Sélection courante dans la grille d'une feuille : un rectangle de
/// cellules de données, une plage de colonnes entières (glisser/Maj-clic sur
/// la rangée des lettres), ou une plage de champs d'en-tête (glisser/
/// Maj-clic sur une ou plusieurs des 4 rangées de champs, sur une ou
/// plusieurs colonnes) — même comportement clic-sélectionne/glisser-étende/
/// double-clic-édite que les cellules de données, voir
/// [`crate::app::header_field_widget`] et [`crate::app::column_header_widget`].
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    kind: SelectionKind,
}

#[derive(Debug, Clone, PartialEq)]
enum SelectionKind {
    /// Rectangle de cellules de données entre `anchor` (posée au premier
    /// clic, jamais déplacée par la suite) et `cursor` (la dernière cellule
    /// visée — un simple clic pose `cursor == anchor`, un glisser ou un
    /// Shift-clic ne déplace que `cursor`).
    Cells {
        anchor: (NodeId, usize),
        cursor: (NodeId, usize),
    },
    /// Plage de colonnes entières, entre `anchor` et `cursor` (mêmes règles
    /// que `Cells::anchor`/`cursor`, mais en colonnes plutôt qu'en cellules)
    /// — glisser ou Maj-clic sur la rangée des lettres.
    Columns { anchor: NodeId, cursor: NodeId },
    /// Rectangle de champs d'en-tête, entre `anchor` et `cursor` (colonne +
    /// champ). N'intervient dans aucune des méthodes pensées pour les
    /// données de colonne ([`Selection::columns`]/[`Selection::
    /// clear_ranges`]/[`Selection::delete_at_count`], qui y voient toutes
    /// une sélection vide) : Ctrl+C/Suppr sur des champs d'en-tête sont
    /// câblés séparément par l'appelant via [`Selection::
    /// selected_header_field`] (réduit à un seul champ, sinon `None`).
    HeaderFields {
        anchor: (NodeId, HeaderField),
        cursor: (NodeId, HeaderField),
    },
}

/// Point de départ unifié de la navigation clavier (flèches) — la « dernière
/// cellule visée » de la sélection courante, que ce soit une cellule de
/// données ou un champ d'en-tête ; `None` pour une plage de colonnes (pas de
/// « champ suivant » qui ait un sens évident, comme documenté par
/// [`Selection::cursor`] pour l'ancien `Column`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridCursor {
    /// Cellule de données `(colonne, ligne)`.
    Data(NodeId, usize),
    /// Champ d'en-tête `(colonne, champ)`.
    Header(NodeId, HeaderField),
}

/// Colonnes couvertes par `[a, b]` dans l'ordre de `sheet` — partagé entre
/// [`SelectionKind::Cells`], [`SelectionKind::Columns`] et
/// [`SelectionKind::HeaderFields`], qui résolvent tous une plage de colonnes
/// de la même façon, par position dans [`visible_columns_in`] plutôt que par
/// comparaison directe d'identifiants (une colonne peut être glissée dans
/// l'ordre inverse, `a` après `b`).
fn order_range(tree: &Tree, sheet: NodeId, a: NodeId, b: NodeId) -> Vec<NodeId> {
    let order: Vec<NodeId> = visible_columns_in(tree, sheet)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    let start = order.iter().position(|&id| id == a);
    let end = order.iter().position(|&id| id == b);
    match (start, end) {
        (Some(x), Some(y)) => {
            let (lo, hi) = if x <= y { (x, y) } else { (y, x) };
            order[lo..=hi].to_vec()
        }
        _ => Vec::new(),
    }
}

impl Selection {
    /// Sélection d'une seule cellule — l'ancre d'un futur rectangle.
    pub fn cell(column: NodeId, row: usize) -> Self {
        Self {
            kind: SelectionKind::Cells {
                anchor: (column, row),
                cursor: (column, row),
            },
        }
    }

    /// Sélection d'une seule colonne entière — l'ancre d'une future plage.
    pub fn column(column: NodeId) -> Self {
        Self {
            kind: SelectionKind::Columns {
                anchor: column,
                cursor: column,
            },
        }
    }

    /// Sélection d'un seul champ d'en-tête — l'ancre d'une future plage.
    pub fn header_field(column: NodeId, field: HeaderField) -> Self {
        Self {
            kind: SelectionKind::HeaderFields {
                anchor: (column, field),
                cursor: (column, field),
            },
        }
    }

    /// `Some((column, field))` si la sélection est réduite à un seul champ
    /// d'en-tête (ancre et curseur confondus) — jamais pour une plage de
    /// plusieurs champs/colonnes, ni une plage de colonnes, ni un rectangle
    /// de cellules. Sert à démarrer l'édition par simple frappe au clavier,
    /// même règle que [`Selection::single_cell`] pour une cellule de
    /// données : ce geste n'a de sens que pour un champ unique.
    pub fn selected_header_field(&self) -> Option<(NodeId, HeaderField)> {
        match self.kind {
            SelectionKind::HeaderFields { anchor, cursor } if anchor == cursor => Some(anchor),
            _ => None,
        }
    }

    /// Vrai si `(column, field)` fait partie de la plage de champs d'en-tête
    /// sélectionnée.
    pub fn is_header_field_selected(
        &self,
        tree: &Tree,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) -> bool {
        match &self.kind {
            SelectionKind::HeaderFields { anchor, cursor } => {
                let columns = order_range(tree, sheet, anchor.0, cursor.0);
                if !columns.contains(&column) {
                    return false;
                }
                let (lo, hi) = {
                    let (a, b) = (anchor.1.index(), cursor.1.index());
                    if a <= b {
                        (a, b)
                    } else {
                        (b, a)
                    }
                };
                let index = field.index();
                index >= lo && index <= hi
            }
            _ => false,
        }
    }

    /// Étend un rectangle de cellules de données jusqu'à `(column, row)`, en
    /// gardant l'ancre posée par [`Selection::cell`] — sans effet si la
    /// sélection courante n'est pas déjà un rectangle de cellules (il faut
    /// re-cliquer une cellule pour repartir en rectangle).
    pub fn extend_to(&mut self, column: NodeId, row: usize) {
        if let SelectionKind::Cells { cursor, .. } = &mut self.kind {
            *cursor = (column, row);
        }
    }

    /// Étend une plage de colonnes jusqu'à `column`, en gardant l'ancre
    /// posée par [`Selection::column`] — glisser ou Maj-clic sur la rangée
    /// des lettres. Sans effet si la sélection courante n'est pas déjà une
    /// plage de colonnes.
    pub fn extend_columns_to(&mut self, column: NodeId) {
        if let SelectionKind::Columns { cursor, .. } = &mut self.kind {
            *cursor = column;
        }
    }

    /// Étend une plage de champs d'en-tête jusqu'à `(column, field)`, en
    /// gardant l'ancre posée par [`Selection::header_field`] — glisser ou
    /// Maj-clic sur une des 4 rangées de champs. Sans effet si la sélection
    /// courante n'est pas déjà une plage de champs d'en-tête.
    pub fn extend_header_field_to(&mut self, column: NodeId, field: HeaderField) {
        if let SelectionKind::HeaderFields { cursor, .. } = &mut self.kind {
            *cursor = (column, field);
        }
    }

    /// `Some((column, row))` si la sélection est réduite à une seule
    /// cellule (un rectangle de cellules dont l'ancre et le curseur
    /// coïncident — un simple clic, jamais étendu depuis), `None` pour un
    /// rectangle plus grand ou toute autre sorte de sélection.
    ///
    /// Sert à démarrer l'édition par simple frappe au clavier (taper sur une
    /// cellule sélectionnée remplace tout son contenu, comme dans un
    /// tableur) : ce geste n'a de sens que pour une cellule unique, jamais
    /// pour une plage — taper sur une plage sélectionnée ne doit pas écraser
    /// silencieusement plusieurs cellules à la fois.
    pub fn single_cell(&self) -> Option<(NodeId, usize)> {
        match &self.kind {
            SelectionKind::Cells { anchor, cursor } if anchor == cursor => Some(*anchor),
            _ => None,
        }
    }

    /// Cellule de données courante d'un rectangle de cellules — la dernière
    /// visée (`cursor`, pas `anchor`). `None` pour toute autre sorte de
    /// sélection. Réservé à [`crate::state::AppState::move_selection_tab`],
    /// qui ne fait circuler Tab qu'au sein des données ; voir
    /// [`Selection::grid_cursor`] pour la navigation aux flèches, qui
    /// traverse aussi l'en-tête.
    pub fn cursor(&self) -> Option<(NodeId, usize)> {
        match &self.kind {
            SelectionKind::Cells { cursor, .. } => Some(*cursor),
            SelectionKind::Columns { .. } | SelectionKind::HeaderFields { .. } => None,
        }
    }

    /// Point de départ unifié de la navigation aux flèches — la dernière
    /// case visée, cellule de données ou champ d'en-tête. `None` pour une
    /// plage de colonnes : la rangée des lettres n'a pas de « case
    /// suivante » qui ait un sens évident, il faut re-cliquer une cellule ou
    /// un champ pour reprendre la navigation.
    pub fn grid_cursor(&self) -> Option<GridCursor> {
        match &self.kind {
            SelectionKind::Cells { cursor, .. } => Some(GridCursor::Data(cursor.0, cursor.1)),
            SelectionKind::HeaderFields { cursor, .. } => {
                Some(GridCursor::Header(cursor.0, cursor.1))
            }
            SelectionKind::Columns { .. } => None,
        }
    }

    /// Colonnes couvertes, dans l'ordre de la feuille. Vide si l'ancre ou le
    /// curseur ne désignent plus une colonne de `sheet` (colonne supprimée
    /// entre-temps) — vide aussi pour une plage de champs d'en-tête, qui ne
    /// désigne aucune donnée de colonne (voir [`SelectionKind::
    /// HeaderFields`]).
    pub fn columns(&self, tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
        match &self.kind {
            SelectionKind::Columns { anchor, cursor } => order_range(tree, sheet, *anchor, *cursor),
            SelectionKind::Cells { anchor, cursor } => order_range(tree, sheet, anchor.0, cursor.0),
            SelectionKind::HeaderFields { .. } => Vec::new(),
        }
    }

    /// Plage de lignes inclusive `(min, max)` d'un rectangle de cellules ;
    /// `None` pour une plage de colonnes entières — sa plage dépend de la
    /// longueur courante de chaque colonne, propre à l'appelant (voir
    /// [`Selection::clear_ranges`]/[`Selection::delete_at_count`]).
    pub fn row_range(&self) -> Option<(usize, usize)> {
        match &self.kind {
            SelectionKind::Cells { anchor, cursor } => {
                Some((anchor.1.min(cursor.1), anchor.1.max(cursor.1)))
            }
            SelectionKind::Columns { .. } | SelectionKind::HeaderFields { .. } => None,
        }
    }

    /// Vrai si `(column, row)` fait partie de la sélection.
    pub fn contains(&self, tree: &Tree, sheet: NodeId, column: NodeId, row: usize) -> bool {
        match &self.kind {
            SelectionKind::Columns { .. } => self.columns(tree, sheet).contains(&column),
            SelectionKind::HeaderFields { .. } => false,
            SelectionKind::Cells { .. } => match self.row_range() {
                Some((lo, hi)) if row >= lo && row <= hi => {
                    self.columns(tree, sheet).contains(&column)
                }
                _ => false,
            },
        }
    }

    /// Vrai si `column` fait partie de la plage de colonnes sélectionnée
    /// (clic, glisser ou Maj-clic sur la rangée des lettres) — distinct de
    /// [`Selection::contains`], qui teste une cellule précise.
    pub fn is_column_selected(&self, tree: &Tree, sheet: NodeId, column: NodeId) -> bool {
        match &self.kind {
            SelectionKind::Columns { .. } => self.columns(tree, sheet).contains(&column),
            _ => false,
        }
    }

    /// Plages `(colonne, début, nombre)` à vider (Retour arrière/Ctrl+X),
    /// une par colonne touchée, bornées à sa longueur réelle — vider
    /// au-delà de la fin d'une colonne courte ne doit jamais l'allonger.
    pub fn clear_ranges(&self, tree: &Tree, sheet: NodeId) -> Vec<(NodeId, usize, usize)> {
        self.columns(tree, sheet)
            .into_iter()
            .filter_map(|column| {
                let len = tree
                    .node(column)
                    .and_then(plotyx_core::Node::as_column)?
                    .len();
                let (start, count) = match self.row_range() {
                    Some((lo, hi)) => (lo, hi.saturating_sub(lo) + 1),
                    None => (0, len),
                };
                if start >= len {
                    return None;
                }
                let count = count.min(len - start);
                (count > 0).then_some((column, start, count))
            })
            .collect()
    }

    /// `(colonnes, début, nombre)` pour Suppr (vide et remonte) :
    /// `Command::DeleteCells` applique `at`/`count` indépendamment par
    /// colonne (une colonne plus courte que `at + count` n'est retirée que
    /// jusqu'à sa propre fin). Pour un rectangle de cellules, `count` est la
    /// taille de la plage sélectionnée (`hi - lo + 1`) — retirer *exactement*
    /// les lignes sélectionnées, ni plus ni moins, sinon Suppr sur une seule
    /// ligne tronquerait tout ce qui suit au lieu de ne faire remonter que
    /// cette ligne. Pour une plage de colonnes entières (pas de plage de
    /// lignes), `count` reste calculé sur la colonne sélectionnée la plus
    /// longue, pour qu'elle soit intégralement compactée même si une autre
    /// colonne de la plage est plus courte.
    pub fn delete_at_count(&self, tree: &Tree, sheet: NodeId) -> (Vec<NodeId>, usize, usize) {
        let columns = self.columns(tree, sheet);
        let (start, count) = match self.row_range() {
            Some((lo, hi)) => (lo, hi.saturating_sub(lo) + 1),
            None => {
                let count = columns
                    .iter()
                    .filter_map(|&column| tree.node(column).and_then(plotyx_core::Node::as_column))
                    .map(plotyx_core::Column::len)
                    .max()
                    .unwrap_or(0);
                (0, count)
            }
        };
        (columns, start, count)
    }

    /// Cellule d'ancrage — coin haut-gauche logique du rectangle, ou la
    /// première colonne de la plage à la ligne 0 pour une plage de colonnes
    /// ou de champs d'en-tête. Sert de point de départ au collage (Ctrl+V
    /// colle à partir de l'ancre, indépendamment de la taille de la
    /// sélection active, comme dans tout tableur de référence).
    pub fn anchor(&self) -> (NodeId, usize) {
        match &self.kind {
            SelectionKind::Cells { anchor, .. } => *anchor,
            SelectionKind::Columns { anchor, .. } => (*anchor, 0),
            SelectionKind::HeaderFields { anchor, .. } => (anchor.0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`Selection::single_cell`] — verrou du bug 1 (taper sur une cellule
    /// sélectionnée démarre l'édition) : un simple clic reste réduit à une
    /// seule cellule, mais l'étendre — même jusqu'à elle-même via un
    /// Shift-clic ou un glisser sans mouvement réel — ne doit plus jamais
    /// être traité comme une cellule unique une fois `extend_to` appelé,
    /// puisque le geste qui a produit la sélection n'est alors plus « un
    /// simple clic sur une cellule ».
    #[test]
    fn single_cell_is_some_only_for_a_plain_click() {
        let column = NodeId::from_raw(1);
        let other_column = NodeId::from_raw(2);

        let selection = Selection::cell(column, 3);
        assert_eq!(selection.single_cell(), Some((column, 3)));

        let mut extended = Selection::cell(column, 3);
        extended.extend_to(column, 4);
        assert_eq!(
            extended.single_cell(),
            None,
            "un rectangle de plusieurs lignes n'est pas une cellule unique"
        );

        let mut extended_columns = Selection::cell(column, 3);
        extended_columns.extend_to(other_column, 3);
        assert_eq!(
            extended_columns.single_cell(),
            None,
            "un rectangle de plusieurs colonnes n'est pas une cellule unique"
        );

        assert_eq!(
            Selection::column(column).single_cell(),
            None,
            "une plage de colonnes n'est jamais une cellule unique"
        );
    }
}

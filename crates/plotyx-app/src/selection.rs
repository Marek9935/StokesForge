//! Sélection de cellules dans la grille d'une feuille.
//!
//! État UI **pur** : jamais envoyé à l'acteur, jamais dans l'historique
//! (voir la règle 2 en tête de [`crate::state`]). [`crate::state::AppState`]
//! en porte une par feuille affichée, dans une table qu'elle purge comme le
//! reste au fil de [`crate::state::AppState::revalidate_selection`].

use plotyx_core::{NodeId, Tree};

use crate::state::{visible_columns_in, HeaderField};

/// Sélection courante dans la grille d'une feuille : un rectangle de
/// cellules, une colonne entière (clic sur sa lettre d'en-tête), ou un champ
/// d'en-tête (clic sur une des quatre rangées nom long/unité/commentaire/
/// formule) — même comportement clic-sélectionne/double-clic-édite que les
/// cellules de données, voir [`crate::app::header_field_widget`].
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    kind: SelectionKind,
}

#[derive(Debug, Clone, PartialEq)]
enum SelectionKind {
    /// Rectangle de cellules entre `anchor` (posée au premier clic, jamais
    /// déplacée par la suite) et `cursor` (la dernière cellule visée — un
    /// simple clic pose `cursor == anchor`, un glisser ou un Shift-clic ne
    /// déplace que `cursor`).
    Cells {
        anchor: (NodeId, usize),
        cursor: (NodeId, usize),
    },
    /// Colonne entière.
    Column(NodeId),
    /// Un champ d'en-tête précis d'une colonne (nom long/unité/commentaire/
    /// formule) — jamais une plage, une seule cellule d'en-tête à la fois.
    /// N'intervient dans aucune des méthodes pensées pour les données de
    /// colonne ([`Selection::columns`]/[`Selection::clear_ranges`]/
    /// [`Selection::delete_at_count`], qui y voient toutes une sélection
    /// vide) : Ctrl+C/Suppr sur un champ d'en-tête sont câblés séparément par
    /// l'appelant via [`Selection::header_field`].
    HeaderField { column: NodeId, field: HeaderField },
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

    /// Sélection d'une colonne entière.
    pub fn column(column: NodeId) -> Self {
        Self {
            kind: SelectionKind::Column(column),
        }
    }

    /// Sélection d'un champ d'en-tête précis.
    pub fn header_field(column: NodeId, field: HeaderField) -> Self {
        Self {
            kind: SelectionKind::HeaderField { column, field },
        }
    }

    /// `Some((column, field))` si la sélection est un champ d'en-tête —
    /// jamais une plage de cellules ni une colonne entière.
    pub fn selected_header_field(&self) -> Option<(NodeId, HeaderField)> {
        match self.kind {
            SelectionKind::HeaderField { column, field } => Some((column, field)),
            _ => None,
        }
    }

    /// Vrai si `(column, field)` est le champ d'en-tête actuellement
    /// sélectionné.
    pub fn is_header_field_selected(&self, column: NodeId, field: HeaderField) -> bool {
        matches!(
            self.kind,
            SelectionKind::HeaderField { column: c, field: f } if c == column && f == field
        )
    }

    /// Étend un rectangle de cellules jusqu'à `(column, row)`, en gardant
    /// l'ancre posée par [`Selection::cell`] — sans effet sur une colonne
    /// entière sélectionnée (il faut re-cliquer une cellule pour repartir en
    /// rectangle).
    pub fn extend_to(&mut self, column: NodeId, row: usize) {
        if let SelectionKind::Cells { cursor, .. } = &mut self.kind {
            *cursor = (column, row);
        }
    }

    /// `Some((column, row))` si la sélection est réduite à une seule
    /// cellule (un rectangle de cellules dont l'ancre et le curseur
    /// coïncident — un simple clic, jamais étendu depuis), `None` pour un
    /// rectangle plus grand ou une colonne entière.
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

    /// Cellule courante d'un rectangle de cellules — la dernière visée
    /// (`cursor`, pas `anchor`) : point de départ de la navigation clavier
    /// (flèches/Tab/Entrée). `None` pour une colonne entière ou un champ
    /// d'en-tête : il faut re-cliquer une cellule pour reprendre la
    /// navigation, la lettre de colonne ou un champ d'en-tête n'ayant pas de
    /// « case suivante » qui ait un sens évident.
    pub fn cursor(&self) -> Option<(NodeId, usize)> {
        match &self.kind {
            SelectionKind::Cells { cursor, .. } => Some(*cursor),
            SelectionKind::Column(_) | SelectionKind::HeaderField { .. } => None,
        }
    }

    /// Colonnes couvertes, dans l'ordre de la feuille. Vide si l'ancre ou le
    /// curseur ne désignent plus une colonne de `sheet` (colonne supprimée
    /// entre-temps) — vide aussi pour un champ d'en-tête, qui ne désigne
    /// aucune donnée de colonne (voir [`SelectionKind::HeaderField`]).
    pub fn columns(&self, tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
        match &self.kind {
            SelectionKind::Column(column) => {
                if tree
                    .node(*column)
                    .and_then(plotyx_core::Node::as_column)
                    .is_some()
                {
                    vec![*column]
                } else {
                    Vec::new()
                }
            }
            SelectionKind::Cells { anchor, cursor } => {
                let order: Vec<NodeId> = visible_columns_in(tree, sheet)
                    .into_iter()
                    .map(|(id, _, _)| id)
                    .collect();
                let start = order.iter().position(|&id| id == anchor.0);
                let end = order.iter().position(|&id| id == cursor.0);
                match (start, end) {
                    (Some(a), Some(b)) => {
                        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                        order[lo..=hi].to_vec()
                    }
                    _ => Vec::new(),
                }
            }
            SelectionKind::HeaderField { .. } => Vec::new(),
        }
    }

    /// Plage de lignes inclusive `(min, max)` d'un rectangle de cellules ;
    /// `None` pour une colonne entière — sa plage dépend de la longueur
    /// courante de chaque colonne, propre à l'appelant (voir
    /// [`Selection::clear_ranges`]/[`Selection::delete_at_count`]).
    pub fn row_range(&self) -> Option<(usize, usize)> {
        match &self.kind {
            SelectionKind::Cells { anchor, cursor } => {
                Some((anchor.1.min(cursor.1), anchor.1.max(cursor.1)))
            }
            SelectionKind::Column(_) | SelectionKind::HeaderField { .. } => None,
        }
    }

    /// Vrai si `(column, row)` fait partie de la sélection.
    pub fn contains(&self, tree: &Tree, sheet: NodeId, column: NodeId, row: usize) -> bool {
        match &self.kind {
            SelectionKind::Column(selected) => *selected == column,
            SelectionKind::HeaderField { .. } => false,
            SelectionKind::Cells { .. } => match self.row_range() {
                Some((lo, hi)) if row >= lo && row <= hi => {
                    self.columns(tree, sheet).contains(&column)
                }
                _ => false,
            },
        }
    }

    /// Vrai si `column` est sélectionnée dans son entier (clic sur sa lettre
    /// d'en-tête) — distinct de [`Selection::contains`], qui teste une
    /// cellule précise.
    pub fn is_column_selected(&self, column: NodeId) -> bool {
        matches!(&self.kind, SelectionKind::Column(selected) if *selected == column)
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
    /// cette ligne. Pour une colonne entière sélectionnée (pas de plage de
    /// lignes), `count` reste calculé sur la colonne sélectionnée la plus
    /// longue, pour qu'elle soit intégralement compactée même si une autre
    /// colonne du rectangle est plus courte.
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
    /// colonne elle-même à la ligne 0 pour une colonne entière. Sert de
    /// point de départ au collage (Ctrl+V colle à partir de l'ancre,
    /// indépendamment de la taille de la sélection active, comme dans tout
    /// tableur de référence).
    pub fn anchor(&self) -> (NodeId, usize) {
        match &self.kind {
            SelectionKind::Cells { anchor, .. } => *anchor,
            SelectionKind::Column(column) | SelectionKind::HeaderField { column, .. } => {
                (*column, 0)
            }
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
            "une colonne entière n'est jamais une cellule unique"
        );
    }
}

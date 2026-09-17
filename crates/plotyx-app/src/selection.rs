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
//!
//! # Un seul axe unifié pour les champs et les données
//!
//! Les 4 champs d'en-tête et les lignes de données d'une même colonne
//! forment un seul axe ordonné (voir [`GridCursor::rank`]) : Nom long <
//! Unité < Commentaire < Formule < ligne 0 < ligne 1 < … Un rectangle de
//! sélection ([`SelectionKind::Rows`]) est donc soit entièrement dans
//! l'en-tête, soit entièrement dans les données, soit — depuis ce fichier —
//! **à cheval sur les deux**, produit d'un glisser ou d'un Maj-clic qui
//! commence sur un champ et continue dans la grille (ou l'inverse). La
//! plage de colonnes entières ([`SelectionKind::Columns`], glisser/Maj-clic
//! sur la rangée des lettres) reste un geste distinct, jamais mélangé au
//! reste : un glisser qui la croise repose toujours une sélection simple
//! (voir [`crate::state::AppState::extend_selection`]/[`crate::state::
//! AppState::extend_header_field_selection`]/[`crate::state::AppState::
//! extend_column_selection`]).

use plotyx_core::{NodeId, Tree};

use crate::state::{visible_columns_in, HeaderField};

/// Sélection courante dans la grille d'une feuille : un rectangle mixte
/// champs d'en-tête/cellules de données ([`SelectionKind::Rows`]) ou une
/// plage de colonnes entières ([`SelectionKind::Columns`], glisser/Maj-clic
/// sur la rangée des lettres) — même comportement clic-sélectionne/
/// glisser-étende/double-clic-édite pour les deux, voir
/// [`crate::app::header_field_widget`], [`crate::app::cell_widget`] et
/// [`crate::app::column_header_widget`].
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    kind: SelectionKind,
}

#[derive(Debug, Clone, PartialEq)]
enum SelectionKind {
    /// Rectangle unifié champs d'en-tête/cellules de données, entre
    /// `anchor` (posée au premier clic, jamais déplacée par la suite) et
    /// `cursor` (la dernière case visée — un simple clic pose
    /// `cursor == anchor`, un glisser ou un Maj-clic ne déplace que
    /// `cursor`). Les deux extrémités peuvent être de sortes différentes
    /// (l'une un champ, l'autre une cellule) : c'est exactement ce qui rend
    /// le rectangle mixte possible, voir la doc de tête du module.
    Rows {
        anchor: GridCursor,
        cursor: GridCursor,
    },
    /// Plage de colonnes entières, entre `anchor` et `cursor` (mêmes règles
    /// que `Rows::anchor`/`cursor`, mais en colonnes plutôt qu'en cases) —
    /// glisser ou Maj-clic sur la rangée des lettres.
    Columns { anchor: NodeId, cursor: NodeId },
}

/// Point de départ unifié de la navigation clavier (flèches, Tab) et de la
/// sélection à la souris — une case de la grille, cellule de données ou
/// champ d'en-tête, sur l'axe unique décrit en tête du module. `None` pour
/// une plage de colonnes ([`Selection::grid_cursor`]) : la rangée des
/// lettres n'a pas de « case suivante » qui ait un sens évident, il faut
/// re-cliquer une cellule ou un champ pour reprendre la navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridCursor {
    /// Cellule de données `(colonne, ligne)`.
    Data(NodeId, usize),
    /// Champ d'en-tête `(colonne, champ)`.
    Header(NodeId, HeaderField),
}

impl GridCursor {
    /// Colonne visée, quelle que soit la sorte de case.
    fn column(self) -> NodeId {
        match self {
            GridCursor::Data(column, _) | GridCursor::Header(column, _) => column,
        }
    }

    /// Rang sur l'axe unique en-tête puis données (voir la doc de tête du
    /// module) : les 4 champs d'en-tête occupent `-4..=-1` (via
    /// [`HeaderField::index`]), les lignes de données `0..` — comparable
    /// entre deux [`GridCursor`] de la même colonne ou de colonnes
    /// différentes, la colonne se résolvant séparément par position (voir
    /// [`order_range`]).
    fn rank(self) -> isize {
        match self {
            GridCursor::Data(_, row) => row as isize,
            GridCursor::Header(_, field) => field.index() as isize - 4,
        }
    }
}

/// Plage `[lo, hi]` inclusive de rangs (voir [`GridCursor::rank`]) couverte
/// par `a` et `b`, quel que soit leur ordre (un glisser peut viser vers le
/// haut comme vers le bas).
fn rank_range(a: GridCursor, b: GridCursor) -> (isize, isize) {
    let (ra, rb) = (a.rank(), b.rank());
    if ra <= rb {
        (ra, rb)
    } else {
        (rb, ra)
    }
}

/// Colonnes couvertes par `[a, b]` dans l'ordre de `sheet` — partagé entre
/// [`SelectionKind::Rows`] et [`SelectionKind::Columns`], qui résolvent tous
/// deux une plage de colonnes de la même façon, par position dans
/// [`visible_columns_in`] plutôt que par comparaison directe d'identifiants
/// (une colonne peut être glissée dans l'ordre inverse, `a` après `b`).
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
        let cursor = GridCursor::Data(column, row);
        Self {
            kind: SelectionKind::Rows {
                anchor: cursor,
                cursor,
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
        let cursor = GridCursor::Header(column, field);
        Self {
            kind: SelectionKind::Rows {
                anchor: cursor,
                cursor,
            },
        }
    }

    /// `Some((column, field))` si la sélection est réduite à un seul champ
    /// d'en-tête (ancre et curseur confondus) — jamais pour une plage de
    /// plusieurs champs/colonnes/lignes de données, ni une plage de
    /// colonnes. Sert à démarrer l'édition par simple frappe au clavier,
    /// même règle que [`Selection::single_cell`] pour une cellule de
    /// données : ce geste n'a de sens que pour un champ unique.
    pub fn selected_header_field(&self) -> Option<(NodeId, HeaderField)> {
        match self.kind {
            SelectionKind::Rows { anchor, cursor } if anchor == cursor => match anchor {
                GridCursor::Header(column, field) => Some((column, field)),
                GridCursor::Data(..) => None,
            },
            _ => None,
        }
    }

    /// Vrai si `(column, field)` fait partie de la plage sélectionnée.
    pub fn is_header_field_selected(
        &self,
        tree: &Tree,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) -> bool {
        match &self.kind {
            SelectionKind::Rows { anchor, cursor } => {
                let columns = order_range(tree, sheet, anchor.column(), cursor.column());
                if !columns.contains(&column) {
                    return false;
                }
                let (lo, hi) = rank_range(*anchor, *cursor);
                let rank = field.index() as isize - 4;
                rank >= lo && rank <= hi
            }
            SelectionKind::Columns { .. } => false,
        }
    }

    /// Étend le rectangle en cours jusqu'à `target`, en gardant l'ancre déjà
    /// posée — glisser ou Maj-clic, aussi bien depuis une cellule de données
    /// que depuis un champ d'en-tête, `target` pouvant être l'un ou l'autre :
    /// c'est ce qui rend le rectangle mixte possible (voir la doc de tête du
    /// module). Sans effet si la sélection courante n'est pas déjà un
    /// rectangle de ce genre (c'est le cas d'une plage de colonnes : il faut
    /// re-cliquer une cellule ou un champ pour repartir en rectangle, voir
    /// [`crate::state::AppState::extend_selection`]/[`crate::state::
    /// AppState::extend_header_field_selection`]).
    pub fn extend_row_to(&mut self, target: GridCursor) {
        if let SelectionKind::Rows { cursor, .. } = &mut self.kind {
            *cursor = target;
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

    /// Vrai si la sélection est une plage de colonnes entières (rangée des
    /// lettres) — permet à [`crate::state::AppState::extend_selection`]/
    /// [`crate::state::AppState::extend_header_field_selection`] de
    /// distinguer une extension normale (au sein du rectangle mixte) d'un
    /// franchissement de frontière depuis la rangée des lettres, qui doit
    /// reposer une sélection simple plutôt que d'étendre silencieusement
    /// sans effet ; et à [`crate::state::AppState::extend_column_selection`]
    /// de faire l'inverse.
    pub fn is_columns(&self) -> bool {
        matches!(self.kind, SelectionKind::Columns { .. })
    }

    /// `Some((column, row))` si la sélection est réduite à une seule
    /// cellule de données (ancre et curseur confondus, tous deux une
    /// cellule — un simple clic, jamais étendu depuis, et jamais un champ
    /// d'en-tête), `None` pour un rectangle plus grand ou toute autre sorte
    /// de sélection.
    ///
    /// Sert à démarrer l'édition par simple frappe au clavier (taper sur une
    /// cellule sélectionnée remplace tout son contenu, comme dans un
    /// tableur) : ce geste n'a de sens que pour une cellule unique, jamais
    /// pour une plage ni pour un champ d'en-tête (qui a son propre geste,
    /// voir [`Selection::selected_header_field`]) — taper sur une plage
    /// sélectionnée ne doit pas écraser silencieusement plusieurs cellules à
    /// la fois.
    pub fn single_cell(&self) -> Option<(NodeId, usize)> {
        match &self.kind {
            SelectionKind::Rows { anchor, cursor } if anchor == cursor => match anchor {
                GridCursor::Data(column, row) => Some((*column, *row)),
                GridCursor::Header(..) => None,
            },
            _ => None,
        }
    }

    /// Point de départ unifié de la navigation clavier — la dernière case
    /// visée, cellule de données ou champ d'en-tête. `None` pour une plage
    /// de colonnes : la rangée des lettres n'a pas de « case suivante » qui
    /// ait un sens évident, il faut re-cliquer une cellule ou un champ pour
    /// reprendre la navigation. Utilisé aussi bien par [`crate::state::
    /// AppState::move_selection`] (flèches) que par [`crate::state::
    /// AppState::move_selection_tab`] (Tab/Maj+Tab), qui traversent tous
    /// deux la frontière en-tête/données.
    pub fn grid_cursor(&self) -> Option<GridCursor> {
        match &self.kind {
            SelectionKind::Rows { cursor, .. } => Some(*cursor),
            SelectionKind::Columns { .. } => None,
        }
    }

    /// Colonnes couvertes, dans l'ordre de la feuille. Vide si l'ancre ou le
    /// curseur ne désignent plus une colonne de `sheet` (colonne supprimée
    /// entre-temps).
    pub fn columns(&self, tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
        match &self.kind {
            SelectionKind::Columns { anchor, cursor } => order_range(tree, sheet, *anchor, *cursor),
            SelectionKind::Rows { anchor, cursor } => {
                order_range(tree, sheet, anchor.column(), cursor.column())
            }
        }
    }

    /// Portion **données** de la plage sélectionnée, en lignes inclusives
    /// `(min, max)` — `None` si la sélection est une plage de colonnes
    /// entières, ou si elle est un rectangle entièrement contenu dans
    /// l'en-tête (aucune ligne de données touchée). Un rectangle mixte qui
    /// commence dans l'en-tête et continue dans les données (ex. Commentaire
    /// → ligne 19) rend la portion données seule, bornée à 0 : la plage de
    /// rangs `[-2, 19]` (Commentaire, rang -2, à la ligne 19) devient
    /// `Some((0, 19))`, les 20 premières lignes.
    ///
    /// Réservé aux opérations propres aux données de colonne
    /// ([`Selection::clear_ranges`]/[`Selection::delete_at_count`]/
    /// [`crate::state::AppState::selected_cells_tsv`]) — un champ d'en-tête
    /// n'a pas de « ligne » au même sens. Voir [`Selection::full_rank_range`]
    /// pour la plage complète, en-tête compris.
    pub fn row_range(&self) -> Option<(usize, usize)> {
        match &self.kind {
            SelectionKind::Rows { anchor, cursor } => {
                let (lo, hi) = rank_range(*anchor, *cursor);
                if hi < 0 {
                    None
                } else {
                    Some((lo.max(0) as usize, hi as usize))
                }
            }
            SelectionKind::Columns { .. } => None,
        }
    }

    /// Plage complète `[lo, hi]` de rangs (voir [`GridCursor::rank`])
    /// couverte par un rectangle mixte, en-tête compris (contrairement à
    /// [`Selection::row_range`], qui n'en rend que la portion données) —
    /// `None` pour une plage de colonnes entières. Sert à
    /// [`crate::state::AppState::selected_cells_tsv`] (une ligne de TSV par
    /// rang couvert) et à [`Selection::cleared_header_fields`].
    pub fn full_rank_range(&self) -> Option<(isize, isize)> {
        match &self.kind {
            SelectionKind::Rows { anchor, cursor } => Some(rank_range(*anchor, *cursor)),
            SelectionKind::Columns { .. } => None,
        }
    }

    /// Champs d'en-tête `(colonne, champ)` touchés par la sélection, un par
    /// colonne et par champ de la portion en-tête de la plage — vide si la
    /// sélection ne touche aucun champ (plage de colonnes, ou rectangle
    /// entièrement dans les données). Sert à [`crate::state::AppState::
    /// clear_selected_cells`] : Retour arrière/Ctrl+X sur un rectangle mixte
    /// vide aussi les champs d'en-tête touchés, pas seulement la portion
    /// données.
    pub fn cleared_header_fields(&self, tree: &Tree, sheet: NodeId) -> Vec<(NodeId, HeaderField)> {
        let SelectionKind::Rows { anchor, cursor } = &self.kind else {
            return Vec::new();
        };
        let (lo, hi) = rank_range(*anchor, *cursor);
        if lo >= 0 {
            return Vec::new();
        }
        let hi = hi.min(-1);
        let columns = order_range(tree, sheet, anchor.column(), cursor.column());
        columns
            .into_iter()
            .flat_map(|column| {
                (lo..=hi).map(move |rank| (column, HeaderField::from_index((rank + 4) as usize)))
            })
            .collect()
    }

    /// Vrai si `(column, row)` fait partie de la sélection.
    pub fn contains(&self, tree: &Tree, sheet: NodeId, column: NodeId, row: usize) -> bool {
        match &self.kind {
            SelectionKind::Columns { .. } => self.columns(tree, sheet).contains(&column),
            SelectionKind::Rows { .. } => match self.row_range() {
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
    /// Ne couvre que la portion **données** ; voir [`Selection::
    /// cleared_header_fields`] pour la portion en-tête d'un rectangle mixte.
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
                    None if self.is_columns() => (0, len),
                    None => return None,
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
    /// taille de la portion données de la plage sélectionnée
    /// (`hi - lo + 1`) — retirer *exactement* les lignes sélectionnées, ni
    /// plus ni moins, sinon Suppr sur une seule ligne tronquerait tout ce
    /// qui suit au lieu de ne faire remonter que cette ligne. Pour une plage
    /// de colonnes entières (pas de plage de lignes), `count` reste calculé
    /// sur la colonne sélectionnée la plus longue, pour qu'elle soit
    /// intégralement compactée même si une autre colonne de la plage est
    /// plus courte. Un rectangle entièrement dans l'en-tête (aucune donnée
    /// touchée) rend `count == 0` : Suppr n'a pas d'équivalent sur un champ
    /// d'en-tête, qui n'est pas répété par ligne.
    pub fn delete_at_count(&self, tree: &Tree, sheet: NodeId) -> (Vec<NodeId>, usize, usize) {
        let columns = self.columns(tree, sheet);
        let (start, count) = match &self.kind {
            SelectionKind::Columns { .. } => {
                let count = columns
                    .iter()
                    .filter_map(|&column| tree.node(column).and_then(plotyx_core::Node::as_column))
                    .map(plotyx_core::Column::len)
                    .max()
                    .unwrap_or(0);
                (0, count)
            }
            SelectionKind::Rows { .. } => match self.row_range() {
                Some((lo, hi)) => (lo, hi.saturating_sub(lo) + 1),
                None => (0, 0),
            },
        };
        (columns, start, count)
    }

    /// Cellule d'ancrage — coin haut-gauche logique du rectangle en données,
    /// ou la première colonne de la plage à la ligne 0 pour une plage de
    /// colonnes ou une ancre posée sur un champ d'en-tête. Sert de point de
    /// départ au collage (Ctrl+V colle à partir de l'ancre, indépendamment
    /// de la taille de la sélection active, comme dans tout tableur de
    /// référence) : coller ne touche jamais les champs d'en-tête eux-mêmes,
    /// seulement les données, d'où ce repli sur la ligne 0 quand l'ancre est
    /// un champ.
    pub fn anchor(&self) -> (NodeId, usize) {
        match &self.kind {
            SelectionKind::Rows { anchor, .. } => match anchor {
                GridCursor::Data(column, row) => (*column, *row),
                GridCursor::Header(column, _) => (*column, 0),
            },
            SelectionKind::Columns { anchor, .. } => (*anchor, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`Selection::single_cell`] — verrou du bug 1 (taper sur une cellule
    /// sélectionnée démarre l'édition) : un simple clic reste réduit à une
    /// seule cellule, mais l'étendre — même jusqu'à elle-même via un
    /// Maj-clic ou un glisser sans mouvement réel — ne doit plus jamais être
    /// traité comme une cellule unique une fois `extend_row_to` appelé,
    /// puisque le geste qui a produit la sélection n'est alors plus « un
    /// simple clic sur une cellule ». Un champ d'en-tête unique n'est non
    /// plus jamais `single_cell` (voir [`Selection::selected_header_field`]
    /// pour son pendant).
    #[test]
    fn single_cell_is_some_only_for_a_plain_click() {
        let column = NodeId::from_raw(1);
        let other_column = NodeId::from_raw(2);

        let selection = Selection::cell(column, 3);
        assert_eq!(selection.single_cell(), Some((column, 3)));

        let mut extended = Selection::cell(column, 3);
        extended.extend_row_to(GridCursor::Data(column, 4));
        assert_eq!(
            extended.single_cell(),
            None,
            "un rectangle de plusieurs lignes n'est pas une cellule unique"
        );

        let mut extended_columns = Selection::cell(column, 3);
        extended_columns.extend_row_to(GridCursor::Data(other_column, 3));
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

        assert_eq!(
            Selection::header_field(column, HeaderField::Comment).single_cell(),
            None,
            "un champ d'en-tête unique n'est pas une cellule unique"
        );
    }
}

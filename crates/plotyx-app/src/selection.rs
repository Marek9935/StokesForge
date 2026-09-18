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
//! # Un seul axe, jamais traversé par une sélection
//!
//! Les 4 champs d'en-tête et les lignes de données d'une même colonne
//! forment un seul axe ordonné (voir [`GridCursor::rank`]) : Nom long <
//! Unité < Commentaire < Formule < ligne 0 < ligne 1 < … mais un rectangle de
//! sélection ([`SelectionKind::Rows`]) reste toujours entièrement d'un seul
//! côté de la frontière (rang -1/0) : soit entièrement dans l'en-tête, soit
//! entièrement dans les données — **jamais à cheval sur les deux**. Un
//! glisser ou un Maj-clic qui commence d'un côté et vise l'autre s'arrête à
//! la frontière plutôt que de la traverser ([`Selection::extend_row_to`]) :
//! la case visée est ramenée à la dernière case du côté de l'ancre (le champ
//! Formule si l'ancre est dans l'en-tête, la ligne 0 si elle est dans les
//! données). Trois correctifs successifs ont fait vivre puis grossir un
//! « rectangle mixte » qui traversait cette frontière ; il rendait le
//! copier/coller incompréhensible et a été retiré au profit de cette règle
//! plus simple, décidée avec l'utilisateur. La plage de colonnes entières
//! ([`SelectionKind::Columns`], glisser/Maj-clic sur la rangée des lettres)
//! reste un geste distinct, jamais mélangé au reste : un glisser qui la
//! croise repose toujours une sélection simple (voir [`crate::state::
//! AppState::extend_selection`]/[`crate::state::AppState::
//! extend_header_field_selection`]/[`crate::state::AppState::
//! extend_column_selection`]).

use plotyx_core::{NodeId, Tree};

use crate::state::{visible_columns_in, HeaderField};

/// Sélection courante dans la grille d'une feuille : un rectangle borné à
/// l'en-tête ou aux données ([`SelectionKind::Rows`], jamais les deux à la
/// fois — voir la doc de tête du module) ou une plage de colonnes entières
/// ([`SelectionKind::Columns`], glisser/Maj-clic sur la rangée des lettres)
/// — même comportement clic-sélectionne/glisser-étende/double-clic-édite
/// pour les deux, voir [`crate::app::header_field_widget`],
/// [`crate::app::cell_widget`] et [`crate::app::column_header_widget`].
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    kind: SelectionKind,
}

#[derive(Debug, Clone, PartialEq)]
enum SelectionKind {
    /// Rectangle de champs d'en-tête ou de cellules de données (jamais les
    /// deux), entre `anchor` (posée au premier clic, jamais déplacée par la
    /// suite) et `cursor` (la dernière case visée — un simple clic pose
    /// `cursor == anchor`, un glisser ou un Maj-clic ne déplace que
    /// `cursor`). `anchor` et `cursor` sont toujours de la même sorte
    /// (toutes deux un champ, ou toutes deux une cellule) :
    /// [`Selection::extend_row_to`] ramène systématiquement `cursor` à la
    /// frontière plutôt que de la laisser changer de sorte, voir la doc de
    /// tête du module.
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

/// Ramène `target` à la frontière de la zone de `anchor` s'il n'est pas de
/// la même sorte qu'elle — voir la doc de tête du module. La colonne visée
/// par `target` n'est, elle, jamais bornée : un glisser peut couvrir
/// plusieurs colonnes même en butant sur la frontière de rang.
fn clamp_to_anchor_zone(anchor: GridCursor, target: GridCursor) -> GridCursor {
    match (anchor, target) {
        (GridCursor::Header(..), GridCursor::Data(column, _)) => {
            GridCursor::Header(column, HeaderField::Formula)
        }
        (GridCursor::Data(..), GridCursor::Header(column, _)) => GridCursor::Data(column, 0),
        _ => target,
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
    /// que depuis un champ d'en-tête. `target` est ramené à la frontière de
    /// la zone de l'ancre s'il n'est pas de la même sorte qu'elle (voir
    /// [`clamp_to_anchor_zone`] et la doc de tête du module) : un glisser
    /// qui part de l'en-tête et vise une cellule de données s'arrête au
    /// champ Formule (le dernier champ d'en-tête) de la colonne visée ; un
    /// glisser qui part des données et vise un champ d'en-tête s'arrête à la
    /// ligne 0 de la colonne visée — jamais de bascule d'une zone à l'autre
    /// en cours de glisser. Sans effet si la sélection courante n'est pas
    /// déjà un rectangle de ce genre (c'est le cas d'une plage de colonnes :
    /// il faut re-cliquer une cellule ou un champ pour repartir en
    /// rectangle, voir [`crate::state::AppState::extend_selection`]/
    /// [`crate::state::AppState::extend_header_field_selection`]).
    pub fn extend_row_to(&mut self, target: GridCursor) {
        if let SelectionKind::Rows { anchor, cursor } = &mut self.kind {
            *cursor = clamp_to_anchor_zone(*anchor, target);
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
    /// distinguer une extension normale (au sein du rectangle en cours,
    /// borné à l'en-tête ou aux données) d'un franchissement de frontière
    /// depuis la rangée des lettres, qui doit reposer une sélection simple
    /// plutôt que d'étendre silencieusement sans effet ; et à
    /// [`crate::state::AppState::extend_column_selection`] de faire
    /// l'inverse.
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
    /// l'en-tête (aucune ligne de données touchée : depuis que
    /// [`Selection::extend_row_to`] borne la cible à la frontière, un
    /// rectangle est toujours entièrement dans l'en-tête ou entièrement dans
    /// les données, jamais les deux).
    ///
    /// Réservé aux opérations propres aux données de colonne
    /// ([`Selection::clear_ranges`]/[`Selection::delete_at_count`]/
    /// [`crate::state::AppState::copy_selection`]) — un champ d'en-tête n'a
    /// pas de « ligne » au même sens. Voir [`Selection::full_rank_range`]
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
    /// couverte par le rectangle, en-tête compris (contrairement à
    /// [`Selection::row_range`], qui n'en rend que la portion données) —
    /// toujours entièrement négative (en-tête) ou entièrement positive ou
    /// nulle (données), jamais à cheval (voir la doc de tête du module) —
    /// `None` pour une plage de colonnes entières. Sert à
    /// [`crate::state::AppState::copy_selection`] (une rangée copiée par
    /// rang couvert) et à [`Selection::cleared_header_fields`].
    pub fn full_rank_range(&self) -> Option<(isize, isize)> {
        match &self.kind {
            SelectionKind::Rows { anchor, cursor } => Some(rank_range(*anchor, *cursor)),
            SelectionKind::Columns { .. } => None,
        }
    }

    /// Champs d'en-tête `(colonne, champ)` touchés par la sélection, un par
    /// colonne et par champ — vide si la sélection ne touche aucun champ
    /// (plage de colonnes, ou rectangle entièrement dans les données ; un
    /// rectangle est toujours entièrement dans l'un ou dans l'autre, voir la
    /// doc de tête du module). Sert à [`crate::state::AppState::
    /// clear_selected_cells`] : Retour arrière/Ctrl+X sur une sélection de
    /// champs d'en-tête les vide tous.
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
    /// cleared_header_fields`] pour un rectangle de champs d'en-tête.
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

    /// Ancre de la sélection — la case posée au premier clic, jamais
    /// déplacée par la suite ; la première colonne de la plage à la ligne 0
    /// pour une plage de colonnes entières, qui n'a pas d'ancre au sens
    /// d'une case précise. Sert de point de départ au collage
    /// ([`crate::state::AppState::paste`]/[`crate::state::AppState::
    /// paste_absolute`] collent à partir de l'ancre, indépendamment de la
    /// taille de la sélection active, comme dans tout tableur de référence)
    /// : coller sur un champ d'en-tête écrit dans l'en-tête, coller sur une
    /// cellule écrit dans les données — voir la matrice de comportement en
    /// tête de [`crate::state`].
    pub fn anchor(&self) -> GridCursor {
        match &self.kind {
            SelectionKind::Rows { anchor, .. } => *anchor,
            SelectionKind::Columns { anchor, .. } => GridCursor::Data(*anchor, 0),
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

    /// [`Selection::extend_row_to`] — un glisser parti d'un champ d'en-tête
    /// qui vise une cellule de données s'arrête au champ Formule de la
    /// colonne visée, jamais dans les données elles-mêmes : voir la doc de
    /// tête du module.
    #[test]
    fn extending_from_a_header_field_into_data_stops_at_the_formula_field() {
        let column = NodeId::from_raw(1);
        let other_column = NodeId::from_raw(2);

        let mut selection = Selection::header_field(column, HeaderField::Unit);
        selection.extend_row_to(GridCursor::Data(other_column, 5));

        assert_eq!(
            selection.grid_cursor(),
            Some(GridCursor::Header(other_column, HeaderField::Formula)),
            "le glisser s'arrête au dernier champ d'en-tête de la colonne visée"
        );
    }

    /// Symétrique : un glisser parti d'une cellule de données qui vise un
    /// champ d'en-tête s'arrête à la ligne 0 de la colonne visée, jamais
    /// dans l'en-tête lui-même.
    #[test]
    fn extending_from_data_into_a_header_field_stops_at_row_zero() {
        let column = NodeId::from_raw(1);
        let other_column = NodeId::from_raw(2);

        let mut selection = Selection::cell(column, 5);
        selection.extend_row_to(GridCursor::Header(other_column, HeaderField::Comment));

        assert_eq!(
            selection.grid_cursor(),
            Some(GridCursor::Data(other_column, 0)),
            "le glisser s'arrête à la ligne 0 de la colonne visée"
        );
    }
}

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
//! Les 5 champs d'en-tête et les lignes de données d'une même colonne
//! forment un seul axe ordonné (voir [`GridCursor::rank`]) : x' < Nom long <
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
//! la plage de lignes entières ([`SelectionKind::FullRows`], glisser/
//! Maj-clic sur la gouttière des numéros de ligne) et la plage d'un champ
//! d'en-tête entier ([`SelectionKind::HeaderRows`], glisser/Maj-clic sur le
//! libellé du champ dans la gouttière, en face de « Long Name »/« Units »/
//! « Comments »/« F(x)= ») restent chacune un geste distinct, jamais mélangé
//! au reste ni l'une à l'autre : un glisser qui croise l'une d'elles repose
//! toujours une sélection simple plutôt que d'étendre silencieusement sans
//! effet (voir [`crate::state::AppState::extend_selection`]/[`crate::state::
//! AppState::extend_header_field_selection`]/[`crate::state::AppState::
//! extend_column_selection`]/[`crate::state::AppState::
//! extend_row_selection`]/[`crate::state::AppState::
//! extend_header_row_selection`]). Une plage de lignes entières ne touche par
//! ailleurs jamais l'en-tête, et une plage de champ d'en-tête entier ne
//! touche jamais les données : chacune est le pendant de l'autre zone, comme
//! `Columns` l'est de `FullRows`.

use stokesforge_core::{NodeId, Tree};

use crate::state::{visible_columns_in, HeaderField};

/// Sélection courante dans la grille d'une feuille : un rectangle borné à
/// l'en-tête ou aux données ([`SelectionKind::Rows`], jamais les deux à la
/// fois — voir la doc de tête du module), une plage de colonnes entières
/// ([`SelectionKind::Columns`], glisser/Maj-clic sur la rangée des lettres)
/// une plage de lignes entières ([`SelectionKind::FullRows`], glisser/
/// Maj-clic sur la gouttière des numéros de ligne), ou une plage d'un champ
/// d'en-tête entier ([`SelectionKind::HeaderRows`], glisser/Maj-clic sur son
/// libellé dans la gouttière) — même comportement clic-sélectionne/
/// glisser-étende/double-clic-édite pour toutes, voir
/// `app::grid::header_field_widget`, `app::grid::cell_widget`,
/// `app::grid::column_header_widget`, `app::grid::row_number_widget` et
/// `app::grid::header_row_label_widget`.
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
    ///
    /// `extra` porte les rectangles déjà posés par Ctrl+clic ou Ctrl+glisser
    /// (#159), tous de la même zone (en-tête ou données) ; le rectangle
    /// `anchor`–`cursor` est le **dernier**, celui que Maj+clic étend. Il ne
    /// compte dans la sélection que si `active` : un Ctrl+clic qui retire une
    /// cellule laisse l'ancre sur elle sans la sélectionner.
    Rows {
        extra: Vec<(GridCursor, GridCursor)>,
        anchor: GridCursor,
        cursor: GridCursor,
        active: bool,
    },
    /// Ensemble de colonnes entières (#159) : les colonnes de `base`
    /// (posées une à une par Ctrl+clic, ou gelées par un autre geste), plus,
    /// si `ranged`, la plage `anchor..=cursor` (mêmes règles que
    /// `Rows::anchor`/`cursor`, mais en colonnes plutôt qu'en cases) —
    /// glisser ou Maj-clic sur la rangée des lettres. `anchor` est la
    /// dernière colonne cliquée. L'ordre de `base` n'a aucun sens :
    /// [`Selection::columns`] rend toujours l'ordre de la feuille.
    Columns {
        base: Vec<NodeId>,
        anchor: NodeId,
        cursor: NodeId,
        ranged: bool,
    },
    /// Plage de lignes entières, entre `anchor` et `cursor` (mêmes règles que
    /// `Columns::anchor`/`cursor`, mais en indices de ligne plutôt qu'en
    /// identifiants de colonne) — glisser ou Maj-clic sur la gouttière des
    /// numéros de ligne (`app::grid::row_number_widget`). Symétrique de
    /// `Columns` : elle touche toutes les colonnes visibles de la feuille et,
    /// comme elle, ne touche jamais l'en-tête (une ligne entière est une
    /// sélection de données pures, voir la doc de tête du module).
    ///
    /// Comme `Columns`, c'est un **ensemble** (#159) : les intervalles de
    /// `base` (posés par Ctrl+clic), plus, si `ranged`, la plage
    /// `anchor..=cursor`.
    FullRows {
        base: Vec<(usize, usize)>,
        anchor: usize,
        cursor: usize,
        ranged: bool,
    },
    /// Plage d'un champ d'en-tête entier, entre `anchor` et `cursor` (mêmes
    /// règles que `FullRows::anchor`/`cursor`, mais en champs plutôt qu'en
    /// indices de ligne) — glisser ou Maj-clic sur le libellé du champ dans
    /// la gouttière (`app::grid::header_row_label_widget`). Pendant de
    /// `FullRows` pour la zone d'en-tête, comme `FullRows` l'est de
    /// `Columns` : elle touche toutes les colonnes visibles de la feuille et,
    /// symétriquement, ne touche jamais les données — c'est une sélection
    /// d'en-tête pur.
    HeaderRows {
        anchor: HeaderField,
        cursor: HeaderField,
    },
}

/// Colonnes visibles de `sheet`, sans plage à résoudre — partagé par
/// [`SelectionKind::FullRows`] et [`SelectionKind::HeaderRows`], qui portent
/// chacune toutes les colonnes de la feuille plutôt qu'une sous-plage.
fn all_visible_columns(tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
    visible_columns_in(tree, sheet)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
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
    /// module) : les 5 champs d'en-tête occupent `-5..=-1` (via
    /// [`HeaderField::rank`]), les lignes de données `0..` — comparable
    /// entre deux [`GridCursor`] de la même colonne ou de colonnes
    /// différentes, la colonne se résolvant séparément par position (voir
    /// [`order_range`]).
    fn rank(self) -> isize {
        match self {
            GridCursor::Data(_, row) => row as isize,
            GridCursor::Header(_, field) => field.rank(),
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

/// Un bloc rectangulaire de la sélection : des colonnes (dans l'ordre de la
/// feuille) et une plage de rangs `lo..=hi` (voir [`GridCursor::rank`]). Une
/// sélection multiple (#159) en porte plusieurs ; copier, effacer et
/// supprimer raisonnent bloc par bloc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionBlock {
    pub(crate) columns: Vec<NodeId>,
    pub(crate) lo: isize,
    pub(crate) hi: isize,
}

/// Colonnes et plages de rangs d'une copie ([`Selection::copy_layout`]).
pub(crate) type CopyLayout = (Vec<NodeId>, Vec<(isize, isize)>);

fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Intervalles triés et fusionnés (les intervalles qui se touchent n'en font
/// qu'un : `(1,2)` et `(3,4)` donnent `(1,4)`).
fn merge_intervals(mut intervals: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    intervals.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (lo, hi) in intervals {
        match merged.last_mut() {
            Some(last) if lo <= last.1.saturating_add(1) => last.1 = last.1.max(hi),
            _ => merged.push((lo, hi)),
        }
    }
    merged
}

/// `intervals` privés de `point`, un intervalle coupé en deux au besoin.
fn remove_point(intervals: Vec<(usize, usize)>, point: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (lo, hi) in intervals {
        if point < lo || point > hi {
            out.push((lo, hi));
            continue;
        }
        if point > lo {
            out.push((lo, point - 1));
        }
        if point < hi {
            out.push((point + 1, hi));
        }
    }
    out
}

/// Case de la colonne d'indice `column` (dans `order`) au rang `rank`.
fn cursor_at(order: &[NodeId], column: usize, rank: isize) -> GridCursor {
    if rank < 0 {
        GridCursor::Header(order[column], HeaderField::from_rank(rank))
    } else {
        GridCursor::Data(order[column], rank as usize)
    }
}

/// Bornes `((colonne min, colonne max), (rang min, rang max))` du rectangle
/// `a`–`b`, les colonnes en indices de `order` ; `None` si une colonne a
/// disparu.
fn rect_bounds(
    order: &[NodeId],
    a: GridCursor,
    b: GridCursor,
) -> Option<((usize, usize), (isize, isize))> {
    let ca = order.iter().position(|&c| c == a.column())?;
    let cb = order.iter().position(|&c| c == b.column())?;
    Some((ordered(ca, cb), rank_range(a, b)))
}

/// Le rectangle `a`–`b` privé de la case `(column, rank)`, en rectangles qui
/// la contournent (au plus quatre) ; `a`–`b` tel quel s'il ne la contient pas.
fn rect_minus_cell(
    order: &[NodeId],
    a: GridCursor,
    b: GridCursor,
    column: usize,
    rank: isize,
) -> Vec<(GridCursor, GridCursor)> {
    let Some(((c0, c1), (r0, r1))) = rect_bounds(order, a, b) else {
        return Vec::new();
    };
    if column < c0 || column > c1 || rank < r0 || rank > r1 {
        return vec![(a, b)];
    }
    let mut pieces = Vec::new();
    let mut push = |ca: usize, cb: usize, ra: isize, rb: isize| {
        pieces.push((cursor_at(order, ca, ra), cursor_at(order, cb, rb)));
    };
    if column > c0 {
        push(c0, column - 1, r0, r1);
    }
    if column < c1 {
        push(column + 1, c1, r0, r1);
    }
    if rank > r0 {
        push(column, column, r0, rank - 1);
    }
    if rank < r1 {
        push(column, column, rank + 1, r1);
    }
    pieces
}

impl Selection {
    /// Sélection d'une seule cellule — l'ancre d'un futur rectangle.
    pub fn cell(column: NodeId, row: usize) -> Self {
        let cursor = GridCursor::Data(column, row);
        Self {
            kind: SelectionKind::Rows {
                extra: Vec::new(),
                anchor: cursor,
                cursor,
                active: true,
            },
        }
    }

    /// Sélection d'une seule colonne entière — l'ancre d'une future plage.
    pub fn column(column: NodeId) -> Self {
        Self {
            kind: SelectionKind::Columns {
                base: Vec::new(),
                anchor: column,
                cursor: column,
                ranged: true,
            },
        }
    }

    /// Sélection de **toutes** les colonnes données (dans l'ordre de la
    /// feuille), en-tête compris — la case du coin et Ctrl+A (#159). `None`
    /// pour une feuille sans colonne.
    pub fn all_columns(columns: &[NodeId]) -> Option<Self> {
        let first = *columns.first()?;
        Some(Self {
            kind: SelectionKind::Columns {
                base: columns.to_vec(),
                anchor: first,
                cursor: first,
                ranged: false,
            },
        })
    }

    /// Sélection d'une seule ligne entière — l'ancre d'une future plage.
    pub fn row(row: usize) -> Self {
        Self {
            kind: SelectionKind::FullRows {
                base: Vec::new(),
                anchor: row,
                cursor: row,
                ranged: true,
            },
        }
    }

    /// Sélection d'un champ d'en-tête entier (toutes les colonnes visibles
    /// de la feuille) — l'ancre d'une future plage, pendant de
    /// [`Selection::row`] pour la zone d'en-tête.
    pub fn header_row(field: HeaderField) -> Self {
        Self {
            kind: SelectionKind::HeaderRows {
                anchor: field,
                cursor: field,
            },
        }
    }

    /// Sélection d'un seul champ d'en-tête **d'une colonne précise** —
    /// l'ancre d'une future plage. Distinct de [`Selection::header_row`],
    /// qui touche d'emblée toutes les colonnes : celui-ci n'en touche qu'une,
    /// et un glisser peut ensuite l'étendre à d'autres colonnes (voir
    /// [`Selection::extend_row_to`]) sans jamais couvrir *toutes* les
    /// colonnes d'un même geste — les deux widgets qui posent l'une ou
    /// l'autre sont distincts (`app::grid::header_field_widget` pour
    /// celle-ci, `app::grid::header_row_label_widget` pour
    /// [`Selection::header_row`]).
    pub fn header_field(column: NodeId, field: HeaderField) -> Self {
        let cursor = GridCursor::Header(column, field);
        Self {
            kind: SelectionKind::Rows {
                extra: Vec::new(),
                anchor: cursor,
                cursor,
                active: true,
            },
        }
    }

    pub fn selected_header_field(&self) -> Option<(NodeId, HeaderField)> {
        match &self.kind {
            SelectionKind::Rows {
                extra,
                anchor,
                cursor,
                active: true,
            } if extra.is_empty() && anchor == cursor => match anchor {
                GridCursor::Header(column, field) => Some((*column, *field)),
                GridCursor::Data(..) => None,
            },
            _ => None,
        }
    }

    pub fn is_header_field_selected(
        &self,
        tree: &Tree,
        sheet: NodeId,
        column: NodeId,
        field: HeaderField,
    ) -> bool {
        let rank = field.rank();
        self.blocks(tree, sheet)
            .iter()
            .any(|block| block.lo <= rank && rank <= block.hi && block.columns.contains(&column))
    }

    pub fn extend_row_to(&mut self, target: GridCursor) {
        if let SelectionKind::Rows {
            anchor,
            cursor,
            active,
            ..
        } = &mut self.kind
        {
            *cursor = clamp_to_anchor_zone(*anchor, target);
            *active = true;
        }
    }

    /// Ctrl+clic sur une cellule ou un champ d'en-tête (#159) : ajoute la
    /// case à la sélection, ou la retire (le rectangle qui la contient est
    /// découpé autour d'elle), et y pose l'ancre. `false` si la sélection
    /// n'est pas un ensemble de cases de la même zone (en-tête ou données) :
    /// l'appelant pose alors une sélection simple.
    pub fn toggle_cell(&mut self, tree: &Tree, sheet: NodeId, target: GridCursor) -> bool {
        let SelectionKind::Rows {
            extra,
            anchor,
            cursor,
            active,
        } = &mut self.kind
        else {
            return false;
        };
        if (anchor.rank() < 0) != (target.rank() < 0) {
            return false;
        }
        let order = all_visible_columns(tree, sheet);
        let Some(column) = order.iter().position(|&c| c == target.column()) else {
            return false;
        };
        let rank = target.rank();
        let mut rects = extra.clone();
        if *active {
            rects.push((*anchor, *cursor));
        }
        let hit = rects.iter().any(|&(a, b)| {
            rect_bounds(&order, a, b).is_some_and(|((c0, c1), (r0, r1))| {
                (c0..=c1).contains(&column) && (r0..=r1).contains(&rank)
            })
        });
        if hit {
            *extra = rects
                .into_iter()
                .flat_map(|(a, b)| rect_minus_cell(&order, a, b, column, rank))
                .collect();
            *active = false;
        } else {
            *extra = rects;
            *active = true;
        }
        *anchor = target;
        *cursor = target;
        true
    }

    /// Ctrl+glisser (#159) : ouvre un nouveau rectangle en `target`, sans
    /// toucher aux précédents ; Maj+clic et le glisser l'étendent ensuite.
    /// `false` dans les mêmes cas que [`Selection::toggle_cell`].
    pub fn add_cell(&mut self, target: GridCursor) -> bool {
        let SelectionKind::Rows {
            extra,
            anchor,
            cursor,
            active,
        } = &mut self.kind
        else {
            return false;
        };
        if (anchor.rank() < 0) != (target.rank() < 0) {
            return false;
        }
        if *active {
            extra.push((*anchor, *cursor));
        }
        *anchor = target;
        *cursor = target;
        *active = true;
        true
    }

    /// Étend une plage de colonnes jusqu'à `column`, en gardant l'ancre
    /// posée par [`Selection::column`] — glisser ou Maj-clic sur la rangée
    /// des lettres. Sans effet si la sélection courante n'est pas déjà une
    /// plage de colonnes.
    ///
    /// Les colonnes déjà posées par Ctrl+clic restent : c'est le geste
    /// additif (glisser, Ctrl+Maj+clic). Pour un Maj+clic simple, qui
    /// remplace tout par la plage ancre → `column`, voir
    /// [`Selection::extend_columns_replacing`].
    pub fn extend_columns_to(&mut self, column: NodeId) {
        if let SelectionKind::Columns { cursor, ranged, .. } = &mut self.kind {
            *cursor = column;
            *ranged = true;
        }
    }

    /// Maj+clic : la sélection devient exactement la plage ancre →
    /// `column`, les colonnes posées une à une disparaissent.
    pub fn extend_columns_replacing(&mut self, column: NodeId) {
        if let SelectionKind::Columns { base, .. } = &mut self.kind {
            base.clear();
        }
        self.extend_columns_to(column);
    }

    /// Ctrl+clic : bascule l'appartenance de `column` et déplace l'ancre
    /// sur elle. Sans effet si la sélection n'est pas un ensemble de
    /// colonnes (l'appelant pose alors [`Selection::column`]).
    pub fn toggle_column(&mut self, tree: &Tree, sheet: NodeId, column: NodeId) {
        let selected = self.columns(tree, sheet);
        if let SelectionKind::Columns {
            base,
            anchor,
            cursor,
            ranged,
        } = &mut self.kind
        {
            *anchor = column;
            *cursor = column;
            if selected.contains(&column) {
                *base = selected.into_iter().filter(|&c| c != column).collect();
                *ranged = false;
            } else {
                *base = selected;
                *ranged = true;
            }
        }
    }

    /// Disposition de la copie (#159) : les colonnes (ordre de la feuille) et
    /// les plages de rangs à copier, bout à bout, dans l'ordre de la feuille.
    /// Des blocs qui partagent les **mêmes lignes** se mettent côte à côte,
    /// ceux qui partagent les **mêmes colonnes** s'empilent ; sinon `None` :
    /// la copie serait ambiguë (Copy et Cut grisés). Un bloc unique, comme un
    /// ensemble de colonnes entières, est toujours copiable (`None` aussi
    /// pour ce dernier : il se copie par colonne, sans rangs).
    pub(crate) fn copy_layout(&self, tree: &Tree, sheet: NodeId) -> Option<CopyLayout> {
        let blocks = self.blocks(tree, sheet);
        let first = blocks.first()?;
        if blocks.iter().all(|b| (b.lo, b.hi) == (first.lo, first.hi)) {
            return Some((self.columns(tree, sheet), vec![(first.lo, first.hi)]));
        }
        if blocks.iter().all(|b| b.columns == first.columns) {
            // Rangs décalés de `HeaderField::COUNT` pour fusionner en non
            // signés (les champs d'en-tête occupent `-COUNT..=-1`).
            let merged = merge_intervals(
                blocks
                    .iter()
                    .map(|b| {
                        (
                            (b.lo + HeaderField::COUNT as isize) as usize,
                            (b.hi + HeaderField::COUNT as isize) as usize,
                        )
                    })
                    .collect(),
            );
            let ranges = merged
                .into_iter()
                .map(|(lo, hi)| {
                    (
                        lo as isize - HeaderField::COUNT as isize,
                        hi as isize - HeaderField::COUNT as isize,
                    )
                })
                .collect();
            return Some((first.columns.clone(), ranges));
        }
        None
    }

    pub fn is_contiguous(&self, tree: &Tree, sheet: NodeId) -> bool {
        match &self.kind {
            SelectionKind::Columns { .. } => {
                let selected = self.columns(tree, sheet);
                let order = all_visible_columns(tree, sheet);
                let positions: Vec<usize> = selected
                    .iter()
                    .filter_map(|c| order.iter().position(|o| o == c))
                    .collect();
                positions.windows(2).all(|w| w[1] == w[0] + 1)
            }
            _ => self.blocks(tree, sheet).len() <= 1,
        }
    }

    pub fn extend_full_rows_to(&mut self, row: usize) {
        if let SelectionKind::FullRows { cursor, ranged, .. } = &mut self.kind {
            *cursor = row;
            *ranged = true;
        }
    }

    /// Maj+clic sur un numéro de ligne : la sélection devient exactement la
    /// plage ancre → `row`, les lignes posées au Ctrl+clic disparaissent.
    pub fn extend_full_rows_replacing(&mut self, row: usize) {
        if let SelectionKind::FullRows { base, .. } = &mut self.kind {
            base.clear();
        }
        self.extend_full_rows_to(row);
    }

    /// Ctrl+clic sur un numéro de ligne (#159) : bascule l'appartenance de
    /// `row` et y pose l'ancre. Sans effet si la sélection n'est pas un
    /// ensemble de lignes.
    pub fn toggle_row(&mut self, row: usize) {
        let selected = self.full_row_intervals();
        if let SelectionKind::FullRows {
            base,
            anchor,
            cursor,
            ranged,
        } = &mut self.kind
        {
            *anchor = row;
            *cursor = row;
            if selected.iter().any(|&(lo, hi)| (lo..=hi).contains(&row)) {
                *base = remove_point(selected, row);
                *ranged = false;
            } else {
                *base = selected;
                *ranged = true;
            }
        }
    }

    /// Intervalles de lignes d'un ensemble de lignes entières, triés et
    /// fusionnés ; vide pour toute autre sorte de sélection.
    fn full_row_intervals(&self) -> Vec<(usize, usize)> {
        match &self.kind {
            SelectionKind::FullRows {
                base,
                anchor,
                cursor,
                ranged,
            } => {
                let mut intervals = base.clone();
                if *ranged {
                    intervals.push(ordered(*anchor, *cursor));
                }
                merge_intervals(intervals)
            }
            _ => Vec::new(),
        }
    }

    /// Rectangles de cases d'un ensemble de cases, le dernier compris s'il
    /// est actif ; vide pour toute autre sorte de sélection.
    fn rects(&self) -> Vec<(GridCursor, GridCursor)> {
        match &self.kind {
            SelectionKind::Rows {
                extra,
                anchor,
                cursor,
                active,
            } => {
                let mut rects = extra.clone();
                if *active {
                    rects.push((*anchor, *cursor));
                }
                rects
            }
            _ => Vec::new(),
        }
    }

    /// Les blocs rectangulaires de la sélection (voir [`SelectionBlock`]) ;
    /// vide pour un ensemble de colonnes entières, qui se lit par
    /// [`Selection::columns`].
    pub(crate) fn blocks(&self, tree: &Tree, sheet: NodeId) -> Vec<SelectionBlock> {
        match &self.kind {
            SelectionKind::Rows { .. } => self
                .rects()
                .into_iter()
                .filter_map(|(a, b)| {
                    let columns = order_range(tree, sheet, a.column(), b.column());
                    if columns.is_empty() {
                        return None;
                    }
                    let (lo, hi) = rank_range(a, b);
                    Some(SelectionBlock { columns, lo, hi })
                })
                .collect(),
            SelectionKind::FullRows { .. } => {
                let columns = all_visible_columns(tree, sheet);
                if columns.is_empty() {
                    return Vec::new();
                }
                self.full_row_intervals()
                    .into_iter()
                    .map(|(lo, hi)| SelectionBlock {
                        columns: columns.clone(),
                        lo: lo as isize,
                        hi: hi as isize,
                    })
                    .collect()
            }
            SelectionKind::HeaderRows { .. } => {
                let columns = all_visible_columns(tree, sheet);
                match self.full_rank_range() {
                    Some((lo, hi)) if !columns.is_empty() => {
                        vec![SelectionBlock { columns, lo, hi }]
                    }
                    _ => Vec::new(),
                }
            }
            SelectionKind::Columns { .. } => Vec::new(),
        }
    }

    /// Intervalles de lignes de **données** touchés, triés et fusionnés —
    /// sans l'arbre : une ligne n'a pas besoin des colonnes pour savoir si
    /// elle est touchée.
    fn data_intervals(&self) -> Vec<(usize, usize)> {
        match &self.kind {
            SelectionKind::Rows { .. } => merge_intervals(
                self.rects()
                    .into_iter()
                    .filter_map(|(a, b)| {
                        let (lo, hi) = rank_range(a, b);
                        (hi >= 0).then_some((lo.max(0) as usize, hi as usize))
                    })
                    .collect(),
            ),
            SelectionKind::FullRows { .. } => self.full_row_intervals(),
            SelectionKind::Columns { .. } | SelectionKind::HeaderRows { .. } => Vec::new(),
        }
    }

    /// Étend une plage de champ d'en-tête entier jusqu'à `field`, en gardant
    /// l'ancre posée par [`Selection::header_row`] — glisser ou Maj-clic sur
    /// un libellé de champ dans la gouttière. Sans effet si la sélection
    /// courante n'est pas déjà une plage de ce genre.
    pub fn extend_header_rows_to(&mut self, field: HeaderField) {
        if let SelectionKind::HeaderRows { cursor, .. } = &mut self.kind {
            *cursor = field;
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

    /// Vrai si la sélection est une plage de lignes entières (gouttière des
    /// numéros de ligne) — même rôle que [`Selection::is_columns`], mais
    /// pour la frontière symétrique : permet à [`crate::state::AppState::
    /// extend_selection`]/[`crate::state::AppState::
    /// extend_header_field_selection`] de reposer une sélection simple
    /// plutôt que d'étendre silencieusement sans effet quand un glisser part
    /// de la gouttière, et à [`crate::state::AppState::
    /// extend_row_selection`] de faire l'inverse.
    pub fn is_full_rows(&self) -> bool {
        matches!(self.kind, SelectionKind::FullRows { .. })
    }

    /// Vrai si la sélection est une plage de champ d'en-tête entier
    /// (libellé de champ dans la gouttière) — même rôle que
    /// [`Selection::is_full_rows`], côté en-tête : permet à
    /// [`crate::state::AppState::extend_header_field_selection`] de reposer
    /// une sélection simple plutôt que d'étendre silencieusement sans effet
    /// quand un glisser part de la gouttière, et à [`crate::state::
    /// AppState::extend_header_row_selection`] de faire l'inverse.
    pub fn is_header_rows(&self) -> bool {
        matches!(self.kind, SelectionKind::HeaderRows { .. })
    }

    pub fn single_cell(&self) -> Option<(NodeId, usize)> {
        match &self.kind {
            SelectionKind::Rows {
                extra,
                anchor,
                cursor,
                active: true,
            } if extra.is_empty() && anchor == cursor => match anchor {
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
    /// reprendre la navigation. Même chose pour une plage de lignes entières
    /// ou de champ d'en-tête entier : ni l'une ni l'autre n'a de colonne à
    /// proposer comme case suivante. Utilisé aussi bien par [`crate::state::
    /// AppState::move_selection`] (flèches) que par [`crate::state::
    /// AppState::move_selection_tab`] (Tab/Maj+Tab), qui traversent tous
    /// deux la frontière en-tête/données.
    pub fn grid_cursor(&self) -> Option<GridCursor> {
        match &self.kind {
            SelectionKind::Rows { cursor, .. } => Some(*cursor),
            SelectionKind::Columns { .. }
            | SelectionKind::FullRows { .. }
            | SelectionKind::HeaderRows { .. } => None,
        }
    }

    pub fn columns(&self, tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
        match &self.kind {
            SelectionKind::Columns {
                base,
                anchor,
                cursor,
                ranged,
            } => {
                let range = if *ranged {
                    order_range(tree, sheet, *anchor, *cursor)
                } else {
                    Vec::new()
                };
                all_visible_columns(tree, sheet)
                    .into_iter()
                    .filter(|c| base.contains(c) || range.contains(c))
                    .collect()
            }
            _ => {
                let blocks = self.blocks(tree, sheet);
                all_visible_columns(tree, sheet)
                    .into_iter()
                    .filter(|c| blocks.iter().any(|block| block.columns.contains(c)))
                    .collect()
            }
        }
    }

    /// Les lignes d'une sélection de **lignes entières**, croissantes et sans
    /// doublon — vide pour toute autre sélection (#166) : seul le geste de la
    /// gouttière des numéros de ligne désigne des lignes.
    pub fn full_row_indices(&self) -> Vec<usize> {
        self.full_row_intervals()
            .into_iter()
            .flat_map(|(first, last)| first..=last)
            .collect()
    }

    pub fn row_range(&self) -> Option<(usize, usize)> {
        let intervals = self.data_intervals();
        Some((intervals.first()?.0, intervals.last()?.1))
    }

    pub fn full_rank_range(&self) -> Option<(isize, isize)> {
        match &self.kind {
            SelectionKind::Rows { .. } => {
                let ranges: Vec<(isize, isize)> = self
                    .rects()
                    .into_iter()
                    .map(|(a, b)| rank_range(a, b))
                    .collect();
                Some((
                    ranges.iter().map(|r| r.0).min()?,
                    ranges.iter().map(|r| r.1).max()?,
                ))
            }
            SelectionKind::FullRows { .. } => {
                self.row_range().map(|(lo, hi)| (lo as isize, hi as isize))
            }
            SelectionKind::HeaderRows { anchor, cursor } => {
                let (a, b) = (anchor.rank(), cursor.rank());
                Some(if a <= b { (a, b) } else { (b, a) })
            }
            SelectionKind::Columns { .. } => None,
        }
    }

    pub fn cleared_header_fields(&self, tree: &Tree, sheet: NodeId) -> Vec<(NodeId, HeaderField)> {
        let mut fields: Vec<(NodeId, HeaderField)> = Vec::new();
        for block in self.blocks(tree, sheet) {
            if block.lo >= 0 {
                continue;
            }
            for &column in &block.columns {
                for rank in block.lo..=block.hi.min(-1) {
                    let entry = (column, HeaderField::from_rank(rank));
                    if !fields.contains(&entry) {
                        fields.push(entry);
                    }
                }
            }
        }
        fields
    }

    pub fn contains(&self, tree: &Tree, sheet: NodeId, column: NodeId, row: usize) -> bool {
        match &self.kind {
            SelectionKind::Columns { .. } => self.columns(tree, sheet).contains(&column),
            _ => {
                let row = row as isize;
                self.blocks(tree, sheet).iter().any(|block| {
                    block.hi >= 0
                        && block.lo <= row
                        && row <= block.hi
                        && block.columns.contains(&column)
                })
            }
        }
    }

    /// Vrai si `column` fait partie de la plage de colonnes sélectionnée
    /// (clic, glisser ou Maj-clic sur la rangée des lettres) — distinct de
    /// [`Selection::contains`], qui teste une cellule précise. Faux pour une
    /// plage de lignes entières ou de champ d'en-tête entier : ce n'est pas
    /// la même sélection, même si elle finit par toucher les mêmes colonnes
    /// (voir [`Selection::is_row_selected`]/[`Selection::
    /// is_header_row_selected`] pour leurs pendants respectifs).
    pub fn is_column_selected(&self, tree: &Tree, sheet: NodeId, column: NodeId) -> bool {
        match &self.kind {
            SelectionKind::Columns { .. } => self.columns(tree, sheet).contains(&column),
            _ => false,
        }
    }

    pub fn is_row_selected(&self, row: usize) -> bool {
        self.full_row_intervals()
            .iter()
            .any(|&(lo, hi)| (lo..=hi).contains(&row))
    }

    /// Vrai si `field` fait partie de la plage de champ d'en-tête entier
    /// sélectionnée (clic, glisser ou Maj-clic sur le libellé du champ dans
    /// la gouttière) — pendant de [`Selection::is_row_selected`], côté
    /// en-tête, pour la surbrillance de `app::grid::header_row_label_widget`.
    /// Ne prend ni `tree` ni `sheet`, pour la même
    /// raison que [`Selection::is_row_selected`].
    pub fn is_header_row_selected(&self, field: HeaderField) -> bool {
        match &self.kind {
            SelectionKind::HeaderRows { .. } => self.full_rank_range().is_some_and(|(lo, hi)| {
                let rank = field.rank();
                rank >= lo && rank <= hi
            }),
            _ => false,
        }
    }

    /// Vrai si `column` est touchée par la sélection courante, **quelle que
    /// soit sa sorte** — contrairement à [`Selection::is_column_selected`],
    /// qui ne vaut que pour un clic direct sur la rangée des lettres, ceci
    /// vaut aussi pour un simple rectangle de cellules de données dont
    /// `column` fait partie. Sert à teinter la rangée des lettres d'une
    /// intensité intermédiaire ([`crate::theme::
    /// header_letter_row_touched_fill`]) quand une cellule de sa colonne est
    /// sélectionnée sans que la colonne elle-même le soit — le repère « ta
    /// colonne est concernée » d'un tableur.
    pub fn touches_column(&self, tree: &Tree, sheet: NodeId, column: NodeId) -> bool {
        self.columns(tree, sheet).contains(&column)
    }

    pub fn touches_row(&self, row: usize) -> bool {
        self.data_intervals()
            .iter()
            .any(|&(lo, hi)| (lo..=hi).contains(&row))
    }

    pub fn clear_ranges(&self, tree: &Tree, sheet: NodeId) -> Vec<(NodeId, usize, usize)> {
        let blocks = self.blocks(tree, sheet);
        let mut ranges = Vec::new();
        for column in self.columns(tree, sheet) {
            let Some(len) = tree
                .node(column)
                .and_then(stokesforge_core::Node::as_column)
                .map(stokesforge_core::Column::len)
            else {
                continue;
            };
            let intervals = if self.is_columns() {
                vec![(0, len.saturating_sub(1))]
            } else {
                merge_intervals(
                    blocks
                        .iter()
                        .filter(|block| block.hi >= 0 && block.columns.contains(&column))
                        .map(|block| (block.lo.max(0) as usize, block.hi as usize))
                        .collect(),
                )
            };
            for (lo, hi) in intervals {
                if lo >= len {
                    continue;
                }
                let count = (hi - lo + 1).min(len - lo);
                if count > 0 {
                    ranges.push((column, lo, count));
                }
            }
        }
        ranges
    }

    pub fn delete_at_count(&self, tree: &Tree, sheet: NodeId) -> (Vec<NodeId>, usize, usize) {
        let columns = self.columns(tree, sheet);
        let (start, count) = if self.is_columns() {
            let count = columns
                .iter()
                .filter_map(|&column| {
                    tree.node(column)
                        .and_then(stokesforge_core::Node::as_column)
                })
                .map(stokesforge_core::Column::len)
                .max()
                .unwrap_or(0);
            (0, count)
        } else {
            // Un seul bloc de données : on retire exactement ses lignes. Une
            // sélection multiple (#159) laisserait des trous : remonter
            // n'aurait pas de sens univoque, Suppr se contente de vider.
            match self.blocks(tree, sheet).as_slice() {
                [block] if block.hi >= 0 => (
                    block.lo.max(0) as usize,
                    (block.hi - block.lo.max(0)) as usize + 1,
                ),
                _ => (0, 0),
            }
        };
        (columns, start, count)
    }

    pub fn anchor(&self, tree: &Tree, sheet: NodeId) -> Option<GridCursor> {
        match &self.kind {
            SelectionKind::Rows { anchor, .. } => Some(*anchor),
            SelectionKind::Columns { .. } => self
                .columns(tree, sheet)
                .first()
                .map(|&column| GridCursor::Data(column, 0)),
            SelectionKind::FullRows { .. } => {
                let first_column = visible_columns_in(tree, sheet).into_iter().next()?.0;
                let first_row = self.full_row_intervals().first()?.0;
                Some(GridCursor::Data(first_column, first_row))
            }
            SelectionKind::HeaderRows { anchor, .. } => {
                let first_column = visible_columns_in(tree, sheet).into_iter().next()?.0;
                Some(GridCursor::Header(first_column, *anchor))
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

    /// [`Selection::extend_row_to`] ne fait rien sur une plage de lignes
    /// entières : elle n'est pas un rectangle de ce genre. Verrou du
    /// non-franchissement de frontière symétrique à [`Selection::
    /// extend_columns_to`] (déjà verrouillé côté colonnes par les tests de
    /// `AppState` dans `tests/shell.rs`) — un glisser qui, parti de la
    /// gouttière, croiserait une cellule ne doit jamais faire glisser un
    /// `FullRows` vers un rectangle de cases.
    #[test]
    fn extend_row_to_has_no_effect_on_a_full_rows_selection() {
        let column = NodeId::from_raw(1);
        let mut selection = Selection::row(2);

        selection.extend_row_to(GridCursor::Data(column, 5));

        assert!(
            selection.is_full_rows(),
            "la sélection reste une plage de lignes entières"
        );
        assert_eq!(
            selection.row_range(),
            Some((2, 2)),
            "le curseur n'a pas bougé"
        );
    }

    /// Symétrique : [`Selection::extend_full_rows_to`] ne fait rien sur un
    /// rectangle de cases ou une plage de colonnes — il faut re-cliquer la
    /// gouttière pour repartir en plage de lignes.
    #[test]
    fn extend_full_rows_to_has_no_effect_outside_a_full_rows_selection() {
        let column = NodeId::from_raw(1);

        let mut cell_selection = Selection::cell(column, 3);
        cell_selection.extend_full_rows_to(7);
        assert_eq!(
            cell_selection.single_cell(),
            Some((column, 3)),
            "la sélection de cellule n'a pas bougé"
        );

        let mut column_selection = Selection::column(column);
        column_selection.extend_full_rows_to(7);
        assert!(
            column_selection.is_columns(),
            "la plage de colonnes n'a pas basculé en plage de lignes"
        );
    }

    /// Une plage de lignes entières n'est jamais une cellule unique ni un
    /// champ d'en-tête sélectionné — c'est une sélection de données pures,
    /// distincte des deux (voir la doc de tête du module).
    #[test]
    fn a_full_rows_selection_is_neither_a_single_cell_nor_a_header_field() {
        let selection = Selection::row(4);

        assert_eq!(selection.single_cell(), None);
        assert_eq!(selection.selected_header_field(), None);
        assert_eq!(
            selection.grid_cursor(),
            None,
            "pas de « case suivante » pour une plage de lignes, comme pour une plage de colonnes"
        );
    }

    /// [`Selection::row_range`]/[`Selection::is_row_selected`] couvrent la
    /// plage quel que soit le sens du glisser (ancre après le curseur).
    #[test]
    fn full_rows_range_is_order_independent() {
        let mut selection = Selection::row(5);
        selection.extend_full_rows_to(2);

        assert_eq!(selection.row_range(), Some((2, 5)));
        assert!(selection.is_row_selected(2));
        assert!(selection.is_row_selected(4));
        assert!(selection.is_row_selected(5));
        assert!(!selection.is_row_selected(1));
        assert!(!selection.is_row_selected(6));
    }

    /// [`Selection::extend_row_to`]/[`Selection::extend_columns_to`]/
    /// [`Selection::extend_full_rows_to`] ne font rien sur une plage de champ
    /// d'en-tête entier — verrou du non-franchissement de frontière
    /// symétrique à [`extend_row_to_has_no_effect_on_a_full_rows_selection`]
    /// : un glisser qui, parti d'un libellé de champ dans la gouttière,
    /// croiserait une cellule, une colonne ou la gouttière des numéros de
    /// ligne ne doit jamais faire glisser un `HeaderRows` vers une autre
    /// sorte de sélection.
    #[test]
    fn no_other_extend_method_touches_a_header_rows_selection() {
        let column = NodeId::from_raw(1);
        let mut selection = Selection::header_row(HeaderField::Unit);

        selection.extend_row_to(GridCursor::Data(column, 5));
        selection.extend_columns_to(column);
        selection.extend_full_rows_to(3);

        assert!(
            selection.is_header_rows(),
            "la sélection reste une plage de champ d'en-tête entier"
        );
        assert_eq!(
            selection.full_rank_range(),
            Some((-3, -3)),
            "le curseur n'a pas bougé (Unit = rang -3, voir HeaderField::index)"
        );
    }

    /// Symétrique : [`Selection::extend_header_rows_to`] ne fait rien en
    /// dehors d'une plage de champ d'en-tête entier — il faut re-cliquer un
    /// libellé de la gouttière pour repartir en plage de ce genre.
    #[test]
    fn extend_header_rows_to_has_no_effect_outside_a_header_rows_selection() {
        let column = NodeId::from_raw(1);

        let mut cell_selection = Selection::cell(column, 3);
        cell_selection.extend_header_rows_to(HeaderField::Formula);
        assert_eq!(
            cell_selection.single_cell(),
            Some((column, 3)),
            "la sélection de cellule n'a pas bougé"
        );

        let mut field_selection = Selection::header_field(column, HeaderField::LongName);
        field_selection.extend_header_rows_to(HeaderField::Formula);
        assert_eq!(
            field_selection.selected_header_field(),
            Some((column, HeaderField::LongName)),
            "le champ d'une seule colonne n'a pas basculé en champ entier"
        );
    }

    /// Une plage de champ d'en-tête entier n'est jamais une cellule unique
    /// ni un champ d'en-tête d'une seule colonne — c'est une sélection
    /// d'en-tête pur, mais qui touche par construction toutes les colonnes,
    /// distincte des deux (voir [`Selection::header_field`] pour son
    /// pendant à une colonne).
    #[test]
    fn a_header_rows_selection_is_neither_a_single_cell_nor_a_single_column_header_field() {
        let selection = Selection::header_row(HeaderField::Comment);

        assert_eq!(selection.single_cell(), None);
        assert_eq!(selection.selected_header_field(), None);
        assert_eq!(
            selection.grid_cursor(),
            None,
            "pas de « case suivante » pour un champ entier, comme pour une plage de lignes"
        );
        assert_eq!(
            selection.row_range(),
            None,
            "un champ d'en-tête entier ne touche jamais les données"
        );
    }

    /// [`Selection::full_rank_range`]/[`Selection::is_header_row_selected`]
    /// couvrent la plage quel que soit le sens du glisser, et restent
    /// strictement dans l'en-tête (rangs négatifs).
    #[test]
    fn header_rows_range_is_order_independent_and_stays_in_the_header() {
        let mut selection = Selection::header_row(HeaderField::Formula);
        selection.extend_header_rows_to(HeaderField::Unit);

        // Unit = rang -3, Formula = rang -1 (voir HeaderField::index).
        assert_eq!(selection.full_rank_range(), Some((-3, -1)));
        assert!(selection.is_header_row_selected(HeaderField::Unit));
        assert!(selection.is_header_row_selected(HeaderField::Comment));
        assert!(selection.is_header_row_selected(HeaderField::Formula));
        assert!(!selection.is_header_row_selected(HeaderField::LongName));
    }
}

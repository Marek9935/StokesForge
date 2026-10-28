//! Cell selection in a worksheet's grid.
//!
//! **Pure** UI state: never sent to the actor, never in the history (see rule 2
//! at the top of [`crate::state`]). [`crate::state::AppState`] carries one per
//! displayed worksheet, in a table it purges like the rest as
//! [`crate::state::AppState::revalidate_selection`] runs.
//!
//! # Header vocabulary
//!
//! The stacked header of a column has 5 rows: the **letter row** (A, B, C...,
//! the one that selects the whole column) then 4 **header fields** — Long Name,
//! Units, Comments, F(x)= ([`crate::state::HeaderField`]) — each one a row of its
//! own value, repeated per column.
//!
//! # A single axis, never crossed by a selection
//!
//! The 5 header fields and the data rows of the same column form a single
//! ordered axis (see [`GridCursor::rank`]): x' < Long Name < Units < Comments <
//! F(x)= < row 0 < row 1 < ... but a selection rectangle
//! ([`SelectionKind::Rows`]) always stays entirely on one side of the boundary
//! (rank -1/0): either entirely in the header, or entirely in the data —
//! **never straddling both**. A drag or a Shift-click that starts on one side
//! and aims at the other stops at the boundary instead of crossing it
//! ([`Selection::extend_row_to`]): the targeted cell is brought back to the last
//! cell on the anchor's side (the F(x)= field if the anchor is in the header,
//! row 0 if it is in the data). Three successive fixes kept alive, then grew, a
//! "mixed rectangle" that crossed this boundary; it made copy/paste
//! incomprehensible and was removed in favor of this simpler rule, decided with
//! the user. The range of whole columns ([`SelectionKind::Columns`],
//! drag/Shift-click on the letter row), the range of whole rows
//! ([`SelectionKind::FullRows`], drag/Shift-click on the row-number gutter) and
//! the range of a whole header field ([`SelectionKind::HeaderRows`],
//! drag/Shift-click on the field label in the gutter, facing "Long Name"/
//! "Units"/"Comments"/"F(x)=") each remain a distinct gesture, never mixed with
//! the rest or with one another: a drag that crosses one of them always sets a
//! plain selection instead of silently extending to no effect (see
//! [`crate::state::AppState::extend_selection`]/
//! [`crate::state::AppState::extend_header_field_selection`]/
//! [`crate::state::AppState::extend_column_selection`]/
//! [`crate::state::AppState::extend_row_selection`]/
//! [`crate::state::AppState::extend_header_row_selection`]). A range of whole rows
//! also never touches the header, and a range of a whole header field never
//! touches the data: each is the counterpart of the other zone, as `Columns` is
//! of `FullRows`.

use stokesforge_core::{NodeId, Tree};

use crate::state::{visible_columns_in, HeaderField};

/// Current selection in a worksheet's grid: a rectangle bounded to the header or
/// to the data ([`SelectionKind::Rows`], never both at once — see the module's
/// top doc), a range of whole columns ([`SelectionKind::Columns`],
/// drag/Shift-click on the letter row), a range of whole rows
/// ([`SelectionKind::FullRows`], drag/Shift-click on the row-number gutter), or
/// a range of a whole header field ([`SelectionKind::HeaderRows`],
/// drag/Shift-click on its label in the gutter) — same click-selects/
/// drag-extends/double-click-edits behavior for all, see
/// `app::grid::header_field_widget`, `app::grid::cell_widget`,
/// `app::grid::column_header_widget`, `app::grid::row_number_widget` and
/// `app::grid::header_row_label_widget`.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    kind: SelectionKind,
}

#[derive(Debug, Clone, PartialEq)]
enum SelectionKind {
    /// Rectangle of header fields or data cells (never both), between `anchor` (set
    /// at the first click, never moved afterwards) and `cursor` (the last cell
    /// aimed at — a plain click sets `cursor == anchor`, a drag or a Shift-click
    /// moves only `cursor`). `anchor` and `cursor` are always of the same kind
    /// (both a field, or both a cell): [`Selection::extend_row_to`] systematically
    /// brings `cursor` back to the boundary rather than let it change kind, see the
    /// module's top doc.
    ///
    /// `extra` carries the rectangles already set by Ctrl+click or Ctrl+drag
    /// (#159), all in the same zone (header or data); the `anchor`–`cursor`
    /// rectangle is the **last** one, the one Shift+click extends. It counts in the
    /// selection only if `active`: a Ctrl+click that removes a cell leaves the
    /// anchor on it without selecting it.
    Rows {
        extra: Vec<(GridCursor, GridCursor)>,
        anchor: GridCursor,
        cursor: GridCursor,
        active: bool,
    },
    /// Set of whole columns (#159): the columns of `base` (set one by one by
    /// Ctrl+click, or frozen by another gesture), plus, if `ranged`, the
    /// `anchor..=cursor` range (same rules as `Rows::anchor`/`cursor`, but in
    /// columns rather than cells) — drag or Shift-click on the letter row. `anchor`
    /// is the last column clicked. The order of `base` has no meaning:
    /// [`Selection::columns`] always returns the worksheet's order.
    Columns {
        base: Vec<NodeId>,
        anchor: NodeId,
        cursor: NodeId,
        ranged: bool,
    },
    /// Range of whole rows, between `anchor` and `cursor` (same rules as
    /// `Columns::anchor`/`cursor`, but in row indices rather than column
    /// identifiers) — drag or Shift-click on the row-number gutter
    /// (`app::grid::row_number_widget`). Symmetric to `Columns`: it touches all the
    /// visible columns of the worksheet and, like it, never touches the header (a
    /// whole row is a pure data selection, see the module's top doc).
    ///
    /// Like `Columns`, it is a **set** (#159): the intervals of `base` (set by
    /// Ctrl+click), plus, if `ranged`, the `anchor..=cursor` range.
    FullRows {
        base: Vec<(usize, usize)>,
        anchor: usize,
        cursor: usize,
        ranged: bool,
    },
    /// Range of a whole header field, between `anchor` and `cursor` (same rules as
    /// `FullRows::anchor`/`cursor`, but in fields rather than row indices) — drag
    /// or Shift-click on the field label in the gutter
    /// (`app::grid::header_row_label_widget`). Counterpart of `FullRows` for the
    /// header zone, as `FullRows` is of `Columns`: it touches all the visible
    /// columns of the worksheet and, symmetrically, never touches the data — it is
    /// a pure header selection.
    HeaderRows {
        anchor: HeaderField,
        cursor: HeaderField,
    },
}

/// Visible columns of `sheet`, with no range to resolve — shared by
/// [`SelectionKind::FullRows`] and [`SelectionKind::HeaderRows`], which each
/// carry all the columns of the worksheet rather than a sub-range.
fn all_visible_columns(tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
    visible_columns_in(tree, sheet)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

/// Unified starting point of keyboard navigation (arrows, Tab) and of mouse
/// selection — a grid cell, data cell or header field, on the single axis
/// described at the top of the module. `None` for a range of columns
/// ([`Selection::grid_cursor`]): the letter row has no "next cell" with an
/// obvious meaning, a cell or a field must be clicked again to resume
/// navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridCursor {
    /// Data cell `(column, row)`.
    Data(NodeId, usize),
    /// Header field `(column, field)`.
    Header(NodeId, HeaderField),
}

impl GridCursor {
    /// Targeted column, whatever the kind of cell.
    fn column(self) -> NodeId {
        match self {
            GridCursor::Data(column, _) | GridCursor::Header(column, _) => column,
        }
    }

    /// Rank on the single header-then-data axis (see the module's top doc): the 5
    /// header fields take `-5..=-1` (via [`HeaderField::rank`]), the data rows
    /// `0..` — comparable between two [`GridCursor`]s of the same column or of
    /// different columns, the column being resolved separately by position (see
    /// [`order_range`]).
    fn rank(self) -> isize {
        match self {
            GridCursor::Data(_, row) => row as isize,
            GridCursor::Header(_, field) => field.rank(),
        }
    }
}

/// Inclusive range `[lo, hi]` of ranks (see [`GridCursor::rank`]) covered by
/// `a` and `b`, whatever their order (a drag can aim upward as well as downward).
fn rank_range(a: GridCursor, b: GridCursor) -> (isize, isize) {
    let (ra, rb) = (a.rank(), b.rank());
    if ra <= rb {
        (ra, rb)
    } else {
        (rb, ra)
    }
}

/// Brings `target` back to the boundary of `anchor`'s zone if it is not of the
/// same kind — see the module's top doc. The column aimed at by `target` is
/// never bounded: a drag can cover several columns even when hitting the rank
/// boundary.
fn clamp_to_anchor_zone(anchor: GridCursor, target: GridCursor) -> GridCursor {
    match (anchor, target) {
        (GridCursor::Header(..), GridCursor::Data(column, _)) => {
            GridCursor::Header(column, HeaderField::Formula)
        }
        (GridCursor::Data(..), GridCursor::Header(column, _)) => GridCursor::Data(column, 0),
        _ => target,
    }
}

/// Columns covered by `[a, b]` in the order of `sheet` — shared between
/// [`SelectionKind::Rows`] and [`SelectionKind::Columns`], which both resolve a
/// range of columns the same way, by position in [`visible_columns_in`] rather
/// than by direct comparison of identifiers (a column can be dragged in reverse
/// order, `a` after `b`).
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

/// A rectangular block of the selection: columns (in the worksheet's order) and
/// a range of ranks `lo..=hi` (see [`GridCursor::rank`]). A multiple selection
/// (#159) carries several; copy, clear and delete reason block by block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionBlock {
    pub(crate) columns: Vec<NodeId>,
    pub(crate) lo: isize,
    pub(crate) hi: isize,
}

/// Columns and rank ranges of a copy ([`Selection::copy_layout`]).
pub(crate) type CopyLayout = (Vec<NodeId>, Vec<(isize, isize)>);

fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Sorted and merged intervals (intervals that touch make only one: `(1,2)` and
/// `(3,4)` give `(1,4)`).
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

/// `intervals` minus `point`, an interval split in two when needed.
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

/// Cell of the column of index `column` (in `order`) at rank `rank`.
fn cursor_at(order: &[NodeId], column: usize, rank: isize) -> GridCursor {
    if rank < 0 {
        GridCursor::Header(order[column], HeaderField::from_rank(rank))
    } else {
        GridCursor::Data(order[column], rank as usize)
    }
}

/// Bounds `((min column, max column), (min rank, max rank))` of the rectangle
/// `a`–`b`, columns as indices of `order`; `None` if a column has disappeared.
fn rect_bounds(
    order: &[NodeId],
    a: GridCursor,
    b: GridCursor,
) -> Option<((usize, usize), (isize, isize))> {
    let ca = order.iter().position(|&c| c == a.column())?;
    let cb = order.iter().position(|&c| c == b.column())?;
    Some((ordered(ca, cb), rank_range(a, b)))
}

/// The rectangle `a`–`b` minus the cell `(column, rank)`, as rectangles that go
/// around it (at most four); `a`–`b` as is if it does not contain it.
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
    /// Selection of a single cell — the anchor of a future rectangle.
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

    /// Selection of a single whole column — the anchor of a future range.
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

    /// Selection of **all** the given columns (in the worksheet's order), header
    /// included — the corner cell and Ctrl+A (#159). `None` for a worksheet with no
    /// column.
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

    /// Selection of a single whole row — the anchor of a future range.
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

    /// Selection of a whole header field (all the visible columns of the worksheet)
    /// — the anchor of a future range, counterpart of [`Selection::row`] for the
    /// header zone.
    pub fn header_row(field: HeaderField) -> Self {
        Self {
            kind: SelectionKind::HeaderRows {
                anchor: field,
                cursor: field,
            },
        }
    }

    /// Selection of a single header field **of a specific column** — the anchor of
    /// a future range. Distinct from [`Selection::header_row`], which touches all
    /// the columns at once: this one touches only one, and a drag can then extend
    /// it to other columns (see [`Selection::extend_row_to`]) without ever covering
    /// *all* the columns in one gesture — the two widgets that set one or the other
    /// are distinct (`app::grid::header_field_widget` for this one,
    /// `app::grid::header_row_label_widget` for [`Selection::header_row`]).
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

    /// Ctrl+click on a cell or a header field (#159): adds the cell to the
    /// selection, or removes it (the rectangle that contains it is split around
    /// it), and puts the anchor there. `false` if the selection is not a set of
    /// cells of the same zone (header or data): the caller then sets a plain
    /// selection.
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

    /// Ctrl+drag (#159): opens a new rectangle at `target`, without touching the
    /// previous ones; Shift+click and the drag then extend it. `false` in the same
    /// cases as [`Selection::toggle_cell`].
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

    /// Extends a range of columns up to `column`, keeping the anchor set by
    /// [`Selection::column`] — drag or Shift-click on the letter row. No effect if
    /// the current selection is not already a range of columns.
    ///
    /// The columns already set by Ctrl+click remain: this is the additive gesture
    /// (drag, Ctrl+Shift+click). For a plain Shift+click, which replaces everything
    /// with the anchor → `column` range, see [`Selection::extend_columns_replacing`].
    pub fn extend_columns_to(&mut self, column: NodeId) {
        if let SelectionKind::Columns { cursor, ranged, .. } = &mut self.kind {
            *cursor = column;
            *ranged = true;
        }
    }

    /// Shift+click: the selection becomes exactly the anchor → `column` range, the
    /// columns set one by one disappear.
    pub fn extend_columns_replacing(&mut self, column: NodeId) {
        if let SelectionKind::Columns { base, .. } = &mut self.kind {
            base.clear();
        }
        self.extend_columns_to(column);
    }

    /// Ctrl+click: toggles the membership of `column` and moves the anchor onto it.
    /// No effect if the selection is not a set of columns (the caller then sets
    /// [`Selection::column`]).
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

    /// Copy layout (#159): the columns (worksheet order) and the rank ranges to
    /// copy, end to end, in the worksheet's order. Blocks that share the **same
    /// rows** go side by side, those that share the **same columns** are stacked;
    /// otherwise `None`: the copy would be ambiguous (Copy and Cut grayed out). A
    /// single block, like a set of whole columns, is always copyable (`None` too
    /// for the latter: it is copied by column, without ranks).
    pub(crate) fn copy_layout(&self, tree: &Tree, sheet: NodeId) -> Option<CopyLayout> {
        let blocks = self.blocks(tree, sheet);
        let first = blocks.first()?;
        if blocks.iter().all(|b| (b.lo, b.hi) == (first.lo, first.hi)) {
            return Some((self.columns(tree, sheet), vec![(first.lo, first.hi)]));
        }
        if blocks.iter().all(|b| b.columns == first.columns) {
            // Ranks shifted by `HeaderField::COUNT` to merge as unsigned (header fields
            // take `-COUNT..=-1`).
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

    /// Shift+click on a row number: the selection becomes exactly the anchor →
    /// `row` range, the rows set with Ctrl+click disappear.
    pub fn extend_full_rows_replacing(&mut self, row: usize) {
        if let SelectionKind::FullRows { base, .. } = &mut self.kind {
            base.clear();
        }
        self.extend_full_rows_to(row);
    }

    /// Ctrl+click on a row number (#159): toggles the membership of `row` and puts
    /// the anchor there. No effect if the selection is not a set of rows.
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

    /// Row intervals of a set of whole rows, sorted and merged; empty for any other
    /// kind of selection.
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

    /// Cell rectangles of a set of cells, the last one included if it is active;
    /// empty for any other kind of selection.
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

    /// The rectangular blocks of the selection (see [`SelectionBlock`]); empty for a
    /// set of whole columns, which is read through [`Selection::columns`].
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

    /// Intervals of **data** rows touched, sorted and merged — without the tree: a
    /// row does not need the columns to know whether it is touched.
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

    /// Extends a range of a whole header field up to `field`, keeping the anchor
    /// set by [`Selection::header_row`] — drag or Shift-click on a field label in
    /// the gutter. No effect if the current selection is not already a range of
    /// this kind.
    pub fn extend_header_rows_to(&mut self, field: HeaderField) {
        if let SelectionKind::HeaderRows { cursor, .. } = &mut self.kind {
            *cursor = field;
        }
    }

    /// True if the selection is a range of whole columns (letter row) — lets
    /// [`crate::state::AppState::extend_selection`]/
    /// [`crate::state::AppState::extend_header_field_selection`] tell a normal
    /// extension (within the current rectangle, bounded to the header or to the
    /// data) from a boundary crossing from the letter row, which must set a plain
    /// selection instead of silently extending to no effect; and lets
    /// [`crate::state::AppState::extend_column_selection`] do the opposite.
    pub fn is_columns(&self) -> bool {
        matches!(self.kind, SelectionKind::Columns { .. })
    }

    /// True if the selection is a range of whole rows (row-number gutter) — same
    /// role as [`Selection::is_columns`], but for the symmetric boundary: lets
    /// [`crate::state::AppState::extend_selection`]/
    /// [`crate::state::AppState::extend_header_field_selection`] set a plain
    /// selection instead of silently extending to no effect when a drag starts from
    /// the gutter, and lets [`crate::state::AppState::extend_row_selection`] do the
    /// opposite.
    pub fn is_full_rows(&self) -> bool {
        matches!(self.kind, SelectionKind::FullRows { .. })
    }

    /// True if the selection is a range of a whole header field (field label in the
    /// gutter) — same role as [`Selection::is_full_rows`], on the header side: lets
    /// [`crate::state::AppState::extend_header_field_selection`] set a plain
    /// selection instead of silently extending to no effect when a drag starts from
    /// the gutter, and lets [`crate::state::AppState::extend_header_row_selection`]
    /// do the opposite.
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

    /// Unified starting point of keyboard navigation — the last cell aimed at, data
    /// cell or header field. `None` for a range of columns: the letter row has no
    /// "next cell" with an obvious meaning, a cell or a field must be clicked again
    /// to resume navigation. Same for a range of whole rows or of a whole header
    /// field: neither has a column to offer as the next cell. Used both by
    /// [`crate::state::AppState::move_selection`] (arrows) and by
    /// [`crate::state::AppState::move_selection_tab`] (Tab/Shift+Tab), which both
    /// cross the header/data boundary.
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

    /// The rows of a selection of **whole rows**, ascending and without duplicates
    /// — empty for any other selection (#166): only the row-number gutter gesture
    /// designates rows.
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

    /// True if `column` is part of the selected range of columns (click, drag or
    /// Shift-click on the letter row) — distinct from [`Selection::contains`],
    /// which tests a specific cell. False for a range of whole rows or of a whole
    /// header field: it is not the same selection, even if it ends up touching the
    /// same columns (see [`Selection::is_row_selected`]/
    /// [`Selection::is_header_row_selected`] for their respective counterparts).
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

    /// True if `field` is part of the selected range of a whole header field (click,
    /// drag or Shift-click on the field label in the gutter) — counterpart of
    /// [`Selection::is_row_selected`], on the header side, for the highlight of
    /// `app::grid::header_row_label_widget`. Takes neither `tree` nor `sheet`, for
    /// the same reason as [`Selection::is_row_selected`].
    pub fn is_header_row_selected(&self, field: HeaderField) -> bool {
        match &self.kind {
            SelectionKind::HeaderRows { .. } => self.full_rank_range().is_some_and(|(lo, hi)| {
                let rank = field.rank();
                rank >= lo && rank <= hi
            }),
            _ => false,
        }
    }

    /// True if `column` is touched by the current selection, **whatever its kind**
    /// — unlike [`Selection::is_column_selected`], which holds only for a direct
    /// click on the letter row, this also holds for a plain rectangle of data cells
    /// that `column` is part of. Used to tint the letter row with an intermediate
    /// intensity (`stokesforge_app::theme::header_letter_row_touched_fill`) when a
    /// cell of its column is selected without the column itself being so — the
    /// "your column is concerned" marker of a spreadsheet.
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
            // A single data block: exactly its rows are removed. A multiple selection
            // (#159) would leave holes: moving up would have no unambiguous meaning,
            // Delete just clears.
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

    /// [`Selection::single_cell`] — lock of bug 1 (typing on a selected cell starts
    /// editing): a plain click stays reduced to a single cell, but extending it —
    /// even up to itself via a Shift-click or a drag with no real movement — must
    /// never again be treated as a single cell once `extend_row_to` has been called,
    /// since the gesture that produced the selection is then no longer "a plain
    /// click on a cell". A single header field is likewise never `single_cell` (see
    /// [`Selection::selected_header_field`] for its counterpart).
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
            "a rectangle of several rows is not a single cell"
        );

        let mut extended_columns = Selection::cell(column, 3);
        extended_columns.extend_row_to(GridCursor::Data(other_column, 3));
        assert_eq!(
            extended_columns.single_cell(),
            None,
            "a rectangle of several columns is not a single cell"
        );

        assert_eq!(
            Selection::column(column).single_cell(),
            None,
            "a range of columns is never a single cell"
        );

        assert_eq!(
            Selection::header_field(column, HeaderField::Comment).single_cell(),
            None,
            "a single header field is not a single cell"
        );
    }

    /// [`Selection::extend_row_to`] — a drag that starts from a header field and
    /// aims at a data cell stops at the F(x)= field of the targeted column, never in
    /// the data itself: see the module's top doc.
    #[test]
    fn extending_from_a_header_field_into_data_stops_at_the_formula_field() {
        let column = NodeId::from_raw(1);
        let other_column = NodeId::from_raw(2);

        let mut selection = Selection::header_field(column, HeaderField::Unit);
        selection.extend_row_to(GridCursor::Data(other_column, 5));

        assert_eq!(
            selection.grid_cursor(),
            Some(GridCursor::Header(other_column, HeaderField::Formula)),
            "the drag stops at the last header field of the targeted column"
        );
    }

    /// Symmetric: a drag that starts from a data cell and aims at a header field
    /// stops at row 0 of the targeted column, never in the header itself.
    #[test]
    fn extending_from_data_into_a_header_field_stops_at_row_zero() {
        let column = NodeId::from_raw(1);
        let other_column = NodeId::from_raw(2);

        let mut selection = Selection::cell(column, 5);
        selection.extend_row_to(GridCursor::Header(other_column, HeaderField::Comment));

        assert_eq!(
            selection.grid_cursor(),
            Some(GridCursor::Data(other_column, 0)),
            "the drag stops at row 0 of the targeted column"
        );
    }

    /// [`Selection::extend_row_to`] does nothing on a range of whole rows: it is not
    /// a rectangle of this kind. Lock of the boundary non-crossing, symmetric to
    /// [`Selection::extend_columns_to`] (already locked on the column side by the
    /// `AppState` tests in `tests/shell.rs`) — a drag that, starting from the
    /// gutter, would cross a cell must never turn a `FullRows` into a rectangle of
    /// cells.
    #[test]
    fn extend_row_to_has_no_effect_on_a_full_rows_selection() {
        let column = NodeId::from_raw(1);
        let mut selection = Selection::row(2);

        selection.extend_row_to(GridCursor::Data(column, 5));

        assert!(
            selection.is_full_rows(),
            "the selection stays a range of whole rows"
        );
        assert_eq!(
            selection.row_range(),
            Some((2, 2)),
            "the cursor did not move"
        );
    }

    /// Symmetric: [`Selection::extend_full_rows_to`] does nothing on a rectangle of
    /// cells or a range of columns — the gutter must be clicked again to start over
    /// as a range of rows.
    #[test]
    fn extend_full_rows_to_has_no_effect_outside_a_full_rows_selection() {
        let column = NodeId::from_raw(1);

        let mut cell_selection = Selection::cell(column, 3);
        cell_selection.extend_full_rows_to(7);
        assert_eq!(
            cell_selection.single_cell(),
            Some((column, 3)),
            "the cell selection did not move"
        );

        let mut column_selection = Selection::column(column);
        column_selection.extend_full_rows_to(7);
        assert!(
            column_selection.is_columns(),
            "the column range did not switch to a row range"
        );
    }

    /// A range of whole rows is never a single cell nor a selected header field —
    /// it is a pure data selection, distinct from both (see the module's top doc).
    #[test]
    fn a_full_rows_selection_is_neither_a_single_cell_nor_a_header_field() {
        let selection = Selection::row(4);

        assert_eq!(selection.single_cell(), None);
        assert_eq!(selection.selected_header_field(), None);
        assert_eq!(
            selection.grid_cursor(),
            None,
            "no \"next cell\" for a range of rows, as for a range of columns"
        );
    }

    /// [`Selection::row_range`]/[`Selection::is_row_selected`] cover the range
    /// whatever the direction of the drag (anchor after the cursor).
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
    /// [`Selection::extend_full_rows_to`] do nothing on a range of a whole header
    /// field — lock of the boundary non-crossing, symmetric to
    /// [`extend_row_to_has_no_effect_on_a_full_rows_selection`]: a drag that,
    /// starting from a field label in the gutter, would cross a cell, a column or
    /// the row-number gutter must never turn a `HeaderRows` into another kind of
    /// selection.
    #[test]
    fn no_other_extend_method_touches_a_header_rows_selection() {
        let column = NodeId::from_raw(1);
        let mut selection = Selection::header_row(HeaderField::Unit);

        selection.extend_row_to(GridCursor::Data(column, 5));
        selection.extend_columns_to(column);
        selection.extend_full_rows_to(3);

        assert!(
            selection.is_header_rows(),
            "the selection stays a range of a whole header field"
        );
        assert_eq!(
            selection.full_rank_range(),
            Some((-3, -3)),
            "the cursor did not move (Unit = rank -3, see HeaderField::index)"
        );
    }

    /// Symmetric: [`Selection::extend_header_rows_to`] does nothing outside a range
    /// of a whole header field — a gutter label must be clicked again to start over
    /// as a range of this kind.
    #[test]
    fn extend_header_rows_to_has_no_effect_outside_a_header_rows_selection() {
        let column = NodeId::from_raw(1);

        let mut cell_selection = Selection::cell(column, 3);
        cell_selection.extend_header_rows_to(HeaderField::Formula);
        assert_eq!(
            cell_selection.single_cell(),
            Some((column, 3)),
            "the cell selection did not move"
        );

        let mut field_selection = Selection::header_field(column, HeaderField::LongName);
        field_selection.extend_header_rows_to(HeaderField::Formula);
        assert_eq!(
            field_selection.selected_header_field(),
            Some((column, HeaderField::LongName)),
            "the single-column field did not switch to a whole field"
        );
    }

    /// A range of a whole header field is never a single cell nor a header field of
    /// a single column — it is a pure header selection, but one that touches all
    /// the columns by construction, distinct from both (see
    /// [`Selection::header_field`] for its single-column counterpart).
    #[test]
    fn a_header_rows_selection_is_neither_a_single_cell_nor_a_single_column_header_field() {
        let selection = Selection::header_row(HeaderField::Comment);

        assert_eq!(selection.single_cell(), None);
        assert_eq!(selection.selected_header_field(), None);
        assert_eq!(
            selection.grid_cursor(),
            None,
            "no \"next cell\" for a whole field, as for a range of rows"
        );
        assert_eq!(
            selection.row_range(),
            None,
            "a whole header field never touches the data"
        );
    }

    /// [`Selection::full_rank_range`]/[`Selection::is_header_row_selected`] cover
    /// the range whatever the direction of the drag, and stay strictly in the
    /// header (negative ranks).
    #[test]
    fn header_rows_range_is_order_independent_and_stays_in_the_header() {
        let mut selection = Selection::header_row(HeaderField::Formula);
        selection.extend_header_rows_to(HeaderField::Unit);

        // Unit = rank -3, Formula = rank -1 (see HeaderField::index).
        //
        assert_eq!(selection.full_rank_range(), Some((-3, -1)));
        assert!(selection.is_header_row_selected(HeaderField::Unit));
        assert!(selection.is_header_row_selected(HeaderField::Comment));
        assert!(selection.is_header_row_selected(HeaderField::Formula));
        assert!(!selection.is_header_row_selected(HeaderField::LongName));
    }
}

//! Composition of a graph from the columns of a workbook (#257): the
//! rules that read the [`Tree`] and build [`SeriesSpec`]s or read a
//! [`PlotSpec`], with no interface and no sentence.
//!
//! They used to live in `stokesforge-app/src/state.rs`, where `pyst` and the CLI
//! could not see them and redid part of them by hand (the palette, the
//! search for the abscissa). They are here so that **the interface, `pyst` and
//! the analyses apply the same rule**:
//!
//! - **pairing**, OriginLab style: each Y column is paired with the
//!   nearest X column to its **left** in the sheet (role
//!   [`ColumnRole::X`]), including unselected X columns; a Yn
//!   (Y2, Y3, Y4) follows the same rule with the Xn columns ([`x_for`],
//!   [`series_from_columns`]). With no X to the left, the abscissa stays implicit
//!   (row number);
//! - **error pairing** (#360): the first column of role
//!   [`ColumnRole::YError`] (resp. [`ColumnRole::XError`]) to the **right** of
//!   a Y column (Y2–Y4 too), before the next ordinate column (any of Y, Y2,
//!   Y3, Y4), fills `y_error` (`x_error`), selected or not ([`errors_for`]);
//! - **palette**: the color of a series is that of its rank
//!   ([`SeriesSpec::from_palette`]), never written by hand;
//! - **layers**: the ordinary layer that an overlay layer covers
//!   ([`overlay_base`]) and the Xn/Yn role layer of a base layer
//!   ([`role_layer_of`]);
//! - **reading the spec**: [`current_plot_spec`], [`panel_plot_type`],
//!   [`spec_has_x`].
//!
//! This module mutates nothing: it builds values that the caller sends through
//! an undoable command (invariant 1). It produces no text:
//! the interface passes the translated name of the "row" series as a parameter
//! ([`row_series`]).

use std::collections::BTreeMap;

use stokesforge_id::NodeId;
use stokesforge_plot::{
    FreeFrame, LayerPlacement, OverlaySpec, PanelSpec, PlotSpec, PlotType, SeriesSpec,
};

use crate::column::ColumnRole;
use crate::formula::Formula;
use crate::node::{Node, NodeKind};
use crate::tree::Tree;

/// The series to plot for a selection of columns, split by layer
/// (#167, #168): the main layer and the overlay layers of the roles
/// X2/Y2, X3/Y3 and X4/Y4.
#[derive(Debug, Default)]
pub struct PlannedSeries {
    /// Y-role series, on the main axes.
    pub primary: Vec<SeriesSpec>,
    /// Y2, Y3 and Y4 role series (in that order): each on the overlay
    /// layer of its number (X axis on top, Y axis on the right).
    pub overlays: [Vec<SeriesSpec>; OVERLAY_ROLES],
}

/// Number of overlay layers fed by the Xn/Yn roles (X2…X4).
pub const OVERLAY_ROLES: usize =
    (OverlaySpec::LAST_ROLE_LAYER - OverlaySpec::FIRST_ROLE_LAYER + 1) as usize;

impl PlannedSeries {
    /// No series, neither main nor overlay.
    pub fn is_empty(&self) -> bool {
        self.primary.is_empty() && self.overlays.iter().all(Vec::is_empty)
    }

    /// The series of all overlay layers, from layer 2 to layer 4.
    pub fn overlay_series(&self) -> impl Iterator<Item = &SeriesSpec> {
        self.overlays.iter().flatten()
    }
}

/// The columns of `sheet`, in sheet order (empty if `sheet` is
/// not a node of the tree).
fn sheet_columns(tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
    tree.node(sheet)
        .map(Node::children)
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|id| {
            tree.node(*id)
                .is_some_and(|node| node.as_column().is_some())
        })
        .collect()
}

fn role_of(tree: &Tree, id: NodeId) -> Option<ColumnRole> {
    tree.node(id)
        .and_then(Node::as_column)
        .map(|column| column.meta().role)
}

/// The column of role `x_role` nearest **to the left** of the column at
/// rank `position` in `columns`.
fn nearest_left(
    tree: &Tree,
    columns: &[NodeId],
    position: usize,
    x_role: ColumnRole,
) -> Option<NodeId> {
    columns[..position]
        .iter()
        .rev()
        .copied()
        .find(|id| role_of(tree, *id) == Some(x_role))
}

/// The X column paired with column `y`: the X (Xn if `y` has role Yn) nearest
/// to its left in its sheet, `None` with no X to the left or if `y`
/// is not a column of a sheet. It is the rule of
/// [`series_from_columns`], for whoever wants only the abscissa (attachment of an
/// analysis, `pyst.plot(y)` without `x=`).
pub fn x_for(tree: &Tree, y: NodeId) -> Option<NodeId> {
    let sheet = tree.node(y).and_then(Node::parent)?;
    let columns = sheet_columns(tree, sheet);
    let position = columns.iter().position(|id| *id == y)?;
    let x_role = role_of(tree, y)?.paired_x().unwrap_or(ColumnRole::X);
    nearest_left(tree, &columns, position, x_role)
}

/// The X column of each column a formula reads the X of
/// ([`Formula::columns_needing_x`]), by the rule of [`x_for`] (#402). A column
/// with no X to its left is left out of the map: the formula computes as
/// [`crate::formula::FormulaError::NoPairedX`].
///
/// One function for the recalculation (fingerprint, dependency order) and for
/// the message of the formula cell: they cannot disagree on the X.
pub fn paired_x_columns(tree: &Tree, formula: &Formula) -> BTreeMap<NodeId, NodeId> {
    formula
        .columns_needing_x()
        .into_iter()
        .filter_map(|y| x_for(tree, y).map(|x| (y, x)))
        .collect()
}

/// The first column of role `error_role` to the **right** of the column at
/// rank `position` in `columns`, before the next ordinate column (Y, Y2, Y3
/// or Y4).
fn first_error_right(
    tree: &Tree,
    columns: &[NodeId],
    position: usize,
    error_role: ColumnRole,
) -> Option<NodeId> {
    for id in &columns[position + 1..] {
        match role_of(tree, *id) {
            Some(role) if role == error_role => return Some(*id),
            Some(ColumnRole::Y | ColumnRole::Y2 | ColumnRole::Y3 | ColumnRole::Y4) => return None,
            _ => {}
        }
    }
    None
}

/// The error columns paired with column `y`, as `(x_error, y_error)`: the first
/// X Error (resp. Y Error) column to its right in its sheet, before the next
/// ordinate column (#360). `(None, None)` if `y` is not a column of a sheet
/// or is not an ordinate. It is the rule of [`series_from_columns`], for
/// whoever wants only the error columns (`pyst.plot(y)`).
pub fn errors_for(tree: &Tree, y: NodeId) -> (Option<NodeId>, Option<NodeId>) {
    let Some(sheet) = tree.node(y).and_then(Node::parent) else {
        return (None, None);
    };
    let columns = sheet_columns(tree, sheet);
    let Some(position) = columns.iter().position(|id| *id == y) else {
        return (None, None);
    };
    if role_of(tree, y).and_then(ColumnRole::paired_x).is_none() {
        return (None, None);
    }
    (
        first_error_right(tree, &columns, position, ColumnRole::XError),
        first_error_right(tree, &columns, position, ColumnRole::YError),
    )
}

/// Builds the series of the selection `columns` of `sheet`: the columns of
/// role Y (or Y2, Y3, Y4) present in `columns`, in the order they appear
/// in the sheet, each paired with its X ([`x_for`]) — including an unselected
/// X. The Yn go into [`PlannedSeries::overlays`].
///
/// Each series takes the palette color of its rank, `start_index` included
/// (`overlay_starts` for the overlay layers): an offset useful when adding
/// series to a graph, which continues the palette after the series already
/// present instead of restarting from zero. Returns empty lists if
/// `columns` holds no Y or Y2 — this is what callers read
/// to refuse an empty graph or an empty addition.
pub fn series_from_columns(
    tree: &Tree,
    sheet: NodeId,
    columns: &[NodeId],
    plot_type: &PlotType,
    start_index: usize,
    overlay_starts: [usize; OVERLAY_ROLES],
) -> PlannedSeries {
    let ordered = sheet_columns(tree, sheet);
    let mut planned = PlannedSeries::default();
    for (position, column) in ordered.iter().enumerate() {
        if !columns.contains(column) {
            continue;
        }
        let Some(role) = role_of(tree, *column) else {
            continue;
        };
        let (list, start) = match role {
            ColumnRole::Y => (&mut planned.primary, start_index),
            ColumnRole::Y2 | ColumnRole::Y3 | ColumnRole::Y4 => {
                let slot = role.layer().map_or(0, |layer| {
                    usize::from(layer - OverlaySpec::FIRST_ROLE_LAYER)
                });
                (&mut planned.overlays[slot], overlay_starts[slot])
            }
            _ => continue,
        };
        let x_role = role.paired_x().unwrap_or(ColumnRole::X);
        let mut spec = SeriesSpec::from_palette(*column, start + list.len());
        spec.x = nearest_left(tree, &ordered, position, x_role);
        spec.x_error = first_error_right(tree, &ordered, position, ColumnRole::XError);
        spec.y_error = first_error_right(tree, &ordered, position, ColumnRole::YError);
        spec.plot_type = plot_type.clone();
        list.push(spec);
    }
    planned
}

/// The "row" series (#166) of `rows`, anchored on `anchor`, with the
/// palette colors from rank `start`. `name` gives the name of the
/// series of a row (0 = the first): an interface sentence, which this crate
/// cannot write.
pub fn row_series(
    anchor: NodeId,
    rows: &[usize],
    start: usize,
    plot_type: &PlotType,
    name: impl Fn(usize) -> String,
) -> Vec<SeriesSpec> {
    rows.iter()
        .enumerate()
        .map(|(offset, &row)| {
            let mut series = SeriesSpec::from_palette(anchor, start + offset);
            series.row = Some(row);
            series.name = name(row);
            series.plot_type = plot_type.clone();
            series
        })
        .collect()
}

/// The columns of `sheet` that carry a finite x' value, in sheet
/// order (#164, #166).
pub fn x_prime_columns(tree: &Tree, sheet: NodeId) -> Vec<NodeId> {
    sheet_columns(tree, sheet)
        .into_iter()
        .filter(|id| {
            tree.node(*id)
                .and_then(Node::as_column)
                .is_some_and(|column| column.meta().x_prime.is_some_and(f64::is_finite))
        })
        .collect()
}

/// Does the graph already have an X: a series whose abscissa comes from a column
/// of role X, or a "row" series (abscissa = x')? Decides whether dropping
/// rows raises the axis question (#166).
pub fn spec_has_x(spec: &PlotSpec) -> bool {
    spec.panels
        .iter()
        .flat_map(|panel| &panel.series)
        .any(|series| series.x.is_some() || series.row.is_some())
}

/// The ordinary layer that `panel` covers (itself if it is ordinary):
/// the last ordinary layer that precedes it in its cell.
pub fn overlay_base(spec: &PlotSpec, panel: usize) -> usize {
    let Some(target) = spec.panels.get(panel) else {
        return panel;
    };
    if target.overlay.is_none() {
        return panel;
    }
    (0..panel)
        .rev()
        .find(|index| {
            spec.panels.get(*index).is_some_and(|candidate| {
                candidate.overlay.is_none()
                    && (candidate.row, candidate.column) == (target.row, target.column)
            })
        })
        .unwrap_or(panel)
}

/// The index of the role layer numbered `layer` (2 to 4: X axis on top, Y axis
/// on the right, [`OverlaySpec::role_layer`]) placed on the ordinary layer `base`,
/// if it exists.
pub fn role_layer_of(spec: &PlotSpec, base: usize, layer: u8) -> Option<usize> {
    let cell = spec
        .panels
        .get(base)
        .map(|panel| (panel.row, panel.column))?;
    (base + 1..spec.panels.len()).find(|index| {
        spec.panels.get(*index).is_some_and(|panel| {
            panel.overlay == Some(OverlaySpec::role_layer(layer))
                && (panel.row, panel.column) == cell
        })
    })
}

/// Maximum number of overlay layers of a cell that "New Layer" allows
/// adding: as many as there are Xn/Yn roles (#168), "row" layers included.
pub const MAX_OVERLAYS: usize = OVERLAY_ROLES;

/// The overlay layers of the cell of layer `panel`.
fn overlays_in_cell(spec: &PlotSpec, panel: usize) -> impl Iterator<Item = &PanelSpec> {
    let cell = spec
        .panels
        .get(panel)
        .map(|target| (target.row, target.column));
    spec.panels.iter().filter(move |candidate| {
        candidate.overlay.is_some() && Some((candidate.row, candidate.column)) == cell
    })
}

/// Does the cell of layer `panel` already hold [`MAX_OVERLAYS`] overlay
/// layers? Then "New Layer" is greyed out.
pub fn overlays_full(spec: &PlotSpec, panel: usize) -> bool {
    overlays_in_cell(spec, panel).count() >= MAX_OVERLAYS
}

/// The smallest role layer number (2 to 4) that no layer of the cell
/// of layer `panel` holds: that of a new two-axis layer (#168).
/// `None` if all three are taken.
pub fn free_role_layer(spec: &PlotSpec, panel: usize) -> Option<u8> {
    (OverlaySpec::FIRST_ROLE_LAYER..=OverlaySpec::LAST_ROLE_LAYER).find(|layer| {
        !overlays_in_cell(spec, panel).any(|candidate| {
            candidate
                .overlay
                .is_some_and(|overlay| overlay.layer == *layer)
        })
    })
}

/// Largest grid (rows x columns) of cells: the one the renderer lays out
/// as declared (`stokesforge_render::pipeline::MAX_CELLS`; core cannot depend
/// on the renderer). Beyond it the renderer falls back to declaration
/// order, so adding a cell past it is refused.
pub const MAX_GRID_CELLS: usize = 256;

/// Where a new cell goes relative to a reference cell (#357).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellSide {
    /// In the same row, one column to the right.
    Right,
    /// In the same column, one row below.
    Below,
}

/// Adds an empty cell beside the cell of layer `panel` (#357) and returns the
/// index of its layer (always the last one: existing indices do not move).
///
/// A **whole grid column** (Right) or **row** (Below) is inserted (#394): every
/// layer on the far side of the reference shifts one column (row) further,
/// whatever its row (column), with its overlay layers; the other rows (columns)
/// get a hole where the new cell is. The new column (row) takes the frame width
/// (height) of the reference cell, so the new plot area is exactly the
/// reference's and the figure grows by the new cell's bands and frame. A graph
/// without stored frames (legacy, empty vectors) stays so: core does not
/// measure, the application materializes the frames before editing.
/// `None`, with the spec unchanged, if `panel` does not exist, if a position
/// would overflow (rows and columns are free `usize`s once read from a file)
/// or if the grid would exceed [`MAX_GRID_CELLS`].
pub fn insert_cell(spec: &mut PlotSpec, panel: usize, side: CellSide) -> Option<usize> {
    if spec.layer_placement == LayerPlacement::Free {
        return insert_free_cell(spec, panel, side);
    }
    let base = spec.panels.get(overlay_base(spec, panel))?;
    let (row, column) = (base.row, base.column);
    let (new_row, new_column) = match side {
        CellSide::Right => (row, column.checked_add(1)?),
        CellSide::Below => (row.checked_add(1)?, column),
    };
    let shifts = |candidate: &PanelSpec| match side {
        CellSide::Right => candidate.column > column,
        CellSide::Below => candidate.row > row,
    };
    // Checks everything before touching the spec.
    let mut rows = new_row.checked_add(1)?;
    let mut columns = new_column.checked_add(1)?;
    for candidate in spec.panels.iter() {
        let (r, c) = if shifts(candidate) {
            match side {
                CellSide::Right => (candidate.row, candidate.column.checked_add(1)?),
                CellSide::Below => (candidate.row.checked_add(1)?, candidate.column),
            }
        } else {
            (candidate.row, candidate.column)
        };
        rows = rows.max(r.checked_add(1)?);
        columns = columns.max(c.checked_add(1)?);
    }
    if rows.checked_mul(columns)? > MAX_GRID_CELLS {
        return None;
    }
    // The sizes, read before the positions move.
    let widths = spec.sanitized_frame_widths();
    let heights = spec.sanitized_frame_heights();
    for candidate in spec.panels.iter_mut().filter(|candidate| shifts(candidate)) {
        match side {
            CellSide::Right => candidate.column += 1,
            CellSide::Below => candidate.row += 1,
        }
    }
    match side {
        CellSide::Right => {
            if let Some(mut widths) = widths {
                widths.insert(new_column, widths[column]);
                spec.frame_widths = widths;
            }
        }
        CellSide::Below => {
            if let Some(mut heights) = heights {
                heights.insert(new_row, heights[row]);
                spec.frame_heights = heights;
            }
        }
    }
    spec.panels.push(PanelSpec::at(new_row, new_column));
    Some(spec.panels.len() - 1)
}

/// [`insert_cell`] in free placement (#398): nothing shifts and no frame size
/// is stored per column, the new cell only needs an identity no other cell
/// has: past the last column of the reference row (Right) or the last row of
/// the reference column (Below). The caller gives it its frame
/// ([`set_free_frame`]).
fn insert_free_cell(spec: &mut PlotSpec, panel: usize, side: CellSide) -> Option<usize> {
    let base = spec.panels.get(spec.cell_bases().get(panel).copied()?)?;
    let (row, column) = (base.row, base.column);
    let (new_row, new_column) = match side {
        CellSide::Right => (
            row,
            spec.panels
                .iter()
                .filter(|p| p.row == row)
                .map(|p| p.column)
                .max()?
                .checked_add(1)?,
        ),
        CellSide::Below => (
            spec.panels
                .iter()
                .filter(|p| p.column == column)
                .map(|p| p.row)
                .max()?
                .checked_add(1)?,
            column,
        ),
    };
    let rows = spec.rows().max(new_row.checked_add(1)?);
    let columns = spec.columns().max(new_column.checked_add(1)?);
    if rows.checked_mul(columns)? > MAX_GRID_CELLS {
        return None;
    }
    // The new cell is in front of every other one (#398).
    let front = spec
        .panels
        .iter()
        .filter_map(|p| p.free_frame)
        .map(|frame| frame.stack)
        .max()
        .map_or(0, |top| top.saturating_add(1));
    spec.panels.push(PanelSpec {
        free_frame: Some(FreeFrame {
            stack: front,
            ..FreeFrame::FALLBACK
        }),
        ..PanelSpec::at(new_row, new_column)
    });
    Some(spec.panels.len() - 1)
}

/// Which dimension of the grid a frame size belongs to (#394).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAxis {
    /// The width of the frame of a grid column.
    Column,
    /// The height of the frame of a grid row.
    Row,
}

/// Sets the frame width of grid column `index` or the frame height of grid row
/// `index` to `size` points, brought into `[MIN_FRAME_SIZE, MAX_FRAME_SIZE]`
/// (#394). The other columns and rows keep their frame, so the figure grows or
/// shrinks by the difference. `false`, with the spec unchanged, if the size is
/// not finite, the index is outside the grid or the graph is legacy (no stored
/// frames: the application materializes them first, core does not measure).
pub fn set_frame_size(spec: &mut PlotSpec, axis: FrameAxis, index: usize, size: f64) -> bool {
    if !size.is_finite() {
        return false;
    }
    let sizes = match axis {
        FrameAxis::Column => spec.sanitized_frame_widths(),
        FrameAxis::Row => spec.sanitized_frame_heights(),
    };
    let Some(mut sizes) = sizes.filter(|sizes| index < sizes.len()) else {
        return false;
    };
    sizes[index] = size.clamp(
        stokesforge_plot::MIN_FRAME_SIZE,
        stokesforge_plot::MAX_FRAME_SIZE,
    );
    match axis {
        FrameAxis::Column => spec.frame_widths = sizes,
        FrameAxis::Row => spec.frame_heights = sizes,
    }
    true
}

/// Which offset of the first frame from the edge of the image (#405).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FigureOffset {
    /// Left edge of the image to the frame of the first column.
    Left,
    /// Top edge of the image (graph title included) to the frame of the first
    /// row.
    Top,
}

/// Sets Left or Top of the figure to `value` points (`None`: Auto), brought
/// into `[0, MAX_FRAME_SIZE]` (#405). `false`, with the spec unchanged, if the
/// value is not finite or negative, or the graph is legacy (no stored frames:
/// the application materializes them first, core does not measure).
pub fn set_figure_offset(spec: &mut PlotSpec, which: FigureOffset, value: Option<f64>) -> bool {
    if spec.frame_widths.is_empty() && spec.frame_heights.is_empty() {
        return false;
    }
    let value = match value {
        Some(v) if !v.is_finite() || v < 0.0 => return false,
        Some(v) => Some(v.min(stokesforge_plot::MAX_FRAME_SIZE)),
        None => None,
    };
    match which {
        FigureOffset::Left => spec.figure_left = value,
        FigureOffset::Top => spec.figure_top = value,
    }
    true
}

/// Sets "Figure size = Fixed" (#405). `false`, with the spec unchanged, if the
/// graph is legacy (the application materializes the frames first).
pub fn set_fixed_figure(spec: &mut PlotSpec, fixed: bool) -> bool {
    if spec.frame_widths.is_empty() && spec.frame_heights.is_empty() {
        return false;
    }
    spec.fixed_figure = fixed;
    true
}

/// Puts the graph in free placement (#398) with `frames`, one entry per layer
/// in spec order: the frame of each ordinary layer, `None` for an overlaid one
/// (it uses its base's; a frame given to it is dropped). `false`, with the spec
/// unchanged, if `frames` does not have one entry per layer. The grid fields
/// ([`PlotSpec::frame_widths`], `figure_left`…) stay stored, ignored until the
/// graph goes back to the grid. Core does not measure: the frames of a graph
/// leaving the grid come from `stokesforge_render::pipeline::free_frames_from_grid`.
pub fn set_free_placement(spec: &mut PlotSpec, frames: &[Option<FreeFrame>]) -> bool {
    if frames.len() != spec.panels.len() {
        return false;
    }
    spec.layer_placement = LayerPlacement::Free;
    for (panel, frame) in spec.panels.iter_mut().zip(frames) {
        panel.free_frame = if panel.overlay.is_some() {
            None
        } else {
            *frame
        };
    }
    true
}

/// Puts the graph back on the regular grid (#398, L7 then C4), from the
/// positions of the free frames alone (pure, no measurement):
///
/// - panels whose vertical extents overlap share a row, transitively (A over
///   B, B over C: one row even if A and C do not meet); extents that only touch
///   make two rows; rows run top to bottom;
/// - inside a row, panels take columns left to right;
/// - each column then takes its widest frame, each row its tallest (C4: no
///   frame shrinks).
///
/// Overlaid layers follow the new identity of their base; the free frames are
/// dropped. `false`, with the spec unchanged, if the graph is not free or the
/// grid would exceed [`MAX_GRID_CELLS`].
pub fn set_grid_placement(spec: &mut PlotSpec) -> bool {
    if spec.layer_placement != LayerPlacement::Free {
        return false;
    }
    let bases = spec.cell_bases();
    // One entry per cell: (old identity, frame, index of its base layer).
    let mut cells: Vec<((usize, usize), FreeFrame, usize)> = Vec::new();
    for (index, &base) in bases.iter().enumerate() {
        if base == index {
            let panel = &spec.panels[index];
            cells.push((
                (panel.row, panel.column),
                panel.sanitized_free_frame(),
                index,
            ));
        }
    }
    // L7: rows by transitive overlap of the vertical extents.
    cells.sort_by(|a, b| a.1.top.total_cmp(&b.1.top).then(a.2.cmp(&b.2)));
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut reach = f64::NEG_INFINITY;
    for (at, (_, frame, _)) in cells.iter().enumerate() {
        match rows.last_mut() {
            Some(row) if frame.top < reach => {
                row.push(at);
                reach = reach.max(frame.top + frame.height);
            }
            _ => {
                rows.push(vec![at]);
                reach = frame.top + frame.height;
            }
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| {
            cells[*a]
                .1
                .left
                .total_cmp(&cells[*b].1.left)
                .then(cells[*a].2.cmp(&cells[*b].2))
        });
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1);
    if rows
        .len()
        .checked_mul(columns)
        .is_none_or(|n| n > MAX_GRID_CELLS)
    {
        return false;
    }
    // C4: the largest frame of each column and each row.
    let mut widths = vec![0.0_f64; columns];
    let mut heights = vec![0.0_f64; rows.len()];
    let mut identity: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
    for (row, members) in rows.iter().enumerate() {
        for (column, &at) in members.iter().enumerate() {
            let (old, frame, _) = &cells[at];
            widths[column] = widths[column].max(frame.width);
            heights[row] = heights[row].max(frame.height);
            identity.insert(*old, (row, column));
        }
    }
    for panel in spec.panels.iter_mut() {
        if let Some(&(row, column)) = identity.get(&(panel.row, panel.column)) {
            panel.row = row;
            panel.column = column;
        }
        panel.free_frame = None;
    }
    spec.frame_widths = widths;
    spec.frame_heights = heights;
    spec.layer_placement = LayerPlacement::Grid;
    true
}

/// Which number of a free frame (#398).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeField {
    /// Left edge of the frame.
    Left,
    /// Top edge of the frame.
    Top,
    /// Width of the frame.
    Width,
    /// Height of the frame.
    Height,
}

/// Sets one number of the free frame of the cell of layer `panel` (#398; an
/// overlaid layer writes its base's), brought into bounds like
/// [`FreeFrame::sanitized`]. A cell with no frame yet starts from the fallback
/// frame. `false`, with the spec unchanged, in grid placement, for a missing
/// layer or a value that is not finite.
pub fn set_free_frame(spec: &mut PlotSpec, panel: usize, field: FreeField, value: f64) -> bool {
    if spec.layer_placement != LayerPlacement::Free || !value.is_finite() {
        return false;
    }
    let Some(&base) = spec.cell_bases().get(panel) else {
        return false;
    };
    let mut frame = spec.panels[base].sanitized_free_frame();
    match field {
        FreeField::Left => frame.left = value,
        FreeField::Top => frame.top = value,
        FreeField::Width => frame.width = value,
        FreeField::Height => frame.height = value,
    }
    if let Some(target) = spec.panels.get_mut(base) {
        target.free_frame = Some(frame.sanitized());
    }
    true
}

/// Which edge or center the layers of an Align line up on (#398, L4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignEdge {
    /// Bottom edges of the frames.
    Bottom,
    /// Top edges of the frames.
    Top,
    /// Vertical centers of the frames.
    VCenter,
    /// Left edges of the frames.
    Left,
    /// Right edges of the frames.
    Right,
    /// Horizontal centers of the frames.
    HCenter,
}

/// Which size of the reference the other layers take (#398, L4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    /// Width of the plot area.
    Width,
    /// Height of the plot area.
    Height,
}

/// The cells an arrange action (Align, Same Width…) acts on (#398, L5), as
/// base layer indices: `active` is the reference and comes first, then the
/// `extra` selected layers, an overlay standing for its base and a cell
/// counted once. With fewer than two distinct cells, every cell of the graph:
/// the active one first, then the others in creation order.
pub fn arrange_targets(spec: &PlotSpec, active: usize, extra: &[usize]) -> Vec<usize> {
    let bases = spec.cell_bases();
    let mut picked: Vec<usize> = Vec::new();
    for panel in std::iter::once(&active).chain(extra) {
        if let Some(&base) = bases.get(*panel) {
            if !picked.contains(&base) {
                picked.push(base);
            }
        }
    }
    if picked.len() >= 2 {
        return picked;
    }
    let mut all: Vec<usize> = bases
        .iter()
        .enumerate()
        .filter_map(|(index, &base)| (index == base).then_some(index))
        .collect();
    if let Some(&first) = picked.first() {
        all.retain(|&base| base != first);
        all.insert(0, first);
    }
    all
}

/// The base cells of `targets` with their frames, the reference first; `None`
/// in grid placement, for a missing layer or an empty list.
fn target_frames(spec: &PlotSpec, targets: &[usize]) -> Option<Vec<(usize, FreeFrame)>> {
    if spec.layer_placement != LayerPlacement::Free || targets.is_empty() {
        return None;
    }
    let bases = spec.cell_bases();
    let mut out: Vec<(usize, FreeFrame)> = Vec::with_capacity(targets.len());
    for panel in targets {
        let base = *bases.get(*panel)?;
        if !out.iter().any(|(b, _)| *b == base) {
            out.push((base, spec.panels[base].sanitized_free_frame()));
        }
    }
    Some(out)
}

/// Lines the plot areas of `targets` (base layers, the **first is the
/// reference** and does not move) up on one edge or center of the
/// reference's frame (#398, L4, L5). Frames, not outer rectangles, like the
/// magnets; sizes and stacking keys are kept. `false`, with the spec
/// unchanged, in grid placement or for a missing layer.
pub fn align_layers(spec: &mut PlotSpec, targets: &[usize], edge: AlignEdge) -> bool {
    let Some(frames) = target_frames(spec, targets) else {
        return false;
    };
    let reference = frames[0].1;
    for (base, mut frame) in frames.into_iter().skip(1) {
        match edge {
            AlignEdge::Bottom => frame.top = reference.top + reference.height - frame.height,
            AlignEdge::Top => frame.top = reference.top,
            AlignEdge::VCenter => {
                frame.top = reference.top + (reference.height - frame.height) / 2.0;
            }
            AlignEdge::Left => frame.left = reference.left,
            AlignEdge::Right => frame.left = reference.left + reference.width - frame.width,
            AlignEdge::HCenter => {
                frame.left = reference.left + (reference.width - frame.width) / 2.0;
            }
        }
        if let Some(panel) = spec.panels.get_mut(base) {
            panel.free_frame = Some(frame.sanitized());
        }
    }
    true
}

/// Gives the plot areas of `targets` (the first is the reference) the width
/// or the height of the reference's, their left and top kept (#398, L4, L5).
/// `false`, with the spec unchanged, in grid placement or for a missing layer.
pub fn match_layer_size(spec: &mut PlotSpec, targets: &[usize], dimension: Dimension) -> bool {
    let Some(frames) = target_frames(spec, targets) else {
        return false;
    };
    let reference = frames[0].1;
    for (base, mut frame) in frames.into_iter().skip(1) {
        match dimension {
            Dimension::Width => frame.width = reference.width,
            Dimension::Height => frame.height = reference.height,
        }
        if let Some(panel) = spec.panels.get_mut(base) {
            panel.free_frame = Some(frame.sanitized());
        }
    }
    true
}

/// Rewrites the stacking keys of the cells to `0..n` following `order` (base
/// layer indices, back to front), so no key ever overflows.
fn restack(spec: &mut PlotSpec, order: &[usize]) {
    for (rank, &base) in order.iter().enumerate() {
        let Some(panel) = spec.panels.get_mut(base) else {
            continue;
        };
        let mut frame = panel.sanitized_free_frame();
        frame.stack = i32::try_from(rank).unwrap_or(i32::MAX);
        panel.free_frame = Some(frame);
    }
}

/// Puts the cell of layer `panel` (an overlay acts on its base) in front of
/// all the others (#398). Layer numbers are kept; only the drawing order
/// changes. `false`, with the spec unchanged, in grid placement, for a
/// missing layer or when the cell is already in front.
pub fn bring_to_front(spec: &mut PlotSpec, panel: usize) -> bool {
    move_in_stack(spec, panel, true)
}

/// Puts the cell of layer `panel` behind all the others (#398); see
/// [`bring_to_front`].
pub fn send_to_back(spec: &mut PlotSpec, panel: usize) -> bool {
    move_in_stack(spec, panel, false)
}

fn move_in_stack(spec: &mut PlotSpec, panel: usize, front: bool) -> bool {
    if spec.layer_placement != LayerPlacement::Free {
        return false;
    }
    let Some(&base) = spec.cell_bases().get(panel) else {
        return false;
    };
    let mut order = spec.free_draw_order();
    let Some(at) = order.iter().position(|&b| b == base) else {
        return false;
    };
    if (front && at + 1 == order.len()) || (!front && at == 0) {
        return false;
    }
    order.remove(at);
    if front {
        order.push(base);
    } else {
        order.insert(0, base);
    }
    restack(spec, &order);
    true
}

/// True if the cell of layer `panel` is in front of every other cell (or
/// alone): there is nothing to bring forward (#398).
pub fn is_in_front(spec: &PlotSpec, panel: usize) -> bool {
    let Some(&base) = spec.cell_bases().get(panel) else {
        return true;
    };
    spec.free_draw_order().last() == Some(&base)
}

/// True if the cell of layer `panel` is behind every other cell (or alone).
pub fn is_at_back(spec: &PlotSpec, panel: usize) -> bool {
    let Some(&base) = spec.cell_bases().get(panel) else {
        return true;
    };
    spec.free_draw_order().first() == Some(&base)
}

/// Sets the background of the cell of layer `panel` (#398; an overlay writes
/// its base's): empty = Auto, else a color string. `false`, with the spec
/// unchanged, in grid placement or for a missing layer.
pub fn set_layer_background(spec: &mut PlotSpec, panel: usize, color: &str) -> bool {
    if spec.layer_placement != LayerPlacement::Free {
        return false;
    }
    let Some(&base) = spec.cell_bases().get(panel) else {
        return false;
    };
    if let Some(target) = spec.panels.get_mut(base) {
        target.layer_background = color.to_string();
    }
    true
}

/// The layers that deleting layer `panel` removes (#357): an overlay layer
/// alone; an ordinary layer (a cell) with all the overlay layers of its cell.
/// Empty if `panel` does not exist.
pub fn layers_removed_with(spec: &PlotSpec, panel: usize) -> Vec<usize> {
    let Some(target) = spec.panels.get(panel) else {
        return Vec::new();
    };
    if target.overlay.is_some() {
        return vec![panel];
    }
    let cell = (target.row, target.column);
    spec.panels
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            *index == panel
                || (candidate.overlay.is_some() && (candidate.row, candidate.column) == cell)
        })
        .map(|(index, _)| index)
        .collect()
}

/// Can layer `panel` be deleted (#357)? An overlay layer always; an ordinary
/// layer only if another cell remains (a graph keeps at least one).
pub fn can_remove_layer(spec: &PlotSpec, panel: usize) -> bool {
    match spec.panels.get(panel) {
        None => false,
        Some(target) if target.overlay.is_some() => true,
        Some(_) => spec.panels.iter().filter(|p| p.overlay.is_none()).count() >= 2,
    }
}

/// Deletes layer `panel` (#357, see [`layers_removed_with`]). A grid column
/// (row) left with no layer disappears with its width (height), so the layers
/// beyond it move back one column (row) and the figure shrinks by that size
/// (#394); a column (row) that still holds another cell stays, the deleted cell
/// leaving a hole. The graph title position is in the frame of the first layer,
/// so it is forgotten when that layer goes. `false`, with the spec unchanged, if
/// [`can_remove_layer`] says no.
pub fn remove_cell(spec: &mut PlotSpec, panel: usize) -> bool {
    if !can_remove_layer(spec, panel) {
        return false;
    }
    let removed = layers_removed_with(spec, panel);
    let Some(target) = spec.panels.get(panel) else {
        return false;
    };
    let is_cell = target.overlay.is_none();
    let (row, column) = (target.row, target.column);
    let widths = spec.sanitized_frame_widths();
    let heights = spec.sanitized_frame_heights();
    if !spec.panels.remove_indices(&removed) {
        return false;
    }
    if removed.contains(&0) {
        spec.title_anchor = None;
    }
    if is_cell {
        if !spec
            .panels
            .iter()
            .any(|candidate| candidate.column == column)
        {
            for candidate in spec.panels.iter_mut() {
                if candidate.column > column {
                    candidate.column -= 1;
                }
            }
            if let Some(mut widths) = widths {
                if column < widths.len() {
                    widths.remove(column);
                }
                spec.frame_widths = widths;
            }
        }
        if !spec.panels.iter().any(|candidate| candidate.row == row) {
            for candidate in spec.panels.iter_mut() {
                if candidate.row > row {
                    candidate.row -= 1;
                }
            }
            if let Some(mut heights) = heights {
                if row < heights.len() {
                    heights.remove(row);
                }
                spec.frame_heights = heights;
            }
        }
    }
    true
}

/// Where each layer of a spec went after `removed` (sorted or not) layers
/// were deleted: `None` for a deleted layer, the new index otherwise.
pub fn remap_after_removal(len: usize, removed: &[usize]) -> Vec<Option<usize>> {
    let mut next = 0;
    (0..len)
        .map(|index| {
            if removed.contains(&index) {
                None
            } else {
                next += 1;
                Some(next - 1)
            }
        })
        .collect()
}

/// Where each layer of `previous` is in `next` when layers were added or
/// removed by history (undo, redo): layers match in order, by content (their
/// position in the grid aside). `None` for a layer that is gone.
pub fn remap_between(previous: &PlotSpec, next: &PlotSpec) -> Vec<Option<usize>> {
    let same = |a: &PanelSpec, b: &PanelSpec| {
        let mut b = b.clone();
        b.row = a.row;
        b.column = a.column;
        b.free_frame = a.free_frame;
        b.layer_background = a.layer_background.clone();
        *a == b
    };
    let mut from = 0;
    previous
        .panels
        .iter()
        .map(|old| {
            let found = (from..next.panels.len()).find(|j| same(old, &next.panels[*j]))?;
            from = found + 1;
            Some(found)
        })
        .collect()
}

/// The current [`PlotSpec`] of `graph`, or `None` if it is not a graph of
/// this project.
pub fn current_plot_spec(tree: &Tree, graph: NodeId) -> Option<PlotSpec> {
    match tree.node(graph)?.kind() {
        NodeKind::Graph { spec, .. } => Some(spec.clone()),
        _ => None,
    }
}

/// Plot type of the series added to `panel` when the gesture names
/// none: that of its **last** series (a layer that mixes several
/// gives the last), [`PlotType::Line`] for an empty layer.
pub fn panel_plot_type(panel: &PanelSpec) -> PlotType {
    panel
        .series
        .last()
        .map_or(PlotType::Line, |series| series.plot_type.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, ColumnMeta, Project};
    use stokesforge_plot::{palette_color, Panels, DEFAULT_PALETTE};

    /// A workbook whose columns, in order, carry the given roles
    /// (named `c0`, `c1`…).
    fn sheet_with(roles: &[ColumnRole]) -> (Project, NodeId, Vec<NodeId>) {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let columns: Vec<NodeId> = roles
            .iter()
            .enumerate()
            .map(|(i, role)| {
                let id = project
                    .add_column(
                        sheet,
                        None,
                        Some(&format!("c{i}")),
                        Column::from_f64(&[1.0, 2.0, 3.0]),
                    )
                    .unwrap();
                project
                    .set_column_meta(
                        id,
                        ColumnMeta {
                            role: *role,
                            ..ColumnMeta::default()
                        },
                    )
                    .unwrap();
                id
            })
            .collect();
        (project, sheet, columns)
    }

    use ColumnRole::{None as NoRole, XError, YError, X, X2, X3, X4, Y, Y2, Y3, Y4};

    /// Pairing of the error columns for the selection `selected` of a sheet.
    fn planned_for(roles: &[ColumnRole], selected: &[usize]) -> (PlannedSeries, Vec<NodeId>) {
        let (project, sheet, c) = sheet_with(roles);
        let ids: Vec<NodeId> = selected.iter().map(|i| c[*i]).collect();
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &ids,
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        (planned, c)
    }

    #[test]
    fn the_y_error_to_the_right_fills_y_error_even_unselected() {
        let (p, c) = planned_for(&[X, Y, YError], &[1]);
        assert_eq!(p.primary[0].y_error, Some(c[2]));
        assert_eq!(p.primary[0].x_error, None);
    }

    #[test]
    fn the_error_pairs_with_the_y_that_precedes_it_not_the_first_y() {
        let (p, c) = planned_for(&[X, Y, Y, YError], &[1, 2]);
        assert_eq!(p.primary[0].y_error, None);
        assert_eq!(p.primary[1].y_error, Some(c[3]));
    }

    #[test]
    fn the_first_error_column_wins() {
        let (p, c) = planned_for(&[X, Y, YError, YError], &[1]);
        assert_eq!(p.primary[0].y_error, Some(c[2]));
    }

    #[test]
    fn both_error_roles_pair() {
        let (p, c) = planned_for(&[X, Y, XError, YError], &[1]);
        assert_eq!(p.primary[0].x_error, Some(c[2]));
        assert_eq!(p.primary[0].y_error, Some(c[3]));
    }

    #[test]
    fn an_error_column_to_the_left_does_not_pair() {
        let (p, _) = planned_for(&[X, YError, Y], &[2]);
        assert_eq!(p.primary[0].y_error, None);
    }

    #[test]
    fn an_x_column_between_does_not_stop_the_scan() {
        let (p, c) = planned_for(&[X, Y, X, YError], &[1]);
        assert_eq!(p.primary[0].y_error, Some(c[3]));
    }

    #[test]
    fn a_y2_pairs_with_its_error_on_the_overlay_series() {
        let (p, c) = planned_for(&[X, Y, X2, Y2, YError], &[1, 3]);
        assert_eq!(p.primary[0].y_error, None, "stopped by the Y2");
        assert_eq!(p.overlays[0][0].y_error, Some(c[4]));
    }

    #[test]
    fn errors_for_follows_the_rule_of_the_plot() {
        let (project, _sheet, c) = sheet_with(&[X, Y, XError, YError, Y]);
        let tree = project.tree();
        assert_eq!(errors_for(tree, c[1]), (Some(c[2]), Some(c[3])));
        assert_eq!(errors_for(tree, c[4]), (None, None));
        assert_eq!(errors_for(tree, c[0]), (None, None), "not an ordinate");
    }

    #[test]
    fn each_y_takes_the_nearest_x_to_its_left_even_if_unselected() {
        // X0 Y1 X2 Y3 Y4: Y1 -> X0, Y3 and Y4 -> X2 (the nearest to the left).
        let (project, sheet, c) = sheet_with(&[X, Y, X, Y, Y]);
        let tree = project.tree();
        // Only the Y are selected: the X are not, yet they still serve.
        let planned = series_from_columns(
            tree,
            sheet,
            &[c[1], c[3], c[4]],
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        let pairs: Vec<(Option<NodeId>, NodeId)> =
            planned.primary.iter().map(|s| (s.x, s.y)).collect();
        assert_eq!(
            pairs,
            [(Some(c[0]), c[1]), (Some(c[2]), c[3]), (Some(c[2]), c[4])]
        );
        assert!(planned.overlays.iter().all(Vec::is_empty));
        // The witness: a pairing "to the right" would give another X.
        assert_ne!(planned.primary[0].x, Some(c[2]));
    }

    #[test]
    fn a_y2_pairs_with_x2_and_goes_to_the_second_layer() {
        // X0 Y1 X2_2 Y2_3: Y2 ignores X0 and takes X2.
        let (project, sheet, c) = sheet_with(&[X, Y, X2, Y2]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &[c[1], c[3]],
            &PlotType::Scatter,
            0,
            [0; OVERLAY_ROLES],
        );
        assert_eq!(planned.primary.len(), 1);
        assert_eq!(planned.primary[0].x, Some(c[0]));
        assert_eq!(planned.overlays[0].len(), 1);
        assert_eq!(planned.overlays[0][0].x, Some(c[2]));
        assert_eq!(planned.overlays[0][0].y, c[3]);
        assert!(planned
            .primary
            .iter()
            .chain(&planned.overlays[0])
            .all(|s| s.plot_type == PlotType::Scatter));
    }

    /// #168: Y3 and Y4 pair with X3 and X4 (never with X or X2) and go to
    /// their own layer; a Y3 without Y2 does not fill layer 2.
    #[test]
    fn y3_and_y4_pair_with_their_x_and_go_to_their_layer() {
        let (project, sheet, c) = sheet_with(&[X, Y, X2, Y2, X3, Y3, X4, Y4]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        let pairs = |i: usize| -> Vec<(Option<NodeId>, NodeId)> {
            planned.overlays[i].iter().map(|s| (s.x, s.y)).collect()
        };
        assert_eq!(pairs(0), [(Some(c[2]), c[3])]);
        assert_eq!(pairs(1), [(Some(c[4]), c[5])]);
        assert_eq!(pairs(2), [(Some(c[6]), c[7])]);
        assert_eq!(x_for(project.tree(), c[7]), Some(c[6]));

        let (project, sheet, c) = sheet_with(&[X, Y, X3, Y3]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &[c[1], c[3]],
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        assert!(planned.overlays[0].is_empty(), "no Y2: layer 2 empty");
        assert_eq!(planned.overlays[1].len(), 1);
        assert_eq!(planned.overlay_series().count(), 1);
    }

    #[test]
    fn without_x_to_the_left_the_abscissa_stays_implicit_and_without_y_nothing_is_planned() {
        // Y0 X1 Y2: Y0 has no X to its left, Y2 has X1.
        let (project, sheet, c) = sheet_with(&[Y, X, Y]);
        let tree = project.tree();
        let planned = series_from_columns(tree, sheet, &c, &PlotType::Line, 0, [0; OVERLAY_ROLES]);
        assert_eq!(planned.primary[0].x, None);
        assert_eq!(planned.primary[1].x, Some(c[1]));
        // A Y2 without X2 to the left stays implicit too, even with an X.
        let (project, sheet, c) = sheet_with(&[X, Y2]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        assert_eq!(planned.overlays[0][0].x, None);
        // No Y or Y2 column selected: empty.
        let (project, sheet, c) = sheet_with(&[X, NoRole]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            0,
            [0; OVERLAY_ROLES],
        );
        assert!(planned.is_empty());
    }

    #[test]
    fn x_for_follows_the_same_rule_as_series_from_columns() {
        let (project, _sheet, c) = sheet_with(&[Y, X, Y, X2, Y2, Y]);
        let tree = project.tree();
        assert_eq!(x_for(tree, c[0]), None, "no X to the left");
        assert_eq!(x_for(tree, c[2]), Some(c[1]));
        assert_eq!(x_for(tree, c[4]), Some(c[3]), "a Y2 takes X2");
        assert_eq!(x_for(tree, c[5]), Some(c[1]), "a Y ignores X2");
        // Not a column: nothing.
        assert_eq!(x_for(tree, tree.root()), None);
    }

    /// Gives column `at` the formula `text` (read in letters).
    fn set_formula(project: &mut Project, at: NodeId, text: &str) {
        let formula = project.tree().formula_from_letters(at, text).unwrap();
        let mut meta = project
            .tree()
            .node(at)
            .and_then(Node::as_column)
            .unwrap()
            .meta()
            .clone();
        meta.formula = Some(formula);
        project.set_column_meta(at, meta).unwrap();
    }

    #[test]
    fn the_formula_x_follows_the_roles_and_the_position_of_the_column() {
        // c0 X, c1 Y, c2 Y (formula column).
        let (mut project, _sheet, c) = sheet_with(&[X, Y, Y]);
        set_formula(&mut project, c[2], "B / at(B, 2)");
        let compiled = |p: &Project| {
            p.tree()
                .node(c[2])
                .and_then(Node::as_column)
                .unwrap()
                .meta()
                .formula
                .clone()
                .unwrap()
                .compile()
                .unwrap()
        };
        let pairs = |p: &Project| paired_x_columns(p.tree(), &compiled(p));
        assert_eq!(pairs(&project), BTreeMap::from([(c[1], c[0])]));
        assert!(!project.tree().formula_lacks_paired_x(c[2]));
        // B no longer has an X to its left: the pairing disappears.
        let mut meta = project
            .tree()
            .node(c[0])
            .and_then(Node::as_column)
            .unwrap()
            .meta()
            .clone();
        meta.role = Y;
        project.set_column_meta(c[0], meta).unwrap();
        assert_eq!(pairs(&project), BTreeMap::new());
        assert!(project.tree().formula_lacks_paired_x(c[2]));
        // A formula that needs no X is never short of one.
        set_formula(&mut project, c[2], "B / max(B)");
        assert!(!project.tree().formula_lacks_paired_x(c[2]));
    }

    #[test]
    fn a_formula_whose_every_cell_is_empty_found_no_point() {
        let (mut project, _sheet, c) = sheet_with(&[X, Y, Y]);
        set_formula(&mut project, c[2], "at(B, 900)");
        let all_empty = |p: &mut Project| {
            let n = 3;
            p.set_length(c[2], n).unwrap();
            p.set_column_type(c[2], crate::column::ColumnType::Mixed)
                .unwrap();
            p.set_values(
                c[2],
                0,
                crate::column::ValueBlock::Mixed(vec![crate::column::MixedCell::Empty; n]),
            )
            .unwrap();
        };
        // Cells not yet written as empty: a number among them.
        assert!(!project.tree().formula_found_no_point(c[2]));
        all_empty(&mut project);
        assert!(project.tree().formula_found_no_point(c[2]));
        // `---` is another message, a number is a result.
        project
            .set_values(
                c[2],
                1,
                crate::column::ValueBlock::Mixed(vec![crate::column::MixedCell::Text(
                    "---".into(),
                )]),
            )
            .unwrap();
        assert!(!project.tree().formula_found_no_point(c[2]));
        // No column function: no such message.
        all_empty(&mut project);
        set_formula(&mut project, c[2], "B * 2");
        assert!(!project.tree().formula_found_no_point(c[2]));
    }

    #[test]
    fn the_palette_follows_the_rank_with_the_offset_of_each_layer() {
        let (project, sheet, c) = sheet_with(&[X, Y, Y, X2, Y2]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &[c[1], c[2], c[4]],
            &PlotType::Line,
            3,
            [7, 0, 0],
        );
        let colors: Vec<&str> = planned
            .primary
            .iter()
            .map(|s| s.line.color.as_str())
            .collect();
        assert_eq!(colors, [palette_color(3), palette_color(4)]);
        assert_eq!(planned.overlays[0][0].line.color, palette_color(7));
        // Line and markers together.
        assert!(planned
            .primary
            .iter()
            .chain(&planned.overlays[0])
            .all(|s| s.symbol.color == s.line.color));
        // The rank wraps around the palette.
        let (project, sheet, c) = sheet_with(&[Y]);
        let planned = series_from_columns(
            project.tree(),
            sheet,
            &c,
            &PlotType::Line,
            DEFAULT_PALETTE.len(),
            [0; OVERLAY_ROLES],
        );
        assert_eq!(planned.primary[0].line.color, DEFAULT_PALETTE[0]);
    }

    #[test]
    fn row_series_take_the_palette_of_the_rank_and_the_given_name() {
        let (_project, _sheet, c) = sheet_with(&[Y]);
        let series = row_series(c[0], &[4, 9], 2, &PlotType::Scatter, |row| {
            format!("row {}", row + 1)
        });
        assert_eq!(series.len(), 2);
        assert_eq!((series[0].row, series[1].row), (Some(4), Some(9)));
        assert_eq!(
            (series[0].name.as_str(), series[1].name.as_str()),
            ("row 5", "row 10")
        );
        assert_eq!(series[0].line.color, palette_color(2));
        assert_eq!(series[1].line.color, palette_color(3));
        assert_eq!(series[1].symbol.color, palette_color(3));
        assert!(series.iter().all(|s| s.y == c[0] && s.x.is_none()));
        assert!(series.iter().all(|s| s.plot_type == PlotType::Scatter));
    }

    #[test]
    fn x_prime_columns_are_those_whose_value_is_finite() {
        let (mut project, sheet, c) = sheet_with(&[Y, Y, Y, Y]);
        for (id, x_prime) in [
            (c[0], None),
            (c[1], Some(1.5)),
            (c[2], Some(f64::NAN)),
            (c[3], Some(0.0)),
        ] {
            project
                .set_column_meta(
                    id,
                    ColumnMeta {
                        x_prime,
                        ..ColumnMeta::default()
                    },
                )
                .unwrap();
        }
        assert_eq!(x_prime_columns(project.tree(), sheet), [c[1], c[3]]);
    }

    /// A spec with three layers: ordinary, overlaid X2/Y2, ordinary in cell (0, 1).
    fn spec_with_layers() -> PlotSpec {
        let base = PanelSpec::default();
        let second = PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES);
        let rows = PanelSpec::overlay_on(&base, OverlaySpec::TOP_X);
        let elsewhere = PanelSpec::at(0, 1);
        PlotSpec {
            panels: Panels::new(vec![base, rows, second, elsewhere]),
            ..PlotSpec::default()
        }
    }

    #[test]
    fn an_overlay_layer_covers_the_last_ordinary_layer_of_its_cell() {
        let spec = spec_with_layers();
        assert_eq!(overlay_base(&spec, 0), 0, "ordinary: itself");
        assert_eq!(overlay_base(&spec, 1), 0);
        assert_eq!(overlay_base(&spec, 2), 0);
        assert_eq!(overlay_base(&spec, 3), 3);
        assert_eq!(overlay_base(&spec, 9), 9, "nonexistent: unchanged");
    }

    #[test]
    fn the_second_layer_is_the_two_axes_one_not_the_rows_one() {
        let spec = spec_with_layers();
        // Layer 1 (X on top only) is not the second layer: layer 2 is.
        assert_eq!(role_layer_of(&spec, 0, 2), Some(2));
        assert_eq!(role_layer_of(&spec, 0, 3), None, "no layer 3");
        assert_eq!(role_layer_of(&spec, 3, 2), None, "other cell");
        assert_eq!(role_layer_of(&spec, 9, 2), None);
    }

    /// #168: a role layer is found by its **number**, not by its
    /// position: layer 3 is that of Y3 even without a layer 2 before it.
    #[test]
    fn a_role_layer_is_found_by_its_number() {
        let base = PanelSpec::default();
        let third = PanelSpec::overlay_on(&base, OverlaySpec::role_layer(3));
        let spec = PlotSpec {
            panels: Panels::new(vec![base, third]),
            ..PlotSpec::default()
        };
        assert_eq!(role_layer_of(&spec, 0, 3), Some(1));
        assert_eq!(role_layer_of(&spec, 0, 2), None);
    }

    #[test]
    fn the_graph_has_an_x_through_an_x_column_or_a_row_series() {
        let (_project, _sheet, c) = sheet_with(&[X, Y]);
        let mut spec = PlotSpec::default();
        assert!(!spec_has_x(&spec));
        spec.panels
            .get_mut(0)
            .unwrap()
            .series
            .push(SeriesSpec::new(c[1]));
        assert!(!spec_has_x(&spec), "implicit abscissa: no X");
        spec.panels
            .get_mut(0)
            .unwrap()
            .series
            .push(SeriesSpec::new_xy(c[0], c[1]));
        assert!(spec_has_x(&spec));
        let mut rows = PlotSpec::default();
        let mut series = SeriesSpec::new(c[1]);
        series.row = Some(0);
        rows.panels.get_mut(0).unwrap().series.push(series);
        assert!(spec_has_x(&rows));
    }

    #[test]
    fn the_plot_type_of_a_layer_is_that_of_its_last_series() {
        let (_project, _sheet, c) = sheet_with(&[Y]);
        let mut panel = PanelSpec::default();
        assert_eq!(panel_plot_type(&panel), PlotType::Line, "empty layer");
        let mut a = SeriesSpec::new(c[0]);
        a.plot_type = PlotType::Scatter;
        let b = SeriesSpec::new(c[0]);
        panel.series = vec![a, b];
        assert_eq!(panel_plot_type(&panel), PlotType::Line);
        panel.series.reverse();
        assert_eq!(panel_plot_type(&panel), PlotType::Scatter);
    }

    #[test]
    fn the_current_spec_is_the_graphs_and_nothing_else() {
        let (mut project, sheet, _c) = sheet_with(&[Y]);
        let spec = spec_with_layers();
        let graph = project
            .add_graph(project.root(), Some("G"), spec.clone())
            .unwrap();
        let tree = project.tree();
        assert_eq!(current_plot_spec(tree, graph), Some(spec));
        assert_eq!(current_plot_spec(tree, sheet), None, "a sheet");
        assert_eq!(current_plot_spec(tree, NodeId::from_raw(987_654)), None);
    }

    /// Position (row, column) and overlay flag of every layer, in order.
    fn grid(spec: &PlotSpec) -> Vec<(usize, usize, bool)> {
        spec.panels
            .iter()
            .map(|p| (p.row, p.column, p.overlay.is_some()))
            .collect()
    }

    fn spec_of(cells: &[(usize, usize)]) -> PlotSpec {
        PlotSpec {
            panels: Panels::new(cells.iter().map(|(r, c)| PanelSpec::at(*r, *c))),
            ..PlotSpec::default()
        }
    }

    #[test]
    fn insert_right_in_a_single_cell_graph_appends_cell_zero_one() {
        let mut spec = PlotSpec::default();
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Right), Some(1));
        assert_eq!(grid(&spec), [(0, 0, false), (0, 1, false)]);
    }

    #[test]
    fn insert_right_shifts_every_cell_beyond_the_column_with_its_overlays() {
        let base = PanelSpec::at(0, 1);
        let mut spec = PlotSpec {
            panels: Panels::new(vec![
                PanelSpec::at(0, 0),
                base.clone(),
                PanelSpec::overlay_on(&base, OverlaySpec::TOP_X),
                PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES),
                PanelSpec::at(1, 1),
            ]),
            ..PlotSpec::default()
        };
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Right), Some(5));
        assert_eq!(
            grid(&spec),
            [
                (0, 0, false),
                (0, 2, false),
                (0, 2, true),
                (0, 2, true),
                (1, 2, false), // other row: the whole column is inserted
                (0, 1, false), // the new cell
            ]
        );
        assert_eq!(overlay_base(&spec, 2), 1, "overlays still cover their base");
        assert_eq!(overlay_base(&spec, 3), 1);
    }

    #[test]
    fn insert_below_shifts_every_cell_below_the_row() {
        let mut spec = spec_of(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Below), Some(4));
        assert_eq!(
            grid(&spec),
            [
                (0, 0, false),
                (0, 1, false),
                (2, 0, false),
                (2, 1, false),
                (1, 0, false),
            ]
        );
    }

    #[test]
    fn insert_right_in_a_grid_inserts_a_whole_column_leaving_a_hole_in_the_other_rows() {
        let mut spec = spec_of(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(insert_cell(&mut spec, 2, CellSide::Right), Some(4));
        assert_eq!(
            grid(&spec),
            [
                (0, 0, false),
                (0, 2, false),
                (1, 0, false),
                (1, 2, false),
                (1, 1, false), // the new cell; (0, 1) is a hole
            ]
        );
    }

    #[test]
    fn insert_from_an_overlay_uses_the_cell_of_its_base() {
        let base = PanelSpec::at(0, 0);
        let mut spec = PlotSpec {
            panels: Panels::new(vec![
                base.clone(),
                PanelSpec::overlay_on(&base, OverlaySpec::TOP_X),
            ]),
            ..PlotSpec::default()
        };
        assert_eq!(insert_cell(&mut spec, 1, CellSide::Right), Some(2));
        assert_eq!(grid(&spec)[2], (0, 1, false));
    }

    #[test]
    fn insert_refuses_overflow_and_a_grid_beyond_the_cap_without_touching_the_spec() {
        let mut spec = spec_of(&[(0, usize::MAX)]);
        let before = spec.clone();
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Right), None);
        assert_eq!(
            insert_cell(&mut spec, 0, CellSide::Below),
            None,
            "grid too big"
        );
        assert_eq!(spec, before);
        let mut spec = spec_of(&[(usize::MAX, 0)]);
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Below), None);

        let mut full = spec_of(&[(0, 0), (0, MAX_GRID_CELLS - 1)]);
        let before = full.clone();
        assert_eq!(insert_cell(&mut full, 0, CellSide::Right), None, "cap");
        assert_eq!(full, before);
        assert_eq!(
            insert_cell(&mut full, 9, CellSide::Right),
            None,
            "no such layer"
        );
    }

    #[test]
    fn remove_a_cell_takes_its_overlays_and_leaves_a_hole_when_its_column_and_row_stay_used() {
        let first = PanelSpec::at(0, 0);
        let mut spec = PlotSpec {
            panels: Panels::new(vec![
                PanelSpec::at(0, 1),
                first.clone(),
                PanelSpec::overlay_on(&first, OverlaySpec::TOP_X),
                PanelSpec::overlay_on(&first, OverlaySpec::SECOND_AXES),
                PanelSpec::at(1, 0),
            ]),
            ..PlotSpec::default()
        };
        // Removes the cell (0, 0): 3 layers. Its column holds (1, 0) and its
        // row holds (0, 1): nothing shifts, (0, 0) becomes a hole.
        assert!(remove_cell(&mut spec, 1));
        assert_eq!(grid(&spec), [(0, 1, false), (1, 0, false)]);
    }

    #[test]
    fn remove_the_only_cell_of_a_row_moves_the_rows_below_up() {
        let mut spec = spec_of(&[(0, 0), (1, 0), (2, 0)]);
        assert!(remove_cell(&mut spec, 1));
        assert_eq!(grid(&spec), [(0, 0, false), (1, 0, false)]);
    }

    #[test]
    fn remove_the_only_cell_is_refused_and_an_overlay_alone_leaves_its_base() {
        let mut one = PlotSpec::default();
        assert!(!remove_cell(&mut one, 0));
        assert_eq!(one, PlotSpec::default());

        let mut spec = spec_with_layers();
        let before = spec.clone();
        assert!(!remove_cell(&mut spec, 9));
        assert!(remove_cell(&mut spec, 1));
        assert_eq!(spec.panels.len(), 3);
        assert_eq!(spec.panels[0], before.panels[0], "base untouched");
        assert_eq!(spec.panels[1], before.panels[2], "the other overlay stays");
    }

    #[test]
    fn remove_the_first_cell_forgets_the_title_position() {
        let mut spec = spec_of(&[(0, 0), (0, 1)]);
        spec.title_anchor = Some(stokesforge_plot::TextAnchor::new(0.5, 0.5));
        assert!(remove_cell(&mut spec, 1));
        assert!(spec.title_anchor.is_some(), "another cell: kept");
        let mut spec = spec_of(&[(0, 0), (0, 1)]);
        spec.title_anchor = Some(stokesforge_plot::TextAnchor::new(0.5, 0.5));
        assert!(remove_cell(&mut spec, 0));
        assert_eq!(spec.title_anchor, None);
    }

    #[test]
    fn remaps_follow_a_removal() {
        assert_eq!(
            remap_after_removal(4, &[1, 2]),
            [Some(0), None, None, Some(1)]
        );
        let mut before = spec_of(&[(0, 0), (0, 1)]);
        before.panels.get_mut(1).unwrap().title_visible = false;
        let after = spec_of(&[(0, 0)]);
        assert_eq!(remap_between(&before, &after), [Some(0), None]);
        assert_eq!(remap_between(&after, &before), [Some(0)]);
    }

    fn sized(cells: &[(usize, usize)], widths: &[f64], heights: &[f64]) -> PlotSpec {
        PlotSpec {
            frame_widths: widths.to_vec(),
            frame_heights: heights.to_vec(),
            ..spec_of(cells)
        }
    }

    #[test]
    fn insert_right_inserts_the_reference_width_next_to_the_reference_column() {
        let mut spec = sized(&[(0, 0), (0, 1)], &[300.0, 200.0], &[250.0]);
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Right), Some(2));
        assert_eq!(spec.frame_widths, [300.0, 300.0, 200.0]);
        assert_eq!(
            spec.frame_heights,
            [250.0],
            "a column leaves the heights alone"
        );
    }

    #[test]
    fn insert_below_inserts_the_reference_height_and_keeps_the_widths() {
        let mut spec = sized(&[(0, 0), (1, 0)], &[400.0], &[250.0, 100.0]);
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Below), Some(2));
        assert_eq!(spec.frame_heights, [250.0, 250.0, 100.0]);
        assert_eq!(spec.frame_widths, [400.0]);
    }

    #[test]
    fn a_new_cell_has_the_frame_of_its_reference() {
        let mut spec = sized(&[(0, 0)], &[392.0], &[294.0]);
        insert_cell(&mut spec, 0, CellSide::Right);
        assert_eq!(spec.sanitized_frame_widths(), Some(vec![392.0, 392.0]));
        assert_eq!(spec.sanitized_frame_heights(), Some(vec![294.0]));
    }

    #[test]
    fn inserting_in_a_legacy_graph_leaves_the_sizes_unstored() {
        let mut spec = spec_of(&[(0, 0)]);
        insert_cell(&mut spec, 0, CellSide::Right);
        assert!(spec.frame_widths.is_empty() && spec.frame_heights.is_empty());
    }

    #[test]
    fn removing_the_last_cell_of_a_column_or_row_removes_its_size() {
        let mut spec = sized(&[(0, 0), (0, 1), (0, 2)], &[100.0, 200.0, 300.0], &[250.0]);
        assert!(remove_cell(&mut spec, 1));
        assert_eq!(spec.frame_widths, [100.0, 300.0], "figure shrinks by 200");
        assert_eq!(spec.frame_heights, [250.0]);

        let mut spec = sized(&[(0, 0), (1, 0)], &[400.0], &[250.0, 100.0]);
        assert!(remove_cell(&mut spec, 0));
        assert_eq!(spec.frame_heights, [100.0]);
        assert_eq!(spec.frame_widths, [400.0]);
    }

    #[test]
    fn removing_a_cell_whose_column_stays_used_keeps_the_sizes() {
        let mut spec = sized(
            &[(0, 0), (0, 1), (1, 0), (1, 1)],
            &[100.0, 200.0],
            &[50.0, 60.0],
        );
        assert!(remove_cell(&mut spec, 3));
        assert_eq!(spec.frame_widths, [100.0, 200.0]);
        assert_eq!(spec.frame_heights, [50.0, 60.0]);
    }

    #[test]
    fn set_frame_size_clamps_and_touches_one_entry() {
        let mut spec = sized(&[(0, 0), (0, 1)], &[300.0, 200.0], &[250.0]);
        assert!(set_frame_size(&mut spec, FrameAxis::Column, 1, 123.5));
        assert_eq!(spec.frame_widths, [300.0, 123.5]);
        assert!(set_frame_size(&mut spec, FrameAxis::Row, 0, 1e9));
        assert_eq!(spec.frame_heights, [stokesforge_plot::MAX_FRAME_SIZE]);
        assert!(set_frame_size(&mut spec, FrameAxis::Row, 0, 1.0));
        assert_eq!(spec.frame_heights, [stokesforge_plot::MIN_FRAME_SIZE]);
    }

    #[test]
    fn set_frame_size_refuses_a_legacy_graph_a_bad_index_and_a_bad_size() {
        let mut legacy = spec_of(&[(0, 0)]);
        assert!(!set_frame_size(&mut legacy, FrameAxis::Column, 0, 300.0));
        let mut spec = sized(&[(0, 0)], &[300.0], &[250.0]);
        let before = spec.clone();
        assert!(!set_frame_size(&mut spec, FrameAxis::Column, 1, 300.0));
        assert!(!set_frame_size(&mut spec, FrameAxis::Row, 0, f64::NAN));
        assert_eq!(spec, before);
    }

    #[test]
    fn figure_offsets_are_clamped_cleared_and_refuse_nan_negative_and_legacy() {
        let mut spec = sized(&[(0, 0)], &[300.0], &[250.0]);
        assert!(set_figure_offset(
            &mut spec,
            FigureOffset::Left,
            Some(150.0)
        ));
        assert!(set_figure_offset(&mut spec, FigureOffset::Top, Some(1e9)));
        assert_eq!(spec.figure_left, Some(150.0));
        assert_eq!(spec.figure_top, Some(stokesforge_plot::MAX_FRAME_SIZE));
        assert!(set_figure_offset(&mut spec, FigureOffset::Left, None));
        assert_eq!(spec.figure_left, None);
        let before = spec.clone();
        for bad in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(!set_figure_offset(&mut spec, FigureOffset::Left, Some(bad)));
        }
        assert_eq!(spec, before);
        let mut legacy = spec_of(&[(0, 0)]);
        assert!(!set_figure_offset(
            &mut legacy,
            FigureOffset::Top,
            Some(10.0)
        ));
        assert!(!set_fixed_figure(&mut legacy, true));
        assert!(!legacy.fixed_figure && legacy.figure_top.is_none());
        assert!(set_fixed_figure(&mut spec, true) && spec.fixed_figure);
    }
}

#[cfg(test)]
mod free_placement_tests {
    use super::*;
    use stokesforge_plot::Panels;

    fn cell(row: usize, column: usize, frame: (f64, f64, f64, f64)) -> PanelSpec {
        PanelSpec {
            free_frame: Some(FreeFrame {
                left: frame.0,
                top: frame.1,
                width: frame.2,
                height: frame.3,
                stack: 0,
            }),
            ..PanelSpec::at(row, column)
        }
    }

    fn free(panels: Vec<PanelSpec>) -> PlotSpec {
        PlotSpec {
            layer_placement: LayerPlacement::Free,
            ..PlotSpec::empty("free").with_panels(Panels::new(panels))
        }
    }

    fn identities(spec: &PlotSpec) -> Vec<(usize, usize)> {
        spec.panels.iter().map(|p| (p.row, p.column)).collect()
    }

    fn three() -> PlotSpec {
        free(vec![
            cell(0, 0, (100.0, 50.0, 300.0, 200.0)),
            cell(0, 1, (500.0, 10.0, 200.0, 100.0)),
            cell(0, 2, (800.0, 120.0, 150.0, 300.0)),
        ])
    }

    fn frame(spec: &PlotSpec, panel: usize) -> FreeFrame {
        spec.panels[panel].sanitized_free_frame()
    }

    /// Align puts the others on the reference's edge or center and touches
    /// nothing else. Witness: Bottom written with `top`, or Top and Bottom
    /// swapped.
    #[test]
    fn align_lines_the_frames_up_on_the_reference() {
        let reference = frame(&three(), 0);
        for (edge, check) in [
            (AlignEdge::Bottom, 0),
            (AlignEdge::Top, 1),
            (AlignEdge::VCenter, 2),
            (AlignEdge::Left, 3),
            (AlignEdge::Right, 4),
            (AlignEdge::HCenter, 5),
        ] {
            let mut spec = three();
            assert!(align_layers(&mut spec, &[0, 1, 2], edge));
            assert_eq!(frame(&spec, 0), reference, "the reference stays");
            for panel in [1, 2] {
                let (before, after) = (frame(&three(), panel), frame(&spec, panel));
                assert_eq!((after.width, after.height), (before.width, before.height));
                assert_eq!(after.stack, before.stack);
                let key = |f: FreeFrame| match check {
                    0 => f.top + f.height,
                    1 => f.top,
                    2 => f.top + f.height / 2.0,
                    3 => f.left,
                    4 => f.left + f.width,
                    _ => f.left + f.width / 2.0,
                };
                assert_eq!(key(after), key(reference), "{edge:?} panel {panel}");
                // The other coordinate is kept.
                if check <= 2 {
                    assert_eq!(after.left, before.left);
                } else {
                    assert_eq!(after.top, before.top);
                }
            }
        }
    }

    /// Same Width gives the reference's width and keeps left and top.
    /// Witness: assigning the height.
    #[test]
    fn match_size_takes_the_reference_dimension_only() {
        let mut spec = three();
        assert!(match_layer_size(&mut spec, &[0, 1, 2], Dimension::Width));
        for panel in [1, 2] {
            let (before, after) = (frame(&three(), panel), frame(&spec, panel));
            assert_eq!(after.width, 300.0);
            assert_eq!(after.height, before.height);
            assert_eq!((after.left, after.top), (before.left, before.top));
        }
        let mut spec = three();
        assert!(match_layer_size(&mut spec, &[0, 1], Dimension::Height));
        assert_eq!(frame(&spec, 1).height, 200.0);
        assert_eq!(frame(&spec, 1).width, 200.0);
        assert_eq!(frame(&spec, 2), frame(&three(), 2), "not selected");
    }

    /// On the grid, or for a missing layer, nothing changes. Witness: the
    /// Free guard removed.
    #[test]
    fn arranging_is_refused_on_the_grid_and_for_a_missing_layer() {
        let mut grid = three();
        grid.layer_placement = LayerPlacement::Grid;
        let before = grid.clone();
        assert!(!align_layers(&mut grid, &[0, 1], AlignEdge::Left));
        assert!(!match_layer_size(&mut grid, &[0, 1], Dimension::Width));
        assert_eq!(grid, before);
        let mut spec = three();
        let before = spec.clone();
        assert!(!align_layers(&mut spec, &[0, 9], AlignEdge::Left));
        assert!(!match_layer_size(&mut spec, &[0, 9], Dimension::Width));
        assert_eq!(spec, before);
    }

    /// An overlay of the reference counts as the reference: one cell, so every
    /// cell is targeted, active first. Witness: returning the selection as given.
    #[test]
    fn targets_collapse_overlays_and_fall_back_to_all_cells() {
        let mut spec = three();
        let mut overlay = PanelSpec::at(0, 1);
        overlay.overlay = Some(Default::default());
        spec.panels.push(overlay);
        // Panel 3 is an overlay of the cell of panel 1.
        assert_eq!(arrange_targets(&spec, 1, &[3]), [1, 0, 2]);
        assert_eq!(arrange_targets(&spec, 2, &[]), [2, 0, 1]);
        // Several distinct cells: the selection, reference first, no duplicates.
        assert_eq!(arrange_targets(&spec, 2, &[0, 3, 0]), [2, 0, 1]);
        assert_eq!(arrange_targets(&spec, 1, &[2]), [1, 2]);
    }

    /// A overlaps B, B overlaps C, A does not meet C: one row. Witness: a
    /// grouping that only compares with the first member makes two rows.
    #[test]
    fn rows_are_grouped_transitively() {
        let mut spec = free(vec![
            cell(0, 0, (0.0, 0.0, 100.0, 100.0)),
            cell(0, 1, (200.0, 80.0, 100.0, 100.0)),
            cell(0, 2, (400.0, 170.0, 100.0, 100.0)),
        ]);
        assert!(set_grid_placement(&mut spec));
        assert_eq!(identities(&spec), [(0, 0), (0, 1), (0, 2)]);
        assert_eq!(spec.layer_placement, LayerPlacement::Grid);
    }

    /// Extents that only touch are two rows. Witness: `<=` instead of `<`.
    #[test]
    fn touching_vertical_extents_make_two_rows() {
        let mut spec = free(vec![
            cell(0, 0, (0.0, 0.0, 100.0, 100.0)),
            cell(0, 1, (200.0, 100.0, 100.0, 50.0)),
        ]);
        assert!(set_grid_placement(&mut spec));
        assert_eq!(identities(&spec), [(0, 0), (1, 0)]);
    }

    /// Inside a row the columns run left to right, rows top to bottom,
    /// whatever the old identities were.
    #[test]
    fn a_row_is_ordered_by_left_and_rows_by_top() {
        let mut spec = free(vec![
            cell(0, 0, (300.0, 0.0, 100.0, 100.0)),
            cell(0, 1, (0.0, 20.0, 100.0, 100.0)),
            cell(0, 2, (50.0, 500.0, 100.0, 100.0)),
        ]);
        assert!(set_grid_placement(&mut spec));
        assert_eq!(identities(&spec), [(0, 1), (0, 0), (1, 0)]);
    }

    /// C4: each column takes its widest frame, each row its tallest.
    /// Witness: `min` instead of `max`.
    #[test]
    fn the_grid_takes_the_largest_frame_of_each_column_and_row() {
        let mut spec = free(vec![
            cell(0, 0, (0.0, 0.0, 100.0, 80.0)),
            cell(0, 1, (200.0, 10.0, 150.0, 120.0)),
            cell(1, 0, (0.0, 400.0, 180.0, 60.0)),
        ]);
        assert!(set_grid_placement(&mut spec));
        assert_eq!(spec.frame_widths, vec![180.0, 150.0]);
        assert_eq!(spec.frame_heights, vec![120.0, 60.0]);
        assert!(spec.panels.iter().all(|p| p.free_frame.is_none()));
    }

    #[test]
    fn overlays_follow_the_old_identity_of_their_base() {
        let base_a = cell(0, 0, (300.0, 0.0, 100.0, 100.0));
        let base_b = cell(0, 1, (0.0, 0.0, 100.0, 100.0));
        let overlay = PanelSpec::overlay_on(&base_a, OverlaySpec::new(true, true));
        let mut spec = free(vec![base_a, overlay, base_b]);
        assert!(set_grid_placement(&mut spec));
        // B (left 0) is now column 0, A column 1; A's overlay goes with A.
        assert_eq!(identities(&spec), [(0, 1), (0, 1), (0, 0)]);
    }

    #[test]
    fn going_to_the_grid_needs_a_free_graph_and_a_bounded_grid() {
        let mut grid = PlotSpec::empty("grid");
        assert!(!set_grid_placement(&mut grid));
        // 17 rows, the first with 17 cells: 17 x 17 > 256.
        let mut panels: Vec<PanelSpec> = (0..17)
            .map(|i| cell(0, i, (i as f64 * 200.0, 0.0, 100.0, 100.0)))
            .collect();
        panels.extend((1..17).map(|i| cell(i, 0, (0.0, i as f64 * 200.0, 100.0, 100.0))));
        let mut spec = free(panels);
        let before = spec.clone();
        assert!(!set_grid_placement(&mut spec));
        assert_eq!(spec, before, "refused, unchanged");
    }

    #[test]
    fn free_placement_stores_frames_on_base_layers_only() {
        let base = PanelSpec::at(0, 0);
        let overlay = PanelSpec::overlay_on(&base, OverlaySpec::new(true, true));
        let mut spec = PlotSpec::empty("g").with_panels(Panels::new(vec![base, overlay]));
        let frame = FreeFrame {
            left: -10.0,
            top: 5.0,
            width: 300.0,
            height: 200.0,
            stack: 0,
        };
        assert!(
            !set_free_placement(&mut spec, &[Some(frame)]),
            "wrong length"
        );
        assert_eq!(spec.layer_placement, LayerPlacement::Grid);
        assert!(set_free_placement(&mut spec, &[Some(frame), Some(frame)]));
        assert_eq!(spec.layer_placement, LayerPlacement::Free);
        assert_eq!(spec.panels[0].free_frame, Some(frame));
        assert_eq!(spec.panels[1].free_frame, None, "an overlay has none");
    }

    #[test]
    fn set_free_frame_on_an_overlay_writes_its_base() {
        let base = cell(0, 0, (0.0, 0.0, 300.0, 200.0));
        let overlay = PanelSpec::overlay_on(&base, OverlaySpec::new(true, true));
        let mut spec = free(vec![base, overlay]);
        assert!(set_free_frame(&mut spec, 1, FreeField::Left, -50.0));
        assert_eq!(spec.panels[0].free_frame.map(|f| f.left), Some(-50.0));
        assert_eq!(spec.panels[1].free_frame, None);
        assert!(set_free_frame(&mut spec, 0, FreeField::Width, 1e300));
        assert_eq!(
            spec.panels[0].free_frame.map(|f| f.width),
            Some(stokesforge_plot::MAX_FRAME_SIZE)
        );
    }

    #[test]
    fn set_free_frame_refuses_the_grid_a_missing_layer_and_non_finite_values() {
        let mut grid = PlotSpec::empty("g");
        assert!(!set_free_frame(&mut grid, 0, FreeField::Left, 1.0));
        let mut spec = free(vec![cell(0, 0, (0.0, 0.0, 300.0, 200.0))]);
        let before = spec.clone();
        assert!(!set_free_frame(&mut spec, 5, FreeField::Left, 1.0));
        assert!(!set_free_frame(&mut spec, 0, FreeField::Top, f64::NAN));
        assert_eq!(spec, before);
    }

    fn stacked_three() -> PlotSpec {
        free(vec![
            cell(0, 0, (0.0, 0.0, 100.0, 100.0)),
            cell(0, 1, (50.0, 0.0, 100.0, 100.0)),
            cell(0, 2, (100.0, 0.0, 100.0, 100.0)),
        ])
    }

    /// Witness: ties broken by descending index give [2, 1, 0].
    #[test]
    fn equal_stacking_keys_are_drawn_in_creation_order() {
        assert_eq!(stacked_three().free_draw_order(), [0, 1, 2]);
    }

    #[test]
    fn bring_to_front_and_send_to_back_reorder_without_renumbering() {
        let mut spec = stacked_three();
        assert!(bring_to_front(&mut spec, 0));
        assert_eq!(spec.free_draw_order(), [1, 2, 0]);
        assert!(is_in_front(&spec, 0) && !is_at_back(&spec, 0));
        assert!(!bring_to_front(&mut spec, 0), "already in front");
        assert!(send_to_back(&mut spec, 2));
        assert_eq!(spec.free_draw_order(), [2, 1, 0]);
        let keys: Vec<i32> = spec
            .panels
            .iter()
            .map(|p| p.free_frame.unwrap().stack)
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(sorted, [0, 1, 2], "keys rewritten to 0..n, no overflow");
        assert_eq!(
            spec.panels
                .iter()
                .map(|p| (p.row, p.column))
                .collect::<Vec<_>>(),
            [(0, 0), (0, 1), (0, 2)],
            "layer numbers are kept"
        );
    }

    #[test]
    fn send_to_back_puts_the_cell_behind_the_others() {
        let mut spec = stacked_three();
        assert!(send_to_back(&mut spec, 2));
        assert_eq!(spec.free_draw_order(), [2, 0, 1]);
    }

    /// Witness: writing the key on the overlay instead of its base.
    #[test]
    fn an_overlay_acts_on_its_base_cell() {
        let mut spec = stacked_three();
        let overlay = PanelSpec::overlay_on(&spec.panels[0], OverlaySpec::new(true, true));
        let mut panels: Vec<PanelSpec> = spec.panels.iter().cloned().collect();
        panels.insert(1, overlay);
        spec.panels = Panels::new(panels);
        assert!(bring_to_front(&mut spec, 1));
        assert_eq!(spec.free_draw_order(), [2, 3, 0]);
        assert_eq!(spec.panels[1].free_frame, None, "an overlay has none");
        assert!(set_layer_background(&mut spec, 1, "#ff0000"));
        assert_eq!(spec.panels[0].layer_background, "#ff0000");
        assert_eq!(spec.panels[1].layer_background, "");
    }

    #[test]
    fn stacking_and_background_are_refused_on_the_grid() {
        let mut spec = PlotSpec::default();
        assert!(!bring_to_front(&mut spec, 0));
        assert!(!send_to_back(&mut spec, 0));
        assert!(!set_layer_background(&mut spec, 0, "#fff"));
    }

    /// Going back to the grid drops the stacking key and keeps the color.
    #[test]
    fn leaving_free_clears_the_stack_and_keeps_the_background() {
        let mut spec = stacked_three();
        assert!(bring_to_front(&mut spec, 0));
        assert!(set_layer_background(&mut spec, 0, "#ff0000"));
        assert!(set_grid_placement(&mut spec));
        assert!(spec.panels.iter().all(|p| p.free_frame.is_none()));
        assert_eq!(spec.panels[0].layer_background, "#ff0000");
    }

    /// In free placement a new cell only gets an unused identity: nothing
    /// shifts, no column width is stored.
    #[test]
    fn inserting_a_cell_in_free_placement_only_allocates_an_identity() {
        let mut spec = free(vec![
            cell(0, 0, (0.0, 0.0, 100.0, 100.0)),
            cell(0, 1, (200.0, 0.0, 100.0, 100.0)),
        ]);
        let before: Vec<PanelSpec> = spec.panels.iter().cloned().collect();
        let at = insert_cell(&mut spec, 0, CellSide::Right).expect("inserted");
        assert_eq!(at, 2);
        assert_eq!((spec.panels[2].row, spec.panels[2].column), (0, 2));
        // The new cell is in front of the others (#398); its frame is the
        // fallback until the caller gives it one.
        let new_frame = spec.panels[2].free_frame.expect("a frame");
        assert_eq!(new_frame.stack, 1);
        assert_eq!(spec.free_draw_order(), [0, 1, 2]);
        assert_eq!(
            spec.panels.iter().take(2).cloned().collect::<Vec<_>>(),
            before
        );
        assert!(spec.frame_widths.is_empty());
        let below = insert_cell(&mut spec, 0, CellSide::Below).expect("inserted");
        assert_eq!((spec.panels[below].row, spec.panels[below].column), (1, 0));
    }

    /// Deleting a cell leaves the frames of the others alone.
    #[test]
    fn removing_a_cell_in_free_placement_keeps_the_other_frames() {
        let mut spec = free(vec![
            cell(0, 0, (-30.0, 0.0, 100.0, 100.0)),
            cell(0, 1, (200.0, 40.0, 120.0, 90.0)),
        ]);
        let kept = spec.panels[1].free_frame;
        assert!(remove_cell(&mut spec, 0));
        assert_eq!(spec.panels[0].free_frame, kept);
    }

    /// Layers that differ only by their free frame are the same layer.
    /// Witness: the `free_frame` line of `remap_between` removed.
    #[test]
    fn remap_between_ignores_the_free_frame() {
        let before = free(vec![cell(0, 0, (0.0, 0.0, 100.0, 100.0))]);
        let after = free(vec![cell(0, 0, (50.0, 20.0, 300.0, 200.0))]);
        assert_eq!(remap_between(&before, &after), vec![Some(0)]);
    }
}

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

use stokesforge_id::NodeId;
use stokesforge_plot::{OverlaySpec, PanelSpec, PlotSpec, PlotType, SeriesSpec};

use crate::column::ColumnRole;
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
/// **Right**: only the row of the reference cell shifts, the cells to its
/// right (column greater) move one column further, with their overlay layers.
/// **Below**: same on the column. Other rows and columns are untouched.
/// `None`, with the spec unchanged, if `panel` does not exist, if a position
/// would overflow (rows and columns are free `usize`s once read from a file)
/// or if the grid would exceed [`MAX_GRID_CELLS`].
pub fn insert_cell(spec: &mut PlotSpec, panel: usize, side: CellSide) -> Option<usize> {
    let base = spec.panels.get(overlay_base(spec, panel))?;
    let (row, column) = (base.row, base.column);
    let (new_row, new_column) = match side {
        CellSide::Right => (row, column.checked_add(1)?),
        CellSide::Below => (row.checked_add(1)?, column),
    };
    let shifts = |candidate: &PanelSpec| match side {
        CellSide::Right => candidate.row == row && candidate.column > column,
        CellSide::Below => candidate.column == column && candidate.row > row,
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
    for candidate in spec.panels.iter_mut().filter(|candidate| shifts(candidate)) {
        match side {
            CellSide::Right => candidate.column += 1,
            CellSide::Below => candidate.row += 1,
        }
    }
    spec.panels.push(PanelSpec::at(new_row, new_column));
    Some(spec.panels.len() - 1)
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

/// Deletes layer `panel` (#357, see [`layers_removed_with`]). Deleting a cell
/// closes the gap in its row (the cells to its right move back one column)
/// and a row left empty disappears (the rows below move up). The graph title
/// position is in the frame of the first layer, so it is forgotten when that
/// layer goes. `false`, with the spec unchanged, if [`can_remove_layer`] says no.
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
    if !spec.panels.remove_indices(&removed) {
        return false;
    }
    if removed.contains(&0) {
        spec.title_anchor = None;
    }
    if is_cell {
        for candidate in spec.panels.iter_mut() {
            if candidate.row == row && candidate.column > column {
                candidate.column -= 1;
            }
        }
        if !spec.panels.iter().any(|candidate| candidate.row == row) {
            for candidate in spec.panels.iter_mut() {
                if candidate.row > row {
                    candidate.row -= 1;
                }
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
    fn insert_right_shifts_the_cells_to_the_right_with_their_overlays_only() {
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
                (1, 1, false), // other row: untouched
                (0, 1, false), // the new cell
            ]
        );
        assert_eq!(overlay_base(&spec, 2), 1, "overlays still cover their base");
        assert_eq!(overlay_base(&spec, 3), 1);
    }

    #[test]
    fn insert_below_shifts_only_the_column_of_the_reference_cell() {
        let mut spec = spec_of(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(insert_cell(&mut spec, 0, CellSide::Below), Some(4));
        assert_eq!(
            grid(&spec),
            [
                (0, 0, false),
                (0, 1, false),
                (2, 0, false),
                (1, 1, false),
                (1, 0, false),
            ]
        );
    }

    #[test]
    fn insert_right_in_a_grid_touches_one_row_only() {
        let mut spec = spec_of(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(insert_cell(&mut spec, 2, CellSide::Right), Some(4));
        assert_eq!(
            grid(&spec),
            [
                (0, 0, false),
                (0, 1, false),
                (1, 0, false),
                (1, 2, false),
                (1, 1, false),
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
    fn remove_a_cell_takes_its_overlays_closes_the_gap_and_keeps_the_order() {
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
        // Removes the cell (0, 0): 3 layers.
        assert!(remove_cell(&mut spec, 1));
        assert_eq!(grid(&spec), [(0, 0, false), (1, 0, false)]);
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
}

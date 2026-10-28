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

    use ColumnRole::{None as NoRole, X, X2, X3, X4, Y, Y2, Y3, Y4};

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
}

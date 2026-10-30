//! Which component of a graph is under a pixel — title, plot area, axis or
//! axis title.
//!
//! Used by the interface's right click ("properties of what I point at").
//! Lives here rather than in `stokesforge-render-egui` for the same reason as
//! [`crate::interact`]: it is pure geometry on a [`DrawnPlot`], independent of
//! the backend.
//!
//! # Where the geometry comes from
//!
//! Nothing is rebuilt next to the drawing: the plot area
//! ([`DrawnPanel::pixel_origin`]/[`DrawnPanel::pixel_size`]), the thickness
//! of the tick areas ([`DrawnPanel::axis_areas`]) and the title band
//! ([`DrawnPlot::title_height`]) are those that Plotters actually allocated,
//! copied over by [`crate::pipeline`].
//!
//! The only approximation: the split, within a tick area, between the tick
//! labels and the axis title. Plotters puts this title against the **outer**
//! edge of the area (`draw_axis_and_labels`); we give it a band of
//! [`AXIS_TITLE_BAND`] pixels on that side, at most half the area. A click on
//! the exact boundary may thus land on either one — harmless, both open the
//! settings of the same axis.

use crate::pipeline::{AxisArea, DrawnPanel, DrawnPlot, AXIS_DESC_SIZE};
use stokesforge_plot::TextTarget;

/// Thickness given to an axis title in its tick area: the height of a line of
/// [`AXIS_DESC_SIZE`], line spacing included.
pub const AXIS_TITLE_BAND: f64 = AXIS_DESC_SIZE * 1.5;

/// Width, **in points on screen**, of the strip inward from the edge of the
/// plot area that picks the axis line (#385). The caller converts it into
/// figure pixels (`4 / display_scale`) so that it stays the same on screen
/// whatever the display density or the enlargement of the figure.
pub const AXIS_LINE_TOLERANCE_POINTS: f64 = 4.0;

/// Fraction of the size of the plot area that the strip never exceeds, so
/// that a tiny plot keeps an interior.
const AXIS_LINE_MAX_FRACTION: f64 = 0.25;

/// Width, **in points on screen**, of the strip either side of the outer
/// boundary of a cell that picks its resize handle (#394). Converted into
/// figure pixels by the caller, like [`AXIS_LINE_TOLERANCE_POINTS`].
pub const CELL_EDGE_TOLERANCE_POINTS: f64 = 4.0;

/// The strip of a cell handle never exceeds this many figure pixels, so that
/// it stays inside the margin the pipeline leaves outside the axis bands
/// ([`crate::pipeline::MARGIN`], 12 px) and never reaches an axis line or a
/// tick label, whatever the zoom of the display.
pub const CELL_EDGE_MAX_TOLERANCE: f64 = crate::pipeline::MARGIN as f64 / 2.0;

/// The resize handle under a pixel (#394): the outer boundary of a cell, beyond
/// the tick labels and the axis title, never the axis line. Dragging it by a
/// distance changes the **frame** (plot area) of the column and/or the row by
/// that distance. The boundary a cell shares with its right neighbour is the
/// handle of the left column, the one it shares with the cell below is the
/// handle of the upper row; the right edge of the figure is that of the last
/// column and its bottom edge that of the last row. Left and top edges have
/// none. Near the corner of a cell both are set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellEdge {
    /// Grid column whose frame width the vertical boundary changes.
    pub column: Option<usize>,
    /// Grid row whose frame height the horizontal boundary changes.
    pub row: Option<usize>,
}

/// The cell handle at pixel `(x, y)` (backend coordinate space), within
/// `tolerance` figure pixels (bounded by [`CELL_EDGE_MAX_TOLERANCE`]) of the
/// outer boundary of a cell ([`DrawnPanel::cell_origin`]/[`DrawnPanel::cell_size`]).
/// `None` elsewhere, in particular inside a frame and on an axis line.
pub fn cell_edge_at(plot: &DrawnPlot, x: f64, y: f64, tolerance: f64) -> Option<CellEdge> {
    let tolerance = if tolerance.is_finite() {
        tolerance.clamp(0.0, CELL_EDGE_MAX_TOLERANCE)
    } else {
        0.0
    };
    let mut edge = CellEdge {
        column: None,
        row: None,
    };
    for panel in &plot.panels {
        let left = panel.cell_origin.0 as f64;
        let top = panel.cell_origin.1 as f64;
        let right = left + panel.cell_size.0 as f64;
        let bottom = top + panel.cell_size.1 as f64;
        let near_right = (x - right).abs() <= tolerance;
        let near_bottom = (y - bottom).abs() <= tolerance;
        let within_x = x >= left && x <= right + tolerance;
        let within_y = y >= top && y <= bottom + tolerance;
        if near_right && within_y && edge.column.is_none() {
            edge.column = Some(panel.column);
        }
        if near_bottom && within_x && edge.row.is_none() {
            edge.row = Some(panel.row);
        }
    }
    (edge.column.is_some() || edge.row.is_some()).then_some(edge)
}

/// The edge of the figure under a pixel (#405): its left edge, which sets the
/// distance from the image to the frame of the first column (Left), and its top
/// edge, which sets the same for the first row (Top). Both are set near the
/// top-left corner. The right and bottom edges of the figure are the handles
/// of the last column and row ([`CellEdge`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FigureEdge {
    /// Near the left edge of the figure.
    pub left: bool,
    /// Near the top edge of the figure.
    pub top: bool,
}

/// The figure edge at pixel `(x, y)` (backend coordinate space), within
/// `tolerance` figure pixels (bounded by [`CELL_EDGE_MAX_TOLERANCE`], so it
/// never reaches the axis line the margin keeps away). `None` elsewhere and
/// for a figure with no drawn panel.
pub fn figure_edge_at(plot: &DrawnPlot, x: f64, y: f64, tolerance: f64) -> Option<FigureEdge> {
    if plot.panels.is_empty() {
        return None;
    }
    let tolerance = if tolerance.is_finite() {
        tolerance.clamp(0.0, CELL_EDGE_MAX_TOLERANCE)
    } else {
        0.0
    };
    let edge = FigureEdge {
        left: (0.0..=tolerance).contains(&x),
        top: (0.0..=tolerance).contains(&y),
    };
    (edge.left || edge.top).then_some(edge)
}

/// An axis of a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisSide {
    /// Abscissas, below the plot area.
    Bottom,
    /// Main ordinate, on the left.
    Left,
    /// Secondary ordinate, on the right.
    Right,
    /// The top mirror axis of an ordinary layer (#396). An overlaid layer's
    /// X axis, drawn at the top, is still reported as `Bottom`.
    Top,
}

/// A component of a graph, picked by a pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotComponent {
    /// The graph title, at the top of the figure.
    Title,
    /// The plot area of the panel with the given index (in
    /// [`DrawnPlot::panels`]).
    PlotArea { panel: usize },
    /// An axis: its line and its tick labels.
    Axis { panel: usize, side: AxisSide },
    /// An axis title.
    AxisTitle { panel: usize, side: AxisSide },
    /// A free text (issue #35): `index` in `PanelSpec::annotations`.
    /// Takes priority over everything else — it is drawn on top.
    Annotation { panel: usize, index: usize },
    /// A graphic object (issue #76): `index` in `PanelSpec::shapes`.
    /// Picked on its stroke, or inside it if filled; takes priority over the
    /// plot area, below the free texts.
    Shape { panel: usize, index: usize },
    /// A title or a legend (issue #53): double-click to edit it,
    /// drag to move it. Drawn over the plot area like the free texts, hence
    /// it takes priority over it and over the axes.
    Text(TextTarget),
    /// A legend entry: the series of panel `panel` (rank in
    /// `PanelSpec::series`) that it shows. Takes priority over [`Self::Text`]
    /// of the same legend.
    LegendEntry { panel: usize, series: usize },
}

/// The component under pixel `(x, y)` (backend coordinate space, that of
/// [`DrawnPanel::pixel_origin`]), or `None` if it only points at the
/// background (margins, space between panels).
pub fn component_at(plot: &DrawnPlot, x: f64, y: f64) -> Option<PlotComponent> {
    component_at_with_tolerance(plot, x, y, AXIS_LINE_TOLERANCE_POINTS)
}

/// Same as [`component_at`], with the width of the axis-line strip given in
/// **figure pixels** (#385): the caller passes
/// `AXIS_LINE_TOLERANCE_POINTS / display_scale`. It is clamped to a quarter
/// of the plot area; a negative or non-finite value means no strip.
pub fn component_at_with_tolerance(
    plot: &DrawnPlot,
    x: f64,
    y: f64,
    line_tolerance: f64,
) -> Option<PlotComponent> {
    // The last drawn is on top: search from the last to the first.
    for (panel, drawn) in plot.panels.iter().enumerate().rev() {
        if let Some(annotation) = drawn.annotations.iter().rev().find(|a| a.contains(x, y)) {
            return Some(PlotComponent::Annotation {
                panel,
                index: annotation.index,
            });
        }
    }
    // The graphic objects (issue #76): the topmost first, and only within
    // the plot area where they are clipped.
    for (panel, drawn) in plot.panels.iter().enumerate().rev() {
        let (left, top) = (drawn.pixel_origin.0 as f64, drawn.pixel_origin.1 as f64);
        let inside = x >= left
            && x <= left + drawn.pixel_size.0 as f64
            && y >= top
            && y <= top + drawn.pixel_size.1 as f64;
        if !inside {
            continue;
        }
        if let Some(shape) = drawn.shapes.iter().rev().find(|s| s.contains(x, y)) {
            return Some(PlotComponent::Shape {
                panel,
                index: shape.index,
            });
        }
    }
    // The titles and legends (issue #53), from the last panel to the first;
    // the legend first by its rows, then by its frame.
    for (panel, drawn) in plot.panels.iter().enumerate().rev() {
        let on_legend = drawn
            .texts
            .iter()
            .any(|text| matches!(text.target, TextTarget::Legend { .. }) && text.contains(x, y));
        if on_legend {
            let row = drawn
                .legend_rows
                .iter()
                .find(|row| (y - row.center_y).abs() <= row.height / 2.0);
            return Some(match row {
                Some(row) => PlotComponent::LegendEntry {
                    panel,
                    series: row.series,
                },
                None => PlotComponent::Text(TextTarget::Legend { panel: drawn.index }),
            });
        }
        if let Some(text) = drawn.texts.iter().rev().find(|text| text.contains(x, y)) {
            return Some(PlotComponent::Text(text.target));
        }
    }
    if let Some(title) = plot.title.filter(|title| title.contains(x, y)) {
        return Some(PlotComponent::Text(title.target));
    }
    if y >= 0.0 && y < plot.title_height as f64 {
        return Some(PlotComponent::Title);
    }
    // The tick areas first, all layers together: overlaid layers (#167)
    // share the plot area, but each has its own axes.
    plot.panels
        .iter()
        .enumerate()
        .find_map(|(index, panel)| axis_component_at(index, panel, x, y))
        .or_else(|| {
            // Then the axis lines: a strip inside the plot area (#385).
            plot.panels
                .iter()
                .enumerate()
                .find_map(|(index, panel)| axis_line_at(index, panel, x, y, line_tolerance))
        })
        .or_else(|| {
            plot.panels.iter().enumerate().find_map(|(index, panel)| {
                let left = panel.pixel_origin.0 as f64;
                let top = panel.pixel_origin.1 as f64;
                let inside = (left..=left + panel.pixel_size.0 as f64).contains(&x)
                    && (top..=top + panel.pixel_size.1 as f64).contains(&y);
                inside.then_some(PlotComponent::PlotArea { panel: index })
            })
        })
}

/// The axis of a panel whose tick area contains `(x, y)`. The reported side
/// is that of the axis **in the spec**: an overlaid layer shows its X axis
/// at the top and its Y axis on the right, but they are still `Bottom` and
/// `Left`.
fn axis_component_at(index: usize, panel: &DrawnPanel, x: f64, y: f64) -> Option<PlotComponent> {
    let left = panel.pixel_origin.0 as f64;
    let top = panel.pixel_origin.1 as f64;
    let right = left + panel.pixel_size.0 as f64;
    let bottom = top + panel.pixel_size.1 as f64;
    let within_x = (left..=right).contains(&x);
    let within_y = (top..=bottom).contains(&y);
    let areas = &panel.axis_areas;
    let (top_side, right_side) = if panel.overlay.is_some() {
        (AxisSide::Bottom, AxisSide::Left)
    } else {
        (AxisSide::Top, AxisSide::Right)
    };
    // Distance to the edge of the plot area, outward, in each candidate
    // area.
    let candidates = [
        (AxisSide::Bottom, within_x, y - bottom, areas.bottom),
        (top_side, within_x, top - y, areas.top),
        (AxisSide::Left, within_y, left - x, areas.left),
        (right_side, within_y, x - right, areas.right),
    ];
    candidates
        .into_iter()
        .find_map(|(side, aligned, depth, area)| {
            let offset = area.offset as f64;
            (aligned && depth > offset && depth <= offset + area.size as f64)
                .then(|| axis_part(index, side, depth - offset, area))
        })
}

/// The axis whose **line** (edge of the plot area) is within `tolerance`
/// figure pixels inward of `(x, y)` (#385). No strip for an edge without an
/// axis area (`size == 0`) nor for a stacked band (`offset > 0`, #168). In a
/// corner the nearest edge wins, a tie going to the first candidate.
fn axis_line_at(
    index: usize,
    panel: &DrawnPanel,
    x: f64,
    y: f64,
    tolerance: f64,
) -> Option<PlotComponent> {
    let left = panel.pixel_origin.0 as f64;
    let top = panel.pixel_origin.1 as f64;
    let width = panel.pixel_size.0 as f64;
    let height = panel.pixel_size.1 as f64;
    let (right, bottom) = (left + width, top + height);
    let within_x = (left..=right).contains(&x);
    let within_y = (top..=bottom).contains(&y);
    let areas = &panel.axis_areas;
    let (top_side, right_side) = if panel.overlay.is_some() {
        (AxisSide::Bottom, AxisSide::Left)
    } else {
        (AxisSide::Top, AxisSide::Right)
    };
    let tolerance = if tolerance.is_finite() {
        tolerance.max(0.0)
    } else {
        0.0
    };
    let horizontal = tolerance.min(height * AXIS_LINE_MAX_FRACTION);
    let vertical = tolerance.min(width * AXIS_LINE_MAX_FRACTION);
    // Distance to the edge, inward, and the strip width of that edge.
    let candidates = [
        (
            AxisSide::Bottom,
            within_x,
            bottom - y,
            horizontal,
            areas.bottom,
        ),
        (top_side, within_x, y - top, horizontal, areas.top),
        (AxisSide::Left, within_y, x - left, vertical, areas.left),
        (right_side, within_y, right - x, vertical, areas.right),
    ];
    candidates
        .into_iter()
        .filter(|(_, aligned, inward, strip, area)| {
            *aligned && area.size > 0 && area.offset == 0 && (0.0..=*strip).contains(inward)
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(side, ..)| PlotComponent::Axis { panel: index, side })
}

/// Axis or axis title, depending on the click depth in the area.
fn axis_part(panel: usize, side: AxisSide, depth: f64, area: AxisArea) -> PlotComponent {
    let size = area.size as f64;
    let band = AXIS_TITLE_BAND.min(size / 2.0);
    if area.titled && depth > size - band {
        PlotComponent::AxisTitle { panel, side }
    } else {
        PlotComponent::Axis { panel, side }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::AxisAreas;
    use stokesforge_plot::AxisScale;

    /// A 480×360 figure: 40 px title, plot area from (80, 60) to
    /// (440, 280), 52 px bottom area, 64 px left area, no secondary
    /// axis.
    fn figure() -> DrawnPlot {
        let area = |size| AxisArea {
            size,
            titled: true,
            offset: 0,
        };
        DrawnPlot {
            panels: vec![DrawnPanel {
                index: 0,
                overlay: None,
                row: 0,
                column: 0,
                x_range: (0.0, 1.0),
                y_range: (0.0, 1.0),
                y2_range: None,
                pixel_origin: (80, 60),
                pixel_size: (360, 220),
                cell_origin: (0, 40),
                cell_size: (480, 320),
                x_scale: AxisScale::Linear,
                y_scale: AxisScale::Linear,
                y2_scale: AxisScale::Linear,
                drawn_series: 0,
                broken_series: 0,
                unknown_series: 0,
                legend_entries: 0,
                decimated_points: 0,
                axis_areas: AxisAreas {
                    bottom: area(52),
                    left: area(64),
                    right: AxisArea::default(),
                    top: AxisArea::default(),
                },
                annotations: Vec::new(),
                shapes: Vec::new(),
                series_geometry: Vec::new(),
                texts: Vec::new(),
                legend_rows: Vec::new(),
            }],
            omitted_panels: 0,
            drawn_series: 0,
            broken_series: 0,
            unknown_series: 0,
            decimated_points: 0,
            title_height: 40,
            title: None,
        }
    }

    #[test]
    fn each_zone_picks_its_component() {
        let plot = figure();
        let panel = 0;
        assert_eq!(component_at(&plot, 240.0, 20.0), Some(PlotComponent::Title));
        assert_eq!(
            component_at(&plot, 240.0, 150.0),
            Some(PlotComponent::PlotArea { panel })
        );
        // Just below the plot area: the ticks of the bottom axis.
        assert_eq!(
            component_at(&plot, 240.0, 290.0),
            Some(PlotComponent::Axis {
                panel,
                side: AxisSide::Bottom
            })
        );
        // Against the outer edge of the bottom area: its title.
        assert_eq!(
            component_at(&plot, 240.0, 328.0),
            Some(PlotComponent::AxisTitle {
                panel,
                side: AxisSide::Bottom
            })
        );
        assert_eq!(
            component_at(&plot, 70.0, 150.0),
            Some(PlotComponent::Axis {
                panel,
                side: AxisSide::Left
            })
        );
        assert_eq!(
            component_at(&plot, 20.0, 150.0),
            Some(PlotComponent::AxisTitle {
                panel,
                side: AxisSide::Left
            })
        );
        // Margin on the right, no secondary axis: nothing.
        assert_eq!(component_at(&plot, 460.0, 150.0), None);
        // Bottom-left corner, outside both areas: nothing.
        assert_eq!(component_at(&plot, 40.0, 320.0), None);
    }

    /// #168: the axis of an overlaid layer stacked in a second band is only
    /// reached in that band, not in that of the older layer.
    #[test]
    fn an_offset_band_picks_only_its_layer() {
        let mut plot = figure();
        let mut second = plot.panels[0].clone();
        second.index = 1;
        second.overlay = Some(stokesforge_plot::OverlaySpec::SECOND_AXES);
        plot.panels[0].axis_areas.top = AxisArea {
            size: 8,
            titled: false,
            offset: 0,
        };
        second.axis_areas.top = AxisArea {
            size: 8,
            titled: false,
            offset: 8,
        };
        plot.panels.push(second);
        // The plot area starts at y = 60: 0-8 px above for the
        // first layer, 8-16 px for the second.
        let axis = |panel, side| Some(PlotComponent::Axis { panel, side });
        // The ordinary layer reports its top mirror as `Top` (#396); the
        // overlaid one reports the field it draws there, `Bottom`.
        assert_eq!(component_at(&plot, 240.0, 56.0), axis(0, AxisSide::Top));
        assert_eq!(component_at(&plot, 240.0, 48.0), axis(1, AxisSide::Bottom));
    }

    #[test]
    fn an_untitled_axis_has_no_title_band() {
        let mut plot = figure();
        plot.panels[0].axis_areas.bottom.titled = false;
        assert_eq!(
            component_at(&plot, 240.0, 328.0),
            Some(PlotComponent::Axis {
                panel: 0,
                side: AxisSide::Bottom
            })
        );
    }

    /// Issue #35: a rotated text is picked on its **rotated** rectangle,
    /// not on its bounding box, and goes in front of the plot area and of the
    /// axis title that it covers.
    #[test]
    fn a_rotated_text_is_picked_on_its_rotated_rectangle() {
        use crate::pipeline::DrawnAnnotation;
        let mut plot = figure();
        // 100 × 20 px, rotated by a quarter turn: vertical, centered at
        // (240, 150).
        plot.panels[0].annotations.push(DrawnAnnotation {
            index: 3,
            center: (240.0, 150.0),
            size: (100, 20),
            rotation: 90.0,
        });
        let annotation = Some(PlotComponent::Annotation { panel: 0, index: 3 });
        assert_eq!(
            component_at(&plot, 240.0, 105.0),
            annotation,
            "top of the text"
        );
        assert_eq!(component_at(&plot, 245.0, 190.0), annotation);
        // Inside the box of the **upright** text, but outside the rotated text.
        assert_eq!(
            component_at(&plot, 280.0, 150.0),
            Some(PlotComponent::PlotArea { panel: 0 })
        );
        // At 45°, a corner of the bounding box is not the text.
        plot.panels[0].annotations[0].rotation = 45.0;
        assert_eq!(component_at(&plot, 240.0, 150.0), annotation, "center");
        assert_ne!(
            component_at(&plot, 280.0, 185.0),
            annotation,
            "empty corner"
        );
        assert_eq!(
            component_at(&plot, 270.0, 120.0),
            annotation,
            "along the text"
        );
    }

    /// Issue #53: a title, a rotated axis title or a legend are picked
    /// on their (rotated) rectangle, before the plot area that they cover,
    /// and a legend row before the whole frame.
    #[test]
    fn a_title_or_a_legend_is_picked_before_the_plot_area() {
        use crate::pipeline::{DrawnLegendRow, DrawnText};
        use stokesforge_plot::AxisSide as Side;
        let mut plot = figure();
        let left = TextTarget::AxisTitle {
            panel: 0,
            side: Side::Left,
        };
        plot.panels[0].texts = vec![
            // Vertical axis title, moved right into the plot area.
            DrawnText {
                target: left,
                center: (200.0, 150.0),
                size: (120, 20),
                rotation: 90.0,
                anchored: true,
            },
            DrawnText {
                target: TextTarget::Legend { panel: 0 },
                center: (380.0, 100.0),
                size: (100, 60),
                rotation: 0.0,
                anchored: false,
            },
        ];
        plot.panels[0].legend_rows = vec![
            DrawnLegendRow {
                series: 0,
                center_y: 85.0,
                height: 20.0,
            },
            DrawnLegendRow {
                series: 3,
                center_y: 110.0,
                height: 20.0,
            },
        ];
        assert_eq!(
            component_at(&plot, 200.0, 100.0),
            Some(PlotComponent::Text(left)),
            "top of the vertical title"
        );
        assert_eq!(
            component_at(&plot, 230.0, 150.0),
            Some(PlotComponent::PlotArea { panel: 0 }),
            "next to the rotated title: the plot area"
        );
        assert_eq!(
            component_at(&plot, 380.0, 112.0),
            Some(PlotComponent::LegendEntry {
                panel: 0,
                series: 3
            })
        );
        assert_eq!(
            component_at(&plot, 380.0, 72.0),
            Some(PlotComponent::Text(TextTarget::Legend { panel: 0 })),
            "the frame, outside the rows"
        );
        plot.title = Some(DrawnText {
            target: TextTarget::Title,
            center: (240.0, 20.0),
            size: (100, 24),
            rotation: 0.0,
            anchored: false,
        });
        assert_eq!(
            component_at(&plot, 240.0, 20.0),
            Some(PlotComponent::Text(TextTarget::Title))
        );
        assert_eq!(
            component_at(&plot, 40.0, 20.0),
            Some(PlotComponent::Title),
            "the title band, outside the text, keeps its properties"
        );
    }

    #[test]
    fn without_a_title_the_top_of_the_figure_is_not_the_title() {
        let mut plot = figure();
        plot.title_height = 0;
        assert_eq!(component_at(&plot, 240.0, 20.0), None);
    }

    fn axis(panel: usize, side: AxisSide) -> Option<PlotComponent> {
        Some(PlotComponent::Axis { panel, side })
    }

    /// #385: the line of an axis (edge of the plot area) picks the axis,
    /// within a strip inward from the edge.
    #[test]
    fn the_axis_line_is_a_strip_inward_from_the_edge() {
        let plot = figure();
        let area = Some(PlotComponent::PlotArea { panel: 0 });
        assert_eq!(
            component_at(&plot, 240.0, 280.0),
            axis(0, AxisSide::Bottom),
            "the edge pixel"
        );
        assert_eq!(component_at(&plot, 240.0, 277.0), axis(0, AxisSide::Bottom));
        assert_eq!(component_at(&plot, 83.0, 150.0), axis(0, AxisSide::Left));
        assert_eq!(component_at(&plot, 80.0, 150.0), axis(0, AxisSide::Left));
        assert_eq!(component_at(&plot, 240.0, 276.0), axis(0, AxisSide::Bottom));
        assert_eq!(component_at(&plot, 240.0, 275.5), area, "beyond the strip");
        assert_eq!(component_at(&plot, 240.0, 274.0), area);
        assert_eq!(component_at(&plot, 85.0, 150.0), area);
    }

    /// #385: no strip for an edge without an axis area, nor for a hidden axis.
    #[test]
    fn no_strip_without_an_axis_area() {
        let mut plot = figure();
        let area = Some(PlotComponent::PlotArea { panel: 0 });
        // No top / right axis in the fixture.
        assert_eq!(component_at(&plot, 240.0, 62.0), area);
        assert_eq!(component_at(&plot, 438.0, 150.0), area);
        // Hidden bottom axis: its area has no size.
        plot.panels[0].axis_areas.bottom = AxisArea::default();
        assert_eq!(component_at(&plot, 240.0, 277.0), area);
    }

    /// #385: in a corner, the nearest edge wins.
    #[test]
    fn the_nearest_edge_wins_in_a_corner() {
        let plot = figure();
        assert_eq!(component_at(&plot, 81.0, 277.0), axis(0, AxisSide::Left));
        assert_eq!(component_at(&plot, 83.0, 279.0), axis(0, AxisSide::Bottom));
        // Equidistant: the first candidate (bottom).
        assert_eq!(component_at(&plot, 82.0, 278.0), axis(0, AxisSide::Bottom));
        // Corner outside the plot area: nothing.
        assert_eq!(component_at(&plot, 79.0, 281.0), None);
    }

    /// #385: the overlaid layer keeps its spec sides on its own edges.
    #[test]
    fn an_overlay_strip_maps_to_its_spec_sides() {
        let mut plot = figure();
        let mut second = plot.panels[0].clone();
        second.index = 1;
        second.overlay = Some(stokesforge_plot::OverlaySpec::SECOND_AXES);
        let shown = AxisArea {
            size: 30,
            titled: false,
            offset: 0,
        };
        second.axis_areas = AxisAreas {
            bottom: AxisArea::default(),
            left: AxisArea::default(),
            top: shown,
            right: shown,
        };
        plot.panels.push(second);
        assert_eq!(component_at(&plot, 240.0, 62.0), axis(1, AxisSide::Bottom));
        assert_eq!(component_at(&plot, 438.0, 150.0), axis(1, AxisSide::Left));
        // The first layer keeps its own edges.
        assert_eq!(component_at(&plot, 240.0, 278.0), axis(0, AxisSide::Bottom));
    }

    /// #385 / #168: a stacked band (offset > 0) has no strip, and the strip
    /// of the nearest layer never goes to the stacked one.
    #[test]
    fn a_stacked_band_has_no_strip() {
        let mut plot = figure();
        let mut second = plot.panels[0].clone();
        second.index = 1;
        second.overlay = Some(stokesforge_plot::OverlaySpec::SECOND_AXES);
        plot.panels[0].axis_areas.top = AxisArea {
            size: 8,
            titled: false,
            offset: 0,
        };
        second.axis_areas.top = AxisArea {
            size: 8,
            titled: false,
            offset: 8,
        };
        second.axis_areas.bottom = AxisArea::default();
        second.axis_areas.left = AxisArea::default();
        plot.panels.push(second);
        assert_eq!(component_at(&plot, 240.0, 62.0), axis(0, AxisSide::Top));
        // Only the stacked layer has a top axis: no strip at all.
        plot.panels[0].axis_areas.top = AxisArea::default();
        assert_eq!(
            component_at(&plot, 240.0, 62.0),
            Some(PlotComponent::PlotArea { panel: 0 })
        );
    }

    /// #385: the strip never eats a tiny plot, and a degenerate one does not
    /// panic (invariant 10).
    #[test]
    fn a_tiny_plot_keeps_an_interior() {
        let mut plot = figure();
        plot.panels[0].pixel_size = (8, 8);
        plot.panels[0].axis_areas.top = AxisArea {
            size: 10,
            titled: false,
            offset: 0,
        };
        // Clamped to 2 px: the centre (4 px from every edge) stays interior.
        assert_eq!(
            component_at(&plot, 84.0, 64.0),
            Some(PlotComponent::PlotArea { panel: 0 })
        );
        assert_eq!(component_at(&plot, 84.0, 66.0), axis(0, AxisSide::Bottom));
        plot.panels[0].pixel_size = (0, 0);
        let _ = component_at(&plot, 80.0, 60.0);
        let _ = component_at_with_tolerance(&plot, 80.0, 60.0, f64::NAN);
    }

    /// #385: the tolerance is in figure pixels given by the caller: a figure
    /// shown at 2x gets half the strip, so it stays 4 points on screen.
    #[test]
    fn the_tolerance_is_given_by_the_caller() {
        let plot = figure();
        let area = Some(PlotComponent::PlotArea { panel: 0 });
        let at = |y, tolerance| component_at_with_tolerance(&plot, 240.0, y, tolerance);
        assert_eq!(
            at(277.0, AXIS_LINE_TOLERANCE_POINTS),
            axis(0, AxisSide::Bottom)
        );
        assert_eq!(at(277.0, AXIS_LINE_TOLERANCE_POINTS / 2.0), area);
        assert_eq!(
            at(279.0, AXIS_LINE_TOLERANCE_POINTS / 2.0),
            axis(0, AxisSide::Bottom)
        );
        assert_eq!(
            at(272.0, AXIS_LINE_TOLERANCE_POINTS * 2.0),
            axis(0, AxisSide::Bottom)
        );
        assert_eq!(at(280.0, 0.0), axis(0, AxisSide::Bottom), "edge pixel");
        assert_eq!(at(279.0, 0.0), area);
    }

    /// #394: the handle sits on the outer boundary of the cell (480 wide, from
    /// y = 40 to 360 in the fixture), not on the axis line of the plot area.
    #[test]
    fn the_cell_handle_is_on_the_outer_boundary_never_on_the_axis_line() {
        let plot = figure();
        let tol = CELL_EDGE_TOLERANCE_POINTS;
        let column = |c| {
            Some(CellEdge {
                column: Some(c),
                row: None,
            })
        };
        // Inside the frame, and on its axis lines: no handle.
        assert_eq!(cell_edge_at(&plot, 240.0, 150.0, tol), None);
        assert_eq!(cell_edge_at(&plot, 438.0, 150.0, tol), None, "frame edge");
        assert_eq!(
            cell_edge_at(&plot, 240.0, 279.0, tol),
            None,
            "bottom axis line"
        );
        assert_eq!(
            cell_edge_at(&plot, 82.0, 150.0, tol),
            None,
            "left axis line"
        );
        // Tick labels and the axis title are inside the cell: no handle.
        assert_eq!(cell_edge_at(&plot, 240.0, 300.0, tol), None);
        // Left and top boundaries are inactive.
        assert_eq!(cell_edge_at(&plot, 0.0, 150.0, tol), None);
        assert_eq!(cell_edge_at(&plot, 240.0, 40.0, tol), None);
        // Outer boundary, either side within the tolerance.
        assert_eq!(cell_edge_at(&plot, 480.0, 150.0, tol), column(0));
        assert_eq!(cell_edge_at(&plot, 476.0, 150.0, tol), column(0));
        assert_eq!(cell_edge_at(&plot, 484.0, 150.0, tol), column(0));
        assert_eq!(cell_edge_at(&plot, 470.0, 150.0, tol), None);
        assert_eq!(
            cell_edge_at(&plot, 240.0, 358.0, tol),
            Some(CellEdge {
                column: None,
                row: Some(0)
            })
        );
        // Corner: both at once.
        assert_eq!(
            cell_edge_at(&plot, 479.0, 361.0, tol),
            Some(CellEdge {
                column: Some(0),
                row: Some(0)
            })
        );
    }

    /// #405: the left and top edges of the figure are handles of their own;
    /// the boundary two columns share stays the width handle of the left one.
    #[test]
    fn the_figure_edges_are_the_left_and_top_handles() {
        let plot = figure();
        let tol = CELL_EDGE_TOLERANCE_POINTS;
        let edge = |left, top| Some(FigureEdge { left, top });
        assert_eq!(figure_edge_at(&plot, 0.0, 150.0, tol), edge(true, false));
        assert_eq!(figure_edge_at(&plot, 4.0, 150.0, tol), edge(true, false));
        assert_eq!(figure_edge_at(&plot, 5.0, 150.0, tol), None);
        assert_eq!(figure_edge_at(&plot, 240.0, 0.0, tol), edge(false, true));
        assert_eq!(figure_edge_at(&plot, 1.0, 2.0, tol), edge(true, true));
        // The axis line (x = 82) and the frame are not figure edges.
        assert_eq!(figure_edge_at(&plot, 82.0, 150.0, tol), None);
        assert_eq!(figure_edge_at(&plot, 240.0, 150.0, tol), None);
        // The strip is bounded by half the margin; hostile tolerances do not
        // panic.
        assert_eq!(figure_edge_at(&plot, 8.0, 150.0, 1000.0), None);
        assert_eq!(figure_edge_at(&plot, 1.0, 150.0, f64::NAN), None);
        assert!(figure_edge_at(&plot, 0.0, 150.0, f64::NAN).is_some());
        // The shared boundary of two columns keeps its width handle.
        let mut two = figure();
        two.panels[0].cell_size = (240, 320);
        let mut right = two.panels[0].clone();
        right.index = 1;
        right.column = 1;
        right.cell_origin = (240, 40);
        two.panels.push(right);
        assert_eq!(figure_edge_at(&two, 240.0, 100.0, tol), None);
        assert_eq!(
            cell_edge_at(&two, 240.0, 100.0, tol),
            Some(CellEdge {
                column: Some(0),
                row: None
            })
        );
    }

    /// #394: the strip is bounded so that it never reaches an axis line, and a
    /// hostile tolerance does not panic.
    #[test]
    fn the_cell_handle_strip_is_bounded() {
        let plot = figure();
        assert_eq!(
            cell_edge_at(&plot, 470.0, 150.0, 1000.0),
            None,
            "bounded by half the margin"
        );
        assert!(cell_edge_at(&plot, 476.0, 150.0, 1000.0).is_some());
        assert_eq!(cell_edge_at(&plot, 479.0, 150.0, f64::NAN), None);
        assert!(cell_edge_at(&plot, 480.0, 150.0, f64::NAN).is_some());
    }

    /// #394: the boundary two cells share is the handle of the left column and
    /// of the upper row; an overlay shares its base cell.
    #[test]
    fn a_shared_boundary_belongs_to_the_left_column_and_the_upper_row() {
        let mut plot = figure();
        plot.panels[0].cell_size = (240, 160);
        let mut right = plot.panels[0].clone();
        right.index = 1;
        right.column = 1;
        right.cell_origin = (240, 40);
        let mut below = plot.panels[0].clone();
        below.index = 2;
        below.row = 1;
        below.cell_origin = (0, 200);
        plot.panels.extend([right, below]);
        let tol = CELL_EDGE_TOLERANCE_POINTS;
        assert_eq!(
            cell_edge_at(&plot, 240.0, 100.0, tol),
            Some(CellEdge {
                column: Some(0),
                row: None
            })
        );
        assert_eq!(
            cell_edge_at(&plot, 100.0, 200.0, tol),
            Some(CellEdge {
                column: None,
                row: Some(0)
            })
        );
        assert_eq!(
            cell_edge_at(&plot, 480.0, 100.0, tol),
            Some(CellEdge {
                column: Some(1),
                row: None
            })
        );
        assert_eq!(
            cell_edge_at(&plot, 100.0, 360.0, tol),
            Some(CellEdge {
                column: None,
                row: Some(1)
            })
        );
    }
}

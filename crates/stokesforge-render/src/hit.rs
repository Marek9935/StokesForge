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

/// An axis of a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisSide {
    /// Abscissas, below the plot area.
    Bottom,
    /// Main ordinate, on the left.
    Left,
    /// Secondary ordinate, on the right.
    Right,
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
    let right_side = if panel.overlay.is_some() {
        AxisSide::Left
    } else {
        AxisSide::Right
    };
    // Distance to the edge of the plot area, outward, in each candidate
    // area.
    let candidates = [
        (AxisSide::Bottom, within_x, y - bottom, areas.bottom),
        (AxisSide::Bottom, within_x, top - y, areas.top),
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
        let axis = |panel| {
            Some(PlotComponent::Axis {
                panel,
                side: AxisSide::Bottom,
            })
        };
        assert_eq!(component_at(&plot, 240.0, 56.0), axis(0));
        assert_eq!(component_at(&plot, 240.0, 48.0), axis(1));
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
}

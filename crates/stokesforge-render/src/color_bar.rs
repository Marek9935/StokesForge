//! The colour bar of a layer (issue #499, step C).
//!
//! This module holds what the figure needs to **colour its curves by value**:
//! [`value_colors`] gives, for each series of a layer, the colour its value
//! takes on the layer's bar, or `None` when it keeps its own. The colour is
//! computed at each draw and handed to the painting as a copy of the style
//! (`Prepared::value_color`); it is never written to the `SeriesSpec`.

use stokesforge_core::resolve::{ResolvedPanel, ResolvedSeries};
use stokesforge_plot::{value_color, ColorBarSpec, GRADIENT_PRESETS};

/// The value colour of each series of `panel`, in `panel.series` order.
///
/// A series is on the bar when the layer has a visible bar, the series is
/// visible, not broken, belongs to a group with a segment, and has a value
/// (`ResolvedSeries::value`) that maps onto the segment's range. The range
/// of a segment is its typed ends completed by the smallest and largest value
/// of its members on the bar (hidden and broken members do not count). The
/// gradient is the one the group retains, the first preset if it retained
/// none.
pub fn value_colors(panel: &ResolvedPanel<'_>) -> Vec<Option<String>> {
    let mut colors = vec![None; panel.series.len()];
    let Some(bar) = panel.spec.color_bar.as_ref().filter(|bar| bar.visible) else {
        return colors;
    };
    // First segment of a group wins; a group is never in two segments.
    let mut seen = Vec::new();
    for segment in &bar.segments {
        if seen.contains(&segment.group) {
            continue;
        }
        seen.push(segment.group);
        let members: Vec<usize> = panel
            .series
            .iter()
            .enumerate()
            .filter(|(_, series)| on_bar(series, segment.group))
            .map(|(index, _)| index)
            .collect();
        let values: Vec<f64> = members
            .iter()
            .filter_map(|&index| panel.series[index].value)
            .collect();
        let Some(range) = ColorBarSpec::natural_range(segment, &values) else {
            continue;
        };
        let stops: &[String] = panel.spec.group_gradient(segment.group).unwrap_or(&[]);
        for index in members {
            let Some(value) = panel.series[index].value else {
                continue;
            };
            colors[index] = if stops.is_empty() {
                value_color(bar, GRADIENT_PRESETS[0].stops, range, value)
            } else {
                value_color(bar, stops, range, value)
            };
        }
    }
    colors
}

/// True if `series` counts for the segment of `group`.
fn on_bar(series: &ResolvedSeries<'_>, group: u32) -> bool {
    series.spec.visible && !series.broken() && series.spec.group == Some(group)
}

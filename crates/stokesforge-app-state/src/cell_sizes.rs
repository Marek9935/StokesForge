//! Making the sizes of a graph's cells explicit (#394).
//!
//! A graph from an old file stores no cell size (`column_widths` and
//! `row_heights` empty): it is drawn 480 x 360 pt shared equally, the title band
//! inside. Before any size edit (a field, a handle, a cell added or deleted)
//! the sizes it is *drawn with* are written into the spec, so the existing
//! cells keep their exact size and only the edited one changes. Core does not
//! measure text, so the measured values come from the caller: the layout
//! `figure_layout` computed, as the screen and the exports measure it.

use stokesforge_core::compose::{set_cell_size, CellAxis};
use stokesforge_plot::PlotSpec;

/// True if at least one of the two size vectors is empty, i.e. the graph still
/// lacks explicit sizes.
pub fn is_legacy(spec: &PlotSpec) -> bool {
    spec.column_widths.is_empty() || spec.row_heights.is_empty()
}

/// Fills the empty size vectors of `spec` with the sizes it is drawn with
/// (`widths`, `heights`, in points). A vector that already holds sizes is left
/// alone.
pub fn materialize_with(spec: &mut PlotSpec, widths: &[f64], heights: &[f64]) {
    if spec.column_widths.is_empty() {
        spec.column_widths = widths.to_vec();
    }
    if spec.row_heights.is_empty() {
        spec.row_heights = heights.to_vec();
    }
}

/// Sets one size of a draft spec: [`materialize_with`] the drawn sizes, then
/// [`set_cell_size`]. For the size fields of Graph Properties, which edit a
/// draft. `false`, with the draft unchanged, if the size is refused.
pub fn set_size_in_draft(
    spec: &mut PlotSpec,
    drawn: (&[f64], &[f64]),
    axis: CellAxis,
    index: usize,
    size: f64,
) -> bool {
    let mut candidate = spec.clone();
    materialize_with(&mut candidate, drawn.0, drawn.1);
    if !set_cell_size(&mut candidate, axis, index, size) {
        return false;
    }
    *spec = candidate;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_legacy_draft_is_made_explicit_by_the_first_edit() {
        let mut spec = PlotSpec::default();
        assert!(is_legacy(&spec));
        assert!(set_size_in_draft(
            &mut spec,
            (&[480.0], &[323.0]),
            CellAxis::Row,
            0,
            200.0
        ));
        assert_eq!(
            spec.column_widths,
            [480.0],
            "the other axis keeps what it was drawn with"
        );
        assert_eq!(spec.row_heights, [200.0]);
        assert!(!is_legacy(&spec));
    }

    #[test]
    fn a_refused_size_leaves_the_draft_legacy() {
        let mut spec = PlotSpec::default();
        assert!(!set_size_in_draft(
            &mut spec,
            (&[480.0], &[360.0]),
            CellAxis::Column,
            3,
            200.0
        ));
        assert!(is_legacy(&spec));
    }

    #[test]
    fn explicit_sizes_are_never_overwritten_by_the_drawn_ones() {
        let mut spec = PlotSpec::new_graph();
        materialize_with(&mut spec, &[1.0], &[2.0]);
        assert_eq!((spec.column_widths[0], spec.row_heights[0]), (480.0, 360.0));
    }
}

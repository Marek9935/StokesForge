//! Fixing the frames of a graph (#394).
//!
//! A graph from an old file stores no frame size (`frame_widths` and
//! `frame_heights` empty): it is drawn 480 x 360 pt shared equally, the axis
//! bands and the title inside. The stored size is the **plot area**, with the
//! bands added around it, so a legacy spec is given its frames *before it is
//! replaced*: the frames it is drawn with are written into the new spec, which
//! is then "the same picture, computed at opening, then fixed" (G7). The
//! plot areas keep their exact size and only what the edit changes moves. Core
//! does not measure text, so the measured values come from the caller:
//! `stokesforge_render_svg::fix_legacy_frames`, measured as the screen and the
//! exports measure.

use stokesforge_core::compose::{set_frame_size, FrameAxis};
use stokesforge_plot::PlotSpec;

/// True if the graph stores no frame at all (an old file).
pub fn is_legacy(spec: &PlotSpec) -> bool {
    spec.frame_widths.is_empty() && spec.frame_heights.is_empty()
}

/// Fixes the frames of a legacy `target` from `reference`, the spec as it is
/// now (see the module doc). `false` when nothing was to be done.
pub fn fix_from(reference: &PlotSpec, target: &mut PlotSpec) -> bool {
    if !is_legacy(target) {
        return false;
    }
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    stokesforge_render_svg::fix_legacy_frames(reference, target, &measurer, &choice)
        .unwrap_or(false)
}

/// Fixes a spec that was just created legacy: its frames are the ones it is
/// drawn with, so that a new graph is exactly 480 x 360 pt and explicit.
pub fn fix_new(spec: &mut PlotSpec) {
    let reference = spec.clone();
    fix_from(&reference, spec);
}

/// The layout of the figure of `spec`, computed now with the measurement of the
/// exports (Typst titles included): for the surfaces that cannot rely on a
/// painted view, like the size fields of Graph Properties when the graph is not
/// on screen or has just been edited in the draft. Measuring a title builds the
/// Typst engine: the caller memoizes.
pub fn figure_layout(spec: &PlotSpec) -> Option<stokesforge_render::pipeline::FigureLayout> {
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    stokesforge_render_svg::figure_layout(spec, &measurer, &choice).ok()
}

/// Sets one frame size of a draft spec: gives it the `drawn` frames if it is
/// legacy, then [`set_frame_size`]. For the size fields of Graph Properties,
/// which edit a draft. `false`, with the draft unchanged, if the size is
/// refused.
pub fn set_size_in_draft(
    spec: &mut PlotSpec,
    drawn: (&[f64], &[f64]),
    axis: FrameAxis,
    index: usize,
    size: f64,
) -> bool {
    let mut candidate = spec.clone();
    if is_legacy(&candidate) {
        candidate.frame_widths = drawn.0.to_vec();
        candidate.frame_heights = drawn.1.to_vec();
    }
    if !set_frame_size(&mut candidate, axis, index, size) {
        return false;
    }
    *spec = candidate;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::{PanelSpec, Panels};

    #[test]
    fn a_legacy_draft_is_given_the_drawn_frames_by_the_first_edit() {
        let mut spec = PlotSpec::default();
        assert!(is_legacy(&spec));
        assert!(set_size_in_draft(
            &mut spec,
            (&[392.0], &[283.0]),
            FrameAxis::Row,
            0,
            200.0
        ));
        assert_eq!(
            spec.frame_widths,
            [392.0],
            "the other axis keeps what it was drawn with"
        );
        assert_eq!(spec.frame_heights, [200.0]);
        assert!(!is_legacy(&spec));
    }

    #[test]
    fn a_refused_size_leaves_the_draft_legacy() {
        let mut spec = PlotSpec::default();
        assert!(!set_size_in_draft(
            &mut spec,
            (&[392.0], &[284.0]),
            FrameAxis::Column,
            3,
            200.0
        ));
        assert!(is_legacy(&spec));
    }

    #[test]
    fn stored_frames_are_never_overwritten_by_the_drawn_ones() {
        let mut spec = PlotSpec {
            frame_widths: vec![300.0],
            frame_heights: vec![200.0],
            ..PlotSpec::default()
        };
        assert!(set_size_in_draft(
            &mut spec,
            (&[1.0], &[2.0]),
            FrameAxis::Column,
            0,
            250.0
        ));
        assert_eq!(
            (spec.frame_widths[0], spec.frame_heights[0]),
            (250.0, 200.0)
        );
    }

    #[test]
    fn a_legacy_spec_takes_the_frames_of_its_reference_only_on_the_same_grid() {
        let reference = PlotSpec::default();
        let mut same = PlotSpec::default();
        assert!(fix_from(&reference, &mut same));
        assert_eq!(same.frame_widths.len(), 1);
        let mut wider = PlotSpec::default()
            .with_panels(Panels::new([PanelSpec::at(0, 0), PanelSpec::at(0, 1)]));
        assert!(
            !fix_from(&reference, &mut wider),
            "another grid: left alone"
        );
        assert!(is_legacy(&wider));
    }

    #[test]
    fn a_new_graph_is_exactly_480_by_360_and_explicit() {
        let mut spec = PlotSpec::default();
        fix_new(&mut spec);
        assert!(!is_legacy(&spec));
        let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
        let size = stokesforge_render_svg::figure_size(
            &spec,
            &measurer,
            &stokesforge_render_svg::FontChoice::default(),
        );
        assert_eq!(size, (480, 360));
    }
}

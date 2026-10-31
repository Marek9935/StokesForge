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

use stokesforge_core::compose::{
    set_figure_offset, set_fixed_figure, set_frame_size, set_free_frame, set_free_placement,
    set_grid_placement, CellSide, FigureOffset, FrameAxis, FreeField,
};
use stokesforge_plot::{FreeFrame, LayerPlacement, PlotSpec};
use stokesforge_render::pipeline::{BesideSide, FigureLayout};

/// True if the graph stores no frame at all (an old file). A **free** graph
/// stores its frames in its layers (#398): it is never legacy, so nothing here
/// ever puts it back on the grid.
pub fn is_legacy(spec: &PlotSpec) -> bool {
    spec.layer_placement == LayerPlacement::Grid
        && spec.frame_widths.is_empty()
        && spec.frame_heights.is_empty()
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

/// Gives a legacy draft the `drawn` frames (a no-op for a graph that stores
/// them): what Fixed, Left and Top need before they mean anything (#405).
fn materialize(spec: &mut PlotSpec, drawn: (&[f64], &[f64])) {
    if is_legacy(spec) {
        spec.frame_widths = drawn.0.to_vec();
        spec.frame_heights = drawn.1.to_vec();
    }
}

/// Sets "Figure size = Fixed" in a draft: a legacy draft is given the `drawn`
/// frames first, so that the same command fixes them (#405).
pub fn set_fixed_in_draft(spec: &mut PlotSpec, drawn: (&[f64], &[f64]), fixed: bool) -> bool {
    let mut candidate = spec.clone();
    materialize(&mut candidate, drawn);
    if !set_fixed_figure(&mut candidate, fixed) {
        return false;
    }
    *spec = candidate;
    true
}

/// Sets Left or Top (`None`: Auto) in a draft, like [`set_fixed_in_draft`].
pub fn set_offset_in_draft(
    spec: &mut PlotSpec,
    drawn: (&[f64], &[f64]),
    which: FigureOffset,
    value: Option<f64>,
) -> bool {
    let mut candidate = spec.clone();
    materialize(&mut candidate, drawn);
    if !set_figure_offset(&mut candidate, which, value) {
        return false;
    }
    *spec = candidate;
    true
}

/// Switches a draft between the regular grid and free placement (#398).
///
/// To free: a legacy draft is given its frames first, then every layer takes
/// the frame the grid draws it with, so nothing moves on screen. To the grid:
/// [`set_grid_placement`] (L7 then C4). `false`, with the draft unchanged, if
/// the draft is already in that placement or the conversion is refused.
pub fn set_placement_in_draft(spec: &mut PlotSpec, free: bool) -> bool {
    let mut candidate = spec.clone();
    if free {
        if candidate.layer_placement == LayerPlacement::Free {
            return false;
        }
        fix_from(&spec.clone(), &mut candidate);
        let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
        let choice = stokesforge_render_svg::FontChoice::default();
        let Ok(Some(frames)) =
            stokesforge_render_svg::free_frames_from_grid(&candidate, &measurer, &choice)
        else {
            return false;
        };
        if !set_free_placement(&mut candidate, &frames) {
            return false;
        }
    } else if !set_grid_placement(&mut candidate) {
        return false;
    }
    *spec = candidate;
    true
}

/// The four numbers of the frame of a layer as Graph Properties shows them in
/// free placement, in points (#398), with the smallest Left and Top its own
/// axes allow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeFields {
    /// Distance from the left edge of the image to the frame.
    pub left: f64,
    /// Distance from the top edge of the image (title band included) to the
    /// frame.
    pub top: f64,
    /// Width of the frame.
    pub width: f64,
    /// Height of the frame.
    pub height: f64,
    /// Smallest Left: the left bands of the layer itself.
    pub min_left: f64,
    /// Smallest Top: the title band and the top bands of the layer itself.
    pub min_top: f64,
}

/// What the free fields show for layer `panel` of `spec` (#398), read on
/// `layout`, the layout of this very spec. `None` if the graph is not free or
/// the layer has no cell in the layout.
pub fn free_fields(spec: &PlotSpec, layout: &FigureLayout, panel: usize) -> Option<FreeFields> {
    if spec.layer_placement != LayerPlacement::Free {
        return None;
    }
    let base = *spec.cell_bases().get(panel)?;
    let cell = layout.free_cells.iter().find(|cell| cell.base == base)?;
    let frame = spec.panels.get(base)?.sanitized_free_frame();
    let band = f64::from(layout.title_band);
    Some(FreeFields {
        left: frame.left - layout.free_origin.0,
        top: frame.top - layout.free_origin.1 + band,
        width: frame.width,
        height: frame.height,
        min_left: f64::from(cell.frame.0.saturating_sub(cell.outer.0)),
        min_top: band + f64::from(cell.frame.1.saturating_sub(cell.outer.1)),
    })
}

/// Sets one field of layer `panel` of a free draft from the value Graph
/// Properties shows (#398): Left and Top are distances from the edge of the
/// image, raised to the layer's own bands, stored as `origin + value`; Width
/// and Height are the frame itself. `layout` is the layout of `spec` as it is.
/// `false`, with the draft unchanged, if the field is refused.
pub fn set_free_field_in_draft(
    spec: &mut PlotSpec,
    layout: &FigureLayout,
    panel: usize,
    field: FreeField,
    shown: f64,
) -> bool {
    let Some(fields) = free_fields(spec, layout, panel) else {
        return false;
    };
    let stored = match field {
        FreeField::Left => shown.max(fields.min_left) + layout.free_origin.0,
        FreeField::Top => {
            shown.max(fields.min_top) + layout.free_origin.1 - f64::from(layout.title_band)
        }
        FreeField::Width | FreeField::Height => shown,
    };
    let mut candidate = spec.clone();
    if !set_free_frame(&mut candidate, panel, field, stored) {
        return false;
    }
    *spec = candidate;
    true
}

/// The frame of a layer added beside layer `panel` of a free `spec` (#398,
/// L8), measured like the exports. `None` if the graph is not free.
pub fn place_beside(spec: &PlotSpec, panel: usize, side: CellSide) -> Option<FreeFrame> {
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    let side = match side {
        CellSide::Right => BesideSide::Right,
        CellSide::Below => BesideSide::Below,
    };
    stokesforge_render_svg::place_beside(spec, &measurer, &choice, panel, side)
        .ok()
        .flatten()
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
    fn fixed_left_and_top_give_a_legacy_draft_its_drawn_frames() {
        let mut spec = PlotSpec::default();
        assert!(set_fixed_in_draft(&mut spec, (&[392.0], &[284.0]), true));
        assert!(spec.fixed_figure && !is_legacy(&spec));
        let mut spec = PlotSpec::default();
        assert!(set_offset_in_draft(
            &mut spec,
            (&[392.0], &[284.0]),
            FigureOffset::Left,
            Some(120.0)
        ));
        assert_eq!(spec.figure_left, Some(120.0));
        assert_eq!(spec.frame_widths, [392.0]);
        let mut refused = PlotSpec::default();
        assert!(!set_offset_in_draft(
            &mut refused,
            (&[392.0], &[284.0]),
            FigureOffset::Top,
            Some(f64::NAN)
        ));
        assert!(is_legacy(&refused), "a refusal leaves the draft untouched");
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

    fn two_columns() -> PlotSpec {
        let mut spec = PlotSpec::default()
            .with_panels(Panels::new([PanelSpec::at(0, 0), PanelSpec::at(0, 1)]));
        spec.frame_widths = vec![300.0, 200.0];
        spec.frame_heights = vec![250.0];
        spec
    }

    fn free_two_columns() -> PlotSpec {
        let mut spec = two_columns();
        assert!(set_placement_in_draft(&mut spec, true));
        spec
    }

    /// #398: grid to free keeps every plot area and the size of the image, for
    /// a stored grid and for a legacy one (given its frames first).
    #[test]
    fn going_free_moves_nothing_on_screen() {
        for mut spec in [two_columns(), {
            let mut legacy = two_columns();
            legacy.frame_widths.clear();
            legacy.frame_heights.clear();
            legacy
        }] {
            let grid = figure_layout(&spec).expect("grid layout");
            assert!(set_placement_in_draft(&mut spec, true));
            assert_eq!(spec.layer_placement, LayerPlacement::Free);
            assert!(!is_legacy(&spec), "a free graph is never legacy");
            let free = figure_layout(&spec).expect("free layout");
            assert_eq!(free.size, grid.size, "the image does not change");
            for panel in spec.panels.iter() {
                let cell = free
                    .free_cells
                    .iter()
                    .find(|cell| (cell.row, cell.column) == (panel.row, panel.column))
                    .expect("a cell");
                if grid.is_explicit() {
                    assert_eq!(
                        Some(cell.frame),
                        grid.frame_rect(panel.row, panel.column),
                        "plot area of ({}, {})",
                        panel.row,
                        panel.column
                    );
                } else {
                    // A legacy grid stores nothing to place: its sizes carry over.
                    assert_eq!(
                        f64::from(cell.frame.2),
                        grid.frame_widths[panel.column].round()
                    );
                    assert_eq!(
                        f64::from(cell.frame.3),
                        grid.frame_heights[panel.row].round()
                    );
                }
            }
            assert!(!set_placement_in_draft(&mut spec, true), "already free");
        }
    }

    /// #398: free to grid applies the rules of the model (L7 then C4): two
    /// layers side by side share a row, each column takes its frame.
    #[test]
    fn going_back_to_the_grid_groups_by_position_and_keeps_the_largest_frames() {
        let mut spec = free_two_columns();
        // Layer 0 is moved right of layer 1 and made taller.
        let first = spec.panels[0].free_frame.expect("frame");
        let second = spec.panels[1].free_frame.expect("frame");
        spec.panels.get_mut(0).unwrap().free_frame = Some(FreeFrame {
            left: second.left + 400.0,
            height: 300.0,
            ..first
        });
        assert!(set_placement_in_draft(&mut spec, false));
        assert_eq!(spec.layer_placement, LayerPlacement::Grid);
        assert_eq!((spec.panels[0].row, spec.panels[0].column), (0, 1));
        assert_eq!((spec.panels[1].row, spec.panels[1].column), (0, 0));
        assert_eq!(spec.frame_widths, [200.0, 300.0]);
        assert_eq!(spec.frame_heights, [300.0]);
        assert!(!set_placement_in_draft(&mut spec, false), "already a grid");
    }

    /// #398: Left and Top show the distance from the image edge (title band
    /// included at the top), not the stored coordinate; typing one stores
    /// `origin + value`; a value below the layer's own bands is raised.
    #[test]
    fn free_fields_are_distances_from_the_image_edge() {
        let mut spec = free_two_columns();
        spec.title = "T".to_owned();
        // Shift the whole picture far from the stored origin.
        for panel in spec.panels.iter_mut() {
            if let Some(frame) = panel.free_frame.as_mut() {
                frame.left += 1000.0;
                frame.top += 700.0;
            }
        }
        let layout = figure_layout(&spec).expect("layout");
        let cell = &layout.free_cells[0];
        let fields = free_fields(&spec, &layout, 0).expect("fields");
        assert_ne!(
            fields.left,
            spec.panels[0].free_frame.unwrap().left,
            "not raw"
        );
        assert!(
            (fields.left - f64::from(cell.frame.0)).abs() < 1.0,
            "{fields:?}"
        );
        assert!(
            (fields.top - f64::from(cell.frame.1)).abs() < 1.0,
            "{fields:?}"
        );
        assert!(layout.title_band > 0 && fields.top > f64::from(layout.title_band));

        // Layer 1 moved to 50 pt more than layer 0's Left: stored = origin + 50.
        let target = fields.left + 50.0;
        assert!(set_free_field_in_draft(
            &mut spec,
            &layout,
            1,
            FreeField::Left,
            target
        ));
        let moved = spec.panels[1].free_frame.unwrap().left;
        assert!(
            (moved - (layout.free_origin.0 + target)).abs() < 1e-9,
            "{moved}"
        );
        // Below its own left bands: raised to them.
        let layout = figure_layout(&spec).expect("layout");
        let low = free_fields(&spec, &layout, 1).expect("fields");
        assert!(set_free_field_in_draft(
            &mut spec,
            &layout,
            1,
            FreeField::Left,
            0.0
        ));
        let layout = figure_layout(&spec).expect("layout");
        let raised = free_fields(&spec, &layout, 1).expect("fields");
        assert!(
            (raised.left - low.min_left).abs() < 1.0,
            "{raised:?} {low:?}"
        );
    }

    /// #398: the fields belong to the layer asked for, an overlaid layer reads
    /// and writes its base's cell, and a grid draft has none.
    #[test]
    fn free_fields_follow_the_layer_asked_for() {
        let mut spec = free_two_columns();
        let layout = figure_layout(&spec).expect("layout");
        let a = free_fields(&spec, &layout, 0).expect("fields");
        let b = free_fields(&spec, &layout, 1).expect("fields");
        assert_ne!(a.left, b.left);
        assert_eq!((a.width, b.width), (300.0, 200.0));
        assert!(set_free_field_in_draft(
            &mut spec,
            &layout,
            1,
            FreeField::Width,
            150.0
        ));
        assert_eq!(spec.panels[0].free_frame.unwrap().width, 300.0);
        assert_eq!(spec.panels[1].free_frame.unwrap().width, 150.0);
        assert!(!set_free_field_in_draft(
            &mut spec,
            &layout,
            1,
            FreeField::Width,
            f64::NAN
        ));
        assert!(free_fields(&two_columns(), &layout, 0).is_none());
    }
}

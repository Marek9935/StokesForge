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
    set_figure_offset, set_fixed_figure, set_frame_size, set_free_frame, CellSide, FigureOffset,
    FrameAxis, FreeField,
};
use stokesforge_plot::{FreeFrame, LayerPlacement, PlotSpec};
use stokesforge_render::pipeline::{BesideSide, SizeAxis};

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

/// What fitting a total image size changed (#407).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TotalFit {
    /// The smallest and the largest size of the typed axis, in points.
    pub range: (f64, f64),
    /// The typed value was outside `range` and was brought to its nearest end.
    pub raised: bool,
    /// With a closed ratio lock, the same for the other axis (the ratio can ask
    /// for more than it allows); `None` without a lock.
    pub other: Option<((f64, f64), bool)>,
}

/// The lock beside the total width and height of Graph Properties (#407): while
/// it is closed, the height / width ratio of the image taken when it was
/// closed is kept by every total typed. Window state like `selected_series`:
/// never stored in the `.stf`, unlocked when the window opens.
///
/// The ratio must never go stale: any **other** change of the image made while
/// the lock is closed (a frame field, a drag of a frame on the page, Undo,
/// Redo, Left, Top, a title or an axis toggled) takes it again from the image
/// as it is, which [`SizeLock::follow`] does once per frame from the one place
/// that knows the image (the Size section). What a total typed does itself,
/// raise included, is [`SizeLock::adopt_next`]: the ratio stays the captured one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeLock {
    /// Height / width of the image when the lock was closed (or last
    /// re-captured).
    pub ratio: f64,
    /// The image size last seen, `None` right after a total was typed (the
    /// next one seen is the effect of that typing).
    seen: Option<(f64, f64)>,
}

impl SizeLock {
    /// Closes the lock on an image of `image` (width, height) points; `None`
    /// if that image has no usable ratio.
    pub fn close(image: (f64, f64)) -> Option<Self> {
        let ratio = image.1 / image.0;
        (ratio.is_finite() && ratio > 0.0).then_some(Self {
            ratio,
            seen: Some(image),
        })
    }

    /// Called with the image as it is now: a change nobody announced with
    /// [`SizeLock::adopt_next`] takes the ratio again from it.
    pub fn follow(&mut self, image: (f64, f64)) {
        let moved = self
            .seen
            .is_some_and(|seen| (seen.0 - image.0).abs() > 0.5 || (seen.1 - image.1).abs() > 0.5);
        if moved {
            if let Some(lock) = Self::close(image) {
                *self = lock;
                return;
            }
        }
        self.seen = Some(image);
    }

    /// A total was just typed: the next image seen is its effect, the ratio
    /// stays.
    pub fn adopt_next(&mut self) {
        self.seen = None;
    }
}

/// Sets the total size of the image along `axis` in a draft, the frames
/// following in proportion (#407): a legacy draft is given its frames first
/// ([`fix_from`]), then [`stokesforge_render::pipeline::fit_figure_size`], all
/// in the same draft so that one `SetPlotSpec` carries it.
///
/// With `lock_ratio = Some(r)`, `r` being height / width of the image when the
/// lock was closed, the other axis then follows: height = **applied** width
/// x `r` (after any raise of the typed value), or width = applied height / `r`.
/// `None`, with the draft unchanged, if the target or the ratio is not usable
/// or the graph has no frame to scale.
pub fn set_total_in_draft(
    spec: &mut PlotSpec,
    axis: SizeAxis,
    target_pt: f64,
    lock_ratio: Option<f64>,
) -> Option<TotalFit> {
    if lock_ratio.is_some_and(|ratio| !(ratio.is_finite() && ratio > 0.0)) {
        return None;
    }
    let mut candidate = spec.clone();
    fix_from(spec, &mut candidate);
    let first = fit_and_apply(&mut candidate, axis, target_pt)?;
    let other = match lock_ratio {
        None => None,
        Some(ratio) => {
            let size = figure_layout(&candidate)?.size;
            let (other_axis, other_target) = match axis {
                SizeAxis::Width => (SizeAxis::Height, f64::from(size.0) * ratio),
                SizeAxis::Height => (SizeAxis::Width, f64::from(size.1) / ratio),
            };
            let second = fit_and_apply(&mut candidate, other_axis, other_target)?;
            Some((second.range, second.raised))
        }
    };
    *spec = candidate;
    Some(TotalFit {
        range: first.range,
        raised: first.raised,
        other,
    })
}

/// Fits `spec` along `axis` and writes the frames; what the fit said.
fn fit_and_apply(
    spec: &mut PlotSpec,
    axis: SizeAxis,
    target_pt: f64,
) -> Option<stokesforge_render::pipeline::FittedFigure> {
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    let fitted = stokesforge_render_svg::fit_figure_size(spec, &measurer, &choice, axis, target_pt)
        .ok()??;
    fitted.apply_to(spec);
    Some(fitted)
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
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    stokesforge_render_svg::set_layer_placement(spec, free, &measurer, &choice)
}

/// The four numbers of the frame of a layer as Graph Properties shows them in
/// free placement, in points (#398).
///
/// Left and Top are **page coordinates**, exactly what is stored: the origin
/// is the top-left corner of the image at the moment the graph went free
/// (`free_frames_from_grid`), so a layer that does not move keeps its numbers
/// whatever the others do, and a value is read back as typed. They may be
/// negative. Top does **not** include the graph title band (the stored value
/// does not), so showing or hiding the title changes neither the stored nor the
/// shown numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeFields {
    /// Page x of the left edge of the plot area.
    pub left: f64,
    /// Page y of the top edge of the plot area, title band excluded.
    pub top: f64,
    /// Width of the plot area.
    pub width: f64,
    /// Height of the plot area.
    pub height: f64,
}

/// What the free fields show for layer `panel` of `spec` (#398). `None` if the
/// graph is not free or the layer does not exist.
pub fn free_fields(spec: &PlotSpec, panel: usize) -> Option<FreeFields> {
    if spec.layer_placement != LayerPlacement::Free {
        return None;
    }
    let base = *spec.cell_bases().get(panel)?;
    let frame = spec.panels.get(base)?.sanitized_free_frame();
    Some(FreeFields {
        left: frame.left,
        top: frame.top,
        width: frame.width,
        height: frame.height,
    })
}

/// Sets one field of layer `panel` of a free draft (#398), stored as given
/// (brought into bounds like [`FreeFrame::sanitized`]). `false`, with the
/// draft unchanged, if the field is refused.
pub fn set_free_field_in_draft(
    spec: &mut PlotSpec,
    panel: usize,
    field: FreeField,
    value: f64,
) -> bool {
    let mut candidate = spec.clone();
    if !set_free_frame(&mut candidate, panel, field, value) {
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

/// The new frames of the cells `targets` of a free `spec` spread along `axis`
/// with equal gaps between their outer rectangles (#398, L4), measured like
/// the exports. `None` if the graph is not free or there are fewer than three
/// cells.
pub fn distribute_frames(
    spec: &PlotSpec,
    targets: &[usize],
    axis: stokesforge_render::pipeline::DistributeAxis,
) -> Option<stokesforge_render::pipeline::CellFrames> {
    let measurer = stokesforge_render_svg::SvgTextMeasurer::default();
    let choice = stokesforge_render_svg::FontChoice::default();
    stokesforge_render_svg::distribute_frames(spec, &measurer, &choice, targets, axis)
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

    /// #398: Left and Top are stable page coordinates: what is typed is read
    /// back, negative values are accepted, and a layer that does not move keeps
    /// its numbers when another one moves.
    #[test]
    fn free_positions_are_page_coordinates_read_back_as_typed() {
        let mut spec = free_two_columns();
        let first = free_fields(&spec, 0).expect("fields");
        let second = free_fields(&spec, 1).expect("fields");
        // Layer 1 moved to 25 cm (708.66 pt), right of layer 2.
        let typed = 25.0 * 72.0 / 2.54;
        assert!(set_free_field_in_draft(
            &mut spec,
            0,
            FreeField::Left,
            typed
        ));
        assert_eq!(free_fields(&spec, 0).unwrap().left, typed);
        assert_eq!(
            free_fields(&spec, 1).unwrap(),
            second,
            "layer 2 keeps its numbers"
        );
        // A negative position, left of the origin, round-trips.
        assert!(set_free_field_in_draft(
            &mut spec,
            1,
            FreeField::Left,
            -120.0
        ));
        assert!(set_free_field_in_draft(&mut spec, 1, FreeField::Top, -30.0));
        let moved = free_fields(&spec, 1).unwrap();
        assert_eq!((moved.left, moved.top), (-120.0, -30.0));
        assert_eq!(
            free_fields(&spec, 0).unwrap().left,
            typed,
            "and layer 1 too"
        );
        assert_eq!(free_fields(&spec, 0).unwrap().top, first.top);
        // Hiding or showing the title changes no number.
        let before = free_fields(&spec, 0).unwrap();
        spec.title = "T".to_owned();
        assert_eq!(free_fields(&spec, 0).unwrap(), before);
    }

    /// #398: right after Grid to Free the numbers are the former distances
    /// from the image edge (no title: the plot area's place in the image).
    #[test]
    fn right_after_going_free_the_positions_are_the_former_distances() {
        let grid = figure_layout(&two_columns()).expect("layout");
        let spec = free_two_columns();
        for (index, (row, column)) in [(0, 0), (0, 1)].into_iter().enumerate() {
            let (x, y, _, _) = grid.frame_rect(row, column).expect("explicit");
            let fields = free_fields(&spec, index).expect("fields");
            assert_eq!((fields.left, fields.top), (f64::from(x), f64::from(y)));
        }
    }

    /// #398: the fields belong to the layer asked for, an overlaid layer reads
    /// and writes its base's cell, and a grid draft has none.
    #[test]
    fn free_fields_follow_the_layer_asked_for() {
        let mut spec = free_two_columns();
        let a = free_fields(&spec, 0).expect("fields");
        let b = free_fields(&spec, 1).expect("fields");
        assert_ne!(a.left, b.left);
        assert_eq!((a.width, b.width), (300.0, 200.0));
        assert!(set_free_field_in_draft(
            &mut spec,
            1,
            FreeField::Width,
            150.0
        ));
        assert_eq!(spec.panels[0].free_frame.unwrap().width, 300.0);
        assert_eq!(spec.panels[1].free_frame.unwrap().width, 150.0);
        assert!(!set_free_field_in_draft(
            &mut spec,
            1,
            FreeField::Width,
            f64::NAN
        ));
        assert!(free_fields(&two_columns(), 0).is_none());
    }

    fn size_of(spec: &PlotSpec) -> (u32, u32) {
        figure_layout(spec).expect("layout").size
    }

    /// #407: a legacy draft is given its frames and scaled in the same draft.
    /// Witness: `fix_from` forgotten (the fit finds nothing to scale).
    #[test]
    fn a_legacy_draft_is_materialised_and_scaled_in_one_draft() {
        let mut spec = PlotSpec::default();
        assert!(is_legacy(&spec));
        let fit = set_total_in_draft(&mut spec, SizeAxis::Width, 600.0, None).expect("a fit");
        assert!(!fit.raised && fit.other.is_none());
        assert!(!is_legacy(&spec));
        assert_eq!(size_of(&spec).0, 600);
        assert_eq!(size_of(&spec).1, 360, "the height is what it was drawn");
    }

    /// #407: without a lock the other axis is untouched; with `Some(r)` it
    /// follows from the **applied** value, after a raise too. Witness: the
    /// ratio ignored (heights identical), or applied to the typed value.
    #[test]
    fn a_closed_lock_makes_the_other_axis_follow_the_applied_size() {
        let mut unlocked = two_columns();
        let before = unlocked.frame_heights.clone();
        set_total_in_draft(&mut unlocked, SizeAxis::Width, 800.0, None).expect("a fit");
        assert_eq!(unlocked.frame_heights, before, "no lock: heights identical");
        let mut spec = two_columns();
        let (w0, h0) = size_of(&spec);
        let ratio = f64::from(h0) / f64::from(w0);
        let fit =
            set_total_in_draft(&mut spec, SizeAxis::Width, 800.0, Some(ratio)).expect("a fit");
        assert!(!fit.raised);
        let (w, h) = size_of(&spec);
        assert_eq!(w, 800);
        assert!(
            (f64::from(h) - 800.0 * ratio).abs() <= 1.0,
            "height {h} follows 800 x {ratio}"
        );
        // A typed value below the range is raised; the height follows the
        // raised width, not the typed one.
        let mut small = two_columns();
        let fit = set_total_in_draft(&mut small, SizeAxis::Width, 10.0, Some(1.0)).expect("a fit");
        assert!(fit.raised && fit.other.is_some_and(|other| !other.1));
        let (w, h) = size_of(&small);
        assert!(w > 10, "raised to the minimum, not the typed 10");
        assert!(
            w.abs_diff(h) <= 1,
            "square: {w} x {h} follows the applied width"
        );
        // Typing the height: the width follows (divided by the ratio).
        let mut tall = two_columns();
        set_total_in_draft(&mut tall, SizeAxis::Height, 500.0, Some(ratio)).expect("a fit");
        let (w, h) = size_of(&tall);
        assert_eq!(h, 500);
        assert!((f64::from(w) - 500.0 / ratio).abs() <= 1.0);
    }

    /// #407: a refused target or ratio leaves the draft as it was.
    #[test]
    fn an_unusable_total_or_ratio_leaves_the_draft_untouched() {
        for (target, ratio) in [
            (f64::NAN, None),
            (500.0, Some(0.0)),
            (500.0, Some(f64::NAN)),
        ] {
            let mut spec = PlotSpec::default();
            assert!(set_total_in_draft(&mut spec, SizeAxis::Width, target, ratio).is_none());
            assert_eq!(spec, PlotSpec::default());
        }
    }

    /// #407: the ratio is re-captured when the image changes by any other way,
    /// kept after a total typed. Witness: no re-capture (stale ratio), or a
    /// re-capture after a total typed (drift after a raise).
    #[test]
    fn the_lock_recaptures_other_changes_and_keeps_its_ratio_after_a_total() {
        let mut lock = SizeLock::close((400.0, 200.0)).expect("a ratio");
        assert_eq!(lock.ratio, 0.5);
        lock.follow((400.0, 200.0));
        assert_eq!(lock.ratio, 0.5, "same image: unchanged");
        lock.follow((400.0, 300.0));
        assert_eq!(lock.ratio, 0.75, "a frame field changed the image");
        lock.adopt_next();
        lock.follow((500.0, 90.0));
        assert_eq!(
            lock.ratio, 0.75,
            "the effect of a total typed, raise included"
        );
        lock.follow((500.0, 90.0));
        lock.follow((250.0, 90.0));
        assert!((lock.ratio - 0.36).abs() < 1e-9, "next change re-captures");
        assert!(SizeLock::close((0.0, 10.0)).is_none());
    }
}

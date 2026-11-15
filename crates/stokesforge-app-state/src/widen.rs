//! Widening the written axis ranges when a series is added (#529, L5).
//!
//! A graph's Auto ranges are written once (`frame_sizes::freeze_ranges`), so a
//! series dragged onto it that goes beyond them would be cut by the frame.
//! After the add, [`widen_to_data`] moves **only the exceeded side** of each
//! axis to the automatic bound computed with the user's margins on all the
//! layer's data; the other side keeps its written value. The result is a spec
//! that the caller commits as a second history entry
//! ([`stokesforge_commands::PlotEditKind::WidenRanges`]): the first Ctrl+Z
//! restores the old ranges and keeps the series.
//!
//! A side moves only when the **new** series is what takes the data beyond it:
//! a graph the user zoomed in by hand keeps its zoom when a series that stays
//! inside the data it had is added. `before` is the graph as it was before the add.
//!
//! Nothing is measured here: the automatic bound is the one the renderer draws
//! (`freeze_ranges` on a copy whose axes are put back on Auto, once with no
//! margin to read the extent of the data, once with the margins to read the new
//! bound), so a log axis goes to its decade, an inverted axis keeps its
//! direction and a bar keeps its baseline without a line of arithmetic of its own.

use stokesforge_core::compose::overlay_base;
use stokesforge_core::Tree;
use stokesforge_plot::{AxisSpec, PanelSpec, PlotSpec};
use stokesforge_render_svg::AutoMargins;

use crate::frame_sizes::freeze_ranges;

/// An axis of a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Bottom,
    Left,
    Right,
}

/// The axis that was widened, for the sentence that says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidenedAxis {
    /// The bottom X axis.
    X,
    /// The left Y axis.
    Y,
    /// The X axis shown on top by an overlaid layer.
    TopX,
    /// The right Y axis (the Y axis of a layer, or of a series on the right).
    RightY,
}

/// The spec with its ranges widened, and which axes moved.
#[derive(Debug)]
pub struct Widened {
    pub spec: PlotSpec,
    pub axes: Vec<WidenedAxis>,
}

/// An axis to check: the one measured, and the one that takes the new bound
/// (they differ for the linked axis of an overlaid layer, which reads the
/// covered layer's).
#[derive(Clone, Copy, Debug)]
struct Target {
    at: (usize, Side),
    dest: (usize, Side),
}

fn axis(spec: &PlotSpec, (panel, side): (usize, Side)) -> Option<&AxisSpec> {
    let panel: &PanelSpec = spec.panels.get(panel)?;
    Some(match side {
        Side::Bottom => &panel.bottom,
        Side::Left => &panel.left,
        Side::Right => &panel.right,
    })
}

fn axis_mut(spec: &mut PlotSpec, (panel, side): (usize, Side)) -> Option<&mut AxisSpec> {
    let panel = spec.panels.get_mut(panel)?;
    Some(match side {
        Side::Bottom => &mut panel.bottom,
        Side::Left => &mut panel.left,
        Side::Right => &mut panel.right,
    })
}

/// The written range of an axis, low then high (the stored order follows the
/// direction of the axis).
fn written(axis: &AxisSpec) -> Option<(f64, f64)> {
    if axis.auto_range {
        return None;
    }
    let (a, b) = (axis.range_min?, axis.range_max?);
    (a.is_finite() && b.is_finite()).then(|| (a.min(b), a.max(b)))
}

/// A copy of `spec` with the measured axes of `targets` back on Auto, framed
/// with `margins`. A linked axis is measured as an own one (its own data), so
/// that the data of an overlaid layer sharing the covered layer's axis is seen.
fn probe(
    tree: &Tree,
    spec: &PlotSpec,
    targets: &[Target],
    margins: AutoMargins,
) -> Option<PlotSpec> {
    let mut copy = spec.clone();
    for target in targets {
        let (index, side) = target.at;
        if let Some(overlay) = copy.panels.get_mut(index).and_then(|p| p.overlay.as_mut()) {
            match side {
                Side::Bottom => overlay.x_top = true,
                Side::Left => overlay.y_right = true,
                Side::Right => {}
            }
        }
        axis_mut(&mut copy, target.at)?.auto_range = true;
    }
    freeze_ranges(tree, &mut copy, margins).then_some(copy)
}

/// Widens the exceeded sides of `targets` in `out`; true if anything moved.
fn widen_round(
    tree: &Tree,
    out: &mut PlotSpec,
    targets: &[Target],
    margins: AutoMargins,
    before: &PlotSpec,
    moved: &mut Vec<(usize, Side)>,
) -> bool {
    if targets.is_empty() {
        return false;
    }
    let none = AutoMargins {
        left: 0.0,
        right: 0.0,
        bottom: 0.0,
        top: 0.0,
    };
    let (Some(data), Some(framed)) = (
        probe(tree, out, targets, none),
        probe(tree, out, targets, margins),
    ) else {
        return false;
    };
    // The extent of the data before the add, where the layer already existed.
    let known: Vec<Target> = targets
        .iter()
        .copied()
        .filter(|t| t.at.0 < before.panels.len() && t.dest.0 < before.panels.len())
        .collect();
    let previous = probe(tree, before, &known, none);
    let mut any = false;
    for target in targets {
        let (Some(extent), Some(bound)) = (
            axis(&data, target.at).and_then(written),
            axis(&framed, target.at).and_then(written),
        ) else {
            continue;
        };
        let Some(current) = axis(out, target.dest).and_then(written) else {
            continue;
        };
        let tolerance = 1e-9 * (current.1 - current.0).abs();
        // Did the add take the data beyond what it was? No known extent
        // (a new layer, or no data before): yes.
        let old = previous
            .as_ref()
            .filter(|_| known.iter().any(|k| k.at == target.at))
            .and_then(|p| axis(p, target.at))
            .and_then(written);
        let grew_high = old.is_none_or(|o| extent.1 > o.1 + 1e-9 * (o.1 - o.0).abs());
        let grew_low = old.is_none_or(|o| extent.0 < o.0 - 1e-9 * (o.1 - o.0).abs());
        let (mut low, mut high) = current;
        if grew_high && extent.1 > current.1 + tolerance {
            high = bound.1.max(current.1);
        }
        if grew_low && extent.0 < current.0 - tolerance {
            low = bound.0.min(current.0);
        }
        if (low, high) == current {
            continue;
        }
        if let Some(target_axis) = axis_mut(out, target.dest) {
            target_axis.range_min = Some(low);
            target_axis.range_max = Some(high);
            // The stored order follows the direction, like the From and To fields.
            *target_axis = target_axis.orient();
            moved.push(target.dest);
            any = true;
        }
    }
    any
}

/// `spec` (the graph as it is after the add, ranges written; `before`, as it
/// was before) with the sides
/// that its data now exceeds moved to the automatic bound; `None` when nothing
/// is exceeded. Only written axes move: one still on Auto follows its data.
pub fn widen_to_data(
    tree: &Tree,
    before: &PlotSpec,
    spec: &PlotSpec,
    margins: AutoMargins,
) -> Option<Widened> {
    let mut base_targets = Vec::new();
    let mut own_targets = Vec::new();
    for (index, panel) in spec.panels.iter().enumerate() {
        let base = overlay_base(spec, index);
        match panel.overlay {
            Some(overlay) if base != index => {
                let (x, y) = ((index, Side::Bottom), (index, Side::Left));
                if overlay.x_top {
                    own_targets.push(Target { at: x, dest: x });
                } else {
                    base_targets.push(Target {
                        at: x,
                        dest: (base, Side::Bottom),
                    });
                }
                if overlay.y_right {
                    own_targets.push(Target { at: y, dest: y });
                } else {
                    base_targets.push(Target {
                        at: y,
                        dest: (base, Side::Left),
                    });
                }
            }
            _ => {
                for side in [Side::Bottom, Side::Left, Side::Right] {
                    base_targets.push(Target {
                        at: (index, side),
                        dest: (index, side),
                    });
                }
            }
        }
    }
    let mut out = spec.clone();
    let mut moved = Vec::new();
    let mut base_moved = Vec::new();
    widen_round(
        tree,
        &mut out,
        &base_targets,
        margins,
        before,
        &mut base_moved,
    );
    // An overlaid layer's own axis (its top X, its right Y) widens on its own
    // data only (#529, form 529-5: it is never aligned on the covered layer).
    let mut own_moved = Vec::new();
    widen_round(
        tree,
        &mut out,
        &own_targets,
        margins,
        before,
        &mut own_moved,
    );
    moved.extend(base_moved);
    moved.extend(own_moved);
    if moved.is_empty() {
        return None;
    }
    let mut axes = Vec::new();
    for (index, side) in moved {
        let overlaid = out.panels.get(index).is_some_and(|p| p.overlay.is_some());
        let named = match (side, overlaid) {
            (Side::Bottom, false) => WidenedAxis::X,
            (Side::Bottom, true) => WidenedAxis::TopX,
            (Side::Left, false) => WidenedAxis::Y,
            (Side::Left, true) | (Side::Right, _) => WidenedAxis::RightY,
        };
        if !axes.contains(&named) {
            axes.push(named);
        }
    }
    Some(Widened { spec: out, axes })
}

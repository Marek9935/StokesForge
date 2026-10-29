//! The view state — zoom and pan, task R7.1.
//!
//! # What this module is not
//!
//! It is **not** a model mutation. `CLAUDE.md` is categorical: "every model
//! mutation goes through an undoable command of `stokesforge-commands`" — but
//! a wheel zoom is not a model mutation, it is an ephemeral view framing, and
//! sending it through an undoable command would make it a history entry just
//! like a column rename. Nobody wants to undo a zoom with `Ctrl+Z`.
//! [`ViewState`] therefore never touches the `PlotSpec` stored in the
//! [`stokesforge_core::Tree`]: it lives next to it, in `stokesforge-render`
//! (#262: nothing here depends on egui), and `GraphView`, from
//! `stokesforge-render-egui`, carries it as a field.
//!
//! # The mechanism: an explicit framing, applied to a clone
//!
//! `stokesforge-plot` already has everything needed for an explicit framing
//! to take precedence over the computed data — it is
//! [`stokesforge_plot::AxisSpec::has_explicit_range`], exercised by
//! `crates/stokesforge-render/tests/geometry.rs::an_explicit_framing_takes_precedence_over_data`.
//! [`ViewState::apply`] sets `auto_range = false` with the two bounds on a
//! **clone** of the `PlotSpec`, just before `resolve_spec` — never on the
//! original. Nothing new was therefore invented for the framing itself:
//! only the trigger (wheel, drag, rubber-band rectangle — R7.1/7.4) is new,
//! in `GraphView`; the computation of the framing a gesture gives
//! ([`ViewState::commit_gesture`], [`ViewState::commit_axis_gesture`]) is
//! here, without egui, hence testable without a window.
//!
//! # Identity of a panel: its index in the spec
//!
//! [`stokesforge_render::DrawnPlot::panels`] omits the panels too small to be
//! drawn ([`stokesforge_render::pipeline::MIN_CELL`]): its index is thus not
//! stable from one frame to the next if the window changes size. The framing
//! is therefore stored by the layer's index in
//! [`stokesforge_plot::PlotSpec::panels`] (`DrawnPanel::index`), stable and
//! distinct even when several layers overlap in the same cell.
//!
//! # The cache key: a counter, not the bounds
//!
//! Zoom/pan changes the drawn geometry — unlike *feathering*, which only
//! changes the tessellation (`stokesforge_render_egui::feathering`, and
//! `architecture-choisie.md` §4.2). It **must** therefore enter the cache key
//! (`stokesforge_render_egui::CacheKey::view_generation`).
//! [`ViewState::generation`] is what enters it: an integer that goes up with
//! each committed framing, rather than the bounds themselves (variable in
//! number, per panel, as `f64` — nothing very `Copy`).

use std::collections::HashMap;

use stokesforge_plot::PlotSpec;

use crate::hit::AxisSide;
use crate::interact::{Gesture, PanelTransform};
use crate::pipeline::DrawnPanel;

/// The explicit framing of a layer's **own scales**: the bounds that
/// zoom/pan computed, in data space (not transformed — like
/// [`stokesforge_plot::AxisSpec::range_min`]/`range_max`, whose value it
/// directly is). A missing scale is not framed: a scale linked to another
/// layer (#167) has no framing of its own, it follows that of the layer it
/// covers.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Framing {
    /// Bounds of the abscissa axis. Order does not matter: the resolution
    /// sorts them anyway (`pipeline::axis_range`).
    pub x: Option<(f64, f64)>,
    /// Bounds of the main ordinate.
    pub y: Option<(f64, f64)>,
    /// Bounds of the right-hand ordinate.
    pub y2: Option<(f64, f64)>,
}

impl Framing {
    /// Framing of the main axes only.
    pub fn xy(x: (f64, f64), y: (f64, f64)) -> Self {
        Self {
            x: Some(x),
            y: Some(y),
            y2: None,
        }
    }
}

/// The view state of a `GraphView` (`stokesforge-render-egui`): the explicit
/// framings in progress, per panel.
///
/// Empty by default — a new graph is shown framed on its data, as before R7.
/// Limited to the main axis (bottom and left): point picking (task 7.2) is
/// too, for the same reason — see its documentation in
/// `stokesforge_render_egui::PointHit`.
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    framings: HashMap<usize, Framing>,
    generation: u64,
}

impl ViewState {
    /// A new view state, with no explicit framing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The counter to put in `stokesforge_render_egui::CacheKey::view_generation`.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// True if no panel has an explicit framing — the common case, where
    /// [`Self::apply`] has nothing to do.
    pub fn is_empty(&self) -> bool {
        self.framings.is_empty()
    }

    /// The explicit framing of layer `panel`, if there is one.
    pub fn framing(&self, panel: usize) -> Option<Framing> {
        self.framings.get(&panel).copied()
    }

    /// Sets the explicit framing of a layer, scale by scale: a scale missing
    /// from `framing` keeps its previous framing. Advances the generation —
    /// hence invalidates the cache.
    pub fn set_framing(&mut self, panel: usize, framing: Framing) {
        let entry = self.framings.entry(panel).or_default();
        entry.x = framing.x.or(entry.x);
        entry.y = framing.y.or(entry.y);
        entry.y2 = framing.y2.or(entry.y2);
        self.generation += 1;
    }

    /// Moves the framings to the new index of their layer after layers were
    /// removed (#357): `map[old]` is the new index, `None` if the layer is
    /// gone (its framing goes with it). Advances the generation.
    pub fn remap_panels(&mut self, map: &[Option<usize>]) {
        if self.framings.is_empty() {
            return;
        }
        self.framings = self
            .framings
            .drain()
            .filter_map(|(old, framing)| Some((map.get(old).copied().flatten()?, framing)))
            .collect();
        self.generation += 1;
    }

    /// Clears all explicit framings: back to framing on the data for all
    /// panels. No effect — and hence no cache invalidation — if none was
    /// set.
    pub fn reset(&mut self) {
        if !self.framings.is_empty() {
            self.framings.clear();
            self.generation += 1;
        }
    }

    /// Applies a framing gesture (zoom, drag, rectangle) to **all the own
    /// scales of all the layers of the cell** of layer `panel_index` (in
    /// `panels`, the drawn panels — `DrawnPlot::panels`) (#167). The layers
    /// of a cell share the same plot area: the same transformation in pixels
    /// thus keeps their ticks aligned, including those of a secondary axis.
    /// A scale linked to another layer has no framing of its own and follows
    /// the one it covers.
    ///
    /// No effect if `panel_index` is not a drawn panel.
    pub fn commit_gesture(&mut self, panels: &[DrawnPanel], panel_index: usize, gesture: Gesture) {
        let Some(hit) = panels.get(panel_index) else {
            return;
        };
        let cell = (hit.row, hit.column);
        let mut updates = Vec::new();
        for panel in panels.iter().filter(|p| (p.row, p.column) == cell) {
            let (own_x, own_y) = match panel.overlay {
                Some(overlay) => (overlay.x_top, overlay.y_right),
                None => (true, true),
            };
            let mut framing = Framing::default();
            if let Some((x, y)) = PanelTransform::of(panel).and_then(|t| t.apply(&gesture)) {
                framing.x = own_x.then_some(x);
                framing.y = own_y.then_some(y);
            }
            if panel.overlay.is_none() && panel.y2_range.is_some() {
                framing.y2 = PanelTransform::of_axis(panel, true)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y2)| y2);
            }
            updates.push((panel.index, framing));
        }
        for (index, framing) in updates {
            self.set_framing(index, framing);
        }
    }

    /// Applies a gesture to **a single scale** (#167): that of axis `side` of
    /// panel `panel_index` (in `panels`). The other scales, linked or not,
    /// do not move. The gesture acts along the axis only: the other component
    /// is neutralized at the center of the plot area.
    ///
    /// No effect if `panel_index` is not a drawn panel.
    pub fn commit_axis_gesture(
        &mut self,
        panels: &[DrawnPanel],
        panel_index: usize,
        side: AxisSide,
        gesture: Gesture,
    ) {
        let Some(panel) = panels.get(panel_index) else {
            return;
        };
        let horizontal = side == AxisSide::Bottom;
        let center = (
            panel.pixel_origin.0 as f64 + panel.pixel_size.0 as f64 / 2.0,
            panel.pixel_origin.1 as f64 + panel.pixel_size.1 as f64 / 2.0,
        );
        let gesture = match gesture {
            Gesture::Zoom { anchor, factor } => Gesture::Zoom {
                anchor: if horizontal {
                    (anchor.0, center.1)
                } else {
                    (center.0, anchor.1)
                },
                factor,
            },
            Gesture::Pan { dx, dy } => {
                if horizontal {
                    Gesture::Pan { dx, dy: 0.0 }
                } else {
                    Gesture::Pan { dx: 0.0, dy }
                }
            }
            other => other,
        };
        let mut framing = Framing::default();
        match side {
            AxisSide::Bottom => {
                framing.x = PanelTransform::of(panel)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(x, _)| x);
            }
            AxisSide::Left => {
                framing.y = PanelTransform::of(panel)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y)| y);
            }
            AxisSide::Right => {
                framing.y2 = PanelTransform::of_axis(panel, true)
                    .and_then(|t| t.apply(&gesture))
                    .map(|(_, y2)| y2);
            }
        }
        self.set_framing(panel.index, framing);
    }

    /// Applies the framings in progress to a clone of the `PlotSpec`, just
    /// before resolution. Touches nothing if no panel has an explicit
    /// framing — the fast path of the common case, with no allocation or
    /// traversal.
    pub fn apply(&self, spec: &mut PlotSpec) {
        if self.framings.is_empty() {
            return;
        }
        // `Panels` only lends its content as `&[PanelSpec]` (`Deref`, not
        // `DerefMut`: the field is private, the "at least one panel"
        // invariant holds by construction) — hence the loop over the
        // index and `get_mut` rather than a direct iteration.
        for index in 0..spec.panels.len() {
            let Some(panel) = spec.panels.get_mut(index) else {
                continue;
            };
            let Some(framing) = self.framing(index) else {
                continue;
            };
            for (axis, range) in [
                (&mut panel.bottom, framing.x),
                (&mut panel.left, framing.y),
                (&mut panel.right, framing.y2),
            ] {
                if let Some(range) = range {
                    axis.auto_range = false;
                    axis.range_min = Some(range.0);
                    axis.range_max = Some(range.1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::panel;
    use stokesforge_plot::{AxisScale, OverlaySpec, PanelSpec, Panels};

    fn spec_with_two_panels() -> PlotSpec {
        PlotSpec::empty("View")
            .with_panels(Panels::new(vec![PanelSpec::at(0, 0), PanelSpec::at(0, 1)]))
    }

    #[test]
    fn empty_touches_nothing() {
        let view = ViewState::new();
        let mut spec = spec_with_two_panels();
        let before = spec.clone();
        view.apply(&mut spec);
        assert_eq!(spec, before);
        assert_eq!(view.generation(), 0);
    }

    #[test]
    fn setting_a_framing_touches_only_its_panel() {
        let mut view = ViewState::new();
        view.set_framing(0, Framing::xy((1.0, 2.0), (3.0, 4.0)));
        let mut spec = spec_with_two_panels();
        view.apply(&mut spec);

        let first = &spec.panels[0];
        assert!(!first.bottom.auto_range);
        assert_eq!(first.bottom.range_min, Some(1.0));
        assert_eq!(first.bottom.range_max, Some(2.0));
        assert!(!first.left.auto_range);
        assert_eq!(first.left.range_min, Some(3.0));
        assert_eq!(first.left.range_max, Some(4.0));

        let second = &spec.panels[1];
        assert!(second.bottom.auto_range, "the second panel did not move");
    }

    #[test]
    fn each_framing_set_advances_the_generation() {
        let mut view = ViewState::new();
        assert_eq!(view.generation(), 0);
        view.set_framing(0, Framing::xy((0.0, 1.0), (0.0, 1.0)));
        assert_eq!(view.generation(), 1);
        view.set_framing(0, Framing::xy((0.0, 2.0), (0.0, 1.0)));
        assert_eq!(view.generation(), 2);
    }

    #[test]
    fn framings_follow_their_layer_when_layers_are_removed() {
        let mut view = ViewState::new();
        let zoom = Framing::xy((0.0, 1.0), (0.0, 1.0));
        view.set_framing(1, zoom);
        view.set_framing(0, Framing::xy((5.0, 6.0), (5.0, 6.0)));
        let before = view.generation();
        // Layer 0 deleted: layer 1 becomes 0, its zoom with it.
        view.remap_panels(&[None, Some(0)]);
        assert_eq!(view.framing(0), Some(zoom));
        assert_eq!(view.framing(1), None, "nothing left on the old index");
        assert!(view.generation() > before);
    }

    #[test]
    fn reset_without_framing_invalidates_nothing() {
        let mut view = ViewState::new();
        view.reset();
        assert_eq!(view.generation(), 0);
    }

    #[test]
    fn reset_clears_and_advances_the_generation_once() {
        let mut view = ViewState::new();
        view.set_framing(0, Framing::xy((0.0, 1.0), (0.0, 1.0)));
        view.reset();
        assert!(view.is_empty());
        assert_eq!(view.generation(), 2);
        let mut spec = spec_with_two_panels();
        let before = spec.clone();
        view.apply(&mut spec);
        assert_eq!(spec, before, "nothing left to apply after a reset");
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn a_drag_shifts_the_framing_in_the_direction_of_the_cursor() {
        let panels = [panel(0, 0, 0)];
        let mut view = ViewState::new();
        view.commit_gesture(&panels, 0, Gesture::Pan { dx: 15.0, dy: 8.0 });
        let framing = view.framing(0).expect("framing set");
        // The content follows the cursor: to the right, the framing goes back in X;
        // downward, it goes up in Y (pixel Y grows downward).
        assert!(close(framing.x.expect("x"), (-15.0, 85.0)), "{framing:?}");
        assert!(close(framing.y.expect("y"), (8.0, 108.0)), "{framing:?}");
        assert_eq!(framing.y2, None);
        assert_eq!(view.generation(), 1);
    }

    #[test]
    fn a_zoom_keeps_the_data_under_the_cursor() {
        let panels = [panel(0, 0, 0)];
        let mut view = ViewState::new();
        view.commit_gesture(
            &panels,
            0,
            Gesture::Zoom {
                anchor: (20.0, 50.0),
                factor: 0.5,
            },
        );
        let framing = view.framing(0).expect("framing set");
        assert!(close(framing.x.expect("x"), (10.0, 60.0)), "{framing:?}");
        assert!(close(framing.y.expect("y"), (25.0, 75.0)), "{framing:?}");
    }

    #[test]
    fn a_zoom_on_a_log_axis_works_on_the_logarithms() {
        let mut log = panel(0, 0, 0);
        log.x_range = (1.0, 1_000.0);
        log.x_scale = AxisScale::Log;
        let mut view = ViewState::new();
        view.commit_gesture(
            &[log],
            0,
            Gesture::Zoom {
                anchor: (50.0, 50.0),
                factor: 0.5,
            },
        );
        let (min, max) = view.framing(0).and_then(|f| f.x).expect("x framed");
        // 10^0.75 .. 10^2.25: centered on the geometric mean, not on 500.
        assert!((min - 10f64.powf(0.75)).abs() < 1e-6, "{min}");
        assert!((max - 10f64.powf(2.25)).abs() < 1e-6, "{max}");
    }

    #[test]
    fn an_inverted_axis_keeps_its_direction_after_a_rectangle() {
        let mut inverted = panel(0, 0, 0);
        inverted.x_range = (100.0, 0.0);
        let mut view = ViewState::new();
        view.commit_gesture(
            &[inverted],
            0,
            Gesture::Box {
                a: (10.0, 10.0),
                b: (60.0, 40.0),
            },
        );
        // Pixels 10 and 60 are the data 90 and 40: the pair stays going from
        // 100 to 0, otherwise the figure would flip on zoom.
        let x = view.framing(0).and_then(|f| f.x).expect("x framed");
        assert!(close(x, (90.0, 40.0)), "{x:?}");
    }

    #[test]
    fn a_gesture_frames_the_layers_of_the_cell_and_not_the_other_cells() {
        let mut overlaid = panel(1, 0, 0);
        overlaid.overlay = Some(OverlaySpec::new(false, true));
        overlaid.y_range = (0.0, 10.0);
        let mut base = panel(0, 0, 0);
        base.y2_range = Some((0.0, 1.0));
        let neighbor = panel(2, 0, 1);
        let mut own_abscissa = panel(3, 0, 0);
        own_abscissa.overlay = Some(OverlaySpec::new(true, false));
        let panels = [base, overlaid, neighbor, own_abscissa];

        let mut view = ViewState::new();
        view.commit_gesture(&panels, 0, Gesture::Pan { dx: 10.0, dy: 0.0 });

        let base = view.framing(0).expect("base layer");
        assert!(close(base.x.expect("x"), (-10.0, 90.0)));
        assert!(base.y.is_some());
        assert!(
            base.y2.is_some(),
            "the right-hand ordinate follows the gesture"
        );
        // The overlaid layer has only its own ordinate: its abscissa is
        // linked to that of the covered layer.
        let top = view.framing(1).expect("overlaid layer");
        assert_eq!(top.x, None);
        assert!(top.y.is_some());
        assert_eq!(top.y2, None);
        assert_eq!(view.framing(2), None, "another cell does not move");
        // The reverse: own abscissa, ordinate linked to the covered layer.
        let own = view.framing(3).expect("layer with its own abscissa");
        assert!(own.x.is_some());
        assert_eq!((own.y, own.y2), (None, None));
    }

    #[test]
    fn a_gesture_on_an_unknown_panel_does_nothing() {
        let mut view = ViewState::new();
        view.commit_gesture(&[panel(0, 0, 0)], 3, Gesture::Pan { dx: 1.0, dy: 1.0 });
        view.commit_axis_gesture(
            &[panel(0, 0, 0)],
            3,
            AxisSide::Bottom,
            Gesture::Pan { dx: 1.0, dy: 1.0 },
        );
        assert!(view.is_empty());
        assert_eq!(view.generation(), 0);
    }

    #[test]
    fn a_gesture_on_an_axis_frames_only_that_scale() {
        let mut base = panel(0, 0, 0);
        base.y2_range = Some((0.0, 1.0));
        let panels = [base];
        let drag = Gesture::Pan { dx: 10.0, dy: 20.0 };

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Bottom, drag);
        let bottom = view.framing(0).expect("framing");
        assert!(close(bottom.x.expect("x"), (-10.0, 90.0)), "{bottom:?}");
        assert_eq!((bottom.y, bottom.y2), (None, None));

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Left, drag);
        let left = view.framing(0).expect("framing");
        assert!(close(left.y.expect("y"), (20.0, 120.0)), "{left:?}");
        assert_eq!((left.x, left.y2), (None, None));

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Right, drag);
        let right = view.framing(0).expect("framing");
        assert!(close(right.y2.expect("y2"), (0.2, 1.2)), "{right:?}");
        assert_eq!((right.x, right.y), (None, None));
    }

    #[test]
    fn a_zoom_on_an_axis_is_centered_on_the_cursor_of_that_axis() {
        let panels = [panel(0, 0, 0)];
        let zoom = Gesture::Zoom {
            anchor: (20.0, 90.0),
            factor: 0.5,
        };
        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Bottom, zoom);
        let x = view.framing(0).and_then(|f| f.x).expect("x");
        assert!(close(x, (10.0, 60.0)), "{x:?}");

        let mut view = ViewState::new();
        view.commit_axis_gesture(&panels, 0, AxisSide::Left, zoom);
        // Pixel 90 is the data 10: (0-10)*0.5+10 .. (100-10)*0.5+10.
        let y = view.framing(0).and_then(|f| f.y).expect("y");
        assert!(close(y, (5.0, 55.0)), "{y:?}");
    }

    #[test]
    fn a_framing_set_by_a_gesture_goes_into_the_spec() {
        let mut view = ViewState::new();
        view.commit_gesture(&[panel(0, 0, 0)], 0, Gesture::Pan { dx: 10.0, dy: 0.0 });
        let mut spec = PlotSpec::empty("View").with_panels(Panels::new(vec![PanelSpec::at(0, 0)]));
        view.apply(&mut spec);
        let bottom = &spec.panels[0].bottom;
        assert!(!bottom.auto_range);
        assert_eq!(
            (bottom.range_min, bottom.range_max),
            (Some(-10.0), Some(90.0))
        );
    }
}

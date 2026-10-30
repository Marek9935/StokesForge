//! Who holds which side of a graph cell (#396).
//!
//! A cell of the grid has four sides: bottom, left, right, top. An ordinary
//! layer draws its X axis at the bottom and its Y axis on the left; its top
//! and right are mirror axes (#176), or, on the right, the real axis of the
//! series attached to it. An overlaid layer (#167) stores its X axis in the
//! `bottom` field and its Y axis in `left`, but draws them at the top and on
//! the right, each with its own scale only if `x_top` / `y_right` (#168).
//!
//! This module is the **single** answer to "which side does this field sit
//! on, and is the side free for this layer": the Object Manager (checkbox of
//! a mirror, disabled when an overlay takes its side) and Graph Properties ›
//! Axes (four tabs, greyed when the side is taken) both ask it. Two overlays
//! never take a side from each other: their axes stack outward.
//!
//! No UI type here (#263): texts and tabs are built by the callers.

use stokesforge_core::compose::overlay_base;
use stokesforge_plot::{AxisSide, PanelSpec, PlotSpec};

/// The four sides of a cell, in the order of the tabs of Graph Properties.
pub const PLACES: [AxisSide; 4] = [
    AxisSide::Bottom,
    AxisSide::Left,
    AxisSide::Right,
    AxisSide::Top,
];

/// Who makes a side unavailable to a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideHolder {
    /// The axis of the ordinary layer the overlay covers (bottom, left).
    Base { layer: usize },
    /// The mirror axis that an ordinary layer displays (top, right), which
    /// keeps the side as long as no overlay draws its own axis there.
    Mirror { layer: usize },
    /// An overlaid layer draws its own axis here: the mirror of the ordinary
    /// layer is not displayed.
    Overlay { layer: usize },
}

/// What a side of the cell means for one layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideUse {
    /// The side of the cell (the name of the tab).
    pub place: AxisSide,
    /// The `PanelSpec` field that this side edits; `None` when the layer has
    /// no axis of its own here.
    pub field: Option<AxisSide>,
    /// The field is a mirror axis (#176): its checkbox is `AxisSpec::mirror`.
    pub mirror: bool,
    /// The side carries the overlay's "Own scale" (`x_top` / `y_right`).
    pub own_scale: bool,
    /// Why the side is unavailable to this layer, if it is.
    pub held_by: Option<SideHolder>,
}

impl SideUse {
    /// The layer can edit this side.
    pub fn available(&self) -> bool {
        self.field.is_some() && self.held_by.is_none()
    }

    /// The tooltip of a greyed tab: who holds the side, and what to do (S-G2).
    /// `None` when the side is available.
    pub fn reason(&self) -> Option<String> {
        use crate::state::PlotAxis;
        let holder = self.held_by?;
        let number = |layer: usize| layer as u64 + 1;
        let vertical = matches!(self.place, AxisSide::Left | AxisSide::Right);
        Some(match holder {
            SideHolder::Base { layer } => {
                // The overlay's own axis is on the opposite side of the same direction.
                let tab = PlotAxis::from_side(if vertical {
                    AxisSide::Right
                } else {
                    AxisSide::Top
                })
                .label();
                if vertical {
                    crate::tr!(
                        "disabled-axis-side-base-y",
                        layer = number(layer),
                        tab = tab
                    )
                } else {
                    crate::tr!(
                        "disabled-axis-side-base-x",
                        layer = number(layer),
                        tab = tab
                    )
                }
            }
            SideHolder::Mirror { layer } => crate::tr!(
                "disabled-axis-side-mirror",
                layer = number(layer),
                tab = PlotAxis::from_side(self.place).label()
            ),
            SideHolder::Overlay { layer } => {
                let tab = PlotAxis::from_side(self.place).label();
                if vertical {
                    crate::tr!(
                        "disabled-axis-side-overlay-y",
                        layer = number(layer),
                        tab = tab
                    )
                } else {
                    crate::tr!(
                        "disabled-axis-side-overlay-x",
                        layer = number(layer),
                        tab = tab
                    )
                }
            }
        })
    }
}

/// The side an overlay draws the axis stored in `field`: its X axis (`bottom`)
/// at the top, its Y axis (`left`) on the right (#171).
pub fn displayed_place(field: AxisSide) -> AxisSide {
    match field {
        AxisSide::Left => AxisSide::Right,
        AxisSide::Bottom => AxisSide::Top,
        other => other,
    }
}

/// The overlay covers an ordinary layer of its cell; an overlay with nothing
/// to cover is drawn, and edited, like an ordinary layer.
fn covering(spec: &PlotSpec, panel: usize) -> Option<usize> {
    let base = overlay_base(spec, panel);
    (base != panel).then_some(base)
}

/// An overlay of the cell draws its own axis at `place` (top or right).
fn overlay_drawn_at(spec: &PlotSpec, cell: (usize, usize), place: AxisSide) -> Option<usize> {
    spec.panels.iter().enumerate().find_map(|(index, other)| {
        let overlay = other.overlay?;
        let drawn = match place {
            AxisSide::Top => overlay.x_top && other.bottom.visible,
            _ => overlay.y_right && other.left.visible,
        };
        (cell_of(other) == cell && drawn).then_some(index)
    })
}

fn cell_of(panel: &PanelSpec) -> (usize, usize) {
    (panel.row, panel.column)
}

/// The ordinary layer of the cell whose mirror at `place` is displayed:
/// requested, and with no overlay axis on that side.
fn mirror_displayed(spec: &PlotSpec, cell: (usize, usize), place: AxisSide) -> Option<usize> {
    if overlay_drawn_at(spec, cell, place).is_some() {
        return None;
    }
    spec.panels.iter().position(|other| {
        cell_of(other) == cell
            && match place {
                AxisSide::Top => other.mirrors_top(),
                _ => other.mirrors_right(),
            }
    })
}

/// What the side `place` of the cell of layer `panel` means for that layer.
/// A layer that does not exist has nothing anywhere.
pub fn side_use(spec: &PlotSpec, panel: usize, place: AxisSide) -> SideUse {
    let none = SideUse {
        place,
        field: None,
        mirror: false,
        own_scale: false,
        held_by: None,
    };
    let Some(layer) = spec.panels.get(panel) else {
        return none;
    };
    let cell = cell_of(layer);
    if layer.overlay.is_none() {
        let mirror = match place {
            AxisSide::Top => true,
            AxisSide::Right => !layer.uses_right_axis(),
            _ => false,
        };
        let held_by = mirror
            .then(|| overlay_drawn_at(spec, cell, place))
            .flatten()
            .map(|layer| SideHolder::Overlay { layer });
        return SideUse {
            field: Some(place),
            mirror,
            held_by,
            ..none
        };
    }
    let Some(base) = covering(spec, panel) else {
        return SideUse {
            field: Some(place),
            ..none
        };
    };
    match place {
        AxisSide::Bottom | AxisSide::Left => SideUse {
            held_by: Some(SideHolder::Base { layer: base }),
            ..none
        },
        AxisSide::Top | AxisSide::Right => SideUse {
            field: Some(if place == AxisSide::Top {
                AxisSide::Bottom
            } else {
                AxisSide::Left
            }),
            own_scale: true,
            held_by: mirror_displayed(spec, cell, place).map(|layer| SideHolder::Mirror { layer }),
            ..none
        },
    }
}

/// The four sides of layer `panel`, in the order of [`PLACES`].
pub fn sides(spec: &PlotSpec, panel: usize) -> [SideUse; 4] {
    PLACES.map(|place| side_use(spec, panel, place))
}

/// The side that the field `field` of layer `panel` is on: the reverse of
/// [`SideUse::field`]. Every opener of the axis properties (Object Manager,
/// click on the figure, right-click) goes through it.
pub fn place_of(spec: &PlotSpec, panel: usize, field: AxisSide) -> AxisSide {
    if covering(spec, panel).is_some() {
        displayed_place(field)
    } else {
        field
    }
}

/// The side to show when `wanted` is not available to layer `panel`: the other
/// side of the same axis (bottom/top, left/right), then the first available
/// side, then `wanted` itself when the layer has nothing.
pub fn usable_place(spec: &PlotSpec, panel: usize, wanted: AxisSide) -> AxisSide {
    let sides = sides(spec, panel);
    let available = |place: AxisSide| sides.iter().any(|s| s.place == place && s.available());
    let opposite = match wanted {
        AxisSide::Bottom => AxisSide::Top,
        AxisSide::Top => AxisSide::Bottom,
        AxisSide::Left => AxisSide::Right,
        AxisSide::Right => AxisSide::Left,
    };
    [wanted, opposite]
        .into_iter()
        .chain(PLACES)
        .find(|place| available(*place))
        .unwrap_or(wanted)
}

/// An overlay that draws its axis at the top or on the right takes the cell's
/// mirror there (#396): the mirror is switched off in the same spec, hence in
/// the same undoable command as the creation of the overlay. Applied to every
/// overlay of `spec`; a mirror that is not requested stays untouched.
pub fn switch_off_mirrors_under_overlays(spec: &mut PlotSpec) {
    let taken: Vec<((usize, usize), AxisSide)> = spec
        .panels
        .iter()
        .flat_map(|panel| {
            let overlay = panel.overlay?;
            Some([
                (overlay.x_top && panel.bottom.visible).then_some((cell_of(panel), AxisSide::Top)),
                (overlay.y_right && panel.left.visible)
                    .then_some((cell_of(panel), AxisSide::Right)),
            ])
        })
        .flatten()
        .flatten()
        .collect();
    for (cell, place) in taken {
        for panel in spec.panels.iter_mut() {
            if cell_of(panel) != cell || panel.overlay.is_some() {
                continue;
            }
            if place == AxisSide::Top && panel.top.mirror {
                panel.top.mirror = false;
            } else if place == AxisSide::Right && panel.mirrors_right() {
                panel.right.mirror = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::NodeId;
    use stokesforge_plot::{OverlaySpec, Panels, SeriesAxis, SeriesSpec};

    fn spec_of(panels: Vec<PanelSpec>) -> PlotSpec {
        PlotSpec::empty("T").with_panels(Panels::new(panels))
    }

    /// Layer 0 ordinary, layer 1 overlaid with `overlay`.
    fn base_and_overlay(overlay: OverlaySpec) -> PlotSpec {
        let base = PanelSpec::at(0, 0);
        let top = PanelSpec::overlay_on(&base, overlay);
        spec_of(vec![base, top])
    }

    #[test]
    fn an_ordinary_layer_alone_owns_its_four_sides_under_their_own_names() {
        let spec = spec_of(vec![PanelSpec::at(0, 0)]);
        for side in sides(&spec, 0) {
            assert_eq!(side.field, Some(side.place));
            assert_eq!(side.held_by, None);
            assert!(!side.own_scale);
        }
        let mirrors: Vec<bool> = sides(&spec, 0).iter().map(|s| s.mirror).collect();
        assert_eq!(mirrors, [false, false, true, true]);
    }

    #[test]
    fn an_overlay_edits_its_x_at_the_top_and_its_y_on_the_right() {
        let spec = base_and_overlay(OverlaySpec::new(true, true));
        let top = side_use(&spec, 1, AxisSide::Top);
        let right = side_use(&spec, 1, AxisSide::Right);
        assert_eq!(top.field, Some(AxisSide::Bottom));
        assert_eq!(right.field, Some(AxisSide::Left));
        assert!(top.own_scale && right.own_scale);
        assert!(top.available() && right.available());
        for place in [AxisSide::Bottom, AxisSide::Left] {
            let side = side_use(&spec, 1, place);
            assert_eq!(side.field, None);
            assert_eq!(side.held_by, Some(SideHolder::Base { layer: 0 }));
            assert!(!side.available());
        }
    }

    #[test]
    fn the_series_of_an_overlay_on_the_right_do_not_move_its_right_tab() {
        let mut spec = base_and_overlay(OverlaySpec::new(true, true));
        let layer = &mut spec.panels.get_mut(1).unwrap();
        layer.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        layer.series[0].axis = SeriesAxis::Right;
        assert_eq!(
            side_use(&spec, 1, AxisSide::Right).field,
            Some(AxisSide::Left)
        );
        assert!(!side_use(&spec, 1, AxisSide::Right).mirror);
    }

    #[test]
    fn the_top_of_the_base_is_held_only_while_an_overlay_draws_its_x_there() {
        let mut spec = base_and_overlay(OverlaySpec::new(true, false));
        let held = side_use(&spec, 0, AxisSide::Top).held_by;
        assert_eq!(held, Some(SideHolder::Overlay { layer: 1 }));
        // Axis hidden: the overlay keeps its scale but draws nothing.
        spec.panels.get_mut(1).unwrap().bottom.visible = false;
        assert_eq!(side_use(&spec, 0, AxisSide::Top).held_by, None);
        // Linked scale: no axis either, even if the flag says visible.
        let mut spec = base_and_overlay(OverlaySpec::new(false, false));
        spec.panels.get_mut(1).unwrap().bottom.visible = true;
        assert_eq!(side_use(&spec, 0, AxisSide::Top).held_by, None);
    }

    #[test]
    fn a_real_right_axis_is_not_held_by_an_overlay_on_the_right() {
        let mut spec = base_and_overlay(OverlaySpec::new(false, true));
        assert_eq!(
            side_use(&spec, 0, AxisSide::Right).held_by,
            Some(SideHolder::Overlay { layer: 1 })
        );
        let base = spec.panels.get_mut(0).unwrap();
        base.series.push(SeriesSpec::new(NodeId::from_raw(900)));
        base.series[0].axis = SeriesAxis::Right;
        let right = side_use(&spec, 0, AxisSide::Right);
        assert!(!right.mirror);
        assert_eq!(right.held_by, None, "it stacks with the overlay bands");
    }

    #[test]
    fn two_overlays_never_hold_a_side_from_each_other() {
        let base = PanelSpec::at(0, 0);
        let second = PanelSpec::overlay_on(&base, OverlaySpec::role_layer(2));
        let third = PanelSpec::overlay_on(&base, OverlaySpec::role_layer(3));
        let spec = spec_of(vec![base, second, third]);
        for layer in [1, 2] {
            for place in [AxisSide::Top, AxisSide::Right] {
                assert_eq!(side_use(&spec, layer, place).held_by, None);
            }
        }
    }

    #[test]
    fn an_overlay_in_another_cell_takes_nothing() {
        let first = PanelSpec::at(0, 0);
        let second = PanelSpec::at(0, 1);
        let overlay = PanelSpec::overlay_on(&second, OverlaySpec::new(true, true));
        let spec = spec_of(vec![first, second, overlay]);
        assert_eq!(side_use(&spec, 0, AxisSide::Top).held_by, None);
        assert_eq!(side_use(&spec, 0, AxisSide::Right).held_by, None);
        assert_eq!(
            side_use(&spec, 1, AxisSide::Top).held_by,
            Some(SideHolder::Overlay { layer: 2 })
        );
    }

    #[test]
    fn a_displayed_mirror_holds_the_overlay_side_and_a_hidden_axis_frees_it() {
        let mut spec = base_and_overlay(OverlaySpec::new(true, true));
        // The overlay axes are hidden, the base shows its top mirror.
        spec.panels.get_mut(1).unwrap().bottom.visible = false;
        spec.panels.get_mut(0).unwrap().top.mirror = true;
        assert_eq!(
            side_use(&spec, 1, AxisSide::Top).held_by,
            Some(SideHolder::Mirror { layer: 0 })
        );
        // Right: the overlay's Y is hidden too, but no mirror on the right.
        spec.panels.get_mut(1).unwrap().left.visible = false;
        assert_eq!(side_use(&spec, 1, AxisSide::Right).held_by, None);
        spec.panels.get_mut(0).unwrap().right.mirror = true;
        assert_eq!(
            side_use(&spec, 1, AxisSide::Right).held_by,
            Some(SideHolder::Mirror { layer: 0 })
        );
        // The mirror off: the tab is back.
        spec.panels.get_mut(0).unwrap().top.mirror = false;
        assert_eq!(side_use(&spec, 1, AxisSide::Top).held_by, None);
    }

    #[test]
    fn an_old_file_with_both_keeps_the_overlay_and_blocks_the_mirror() {
        let mut spec = base_and_overlay(OverlaySpec::new(true, false));
        spec.panels.get_mut(0).unwrap().top.mirror = true;
        assert_eq!(
            side_use(&spec, 0, AxisSide::Top).held_by,
            Some(SideHolder::Overlay { layer: 1 })
        );
        assert_eq!(side_use(&spec, 1, AxisSide::Top).held_by, None);
    }

    #[test]
    fn an_overlay_with_nothing_to_cover_is_edited_like_an_ordinary_layer() {
        let mut orphan = PanelSpec::at(0, 0);
        orphan.overlay = Some(OverlaySpec::new(true, true));
        let spec = spec_of(vec![orphan]);
        for side in sides(&spec, 0) {
            assert_eq!(side.field, Some(side.place));
            assert_eq!(side.held_by, None);
        }
        assert_eq!(place_of(&spec, 0, AxisSide::Bottom), AxisSide::Bottom);
    }

    #[test]
    fn the_reverse_lookup_names_a_field_where_it_is_drawn() {
        let spec = base_and_overlay(OverlaySpec::new(true, true));
        assert_eq!(place_of(&spec, 1, AxisSide::Bottom), AxisSide::Top);
        assert_eq!(place_of(&spec, 1, AxisSide::Left), AxisSide::Right);
        for field in PLACES {
            assert_eq!(place_of(&spec, 0, field), field);
        }
    }

    #[test]
    fn a_taken_tab_falls_back_to_the_other_side_of_the_same_axis() {
        let spec = base_and_overlay(OverlaySpec::new(true, true));
        assert_eq!(usable_place(&spec, 1, AxisSide::Bottom), AxisSide::Top);
        assert_eq!(usable_place(&spec, 1, AxisSide::Left), AxisSide::Right);
        assert_eq!(usable_place(&spec, 1, AxisSide::Top), AxisSide::Top);
        // Top also held (a mirror): the first available tab is taken.
        let mut spec = spec;
        spec.panels.get_mut(1).unwrap().bottom.visible = false;
        spec.panels.get_mut(0).unwrap().top.mirror = true;
        assert_eq!(usable_place(&spec, 1, AxisSide::Bottom), AxisSide::Right);
    }

    #[test]
    fn creating_an_overlay_axis_switches_the_mirror_off_in_the_same_spec() {
        let mut spec = base_and_overlay(OverlaySpec::new(true, true));
        spec.panels.get_mut(0).unwrap().top.mirror = true;
        spec.panels.get_mut(0).unwrap().right.mirror = true;
        switch_off_mirrors_under_overlays(&mut spec);
        assert!(!spec.panels[0].top.mirror && !spec.panels[0].right.mirror);
        // An overlay without a displayed axis leaves the mirror alone.
        let mut spec = base_and_overlay(OverlaySpec::new(false, false));
        spec.panels.get_mut(0).unwrap().top.mirror = true;
        switch_off_mirrors_under_overlays(&mut spec);
        assert!(spec.panels[0].top.mirror);
    }
}

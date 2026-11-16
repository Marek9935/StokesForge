//! Applying a graph theme to a graph (#538): the part of the theme engine that
//! needs the project (text measurement, grid helpers of `stokesforge-core`),
//! around the pure copy of [`PlotSpec::apply_theme_families`].
//!
//! `stokesforge-plot` copies the families but cannot measure text nor reshape
//! the grid, so two steps live here, before the copy:
//!
//! - **placement**: when the Size and page family is applied and the theme and
//!   the graph differ in placement (regular grid or free frames), the graph is
//!   switched first through [`frame_sizes::set_placement_in_draft`], the same
//!   helper as the Graph Properties switch, so nothing moves on screen before
//!   the theme's own sizes land. A refused switch leaves the sizes alone;
//! - **missing layers**: with « Add missing layers », a graph that has fewer
//!   layers than the theme gets empty layers up to the theme's count
//!   ([`add_missing_layers`]); without it only the existing layers are styled.
//!
//! The result is a spec for [`stokesforge_commands::Command::SetPlotSpec`],
//! which the application state sends through `execute`: that path already
//! fixes the frames of a legacy graph and freezes the automatic ranges (#529),
//! so nothing of that is done here.

use stokesforge_core::compose::{insert_cell, CellSide};
use stokesforge_core::Tree;
use stokesforge_plot::{GraphTheme, LayerPlacement, PlotSpec, ThemeFamilies, ThemeFamily};

use crate::frame_sizes;

/// The spec `current` once the `families` of `theme` are applied (module
/// documentation). `current` itself is never changed; the caller compares the
/// result with it to know whether anything changed.
pub fn themed_spec(
    tree: &Tree,
    current: &PlotSpec,
    theme: &GraphTheme,
    families: ThemeFamilies,
    add_missing: bool,
) -> PlotSpec {
    let mut next = current.clone();
    if !theme.colors_only {
        if families.contains(ThemeFamily::SizeAndPage)
            && theme.style.layer_placement != next.layer_placement
        {
            let free = theme.style.layer_placement == LayerPlacement::Free;
            // Refused: the placements stay different and the copy skips the sizes.
            frame_sizes::set_placement_in_draft(tree, &mut next, free);
        }
        if add_missing {
            add_missing_layers(&mut next, &theme.style);
        }
    }
    next.apply_theme_families(theme, families);
    next
}

/// Adds empty layers to `spec` until it has as many as `theme`.
///
/// Each new layer opens a cell beside the previous layer, **below** it when the
/// theme's layer sits in a lower row than the theme's previous layer and in no
/// further column (a stack, like a fit and its residuals), **right** of it
/// otherwise. An overlaid layer of the theme becomes an ordinary cell: the
/// overlay roles (axes on top or right) belong to the data they carry. Stops
/// quietly if the grid is full (`insert_cell` refuses past its limit).
fn add_missing_layers(spec: &mut PlotSpec, theme: &PlotSpec) {
    for index in spec.panels.len()..theme.panels.len() {
        let (this, previous) = (
            &theme.panels.as_slice()[index],
            &theme.panels.as_slice()[index - 1],
        );
        let side = if this.row > previous.row && this.column <= previous.column {
            CellSide::Below
        } else {
            CellSide::Right
        };
        if insert_cell(spec, index - 1, side).is_none() {
            break;
        }
    }
}

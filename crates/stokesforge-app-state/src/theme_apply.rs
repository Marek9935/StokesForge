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
//! Both steps now live in [`stokesforge_render_svg::themed_spec`] (#546), shared
//! with scripts; this module only passes the application's figure texts.
//!
//! The result is a spec for [`stokesforge_commands::Command::SetPlotSpec`],
//! which the application state sends through `execute`: that path already
//! fixes the frames of a legacy graph and freezes the automatic ranges (#529),
//! so nothing of that is done here.

use stokesforge_core::Tree;
use stokesforge_plot::{GraphTheme, PlotSpec, ThemeFamilies};

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
    stokesforge_render_svg::themed_spec(
        tree,
        current,
        theme,
        families,
        add_missing,
        &crate::figure_texts::figure_texts(),
    )
}

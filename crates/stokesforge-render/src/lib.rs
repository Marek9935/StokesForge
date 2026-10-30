//! The common base of the two rendering backends — phase R4.
//!
//! This crate draws nothing by itself. It carries the two things that
//! `stokesforge-render-egui` and `stokesforge-render-svg` must absolutely have
//! **in a single copy**, without which the end criterion of the rendering
//! work package — *axes in the same place on screen and in the export* —
//! would only be a wish:
//!
//! - [`text`]: the single source of text measurement (task R4.1), an
//!   `epaint::FontDefinitions` and the one `estimate_text_size` that follows
//!   from it;
//! - [`pipeline`]: the sequence of Plotters calls, generic over the backend,
//!   so that there are not two ways to build the same figure;
//! - [`color`]: the reading of the spec's `#RRGGBB` colors, also shared.
//!
//! # What phase R5 added here
//!
//! The whole extension to the real `PlotSpec` went down here, and not one
//! line into either backend — this is the phase rule ("no feature enters a
//! single backend"), held by the structure:
//!
//! - [`series`]: the six [`stokesforge_plot::PlotType`]s translated into
//!   geometry, once, for the framing as well as for the drawing;
//! - [`scale`]: the axis scales, including the logarithmic one;
//! - [`paint`]: the pixel primitives — line styles, markers, bars,
//!   whiskers;
//! - [`legend`]: the legend, laid out on the project's measurement and not
//!   on Plotters' (the reason is in the module);
//! - [`decimate`]: the LTTB;
//! - [`sample`]: a realistic graph, shared by the export example and by the
//!   agreement test, so that what one looks at is what the test compares.
//!
//! # A crate not planned by `architecture-choisie.md` §8
//!
//! The decided split planned only `stokesforge-render-egui` and
//! `stokesforge-render-svg`. This crate is added, for the same reason that
//! `stokesforge-id` was added in R3: two sibling crates need the same thing,
//! and neither can depend on the other without lying about what it is.
//! Making `stokesforge-render-svg` carry the measurement would force the
//! screen backend to depend on the export crate; making
//! `stokesforge-render-egui` carry it would pull the whole egui stack into
//! the export, which the split explicitly forbids. The way out is the same
//! as in R3: move the common part down into a crate below the two.
//!
//! Consequence on the boundaries, and it goes the right way: `epaint`
//! enters `stokesforge-render-svg` through here, not directly, and stays the
//! **same version** as the one `egui` resolves for `stokesforge-render-egui`
//! — this is checked by `tests/no_ui_dependency.rs` rather than assumed.
//!
//! # What this crate does not know
//!
//! Neither `egui`, nor `eframe`, nor `plotters-svg`. It sees `epaint` (to
//! measure) and `plotters` without its fonts (to build the figure) — nothing
//! of the output surface. This is what lets it be the common reference
//! rather than a third dialect.
//!
//! # What a one-off task of 2026-09-13 added here
//!
//! [`fonts`]: the choice between three pairs of embedded fonts (Libertinus,
//! Fira, Noto — text and Math) or a system font designated by its name, with
//! a silent fallback on Noto. [`text::font_definitions`] becomes the default
//! entry point; the invariant that holds (measure with exactly the same set
//! of fonts on both sides) does not change — see the module documentation and
//! `docs/archive/newarchitecture/architecture-choisie.md` §7.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod clip;
pub mod color;
pub mod decimate;
pub mod fonts;
pub mod formula;
pub mod hit;
pub mod interact;
pub mod legend;
pub mod paint;
pub mod pipeline;
pub mod sample;
pub mod scale;
pub mod series;
pub mod series_selection;
pub mod shapes;
pub mod subpixel;
pub mod text;
pub mod texts;
pub mod view;

#[cfg(test)]
mod test_support;

use stokesforge_core::NodeId;

/// What can fail when displaying a graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The requested node does not exist in the snapshot.
    UnknownNode(NodeId),
    /// The node exists but is not a graph.
    NotAGraph(NodeId),
    /// Plotters refused to draw. The message is Plotters': rewording it
    /// would lose the only useful information.
    Drawing(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(f, "no node {id:?} in this snapshot"),
            Self::NotAGraph(id) => write!(f, "node {id:?} is not a graph"),
            Self::Drawing(message) => write!(f, "drawing failed: {message}"),
        }
    }
}

impl std::error::Error for RenderError {}

pub use color::{parse_hex, rgb_or};
pub use decimate::lttb;
pub use fonts::{available_families, FamilyEntry, FontChoice, FontSet, FontSource, ResolvedFonts};
pub use hit::{
    cell_edge_at, component_at, component_at_with_tolerance, figure_edge_at, AxisSide, CellEdge,
    FigureEdge, PlotComponent, AXIS_LINE_TOLERANCE_POINTS, CELL_EDGE_MAX_TOLERANCE,
    CELL_EDGE_TOLERANCE_POINTS,
};
pub use interact::{
    axis_frame, fit_range, nearest_on_curve, snap_to_curve, snap_to_curve_zone, Gesture,
    PanelTransform, SnapZone,
};
pub use pipeline::{
    draw, draw_on, draw_on_with_texts, draw_with_texts, AxisArea, AxisAreas, DrawnAnnotation,
    DrawnLegendRow, DrawnPanel, DrawnPlot, DrawnText,
};
pub use series::{Geometry, Prepared};
pub use shapes::DrawnShape;
pub use text::{estimate, font_definitions, font_id, text_options, TextMeasurer, TextSource};
pub use texts::FigureTexts;

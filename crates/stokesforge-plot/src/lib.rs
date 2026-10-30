//! `PlotSpec`: the description of a graph, **independent of any renderer**.
//!
//! This crate says *what to plot*. It does not know *how* to plot it,
//! and the compiler checks this: its only dependency is
//! `stokesforge-id`, for the [`stokesforge_id::NodeId`]
//! (`tests/no_ui_dependency.rs`). No `plotters`, no `egui`, no `typst` — not
//! even `serde`, since persistence has its own record types and
//! does the conversion itself.
//!
//! # Since phase R3: `stokesforge-id`, not `stokesforge-core`
//!
//! Until phase R3, this dependency was the whole of `stokesforge-core`, for
//! this one type. `NodeKind::Graph` had to start carrying a [`PlotSpec`] —
//! which created the reverse edge `stokesforge-core → stokesforge-plot` — and the
//! two edges together would have formed a direct Cargo cycle (`stokesforge-plot →
//! stokesforge-core → stokesforge-plot`), which Cargo refuses for normal
//! dependencies. So `NodeId` was moved down into `stokesforge-id`, a minimal crate
//! with no dependency: this crate now points to it, and `stokesforge-core` points
//! to it too *and* depends on `stokesforge-plot`. Nothing in the API or the
//! behavior of this crate changed — only the address of its single dependency
//! moved, to a crate smaller than the old one. Full detail:
//! `stokesforge-id/src/lib.rs`.
//!
//! This is the exact counterpart of the "no Qt in `core`" rule of the base, and
//! the reason is measured: the equivalent layer of the Python version
//! survived a complete change of renderer without a single line moving
//! (`docs/archive/newarchitecture/roadmap-rendu.md`).
//!
//! # What the layer describes
//!
//! ```text
//! PlotSpec
//! ├── title, background
//! └── panels : PanelSpec       (row/column grid, at least one panel)
//!     ├── title
//!     ├── bottom / left / right / top → AxisSpec  (scale, limits, grid)
//!     ├── legend : LegendSpec
//!     └── series : SeriesSpec
//!         ├── y : NodeId, x / x_error / y_error : Option<NodeId>
//!         ├── plot_type : scatter | line | line+scatter | bar | histogram | error_bar
//!         ├── group : Option<u32>   (curve group of the panel, #177)
//!         ├── line : LineSpec      (style, width, color)
//!         ├── symbol : SymbolSpec  (shape, size, colors)
//!         └── axis : SeriesAxis    (left or right, and nothing else)
//! ```
//!
//! # Three decisions shape this crate
//!
//! - **reference by identity** — a series designates its columns by
//!   [`stokesforge_id::NodeId`], never by name or path
//!   (`architecture-choisie.md` §11.1). Renaming or moving a column
//!   leaves the graph intact; deleting it produces a *detectable* broken
//!   reference, which rendering must show as such and never as an empty
//!   series. This is literally the first dependency edge of the
//!   project: a graph is a derived node that computes nothing;
//!
//! - **no data** — a spec only carries identities and style. It weighs
//!   its strings, which makes undoing a graph change
//!   trivial: the undo delta is the old spec;
//!
//! - **illegal states are not validated, they cannot be written** — a
//!   series attaches to [`SeriesAxis`] (two variants) and not to
//!   [`AxisSide`] (four); its Y column is a `NodeId` and not a string
//!   that could be empty; its bin count is a
//!   [`std::num::NonZeroU32`] that does not have to be brought back to 1; the position of a
//!   panel in the grid is a `usize` that does not have to be brought back to 0. Where
//!   the Python version raised or corrected at run time, there is nothing left
//!   to raise. The only invariant left to hold by construction is "at
//!   least one panel" ([`Panels`]).
//!
//! # What is not here
//!
//! **Resolution** (`NodeId` → tree column, point preparation,
//! histogram bins, derived bar width) is the next slice;
//! it will be the only place in the rendering work package that knows a `Tree`.
//! **Editing** (pure spec → spec functions) and **serialization** are
//! not in this crate either: the on-disk format belongs to
//! `stokesforge-persistence`, which converts from its own records.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod annotation;
pub mod axis;
pub mod gradient;
pub mod legend;
pub mod palettes;
pub mod panel;
pub mod raw;
pub mod saved;
pub mod series;
pub mod shape;
pub mod spec;
pub mod style;
pub mod text_anchor;
pub mod text_font;
pub mod theme;

pub use annotation::{AnnotationRef, AnnotationSpec, TextAlign};
pub use axis::{AxisScale, AxisSide, AxisSpec, TickDirection};
pub use gradient::{gradient_colors, GradientPreset, GRADIENT_PRESETS};
pub use legend::{LegendPosition, LegendSpec};
pub use palettes::{NamedPalette, PaletteNature, CURVE_PALETTES};
pub use panel::{OverlaySpec, PanelSpec, SeriesSlot};
pub use raw::RawVariant;
pub use saved::{ColorUsage, SavedColors, SavedGradient, MAX_SAVED_COLORS};
pub use series::{
    BinCount, PlotType, SeriesAxis, SeriesSpec, DEFAULT_HISTOGRAM_BINS, MAX_HISTOGRAM_BINS,
};
pub use shape::{ShapeKind, ShapeRef, ShapeSpec};
pub use spec::{
    Panels, PlotSpec, SeriesGroupRef, SeriesRef, DEFAULT_FIGURE_SIZE, MAX_CELL_SIZE,
    MAX_FIGURE_EXTENT, MIN_CELL, TRANSPARENT,
};
pub use style::{
    palette_color, parse_hex, LineSpec, LineStyle, SymbolShape, SymbolSpec, DEFAULT_PALETTE,
};
pub use text_anchor::{TextAnchor, TextTarget};
pub use text_font::TextFont;
pub use theme::{Theme, THEMES};

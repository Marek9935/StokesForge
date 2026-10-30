//! StokesForge data model: the project tree (`Folder → Workbook →
//! Worksheet → Column`, with `Graph`/`Matrix`/`Note` as sheets) and nothing
//! else — see `docs/archive/rustrewrite/roadmap.md` phase 1.
//!
//! Non-negotiable rule, checked by `tests/no_ui_dependency.rs`: this crate
//! imports no UI dependency.
//!
//! # Since phase R3: one dependency on `stokesforge-plot`, for one reason
//!
//! `NodeKind::Graph` now carries a `stokesforge_plot::PlotSpec`
//! (`docs/archive/newarchitecture/roadmap-rendu.md` phase R3): a project graph
//! carries its plot content, as a column carries its own values. This is
//! **not** an open door to rendering — `stokesforge-plot` itself stays free of
//! drawing dependencies (`stokesforge-plot/tests/no_ui_dependency.rs`), and its
//! only dependency is now `stokesforge-id` (see below), not this crate.
//! [`resolve`] is the only module that turns a spec into data ready to
//! plot (`NodeId` → column); the rest of the crate still ignores
//! the very existence of `PlotSpec`.
//!
//! **Why `stokesforge-id` and not `stokesforge-core` in the `Cargo.toml` of
//! `stokesforge-plot`.** Before R3, `stokesforge-plot` depended on this whole
//! crate for its sole use of `NodeId`. Once this crate itself depends on
//! `stokesforge-plot` (for `PlotSpec`), keeping both edges would have formed a
//! direct Cargo cycle — checked, not assumed: `cargo check` rejects it with
//! *cyclic package dependency*, since Cargo does not allow circular normal
//! dependencies (only development dependencies can be circular, which does
//! not apply here because `PlotSpec` is a production field, not a test
//! need). So `NodeId` moved down into
//! `stokesforge-id`, a minimal crate with no dependency, re-exported here unchanged
//! (`pub use id::NodeId`, below): no caller of
//! `stokesforge_core::NodeId` has to change. Full details in
//! `crates/stokesforge-id/src/lib.rs`.
//!
//! The model is now built. It rests on three decisions:
//!
//! - **identity** — `NodeId` is a monotonic counter, never reused over
//!   the whole life of a project (reloading included); the allocator lives in
//!   `Project`, outside the tree, so that an undo does not move it
//!   back;
//! - **persistent tree** — `Arc<Tree>` over a flat map
//!   `NodeId → Arc<Node>`: a mutation clones the map and only the touched nodes,
//!   so an old `Arc<Tree>` stays a frozen snapshot whose untouched nodes
//!   are physically shared with the current tree;
//! - **a single mutation facade** — every mutation goes through `Project`,
//!   whose only `&mut` will be held by the actor of `stokesforge-session`;
//!   `Tree` publicly exposes read access only.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod analysis_store;
pub mod calc;
pub mod column;
pub mod compose;
pub mod copy;
pub mod datetime;
pub mod error;
pub mod formula;
pub mod id;
pub mod name;
pub mod node;
pub mod number_format;
pub mod path;
pub mod project;
pub mod resolve;
pub mod transpose;
pub mod tree;
pub mod tsv;

pub use calc::{
    compute, family_english, output_unit, wants_output, AuxRef, Calculation, CalculationDraft,
    ComputeError, Computed, ComputedColumn, CorrectionRecord, OutputDraft, OutputName, OutputRef,
    PeakOutputs, ResultValue, SeriesRef, SeriesResult, Staleness, MAX_CALCULATION_SERIES,
};
pub use column::{
    Cell, Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, ImportSource, MetaLine,
    MixedCell, NumericStatus, ValueBlock, ValuesDelta,
};
pub use compose::PlannedSeries;
pub use copy::NodeCopy;
pub use error::{CoreError, CoreResult};
pub use formula::{
    evaluate as evaluate_formula, topological_order, Callee, ColumnFormula, Formula, FormulaError,
    FormulaEvaluation, FormulaPart, REF_TOKEN,
};
pub use id::{IdAllocator, NodeId};
pub use name::{check_name, RESERVED_NAME_CHARS};
pub use node::{Node, NodeKind, WindowMeta};
pub use path::{format_path, parse_path, ObjectPath, PathDepth};
pub use project::{spreadsheet_name, Project};
pub use resolve::{
    bar_width, histogram, resolve_column, resolve_panel, resolve_series, resolve_spec,
    series_label, MissingRole, ResolvedPanel, ResolvedPlot, ResolvedSeries, ResolvedValues,
    SeriesLabel,
};
pub use transpose::{
    content_stamp, transposed_name, TransposeLink, TransposeSourceColumn, TransposeState,
    TransposedRow, TRANSPOSE_MARK, TRANSPOSE_X_NAME,
};
pub use tree::Tree;

//! The demo project: enough to try many features right at opening, without a
//! file (user decision: the demo lives in the code). A module of the
//! **binary**, not of the library: the application does not need it, only
//! `main` opens it.
//!
//! ```text
//! Demo
//! ├── Book1 (Sheet1…4, Spectrum, Kinetics) and Graph1   — unchanged
//! ├── Workbooks : Types, Measurements, Matrix, ReadMe   (workbooks.rs)
//! ├── Analysis  : one workbook per computation family   (analysis.rs)
//! └── Panels, Axes, PlotTypes, Annotations, Groups, Rows   (graphs.rs)
//! ```
//!
//! `noise.rs` provides the pseudo-random noise with a fixed seed: the demo is
//! the same at every launch, which the tests pin.
//!
//! # Constraints that fixed the shape
//!
//! - **A fresh `Project`, before `Session::spawn`** (invariant 1): the
//!   primitives of `stokesforge-core` can then be called, never afterwards.
//!   The computations already done go through `add_calculation` on this same
//!   fresh project.
//! - **Formula columns get no value here** (invariant 11, #193): only the
//!   formula is set; `demo_session` asks for a full recalculation right after
//!   the actor starts, without which these columns would stay empty.
//! - **Window names are unique across the whole project**, not only among
//!   siblings: the `[Name]` path designates them without a folder. This is
//!   why the plot types graph is called `PlotTypes` (the `Types` workbook
//!   already exists).
//! - **Datasets on a logarithmic axis are strictly positive** (invariant 10).
//! - A few hundred points per dataset at most: the autosave (#317) copies the
//!   demo.
//! - Column names stay the automatic letters (A, B, C…); the readable titles
//!   are long names.

mod analysis;
mod assemble;
mod graphs;
mod noise;
mod workbooks;

#[cfg(test)]
mod tests;

pub(crate) use assemble::demo_project;
use assemble::set_x_role;

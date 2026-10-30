//! Mathematical processing of StokesForge data (work package #181).
//!
//! **Pure calculation** crate: baselines, correction by a reference,
//! integrals, fits, decomposition into peaks, deconvolution, smoothing. It depends on
//! no crate of the workspace (like `stokesforge-import`), nor on an interface,
//! nor on Python: everything the menus offer works without an interpreter
//! (decision of 2026-10-08). The vocabulary is that of raw data: plain
//! `f64` where `NaN` means "empty", "text" or "invalid".
//!
//! No interface name here: results carry **stable keys**
//! (`area`, `tau`, `r_squared`…) that `stokesforge-app` turns into labels.

// Two numerical-calculation lints, dismissed here with their reason:
// - `!(x > 0.0)` is intended: it is also true for `NaN`, which `x <= 0.0` would
//   not be; rewriting it with `partial_cmp` would weigh down every guard;
// - the algebra loops index several arrays by the same indices
//   (`a[i][j]`, `x[j]`): an iterator is less readable there than the index.
#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::needless_range_loop)]

pub mod baseline;
pub mod column_values;
pub mod correct;
pub mod deconv;
pub mod error;
pub mod expr;
pub mod fit;
pub mod functions;
pub mod integrate;
pub mod linalg;
pub mod model;
pub mod ops;
pub mod peaks;
pub mod series;
pub mod smooth;
pub mod solver;
pub mod tree;
pub mod voigt;

pub use error::AnalysisError;

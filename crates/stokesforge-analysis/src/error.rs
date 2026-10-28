//! The crate errors: structured, without sentences.
//!
//! The `Display` is in **plain English** (CLI, `pyst`, logs); it is
//! `stokesforge-app` that turns each variant into a translated sentence (repository
//! convention, `errors::<family>::error_message`). Each variant therefore carries
//! data fields, never a composed text.

use crate::expr::ExprError;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnalysisError {
    #[error("not enough points: {found} usable, {needed} needed")]
    NotEnoughPoints { found: usize, needed: usize },
    #[error("X values must be strictly increasing after sorting (duplicates found)")]
    DuplicateX,
    #[error("invalid model expression: {0}")]
    BadExpression(ExprError),
    #[error("model has no free parameter")]
    NoFreeParameter,
    #[error("parameter '{name}' has an empty range (min {min} > max {max})")]
    EmptyBounds { name: String, min: f64, max: f64 },
    #[error("the model gives a non-finite value at the starting parameters")]
    NonFiniteModel,
    #[error("the model gives a non-finite slope at the current parameters")]
    ModelNotEvaluable,
    #[error("the fit does not determine its parameters (singular system)")]
    SingularSystem,
    #[error("the fit did not converge within {iterations} iterations")]
    NotConverged { iterations: usize },
    #[error("the baseline needs at least {needed} anchor point(s), {found} given")]
    NotEnoughAnchors { found: usize, needed: usize },
    #[error("the baseline zones contain {found} point(s), {needed} needed for degree {degree}")]
    ZonesTooSmall {
        found: usize,
        needed: usize,
        degree: u32,
    },
    #[error("two anchor points share the same X value")]
    DuplicateAnchor,
    #[error("the reference column has no usable point")]
    EmptyReference,
    #[error("the instrument response has no usable signal")]
    EmptyResponse,
    #[error("the range is empty or holds no point")]
    EmptyRange,
    #[error("invalid value for '{name}'")]
    InvalidParameter { name: String },
    #[error("a series has a different number of rows than its X column")]
    LengthMismatch,
    #[error("an output column no longer accepts the result")]
    OutputUnwritable,
}

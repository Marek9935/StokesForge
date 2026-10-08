//! Les erreurs du crate : structurées, sans phrase.
//!
//! Le `Display` est en **anglais brut** (CLI, `pyst`, journaux) ; c'est
//! `stokesforge-app` qui met chaque variante en phrase traduite (convention du
//! dépôt, `errors::<famille>::error_message`). Chaque variante porte donc des
//! champs de données, jamais un texte composé.

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
}

//! The demo project: enough to try many features right at opening, without a
//! file (user decision: the demo lives in the code). A module of the
//! **binary**, not of the library: the application does not need it, only
//! `main` opens it.
//!
//! ```text
//! Demo
//! ├── Book1 (Sheet1…4, Spectre, Cinetique) and Graph1   — unchanged
//! ├── Classeurs : Types, Mesures, Matrice, Lisez-moi   (workbooks.rs)
//! ├── Analyse   : one workbook per computation family   (analysis.rs)
//! └── Panneaux, Axes, TypesTrace, Annotations, Groupes, Lignes   (graphs.rs)
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
//!   why the plot types graph is called `TypesTrace` (the `Types` workbook
//!   already exists).
//! - **Datasets on a logarithmic axis are strictly positive** (invariant 10).
//! - A few hundred points per dataset at most: the autosave (#317) copies the
//!   demo.
//! - Column names stay the automatic letters (A, B, C…); the readable titles
//!   are long names.

mod analysis;
mod graphs;
mod noise;
mod workbooks;

#[cfg(test)]
mod tests;

use anyhow::Context as _;
use stokesforge_core::{NodeId, Project};

/// Gives the X role (abscissa) to a column of the demo: the first column of
/// a sheet is the abscissa axis.
fn set_x_role(project: &mut Project, column: NodeId) -> anyhow::Result<()> {
    let mut meta = project
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map(|column| column.meta().clone())
        .unwrap_or_default();
    meta.role = stokesforge_core::ColumnRole::X;
    project
        .set_column_meta(column, meta)
        .context("X role of the demo")?;
    Ok(())
}

/// The demo project (see the module documentation). `Book1` and `Graph1`
/// first, in this order: the example script reads `[Book1]Sheet1!A`.
pub(crate) fn demo_project() -> anyhow::Result<Project> {
    let mut project = Project::new("Demo").context("demo project")?;
    let book1 = workbooks::book1(&mut project)?;
    graphs::graph1(&mut project, &book1)?;
    let measures = workbooks::folder(&mut project)?;
    analysis::folder(&mut project)?;
    graphs::others(&mut project, &book1, &measures)?;
    Ok(project)
}

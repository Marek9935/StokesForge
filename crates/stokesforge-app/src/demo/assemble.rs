//! The assembly of the demo project and the X role of its first columns. Apart
//! from `mod.rs` so that the manual screenshots bench (`app/manual_screenshots.rs`,
//! #365), which belongs to the library and not to the binary, can include the
//! demo without its binary-only tests.

use anyhow::Context as _;
use stokesforge_core::{NodeId, Project};

use super::{analysis, graphs, workbooks};

/// Gives the X role (abscissa) to a column of the demo: the first column of
/// a sheet is the abscissa axis.
pub(super) fn set_x_role(project: &mut Project, column: NodeId) -> anyhow::Result<()> {
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

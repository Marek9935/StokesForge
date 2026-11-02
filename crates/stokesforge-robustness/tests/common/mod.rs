//! Helpers shared by the twin-session oracles `b22` and `b23`.
//!
//! Each test file includes this module (`mod common;`) and uses both helpers.

use std::collections::BTreeMap;

use stokesforge_core::{NodeId, Tree};

/// The tree without its transposed-sheet links. A link records the **version**
/// of every source column it read, and the two live modes legitimately number
/// formula columns differently (the full pass rewrites more), so links are
/// compared by shape ([`link_shapes`]) instead. Bug #430 (a load or
/// a full pass restamps formula columns that did not change, so a link reads
/// stale): when it is fixed, remove `strip_links` and `link_shapes`.
pub fn strip_links(tree: &Tree) -> Tree {
    tree.clone().with_transposes(BTreeMap::new())
}

/// Everything of a link except version numbers: sheet, source, x' column, rows
/// with their content stamp, and the source columns read.
#[allow(clippy::type_complexity)]
pub fn link_shapes(
    tree: &Tree,
) -> Vec<(
    NodeId,
    NodeId,
    NodeId,
    u64,
    Vec<(usize, NodeId, u64)>,
    Vec<NodeId>,
)> {
    tree.transposes()
        .iter()
        .map(|(sheet, link)| {
            (
                *sheet,
                link.source,
                link.x_column,
                link.x_stamp,
                link.rows
                    .iter()
                    .map(|r| (r.row, r.column, r.stamp))
                    .collect(),
                link.inputs.iter().map(|i| i.column).collect(),
            )
        })
        .collect()
}

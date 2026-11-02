//! Helpers shared by the twin-session oracles `b22` and `b23`.
//!
//! Each test file includes this module (`mod common;`) and uses the helpers it needs.

use std::collections::BTreeMap;

use stokesforge_core::{NodeId, Tree};

/// The tree without its transposed-sheet links. A link records the **version**
/// of every source column it read, and the two live modes legitimately number
/// formula columns differently (the full pass rewrites more, and `Full` also
/// restamps after a load, where the targeted side trusts the links, #430), so
/// links are compared by shape ([`link_shapes`]) instead.
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

/// The derived state of every link: (sheet, stale, modified). Compared across a
/// reload of the targeted twin, where the first pass must not change it (#430).
#[allow(dead_code)] // b23 only
pub fn link_states(tree: &Tree) -> Vec<(NodeId, bool, bool)> {
    tree.transposes()
        .iter()
        .map(|(sheet, link)| {
            let state = link.state(tree);
            (*sheet, state.stale, state.modified)
        })
        .collect()
}

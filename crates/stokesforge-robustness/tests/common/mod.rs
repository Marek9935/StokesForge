//! Helpers shared by the twin-session oracles `b22` and `b23`.
//!
//! Each test file includes this module (`mod common;`) and uses the helpers it needs.
//!
//! The twins' trees are compared exactly, transposed-sheet links and column
//! versions included (#439): a pass that finds the same content keeps the
//! version, so the targeted and full twins number alike.

use stokesforge_core::{NodeId, Tree};

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

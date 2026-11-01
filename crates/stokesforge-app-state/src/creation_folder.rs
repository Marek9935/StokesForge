//! Where a new window goes (#425): the folder of the Project Manager selection.
//!
//! One folder selected: inside it. One window selected: in that window's
//! folder. Nothing, or several rows: the project root. A pure question over the
//! tree, shared by every "new window" the user can trigger (File menu, toolbar,
//! shortcut) and by *Paste*, which lands in the same place.

use stokesforge_core::{NodeId, Tree};

/// The folder that receives a new window, given the selected nodes.
pub fn creation_folder(tree: &Tree, selection: &[NodeId]) -> NodeId {
    let root = tree.root();
    let [only] = selection else {
        return root;
    };
    let mut current = *only;
    loop {
        let Some(node) = tree.node(current) else {
            return root;
        };
        if node.kind().is_folder() {
            return current;
        }
        match node.parent() {
            Some(parent) => current = parent,
            None => return root,
        }
    }
}

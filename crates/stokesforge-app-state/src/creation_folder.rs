//! Where a new window goes (#425): the folder of the Project Manager selection.
//!
//! One folder selected: inside it. One window selected: in that window's
//! folder. Nothing, or several rows: the project root. A pure question over the
//! tree, shared by every "new window" the user can trigger (File menu, toolbar,
//! shortcut) and by *Paste*, which lands in the same place.

use stokesforge_core::{NodeId, Tree};

/// The folder that receives a new window, given the selected nodes.
pub fn creation_folder(tree: &Tree, selection: &[NodeId]) -> NodeId {
    creation_folder_by(tree.root(), selection, |id| {
        tree.node(id)
            .map(|node| (node.kind().is_folder(), node.parent()))
    })
}

/// The same rule over any view of the tree: `lookup` tells, for a node, whether
/// it is a folder and which node contains it (`None` at the top). The Project
/// Manager panel, which paints a simplified tree, asks the rule through this
/// door, so there is one rule only.
pub fn creation_folder_by(
    root: NodeId,
    selection: &[NodeId],
    lookup: impl Fn(NodeId) -> Option<(bool, Option<NodeId>)>,
) -> NodeId {
    let [only] = selection else {
        return root;
    };
    let mut current = *only;
    loop {
        let Some((is_folder, parent)) = lookup(current) else {
            return root;
        };
        if is_folder {
            return current;
        }
        match parent {
            Some(parent) => current = parent,
            None => return root,
        }
    }
}

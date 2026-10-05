//! Copie autonome d'un sous-arbre : des valeurs, sans projet ni allocateur.
//! Les buffers de colonnes restent partagés jusqu'à une écriture. Les natures
//! sont clonées sans liste fermée de types copiables ; leurs enfants sont
//! reconstruits par le même chemin d'insertion que les créations ordinaires.

use crate::{CoreResult, Node, NodeId, Tree};

/// Contenu figé à l'instant du geste Copier, indépendant du projet source.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeCopy {
    nodes: Vec<Node>,
}

impl NodeCopy {
    /// Capture un sous-arbre valide, en ordre préfixe.
    pub fn capture(tree: &Tree, root: NodeId) -> CoreResult<Self> {
        let nodes = tree
            .walk(root)?
            .into_iter()
            .map(|id| tree.get(id).cloned())
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(Self { nodes })
    }

    /// Nœuds source, racine en premier ; jamais mutables par l'appelant.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Racine du contenu copié.
    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }
}

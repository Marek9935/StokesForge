//! Copie autonome d'un sous-arbre : des valeurs, sans projet ni allocateur.
//! Les buffers de colonnes restent partagés jusqu'à une écriture. Les natures
//! sont clonées sans liste fermée de types copiables ; leurs enfants sont
//! reconstruits par le même chemin d'insertion que les créations ordinaires.
//! Les graphes capturent uniquement leurs colonnes référencées, buffers partagés.
//! `into_external` active leur insertion dans la destination ; les copies locales
//! conservent leurs références, sans dépendances supplémentaires dans le modèle.

use crate::{CoreResult, Node, NodeId, NodeKind, Tree, WindowMeta};
use std::collections::HashSet;

/// Contenu figé à l'instant du geste Copier, indépendant du projet source.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeCopy {
    nodes: Vec<Node>,
    graph_data: Vec<(NodeId, NodeCopy)>,
    external: bool,
}

impl NodeCopy {
    /// Capture un sous-arbre valide, en ordre préfixe.
    pub fn capture(tree: &Tree, root: NodeId) -> CoreResult<Self> {
        let nodes: Vec<Node> = tree
            .walk(root)?
            .into_iter()
            .map(|id| tree.get(id).cloned())
            .collect::<CoreResult<Vec<_>>>()?;
        let mut graph_data = Vec::new();
        for node in &nodes {
            if let NodeKind::Graph { spec, .. } = node.kind() {
                let mut referenced: HashSet<_> = spec
                    .panels
                    .iter()
                    .flat_map(|p| &p.series)
                    .flat_map(|s| s.referenced_columns())
                    .collect();
                // Une série « ligne » (#166) lit toutes les colonnes x' de la
                // feuille de son ancre, pas seulement l'ancre.
                for series in spec.panels.iter().flat_map(|p| &p.series) {
                    if series.row.is_none() {
                        continue;
                    }
                    let Some(sheet) = tree.parent(series.y).ok().flatten() else {
                        continue;
                    };
                    for column in tree.node(sheet).map(Node::children).unwrap_or_default() {
                        if tree
                            .node(*column)
                            .and_then(Node::as_column)
                            .is_some_and(|c| c.meta().x_prime.is_some())
                        {
                            referenced.insert(*column);
                        }
                    }
                }
                let sheets: Vec<_> = tree
                    .walk_all()
                    .into_iter()
                    .filter(|id| {
                        tree.node(*id).is_some_and(|n| {
                            n.kind().is_worksheet()
                                && n.children().iter().any(|c| {
                                    referenced.contains(c)
                                        && tree.node(*c).is_some_and(|n| n.as_column().is_some())
                                })
                        })
                    })
                    .collect();
                if sheets.is_empty() {
                    continue;
                }
                // Le graphe fournit une identité de transport libre parmi les
                // données ; aucune identité nouvelle n’est allouée à la copie.
                let mut data = vec![Node::new(
                    node.id(),
                    node.name(),
                    NodeKind::Workbook {
                        sheets: sheets.clone(),
                        meta: WindowMeta {
                            hidden: true,
                            ..Default::default()
                        },
                    },
                )?];
                let mut names = HashSet::new();
                for sheet in sheets {
                    let source = tree.get(sheet)?;
                    let columns: Vec<_> = source
                        .children()
                        .iter()
                        .copied()
                        .filter(|id| {
                            referenced.contains(id)
                                && tree.node(*id).is_some_and(|n| n.as_column().is_some())
                        })
                        .collect();
                    let mut name = source.name().to_string();
                    let mut number = 1;
                    while names.contains(&name) {
                        // Rester dans la limite de nom du modèle même avec
                        // plusieurs feuilles homonymes de classeurs différents.
                        name = format!(
                            "{} {number}",
                            source.name().chars().take(230).collect::<String>()
                        );
                        number += 1;
                    }
                    names.insert(name.clone());
                    data.push(Node::with_parent(
                        sheet,
                        &name,
                        Some(node.id()),
                        NodeKind::Worksheet {
                            columns: columns.clone(),
                        },
                    )?);
                    for column in columns {
                        let mut value = tree.get(column)?.clone();
                        if let NodeKind::Column(column) = value.kind_mut() {
                            // La feuille embarquée omet les colonnes non tracées :
                            // une formule y lirait des colonnes absentes de la copie
                            // (`#REF`, cellules vides) au lieu de ses résultats.
                            // Conserver les résultats figés uniquement.
                            let mut meta = column.meta().clone();
                            meta.formula = None;
                            column.set_meta(meta);
                        }
                        data.push(value);
                    }
                }
                graph_data.push((
                    node.id(),
                    Self {
                        nodes: data,
                        graph_data: Vec::new(),
                        external: false,
                    },
                ));
            }
        }
        Ok(Self {
            nodes,
            graph_data,
            external: false,
        })
    }

    /// Dépendances figées de chaque graphe, pour le transport inter-projets.
    pub fn graph_data(&self) -> &[(NodeId, NodeCopy)] {
        &self.graph_data
    }

    /// Associe les dépendances validées par le lecteur du presse-papiers.
    pub fn with_graph_data(mut self, data: Vec<(NodeId, NodeCopy)>) -> CoreResult<Self> {
        let mut seen = HashSet::new();
        for (graph, copy) in &data {
            if !seen.insert(*graph)
                || !self
                    .nodes
                    .iter()
                    .any(|n| n.id() == *graph && matches!(n.kind(), NodeKind::Graph { .. }))
                || !matches!(copy.root().kind(), NodeKind::Workbook { .. })
            {
                return Err(crate::CoreError::Structure(
                    "invalid graph clipboard data".into(),
                ));
            }
        }
        self.graph_data = data;
        Ok(self)
    }

    /// Active l’insertion des données et le remplacement de toutes les références.
    pub fn into_external(mut self) -> Self {
        self.external = true;
        self
    }

    pub(crate) fn is_external(&self) -> bool {
        self.external
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

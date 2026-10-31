//! Self-contained copy of a subtree: values, with no project or allocator.
//! Column buffers stay shared until a write. Kinds are cloned without a
//! closed list of copyable types; their children are rebuilt through the
//! same insertion path as ordinary creations.
//! Graphs capture only their referenced columns, with shared buffers.
//! `into_external` enables their insertion into the destination. A graph reads
//! the copy for what is inside the copied subtree and the original otherwise
//! (#391); the columns inside the subtree are never embedded.
//! Calculations and transposed sheets whose ends are inside the subtree travel
//! with it.

use crate::{Calculation, CoreResult, Node, NodeId, NodeKind, TransposeLink, Tree, WindowMeta};
use std::collections::HashSet;

/// Content frozen at the moment of the Copy gesture, independent of the source project.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeCopy {
    nodes: Vec<Node>,
    graph_data: Vec<(NodeId, NodeCopy)>,
    /// Calculations whose output columns are inside the subtree.
    calculations: Vec<(NodeId, Calculation)>,
    /// Transposed sheets of the subtree whose source is inside it too.
    transposes: Vec<(NodeId, TransposeLink)>,
    external: bool,
}

impl NodeCopy {
    /// Captures a valid subtree, in prefix order.
    pub fn capture(tree: &Tree, root: NodeId) -> CoreResult<Self> {
        let mut copies = Self::capture_all(tree, &[root])?;
        copies
            .pop()
            .ok_or_else(|| crate::CoreError::Structure("empty copy".into()))
    }

    /// Captures the roots of **one gesture**, one `NodeCopy` per root. What is
    /// inside any of the roots counts as "inside" for all of them (#391, S1):
    /// a graph never embeds a column of another root, and a calculation or a
    /// transposed sheet whose ends are in different roots travels (with the
    /// root that holds its first output, or its sheet). The roots must not be
    /// nested.
    pub fn capture_all(tree: &Tree, roots: &[NodeId]) -> CoreResult<Vec<Self>> {
        let mut inside = HashSet::new();
        for root in roots {
            inside.extend(tree.walk(*root)?);
        }
        let mut copies = roots
            .iter()
            .map(|root| Self::capture_root(tree, *root, &inside))
            .collect::<CoreResult<Vec<_>>>()?;
        let owner = |column: &NodeId, copies: &[Self]| {
            copies
                .iter()
                .position(|copy| copy.nodes.iter().any(|n| n.id() == *column))
        };
        for (id, calc) in tree.calculations() {
            let existing: Vec<NodeId> = calc
                .outputs
                .iter()
                .filter_map(|o| o.column)
                // Only the outputs that still exist count: an output deleted
                // from the sheet does not tie the calculation elsewhere.
                .filter(|c| tree.node(*c).is_some())
                .collect();
            if existing.is_empty() || !existing.iter().all(|c| inside.contains(c)) {
                continue;
            }
            if let Some(at) = owner(&existing[0], &copies) {
                copies[at].calculations.push((*id, calc.as_ref().clone()));
            }
        }
        for (sheet, link) in tree.transposes() {
            if !inside.contains(sheet) || !inside.contains(&link.source) {
                continue;
            }
            if let Some(at) = owner(sheet, &copies) {
                copies[at].transposes.push((*sheet, link.as_ref().clone()));
            }
        }
        Ok(copies)
    }

    fn capture_root(tree: &Tree, root: NodeId, inside: &HashSet<NodeId>) -> CoreResult<Self> {
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
                // A "row" series (#166) reads all the x' columns of the
                // sheet of its anchor, not only the anchor.
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
                // What is inside the copy reads the copy: no hidden workbook
                // for it, even in an external paste.
                referenced.retain(|column| !inside.contains(column));
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
                // The graph supplies a free transport identity among the
                // data; no new identity is allocated for the copy.
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
                        // Stay within the model's name limit even with
                        // several same-named sheets from different workbooks.
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
                            // The embedded sheet omits the unplotted columns:
                            // a formula there would read columns absent from the copy
                            // (`#REF`, empty cells) instead of its results.
                            // Keep the frozen results only.
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
                        calculations: Vec::new(),
                        transposes: Vec::new(),
                        external: false,
                    },
                ));
            }
        }
        Ok(Self {
            nodes,
            graph_data,
            calculations: Vec::new(),
            transposes: Vec::new(),
            external: false,
        })
    }

    /// Calculations carried by the copy, with their identifier in the source.
    pub fn calculations(&self) -> &[(NodeId, Calculation)] {
        &self.calculations
    }

    /// Transposed sheets carried by the copy, by transposed sheet.
    pub fn transposes(&self) -> &[(NodeId, TransposeLink)] {
        &self.transposes
    }

    /// Frozen dependencies of each graph, for inter-project transport.
    pub fn graph_data(&self) -> &[(NodeId, NodeCopy)] {
        &self.graph_data
    }

    /// Attaches the dependencies validated by the clipboard reader.
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

    /// Enables insertion of the data and replacement of all references.
    pub fn into_external(mut self) -> Self {
        self.external = true;
        self
    }

    pub(crate) fn is_external(&self) -> bool {
        self.external
    }

    /// Source nodes, root first; never mutable by the caller.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Root of the copied content.
    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }
}

/// One root of a paste: the destination container, the captured copy and the
/// name it takes there. `Project::insert_copies` inserts the items of one
/// gesture with a single map of new identifiers.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyInsertion {
    /// Destination container.
    pub parent: NodeId,
    /// Values captured at copy time.
    pub copy: NodeCopy,
    /// Name chosen in the destination container.
    pub name: String,
}

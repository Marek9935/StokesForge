//! Nodes of the project tree: kind (`NodeKind`), identity, name, parent.
//!
//! A `Node` is immutable seen from outside the crate: it lives behind an
//! `Arc` in `Tree`, and only `Project` mutates it, going through `Tree` and
//! `Arc::make_mut`. The child lists live *inside* the node's kind, which
//! guarantees at compile time that a column, a graph, a matrix or
//! a note has none.

use stokesforge_plot::{PlotSpec, RawVariant};

use crate::column::Column;
use crate::error::CoreResult;
use crate::id::NodeId;
use crate::name::check_name;

/// Metadata common to windows (workbook, graph, matrix, note).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowMeta {
    /// Label shown in the window banner (empty = show the name).
    pub label: String,
    /// Free comment.
    pub comment: String,
    /// Window absent from the manager; used for embedded workbooks.
    pub hidden: bool,
}

/// Kind of a node and its own content. Child lists are ordered:
/// that order is what is displayed and persisted.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Storage folder: contains folders and windows.
    Folder {
        /// Ordered children (folders, workbooks, graphs, matrices, notes).
        children: Vec<NodeId>,
    },
    /// Workbook: a stack of ordered sheets.
    Workbook {
        /// Sheets of the workbook, in tab order.
        sheets: Vec<NodeId>,
        /// Window metadata.
        meta: WindowMeta,
    },
    /// Worksheet: an ordered sequence of columns.
    Worksheet {
        /// Columns of the sheet, from left to right.
        columns: Vec<NodeId>,
    },
    /// Data column (the name lives in the node, not in the `Column`).
    Column(Column),
    /// Graph: carries its plot content (phase R3, `docs/archive/newarchitecture/roadmap-rendu.md`).
    Graph {
        /// Window metadata.
        meta: WindowMeta,
        /// Description of the graph, independent of any renderer. The
        /// series reference their columns by [`NodeId`] (`stokesforge-plot`
        /// §11.1 of `architecture-choisie.md`): renaming or moving a
        /// column leaves the graph intact, deleting it produces a
        /// broken reference detectable at resolution
        /// (`crate::resolve::resolve_spec`), never a silently
        /// empty series. Replaceable **as a whole** after creation by
        /// [`Project::set_plot_spec`] — never by sub-field (no more
        /// than a cell write changes a single value of a block,
        /// see the doctrine of `Project::set_values`): fine-grained mutation
        /// (series, axis, legend) stays out of scope.
        spec: PlotSpec,
    },
    /// Matrix (content added in a later slice).
    Matrix {
        /// Window metadata.
        meta: WindowMeta,
    },
    /// Note: a free-text window.
    Note {
        /// Window metadata.
        meta: WindowMeta,
        /// Text of the note.
        text: String,
    },
    /// Node kind from a more recent version of StokesForge, unknown to this
    /// reader (C5, level 2, `guide-de-correction-robustesse.md` §F3): this
    /// node is not shown as a folder, a workbook or one of the
    /// other known kinds, but its original content (variant name
    /// and raw bytes) is kept so that the next
    /// save rewrites the file identically, rather than losing the
    /// node — and the rest of the file with it, before this fix.
    Unknown(RawVariant),
}

impl NodeKind {
    /// True for `Workbook`, `Graph`, `Matrix`, `Note` — the nodes addressable
    /// by `[Name]`, whose name is unique in their parent folder (#105).
    ///
    /// Also true for `Unknown`: in the on-disk schema (`schema.rs`),
    /// a node kind is a flat child of `Folder` just like
    /// `Workbook`/`Graph`/`Matrix`/`Note` — never `Worksheet`/`Column` —
    /// so a future kind necessarily behaves like a window, not
    /// like a folder or a sheet, from the point of view of containment
    /// (`accepts_child`) and addressing (`Project::path_of`).
    pub fn is_window(&self) -> bool {
        matches!(
            self,
            NodeKind::Workbook { .. }
                | NodeKind::Graph { .. }
                | NodeKind::Matrix { .. }
                | NodeKind::Note { .. }
                | NodeKind::Unknown(_)
        )
    }

    /// True for a folder.
    pub fn is_folder(&self) -> bool {
        matches!(self, NodeKind::Folder { .. })
    }

    /// True for a worksheet.
    pub fn is_worksheet(&self) -> bool {
        matches!(self, NodeKind::Worksheet { .. })
    }

    /// True for a column.
    pub fn is_column(&self) -> bool {
        matches!(self, NodeKind::Column(_))
    }

    /// Ordered children. Empty for the leaves of the tree: `Column`, `Graph`,
    /// `Matrix`, `Note`.
    pub fn children(&self) -> &[NodeId] {
        match self {
            NodeKind::Folder { children } => children,
            NodeKind::Workbook { sheets, .. } => sheets,
            NodeKind::Worksheet { columns } => columns,
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => &[],
        }
    }

    /// Readable name of the kind, for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Folder { .. } => "folder",
            NodeKind::Workbook { .. } => "workbook",
            NodeKind::Worksheet { .. } => "worksheet",
            NodeKind::Column(_) => "column",
            NodeKind::Graph { .. } => "graph",
            NodeKind::Matrix { .. } => "matrix",
            NodeKind::Note { .. } => "note",
            // Degraded display rather than a panic or a missing rendering
            // (C5, level 2): the user sees "unrecognized kind"
            // instead of a type text invented for content this
            // reader does not understand.
            NodeKind::Unknown(_) => "unrecognized kind",
        }
    }

    /// True if a node of this kind can contain `child`:
    /// `Folder ⊃ {Folder, Workbook, Graph, Matrix, Note}`; `Workbook ⊃ {Worksheet}`;
    /// `Worksheet ⊃ {Column}`; nothing else.
    ///
    /// **Plus [`NodeKind::Unknown`], which the three containers accept.**
    /// An unrecognized kind always comes from a file written by a
    /// more recent version (C5, level 2); refusing it here does not repair it,
    /// it makes the tree validation of `stokesforge_persistence::load` fail,
    /// so it loses **the whole file** instead of just that node — exactly what
    /// the `RawVariant` capture exists to avoid (invariant 10).
    /// `Folder` already accepted it, indirectly (`is_window` counts
    /// `Unknown`); `Workbook` and `Worksheet` did not, and that is
    /// how an unknown column type — a mixed column read by an
    /// earlier reader, for example — took the whole file down
    /// (`stokesforge-robustesse`, `b19_un_type_de_colonne_inconnu_ne_perd_que_cette_colonne`).
    ///
    /// An unknown node stored this way is never mistaken for a column:
    /// everything that reads a sheet goes through `Node::as_column`, which returns
    /// `None` for it (`Project::row_targets`, `resolve`, the grid of
    /// `stokesforge-app`). It takes up a place in the sheet, and that is all.
    pub fn accepts_child(&self, child: &NodeKind) -> bool {
        if child.is_unknown() {
            return self.allowed_children().is_some();
        }
        match self {
            NodeKind::Folder { .. } => child.is_folder() || child.is_window(),
            NodeKind::Workbook { .. } => child.is_worksheet(),
            NodeKind::Worksheet { .. } => child.is_column(),
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => false,
        }
    }

    /// True for a kind that no variant of this reader names, read
    /// from a file written by a more recent version (C5, level 2).
    pub fn is_unknown(&self) -> bool {
        matches!(self, NodeKind::Unknown(_))
    }

    /// Name of the type preceded by its indefinite article, for messages:
    /// "a column", "a workbook"…
    pub fn with_article(&self) -> &'static str {
        match self {
            NodeKind::Folder { .. } => "a folder",
            NodeKind::Workbook { .. } => "a workbook",
            NodeKind::Worksheet { .. } => "a worksheet",
            NodeKind::Column(_) => "a column",
            NodeKind::Graph { .. } => "a graph",
            NodeKind::Matrix { .. } => "a matrix",
            NodeKind::Note { .. } => "a note",
            NodeKind::Unknown(_) => "an unrecognized kind",
        }
    }

    /// Description of what this kind accepts, or `None` if
    /// it accepts no child.
    pub(crate) fn allowed_children(&self) -> Option<&'static str> {
        match self {
            NodeKind::Folder { .. } => Some("folders, workbooks, graphs, matrices and notes"),
            NodeKind::Workbook { .. } => Some("worksheets"),
            NodeKind::Worksheet { .. } => Some("columns"),
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => None,
        }
    }

    /// Mutable child list, or `None` for a leaf of the tree.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        match self {
            NodeKind::Folder { children } => Some(children),
            NodeKind::Workbook { sheets, .. } => Some(sheets),
            NodeKind::Worksheet { columns } => Some(columns),
            NodeKind::Column(_)
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => None,
        }
    }

    /// Mutable window metadata, or `None` if this is not a window.
    ///
    /// `Unknown` returns `None` although `is_window()` counts it as a
    /// window: its content is opaque, there is no `WindowMeta` to extract
    /// to mutate — `Project::set_window_meta` therefore fails
    /// cleanly (`Structure`) on a node of an unknown kind.
    pub(crate) fn window_meta_mut(&mut self) -> Option<&mut WindowMeta> {
        match self {
            NodeKind::Workbook { meta, .. }
            | NodeKind::Graph { meta, .. }
            | NodeKind::Matrix { meta }
            | NodeKind::Note { meta, .. } => Some(meta),
            NodeKind::Folder { .. }
            | NodeKind::Worksheet { .. }
            | NodeKind::Column(_)
            | NodeKind::Unknown(_) => None,
        }
    }

    /// Mutable plot spec, or `None` if this is not a graph. Used
    /// by [`Project::set_plot_spec`] to replace the `PlotSpec` as a whole.
    pub(crate) fn plot_spec_mut(&mut self) -> Option<&mut PlotSpec> {
        match self {
            NodeKind::Graph { spec, .. } => Some(spec),
            NodeKind::Folder { .. }
            | NodeKind::Workbook { .. }
            | NodeKind::Worksheet { .. }
            | NodeKind::Column(_)
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => None,
        }
    }
}

/// Error message explaining why `parent` cannot contain `child`.
pub(crate) fn containment_message(parent: &NodeKind, child: &NodeKind) -> String {
    match parent.allowed_children() {
        Some(allowed) => format!(
            "{} only contains {}, not {}",
            parent.with_article(),
            allowed,
            child.with_article()
        ),
        None => format!(
            "{} cannot contain any child, not {}",
            parent.with_article(),
            child.with_article()
        ),
    }
}

/// A node of the tree. Immutable from outside: it lives behind an `Arc` in
/// `Tree`, and only `Project` mutates it.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    id: NodeId,
    name: String,
    parent: Option<NodeId>,
    kind: NodeKind,
}

impl Node {
    /// Builds a detached node (`parent = None`). `Err(InvalidName)` if the name
    /// is invalid. Public constructor: it is also what persistence
    /// uses to rebuild a tree with imposed identifiers.
    pub fn new(id: NodeId, name: &str, kind: NodeKind) -> CoreResult<Self> {
        Self::with_parent(id, name, None, kind)
    }

    /// Like `new`, but with an already known parent (rebuilding from a file).
    pub fn with_parent(
        id: NodeId,
        name: &str,
        parent: Option<NodeId>,
        kind: NodeKind,
    ) -> CoreResult<Self> {
        check_name(name)?;
        Ok(Self {
            id,
            name: name.to_string(),
            parent,
            kind,
        })
    }

    /// Builds a node without validating the name. Reserved for the crate's
    /// literal names (default root), to offer an infallible construction
    /// path with no `unwrap`.
    pub(crate) fn new_unchecked(id: NodeId, name: &str, kind: NodeKind) -> Self {
        Self {
            id,
            name: name.to_string(),
            parent: None,
            kind,
        }
    }

    /// Identifier of the node, stable over its whole life.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Short name of the node (the one that appears in paths).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Parent of the node, `None` for the root (or a still detached node).
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// Kind of the node and its own content.
    pub fn kind(&self) -> &NodeKind {
        &self.kind
    }

    /// Shortcut: `Some(&Column)` if the node is a column.
    pub fn as_column(&self) -> Option<&Column> {
        match &self.kind {
            NodeKind::Column(column) => Some(column),
            _ => None,
        }
    }

    /// Ordered children (empty for the leaves of the tree).
    pub fn children(&self) -> &[NodeId] {
        self.kind.children()
    }

    /// Replaces the name. Returns the previous one. Validation is done by `Tree`.
    pub(crate) fn set_name(&mut self, name: String) -> String {
        std::mem::replace(&mut self.name, name)
    }

    /// Replaces the parent. Returns the previous one.
    pub(crate) fn set_parent(&mut self, parent: Option<NodeId>) -> Option<NodeId> {
        std::mem::replace(&mut self.parent, parent)
    }

    /// Mutable kind.
    pub(crate) fn kind_mut(&mut self) -> &mut NodeKind {
        &mut self.kind
    }

    /// Mutable child list, or `None` for a leaf of the tree.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        self.kind.children_mut()
    }

    /// Mutable column, or `None` if the node is not a column.
    pub(crate) fn column_mut(&mut self) -> Option<&mut Column> {
        match &mut self.kind {
            NodeKind::Column(column) => Some(column),
            _ => None,
        }
    }

    /// Mutable note text, or `None` if the node is not a note.
    pub(crate) fn note_text_mut(&mut self) -> Option<&mut String> {
        match &mut self.kind {
            NodeKind::Note { text, .. } => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::ColumnType;
    use crate::error::CoreError;

    fn folder() -> NodeKind {
        NodeKind::Folder {
            children: Vec::new(),
        }
    }

    fn workbook() -> NodeKind {
        NodeKind::Workbook {
            sheets: Vec::new(),
            meta: WindowMeta::default(),
        }
    }

    fn worksheet() -> NodeKind {
        NodeKind::Worksheet {
            columns: Vec::new(),
        }
    }

    fn column() -> NodeKind {
        NodeKind::Column(Column::new(ColumnType::Numeric))
    }

    fn graph() -> NodeKind {
        NodeKind::Graph {
            meta: WindowMeta::default(),
            spec: PlotSpec::default(),
        }
    }

    fn matrix() -> NodeKind {
        NodeKind::Matrix {
            meta: WindowMeta::default(),
        }
    }

    fn note() -> NodeKind {
        NodeKind::Note {
            meta: WindowMeta::default(),
            text: String::new(),
        }
    }

    #[test]
    fn windows_are_workbook_graph_matrix_note() {
        assert!(workbook().is_window());
        assert!(graph().is_window());
        assert!(matrix().is_window());
        assert!(note().is_window());
        assert!(!folder().is_window());
        assert!(!worksheet().is_window());
        assert!(!column().is_window());
    }

    #[test]
    fn kind_predicates_are_exclusive() {
        assert!(folder().is_folder());
        assert!(worksheet().is_worksheet());
        assert!(column().is_column());
        assert!(!folder().is_column());
        assert!(!column().is_folder());
    }

    #[test]
    fn leaf_kinds_have_no_children() {
        for kind in [column(), graph(), matrix(), note()] {
            assert!(
                kind.children().is_empty(),
                "{} without children",
                kind.type_name()
            );
        }
    }

    #[test]
    fn containers_expose_their_children() {
        let ids = vec![NodeId::from_raw(2), NodeId::from_raw(3)];
        assert_eq!(
            NodeKind::Folder {
                children: ids.clone()
            }
            .children(),
            &ids[..]
        );
        assert_eq!(
            NodeKind::Workbook {
                sheets: ids.clone(),
                meta: WindowMeta::default()
            }
            .children(),
            &ids[..]
        );
        assert_eq!(
            NodeKind::Worksheet {
                columns: ids.clone()
            }
            .children(),
            &ids[..]
        );
    }

    #[test]
    fn accepts_child_follows_the_containment_rules() {
        assert!(folder().accepts_child(&folder()));
        assert!(folder().accepts_child(&workbook()));
        assert!(folder().accepts_child(&graph()));
        assert!(folder().accepts_child(&matrix()));
        assert!(folder().accepts_child(&note()));
        assert!(!folder().accepts_child(&worksheet()));
        assert!(!folder().accepts_child(&column()));

        assert!(workbook().accepts_child(&worksheet()));
        assert!(!workbook().accepts_child(&column()));
        assert!(!workbook().accepts_child(&workbook()));

        assert!(worksheet().accepts_child(&column()));
        assert!(!worksheet().accepts_child(&worksheet()));

        for leaf in [column(), graph(), matrix(), note()] {
            assert!(!leaf.accepts_child(&column()));
            assert!(!leaf.accepts_child(&folder()));
        }
    }

    /// An unrecognized kind (C5, level 2) is accepted by the **three**
    /// containers, and by them alone: refusing it would lose the whole
    /// file instead of just that node (see the doc of `accepts_child`).
    #[test]
    fn every_container_accepts_an_unknown_nature() {
        let unknown = NodeKind::Unknown(stokesforge_plot::RawVariant {
            name: "Image".to_string(),
            payload: Vec::new(),
        });
        assert!(folder().accepts_child(&unknown));
        assert!(workbook().accepts_child(&unknown));
        assert!(worksheet().accepts_child(&unknown));

        for leaf in [column(), graph(), matrix(), note(), unknown.clone()] {
            assert!(
                !leaf.accepts_child(&unknown),
                "a leaf of the tree accepts no child, unknown included"
            );
        }
    }

    #[test]
    fn type_names_are_english() {
        assert_eq!(folder().type_name(), "folder");
        assert_eq!(workbook().type_name(), "workbook");
        assert_eq!(worksheet().type_name(), "worksheet");
        assert_eq!(column().type_name(), "column");
        assert_eq!(graph().type_name(), "graph");
        assert_eq!(matrix().type_name(), "matrix");
        assert_eq!(note().type_name(), "note");
    }

    #[test]
    fn containment_message_names_both_natures() {
        let message = containment_message(&workbook(), &column());
        assert_eq!(message, "a workbook only contains worksheets, not a column");
        let message = containment_message(&note(), &column());
        assert!(message.contains("cannot contain any child"), "{message}");
    }

    #[test]
    fn new_rejects_invalid_names() {
        let error = Node::new(NodeId::from_raw(1), "A!B", folder()).unwrap_err();
        assert!(matches!(error, CoreError::InvalidName(_)), "{error:?}");
    }

    #[test]
    fn new_builds_a_detached_node() {
        let node = Node::new(NodeId::from_raw(7), "Manips", folder()).unwrap();
        assert_eq!(node.id(), NodeId::from_raw(7));
        assert_eq!(node.name(), "Manips");
        assert_eq!(node.parent(), None);
        assert!(node.children().is_empty());
        assert!(node.as_column().is_none());
    }

    #[test]
    fn with_parent_keeps_the_given_parent() {
        let parent = NodeId::from_raw(1);
        let node = Node::with_parent(NodeId::from_raw(2), "A", Some(parent), column()).unwrap();
        assert_eq!(node.parent(), Some(parent));
        assert!(node.as_column().is_some());
    }

    #[test]
    fn mutators_return_the_previous_value() {
        let mut node = Node::new(NodeId::from_raw(1), "A", folder()).unwrap();
        assert_eq!(node.set_name("B".to_string()), "A");
        assert_eq!(node.name(), "B");
        assert_eq!(node.set_parent(Some(NodeId::from_raw(9))), None);
        assert_eq!(node.parent(), Some(NodeId::from_raw(9)));
        node.children_mut().unwrap().push(NodeId::from_raw(3));
        assert_eq!(node.children(), &[NodeId::from_raw(3)]);
    }

    #[test]
    fn note_text_and_window_meta_are_reachable_only_for_the_right_kinds() {
        let mut a_note = Node::new(NodeId::from_raw(1), "Note1", note()).unwrap();
        assert!(a_note.note_text_mut().is_some());
        assert!(a_note.kind_mut().window_meta_mut().is_some());

        let mut a_folder = Node::new(NodeId::from_raw(2), "F", folder()).unwrap();
        assert!(a_folder.note_text_mut().is_none());
        assert!(a_folder.kind_mut().window_meta_mut().is_none());
        assert!(a_folder.column_mut().is_none());
    }
}

//! Persistent project tree: a flat map `NodeId → Arc<Node>` and a root.
//!
//! Mutating a tree means doing `Arc::make_mut` on it (which clones the map —
//! 16-byte pairs, not the nodes) then `Arc::make_mut` on the touched
//! nodes only. A client holding an old `Arc<Tree>` therefore sees a frozen
//! snapshot, and all untouched nodes stay physically
//! shared between the old and the new tree. This is the mechanism of the
//! "structure snapshot" undo.
//!
//! Mutations are `pub(crate)`: `Project` is the crate's only public mutation
//! facade. Each one validates **everything** before writing anything,
//! so that a failure leaves the tree unchanged.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use stokesforge_plot::SavedColors;

use crate::calc::{Calculation, CorrectionRecord};
use crate::column::Column;
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::name::{check_name, same_name_ignoring_case};
use crate::node::{containment_message, Node, NodeKind};
use crate::path::{format_path, parse_path, ObjectPath};
use crate::transpose::TransposeLink;

/// Readable form of a sequence of folders for error messages, `/` for
/// the root. `pub(crate)` because `Project::resolve` relied on it before
/// delegating, and its unit test still lives in `project.rs`.
pub(crate) fn display_folders(folders: &[String]) -> String {
    if folders.is_empty() {
        "/".to_string()
    } else {
        folders.join("/")
    }
}

/// Project tree: flat map `NodeId → Arc<Node>` and root. Shared as
/// `Arc<Tree>`; every mutation goes through `Project`, which does `Arc::make_mut`
/// on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    nodes: HashMap<NodeId, Arc<Node>>,
    root: NodeId,
    /// Colors saved in the project, by use — carried by the tree
    /// to travel with it: a session snapshot, a structural undo
    /// and persistence see them without a separate path.
    saved_colors: SavedColors,
    /// Analysis calculations (#181) — see [`crate::calc`]. Carried by the tree to
    /// travel with it, like the saved colors.
    calculations: BTreeMap<NodeId, Arc<Calculation>>,
    correction_traces: BTreeMap<NodeId, Arc<CorrectionRecord>>,
    /// Links of the transposed sheets (#253), by transposed sheet — see
    /// [`crate::transpose`]. Carried by the tree like calculations.
    transposes: BTreeMap<NodeId, Arc<TransposeLink>>,
}

impl Tree {
    /// Tree reduced to a root folder. `Err(Structure)` if `root` is not a
    /// folder or already has a parent.
    pub(crate) fn new(root: Node) -> CoreResult<Self> {
        if !root.kind().is_folder() {
            return Err(CoreError::Structure(format!(
                "the root must be a folder, not {}",
                root.kind().with_article()
            )));
        }
        if root.parent().is_some() {
            return Err(CoreError::Structure(
                "the root cannot have a parent".to_string(),
            ));
        }
        Ok(Self::with_root(root))
    }

    /// Tree reduced to `root`, without validation. Reserved for roots built
    /// by the crate (infallible construction path, no `unwrap`).
    pub(crate) fn with_root(root: Node) -> Self {
        let id = root.id();
        let mut nodes = HashMap::new();
        nodes.insert(id, Arc::new(root));
        Self {
            nodes,
            root: id,
            saved_colors: SavedColors::default(),
            calculations: BTreeMap::new(),
            correction_traces: BTreeMap::new(),
            transposes: BTreeMap::new(),
        }
    }

    /// The linked transposed sheets, by transposed sheet (#253).
    pub fn transposes(&self) -> &BTreeMap<NodeId, Arc<TransposeLink>> {
        &self.transposes
    }

    /// The link of the transposed sheet `sheet`, if it is one.
    pub fn transpose_link(&self, sheet: NodeId) -> Option<&TransposeLink> {
        self.transposes.get(&sheet).map(Arc::as_ref)
    }

    /// The transposed sheet linked to the sheet `source`, with its link.
    pub fn transposed_from(&self, source: NodeId) -> Option<(NodeId, &TransposeLink)> {
        self.transposes
            .iter()
            .find(|(_, link)| link.source == source)
            .map(|(sheet, link)| (*sheet, link.as_ref()))
    }

    /// The same tree, with these links — for persistence.
    pub fn with_transposes(mut self, transposes: BTreeMap<NodeId, Arc<TransposeLink>>) -> Self {
        self.transposes = transposes;
        self
    }

    pub(crate) fn transposes_mut(&mut self) -> &mut BTreeMap<NodeId, Arc<TransposeLink>> {
        &mut self.transposes
    }

    /// The final corrections of the source data, without a calculation link.
    pub fn correction_traces(&self) -> &BTreeMap<NodeId, Arc<CorrectionRecord>> {
        &self.correction_traces
    }

    /// Rebuilds the traces read from the file, without reactivating a calculation.
    pub fn with_correction_traces(
        mut self,
        traces: BTreeMap<NodeId, Arc<CorrectionRecord>>,
    ) -> Self {
        self.correction_traces = traces;
        self
    }

    pub(crate) fn correction_traces_mut(&mut self) -> &mut BTreeMap<NodeId, Arc<CorrectionRecord>> {
        &mut self.correction_traces
    }

    /// The colors saved in the project.
    pub fn saved_colors(&self) -> &SavedColors {
        &self.saved_colors
    }

    /// The same tree, with these saved colors (bounded to
    /// [`stokesforge_plot::MAX_SAVED_COLORS`] per use) — for persistence,
    /// which rebuilds the tree then puts back what the file carried.
    pub fn with_saved_colors(mut self, colors: SavedColors) -> Self {
        self.saved_colors = colors.bounded();
        self
    }

    pub(crate) fn saved_colors_mut(&mut self) -> &mut SavedColors {
        &mut self.saved_colors
    }

    /// The analysis calculations, by identifier.
    pub fn calculations(&self) -> &BTreeMap<NodeId, Arc<Calculation>> {
        &self.calculations
    }

    /// The calculation `id`, if it exists.
    pub fn calculation(&self, id: NodeId) -> Option<&Arc<Calculation>> {
        self.calculations.get(&id)
    }

    /// The calculation that produced `column`, and the rank of the matching output
    /// in `Calculation::outputs`.
    pub fn calculation_of_column(&self, column: NodeId) -> Option<(NodeId, usize)> {
        self.calculations.iter().find_map(|(id, calc)| {
            calc.outputs
                .iter()
                .position(|o| o.column == Some(column))
                .map(|rank| (*id, rank))
        })
    }

    /// The same tree, with these calculations — for persistence, which rebuilds
    /// the tree then puts back what the file carried.
    pub fn with_calculations(mut self, calculations: BTreeMap<NodeId, Arc<Calculation>>) -> Self {
        self.calculations = calculations;
        self
    }

    pub(crate) fn calculations_mut(&mut self) -> &mut BTreeMap<NodeId, Arc<Calculation>> {
        &mut self.calculations
    }

    /// Rebuilds a tree from a list of nodes (persistence). Validates
    /// everything: identifier uniqueness; root present, a folder, without a parent;
    /// cross-consistency parent ↔ children; accepted kinds; name
    /// uniqueness; reachability from the root. `Err(Structure(..))` names the
    /// faulty node.
    pub fn from_nodes(root: NodeId, nodes: Vec<Node>) -> CoreResult<Self> {
        let mut map: HashMap<NodeId, Arc<Node>> = HashMap::with_capacity(nodes.len());
        for node in nodes {
            let id = node.id();
            if map.insert(id, Arc::new(node)).is_some() {
                return Err(CoreError::Structure(format!(
                    "duplicate identifier in the node list: {id}"
                )));
            }
        }
        let tree = Self {
            nodes: map,
            root,
            saved_colors: SavedColors::default(),
            calculations: BTreeMap::new(),
            correction_traces: BTreeMap::new(),
            transposes: BTreeMap::new(),
        };
        tree.validate_loaded()?;
        Ok(tree)
    }

    /// Checks all the invariants of a freshly loaded tree.
    fn validate_loaded(&self) -> CoreResult<()> {
        let root_node = self.nodes.get(&self.root).ok_or_else(|| {
            CoreError::Structure(format!("root missing from the node list: {}", self.root))
        })?;
        if !root_node.kind().is_folder() {
            return Err(CoreError::Structure(format!(
                "the root {} must be a folder, not {}",
                self.root,
                root_node.kind().with_article()
            )));
        }
        if let Some(parent) = root_node.parent() {
            return Err(CoreError::Structure(format!(
                "the root {} cannot have a parent ({parent})",
                self.root
            )));
        }

        // One pass: occurrences of each (parent, child) in the child
        // lists — avoids a `filter().count()` per sibling (quadratic on
        // a folder with many children, #316).
        let mut listed: HashMap<(NodeId, NodeId), usize> = HashMap::new();
        for (id, node) in &self.nodes {
            for child in node.children() {
                *listed.entry((*id, *child)).or_insert(0) += 1;
            }
        }

        for (id, node) in &self.nodes {
            for child in node.children() {
                let child_node = self.nodes.get(child).ok_or_else(|| {
                    CoreError::Structure(format!("{id} lists a missing child: {child}"))
                })?;
                match child_node.parent() {
                    Some(parent) if parent == *id => {}
                    Some(parent) => {
                        return Err(CoreError::Structure(format!(
                        "{child} is listed as a child of {id} but declares {parent} as its parent"
                    )))
                    }
                    None => {
                        return Err(CoreError::Structure(format!(
                            "{child} is listed as a child of {id} but declares no parent"
                        )))
                    }
                }
            }

            if *id == self.root {
                continue;
            }
            let Some(parent) = node.parent() else {
                return Err(CoreError::Structure(format!(
                    "{id} has no parent but is not the root"
                )));
            };
            let parent_node = self.nodes.get(&parent).ok_or_else(|| {
                CoreError::Structure(format!("{id} declares a missing parent: {parent}"))
            })?;
            if !parent_node.kind().accepts_child(node.kind()) {
                return Err(CoreError::Structure(format!(
                    "{id} is misplaced: {}",
                    containment_message(parent_node.kind(), node.kind())
                )));
            }
            let occurrences = listed.get(&(parent, *id)).copied().unwrap_or(0);
            if occurrences != 1 {
                return Err(CoreError::Structure(format!(
                    "{id} is listed {occurrences} times as a child of {parent} (exactly one expected)"
                )));
            }
        }

        self.validate_loaded_names()?;

        let reachable = self.walk(self.root)?.len();
        if reachable != self.nodes.len() {
            return Err(CoreError::Structure(format!(
                "{} node(s) unreachable from the root {}",
                self.nodes.len() - reachable,
                self.root
            )));
        }
        Ok(())
    }

    /// Uniqueness among all direct siblings, windows included (#105).
    /// The canonical resolution carries the folder; a bare window name
    /// stays usable if it designates a single window in the project.
    fn validate_loaded_names(&self) -> CoreResult<()> {
        for (id, node) in &self.nodes {
            let mut seen: HashMap<&str, NodeId> = HashMap::new();
            for &child in node.children() {
                let Some(child_node) = self.nodes.get(&child) else {
                    continue;
                };
                // No exception for windows here: `check_name_
                // available` (see its doc, A8) now refuses a
                // window taking the name of a sibling folder just as much as
                // the reverse — two direct siblings never share a
                // name, whatever their kind.
                if let Some(&earlier) = seen.get(child_node.name()) {
                    return Err(CoreError::Structure(format!(
                        "{child} ({}) cannot be named {} in {id}: {earlier} \
                         already has that name",
                        child_node.kind().type_name(),
                        child_node.name()
                    )));
                }
                seen.insert(child_node.name(), child);
            }
        }
        Ok(())
    }

    // -- reading --

    /// Identifier of the root folder.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of nodes, root included.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always false: a tree contains at least its root. Provided for
    /// symmetry with `len` (and for clippy).
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// True if `id` is present in the tree.
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Node `id`, or `None`.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id).map(Arc::as_ref)
    }

    /// Node `id`, or `Err(UnknownNode)`.
    pub fn get(&self, id: NodeId) -> CoreResult<&Node> {
        self.node(id).ok_or(CoreError::UnknownNode(id))
    }

    /// The `Arc` of node `id`: used by structural-sharing tests and by
    /// incremental persistence.
    pub fn node_arc(&self, id: NodeId) -> Option<&Arc<Node>> {
        self.nodes.get(&id)
    }

    /// Ordered children of `id`.
    pub fn children(&self, id: NodeId) -> CoreResult<&[NodeId]> {
        Ok(self.get(id)?.children())
    }

    /// Parent of `id` (`None` for the root).
    pub fn parent(&self, id: NodeId) -> CoreResult<Option<NodeId>> {
        Ok(self.get(id)?.parent())
    }

    /// Ancestors of `id`, from the nearest to the root. Excludes `id`.
    pub fn ancestors(&self, id: NodeId) -> CoreResult<Vec<NodeId>> {
        let mut out = Vec::new();
        let mut current = self.get(id)?.parent();
        while let Some(parent) = current {
            out.push(parent);
            current = self.get(parent)?.parent();
        }
        Ok(out)
    }

    /// Index of `id` in the child list of its parent. `Err(Structure)`
    /// for the root (it has no parent).
    pub fn index_in_parent(&self, id: NodeId) -> CoreResult<usize> {
        let node = self.get(id)?;
        let Some(parent) = node.parent() else {
            return Err(CoreError::Structure(format!(
                "{id} is the root: it has no position in a parent"
            )));
        };
        self.get(parent)?
            .children()
            .iter()
            .position(|child| *child == id)
            .ok_or_else(|| {
                CoreError::Structure(format!(
                    "{id} is missing from the child list of its parent {parent}"
                ))
            })
    }

    /// Pre-order walk of `id` and all its descendants (`id` first).
    pub fn walk(&self, id: NodeId) -> CoreResult<Vec<NodeId>> {
        self.get(id)?;
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            out.push(current);
            if let Some(node) = self.node(current) {
                for child in node.children().iter().rev() {
                    stack.push(*child);
                }
            }
        }
        Ok(out)
    }

    /// Pre-order walk from the root.
    pub fn walk_all(&self) -> Vec<NodeId> {
        self.walk(self.root).unwrap_or_default()
    }

    /// All the windows of the project, in walk order.
    pub fn windows(&self) -> Vec<NodeId> {
        self.walk_all()
            .into_iter()
            .filter(|id| self.node(*id).is_some_and(|node| node.kind().is_window()))
            .collect()
    }

    /// Number of windows whose name starts with `prefix`.
    ///
    /// D1/D2 — same walk as [`Tree::find_window`]: used by `Project::
    /// next_window_name` to guess a starting point close to the right name,
    /// without going through `windows()` (which actually keeps the list
    /// and allocates accordingly — needed where the caller wants the
    /// identifiers, not here where only the count matters). Counted **by
    /// prefix** and not all windows together: a project with two
    /// workbooks ("Book1", "Book2") and zero graphs must offer
    /// "Graph1" for the first graph, not "Graph3".
    pub fn window_count_with_prefix(&self, prefix: &str) -> u64 {
        let mut count = 0u64;
        self.visit_windows(|id| {
            if self
                .node(id)
                .is_some_and(|node| node.name().starts_with(prefix))
            {
                count += 1;
            }
            false
        });
        count
    }

    /// First window with this name, or `None`. Used to avoid automatic
    /// names already taken; `resolve_parsed` refuses ambiguous names.
    ///
    /// D2 (option 2, `guide-de-correction-robustesse.md` §F7) — direct
    /// walk that stops at the first node found, instead of going through
    /// `windows()` → `walk_all()`, which allocated a `Vec` of **all** the
    /// nodes of the project at every path resolution.
    pub fn find_window(&self, name: &str) -> Option<NodeId> {
        let mut found = None;
        self.visit_windows(|id| {
            let is_match = self
                .node(id)
                .is_some_and(|node| same_name_ignoring_case(node.name(), name));
            if is_match {
                found = Some(id);
            }
            is_match
        });
        found
    }

    /// Visits each window of the project, without ever stacking all the
    /// children of a node at once. `on_window` returns `true` to stop the
    /// walk at this window (used by `find_window`), `false` to
    /// continue.
    ///
    /// A window never contains another window (only folders
    /// do): it is therefore enough to descend into the children that are
    /// folders, and to visit the others in place without stacking them. On
    /// a folder that contains N windows and no subfolder — the usual
    /// case, a root that contains all the workbooks —, the stack
    /// never grows: `stack.extend` on all the children at once (the
    /// first version of this fix) would have allocated a buffer of size N
    /// even when stopping at the first window visited, which a
    /// node-by-node visit does not do.
    fn visit_windows(&self, mut on_window: impl FnMut(NodeId) -> bool) {
        let mut stack = vec![self.root];
        while let Some(current) = stack.pop() {
            let Some(node) = self.node(current) else {
                continue;
            };
            for &child in node.children() {
                let Some(child_node) = self.node(child) else {
                    continue;
                };
                if child_node.kind().is_window() {
                    if on_window(child) {
                        return;
                    }
                } else if child_node.kind().is_folder() {
                    stack.push(child);
                }
            }
        }
    }

    /// Direct child of `parent` with this name, or `None`.
    pub fn child_by_name(&self, parent: NodeId, name: &str) -> CoreResult<Option<NodeId>> {
        Ok(self
            .get(parent)?
            .children()
            .iter()
            .copied()
            .find(|child| self.node(*child).is_some_and(|node| node.name() == name)))
    }

    /// **Folder** child of `parent` with this name, or `None`.
    ///
    /// A8 (`guide-de-correction-robustesse.md` §F9) — distinct from
    /// `child_by_name`: a folder is not the only parent in the model to
    /// welcome two categories of children (folders and windows,
    /// `NodeKind::accepts_child`), and `check_name_available` does not prevent
    /// a window from taking the name of a sibling folder. Searching by name
    /// *then* filtering by kind would pick the namesake sibling met first
    /// in `children()`, whether it is the right one or not; searching
    /// directly among the folder children makes the resolution of a path
    /// segment independent of that order.
    fn folder_child_by_name(&self, parent: NodeId, name: &str) -> CoreResult<Option<NodeId>> {
        Ok(self.get(parent)?.children().iter().copied().find(|child| {
            self.node(*child)
                .is_some_and(|node| node.kind().is_folder() && node.name() == name)
        }))
    }

    /// Number of rows of a sheet: the maximum of the lengths of its
    /// columns, 0 if it has none. `Err(Structure)` if `sheet` is not
    /// a sheet.
    pub fn row_count(&self, sheet: NodeId) -> CoreResult<usize> {
        let node = self.get(sheet)?;
        if !node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                node.kind().with_article()
            )));
        }
        Ok(node
            .children()
            .iter()
            .filter_map(|child| self.node(*child).and_then(Node::as_column))
            .map(Column::len)
            .max()
            .unwrap_or(0))
    }

    /// Columns of the sheet of `column` (its siblings, itself included), in
    /// the order of the sheet — which makes the letters. Empty if `column` has
    /// no parent.
    pub(crate) fn sheet_columns_of(&self, column: NodeId) -> &[NodeId] {
        self.parent(column)
            .ok()
            .flatten()
            .and_then(|sheet| self.children(sheet).ok())
            .unwrap_or(&[])
    }

    /// True if the formula of `column` reads the X of a column that has none
    /// paired with it (#402: `at(B, x)`, `integral(B)`, `max(B, x1, x2)`
    /// without an X to the left of B): it computes as
    /// [`FormulaError::NoPairedX`], cells `---`. False for a formula that does
    /// not compile, or that reads a deleted column (that message comes first).
    pub fn formula_lacks_paired_x(&self, column: NodeId) -> bool {
        let Some(formula) = self
            .node(column)
            .and_then(Node::as_column)
            .and_then(|column| column.meta().formula.as_ref())
        else {
            return false;
        };
        if formula.has_deleted_reference_in(self.sheet_columns_of(column), self.scope_of(column)) {
            return false;
        }
        let Ok(compiled) = formula.compile() else {
            return false;
        };
        let paired = crate::compose::paired_x_columns(self, &compiled);
        compiled
            .columns_needing_x()
            .iter()
            .any(|y| !paired.contains_key(y))
    }

    /// True if the formula of `column` calls a column function and **every**
    /// cell of its result is empty (#402): the functions found no point (empty
    /// column, empty range, `at` outside the X domain). A row with no value
    /// among others stays silent; so does a result whose cells show `---`
    /// (another message) and a column with no row.
    pub fn formula_found_no_point(&self, column: NodeId) -> bool {
        let Some(node) = self.node(column).and_then(Node::as_column) else {
            return false;
        };
        let Some(formula) = node.meta().formula.as_ref() else {
            return false;
        };
        let calls_a_column_function = formula
            .compile()
            .is_ok_and(|compiled| compiled.has_column_function());
        calls_a_column_function
            && !node.is_empty()
            && (0..node.len()).all(|row| node.is_missing(row))
    }

    /// Iterates over `(NodeId, &Node)` in no guaranteed order (persistence, statistics).
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().map(|(id, node)| (*id, node.as_ref()))
    }

    // -- path resolution --

    /// Resolves a text path of the `[Book1]Sheet1!ColA` grammar to
    /// the identifier of the node it designates.
    ///
    /// This is **the repository's only implementation of resolution**:
    /// `Project::resolve_path` boils down to it, and a client that holds only an
    /// `Arc<Tree>` snapshot (the Python bridge, tomorrow graph rendering)
    /// calls it directly instead of rewriting a copy against the read
    /// API. That is the reason for this method to exist: resolution
    /// needs only reading, so it had no business in the mutation
    /// facade.
    ///
    /// Every error is a [`CoreError::Path`], whether the fault is syntax or
    /// the object is missing: seen from outside, it is the same case — a path we
    /// cannot follow — and only the message tells the two apart.
    pub fn resolve(&self, text: &str) -> CoreResult<NodeId> {
        self.resolve_parsed(&parse_path(text)?)
    }

    /// Resolves an already parsed path, for the caller that holds an
    /// [`ObjectPath`] and need not re-serialize it to have it re-parsed.
    ///
    /// Two properties of the grammar, easy to lose when rewriting it:
    ///
    /// - the `rooted` flag is **not** consulted. There is no current folder
    ///   in the model — that is an interface notion, not a data one —
    ///   so the descent always starts at the root and `Experiments/[Book1]Sheet1`
    ///   designates the same node as `/Experiments/[Book1]Sheet1`;
    /// - a window with a prefix is searched for in that folder;
    /// - without a prefix, its name must be unambiguous project-wide.
    ///
    /// The canonical path is rooted, even for a window at the root.
    ///
    /// Name comparison is that of `child_by_name` and `find_window`:
    /// exact equality, hence **case-sensitive**.
    pub fn resolve_parsed(&self, path: &ObjectPath) -> CoreResult<NodeId> {
        // Path without a window: a descent through folders from the root.
        // `folder_child_by_name` considers only folder children from the
        // search itself (A8): `check_name_available` does not prevent a
        // window from taking the name of a sibling folder (see its doc), so a
        // generic `child_by_name` could land on the window rather than
        // on the folder and make the latter "not found" depending on the order
        // of the children — which would break `path_of ∘ resolve_path = identity`.
        let Some(window_name) = path.window.as_deref() else {
            let mut current = self.root();
            for segment in &path.folders {
                current = self
                    .folder_child_by_name(current, segment)?
                    .ok_or_else(|| CoreError::Path(format!("folder not found: {segment}")))?;
            }
            return Ok(current);
        };

        let window = if path.rooted || !path.folders.is_empty() {
            let mut parent = self.root();
            for segment in &path.folders {
                parent = self
                    .folder_child_by_name(parent, segment)?
                    .ok_or_else(|| CoreError::Path(format!("folder not found: {segment}")))?;
            }
            self.child_by_name(parent, window_name)?
                .filter(|id| self.node(*id).is_some_and(|n| n.kind().is_window()))
                .ok_or_else(|| {
                    CoreError::Path(format!(
                        "window not found in {}: [{window_name}]",
                        display_folders(&path.folders)
                    ))
                })?
        } else {
            let mut found = None;
            let mut ambiguous = false;
            self.visit_windows(|id| {
                if self.node(id).is_some_and(|n| n.name() == window_name) {
                    if found.is_some() {
                        ambiguous = true;
                        return true;
                    }
                    found = Some(id);
                }
                false
            });
            if ambiguous {
                return Err(CoreError::Path(format!(
                    "ambiguous window name: [{window_name}]; use its folder path"
                )));
            }
            found.ok_or_else(|| CoreError::Path(format!("window not found: [{window_name}]")))?
        };

        let Some(sheet_name) = path.sheet.as_deref() else {
            return Ok(window);
        };
        // A graph or a note has no sheet: saying so is better than
        // returning "sheet not found" on a window that will never have one.
        if !matches!(self.get(window)?.kind(), NodeKind::Workbook { .. }) {
            return Err(CoreError::Path(format!(
                "[{window_name}] is not a workbook, it has no worksheet"
            )));
        }
        let sheet = self.child_by_name(window, sheet_name)?.ok_or_else(|| {
            CoreError::Path(format!("worksheet not found: [{window_name}]{sheet_name}"))
        })?;

        let Some(column_name) = path.column.as_deref() else {
            return Ok(sheet);
        };
        // The message reuses the canonical form of the path, not the input
        // string: it is the only branch that names the whole path, and
        // `resolve_parsed` no longer has the original text anyway.
        self.child_by_name(sheet, column_name)?
            .ok_or_else(|| CoreError::Path(format!("column not found: {}", format_path(path))))
    }

    /// Names of the folders above `id`, from the root (excluded) towards `id`.
    ///
    /// Serves both sides of the path: resolution uses it to check a
    /// prefix, `Project::path_of` to rebuild it.
    pub(crate) fn folder_names_above(&self, id: NodeId) -> CoreResult<Vec<String>> {
        let root = self.root();
        let mut names: Vec<String> = self
            .ancestors(id)?
            .into_iter()
            .filter(|ancestor| *ancestor != root)
            .filter_map(|ancestor| self.node(ancestor))
            .filter(|node| node.kind().is_folder())
            .map(|node| node.name().to_string())
            .collect();
        names.reverse();
        Ok(names)
    }

    // -- validation (read-only) --

    /// Checks that a name is free within the uniqueness scope of `kind`.
    /// `ignore` excludes a node from the comparison (rename, in-place move).
    fn check_name_available(
        &self,
        parent: Option<NodeId>,
        kind: &NodeKind,
        name: &str,
        ignore: Option<NodeId>,
    ) -> CoreResult<()> {
        let Some(parent) = parent else {
            return Ok(()); // node without a parent (root): no sibling
        };
        let Some(parent_node) = self.node(parent) else {
            return Ok(());
        };
        for child in parent_node.children() {
            if Some(*child) == ignore {
                continue;
            }
            // Case is ignored for every sibling except a column (#403): a
            // formula names sheets and workbooks without regard to case, so
            // two of them may not differ only by case. Columns are written by
            // letter and keep the exact comparison.
            let collides = self.node(*child).is_some_and(|node| {
                if kind.is_column() {
                    node.name() == name
                } else {
                    same_name_ignoring_case(node.name(), name)
                }
            });
            if collides {
                return Err(CoreError::DuplicateName(format!(
                    "{name} in {}",
                    parent_node.name()
                )));
            }
        }
        Ok(())
    }

    /// Validates an insertion without writing anything: name, kind, parent, index.
    pub(crate) fn validate_insert(
        &self,
        parent: NodeId,
        index: usize,
        name: &str,
        kind: &NodeKind,
    ) -> CoreResult<()> {
        check_name(name)?;
        if !kind.children().is_empty() {
            return Err(CoreError::Structure(
                "an inserted node cannot carry children: rebuild the tree with from_nodes"
                    .to_string(),
            ));
        }
        let parent_node = self.get(parent)?;
        if !parent_node.kind().accepts_child(kind) {
            return Err(CoreError::Structure(containment_message(
                parent_node.kind(),
                kind,
            )));
        }
        self.check_name_available(Some(parent), kind, name, None)?;
        let count = parent_node.children().len();
        if index > count {
            return Err(CoreError::IndexOutOfBounds(format!(
                "position {index} out of range in {} (children: {count})",
                parent_node.name()
            )));
        }
        Ok(())
    }

    /// Validates a removal without writing anything. Returns the index occupied.
    pub(crate) fn validate_remove(&self, id: NodeId) -> CoreResult<usize> {
        if id == self.root {
            return Err(CoreError::Structure(
                "the project root cannot be removed".to_string(),
            ));
        }
        self.index_in_parent(id)
    }

    /// Validates a move without writing anything. Returns `(old parent, old index)`.
    pub(crate) fn validate_move(
        &self,
        id: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> CoreResult<(NodeId, usize)> {
        if id == self.root {
            return Err(CoreError::Structure(
                "the project root cannot be moved".to_string(),
            ));
        }
        let node = self.get(id)?;
        let target = self.get(new_parent)?;
        if !target.kind().accepts_child(node.kind()) {
            return Err(CoreError::Structure(containment_message(
                target.kind(),
                node.kind(),
            )));
        }
        if self.walk(id)?.contains(&new_parent) {
            return Err(CoreError::Structure(format!(
                "circular move refused: {new_parent} is {id} or one of its descendants"
            )));
        }
        let Some(old_parent) = node.parent() else {
            return Err(CoreError::Structure(format!(
                "{id} has no parent: it cannot be moved"
            )));
        };
        let old_index = self.index_in_parent(id)?;
        self.check_name_available(Some(new_parent), node.kind(), node.name(), Some(id))?;
        let mut count = target.children().len();
        if new_parent == old_parent {
            count = count.saturating_sub(1);
        }
        if index > count {
            return Err(CoreError::IndexOutOfBounds(format!(
                "position {index} out of range in {} (other children: {count})",
                target.name()
            )));
        }
        Ok((old_parent, old_index))
    }

    /// Validates a rename without writing anything. Returns the current name.
    pub(crate) fn validate_rename(&self, id: NodeId, name: &str) -> CoreResult<String> {
        let node = self.get(id)?;
        check_name(name)?;
        let previous = node.name().to_string();
        if previous == name {
            return Ok(previous);
        }
        self.check_name_available(node.parent(), node.kind(), name, Some(id))?;
        Ok(previous)
    }

    // -- pub(crate) mutations — called only by Project --

    /// Inserts a detached node without children at `index` in `parent`.
    pub(crate) fn insert(
        &mut self,
        parent: NodeId,
        index: usize,
        node: Node,
    ) -> CoreResult<NodeId> {
        if let Some(existing) = node.parent() {
            return Err(CoreError::Structure(format!(
                "the node to insert must be detached, but it declares {existing} as its parent"
            )));
        }
        if self.contains(node.id()) {
            return Err(CoreError::Structure(format!(
                "{} is already in the tree",
                node.id()
            )));
        }
        self.validate_insert(parent, index, node.name(), node.kind())?;

        let id = node.id();
        let mut node = node;
        node.set_parent(Some(parent));

        let parent_arc = self
            .nodes
            .get_mut(&parent)
            .ok_or(CoreError::UnknownNode(parent))?;
        let Some(children) = Arc::make_mut(parent_arc).children_mut() else {
            return Err(CoreError::Structure(format!(
                "{parent} cannot accept a child"
            )));
        };
        let at = index.min(children.len());
        children.insert(at, id);
        self.nodes.insert(id, Arc::new(node));
        Ok(id)
    }

    /// Removes `id` and all its descendants from the map, and `id` from the child
    /// list of its parent. Returns the index it occupied. Refuses the root.
    pub(crate) fn remove(&mut self, id: NodeId) -> CoreResult<usize> {
        let index = self.validate_remove(id)?;
        let Some(parent) = self.parent(id)? else {
            return Err(CoreError::Structure(format!("{id} has no parent")));
        };
        let doomed = self.walk(id)?;

        if let Some(parent_arc) = self.nodes.get_mut(&parent) {
            if let Some(children) = Arc::make_mut(parent_arc).children_mut() {
                children.retain(|child| *child != id);
            }
        }
        for victim in doomed {
            self.nodes.remove(&victim);
        }
        Ok(index)
    }

    /// Moves `id` under `new_parent` at `index`. All validation happens
    /// before the slightest detachment. Returns `(old parent, old index)`.
    pub(crate) fn move_node(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> CoreResult<(NodeId, usize)> {
        let (old_parent, old_index) = self.validate_move(id, new_parent, index)?;

        if let Some(parent_arc) = self.nodes.get_mut(&old_parent) {
            if let Some(children) = Arc::make_mut(parent_arc).children_mut() {
                children.retain(|child| *child != id);
            }
        }
        let target_arc = self
            .nodes
            .get_mut(&new_parent)
            .ok_or(CoreError::UnknownNode(new_parent))?;
        if let Some(children) = Arc::make_mut(target_arc).children_mut() {
            let at = index.min(children.len());
            children.insert(at, id);
        }
        if let Some(node_arc) = self.nodes.get_mut(&id) {
            Arc::make_mut(node_arc).set_parent(Some(new_parent));
        }
        Ok((old_parent, old_index))
    }

    /// Renames `id`. Returns the old name. A rename to the current name is a
    /// no-op that does not even touch the node's `Arc`.
    pub(crate) fn rename(&mut self, id: NodeId, name: &str) -> CoreResult<String> {
        let previous = self.validate_rename(id, name)?;
        if previous == name {
            return Ok(previous);
        }
        if let Some(node_arc) = self.nodes.get_mut(&id) {
            Arc::make_mut(node_arc).set_name(name.to_string());
        }
        Ok(previous)
    }

    /// Mutable column (`Arc::make_mut` on the node). `Err(Structure)` if `id`
    /// is not a column.
    pub(crate) fn column_mut(&mut self, id: NodeId) -> CoreResult<&mut Column> {
        let kind_label = {
            let node = self.get(id)?;
            if node.kind().is_column() {
                None
            } else {
                Some(node.kind().with_article())
            }
        };
        if let Some(label) = kind_label {
            return Err(CoreError::Structure(format!(
                "{id} is not a column but {label}"
            )));
        }
        let node_arc = self.nodes.get_mut(&id).ok_or(CoreError::UnknownNode(id))?;
        Arc::make_mut(node_arc)
            .column_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a column")))
    }

    /// Mutable node (`Arc::make_mut`). No validation: the caller is
    /// responsible for the invariants it touches.
    pub(crate) fn node_mut(&mut self, id: NodeId) -> CoreResult<&mut Node> {
        let node_arc = self.nodes.get_mut(&id).ok_or(CoreError::UnknownNode(id))?;
        Ok(Arc::make_mut(node_arc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::ColumnType;
    use crate::node::WindowMeta;

    fn folder_kind() -> NodeKind {
        NodeKind::Folder {
            children: Vec::new(),
        }
    }

    fn workbook_kind() -> NodeKind {
        NodeKind::Workbook {
            sheets: Vec::new(),
            meta: WindowMeta::default(),
        }
    }

    fn worksheet_kind() -> NodeKind {
        NodeKind::Worksheet {
            columns: Vec::new(),
        }
    }

    fn column_kind() -> NodeKind {
        NodeKind::Column(Column::new(ColumnType::Numeric))
    }

    /// Small tree: root `Project` → folder `Experiments` → `Book1` → `Sheet1` → `A`.
    struct Fixture {
        tree: Tree,
        root: NodeId,
        folder: NodeId,
        book: NodeId,
        sheet: NodeId,
        column: NodeId,
        next: u64,
    }

    impl Fixture {
        fn new() -> Self {
            let root_node = Node::new(NodeId::from_raw(1), "Project", folder_kind()).unwrap();
            let mut fixture = Fixture {
                tree: Tree::new(root_node).unwrap(),
                root: NodeId::from_raw(1),
                folder: NodeId::from_raw(0),
                book: NodeId::from_raw(0),
                sheet: NodeId::from_raw(0),
                column: NodeId::from_raw(0),
                next: 2,
            };
            fixture.folder = fixture.add(fixture.root, "Experiments", folder_kind());
            fixture.book = fixture.add(fixture.folder, "Book1", workbook_kind());
            fixture.sheet = fixture.add(fixture.book, "Sheet1", worksheet_kind());
            fixture.column = fixture.add(fixture.sheet, "A", column_kind());
            fixture
        }

        fn add(&mut self, parent: NodeId, name: &str, kind: NodeKind) -> NodeId {
            let id = NodeId::from_raw(self.next);
            self.next += 1;
            let index = self.tree.children(parent).unwrap().len();
            self.tree
                .insert(parent, index, Node::new(id, name, kind).unwrap())
                .unwrap()
        }
    }

    /// Loads a tree whose root (1) lists `listed` and whose nodes
    /// `others` are folders declaring the root as their parent.
    fn load_flat(listed: Vec<u64>, others: &[u64]) -> CoreResult<Tree> {
        let children = listed.into_iter().map(NodeId::from_raw).collect();
        let mut nodes = vec![Node::new(
            NodeId::from_raw(1),
            "Project",
            NodeKind::Folder { children },
        )
        .unwrap()];
        for raw in others {
            nodes.push(
                Node::with_parent(
                    NodeId::from_raw(*raw),
                    &format!("D{raw}"),
                    Some(NodeId::from_raw(1)),
                    folder_kind(),
                )
                .unwrap(),
            );
        }
        Tree::from_nodes(NodeId::from_raw(1), nodes)
    }

    fn structure_message(result: CoreResult<Tree>) -> String {
        match result {
            Err(CoreError::Structure(message)) => message,
            other => panic!("expected Structure, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn from_nodes_refuses_a_child_listed_twice() {
        let message = structure_message(load_flat(vec![2, 2], &[2]));
        assert!(message.contains("listed 2 times"), "{message}");
    }

    #[test]
    fn from_nodes_refuses_a_node_missing_from_its_parent_list() {
        let message = structure_message(load_flat(vec![], &[2]));
        assert!(message.contains("listed 0 times"), "{message}");
    }

    #[test]
    fn from_nodes_accepts_a_child_listed_once() {
        assert!(load_flat(vec![2, 3], &[2, 3]).is_ok());
    }

    #[test]
    fn new_refuses_a_non_folder_root() {
        let node = Node::new(NodeId::from_raw(1), "Book1", workbook_kind()).unwrap();
        assert!(matches!(Tree::new(node), Err(CoreError::Structure(_))));
    }

    #[test]
    fn new_refuses_a_root_with_a_parent() {
        let node = Node::with_parent(
            NodeId::from_raw(1),
            "Project",
            Some(NodeId::from_raw(9)),
            folder_kind(),
        )
        .unwrap();
        assert!(matches!(Tree::new(node), Err(CoreError::Structure(_))));
    }

    #[test]
    fn reading_walks_and_indexes() {
        let f = Fixture::new();
        assert_eq!(f.tree.len(), 5);
        assert!(!f.tree.is_empty());
        assert_eq!(f.tree.root(), f.root);
        assert!(f.tree.contains(f.column));
        assert_eq!(
            f.tree.walk_all(),
            vec![f.root, f.folder, f.book, f.sheet, f.column]
        );
        assert_eq!(
            f.tree.walk(f.book).unwrap(),
            vec![f.book, f.sheet, f.column]
        );
        assert_eq!(
            f.tree.ancestors(f.column).unwrap(),
            vec![f.sheet, f.book, f.folder, f.root]
        );
        assert_eq!(f.tree.index_in_parent(f.column).unwrap(), 0);
        assert!(matches!(
            f.tree.index_in_parent(f.root),
            Err(CoreError::Structure(_))
        ));
        assert_eq!(f.tree.windows(), vec![f.book]);
        assert_eq!(f.tree.find_window("Book1"), Some(f.book));
        assert_eq!(f.tree.find_window("Missing"), None);
        assert_eq!(f.tree.child_by_name(f.sheet, "A").unwrap(), Some(f.column));
        assert_eq!(f.tree.child_by_name(f.sheet, "Z").unwrap(), None);
        assert_eq!(f.tree.iter().count(), 5);
    }

    #[test]
    fn get_reports_unknown_nodes() {
        let f = Fixture::new();
        let ghost = NodeId::from_raw(999);
        assert!(matches!(f.tree.get(ghost), Err(CoreError::UnknownNode(_))));
        assert!(f.tree.node(ghost).is_none());
        assert!(f.tree.node_arc(ghost).is_none());
    }

    #[test]
    fn row_count_is_the_longest_column() {
        let mut f = Fixture::new();
        assert_eq!(f.tree.row_count(f.sheet).unwrap(), 0);
        f.tree
            .column_mut(f.column)
            .unwrap()
            .set_values(0, crate::column::ValueBlock::Float(vec![1.0, 2.0]))
            .unwrap();
        let long = f.add(f.sheet, "B", NodeKind::Column(Column::from_f64(&[1.0; 5])));
        assert_eq!(f.tree.row_count(f.sheet).unwrap(), 5);
        assert!(f.tree.contains(long));
        assert!(matches!(
            f.tree.row_count(f.book),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn insert_refuses_a_node_that_already_has_children() {
        let mut f = Fixture::new();
        let kind = NodeKind::Folder {
            children: vec![NodeId::from_raw(42)],
        };
        let node = Node::new(NodeId::from_raw(50), "X", kind).unwrap();
        assert!(matches!(
            f.tree.insert(f.root, 0, node),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn insert_refuses_an_attached_node_or_a_known_id() {
        let mut f = Fixture::new();
        let attached =
            Node::with_parent(NodeId::from_raw(50), "X", Some(f.root), folder_kind()).unwrap();
        assert!(matches!(
            f.tree.insert(f.root, 0, attached),
            Err(CoreError::Structure(_))
        ));
        let clash = Node::new(f.column, "X", folder_kind()).unwrap();
        assert!(matches!(
            f.tree.insert(f.root, 0, clash),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn insert_checks_the_index() {
        let mut f = Fixture::new();
        let node = Node::new(NodeId::from_raw(50), "B", column_kind()).unwrap();
        assert!(matches!(
            f.tree.insert(f.sheet, 5, node),
            Err(CoreError::IndexOutOfBounds(_))
        ));
    }

    #[test]
    fn remove_takes_the_whole_subtree() {
        let mut f = Fixture::new();
        let index = f.tree.remove(f.book).unwrap();
        assert_eq!(index, 0);
        assert_eq!(f.tree.len(), 2);
        assert!(!f.tree.contains(f.sheet));
        assert!(!f.tree.contains(f.column));
        assert!(f.tree.children(f.folder).unwrap().is_empty());
    }

    #[test]
    fn remove_refuses_the_root() {
        let mut f = Fixture::new();
        assert!(matches!(
            f.tree.remove(f.root),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn rename_to_the_same_name_keeps_the_node_shared() {
        let mut f = Fixture::new();
        let before = f.tree.node_arc(f.column).cloned().unwrap();
        assert_eq!(f.tree.rename(f.column, "A").unwrap(), "A");
        let after = f.tree.node_arc(f.column).unwrap();
        assert!(Arc::ptr_eq(&before, after));
    }

    #[test]
    fn move_node_refuses_a_cycle() {
        let mut f = Fixture::new();
        let inner = f.add(f.folder, "Inner", folder_kind());
        let error = f.tree.move_node(f.folder, inner, 0).unwrap_err();
        match error {
            CoreError::Structure(message) => assert!(message.contains("circular"), "{message}"),
            other => panic!("expected Structure, got {other:?}"),
        }
    }

    #[test]
    fn move_node_within_the_same_parent_counts_after_removal() {
        let mut f = Fixture::new();
        let b = f.add(f.sheet, "B", column_kind());
        let c = f.add(f.sheet, "C", column_kind());
        assert_eq!(
            f.tree.move_node(f.column, f.sheet, 2).unwrap(),
            (f.sheet, 0)
        );
        assert_eq!(f.tree.children(f.sheet).unwrap(), &[b, c, f.column]);
        assert!(matches!(
            f.tree.move_node(f.column, f.sheet, 3),
            Err(CoreError::IndexOutOfBounds(_))
        ));
    }

    #[test]
    fn column_mut_refuses_other_kinds() {
        let mut f = Fixture::new();
        assert!(matches!(
            f.tree.column_mut(f.sheet),
            Err(CoreError::Structure(_))
        ));
        assert!(f.tree.column_mut(f.column).is_ok());
        assert!(f.tree.node_mut(f.sheet).is_ok());
        assert!(matches!(
            f.tree.node_mut(NodeId::from_raw(999)),
            Err(CoreError::UnknownNode(_))
        ));
    }

    // -- path resolution --
    //
    // Resolution now lives here, and no longer in `Project` nor copied
    // into the Python bridge: these tests cover what moved, including the
    // two points on which the two old copies could diverge
    // without any test seeing it (the message for a column not found, and the
    // fact that `rooted` changes nothing).

    #[test]
    fn resolve_follows_the_four_levels_of_the_grammar() {
        let f = Fixture::new();
        assert_eq!(f.tree.resolve("/").unwrap(), f.root);
        assert_eq!(f.tree.resolve("/Experiments").unwrap(), f.folder);
        assert_eq!(f.tree.resolve("[Book1]").unwrap(), f.book);
        assert_eq!(f.tree.resolve("[Book1]Sheet1").unwrap(), f.sheet);
        assert_eq!(f.tree.resolve("[Book1]Sheet1!A").unwrap(), f.column);
        assert_eq!(
            f.tree.resolve("/Experiments/[Book1]Sheet1!A").unwrap(),
            f.column
        );
    }

    #[test]
    fn resolve_ignores_the_rooted_flag() {
        // There is no current folder in the model: the descent always starts
        // at the root, with or without a leading `/`.
        let f = Fixture::new();
        assert_eq!(f.tree.resolve("Experiments").unwrap(), f.folder);
        assert_eq!(
            f.tree.resolve("Experiments/[Book1]Sheet1!A").unwrap(),
            f.column
        );
    }

    #[test]
    fn resolve_finds_a_window_without_its_folder_prefix() {
        // An unambiguous name can be written without a prefix; the canonical path
        // still designates the precise folder, with no implicit resolution.
        let f = Fixture::new();
        assert_eq!(f.tree.resolve("[Book1]Sheet1").unwrap(), f.sheet);
        let err = f.tree.resolve("/Elsewhere/[Book1]Sheet1").unwrap_err();
        let CoreError::Path(message) = err else {
            panic!("expected Path, got {err:?}");
        };
        assert!(message.contains("folder not found"), "{message}");
    }

    #[test]
    fn resolve_is_case_sensitive() {
        let f = Fixture::new();
        assert!(matches!(
            f.tree.resolve("[book1]Sheet1"),
            Err(CoreError::Path(_))
        ));
        assert!(matches!(
            f.tree.resolve("[Book1]sheet1"),
            Err(CoreError::Path(_))
        ));
        assert!(matches!(
            f.tree.resolve("[Book1]Sheet1!a"),
            Err(CoreError::Path(_))
        ));
    }

    #[test]
    fn resolve_turns_every_failure_into_a_path_error() {
        let mut f = Fixture::new();
        f.add(
            f.root,
            "Fig1",
            NodeKind::Graph {
                meta: WindowMeta::default(),
                spec: stokesforge_plot::PlotSpec::default(),
            },
        );
        for text in [
            "",                      // syntax: empty path
            "[Book1",                // syntax: unclosed bracket
            "Sheet1!A",              // syntax: column without a workbook
            "[Missing]",             // unknown window
            "[Book1]SheetX",         // unknown sheet
            "[Book1]Sheet1!Missing", // unknown column
            "/Unknown",              // unknown folder
            "[Fig1]Sheet1",          // window without a sheet
        ] {
            assert!(
                matches!(f.tree.resolve(text), Err(CoreError::Path(_))),
                "expected Path for {text:?}, got {:?}",
                f.tree.resolve(text)
            );
        }
    }

    #[test]
    fn resolve_does_not_mistake_a_window_for_a_folder() {
        // `Experiments/Book1` descends through folders: `Book1` is a workbook, so the
        // segment must not be followed.
        let f = Fixture::new();
        assert!(matches!(
            f.tree.resolve("/Experiments/Book1"),
            Err(CoreError::Path(_))
        ));
    }

    #[test]
    fn resolve_names_the_canonical_path_when_a_column_is_missing() {
        // The message reuses the canonical form, not the input string: the
        // edge spaces and the extra `//` do not show up in it.
        let f = Fixture::new();
        let err = f
            .tree
            .resolve("  /Experiments//[Book1]Sheet1!Missing  ")
            .unwrap_err();
        let CoreError::Path(message) = err else {
            panic!("expected Path, got {err:?}");
        };
        assert_eq!(
            message,
            "column not found: /Experiments/[Book1]Sheet1!Missing"
        );
    }

    #[test]
    fn resolve_parsed_agrees_with_resolve() {
        let f = Fixture::new();
        let path = crate::path::parse_path("[Book1]Sheet1!A").unwrap();
        assert_eq!(
            f.tree.resolve_parsed(&path).unwrap(),
            f.tree.resolve("[Book1]Sheet1!A").unwrap()
        );
    }

    #[test]
    fn folder_names_above_lists_folders_from_the_root_down() {
        let mut f = Fixture::new();
        let day1 = f.add(f.folder, "Day1", folder_kind());
        let book2 = f.add(day1, "Book2", workbook_kind());
        assert_eq!(
            f.tree.folder_names_above(f.book).unwrap(),
            vec!["Experiments"]
        );
        assert_eq!(
            f.tree.folder_names_above(book2).unwrap(),
            vec!["Experiments", "Day1"]
        );
        assert!(f.tree.folder_names_above(f.root).unwrap().is_empty());
    }

    #[test]
    fn from_nodes_round_trips_a_valid_tree() {
        let f = Fixture::new();
        let nodes: Vec<Node> = f.tree.iter().map(|(_, node)| node.clone()).collect();
        let rebuilt = Tree::from_nodes(f.root, nodes).unwrap();
        assert_eq!(rebuilt, f.tree);
    }

    #[test]
    fn from_nodes_refuses_a_duplicate_id() {
        let a = Node::new(NodeId::from_raw(1), "Project", folder_kind()).unwrap();
        let b = Node::new(NodeId::from_raw(1), "Other", folder_kind()).unwrap();
        assert!(matches!(
            Tree::from_nodes(NodeId::from_raw(1), vec![a, b]),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn from_nodes_refuses_an_unreachable_node() {
        let root = Node::new(NodeId::from_raw(1), "Project", folder_kind()).unwrap();
        let orphan = Node::new(NodeId::from_raw(2), "Elsewhere", folder_kind()).unwrap();
        assert!(matches!(
            Tree::from_nodes(NodeId::from_raw(1), vec![root, orphan]),
            Err(CoreError::Structure(_))
        ));
    }

    #[test]
    fn from_nodes_refuses_a_mutual_cycle_outside_the_root() {
        let root = Node::new(NodeId::from_raw(1), "Project", folder_kind()).unwrap();
        let a = Node::with_parent(
            NodeId::from_raw(2),
            "A",
            Some(NodeId::from_raw(3)),
            NodeKind::Folder {
                children: vec![NodeId::from_raw(3)],
            },
        )
        .unwrap();
        let b = Node::with_parent(
            NodeId::from_raw(3),
            "B",
            Some(NodeId::from_raw(2)),
            NodeKind::Folder {
                children: vec![NodeId::from_raw(2)],
            },
        )
        .unwrap();
        assert!(matches!(
            Tree::from_nodes(NodeId::from_raw(1), vec![root, a, b]),
            Err(CoreError::Structure(_))
        ));
    }
}

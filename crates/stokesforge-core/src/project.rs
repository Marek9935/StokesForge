//! Root of the model: the current tree (shared) and the identifier allocator.
//!
//! `Project` is the crate's **only public mutation facade**. Its
//! methods take `&mut self`; the only `&mut Project` of the process will be
//! held by the actor of `stokesforge-session`, which makes "no mutation
//! outside a command" a property of ownership, not a convention.
//!
//! The allocator lives here, **outside the tree**, so that an undo that
//! restores an old tree (`replace_tree`) does not move the counter back:
//! an identifier is never reused over the whole life of a project,
//! reloading included.

use std::sync::Arc;

use stokesforge_plot::{PlotSpec, SavedColors};

use crate::column::{Column, ColumnMeta, ColumnType, ColumnValues, ValueBlock, ValuesDelta};
use crate::error::{CoreError, CoreResult};
use crate::id::{IdAllocator, NodeId};
use crate::node::{Node, NodeKind, WindowMeta};
use crate::path::{format_path, ObjectPath};
use crate::tree::Tree;

/// Name of a folder or a column "spreadsheet style": A, B, …, Z, AA, AB, …
///
/// Public because it is not only a default-name generator:
/// it is **the** position → letter mapping of the product, and the layers
/// above must designate a column in it the same way as the grid
/// shows it — the header of `stokesforge-app` as well as the history labels of
/// `stokesforge-commands`. Three diverging implementations of this algorithm
/// meant three ways of calling the same column.
pub fn spreadsheet_name(index: usize) -> String {
    let mut name = String::new();
    let mut remaining = index as i64;
    loop {
        let offset = (remaining % 26) as u8;
        name.insert(0, char::from(b'A' + offset));
        remaining = remaining / 26 - 1;
        if remaining < 0 {
            break;
        }
    }
    name
}

/// Root of the model: the current tree (shared) and the identifier allocator
/// (never moved back).
///
/// Is **not** `Clone` (B9, `guide-de-correction-robustesse.md` §F2): a
/// `#[derive(Clone)]` would duplicate `IdAllocator` with the tree, so that
/// two copies of the same project would then assign the same
/// identifiers to different nodes — exactly what the documentation
/// of `NodeId` promises never to do. `Arc<Tree>` (via [`Project::tree`])
/// stays cloneable, for free, for whoever needs a shared snapshot.
#[derive(Debug)]
pub struct Project {
    tree: Arc<Tree>,
    ids: IdAllocator,
    /// Identity of **this instance** of `Project`, stable over its whole life —
    /// unlike `tree`, which `replace_tree` (undo/redo
    /// of a structural entry) reassigns to an already existing `Arc<Tree>`,
    /// potentially the same one as at an earlier moment of this same
    /// instance. Serves whoever needs to recognize "this precise
    /// `Project`, seen again later" without relying on the identity of its
    /// current tree — for example B8 (`stokesforge-persistence`, detection of a
    /// concurrent writer), which must keep recognizing an instance
    /// after an `undo` made it point to a tree already seen. An
    /// `Arc<()>` rather than a counter: identity by address, free,
    /// with no registry to maintain here.
    identity: Arc<()>,
    /// Next column version number to assign (A7,
    /// `guide-de-correction-robustesse.md` §F2 — doctrine of stamps:
    /// "content version, assigned by the project"). A first version
    /// of this fix let each `Column` increment itself
    /// (a local `wrapping_add(1)`) and only reassigned a fresh number
    /// at the time of an `undo` on a structural entry, computed
    /// from the trees still visible in the history — insufficient:
    /// a random sweep
    /// (`b14_a_version_never_designates_two_contents`) found a
    /// number reassigned to two different contents all the same, an
    /// ordinary write being able to advance a column to a number
    /// that another path (undo then new write) also reaches
    /// elsewhere. A single counter, held by the project and consulted by
    /// **every** column write (ordinary or restoration), closes the
    /// question once and for all — the same pattern as `IdAllocator` for
    /// `NodeId`s, at the scale of column versions.
    column_version_counter: u64,
}

impl Project {
    /// New project: a root folder named `name` (identifier 1), nothing else.
    pub fn new(name: &str) -> CoreResult<Self> {
        let mut ids = IdAllocator::new();
        // The very first identifier of a new allocator (`next == 1`) can
        // never exhaust `u64`; `?` stays the plainest form for
        // a function that already returns `CoreResult`.
        let id = ids.next()?;
        let root = Node::new(
            id,
            name,
            NodeKind::Folder {
                children: Vec::new(),
            },
        )?;
        Ok(Self {
            tree: Arc::new(Tree::new(root)?),
            ids,
            identity: Arc::new(()),
            column_version_counter: 1,
        })
    }

    /// New project without name validation: infallible path reserved for the crate's
    /// literal names (see `Default`).
    fn with_root_name(name: &str) -> Self {
        let mut ids = IdAllocator::new();
        // `Default` requires an infallible function; the very first
        // identifier of a new allocator (`next == 1`) can never
        // exhaust `u64`, so this `expect` never fires in
        // practice — which the crate's linter cannot know.
        #[allow(clippy::expect_used)]
        let id = ids
            .next()
            .expect("the first identifier cannot be exhausted");
        let root = Node::new_unchecked(
            id,
            name,
            NodeKind::Folder {
                children: Vec::new(),
            },
        );
        Self {
            tree: Arc::new(Tree::with_root(root)),
            ids,
            identity: Arc::new(()),
            column_version_counter: 1,
        }
    }

    /// Rebuilds from a loaded tree and the persisted value of the counter;
    /// `next_id_hint` is already the **next** identifier to assign (see
    /// [`IdAllocator::resume`]), and the counter resumes at
    /// `max(next_id_hint, largest identifier of the tree + 1)`.
    pub fn from_tree(tree: Arc<Tree>, next_id_hint: u64) -> Self {
        let highest = tree
            .iter()
            .map(|(id, _)| id.raw())
            .chain(tree.calculations().keys().map(|id| id.raw()))
            .chain(tree.correction_traces().keys().map(|id| id.raw()))
            // The identifiers read by formulas (#321): a read column
            // may no longer be in the tree (deleted, then file
            // saved), and its number must never be reassigned — the
            // formula would read another one (invariant 7).
            .chain(
                tree.iter()
                    .filter_map(|(_, node)| node.as_column())
                    .filter_map(|column| column.meta().formula.as_ref())
                    .flat_map(|formula| formula.columns())
                    .map(|id| id.raw()),
            )
            .max()
            .unwrap_or(0);
        // A7 — the column version counter is not yet persisted
        // (unlike `next_id_hint`): resuming beyond the highest
        // version already read at least avoids regressing WITHIN this
        // load; two `Project`s loaded separately from the same file
        // can, however, each assign the same numbers to different
        // contents — outside the scope of A7, which only
        // covers in-memory restoration of a single instance.
        let highest_version = tree
            .iter()
            .filter_map(|(_, node)| node.as_column())
            .map(Column::version)
            .max()
            .unwrap_or(0);
        Self {
            ids: IdAllocator::resume(next_id_hint, highest),
            tree,
            identity: Arc::new(()),
            column_version_counter: highest_version + 1,
        }
    }

    /// `pub(crate)` access to the tree, for the crate modules that write
    /// records carried by `Tree` (see [`crate::calc`]).
    pub(crate) fn tree_arc_mut(&mut self) -> &mut Arc<Tree> {
        &mut self.tree
    }

    /// Allocates a project identifier (never reused, invariant 7).
    pub(crate) fn allocate_id(&mut self) -> CoreResult<NodeId> {
        self.ids.next()
    }

    // -- reading --

    /// Current tree. Cloning it (`project.tree().clone()`) freezes a snapshot.
    pub fn tree(&self) -> &Arc<Tree> {
        &self.tree
    }

    /// Weak handle on the identity of **this instance**, stable over
    /// its whole life (see the doc of the `identity` field). `Weak::upgrade` fails as soon
    /// as this `Project` is destroyed; two distinct instances — even
    /// loaded from the same file — never return two handles that
    /// upgrade to the same address.
    pub fn identity(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&self.identity)
    }

    /// Identifier of the root folder.
    pub fn root(&self) -> NodeId {
        self.tree.root()
    }

    /// Name of the project, that is, of its root folder.
    pub fn name(&self) -> &str {
        self.tree
            .node(self.tree.root())
            .map(Node::name)
            .unwrap_or_default()
    }

    /// Value of the next identifier that will be assigned (to persist).
    pub fn next_id_hint(&self) -> u64 {
        self.ids.peek()
    }

    // -- restoration (undo) --

    /// Replaces the current tree with an earlier snapshot. **Does not touch**
    /// the identifier counter: a restored node comes back under its
    /// original identifier, and later creations stay above.
    pub fn replace_tree(&mut self, tree: Arc<Tree>) {
        self.tree = tree;
    }

    // -- structure --

    /// Adds a folder at the end of `parent`.
    pub fn add_folder(&mut self, parent: NodeId, name: &str) -> CoreResult<NodeId> {
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            name,
            NodeKind::Folder {
                children: Vec::new(),
            },
        )
    }

    /// Adds a workbook with a "Sheet1" sheet. `name: None` → first free
    /// "BookN" among the project's windows.
    pub fn add_workbook(&mut self, parent: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        self.add_workbook_with_sheet(parent, name, None)
    }

    /// Adds a workbook and its first sheet, atomically: if adding
    /// the sheet fails, the freshly created workbook is removed and the error
    /// of the sheet is returned as is — the tree then goes back
    /// identical to its state before the call. Only the identifier
    /// counter may have advanced (it never goes back, including on this
    /// rollback), which is harmless since an identifier is
    /// never reused.
    fn add_workbook_with_sheet(
        &mut self,
        parent: NodeId,
        name: Option<&str>,
        sheet_name: Option<&str>,
    ) -> CoreResult<NodeId> {
        let book = self.add_empty_workbook(parent, name)?;
        match self.add_worksheet(book, sheet_name) {
            Ok(_) => Ok(book),
            Err(err) => {
                // The workbook was just created, with no other possible reference:
                // this removal should never fail. If it failed all the
                // same, we prefer to return the original error rather than a
                // cleanup error that would mask it.
                let _ = self.remove(book);
                Err(err)
            }
        }
    }

    /// Adds a workbook **without** a sheet (persistence, tests).
    pub fn add_empty_workbook(&mut self, parent: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        let name = self.window_name(name, "Book");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Workbook {
                sheets: Vec::new(),
                meta: WindowMeta::default(),
            },
        )
    }

    /// Adds a sheet at the end of a workbook. `name: None` → first free "SheetN"
    /// in this workbook.
    pub fn add_worksheet(&mut self, book: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        let name = match name {
            Some(name) => name.to_string(),
            None => self.next_child_name(book, "Sheet")?,
        };
        let index = self.tree.children(book)?.len();
        self.insert_node(
            book,
            index,
            &name,
            NodeKind::Worksheet {
                columns: Vec::new(),
            },
        )
    }

    /// Duplicates a sheet, columns included, right after it in its
    /// workbook, under the name `name`. The copy is an ordinary sheet with no
    /// link to the original: each column receives a new identifier (the
    /// value buffers stay shared copy-on-write until the first
    /// write). The series that reference the columns of the original
    /// still reference them.
    pub fn duplicate_worksheet(&mut self, sheet: NodeId, name: &str) -> CoreResult<NodeId> {
        let source = self.tree.get(sheet)?;
        if !source.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                source.kind().with_article()
            )));
        }
        let book = self.tree.parent(sheet)?.ok_or_else(|| {
            CoreError::Structure(format!("{sheet} has no workbook to duplicate into"))
        })?;
        let index = self
            .tree
            .children(book)?
            .iter()
            .position(|&child| child == sheet)
            .map_or(0, |at| at + 1);
        let content = crate::NodeCopy::capture(&self.tree, sheet)?;
        let before = self.tree.clone();
        let result = self.insert_copy(book, &content, name).and_then(|copy| {
            self.move_node(copy, book, index)?;
            Ok(copy)
        });
        if result.is_err() {
            self.tree = before;
        }
        result
    }

    /// Pastes a subtree with identifiers created in this project.
    /// Graphs keep their references: transferring their data
    /// between projects belongs to another gesture (#107). Any failure restores the tree,
    /// but never moves the allocator back.
    pub fn insert_copy(
        &mut self,
        parent: NodeId,
        copy: &crate::NodeCopy,
        name: &str,
    ) -> CoreResult<NodeId> {
        let before = self.tree.clone();
        let result = (|| {
            let mut ids = std::collections::HashMap::new();
            let mut graph_specs = std::collections::HashMap::new();
            if copy.is_external() {
                for source in copy.nodes() {
                    let NodeKind::Graph { spec, .. } = source.kind() else {
                        continue;
                    };
                    let mut columns = std::collections::HashMap::new();
                    if let Some((_, data)) =
                        copy.graph_data().iter().find(|(id, _)| *id == source.id())
                    {
                        let book_name = self.next_window_name("Book");
                        let book = self.insert_copy(parent, data, &book_name)?;
                        let inserted = self.tree.walk(book)?;
                        for (old, new) in data.nodes().iter().zip(inserted) {
                            if old.as_column().is_some() {
                                columns.insert(old.id(), new);
                            }
                        }
                    }
                    // A broken reference receives a reservation with no node:
                    // neither an existing column nor a future allocation
                    // must give it another meaning in the destination.
                    let mut spec = spec.clone();
                    for series in spec.panels.iter_mut().flat_map(|p| &mut p.series) {
                        for reference in [&mut series.x, &mut series.x_error, &mut series.y_error]
                            .into_iter()
                            .flatten()
                        {
                            let id = match columns.get(reference) {
                                Some(id) => *id,
                                None => {
                                    let id = self.ids.next()?;
                                    columns.insert(*reference, id);
                                    id
                                }
                            };
                            *reference = id;
                        }
                        let id = match columns.get(&series.y) {
                            Some(id) => *id,
                            None => {
                                let id = self.ids.next()?;
                                columns.insert(series.y, id);
                                id
                            }
                        };
                        series.y = id;
                    }
                    graph_specs.insert(source.id(), spec);
                }
            }
            let mut root = None;
            for source in copy.nodes() {
                let destination = if source.id() == copy.root().id() {
                    parent
                } else {
                    source
                        .parent()
                        .and_then(|id| ids.get(&id).copied())
                        .ok_or_else(|| CoreError::Structure("copy is not in prefix order".into()))?
                };
                let mut kind = source.kind().clone();
                if let NodeKind::Graph { spec, .. } = &mut kind {
                    if let Some(replacement) = graph_specs.remove(&source.id()) {
                        *spec = replacement;
                    }
                }
                // A version coming from another project must never be
                // reassigned later to other content of this column:
                // `insert_node`, below, moves the counter past it.
                if let Some(children) = kind.children_mut() {
                    children.clear();
                }
                let index = self.tree.children(destination)?.len();
                let id = self.insert_node(
                    destination,
                    index,
                    if root.is_none() { name } else { source.name() },
                    kind,
                )?;
                ids.insert(source.id(), id);
                root.get_or_insert(id);
            }
            // The formulas of the copied columns read the columns **of the
            // copy** (#321), like the series above: renumbered after
            // the loop, because a formula may read a sibling inserted later
            // (prefix order). A reference outside the copy receives a
            // reservation with no node — it is shown as `#REF`, and neither an
            // existing column nor a future allocation must give it another
            // meaning in the destination.
            let mut reserved = std::collections::HashMap::new();
            for source in copy.nodes() {
                let Some(formula) = source
                    .as_column()
                    .and_then(|column| column.meta().formula.as_ref())
                else {
                    continue;
                };
                let Some(&new_column) = ids.get(&source.id()) else {
                    continue;
                };
                let mut failure = None;
                let remapped = formula.remap(|old| {
                    if let Some(&id) = ids.get(&old).or_else(|| reserved.get(&old)) {
                        return id;
                    }
                    match self.ids.next() {
                        Ok(id) => {
                            reserved.insert(old, id);
                            id
                        }
                        Err(error) => {
                            failure.get_or_insert(error);
                            old
                        }
                    }
                });
                if let Some(error) = failure {
                    return Err(error);
                }
                let column = Arc::make_mut(&mut self.tree).column_mut(new_column)?;
                let mut meta = column.meta().clone();
                meta.formula = Some(remapped);
                column.set_meta(meta);
            }
            root.ok_or_else(|| CoreError::Structure("empty copy".into()))
        })();
        if result.is_err() {
            self.tree = before;
        }
        result
    }

    /// Inserts a column at `index` (`None` → at the end). `name: None` → first
    /// free spreadsheet-style name (A, B, …, Z, AA, AB, …) in this sheet.
    pub fn add_column(
        &mut self,
        sheet: NodeId,
        index: Option<usize>,
        name: Option<&str>,
        column: Column,
    ) -> CoreResult<NodeId> {
        let sheet_node = self.tree.get(sheet)?;
        if !sheet_node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                sheet_node.kind().with_article()
            )));
        }
        let index = index.unwrap_or_else(|| sheet_node.children().len());
        let name = match name {
            Some(name) => name.to_string(),
            None => self.next_column_name(sheet)?,
        };
        self.insert_node(sheet, index, &name, NodeKind::Column(column))
    }

    /// Adds a graph carrying `spec`. `name: None` → first free
    /// "GraphN".
    ///
    /// Unlike `add_workbook`/`add_matrix`/`add_note`, the content
    /// is a required parameter — same choice as [`Project::add_column`]
    /// for the same reason: a graph without a spec describes nothing more
    /// than an empty window, better to make it explicit at the call than
    /// to fabricate a default nobody asked for. A caller who really
    /// wants an empty graph passes `PlotSpec::default()`.
    ///
    /// [`Project::set_plot_spec`] then replaces the spec of an already
    /// created graph, always as a whole.
    pub fn add_graph(
        &mut self,
        parent: NodeId,
        name: Option<&str>,
        spec: PlotSpec,
    ) -> CoreResult<NodeId> {
        let name = self.window_name(name, "Graph");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Graph {
                meta: WindowMeta::default(),
                spec,
            },
        )
    }

    /// Adds a matrix. `name: None` → first free "MBookN".
    pub fn add_matrix(&mut self, parent: NodeId, name: Option<&str>) -> CoreResult<NodeId> {
        let name = self.window_name(name, "MBook");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Matrix {
                meta: WindowMeta::default(),
            },
        )
    }

    /// Adds a note carrying `text`. `name: None` → first free "NoteN".
    pub fn add_note(
        &mut self,
        parent: NodeId,
        name: Option<&str>,
        text: &str,
    ) -> CoreResult<NodeId> {
        let name = self.window_name(name, "Note");
        let index = self.tree.children(parent)?.len();
        self.insert_node(
            parent,
            index,
            &name,
            NodeKind::Note {
                meta: WindowMeta::default(),
                text: text.to_string(),
            },
        )
    }

    /// Inserts a generic node **without children** at `index` in `parent`.
    /// Low-level facade of the `add_*` methods.
    ///
    /// A column keeps the version it carries (coming from a file, another
    /// project, a snapshot from before a load), but the project's counter
    /// moves past it: otherwise a future write of this
    /// column could draw that number for other content, and the
    /// targeted recalculation of formulas (#193) would believe a formula that
    /// reads it up to date — a false "clean" found in review, locked by
    /// `a_column_added_with_its_version_does_not_make_that_number_serve_again`
    /// (`stokesforge-session/tests/recalc_targeted.rs`).
    pub fn insert_node(
        &mut self,
        parent: NodeId,
        index: usize,
        name: &str,
        kind: NodeKind,
    ) -> CoreResult<NodeId> {
        // Read-only validation first: a failure must not clone
        // the tree nor consume an identifier.
        self.tree.validate_insert(parent, index, name, &kind)?;
        let id = self.ids.next()?;
        let version = match &kind {
            NodeKind::Column(column) => Some(column.version()),
            _ => None,
        };
        let node = Node::new(id, name, kind)?;
        let tree = Arc::make_mut(&mut self.tree);
        let inserted = tree.insert(parent, index, node)?;
        if let Some(version) = version {
            self.column_version_counter =
                self.column_version_counter.max(version.saturating_add(1));
        }
        Ok(inserted)
    }

    /// Removes `id` and all its descendants. Returns the index it occupied.
    /// The copies in the hidden workbook that no analysis cites any more leave
    /// with it (#248).
    pub fn remove(&mut self, id: NodeId) -> CoreResult<usize> {
        let index = self.remove_without_gc(id)?;
        self.gc_analysis_store();
        Ok(index)
    }

    /// [`Project::remove`] without the hidden-workbook cleanup: for an
    /// operation that has just created copies no calculation cites yet
    /// and that does the cleanup at its end.
    pub(crate) fn remove_without_gc(&mut self, id: NodeId) -> CoreResult<usize> {
        self.tree.validate_remove(id)?;
        let tree = Arc::make_mut(&mut self.tree);
        let index = tree.remove(id)?;
        self.prune_orphaned_calculations();
        self.prune_transposes();
        Ok(index)
    }

    /// Removes the calculations whose input Y columns have **all** disappeared
    /// (sheet, workbook or columns deleted): they have no sheet left to
    /// show on nor anything to recalculate. A calculation that keeps at least one series
    /// stays, stale for the others (`Staleness::InputMissing`). Undoing
    /// the deletion brings them back with the tree.
    fn prune_orphaned_calculations(&mut self) {
        let orphans: Vec<NodeId> = self
            .tree
            .calculations()
            .iter()
            .filter(|(_, c)| c.series.iter().all(|s| self.tree.node(s.y).is_none()))
            .map(|(id, _)| *id)
            .collect();
        let traces: Vec<NodeId> = self
            .tree
            .correction_traces()
            .iter()
            .filter(|(_, c)| c.series.iter().all(|s| self.tree.node(s.y).is_none()))
            .map(|(id, _)| *id)
            .collect();
        if orphans.is_empty() && traces.is_empty() {
            return;
        }
        let tree = Arc::make_mut(&mut self.tree);
        for id in orphans {
            tree.calculations_mut().remove(&id);
        }
        for id in traces {
            tree.correction_traces_mut().remove(&id);
        }
    }

    /// Renames `id`. Returns the old name.
    ///
    /// A source sheet of a transposed sheet (#253) drags the name of
    /// the latter along: "Newᵀ".
    pub fn rename(&mut self, id: NodeId, name: &str) -> CoreResult<String> {
        let previous = self.rename_without_follow(id, name)?;
        if previous != name {
            self.follow_source_rename(id, &previous);
        }
        Ok(previous)
    }

    pub(crate) fn rename_without_follow(&mut self, id: NodeId, name: &str) -> CoreResult<String> {
        let previous = self.tree.validate_rename(id, name)?;
        if previous == name {
            return Ok(previous);
        }
        let tree = Arc::make_mut(&mut self.tree);
        tree.rename(id, name)
    }

    /// Moves `id` under `new_parent` at `index`. Returns `(old parent, old index)`.
    pub fn move_node(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> CoreResult<(NodeId, usize)> {
        self.tree.validate_move(id, new_parent, index)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.move_node(id, new_parent, index)
    }

    /// Replaces the metadata of a window. Returns the previous ones.
    /// `Err(Structure)` if `id` is not a window.
    pub fn set_window_meta(&mut self, id: NodeId, meta: WindowMeta) -> CoreResult<WindowMeta> {
        let node = self.tree.get(id)?;
        if !node.kind().is_window() {
            return Err(CoreError::Structure(format!(
                "{id} is not a window but {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .kind_mut()
            .window_meta_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a window")))?;
        Ok(std::mem::replace(slot, meta))
    }

    /// Replaces the text of a note. Returns the previous one. `Err(Structure)` if `id`
    /// is not a note.
    pub fn set_note_text(&mut self, id: NodeId, text: &str) -> CoreResult<String> {
        let node = self.tree.get(id)?;
        if !matches!(node.kind(), NodeKind::Note { .. }) {
            return Err(CoreError::Structure(format!(
                "{id} is not a note but {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .note_text_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a note")))?;
        Ok(std::mem::replace(slot, text.to_string()))
    }

    /// Replaces the `PlotSpec` of an existing graph, **as a whole** — never by
    /// sub-field (series, axis, legend): same logic as
    /// [`Project::set_values`], which replaces a whole range rather than
    /// cell by cell. Returns the previous one. `Err(Structure)` if `id` is
    /// not a graph.
    pub fn set_plot_spec(&mut self, id: NodeId, spec: PlotSpec) -> CoreResult<PlotSpec> {
        let node = self.tree.get(id)?;
        if !matches!(node.kind(), NodeKind::Graph { .. }) {
            return Err(CoreError::Structure(format!(
                "{id} is not a graph but {}",
                node.kind().with_article()
            )));
        }
        let tree = Arc::make_mut(&mut self.tree);
        let slot = tree
            .node_mut(id)?
            .kind_mut()
            .plot_spec_mut()
            .ok_or_else(|| CoreError::Structure(format!("{id} is not a graph")))?;
        Ok(std::mem::replace(slot, spec))
    }

    /// Replaces the colors saved in the project, and returns the
    /// previous ones. Bounded to [`stokesforge_plot::MAX_SAVED_COLORS`] per use.
    pub fn set_saved_colors(&mut self, colors: SavedColors) -> SavedColors {
        let tree = Arc::make_mut(&mut self.tree);
        std::mem::replace(tree.saved_colors_mut(), colors.bounded())
    }

    // -- data --

    /// Writes `block` into `column` starting at row `start`.
    ///
    /// A7 (`guide-de-correction-robustesse.md` §F2) — the version number
    /// that `Column::set_values` assigns itself internally is overwritten by a
    /// number drawn from [`Project::next_column_version`], never reassigned
    /// over the whole life of the project (see the doc of the field
    /// `column_version_counter`). Drawn only after the `?`: a refused
    /// call (block of the wrong type, bound exceeded) consumes no
    /// number.
    pub fn set_values(
        &mut self,
        column: NodeId,
        start: usize,
        block: ValueBlock,
    ) -> CoreResult<ValuesDelta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let delta = tree.column_mut(column)?.set_values(start, block)?;
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(delta)
    }

    /// Truncates or extends `column` to `length` rows. See [`Project::
    /// set_values`] for why the version number is reassigned by
    /// the project (A7).
    pub fn set_length(&mut self, column: NodeId, length: usize) -> CoreResult<ValuesDelta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let delta = tree.column_mut(column)?.set_length(length);
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(delta)
    }

    /// Replaces the metadata of a column. Returns the previous ones.
    pub fn set_column_meta(&mut self, column: NodeId, meta: ColumnMeta) -> CoreResult<ColumnMeta> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        Ok(tree.column_mut(column)?.set_meta(meta))
    }

    /// Changes the type of a column. Returns `(old type, old values)`.
    /// Version reassigned by the project (A7) — see [`Project::set_values`].
    pub fn set_column_type(
        &mut self,
        column: NodeId,
        column_type: ColumnType,
    ) -> CoreResult<(ColumnType, ColumnValues)> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let previous = tree.column_mut(column)?.set_column_type(column_type);
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(previous)
    }

    /// Restores type and values of a column (undo of a type
    /// change). Version reassigned by the project (A7) — see
    /// [`Project::set_values`]: "undoing is one more write" (§6 of the
    /// guide), hence a write that must also receive a number
    /// this project never assigned.
    pub fn restore_column(
        &mut self,
        column: NodeId,
        column_type: ColumnType,
        values: ColumnValues,
    ) -> CoreResult<()> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.restore(column_type, values)?;
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(())
    }

    /// The next column version number, never reassigned (see the
    /// doc of the field `column_version_counter`). `pub`: `stokesforge-commands`
    /// (A7) needs it for every column write, ordinary or
    /// restoration — see [`Project::restamp_column_version`].
    pub fn next_column_version(&mut self) -> u64 {
        let version = self.column_version_counter;
        self.column_version_counter = self.column_version_counter.wrapping_add(1);
        version
    }

    /// Imposes a version number on `column`, without touching its content.
    ///
    /// **Reserved for `History::revert`** (`stokesforge-commands`, A7,
    /// `guide-de-correction-robustesse.md` §F2): `Entry::Structural`
    /// restores a whole `Arc<Tree>` via [`Project::replace_tree`],
    /// version counters included, which can make that of a
    /// column go back to a number that different content (its own, at another
    /// moment of its history) has already carried. `History::revert` calls
    /// this method right after `replace_tree`, with a number drawn from
    /// [`Project::next_column_version`], to reassign to the affected
    /// columns only a number that THIS project never assigned to
    /// anyone — otherwise a future write could make
    /// two contents coincide under the same number, exactly what the doctrine of
    /// stamps in the guide forbids.
    ///
    /// `Err(Structure)` if `column` is not a column.
    pub fn restamp_column_version(&mut self, column: NodeId, version: u64) -> CoreResult<()> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.restamp_version(version);
        Ok(())
    }

    /// Inserts `count` rows at `at` in **each column of the sheet such
    /// that `at <= len(column)`**. A column too short to have a
    /// row `at` (`at > len`) is ignored: lengths are free between
    /// columns of the same sheet, and lengthening a short column
    /// would invent data. `at == len` — the end of the column — **is**
    /// a target: it is the only way to add a row at the end of the
    /// sheet (B5, `guide-de-correction-robustesse.md` §F5), and it is
    /// exactly the range that `Column::insert_rows` already accepts per
    /// column (`at ∈ 0..=len`). Returns, in sheet order, the
    /// identifiers of the columns actually lengthened (empty if none);
    /// `History::execute` then pushes nothing for that case (F5).
    pub fn insert_rows(
        &mut self,
        sheet: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<Vec<NodeId>> {
        let targets = self.row_targets(sheet, at, true)?;
        if targets.is_empty() || count == 0 {
            return Ok(Vec::new());
        }
        // A7 — a fresh number per column actually touched, drawn before
        // taking `tree` as an exclusive borrow (`next_column_version` needs
        // the whole `&mut self`).
        let fresh_versions: Vec<u64> = targets.iter().map(|_| self.next_column_version()).collect();
        let tree = Arc::make_mut(&mut self.tree);
        for (&id, fresh) in targets.iter().zip(fresh_versions) {
            tree.column_mut(id)?.insert_rows(at, count)?;
            tree.column_mut(id)?.restamp_version(fresh);
        }
        Ok(targets)
    }

    /// Deletes `count` rows at `at` in **the same set** of columns
    /// (`at < len` — unlike `insert_rows`, `at == len` designates
    /// no existing row and must target nothing). Returns, per column
    /// touched and in sheet order, `(NodeId, removed values)`.
    pub fn delete_rows(
        &mut self,
        sheet: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<Vec<(NodeId, ValueBlock)>> {
        let targets = self.row_targets(sheet, at, false)?;
        if targets.is_empty() || count == 0 {
            return Ok(Vec::new());
        }
        // A7 — see `Project::insert_rows`.
        let fresh_versions: Vec<u64> = targets.iter().map(|_| self.next_column_version()).collect();
        let tree = Arc::make_mut(&mut self.tree);
        let mut removed = Vec::with_capacity(targets.len());
        for (id, fresh) in targets.into_iter().zip(fresh_versions) {
            let block = tree.column_mut(id)?.delete_rows(at, count)?;
            tree.column_mut(id)?.restamp_version(fresh);
            removed.push((id, block));
        }
        Ok(removed)
    }

    /// Inserts `count` missing rows at `at` in **this single column**
    /// (`at` in `0..=len`). Per-column primitive, used to replay
    /// the undo of `delete_rows` column by column (a column emptied
    /// to its end no longer has a row `at` and escapes `insert_rows`, which
    /// operates at sheet level).
    pub fn insert_column_rows(
        &mut self,
        column: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<()> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        tree.column_mut(column)?.insert_rows(at, count)?;
        // A7 — see `Project::insert_rows`. Undo primitive: "undoing
        // is one more write" (§6 of the guide).
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(())
    }

    /// Deletes `count` rows at `at` in **this single column**, clamped to
    /// the end. Returns the removed values. Per-column primitive, symmetric
    /// to `insert_column_rows`.
    pub fn delete_column_rows(
        &mut self,
        column: NodeId,
        at: usize,
        count: usize,
    ) -> CoreResult<ValueBlock> {
        self.check_column(column)?;
        let tree = Arc::make_mut(&mut self.tree);
        let removed = tree.column_mut(column)?.delete_rows(at, count)?;
        // A7 — see `Project::insert_column_rows`.
        let fresh = self.next_column_version();
        Arc::make_mut(&mut self.tree)
            .column_mut(column)?
            .restamp_version(fresh);
        Ok(removed)
    }

    // -- paths --

    /// Parses `text` then resolves it.
    ///
    /// Simple delegation to [`Tree::resolve`]: resolution needs only
    /// reading, so it lives in `Tree`, where clients that have only an
    /// `Arc<Tree>` snapshot can reach it. This method stays here
    /// because it is the natural entry point when one holds a `Project`.
    pub fn resolve_path(&self, text: &str) -> CoreResult<NodeId> {
        self.tree.resolve(text)
    }

    /// Resolves a parsed path to a node identifier. Delegates to
    /// [`Tree::resolve_parsed`].
    pub fn resolve(&self, path: &ObjectPath) -> CoreResult<NodeId> {
        self.tree.resolve_parsed(path)
    }

    /// Canonical anchored path: `/` for the root, `/A/B` for a folder,
    /// `/A/[Book1]Sheet1!Col` for the rest (the prefix is the sequence of
    /// folders containing the window, root excluded).
    pub fn path_of(&self, id: NodeId) -> CoreResult<String> {
        let node = self.tree.get(id)?;
        let path = match node.kind() {
            NodeKind::Folder { .. } => {
                let mut folders = self.folder_names_above(id)?;
                if id != self.root() {
                    folders.push(node.name().to_string());
                }
                ObjectPath {
                    folders,
                    rooted: true,
                    ..Default::default()
                }
            }
            NodeKind::Workbook { .. }
            | NodeKind::Graph { .. }
            | NodeKind::Matrix { .. }
            | NodeKind::Note { .. }
            | NodeKind::Unknown(_) => ObjectPath {
                folders: self.folder_names_above(id)?,
                window: Some(node.name().to_string()),
                rooted: true,
                ..Default::default()
            },
            NodeKind::Worksheet { .. } => {
                let book = self.required_parent(id)?;
                ObjectPath {
                    folders: self.folder_names_above(book)?,
                    window: Some(self.tree.get(book)?.name().to_string()),
                    sheet: Some(node.name().to_string()),
                    rooted: true,
                    ..Default::default()
                }
            }
            NodeKind::Column(_) => {
                let sheet = self.required_parent(id)?;
                let book = self.required_parent(sheet)?;
                ObjectPath {
                    folders: self.folder_names_above(book)?,
                    window: Some(self.tree.get(book)?.name().to_string()),
                    sheet: Some(self.tree.get(sheet)?.name().to_string()),
                    column: Some(node.name().to_string()),
                    rooted: true,
                }
            }
        };
        Ok(format_path(&path))
    }

    /// First `{prefix}{n}` (n ≥ 1) that is the name of no existing window.
    ///
    /// D1 — no longer collects the list of **all** windows at each
    /// call (nor, below it, the list of **all** nodes of the project, see
    /// D2/`Tree::find_window`): `window_count` gives a starting point
    /// close to the right name in O(nodes) but without proportional allocation,
    /// and `find_window` tests each candidate directly. In the usual
    /// case — names assigned in order — a single candidate is
    /// tried.
    pub fn next_window_name(&self, prefix: &str) -> String {
        let mut n: u64 = self.tree.window_count_with_prefix(prefix) + 1;
        let mut candidate = format!("{prefix}{n}");
        while self.tree.find_window(&candidate).is_some() {
            n += 1;
            candidate = format!("{prefix}{n}");
        }
        candidate
    }

    // -- internal --

    /// Requested window name, or first free "{prefix}N".
    fn window_name(&self, name: Option<&str>, prefix: &str) -> String {
        match name {
            Some(name) => name.to_string(),
            None => self.next_window_name(prefix),
        }
    }

    /// First free "{prefix}N" (n ≥ 1) among the direct children of `parent`.
    ///
    /// D1 — no longer collects the names of the children in a `Vec`/`HashSet` at
    /// each call (that was the allocation that made automatic naming
    /// O(n²) per node, O(n³) for the siblings): `children(parent)` already returns
    /// the slice without a copy, and the candidate-by-candidate comparison
    /// costs only comparisons, not allocations — invisible to the
    /// byte counter of `b17`. Start at `children.len()` rather than 1:
    /// in the usual case (names assigned in order), a single candidate
    /// is enough.
    fn next_child_name(&self, parent: NodeId, prefix: &str) -> CoreResult<String> {
        let children = self.tree.children(parent)?;
        let mut n: u64 = children.len() as u64 + 1;
        let mut candidate = format!("{prefix}{n}");
        while children
            .iter()
            .filter_map(|child| self.tree.node(*child))
            .any(|node| node.name() == candidate)
        {
            n += 1;
            candidate = format!("{prefix}{n}");
        }
        Ok(candidate)
    }

    /// First free spreadsheet-style column name in `sheet`.
    ///
    /// D1 — see [`Project::next_child_name`]: no collection of names
    /// is built, and the index starts at `children.len()` rather than
    /// 0.
    fn next_column_name(&self, sheet: NodeId) -> CoreResult<String> {
        let children = self.tree.children(sheet)?;
        let mut index: usize = children.len();
        let mut candidate = spreadsheet_name(index);
        while children
            .iter()
            .filter_map(|child| self.tree.node(*child))
            .any(|node| node.name() == candidate)
        {
            index += 1;
            candidate = spreadsheet_name(index);
        }
        Ok(candidate)
    }

    /// Names of the folders containing `id`, root excluded, from top to bottom.
    fn folder_names_above(&self, id: NodeId) -> CoreResult<Vec<String>> {
        self.tree.folder_names_above(id)
    }

    /// Parent of `id`, or `Err(Structure)` if it has none (detached node or root).
    fn required_parent(&self, id: NodeId) -> CoreResult<NodeId> {
        self.tree
            .parent(id)?
            .ok_or_else(|| CoreError::Structure(format!("{id} has no parent")))
    }

    /// Checks read-only that `id` is a column — avoids cloning
    /// the tree for nothing on the usual failure case.
    fn check_column(&self, id: NodeId) -> CoreResult<()> {
        let node = self.tree.get(id)?;
        if node.kind().is_column() {
            Ok(())
        } else {
            Err(CoreError::Structure(format!(
                "{id} is not a column but {}",
                node.kind().with_article()
            )))
        }
    }

    /// Columns of `sheet`, in order, that have a row `at`.
    /// Columns of `sheet` whose row `at` exists (`insert_rows`:
    /// `allow_append` also includes `at == len`, the end of the column;
    /// `delete_rows` does not want it — see B5, `guide-de-correction-
    /// robustesse.md` §F5).
    fn row_targets(&self, sheet: NodeId, at: usize, allow_append: bool) -> CoreResult<Vec<NodeId>> {
        let sheet_node = self.tree.get(sheet)?;
        if !sheet_node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{sheet} is not a worksheet but {}",
                sheet_node.kind().with_article()
            )));
        }
        Ok(sheet_node
            .children()
            .iter()
            .copied()
            .filter(|id| {
                self.tree
                    .node(*id)
                    .and_then(Node::as_column)
                    .is_some_and(|column| {
                        if allow_append {
                            at <= column.len()
                        } else {
                            at < column.len()
                        }
                    })
            })
            .collect())
    }
}

impl Default for Project {
    /// Empty project named "Project".
    fn default() -> Self {
        Self::with_root_name("Project")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The helper followed the resolution into `tree.rs`; its test stays here,
    // unchanged, and imports it from its new home.
    use crate::tree::display_folders;

    /// A column inserted with its version keeps it, and the counter moves
    /// past it: no future write will draw that number (#193). The
    /// witness: a new column (version 0) does not move the
    /// counter back, which stays past what it has already given.
    #[test]
    fn an_inserted_column_moves_the_counter_past_its_version() {
        let mut project = Project::new("Demo").expect("project");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("workbook");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("sheet");
        let incoming = Column::from_block_with_version(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0]),
            ColumnMeta::default(),
            40,
        )
        .expect("column");
        let id = project
            .add_column(sheet, None, None, incoming)
            .expect("insertion");
        assert_eq!(
            project
                .tree()
                .node(id)
                .and_then(Node::as_column)
                .map(Column::version),
            Some(40)
        );
        assert_eq!(project.next_column_version(), 41);

        project
            .add_column(sheet, None, None, Column::from_f64(&[2.0]))
            .expect("insertion");
        assert_eq!(project.next_column_version(), 42, "witness: no step back");
    }

    #[test]
    fn spreadsheet_names_follow_the_reference_algorithm() {
        assert_eq!(spreadsheet_name(0), "A");
        assert_eq!(spreadsheet_name(25), "Z");
        assert_eq!(spreadsheet_name(26), "AA");
        assert_eq!(spreadsheet_name(27), "AB");
        assert_eq!(spreadsheet_name(51), "AZ");
        assert_eq!(spreadsheet_name(52), "BA");
        assert_eq!(spreadsheet_name(701), "ZZ");
        assert_eq!(spreadsheet_name(702), "AAA");
    }

    #[test]
    fn display_folders_shows_the_root_as_a_slash() {
        assert_eq!(display_folders(&[]), "/");
        assert_eq!(display_folders(&["A".to_string(), "B".to_string()]), "A/B");
    }

    #[test]
    fn default_is_an_empty_project_named_project() {
        let project = Project::default();
        assert_eq!(project.name(), "Project");
        assert_eq!(project.tree().len(), 1);
        assert_eq!(project.next_id_hint(), 2);
    }

    #[test]
    fn new_rejects_an_invalid_name() {
        assert!(matches!(
            Project::new("[oops]"),
            Err(CoreError::InvalidName(_))
        ));
    }

    #[test]
    fn a_failed_insertion_consumes_no_identifier() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        project.add_folder(root, "Experiments").unwrap();
        let before = project.next_id_hint();
        assert!(project.add_folder(root, "Experiments").is_err());
        assert_eq!(project.next_id_hint(), before);
    }

    #[test]
    fn add_workbook_with_sheet_rolls_back_the_workbook_on_a_failed_sheet() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        let before = project.tree().clone();

        let err = project.add_workbook_with_sheet(root, None, Some("Sheet!1"));
        assert!(matches!(err, Err(CoreError::InvalidName(_))));
        assert_eq!(**project.tree(), *before);
    }

    #[test]
    fn window_names_skip_the_taken_ones() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        project.add_empty_workbook(root, Some("Book2")).unwrap();
        // D1 (`guide-de-correction-robustesse.md` §F7) — the automatic name
        // starts from the NUMBER of existing windows, not from 1: rebuilding the
        // list of taken names to find the smallest free number is
        // precisely the cost that D1 removes (O(n²) per name, O(n³) for the
        // sheet, measured at 17.8 s for 4,000 columns). With a single
        // window already there ("Book2"), the first candidate tried is therefore
        // "Book2" (already taken), then "Book3" (free) — no longer "Book1",
        // which stays a valid name but is no longer searched.
        assert_eq!(project.next_window_name("Book"), "Book3");
        project.add_empty_workbook(root, Some("Book1")).unwrap();
        assert_eq!(project.next_window_name("Book"), "Book3");
    }

    // -- formulas by identifier (#321) --

    /// Sheet of `book` ("Sheet1") and `count` ordinary columns in a row.
    fn sheet_with_columns(
        project: &mut Project,
        book: &str,
        count: usize,
    ) -> (NodeId, Vec<NodeId>) {
        let root = project.root();
        project.add_workbook(root, Some(book)).expect("workbook");
        let sheet = project
            .resolve_path(&format!("[{book}]Sheet1"))
            .expect("sheet");
        let columns = (0..count)
            .map(|k| {
                project
                    .add_column(sheet, None, None, Column::from_f64(&[k as f64 + 1.0]))
                    .expect("column")
            })
            .collect();
        (sheet, columns)
    }

    /// Sets the formula `text` (in letters) on `column`, as the interface does.
    fn set_formula(project: &mut Project, column: NodeId, text: &str) {
        let formula = project
            .tree()
            .formula_from_letters(column, text)
            .expect("formula");
        let mut meta = project
            .tree()
            .node(column)
            .and_then(Node::as_column)
            .expect("column")
            .meta()
            .clone();
        meta.formula = Some(formula);
        project.set_column_meta(column, meta).expect("meta");
    }

    /// Columns of the sheet, in order.
    fn columns_of(project: &Project, sheet: NodeId) -> Vec<NodeId> {
        project.tree().children(sheet).expect("sheet").to_vec()
    }

    /// The stored formula of `column`.
    fn formula_of(project: &Project, column: NodeId) -> crate::ColumnFormula {
        project
            .tree()
            .node(column)
            .and_then(Node::as_column)
            .and_then(|column| column.meta().formula.clone())
            .expect("formula")
    }

    /// A duplicated sheet: the formula of the copy reads the columns **of the
    /// copy**, not those of the original (F7). Witness: the original keeps its own.
    /// Mutant: renumbering removed -> the copy reads the columns of the original
    /// (its letters show as `#REF`).
    #[test]
    fn duplicating_a_sheet_makes_the_copy_read_its_own_columns() {
        let mut project = Project::new("Demo").expect("project");
        let (sheet, columns) = sheet_with_columns(&mut project, "Book1", 3);
        set_formula(&mut project, columns[2], "A+B*2");

        let copy = project.duplicate_worksheet(sheet, "Copy").expect("copy");
        let copied = columns_of(&project, copy);
        assert_eq!(copied.len(), 3);

        assert_eq!(
            formula_of(&project, copied[2]).columns(),
            std::collections::BTreeSet::from([copied[0], copied[1]]),
            "the copy reads the columns of the copy"
        );
        assert_eq!(
            project.tree().formula_letters(copied[2]).as_deref(),
            Some("A+B*2")
        );
        // Witness: the original still reads its own.
        assert_eq!(
            formula_of(&project, columns[2]).columns(),
            std::collections::BTreeSet::from([columns[0], columns[1]])
        );
        assert_eq!(
            project.tree().formula_letters(columns[2]).as_deref(),
            Some("A+B*2")
        );
    }

    /// Pasting a lone formula column into another project: its references
    /// leave the copy, they show as `#REF` and stay so, even when
    /// the destination receives enough new columns to catch up with the
    /// numbers of the source (reservation with no node, like the series).
    /// Mutant: references left as they are, or reservation removed.
    #[test]
    fn a_reference_broken_by_the_copy_stays_ref_after_columns_are_added() {
        let mut source = Project::new("Source").expect("project");
        let (_, source_columns) = sheet_with_columns(&mut source, "BookS", 3);
        set_formula(&mut source, source_columns[2], "A+B");
        let copy = crate::NodeCopy::capture(source.tree(), source_columns[2])
            .expect("capture")
            .into_external();

        let mut destination = Project::new("Destination").expect("project");
        let (sheet, _) = sheet_with_columns(&mut destination, "BookD", 0);
        let pasted = destination
            .insert_copy(sheet, &copy, "Pasted")
            .expect("paste");
        assert_eq!(
            destination.tree().formula_letters(pasted).as_deref(),
            Some("#REF+#REF")
        );

        // The destination receives enough columns to pass the numbers of the source.
        for _ in 0..12 {
            destination
                .add_column(sheet, None, None, Column::from_f64(&[0.0]))
                .expect("column");
        }
        assert_eq!(
            destination.tree().formula_letters(pasted).as_deref(),
            Some("#REF+#REF"),
            "no new column is read by mistake"
        );
        // Witness: the reserved numbers are those of no column of the sheet.
        let formula = formula_of(&destination, pasted);
        let ids = columns_of(&destination, sheet);
        assert!(formula.has_deleted_reference(&ids));
        assert!(formula.columns().iter().all(|id| !ids.contains(id)));
    }

    /// `next_id_hint` behind: a file saved after the deletion
    /// of a read column keeps its number in the formula, and that number must
    /// never be reassigned. Mutant: the chain of formula identifiers
    /// removed from `highest`.
    #[test]
    fn from_tree_does_not_reassign_an_identifier_read_by_a_formula() {
        let mut project = Project::new("Demo").expect("project");
        let (_, columns) = sheet_with_columns(&mut project, "Book1", 2);
        let mut meta = project
            .tree()
            .node(columns[1])
            .and_then(Node::as_column)
            .expect("column")
            .meta()
            .clone();
        meta.formula = Some(crate::ColumnFormula::from_parts(vec![
            crate::FormulaPart::Column(NodeId::from_raw(99)),
            crate::FormulaPart::Text("*2".into()),
        ]));
        project.set_column_meta(columns[1], meta).expect("meta");

        let reloaded = Project::from_tree(project.tree().clone(), 1);
        assert!(
            reloaded.next_id_hint() > 99,
            "next identifier: {}",
            reloaded.next_id_hint()
        );
    }
}

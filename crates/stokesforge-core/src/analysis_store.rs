//! The hidden workbook of analysis copies (#248).
//!
//! Two kinds of column copies live there, **a single hidden workbook per
//! project**:
//!
//! - the **frozen reference** of a correction by a reference column (with
//!   its X): the calculation reads the copy, not the original column, so
//!   deleting or changing the latter, or its workbook, breaks nothing. A
//!   copy is shared by all the spectra of the same calculation (a single
//!   reference per calculation) and by the calculations that correct with the same column
//!   in the same state;
//! - the **original** of each column that a data replacement overwrote,
//!   kept in the `.stf` so that "Restore original data" can give it back,
//!   even after closing.
//!
//! # Identification and life cycle
//!
//! The workbook is an ordinary workbook of the tree, `hidden` and marked by
//! [`ANALYSIS_STORE_MARK`] in its window comment: it therefore travels
//! with the tree (session snapshot, undo, `.stf`) with no separate schema,
//! and stays out of `books()`, the Project Manager and the Object Manager,
//! like the hidden workbooks that carry the data of a copied graph.
//!
//! No copy has a life of its own: a column of the workbook is kept
//! as long as a calculation (`Calculation::aux`) or a correction (`CorrectionRecord`
//! : its reference or its originals) cites it, and removed with the last
//! analysis that uses it ([`Project::gc_analysis_store`]). The workbook itself
//! disappears with its last copy. All these writes are part of one
//! **structural** command: undo brings back the whole tree, copies included.
//!
//! # Why not a separate table in the tree
//!
//! One more `BTreeMap<NodeId, Column>` in [`Tree`] would have needed a `.stf`
//! schema field, a copy in the snapshots and its own walk for
//! the column version (invariant 11: every piece of data read by a calculation enters
//! the fingerprint). An ordinary column already has all of this: version,
//! persistence, targeted recalculation.

use std::collections::HashSet;

use crate::calc::AuxRef;
use crate::column::{Column, ColumnType, ColumnValues};
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::node::{NodeKind, WindowMeta};
use crate::project::Project;
use crate::tree::Tree;

/// Same type and same cells (binary comparison: `NaN` equals `NaN`).
fn same_content(a: &Column, b: &Column) -> bool {
    a.column_type() == b.column_type()
        && a.len() == b.len()
        && (0..a.len()).all(|row| a.get(row) == b.get(row))
}

/// The window comment that designates the hidden workbook of copies.
pub const ANALYSIS_STORE_MARK: &str = "stokesforge:analysis-store";

/// Prefix of the column comment that says where a copy comes from
/// (`copy-of:<id>`): the same column, in the same state, is copied only
/// once. The **content** decides the "state", never a version number: it
/// depends on the history of the session's writes (targeted recalculation
/// draws fewer than the full one), and two trees that the `b22` oracle compares
/// must stay equal.
const SOURCE_MARK: &str = "copy-of:";

/// Name of the hidden workbook.
const STORE_NAME: &str = "Analysis Data";

impl Tree {
    /// The sheet of the hidden workbook of copies, if it exists.
    pub fn analysis_store_sheet(&self) -> Option<NodeId> {
        self.children(self.root())
            .ok()?
            .iter()
            .find_map(|id| match self.node(*id)?.kind() {
                NodeKind::Workbook { sheets, meta }
                    if meta.hidden && meta.comment == ANALYSIS_STORE_MARK =>
                {
                    sheets.first().copied()
                }
                _ => None,
            })
    }

    /// Is `column` a copy in the hidden workbook?
    pub fn is_analysis_copy(&self, column: NodeId) -> bool {
        self.analysis_store_sheet()
            .is_some_and(|sheet| self.parent(column).ok().flatten() == Some(sheet))
    }
}

impl Project {
    /// The sheet of the hidden workbook of copies, if it exists.
    pub fn analysis_store_sheet(&self) -> Option<NodeId> {
        self.tree().analysis_store_sheet()
    }

    /// Is `column` a copy in the hidden workbook?
    pub fn is_analysis_copy(&self, column: NodeId) -> bool {
        self.tree().is_analysis_copy(column)
    }

    /// The sheet of copies, created if needed (hidden workbook, one per project).
    fn ensure_analysis_store(&mut self) -> CoreResult<NodeId> {
        if let Some(sheet) = self.analysis_store_sheet() {
            return Ok(sheet);
        }
        let root = self.root();
        // A user workbook may already carry this name: ours is
        // told apart by its marker, not by its name.
        let mut name = STORE_NAME.to_string();
        let mut n = 1;
        while self
            .tree()
            .children(root)?
            .iter()
            .any(|c| self.tree().node(*c).is_some_and(|node| node.name() == name))
        {
            n += 1;
            name = format!("{STORE_NAME} {n}");
        }
        let book = self.add_empty_workbook(root, Some(&name))?;
        self.set_window_meta(
            book,
            WindowMeta {
                label: String::new(),
                comment: ANALYSIS_STORE_MARK.to_string(),
                hidden: true,
            },
        )?;
        self.add_worksheet(book, Some("Copies"))
    }

    /// Copies `source` into the hidden workbook. `share`: reuses the copy already
    /// made of this column in this same state (frozen reference); otherwise the
    /// copy is new (original of a replacement).
    fn copy_into_store(&mut self, source: NodeId, share: bool) -> CoreResult<NodeId> {
        let column = self
            .tree()
            .get(source)?
            .as_column()
            .ok_or_else(|| CoreError::Structure("only a column can be copied".into()))?
            .clone();
        let mark = format!("{SOURCE_MARK}{}", source.raw());
        if share {
            if let Some(sheet) = self.analysis_store_sheet() {
                let found = self.tree().children(sheet)?.iter().copied().find(|c| {
                    self.tree()
                        .node(*c)
                        .and_then(|n| n.as_column())
                        .is_some_and(|col| col.meta().comment == mark && same_content(col, &column))
                });
                if let Some(found) = found {
                    return Ok(found);
                }
            }
        }
        let label = self.path_of(source).unwrap_or_default();
        let sheet = self.ensure_analysis_store()?;
        let mut meta = column.meta().clone();
        // The copy is a photo: no formula to recalculate, no plot role.
        meta.formula = None;
        meta.long_name = label;
        meta.comment = mark;
        let mut copy = column;
        copy.set_meta(meta);
        let id = self.add_column(sheet, None, None, copy)?;
        // A version number that this project never assigned to other
        // content (A7): the copy does not inherit its source's.
        let version = self.next_column_version();
        self.restamp_column_version(id, version)?;
        Ok(id)
    }

    /// Freezes the reference column of a correction (and its X): the calculation will read
    /// these copies. A reference already frozen is returned as is.
    pub(crate) fn freeze_reference(&mut self, aux: &AuxRef) -> CoreResult<AuxRef> {
        if self.is_analysis_copy(aux.y) {
            return Ok(aux.clone());
        }
        let y = self.copy_into_store(aux.y, true)?;
        let x = match aux.x {
            Some(x) if self.is_analysis_copy(x) => Some(x),
            Some(x) => Some(self.copy_into_store(x, true)?),
            None => None,
        };
        Ok(AuxRef { x, y })
    }

    /// Keeps the original of `column` before a data replacement.
    pub(crate) fn keep_original(&mut self, column: NodeId) -> CoreResult<NodeId> {
        self.copy_into_store(column, false)
    }

    /// Gives `column` back the type and values kept in `copy`.
    pub(crate) fn restore_from_copy(&mut self, column: NodeId, copy: NodeId) -> CoreResult<()> {
        let source = self
            .tree()
            .get(copy)?
            .as_column()
            .ok_or_else(|| CoreError::Structure("an original copy must be a column".into()))?;
        let column_type: ColumnType = source.column_type();
        let values: ColumnValues = source.values().clone();
        self.restore_column(column, column_type, values)
    }

    /// Removes the copies that no analysis cites any more, then the hidden
    /// workbook if it is empty.
    pub(crate) fn gc_analysis_store(&mut self) {
        let Some(sheet) = self.analysis_store_sheet() else {
            return;
        };
        let mut used: HashSet<NodeId> = HashSet::new();
        let tree = self.tree();
        for calc in tree.calculations().values() {
            if let Some(aux) = &calc.aux {
                used.insert(aux.y);
                used.extend(aux.x);
            }
        }
        for trace in tree.correction_traces().values() {
            if let Some(aux) = &trace.aux {
                used.insert(aux.y);
                used.extend(aux.x);
            }
            used.extend(trace.originals.iter().map(|(_, copy)| *copy));
        }
        let dead: Vec<NodeId> = tree
            .children(sheet)
            .map(|children| {
                children
                    .iter()
                    .copied()
                    .filter(|c| !used.contains(c))
                    .collect()
            })
            .unwrap_or_default();
        for column in dead {
            // A copy that cannot be removed stays: a failed cleanup does not undo
            // the analysis that triggered it.
            let _ = self.remove_without_gc(column);
        }
        let empty = self
            .tree()
            .children(sheet)
            .map(<[NodeId]>::is_empty)
            .unwrap_or(false);
        if empty {
            if let Some(book) = self.tree().parent(sheet).ok().flatten() {
                let _ = self.remove_without_gc(book);
            }
        }
    }
}

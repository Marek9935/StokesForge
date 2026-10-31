//! Linked transposed sheets (#253): analyzing a **row** of a sheet
//! means analyzing a column of a transposed sheet.
//!
//! Analysis stays entirely in columns. A sheet "`<Source>ᵀ`" carries
//! an x' column (the x' of the source columns) then one column per requested
//! row ("Row 10"). The link is a **recomputable snapshot**, not a
//! live link: it remembers what it read ([`TransposeSourceColumn`]) and the
//! sheet is called *stale* when the source no longer matches it, *modified*
//! when a value of a column created by the link has been edited. Neither
//! state is stored: they are derived from the tree (like the staleness
//! of a calculation), and do not go through the targeted recalculation of formulas.
//!
//! The link is carried by the tree ([`Tree::transposes`]), like calculations:
//! it travels with the history snapshots and the file.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::column::{Column, ColumnMeta, ColumnRole, ColumnType, ColumnValues, MixedCell};
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::name::same_name_ignoring_case;
use crate::node::NodeKind;
use crate::project::{spreadsheet_name, Project};
use crate::tree::Tree;

/// The suffix of the name of a transposed sheet.
pub const TRANSPOSE_MARK: char = '\u{1D40}';

/// Name of the x' column in the transposed sheet.
pub const TRANSPOSE_X_NAME: &str = "x'";

/// What a source column had when the link read it: its
/// identifier, its version (type and values) and its x' (as bits, `NaN` if
/// absent). A version does not cover the metadata: the x' is therefore
/// kept separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransposeSourceColumn {
    /// The source column.
    pub column: NodeId,
    /// Its version as read.
    pub version: u64,
    /// Its x' as read, in `f64` bits (`NaN`: none).
    pub x_prime_bits: u64,
}

/// A "row" column of the transposed sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransposedRow {
    /// The row of the source, 0 = the first.
    pub row: usize,
    /// The column of the transposed sheet that carries it.
    pub column: NodeId,
    /// The fingerprint of its values at the last write by the link.
    pub stamp: u64,
}

/// The link of a transposed sheet with its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransposeLink {
    /// The source sheet.
    pub source: NodeId,
    /// The x' column in the transposed sheet.
    pub x_column: NodeId,
    /// The fingerprint of its values at the last write by the link.
    pub x_stamp: u64,
    /// The requested rows, in the order they were requested.
    pub rows: Vec<TransposedRow>,
    /// The source columns as the link read them, in the order
    /// of the sheet.
    pub inputs: Vec<TransposeSourceColumn>,
}

/// The derived state of a transposed sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransposeState {
    /// What the source contains no longer matches what the link read.
    pub stale: bool,
    /// A value of a column created by the link has been edited.
    pub modified: bool,
}

/// What the link of a transposed sheet contains, to write it into a
/// file (persistence does not touch the link fields one by one).
impl TransposeLink {
    /// The state of the link in `tree`.
    pub fn state(&self, tree: &Tree) -> TransposeState {
        let current = read_inputs(tree, self.source);
        let stale = current != self.inputs;
        let changed = |column: NodeId, stamp: u64| {
            tree.node(column)
                .and_then(|n| n.as_column())
                .is_some_and(|c| content_stamp(c) != stamp)
        };
        let modified = changed(self.x_column, self.x_stamp)
            || self.rows.iter().any(|r| changed(r.column, r.stamp));
        TransposeState { stale, modified }
    }
}

/// The fingerprint of the **numeric content** of a column (FNV-1a 64 bits, stable
/// from one Rust version to another: it is written to the file). A
/// type change that keeps the same numbers does not change it; an empty
/// cell counts as `NaN`, a text is hashed with a mark.
pub fn content_stamp(column: &Column) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    fn feed(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(PRIME);
        }
    }
    let canonical = |v: f64| {
        if v.is_nan() {
            f64::NAN.to_bits()
        } else {
            v.to_bits()
        }
    };
    let mut hash = OFFSET;
    match column.values() {
        ColumnValues::Float(values) => {
            for v in values.iter() {
                feed(&mut hash, &canonical(*v).to_le_bytes());
            }
        }
        ColumnValues::Mixed(cells) => {
            for cell in cells.iter() {
                match cell {
                    MixedCell::Empty => feed(&mut hash, &f64::NAN.to_bits().to_le_bytes()),
                    MixedCell::Number(v) => feed(&mut hash, &canonical(*v).to_le_bytes()),
                    MixedCell::Text(text) => {
                        feed(&mut hash, &[0xFF]);
                        feed(&mut hash, text.as_bytes());
                    }
                }
            }
        }
    }
    hash
}

/// The columns of `source` as a link would read them now.
fn read_inputs(tree: &Tree, source: NodeId) -> Vec<TransposeSourceColumn> {
    let Ok(children) = tree.children(source) else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|id| {
            let column = tree.node(*id)?.as_column()?;
            // Only Y columns are transposed (#253): an X, an error
            // bar or a label is not a curve of the row.
            if column.meta().role != ColumnRole::Y {
                return None;
            }
            Some(TransposeSourceColumn {
                column: *id,
                version: column.version(),
                x_prime_bits: column
                    .meta()
                    .x_prime
                    .map_or(f64::NAN.to_bits(), |v| v.to_bits()),
            })
        })
        .collect()
}

fn is_numeric(column: &Column) -> bool {
    match column.values() {
        ColumnValues::Float(_) => true,
        ColumnValues::Mixed(cells) => !cells.iter().any(|c| matches!(c, MixedCell::Text(_))),
    }
}

/// The name of the transposed sheet of the sheet named `source_name`.
pub fn transposed_name(source_name: &str) -> String {
    format!("{source_name}{TRANSPOSE_MARK}")
}

impl Project {
    /// Creates or completes the transposed sheet of `source` with the rows
    /// `rows` (0 = the first) and returns that sheet. Atomic.
    ///
    /// - The sheet is named "`<Source>ᵀ`"; if that name is taken by a
    ///   sheet not linked to `source`, "`<Source>ᵀ 2`", etc. Only
    ///   a sheet already linked to `source` is reused.
    /// - A row already present reuses its column; otherwise a column
    ///   "Row N" is added at the end of the sheet.
    /// - Only columns with the Y role are transposed (the others are skipped).
    /// - Every Y column of `source` must be numeric and carry an x': otherwise
    ///   the refusal names the column (`CoreError::TransposeNotNumeric`,
    ///   `TransposeNoXPrime`). A column shorter than the row gives
    ///   `NaN`.
    /// - If the sheet is stale but not modified, its linked columns are
    ///   refreshed first. Modified, it is not overwritten: the new
    ///   columns are read from the current state of the source, and the link
    ///   keeps its reading (it stays stale).
    pub fn transpose_rows(&mut self, source: NodeId, rows: &[usize]) -> CoreResult<NodeId> {
        let before = self.tree().clone();
        let result = self.transpose_rows_inner(source, rows);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn transpose_rows_inner(&mut self, source: NodeId, rows: &[usize]) -> CoreResult<NodeId> {
        if rows.is_empty() {
            return Err(CoreError::Structure("no row to transpose".to_string()));
        }
        let inputs = self.checked_transpose_inputs(source)?;
        let existing = self
            .tree()
            .transposed_from(source)
            .map(|(sheet, link)| (sheet, link.clone()));
        let (sheet, mut link, fresh_sheet) = match existing {
            Some((sheet, link)) => (sheet, link, false),
            None => {
                let sheet = self.create_transposed_sheet(source)?;
                let link = TransposeLink {
                    source,
                    x_column: sheet,
                    x_stamp: 0,
                    rows: Vec::new(),
                    inputs: Vec::new(),
                };
                (sheet, link, true)
            }
        };
        let state = if fresh_sheet {
            TransposeState::default()
        } else {
            link.state(self.tree())
        };
        // A stale, untouched sheet is refreshed first: it would otherwise be
        // inconsistent (new columns longer than the old ones).
        let refresh = fresh_sheet || (state.stale && !state.modified);
        let mut seen = Vec::new();
        for row in rows {
            if !seen.contains(row) {
                seen.push(*row);
            }
        }
        for row in &seen {
            if !link.rows.iter().any(|r| {
                r.row == *row
                    && self
                        .tree()
                        .node(r.column)
                        .is_some_and(|n| n.as_column().is_some())
            }) {
                link.rows.retain(|r| r.row != *row);
                let placeholder = TransposedRow {
                    row: *row,
                    column: sheet,
                    stamp: 0,
                };
                link.rows.push(placeholder);
            }
        }
        self.write_transpose(sheet, &mut link, &inputs, refresh, &seen)?;
        if refresh {
            link.inputs = inputs;
        }
        Arc::make_mut(self.tree_arc_mut())
            .transposes_mut()
            .insert(sheet, Arc::new(link));
        Ok(sheet)
    }

    /// Refreshes the transposed sheet `sheet` from its source: rewrites
    /// the values of all its linked columns (those the user edited
    /// lose their changes), recreates those that disappeared and
    /// re-reads the source. Atomic.
    pub fn recalculate_transpose(&mut self, sheet: NodeId) -> CoreResult<()> {
        let before = self.tree().clone();
        let result = self.recalculate_transpose_inner(sheet);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn recalculate_transpose_inner(&mut self, sheet: NodeId) -> CoreResult<()> {
        let mut link =
            self.tree().transpose_link(sheet).cloned().ok_or_else(|| {
                CoreError::Structure(format!("{sheet} is not a transposed sheet"))
            })?;
        let inputs = self.checked_transpose_inputs(link.source)?;
        let rows: Vec<usize> = link.rows.iter().map(|r| r.row).collect();
        self.write_transpose(sheet, &mut link, &inputs, true, &rows)?;
        link.inputs = inputs;
        Arc::make_mut(self.tree_arc_mut())
            .transposes_mut()
            .insert(sheet, Arc::new(link));
        Ok(())
    }

    /// Reads the columns of `source` and refuses the first one that is not
    /// transposable.
    fn checked_transpose_inputs(&self, source: NodeId) -> CoreResult<Vec<TransposeSourceColumn>> {
        let node = self.tree().get(source)?;
        if !node.kind().is_worksheet() {
            return Err(CoreError::Structure(format!(
                "{source} is not a worksheet but {}",
                node.kind().with_article()
            )));
        }
        if node.children().is_empty() {
            return Err(CoreError::Structure(
                "the worksheet has no column to transpose".to_string(),
            ));
        }
        for (position, id) in node.children().iter().enumerate() {
            let Some(child) = self.tree().node(*id) else {
                continue;
            };
            let Some(column) = child.as_column() else {
                continue;
            };
            if column.meta().role != ColumnRole::Y {
                continue;
            }
            let letter = spreadsheet_name(position);
            let name = child.name().to_string();
            if !is_numeric(column) {
                return Err(CoreError::TransposeNotNumeric { letter, name });
            }
            if column.meta().x_prime.is_none_or(|v| !v.is_finite()) {
                return Err(CoreError::TransposeNoXPrime { letter, name });
            }
        }
        let inputs = read_inputs(self.tree(), source);
        if inputs.is_empty() {
            return Err(CoreError::Structure(
                "the worksheet has no Y column to transpose".to_string(),
            ));
        }
        Ok(inputs)
    }

    /// Creates the transposed sheet of `source`, empty, right after it.
    fn create_transposed_sheet(&mut self, source: NodeId) -> CoreResult<NodeId> {
        let book = self
            .tree()
            .parent(source)?
            .ok_or_else(|| CoreError::Structure("the worksheet has no workbook".to_string()))?;
        let base = transposed_name(self.tree().get(source)?.name());
        let name = self.free_child_name(book, &base)?;
        let index = self
            .tree()
            .children(book)?
            .iter()
            .position(|c| *c == source)
            .map_or(usize::MAX, |i| i + 1);
        let index = index.min(self.tree().children(book)?.len());
        self.insert_node(
            book,
            index,
            &name,
            NodeKind::Worksheet {
                columns: Vec::new(),
            },
        )
    }

    /// `base`, or "`base` 2", "`base` 3"…: the first free name.
    fn free_child_name(&self, parent: NodeId, base: &str) -> CoreResult<String> {
        let taken = |name: &str| {
            self.tree().children(parent).is_ok_and(|kids| {
                kids.iter()
                    .filter_map(|k| self.tree().node(*k))
                    .any(|n| same_name_ignoring_case(n.name(), name))
            })
        };
        let mut candidate = base.to_string();
        let mut n = 2u32;
        while taken(&candidate) {
            candidate = format!("{base} {n}");
            n += 1;
        }
        Ok(candidate)
    }

    /// Writes the columns of the link. `refresh`: all of them (x' and rows already there),
    /// otherwise only those of `fresh_rows` that have no column yet.
    /// Missing columns are created. Updates the fingerprints written.
    fn write_transpose(
        &mut self,
        sheet: NodeId,
        link: &mut TransposeLink,
        inputs: &[TransposeSourceColumn],
        refresh: bool,
        fresh_rows: &[usize],
    ) -> CoreResult<()> {
        let source_name = self.tree().get(link.source)?.name().to_string();
        let x_values: Vec<f64> = inputs
            .iter()
            .map(|i| f64::from_bits(i.x_prime_bits))
            .collect();
        let columns: Vec<Arc<Vec<f64>>> = inputs
            .iter()
            .map(|i| {
                self.tree()
                    .node(i.column)
                    .and_then(|n| n.as_column())
                    .and_then(Column::numeric_view)
                    .unwrap_or_default()
            })
            .collect();
        let unit = common_unit(self.tree(), inputs);

        // The x' column.
        let x_exists = link.x_column != sheet
            && self
                .tree()
                .node(link.x_column)
                .is_some_and(|n| n.as_column().is_some());
        if !x_exists {
            let mut column = Column::from_f64(&x_values);
            column.set_meta(ColumnMeta {
                role: ColumnRole::X,
                long_name: format!("x' of {source_name}"),
                ..ColumnMeta::default()
            });
            let name = self.free_child_name(sheet, TRANSPOSE_X_NAME)?;
            link.x_column = self.add_column(sheet, Some(0), Some(&name), column)?;
            link.x_stamp = self.stamp_of(link.x_column)?;
        } else if refresh {
            self.overwrite(link.x_column, x_values)?;
            link.x_stamp = self.stamp_of(link.x_column)?;
        }

        // The rows.
        for index in 0..link.rows.len() {
            let TransposedRow { row, column, .. } = link.rows[index];
            let present = column != sheet
                && self
                    .tree()
                    .node(column)
                    .is_some_and(|n| n.as_column().is_some());
            let data: Vec<f64> = columns
                .iter()
                .map(|values| values.get(row).copied().unwrap_or(f64::NAN))
                .collect();
            if !present {
                if !fresh_rows.contains(&row) && !refresh {
                    continue;
                }
                let mut created = Column::from_f64(&data);
                created.set_meta(ColumnMeta {
                    role: ColumnRole::Y,
                    unit: unit.clone(),
                    long_name: format!("Row {} of {source_name}", row.saturating_add(1)),
                    ..ColumnMeta::default()
                });
                let name =
                    self.free_child_name(sheet, &format!("Row {}", row.saturating_add(1)))?;
                let id = self.add_column(sheet, None, Some(&name), created)?;
                link.rows[index].column = id;
                link.rows[index].stamp = self.stamp_of(id)?;
            } else if refresh {
                self.overwrite(column, data)?;
                link.rows[index].stamp = self.stamp_of(column)?;
            }
        }
        Ok(())
    }

    /// Replaces type and values of `column` with these numbers (new version).
    fn overwrite(&mut self, column: NodeId, data: Vec<f64>) -> CoreResult<()> {
        self.restore_column(
            column,
            ColumnType::Numeric,
            ColumnValues::Float(Arc::new(data)),
        )
    }

    fn stamp_of(&self, column: NodeId) -> CoreResult<u64> {
        self.tree()
            .node(column)
            .and_then(|n| n.as_column())
            .map(content_stamp)
            .ok_or(CoreError::UnknownNode(column))
    }

    /// Makes the name of the transposed sheets follow a renamed sheet:
    /// "Newᵀ" for the sheet that carried "Oldᵀ" (or "Oldᵀ 2").
    /// A sheet the user renamed in their own way keeps its name. A
    /// name that is not free or not valid leaves the sheet as it is.
    pub(crate) fn follow_source_rename(&mut self, source: NodeId, old_name: &str) {
        let Some((sheet, _)) = self.tree().transposed_from(source) else {
            return;
        };
        let Some(current) = self.tree().node(sheet).map(|n| n.name().to_string()) else {
            return;
        };
        let old_base = transposed_name(old_name);
        let Some(suffix) = current.strip_prefix(&old_base) else {
            return;
        };
        let Some(new_name) = self.tree().node(source).map(|n| n.name().to_string()) else {
            return;
        };
        let wanted = format!("{}{suffix}", transposed_name(&new_name));
        let Ok(Some(book)) = self.tree().parent(sheet) else {
            return;
        };
        let free = self.tree().children(book).is_ok_and(|kids| {
            !kids
                .iter()
                .filter(|k| **k != sheet)
                .filter_map(|k| self.tree().node(*k))
                .any(|n| same_name_ignoring_case(n.name(), &wanted))
        });
        if free {
            let _ = self.rename_without_follow(sheet, &wanted);
        }
    }

    /// Removes the links whose source or transposed sheet no longer exists:
    /// the remaining sheet becomes ordinary, nothing is lost.
    pub(crate) fn prune_transposes(&mut self) {
        let dead: Vec<NodeId> = self
            .tree()
            .transposes()
            .iter()
            .filter(|(sheet, link)| {
                self.tree().node(**sheet).is_none() || self.tree().node(link.source).is_none()
            })
            .map(|(sheet, _)| *sheet)
            .collect();
        if dead.is_empty() {
            return;
        }
        let tree = Arc::make_mut(self.tree_arc_mut());
        for sheet in dead {
            tree.transposes_mut().remove(&sheet);
        }
    }
}

/// The common unit of the source columns, empty if they differ.
fn common_unit(tree: &Tree, inputs: &[TransposeSourceColumn]) -> String {
    let mut units = inputs.iter().filter_map(|i| {
        tree.node(i.column)?
            .as_column()
            .map(|c| c.meta().unit.clone())
    });
    let Some(first) = units.next() else {
        return String::new();
    };
    if units.all(|u| u == first) {
        first
    } else {
        String::new()
    }
}

/// The links, as the tree carries them.
pub type TransposeLinks = BTreeMap<NodeId, Arc<TransposeLink>>;

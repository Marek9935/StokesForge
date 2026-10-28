//! Linked transposed sheets (#253): writing to disk.
//!
//! The link (`stokesforge_core::TransposeLink`) is stored flat: the transposed
//! sheet, its source, the x' column, the requested rows with the
//! fingerprint written by the link, and the columns of the source as the
//! link read them. "Stale" and "modified" are not written: they are
//! derived from the tree at read. Tolerant reading (invariant 10): a
//! link whose sheet or source no longer exists, or one of whose identifiers
//! exceeds the bound, is dropped — the sheet stays, ordinary.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use stokesforge_core::{
    NodeId, TransposeLink, TransposeSourceColumn, TransposedRow, Tree as ModelTree,
};

/// A "row" column of the transposed sheet: the row of the source, the
/// column that carries it, the fingerprint of its values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransposedRowRecord {
    pub(crate) row: u64,
    pub(crate) column: u64,
    pub(crate) stamp: u64,
}

/// A column of the source as the link read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransposeInputRecord {
    pub(crate) column: u64,
    pub(crate) version: u64,
    pub(crate) x_prime_bits: u64,
}

/// The link of a transposed sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransposeRecord {
    /// The transposed sheet.
    pub(crate) sheet: u64,
    /// Its source.
    pub(crate) source: u64,
    pub(crate) x_column: u64,
    pub(crate) x_stamp: u64,
    #[serde(default)]
    pub(crate) rows: Vec<TransposedRowRecord>,
    #[serde(default)]
    pub(crate) inputs: Vec<TransposeInputRecord>,
}

/// The links of the tree, in the order of their sheet (two saves of the same
/// project stay byte-identical).
pub(crate) fn transposes_to_records(tree: &ModelTree) -> Vec<TransposeRecord> {
    tree.transposes()
        .iter()
        .map(|(sheet, link)| TransposeRecord {
            sheet: sheet.raw(),
            source: link.source.raw(),
            x_column: link.x_column.raw(),
            x_stamp: link.x_stamp,
            rows: link
                .rows
                .iter()
                .map(|r| TransposedRowRecord {
                    row: u64::try_from(r.row).unwrap_or(u64::MAX),
                    column: r.column.raw(),
                    stamp: r.stamp,
                })
                .collect(),
            inputs: link
                .inputs
                .iter()
                .map(|i| TransposeInputRecord {
                    column: i.column.raw(),
                    version: i.version,
                    x_prime_bits: i.x_prime_bits,
                })
                .collect(),
        })
        .collect()
}

/// Reads the links back. `max_id` bounds the identifiers; a link whose sheet
/// or source is not a node of `tree` is dropped.
pub(crate) fn records_to_transposes(
    records: Vec<TransposeRecord>,
    tree: &ModelTree,
    max_id: u64,
) -> BTreeMap<NodeId, Arc<TransposeLink>> {
    let mut out = BTreeMap::new();
    for record in records {
        let ids = [record.sheet, record.source, record.x_column];
        let in_bounds = ids.iter().all(|id| *id <= max_id)
            && record.rows.iter().all(|r| r.column <= max_id)
            && record.inputs.iter().all(|i| i.column <= max_id);
        let sheet = NodeId::from_raw(record.sheet);
        let source = NodeId::from_raw(record.source);
        let is_sheet = |id: NodeId| tree.node(id).is_some_and(|n| n.kind().is_worksheet());
        if !in_bounds || !is_sheet(sheet) || !is_sheet(source) || sheet == source {
            continue;
        }
        let link = TransposeLink {
            source,
            x_column: NodeId::from_raw(record.x_column),
            x_stamp: record.x_stamp,
            rows: record
                .rows
                .iter()
                .map(|r| TransposedRow {
                    row: usize::try_from(r.row).unwrap_or(usize::MAX),
                    column: NodeId::from_raw(r.column),
                    stamp: r.stamp,
                })
                .collect(),
            inputs: record
                .inputs
                .iter()
                .map(|i| TransposeSourceColumn {
                    column: NodeId::from_raw(i.column),
                    version: i.version,
                    x_prime_bits: i.x_prime_bits,
                })
                .collect(),
        };
        out.insert(sheet, Arc::new(link));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::Project;

    fn record(sheet: NodeId, source: NodeId) -> TransposeRecord {
        TransposeRecord {
            sheet: sheet.raw(),
            source: source.raw(),
            x_column: 1,
            x_stamp: 0,
            rows: Vec::new(),
            inputs: Vec::new(),
        }
    }

    /// A crafted file does not make the read fail: a link one of whose
    /// ends is not a sheet, or one of whose identifiers exceeds the
    /// bound, is dropped alone (invariant 10).
    #[test]
    fn hostile_links_are_dropped_one_by_one() {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("B")).unwrap();
        let a = project.tree().children(book).unwrap()[0];
        let b = project.add_worksheet(book, Some("Other")).unwrap();
        let ok = record(b, a);
        let ghost = record(NodeId::from_raw(9_999), a);
        let not_a_sheet = record(book, a);
        let itself = record(a, a);
        let mut huge = record(b, a);
        huge.rows.push(TransposedRowRecord {
            row: 0,
            column: 1_000_000_000_001,
            stamp: 0,
        });
        let kept = records_to_transposes(
            vec![ghost, not_a_sheet, itself, huge, ok],
            project.tree(),
            1_000_000_000_000,
        );
        assert_eq!(kept.keys().copied().collect::<Vec<_>>(), vec![b]);
    }
}

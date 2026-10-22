//! Feuilles transposées liées (#253) : enregistrement sur disque.
//!
//! Le lien (`stokesforge_core::TransposeLink`) se range à plat : la feuille
//! transposée, sa source, la colonne des x', les lignes demandées avec
//! l'empreinte écrite par le lien, et les colonnes de la source telles que le
//! lien les a lues. « Périmée » et « modifiée » ne sont pas écrites : elles se
//! dérivent de l'arbre à la lecture. Lecture tolérante (invariant 10) : un
//! lien dont la feuille ou la source n'existe plus, ou dont un identifiant
//! dépasse la borne, est écarté — la feuille reste, ordinaire.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use stokesforge_core::{
    NodeId, TransposeLink, TransposeSourceColumn, TransposedRow, Tree as ModelTree,
};

/// Une colonne « ligne » de la feuille transposée : la ligne de la source, la
/// colonne qui la porte, l'empreinte de ses valeurs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransposedRowRecord {
    pub(crate) row: u64,
    pub(crate) column: u64,
    pub(crate) stamp: u64,
}

/// Une colonne de la source telle que le lien l'a lue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransposeInputRecord {
    pub(crate) column: u64,
    pub(crate) version: u64,
    pub(crate) x_prime_bits: u64,
}

/// Le lien d'une feuille transposée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransposeRecord {
    /// La feuille transposée.
    pub(crate) sheet: u64,
    /// Sa source.
    pub(crate) source: u64,
    pub(crate) x_column: u64,
    pub(crate) x_stamp: u64,
    #[serde(default)]
    pub(crate) rows: Vec<TransposedRowRecord>,
    #[serde(default)]
    pub(crate) inputs: Vec<TransposeInputRecord>,
}

/// Les liens de l'arbre, dans l'ordre de leur feuille (deux sauvegardes du même
/// projet restent identiques octet à octet).
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

/// Relit les liens. `max_id` borne les identifiants ; un lien dont la feuille
/// ou la source n'est pas un nœud de `tree` est écarté.
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

    /// Un fichier fabriqué ne fait pas échouer la lecture : le lien dont une
    /// extrémité n'est pas une feuille, ou dont un identifiant dépasse la
    /// borne, est écarté seul (invariant 10).
    #[test]
    fn hostile_links_are_dropped_one_by_one() {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("B")).unwrap();
        let a = project.tree().children(book).unwrap()[0];
        let b = project.add_worksheet(book, Some("Autre")).unwrap();
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
